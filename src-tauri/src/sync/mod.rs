use crate::error::AppResult;
use async_trait::async_trait;

pub mod crypto;

#[async_trait]
pub trait SyncTransport {
    /// Download the CRDT snapshot for a specific document from the remote source.
    /// Returns Ok(Vec<u8>) containing the snapshot bytes, or an AppError.
    async fn pull_snapshot(&self, doc_id: &str) -> AppResult<Vec<u8>>;
    
    /// Upload the local CRDT snapshot for a specific document to the remote source.
    async fn push_snapshot(&self, doc_id: &str, snapshot: Vec<u8>) -> AppResult<()>;
}
