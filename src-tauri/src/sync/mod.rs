use crate::error::AppResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod crypto;
pub mod engine;
pub mod hlc;
pub mod identity;
pub mod key_rotation;
pub mod migration;
pub mod progress;
pub mod protocol;

pub mod utils;

// ---------------------------------------------------------------------------
// Sync result types
// ---------------------------------------------------------------------------

/// Result of a sync operation, used by the UI to show what happened.
#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub pulled: u32,
    pub pulled_files: Vec<String>,
    pub pushed: u32,
    pub deleted: u32,
    pub errors: Vec<String>,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
}

impl SyncResult {
    pub fn empty() -> Self {
        Self {
            pulled: 0,
            pulled_files: Vec::new(),
            pushed: 0,
            deleted: 0,
            errors: Vec::new(),
            tx_bytes: 0,
            rx_bytes: 0,
        }
    }
}

/// A remote sync entry returned by the server (or any transport).
#[derive(Debug, Clone)]
pub struct RemoteSyncEntry {
    /// Sequence number (monotonic per vault)
    pub seq: u64,
    /// Document identifier hash
    pub doc_hash: [u8; 32],
    /// Which device pushed this
    pub source_device: String,
    /// Encrypted CRDT payload (opaque, decrypt on client)
    pub encrypted_payload: Vec<u8>,
    /// BLAKE3 hash of the encrypted payload (integrity check)
    pub payload_hash: [u8; 32],
    /// Server-side timestamp
    pub timestamp: i64,
    /// Whether this is a deletion tombstone
    pub is_delete: bool,
}

// ---------------------------------------------------------------------------
// SyncTransport trait — provider-agnostic sync interface
// ---------------------------------------------------------------------------

/// Transport layer for syncing encrypted CRDT data.
///
/// Implementations:
/// - `SynabitServerTransport` — connects to Synabit Sync Server (primary)
/// - `GDriveTransport` — Google Drive sync (legacy, existing)
///
/// All payloads are pre-encrypted by the caller. The transport NEVER sees
/// plaintext data.
#[async_trait]
pub trait SyncTransport: Send + Sync {
    /// Human-readable provider name (for UI display).
    fn provider_name(&self) -> &str;

    /// Authenticate with the remote server/service.
    async fn authenticate(&self) -> AppResult<()>;

    /// Disconnect from the remote service (important for mobile backgrounding).
    async fn disconnect(&self) -> AppResult<()> {
        Ok(()) // Default no-op implementation
    }

    /// Push an encrypted document snapshot to the remote.
    /// Returns the assigned sequence number.
    async fn push_doc(
        &self,
        doc_hash: &[u8; 32],
        encrypted_payload: Vec<u8>,
    ) -> AppResult<u64>;

    /// Pull all entries since a given sequence number.
    async fn pull_since(&self, since_seq: u64) -> AppResult<Vec<RemoteSyncEntry>>;

    /// Acknowledge that this device has processed entries up to `seq`.
    async fn ack(&self, up_to_seq: u64) -> AppResult<()>;

    /// Push an encrypted asset (image, attachment).
    async fn push_asset(
        &self,
        asset_hash: &[u8; 32],
        encrypted_data: Vec<u8>,
    ) -> AppResult<()>;

    /// Pull an encrypted asset by its hash.
    async fn pull_asset(&self, asset_hash: &[u8; 32]) -> AppResult<Option<Vec<u8>>>;

    /// Push a deletion tombstone for a document.
    async fn push_delete(&self, doc_hash: &[u8; 32]) -> AppResult<u64>;

    /// Check if the transport is currently connected/available.
    async fn is_available(&self) -> bool;
}

/// Configuration for connecting to a Synabit Sync Server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncServerConfig {
    /// Server address (e.g., "sync.synabit.app:4433")
    pub server_addr: String,
    /// Device ID (stable UUID per device)
    pub device_id: String,
}
