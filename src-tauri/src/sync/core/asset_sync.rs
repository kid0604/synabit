//! Moving attachment bytes to and from a sync target.
//!
//! Kept apart from the document path deliberately. Documents travel inside
//! mailbox entries; attachments travel beside them, and the entry carries only a
//! reference. That ordering is what makes the reference meaningful: chunks are
//! uploaded before the reference is published, and fetched before the file is
//! written, so no peer ever sees a reference whose bytes are unavailable.

use std::path::Path;

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::sync::adapter::SyncAdapter;
use crate::sync::core::asset;
use crate::sync::core::types::{SyncPayload, SyncResult};
use synabit_protocol::{AssetChunkRef, AssetRef};

/// Upload the chunks for every attachment waiting to be published, then release
/// those entries for dispatch.
///
/// An entry stays in `Prepared` until its bytes are on the server. If an upload
/// fails the entry simply stays put and is retried on the next run, which is
/// preferable to publishing a reference no peer can resolve.
pub async fn upload_pending_assets(
    db_state: &DbState,
    vault: &Path,
    vault_id: &str,
    provider_id: &str,
    e2ee_key: &[u8; 32],
    adapter: &dyn SyncAdapter,
) -> AppResult<Vec<String>> {
    let pending = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_prepared_asset_operations(vault_id, provider_id)?
    };

    let mut failures = Vec::new();

    for (operation_id, rel_path) in pending {
        match upload_one(vault, &rel_path, e2ee_key, adapter).await {
            Ok(()) => {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.mark_asset_operation_ready(
                    vault_id,
                    provider_id,
                    &operation_id,
                    chrono::Utc::now().timestamp_millis(),
                )?;
            }
            Err(e) => {
                log::warn!("could not upload attachment {}: {}", rel_path, e);
                failures.push(format!("{}: {}", rel_path, e));
            }
        }
    }

    Ok(failures)
}

async fn upload_one(
    vault: &Path,
    rel_path: &str,
    e2ee_key: &[u8; 32],
    adapter: &dyn SyncAdapter,
) -> AppResult<()> {
    let contents = std::fs::read(vault.join(rel_path))
        .map_err(|e| AppError::General(format!("cannot read {}: {}", rel_path, e)))?;

    // Regenerated rather than stored: chunking is deterministic, so keeping the
    // encrypted bytes anywhere would only duplicate the file we already have.
    let (_asset, chunks) = asset::prepare(e2ee_key, rel_path, rel_path, &contents)?;

    for chunk in chunks {
        adapter
            .push_asset(chunk.reference.chunk_id, chunk.encrypted)
            .await?;
    }
    Ok(())
}

/// Fetch and write every attachment named by the staged entries of a page.
///
/// Runs before the page is applied, so by the time the durable apply pass sees
/// these entries they are already finished with. Anything that cannot be
/// completed is left for the next run rather than failing the page.
pub async fn fetch_staged_assets(
    db_state: &DbState,
    vault: &Path,
    vault_id: &str,
    provider_id: &str,
    page_cursor: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    adapter: &dyn SyncAdapter,
    result: &mut SyncResult,
) -> AppResult<()> {
    use crate::db::sync_inbox::InboxState;

    let entries = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_inbox_page_entries(vault_id, provider_id, page_cursor, 1000)?
    };

    for (_page_entry, record) in entries {
        if record.entry_kind != synabit_protocol::SyncEntryKind::AssetReference {
            continue;
        }
        if !matches!(record.state, InboxState::Pending | InboxState::PendingAsset) {
            continue;
        }

        // Our own attachment coming back: the file is already here.
        let is_own = crate::sync::coordinator::is_verified_own_operation(
            db_state,
            vault_id,
            provider_id,
            &record.operation_id,
            record.source_device.as_deref(),
            device_id,
        )?;

        let target = if is_own {
            InboxState::IgnoredOwnOperation
        } else {
            match write_one(db_state, vault, vault_id, provider_id, &record, e2ee_key, adapter)
                .await
            {
                Ok(rel_path) => {
                    result.pulled += 1;
                    result.pulled_files.push(rel_path);
                    InboxState::Applied
                }
                Err(e) => {
                    log::warn!("attachment not fetched yet: {}", e);
                    result.errors.push(e.to_string());
                    // Left for the next run rather than failing the page.
                    continue;
                }
            }
        };

        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let now = chrono::Utc::now().timestamp_millis();
        db.transition_inbox_state(
            vault_id,
            provider_id,
            &record.operation_id,
            record.state,
            InboxState::Applying,
            None,
            now,
        )?;
        db.transition_inbox_state(
            vault_id,
            provider_id,
            &record.operation_id,
            InboxState::Applying,
            target,
            None,
            now,
        )?;
    }

    Ok(())
}

async fn write_one(
    db_state: &DbState,
    vault: &Path,
    vault_id: &str,
    provider_id: &str,
    record: &crate::db::sync_inbox::InboxRecord,
    e2ee_key: &[u8; 32],
    adapter: &dyn SyncAdapter,
) -> AppResult<String> {
    let encrypted = record
        .encrypted_payload
        .as_ref()
        .ok_or_else(|| AppError::SyncError("attachment entry has no payload".into()))?;
    let payload_hash = record
        .payload_hash
        .ok_or_else(|| AppError::SyncError("attachment entry has no payload hash".into()))?;

    let parsed = crate::sync::coordinator::validate_and_parse_remote_entry(
        encrypted,
        &payload_hash,
        e2ee_key,
        &record.entry_kind,
    )
    .map_err(|k| AppError::SyncError(format!("unreadable attachment entry: {}", k.as_str())))?;

    let asset: AssetRef = match parsed {
        SyncPayload::AssetReference(a) => a,
        _ => return Err(AppError::SyncError("entry is not an attachment".into())),
    };

    // Defence in depth, matching the delete path: the resolved location must
    // stay inside the vault.
    let local_path = vault.join(&asset.rel_path);
    if !local_path.starts_with(vault) {
        return Err(AppError::SyncError(format!(
            "refusing to write outside the vault: {}",
            asset.rel_path
        )));
    }

    let mut fetched: Vec<(AssetChunkRef, Vec<u8>)> = Vec::with_capacity(asset.chunks.len());
    for reference in &asset.chunks {
        let bytes = adapter
            .pull_asset(reference.chunk_id)
            .await?
            .ok_or_else(|| {
                AppError::SyncError(format!("{}: a chunk is not on the server yet", asset.rel_path))
            })?;
        fetched.push((reference.clone(), bytes));
    }

    let contents = asset::reassemble(e2ee_key, &asset, &fetched)?;

    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = local_path.with_extension(format!(
        "{}.part",
        local_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("tmp")
    ));
    std::fs::write(&tmp, &contents)?;
    std::fs::rename(&tmp, &local_path)?;

    {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.upsert_document_path(vault_id, &asset.node_id, &asset.rel_path)?;
        db.upsert_document_baseline(
            vault_id,
            provider_id,
            &asset.rel_path,
            &crate::sync::utils::sha256_hex(&contents),
        )?;
    }

    log::info!("attachment written: {}", asset.rel_path);
    Ok(asset.rel_path)
}
