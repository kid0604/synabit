use crate::db::sync_outbox::OutboxRecord;
use crate::db::sync_provider_state::ProviderSyncState;
use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::sync::adapter::{
    AdapterSyncMode, AdapterSyncPlan, PullLimits, PushAck, SyncAdapter,
};
use crate::sync::core::change::{
    detect_deletions, detect_local_changes, prepare_durable_outbox_operations, LocalChange,
};
use crate::sync::core::identity::VaultIdentity;
use crate::sync::core::types::{
    DocSyncPayload, SyncPayload, SyncResult, SyncRunContext,
};
use std::path::Path;
use std::sync::Arc;
use synabit_protocol::SyncEntryKind;
use tauri::Manager;

// Used only by the tests below and by the oracle files they include. Kept out of
// the main import list so the build does not warn about them in a normal build.
#[cfg(test)]
use crate::db::sync_outbox::OutboxState;
#[cfg(test)]
use crate::db::DbBridge;
#[cfg(test)]
use crate::sync::adapter::PushResult;
#[cfg(test)]
use crate::sync::core::types::SyncOperation;

pub trait InboxEntryApplier<R: tauri::Runtime = tauri::Wry>: Send + Sync {
    fn apply(
        &self,
        app_handle: &tauri::AppHandle<R>,
        vault_path_obj: &Path,
        vault_path: &str,
        payload: &DocSyncPayload,
        result: &mut SyncResult,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<()>;

    /// Apply a remote tombstone. Separate from `apply` because a delete carries
    /// only an identity, never a document body.
    fn apply_delete(
        &self,
        app_handle: &tauri::AppHandle<R>,
        vault_path_obj: &Path,
        payload: &synabit_protocol::DeletePayload,
        remote_seq: Option<u64>,
        result: &mut SyncResult,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<()>;
}

pub struct ProductionInboxEntryApplier<R: tauri::Runtime = tauri::Wry> {
    pub app_handle: tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> InboxEntryApplier<R> for ProductionInboxEntryApplier<R> {
    fn apply(
        &self,
        _app_handle: &tauri::AppHandle<R>,
        vault_path_obj: &Path,
        vault_path: &str,
        payload: &DocSyncPayload,
        result: &mut SyncResult,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<()> {
        crate::sync::core::apply::apply_doc_payload(
            &self.app_handle,
            vault_path_obj,
            vault_path,
            payload,
            result,
            vault_id,
            provider_id,
        )
    }

    fn apply_delete(
        &self,
        _app_handle: &tauri::AppHandle<R>,
        vault_path_obj: &Path,
        payload: &synabit_protocol::DeletePayload,
        remote_seq: Option<u64>,
        result: &mut SyncResult,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<()> {
        crate::sync::core::apply::apply_delete_payload(
            &self.app_handle,
            vault_path_obj,
            payload,
            remote_seq,
            result,
            vault_id,
            provider_id,
        )
        .map(|_outcome| ())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxApplyFailureKind {
    Corrupt,
    Retryable,
    PendingAsset,
    UnsupportedDelete,
}

impl InboxApplyFailureKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            InboxApplyFailureKind::Corrupt => "corrupt",
            InboxApplyFailureKind::Retryable => "retryable",
            InboxApplyFailureKind::PendingAsset => "pending_asset",
            InboxApplyFailureKind::UnsupportedDelete => "unsupported_delete",
        }
    }
}

pub fn remote_entry_to_inbox_entry(
    entry: &crate::sync::adapter::RemoteEntry,
) -> AppResult<crate::db::sync_inbox::InboxEntryToStage> {
    if entry.remote_position.trim().is_empty() {
        return Err(AppError::General("Empty remote_position".into()));
    }
    Ok(crate::db::sync_inbox::InboxEntryToStage {
        remote_position: entry.remote_position.clone(),
        remote_seq: entry.remote_seq,
        operation_id: entry.operation_id,
        doc_hash: entry.doc_hash,
        entry_kind: entry.entry_kind.clone(),
        encrypted_payload: Some(entry.encrypted_payload.clone()),
        payload_hash: Some(entry.payload_hash),
        source_device: Some(entry.source_device.clone()),
    })
}

/// Did *this* device produce the given operation?
///
/// The only sound evidence is our own outbox: an operation we pushed leaves a
/// row keyed by its `operation_id`, and acknowledged rows are never purged.
///
/// The label the transport attaches (`source_device`) is deliberately *not*
/// trusted. It is chosen by whoever pushed the entry, and any deployment where
/// two installs report the same id — the Google Drive path derived it from the
/// application bundle identifier, identical on every machine — would otherwise
/// classify every peer's work as its own and silently discard all of it.
pub fn is_verified_own_operation(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    operation_id: &[u8; 16],
    _source_device: Option<&str>,
    _device_id: &str,
) -> AppResult<bool> {
    let db = match db_state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let outbox_entry = db.get_outbox_by_id(vault_id, provider_id, operation_id)?;
    Ok(outbox_entry.is_some())
}

pub fn is_terminal_noop_page(
    entries: &[crate::db::sync_inbox::InboxEntryToStage],
    has_more: bool,
    next_cursor: &str,
    current_cursor: &str,
) -> bool {
    entries.is_empty() && !has_more && next_cursor == current_cursor
}

pub fn decode_exact_payload<T: serde::de::DeserializeOwned>(decrypted: &[u8]) -> Option<T> {
    postcard::take_from_bytes(decrypted)
        .ok()
        .and_then(|(val, remainder)| {
            if remainder.is_empty() {
                Some(val)
            } else {
                None
            }
        })
}

fn validate_delete_payload(payload: &synabit_protocol::DeletePayload) -> bool {
    let node_id = &payload.node_id;
    if node_id.trim().is_empty() || node_id.len() > 128 || node_id.contains('\0') {
        return false;
    }

    let rel_path = &payload.rel_path;
    if rel_path.trim().is_empty()
        || rel_path.len() > 16384
        || rel_path.contains('\0')
        || rel_path.contains('\\')
        || rel_path.starts_with('/')
    {
        return false;
    }

    let bytes = rel_path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return false;
    }

    for segment in rel_path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
    }

    true
}

pub fn validate_and_parse_remote_entry(
    encrypted_payload: &[u8],
    payload_hash: &[u8; 32],
    e2ee_key: &[u8; 32],
    entry_kind: &SyncEntryKind,
) -> Result<SyncPayload, InboxApplyFailureKind> {
    let computed_hash = *blake3::hash(encrypted_payload).as_bytes();
    if &computed_hash != payload_hash {
        return Err(InboxApplyFailureKind::Corrupt);
    }

    let decrypted = match crate::sync::core::crypto::decrypt(e2ee_key, encrypted_payload) {
        Ok(d) => d,
        Err(_) => return Err(InboxApplyFailureKind::Corrupt),
    };

    let sync_payload = match decode_exact_payload::<SyncPayload>(&decrypted) {
        Some(sp) => sp,
        None => {
            if *entry_kind == SyncEntryKind::Upsert {
                if let Some(doc_payload) = decode_exact_payload::<DocSyncPayload>(&decrypted) {
                    let doc_bytes = match postcard::to_stdvec(&doc_payload) {
                        Ok(b) => b,
                        Err(_) => return Err(InboxApplyFailureKind::Corrupt),
                    };
                    SyncPayload::Upsert(doc_bytes)
                } else {
                    return Err(InboxApplyFailureKind::Corrupt);
                }
            } else {
                return Err(InboxApplyFailureKind::Corrupt);
            }
        }
    };

    match (entry_kind, &sync_payload) {
        (SyncEntryKind::Upsert, SyncPayload::Upsert(_)) => Ok(sync_payload),
        // Attachments are readable now. The bytes are fetched separately, before
        // the page is applied, so by this point the entry is normally already
        // finished with; anything still here is waiting on chunks.
        (SyncEntryKind::AssetReference, SyncPayload::AssetReference(_)) => Ok(sync_payload),
        (SyncEntryKind::Delete, SyncPayload::Delete(del)) => {
            if validate_delete_payload(del) {
                Ok(sync_payload)
            } else {
                Err(InboxApplyFailureKind::Corrupt)
            }
        }
        _ => Err(InboxApplyFailureKind::Corrupt),
    }
}

/// How many times an entry may fail to apply before it is set aside.
///
/// Transient problems — a locked file, a full disk, a directory that is not
/// there yet — clear within a sync or two. Anything that survives this many
/// attempts will not be fixed by retrying it forever at the cost of every
/// entry queued behind it.
pub const MAX_INBOX_APPLY_ATTEMPTS: u32 = 3;

/// Record a failed apply and decide the entry's fate.
///
/// Returns `true` when the caller should stop processing this page so the next
/// sync can retry, and `false` when the entry has exhausted its budget, was
/// quarantined, and the loop should move on.
pub(crate) fn record_apply_failure(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    operation_id: &[u8; 16],
    reason: &str,
    result: &mut SyncResult,
) -> AppResult<bool> {
    use crate::db::sync_inbox::InboxState;

    let now = chrono::Utc::now().timestamp_millis();
    let attempts = {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.increment_inbox_retry(vault_id, provider_id, operation_id, now)?
    };

    let exhausted = attempts >= MAX_INBOX_APPLY_ATTEMPTS;
    let (target, label) = if exhausted {
        (InboxState::Quarantined, InboxApplyFailureKind::Corrupt)
    } else {
        (InboxState::Failed, InboxApplyFailureKind::Retryable)
    };

    {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.transition_inbox_state(
            vault_id,
            provider_id,
            operation_id,
            InboxState::Applying,
            target,
            Some(label.as_str()),
            now,
        )?;
    }

    if exhausted {
        result.errors.push(format!(
            "quarantined {} after {} attempts: {}",
            hex::encode(operation_id),
            attempts,
            reason
        ));
        log::warn!(
            "inbox entry {} quarantined after {} attempts: {}",
            hex::encode(operation_id),
            attempts,
            reason
        );
    }

    Ok(!exhausted)
}

pub fn process_staged_inbox_page<R: tauri::Runtime>(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    page_cursor: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    applier: &dyn InboxEntryApplier<R>,
    app_handle: &tauri::AppHandle<R>,
    vault_path_obj: &Path,
    vault_path: &str,
    result: &mut SyncResult,
) -> AppResult<()> {
    use crate::db::sync_inbox::InboxState;

    let entries = {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.get_inbox_page_entries(vault_id, provider_id, page_cursor, 1000)?
    };

    for (_page_entry, mut inbox_record) in entries {
        match inbox_record.state {
            InboxState::Applied | InboxState::IgnoredOwnOperation => continue,
            // Set aside, not blocking. An entry we cannot use must never stop
            // the entries behind it: one bad payload used to wedge a vault's
            // sync permanently.
            InboxState::PendingAsset => {
                result.errors.push(format!(
                    "skipped {}: waiting on an asset that is not available",
                    hex::encode(inbox_record.operation_id)
                ));
                continue;
            }
            InboxState::Quarantined => {
                result.errors.push(format!(
                    "skipped {}: quarantined ({})",
                    hex::encode(inbox_record.operation_id),
                    inbox_record.last_error.as_deref().unwrap_or("unusable")
                ));
                continue;
            }
            InboxState::Failed => {
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Failed,
                    InboxState::Applying,
                    None,
                    chrono::Utc::now().timestamp_millis(),
                )?;
                inbox_record.state = InboxState::Applying;
            }
            InboxState::Pending => {
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Pending,
                    InboxState::Applying,
                    None,
                    chrono::Utc::now().timestamp_millis(),
                )?;
                inbox_record.state = InboxState::Applying;
            }
            InboxState::Applying => {
                // Resume applying
            }
        }

        let is_own = is_verified_own_operation(
            db_state,
            vault_id,
            provider_id,
            &inbox_record.operation_id,
            inbox_record.source_device.as_deref(),
            device_id,
        )?;
        if is_own {
            let db = match db_state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            db.transition_inbox_state(
                vault_id,
                provider_id,
                &inbox_record.operation_id,
                InboxState::Applying,
                InboxState::IgnoredOwnOperation,
                None,
                chrono::Utc::now().timestamp_millis(),
            )?;
            continue;
        }

        let encrypted_payload = match inbox_record.encrypted_payload {
            Some(ep) => ep,
            None => {
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Applying,
                    InboxState::Quarantined,
                    Some(InboxApplyFailureKind::Corrupt.as_str()),
                    chrono::Utc::now().timestamp_millis(),
                )?;
                result
                    .errors
                    .push(format!("quarantined {}: Missing encrypted_payload", hex::encode(inbox_record.operation_id)));
                continue;
            }
        };

        let payload_hash = match inbox_record.payload_hash {
            Some(ph) => ph,
            None => {
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Applying,
                    InboxState::Quarantined,
                    Some(InboxApplyFailureKind::Corrupt.as_str()),
                    chrono::Utc::now().timestamp_millis(),
                )?;
                result
                    .errors
                    .push(format!("quarantined {}: Missing payload_hash", hex::encode(inbox_record.operation_id)));
                continue;
            }
        };

        let parsed_payload = match validate_and_parse_remote_entry(
            &encrypted_payload,
            &payload_hash,
            e2ee_key,
            &inbox_record.entry_kind,
        ) {
            Ok(p) => p,
            Err(failure_kind) => {
                match failure_kind {
                    // Neither of these gets better by being retried, and neither
                    // may hold up the entries behind it.
                    InboxApplyFailureKind::Corrupt | InboxApplyFailureKind::PendingAsset => {
                        let target = if failure_kind == InboxApplyFailureKind::Corrupt {
                            InboxState::Quarantined
                        } else {
                            InboxState::PendingAsset
                        };
                        let err_param = if failure_kind == InboxApplyFailureKind::Corrupt {
                            Some(failure_kind.as_str())
                        } else {
                            None
                        };
                        let db = match db_state.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner(),
                        };
                        db.transition_inbox_state(
                            vault_id,
                            provider_id,
                            &inbox_record.operation_id,
                            InboxState::Applying,
                            target,
                            err_param,
                            chrono::Utc::now().timestamp_millis(),
                        )?;
                        result.errors.push(format!(
                            "skipped {}: {}",
                            hex::encode(inbox_record.operation_id),
                            failure_kind.as_str()
                        ));
                        continue;
                    }
                    InboxApplyFailureKind::UnsupportedDelete | InboxApplyFailureKind::Retryable => {
                        if record_apply_failure(
                            db_state,
                            vault_id,
                            provider_id,
                            &inbox_record.operation_id,
                            failure_kind.as_str(),
                            result,
                        )? {
                            return Err(AppError::General(format!(
                                "Inbox apply failed: {}",
                                failure_kind.as_str()
                            )));
                        }
                        continue;
                    }
                }
            }
        };

        match parsed_payload {
            SyncPayload::Upsert(doc_bytes) => {
                let doc_payload: DocSyncPayload = match decode_exact_payload(&doc_bytes) {
                    Some(dp) => dp,
                    None => {
                        let db = match db_state.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner(),
                        };
                        db.transition_inbox_state(
                            vault_id,
                            provider_id,
                            &inbox_record.operation_id,
                            InboxState::Applying,
                            InboxState::Quarantined,
                            Some(InboxApplyFailureKind::Corrupt.as_str()),
                            chrono::Utc::now().timestamp_millis(),
                        )?;
                        result.errors.push(format!(
                            "quarantined {}: unreadable document payload",
                            hex::encode(inbox_record.operation_id)
                        ));
                        continue;
                    }
                };
                // A *different* document claiming a position we have already
                // published something newer for.
                //
                // Two devices that independently create a note at the same path
                // produce entries with the same `doc_hash` — it is the hash of
                // the path — but different `node_id`s. They are not two versions
                // of one document, so no CRDT merge applies; one of them is
                // simply going to lose the position.
                //
                // Which one must be the same everywhere. Without this check the
                // device whose entry landed later skipped its own operation,
                // wrote the other's earlier one over the top, and came to rest
                // below the head while every other device came to rest on it.
                // Nothing afterwards reconciled that: neither side saw a local
                // change, so neither republished, and the vault stayed split.
                //
                // Matching identities fall straight through — concurrent edits
                // to the same document are exactly what the CRDT is for, and
                // arriving out of order is normal.
                let our_document_here = {
                    let db = match db_state.lock() {
                        Ok(g) => g,
                        Err(p) => p.into_inner(),
                    };
                    db.get_node_id_by_path(vault_id, &doc_payload.rel_path)?
                        .filter(|ours| *ours != doc_payload.node_id)
                };

                if let Some(our_node_id) = our_document_here {
                    let ours_is_newer = match inbox_record.remote_seq {
                        Some(theirs) => {
                            let db = match db_state.lock() {
                                Ok(g) => g,
                                Err(p) => p.into_inner(),
                            };
                            db.latest_acked_outbox_seq_for_doc_hash(
                                vault_id,
                                provider_id,
                                &inbox_record.doc_hash,
                            )?
                            .is_some_and(|ours| ours > theirs)
                        }
                        None => false,
                    };

                    if ours_is_newer {
                        log::info!(
                            "sync: keeping our newer note at {} over an older one from another device",
                            doc_payload.rel_path
                        );
                        let db = match db_state.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner(),
                        };
                        db.transition_inbox_state(
                            vault_id,
                            provider_id,
                            &inbox_record.operation_id,
                            InboxState::Applying,
                            InboxState::Applied,
                            None,
                            chrono::Utc::now().timestamp_millis(),
                        )?;
                        continue;
                    }

                    // We are the one losing the position. Their document is about
                    // to be written where ours is, so ours is set aside first —
                    // otherwise the only copy of it disappears under the write.
                    //
                    // The name is derived from our own identity, so it is stable:
                    // running this twice finds the file already there rather than
                    // making a second copy. Re-pointing the mapping is what makes
                    // the rest ordinary — the next run sees our document at a new
                    // path and publishes it as a plain rename, which carries the
                    // copy to every other device with no new machinery at all.
                    let kept = crate::sync::core::asset::conflict_path(
                        &doc_payload.rel_path,
                        &our_node_id.replace('-', ""),
                    );
                    let ours_on_disk = vault_path_obj.join(&doc_payload.rel_path);
                    let kept_on_disk = vault_path_obj.join(&kept);

                    if ours_on_disk.exists() && !kept_on_disk.exists() {
                        if let Some(parent) = kept_on_disk.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::rename(&ours_on_disk, &kept_on_disk)?;
                        {
                            let db = match db_state.lock() {
                                Ok(g) => g,
                                Err(p) => p.into_inner(),
                            };
                            db.upsert_document_path(vault_id, &our_node_id, &kept)?;
                        }
                        log::info!(
                            "sync: {} was taken by another device's note; ours kept as {}",
                            doc_payload.rel_path,
                            kept
                        );
                        result.conflicts.push(crate::sync::core::types::SyncConflict {
                            rel_path: doc_payload.rel_path.clone(),
                            kept_as: kept.clone(),
                        });
                    }
                }

                if let Err(_e) = applier.apply(
                    app_handle,
                    vault_path_obj,
                    vault_path,
                    &doc_payload,
                    result,
                    vault_id,
                    provider_id,
                ) {
                    if record_apply_failure(
                        db_state,
                        vault_id,
                        provider_id,
                        &inbox_record.operation_id,
                        "writing the document failed",
                        result,
                    )? {
                        return Err(AppError::General("Apply doc payload failed".into()));
                    }
                    continue;
                }
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Applying,
                    InboxState::Applied,
                    None,
                    chrono::Utc::now().timestamp_millis(),
                )?;
            }
            SyncPayload::Delete(delete_payload) => {
                if let Err(_e) = applier.apply_delete(
                    app_handle,
                    vault_path_obj,
                    &delete_payload,
                    inbox_record.remote_seq,
                    result,
                    vault_id,
                    provider_id,
                ) {
                    if record_apply_failure(
                        db_state,
                        vault_id,
                        provider_id,
                        &inbox_record.operation_id,
                        "removing the document failed",
                        result,
                    )? {
                        return Err(AppError::General("Apply delete payload failed".into()));
                    }
                    continue;
                }
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Applying,
                    InboxState::Applied,
                    None,
                    chrono::Utc::now().timestamp_millis(),
                )?;
            }
            // Its chunks were not all available when the fetch pass ran. Left
            // where it is so the next run can try again — not quarantined,
            // because there is nothing wrong with it.
            SyncPayload::AssetReference(asset) => {
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Applying,
                    InboxState::PendingAsset,
                    None,
                    chrono::Utc::now().timestamp_millis(),
                )?;
                result
                    .errors
                    .push(format!("{}: waiting on attachment data", asset.rel_path));
                continue;
            }

        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    let db = match db_state.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    db.mark_inbox_page_applied_if_safe(vault_id, provider_id, page_cursor, now)?;
    db.commit_applied_inbox_page_cursor(vault_id, provider_id, page_cursor, now)?;

    Ok(())
}

pub async fn retry_remote_ack_gap(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    adapter: &dyn SyncAdapter,
) -> AppResult<()> {
    let (cursor, ack_cursor) = {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let state = db
            .get_sync_provider_state(vault_id, provider_id)?
            .ok_or_else(|| AppError::General("No provider state".into()))?;
        (state.cursor, state.ack_cursor)
    };

    if !cursor.is_empty() && Some(cursor.clone()) != ack_cursor {
        let now = chrono::Utc::now().timestamp_millis();
        match adapter.ack(&cursor).await {
            Ok(()) => {
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.mark_sync_provider_cursor_acked_cas(
                    vault_id,
                    provider_id,
                    ack_cursor.as_deref(),
                    &cursor,
                    now,
                )?;
            }
            Err(AppError::UnsupportedCapability(_)) => {
                let db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                db.mark_sync_provider_cursor_acked_cas(
                    vault_id,
                    provider_id,
                    ack_cursor.as_deref(),
                    &cursor,
                    now,
                )?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub async fn resume_durable_inbox_before_pull<R: tauri::Runtime>(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    applier: &dyn InboxEntryApplier<R>,
    app_handle: &tauri::AppHandle<R>,
    vault_path_obj: &Path,
    vault_path: &str,
    adapter: &dyn SyncAdapter,
    result: &mut SyncResult,
) -> AppResult<()> {
    loop {
        let provider_cursor = {
            let db = match db_state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let state = db
                .get_sync_provider_state(vault_id, provider_id)?
                .ok_or_else(|| AppError::General("No provider state".into()))?;
            state.cursor
        };

        let page = {
            let db = match db_state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            db.get_inbox_page(vault_id, provider_id, &provider_cursor)?
        };

        if let Some(page) = page {
            use crate::db::sync_inbox::InboxPageState;
            match page.state {
                InboxPageState::Staged => {
                    crate::sync::core::asset_sync::fetch_staged_assets(
                        db_state,
                        vault_path_obj,
                        vault_id,
                        provider_id,
                        &provider_cursor,
                        device_id,
                        e2ee_key,
                        adapter,
                        result,
                    )
                    .await?;
                    process_staged_inbox_page(
                        db_state,
                        vault_id,
                        provider_id,
                        &provider_cursor,
                        device_id,
                        e2ee_key,
                        applier,
                        app_handle,
                        vault_path_obj,
                        vault_path,
                        result,
                    )?;
                    retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;
                }
                InboxPageState::Applied => {
                    let now = chrono::Utc::now().timestamp_millis();
                    {
                        let db = match db_state.lock() {
                            Ok(g) => g,
                            Err(p) => p.into_inner(),
                        };
                        db.commit_applied_inbox_page_cursor(
                            vault_id,
                            provider_id,
                            &provider_cursor,
                            now,
                        )?;
                    }
                    retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;
                }
                InboxPageState::CursorCommitted => {
                    return Err(AppError::General(
                        "Inconsistent state: page cursor committed but provider cursor hasn't advanced".into(),
                    ));
                }
            }
        } else {
            break;
        }
    }
    Ok(())
}

pub(crate) async fn pull_pages_durable<R: tauri::Runtime>(
    db_state: &DbState,
    adapter: &dyn SyncAdapter,
    vault_id: &str,
    provider_id: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    applier: &dyn InboxEntryApplier<R>,
    app_handle: &tauri::AppHandle<R>,
    vault_path_obj: &Path,
    vault_path: &str,
    sync_plan: &AdapterSyncPlan,
    limits: PullLimits,
    result: &mut SyncResult,
) -> AppResult<u64> {
    let until_cursor = match &sync_plan.mode {
        AdapterSyncMode::Delta { until_cursor } => until_cursor.as_deref(),
        AdapterSyncMode::BootstrapRequired => {
            return Err(AppError::SyncError(
                "Bootstrap required by sync target".into(),
            ));
        }
    };

    retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;

    resume_durable_inbox_before_pull(
        db_state,
        vault_id,
        provider_id,
        device_id,
        e2ee_key,
        applier,
        app_handle,
        vault_path_obj,
        vault_path,
        adapter,
        result,
    )
    .await?;

    let mut current_cursor = {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let state = db
            .get_sync_provider_state(vault_id, provider_id)?
            .ok_or_else(|| AppError::General("No provider state".into()))?;
        state.cursor
    };

    let mut total_rx_bytes = 0u64;

    loop {
        #[rustfmt::skip]
        let page = adapter.pull_page(&current_cursor, until_cursor, limits).await?;

        let added_bytes = match total_rx_bytes.checked_add(page.rx_bytes) {
            Some(b) => b,
            None => return Err(AppError::General("rx_bytes overflow".into())),
        };
        total_rx_bytes = added_bytes;

        let mut entries_to_stage = Vec::with_capacity(page.entries.len());
        for entry in &page.entries {
            entries_to_stage.push(remote_entry_to_inbox_entry(entry)?);
        }

        if is_terminal_noop_page(
            &entries_to_stage,
            page.has_more,
            &page.next_cursor,
            &current_cursor,
        ) {
            break;
        }

        let now = chrono::Utc::now().timestamp_millis();
        {
            let db = match db_state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            db.stage_inbox_page(
                vault_id,
                provider_id,
                &current_cursor,
                &page.next_cursor,
                page.has_more,
                &entries_to_stage,
                now,
            )?;
        }

        crate::sync::core::asset_sync::fetch_staged_assets(
            db_state,
            vault_path_obj,
            vault_id,
            provider_id,
            &current_cursor,
            device_id,
            e2ee_key,
            adapter,
            result,
        )
        .await?;

        process_staged_inbox_page(
            db_state,
            vault_id,
            provider_id,
            &current_cursor,
            device_id,
            e2ee_key,
            applier,
            app_handle,
            vault_path_obj,
            vault_path,
            result,
        )?;

        retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;

        if !page.has_more {
            break;
        }
        current_cursor = page.next_cursor;
    }

    Ok(total_rx_bytes)
}

pub async fn preflight_provider_state(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    adapter: &dyn SyncAdapter,
) -> AppResult<(AdapterSyncPlan, String)> {
    {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.ensure_sync_provider_state(vault_id, provider_id)?;
    }

    // Read after the row exists, not before. `ensure_sync_provider_state` stamps
    // `created_at` from its own reading of the clock, so a reading taken up here
    // can be older than the row it is about to update — and the table quite
    // correctly refuses an update that predates its own creation. Crossing a
    // single millisecond boundary between the two was enough to fail the run.
    let now = chrono::Utc::now().timestamp_millis();

    let stored_state = {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.get_sync_provider_state(vault_id, provider_id)?
            .ok_or_else(|| AppError::General("No provider state row".into()))?
    };

    let plan = adapter
        .get_sync_plan(&stored_state.cursor, stored_state.incarnation_id)
        .await?;

    let req_bootstrap = matches!(plan.mode, AdapterSyncMode::BootstrapRequired);

    {
        let mut db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.reconcile_sync_provider_plan(
            vault_id,
            provider_id,
            plan.incarnation_id,
            plan.remote_vault_id,
            req_bootstrap,
            now,
        )?;
    }

    let final_state = {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.get_sync_provider_state(vault_id, provider_id)?
            .ok_or_else(|| AppError::General("No provider state row after reconcile".into()))?
    };

    if final_state.sync_state != ProviderSyncState::Ready {
        return Err(AppError::SyncError(format!(
            "Provider state is not Ready: {:?}",
            final_state.sync_state
        )));
    }

    Ok((plan, final_state.cursor))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DispatchOutcome {
    pub acknowledged: u32,
    pub tx_bytes: u64,
}

pub fn validate_push_ack_batch(
    dispatchable: &[OutboxRecord],
    accepted: &[PushAck],
    rejected: &[PushAck],
) -> AppResult<()> {
    let dispatch_ops: std::collections::HashSet<[u8; 16]> =
        dispatchable.iter().map(|r| r.operation_id).collect();

    let mut seen_accepted = std::collections::HashSet::new();
    for ack in accepted {
        if !dispatch_ops.contains(&ack.operation_id) {
            return Err(AppError::General(
                "protocol error: accepted operation_id not in batch".into(),
            ));
        }
        if !seen_accepted.insert(ack.operation_id) {
            return Err(AppError::General(
                "protocol error: duplicate operation_id in accepted".into(),
            ));
        }
    }

    let mut seen_rejected = std::collections::HashSet::new();
    for ack in rejected {
        if !dispatch_ops.contains(&ack.operation_id) {
            return Err(AppError::General(
                "protocol error: rejected operation_id not in batch".into(),
            ));
        }
        if !seen_rejected.insert(ack.operation_id) {
            return Err(AppError::General(
                "protocol error: duplicate operation_id in rejected".into(),
            ));
        }
        if seen_accepted.contains(&ack.operation_id) {
            return Err(AppError::General(
                "protocol error: operation_id present in both accepted and rejected".into(),
            ));
        }
    }

    if seen_accepted.len() + seen_rejected.len() != dispatchable.len() {
        return Err(AppError::General(
            "protocol error: missing operation_id outcome in push response".into(),
        ));
    }

    Ok(())
}

pub async fn dispatch_durable_outbox_at(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    adapter: &dyn SyncAdapter,
    limit: usize,
    now: i64,
) -> AppResult<DispatchOutcome> {
    {
        let mut db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.quarantine_incomplete_dispatchable_outbox(vault_id, provider_id, now)?;
    }

    let dispatchable = {
        let db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.get_dispatchable_outbox(vault_id, provider_id, now, limit)?
    };

    if dispatchable.is_empty() {
        return Ok(DispatchOutcome {
            acknowledged: 0,
            tx_bytes: 0,
        });
    }

    let op_ids: Vec<[u8; 16]> = dispatchable.iter().map(|r| r.operation_id).collect();
    let mut sync_ops = Vec::with_capacity(dispatchable.len());
    for record in &dispatchable {
        sync_ops.push(crate::db::sync_outbox::outbox_record_to_sync_operation(
            record,
        )?);
    }

    {
        let mut db = match db_state.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        db.mark_outbox_batch_sent(vault_id, provider_id, &op_ids, now)?;
    }

    match adapter.push(sync_ops).await {
        Ok(push_result) => {
            if let Err(val_err) =
                validate_push_ack_batch(&dispatchable, &push_result.accepted, &push_result.rejected)
            {
                let err_msg = match &val_err {
                    AppError::General(m) => m.clone(),
                    _ => val_err.to_string(),
                };
                let mut db = match db_state.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                if let Err(db_err) =
                    db.schedule_outbox_batch_retry(vault_id, provider_id, &op_ids, &err_msg, now)
                {
                    return Err(AppError::General(format!("{err_msg}, DB error: {db_err}")));
                }
                return Err(val_err);
            }

            let mut db = match db_state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            for record in &dispatchable {
                if let Some(ack) = push_result
                    .accepted
                    .iter()
                    .find(|ack| ack.operation_id == record.operation_id)
                {
                    db.commit_accepted_outbox_operation(record, ack.remote_seq, now)?;
                } else if let Some(rej_ack) = push_result
                    .rejected
                    .iter()
                    .find(|ack| ack.operation_id == record.operation_id)
                {
                    db.schedule_outbox_retry(
                        vault_id,
                        provider_id,
                        &record.operation_id,
                        &format!("rejected: {}", rej_ack.remote_position),
                        now,
                    )?;
                }
            }
            Ok(DispatchOutcome {
                acknowledged: push_result.accepted.len() as u32,
                tx_bytes: push_result.tx_bytes,
            })
        }
        Err(err) => {
            let err_msg = match &err {
                AppError::General(m) => m.clone(),
                _ => err.to_string(),
            };
            let mut db = match db_state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if let Err(db_err) =
                db.schedule_outbox_batch_retry(vault_id, provider_id, &op_ids, &err_msg, now)
            {
                return Err(AppError::General(format!("{err_msg}, DB error: {db_err}")));
            }
            Err(err)
        }
    }
}

pub async fn dispatch_durable_outbox(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    adapter: &dyn SyncAdapter,
    limit: usize,
) -> AppResult<DispatchOutcome> {
    dispatch_durable_outbox_at(
        db_state,
        vault_id,
        provider_id,
        adapter,
        limit,
        chrono::Utc::now().timestamp_millis(),
    )
    .await
}

pub struct SyncCoordinator {
    active_adapter: Option<Arc<dyn SyncAdapter>>,
}

impl Default for SyncCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncCoordinator {
    pub fn new() -> Self {
        Self {
            active_adapter: None,
        }
    }

    pub async fn set_adapter(&mut self, adapter: Arc<dyn SyncAdapter>) -> AppResult<()> {
        if let Some(old) = &self.active_adapter {
            let _ = old.disconnect().await;
        }
        self.active_adapter = Some(adapter);
        Ok(())
    }

    pub async fn clear_adapter(&mut self) -> AppResult<()> {
        if let Some(old) = &self.active_adapter {
            let _ = old.disconnect().await;
        }
        self.active_adapter = None;
        Ok(())
    }

    pub async fn sync<R: tauri::Runtime>(
        &self,
        vault_identity: &VaultIdentity,
        device_id: &str,
        e2ee_key: &[u8; 32],
        _ctx: &SyncRunContext,
        app_handle: &tauri::AppHandle<R>,
    ) -> AppResult<SyncResult> {
        let db_state = app_handle.state::<DbState>();
        let adapter = self
            .active_adapter
            .as_ref()
            .ok_or_else(|| AppError::SyncError("No sync adapter configured".into()))?;

        log::info!(
            "Starting SyncCoordinator run for adapter: {}",
            adapter.name()
        );

        if !adapter.is_connected().await {
            adapter.connect().await?;
        }

        let vault_id = vault_identity.vault_id.to_string();
        let provider_id = adapter.adapter_id();
        let vault_path_obj = &vault_identity.canonical_path;
        let vault_path_str = vault_path_obj.to_string_lossy().to_string();
        let vault_path = &vault_path_str;

        // 1. Preflight
        let (plan, _reconciled_cursor) =
            preflight_provider_state(&db_state, &vault_id, &provider_id, adapter.as_ref()).await?;

        // 2. Drain pre-existing outbox
        let pushed_pre =
            dispatch_durable_outbox(&db_state, &vault_id, &provider_id, adapter.as_ref(), 100)
                .await?;

        // 3. Detect and prepare local changes
        {
            let mut db = match db_state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if let Err(e) = db.compact_all_crdt(&vault_id) {
                log::warn!("Failed to compact CRDT documents before sync: {}", e);
            }
        }

        // Walk the vault once and share the listing: both detectors need it, and
        // the deletion guard needs to distinguish "file removed" from "vault not
        // present at all".
        let all_files = crate::sync::utils::collect_local_files(vault_path);

        // Upserts are prepared before deletions are even looked for, and the
        // order matters. Preparing an upsert is what resolves a file's node_id
        // and re-points that document's path mapping. A renamed file therefore
        // updates its own mapping here, so the deletion pass below sees the
        // document at its new path and never mistakes the rename for a delete.
        let edits: Vec<LocalChange> = detect_local_changes(
            app_handle,
            vault_path_obj,
            &vault_id,
            &provider_id,
            &all_files,
        )?;
        let edit_count = edits.len();
        let mut skipped = prepare_durable_outbox_operations(
            &db_state,
            vault_path_obj,
            edits,
            e2ee_key,
            &vault_id,
            &provider_id,
            adapter.supports_assets(),
        )?;

        // The presence check asks "is the vault there at all", so it looks at
        // everything on disk, not just the documents this engine can publish.
        let deletions: Vec<LocalChange> = detect_deletions(
            app_handle,
            vault_path_obj,
            &vault_id,
            &all_files,
        )?;
        log::info!(
            "Detected {} local change(s) and {} deletion(s)",
            edit_count,
            deletions.len()
        );
        skipped.extend(prepare_durable_outbox_operations(
            &db_state,
            vault_path_obj,
            deletions,
            e2ee_key,
            &vault_id,
            &provider_id,
            adapter.supports_assets(),
        )?);

        // Attachment bytes go up before their references do, so a peer never
        // sees a reference it cannot resolve.
        let asset_failures = crate::sync::core::asset_sync::upload_pending_assets(
            &db_state,
            vault_path_obj,
            &vault_id,
            &provider_id,
            e2ee_key,
            adapter.as_ref(),
        )
        .await?;
        skipped.extend(asset_failures);

        // 4. Drain newly prepared outbox
        let pushed_post =
            dispatch_durable_outbox(&db_state, &vault_id, &provider_id, adapter.as_ref(), 100)
                .await?;

        let total_pushed = pushed_pre.acknowledged + pushed_post.acknowledged;
        let total_tx_bytes = pushed_pre.tx_bytes + pushed_post.tx_bytes;

        let mut result = SyncResult {
            pulled: 0,
            pushed: total_pushed,
            deleted: 0,
            errors: skipped,
            conflicts: Vec::new(),
            pulled_files: Vec::new(),
            tx_bytes: total_tx_bytes,
            rx_bytes: 0,
        };

        let limits = PullLimits {
            max_entries: 1000,
            max_bytes: 10 * 1024 * 1024,
        };

        let applier = ProductionInboxEntryApplier {
            app_handle: app_handle.clone(),
        };

        // 5. Durable Pull
        let rx_bytes = pull_pages_durable(
            &db_state,
            adapter.as_ref(),
            &vault_id,
            &provider_id,
            device_id,
            e2ee_key,
            &applier,
            app_handle,
            vault_path_obj,
            vault_path,
            &plan,
            limits,
            &mut result,
        )
        .await?;

        result.rx_bytes = rx_bytes;

        Ok(result)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProviderStateRow {
    pub vault_id: String,
    pub provider_id: String,
    pub cursor: String,
    pub ack_cursor: Option<String>,
    pub sync_state: String,
    pub incarnation_id: Option<[u8; 16]>,
    pub remote_vault_id: Option<[u8; 32]>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InboxRow {
    pub vault_id: String,
    pub provider_id: String,
    pub page_cursor: String,
    pub remote_position: String,
    pub remote_seq: Option<u64>,
    pub operation_id: [u8; 16],
    pub doc_hash: [u8; 32],
    pub entry_kind: String,
    pub encrypted_payload: Option<Vec<u8>>,
    pub payload_hash: Option<[u8; 32]>,
    pub source_device: Option<String>,
    pub state: String,
    pub last_error: Option<String>,
    pub received_at: i64,
    pub updated_at: i64,
    pub applied_at: Option<i64>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InboxPagesRow {
    pub vault_id: String,
    pub provider_id: String,
    pub start_cursor: String,
    pub next_cursor: String,
    pub has_more: bool,
    pub entry_count: u32,
    pub state: String,
    pub received_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InboxPageEntriesRow {
    pub vault_id: String,
    pub provider_id: String,
    pub start_cursor: String,
    pub page_ordinal: u32,
    pub operation_id: [u8; 16],
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OutboxRow {
    pub vault_id: String,
    pub provider_id: String,
    pub operation_id: [u8; 16],
    pub entry_kind: String,
    pub node_id: String,
    pub rel_path: Option<String>,
    pub doc_hash: Option<[u8; 32]>,
    pub source_hash: Option<[u8; 32]>,
    pub original_timestamp: i64,
    pub encrypted_payload: Option<Vec<u8>>,
    pub payload_hash: Option<[u8; 32]>,
    pub asset_ref_blob: Option<Vec<u8>>,
    pub state: String,
    pub retry_count: u32,
    pub next_retry_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct C2bRuntimeSnapshot {
    pub provider_state: Vec<ProviderStateRow>,
    pub inbox: Vec<InboxRow>,
    pub inbox_pages: Vec<InboxPagesRow>,
    pub inbox_page_entries: Vec<InboxPageEntriesRow>,
    pub outbox: Vec<OutboxRow>,
}

#[cfg(test)]
fn parse_err(msg: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Blob,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            msg.to_string(),
        )),
    )
}

#[cfg(test)]
pub fn snapshot_c2b_runtime_raw(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
) -> AppResult<C2bRuntimeSnapshot> {
    let db = match db_state.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let conn = db.conn();

    let mut stmt1 = conn.prepare(
        "SELECT vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at FROM sync_provider_state WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY vault_id, provider_id"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let provider_state =
        stmt1
            .query_map(rusqlite::params![vault_id, provider_id], |row| {
                let inc_blob: Option<Vec<u8>> = row.get(5)?;
                let incarnation_id: Option<[u8; 16]> = match inc_blob {
                    Some(b) => Some(b.try_into().map_err(|_| {
                        parse_err("malformed incarnation_id in sync_provider_state")
                    })?),
                    None => None,
                };
                let rv_blob: Option<Vec<u8>> = row.get(6)?;
                let remote_vault_id: Option<[u8; 32]> = match rv_blob {
                    Some(b) => Some(b.try_into().map_err(|_| {
                        parse_err("malformed remote_vault_id in sync_provider_state")
                    })?),
                    None => None,
                };
                Ok(ProviderStateRow {
                    vault_id: row.get(0)?,
                    provider_id: row.get(1)?,
                    cursor: row.get(2)?,
                    ack_cursor: row.get(3)?,
                    sync_state: row.get(4)?,
                    incarnation_id,
                    remote_vault_id,
                    last_error: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })
            .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt2 = conn.prepare(
        "SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq, operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash, source_device, state, last_error, received_at, updated_at, applied_at FROM sync_inbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC, remote_position ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let inbox = stmt2
        .query_map(rusqlite::params![vault_id, provider_id], |row| {
            let rseq_raw: Option<i64> = row.get(4)?;
            let remote_seq = match rseq_raw {
                Some(s) => Some(
                    u64::try_from(s)
                        .map_err(|_| parse_err("malformed remote_seq in sync_inbox"))?,
                ),
                None => None,
            };
            let op_blob: Vec<u8> = row.get(5)?;
            let operation_id: [u8; 16] = op_blob
                .try_into()
                .map_err(|_| parse_err("malformed operation_id in sync_inbox"))?;
            let doc_blob: Vec<u8> = row.get(6)?;
            let doc_hash: [u8; 32] = doc_blob
                .try_into()
                .map_err(|_| parse_err("malformed doc_hash in sync_inbox"))?;
            let ph_blob: Option<Vec<u8>> = row.get(9)?;
            let payload_hash: Option<[u8; 32]> = match ph_blob {
                Some(b) => Some(
                    b.try_into()
                        .map_err(|_| parse_err("malformed payload_hash in sync_inbox"))?,
                ),
                None => None,
            };
            Ok(InboxRow {
                vault_id: row.get(0)?,
                provider_id: row.get(1)?,
                page_cursor: row.get(2)?,
                remote_position: row.get(3)?,
                remote_seq,
                operation_id,
                doc_hash,
                entry_kind: row.get(7)?,
                encrypted_payload: row.get(8)?,
                payload_hash,
                source_device: row.get(10)?,
                state: row.get(11)?,
                last_error: row.get(12)?,
                received_at: row.get(13)?,
                updated_at: row.get(14)?,
                applied_at: row.get(15)?,
            })
        })
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt3 = conn.prepare(
        "SELECT vault_id, provider_id, start_cursor, next_cursor, has_more, entry_count, state, received_at, updated_at FROM sync_inbox_pages WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC, start_cursor ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let inbox_pages = stmt3
        .query_map(rusqlite::params![vault_id, provider_id], |row| {
            let has_more_raw: i64 = row.get(4)?;
            let has_more = DbBridge::decode_inbox_page_bool(has_more_raw)
                .map_err(|e| parse_err(&e.to_string()))?;
            let entry_count_raw: i64 = row.get(5)?;
            let entry_count = u32::try_from(entry_count_raw)
                .map_err(|_| parse_err("malformed entry_count in sync_inbox_pages"))?;
            Ok(InboxPagesRow {
                vault_id: row.get(0)?,
                provider_id: row.get(1)?,
                start_cursor: row.get(2)?,
                next_cursor: row.get(3)?,
                has_more,
                entry_count,
                state: row.get(6)?,
                received_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt4 = conn.prepare(
        "SELECT vault_id, provider_id, start_cursor, page_ordinal, operation_id FROM sync_inbox_page_entries WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY start_cursor ASC, page_ordinal ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let inbox_page_entries = stmt4
        .query_map(rusqlite::params![vault_id, provider_id], |row| {
            let ord_raw: i64 = row.get(3)?;
            let page_ordinal = u32::try_from(ord_raw)
                .map_err(|_| parse_err("malformed page_ordinal in sync_inbox_page_entries"))?;
            let op_blob: Vec<u8> = row.get(4)?;
            let operation_id: [u8; 16] = op_blob
                .try_into()
                .map_err(|_| parse_err("malformed operation_id in sync_inbox_page_entries"))?;
            Ok(InboxPageEntriesRow {
                vault_id: row.get(0)?,
                provider_id: row.get(1)?,
                start_cursor: row.get(2)?,
                page_ordinal,
                operation_id,
            })
        })
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt5 = conn.prepare(
        "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash, source_hash, original_timestamp, encrypted_payload, payload_hash, asset_ref_blob, state, retry_count, next_retry_at, last_error, created_at, updated_at FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY updated_at ASC, operation_id ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let outbox = stmt5
        .query_map(rusqlite::params![vault_id, provider_id], |row| {
            let op_blob: Vec<u8> = row.get(2)?;
            let operation_id: [u8; 16] = op_blob
                .try_into()
                .map_err(|_| parse_err("malformed operation_id in sync_outbox"))?;
            let doc_blob: Option<Vec<u8>> = row.get(6)?;
            let doc_hash: Option<[u8; 32]> = match doc_blob {
                Some(b) => Some(
                    b.try_into()
                        .map_err(|_| parse_err("malformed doc_hash in sync_outbox"))?,
                ),
                None => None,
            };
            let src_blob: Option<Vec<u8>> = row.get(7)?;
            let source_hash: Option<[u8; 32]> = match src_blob {
                Some(b) => Some(
                    b.try_into()
                        .map_err(|_| parse_err("malformed source_hash in sync_outbox"))?,
                ),
                None => None,
            };
            let ph_blob: Option<Vec<u8>> = row.get(10)?;
            let payload_hash: Option<[u8; 32]> = match ph_blob {
                Some(b) => Some(
                    b.try_into()
                        .map_err(|_| parse_err("malformed payload_hash in sync_outbox"))?,
                ),
                None => None,
            };
            let retry_raw: i64 = row.get(13)?;
            let retry_count = u32::try_from(retry_raw)
                .map_err(|_| parse_err("malformed retry_count in sync_outbox"))?;
            Ok(OutboxRow {
                vault_id: row.get(0)?,
                provider_id: row.get(1)?,
                operation_id,
                entry_kind: row.get(3)?,
                node_id: row.get(4)?,
                rel_path: row.get(5)?,
                doc_hash,
                source_hash,
                original_timestamp: row.get(8)?,
                encrypted_payload: row.get(9)?,
                payload_hash,
                asset_ref_blob: row.get(11)?,
                state: row.get(12)?,
                retry_count,
                next_retry_at: row.get(14)?,
                last_error: row.get(15)?,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
            })
        })
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    Ok(C2bRuntimeSnapshot {
        provider_state,
        inbox,
        inbox_pages,
        inbox_page_entries,
        outbox,
    })
}

#[cfg(test)]
#[path = "../../../.agents/oracles/d1_c2b_typed_compat.rs"]
mod d1_c2b_typed_compat;

#[cfg(test)]
#[rustfmt::skip]
#[path = "../../../.agents/oracles/d1_tombstone_identity.rs"]
mod d1_tombstone_identity;
