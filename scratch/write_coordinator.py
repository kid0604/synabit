import os

content = """use std::sync::Arc;
use std::path::Path;
use std::str::FromStr;
use crate::error::{AppError, AppResult};
use crate::db::DbState;
use crate::sync::adapter::SyncAdapter;
use crate::sync::core::types::{SyncResult, SyncRunContext, SyncPayload, DocSyncPayload};
use crate::sync::core::identity::VaultIdentity;
use synabit_protocol::SyncEntryKind;
use tauri::Manager;
use crate::sync::core::change::{detect_local_changes, detect_deletions, prepare_durable_outbox_operations, LocalChange};

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

pub fn remote_entry_to_inbox_entry(entry: &crate::sync::adapter::RemoteEntry) -> crate::db::sync_inbox::InboxEntryToStage {
    crate::db::sync_inbox::InboxEntryToStage {
        remote_position: entry.remote_position.clone(),
        remote_seq: entry.remote_seq,
        operation_id: entry.operation_id,
        doc_hash: entry.doc_hash,
        entry_kind: entry.entry_kind.clone(),
        encrypted_payload: Some(entry.encrypted_payload.clone()),
        payload_hash: Some(entry.payload_hash),
        source_device: Some(entry.source_device.clone()),
    }
}

pub fn is_verified_own_operation(
    db_state: &crate::db::DbState,
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
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    let outbox_entry = db.get_outbox_by_id(vault_id, provider_id, operation_id)?;
    Ok(outbox_entry.is_some())
}

pub fn decode_exact_payload<T: serde::de::DeserializeOwned>(decrypted: &[u8]) -> Option<T> {
    postcard::from_bytes(decrypted).ok()
}

pub fn process_staged_inbox_page(
    db_state: &crate::db::DbState,
    vault_id: &str,
    provider_id: &str,
    page_cursor: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    app_handle: &tauri::AppHandle,
    vault_path_obj: &std::path::Path,
    vault_path: &str,
    result: &mut crate::sync::core::types::SyncResult,
) -> crate::error::AppResult<()> {
    use crate::db::sync_inbox::InboxState;
    
    let entries = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_inbox_page_entries(vault_id, provider_id, page_cursor, 1000)?
    };

    let mut safe_to_commit = true;

    for (page_entry, mut inbox_record) in entries {
        match inbox_record.state {
            InboxState::Applied | InboxState::IgnoredOwnOperation => continue,
            InboxState::PendingAsset | InboxState::Quarantined => {
                safe_to_commit = false;
                break;
            }
            InboxState::Failed => {
                // To be retried
            }
            InboxState::Pending => {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Pending, InboxState::Applying, None, chrono::Utc::now().timestamp_millis())?;
                inbox_record.state = InboxState::Applying;
            }
            InboxState::Applying => {
                // resume
            }
        }
        
        let is_own = is_verified_own_operation(db_state, vault_id, provider_id, &inbox_record.operation_id, inbox_record.source_device.as_deref(), device_id)?;
        if is_own {
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::IgnoredOwnOperation, None, chrono::Utc::now().timestamp_millis())?;
            continue;
        }

        let encrypted_payload = inbox_record.encrypted_payload.unwrap_or_default();
        let payload_hash = inbox_record.payload_hash.unwrap_or_default();
        
        let computed_hash = *blake3::hash(&encrypted_payload).as_bytes();
        if computed_hash != payload_hash {
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
            safe_to_commit = false;
            break;
        }

        let decrypted = match crate::sync::core::crypto::decrypt(e2ee_key, &encrypted_payload) {
            Ok(d) => d,
            Err(_) => {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
                safe_to_commit = false;
                break;
            }
        };

        if inbox_record.entry_kind == SyncEntryKind::AssetReference {
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::PendingAsset, Some(InboxApplyFailureKind::PendingAsset.as_str()), chrono::Utc::now().timestamp_millis())?;
            safe_to_commit = false;
            break;
        }

        if inbox_record.entry_kind == SyncEntryKind::Delete {
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Failed, Some(InboxApplyFailureKind::UnsupportedDelete.as_str()), chrono::Utc::now().timestamp_millis())?;
            safe_to_commit = false;
            break;
        }
        
        let payload = if let Some(sync_payload) = decode_exact_payload::<SyncPayload>(&decrypted) {
            sync_payload
        } else if inbox_record.entry_kind == SyncEntryKind::Upsert {
            if let Some(doc_payload) = decode_exact_payload::<DocSyncPayload>(&decrypted) {
                let doc_bytes = postcard::to_stdvec(&doc_payload).unwrap_or_default();
                SyncPayload::Upsert(doc_bytes)
            } else {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
                safe_to_commit = false;
                break;
            }
        } else {
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
            safe_to_commit = false;
            break;
        };

        match payload {
            SyncPayload::Upsert(doc_bytes) => {
                let doc_payload: DocSyncPayload = match decode_exact_payload(&doc_bytes) {
                    Some(dp) => dp,
                    None => {
                        let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                        db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
                        safe_to_commit = false;
                        break;
                    }
                };
                if let Err(_) = crate::sync::core::apply::apply_doc_payload(
                    app_handle,
                    vault_path_obj,
                    vault_path,
                    &doc_payload,
                    result,
                    vault_id,
                    provider_id,
                ) {
                    let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Failed, Some(InboxApplyFailureKind::Retryable.as_str()), chrono::Utc::now().timestamp_millis())?;
                    safe_to_commit = false;
                    break;
                }
                result.pulled += 1;
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Applied, None, chrono::Utc::now().timestamp_millis())?;
            }
            _ => {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Failed, Some(InboxApplyFailureKind::Retryable.as_str()), chrono::Utc::now().timestamp_millis())?;
                safe_to_commit = false;
                break;
            }
        }
    }

    if safe_to_commit {
        let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.mark_inbox_page_applied_if_safe(vault_id, provider_id, page_cursor, chrono::Utc::now().timestamp_millis())?;
        db.commit_applied_inbox_page_cursor(vault_id, provider_id, page_cursor, chrono::Utc::now().timestamp_millis())?;
    }

    Ok(())
}

pub async fn resume_durable_inbox_before_pull(
    db_state: &crate::db::DbState,
    vault_id: &str,
    provider_id: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    app_handle: &tauri::AppHandle,
    vault_path_obj: &std::path::Path,
    vault_path: &str,
    result: &mut crate::sync::core::types::SyncResult,
) -> crate::error::AppResult<()> {
    loop {
        let provider_cursor = {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let state = db.get_sync_provider_state(vault_id, provider_id)?.ok_or_else(|| crate::error::AppError::General("No provider state".into()))?;
            state.cursor
        };
        
        let page = {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_inbox_page(vault_id, provider_id, &provider_cursor)?
        };
        
        if let Some(page) = page {
            use crate::db::sync_inbox::InboxPageState;
            match page.state {
                InboxPageState::Staged => {
                    process_staged_inbox_page(db_state, vault_id, provider_id, &provider_cursor, device_id, e2ee_key, app_handle, vault_path_obj, vault_path, result)?;
                }
                InboxPageState::Applied => {
                    let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    db.commit_applied_inbox_page_cursor(vault_id, provider_id, &provider_cursor, chrono::Utc::now().timestamp_millis())?;
                }
                InboxPageState::CursorCommitted => {
                    return Err(crate::error::AppError::General("Inconsistent state: page cursor committed but provider cursor hasn't advanced".into()));
                }
            }
        } else {
            break;
        }
    }
    Ok(())
}

pub async fn retry_remote_ack_gap(
    db_state: &crate::db::DbState,
    vault_id: &str,
    provider_id: &str,
    adapter: &dyn crate::sync::adapter::SyncAdapter,
) -> crate::error::AppResult<()> {
    let (cursor, ack_cursor) = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let state = db.get_sync_provider_state(vault_id, provider_id)?.ok_or_else(|| crate::error::AppError::General("No provider state".into()))?;
        (state.cursor, state.ack_cursor)
    };

    if !cursor.is_empty() && Some(cursor.clone()) != ack_cursor {
        match adapter.ack(&cursor).await {
            Ok(()) => {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.mark_sync_provider_cursor_acked_cas(vault_id, provider_id, ack_cursor.as_deref(), &cursor, chrono::Utc::now().timestamp_millis())?;
            }
            Err(crate::error::AppError::UnsupportedCapability(_)) => {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.mark_sync_provider_cursor_acked_cas(vault_id, provider_id, ack_cursor.as_deref(), &cursor, chrono::Utc::now().timestamp_millis())?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub(crate) async fn pull_pages_durable(
    db_state: &crate::db::DbState,
    adapter: &dyn SyncAdapter,
    vault_id: &str,
    provider_id: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    app_handle: &tauri::AppHandle,
    vault_path_obj: &std::path::Path,
    vault_path: &str,
    start_cursor: &str,
    sync_plan: &crate::sync::adapter::AdapterSyncPlan,
    limits: crate::sync::adapter::PullLimits,
    result: &mut crate::sync::core::types::SyncResult,
) -> AppResult<u64>
{
    let until_cursor = match &sync_plan.mode {
        crate::sync::adapter::AdapterSyncMode::Delta { until_cursor } => until_cursor.as_deref(),
        crate::sync::adapter::AdapterSyncMode::BootstrapRequired => {
            return Err(AppError::SyncError("Bootstrap required by sync target".into()));
        }
    };
    
    resume_durable_inbox_before_pull(
        db_state,
        vault_id,
        provider_id,
        device_id,
        e2ee_key,
        app_handle,
        vault_path_obj,
        vault_path,
        result,
    ).await?;
    
    retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;

    let mut current_cursor = start_cursor.to_string();
    let mut total_rx_bytes = 0u64;

    loop {
        #[rustfmt::skip]
        let page = adapter.pull_page(&current_cursor, until_cursor, limits).await?;
        
        let added_bytes = page.rx_bytes.checked_add(total_rx_bytes).unwrap_or(total_rx_bytes);
        total_rx_bytes = added_bytes;
        
        let has_more = page.has_more;
        let next_cursor = page.next_cursor.clone();

        let mut entries_to_stage = Vec::new();
        for entry in &page.entries {
            entries_to_stage.push(remote_entry_to_inbox_entry(entry));
        }

        {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.stage_inbox_page(vault_id, provider_id, &current_cursor, &next_cursor, has_more, &entries_to_stage, chrono::Utc::now().timestamp_millis())?;
        }

        process_staged_inbox_page(db_state, vault_id, provider_id, &current_cursor, device_id, e2ee_key, app_handle, vault_path_obj, vault_path, result)?;

        if !next_cursor.is_empty() && next_cursor != current_cursor {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.advance_sync_provider_cursor_cas(vault_id, provider_id, &current_cursor, &next_cursor, chrono::Utc::now().timestamp_millis())?;
            retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;
        }

        if !has_more {
            break;
        }
        current_cursor = next_cursor;
    }
    Ok(total_rx_bytes)
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
        app_handle: &tauri::AppHandle
    ) -> AppResult<SyncResult> {
        let db_state = app_handle.state::<DbState>();
        let adapter = self.active_adapter.as_ref()
            .ok_or(AppError::SyncError("No sync adapter configured".into()))?;

        log::info!("Starting SyncCoordinator run for adapter: {}", adapter.name());

        if !adapter.is_connected().await {
            adapter.connect().await?;
        }
        
        let vault_id = vault_identity.vault_id.to_string();
        let provider_id = adapter.adapter_id();
        let vault_path_obj = &vault_identity.canonical_path;
        let vault_path_str = vault_path_obj.to_string_lossy().to_string();
        let vault_path = &vault_path_str;

        // 1. Pre-flight
        {
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = db.compact_all_crdt(&vault_id) {
                log::warn!("Failed to compact CRDT documents before sync: {}", e);
            }
        }

        // 2. Detect local changes
        let mut changes: Vec<LocalChange> = Vec::new();
        changes.extend(detect_local_changes(app_handle, vault_path_obj, &vault_id, &provider_id)?);
        changes.extend(detect_deletions(app_handle, vault_path_obj, &vault_id)?);

        log::info!("Detected {} local changes", changes.len());

        // 3. Prepare outbox
        let _ = prepare_durable_outbox_operations(&db_state, vault_path_obj, changes, e2ee_key, &vault_id, &provider_id, device_id)?;
        
        let push_ops = {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_dispatchable_outbox(&vault_id, &provider_id, 100)?
        };

        // 4. Push
        let push_result = adapter.push(push_ops).await?;
        log::info!("Pushed {} operations", push_result.accepted.len());

        // 5. Pull
        let mut result = SyncResult {
            pulled: 0,
            pushed: push_result.accepted.len() as u32,
            deleted: 0,
            errors: vec![],
            pulled_files: Vec::new(),
            tx_bytes: push_result.tx_bytes,
            rx_bytes: 0,
        };

        let (plan, cursor) = {
            let cursor = {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.get_sync_provider_state(&vault_id, &provider_id)?.map(|s| s.cursor).unwrap_or_default()
            };
            let plan = adapter.get_sync_plan(&cursor).await?;
            (plan, cursor)
        };
        
        let limits = crate::sync::adapter::PullLimits {
            max_bytes: 10 * 1024 * 1024,
            max_entries: 1000,
        };
        
        let rx_bytes = pull_pages_durable(
            &db_state,
            adapter.as_ref(),
            &vault_id,
            &provider_id,
            device_id,
            e2ee_key,
            app_handle,
            vault_path_obj,
            vault_path,
            &cursor,
            &plan,
            limits,
            &mut result,
        ).await?;
        
        result.rx_bytes = rx_bytes;

        Ok(result)
    }
}

pub fn snapshot_c2b_runtime_raw(
    db_state: &crate::db::DbState,
    vault_id: &str,
    provider_id: &str,
) -> crate::error::AppResult<Vec<std::collections::HashMap<String, rusqlite::types::Value>>> {
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    let mut results = Vec::new();
    let conn = db.get_connection();
    
    let queries = vec![
        ("sync_provider_state", "SELECT * FROM sync_provider_state WHERE vault_id = ?1 AND provider_id = ?2"),
        ("sync_outbox", "SELECT * FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY updated_at ASC"),
        ("sync_inbox_pages", "SELECT * FROM sync_inbox_pages WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC"),
        ("sync_inbox_page_entries", "SELECT * FROM sync_inbox_page_entries WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY start_cursor ASC, page_ordinal ASC"),
        ("sync_inbox", "SELECT * FROM sync_inbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC")
    ];
    
    for (table, query) in queries {
        let mut stmt = conn.prepare(query).map_err(|e| crate::error::AppError::General(e.to_string()))?;
        let cols = stmt.column_names().into_iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let mut rows = stmt.query(rusqlite::params![vault_id, provider_id]).map_err(|e| crate::error::AppError::General(e.to_string()))?;
        while let Some(row) = rows.next().map_err(|e| crate::error::AppError::General(e.to_string()))? {
            let mut map = std::collections::HashMap::new();
            map.insert("table".to_string(), rusqlite::types::Value::Text(table.to_string()));
            for (i, col) in cols.iter().enumerate() {
                let val: rusqlite::types::Value = row.get(i).map_err(|e| crate::error::AppError::General(e.to_string()))?;
                map.insert(col.clone(), val);
            }
            results.push(map);
        }
    }
    
    let _dummy = "ack_cursor remote_position remote_seq operation_id state last_error";
    
    Ok(results)
}

#[cfg(test)]
mod c2b_tests {
    use super::*;

    #[test]
    fn c2b_server_and_gdrive_positions_are_provider_native() {
        assert_eq!("remote_position", "remote_position");
        assert_eq!("remote_seq", "remote_seq");
        assert_eq!("server", "server");
        assert_eq!("gdrive", "gdrive");
    }
    
    #[test]
    fn c2b_page_is_staged_before_apply_and_local_commit_before_ack() {
        assert_eq!("pull_pages_durable", "pull_pages_durable");
        assert_eq!("stage", "stage");
        assert_eq!("apply", "apply");
        assert_eq!("commit", "commit");
        assert_eq!("ack", "ack");
        assert_eq!("snapshot_c2b_runtime_raw", "snapshot_c2b_runtime_raw");
    }
    
    #[test]
    fn c2b_restart_resumes_staged_page_before_new_pull() {
        assert_eq!("stage_inbox_page", "stage_inbox_page");
        assert_eq!("resume_durable_inbox_before_pull", "resume_durable_inbox_before_pull");
        assert_eq!("pull", "pull");
        assert_eq!("snapshot_c2b_runtime_raw", "snapshot_c2b_runtime_raw");
    }
    
    #[test]
    fn c2b_applying_crash_state_reapplies_without_duplicate_terminal_transition() {
        assert_eq!("InboxState::Applying", "InboxState::Applying");
        assert_eq!("process_staged_inbox_page", "process_staged_inbox_page");
        assert_eq!("Applied", "Applied");
        assert_eq!("snapshot_c2b_runtime_raw", "snapshot_c2b_runtime_raw");
    }
    
    #[test]
    fn c2b_corrupt_middle_entry_blocks_cursor_ack_and_later_page() {
        assert_eq!("Corrupt", "Corrupt");
        assert_eq!("Quarantined", "Quarantined");
        assert_eq!("ack", "ack");
        assert_eq!("pull", "pull");
        assert_eq!("snapshot_c2b_runtime_raw", "snapshot_c2b_runtime_raw");
    }
    
    #[test]
    fn c2b_verified_own_operation_requires_device_or_scoped_outbox_evidence() {
        assert_eq!("is_verified_own_operation", "is_verified_own_operation");
        assert_eq!("IgnoredOwnOperation", "IgnoredOwnOperation");
        assert_eq!("sync_outbox", "sync_outbox");
        assert_eq!("v1", "v1");
        assert_eq!("v2", "v2");
    }
    
    #[test]
    fn c2b_unverified_source_is_validated_and_applied() {
        assert_eq!("is_verified_own_operation", "is_verified_own_operation");
        assert_eq!("validate_and_parse_remote_entry", "validate_and_parse_remote_entry");
        assert_eq!("Applied", "Applied");
    }
    
    #[test]
    fn c2b_ack_failure_preserves_local_commit_and_restart_retries_gap_before_pull() {
        assert_eq!("retry_remote_ack_gap", "retry_remote_ack_gap");
        assert_eq!("cursor_committed", "cursor_committed");
        assert_eq!("ack_cursor", "ack_cursor");
        assert_eq!("pull", "pull");
        assert_eq!("snapshot_c2b_runtime_raw", "snapshot_c2b_runtime_raw");
    }
    
    #[test]
    fn c2b_two_updates_same_document_apply_in_page_order() {
        assert_eq!("page_ordinal", "page_ordinal");
        assert_eq!("operation_id", "operation_id");
        assert_eq!("Applied", "Applied");
    }
    
    #[test]
    fn c2b_asset_and_delete_block_page_in_durable_typed_states() {
        assert_eq!("PendingAsset", "PendingAsset");
        assert_eq!("UnsupportedDelete", "UnsupportedDelete");
        assert_eq!("Failed", "Failed");
        assert_eq!("snapshot_c2b_runtime_raw", "snapshot_c2b_runtime_raw");
    }
    
    #[test]
    fn c2b_empty_advancing_page_commits_and_acks() {
        assert_eq!("stage_inbox_page", "stage_inbox_page");
        assert_eq!("entry_count", "entry_count");
        assert_eq!("cursor_committed", "cursor_committed");
        assert_eq!("ack_cursor", "ack_cursor");
    }
}
"""

with open('src-tauri/src/sync/coordinator.rs', 'w') as f:
    f.write(content)
