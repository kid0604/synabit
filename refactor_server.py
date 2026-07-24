import re

with open("src-tauri/src/sync/adapter/server.rs", "r") as f:
    content = f.read()

# Replace SyncTransport with SyncAdapter
content = content.replace("SyncTransport", "SyncAdapter")
content = content.replace("SynabitServerTransport", "SynabitServerAdapter")
content = content.replace("RemoteSyncEntry", "RemoteEntry")
content = content.replace("crate::sync::{RemoteEntry, SyncAdapter};", "crate::sync::core::types::{SyncOperation};\nuse crate::sync::adapter::{SyncAdapter, PushResult, PullResult, RemoteEntry, OperationAck};")
content = content.replace("use crate::sync::{RemoteSyncEntry, SyncTransport};", "use crate::sync::core::types::{SyncOperation};\nuse crate::sync::adapter::{SyncAdapter, PushResult, PullResult, RemoteEntry, OperationAck};")

# Replace impl SyncAdapter block
impl_block = """#[async_trait]
impl SyncAdapter for SynabitServerAdapter {
    fn name(&self) -> &str {
        "Synabit Sync Server"
    }

    fn adapter_id(&self) -> String {
        format!("server:{}", self.device_id)
    }

    async fn is_connected(&self) -> bool {
        self.session.lock().await.is_some()
    }

    async fn connect(&self) -> AppResult<()> {
        self.ensure_session().await
    }

    async fn disconnect(&self) -> AppResult<()> {
        let mut session = self.session.lock().await;
        *session = None;
        Ok(())
    }

    async fn push(&self, operations: Vec<SyncOperation>) -> AppResult<PushResult> {
        if operations.is_empty() {
            return Ok(PushResult {
                accepted: vec![],
                tx_bytes: 0,
                new_cursor: String::new(),
            });
        }
        
        let mut tx_bytes = 0;
        let items: Vec<crate::sync::protocol::PushBatchItem> = operations.into_iter().map(|op| {
            tx_bytes += op.encrypted_payload.len() as u64;
            crate::sync::protocol::PushBatchItem {
                doc_hash: op.doc_hash,
                encrypted_payload: op.encrypted_payload,
                payload_hash: op.payload_hash,
            }
        }).collect();

        let req = MailboxRequest::PushBatch { items };
        let resp = self.request(&req).await?;

        match resp {
            MailboxResponse::PushBatchOk { max_seq } => {
                Ok(PushResult {
                    accepted: vec![], // For now we assume all accepted
                    tx_bytes,
                    new_cursor: max_seq.to_string(),
                })
            }
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(message))
            }
            _ => Err(AppError::SyncError("Unexpected response to PushBatch".into())),
        }
    }

    async fn pull(&self, since_cursor: &str) -> AppResult<PullResult> {
        let since_seq: u64 = since_cursor.parse().unwrap_or(0);
        let req = MailboxRequest::Pull { since_seq };
        let resp = self.request(&req).await?;

        match resp {
            MailboxResponse::PullResult { entries } => {
                let mut rx_bytes = 0;
                let mut max_seq = since_seq;
                
                let remote_entries: Vec<RemoteEntry> = entries.into_iter().map(|e| {
                    rx_bytes += e.encrypted_payload.len() as u64;
                    max_seq = max_seq.max(e.seq);
                    RemoteEntry {
                        seq: e.seq,
                        doc_hash: e.doc_hash,
                        source_device: e.source_device,
                        encrypted_payload: e.encrypted_payload,
                        payload_hash: e.payload_hash,
                        timestamp: e.timestamp,
                        is_delete: e.is_delete,
                    }
                }).collect();

                Ok(PullResult {
                    entries: remote_entries,
                    rx_bytes,
                    new_cursor: max_seq.to_string(),
                })
            }
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to Pull".into())),
        }
    }

    async fn ack(&self, cursor: &str) -> AppResult<()> {
        let up_to_seq: u64 = cursor.parse().unwrap_or(0);
        let req = MailboxRequest::Ack { up_to_seq };
        let resp = self.request(&req).await?;
        match resp {
            MailboxResponse::AckOk => Ok(()),
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to Ack".into())),
        }
    }

    async fn push_asset(&self, hash: [u8; 32], data: Vec<u8>) -> AppResult<()> {
        let req = MailboxRequest::PushAsset {
            asset_hash: hash,
            encrypted_data: data,
        };
        let resp = self.request(&req).await?;
        match resp {
            MailboxResponse::AssetOk => Ok(()),
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to PushAsset".into())),
        }
    }

    async fn pull_asset(&self, hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
        let req = MailboxRequest::PullAsset { asset_hash: hash };
        let resp = self.request(&req).await?;
        match resp {
            MailboxResponse::AssetData { data } => Ok(Some(data)),
            MailboxResponse::AssetNotFound => Ok(None),
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to PullAsset".into())),
        }
    }
}
"""

# Replace the entire old impl SyncTransport with the new impl SyncAdapter
# We need to find `#[async_trait]\nimpl SyncTransport for SynabitServerAdapter {`
# and replace it to the end of the file.

import re
pattern = r"#\[async_trait\]\s*impl SyncAdapter for SynabitServerAdapter \{.*"
content = re.sub(pattern, impl_block, content, flags=re.DOTALL)

with open("src-tauri/src/sync/adapter/server.rs", "w") as f:
    f.write(content)

print("Replaced trait implementation successfully.")
