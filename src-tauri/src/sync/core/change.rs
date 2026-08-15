use crate::db::DbState;
use crate::error::AppResult;
use crate::sync::core::crdt::apply_text_update;
use crate::sync::utils::file_sha256;
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

/// A file whose modification time is this recent is re-hashed even when its
/// size and timestamp match the cache.
///
/// Filesystems record modification times at limited resolution, so a write that
/// lands in the same tick as the one we recorded would be invisible to a
/// metadata comparison. Waiting until the tick has demonstrably passed removes
/// that window, and costs nothing in practice: a file touched in the last two
/// seconds is exactly the file we expect to be hashing anyway.
const STAT_CACHE_SETTLE_MS: i64 = 2_000;

/// May the cached digest stand in for hashing this file?
///
/// Only when size and modification time both match what was recorded *and* that
/// timestamp is old enough that no write could still be hiding inside the same
/// filesystem tick. Getting this wrong in the permissive direction means an edit
/// never syncs, so every uncertain case answers `false`.
fn stat_cache_hit(
    observed: (u64, i64),
    entry: &crate::db::StatCacheEntry,
    now_ms: i64,
) -> bool {
    let (size, mtime_ms) = observed;
    entry.file_size == size
        && entry.mtime_ms == mtime_ms
        && now_ms.saturating_sub(mtime_ms) > STAT_CACHE_SETTLE_MS
}

fn file_stat(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some((meta.len(), mtime))
}

pub fn detect_local_changes<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    vault: &Path,
    vault_id: &str,
    provider_id: &str,
    local_files: &[String],
) -> AppResult<Vec<LocalChange>> {
    let db_state = app_handle.state::<DbState>();

    // One query each instead of one database lock per file.
    let (baselines, stat_cache) = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        (
            db.load_document_baselines(vault_id, provider_id)?,
            db.load_stat_cache(vault_id, provider_id)?,
        )
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut changes = Vec::new();
    let mut fresh_cache: Vec<(String, crate::db::StatCacheEntry)> =
        Vec::with_capacity(local_files.len());

    for rel_path in local_files.iter().cloned() {
        let file_path = vault.join(&rel_path);
        let stat = file_stat(&file_path);

        // Reuse the recorded digest when the file demonstrably has not been
        // touched since we took it. Anything uncertain falls through to a hash.
        let cached = match (&stat, stat_cache.get(&rel_path)) {
            (Some(observed), Some(entry)) if stat_cache_hit(*observed, entry, now_ms) => {
                Some(entry.content_hash.clone())
            }
            _ => None,
        };

        let current_hash = match cached {
            Some(hash) => hash,
            None => file_sha256(&file_path),
        };

        if let Some((size, mtime)) = stat {
            if !current_hash.is_empty() {
                fresh_cache.push((
                    rel_path.clone(),
                    crate::db::StatCacheEntry {
                        file_size: size,
                        mtime_ms: mtime,
                        content_hash: current_hash.clone(),
                    },
                ));
            }
        }

        let stored_hash = baselines.get(&rel_path).cloned().unwrap_or_default();
        if current_hash != stored_hash {
            changes.push(LocalChange {
                rel_path,
                is_delete: false,
                new_hash: current_hash,
            });
        }
    }

    {
        let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.replace_stat_cache(vault_id, provider_id, &fresh_cache, now_ms)?;
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

/// Queue an attachment for publication.
///
/// The reference is stored now and the chunks are uploaded at dispatch time,
/// which is why the entry starts in `Prepared` rather than `Ready`: a reference
/// that reached a peer before its bytes did would be a reference to nothing.
///
/// Chunking is deterministic, so the bytes need not be held anywhere in the
/// meantime — they are regenerated from the file when it is time to upload.
fn prepare_asset_operation(
    db_state: &crate::db::DbState,
    vault: &Path,
    change: &LocalChange,
    e2ee_key: &[u8; 32],
    vault_id: &str,
    provider_id: &str,
) -> AppResult<()> {
    let file_path = vault.join(&change.rel_path);

    // Asked of the filesystem, not of the bytes: reading a four-gigabyte video
    // to discover it is too large to send is the failure this prevents.
    let size = fs::metadata(&file_path).map(|m| m.len()).map_err(|e| {
        crate::error::AppError::General(format!("cannot stat {}: {}", change.rel_path, e))
    })?;
    crate::sync::core::asset::check_size(&change.rel_path, size)?;

    let contents = fs::read(&file_path).map_err(|e| {
        crate::error::AppError::General(format!("fs::read failed for {}: {}", change.rel_path, e))
    })?;

    // An attachment is identified by its path: there is nowhere inside a JPEG
    // to record an id without altering the file.
    let node_id = change.rel_path.clone();
    let (asset, _chunks) =
        crate::sync::core::asset::prepare(e2ee_key, &change.rel_path, &node_id, &contents)?;

    let asset_ref_blob = postcard::to_stdvec(&asset).map_err(|e| {
        crate::error::AppError::General(format!("Postcard AssetRef encode error: {}", e))
    })?;

    let payload = crate::sync::core::types::SyncPayload::AssetReference(asset);
    let (encrypted_payload, payload_hash) = encode_sync_payload_v5(e2ee_key, &payload, true)?;

    let published_hash = crate::sync::utils::sha256_hex(&contents);
    let mut source_hash = [0u8; 32];
    let decoded = hex::decode(&published_hash).map_err(|e| {
        crate::error::AppError::General(format!("Invalid hex in attachment hash: {}", e))
    })?;
    if decoded.len() != 32 {
        return Err(crate::error::AppError::General(
            "attachment hash must be exactly 32 bytes".into(),
        ));
    }
    source_hash.copy_from_slice(&decoded);

    let timestamp = chrono::Utc::now().timestamp_millis();
    let record = crate::db::sync_outbox::OutboxRecord {
        vault_id: vault_id.to_string(),
        provider_id: provider_id.to_string(),
        operation_id: uuid::Uuid::new_v4().into_bytes(),
        entry_kind: crate::sync::core::types::SyncEntryKind::AssetReference,
        node_id: node_id.clone(),
        rel_path: Some(change.rel_path.clone()),
        doc_hash: Some(*blake3::hash(change.rel_path.as_bytes()).as_bytes()),
        source_hash: Some(source_hash),
        original_timestamp: timestamp,
        encrypted_payload: Some(encrypted_payload),
        payload_hash: Some(payload_hash),
        asset_ref_blob: Some(asset_ref_blob),
        state: crate::db::sync_outbox::OutboxState::Prepared,
        retry_count: 0,
        next_retry_at: None,
        last_error: None,
        created_at: timestamp,
        updated_at: timestamp,
    };

    {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.upsert_document_path(vault_id, &node_id, &change.rel_path)?;
    }

    let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    db.enqueue_or_reuse_outbox_operation(&record)?;
    Ok(())
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

/// Turn detected changes into durable outbox entries.
///
/// Returns one message per file that could not be prepared. A single
/// unpublishable document must never stop the others: an unreadable file, a
/// vanished path, or a shape the identity assigner cannot handle used to abort
/// the whole run, so one bad file meant nothing in the vault synced at all.
pub fn prepare_durable_outbox_operations(
    db_state: &crate::db::DbState,
    vault: &Path,
    changes: Vec<LocalChange>,
    e2ee_key: &[u8; 32],
    vault_id: &str,
    provider_id: &str,
    supports_assets: bool,
) -> AppResult<Vec<String>> {
    let mut skipped: Vec<String> = Vec::new();

    for change in changes {
        match prepare_one_operation(
            db_state,
            vault,
            &change,
            e2ee_key,
            vault_id,
            provider_id,
            supports_assets,
        ) {
            Ok(()) => {}
            // A file this target simply cannot carry is not a failure. Reporting
            // it would put the same complaint in front of the user on every
            // sync, for the life of the vault, about something that is working
            // as intended.
            Err(crate::error::AppError::UnsupportedCapability(reason)) => {
                log::debug!("sync: {}", reason);
            }
            // Also not a failure, for the same reason: the file will be exactly
            // this large next time, so reporting it would be a permanent error
            // about a permanent fact. Logged louder than the case above because
            // this one the user can actually do something about.
            Err(crate::error::AppError::AssetTooLarge(reason)) => {
                log::warn!("sync: leaving {} local — {}", change.rel_path, reason);
            }
            Err(e) => {
                log::warn!("sync: skipping {}: {}", change.rel_path, e);
                skipped.push(format!("{}: {}", change.rel_path, e));
            }
        }
    }

    Ok(skipped)
}

fn prepare_one_operation(
    db_state: &crate::db::DbState,
    vault: &Path,
    change: &LocalChange,
    e2ee_key: &[u8; 32],
    vault_id: &str,
    provider_id: &str,
    supports_assets: bool,
) -> AppResult<()> {
    {
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
            return Ok(());
        }

        // Attachments cannot be carried as text. They are published as a small
        // reference naming encrypted chunks; the chunks themselves are uploaded
        // separately, before the reference goes out, so a peer never sees a
        // reference whose bytes are not yet fetchable.
        if !crate::sync::utils::is_syncable_document(&change.rel_path) {
            if !supports_assets {
                // Google Drive has no place to put chunks. Queueing the
                // attachment anyway would leave an entry that can never be
                // published, and report the same failure on every sync for the
                // life of the vault.
                return Err(crate::error::AppError::UnsupportedCapability(format!(
                    "{} stays local: this sync target cannot carry attachments",
                    change.rel_path
                )));
            }
            return prepare_asset_operation(db_state, vault, change, e2ee_key, vault_id, provider_id);
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
        // Reuse the identity we already recorded for this path when the file has
        // lost its own, so an unrelated rewrite cannot split one document in two.
        let known_id = {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_node_id_by_path(vault_id, &change.rel_path)?
        };
        let mut actual_node_id = crate::sync::core::identity::get_or_assign_node_id_with_hint(
            vault,
            &file_path,
            known_id.as_deref(),
        )?;

        // A file that carries an id already held by a different, still-present
        // file is a copy that brought the original's metadata with it. Both are
        // real documents, so the copy needs an identity of its own. Left alone,
        // the two overwrite each other's queued work — the outbox holds one
        // entry per document — and neither ever settles, which spins sync
        // forever.
        let clashing_owner = {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_path_by_node_id(vault_id, &actual_node_id)?
        };
        if let Some(other_path) = clashing_owner {
            if other_path != change.rel_path && vault.join(&other_path).exists() {
                log::warn!(
                    "{} claims the identity of {}; giving it a new one",
                    change.rel_path,
                    other_path
                );
                actual_node_id =
                    crate::sync::core::identity::assign_fresh_node_id(vault, &file_path)?;
            }
        }

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
            // A document that moved leaves a baseline row behind under its old
            // path; drop it so the bookkeeping matches what is on disk.
            if let Some(previous) = db.get_path_by_node_id(vault_id, &actual_node_id)? {
                if previous != change.rel_path {
                    log::info!("PUSH rename: {} -> {}", previous, change.rel_path);
                    db.delete_document_baseline(vault_id, provider_id, &previous)?;
                }
            }
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
mod detect_with_cache_tests {
    use super::*;
    use crate::db::{DbBridge, DbState};
    use tauri::Manager;

    fn app_with_vault() -> (tauri::AppHandle<tauri::test::MockRuntime>, tempfile::TempDir) {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle().clone();
        std::mem::forget(app);

        let db = DbBridge::new_in_memory_full().unwrap();
        db.insert_sync_vault_mapping(&crate::db::sync_vault::SyncVaultRecord {
            vault_id: "v1".into(),
            canonical_root: "/v1".into(),
            metadata_version: 1,
            created_at: 100,
            updated_at: 100,
        })
        .unwrap();
        handle.manage(DbState::new(db));
        {
            let state = handle.state::<DbState>();
            let db = state.lock().unwrap();
            db.ensure_sync_provider_state("v1", "p1").unwrap();
        }

        let dir = tempfile::tempdir().unwrap();
        (handle, dir)
    }

    #[test]
    fn detection_records_what_it_hashed() {
        let (handle, dir) = app_with_vault();
        std::fs::write(dir.path().join("note.md"), "hello").unwrap();

        let files = vec!["note.md".to_string()];
        let changes =
            detect_local_changes(&handle, dir.path(), "v1", "p1", &files).unwrap();
        assert_eq!(changes.len(), 1, "a new file is a change");

        let state = handle.state::<DbState>();
        let db = state.lock().unwrap();
        let cache = db.load_stat_cache("v1", "p1").unwrap();
        let entry = cache.get("note.md").expect("the file was not cached");

        assert_eq!(entry.file_size, 5);
        assert_eq!(entry.content_hash, changes[0].new_hash);
        assert_eq!(
            entry.content_hash,
            crate::sync::utils::sha256_hex(b"hello"),
            "cached digest does not describe the file on disk"
        );
    }

    #[test]
    fn an_edit_that_keeps_the_file_the_same_length_is_still_detected() {
        // The case a size-only check would miss, and the reason the cache also
        // compares modification time.
        let (handle, dir) = app_with_vault();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "aaaaa").unwrap();
        let files = vec!["note.md".to_string()];

        let first = detect_local_changes(&handle, dir.path(), "v1", "p1", &files).unwrap();
        {
            let state = handle.state::<DbState>();
            let db = state.lock().unwrap();
            db.upsert_document_baseline("v1", "p1", "note.md", &first[0].new_hash)
                .unwrap();
        }

        // Nothing changed: no work to do.
        let second = detect_local_changes(&handle, dir.path(), "v1", "p1", &files).unwrap();
        assert!(second.is_empty(), "an untouched file was reported as changed");

        std::fs::write(&path, "bbbbb").unwrap();
        let third = detect_local_changes(&handle, dir.path(), "v1", "p1", &files).unwrap();
        assert_eq!(
            third.len(),
            1,
            "a same-length edit was missed by change detection"
        );
        assert_eq!(third[0].new_hash, crate::sync::utils::sha256_hex(b"bbbbb"));
    }

    #[test]
    fn the_cache_forgets_files_that_are_gone() {
        let (handle, dir) = app_with_vault();
        std::fs::write(dir.path().join("a.md"), "a").unwrap();
        std::fs::write(dir.path().join("b.md"), "b").unwrap();

        let both = vec!["a.md".to_string(), "b.md".to_string()];
        detect_local_changes(&handle, dir.path(), "v1", "p1", &both).unwrap();

        let only_a = vec!["a.md".to_string()];
        detect_local_changes(&handle, dir.path(), "v1", "p1", &only_a).unwrap();

        let state = handle.state::<DbState>();
        let db = state.lock().unwrap();
        let cache = db.load_stat_cache("v1", "p1").unwrap();
        assert!(cache.contains_key("a.md"));
        assert!(
            !cache.contains_key("b.md"),
            "the cache kept an entry for a file that is no longer in the vault"
        );
    }
    #[test]
    fn a_file_whose_identity_was_rewritten_outside_the_app_does_not_abort_the_sync() {
        // An external editor, a checkout or a restore can strip the node_id from
        // a file's frontmatter. The next scan mints a new one, and the path then
        // has two claimants. That used to end the sync with a UNIQUE constraint
        // error; the newer claim simply wins.
        let (handle, _dir) = app_with_vault();
        let state = handle.state::<DbState>();
        let db = state.lock().unwrap();

        db.upsert_document_path("v1", "old-identity", "note.md").unwrap();
        db.upsert_document_path("v1", "new-identity", "note.md").unwrap();

        assert_eq!(
            db.get_node_id_by_path("v1", "note.md").unwrap().as_deref(),
            Some("new-identity")
        );
        assert_eq!(
            db.get_path_by_node_id("v1", "old-identity").unwrap(),
            None,
            "the displaced document should no longer claim the path"
        );
    }

}

#[cfg(test)]
mod stat_cache_tests {
    use super::*;
    use crate::db::StatCacheEntry;

    fn entry(size: u64, mtime_ms: i64) -> StatCacheEntry {
        StatCacheEntry {
            file_size: size,
            mtime_ms,
            content_hash: "a".repeat(64),
        }
    }

    const SETTLED: i64 = 10_000;

    #[test]
    fn settled_file_with_matching_metadata_may_skip_hashing() {
        assert!(stat_cache_hit((120, 1_000), &entry(120, 1_000), SETTLED));
    }

    #[test]
    fn a_different_size_always_hashes() {
        assert!(!stat_cache_hit((121, 1_000), &entry(120, 1_000), SETTLED));
    }

    #[test]
    fn a_different_mtime_always_hashes() {
        // The dangerous case: an edit that happens to leave the file the same
        // length. Only the timestamp reveals it.
        assert!(!stat_cache_hit((120, 1_500), &entry(120, 1_000), SETTLED));
    }

    #[test]
    fn a_recently_written_file_always_hashes() {
        // Within the settle window a second write could share the recorded
        // timestamp, so the metadata proves nothing.
        let mtime = 9_000;
        assert!(!stat_cache_hit((120, mtime), &entry(120, mtime), 10_000));
        assert!(!stat_cache_hit((120, mtime), &entry(120, mtime), 11_000));
        assert!(stat_cache_hit((120, mtime), &entry(120, mtime), 11_001));
    }

    #[test]
    fn a_clock_that_moved_backwards_hashes_rather_than_trusting_the_cache() {
        assert!(!stat_cache_hit((120, 50_000), &entry(120, 50_000), 10_000));
    }
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
            true,
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
            .commit_accepted_outbox_operation(rec, None, 1000)
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
            true,
        );
        // A change that cannot be prepared is reported and skipped rather than
        // failing the run, but it must still leave nothing behind.
        let skipped = res.expect("one bad change must not fail the whole preparation");
        assert_eq!(skipped.len(), 1, "the skipped file should be reported: {skipped:?}");

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
