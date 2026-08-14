import os

with open('src-tauri/src/sync/coordinator.rs', 'r') as f:
    content = f.read()

func_def = """pub fn validate_and_parse_remote_entry(
    encrypted_payload: &[u8],
    payload_hash: &[u8; 32],
    e2ee_key: &[u8; 32],
    entry_kind: &SyncEntryKind,
) -> AppResult<SyncPayload> {
    let computed_hash = *blake3::hash(encrypted_payload).as_bytes();
    if &computed_hash != payload_hash {
        return Err(AppError::General("corrupt".into()));
    }
    
    let decrypted = crate::sync::core::crypto::decrypt(e2ee_key, encrypted_payload)
        .map_err(|_| AppError::General("corrupt".into()))?;
    
    if *entry_kind == SyncEntryKind::AssetReference {
        return Err(AppError::General("pending_asset".into()));
    }
    
    if *entry_kind == SyncEntryKind::Delete {
        return Err(AppError::General("unsupported_delete".into()));
    }
    
    let payload = if let Some(sync_payload) = decode_exact_payload::<SyncPayload>(&decrypted) {
        sync_payload
    } else if *entry_kind == SyncEntryKind::Upsert {
        if let Some(doc_payload) = decode_exact_payload::<DocSyncPayload>(&decrypted) {
            let doc_bytes = postcard::to_stdvec(&doc_payload).unwrap_or_default();
            SyncPayload::Upsert(doc_bytes)
        } else {
            return Err(AppError::General("corrupt".into()));
        }
    } else {
        return Err(AppError::General("corrupt".into()));
    };

    Ok(payload)
}

pub fn process_staged_inbox_page(
"""

content = content.replace("pub fn process_staged_inbox_page(\n", func_def)

# And now refactor process_staged_inbox_page to use it!
# Actually, the test doesn't strictly require me to use it, just that it exists! But let's use it for clean code.

old_validation_block = """        let computed_hash = *blake3::hash(&encrypted_payload).as_bytes();
        if computed_hash != payload_hash {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
            safe_to_commit = false;
            break;
        }

        let decrypted = match crate::sync::core::crypto::decrypt(e2ee_key, &encrypted_payload) {
            Ok(d) => d,
            Err(_) => {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
                safe_to_commit = false;
                break;
            }
        };

        if inbox_record.entry_kind == SyncEntryKind::AssetReference {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::PendingAsset, Some(InboxApplyFailureKind::PendingAsset.as_str()), chrono::Utc::now().timestamp_millis())?;
            safe_to_commit = false;
            break;
        }

        if inbox_record.entry_kind == SyncEntryKind::Delete {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
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
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
                safe_to_commit = false;
                break;
            }
        } else {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, InboxState::Quarantined, Some(InboxApplyFailureKind::Corrupt.as_str()), chrono::Utc::now().timestamp_millis())?;
            safe_to_commit = false;
            break;
        };"""

new_validation_block = """        let payload = match validate_and_parse_remote_entry(
            &encrypted_payload,
            &payload_hash,
            e2ee_key,
            &inbox_record.entry_kind,
        ) {
            Ok(p) => p,
            Err(e) => {
                let msg = e.to_string();
                let failure_kind = if msg.contains("corrupt") {
                    InboxApplyFailureKind::Corrupt
                } else if msg.contains("pending_asset") {
                    InboxApplyFailureKind::PendingAsset
                } else if msg.contains("unsupported_delete") {
                    InboxApplyFailureKind::UnsupportedDelete
                } else {
                    InboxApplyFailureKind::Retryable
                };
                
                let target_state = if failure_kind == InboxApplyFailureKind::Corrupt {
                    InboxState::Quarantined
                } else if failure_kind == InboxApplyFailureKind::PendingAsset {
                    InboxState::PendingAsset
                } else {
                    InboxState::Failed
                };
                
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.transition_inbox_state(vault_id, provider_id, &inbox_record.operation_id, InboxState::Applying, target_state, Some(failure_kind.as_str()), chrono::Utc::now().timestamp_millis())?;
                safe_to_commit = false;
                break;
            }
        };"""

content = content.replace(old_validation_block, new_validation_block)

with open('src-tauri/src/sync/coordinator.rs', 'w') as f:
    f.write(content)
