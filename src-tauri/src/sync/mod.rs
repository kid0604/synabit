use serde::{Deserialize, Serialize};

pub mod coordinator;
pub mod hlc;
pub mod key_rotation;
pub mod progress;
pub mod protocol;

pub mod utils;

pub mod adapter;
pub mod core;

// Re-export common types
pub use core::types::{SyncOperation, SyncResult, SyncRunContext};

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
