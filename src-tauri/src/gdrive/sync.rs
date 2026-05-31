use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::api::{
    collect_drive_files, drive_delete_file, drive_download_file, drive_update_file,
    drive_upload_file, ensure_drive_folder_path, find_or_create_vault_folder,
};
use super::auth::get_valid_token;
use super::{
    file_sha256, gdrive_cache_dir, load_manifest, save_manifest, DriveFile, SyncFileEntry,
};

/// Compares two RFC3339 timestamps with a 3-second tolerance.
/// This accounts for Google Drive API randomly mutating the fractional seconds
/// of modifiedTime after an upload finishes.
fn is_mtime_equal(t1: &str, t2: &str) -> bool {
    if t1 == t2 {
        return true;
    }
    let dt1 = chrono::DateTime::parse_from_rfc3339(t1).ok();
    let dt2 = chrono::DateTime::parse_from_rfc3339(t2).ok();
    if let (Some(d1), Some(d2)) = (dt1, dt2) {
        d1.signed_duration_since(d2).num_seconds().abs() <= 3
    } else {
        false
    }
}

#[derive(Serialize, Clone)]
pub struct SyncResult {
    pub pulled: u32,
    pub pulled_files: Vec<String>,
    pub pushed: u32,
    pub deleted: u32,
    pub errors: Vec<String>,
}

/// Collect all local files relative to vault_path.
fn collect_local_files(vault_path: &str) -> Vec<String> {
    let base = Path::new(vault_path);
    let mut files = Vec::new();

    for entry in walkdir::WalkDir::new(base)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            if name.starts_with('.') || name == ".synabit_sync_manifest.json" {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(base) {
                let rel_str = rel.to_string_lossy().to_string();
                files.push(rel_str.replace('\\', "/"));
            }
        }
    }

    files
}


/// Get current time as Unix epoch seconds string (matches fs::metadata mtime format).
fn epoch_secs_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// Encrypt payload with the auto-key, aborting sync on failure (C5 fix).
fn encrypt_or_abort(key: &[u8; 32], payload: &[u8]) -> Result<Vec<u8>, String> {
    crate::sync::crypto::encrypt(key, payload)
        .map_err(|e| format!("E2EE encryption failed — aborting to prevent plaintext upload: {}", e))
}

/// Decrypt payload with the auto-key (v3 format).
fn decrypt_payload(key: &[u8; 32], payload: &[u8]) -> Result<Vec<u8>, String> {
    crate::sync::crypto::decrypt(key, payload)
}

#[tauri::command]
pub async fn gdrive_sync_full(
    app_handle: tauri::AppHandle,
    vault_path: String,
) -> Result<SyncResult, String> {
    use tauri::{Manager, Emitter};
    use futures::stream::{self, StreamExt};
    
    const CONCURRENT_LIMIT: usize = 8;
    
    log::info!("Starting full Google Drive sync for vault: {}", vault_path);
    let token = get_valid_token(&app_handle).await?;
    let mut manifest = load_manifest(&vault_path);
    let mut result = SyncResult {
        pulled: 0,
        pulled_files: Vec::new(),
        pushed: 0,
        deleted: 0,
        errors: Vec::new(),
    };

    // Create a shared HTTP client for connection pooling (H6 fix)
    let http_client = reqwest::Client::new();

    // E2EE key setup: read auto-key from keychain (instant, no Argon2)
    let e2ee_key: [u8; 32];
    let mut force_full_push = false;
    
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.compact_all_crdt();
        
        // Get the auto-key (required — E2EE is always enforced)
        e2ee_key = crate::secrets::SecretManager::get_e2ee_key(Some(&app_handle))
            .ok_or_else(|| {
                let _ = app_handle.emit("e2ee-setup-required", ());
                "E2EE key not set up. Please set up encryption first.".to_string()
            })?;
        
        force_full_push = db.get_kv("force_e2ee_sync").unwrap_or_default().is_some();
    }

    // 1. Ensure vault root folder exists on Drive
    if manifest.vault_folder_id.is_empty() {
        manifest.vault_folder_id = find_or_create_vault_folder(&http_client, &token).await?;
    }

    let vault = Path::new(&vault_path);
    if !vault.exists() {
        fs::create_dir_all(vault).map_err(|e| e.to_string())?;
    }

    // 2. Collect remote files
    let drive_files = collect_drive_files(&http_client, &token, &manifest.vault_folder_id, "").await?;

    let mut drive_map: HashMap<String, DriveFile> = HashMap::new();
    let mut crdt_drive_map: HashMap<String, DriveFile> = HashMap::new();
    let mut placeholder_ids: Vec<String> = Vec::new(); // Old placeholders to clean up
    for (rel, f) in &drive_files {
        let df = DriveFile {
            id: f.id.clone(),
            name: f.name.clone(),
            mime_type: f.mime_type.clone(),
            modified_time: f.modified_time.clone(),
            md5_checksum: f.md5_checksum.clone(),
        };
        if rel.starts_with(".synabit_crdt/") {
            // CRDT files are the primary source of truth for markdown
            let base_rel = rel.trim_start_matches(".synabit_crdt/").trim_end_matches(".loro").to_string();
            // Markdown files: use .loro as the drive_map entry
            if !base_rel.starts_with("assets/") {
                drive_map.insert(base_rel.clone(), DriveFile {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    mime_type: f.mime_type.clone(),
                    modified_time: f.modified_time.clone(),
                    md5_checksum: f.md5_checksum.clone(),
                });
            }
            crdt_drive_map.insert(base_rel, df);
        } else if rel.starts_with("assets/") {
            // Assets: use the encrypted blob as the drive_map entry
            drive_map.insert(rel.clone(), df);
        } else {
            // Old placeholder .md file — schedule for cleanup
            if let Some(ref id) = df.id {
                placeholder_ids.push(id.clone());
            }
        }
    }

    // Clean up old placeholder .md files from Drive (one-time migration)
    if !placeholder_ids.is_empty() {
        log::info!("Cleaning up {} old placeholder files from Drive", placeholder_ids.len());
        let cleanup_results: Vec<_> = stream::iter(placeholder_ids.into_iter().map(|id| {
            let client = http_client.clone();
            let tok = token.clone();
            async move {
                let _ = drive_delete_file(&client, &tok, &id).await;
            }
        }))
        .buffer_unordered(CONCURRENT_LIMIT)
        .collect()
        .await;
        let _ = cleanup_results; // Errors are non-fatal
    }

    // 3. Collect local files
    let local_files = collect_local_files(&vault_path);

    // ═══════════════════════════════════════════════════════
    // 4. PULL: Concurrent downloads + sequential processing
    // ═══════════════════════════════════════════════════════
    
    // Phase 1: Identify files to pull
    struct PullItem {
        rel_path: String,
        drive_id: String,
        drive_mtime: String,
        is_asset: bool,
        is_conflict: bool,
    }
    
    let mut pull_items: Vec<PullItem> = Vec::new();
    
    for (rel_path, df) in &drive_map {
        let local_path = vault.join(rel_path);
        let drive_id = df.id.clone().unwrap_or_default();
        let drive_mtime = df.modified_time.clone().unwrap_or_default();

        let (should_pull, is_conflict) = if !local_path.exists() {
            if manifest.files.contains_key(rel_path) {
                (false, false)
            } else {
                (true, false)
            }
        } else if let Some(entry) = manifest.files.get(rel_path) {
            let local_mtime = fs::metadata(&local_path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default();
            let manifest_mtime = entry.local_modified_time.clone();
            
            let local_changed = if local_mtime == manifest_mtime {
                false
            } else {
                file_sha256(&local_path) != entry.local_sha256
            };
            
            let mut remote_changed = !is_mtime_equal(&drive_mtime, &entry.drive_modified_time);
            let drive_md5 = df.md5_checksum.clone().unwrap_or_default();
            if remote_changed && !drive_md5.is_empty() && !entry.local_md5.is_empty() {
                if drive_md5 == entry.local_md5 {
                    remote_changed = false;
                }
            }

            match (local_changed, remote_changed) {
                (false, false) => (false, false),
                (false, true) => (true, false),
                (true, false) => (false, false),
                (true, true) => (true, true),
            }
        } else {
            (false, false)
        };

        if should_pull {
            let is_asset = rel_path.starts_with("assets/");
            
            pull_items.push(PullItem {
                rel_path: rel_path.clone(),
                drive_id,
                drive_mtime,
                is_asset,
                is_conflict,
            });
        }
    }
    
    log::info!("PULL: {} files to download concurrently", pull_items.len());
    
    // Phase 2: Download all files concurrently
    // For assets: drive_id points to encrypted blob
    // For markdown: drive_id points to .loro CRDT file
    let download_tasks: Vec<(String, String, bool)> = pull_items.iter()
        .map(|item| (item.rel_path.clone(), item.drive_id.clone(), item.is_asset))
        .collect();
    
    let download_results: Vec<_> = stream::iter(download_tasks.into_iter().map(|(rel, file_id, is_asset)| {
        let client = http_client.clone();
        let tok = token.clone();
        async move {
            let content = drive_download_file(&client, &tok, &file_id).await;
            (rel, is_asset, content)
        }
    }))
    .buffer_unordered(CONCURRENT_LIMIT)
    .collect()
    .await;
    
    // Index downloaded content by rel_path
    let mut downloaded: HashMap<String, Vec<u8>> = HashMap::new();
    for (rel, _is_asset, content_res) in download_results {
        match content_res {
            Ok(c) => { downloaded.insert(rel, c); }
            Err(e) => { result.errors.push(format!("Download {}: {}", rel, e)); }
        }
    }
    
    // Phase 3: Process downloaded content sequentially
    for item in &pull_items {
        let content = match downloaded.get(&item.rel_path) {
            Some(c) => c,
            None => continue, // Error already recorded
        };
        
        let local_path = vault.join(&item.rel_path);
        let rel_path = &item.rel_path;

        if let Some(parent) = local_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if item.is_asset {
            // ── Asset pull: decrypt encrypted blob and write to disk ──
            let decrypted = match decrypt_payload(&e2ee_key, content) {
                Ok(dec) => dec,
                Err(_) => {
                    return Err("Decryption failed — wrong encryption key. Sync aborted.".to_string());
                }
            };
            if let Err(e) = fs::write(&local_path, &decrypted) {
                result.errors.push(format!("Write asset {}: {}", rel_path, e));
                continue;
            }
        } else {
            // ── Markdown pull: content IS the encrypted CRDT snapshot ──
            let decrypted_crdt = match decrypt_payload(&e2ee_key, content) {
                Ok(dec) => dec,
                Err(_) => {
                    return Err("Decryption failed — wrong encryption key. Sync aborted.".to_string());
                }
            };

            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

            if item.is_conflict {
                // Conflict: merge remote CRDT with local CRDT
                if let Ok(doc) = db.get_crdt_doc(rel_path) {
                    match crate::crdt_bridge::merge_remote_snapshot(&doc, &decrypted_crdt) {
                        Ok((delta, merged_text)) => {
                            if let Err(e) = db.save_crdt_delta(rel_path, delta) {
                                log::warn!("CRDT delta save failed for {}: {}", rel_path, e);
                            }
                            if let Err(e) = fs::write(&local_path, &merged_text) {
                                result.errors.push(format!("Write merged {}: {}", rel_path, e));
                            }
                        }
                        Err(e) => {
                            log::warn!("CRDT merge failed for {}: {}", rel_path, e);
                            result.errors.push(format!("CRDT merge {}: {}", rel_path, e));
                        }
                    }
                }
            } else {
                // Normal pull: save CRDT snapshot, extract text, write file
                let _ = db.save_crdt_snapshot(rel_path, decrypted_crdt);
                if let Ok(doc) = db.get_crdt_doc(rel_path) {
                    let text = doc.get_text("content").to_string();
                    if let Err(e) = fs::write(&local_path, &text) {
                        result.errors.push(format!("Write {}: {}", rel_path, e));
                        continue;
                    }
                }
            }
        }

        // Update manifest
        let hash = file_sha256(&local_path);
        manifest.files.insert(
            rel_path.clone(),
            SyncFileEntry {
                drive_file_id: item.drive_id.clone(),
                local_sha256: hash,
                local_md5: if item.is_asset { "e2ee_asset".to_string() } else { "crdt".to_string() },
                drive_modified_time: item.drive_mtime.clone(),
                local_modified_time: epoch_secs_now(),
            },
        );
        result.pulled += 1;
        result.pulled_files.push(rel_path.clone());
    }

    // 4.5 Handle files deleted remotely but still present locally and in manifest
    let remotely_deleted_keys: Vec<String> = manifest
        .files
        .keys()
        .filter(|k| vault.join(k).exists() && !drive_map.contains_key(k.as_str()))
        .cloned()
        .collect();

    for key in &remotely_deleted_keys {
        let local_path = vault.join(key);
        let current_hash = file_sha256(&local_path);
        let entry_hash = manifest
            .files
            .get(key)
            .map(|e| e.local_sha256.clone())
            .unwrap_or_default();

        if current_hash == entry_hash {
            let _ = fs::remove_file(&local_path);
            manifest.files.remove(key);
            result.deleted += 1;
        } else {
            manifest.files.remove(key);
        }
    }

    // ═══════════════════════════════════════════════════════
    // 5. PUSH: Prepare locally → concurrent uploads
    // ═══════════════════════════════════════════════════════
    
    struct PushItem {
        rel_path: String,
        content: Vec<u8>,               // Encrypted asset content OR encrypted CRDT snapshot
        target_drive_id: Option<String>, // Some = update existing, None = upload new
        folder_id: String,              // Parent folder for new uploads
        is_asset: bool,
        needs_id_update: bool,
        local_sha256: String,
        filename: String,               // Original filename for assets, "name.loro" for markdown
    }
    
    let mut push_items: Vec<PushItem> = Vec::new();
    
    // Pre-resolve all needed folder paths (these are cached in manifest)
    // Collect unique directories first
    let mut needed_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for rel_path in &local_files {
        let rel_dir = Path::new(rel_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
            .replace('\\', "/");
        needed_dirs.insert(rel_dir.clone());
        if !rel_path.starts_with("assets/") {
            let crdt_dir = if rel_dir.is_empty() { ".synabit_crdt".to_string() } else { format!(".synabit_crdt/{}", rel_dir) };
            needed_dirs.insert(crdt_dir);
        }
    }
    
    // Ensure all directories exist (sequentially but cached)
    for dir in &needed_dirs {
        let _ = ensure_drive_folder_path(&http_client, &token, &mut manifest, dir).await;
    }
    
    // Prepare push items
    for rel_path in &local_files {
        let local_path = vault.join(rel_path);
        let is_asset = rel_path.starts_with("assets/");

        let current_hash = if let Some(entry) = manifest.files.get(rel_path) {
            let local_mtime = fs::metadata(&local_path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default();
            
            if local_mtime == entry.local_modified_time && !force_full_push {
                entry.local_sha256.clone()
            } else {
                file_sha256(&local_path)
            }
        } else {
            file_sha256(&local_path)
        };

        let entry_clone = manifest.files.get(rel_path).cloned();
        
        // EXISTING file — check if needs update
        if let Some(entry) = entry_clone {
            if current_hash != entry.local_sha256 || force_full_push {
                let remote_mtime = drive_map
                    .get(rel_path)
                    .and_then(|df| df.modified_time.clone())
                    .unwrap_or_default();
                let drive_md5 = drive_map.get(rel_path).and_then(|df| df.md5_checksum.clone()).unwrap_or_default();
                
                let mut remote_changed = !remote_mtime.is_empty() && !is_mtime_equal(&remote_mtime, &entry.drive_modified_time);
                if remote_changed && !drive_md5.is_empty() && !entry.local_md5.is_empty() {
                    if drive_md5 == entry.local_md5 {
                        remote_changed = false;
                    }
                }
                
                if remote_changed && !force_full_push {
                    continue;
                }

                // Skip unchanged files during force_full_push if already encrypted
                if current_hash == entry.local_sha256 && force_full_push {
                    let already_encrypted = entry.local_md5 == "e2ee_asset" || entry.local_md5 == "e2ee_dummy" || entry.local_md5 == "crdt";
                    if already_encrypted {
                        continue;
                    }
                }
                
                match fs::read(&local_path) {
                    Ok(mut content) => {
                        let needs_id_update = entry.local_md5 != "e2ee_asset" && entry.local_md5 != "e2ee_dummy" && entry.local_md5 != "crdt";
                        
                        // Prepare folder IDs
                        let rel_dir = Path::new(rel_path)
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default()
                            .replace('\\', "/");
                        
                        if is_asset {
                            // Asset: encrypt content, upload with original filename
                            content = encrypt_or_abort(&e2ee_key, &content)?;
                            let folder_id = manifest.folder_ids.get(&rel_dir)
                                .cloned()
                                .unwrap_or_else(|| manifest.vault_folder_id.clone());
                            let filename = Path::new(rel_path).file_name().unwrap_or_default().to_string_lossy().to_string();
                            
                            push_items.push(PushItem {
                                rel_path: rel_path.clone(),
                                content,
                                target_drive_id: if needs_id_update { None } else { Some(entry.drive_file_id.clone()) },
                                folder_id,
                                is_asset: true,
                                needs_id_update,
                                local_sha256: current_hash,
                                filename,
                            });
                        } else {
                            // Markdown: apply text to CRDT, encrypt CRDT snapshot, upload .loro
                            if let Ok(file_str) = String::from_utf8(content.clone()) {
                                let db_state = app_handle.state::<crate::db::DbState>();
                                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                                if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                    let old_vv = doc.oplog_vv();
                                    match crate::crdt_bridge::apply_text_update(&doc, &file_str) {
                                        Ok(delta) => {
                                            if doc.oplog_vv() != old_vv {
                                                if let Err(e) = db.save_crdt_delta(rel_path, delta) {
                                                    log::warn!("CRDT delta save failed for {}: {}", rel_path, e);
                                                }
                                            }
                                        }
                                        Err(e) => log::warn!("CRDT update failed for {}: {}", rel_path, e),
                                    }
                                }
                            }
                            
                            let db_state = app_handle.state::<crate::db::DbState>();
                            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                            if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                let snap = doc.export_snapshot();
                                let encrypted_snap = encrypt_or_abort(&e2ee_key, &snap)?;
                                let crdt_dir = if rel_dir.is_empty() { ".synabit_crdt".to_string() } else { format!(".synabit_crdt/{}", rel_dir) };
                                let crdt_folder_id = manifest.folder_ids.get(&crdt_dir)
                                    .cloned()
                                    .unwrap_or_else(|| manifest.vault_folder_id.clone());
                                let filename = Path::new(rel_path).file_name().unwrap_or_default().to_string_lossy().to_string();
                                
                                push_items.push(PushItem {
                                    rel_path: rel_path.clone(),
                                    content: encrypted_snap,
                                    target_drive_id: crdt_drive_map.get(rel_path).and_then(|c| c.id.clone()),
                                    folder_id: crdt_folder_id,
                                    is_asset: false,
                                    needs_id_update: false, // CRDT files are always in .synabit_crdt/
                                    local_sha256: current_hash,
                                    filename: format!("{}.loro", filename),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("Read {}: {}", rel_path, e));
                    }
                }
            }
        }
        // NEW file — not in manifest and not on Drive
        else if !drive_map.contains_key(rel_path) {
            let rel_dir = Path::new(rel_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
                .replace('\\', "/");

            let filename = Path::new(rel_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            match fs::read(&local_path) {
                Ok(mut content) => {
                    if is_asset {
                        // Asset: encrypt and upload with original filename
                        content = encrypt_or_abort(&e2ee_key, &content)?;
                        let folder_id = manifest.folder_ids.get(&rel_dir)
                            .cloned()
                            .unwrap_or_else(|| manifest.vault_folder_id.clone());
                        
                        push_items.push(PushItem {
                            rel_path: rel_path.clone(),
                            content,
                            target_drive_id: None,
                            folder_id,
                            is_asset: true,
                            needs_id_update: false,
                            local_sha256: current_hash,
                            filename,
                        });
                    } else {
                        // Markdown: apply to CRDT, encrypt snapshot, upload .loro
                        if let Ok(file_str) = String::from_utf8(content.clone()) {
                            let db_state = app_handle.state::<crate::db::DbState>();
                            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                            if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                let old_vv = doc.oplog_vv();
                                match crate::crdt_bridge::apply_text_update(&doc, &file_str) {
                                    Ok(delta) => {
                                        if doc.oplog_vv() != old_vv {
                                            if let Err(e) = db.save_crdt_delta(rel_path, delta) {
                                                log::warn!("CRDT delta save failed for {}: {}", rel_path, e);
                                            }
                                        }
                                    }
                                    Err(e) => log::warn!("CRDT update failed for {}: {}", rel_path, e),
                                }
                            }
                        }
                        
                        let db_state = app_handle.state::<crate::db::DbState>();
                        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                        if let Ok(doc) = db.get_crdt_doc(rel_path) {
                            let snap = doc.export_snapshot();
                            let encrypted_snap = encrypt_or_abort(&e2ee_key, &snap)?;
                            let crdt_dir = if rel_dir.is_empty() { ".synabit_crdt".to_string() } else { format!(".synabit_crdt/{}", rel_dir) };
                            let crdt_folder_id = manifest.folder_ids.get(&crdt_dir)
                                .cloned()
                                .unwrap_or_else(|| manifest.vault_folder_id.clone());
                            
                            push_items.push(PushItem {
                                rel_path: rel_path.clone(),
                                content: encrypted_snap,
                                target_drive_id: None,
                                folder_id: crdt_folder_id,
                                is_asset: false,
                                needs_id_update: false,
                                local_sha256: current_hash,
                                filename: format!("{}.loro", filename),
                            });
                        }
                    }
                }
                Err(e) => {
                    result.errors.push(format!("Read {}: {}", rel_path, e));
                }
            }
        }
    }
    
    log::info!("PUSH: {} files to upload concurrently", push_items.len());
    
    // Delete old plaintext files that need ID update (must be sequential before uploads)
    for item in &push_items {
        if item.needs_id_update {
            if let Some(entry) = manifest.files.get(&item.rel_path) {
                let _ = drive_delete_file(&http_client, &token, &entry.drive_file_id).await;
            }
        }
    }
    
    // Upload all files concurrently
    // Each upload task: (1) upload CRDT if needed, (2) upload content
    struct UploadTask {
        rel_path: String,
        content: Vec<u8>,
        target_drive_id: Option<String>,
        folder_id: String,
        filename: String,
        is_asset: bool,
    }
    
    let upload_tasks: Vec<UploadTask> = push_items.iter().map(|item| UploadTask {
        rel_path: item.rel_path.clone(),
        content: item.content.clone(),
        target_drive_id: item.target_drive_id.clone(),
        folder_id: item.folder_id.clone(),
        filename: item.filename.clone(),
        is_asset: item.is_asset,
    }).collect();
    
    let upload_results: Vec<_> = stream::iter(upload_tasks.into_iter().map(|task| {
        let client = http_client.clone();
        let tok = token.clone();
        
        async move {
            let rel_path = task.rel_path;
            // Single upload: either encrypted asset or encrypted CRDT .loro
            let content_result = if let Some(ref id) = task.target_drive_id {
                drive_update_file(&client, &tok, id, &task.content).await
                    .map(|mtime| (id.clone(), mtime))
            } else {
                drive_upload_file(&client, &tok, &task.folder_id, &task.filename, &task.content).await
            };
            
            (rel_path, task.is_asset, content_result)
        }
    }))
    .buffer_unordered(CONCURRENT_LIMIT)
    .collect()
    .await;
    
    // Process upload results — update manifest
    for (rel_path, is_asset, upload_result) in &upload_results {
        match upload_result {
            Ok((new_id, new_mtime)) => {
                let item = push_items.iter().find(|i| &i.rel_path == rel_path).unwrap();
                manifest.files.insert(
                    rel_path.clone(),
                    SyncFileEntry {
                        drive_file_id: new_id.clone(),
                        local_sha256: item.local_sha256.clone(),
                        local_md5: if *is_asset { "e2ee_asset".to_string() } else { "crdt".to_string() },
                        drive_modified_time: new_mtime.clone(),
                        local_modified_time: epoch_secs_now(),
                    },
                );
                result.pushed += 1;
            }
            Err(e) => {
                result.errors.push(format!("Upload {}: {}", rel_path, e));
            }
        }
    }

    // 6. DELETE from manifest entries that no longer exist locally or on Drive
    let stale_keys: Vec<String> = manifest
        .files
        .keys()
        .filter(|k| {
            let local_exists = vault.join(k).exists();
            let drive_exists = drive_map.contains_key(k.as_str());
            !local_exists && !drive_exists
        })
        .cloned()
        .collect();

    for key in &stale_keys {
        manifest.files.remove(key);
        result.deleted += 1;
    }

    // Handle files deleted locally but still on Drive
    let locally_deleted: Vec<(String, String, String, String)> = manifest
        .files
        .iter()
        .filter(|(k, _v)| !vault.join(k).exists() && drive_map.contains_key(k.as_str()))
        .map(|(k, v)| {
            let remote_mtime = drive_map
                .get(k.as_str())
                .and_then(|df| df.modified_time.clone())
                .unwrap_or_default();
            (
                k.clone(),
                v.drive_file_id.clone(),
                v.drive_modified_time.clone(),
                remote_mtime,
            )
        })
        .collect();

    // Delete remotely concurrently
    if !locally_deleted.is_empty() {
        let delete_items: Vec<(String, String)> = locally_deleted.iter()
            .filter(|(_, _, base_mtime, remote_mtime)| is_mtime_equal(remote_mtime, base_mtime))
            .map(|(k, id, _, _)| (k.clone(), id.clone()))
            .collect();
        
        let redownload_items: Vec<(String, String, String)> = locally_deleted.iter()
            .filter(|(_, _, base_mtime, remote_mtime)| !is_mtime_equal(remote_mtime, base_mtime))
            .map(|(k, id, _, rm)| (k.clone(), id.clone(), rm.clone()))
            .collect();
        
        // Concurrent deletes
        let delete_results: Vec<_> = stream::iter(delete_items.into_iter().map(|(key, drive_id)| {
            let client = http_client.clone();
            let tok = token.clone();
            async move {
                (key, drive_delete_file(&client, &tok, &drive_id).await)
            }
        }))
        .buffer_unordered(CONCURRENT_LIMIT)
        .collect()
        .await;
        
        for (key, del_result) in delete_results {
            match del_result {
                Ok(_) => {
                    manifest.files.remove(&key);
                    result.deleted += 1;
                }
                Err(e) => {
                    result.errors.push(format!("Delete remote {}: {}", key, e));
                }
            }
        }
        
        // Concurrent re-downloads for remotely modified files
        let redownload_results: Vec<_> = stream::iter(redownload_items.into_iter().map(|(key, drive_id, remote_mtime)| {
            let client = http_client.clone();
            let tok = token.clone();
            async move {
                (key, remote_mtime, drive_download_file(&client, &tok, &drive_id).await)
            }
        }))
        .buffer_unordered(CONCURRENT_LIMIT)
        .collect()
        .await;
        
        for (key, remote_mtime, dl_result) in redownload_results {
            let local_path = vault.join(&key);
            if let Some(parent) = local_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match dl_result {
                Ok(content) => {
                    let is_asset = key.starts_with("assets/");
                    
                    if is_asset {
                        // Asset: drive_id points to encrypted blob — decrypt and write
                        match decrypt_payload(&e2ee_key, &content) {
                            Ok(dec) => {
                                if let Err(e) = fs::write(&local_path, &dec) {
                                    result.errors.push(format!("Re-download write {}: {}", key, e));
                                    continue;
                                }
                            }
                            Err(e) => {
                                result.errors.push(format!("Re-download decrypt {}: {}", key, e));
                                continue;
                            }
                        }
                    } else {
                        // Markdown: drive_id points to .loro — decrypt CRDT, reconstruct text
                        match decrypt_payload(&e2ee_key, &content) {
                            Ok(decrypted_crdt) => {
                                let db_state = app_handle.state::<crate::db::DbState>();
                                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                                let _ = db.save_crdt_snapshot(&key, decrypted_crdt);
                                if let Ok(doc) = db.get_crdt_doc(&key) {
                                    let text = doc.get_text("content").to_string();
                                    if let Err(e) = fs::write(&local_path, &text) {
                                        result.errors.push(format!("Re-download write {}: {}", key, e));
                                        continue;
                                    }
                                }
                            }
                            Err(e) => {
                                result.errors.push(format!("Re-download CRDT decrypt {}: {}", key, e));
                                continue;
                            }
                        }
                    }
                    
                    let hash = file_sha256(&local_path);
                    if let Some(entry) = manifest.files.get_mut(&key) {
                        entry.local_sha256 = hash;
                        entry.drive_modified_time = remote_mtime;
                        entry.local_modified_time = epoch_secs_now();
                    }
                    result.pulled += 1;
                    result.pulled_files.push(key);
                }
                Err(e) => {
                    result.errors.push(format!("Re-download {}: {}", key, e));
                }
            }
        }
    }

    // 7. Save manifest
    save_manifest(&vault_path, &manifest)?;

    log::info!(
        "Google Drive sync complete. Pulled: {}, Pushed: {}, Deleted: {}, Errors: {}",
        result.pulled,
        result.pushed,
        result.deleted,
        result.errors.len()
    );

    if result.errors.is_empty() {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.delete_kv("force_e2ee_sync");
    }

    Ok(result)
}

#[tauri::command]
pub fn gdrive_get_cache_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    let cache = gdrive_cache_dir(&app_handle);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    Ok(cache.to_string_lossy().to_string())
}
