use std::path::Path;
use std::fs;
use crate::error::AppResult;
use crate::db::DbState;
use tauri::Manager;
use crate::sync::core::types::SyncOperation;
use crate::sync::utils::{collect_local_files, file_sha256};
use crate::sync::core::crdt::apply_text_update;

pub struct LocalChange {
    pub rel_path: String,
    pub is_delete: bool,
    pub new_hash: String,
}

pub fn detect_local_changes(app_handle: &tauri::AppHandle, vault: &Path) -> AppResult<Vec<LocalChange>> {
    let mut changes = Vec::new();
    let local_files = collect_local_files(&vault.to_string_lossy());
    
    for rel_path in local_files {
        let file_path = vault.join(&rel_path);
        let current_hash = file_sha256(&file_path);
        
        let stored_hash = {
            let db_state = app_handle.state::<DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_kv(&format!("sync_hash_{}", rel_path))?.unwrap_or_default()
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

pub fn detect_deletions(app_handle: &tauri::AppHandle, vault: &Path) -> AppResult<Vec<LocalChange>> {
    let mut deletions = Vec::new();
    
    let paths = {
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_all_document_paths().unwrap_or_default()
    };
    
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
    
    Ok(deletions)
}

pub fn prepare_push_operations(
    app_handle: &tauri::AppHandle,
    vault: &Path,
    changes: Vec<LocalChange>,
    e2ee_key: &[u8; 32],
    _device_id: &str, // Deprecated, unused for node_id assignment
) -> AppResult<Vec<SyncOperation>> {
    let mut ops = Vec::new();
    
    for change in changes {
        let doc_hash = *blake3::hash(change.rel_path.as_bytes()).as_bytes();
        let timestamp = chrono::Utc::now().timestamp_millis();
        
        let actual_node_id = if change.is_delete {
            let db_state = app_handle.state::<DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            match db.get_node_id_by_path(&change.rel_path) {
                Ok(Some(id)) => id,
                _ => {
                    log::warn!("Cannot find node_id for deleted file {}", change.rel_path);
                    continue;
                }
            }
        } else {
            let file_path = vault.join(&change.rel_path);
            match crate::sync::core::identity::get_or_assign_node_id(vault, &file_path) {
                Ok(id) => {
                    let db_state = app_handle.state::<DbState>();
                    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = db.upsert_document_path(&id, &change.rel_path);
                    id
                }
                Err(e) => {
                    log::warn!("Failed to get node_id for {}: {}", change.rel_path, e);
                    continue;
                }
            }
        };
        
        if change.is_delete {
            ops.push(SyncOperation {
                operation_id: uuid::Uuid::new_v4().into_bytes(),
                doc_hash,
                node_id: actual_node_id.clone(),
                rel_path: change.rel_path,
                encrypted_payload: vec![],
                payload_hash: [0; 32],
                is_delete: true,
                timestamp,
            });
            continue;
        }
        
        let file_path = vault.join(&change.rel_path);
        if let Ok(content) = fs::read(&file_path) {
            let mut doc = {
                let db_state = app_handle.state::<DbState>();
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.get_crdt_doc(&actual_node_id)?
            };
            
            // Assume text/markdown for now
            if let Ok(text) = String::from_utf8(content) {
                let _delta = apply_text_update(&doc, &text)
                    .map_err(crate::error::AppError::General)?;
                
                if !_delta.is_empty() {
                    let db_state = app_handle.state::<DbState>();
                    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = db.save_crdt_delta(&actual_node_id, _delta);
                }

                let snapshot = doc.export_snapshot();
                
                let is_json = change.rel_path.ends_with(".json") || change.rel_path.ends_with(".canvas");
                let payload = crate::sync::core::types::DocSyncPayload {
                    node_id: actual_node_id.clone(),
                    rel_path: change.rel_path.clone(),
                    snapshot,
                    is_json,
                };
                
                let payload_bytes = postcard::to_stdvec(&payload)
                    .map_err(|e| crate::error::AppError::General(format!("Postcard error: {}", e)))?;
                
                // Set compress to true for encrypt_v5
                let encrypted_payload = crate::sync::core::crypto::encrypt_v5(e2ee_key, &payload_bytes, true)
                    .map_err(|e| crate::error::AppError::General(format!("Encryption error: {:?}", e)))?;
                    
                let payload_hash = *blake3::hash(&encrypted_payload).as_bytes();
                
                ops.push(SyncOperation {
                    operation_id: uuid::Uuid::new_v4().into_bytes(),
                    doc_hash,
                    node_id: actual_node_id,
                    rel_path: change.rel_path.clone(),
                    encrypted_payload,
                    payload_hash,
                    is_delete: false,
                    timestamp,
                });
                
                let db_state = app_handle.state::<DbState>();
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.set_kv(&format!("sync_hash_{}", change.rel_path), &change.new_hash)?;
            }
        }
    }
    
    Ok(ops)
}
