//! Sync progress event types for UI feedback.
//!
//! These types are emitted via `app_handle.emit()` and consumed by the
//! frontend to show real-time sync progress, conflict info, and quota status.

use serde::{Deserialize, Serialize};

/// Sync progress event emitted via app_handle.emit()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgressEvent {
    pub phase: SyncPhase,
    pub current: u32,
    pub total: u32,
    pub file_name: String,
    pub bytes_transferred: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncPhase {
    Pull,
    Push,
    Ack,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaInfo {
    pub current_bytes: u64,
    pub limit_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflictInfo {
    pub file_name: String,
    pub resolution: String, // "crdt_merge", "lww_remote", "lww_local"
}
