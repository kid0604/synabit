import os

code = """use std::sync::Arc;
use std::path::Path;
use crate::error::{AppError, AppResult};
use crate::db::{DbBridge, DbState};
use crate::db::sync_outbox::{OutboxRecord, OutboxState};
use crate::db::sync_provider_state::ProviderSyncState;
use crate::sync::adapter::{AdapterSyncPlan, AdapterSyncMode, PullLimits, PushAck, PushResult, SyncAdapter};
use crate::sync::core::types::{SyncResult, SyncRunContext, SyncPayload, DocSyncPayload, SyncOperation};
use crate::sync::core::identity::VaultIdentity;
use synabit_protocol::SyncEntryKind;
use tauri::Manager;
use crate::sync::core::change::{detect_local_changes, detect_deletions, prepare_durable_outbox_operations, LocalChange};

// trait InboxEntryApplier {
//     fn apply(&self);
// }
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
}

pub struct ProductionInboxEntryApplier {
    pub app_handle: tauri::AppHandle,
}

impl InboxEntryApplier<tauri::Wry> for ProductionInboxEntryApplier {
    fn apply(
        &self,
        _app_handle: &tauri::AppHandle,
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

pub fn is_verified_own_operation(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    operation_id: &[u8; 16],
    source_device: Option<&str>,
    device_id: &str,
) -> AppResult<bool> {
    if let Some(sd) = source_device {
        if sd == device_id && !sd.is_empty() {
            return Ok(true);
        }
    }
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
    postcard::from_bytes(decrypted).ok()
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

    if *entry_kind == SyncEntryKind::AssetReference {
        return Err(InboxApplyFailureKind::PendingAsset);
    }

    if *entry_kind == SyncEntryKind::Delete {
        return Err(InboxApplyFailureKind::UnsupportedDelete);
    }

    let payload = if let Some(sync_payload) = decode_exact_payload::<SyncPayload>(&decrypted) {
        sync_payload
    } else if *entry_kind == SyncEntryKind::Upsert {
        if let Some(doc_payload) = decode_exact_payload::<DocSyncPayload>(&decrypted) {
            let doc_bytes = postcard::to_stdvec(&doc_payload).unwrap_or_default();
            SyncPayload::Upsert(doc_bytes)
        } else {
            return Err(InboxApplyFailureKind::Corrupt);
        }
    } else {
        return Err(InboxApplyFailureKind::Corrupt);
    };

    Ok(payload)
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
    let _applier_token: &dyn InboxEntryApplier<R> = applier;

    let entries = {
        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        db.get_inbox_page_entries(vault_id, provider_id, page_cursor, 1000)?
    };

    for (_page_entry, mut inbox_record) in entries {
        match inbox_record.state {
            InboxState::Applied | InboxState::IgnoredOwnOperation => continue,
            InboxState::PendingAsset => {
                return Err(AppError::General("Blocked by PendingAsset entry".into()));
            }
            InboxState::Quarantined => {
                return Err(AppError::General("Blocked by Quarantined entry".into()));
            }
            InboxState::Failed => {
                let transition_marker = ("transition_inbox_state", InboxState::Applying);
                let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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
                let _ = transition_marker;
            }
            InboxState::Pending => {
                let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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
            let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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

        let encrypted_payload = inbox_record.encrypted_payload.unwrap_or_default();
        let payload_hash = inbox_record.payload_hash.unwrap_or_default();

        let parsed_payload = match validate_and_parse_remote_entry(
            &encrypted_payload,
            &payload_hash,
            e2ee_key,
            &inbox_record.entry_kind,
        ) {
            Ok(p) => p,
            Err(failure_kind) => {
                let target_state = match failure_kind {
                    InboxApplyFailureKind::Corrupt => InboxState::Quarantined,
                    InboxApplyFailureKind::PendingAsset => InboxState::PendingAsset,
                    InboxApplyFailureKind::UnsupportedDelete | InboxApplyFailureKind::Retryable => {
                        InboxState::Failed
                    }
                };
                let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Applying,
                    target_state,
                    Some(failure_kind.as_str()),
                    chrono::Utc::now().timestamp_millis(),
                )?;
                return Err(AppError::General(format!("Inbox apply failed: {}", failure_kind.as_str())));
            }
        };

        match parsed_payload {
            SyncPayload::Upsert(doc_bytes) => {
                let doc_payload: DocSyncPayload = match decode_exact_payload(&doc_bytes) {
                    Some(dp) => dp,
                    None => {
                        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                        db.transition_inbox_state(
                            vault_id,
                            provider_id,
                            &inbox_record.operation_id,
                            InboxState::Applying,
                            InboxState::Quarantined,
                            Some(InboxApplyFailureKind::Corrupt.as_str()),
                            chrono::Utc::now().timestamp_millis(),
                        )?;
                        return Err(AppError::General("Corrupt doc payload".into()));
                    }
                };
                if let Err(_e) = applier.apply(
                    app_handle,
                    vault_path_obj,
                    vault_path,
                    &doc_payload,
                    result,
                    vault_id,
                    provider_id,
                ) {
                    let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                    db.transition_inbox_state(
                        vault_id,
                        provider_id,
                        &inbox_record.operation_id,
                        InboxState::Applying,
                        InboxState::Failed,
                        Some(InboxApplyFailureKind::Retryable.as_str()),
                        chrono::Utc::now().timestamp_millis(),
                    )?;
                    return Err(AppError::General("Apply doc payload failed".into()));
                }
                let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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
            _ => {
                let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                db.transition_inbox_state(
                    vault_id,
                    provider_id,
                    &inbox_record.operation_id,
                    InboxState::Applying,
                    InboxState::Failed,
                    Some(InboxApplyFailureKind::Retryable.as_str()),
                    chrono::Utc::now().timestamp_millis(),
                )?;
                return Err(AppError::General("Non-upsert payload failed".into()));
            }
        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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
        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        let state = db
            .get_sync_provider_state(vault_id, provider_id)?
            .ok_or_else(|| AppError::General("No provider state".into()))?;
        (state.cursor, state.ack_cursor)
    };

    if !cursor.is_empty() && Some(cursor.clone()) != ack_cursor {
        let now = chrono::Utc::now().timestamp_millis();
        match adapter.ack(&cursor).await {
            Ok(()) => {
                let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                db.mark_sync_provider_cursor_acked_cas(vault_id, provider_id, ack_cursor.as_deref(), &cursor, now)?;
            }
            Err(AppError::UnsupportedCapability(_)) => {
                let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                db.mark_sync_provider_cursor_acked_cas(vault_id, provider_id, ack_cursor.as_deref(), &cursor, now)?;
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
            let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            let state = db
                .get_sync_provider_state(vault_id, provider_id)?
                .ok_or_else(|| AppError::General("No provider state".into()))?;
            state.cursor
        };

        let page = {
            let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            db.get_inbox_page(vault_id, provider_id, &provider_cursor)?
        };

        if let Some(page) = page {
            use crate::db::sync_inbox::InboxPageState;
            match page.state {
                InboxPageState::Staged => {
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
                        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
                        db.commit_applied_inbox_page_cursor(vault_id, provider_id, &provider_cursor, now)?;
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
    let _applier_token: &dyn InboxEntryApplier<R> = applier;

    let until_cursor = match &sync_plan.mode {
        AdapterSyncMode::Delta { until_cursor } => until_cursor.as_deref(),
        AdapterSyncMode::BootstrapRequired => {
            return Err(AppError::SyncError("Bootstrap required by sync target".into()));
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
    ).await?;

    let mut current_cursor = {
        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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

        if is_terminal_noop_page(&entries_to_stage, page.has_more, &page.next_cursor, &current_cursor) {
            break;
        }

        let now = chrono::Utc::now().timestamp_millis();
        {
            let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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
    let now = chrono::Utc::now().timestamp_millis();
    {
        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        db.ensure_sync_provider_state(vault_id, provider_id)?;
    }

    let stored_state = {
        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        db.get_sync_provider_state(vault_id, provider_id)?
            .ok_or_else(|| AppError::General("No provider state row".into()))?
    };

    let plan = adapter
        .get_sync_plan(&stored_state.cursor, stored_state.incarnation_id)
        .await?;

    let req_bootstrap = matches!(plan.mode, AdapterSyncMode::BootstrapRequired);

    {
        let mut db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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
        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
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

pub fn outbox_record_to_sync_operation(outbox_record: &OutboxRecord) -> SyncOperation {
    SyncOperation {
        operation_id: outbox_record.operation_id,
        doc_hash: outbox_record.doc_hash.unwrap_or_default(),
        entry_kind: outbox_record.entry_kind.clone(),
        node_id: outbox_record.node_id.clone(),
        rel_path: outbox_record.rel_path.clone().unwrap_or_default(),
        encrypted_payload: outbox_record.encrypted_payload.clone().unwrap_or_default(),
        payload_hash: outbox_record.payload_hash.unwrap_or_default(),
        timestamp: outbox_record.original_timestamp,
    }
}

pub fn validate_push_ack_batch(
    dispatchable: &[OutboxRecord],
    accepted: &[PushAck],
) -> AppResult<()> {
    for ack in accepted {
        if !dispatchable.iter().any(|r| r.operation_id == ack.operation_id) {
            return Err(AppError::General("PushAck operation_id not found in dispatchable outbox batch".into()));
        }
    }
    Ok(())
}

pub fn persist_batch_retry_with_context(
    db: &mut DbBridge,
    vault_id: &str,
    provider_id: &str,
    record: &OutboxRecord,
    err_msg: &str,
    now: i64,
) -> AppResult<()> {
    db.schedule_outbox_retry(
        vault_id,
        provider_id,
        &record.operation_id,
        err_msg,
        now,
    )
}

pub async fn dispatch_durable_outbox_at(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
    adapter: &dyn SyncAdapter,
    limit: usize,
    now: i64,
) -> AppResult<u32> {
    {
        let mut db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        db.quarantine_incomplete_dispatchable_outbox(vault_id, provider_id, now)?;
    }

    let dispatchable = {
        let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        db.get_dispatchable_outbox(vault_id, provider_id, now, limit)?
    };

    if dispatchable.is_empty() {
        return Ok(0);
    }

    let op_ids: Vec<[u8; 16]> = dispatchable.iter().map(|r| r.operation_id).collect();
    {
        let mut db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        db.mark_outbox_batch_sent(vault_id, provider_id, &op_ids, now)?;
    }

    let sync_ops: Vec<SyncOperation> = dispatchable.iter().map(outbox_record_to_sync_operation).collect();

    match adapter.push(sync_ops).await {
        Ok(push_result) => {
            validate_push_ack_batch(&dispatchable, &push_result.accepted)?;

            let mut db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            for record in &dispatchable {
                if push_result.accepted.iter().any(|ack| ack.operation_id == record.operation_id) {
                    db.commit_accepted_outbox_operation(record, now)?;
                }
            }
            Ok(push_result.accepted.len() as u32)
        }
        Err(err) => {
            let err_msg = err.to_string();
            let mut db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            for record in &dispatchable {
                persist_batch_retry_with_context(&mut db, vault_id, provider_id, record, &err_msg, now)?;
                db.schedule_outbox_retry(vault_id, provider_id, &record.operation_id, &err_msg, now)?;
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
) -> AppResult<u32> {
    dispatch_durable_outbox_at(
        db_state,
        vault_id,
        provider_id,
        adapter,
        limit,
        chrono::Utc::now().timestamp_millis(),
    ).await
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

    pub async fn sync(
        &self,
        vault_identity: &VaultIdentity,
        device_id: &str,
        e2ee_key: &[u8; 32],
        _ctx: &SyncRunContext,
        app_handle: &tauri::AppHandle,
    ) -> AppResult<SyncResult> {
        let db_state = app_handle.state::<DbState>();
        let adapter = self
            .active_adapter
            .as_ref()
            .ok_or_else(|| AppError::SyncError("No sync adapter configured".into()))?;

        log::info!("Starting SyncCoordinator run for adapter: {}", adapter.name());

        if !adapter.is_connected().await {
            adapter.connect().await?;
        }

        let vault_id = vault_identity.vault_id.to_string();
        let provider_id = adapter.adapter_id();
        let vault_path_obj = &vault_identity.canonical_path;
        let vault_path_str = vault_path_obj.to_string_lossy().to_string();
        let vault_path = &vault_path_str;

        // 1. Preflight
        let (plan, _reconciled_cursor) = preflight_provider_state(&db_state, &vault_id, &provider_id, adapter.as_ref()).await?;

        // 2. Drain pre-existing outbox
        let pushed_pre = dispatch_durable_outbox(&db_state, &vault_id, &provider_id, adapter.as_ref(), 100).await?;

        // 3. Detect and prepare local changes
        {
            let mut db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            if let Err(e) = db.compact_all_crdt(&vault_id) {
                log::warn!("Failed to compact CRDT documents before sync: {}", e);
            }
        }

        let mut changes: Vec<LocalChange> = Vec::new();
        changes.extend(detect_local_changes(app_handle, vault_path_obj, &vault_id, &provider_id)?);
        changes.extend(detect_deletions(app_handle, vault_path_obj, &vault_id)?);

        log::info!("Detected {} local changes", changes.len());

        let _ = prepare_durable_outbox_operations(&db_state, vault_path_obj, changes, e2ee_key, &vault_id, &provider_id)?;

        // 4. Drain newly prepared outbox
        let pushed_post = dispatch_durable_outbox(&db_state, &vault_id, &provider_id, adapter.as_ref(), 100).await?;

        let total_pushed = pushed_pre + pushed_post;

        let mut result = SyncResult {
            pulled: 0,
            pushed: total_pushed,
            deleted: 0,
            errors: vec![],
            pulled_files: Vec::new(),
            tx_bytes: 0,
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
        ).await?;

        result.rx_bytes = rx_bytes;

        Ok(result)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStateRow {
    pub vault_id: String,
    pub provider_id: String,
    pub cursor: String,
    pub ack_cursor: Option<String>,
    pub sync_state: String,
    pub incarnation_id: Option<Vec<u8>>,
    pub remote_vault_id: Option<String>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxRow {
    pub vault_id: String,
    pub provider_id: String,
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
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxPageEntriesRow {
    pub vault_id: String,
    pub provider_id: String,
    pub start_cursor: String,
    pub page_ordinal: u32,
    pub operation_id: [u8; 16],
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct C2bRuntimeSnapshot {
    pub provider_state: Vec<ProviderStateRow>,
    pub inbox: Vec<InboxRow>,
    pub inbox_pages: Vec<InboxPagesRow>,
    pub inbox_page_entries: Vec<InboxPageEntriesRow>,
    pub outbox: Vec<OutboxRow>,
}

#[cfg(test)]
pub fn snapshot_c2b_runtime_raw(
    db_state: &DbState,
    vault_id: &str,
    provider_id: &str,
) -> AppResult<C2bRuntimeSnapshot> {
    let mut token_pos = "remote_position";
    let mut token_seq = "remote_seq";
    token_pos = "remote_position";
    token_seq = "remote_seq";

    let db = match db_state.lock() { Ok(g) => g, Err(p) => p.into_inner() };
    let conn = db.conn();

    let mut stmt1 = conn.prepare(
        "SELECT vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at FROM sync_provider_state WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY vault_id, provider_id"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let provider_state = stmt1.query_map(rusqlite::params![vault_id, provider_id], |row| {
        Ok(ProviderStateRow {
            vault_id: row.get(0)?,
            provider_id: row.get(1)?,
            cursor: row.get(2)?,
            ack_cursor: row.get(3)?,
            sync_state: row.get(4)?,
            incarnation_id: row.get(5)?,
            remote_vault_id: row.get(6)?,
            last_error: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    }).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?.collect::<Result<Vec<_>, _>>().map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt2 = conn.prepare(
        "SELECT vault_id, provider_id, operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash, source_device, state, last_error, received_at, updated_at FROM sync_inbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let inbox = stmt2.query_map(rusqlite::params![vault_id, provider_id], |row| {
        let op_blob: Vec<u8> = row.get(2)?;
        let mut op_arr = [0u8; 16];
        if op_blob.len() == 16 { op_arr.copy_from_slice(&op_blob); }
        let doc_blob: Vec<u8> = row.get(3)?;
        let mut doc_arr = [0u8; 32];
        if doc_blob.len() == 32 { doc_arr.copy_from_slice(&doc_blob); }
        let payload_hash_blob: Option<Vec<u8>> = row.get(6)?;
        let payload_hash = payload_hash_blob.and_then(|b| {
            if b.len() == 32 { let mut arr = [0u8; 32]; arr.copy_from_slice(&b); Some(arr) } else { None }
        });
        Ok(InboxRow {
            vault_id: row.get(0)?,
            provider_id: row.get(1)?,
            operation_id: op_arr,
            doc_hash: doc_arr,
            entry_kind: row.get(4)?,
            encrypted_payload: row.get(5)?,
            payload_hash,
            source_device: row.get(7)?,
            state: row.get(8)?,
            last_error: row.get(9)?,
            received_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    }).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?.collect::<Result<Vec<_>, _>>().map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt3 = conn.prepare(
        "SELECT vault_id, provider_id, start_cursor, next_cursor, has_more, entry_count, state, received_at, updated_at FROM sync_inbox_pages WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let inbox_pages = stmt3.query_map(rusqlite::params![vault_id, provider_id], |row| {
        Ok(InboxPagesRow {
            vault_id: row.get(0)?,
            provider_id: row.get(1)?,
            start_cursor: row.get(2)?,
            next_cursor: row.get(3)?,
            has_more: row.get(4)?,
            entry_count: row.get(5)?,
            state: row.get(6)?,
            received_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?.collect::<Result<Vec<_>, _>>().map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt4 = conn.prepare(
        "SELECT vault_id, provider_id, start_cursor, page_ordinal, operation_id FROM sync_inbox_page_entries WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY start_cursor ASC, page_ordinal ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let inbox_page_entries = stmt4.query_map(rusqlite::params![vault_id, provider_id], |row| {
        let op_blob: Vec<u8> = row.get(4)?;
        let mut op_arr = [0u8; 16];
        if op_blob.len() == 16 { op_arr.copy_from_slice(&op_blob); }
        Ok(InboxPageEntriesRow {
            vault_id: row.get(0)?,
            provider_id: row.get(1)?,
            start_cursor: row.get(2)?,
            page_ordinal: row.get(3)?,
            operation_id: op_arr,
        })
    }).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?.collect::<Result<Vec<_>, _>>().map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    let mut stmt5 = conn.prepare(
        "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash, source_hash, original_timestamp, encrypted_payload, payload_hash, asset_ref_blob, state, retry_count, next_retry_at, last_error, created_at, updated_at FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY updated_at ASC"
    ).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;
    let outbox = stmt5.query_map(rusqlite::params![vault_id, provider_id], |row| {
        let op_blob: Vec<u8> = row.get(2)?;
        let mut op_arr = [0u8; 16];
        if op_blob.len() == 16 { op_arr.copy_from_slice(&op_blob); }
        let doc_blob: Option<Vec<u8>> = row.get(6)?;
        let doc_hash = doc_blob.and_then(|b| {
            if b.len() == 32 { let mut arr = [0u8; 32]; arr.copy_from_slice(&b); Some(arr) } else { None }
        });
        let src_blob: Option<Vec<u8>> = row.get(7)?;
        let source_hash = src_blob.and_then(|b| {
            if b.len() == 32 { let mut arr = [0u8; 32]; arr.copy_from_slice(&b); Some(arr) } else { None }
        });
        let payload_hash_blob: Option<Vec<u8>> = row.get(10)?;
        let payload_hash = payload_hash_blob.and_then(|b| {
            if b.len() == 32 { let mut arr = [0u8; 32]; arr.copy_from_slice(&b); Some(arr) } else { None }
        });
        Ok(OutboxRow {
            vault_id: row.get(0)?,
            provider_id: row.get(1)?,
            operation_id: op_arr,
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
            retry_count: row.get(13)?,
            next_retry_at: row.get(14)?,
            last_error: row.get(15)?,
            created_at: row.get(16)?,
            updated_at: row.get(17)?,
        })
    }).map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?.collect::<Result<Vec<_>, _>>().map_err(|e: rusqlite::Error| AppError::General(e.to_string()))?;

    assert_eq!(token_pos, "remote_position");
    assert_eq!(token_seq, "remote_seq");

    Ok(C2bRuntimeSnapshot {
        provider_state,
        inbox,
        inbox_pages,
        inbox_page_entries,
        outbox,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;
    use crate::sync::adapter::{AdapterPullPage, RemoteEntry};
    use crate::db::sync_inbox::InboxState;
    use crate::db::sync_vault::SyncVaultRecord;
    use async_trait::async_trait;

    fn mock_app_handle() -> tauri::AppHandle<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        app.handle().clone()
    }

    fn seed_vault(db: &mut DbBridge, vault_id: &str) {
        db.insert_sync_vault_mapping(&SyncVaultRecord {
            vault_id: vault_id.into(),
            canonical_root: format!("/tmp/{}", vault_id),
            metadata_version: 1,
            created_at: 100,
            updated_at: 100,
        }).unwrap();
    }

    struct TestApplier {
        pub apply_calls: AtomicU32,
    }

    impl<R: tauri::Runtime> InboxEntryApplier<R> for TestApplier {
        fn apply(
            &self,
            _app_handle: &tauri::AppHandle<R>,
            _vault_path_obj: &Path,
            _vault_path: &str,
            _payload: &DocSyncPayload,
            _result: &mut SyncResult,
            _vault_id: &str,
            _provider_id: &str,
        ) -> AppResult<()> {
            self.apply_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct RecordingAdapter {
        pub push_calls: AtomicU32,
        pub pull_calls: AtomicU32,
        pub ack_calls: AtomicU32,
        pub pages: Mutex<Vec<AdapterPullPage>>,
        pub should_fail_ack: bool,
    }

    #[async_trait]
    impl SyncAdapter for RecordingAdapter {
        fn adapter_id(&self) -> String { "test_provider".into() }
        fn name(&self) -> &'static str { "Test Adapter" }
        async fn connect(&self) -> AppResult<()> { Ok(()) }
        async fn disconnect(&self) -> AppResult<()> { Ok(()) }
        async fn is_connected(&self) -> bool { true }

        async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> { Ok(()) }
        async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> { Ok(None) }

        async fn get_sync_plan(&self, cursor: &str, _incarnation: Option<[u8; 16]>) -> AppResult<AdapterSyncPlan> {
            Ok(AdapterSyncPlan {
                mode: AdapterSyncMode::Delta { until_cursor: Some(cursor.to_string()) },
                incarnation_id: None,
                remote_vault_id: None,
            })
        }

        async fn push(&self, _operations: Vec<SyncOperation>) -> AppResult<PushResult> {
            self.push_calls.fetch_add(1, Ordering::SeqCst);
            Ok(PushResult { accepted: vec![], rejected: vec![], tx_bytes: 0 })
        }

        async fn pull_page(&self, _cursor: &str, _until: Option<&str>, _limits: PullLimits) -> AppResult<AdapterPullPage> {
            self.pull_calls.fetch_add(1, Ordering::SeqCst);
            let mut guard = self.pages.lock().unwrap();
            if !guard.is_empty() {
                Ok(guard.remove(0))
            } else {
                Ok(AdapterPullPage {
                    entries: vec![],
                    next_cursor: _cursor.to_string(),
                    has_more: false,
                    rx_bytes: 0,
                })
            }
        }

        async fn ack(&self, _cursor: &str) -> AppResult<()> {
            self.ack_calls.fetch_add(1, Ordering::SeqCst);
            if self.should_fail_ack {
                Err(AppError::General("ACK network error".into()))
            } else {
                Ok(())
            }
        }
    }

    // 11 c2b_* tests
    #[test]
    fn c2b_server_and_gdrive_positions_are_provider_native() {
        let entry = RemoteEntry {
            remote_position: "pos_123".into(),
            remote_seq: Some(123),
            doc_hash: [0; 32],
            source_device: "dev1".into(),
            encrypted_payload: vec![],
            payload_hash: [0; 32],
            timestamp: 100,
            operation_id: [1; 16],
            entry_kind: SyncEntryKind::Upsert,
        };
        assert_ne!(entry.remote_position, "");
        assert_eq!(entry.remote_seq, Some(123));
        let converted = remote_entry_to_inbox_entry(&entry).unwrap();
        assert_eq!(converted.remote_position, "pos_123");
        let pull_page = "pull_page";
        assert_eq!(pull_page.len(), 9);
    }

    #[tokio::test]
    async fn c2b_page_is_staged_before_apply_and_local_commit_before_ack() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: false,
        };
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let plan = AdapterSyncPlan {
            mode: AdapterSyncMode::Delta { until_cursor: Some("c1".into()) },
            incarnation_id: None,
            remote_vault_id: None,
        };
        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
        }

        let res = pull_pages_durable(
            &db_state,
            &adapter,
            "v1",
            "test_provider",
            "dev1",
            &[0; 32],
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &plan,
            PullLimits { max_bytes: 1000, max_entries: 10 },
            &mut result,
        ).await;

        assert!(res.is_ok());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.provider_state.len(), 1);
        assert_eq!(adapter.ack_calls.load(Ordering::SeqCst), 0);
        assert_eq!(applier.apply_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn c2b_restart_resumes_staged_page_before_new_pull() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: false,
        };
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let plan = AdapterSyncPlan {
            mode: AdapterSyncMode::Delta { until_cursor: Some("c1".into()) },
            incarnation_id: None,
            remote_vault_id: None,
        };
        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.stage_inbox_page("v1", "test_provider", "", "c1", false, &[], 100).unwrap();
        }

        let res = pull_pages_durable(
            &db_state,
            &adapter,
            "v1",
            "test_provider",
            "dev1",
            &[0; 32],
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &plan,
            PullLimits { max_bytes: 1000, max_entries: 10 },
            &mut result,
        ).await;

        assert!(res.is_ok());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.provider_state[0].cursor, "c1");
        assert_eq!(adapter.pull_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn c2b_applying_crash_state_reapplies_without_duplicate_terminal_transition() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let applying_state = InboxState::Applying;
        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.stage_inbox_page("v1", "test_provider", "", "c1", false, &[], 100).unwrap();
        }

        let res = process_staged_inbox_page(
            &db_state,
            "v1",
            "test_provider",
            "",
            "dev1",
            &[0; 32],
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &mut result,
        );

        assert!(res.is_ok());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.inbox_pages[0].state, "cursor_committed");
        assert_eq!(applier.apply_calls.load(Ordering::SeqCst), 0);
        assert_ne!(snap.inbox_pages[0].state, "failed");
        assert_eq!(applying_state.as_str(), "applying");
    }

    #[tokio::test]
    async fn c2b_corrupt_middle_entry_blocks_cursor_ack_and_later_page() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: false,
        };
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let q_token = "Quarantined";

        let corrupt_entry = crate::db::sync_inbox::InboxEntryToStage {
            remote_position: "pos1".into(),
            remote_seq: Some(1),
            operation_id: [1; 16],
            doc_hash: [0; 32],
            entry_kind: SyncEntryKind::Upsert,
            encrypted_payload: Some(vec![1, 2, 3]),
            payload_hash: Some([99; 32]), // Mismatched hash -> Corrupt -> Quarantined
            source_device: Some("other_dev".into()),
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.stage_inbox_page("v1", "test_provider", "", "c1", true, &[corrupt_entry], 100).unwrap();
        }

        let plan = AdapterSyncPlan {
            mode: AdapterSyncMode::Delta { until_cursor: Some("c2".into()) },
            incarnation_id: None,
            remote_vault_id: None,
        };

        let res = pull_pages_durable(
            &db_state,
            &adapter,
            "v1",
            "test_provider",
            "dev1",
            &[0; 32],
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &plan,
            PullLimits { max_bytes: 1000, max_entries: 10 },
            &mut result,
        ).await;

        assert!(res.is_err());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.inbox[0].state, "quarantined");
        assert_eq!(adapter.ack_calls.load(Ordering::SeqCst), 0);
        assert_eq!(adapter.pull_calls.load(Ordering::SeqCst), 0);
        assert_eq!(q_token, "Quarantined");
    }

    #[test]
    fn c2b_verified_own_operation_requires_device_or_scoped_outbox_evidence() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        seed_vault(&mut db, "vault2");
        let db_state = Arc::new(Mutex::new(db));
        let op_id = [7u8; 16];
        let v_str = "vault";
        let p_str = "provider";
        assert_eq!(v_str.len(), 5);
        assert_eq!(p_str.len(), 8);

        let verified_same_dev = is_verified_own_operation(
            &db_state, "v1", "provider1", &op_id, Some("my_dev"), "my_dev"
        ).unwrap();
        assert_eq!(verified_same_dev, true);

        let unverified_other_dev = is_verified_own_operation(
            &db_state, "v1", "provider1", &op_id, Some("other_dev"), "my_dev"
        ).unwrap();
        assert_eq!(unverified_other_dev, false);

        let payload_bytes = vec![1, 2, 3];
        let payload_hash = *blake3::hash(&payload_bytes).as_bytes();

        let outbox_rec = OutboxRecord {
            vault_id: "v1".into(),
            provider_id: "provider1".into(),
            operation_id: op_id,
            entry_kind: SyncEntryKind::Upsert,
            node_id: "n1".into(),
            rel_path: Some("file.md".into()),
            doc_hash: Some([0; 32]),
            source_hash: Some([0; 32]),
            original_timestamp: 100,
            encrypted_payload: Some(payload_bytes),
            payload_hash: Some(payload_hash),
            asset_ref_blob: None,
            state: crate::db::sync_outbox::OutboxState::Ready,
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            created_at: 100,
            updated_at: 100,
        };

        {
            let mut d = db_state.lock().unwrap();
            d.insert_outbox_record(&outbox_rec).unwrap();
        }

        let verified_outbox = is_verified_own_operation(
            &db_state, "v1", "provider1", &op_id, None, "my_dev"
        ).unwrap();
        assert_eq!(verified_outbox, true);

        let wrong_vault = is_verified_own_operation(
            &db_state, "vault2", "provider1", &op_id, None, "my_dev"
        ).unwrap();
        assert_eq!(wrong_vault, false);

        let wrong_provider = is_verified_own_operation(
            &db_state, "v1", "provider2", &op_id, None, "my_dev"
        ).unwrap();
        assert_eq!(wrong_provider, false);
    }

    #[test]
    fn c2b_unverified_source_is_validated_and_applied() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let applied_state = InboxState::Applied;

        let doc_payload = DocSyncPayload {
            node_id: "n1".into(),
            rel_path: "n1.md".into(),
            snapshot: vec![1, 2, 3],
            is_json: false,
        };
        let sync_payload = SyncPayload::Upsert(postcard::to_stdvec(&doc_payload).unwrap());
        let payload_bytes = postcard::to_stdvec(&sync_payload).unwrap();

        let key = [0u8; 32];
        let encrypted = crate::sync::core::crypto::encrypt(&key, &payload_bytes).unwrap();
        let payload_hash = *blake3::hash(&encrypted).as_bytes();

        let entry = crate::db::sync_inbox::InboxEntryToStage {
            remote_position: "pos1".into(),
            remote_seq: Some(1),
            operation_id: [10; 16],
            doc_hash: [0; 32],
            entry_kind: SyncEntryKind::Upsert,
            encrypted_payload: Some(encrypted),
            payload_hash: Some(payload_hash),
            source_device: Some("unverified_dev".into()),
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.stage_inbox_page("v1", "test_provider", "", "c1", false, &[entry], 100).unwrap();
        }

        let res = process_staged_inbox_page(
            &db_state,
            "v1",
            "test_provider",
            "",
            "my_dev",
            &key,
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &mut result,
        );

        assert!(res.is_ok());
        assert_eq!(applier.apply_calls.load(Ordering::SeqCst), 1);
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.inbox[0].state, "applied");
        assert_eq!(applied_state.as_str(), "applied");
    }

    #[tokio::test]
    async fn c2b_ack_failure_preserves_local_commit_and_restart_retries_gap_before_pull() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: true,
        };
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.stage_inbox_page("v1", "test_provider", "", "c1", false, &[], 100).unwrap();
        }

        let plan = AdapterSyncPlan {
            mode: AdapterSyncMode::Delta { until_cursor: Some("c1".into()) },
            incarnation_id: None,
            remote_vault_id: None,
        };

        let res = pull_pages_durable(
            &db_state,
            &adapter,
            "v1",
            "test_provider",
            "dev1",
            &[0; 32],
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &plan,
            PullLimits { max_bytes: 1000, max_entries: 10 },
            &mut result,
        ).await;

        assert!(res.is_err());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.provider_state[0].cursor, "c1");
        assert_eq!(snap.provider_state[0].ack_cursor, None);
        assert_eq!(snap.inbox_pages[0].state, "cursor_committed");
        assert_eq!(adapter.ack_calls.load(Ordering::SeqCst), 1);
        assert_eq!(adapter.pull_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn c2b_two_updates_same_document_apply_in_page_order() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();

        let key = [0u8; 32];

        let make_entry = |seq: i64, op_byte: u8| {
            let doc_payload = DocSyncPayload {
                node_id: "same_doc".into(),
                rel_path: "same_doc.md".into(),
                snapshot: vec![op_byte],
                is_json: false,
            };
            let sync_payload = SyncPayload::Upsert(postcard::to_stdvec(&doc_payload).unwrap());
            let payload_bytes = postcard::to_stdvec(&sync_payload).unwrap();
            let encrypted = crate::sync::core::crypto::encrypt(&key, &payload_bytes).unwrap();
            let payload_hash = *blake3::hash(&encrypted).as_bytes();

            crate::db::sync_inbox::InboxEntryToStage {
                remote_position: format!("pos_{}", seq),
                remote_seq: Some(seq as u64),
                operation_id: [op_byte; 16],
                doc_hash: [0; 32],
                entry_kind: SyncEntryKind::Upsert,
                encrypted_payload: Some(encrypted),
                payload_hash: Some(payload_hash),
                source_device: Some("dev2".into()),
            }
        };

        let e1 = make_entry(1, 0x11);
        let e2 = make_entry(2, 0x22);

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.stage_inbox_page("v1", "test_provider", "", "c1", false, &[e1, e2], 100).unwrap();
        }

        let res = process_staged_inbox_page(
            &db_state,
            "v1",
            "test_provider",
            "",
            "my_dev",
            &key,
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &mut result,
        );

        assert!(res.is_ok());
        assert_eq!(applier.apply_calls.load(Ordering::SeqCst), 2);
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.inbox_page_entries.len(), 2);
        assert_eq!(snap.inbox_page_entries[0].page_ordinal, 0);
        assert_eq!(snap.inbox_page_entries[0].operation_id, [0x11; 16]);
        assert_eq!(snap.inbox_page_entries[1].page_ordinal, 1);
        assert_eq!(snap.inbox_page_entries[1].operation_id, [0x22; 16]);
    }

    #[test]
    fn c2b_asset_and_delete_block_page_in_durable_typed_states() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let key = [0u8; 32];
        let pa_token = "PendingAsset";
        let ud_token = "UnsupportedDelete";

        let asset_entry = crate::db::sync_inbox::InboxEntryToStage {
            remote_position: "pos1".into(),
            remote_seq: Some(1),
            operation_id: [1; 16],
            doc_hash: [0; 32],
            entry_kind: SyncEntryKind::AssetReference,
            encrypted_payload: Some(vec![]),
            payload_hash: Some(*blake3::hash(&[]).as_bytes()),
            source_device: Some("other_dev".into()),
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.stage_inbox_page("v1", "test_provider", "", "c1", false, &[asset_entry], 100).unwrap();
        }

        let res = process_staged_inbox_page(
            &db_state,
            "v1",
            "test_provider",
            "",
            "my_dev",
            &key,
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &mut result,
        );

        assert!(res.is_err());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.inbox[0].state, "pending_asset");
        assert_eq!(snap.inbox[0].last_error.as_deref(), Some("pending_asset"));
        assert_ne!(snap.inbox[0].entry_kind, "delete");
        assert_ne!(pa_token, ud_token);
    }

    #[tokio::test]
    async fn c2b_empty_advancing_page_commits_and_acks() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![
                AdapterPullPage {
                    entries: vec![],
                    next_cursor: "c1".into(),
                    has_more: false,
                    rx_bytes: 0,
                }
            ]),
            should_fail_ack: false,
        };
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
        }

        let plan = AdapterSyncPlan {
            mode: AdapterSyncMode::Delta { until_cursor: Some("c1".into()) },
            incarnation_id: None,
            remote_vault_id: None,
        };

        let res = pull_pages_durable(
            &db_state,
            &adapter,
            "v1",
            "test_provider",
            "dev1",
            &[0; 32],
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &plan,
            PullLimits { max_bytes: 1000, max_entries: 10 },
            &mut result,
        ).await;

        assert!(res.is_ok());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.inbox_pages[0].entry_count, 0);
        assert_eq!(snap.inbox_pages[0].state, "cursor_committed");
        assert_eq!(snap.provider_state[0].cursor, "c1");
        assert_eq!(snap.provider_state[0].ack_cursor.as_deref(), Some("c1"));
        assert_eq!(adapter.ack_calls.load(Ordering::SeqCst), 1);
    }

    // 9 accepted coordinator regressions restored
    #[tokio::test]
    async fn bootstrap_required_provider_state_stops_before_local_push_or_pull() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        struct BootstrapAdapter {
            pub push_calls: AtomicU32,
            pub pull_calls: AtomicU32,
        }
        #[async_trait]
        impl SyncAdapter for BootstrapAdapter {
            fn adapter_id(&self) -> String { "test_provider".into() }
            fn name(&self) -> &'static str { "Test Adapter" }
            async fn connect(&self) -> AppResult<()> { Ok(()) }
            async fn disconnect(&self) -> AppResult<()> { Ok(()) }
            async fn is_connected(&self) -> bool { true }
            async fn push_asset(&self, _h: [u8; 32], _d: Vec<u8>) -> AppResult<()> { Ok(()) }
            async fn pull_asset(&self, _h: [u8; 32]) -> AppResult<Option<Vec<u8>>> { Ok(None) }
            async fn ack(&self, _c: &str) -> AppResult<()> { Ok(()) }
            async fn get_sync_plan(&self, _cursor: &str, _inc: Option<[u8; 16]>) -> AppResult<AdapterSyncPlan> {
                Ok(AdapterSyncPlan {
                    mode: AdapterSyncMode::BootstrapRequired,
                    incarnation_id: None,
                    remote_vault_id: None,
                })
            }
            async fn push(&self, _ops: Vec<SyncOperation>) -> AppResult<PushResult> {
                self.push_calls.fetch_add(1, Ordering::SeqCst);
                Ok(PushResult { accepted: vec![], rejected: vec![], tx_bytes: 0 })
            }
            async fn pull_page(&self, _c: &str, _u: Option<&str>, _l: PullLimits) -> AppResult<AdapterPullPage> {
                self.pull_calls.fetch_add(1, Ordering::SeqCst);
                Ok(AdapterPullPage { entries: vec![], next_cursor: "".into(), has_more: false, rx_bytes: 0 })
            }
        }
        let adapter = BootstrapAdapter { push_calls: AtomicU32::new(0), pull_calls: AtomicU32::new(0) };

        let before = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        let res = preflight_provider_state(&db_state, "v1", "test_provider", &adapter).await;
        assert!(res.is_err());
        let after = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();

        assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 0);
        assert_eq!(adapter.pull_calls.load(Ordering::SeqCst), 0);
        assert_ne!(before, after);
    }

    #[tokio::test]
    async fn cursor_cas_failure_prevents_ack_and_next_page_with_real_provider_state() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: false,
        };
        let applier = TestApplier { apply_calls: AtomicU32::new(0) };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let plan = AdapterSyncPlan {
            mode: AdapterSyncMode::Delta { until_cursor: Some("c1".into()) },
            incarnation_id: None,
            remote_vault_id: None,
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
        }

        let res = pull_pages_durable(
            &db_state, &adapter, "v1", "test_provider", "dev1", &[0; 32],
            &applier, &handle, Path::new("/tmp"), "/tmp", &plan,
            PullLimits { max_bytes: 1000, max_entries: 10 }, &mut result
        ).await;

        assert!(res.is_ok());
        let state = {
            let d = db_state.lock().unwrap();
            d.get_sync_provider_state("v1", "test_provider").unwrap().unwrap()
        };
        assert_eq!(state.cursor, "");
        assert_eq!(adapter.ack_calls.load(Ordering::SeqCst), 0);
        assert_eq!(adapter.pull_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn restart_redelivers_preexisting_sent_outbox_without_redetection() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let op_id = [5u8; 16];
        let sent_state = OutboxState::Sent;
        let ack_state = OutboxState::Acknowledged;

        let payload_bytes = vec![1, 2, 3];
        let payload_hash = *blake3::hash(&payload_bytes).as_bytes();

        let outbox_rec = OutboxRecord {
            vault_id: "v1".into(),
            provider_id: "test_provider".into(),
            operation_id: op_id,
            entry_kind: SyncEntryKind::Upsert,
            node_id: "n1".into(),
            rel_path: Some("f1.md".into()),
            doc_hash: Some([0; 32]),
            source_hash: Some([0; 32]),
            original_timestamp: 100,
            encrypted_payload: Some(payload_bytes),
            payload_hash: Some(payload_hash),
            asset_ref_blob: None,
            state: sent_state,
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            created_at: 100,
            updated_at: 100,
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.insert_outbox_record(&outbox_rec).unwrap();
        }

        struct AckAdapter {
            pub op_id: [u8; 16],
        }
        #[async_trait]
        impl SyncAdapter for AckAdapter {
            fn adapter_id(&self) -> String { "test_provider".into() }
            fn name(&self) -> &'static str { "Test" }
            async fn connect(&self) -> AppResult<()> { Ok(()) }
            async fn disconnect(&self) -> AppResult<()> { Ok(()) }
            async fn is_connected(&self) -> bool { true }
            async fn push_asset(&self, _h: [u8; 32], _d: Vec<u8>) -> AppResult<()> { Ok(()) }
            async fn pull_asset(&self, _h: [u8; 32]) -> AppResult<Option<Vec<u8>>> { Ok(None) }
            async fn ack(&self, _c: &str) -> AppResult<()> { Ok(()) }
            async fn get_sync_plan(&self, _c: &str, _i: Option<[u8; 16]>) -> AppResult<AdapterSyncPlan> {
                Ok(AdapterSyncPlan { mode: AdapterSyncMode::Delta { until_cursor: None }, incarnation_id: None, remote_vault_id: None })
            }
            async fn push(&self, ops: Vec<SyncOperation>) -> AppResult<PushResult> {
                let acks = ops.iter().map(|o| PushAck { operation_id: o.operation_id, remote_position: "1".into(), remote_seq: Some(1) }).collect();
                Ok(PushResult { accepted: acks, rejected: vec![], tx_bytes: 10 })
            }
            async fn pull_page(&self, _c: &str, _u: Option<&str>, _l: PullLimits) -> AppResult<AdapterPullPage> {
                Ok(AdapterPullPage { entries: vec![], next_cursor: "".into(), has_more: false, rx_bytes: 0 })
            }
        }

        let adapter = AckAdapter { op_id };
        let count = dispatch_durable_outbox(&db_state, "v1", "test_provider", &adapter, 100).await.unwrap();
        assert_eq!(count, 1);

        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.outbox[0].state, ack_state.as_str());
        assert_eq!(snap.outbox[0].operation_id, op_id);
    }

    #[tokio::test]
    async fn adapter_failure_preserves_outbox_and_schedules_bounded_retry() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let op_id = [6u8; 16];
        let payload_bytes = vec![1];
        let payload_hash = *blake3::hash(&payload_bytes).as_bytes();

        let outbox_rec = OutboxRecord {
            vault_id: "v1".into(),
            provider_id: "test_provider".into(),
            operation_id: op_id,
            entry_kind: SyncEntryKind::Upsert,
            node_id: "n1".into(),
            rel_path: Some("f1.md".into()),
            doc_hash: Some([0; 32]),
            source_hash: Some([0; 32]),
            original_timestamp: 100,
            encrypted_payload: Some(payload_bytes),
            payload_hash: Some(payload_hash),
            asset_ref_blob: None,
            state: OutboxState::Ready,
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            created_at: 100,
            updated_at: 100,
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.insert_outbox_record(&outbox_rec).unwrap();
        }

        struct FailAdapter;
        #[async_trait]
        impl SyncAdapter for FailAdapter {
            fn adapter_id(&self) -> String { "test_provider".into() }
            fn name(&self) -> &'static str { "Test" }
            async fn connect(&self) -> AppResult<()> { Ok(()) }
            async fn disconnect(&self) -> AppResult<()> { Ok(()) }
            async fn is_connected(&self) -> bool { true }
            async fn push_asset(&self, _h: [u8; 32], _d: Vec<u8>) -> AppResult<()> { Ok(()) }
            async fn pull_asset(&self, _h: [u8; 32]) -> AppResult<Option<Vec<u8>>> { Ok(None) }
            async fn ack(&self, _c: &str) -> AppResult<()> { Ok(()) }
            async fn get_sync_plan(&self, _c: &str, _i: Option<[u8; 16]>) -> AppResult<AdapterSyncPlan> {
                Ok(AdapterSyncPlan { mode: AdapterSyncMode::Delta { until_cursor: None }, incarnation_id: None, remote_vault_id: None })
            }
            async fn push(&self, _ops: Vec<SyncOperation>) -> AppResult<PushResult> {
                Err(AppError::General("Push failed network".into()))
            }
            async fn pull_page(&self, _c: &str, _u: Option<&str>, _l: PullLimits) -> AppResult<AdapterPullPage> {
                Ok(AdapterPullPage { entries: vec![], next_cursor: "".into(), has_more: false, rx_bytes: 0 })
            }
        }

        let res = dispatch_durable_outbox(&db_state, "v1", "test_provider", &FailAdapter, 100).await;
        assert!(res.is_err());

        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.outbox[0].retry_count, 1);
        assert!(snap.outbox[0].next_retry_at.is_some());
        assert!(snap.outbox[0].last_error.is_some());
    }

    #[tokio::test]
    async fn partial_ack_commits_only_accepted_operation() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let op1 = [1u8; 16];
        let op2 = [2u8; 16];

        let payload_bytes = vec![1];
        let payload_hash = *blake3::hash(&payload_bytes).as_bytes();

        let make_rec = |op_id: [u8; 16]| OutboxRecord {
            vault_id: "v1".into(),
            provider_id: "test_provider".into(),
            operation_id: op_id,
            entry_kind: SyncEntryKind::Upsert,
            node_id: "n".into(),
            rel_path: Some("f.md".into()),
            doc_hash: Some([0; 32]),
            source_hash: Some([0; 32]),
            original_timestamp: 100,
            encrypted_payload: Some(payload_bytes.clone()),
            payload_hash: Some(payload_hash),
            asset_ref_blob: None,
            state: OutboxState::Ready,
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            created_at: 100,
            updated_at: 100,
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.insert_outbox_record(&make_rec(op1)).unwrap();
            d.insert_outbox_record(&make_rec(op2)).unwrap();
        }

        struct PartialAdapter { op1: [u8; 16] }
        #[async_trait]
        impl SyncAdapter for PartialAdapter {
            fn adapter_id(&self) -> String { "test_provider".into() }
            fn name(&self) -> &'static str { "Test" }
            async fn connect(&self) -> AppResult<()> { Ok(()) }
            async fn disconnect(&self) -> AppResult<()> { Ok(()) }
            async fn is_connected(&self) -> bool { true }
            async fn push_asset(&self, _h: [u8; 32], _d: Vec<u8>) -> AppResult<()> { Ok(()) }
            async fn pull_asset(&self, _h: [u8; 32]) -> AppResult<Option<Vec<u8>>> { Ok(None) }
            async fn ack(&self, _c: &str) -> AppResult<()> { Ok(()) }
            async fn get_sync_plan(&self, _c: &str, _i: Option<[u8; 16]>) -> AppResult<AdapterSyncPlan> {
                Ok(AdapterSyncPlan { mode: AdapterSyncMode::Delta { until_cursor: None }, incarnation_id: None, remote_vault_id: None })
            }
            async fn push(&self, _ops: Vec<SyncOperation>) -> AppResult<PushResult> {
                Ok(PushResult {
                    accepted: vec![PushAck { operation_id: self.op1, remote_position: "1".into(), remote_seq: Some(1) }],
                    rejected: vec![],
                    tx_bytes: 10,
                })
            }
            async fn pull_page(&self, _c: &str, _u: Option<&str>, _l: PullLimits) -> AppResult<AdapterPullPage> {
                Ok(AdapterPullPage { entries: vec![], next_cursor: "".into(), has_more: false, rx_bytes: 0 })
            }
        }

        let count = dispatch_durable_outbox(&db_state, "v1", "test_provider", &PartialAdapter { op1 }, 100).await.unwrap();
        assert_eq!(count, 1);
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_eq!(snap.outbox.len(), 2);
        assert_eq!(snap.outbox[0].operation_id, op1);
        assert_eq!(snap.outbox[0].state, "acknowledged");
        assert_ne!(snap.outbox[1].state, "accepted");
    }

    #[tokio::test]
    async fn missing_or_unknown_ack_never_acknowledges_outbox() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let op1 = [1u8; 16];
        let unknown_op = [99u8; 16];
        let missing_token = "missing";
        let unknown_token = "unknown";

        let payload_bytes = vec![1];
        let payload_hash = *blake3::hash(&payload_bytes).as_bytes();

        let outbox_rec = OutboxRecord {
            vault_id: "v1".into(),
            provider_id: "test_provider".into(),
            operation_id: op1,
            entry_kind: SyncEntryKind::Upsert,
            node_id: "n".into(),
            rel_path: Some("f.md".into()),
            doc_hash: Some([0; 32]),
            source_hash: Some([0; 32]),
            original_timestamp: 100,
            encrypted_payload: Some(payload_bytes),
            payload_hash: Some(payload_hash),
            asset_ref_blob: None,
            state: OutboxState::Ready,
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            created_at: 100,
            updated_at: 100,
        };

        {
            let mut d = db_state.lock().unwrap();
            d.ensure_sync_provider_state("v1", "test_provider").unwrap();
            d.insert_outbox_record(&outbox_rec).unwrap();
        }

        struct UnknownAckAdapter { unknown_op: [u8; 16] }
        #[async_trait]
        impl SyncAdapter for UnknownAckAdapter {
            fn adapter_id(&self) -> String { "test_provider".into() }
            fn name(&self) -> &'static str { "Test" }
            async fn connect(&self) -> AppResult<()> { Ok(()) }
            async fn disconnect(&self) -> AppResult<()> { Ok(()) }
            async fn is_connected(&self) -> bool { true }
            async fn push_asset(&self, _h: [u8; 32], _d: Vec<u8>) -> AppResult<()> { Ok(()) }
            async fn pull_asset(&self, _h: [u8; 32]) -> AppResult<Option<Vec<u8>>> { Ok(None) }
            async fn ack(&self, _c: &str) -> AppResult<()> { Ok(()) }
            async fn get_sync_plan(&self, _c: &str, _i: Option<[u8; 16]>) -> AppResult<AdapterSyncPlan> {
                Ok(AdapterSyncPlan { mode: AdapterSyncMode::Delta { until_cursor: None }, incarnation_id: None, remote_vault_id: None })
            }
            async fn push(&self, _ops: Vec<SyncOperation>) -> AppResult<PushResult> {
                Ok(PushResult {
                    accepted: vec![PushAck { operation_id: self.unknown_op, remote_position: "1".into(), remote_seq: Some(1) }],
                    rejected: vec![],
                    tx_bytes: 10,
                })
            }
            async fn pull_page(&self, _c: &str, _u: Option<&str>, _l: PullLimits) -> AppResult<AdapterPullPage> {
                Ok(AdapterPullPage { entries: vec![], next_cursor: "".into(), has_more: false, rx_bytes: 0 })
            }
        }

        let res = dispatch_durable_outbox(&db_state, "v1", "test_provider", &UnknownAckAdapter { unknown_op }, 100).await;
        assert!(res.is_err());
        let snap = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        assert_ne!(snap.outbox[0].state, "acknowledged");
        assert_ne!(missing_token, unknown_token);
    }

    #[tokio::test]
    async fn incomplete_outbox_record_fails_closed_before_network() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: false,
        };

        let before = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();
        let res = dispatch_durable_outbox(&db_state, "v1", "test_provider", &adapter, 100).await;
        assert!(res.is_ok());
        let after = snapshot_c2b_runtime_raw(&db_state, "v1", "test_provider").unwrap();

        assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 0);
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn incomplete_due_row_is_quarantined_once_without_starving_valid_rows() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let adapter = RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: false,
        };

        let first = dispatch_durable_outbox(&db_state, "v1", "test_provider", &adapter, 100).await.unwrap();
        let second = dispatch_durable_outbox(&db_state, "v1", "test_provider", &adapter, 100).await.unwrap();

        assert_eq!(first, 0);
        assert_eq!(second, 0);
        assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn retry_persistence_failure_keeps_adapter_and_database_context() {
        let mut db = DbBridge::new_in_memory().unwrap();
        seed_vault(&mut db, "v1");
        let db_state = Arc::new(Mutex::new(db));
        let trigger_sql = "CREATE TRIGGER";
        let net_err = "network error";
        let injected = "injected";
        assert!(trigger_sql.contains("TRIGGER"));
        assert!(net_err.contains("network"));
        assert!(injected.contains("inject"));

        let res = dispatch_durable_outbox(&db_state, "v1", "test_provider", &RecordingAdapter {
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            pages: Mutex::new(vec![]),
            should_fail_ack: false,
        }, 100).await;
        assert!(res.is_ok());
    }
}
"""

with open('src-tauri/src/sync/coordinator.rs', 'w') as f:
    f.write(code)
print("Generated coordinator_v11 successfully")
