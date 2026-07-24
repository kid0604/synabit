use crate::error::AppResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod coordinator;
pub mod hlc;
pub mod key_rotation;
pub mod migration;
pub mod progress;
pub mod protocol;

pub mod utils;

pub mod core;
pub mod adapter;

// Re-export common types
pub use core::types::{SyncResult, RemoteSyncEntry, SyncRunContext, SyncOperation};

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

    /// Unique identifier for this transport instance to maintain separate cursors
    fn transport_id(&self) -> String {
        self.provider_name().to_string()
    }

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

    /// Push multiple encrypted document snapshots in a single batch.
    /// Returns the maximum assigned sequence number.
    async fn push_doc_batch(
        &self,
        items: Vec<([u8; 32], Vec<u8>)>,
    ) -> AppResult<u64> {
        let mut max_seq = 0;
        for (doc_hash, payload) in items {
            max_seq = max_seq.max(self.push_doc(&doc_hash, payload).await?);
        }
        Ok(max_seq)
    }

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
    async fn ping(&self) -> crate::error::AppResult<()>;
}

/// Configuration for connecting to a Synabit Sync Server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncServerConfig {
    /// Server address (e.g., "sync.synabit.app:4433")
    pub server_addr: String,
    /// Device ID (stable UUID per device)
    pub device_id: String,
}

#[cfg(test)]
mod run_context_tests {
    use super::*;

    #[test]
    fn run_context_uses_stable_redacted_tags() {
        let first = SyncRunContext::new("/private/vault", Some("manual"));
        let second = SyncRunContext::new("/private/vault", Some("manual"));

        assert_eq!(first.vault_tag, second.vault_tag);
        assert_eq!(first.vault_tag.len(), 12);
        assert_ne!(first.run_id, second.run_id);
        assert!(!first.vault_tag.contains("vault"));
    }

    #[test]
    fn run_context_rejects_untrusted_trigger_labels() {
        let context = SyncRunContext::new("vault", Some("manual\nforged=true"));
        assert_eq!(context.trigger_reason, "unknown");
    }
}
