use std::sync::Arc;
use std::path::Path;
use crate::error::{AppError, AppResult};
use crate::db::DbState;
use crate::sync::adapter::SyncAdapter;
use crate::sync::core::types::{SyncResult, SyncRunContext};
use tauri::Manager;
use crate::sync::core::change::{detect_local_changes, detect_deletions, prepare_push_operations, LocalChange};

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
&self, vault_path: &str, device_id: &str, e2ee_key: &[u8; 32], _ctx: &SyncRunContext, app_handle: &tauri::AppHandle) -> AppResult<SyncResult> {
        let db_state = app_handle.state::<DbState>();
        let adapter = self.active_adapter.as_ref()
            .ok_or(AppError::SyncError("No sync adapter configured".into()))?;

        log::info!("Starting SyncCoordinator run for adapter: {}", adapter.name());

        if !adapter.is_connected().await {
            adapter.connect().await?;
        }

        // 1. Pre-flight
        {
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            if let Err(e) = db.compact_all_crdt() {
                log::warn!("Failed to compact CRDT documents before sync: {}", e);
            }
        }

        let vault_path_obj = Path::new(vault_path);

        // 2. Detect local changes
        let mut changes: Vec<LocalChange> = Vec::new();
        changes.extend(detect_local_changes(app_handle, vault_path_obj)?);
        changes.extend(detect_deletions(app_handle, vault_path_obj)?);

        log::info!("Detected {} local changes", changes.len());

        let push_ops = prepare_push_operations(app_handle, vault_path_obj, changes, e2ee_key, device_id)?;

        // 3. Push
        let push_result = adapter.push(push_ops).await?;
        log::info!("Pushed {} operations", push_result.accepted.len());

        // 4. Pull
        let cursor_key = format!("sync_cursor_{}", adapter.adapter_id());
        let current_cursor = {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_kv(&cursor_key)?.unwrap_or_default()
        };

        let pull_result = adapter.pull(&current_cursor).await?;
        log::info!("Pulled {} operations", pull_result.entries.len());

        let mut result = SyncResult {
            pulled: 0,
            pushed: push_result.accepted.len() as u32,
            deleted: 0,
            errors: vec![],
            pulled_files: Vec::new(),
            tx_bytes: push_result.tx_bytes,
            rx_bytes: pull_result.rx_bytes,
        };

        // 5. Apply (Merge Remote)
        for entry in pull_result.entries {
            if entry.source_device == device_id {
                continue; // Skip own pushes
            }

            let computed_hash = *blake3::hash(&entry.encrypted_payload).as_bytes();
            if computed_hash != entry.payload_hash {
                log::warn!("Payload hash mismatch for seq {}, skipping", entry.seq);
                continue;
            }

            let decrypted = crate::sync::core::crypto::decrypt(e2ee_key, &entry.encrypted_payload)
                .map_err(|e| AppError::General(format!("Decryption failed: {:?}", e)))?;

            // Parse the DocSyncPayload
            let payload: crate::sync::core::types::DocSyncPayload = match postcard::from_bytes(&decrypted) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("PULL: deserialize failed for seq {}: {}", entry.seq, e);
                    continue;
                }
            };

            // Merge the payload
            crate::sync::core::apply::apply_doc_payload(app_handle, vault_path_obj, vault_path, &payload, &mut result);
        }

        // Save new cursor
        if !pull_result.new_cursor.is_empty() && pull_result.new_cursor != current_cursor {
            {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                db.set_kv(&cursor_key, &pull_result.new_cursor)?;
            }
            let _ = adapter.ack(&pull_result.new_cursor).await;
        }

        Ok(result)
    }
}
