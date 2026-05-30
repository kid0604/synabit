use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Generate a conflict copy path: "filename (Conflict copy 2026-04-17 15-40-00).ext"
fn conflict_copy_path(original: &Path) -> PathBuf {
    let stem = original.file_stem().unwrap_or_default().to_string_lossy();
    let ext = original
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H-%M-%S");
    let new_name = format!("{} (Conflict copy {}){}", stem, timestamp, ext);
    original.with_file_name(new_name)
}

#[tauri::command]
pub async fn gdrive_sync_full(
    app_handle: tauri::AppHandle,
    vault_path: String,
) -> Result<SyncResult, String> {
    use tauri::{Manager, Emitter};
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

    let mut pull_e2ee_key = None;
    let mut push_e2ee_key = None;
    let mut force_full_push = false;
    let mut pending_disable = false;
    let mut pending_reencrypt = false;
    
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.compact_all_crdt();
        
        force_full_push = db.get_kv("force_e2ee_sync").unwrap_or_default().is_some();
        pending_disable = db.get_kv("pending_e2ee_disable").unwrap_or_default().is_some();
        
        let mut current_key = None;
        if let Some(mut pwd) = crate::secrets::SecretManager::get_e2ee_password(Some(&app_handle)) {
            if let Ok(key) = crate::sync::crypto::derive_key(&mut pwd) {
                current_key = Some(key);
            }
        }

        if pending_disable {
            pull_e2ee_key = current_key;
            push_e2ee_key = None;
            force_full_push = true;
        } else if let Some(new_pwd_str) = db.get_kv("pending_e2ee_reencrypt").unwrap_or_default() {
            pending_reencrypt = true;
            pull_e2ee_key = current_key;
            let mut new_pwd = new_pwd_str.clone();
            if let Ok(new_key) = crate::sync::crypto::derive_key(&mut new_pwd) {
                push_e2ee_key = Some(new_key);
            }
            force_full_push = true;
        } else {
            pull_e2ee_key = current_key.clone();
            push_e2ee_key = current_key;
        }
    }

    // 1. Ensure vault root folder exists on Drive
    if manifest.vault_folder_id.is_empty() {
        manifest.vault_folder_id = find_or_create_vault_folder(&token).await?;
    }

    let vault = Path::new(&vault_path);
    if !vault.exists() {
        fs::create_dir_all(vault).map_err(|e| e.to_string())?;
    }

    // 2. Collect remote files
    let drive_files = collect_drive_files(&token, &manifest.vault_folder_id, "").await?;

    let mut drive_map: HashMap<String, DriveFile> = HashMap::new();
    let mut crdt_drive_map: HashMap<String, DriveFile> = HashMap::new();
    for (rel, f) in &drive_files {
        let df = DriveFile {
            id: f.id.clone(),
            name: f.name.clone(),
            mime_type: f.mime_type.clone(),
            modified_time: f.modified_time.clone(),
            md5_checksum: f.md5_checksum.clone(),
        };
        if rel.starts_with(".synabit_crdt/") {
            let base_rel = rel.trim_start_matches(".synabit_crdt/").trim_end_matches(".loro").to_string();
            crdt_drive_map.insert(base_rel, df);
        } else {
            drive_map.insert(rel.clone(), df);
        }
    }

    // 3. Collect local files
    let local_files = collect_local_files(&vault_path);

    // 4. PULL: files on Drive but not locally, or newer on Drive
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
            let local_changed = file_sha256(&local_path) != entry.local_sha256;
            
            let mut remote_changed = !is_mtime_equal(&drive_mtime, &entry.drive_modified_time);
            let drive_md5 = df.md5_checksum.clone().unwrap_or_default();
            if remote_changed && !drive_md5.is_empty() && !entry.local_md5.is_empty() {
                // If mtime drifted, verify content using MD5
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
            
            // First, download the file
            let drive_md_content = match drive_download_file(&token, &drive_id).await {
                Ok(content) => content,
                Err(e) => {
                    result.errors.push(format!("Download {}: {}", rel_path, e));
                    continue;
                }
            };
            
            let is_dummy_text = if is_asset {
                manifest.files.get(rel_path).map_or(
                    pull_e2ee_key.is_some(),
                    |e| e.local_md5 == "e2ee_dummy"
                )
            } else {
                drive_md_content.starts_with(b"E2EE is enabled")
            };
            
            // [FIX Bug 1]: STRICT E2EE RULE
            if is_dummy_text && pull_e2ee_key.is_none() {
                let _ = app_handle.emit("e2ee-password-required", ());
                return Err("Vault is encrypted. Please enter Master Password. Sync aborted.".to_string());
            }

            if let Some(parent) = local_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            if is_conflict {
                // PHASE 2: CRDT Merge
                if let Some(crdt) = crdt_drive_map.get(rel_path) {
                    if let Ok(crdt_content) = drive_download_file(&token, crdt.id.as_ref().unwrap()).await {
                        // [FIX Bug 3]: Decrypt CRDT content before merging ONLY if it is encrypted!
                        let merge_content = if is_dummy_text && pull_e2ee_key.is_some() {
                            match crate::sync::crypto::decrypt_snapshot(pull_e2ee_key.as_ref().unwrap(), &crdt_content) {
                                Ok(dec) => dec,
                                Err(_) => {
                                    let _ = app_handle.emit("e2ee-password-required", ());
                                    return Err("Wrong Master Password for conflict merge. Sync aborted.".to_string());
                                }
                            }
                        } else {
                            // If it's not dummy text, it's NOT encrypted! Don't decrypt it even if we have a key.
                            crdt_content
                        };

                        let db_state = app_handle.state::<crate::db::DbState>();
                        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                        if let Ok(doc) = db.get_crdt_doc(rel_path) {
                            if let Ok((delta, merged_text)) = crate::crdt_bridge::merge_remote_snapshot(&doc, &merge_content) {
                                let _ = db.save_crdt_delta(rel_path, delta);
                                if let Err(e) = fs::write(&local_path, &merged_text) {
                                    result.errors.push(format!("Write merged {}: {}", rel_path, e));
                                }
                                // Do NOT update manifest here! Let PUSH phase see the difference and upload the merged file.
                                result.pulled += 1;
                                result.pulled_files.push(rel_path.clone());
                            }
                        }
                    }
                } else {
                    // Fallback to normal download if no CRDT exists on remote
                    let conflict_path = conflict_copy_path(&local_path);
                    let _ = fs::rename(&local_path, &conflict_path);
                }
            }
            
            if !is_conflict || !crdt_drive_map.contains_key(rel_path) {
                if is_asset {
                    let content_to_save = if is_dummy_text && pull_e2ee_key.is_some() {
                        match crate::sync::crypto::decrypt_snapshot(pull_e2ee_key.as_ref().unwrap(), &drive_md_content) {
                            Ok(dec) => dec,
                            Err(_) => {
                                let _ = app_handle.emit("e2ee-password-required", ());
                                return Err("Wrong Master Password for asset. Sync aborted.".to_string());
                            }
                        }
                    } else {
                        drive_md_content.clone()
                    };
                    if let Err(e) = fs::write(&local_path, &content_to_save) {
                        result.errors.push(format!("Write asset {}: {}", rel_path, e));
                        continue;
                    }
                } else if !is_dummy_text {
                    if let Err(e) = fs::write(&local_path, &drive_md_content) {
                        result.errors.push(format!("Write {}: {}", rel_path, e));
                        continue;
                    }
                }
                
                // Download .loro and save as snapshot locally
                let mut needs_crdt_upload = false;
                let mut asset_skip = is_asset;
                if !asset_skip {
                if let Some(crdt) = crdt_drive_map.get(rel_path) {
                    if let Ok(crdt_content) = drive_download_file(&token, crdt.id.as_ref().unwrap()).await {
                        let content_to_apply = if is_dummy_text && pull_e2ee_key.is_some() {
                            // [FIX Bug 2]: Use proper key
                            match crate::sync::crypto::decrypt_snapshot(pull_e2ee_key.as_ref().unwrap(), &crdt_content) {
                                Ok(dec) => dec,
                                Err(_) => {
                                    let _ = app_handle.emit("e2ee-password-required", ());
                                    return Err("Wrong Master Password. Sync aborted.".to_string());
                                }
                            }
                        } else {
                            crdt_content
                        };
                        let db_state = app_handle.state::<crate::db::DbState>();
                        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                        let _ = db.save_crdt_snapshot(rel_path, content_to_apply);
                        
                        if is_dummy_text {
                            if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                let text_handler = doc.get_text("content");
                                let _ = fs::write(&local_path, text_handler.to_string());
                            }
                        }
                        
                        // Check for remote external edit
                                if !is_dummy_text {
                                    if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                        if let Ok(file_str) = String::from_utf8(drive_md_content.clone()) {
                                            let old_vv = doc.oplog_vv();
                                            if let Ok(delta) = crate::crdt_bridge::apply_text_update(&doc, &file_str) {
                                                if doc.oplog_vv() != old_vv {
                                                    let _ = db.save_crdt_delta(rel_path, delta);
                                                    needs_crdt_upload = true;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // New file on remote without CRDT
                            let db_state = app_handle.state::<crate::db::DbState>();
                            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                            if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                if let Ok(file_str) = String::from_utf8(drive_md_content.clone()) {
                                    let old_vv = doc.oplog_vv();
                                    if let Ok(delta) = crate::crdt_bridge::apply_text_update(&doc, &file_str) {
                                        if doc.oplog_vv() != old_vv {
                                            let _ = db.save_crdt_delta(rel_path, delta);
                                            needs_crdt_upload = true;
                                        }
                                    }
                                }
                            }
                        }

                        if needs_crdt_upload {
                            let snapshot = {
                                let db_state = app_handle.state::<crate::db::DbState>();
                                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                                match db.get_crdt_doc(rel_path) {
                                    Ok(doc) => Some(doc.export_snapshot()),
                                    Err(_) => None,
                                }
                            };
                            if let Some(mut snap) = snapshot {
                                if let Some(key) = &push_e2ee_key {
                                    if let Ok(enc) = crate::sync::crypto::encrypt_snapshot(key, &snap) {
                                        snap = enc;
                                    }
                                }
                                let snapshot = snap;
                                let parent_dir = Path::new(rel_path).parent().unwrap_or(Path::new("")).to_string_lossy();
                                let crdt_dir = if parent_dir.is_empty() { ".synabit_crdt".to_string() } else { format!(".synabit_crdt/{}", parent_dir).replace('\\', "/") };
                                if let Ok(folder_id) = ensure_drive_folder_path(&token, &mut manifest, &crdt_dir).await {
                                    let filename = format!("{}.loro", Path::new(rel_path).file_name().unwrap_or_default().to_string_lossy());
                                    if let Some(crdt) = crdt_drive_map.get(rel_path) {
                                        let _ = drive_update_file(&token, crdt.id.as_ref().unwrap(), &snapshot).await;
                                    } else {
                                        let _ = drive_upload_file(&token, &folder_id, &filename, &snapshot).await;
                                    }
                                }
                            }
                        }
                        } // end if !asset_skip
                        
                        let hash = file_sha256(&local_path);
                        let md5_hash = if is_asset && is_dummy_text { "e2ee_dummy".to_string() } else { super::file_md5(&local_path) };
                        manifest.files.insert(
                            rel_path.clone(),
                            SyncFileEntry {
                                drive_file_id: drive_id,
                                local_sha256: hash,
                                local_md5: md5_hash,
                                drive_modified_time: drive_mtime,
                                local_modified_time: chrono::Utc::now().to_rfc3339(),
                            },
                        );
                        result.pulled += 1;
                        result.pulled_files.push(rel_path.clone());
            }
        }
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

    // 5. PUSH: files local but not on Drive, or modified locally since last sync
    for rel_path in &local_files {
        let local_path = vault.join(rel_path);
        let current_hash = file_sha256(&local_path);
        let is_asset = rel_path.starts_with("assets/");

        let entry_clone = manifest.files.get(rel_path).cloned();
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
                match fs::read(&local_path) {
                    Ok(mut content) => {
                        if !is_asset {
                            // [FIX Bug 4]: Apply external edits to CRDT BEFORE uploading snapshot
                            if let Ok(file_str) = String::from_utf8(content.clone()) {
                                let db_state = app_handle.state::<crate::db::DbState>();
                                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                                if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                    let old_vv = doc.oplog_vv();
                                    if let Ok(delta) = crate::crdt_bridge::apply_text_update(&doc, &file_str) {
                                        if doc.oplog_vv() != old_vv {
                                            let _ = db.save_crdt_delta(rel_path, delta);
                                        }
                                    }
                                }
                            }
                        }

                        // Phase 2 Step 4: Delete old plaintext file if transitioning to E2EE
                        let mut target_drive_id = entry.drive_file_id.clone();
                        let mut needs_id_update = false;
                        let mut parent_folder_id = "".to_string();
                        
                        let is_transitioning_to_e2ee = push_e2ee_key.is_some() && entry.local_md5 != "e2ee_dummy";
                        if is_transitioning_to_e2ee {
                            let rel_dir = Path::new(rel_path)
                                .parent()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_default()
                                .replace('\\', "/");
                            if let Ok(fid) = ensure_drive_folder_path(&token, &mut manifest, &rel_dir).await {
                                parent_folder_id = fid;
                            }
                            let _ = drive_delete_file(&token, &target_drive_id).await;
                            needs_id_update = true;
                        }

                        // Upload .loro snapshot FIRST
                        let mut crdt_upload_success = true;
                        let snapshot = {
                            let db_state = app_handle.state::<crate::db::DbState>();
                            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                            match db.get_crdt_doc(rel_path) {
                                Ok(doc) => Some(doc.export_snapshot()),
                                Err(_) => None,
                            }
                        };
                        if let Some(mut snap) = snapshot {
                            if let Some(key) = &push_e2ee_key {
                                if let Ok(enc) = crate::sync::crypto::encrypt_snapshot(key, &snap) {
                                    snap = enc;
                                }
                            }
                            let snapshot = snap;
                            let mut upload_res = Ok("".to_string());
                            if let Some(crdt_file) = crdt_drive_map.get(rel_path) {
                                upload_res = drive_update_file(&token, crdt_file.id.as_ref().unwrap(), &snapshot).await;
                            } else {
                                let parent_dir = Path::new(rel_path).parent().unwrap_or(Path::new("")).to_string_lossy();
                                let crdt_dir = if parent_dir.is_empty() { ".synabit_crdt".to_string() } else { format!(".synabit_crdt/{}", parent_dir).replace('\\', "/") };
                                if let Ok(folder_id) = ensure_drive_folder_path(&token, &mut manifest, &crdt_dir).await {
                                    let filename = format!("{}.loro", Path::new(rel_path).file_name().unwrap_or_default().to_string_lossy());
                                    match drive_upload_file(&token, &folder_id, &filename, &snapshot).await {
                                        Ok((id, _)) => upload_res = Ok(id),
                                        Err(e) => upload_res = Err(e),
                                    }
                                } else {
                                    upload_res = Err("Failed to ensure CRDT folder".to_string());
                                }
                            }
                            if upload_res.is_err() {
                                crdt_upload_success = false;
                                result.errors.push(format!("Failed to upload CRDT for {}: {:?}", rel_path, upload_res.err().unwrap()));
                            }
                        }

                        if crdt_upload_success {
                            if push_e2ee_key.is_some() {
                                content = b"E2EE is enabled. Open this file in Synabit desktop.".to_vec();
                            }
                            
                            let update_result = if needs_id_update && !parent_folder_id.is_empty() {
                                let filename = Path::new(rel_path).file_name().unwrap_or_default().to_string_lossy().to_string();
                                match drive_upload_file(&token, &parent_folder_id, &filename, &content).await {
                                    Ok((new_id, new_time)) => {
                                        target_drive_id = new_id;
                                        Ok(new_time)
                                    },
                                    Err(e) => Err(e),
                                }
                            } else {
                                drive_update_file(&token, &target_drive_id, &content).await
                            };

                            match update_result {
                                Ok(new_gdrive_time) => {
                                    let mut updated = entry.clone();
                                    updated.drive_file_id = target_drive_id;
                                    updated.local_sha256 = current_hash;
                                    updated.local_md5 = if push_e2ee_key.is_some() { "e2ee_dummy".to_string() } else { super::file_md5(&local_path) };
                                    updated.drive_modified_time = new_gdrive_time;
                                    updated.local_modified_time = chrono::Utc::now().to_rfc3339();
                                    manifest.files.insert(rel_path.clone(), updated);
                                    result.pushed += 1;
                                }
                                Err(e) => {
                                    result.errors.push(format!("Update {}: {}", rel_path, e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("Read {}: {}", rel_path, e));
                    }
                }
            }
        } else if !drive_map.contains_key(rel_path) {
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

            match ensure_drive_folder_path(&token, &mut manifest, &rel_dir).await {
                Ok(parent_folder_id) => match fs::read(&local_path) {
                    Ok(mut content) => {
                        if !is_asset {
                            // [FIX Bug 4]: Apply external edits to CRDT BEFORE uploading snapshot for new files too
                            if let Ok(file_str) = String::from_utf8(content.clone()) {
                                let db_state = app_handle.state::<crate::db::DbState>();
                                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                                if let Ok(doc) = db.get_crdt_doc(rel_path) {
                                    let old_vv = doc.oplog_vv();
                                    if let Ok(delta) = crate::crdt_bridge::apply_text_update(&doc, &file_str) {
                                        if doc.oplog_vv() != old_vv {
                                            let _ = db.save_crdt_delta(rel_path, delta);
                                        }
                                    }
                                }
                            }
                        }

                        let mut crdt_upload_success = true;
                        
                        if is_asset {
                            if let Some(key) = &push_e2ee_key {
                                if let Ok(enc) = crate::sync::crypto::encrypt_snapshot(key, &content) {
                                    content = enc;
                                }
                            }
                        } else {
                            let snapshot = {
                                let db_state = app_handle.state::<crate::db::DbState>();
                                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                                match db.get_crdt_doc(rel_path) {
                                    Ok(doc) => Some(doc.export_snapshot()),
                                    Err(_) => None,
                                }
                            };
                            
                            if let Some(mut snap) = snapshot {
                                if let Some(key) = &push_e2ee_key {
                                    if let Ok(enc) = crate::sync::crypto::encrypt_snapshot(key, &snap) {
                                        snap = enc;
                                    }
                                }
                                let snapshot = snap;
                                let crdt_dir = if rel_dir.is_empty() { ".synabit_crdt".to_string() } else { format!(".synabit_crdt/{}", rel_dir) };
                                if let Ok(folder_id) = ensure_drive_folder_path(&token, &mut manifest, &crdt_dir).await {
                                    let crdt_filename = format!("{}.loro", filename);
                                    if let Err(e) = drive_upload_file(&token, &folder_id, &crdt_filename, &snapshot).await {
                                        crdt_upload_success = false;
                                        result.errors.push(format!("Upload CRDT {}: {}", rel_path, e));
                                    }
                                } else {
                                    crdt_upload_success = false;
                                    result.errors.push(format!("Ensure CRDT dir failed for {}", rel_path));
                                }
                            }
                        }

                        if crdt_upload_success {
                            if !is_asset && push_e2ee_key.is_some() {
                                content = b"E2EE is enabled. Open this file in Synabit desktop.".to_vec();
                            }
                            match drive_upload_file(&token, &parent_folder_id, &filename, &content).await {
                                Ok((new_id, new_gdrive_time)) => {
                                    let new_entry = SyncFileEntry {
                                        drive_file_id: new_id,
                                        local_sha256: current_hash,
                                        local_md5: if push_e2ee_key.is_some() { "e2ee_dummy".to_string() } else { super::file_md5(&local_path) },
                                        drive_modified_time: new_gdrive_time,
                                        local_modified_time: chrono::Utc::now().to_rfc3339(),
                                    };
                                    manifest.files.insert(rel_path.clone(), new_entry);
                                    result.pushed += 1;
                                }
                                Err(e) => {
                                    result.errors.push(format!("Upload {}: {}", rel_path, e));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("Read {}: {}", rel_path, e));
                    }
                },
                Err(e) => {
                    result
                        .errors
                        .push(format!("Ensure folder {}: {}", rel_dir, e));
                }
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

    for (key, drive_id, base_mtime, remote_mtime) in &locally_deleted {
        if is_mtime_equal(remote_mtime, base_mtime) {
            match drive_delete_file(&token, drive_id).await {
                Ok(_) => {
                    manifest.files.remove(key);
                    result.deleted += 1;
                }
                Err(e) => {
                    result.errors.push(format!("Delete remote {}: {}", key, e));
                }
            }
        } else {
            let local_path = vault.join(key);
            if let Some(parent) = local_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match drive_download_file(&token, drive_id).await {
                Ok(content) => {
                    if let Err(e) = fs::write(&local_path, &content) {
                        result
                            .errors
                            .push(format!("Re-download write {}: {}", key, e));
                        continue;
                    }
                    let hash = file_sha256(&local_path);
                    if let Some(entry) = manifest.files.get_mut(key) {
                        entry.local_sha256 = hash;
                        entry.drive_modified_time = remote_mtime.clone();
                        entry.local_modified_time = chrono::Utc::now().to_rfc3339();
                    }
                    result.pulled += 1;
                    result.pulled_files.push(key.clone());
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
        if pending_disable {
            let _ = crate::secrets::SecretManager::clear_e2ee_password(Some(&app_handle));
            let _ = db.delete_kv("pending_e2ee_disable");
        }
        if pending_reencrypt {
            if let Some(new_pwd_str) = db.get_kv("pending_e2ee_reencrypt").unwrap_or_default() {
                let _ = crate::secrets::SecretManager::set_e2ee_password(Some(&app_handle), new_pwd_str);
            }
            let _ = db.delete_kv("pending_e2ee_reencrypt");
        }
    }

    Ok(result)
}

#[tauri::command]
pub fn gdrive_get_cache_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    let cache = gdrive_cache_dir(&app_handle);
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    Ok(cache.to_string_lossy().to_string())
}
