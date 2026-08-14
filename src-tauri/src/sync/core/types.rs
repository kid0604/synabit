use serde::{Deserialize, Serialize};
pub use synabit_protocol::{AssetChunkRef, AssetRef, SyncEntryKind, SyncPayload};

#[derive(Serialize, Deserialize)]
pub struct DocSyncPayload {
    pub node_id: String,
    pub rel_path: String,
    pub snapshot: Vec<u8>,
    pub is_json: bool,
}

/// A single sync operation representing a document change, deletion, or asset reference
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncOperation {
    pub operation_id: [u8; 16],
    pub doc_hash: [u8; 32],
    pub entry_kind: SyncEntryKind,
    pub node_id: String,
    pub rel_path: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub timestamp: i64,
}

impl SyncOperation {
    pub fn is_delete(&self) -> bool {
        self.entry_kind == SyncEntryKind::Delete
    }
}

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

/// Correlates every transport involved in one user-visible sync request.
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
            trigger_reason: Self::normalize_trigger_reason(trigger_reason),
            vault_tag: Self::short_hash_tag(vault_path),
        }
    }

    pub fn transport_tag(&self, transport_id: &str) -> String {
        Self::short_hash_tag(transport_id)
    }

    pub fn log_phase(&self, provider: &str, transport_tag: &str, phase: &str, result: &SyncResult) {
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
}
