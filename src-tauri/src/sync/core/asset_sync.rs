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

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Why an incoming attachment could not be written, and whether asking again
/// could ever change the answer.
///
/// The distinction is the whole point of the type. Treating every failure as
/// retryable meant a malformed entry was re-fetched and re-reported on every
/// sync for the life of the vault; treating every failure as fatal would throw
/// away attachments whose chunks were merely a few seconds behind.
enum AssetWriteError {
    /// Settled. The entry is wrong in a way that time does not fix.
    Terminal(String),
    /// Unsettled. Worth a bounded number of further attempts.
    Transient(String),
}

/// Anything raised on the way to writing a file describes the entry or this
/// machine, not the network, so it defaults to settled. The one genuinely
/// temporary case — chunks that have not arrived — is raised as `Transient` at
/// the point it is detected, where it is known rather than guessed.
///
/// Local I/O is the exception: a full disk or a momentarily locked file says
/// nothing about the entry, and the same bytes may well write cleanly later.
impl From<std::io::Error> for AssetWriteError {
    fn from(err: std::io::Error) -> Self {
        Self::Transient(err.to_string())
    }
}

impl From<AppError> for AssetWriteError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Io(e) => Self::Transient(e.to_string()),
            other => Self::Terminal(other.to_string()),
        }
    }
}

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
    let path = vault.join(rel_path);

    // Checked again rather than trusted from preparation: the file may have
    // grown in between, and this is the point where it is read whole.
    let size = std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| AppError::General(format!("cannot stat {}: {}", rel_path, e)))?;
    asset::check_size(rel_path, size)?;

    let contents = std::fs::read(&path)
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

        // An older version of a file we have already published a newer one for.
        //
        // Attachments are identified by their path, so two devices that both
        // save "Pasted image.png" — the name editors generate, which makes this
        // the collision most likely to happen for real — publish two entries for
        // one document. Unlike text there is nothing to merge: a picture is
        // replaced whole.
        //
        // Each device skips its own entry and applies the other's. The one whose
        // entry landed first lands on the head; the one whose entry landed
        // second writes an older picture over the top and stops below it, and
        // nothing afterwards notices, because neither device sees a local change
        // to republish. The two devices then hold different pictures at the same
        // path for good.
        //
        // The text path guards this by comparing identities, which cannot work
        // here: both entries carry the same `node_id`, since the path *is* the
        // identity. For an attachment the sequence alone settles it — moving
        // backwards is never right when there is no merge to be had.
        let ours_is_newer = match record.remote_seq {
            Some(theirs) => {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.latest_acked_outbox_seq_for_doc_hash(vault_id, provider_id, &record.doc_hash)?
                    .is_some_and(|ours| ours > theirs)
            }
            None => false,
        };

        if ours_is_newer && !is_own {
            log::info!("sync: keeping our newer attachment over an older one from another device");
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let now = now_ms();
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
                InboxState::Applied,
                None,
                now,
            )?;
            continue;
        }

        // Claimed before the attempt, so that a failure has somewhere to be
        // recorded from. `record_apply_failure` counts attempts out of this
        // state, which is what stops a doomed entry retrying for ever.
        {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(
                vault_id,
                provider_id,
                &record.operation_id,
                record.state,
                InboxState::Applying,
                None,
                now_ms(),
            )?;
        }

        let target = if is_own {
            InboxState::IgnoredOwnOperation
        } else {
            match write_one(db_state, vault, vault_id, provider_id, &record, e2ee_key, adapter)
                .await
            {
                Ok((rel_path, conflict)) => {
                    result.pulled += 1;
                    result.pulled_files.push(rel_path);
                    if let Some(conflict) = conflict {
                        result.conflicts.push(conflict);
                    }
                    InboxState::Applied
                }
                // Nothing about this entry will be different next time: it is
                // too large, malformed, or not an attachment at all. Retrying
                // would report the same complaint on every sync for ever, so it
                // is put aside once and said once.
                Err(AssetWriteError::Terminal(reason)) => {
                    log::warn!("attachment set aside: {}", reason);
                    result.errors.push(reason);
                    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    db.transition_inbox_state(
                        vault_id,
                        provider_id,
                        &record.operation_id,
                        InboxState::Applying,
                        InboxState::Quarantined,
                        Some("corrupt"),
                        now_ms(),
                    )?;
                    continue;
                }
                // Could well work later — most often a chunk that has not
                // landed yet. Retried, but a bounded number of times, so a
                // chunk that is never coming stops being asked for.
                // Could well work later — most often a chunk that has not
                // landed yet. Retried, but a bounded number of times, so a
                // chunk that is never coming stops being asked for. Reporting
                // is left to the shared handler, which speaks up only once the
                // attempts are spent rather than on every one of them.
                Err(AssetWriteError::Transient(reason)) => {
                    log::debug!("attachment not ready yet: {}", reason);
                    crate::sync::coordinator::record_apply_failure(
                        db_state,
                        vault_id,
                        provider_id,
                        &record.operation_id,
                        &reason,
                        result,
                    )?;
                    continue;
                }
            }
        };

        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.transition_inbox_state(
            vault_id,
            provider_id,
            &record.operation_id,
            InboxState::Applying,
            target,
            None,
            now_ms(),
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
) -> Result<(String, Option<crate::sync::core::types::SyncConflict>), AssetWriteError> {
    let mut conflict = None;
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
        _ => return Err(AppError::SyncError("entry is not an attachment".into()).into()),
    };

    // Defence in depth, matching the delete path: the resolved location must
    // stay inside the vault.
    let local_path = vault.join(&asset.rel_path);
    if !local_path.starts_with(vault) {
        return Err(AppError::SyncError(format!(
            "refusing to write outside the vault: {}",
            asset.rel_path
        ))
        .into());
    }

    // Before the capacity below is taken from it. Every number in an `AssetRef`
    // was chosen by another device, and `with_capacity` asks no questions.
    asset::validate_incoming(&asset)?;

    let mut fetched: Vec<(AssetChunkRef, Vec<u8>)> = Vec::with_capacity(asset.chunks.len());
    for reference in &asset.chunks {
        let bytes = match adapter.pull_asset(reference.chunk_id).await? {
            Some(bytes) => bytes,
            // The sender uploads chunks before publishing the reference, so this
            // is unusual rather than routine — a partial upload, or a chunk lost
            // on the server. Worth retrying, but not for ever.
            None => {
                return Err(AssetWriteError::Transient(format!(
                    "{}: a chunk is not on the server yet",
                    asset.rel_path
                )))
            }
        };
        fetched.push((reference.clone(), bytes));
    }

    let contents = asset::reassemble(e2ee_key, &asset, &fetched)?;

    // Is there something of ours here that this would destroy?
    //
    // An attachment is identified by its path, so a file already sitting at this
    // one is, to the system, an earlier version of this same document — even when
    // it is really an unrelated file that two devices happened to name alike.
    // Overwriting is right in the first case and destroys work in the second, and
    // the entry carries nothing that tells them apart.
    //
    // Content does. Identical bytes mean there was never anything to lose, which
    // is the common case worth being quiet about: the same file imported on two
    // machines, or one already carried here by other means. Only genuinely
    // different bytes are moved aside, so the vault gains a spare copy exactly
    // when a copy would otherwise have been lost.
    if local_path.exists() {
        let ours = std::fs::read(&local_path)?;
        let theirs_hash = asset.plaintext_hash;

        if *blake3::hash(&ours).as_bytes() != theirs_hash {
            let discriminator = hex::encode(blake3::hash(&ours).as_bytes());
            let kept = asset::conflict_path(&asset.rel_path, &discriminator);
            let kept_path = vault.join(&kept);

            // A device that already made this copy must not make it again: the
            // name is a function of the content, so the same content lands on
            // the same name and finding it there means the work is done.
            if !kept_path.exists() {
                if let Some(parent) = kept_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(&local_path, &kept_path)?;
                log::info!(
                    "sync: {} was replaced by another device's version; ours kept as {}",
                    asset.rel_path,
                    kept
                );
                conflict = Some(crate::sync::core::types::SyncConflict {
                    rel_path: asset.rel_path.clone(),
                    kept_as: kept.clone(),
                });
            }
        }
    }

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
    Ok((asset.rel_path, conflict))
}
