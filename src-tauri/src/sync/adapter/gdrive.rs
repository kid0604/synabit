use super::*;
use async_trait::async_trait;
use crate::error::{AppError, AppResult};
use crate::gdrive::auth::get_valid_token;
use crate::gdrive::api::{drive_list_files, drive_upload_file, drive_download_file, find_or_create_vault_folder, drive_create_folder};
use tokio::sync::Mutex;
use tauri::AppHandle;

pub struct GoogleDriveAdapter {
    app_handle: AppHandle,
    log_folder_id: Mutex<Option<String>>,
}

impl GoogleDriveAdapter {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            log_folder_id: Mutex::new(None),
        }
    }

    async fn get_or_create_log_folder(&self) -> AppResult<String> {
        let mut lock = self.log_folder_id.lock().await;
        if let Some(id) = lock.as_ref() {
            return Ok(id.clone());
        }

        let token = get_valid_token(&self.app_handle).await
            .map_err(|e| AppError::General(format!("GDrive auth error: {}", e)))?;
        let client = reqwest::Client::new();

        // 1. Get vault folder
        let vault_id = find_or_create_vault_folder(&client, &token).await
            .map_err(|e| AppError::General(format!("GDrive vault folder error: {}", e)))?;

        // 2. Find or create .sync_log inside vault
        let query = format!("name='.sync_log' and '{}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false", vault_id);
        let url = format!("https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&pageSize=1", urlencoding::encode(&query));
        
        let resp = client.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send().await
            .map_err(|e| AppError::General(format!("GDrive search error: {}", e)))?;

        let mut log_id = None;
        if resp.status().is_success() {
            let list: crate::gdrive::DriveFileList = resp.json().await.unwrap_or(crate::gdrive::DriveFileList { files: None, next_page_token: None });
            if let Some(files) = list.files {
                if let Some(f) = files.first() {
                    log_id = f.id.clone();
                }
            }
        }

        let id = match log_id {
            Some(id) => id,
            None => {
                drive_create_folder(&client, &token, &vault_id, ".sync_log").await
                    .map_err(|e| AppError::General(format!("GDrive create log folder error: {}", e)))?
            }
        };

        *lock = Some(id.clone());
        Ok(id)
    }
}

#[async_trait]
impl SyncAdapter for GoogleDriveAdapter {
    fn name(&self) -> &str {
        "Google Drive"
    }

    fn adapter_id(&self) -> String {
        "gdrive".to_string()
    }

    async fn is_connected(&self) -> bool {
        // Assume connected if we have a valid token
        get_valid_token(&self.app_handle).await.is_ok()
    }

    async fn connect(&self) -> AppResult<()> {
        let _ = self.get_or_create_log_folder().await?;
        Ok(())
    }

    async fn disconnect(&self) -> AppResult<()> {
        Ok(())
    }

    async fn push(&self, operations: Vec<SyncOperation>) -> AppResult<PushResult> {
        let log_folder_id = self.get_or_create_log_folder().await?;
        let token = get_valid_token(&self.app_handle).await
            .map_err(AppError::General)?;
        let client = reqwest::Client::new();

        let mut accepted = Vec::new();
        let mut tx_bytes = 0;
        let mut highest_ts = 0;

        for op in operations {
            // ts_{timestamp}_{hex::encode(operation_id)}.bin
            let name = format!("ts_{:020}_{}.bin", op.timestamp, hex::encode(op.operation_id));
            
            // To be robust, we upload the entire SyncOperation since we need it in pull()
            let blob = match postcard::to_allocvec(&op) {
                Ok(b) => b,
                Err(e) => {
                    log::error!("GDrive failed to serialize operation: {}", e);
                    continue;
                }
            };

            match drive_upload_file(&client, &token, &log_folder_id, &name, &blob).await {
                Ok(_) => {
                    accepted.push(OperationAck {
                        operation_id: op.operation_id,
                        accepted: true,
                        error: None,
                    });
                    tx_bytes += blob.len() as u64;
                    highest_ts = op.timestamp;
                }
                Err(e) => {
                    log::warn!("GDrive upload failed for ts {}: {}", op.timestamp, e);
                    break; // Stop pushing on first failure to maintain order
                }
            }
        }

        Ok(PushResult {
            accepted,
            tx_bytes,
            new_cursor: highest_ts.to_string(), // In GDrive, cursor is the highest timestamp
        })
    }

    async fn pull(&self, since_cursor: &str) -> AppResult<PullResult> {
        let log_folder_id = self.get_or_create_log_folder().await?;
        let token = get_valid_token(&self.app_handle).await
            .map_err(AppError::General)?;
        let client = reqwest::Client::new();

        let start_ts: i64 = since_cursor.parse().unwrap_or(0);
        
        let files = drive_list_files(&client, &token, &log_folder_id).await
            .map_err(AppError::General)?;

        let mut remote_files = Vec::new();

        // Files are named ts_{ts}_{opid}.bin
        for f in files {
            if let Some(name) = &f.name {
                if name.starts_with("ts_") && name.ends_with(".bin") {
                    let stripped = name.strip_prefix("ts_").unwrap().strip_suffix(".bin").unwrap();
                    if let Some(idx) = stripped.find('_') {
                        if let Ok(ts) = stripped[..idx].parse::<i64>() {
                            if ts > start_ts {
                                remote_files.push((ts, f.id.clone().unwrap()));
                            }
                        }
                    }
                }
            }
        }

        remote_files.sort_by_key(|e| e.0);

        let mut entries = Vec::new();
        let mut rx_bytes = 0;
        let mut highest_ts = start_ts;

        for (ts, id) in remote_files {
            match drive_download_file(&client, &token, &id).await {
                Ok(data) => {
                    rx_bytes += data.len() as u64;
                    highest_ts = ts;
                    
                    if let Ok(op) = postcard::from_bytes::<SyncOperation>(&data) {
                         entries.push(RemoteEntry {
                             seq: op.timestamp as u64, // GDrive doesn't use sequence, use timestamp
                             doc_hash: op.doc_hash,
                             source_device: "gdrive".to_string(),
                             encrypted_payload: op.encrypted_payload,
                             payload_hash: op.payload_hash,
                             timestamp: op.timestamp,
                             is_delete: op.is_delete,
                         });
                    }
                }
                Err(e) => {
                    log::warn!("GDrive download failed for ts {}: {}", ts, e);
                    break; // Stop pulling on first failure
                }
            }
        }

        Ok(PullResult {
            entries,
            rx_bytes,
            new_cursor: highest_ts.to_string(),
        })
    }

    async fn ack(&self, _cursor: &str) -> AppResult<()> {
        // GDrive doesn't need explicit ack as it doesn't maintain state
        Ok(())
    }

    async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> {
        Ok(()) // Not implemented yet
    }

    async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
        Ok(None) // Not implemented yet
    }
}
