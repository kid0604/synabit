//! Epoch-based key rotation and device revocation manager.
//!
//! The master key (BIP39 mnemonic) is NEVER changed. Instead, each "epoch"
//! derives a fresh encryption key via `derive_epoch_key`. When a device is
//! revoked the epoch is incremented, making all future ciphertext unreadable
//! to the revoked device (which only knows epoch keys up to the old epoch).
//!
//! ## Epoch storage
//!
//! The current epoch is stored in the local KV store under key `"e2ee_epoch"`.
//! Epoch 0 is the implicit default (no rotation has ever occurred).

use log::{info, warn};
use tauri::Manager;

use crate::error::{AppError, AppResult};

/// Manages epoch-based key rotation for E2EE sync.
pub struct KeyRotationManager;

impl KeyRotationManager {
    /// Get current epoch from KV store. Defaults to 0.
    pub fn current_epoch(app_handle: &tauri::AppHandle) -> u32 {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("e2ee_epoch")
            .unwrap_or(None)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    }

    /// Increment epoch and store. Returns the new epoch value.
    pub fn increment_epoch(app_handle: &tauri::AppHandle) -> AppResult<u32> {
        let new_epoch = Self::current_epoch(app_handle) + 1;
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.set_kv("e2ee_epoch", &new_epoch.to_string())
            .map_err(|e| AppError::SyncError(format!("Set epoch: {}", e)))?;
        info!("Key rotation: epoch incremented to {}", new_epoch);
        Ok(new_epoch)
    }

    /// Derive the mailbox token for a given epoch.
    ///
    /// The token is `blake3::derive_key("synabit-mailbox-v1", &epoch_key)` where
    /// `epoch_key = derive_epoch_key(master_key, epoch)`. This ensures a rotated
    /// epoch produces a new token the revoked device cannot compute.
    pub fn derive_mailbox_token(master_key: &[u8; 32], epoch: u32) -> Vec<u8> {
        let epoch_key = crate::sync::crypto::derive_epoch_key(master_key, epoch);
        blake3::derive_key("synabit-mailbox-v1", &epoch_key).to_vec()
    }

    /// Full local revoke flow: remove device from registry + increment epoch.
    ///
    /// The caller is responsible for subsequently telling the server to
    /// `RevokeDevice` and `RotateToken` with the new mailbox token.
    ///
    /// Returns the new epoch value.
    pub fn revoke_device_local(
        app_handle: &tauri::AppHandle,
        device_id_to_revoke: &str,
    ) -> AppResult<u32> {
        // 1. Remove from device registry
        if let Err(e) = crate::p2p::devices::DeviceRegistry::remove(app_handle, device_id_to_revoke) {
            warn!(
                "Key rotation: device {} not found in registry (may already be removed): {}",
                device_id_to_revoke, e
            );
        }

        // 2. Increment epoch
        let new_epoch = Self::increment_epoch(app_handle)?;

        info!(
            "Key rotation: device {} revoked, new epoch = {}",
            device_id_to_revoke, new_epoch
        );
        Ok(new_epoch)
    }
}
