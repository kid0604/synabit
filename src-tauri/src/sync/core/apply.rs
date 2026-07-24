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

use std::fs;
use std::path::Path;

use log::{error, info, warn};
use tauri::{Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::sync::progress::SyncConflictInfo;
use crate::sync::SyncResult;
use crate::sync::utils::file_sha256;

// ---------------------------------------------------------------------------
// Payload types
// ---------------------------------------------------------------------------

/// Encrypted payload inner structure. Serialized with postcard, encrypted
/// with XChaCha20-Poly1305, then pushed to the transport.
///
/// `doc_id` is embedded inside the ciphertext so the receiver can map
/// `doc_hash → relative file path` without any server-side metadata.
// Using DocSyncPayload from types

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

pub fn apply_doc_payload(
    app_handle: &tauri::AppHandle,
    vault: &Path,
    vault_path: &str,
    payload: &crate::sync::core::types::DocSyncPayload,
    result: &mut SyncResult,
) {
    let node_id = &payload.node_id;
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
        let _ = pull_json(app_handle, vault_path, &local_path, node_id, payload);
    } else {
        let _ = pull_markdown(
            app_handle,
            vault_path,
            &local_path,
            node_id,
            payload,
            result,
        );
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

// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Pull helpers
// ---------------------------------------------------------------------------

/// Pull a JSON/canvas file using LWW (last-write-wins on `metadata.updated_at`).
fn pull_json(
    app_handle: &tauri::AppHandle,
    _vault_path: &str,
    local_path: &Path,
    node_id: &str,
    payload: &crate::sync::core::types::DocSyncPayload,
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

    let local_text = fs::read_to_string(local_path).unwrap_or_default();
    
    // Conflict resolution: compare timestamps if local file already exists
    let final_text = if local_path.exists() {
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
            local_text.clone()
        }
    } else {
        remote_text
    };

    if final_text != local_text {
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
    }

    Ok(())
}

/// Pull a Markdown file using CRDT merge (conflict-free character-level).
fn pull_markdown(
    app_handle: &tauri::AppHandle,
    _vault_path: &str,
    local_path: &Path,
    node_id: &str,
    payload: &crate::sync::core::types::DocSyncPayload,
    result: &mut SyncResult,
) -> AppResult<()> {
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

    match db.get_crdt_doc(node_id) {
        Ok(doc) => {
            // Local CRDT exists → merge
            let merge_result = std::panic::catch_unwind(
                std::panic::AssertUnwindSafe(|| {
                    if let Err(e) = doc.import(&payload.snapshot) {
                        return Err(format!("CRDT import error: {:?}", e));
                    }
                    let text = doc.get_text("content").to_string();
                    let delta = doc.export_snapshot();
                    Ok((delta, text))
                }),
            );

            match merge_result {
                Ok(Ok((delta, merged_text))) => {
                    if !delta.is_empty() {
                        if let Err(e) = db.save_crdt_delta(node_id, delta) {
                            warn!("CRDT delta save failed for {}: {}", node_id, e);
                        }
                    }
                    
                    drop(db);

                    let local_text = std::fs::read_to_string(local_path).unwrap_or_default();
                    if merged_text != local_text {
                        atomic_write(local_path, merged_text.as_bytes())
                            .map_err(|e| {
                                AppError::SyncError(format!(
                                    "Write merged {}: {}",
                                    node_id, e
                                ))
                            })?;
                        // Emit CRDT merge conflict info only if actual changes occurred
                        let _ = app_handle.emit("sync-conflict", SyncConflictInfo {
                            file_name: payload.rel_path.clone(),
                            resolution: "crdt_merge".to_string(),
                        });
                    }
                }
                Ok(Err(e)) => {
                    warn!(
                        "CRDT merge failed for {}: {}, falling back to remote",
                        node_id, e
                    );
                    drop(db);
                    pull_markdown_reset(
                        app_handle, local_path, node_id, payload, result,
                    );
                }
                Err(_panic) => {
                    error!(
                        "CRDT merge panicked for {}, resetting to remote snapshot",
                        node_id
                    );
                    drop(db);
                    pull_markdown_reset(
                        app_handle, local_path, node_id, payload, result,
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
                    drop(db);
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
    app_handle: &tauri::AppHandle,
    local_path: &Path,
    node_id: &str,
    payload: &crate::sync::core::types::DocSyncPayload,
    result: &mut SyncResult,
) {
    let text = {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.delete_crdt_doc(node_id);
        let _ = db.save_crdt_snapshot(node_id, payload.snapshot.clone());
        match db.get_crdt_doc(node_id) {
            Ok(doc) => doc.get_text("content").to_string(),
            Err(e) => {
                result.errors.push(format!(
                    "CRDT reload failed for {}: {}",
                    node_id, e
                ));
                return;
            }
        }
    };

    let local_text = std::fs::read_to_string(local_path).unwrap_or_default();
    if text != local_text {
        if let Err(e) = atomic_write(local_path, text.as_bytes()) {
            result
                .errors
                .push(format!("Write reset {}: {}", node_id, e));
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
            node_id: "5da91532-a7f0-4990-86de-dc43f084bd08".to_string(),
            rel_path: "Notes/test.md".to_string(),
            snapshot: vec![1, 2, 3],
            is_json: false,
        };
        let bytes = postcard::to_stdvec(&payload).unwrap();
        let decoded: DocSyncPayload = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.node_id, payload.node_id);
        assert_eq!(decoded.rel_path, "Notes/test.md");
        assert_eq!(decoded.snapshot, vec![1, 2, 3]);
        assert!(!decoded.is_json);
    }

    /// Phase 0 baseline for the direct-v1 data-loss report.
    ///
    /// The model mirrors the production ordering in this engine and handler:
    /// A pushes its edited snapshot, B stages without applying it, then A pulls
    /// B's still-stale local snapshot. Phase 1 must make this test pass before
    /// the ignore marker is removed.
    #[test]
    #[ignore = "known direct-P2P v1 rollback; enable after Phase 1 reorders durable apply/pull/push"]
    fn direct_v1_new_note_edit_must_not_roll_back() {
        #[derive(Clone)]
        struct LegacyPeer {
            local: String,
            staged: Option<String>,
        }

        fn legacy_push_then_pull(initiator: &mut LegacyPeer, responder: &mut LegacyPeer) {
            responder.staged = Some(initiator.local.clone());
            initiator.local = responder.local.clone();
        }

        let placeholder = "---\ntitle: Untitled 1784051721342\n---\n\n";
        let edited = "---\ntitle: Maintainer plan\n---\n\nP2P content must survive.\n";
        let mut device_a = LegacyPeer {
            local: edited.to_string(),
            staged: None,
        };
        let mut device_b = LegacyPeer {
            local: placeholder.to_string(),
            staged: None,
        };

        legacy_push_then_pull(&mut device_a, &mut device_b);

        assert_eq!(
            device_a.local,
            edited,
            "device A pulled B's stale placeholder after pushing the edited note"
        );
    }
}
