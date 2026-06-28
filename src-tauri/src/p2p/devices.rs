//! Paired device registry — persistent storage for paired P2P devices.
//!
//! Uses the KV store to persist a JSON array of `PairedDevice` records.
//! All operations are synchronous and follow the same DB access pattern
//! used throughout the codebase: `app_handle.state::<crate::db::DbState>()`.

use log::{info, warn};
use tauri::Manager;

use super::pairing::PairedDevice;
use crate::error::{AppError, AppResult};

/// KV key under which the paired-device list is stored.
const DEVICE_LIST_KEY: &str = "p2p:paired_devices";

// ---------------------------------------------------------------------------
// Device registry
// ---------------------------------------------------------------------------

/// Manages the paired-device registry backed by the KV store.
pub struct DeviceRegistry;

impl DeviceRegistry {
    /// List all paired devices.
    pub fn list(app_handle: &tauri::AppHandle) -> Vec<PairedDevice> {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        match db.get_kv(DEVICE_LIST_KEY) {
            Ok(Some(json_str)) => serde_json::from_str(&json_str).unwrap_or_else(|e| {
                warn!("Failed to parse paired devices JSON: {}", e);
                Vec::new()
            }),
            Ok(None) => Vec::new(),
            Err(e) => {
                warn!("Failed to read paired devices: {}", e);
                Vec::new()
            }
        }
    }

    /// Add a paired device (replaces existing entry with same `node_id_hex`).
    pub fn add(app_handle: &tauri::AppHandle, device: PairedDevice) -> AppResult<()> {
        let mut devices = Self::list(app_handle);
        // Remove existing entry with same node_id (re-pair)
        devices.retain(|d| d.node_id_hex != device.node_id_hex);

        info!(
            "Adding paired device: name={}, node_id={}",
            device.device_name, device.node_id_hex
        );
        devices.push(device);
        Self::save(app_handle, &devices)
    }

    /// Remove a paired device by `node_id_hex`.
    pub fn remove(app_handle: &tauri::AppHandle, node_id_hex: &str) -> AppResult<()> {
        let mut devices = Self::list(app_handle);
        let before = devices.len();
        devices.retain(|d| d.node_id_hex != node_id_hex);

        if devices.len() < before {
            info!("Removed paired device: node_id={}", node_id_hex);
        } else {
            warn!("Attempted to remove unknown device: node_id={}", node_id_hex);
        }

        Self::save(app_handle, &devices)
    }

    /// Check if a device is paired.
    pub fn is_paired(app_handle: &tauri::AppHandle, node_id_hex: &str) -> bool {
        Self::list(app_handle)
            .iter()
            .any(|d| d.node_id_hex == node_id_hex)
    }

    /// Update `last_seen` timestamp for a paired device.
    pub fn update_last_seen(app_handle: &tauri::AppHandle, node_id_hex: &str) -> AppResult<()> {
        let mut devices = Self::list(app_handle);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for device in &mut devices {
            if device.node_id_hex == node_id_hex {
                device.last_seen = now;
            }
        }
        Self::save(app_handle, &devices)
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn save(app_handle: &tauri::AppHandle, devices: &[PairedDevice]) -> AppResult<()> {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        let json_str = serde_json::to_string(devices)
            .map_err(|e| AppError::General(format!("serialize devices: {}", e)))?;
        db.set_kv(DEVICE_LIST_KEY, &json_str)
            .map_err(|e| AppError::General(format!("save devices: {}", e)))
    }
}
