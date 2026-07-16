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
// Sync run observability
// ---------------------------------------------------------------------------

/// Correlates every transport involved in one user-visible sync request.
///
/// Tags are deliberately derived from hashes so structured logs do not need
/// to expose the vault path or a peer's full transport identifier.
#[derive(Debug, Clone)]
pub struct SyncRunContext {
    pub run_id: String,
    pub trigger_reason: String,
    pub vault_tag: String,
}

impl SyncRunContext {
    pub fn new(vault_path: &str, trigger_reason: Option<&str>) -> Self {
        Self {
            run_id: uuid::Uuid::new_v4().to_string(),
            trigger_reason: normalize_trigger_reason(trigger_reason),
            vault_tag: short_hash_tag(vault_path),
        }
    }

    pub fn transport_tag(&self, transport_id: &str) -> String {
        short_hash_tag(transport_id)
    }

    pub fn log_phase(
        &self,
        provider: &str,
        transport_tag: &str,
        phase: &str,
        result: &SyncResult,
    ) {
        log::info!(
            "sync_phase run_id={} trigger={} vault_tag={} provider={} transport_tag={} phase={} pulled={} pushed={} deleted={} errors={} tx_bytes={} rx_bytes={}",
            self.run_id,
            self.trigger_reason,
            self.vault_tag,
            provider,
            transport_tag,
            phase,
            result.pulled,
            result.pushed,
            result.deleted,
            result.errors.len(),
            result.tx_bytes,
            result.rx_bytes,
        );
    }
}

fn short_hash_tag(value: &str) -> String {
    let digest = blake3::hash(value.as_bytes()).to_hex();
    digest[..12].to_string()
}

fn normalize_trigger_reason(value: Option<&str>) -> String {
    match value.unwrap_or("manual").trim() {
        "manual" => "manual",
        "server_push" => "server_push",
        "periodic_timer" => "periodic_timer",
        "app_foreground" => "app_foreground",
        "initial_connect" => "initial_connect",
        "watcher_create_delete" => "watcher_create_delete",
        "watcher_modified" => "watcher_modified",
        "queued_retry" => "queued_retry",
        _ => "unknown",
    }
    .to_string()
}

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
