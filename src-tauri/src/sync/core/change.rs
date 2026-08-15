use crate::db::DbState;
use crate::error::AppResult;
use crate::sync::core::crdt::apply_text_update;
use crate::sync::core::types::SyncOperation;
use crate::sync::utils::{collect_local_files, file_sha256};
use std::fs;
use std::path::Path;
use tauri::Manager;

pub struct LocalChange {
    pub rel_path: String,
    pub is_delete: bool,
    pub new_hash: String,
}

/// Below this many tracked documents, an empty vault says nothing: deleting the
/// only note you had is ordinary.
const MIN_TRACKED_FOR_EMPTY_VAULT_GUARD: usize = 2;

pub fn detect_local_changes<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    vault: &Path,
    vault_id: &str,
    provider_id: &str,
    local_files: &[String],
) -> AppResult<Vec<LocalChange>> {
    let mut changes = Vec::new();

    for rel_path in local_files.iter().cloned() {
        let file_path = vault.join(&rel_path);
        let current_hash = file_sha256(&file_path);

        let stored_hash = {
            let db_state = app_handle.state::<DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_document_baseline(vault_id, provider_id, &rel_path)?
                .unwrap_or_default()
        };

        if current_hash != stored_hash {
            changes.push(LocalChange {
                rel_path,
                is_delete: false,
                new_hash: current_hash,
            });
        }
    }

    Ok(changes)
}

/// Work out which tracked documents have disappeared from disk.
///
/// A missing file is normally a deletion, but it is also what an unmounted
/// drive, a half-populated cloud folder or a vault pointed at the wrong
/// directory looks like — and in those cases publishing a tombstone for every
/// document destroys the vault on every other device. When the evidence points
/// at an absent vault rather than an intentional cleanup, this refuses to
/// produce deletions at all and fails the sync instead, which is recoverable.
pub fn detect_deletions<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    vault: &Path,
    vault_id: &str,
    local_files: &[String],
) -> AppResult<Vec<LocalChange>> {
    let mut deletions = Vec::new();

    let paths = {
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_document_paths(vault_id)?
    };

    let tracked = paths.len();

    for (_doc_id, path) in paths {
        let file_path = vault.join(&path);
        if !file_path.exists() {
            deletions.push(LocalChange {
                rel_path: path,
                is_delete: true,
                new_hash: String::new(),
            });
        }
    }

    // A vault that still tracks documents but currently holds no files at all is
    // far more likely to be unmounted than emptied on purpose. Stop the first
    // time and say so; if the next sync sees the same thing, take it as
    // confirmation and let the deletions through, so a genuine "delete
    // everything" is delayed rather than blocked.
    let vault_looks_absent =
        local_files.is_empty() && tracked >= MIN_TRACKED_FOR_EMPTY_VAULT_GUARD;
    let warn_key = format!("sync:empty_vault_warned:{}", vault_id);

    {
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        if vault_looks_absent {
            let already_warned = db.get_kv(&warn_key)?.is_some();
            if !already_warned {
                db.set_kv(&warn_key, &chrono::Utc::now().timestamp_millis().to_string())?;
                return Err(crate::error::AppError::SyncError(format!(
                    "Stopped before deleting anything: the vault at '{}' holds no \
                     files, but {} document(s) are tracked here. If the drive or \
                     folder simply is not mounted, reconnect it and sync again. If \
                     you really did empty this vault, sync once more to confirm.",
                    vault.display(),
                    tracked
                )));
            }
        } else if !local_files.is_empty() {
            // The vault is back to normal, so a later disappearance warns again.
            db.delete_kv(&warn_key)?;
        }
    }

    Ok(deletions)
}

pub fn encode_sync_payload_v5(
    e2ee_key: &[u8; 32],
    sync_payload: &crate::sync::core::types::SyncPayload,
    compress: bool,
) -> AppResult<(Vec<u8>, [u8; 32])> {
    let payload_bytes = postcard::to_stdvec(sync_payload).map_err(|e| {
        crate::error::AppError::General(format!("Postcard SyncPayload encode error: {}", e))
    })?;
    let encrypted_payload =
        crate::sync::core::crypto::encrypt_v5(e2ee_key, &payload_bytes, compress)
            .map_err(|e| crate::error::AppError::General(format!("Encryption error: {:?}", e)))?;
    let payload_hash = *blake3::hash(&encrypted_payload).as_bytes();
    Ok((encrypted_payload, payload_hash))
}

pub const DELETE_SOURCE_HASH_DOMAIN_V1: &[u8] = b"synabit-delete-v1:";

pub fn delete_source_hash(
    vault_id: &str,
    provider_id: &str,
    node_id: &str,
    rel_path: &str,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DELETE_SOURCE_HASH_DOMAIN_V1);
    hasher.update(&(vault_id.len() as u64).to_le_bytes());
    hasher.update(vault_id.as_bytes());
    hasher.update(&(provider_id.len() as u64).to_le_bytes());
    hasher.update(provider_id.as_bytes());
    hasher.update(&(node_id.len() as u64).to_le_bytes());
    hasher.update(node_id.as_bytes());
    hasher.update(&(rel_path.len() as u64).to_le_bytes());
    hasher.update(rel_path.as_bytes());
    *hasher.finalize().as_bytes()
}

pub fn prepare_durable_outbox_operations(
    db_state: &crate::db::DbState,
    vault: &Path,
    changes: Vec<LocalChange>,
    e2ee_key: &[u8; 32],
    vault_id: &str,
    provider_id: &str,
) -> AppResult<()> {
    for change in changes {
        let doc_hash = *blake3::hash(change.rel_path.as_bytes()).as_bytes();
        let timestamp = chrono::Utc::now().timestamp_millis();

        if change.is_delete {
            let actual_node_id = {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                match db.get_node_id_by_path(vault_id, &change.rel_path)? {
                    Some(id) => id,
                    None => {
                        return Err(crate::error::AppError::General(format!(
                            "Cannot find node_id for deleted file {}",
                            change.rel_path
                        )));
                    }
                }
            };
            let delete_payload =
                crate::sync::core::types::SyncPayload::Delete(synabit_protocol::DeletePayload {
                    node_id: actual_node_id.clone(),
                    rel_path: change.rel_path.clone(),
                });
            let (encrypted_payload, payload_hash) =
                encode_sync_payload_v5(e2ee_key, &delete_payload, true)?;
            let delete_hash =
                delete_source_hash(vault_id, provider_id, &actual_node_id, &change.rel_path);

            let record = crate::db::sync_outbox::OutboxRecord {
                vault_id: vault_id.to_string(),
                provider_id: provider_id.to_string(),
                operation_id: uuid::Uuid::new_v4().into_bytes(),
                entry_kind: crate::sync::core::types::SyncEntryKind::Delete,
                node_id: actual_node_id.clone(),
                rel_path: Some(change.rel_path.clone()),
                doc_hash: Some(doc_hash),
                source_hash: Some(delete_hash),
                original_timestamp: timestamp,
                encrypted_payload: Some(encrypted_payload),
                payload_hash: Some(payload_hash),
                asset_ref_blob: None,
                state: crate::db::sync_outbox::OutboxState::Ready,
                retry_count: 0,
                next_retry_at: None,
                last_error: None,
                created_at: timestamp,
                updated_at: timestamp,
            };
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.enqueue_or_reuse_outbox_operation(&record)?;
            continue;
        }

        // 1. Detected-hash validation (hex::decode BEFORE any path mutation)
        let detected_hash_vec = hex::decode(&change.new_hash).map_err(|e| {
            crate::error::AppError::General(format!("Invalid hex in new_hash: {}", e))
        })?;
        if detected_hash_vec.len() != 32 {
            return Err(crate::error::AppError::General(format!(
                "source_hash must be exactly 32 bytes, got {}",
                detected_hash_vec.len()
            )));
        }

        // 2. File read / type validation (fs::read BEFORE any path mutation)
        let file_path = vault.join(&change.rel_path);
        let content = fs::read(&file_path).map_err(|e| {
            crate::error::AppError::General(format!(
                "fs::read failed for {}: {}",
                change.rel_path, e
            ))
        })?;
        String::from_utf8(content).map_err(|e| {
            crate::error::AppError::General(format!(
                "UTF-8 decode failed for file {}: {}",
                change.rel_path, e
            ))
        })?;

        // 3. Get/assign node_id and mutate durable path AFTER validations pass
        let actual_node_id = crate::sync::core::identity::get_or_assign_node_id(vault, &file_path)?;

        // 4. `get_or_assign_node_id` may have rewritten the file to inject the
        //    id, so both the text we publish and the hash we record must
        //    describe the file as it is now. Publishing the pre-injection text
        //    makes every peer mint its own id for this document; recording the
        //    pre-injection hash makes the file look changed on every later
        //    sync, which re-pushes it forever.
        let text = fs::read_to_string(&file_path).map_err(|e| {
            crate::error::AppError::General(format!(
                "fs::read failed for {} after identity assignment: {}",
                change.rel_path, e
            ))
        })?;

        let published_hash_vec = hex::decode(crate::sync::utils::sha256_hex(text.as_bytes()))
            .map_err(|e| {
                crate::error::AppError::General(format!("Invalid hex in published hash: {}", e))
            })?;
        if published_hash_vec.len() != 32 {
            return Err(crate::error::AppError::General(format!(
                "source_hash must be exactly 32 bytes, got {}",
                published_hash_vec.len()
            )));
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&published_hash_vec);
        let source_hash = Some(buf);

        {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.upsert_document_path(vault_id, &actual_node_id, &change.rel_path)?;
        }

        let doc = {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_crdt_doc(vault_id, &actual_node_id)?
        };

        let _delta = apply_text_update(&doc, &text).map_err(crate::error::AppError::General)?;

        if !_delta.is_empty() {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.save_crdt_delta(vault_id, &actual_node_id, _delta)?;
        }

        let snapshot = doc.export_snapshot();

        let is_json = change.rel_path.ends_with(".json") || change.rel_path.ends_with(".canvas");
        let doc_payload = crate::sync::core::types::DocSyncPayload {
            node_id: actual_node_id.clone(),
            rel_path: change.rel_path.clone(),
            snapshot,
            is_json,
        };

        let doc_bytes = postcard::to_stdvec(&doc_payload)
            .map_err(|e| crate::error::AppError::General(format!("Postcard error: {}", e)))?;

        let sync_payload = crate::sync::core::types::SyncPayload::Upsert(doc_bytes);
        let (encrypted_payload, payload_hash) =
            encode_sync_payload_v5(e2ee_key, &sync_payload, true)?;

        let record = crate::db::sync_outbox::OutboxRecord {
            vault_id: vault_id.to_string(),
            provider_id: provider_id.to_string(),
            operation_id: uuid::Uuid::new_v4().into_bytes(),
            entry_kind: crate::sync::core::types::SyncEntryKind::Upsert,
            node_id: actual_node_id.clone(),
            rel_path: Some(change.rel_path.clone()),
            doc_hash: Some(doc_hash),
            source_hash,
            original_timestamp: timestamp,
            encrypted_payload: Some(encrypted_payload),
            payload_hash: Some(payload_hash),
            asset_ref_blob: None,
            state: crate::db::sync_outbox::OutboxState::Ready,
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            created_at: timestamp,
            updated_at: timestamp,
        };

        let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.enqueue_or_reuse_outbox_operation(&record)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_preparation_does_not_advance_baseline_before_acceptance() {
        let tempdir = tempfile::tempdir().unwrap();
        let vault_path = tempdir.path();
        let file_path = vault_path.join("test.md");
        fs::write(&file_path, "hello world").unwrap();
        let source_hash = crate::sync::utils::file_sha256(&file_path);

        let db = crate::db::sync_outbox::tests::setup_test_db();
        let db_state = std::sync::Arc::new(std::sync::Mutex::new(db));

        let change = LocalChange {
            rel_path: "test.md".into(),
            is_delete: false,
            new_hash: source_hash.clone(),
        };

        let e2ee_key = [1u8; 32];
        prepare_durable_outbox_operations(
            &db_state,
            vault_path,
            vec![change],
            &e2ee_key,
            "v1",
            "gdrive",
        )
        .unwrap();

        let baseline_before = db_state
            .lock()
            .unwrap()
            .get_document_baseline("v1", "gdrive", "test.md")
            .unwrap();
        assert!(baseline_before.is_none());

        let outbox_records = db_state
            .lock()
            .unwrap()
            .get_dispatchable_outbox("v1", "gdrive", 10000, 10)
            .unwrap();
        assert_eq!(outbox_records.len(), 1);
        let rec = &outbox_records[0];

        db_state
            .lock()
            .unwrap()
            .mark_outbox_batch_sent("v1", "gdrive", &[rec.operation_id], 1000)
            .unwrap();
        db_state
            .lock()
            .unwrap()
            .commit_accepted_outbox_operation(rec, 1000)
            .unwrap();

        let baseline_after = db_state
            .lock()
            .unwrap()
            .get_document_baseline("v1", "gdrive", "test.md")
            .unwrap();
        assert!(baseline_after.is_some());
    }

    #[test]
    fn delete_source_identity_is_deterministic_and_invalid_inputs_fail() {
        let h1 = delete_source_hash("v1", "gdrive", "node1", "path.md");
        let h2 = delete_source_hash("v1", "gdrive", "node1", "path.md");
        let h3 = delete_source_hash("v1", "gdrive", "node2", "path.md");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);

        let tempdir = tempfile::tempdir().unwrap();
        let vault_path = tempdir.path();

        let db = crate::db::sync_outbox::tests::setup_test_db();
        let db_state = std::sync::Arc::new(std::sync::Mutex::new(db));

        let before = db_state
            .lock()
            .unwrap()
            .get_dispatchable_outbox("v1", "gdrive", 10000, 10)
            .unwrap();

        let invalid = LocalChange {
            rel_path: "nonexistent.md".into(),
            is_delete: false,
            new_hash: "invalid_hex_string".into(),
        };

        let e2ee_key = [1u8; 32];
        let res = prepare_durable_outbox_operations(
            &db_state,
            vault_path,
            vec![invalid],
            &e2ee_key,
            "v1",
            "gdrive",
        );
        assert!(res.is_err());

        let after = db_state
            .lock()
            .unwrap()
            .get_dispatchable_outbox("v1", "gdrive", 10000, 10)
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn delete_source_hash_is_unambiguous_for_delimiter_counterexamples() {
        let h1 = delete_source_hash("v1", "gdrive", "a:b", "c");
        let h2 = delete_source_hash("v1", "gdrive", "a", "b:c");
        assert_ne!(h1, h2);

        let h3 = delete_source_hash("v1", "gdrive", "a:b", "c");
        assert_eq!(h1, h3);

        let h_diff_vault = delete_source_hash("v2", "gdrive", "a:b", "c");
        assert_ne!(h1, h_diff_vault);

        let h_diff_provider = delete_source_hash("v1", "server", "a:b", "c");
        assert_ne!(h1, h_diff_provider);

        let h_diff_path = delete_source_hash("v1", "gdrive", "a:b", "d");
        assert_ne!(h1, h_diff_path);
    }
}
