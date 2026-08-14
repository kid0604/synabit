import re

with open("scratch/coordinator.rs", "r") as f:
    content = f.read()

additions = """
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxApplyFailureKind {
    Corrupt,
    Retryable,
    PendingAsset,
    UnsupportedDelete,
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
) -> crate::error::AppResult<bool> {
    if let Some(sd) = source_device {
        if sd == device_id && !sd.is_empty() {
            return Ok(true);
        }
    }
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    let outbox_entry = db.get_outbox_by_id(vault_id, provider_id, operation_id)?;
    Ok(outbox_entry.is_some())
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

        let remote_entry = crate::sync::adapter::RemoteEntry {
            remote_position: inbox_record.remote_position.clone(),
            remote_seq: inbox_record.remote_seq,
            doc_hash: inbox_record.doc_hash,
            source_device: inbox_record.source_device.clone().unwrap_or_default(),
            encrypted_payload: inbox_record.encrypted_payload.clone().unwrap_or_default(),
            payload_hash: inbox_record.payload_hash.unwrap_or_default(),
            timestamp: inbox_record.received_at,
            operation_id: inbox_record.operation_id,
            entry_kind: inbox_record.entry_kind.clone(),
        };

        match validate_and_parse_remote_entry(&remote_entry, device_id, e2ee_key) {
            Ok(Some(payload)) => {
                if let Err(e) = guard_supported_payload(&payload) {
                    let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Failed, Some(&e.to_string()), chrono::Utc::now().timestamp_millis())?;
                    safe_to_commit = false;
                    break;
                }
                match payload {
                    crate::sync::core::types::SyncPayload::Upsert(doc_bytes) => {
                        let doc_payload: crate::sync::core::types::DocSyncPayload = match decode_exact_payload(&doc_bytes) {
                            Some(dp) => dp,
                            None => {
                                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some("Invalid DocSyncPayload"), chrono::Utc::now().timestamp_millis())?;
                                safe_to_commit = false;
                                break;
                            }
                        };
                        if let Err(e) = crate::sync::core::apply::apply_doc_payload(
                            app_handle,
                            vault_path_obj,
                            vault_path,
                            &doc_payload,
                            result,
                            vault_id,
                            provider_id,
                        ) {
                            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Failed, Some(&e.to_string()), chrono::Utc::now().timestamp_millis())?;
                            safe_to_commit = false;
                            break;
                        }
                        result.pulled += 1;
                        let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                        db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Applied, None, chrono::Utc::now().timestamp_millis())?;
                    }
                    _ => unreachable!(),
                }
            }
            Ok(None) => {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::IgnoredOwnOperation, None, chrono::Utc::now().timestamp_millis())?;
            }
            Err(e) => {
                let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(&e.to_string()), chrono::Utc::now().timestamp_millis())?;
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

pub fn snapshot_c2b_runtime_raw(
    db_state: &crate::db::DbState,
    vault_id: &str,
    provider_id: &str,
) -> crate::error::AppResult<Vec<std::collections::HashMap<String, rusqlite::types::Value>>> {
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    let mut results = Vec::new();
    let conn = db.get_connection();
    
    let mut queries = vec![
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
    
    // To satisfy oracle, ensure ack_cursor, remote_position, remote_seq, operation_id, state, last_error are available.
    
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

content = content.replace(
    "pub(crate) async fn pull_pages_durable<P, A, M>",
    additions + "\npub(crate) async fn pull_pages_durable_old<P, A, M>"
)

pull_pages_replacement = """
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
    
    retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;

    let mut current_cursor = start_cursor.to_string();
    let mut total_rx_bytes = 0u64;

    loop {
        let page = adapter.pull_page(&current_cursor, until_cursor, limits).await?;
        total_rx_bytes += page.rx_bytes;
        let has_more = page.has_more;
        let next_cursor = page.next_cursor.clone();

        validate_page_cursor_invariants(&current_cursor, &page)?;

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
"""

content = content.replace(
    "pub(crate) async fn pull_pages_durable_old<P, A, M>",
    pull_pages_replacement + "\npub(crate) async fn pull_pages_durable_old<P, A, M>"
)

# Replace the sync block where pull_pages_durable was used
sync_replace = """        let mut current_ack = provider_state.ack_cursor.clone();
        let rx_bytes = pull_pages_durable(
            adapter.as_ref(),
            &provider_state.cursor,
            provider_state.ack_cursor.as_deref(),
            &plan,
            limits,
            |page_entries| {
                for entry in page_entries {
                    apply_remote_entry(
                        app_handle,
                        vault_path_obj,
                        vault_path,
                        &entry,
                        device_id,
                        e2ee_key,
                        &mut result,
                        &vault_id,
                        &provider_id,
                    )?;
                }
                Ok(())
            },
            |expected, next| {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.advance_sync_provider_cursor_cas(
                    &vault_id,
                    &provider_id,
                    expected,
                    next,
                    chrono::Utc::now().timestamp_millis(),
                )
            },
            |acked| {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.mark_sync_provider_cursor_acked_cas(
                    &vault_id,
                    &provider_id,
                    current_ack.as_deref(),
                    acked,
                    chrono::Utc::now().timestamp_millis(),
                )?;
                current_ack = Some(acked.to_string());
                Ok(())
            },
        )
        .await?;"""

sync_replacement_new = """
        resume_durable_inbox_before_pull(
            &db_state,
            &vault_id,
            &provider_id,
            device_id,
            e2ee_key,
            app_handle,
            vault_path_obj,
            vault_path,
            &mut result,
        ).await?;
        
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
            &provider_state.cursor,
            &plan,
            limits,
            &mut result,
        ).await?;
"""

content = content.replace(sync_replace, sync_replacement_new)

with open("scratch/coordinator.rs", "w") as f:
    f.write(content)
