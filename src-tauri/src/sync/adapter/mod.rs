use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::error::AppResult;
use crate::sync::core::types::SyncOperation;

pub mod server;
pub mod gdrive;

/// Core adapter trait — mọi sync target phải implement
#[async_trait]
pub trait SyncAdapter: Send + Sync {
    /// Tên hiển thị
    fn name(&self) -> &str;

    /// ID duy nhất cho adapter instance (dùng cho cursor tracking)
    fn adapter_id(&self) -> String;

    /// Kiểm tra kết nối
    async fn is_connected(&self) -> bool;

    /// Kết nối / authenticate
    async fn connect(&self) -> AppResult<()>;

    /// Ngắt kết nối
    async fn disconnect(&self) -> AppResult<()>;

    /// Push các operations lên remote
    async fn push(&self, operations: Vec<SyncOperation>) -> AppResult<PushResult>;

    /// Pull operations từ remote kể từ cursor
    async fn pull(&self, since_cursor: &str) -> AppResult<PullResult>;

    /// Acknowledge đã xử lý đến cursor
    async fn ack(&self, cursor: &str) -> AppResult<()>;

    /// Push một asset (binary file)
    async fn push_asset(&self, hash: [u8;32], data: Vec<u8>) -> AppResult<()>;

    /// Pull một asset
    async fn pull_asset(&self, hash: [u8;32]) -> AppResult<Option<Vec<u8>>>;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OperationAck {
    pub operation_id: [u8; 16],
    pub accepted: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushResult {
    pub accepted: Vec<OperationAck>,
    pub tx_bytes: u64,
    pub new_cursor: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PullResult {
    pub entries: Vec<RemoteEntry>,
    pub rx_bytes: u64,
    pub new_cursor: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub seq: u64, // Used for logical ordering if needed
    pub doc_hash: [u8; 32],
    pub source_device: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub timestamp: i64,
    pub is_delete: bool,
}
