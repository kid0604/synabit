//! Device revocation and the epoch counter.
//!
//! ## What revocation actually does
//!
//! Every device in a vault holds the *same* 32-byte key, derived from the one
//! BIP39 recovery phrase and stored in the OS keychain. There are no per-device
//! keys and no key wrapping.
//!
//! That has a consequence worth stating plainly, because the code here used to
//! claim the opposite: **revocation cannot be cryptographic in this key model.**
//! A revoked device still holds the vault key, so it can decrypt any copy of the
//! data it already has, and it can derive any value computed from that key —
//! including the key for any future "epoch". Incrementing an epoch counter does
//! not make new ciphertext unreadable to it.
//!
//! What revocation *does* achieve is access control at the sync server: the
//! server stops accepting that device id, so the device no longer receives new
//! operations and can no longer push. That is genuinely useful for a device that
//! was lost or handed on, and it is what `sync_revoke_device` performs.
//!
//! To actually cut off a device that still holds the recovery phrase, the vault
//! needs a new phrase. Making revocation cryptographic without that would mean a
//! different design: a per-device keypair, the vault key wrapped once per
//! device, and rotation re-wrapping only for the devices that remain.
//!
//! ## The epoch counter
//!
//! `e2ee_epoch` in the local KV store counts how many times the user has revoked
//! a device. It is a local audit counter shown in settings. It deliberately does
//! not feed key derivation: an epoch-scoped key would have to be communicated to
//! every other device, and the only channel for that is the vault key they
//! already share — which the revoked device shares too.

use log::info;
use tauri::Manager;

use crate::error::{AppError, AppResult};

/// Tracks the local revocation counter.
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
        info!("Revocation counter advanced to {}", new_epoch);
        Ok(new_epoch)
    }

    /// Record a revocation locally. The server-side revocation is performed by
    /// the caller, which is the part that actually denies access.
    pub fn revoke_device_local(
        app_handle: &tauri::AppHandle,
        device_id_to_revoke: &str,
    ) -> AppResult<u32> {
        let new_epoch = Self::increment_epoch(app_handle)?;

        info!(
            "Device {} revoked at the sync server; local revocation counter = {}",
            device_id_to_revoke, new_epoch
        );
        Ok(new_epoch)
    }
}
