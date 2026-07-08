//! Provider-agnostic P2P sync engine for Synabit.
//!
//! Uses the `SyncTransport` trait (not any specific API) to implement
//! the full sync flow: pull → push → ack.
//!
//! ## Sync model
//!
//! - **Sequence-based**: server assigns monotonic `seq` per vault.
//! - **Client cursor**: tracks last-processed seq in KV (`p2p_sync:cursor`).
//! - **CRDT merge** for Markdown (character-level, conflict-free).
//! - **LWW** for JSON/canvas (timestamp-based last-write-wins).
//! - **E2EE**: all payloads encrypted with XChaCha20-Poly1305 before leaving
//!   the device. The transport never sees plaintext.
//!
//! ## Wire payload
//!
//! ```text
//! DocSyncPayload { doc_id, snapshot, is_json }
//!   → postcard serialize → encrypt(e2ee_key) → push_doc(doc_hash, ciphertext)
//! ```

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::sync::progress::{SyncConflictInfo, SyncPhase, SyncProgressEvent};
use crate::sync::{SyncResult, SyncTransport};
use crate::sync::utils::{collect_local_files, file_sha256};

// ---------------------------------------------------------------------------
// Payload types
// ---------------------------------------------------------------------------

/// Encrypted payload inner structure. Serialized with postcard, encrypted
/// with XChaCha20-Poly1305, then pushed to the transport.
///
/// `doc_id` is embedded inside the ciphertext so the receiver can map
/// `doc_hash → relative file path` without any server-side metadata.
#[derive(Serialize, Deserialize)]
struct DocSyncPayload {
    /// V5 Identity: Stable UUID across renames.
    node_id: String,
    /// Current relative path inside the sender's vault (e.g. `"Notes/diary.md"`).
    rel_path: String,
    /// CRDT snapshot bytes (Loro `export_snapshot()`).
    snapshot: Vec<u8>,
    /// `true` for JSON/canvas (LWW), `false` for Markdown (CRDT merge).
    is_json: bool,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Compute a stable document-address hash: `blake3(doc_id.as_bytes())`.
fn doc_hash(doc_id: &str) -> [u8; 32] {
    *blake3::hash(doc_id.as_bytes()).as_bytes()
}

// collect_local_files and file_sha256 live in crate::sync::utils

/// Atomic file write: write to a sibling temp file, then rename.
///
/// This prevents half-written files if the process crashes mid-write.
fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    // Deterministic temp name derived from the target to avoid collisions
    let tmp_name = format!(
        ".{}.tmp",
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );
    let tmp_path = parent.join(&tmp_name);

    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Extract `metadata.updated_at` from a JSON string for LWW conflict
/// resolution. Returns an empty string if the field is absent.
fn extract_json_updated_at(json_text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(json_text)
        .ok()
        .and_then(|v| {
            v.get("metadata")
                .and_then(|m| m.get("updated_at"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

/// Returns `true` if the file is an asset (binary blob, Phase 2).
fn is_asset(doc_id: &str) -> bool {
    doc_id.starts_with("assets/")
}

/// Returns `true` if the file should use LWW (JSON/canvas) instead of
/// character-level CRDT merge.
fn is_json_file(doc_id: &str) -> bool {
    doc_id.ends_with(".json") || doc_id.ends_with(".canvas")
}

/// Update the DB node + search index after writing a file.
fn update_db_for_file(
    app_handle: &tauri::AppHandle,
    vault_path: &str,
    local_path: &Path,
    doc_id: &str,
) {
    if let Some(node) =
        crate::utils::node_parser::parse_file_to_node(vault_path, local_path)
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.upsert_node(&node);

        let tags = node
            .properties
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let status = node.properties.get("status").and_then(|s| s.as_str());
        let props_str =
            serde_json::to_string(&node.properties).unwrap_or_default();

        db.upsert_search_entry(
            &node.id,
            &node.node_type,
            &node.title,
            &tags,
            &node.content,
            &props_str,
            status,
            &node.updated_at,
            &node.id,
        );
        info!("PULL: updated DB for node: {} ({})", doc_id, node.title);
    }
}

// ---------------------------------------------------------------------------
// Main sync entry-point
// ---------------------------------------------------------------------------

/// Run a full P2P sync cycle: pull → push → ack.
///
/// # Arguments
///
/// * `app_handle` – Tauri app handle (for DB state + secrets)
/// * `transport`  – any `SyncTransport` implementor (server, relay, …)
/// * `vault_path` – absolute path to the local vault directory
/// * `device_id`  – stable UUID identifying this device
pub async fn p2p_sync_full(
    app_handle: &tauri::AppHandle,
    transport: &dyn SyncTransport,
    vault_path: &str,
    device_id: &str,
) -> AppResult<SyncResult> {
    let provider = transport.provider_name();
    info!(
        "P2P sync starting via {} for vault: {}",
        provider, vault_path
    );

    let mut result = SyncResult::empty();
    let vault = Path::new(vault_path);

    if !vault.exists() {
        fs::create_dir_all(vault)?;
    }

    // ── Pre-flight ──────────────────────────────────────────

    // 1. E2EE key (required — abort if missing)
    let e2ee_key: [u8; 32] = {
        use tauri::Emitter;
        crate::secrets::SecretManager::get_e2ee_key(Some(app_handle))
            .ok_or_else(|| {
                let _ = app_handle.emit("e2ee-setup-required", ());
                AppError::SyncError(
                    "E2EE key not set up. Please set up encryption first."
                        .to_string(),
                )
            })?
    };

    // 2. Compact CRDT history to reduce snapshot sizes
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.compact_all_crdt();
    }

    // 3. Load cursor (last-processed server sequence number)
    let cursor: u64 = {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("p2p_sync:cursor")
            .unwrap_or(None)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    };

    info!("P2P sync cursor = {}", cursor);

    // ════════════════════════════════════════════════════════
    // PULL phase
    // ════════════════════════════════════════════════════════

    let entries = transport.pull_since(cursor).await?;
    info!("PULL: {} entries since seq {}", entries.len(), cursor);

    let mut max_seq: u64 = cursor;
    let mut pulled_node_ids: HashSet<String> = HashSet::new();

    let pull_total = entries.len() as u32;
    for (pull_idx, entry) in entries.iter().enumerate() {
        // Emit pull progress (file_name populated after decrypt)
        let _ = app_handle.emit("sync-progress", SyncProgressEvent {
            phase: SyncPhase::Pull,
            current: pull_idx as u32 + 1,
            total: pull_total,
            file_name: String::new(),
            bytes_transferred: entry.encrypted_payload.len() as u64,
        });
        // Track highest seq regardless of processing outcome
        if entry.seq > max_seq {
            max_seq = entry.seq;
        }

        // (a) Skip our own pushes
        if entry.source_device == device_id {
            debug!("PULL skip own push: seq={}", entry.seq);
            continue;
        }
        // ── PULL DELETE ──────────────────────────────────────
        if entry.is_delete {
            info!("PULL delete: doc_hash={} (seq={})", hex::encode(&entry.doc_hash), entry.seq);
            let known_paths = {
                let db_state = app_handle.state::<crate::db::DbState>();
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.get_all_document_paths().unwrap_or_default()
            };
            
            let mut found = None;
            for (nid, rel_path) in known_paths {
                if doc_hash(&nid) == entry.doc_hash {
                    found = Some((nid, rel_path));
                    break;
                }
            }

            if let Some((nid, rel_path)) = found {
                let local_path = vault.join(&rel_path);
                if local_path.exists() {
                    let _ = fs::remove_file(&local_path);
                }
                let db_state = app_handle.state::<crate::db::DbState>();
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                let _ = db.delete_document_path(&nid);
                let _ = db.delete_crdt_doc(&nid);
                let _ = db.delete_node(&nid);
                db.delete_search_entry(&nid);
                info!("PULL delete applied for {}", rel_path);
                result.deleted += 1;
            } else {
                warn!("PULL delete: could not find local node for doc_hash at seq={}", entry.seq);
            }
            continue;
        }

        // (b) Integrity check: BLAKE3(encrypted_payload) == payload_hash
        let computed_hash = *blake3::hash(&entry.encrypted_payload).as_bytes();
        if computed_hash != entry.payload_hash {
            warn!(
                "PULL: payload hash mismatch at seq={}, skipping",
                entry.seq
            );
            result.errors.push(format!(
                "Payload integrity check failed at seq={}",
                entry.seq
            ));
            continue;
        }

        // Track bytes received
        result.rx_bytes += entry.encrypted_payload.len() as u64;

        // (c) Decrypt
        let decrypted = match crate::sync::crypto::decrypt(&e2ee_key, &entry.encrypted_payload) {
            Ok(data) => data,
            Err(e) => {
                warn!("PULL: decrypt failed at seq={}: {}", entry.seq, e);
                result.errors.push(format!("Decrypt failed (seq {}): {}", entry.seq, e));
                max_seq = max_seq.max(entry.seq);
                continue;
            }
        };

        // (d) Deserialize payload
        let payload: DocSyncPayload = match postcard::from_bytes(&decrypted) {
            Ok(p) => p,
            Err(e) => {
                warn!("PULL: deserialize failed at seq={}: {}", entry.seq, e);
                result.errors.push(format!("Deserialize failed (seq {}): {}", entry.seq, e));
                max_seq = max_seq.max(entry.seq);
                continue;
            }
        };

        let node_id = &payload.node_id;
        pulled_node_ids.insert(node_id.clone());

        let local_path = vault.join(&payload.rel_path);

        // Track path mapping
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        // Check if file was moved/renamed locally
        let old_path_opt = db.get_path_by_node_id(node_id).unwrap_or(None);
        if let Some(old_path) = old_path_opt {
            if old_path != payload.rel_path {
                let old_local_path = vault.join(&old_path);
                if old_local_path.exists() {
                    // File was renamed by remote. Move it locally to match.
                    info!("PULL rename: {} -> {}", old_path, payload.rel_path);
                    if let Some(parent) = local_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::rename(&old_local_path, &local_path);
                }
            }
        }
        
        let _ = db.upsert_document_path(node_id, &payload.rel_path);
        // Drop the lock before proceeding to avoid deadlocks with pull_markdown
        drop(db);

        // ── DOCUMENT UPDATE ─────────────────────────────────
        if let Some(parent) = local_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if payload.is_json {
            // ── JSON / Canvas: LWW ──────────────────────────
            pull_json(app_handle, vault_path, &local_path, node_id, &payload)?;
        } else {
            // ── Markdown: CRDT merge ────────────────────────
            pull_markdown(
                app_handle,
                vault_path,
                &local_path,
                node_id,
                &payload,
                &mut result,
            )?;
        }

        // Update node DB + search index
        update_db_for_file(app_handle, vault_path, &local_path, node_id);

        // Store SHA-256 of the file we just wrote so we don't re-push it
        {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let sha = file_sha256(&local_path);
            let _ = db.set_kv(
                &format!("p2p_sync:sha256:{}", payload.rel_path),
                &sha,
            );
        }

        result.pulled += 1;
        result.pulled_files.push(payload.rel_path.clone());
    }

    // Persist cursor immediately after pull succeeds to avoid losing progress if push fails
    if max_seq > cursor {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.set_kv("p2p_sync:cursor", &max_seq.to_string());
        info!("Cursor persisted after pull phase: {}", max_seq);
    }

    // ════════════════════════════════════════════════════════
    // ASSET PULL phase (Phase 2.5)
    // ════════════════════════════════════════════════════════
    let local_files = collect_local_files(vault_path);
    let mut missing_assets = std::collections::HashSet::new();
    let re = regex::Regex::new(r#"assets/[^)"'\s\]]+"#).unwrap();

    for rel_path in &local_files {
        if is_asset(rel_path) {
            continue;
        }
        let local_path = vault.join(rel_path);
        if let Ok(content) = fs::read_to_string(&local_path) {
            for cap in re.captures_iter(&content) {
                let asset_path = urlencoding::decode(&cap[0])
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| cap[0].to_string());
                let asset_full = vault.join(&asset_path);
                if !asset_full.exists() {
                    missing_assets.insert(asset_path);
                }
            }
        }
    }

    for asset_path in missing_assets {
        let asset_hash = doc_hash(&asset_path);
        match transport.pull_asset(&asset_hash).await {
            Ok(Some(encrypted_data)) => {
                result.rx_bytes += encrypted_data.len() as u64;
                match crate::sync::crypto::decrypt(&e2ee_key, &encrypted_data) {
                    Ok(decrypted) => {
                        let asset_full = vault.join(&asset_path);
                        if let Err(e) = atomic_write(&asset_full, &decrypted) {
                            result.errors.push(format!("Write asset {}: {}", asset_path, e));
                        } else {
                            let current_sha = file_sha256(&asset_full);
                            let db_state = app_handle.state::<crate::db::DbState>();
                            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                            let _ = db.set_kv(
                                &format!("p2p_sync:sha256:{}", asset_path),
                                &current_sha,
                            );
                            result.pulled += 1;
                            result.pulled_files.push(asset_path.clone());
                            info!("PULL ASSET OK: {}", asset_path);
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("Decrypt asset {}: {}", asset_path, e));
                    }
                }
            }
            Ok(None) => {
                warn!("Asset not found on remote: {}", asset_path);
            }
            Err(e) => {
                result.errors.push(format!("Pull asset {}: {}", asset_path, e));
            }
        }
    }

    // ════════════════════════════════════════════════════════
    // PUSH phase
    // ════════════════════════════════════════════════════════

    let local_files = collect_local_files(vault_path);
    info!("PUSH: {} local files to consider", local_files.len());

    // ── PUSH DELETIONS ───────────────────────────────────────
    {
        let known_paths = {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_all_document_paths().unwrap_or_default()
        };

        let local_files_set: std::collections::HashSet<&String> = local_files.iter().collect();
        for (node_id, rel_path) in known_paths {
            if !local_files_set.contains(&rel_path) && !is_asset(&rel_path) {
                info!("PUSH DELETE: {} (node_id={})", rel_path, node_id);
                let doc_hash_bytes = doc_hash(&node_id);
                match transport.push_delete(&doc_hash_bytes).await {
                    Ok(_) => {
                        let db_state = app_handle.state::<crate::db::DbState>();
                        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                        let _ = db.delete_document_path(&node_id);
                        let _ = db.delete_crdt_doc(&node_id);
                        let _ = db.delete_node(&node_id);
                        db.delete_search_entry(&node_id);
                        result.deleted += 1;
                    }
                    Err(e) => {
                        if !e.to_string().contains("not supported") {
                            warn!("Push delete failed for {}: {}", rel_path, e);
                            result.errors.push(format!("Push delete failed ({}): {}", rel_path, e));
                        }
                    }
                }
            }
        }
    }

    let push_total = local_files.len() as u32;
    let mut doc_batch = Vec::new();
    let mut doc_batch_info = Vec::new();

    for (push_idx, rel_path) in local_files.iter().enumerate() {
        // (a) Handle assets (Phase 2)
        if is_asset(rel_path) {
            let local_path = vault.join(rel_path);
            let current_sha = file_sha256(&local_path);
            if current_sha.is_empty() {
                continue;
            }

            let stored_sha: Option<String> = {
                let db_state = app_handle.state::<crate::db::DbState>();
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.get_kv(&format!("p2p_sync:sha256:{}", rel_path))
                    .unwrap_or(None)
            };

            if stored_sha.as_deref() == Some(current_sha.as_str()) {
                debug!("PUSH skip asset (unchanged): {}", rel_path);
                continue;
            }

            let file_content = match fs::read(&local_path) {
                Ok(c) => c,
                Err(e) => {
                    result.errors.push(format!("Read asset {}: {}", rel_path, e));
                    continue;
                }
            };

            let asset_hash = doc_hash(rel_path);
            let encrypted = match crate::sync::crypto::encrypt_v5(&e2ee_key, &file_content, false) {
                Ok(enc) => enc,
                Err(e) => {
                    result.errors.push(format!("Encrypt asset {}: {}", rel_path, e));
                    continue;
                }
            };

            result.tx_bytes += encrypted.len() as u64;

            match transport.push_asset(&asset_hash, encrypted).await {
                Ok(_) => {
                    let db_state = app_handle.state::<crate::db::DbState>();
                    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = db.set_kv(
                        &format!("p2p_sync:sha256:{}", rel_path),
                        &current_sha,
                    );
                    result.pushed += 1;
                    info!("PUSH ASSET: {}", rel_path);
                }
                Err(e) => {
                    result.errors.push(format!("Push asset {}: {}", rel_path, e));
                }
            }
            continue;
        }

        // (b) Skip files we just pulled is handled below after determining node_id

        let local_path = vault.join(rel_path);

        // (c) SHA-256 change detection
        let current_sha = file_sha256(&local_path);
        if current_sha.is_empty() {
            // File unreadable
            continue;
        }

        let stored_sha: Option<String> = {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_kv(&format!("p2p_sync:sha256:{}", rel_path))
                .unwrap_or(None)
        };

        // (d-e) Skip unchanged
        if stored_sha.as_deref() == Some(current_sha.as_str()) {
            debug!("PUSH skip (unchanged): {}", rel_path);
            continue;
        }

        // Phase 5 Identity: get node_id
        let node_id = match super::identity::get_or_assign_node_id(vault, &local_path) {
            Ok(id) => id,
            Err(e) => {
                result.errors.push(format!("Identity {}: {}", rel_path, e));
                continue;
            }
        };

        if pulled_node_ids.contains(&node_id) {
            debug!("PUSH skip (just pulled node): {}", node_id);
            continue;
        }

        // (f) Read file
        let file_content = match fs::read_to_string(&local_path) {
            Ok(c) => c,
            Err(e) => {
                result
                    .errors
                    .push(format!("Read {}: {}", rel_path, e));
                continue;
            }
        };

        // (g) Determine type
        let is_json = is_json_file(rel_path);

        // (h) Update CRDT using node_id
        {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

            let _ = db.upsert_document_path(&node_id, rel_path);

            if is_json {
                // JSON: snapshot replacement (last-write-wins)
                crate::commands::nodes::sync_crdt_snapshot_replace(
                    &db,
                    &node_id,
                    &file_content,
                );
            } else {
                // Markdown: character-level CRDT diff with panic recovery
                match db.get_crdt_doc(&node_id) {
                    Ok(doc) => {
                        let doc_ref = &doc;
                        let apply_result = std::panic::catch_unwind(
                            std::panic::AssertUnwindSafe(|| {
                                crate::crdt_bridge::apply_text_update(
                                    doc_ref,
                                    &file_content,
                                )
                            }),
                        );
                        match apply_result {
                            Ok(Ok(delta)) => {
                                if !delta.is_empty() {
                                    if let Err(e) =
                                        db.save_crdt_delta(&node_id, delta)
                                    {
                                        warn!(
                                            "CRDT delta save failed for {}: {}",
                                            node_id, e
                                        );
                                    }
                                }
                            }
                            Ok(Err(e)) => {
                                warn!(
                                    "CRDT update error for {}: {}, resetting",
                                    node_id, e
                                );
                                crate::commands::nodes::sync_crdt_snapshot_replace(
                                    &db,
                                    &node_id,
                                    &file_content,
                                );
                            }
                            Err(_panic) => {
                                error!(
                                    "CRDT panic for {}, resetting doc",
                                    node_id
                                );
                                crate::commands::nodes::sync_crdt_snapshot_replace(
                                    &db,
                                    &node_id,
                                    &file_content,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "No CRDT doc for {}: {}, creating fresh",
                            node_id, e
                        );
                        crate::commands::nodes::sync_crdt_snapshot_replace(
                            &db,
                            &node_id,
                            &file_content,
                        );
                    }
                }
            }
        }

        // (i) Export snapshot
        let snapshot = {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            match db.get_crdt_doc(&node_id) {
                Ok(doc) => doc.export_snapshot(),
                Err(e) => {
                    result.errors.push(format!(
                        "CRDT export failed for {}: {}",
                        node_id, e
                    ));
                    continue;
                }
            }
        };

        // (j) Build payload
        let sync_payload = DocSyncPayload {
            node_id: node_id.clone(),
            rel_path: rel_path.clone(),
            snapshot,
            is_json,
        };

        // (k) Serialize
        let serialized = postcard::to_stdvec(&sync_payload).map_err(|e| {
            AppError::SyncError(format!(
                "Payload serialize failed for {}: {}",
                node_id, e
            ))
        })?;

        // (l) Encrypt
        let encrypted =
            crate::sync::crypto::encrypt_v5(&e2ee_key, &serialized, true).map_err(
                |e| {
                    AppError::SyncError(format!(
                        "Encryption failed for {}: {}",
                        rel_path, e
                    ))
                },
            )?;
        result.tx_bytes += encrypted.len() as u64;

        // (m) Compute doc_hash
        let hash = doc_hash(rel_path);

        doc_batch.push((hash, encrypted));
        doc_batch_info.push((
            rel_path.clone(),
            current_sha.clone(),
            push_idx as u32,
        ));

        // Flush batch if full
        if doc_batch.len() >= 50 {
            match transport.push_doc_batch(doc_batch.clone()).await {
                Ok(seq) => {
                    if seq > max_seq {
                        max_seq = seq;
                    }
                    let db_state = app_handle.state::<crate::db::DbState>();
                    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    for (path, sha, idx) in &doc_batch_info {
                        info!("PUSH BATCH OK: {} → max_seq={}", path, seq);
                        let _ = db.set_kv(&format!("p2p_sync:sha256:{}", path), sha);
                        result.pushed += 1;
                        let _ = app_handle.emit("sync-progress", SyncProgressEvent {
                            phase: SyncPhase::Push,
                            current: *idx + 1,
                            total: push_total,
                            file_name: path.clone(),
                            bytes_transferred: 0,
                        });
                    }
                }
                Err(e) => {
                    let err_msg = format!("{}", e);
                    if err_msg.contains("QuotaExceeded") {
                        warn!("PUSH BATCH: quota exceeded");
                        result.errors.push("Storage quota exceeded while pushing batch".to_string());
                    } else {
                        warn!("PUSH BATCH failed: {}", e);
                        result.errors.push(format!("Push batch failed: {}", e));
                    }
                }
            }
            doc_batch.clear();
            doc_batch_info.clear();
        }
    }

    // Flush any remaining items in the batch
    if !doc_batch.is_empty() {
        match transport.push_doc_batch(doc_batch.clone()).await {
            Ok(seq) => {
                if seq > max_seq {
                    max_seq = seq;
                }
                let db_state = app_handle.state::<crate::db::DbState>();
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                for (path, sha, idx) in &doc_batch_info {
                    info!("PUSH BATCH OK: {} → max_seq={}", path, seq);
                    let _ = db.set_kv(&format!("p2p_sync:sha256:{}", path), sha);
                    result.pushed += 1;
                    let _ = app_handle.emit("sync-progress", SyncProgressEvent {
                        phase: SyncPhase::Push,
                        current: *idx + 1,
                        total: push_total,
                        file_name: path.clone(),
                        bytes_transferred: 0,
                    });
                }
            }
            Err(e) => {
                let err_msg = format!("{}", e);
                if err_msg.contains("QuotaExceeded") {
                    warn!("PUSH BATCH: quota exceeded");
                    result.errors.push("Storage quota exceeded while pushing batch".to_string());
                } else {
                    warn!("PUSH BATCH failed: {}", e);
                    result.errors.push(format!("Push batch failed: {}", e));
                }
            }
        }
        doc_batch.clear();
        doc_batch_info.clear();
    }


    // ════════════════════════════════════════════════════════
    // ACK phase
    // ════════════════════════════════════════════════════════

    if max_seq > cursor {
        // 1. Persist cursor locally
        {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let _ = db.set_kv("p2p_sync:cursor", &max_seq.to_string());
        }

        // 2. Send ACK to server
        if let Err(e) = transport.ack(max_seq).await {
            warn!("Failed to send ACK for {}: {}", max_seq, e);
            result.errors.push(format!("ACK {}: {}", max_seq, e));
        } else {
            info!("ACK: up_to_seq={}", max_seq);
        }
    }

    // Emit completion event
    let _ = app_handle.emit("sync-progress", SyncProgressEvent {
        phase: SyncPhase::Complete,
        current: 0,
        total: 0,
        file_name: String::new(),
        bytes_transferred: 0,
    });

    info!(
        "P2P sync complete: pulled={} pushed={} deleted={} errors={}",
        result.pulled,
        result.pushed,
        result.deleted,
        result.errors.len()
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Pull helpers
// ---------------------------------------------------------------------------

/// Pull a JSON/canvas file using LWW (last-write-wins on `metadata.updated_at`).
fn pull_json(
    app_handle: &tauri::AppHandle,
    _vault_path: &str,
    local_path: &Path,
    node_id: &str,
    payload: &DocSyncPayload,
) -> AppResult<()> {
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

    // Reset CRDT — JSON uses snapshot replacement, not incremental merge
    let _ = db.delete_crdt_doc(node_id);

    // Import remote snapshot into a fresh doc to extract text
    let fresh_doc = loro::LoroDoc::new();
    if let Ok(peer_id) = db.get_or_create_peer_id() {
        fresh_doc.set_peer_id(peer_id).ok();
    }

    let remote_text = if fresh_doc.import(&payload.snapshot).is_ok() {
        fresh_doc.get_text("content").to_string()
    } else {
        // Fallback: treat snapshot as raw UTF-8
        String::from_utf8_lossy(&payload.snapshot).to_string()
    };

    // Conflict resolution: compare timestamps if local file already exists
    let final_text = if local_path.exists() {
        let local_text = fs::read_to_string(local_path).unwrap_or_default();
        let local_ts = extract_json_updated_at(&local_text);
        let remote_ts = extract_json_updated_at(&remote_text);

        if remote_ts >= local_ts {
            info!(
                "JSON LWW for {}: remote wins (remote={} >= local={})",
                node_id, remote_ts, local_ts
            );
            remote_text
        } else {
            info!(
                "JSON LWW for {}: local wins (local={} > remote={})",
                node_id, local_ts, remote_ts
            );
            local_text
        }
    } else {
        remote_text
    };

    // Atomic write
    atomic_write(local_path, final_text.as_bytes()).map_err(|e| {
        AppError::SyncError(format!("Write JSON {}: {}", node_id, e))
    })?;

    // Save a fresh CRDT snapshot from the final winner
    let new_doc = loro::LoroDoc::new();
    if let Ok(peer_id) = db.get_or_create_peer_id() {
        new_doc.set_peer_id(peer_id).ok();
    }
    let text_handler = new_doc.get_text("content");
    if text_handler.insert(0, &final_text).is_ok() {
        new_doc.commit();
        let _ = db.save_crdt_snapshot(node_id, new_doc.export_snapshot());
    }

    Ok(())
}

/// Pull a Markdown file using CRDT merge (conflict-free character-level).
fn pull_markdown(
    app_handle: &tauri::AppHandle,
    _vault_path: &str,
    local_path: &Path,
    node_id: &str,
    payload: &DocSyncPayload,
    result: &mut SyncResult,
) -> AppResult<()> {
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

    match db.get_crdt_doc(node_id) {
        Ok(doc) => {
            // Local CRDT exists → merge
            let doc_ref = &doc;
            let snapshot_ref = &payload.snapshot;
            let merge_result = std::panic::catch_unwind(
                std::panic::AssertUnwindSafe(|| {
                    crate::crdt_bridge::merge_remote_snapshot(
                        doc_ref,
                        snapshot_ref,
                    )
                }),
            );

            match merge_result {
                Ok(Ok((delta, merged_text))) => {
                    if let Err(e) = db.save_crdt_delta(node_id, delta) {
                        warn!("CRDT delta save failed for {}: {}", node_id, e);
                    }
                    atomic_write(local_path, merged_text.as_bytes())
                        .map_err(|e| {
                            AppError::SyncError(format!(
                                "Write merged {}: {}",
                                node_id, e
                            ))
                        })?;
                    // Emit CRDT merge conflict info
                    let _ = app_handle.emit("sync-conflict", SyncConflictInfo {
                        file_name: payload.rel_path.clone(),
                        resolution: "crdt_merge".to_string(),
                    });
                }
                Ok(Err(e)) => {
                    warn!(
                        "CRDT merge failed for {}: {}, falling back to remote",
                        node_id, e
                    );
                    pull_markdown_reset(
                        &db, local_path, node_id, payload, result,
                    );
                }
                Err(_panic) => {
                    error!(
                        "CRDT merge panicked for {}, resetting to remote snapshot",
                        node_id
                    );
                    pull_markdown_reset(
                        &db, local_path, node_id, payload, result,
                    );
                }
            }
        }
        Err(_) => {
            // No local CRDT — save remote snapshot and extract text
            let _ = db.save_crdt_snapshot(
                node_id,
                payload.snapshot.clone(),
            );
            match db.get_crdt_doc(node_id) {
                Ok(doc) => {
                    let text = doc.get_text("content").to_string();
                    if let Err(e) =
                        atomic_write(local_path, text.as_bytes())
                    {
                        result.errors.push(format!(
                            "Write {}: {}",
                            node_id, e
                        ));
                    }
                }
                Err(e) => {
                    result.errors.push(format!(
                        "CRDT load after save for {}: {}",
                        node_id, e
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Fallback: reset CRDT doc and write remote content directly.
fn pull_markdown_reset(
    db: &crate::db::DbBridge,
    local_path: &Path,
    node_id: &str,
    payload: &DocSyncPayload,
    result: &mut SyncResult,
) {
    let _ = db.delete_crdt_doc(node_id);
    let _ = db.save_crdt_snapshot(node_id, payload.snapshot.clone());
    match db.get_crdt_doc(node_id) {
        Ok(doc) => {
            let text = doc.get_text("content").to_string();
            if let Err(e) = atomic_write(local_path, text.as_bytes()) {
                result
                    .errors
                    .push(format!("Write reset {}: {}", node_id, e));
            }
        }
        Err(e) => {
            result.errors.push(format!(
                "CRDT reload failed for {}: {}",
                node_id, e
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_hash_deterministic() {
        let h1 = doc_hash("Notes/diary.md");
        let h2 = doc_hash("Notes/diary.md");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_doc_hash_different_paths() {
        let h1 = doc_hash("Notes/diary.md");
        let h2 = doc_hash("Tasks/todo.md");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_extract_json_updated_at_present() {
        let json = r#"{"title":"Test","metadata":{"updated_at":"2026-06-24T12:00:00Z"}}"#;
        assert_eq!(
            extract_json_updated_at(json),
            "2026-06-24T12:00:00Z"
        );
    }

    #[test]
    fn test_extract_json_updated_at_missing() {
        let json = r#"{"title":"Test"}"#;
        assert_eq!(extract_json_updated_at(json), "");
    }

    #[test]
    fn test_is_json_file() {
        assert!(is_json_file("Tasks/meeting.json"));
        assert!(is_json_file("canvas/board.canvas"));
        assert!(!is_json_file("Notes/diary.md"));
    }

    #[test]
    fn test_is_asset() {
        assert!(is_asset("assets/image.png"));
        assert!(!is_asset("Notes/diary.md"));
    }

    #[test]
    fn test_payload_roundtrip() {
        let payload = DocSyncPayload {
            doc_id: "Notes/test.md".to_string(),
            snapshot: vec![1, 2, 3],
            is_json: false,
        };
        let bytes = postcard::to_stdvec(&payload).unwrap();
        let decoded: DocSyncPayload = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.doc_id, "Notes/test.md");
        assert_eq!(decoded.snapshot, vec![1, 2, 3]);
        assert!(!decoded.is_json);
    }
}
