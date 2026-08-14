use super::*;
use crate::error::{AppError, AppResult};
use crate::gdrive::api::{drive_create_folder, drive_upload_file, find_or_create_vault_folder};
use crate::gdrive::auth::get_valid_token;
use crate::gdrive::{DriveChange, DriveFile};
use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;

const GDRIVE_CURSOR_PREFIX: &str = "synabit-gdrive-v1:";
const GDRIVE_RAW_PAGE_SCAN_BUDGET: usize = 128;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum GDriveCursorState {
    Initial {
        changes_anchor: String,
        file_page_token: Option<String>,
    },
    Changes {
        page_token: String,
    },
}

pub(crate) fn encode_gdrive_cursor(state: &GDriveCursorState) -> String {
    let json_bytes = serde_json::to_vec(state).expect("serialize gdrive cursor");
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json_bytes);
    format!("{}{}", GDRIVE_CURSOR_PREFIX, b64)
}

pub(crate) fn decode_gdrive_cursor(cursor: &str) -> AppResult<GDriveCursorState> {
    if let Some(encoded) = cursor.strip_prefix(GDRIVE_CURSOR_PREFIX) {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|e| {
                AppError::SyncError(format!("Malformed versioned cursor base64: {}", e))
            })?;
        let state: GDriveCursorState = serde_json::from_slice(&bytes)
            .map_err(|e| AppError::SyncError(format!("Malformed versioned cursor json: {}", e)))?;
        Ok(state)
    } else {
        Err(AppError::SyncError(format!(
            "Unknown cursor prefix: {}",
            cursor
        )))
    }
}

pub(crate) async fn parse_gdrive_cursor_state(
    cursor: &str,
    backend: &dyn GDrivePullBackend,
) -> AppResult<GDriveCursorState> {
    if cursor.is_empty() {
        // Fresh initial scan: capture changes anchor BEFORE first file list
        let anchor = backend.get_start_page_token().await?;
        return Ok(GDriveCursorState::Initial {
            changes_anchor: anchor,
            file_page_token: None,
        });
    }

    if cursor.starts_with(GDRIVE_CURSOR_PREFIX) {
        // Versioned cursor: decode directly. Fail closed if malformed!
        return decode_gdrive_cursor(cursor);
    }

    if cursor.starts_with("fileList_") {
        // Legacy initial cursor: capture new anchor and restart full initial scan
        let anchor = backend.get_start_page_token().await?;
        return Ok(GDriveCursorState::Initial {
            changes_anchor: anchor,
            file_page_token: None,
        });
    }

    // Legacy raw changes token: treat as Changes state
    Ok(GDriveCursorState::Changes {
        page_token: cursor.to_string(),
    })
}

/// Decouples GDrive pull state machine from raw HTTP reqwest calls for testing.
#[async_trait]
pub(crate) trait GDrivePullBackend: Send + Sync {
    async fn list_files_page(
        &self,
        folder_id: &str,
        page_token: Option<&str>,
        page_size: u32,
    ) -> AppResult<(Vec<DriveFile>, Option<String>)>;

    async fn get_start_page_token(&self) -> AppResult<String>;

    async fn list_changes_page(
        &self,
        page_token: &str,
        page_size: u32,
    ) -> AppResult<(Vec<DriveChange>, Option<String>, Option<String>)>;

    async fn download_operation(&self, file_id: &str) -> AppResult<Vec<u8>>;
}

pub(crate) struct ReqwestGDriveBackend {
    app_handle: AppHandle,
}

impl ReqwestGDriveBackend {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

#[async_trait]
impl GDrivePullBackend for ReqwestGDriveBackend {
    async fn list_files_page(
        &self,
        folder_id: &str,
        page_token: Option<&str>,
        page_size: u32,
    ) -> AppResult<(Vec<DriveFile>, Option<String>)> {
        let token = get_valid_token(&self.app_handle)
            .await
            .map_err(AppError::General)?;
        let client = reqwest::Client::new();
        crate::gdrive::api::drive_list_files_page(&client, &token, folder_id, page_token, page_size)
            .await
            .map_err(AppError::General)
    }

    async fn get_start_page_token(&self) -> AppResult<String> {
        let token = get_valid_token(&self.app_handle)
            .await
            .map_err(AppError::General)?;
        let client = reqwest::Client::new();
        crate::gdrive::api::drive_get_start_page_token(&client, &token)
            .await
            .map_err(AppError::General)
    }

    async fn list_changes_page(
        &self,
        page_token: &str,
        page_size: u32,
    ) -> AppResult<(Vec<DriveChange>, Option<String>, Option<String>)> {
        let token = get_valid_token(&self.app_handle)
            .await
            .map_err(AppError::General)?;
        let client = reqwest::Client::new();
        crate::gdrive::api::drive_list_changes_page(&client, &token, page_token, page_size)
            .await
            .map_err(AppError::General)
    }

    async fn download_operation(&self, file_id: &str) -> AppResult<Vec<u8>> {
        let token = get_valid_token(&self.app_handle)
            .await
            .map_err(AppError::General)?;
        let client = reqwest::Client::new();
        crate::gdrive::api::drive_download_file(&client, &token, file_id)
            .await
            .map_err(AppError::General)
    }
}

pub struct GoogleDriveAdapter {
    app_handle: Option<AppHandle>,
    log_folder_id: Mutex<Option<String>>,
    backend: Arc<dyn GDrivePullBackend>,
}

const GDRIVE_OP_MAGIC: &[u8; 4] = b"SYV2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacySyncOperationV1 {
    pub operation_id: [u8; 16],
    pub doc_hash: [u8; 32],
    pub entry_kind: synabit_protocol::SyncEntryKind,
    pub node_id: String,
    pub rel_path: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub is_delete: bool,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacySyncOperationV0 {
    pub operation_id: [u8; 16],
    pub doc_hash: [u8; 32],
    pub node_id: String,
    pub rel_path: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub is_delete: bool,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GDriveOpEnvelope {
    V2(SyncOperation),
}

fn decode_exact<T: serde::de::DeserializeOwned>(data: &[u8]) -> Option<T> {
    if let Ok((val, remainder)) = postcard::take_from_bytes::<T>(data) {
        if remainder.is_empty() {
            return Some(val);
        }
    }
    None
}

pub fn encode_gdrive_operation(op: &SyncOperation) -> AppResult<Vec<u8>> {
    let envelope = GDriveOpEnvelope::V2(op.clone());
    let mut blob = GDRIVE_OP_MAGIC.to_vec();
    let env_bytes = postcard::to_allocvec(&envelope)
        .map_err(|e| AppError::General(format!("Postcard encode error: {}", e)))?;
    blob.extend(env_bytes);
    Ok(blob)
}

pub fn decode_gdrive_operation(data: &[u8]) -> AppResult<SyncOperation> {
    // 1. SYV2 || postcard(GDriveOpEnvelope::V2(SyncOperation))
    if data.starts_with(GDRIVE_OP_MAGIC) {
        let env_bytes = &data[GDRIVE_OP_MAGIC.len()..];
        if let Some(envelope) = decode_exact::<GDriveOpEnvelope>(env_bytes) {
            match envelope {
                GDriveOpEnvelope::V2(op) => return Ok(op),
            }
        }
        return Err(AppError::General(
            "GDrive operation with SYV2 magic failed exact-consumption envelope decoding or contained trailing bytes".into(),
        ));
    }

    // 2. Direct postcard(SyncOperation) layout of A-REPAIR-2 before magic envelope
    if let Some(op) = decode_exact::<SyncOperation>(data) {
        return Ok(op);
    }

    // 3. Legacy V1 postcard(LegacySyncOperationV1) layout with entry_kind and is_delete
    if let Some(v1) = decode_exact::<LegacySyncOperationV1>(data) {
        let entry_kind = if v1.is_delete {
            synabit_protocol::SyncEntryKind::Delete
        } else {
            v1.entry_kind
        };
        return Ok(SyncOperation {
            operation_id: v1.operation_id,
            doc_hash: v1.doc_hash,
            entry_kind,
            node_id: v1.node_id,
            rel_path: v1.rel_path,
            encrypted_payload: v1.encrypted_payload,
            payload_hash: v1.payload_hash,
            timestamp: v1.timestamp,
        });
    }

    // 4. Legacy V0 postcard(LegacySyncOperationV0) layout with only is_delete
    if let Some(v0) = decode_exact::<LegacySyncOperationV0>(data) {
        let entry_kind = if v0.is_delete {
            synabit_protocol::SyncEntryKind::Delete
        } else {
            synabit_protocol::SyncEntryKind::Upsert
        };
        return Ok(SyncOperation {
            operation_id: v0.operation_id,
            doc_hash: v0.doc_hash,
            entry_kind,
            node_id: v0.node_id,
            rel_path: v0.rel_path,
            encrypted_payload: v0.encrypted_payload,
            payload_hash: v0.payload_hash,
            timestamp: v0.timestamp,
        });
    }

    Err(AppError::General(
        "Failed to decode GDrive operation in any supported layout".into(),
    ))
}

impl GoogleDriveAdapter {
    pub fn new(app_handle: AppHandle) -> Self {
        let backend = Arc::new(ReqwestGDriveBackend::new(app_handle.clone()));
        Self {
            app_handle: Some(app_handle),
            log_folder_id: Mutex::new(None),
            backend,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_backend(app_handle: AppHandle, backend: Arc<dyn GDrivePullBackend>) -> Self {
        Self {
            app_handle: Some(app_handle),
            log_folder_id: Mutex::new(None),
            backend,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_testing(backend: Arc<dyn GDrivePullBackend>, log_folder: String) -> Self {
        Self {
            app_handle: None,
            log_folder_id: Mutex::new(Some(log_folder)),
            backend,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_testing_dummy() -> Self {
        struct DummyBackend;
        #[async_trait]
        impl GDrivePullBackend for DummyBackend {
            async fn list_files_page(
                &self,
                _folder_id: &str,
                _page_token: Option<&str>,
                _page_size: u32,
            ) -> AppResult<(Vec<DriveFile>, Option<String>)> {
                Ok((vec![], None))
            }
            async fn get_start_page_token(&self) -> AppResult<String> {
                Ok("start_token".into())
            }
            async fn list_changes_page(
                &self,
                _page_token: &str,
                _page_size: u32,
            ) -> AppResult<(Vec<DriveChange>, Option<String>, Option<String>)> {
                Ok((vec![], None, None))
            }
            async fn download_operation(&self, _file_id: &str) -> AppResult<Vec<u8>> {
                Ok(vec![])
            }
        }
        Self {
            app_handle: None,
            log_folder_id: Mutex::new(None),
            backend: Arc::new(DummyBackend),
        }
    }

    async fn get_or_create_log_folder(&self) -> AppResult<String> {
        let mut lock = self.log_folder_id.lock().await;
        if let Some(id) = lock.as_ref() {
            return Ok(id.clone());
        }

        let app_handle = self
            .app_handle
            .as_ref()
            .ok_or_else(|| AppError::General("No app handle available".into()))?;

        let token = get_valid_token(app_handle)
            .await
            .map_err(|e| AppError::General(format!("GDrive auth error: {}", e)))?;
        let client = reqwest::Client::new();

        // 1. Get vault folder
        let vault_id = find_or_create_vault_folder(&client, &token)
            .await
            .map_err(|e| AppError::General(format!("GDrive vault folder error: {}", e)))?;

        // 2. Find or create .sync_log inside vault
        let query = format!(
            "name='.sync_log' and '{}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false",
            vault_id
        );
        let url = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&pageSize=1",
            urlencoding::encode(&query)
        );

        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| AppError::General(format!("GDrive search error: {}", e)))?;

        let mut log_id = None;
        if resp.status().is_success() {
            let list: crate::gdrive::DriveFileList =
                resp.json().await.unwrap_or(crate::gdrive::DriveFileList {
                    files: None,
                    next_page_token: None,
                });
            if let Some(files) = list.files {
                if let Some(f) = files.first() {
                    log_id = f.id.clone();
                }
            }
        }

        let id = match log_id {
            Some(id) => id,
            None => drive_create_folder(&client, &token, &vault_id, ".sync_log")
                .await
                .map_err(|e| AppError::General(format!("GDrive create log folder error: {}", e)))?,
        };

        *lock = Some(id.clone());
        Ok(id)
    }

    pub async fn pull_bounded(
        &self,
        cursor: &str,
        limits: PullLimits,
    ) -> AppResult<AdapterPullPage> {
        if limits.max_entries == 0 || limits.max_bytes == 0 {
            return Err(AppError::General(
                "Invalid pull limits: max_entries and max_bytes must be > 0".into(),
            ));
        }

        let state = parse_gdrive_cursor_state(cursor, self.backend.as_ref()).await?;

        match state {
            GDriveCursorState::Initial {
                changes_anchor,
                file_page_token,
            } => {
                self.pull_initial_scan(changes_anchor, file_page_token, limits)
                    .await
            }
            GDriveCursorState::Changes { page_token } => {
                self.pull_changes_scan(page_token, limits).await
            }
        }
    }

    async fn pull_initial_scan(
        &self,
        changes_anchor: String,
        file_page_token: Option<String>,
        limits: PullLimits,
    ) -> AppResult<AdapterPullPage> {
        let log_folder_id = self.get_or_create_log_folder().await?;
        let mut current_page_token = file_page_token;

        let mut current_cursor_state = GDriveCursorState::Initial {
            changes_anchor: changes_anchor.clone(),
            file_page_token: current_page_token.clone(),
        };

        let mut entries = Vec::new();
        let mut rx_bytes = 0u64;
        let mut pages_scanned = 0;

        while entries.len() < limits.max_entries as usize
            && pages_scanned < GDRIVE_RAW_PAGE_SCAN_BUDGET
        {
            let (files, next_page_token) = self
                .backend
                .list_files_page(&log_folder_id, current_page_token.as_deref(), 1)
                .await?;

            pages_scanned += 1;

            // Token non-progress protection
            if let (Some(req_tok), Some(next_tok)) = (&current_page_token, &next_page_token) {
                if req_tok == next_tok {
                    return Err(AppError::SyncError(
                        "Token non-progress detected in GDrive initial scan".into(),
                    ));
                }
            }

            if files.is_empty() {
                if let Some(ref npt) = next_page_token {
                    // Empty page with next token is NOT EOF
                    current_cursor_state = GDriveCursorState::Initial {
                        changes_anchor: changes_anchor.clone(),
                        file_page_token: Some(npt.clone()),
                    };
                    current_page_token = next_page_token;

                    if pages_scanned == GDRIVE_RAW_PAGE_SCAN_BUDGET {
                        return Ok(AdapterPullPage {
                            entries,
                            next_cursor: encode_gdrive_cursor(&current_cursor_state),
                            has_more: true,
                            rx_bytes,
                        });
                    }
                    continue;
                } else {
                    // Terminal empty page: initial scan finished → transition to Changes mode
                    let next_state = GDriveCursorState::Changes {
                        page_token: changes_anchor.clone(),
                    };
                    return Ok(AdapterPullPage {
                        entries,
                        next_cursor: encode_gdrive_cursor(&next_state),
                        has_more: false,
                        rx_bytes,
                    });
                }
            }

            let file = &files[0];
            let name = file.name.as_deref().unwrap_or("");
            let is_op = (name.starts_with("ts_") && name.ends_with(".bin"))
                || (name.starts_with("op_") && name.ends_with(".bin"));

            let next_cursor_state_after_file = match &next_page_token {
                Some(npt) => GDriveCursorState::Initial {
                    changes_anchor: changes_anchor.clone(),
                    file_page_token: Some(npt.clone()),
                },
                None => GDriveCursorState::Changes {
                    page_token: changes_anchor.clone(),
                },
            };

            if !is_op {
                // Consume irrelevant file
                current_cursor_state = next_cursor_state_after_file.clone();
                if next_page_token.is_none() {
                    return Ok(AdapterPullPage {
                        entries,
                        next_cursor: encode_gdrive_cursor(&current_cursor_state),
                        has_more: false,
                        rx_bytes,
                    });
                }
                if pages_scanned == GDRIVE_RAW_PAGE_SCAN_BUDGET {
                    return Ok(AdapterPullPage {
                        entries,
                        next_cursor: encode_gdrive_cursor(&current_cursor_state),
                        has_more: true,
                        rx_bytes,
                    });
                }
                current_page_token = next_page_token;
                continue;
            }

            let file_id = file
                .id
                .as_deref()
                .ok_or_else(|| AppError::General("File missing ID".into()))?;
            let data = self.backend.download_operation(file_id).await?;
            let op = decode_gdrive_operation(&data)?;

            let item_bytes = data.len() as u64;
            if entries.is_empty() && item_bytes > limits.max_bytes as u64 {
                return Err(AppError::General(format!(
                    "Oversized item: item size {} exceeds max_bytes {}",
                    item_bytes, limits.max_bytes
                )));
            }

            if rx_bytes + item_bytes > limits.max_bytes as u64 {
                // Stop before unconsumed item
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&current_cursor_state),
                    has_more: true,
                    rx_bytes,
                });
            }

            let entry_kind = op.entry_kind.clone();

            rx_bytes += item_bytes;
            entries.push(RemoteEntry {
                remote_position: file_id.to_string(),
                remote_seq: None,
                doc_hash: op.doc_hash,
                source_device: "gdrive".to_string(),
                encrypted_payload: op.encrypted_payload,
                payload_hash: op.payload_hash,
                timestamp: op.timestamp,
                operation_id: op.operation_id,
                entry_kind,
            });

            current_cursor_state = next_cursor_state_after_file.clone();

            if entries.len() == limits.max_entries as usize {
                let has_more = next_page_token.is_some();
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&current_cursor_state),
                    has_more,
                    rx_bytes,
                });
            }

            if next_page_token.is_none() {
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&current_cursor_state),
                    has_more: false,
                    rx_bytes,
                });
            }

            if pages_scanned == GDRIVE_RAW_PAGE_SCAN_BUDGET {
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&current_cursor_state),
                    has_more: true,
                    rx_bytes,
                });
            }

            current_page_token = next_page_token;
        }

        Ok(AdapterPullPage {
            entries,
            next_cursor: encode_gdrive_cursor(&current_cursor_state),
            has_more: true,
            rx_bytes,
        })
    }

    async fn pull_changes_scan(
        &self,
        page_token: String,
        limits: PullLimits,
    ) -> AppResult<AdapterPullPage> {
        let mut current_raw_token = page_token;
        let mut entries = Vec::new();
        let mut rx_bytes = 0u64;
        let mut pages_scanned = 0;

        while entries.len() < limits.max_entries as usize
            && pages_scanned < GDRIVE_RAW_PAGE_SCAN_BUDGET
        {
            let (changes, next_page_token, new_start_page_token) = self
                .backend
                .list_changes_page(&current_raw_token, 1)
                .await?;

            pages_scanned += 1;

            // Token non-progress protection
            if let Some(ref next_pt) = next_page_token {
                if next_pt == &current_raw_token {
                    return Err(AppError::SyncError(
                        "Token non-progress detected in GDrive changes scan".into(),
                    ));
                }
            }

            let safe_next_raw_token = next_page_token
                .clone()
                .or(new_start_page_token.clone())
                .unwrap_or_else(|| current_raw_token.clone());

            let has_more_remote = next_page_token.is_some();

            if changes.is_empty() {
                current_raw_token = safe_next_raw_token;
                if !has_more_remote {
                    let next_state = GDriveCursorState::Changes {
                        page_token: current_raw_token,
                    };
                    return Ok(AdapterPullPage {
                        entries,
                        next_cursor: encode_gdrive_cursor(&next_state),
                        has_more: false,
                        rx_bytes,
                    });
                }
                if pages_scanned == GDRIVE_RAW_PAGE_SCAN_BUDGET {
                    let next_state = GDriveCursorState::Changes {
                        page_token: current_raw_token,
                    };
                    return Ok(AdapterPullPage {
                        entries,
                        next_cursor: encode_gdrive_cursor(&next_state),
                        has_more: true,
                        rx_bytes,
                    });
                }
                continue;
            }

            let change = &changes[0];
            let is_removed = change.removed.unwrap_or(false);

            let is_relevant_op = if is_removed {
                false
            } else if let Some(ref file) = change.file {
                let name = file.name.as_deref().unwrap_or("");
                (name.starts_with("ts_") && name.ends_with(".bin"))
                    || (name.starts_with("op_") && name.ends_with(".bin"))
            } else {
                false
            };

            if !is_relevant_op {
                // Irrelevant change
                current_raw_token = safe_next_raw_token;
                if !has_more_remote {
                    let next_state = GDriveCursorState::Changes {
                        page_token: current_raw_token,
                    };
                    return Ok(AdapterPullPage {
                        entries,
                        next_cursor: encode_gdrive_cursor(&next_state),
                        has_more: false,
                        rx_bytes,
                    });
                }
                if pages_scanned == GDRIVE_RAW_PAGE_SCAN_BUDGET {
                    let next_state = GDriveCursorState::Changes {
                        page_token: current_raw_token,
                    };
                    return Ok(AdapterPullPage {
                        entries,
                        next_cursor: encode_gdrive_cursor(&next_state),
                        has_more: true,
                        rx_bytes,
                    });
                }
                continue;
            }

            let file_id = change
                .file
                .as_ref()
                .and_then(|f| f.id.as_deref())
                .or(change.file_id.as_deref())
                .ok_or_else(|| AppError::General("Change missing file ID".into()))?;

            let data = self.backend.download_operation(file_id).await?;
            let op = decode_gdrive_operation(&data)?;

            let item_bytes = data.len() as u64;
            if entries.is_empty() && item_bytes > limits.max_bytes as u64 {
                return Err(AppError::General(format!(
                    "Oversized item: item size {} exceeds max_bytes {}",
                    item_bytes, limits.max_bytes
                )));
            }

            if rx_bytes + item_bytes > limits.max_bytes as u64 {
                let next_state = GDriveCursorState::Changes {
                    page_token: current_raw_token,
                };
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&next_state),
                    has_more: true,
                    rx_bytes,
                });
            }

            let entry_kind = op.entry_kind.clone();

            rx_bytes += item_bytes;
            entries.push(RemoteEntry {
                remote_position: file_id.to_string(),
                remote_seq: None,
                doc_hash: op.doc_hash,
                source_device: "gdrive".to_string(),
                encrypted_payload: op.encrypted_payload,
                payload_hash: op.payload_hash,
                timestamp: op.timestamp,
                operation_id: op.operation_id,
                entry_kind,
            });

            current_raw_token = safe_next_raw_token;

            if entries.len() == limits.max_entries as usize {
                let has_more = has_more_remote;
                let next_state = GDriveCursorState::Changes {
                    page_token: current_raw_token,
                };
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&next_state),
                    has_more,
                    rx_bytes,
                });
            }

            if !has_more_remote {
                let next_state = GDriveCursorState::Changes {
                    page_token: current_raw_token,
                };
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&next_state),
                    has_more: false,
                    rx_bytes,
                });
            }

            if pages_scanned == GDRIVE_RAW_PAGE_SCAN_BUDGET {
                let next_state = GDriveCursorState::Changes {
                    page_token: current_raw_token,
                };
                return Ok(AdapterPullPage {
                    entries,
                    next_cursor: encode_gdrive_cursor(&next_state),
                    has_more: true,
                    rx_bytes,
                });
            }
        }

        let next_state = GDriveCursorState::Changes {
            page_token: current_raw_token,
        };
        Ok(AdapterPullPage {
            entries,
            next_cursor: encode_gdrive_cursor(&next_state),
            has_more: true,
            rx_bytes,
        })
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
        if let Some(ref handle) = self.app_handle {
            get_valid_token(handle).await.is_ok()
        } else {
            false
        }
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
        let app_handle = self
            .app_handle
            .as_ref()
            .ok_or_else(|| AppError::General("No app handle available".into()))?;

        let token = get_valid_token(app_handle)
            .await
            .map_err(AppError::General)?;
        let client = reqwest::Client::new();

        let mut accepted = Vec::new();
        let mut tx_bytes = 0;
        let mut highest_ts = 0;

        for op in operations {
            let name = format!(
                "ts_{:020}_{}.bin",
                op.timestamp,
                hex::encode(op.operation_id)
            );
            let blob = match encode_gdrive_operation(&op) {
                Ok(b) => b,
                Err(e) => {
                    log::error!("GDrive failed to serialize operation: {}", e);
                    continue;
                }
            };

            match drive_upload_file(&client, &token, &log_folder_id, &name, &blob).await {
                Ok(_) => {
                    accepted.push(crate::sync::adapter::PushAck {
                        operation_id: op.operation_id,
                        remote_position: highest_ts.to_string(),
                        remote_seq: None,
                    });
                    tx_bytes += blob.len() as u64;
                    highest_ts = op.timestamp;
                }
                Err(e) => {
                    log::warn!("GDrive upload failed for ts {}: {}", op.timestamp, e);
                    break;
                }
            }
        }

        Ok(PushResult {
            accepted,
            rejected: vec![],
            tx_bytes,
        })
    }

    async fn get_sync_plan(
        &self,
        _cursor: &str,
        _client_incarnation_id: Option<[u8; 16]>,
    ) -> AppResult<AdapterSyncPlan> {
        Ok(AdapterSyncPlan {
            mode: AdapterSyncMode::Delta { until_cursor: None },
            incarnation_id: None,
            remote_vault_id: None,
        })
    }

    async fn pull_page(
        &self,
        cursor: &str,
        _until_cursor: Option<&str>,
        limits: PullLimits,
    ) -> AppResult<AdapterPullPage> {
        self.pull_bounded(cursor, limits).await
    }

    async fn ack(&self, _cursor: &str) -> AppResult<()> {
        Err(AppError::UnsupportedCapability(
            "GoogleDriveAdapter does not require or support server-side ACK".into(),
        ))
    }

    async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> {
        Err(AppError::UnsupportedCapability(
            "Asset push not supported on GDrive".into(),
        ))
    }

    async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
        Err(AppError::UnsupportedCapability(
            "Asset pull not supported on GDrive".into(),
        ))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn make_test_op(ts: i64, op_id_byte: u8) -> SyncOperation {
        SyncOperation {
            operation_id: [op_id_byte; 16],
            doc_hash: [op_id_byte; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            node_id: format!("node_{}", op_id_byte),
            rel_path: format!("path_{}.md", op_id_byte),
            encrypted_payload: vec![1, 2, 3, op_id_byte],
            payload_hash: [op_id_byte; 32],
            timestamp: ts,
        }
    }

    pub struct FakeGDriveBackend {
        pub files: std::sync::Mutex<Vec<(String, String, Result<Vec<u8>, String>)>>,
        pub changes: std::sync::Mutex<
            Vec<(
                Option<String>,
                Option<String>,
                Option<bool>,
                Result<Vec<u8>, String>,
            )>,
        >,
        pub start_token: std::sync::Mutex<String>,

        pub empty_files_pages: std::sync::Mutex<std::collections::HashSet<usize>>,
        pub empty_changes_pages: std::sync::Mutex<std::collections::HashSet<usize>>,

        pub forced_file_next_tokens:
            std::sync::Mutex<std::collections::HashMap<Option<String>, Option<String>>>,
        pub forced_change_next_tokens:
            std::sync::Mutex<std::collections::HashMap<String, Option<String>>>,

        pub events: std::sync::Mutex<Vec<String>>,
        pub list_files_calls: std::sync::Mutex<Vec<(Option<String>, u32)>>,
        pub list_changes_calls: std::sync::Mutex<Vec<(String, u32)>>,
        pub download_calls: std::sync::Mutex<Vec<String>>,
    }

    impl FakeGDriveBackend {
        pub fn new() -> Self {
            Self {
                files: std::sync::Mutex::new(Vec::new()),
                changes: std::sync::Mutex::new(Vec::new()),
                start_token: std::sync::Mutex::new("start_token_100".to_string()),
                empty_files_pages: std::sync::Mutex::new(std::collections::HashSet::new()),
                empty_changes_pages: std::sync::Mutex::new(std::collections::HashSet::new()),
                forced_file_next_tokens: std::sync::Mutex::new(std::collections::HashMap::new()),
                forced_change_next_tokens: std::sync::Mutex::new(std::collections::HashMap::new()),
                events: std::sync::Mutex::new(Vec::new()),
                list_files_calls: std::sync::Mutex::new(Vec::new()),
                list_changes_calls: std::sync::Mutex::new(Vec::new()),
                download_calls: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl GDrivePullBackend for FakeGDriveBackend {
        async fn list_files_page(
            &self,
            _folder_id: &str,
            page_token: Option<&str>,
            page_size: u32,
        ) -> AppResult<(Vec<DriveFile>, Option<String>)> {
            self.events
                .lock()
                .unwrap()
                .push(format!("list_files_page:pt={:?}", page_token));
            self.list_files_calls
                .lock()
                .unwrap()
                .push((page_token.map(|s| s.to_string()), page_size));

            if let Some(forced_tok) = self
                .forced_file_next_tokens
                .lock()
                .unwrap()
                .get(&page_token.map(|s| s.to_string()))
            {
                return Ok((vec![], forced_tok.clone()));
            }

            let files_guard = self.files.lock().unwrap();
            let index = match page_token {
                None => 0,
                Some(pt) => {
                    let num: usize = pt
                        .strip_prefix("token_")
                        .unwrap_or("0")
                        .parse()
                        .unwrap_or(0);
                    num
                }
            };

            let empty_set = self.empty_files_pages.lock().unwrap();
            if empty_set.contains(&index) {
                let next_pt = if index + 1 < files_guard.len() {
                    Some(format!("token_{}", index + 1))
                } else {
                    None
                };
                return Ok((vec![], next_pt));
            }

            if index >= files_guard.len() {
                return Ok((vec![], None));
            }

            let (id, name, _) = &files_guard[index];
            let df = DriveFile {
                id: Some(id.clone()),
                name: Some(name.clone()),
            };

            let next_pt = if index + 1 < files_guard.len() {
                Some(format!("token_{}", index + 1))
            } else {
                None
            };

            Ok((vec![df], next_pt))
        }

        async fn get_start_page_token(&self) -> AppResult<String> {
            self.events
                .lock()
                .unwrap()
                .push("get_start_page_token".to_string());
            Ok(self.start_token.lock().unwrap().clone())
        }

        async fn list_changes_page(
            &self,
            page_token: &str,
            page_size: u32,
        ) -> AppResult<(Vec<DriveChange>, Option<String>, Option<String>)> {
            self.events
                .lock()
                .unwrap()
                .push(format!("list_changes_page:pt={}", page_token));
            self.list_changes_calls
                .lock()
                .unwrap()
                .push((page_token.to_string(), page_size));

            if let Some(forced_tok) = self
                .forced_change_next_tokens
                .lock()
                .unwrap()
                .get(page_token)
            {
                return Ok((vec![], forced_tok.clone(), None));
            }

            let changes_guard = self.changes.lock().unwrap();
            let start_tok = self.start_token.lock().unwrap().clone();
            let index = if page_token == start_tok {
                0
            } else {
                let num: usize = page_token
                    .strip_prefix("change_token_")
                    .unwrap_or("0")
                    .parse()
                    .unwrap_or(0);
                num
            };

            let empty_set = self.empty_changes_pages.lock().unwrap();
            if empty_set.contains(&index) {
                let next_pt = if index + 1 < changes_guard.len() {
                    Some(format!("change_token_{}", index + 1))
                } else {
                    None
                };
                return Ok((vec![], next_pt, None));
            }

            if index >= changes_guard.len() {
                return Ok((vec![], None, Some(page_token.to_string())));
            }

            let (fid, fname, rem, _) = &changes_guard[index];
            let dc = DriveChange {
                file_id: fid.clone(),
                file: fname.as_ref().map(|n| DriveFile {
                    id: fid.clone(),
                    name: Some(n.clone()),
                }),
                removed: *rem,
            };

            let next_pt = if index + 1 < changes_guard.len() {
                Some(format!("change_token_{}", index + 1))
            } else {
                None
            };

            let new_start = if next_pt.is_none() {
                Some(format!("change_token_{}", index + 1))
            } else {
                None
            };

            Ok((vec![dc], next_pt, new_start))
        }

        async fn download_operation(&self, file_id: &str) -> AppResult<Vec<u8>> {
            self.download_calls
                .lock()
                .unwrap()
                .push(file_id.to_string());

            let files_guard = self.files.lock().unwrap();
            if let Some(found) = files_guard.iter().find(|f| f.0 == file_id) {
                return match &found.2 {
                    Ok(data) => Ok(data.clone()),
                    Err(e) => Err(AppError::General(e.clone())),
                };
            }

            let changes_guard = self.changes.lock().unwrap();
            if let Some(found) = changes_guard
                .iter()
                .find(|c| c.0.as_deref() == Some(file_id))
            {
                return match &found.3 {
                    Ok(data) => Ok(data.clone()),
                    Err(e) => Err(AppError::General(e.clone())),
                };
            }

            Err(AppError::General(format!(
                "File ID {} not found in fake backend",
                file_id
            )))
        }
    }

    fn create_test_adapter(backend: Arc<dyn GDrivePullBackend>) -> GoogleDriveAdapter {
        GoogleDriveAdapter::for_testing(backend, "test_log_folder_id".to_string())
    }

    #[tokio::test]
    async fn initial_empty_page_with_next_token_does_not_end_scan() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let b1 = postcard::to_allocvec(&op1).unwrap();
        fake.files
            .lock()
            .unwrap()
            .push(("id_0".into(), "ts_00.bin".into(), Ok(vec![])));
        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(b1)));

        fake.empty_files_pages.lock().unwrap().insert(0);

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        assert_eq!(p1.entries.len(), 1);
        assert_eq!(p1.entries[0].timestamp, 1);
        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Changes {
                page_token: "start_token_100".to_string()
            }
        );
    }

    #[tokio::test]
    async fn changes_empty_page_with_next_token_does_not_end_scan() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let b1 = postcard::to_allocvec(&op1).unwrap();
        fake.changes.lock().unwrap().push((
            Some("id_0".into()),
            Some("op_00.bin".into()),
            Some(false),
            Ok(vec![]),
        ));
        fake.changes.lock().unwrap().push((
            Some("id_1".into()),
            Some("op_01.bin".into()),
            Some(false),
            Ok(b1),
        ));

        fake.empty_changes_pages.lock().unwrap().insert(0);

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();
        assert_eq!(p1.entries.len(), 1);
        assert_eq!(p1.entries[0].timestamp, 1);
    }

    #[tokio::test]
    async fn initial_irrelevant_pages_stop_at_raw_scan_budget() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=200 {
            fake.files.lock().unwrap().push((
                format!("id_{}", i),
                format!("irrelevant_{}.txt", i),
                Ok(vec![]),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        assert_eq!(p1.entries.len(), 0);
        assert!(p1.has_more);
        let calls = fake.list_files_calls.lock().unwrap();
        assert_eq!(calls.len(), 128);
        assert_eq!(fake.download_calls.lock().unwrap().len(), 0);

        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Initial {
                changes_anchor: "start_token_100".to_string(),
                file_page_token: Some("token_128".to_string()),
            }
        );
    }

    #[tokio::test]
    async fn changes_irrelevant_pages_stop_at_raw_scan_budget() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=200 {
            fake.changes.lock().unwrap().push((
                Some(format!("id_{}", i)),
                Some(format!("irrelevant_{}.txt", i)),
                Some(false),
                Ok(vec![]),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();
        assert_eq!(p1.entries.len(), 0);
        assert!(p1.has_more);
        let calls = fake.list_changes_calls.lock().unwrap();
        assert_eq!(calls.len(), 128);
        assert_eq!(fake.download_calls.lock().unwrap().len(), 0);

        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Changes {
                page_token: "change_token_128".to_string()
            }
        );
    }

    #[tokio::test]
    async fn initial_resume_after_scan_budget_does_not_skip() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=200 {
            fake.files.lock().unwrap().push((
                format!("id_{}", i),
                format!("irrelevant_{}.txt", i),
                Ok(vec![]),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        fake.list_files_calls.lock().unwrap().clear();

        let _p2 = adapter.pull_bounded(&p1.next_cursor, limits).await.unwrap();
        let calls = fake.list_files_calls.lock().unwrap();
        assert_eq!(calls[0].0, Some("token_128".to_string()));
    }

    #[tokio::test]
    async fn changes_resume_after_scan_budget_does_not_skip() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=200 {
            fake.changes.lock().unwrap().push((
                Some(format!("id_{}", i)),
                Some(format!("irrelevant_{}.txt", i)),
                Some(false),
                Ok(vec![]),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();
        fake.list_changes_calls.lock().unwrap().clear();

        let _p2 = adapter.pull_bounded(&p1.next_cursor, limits).await.unwrap();
        let calls = fake.list_changes_calls.lock().unwrap();
        assert_eq!(calls[0].0, "change_token_128");
    }

    #[tokio::test]
    async fn initial_terminal_page_at_exact_budget_has_more_false() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=128 {
            fake.files.lock().unwrap().push((
                format!("id_{}", i),
                format!("irrelevant_{}.txt", i),
                Ok(vec![]),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        let calls = fake.list_files_calls.lock().unwrap();
        assert_eq!(calls.len(), 128);
        assert!(!p1.has_more);
    }

    #[tokio::test]
    async fn changes_terminal_page_at_exact_budget_has_more_false() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=128 {
            fake.changes.lock().unwrap().push((
                Some(format!("id_{}", i)),
                Some(format!("irrelevant_{}.txt", i)),
                Some(false),
                Ok(vec![]),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();
        let calls = fake.list_changes_calls.lock().unwrap();
        assert_eq!(calls.len(), 128);
        assert!(!p1.has_more);
    }

    #[tokio::test]
    async fn initial_repeated_next_token_is_rejected() {
        let fake = Arc::new(FakeGDriveBackend::new());
        fake.forced_file_next_tokens
            .lock()
            .unwrap()
            .insert(Some("token_1".to_string()), Some("token_1".to_string()));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        // Create initial cursor pointing to token_1
        let state = GDriveCursorState::Initial {
            changes_anchor: "start_token_100".to_string(),
            file_page_token: Some("token_1".to_string()),
        };
        let cursor = encode_gdrive_cursor(&state);

        let res = adapter.pull_bounded(&cursor, limits).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn changes_repeated_next_token_is_rejected() {
        let fake = Arc::new(FakeGDriveBackend::new());
        fake.forced_change_next_tokens.lock().unwrap().insert(
            "start_token_100".to_string(),
            Some("start_token_100".to_string()),
        );

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let res = adapter.pull_bounded("start_token_100", limits).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn initial_scan_captures_changes_anchor_before_first_file_list() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        adapter.pull_bounded("", limits).await.unwrap();

        let events = fake.events.lock().unwrap().clone();
        assert!(events.len() >= 2);
        assert_eq!(events[0], "get_start_page_token");
        assert!(events[1].starts_with("list_files_page"));
    }

    #[tokio::test]
    async fn initial_scan_resume_reuses_original_changes_anchor() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=5 {
            let op = make_test_op(i as i64, i as u8);
            let bytes = postcard::to_allocvec(&op).unwrap();
            fake.files.lock().unwrap().push((
                format!("id_{}", i),
                format!("ts_{:020}_{}.bin", i, i),
                Ok(bytes),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 2,
            max_bytes: 10000,
        };

        // Call 1
        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        let state1 = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        if let GDriveCursorState::Initial {
            changes_anchor,
            file_page_token,
        } = &state1
        {
            assert_eq!(changes_anchor, "start_token_100");
            assert_eq!(file_page_token, &Some("token_2".to_string()));
        } else {
            panic!("Expected Initial state");
        }

        // Change start_token in backend to simulate external changes token advancement
        *fake.start_token.lock().unwrap() = "start_token_999".to_string();

        // Clear events log
        fake.events.lock().unwrap().clear();

        // Call 2 (resume)
        let p2 = adapter.pull_bounded(&p1.next_cursor, limits).await.unwrap();
        let state2 = decode_gdrive_cursor(&p2.next_cursor).unwrap();
        if let GDriveCursorState::Initial {
            changes_anchor,
            file_page_token,
        } = &state2
        {
            assert_eq!(changes_anchor, "start_token_100");
            assert_eq!(file_page_token, &Some("token_4".to_string()));
        } else {
            panic!("Expected Initial state");
        }

        let events = fake.events.lock().unwrap().clone();
        assert!(
            !events.contains(&"get_start_page_token".to_string()),
            "get_start_page_token should NOT be called on resume"
        );
    }

    #[tokio::test]
    async fn initial_scan_completion_transitions_to_original_changes_anchor() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let bytes = postcard::to_allocvec(&op1).unwrap();
        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(bytes)));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        // Change backend start_token after creation
        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        assert!(!p1.has_more);

        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Changes {
                page_token: "start_token_100".to_string()
            }
        );
    }

    #[tokio::test]
    async fn concurrent_change_after_anchor_is_not_skipped() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op_initial = make_test_op(1, 1);
        let bytes_initial = postcard::to_allocvec(&op_initial).unwrap();
        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(bytes_initial)));

        // Add concurrent change to changes list under start_token_100
        let op_concurrent = make_test_op(2, 2);
        let bytes_concurrent = postcard::to_allocvec(&op_concurrent).unwrap();
        fake.changes.lock().unwrap().push((
            Some("id_2".into()),
            Some("ts_02.bin".into()),
            Some(false),
            Ok(bytes_concurrent),
        ));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        // Initial scan
        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        assert_eq!(p1.entries.len(), 1);

        // Transitioned cursor used in changes scan
        let p2 = adapter.pull_bounded(&p1.next_cursor, limits).await.unwrap();
        assert_eq!(p2.entries.len(), 1);
        assert_eq!(p2.entries[0].timestamp, 2);
    }

    #[tokio::test]
    async fn versioned_initial_cursor_preserves_file_page_token() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=3 {
            let op = make_test_op(i as i64, i as u8);
            let bytes = postcard::to_allocvec(&op).unwrap();
            fake.files.lock().unwrap().push((
                format!("id_{}", i),
                format!("ts_{:020}_{}.bin", i, i),
                Ok(bytes),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 1,
            max_bytes: 10000,
        };

        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Initial {
                changes_anchor: "start_token_100".to_string(),
                file_page_token: Some("token_1".to_string()),
            }
        );
    }

    #[tokio::test]
    async fn legacy_filelist_cursor_restarts_safe_initial_scan() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter
            .pull_bounded("fileList_token_5", limits)
            .await
            .unwrap();

        let events = fake.events.lock().unwrap().clone();
        assert_eq!(events[0], "get_start_page_token");
        assert_eq!(events[1], "list_files_page:pt=None"); // Restarts from None

        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert!(matches!(state, GDriveCursorState::Changes { .. }));
    }

    #[tokio::test]
    async fn legacy_raw_changes_cursor_is_accepted_and_upgraded() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();

        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Changes {
                page_token: "start_token_100".to_string()
            }
        );
    }

    #[tokio::test]
    async fn malformed_versioned_cursor_is_rejected() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let res = adapter
            .pull_bounded("synabit-gdrive-v1:invalid_base64!", limits)
            .await;
        assert!(res.is_err());

        let events = fake.events.lock().unwrap().clone();
        assert!(
            events.is_empty(),
            "APIs should not be called on malformed cursor"
        );
    }

    #[tokio::test]
    async fn initial_scan_resumes_without_skipping_when_entry_limit_hit() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=100 {
            let op = make_test_op(i as i64, i as u8);
            let bytes = postcard::to_allocvec(&op).unwrap();
            fake.files.lock().unwrap().push((
                format!("id_{}", i),
                format!("ts_{:020}_{}.bin", i, i),
                Ok(bytes),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 50,
            max_bytes: 10 * 1024 * 1024,
        };

        // Call 1
        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        assert_eq!(p1.entries.len(), 50);
        assert!(p1.has_more);

        let state1 = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state1,
            GDriveCursorState::Initial {
                changes_anchor: "start_token_100".to_string(),
                file_page_token: Some("token_50".to_string()),
            }
        );
        assert_eq!(p1.entries[0].timestamp, 1);
        assert_eq!(p1.entries[49].timestamp, 50);

        // Call 2
        let p2 = adapter.pull_bounded(&p1.next_cursor, limits).await.unwrap();
        assert_eq!(p2.entries.len(), 50);
        assert!(!p2.has_more);

        let state2 = decode_gdrive_cursor(&p2.next_cursor).unwrap();
        assert_eq!(
            state2,
            GDriveCursorState::Changes {
                page_token: "start_token_100".to_string()
            }
        );
        assert_eq!(p2.entries[0].timestamp, 51);
        assert_eq!(p2.entries[49].timestamp, 100);

        let calls = fake.list_files_calls.lock().unwrap();
        assert_eq!(calls.len(), 100);
        for c in calls.iter() {
            assert_eq!(c.1, 1);
        }
    }

    #[tokio::test]
    async fn changes_scan_resumes_without_skipping_when_entry_limit_hit() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=100 {
            let op = make_test_op(i as i64, i as u8);
            let bytes = postcard::to_allocvec(&op).unwrap();
            fake.changes.lock().unwrap().push((
                Some(format!("id_{}", i)),
                Some(format!("op_{:020}_{}.bin", i, i)),
                Some(false),
                Ok(bytes),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 50,
            max_bytes: 10 * 1024 * 1024,
        };

        let p1 = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();
        assert_eq!(p1.entries.len(), 50);
        assert!(p1.has_more);

        let state1 = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state1,
            GDriveCursorState::Changes {
                page_token: "change_token_50".to_string()
            }
        );

        let p2 = adapter.pull_bounded(&p1.next_cursor, limits).await.unwrap();
        assert_eq!(p2.entries.len(), 50);
        assert!(!p2.has_more);
        assert_eq!(p2.entries[0].timestamp, 51);
        assert_eq!(p2.entries[49].timestamp, 100);

        let calls = fake.list_changes_calls.lock().unwrap();
        assert_eq!(calls.len(), 100);
        for c in calls.iter() {
            assert_eq!(c.1, 1);
        }
    }

    #[tokio::test]
    async fn byte_limit_stops_before_unconsumed_item() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let b1 = postcard::to_allocvec(&op1).unwrap();
        let s1 = b1.len() as u32;

        let op2 = make_test_op(2, 2);
        let b2 = postcard::to_allocvec(&op2).unwrap();

        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(b1)));
        fake.files
            .lock()
            .unwrap()
            .push(("id_2".into(), "ts_02.bin".into(), Ok(b2)));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: s1 + 10,
        };

        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        assert_eq!(p1.entries.len(), 1);
        assert!(p1.has_more);
        assert_eq!(p1.entries[0].timestamp, 1);

        let state1 = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state1,
            GDriveCursorState::Initial {
                changes_anchor: "start_token_100".to_string(),
                file_page_token: Some("token_1".to_string()),
            }
        );

        let limits2 = PullLimits {
            max_entries: 10,
            max_bytes: 1000,
        };
        let p2 = adapter
            .pull_bounded(&p1.next_cursor, limits2)
            .await
            .unwrap();
        assert_eq!(p2.entries.len(), 1);
        assert_eq!(p2.entries[0].timestamp, 2);
    }

    #[tokio::test]
    async fn first_oversized_item_does_not_advance_cursor() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let b1 = postcard::to_allocvec(&op1).unwrap();
        let s1 = b1.len() as u32;

        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(b1)));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: s1 - 1,
        };

        let res = adapter.pull_bounded("", limits).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn download_failure_does_not_advance_cursor() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let b1 = postcard::to_allocvec(&op1).unwrap();

        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(b1)));
        fake.files.lock().unwrap().push((
            "id_2".into(),
            "ts_02.bin".into(),
            Err("Network error downloading file id_2".into()),
        ));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let res = adapter.pull_bounded("", limits).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn deserialize_failure_does_not_advance_cursor() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let b1 = postcard::to_allocvec(&op1).unwrap();

        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(b1)));
        fake.files.lock().unwrap().push((
            "id_2".into(),
            "ts_02.bin".into(),
            Ok(vec![255, 255, 255]),
        ));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let res = adapter.pull_bounded("", limits).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn initial_scan_transitions_to_changes_token() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let op1 = make_test_op(1, 1);
        let b1 = postcard::to_allocvec(&op1).unwrap();

        fake.files
            .lock()
            .unwrap()
            .push(("id_1".into(), "ts_01.bin".into(), Ok(b1)));

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter.pull_bounded("", limits).await.unwrap();
        assert_eq!(p1.entries.len(), 1);
        assert!(!p1.has_more);

        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Changes {
                page_token: "start_token_100".to_string()
            }
        );
    }

    #[tokio::test]
    async fn irrelevant_changes_advance_cursor_with_bounded_scan_work() {
        let fake = Arc::new(FakeGDriveBackend::new());
        for i in 1..=5 {
            fake.changes.lock().unwrap().push((
                Some(format!("id_{}", i)),
                Some(format!("irrelevant_{}.txt", i)),
                Some(false),
                Ok(vec![]),
            ));
        }

        let adapter = create_test_adapter(fake.clone());
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let p1 = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();
        assert_eq!(p1.entries.len(), 0);
        assert!(!p1.has_more);

        let state = decode_gdrive_cursor(&p1.next_cursor).unwrap();
        assert_eq!(
            state,
            GDriveCursorState::Changes {
                page_token: "change_token_5".to_string()
            }
        );
    }

    #[tokio::test]
    async fn zero_entry_limit_is_rejected() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let adapter = create_test_adapter(fake);
        let limits = PullLimits {
            max_entries: 0,
            max_bytes: 10000,
        };
        let res = adapter.pull_bounded("", limits).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn zero_byte_limit_is_rejected() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let adapter = create_test_adapter(fake);
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 0,
        };
        let res = adapter.pull_bounded("", limits).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn gdrive_sync_plan_has_no_numeric_cursor_requirement() {
        let fake = Arc::new(FakeGDriveBackend::new());
        let adapter = create_test_adapter(fake);
        let plan = adapter
            .get_sync_plan("fileList_non_numeric_token", None)
            .await
            .unwrap();
        match plan.mode {
            AdapterSyncMode::Delta { until_cursor } => {
                assert_eq!(until_cursor, None);
            }
            _ => panic!("Expected Delta mode"),
        }
    }

    #[tokio::test]
    async fn gdrive_initial_scan_preserves_operation_id_and_typed_kind() {
        let fake = Arc::new(FakeGDriveBackend::new());

        let op_upsert = make_test_op(100, 11);
        let mut op_delete = make_test_op(200, 22);
        op_delete.entry_kind = synabit_protocol::SyncEntryKind::Delete;
        let mut op_asset = make_test_op(300, 33);
        op_asset.entry_kind = synabit_protocol::SyncEntryKind::AssetReference;

        fake.files.lock().unwrap().push((
            "id_1".into(),
            "ts_100.bin".into(),
            Ok(encode_gdrive_operation(&op_upsert).unwrap()),
        ));
        fake.files.lock().unwrap().push((
            "id_2".into(),
            "ts_200.bin".into(),
            Ok(encode_gdrive_operation(&op_delete).unwrap()),
        ));
        fake.files.lock().unwrap().push((
            "id_3".into(),
            "ts_300.bin".into(),
            Ok(encode_gdrive_operation(&op_asset).unwrap()),
        ));

        let adapter = create_test_adapter(fake);
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let page = adapter.pull_bounded("", limits).await.unwrap();
        assert_eq!(page.entries.len(), 3);

        assert_eq!(page.entries[0].operation_id, [11; 16]);
        assert_eq!(
            page.entries[0].entry_kind,
            synabit_protocol::SyncEntryKind::Upsert
        );

        assert_eq!(page.entries[1].operation_id, [22; 16]);
        assert_eq!(
            page.entries[1].entry_kind,
            synabit_protocol::SyncEntryKind::Delete
        );

        assert_eq!(page.entries[2].operation_id, [33; 16]);
        assert_eq!(
            page.entries[2].entry_kind,
            synabit_protocol::SyncEntryKind::AssetReference
        );
    }

    #[tokio::test]
    async fn gdrive_changes_scan_preserves_operation_id_and_typed_kind() {
        let fake = Arc::new(FakeGDriveBackend::new());

        let op_upsert = make_test_op(300, 33);
        let mut op_delete = make_test_op(400, 44);
        op_delete.entry_kind = synabit_protocol::SyncEntryKind::Delete;
        let mut op_asset = make_test_op(500, 55);
        op_asset.entry_kind = synabit_protocol::SyncEntryKind::AssetReference;

        fake.files.lock().unwrap().push((
            "id_c1".into(),
            "op_300.bin".into(),
            Ok(encode_gdrive_operation(&op_upsert).unwrap()),
        ));
        fake.files.lock().unwrap().push((
            "id_c2".into(),
            "op_400.bin".into(),
            Ok(encode_gdrive_operation(&op_delete).unwrap()),
        ));
        fake.files.lock().unwrap().push((
            "id_c3".into(),
            "op_500.bin".into(),
            Ok(encode_gdrive_operation(&op_asset).unwrap()),
        ));

        fake.changes.lock().unwrap().push((
            Some("id_c1".into()),
            Some("op_300.bin".into()),
            Some(false),
            Ok(encode_gdrive_operation(&op_upsert).unwrap()),
        ));
        fake.changes.lock().unwrap().push((
            Some("id_c2".into()),
            Some("op_400.bin".into()),
            Some(false),
            Ok(encode_gdrive_operation(&op_delete).unwrap()),
        ));
        fake.changes.lock().unwrap().push((
            Some("id_c3".into()),
            Some("op_500.bin".into()),
            Some(false),
            Ok(encode_gdrive_operation(&op_asset).unwrap()),
        ));

        let adapter = create_test_adapter(fake);
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 10000,
        };

        let page = adapter
            .pull_bounded("start_token_100", limits)
            .await
            .unwrap();
        assert_eq!(page.entries.len(), 3);

        assert_eq!(page.entries[0].operation_id, [33; 16]);
        assert_eq!(
            page.entries[0].entry_kind,
            synabit_protocol::SyncEntryKind::Upsert
        );

        assert_eq!(page.entries[1].operation_id, [44; 16]);
        assert_eq!(
            page.entries[1].entry_kind,
            synabit_protocol::SyncEntryKind::Delete
        );

        assert_eq!(page.entries[2].operation_id, [55; 16]);
        assert_eq!(
            page.entries[2].entry_kind,
            synabit_protocol::SyncEntryKind::AssetReference
        );
    }

    #[tokio::test]
    async fn gdrive_operation_codec_supports_frozen_historical_layouts() {
        // Layout 1: SYV2 || postcard(GDriveOpEnvelope::V2(SyncOperation))
        let op = make_test_op(100, 99);
        let encoded_v2 = encode_gdrive_operation(&op).unwrap();
        assert!(encoded_v2.starts_with(b"SYV2"));
        let decoded_v2 = decode_gdrive_operation(&encoded_v2).unwrap();
        assert_eq!(decoded_v2.operation_id, op.operation_id);
        assert_eq!(decoded_v2.entry_kind, op.entry_kind);

        // Verify trailing bytes on SYV2 magic blob fail closed
        let mut trailing_v2 = encoded_v2.clone();
        trailing_v2.extend_from_slice(b"GARBAGE_TRAILING_BYTES");
        assert!(decode_gdrive_operation(&trailing_v2).is_err());

        // Verify malformed payload after SYV2 magic fails closed without fallback
        let mut malformed_v2 = b"SYV2".to_vec();
        malformed_v2.extend_from_slice(b"CORRUPTED_ENVELOPE_DATA");
        assert!(decode_gdrive_operation(&malformed_v2).is_err());

        // Layout 2: Direct postcard(SyncOperation) layout of A-REPAIR-2
        let direct_bytes = postcard::to_allocvec(&op).unwrap();
        let decoded_direct = decode_gdrive_operation(&direct_bytes).unwrap();
        assert_eq!(decoded_direct.operation_id, op.operation_id);
        assert_eq!(decoded_direct.entry_kind, op.entry_kind);

        let mut trailing_direct = direct_bytes.clone();
        trailing_direct.extend_from_slice(b"EXTRA");
        assert!(decode_gdrive_operation(&trailing_direct).is_err());

        let malformed_direct = direct_bytes[..direct_bytes.len() - 5].to_vec();
        assert!(decode_gdrive_operation(&malformed_direct).is_err());

        // Layout 3: postcard(LegacySyncOperationV1) layout with entry_kind and is_delete
        let legacy_v1 = LegacySyncOperationV1 {
            operation_id: [77; 16],
            doc_hash: [88; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            node_id: "node_legacy".into(),
            rel_path: "legacy.md".into(),
            encrypted_payload: vec![1, 2, 3],
            payload_hash: [123; 32],
            is_delete: true,
            timestamp: 555,
        };
        let legacy_v1_bytes = postcard::to_allocvec(&legacy_v1).unwrap();
        let decoded_v1 = decode_gdrive_operation(&legacy_v1_bytes).unwrap();
        assert_eq!(decoded_v1.operation_id, [77; 16]);
        assert_eq!(
            decoded_v1.entry_kind,
            synabit_protocol::SyncEntryKind::Delete
        );
        assert_eq!(decoded_v1.timestamp, 555);

        let mut trailing_v1 = legacy_v1_bytes.clone();
        trailing_v1.extend_from_slice(b"GARBAGE");
        assert!(decode_gdrive_operation(&trailing_v1).is_err());

        let malformed_v1 = legacy_v1_bytes[..legacy_v1_bytes.len() - 5].to_vec();
        assert!(decode_gdrive_operation(&malformed_v1).is_err());

        // Layout 4: postcard(LegacySyncOperationV0) layout with only is_delete
        let legacy_v0_delete = LegacySyncOperationV0 {
            operation_id: [88; 16],
            doc_hash: [99; 32],
            node_id: "node_v0".into(),
            rel_path: "v0.md".into(),
            encrypted_payload: vec![4, 5, 6],
            payload_hash: [222; 32],
            is_delete: true,
            timestamp: 666,
        };
        let v0_del_bytes = postcard::to_allocvec(&legacy_v0_delete).unwrap();
        let decoded_v0_del = decode_gdrive_operation(&v0_del_bytes).unwrap();
        assert_eq!(decoded_v0_del.operation_id, [88; 16]);
        assert_eq!(
            decoded_v0_del.entry_kind,
            synabit_protocol::SyncEntryKind::Delete
        );

        let legacy_v0_upsert = LegacySyncOperationV0 {
            operation_id: [89; 16],
            doc_hash: [99; 32],
            node_id: "node_v0".into(),
            rel_path: "v0.md".into(),
            encrypted_payload: vec![4, 5, 6],
            payload_hash: [222; 32],
            is_delete: false,
            timestamp: 667,
        };
        let v0_up_bytes = postcard::to_allocvec(&legacy_v0_upsert).unwrap();
        let decoded_v0_up = decode_gdrive_operation(&v0_up_bytes).unwrap();
        assert_eq!(decoded_v0_up.operation_id, [89; 16]);
        assert_eq!(
            decoded_v0_up.entry_kind,
            synabit_protocol::SyncEntryKind::Upsert
        );

        let mut trailing_v0 = v0_up_bytes.clone();
        trailing_v0.extend_from_slice(b"GARBAGE");
        assert!(decode_gdrive_operation(&trailing_v0).is_err());

        let malformed_v0 = v0_up_bytes[..v0_up_bytes.len() - 5].to_vec();
        assert!(decode_gdrive_operation(&malformed_v0).is_err());
    }

    #[tokio::test]
    async fn gdrive_ack_returns_unsupported_capability() {
        let adapter = GoogleDriveAdapter::for_testing_dummy();
        let err = adapter.ack("some_cursor").await.unwrap_err();
        match err {
            AppError::UnsupportedCapability(msg) => {
                assert!(
                    msg.contains("GoogleDriveAdapter does not require or support server-side ACK")
                );
            }
            _ => panic!("Expected UnsupportedCapability error for GDrive ack"),
        }
    }
}
