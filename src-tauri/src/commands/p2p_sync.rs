//! Tauri commands for P2P sync via the Synabit Sync Server.
//!
//! This module exposes `#[tauri::command]` functions that the frontend calls
//! to connect, sync, disconnect, and query P2P sync status.
//!
//! ## State model
//!
//! We store only the lightweight `P2pSyncConfig` (server address + server ID hex)
//! in Tauri managed state. The actual `SynabitServerTransport` is created on
//! demand for each sync operation because the QUIC transport is connection-
//! oriented and should not be held across idle periods.

use std::sync::Mutex;

use serde_json::json;
use tauri::Manager;

use crate::p2p::devices::DeviceRegistry;
use crate::p2p::pairing::{normalize_code, validate_code, PairedDevice, PairingInfo, PairingSession};
use crate::p2p::transport::SynabitServerTransport;
use crate::secrets::SecretManager;

// ---------------------------------------------------------------------------
// Managed state
// ---------------------------------------------------------------------------

/// Lightweight config stored between connect/disconnect.
#[derive(Debug, Clone)]
pub struct P2pSyncConfig {
    pub server_addr: String,
    pub server_id_hex: String,
}

/// Tauri managed state — `None` means disconnected.
pub type P2pSyncState = Mutex<Option<P2pSyncConfig>>;

/// Tauri managed state for an active pairing session.
pub type PairingState = Mutex<Option<PairingSession>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a hex-encoded 32-byte server ID into an `iroh::EndpointId`.
fn parse_server_id(hex_str: &str) -> Result<iroh::EndpointId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex: {}", e))?;

    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "server ID must be exactly 32 bytes (64 hex chars)".to_string())?;

    iroh::PublicKey::try_from(&bytes[..])
        .map_err(|e| format!("invalid server public key: {}", e))
}

/// Get (or generate + persist) the stable device ID from the KV store.
fn ensure_device_id(app_handle: &tauri::AppHandle) -> Result<String, String> {
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    match db.get_kv("device_id") {
        Ok(Some(id)) if !id.is_empty() => Ok(id),
        _ => {
            let id = uuid::Uuid::new_v4().to_string();
            db.set_kv("device_id", &id)
                .map_err(|e| format!("failed to persist device_id: {}", e))?;
            Ok(id)
        }
    }
}

/// Build a `SynabitServerTransport` from the current config + secrets.
async fn build_transport(
    app_handle: &tauri::AppHandle,
    config: &P2pSyncConfig,
) -> Result<SynabitServerTransport, String> {
    let e2ee_key = SecretManager::get_e2ee_key(Some(app_handle))
        .ok_or_else(|| "E2EE key not configured — set up encryption first".to_string())?;

    let device_id = ensure_device_id(app_handle)?;
    let server_id = parse_server_id(&config.server_id_hex)?;

    SynabitServerTransport::new(&config.server_addr, server_id, &e2ee_key, &device_id)
        .await
        .map_err(|e| format!("failed to create transport: {}", e))
}

/// Get the local hostname from environment variables (no external crate).
fn local_device_name() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "Desktop".to_string())
}

// ---------------------------------------------------------------------------
// Sync commands
// ---------------------------------------------------------------------------

/// Connect to a Synabit Sync Server.
///
/// Validates the server ID, ensures E2EE + device ID are available, creates a
/// transport, authenticates, and stores the config in managed state.
///
/// Returns `"connected"` on success.
#[tauri::command]
pub async fn p2p_sync_connect(
    app_handle: tauri::AppHandle,
    server_addr: String,
    server_id_hex: String,
) -> Result<String, String> {
    // Validate inputs early — fail fast before any I/O.
    if server_addr.is_empty() {
        return Err("server_addr must not be empty".to_string());
    }
    if server_id_hex.is_empty() {
        return Err("server_id_hex must not be empty".to_string());
    }

    // Validate hex parse before creating transport
    let _ = parse_server_id(&server_id_hex)?;

    let config = P2pSyncConfig {
        server_addr: server_addr.clone(),
        server_id_hex: server_id_hex.clone(),
    };

    // Create transport and test connection
    let transport = build_transport(&app_handle, &config).await?;

    use crate::sync::SyncTransport;
    transport
        .authenticate()
        .await
        .map_err(|e| format!("authentication failed: {}", e))?;

    // Store config in managed state
    {
        let state = app_handle.state::<P2pSyncState>();
        let mut guard = state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *guard = Some(config);
    }

    // Gracefully close connection after auth test
    let _ = transport.disconnect().await;

    // Record last successful connection time
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let now = chrono::Utc::now().to_rfc3339();
        let _ = db.set_kv("p2p_last_connected", &now);
        let _ = db.set_kv("p2p_server_addr", &server_addr);
    }

    log::info!("P2P sync connected to {}", server_addr);
    Ok("connected".to_string())
}

/// Run a full bidirectional sync against the connected Sync Server.
///
/// This is the main sync entry point — it pushes local changes and pulls
/// remote changes, merging CRDT documents as needed.
#[tauri::command]
pub async fn p2p_sync_full(
    app_handle: tauri::AppHandle,
    vault_path: String,
    is_cellular: bool,
) -> Result<crate::sync::SyncResult, String> {
    if vault_path.is_empty() {
        return Err("vault_path must not be empty".to_string());
    }

    // Read config from managed state
    let config = {
        let state = app_handle.state::<P2pSyncState>();
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .clone()
            .ok_or_else(|| "not connected — call p2p_sync_connect first".to_string())?
    };

    let transport = build_transport(&app_handle, &config).await?;

    let device_id = ensure_device_id(&app_handle)?;

    let result = crate::sync::engine::p2p_sync_full(
        &app_handle,
        &transport,
        &vault_path,
        &device_id,
    )
    .await
    .map_err(|e| format!("sync failed: {}", e))?;

    // Record last sync time & metrics
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let now = chrono::Utc::now();
        let _ = db.set_kv("p2p_last_sync_time", &now.to_rfc3339());
        
        // Record data usage metrics
        let today = now.format("%Y-%m-%d").to_string();
        let _ = db.record_sync_metric(
            &today,
            is_cellular,
            result.tx_bytes,
            result.rx_bytes,
        );
    }

    log::info!(
        "P2P sync complete: pushed={}, pulled={}, deleted={}, errors={}",
        result.pushed,
        result.pulled,
        result.deleted,
        result.errors.len()
    );

    // Gracefully close the connection to avoid endpoint dropped errors
    use crate::sync::SyncTransport;
    let _ = transport.disconnect().await;

    Ok(result)
}

/// Disconnect from the Sync Server by clearing the stored config.
#[tauri::command]
pub async fn p2p_sync_disconnect(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let state = app_handle.state::<P2pSyncState>();
    let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
    *guard = None;

    log::info!("P2P sync disconnected");
    Ok(())
}

/// Query the current P2P sync status.
///
/// Returns a JSON object with:
/// - `connected`: whether a server config is stored
/// - `server_addr`: the server address (if connected)
/// - `last_sync_time`: ISO-8601 timestamp of the last successful sync (if any)
#[tauri::command]
pub fn p2p_sync_status(
    app_handle: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<P2pSyncState>();
    let guard = state.lock().unwrap_or_else(|e| e.into_inner());

    let connected = guard.is_some();
    let server_addr = guard
        .as_ref()
        .map(|c| c.server_addr.clone())
        .unwrap_or_default();

    // Read last sync time from KV store
    let last_sync_time = {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("p2p_last_sync_time").unwrap_or(None)
    };

    Ok(json!({
        "connected": connected,
        "server_addr": server_addr,
        "last_sync_time": last_sync_time,
    }))
}

/// Fetch sync data metrics for a specific date (YYYY-MM-DD).
#[tauri::command]
pub fn p2p_sync_metrics(
    app_handle: tauri::AppHandle,
    date: String,
) -> Result<crate::db::metrics::SyncMetrics, String> {
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    
    db.get_sync_metrics(&date)
        .map_err(|e| format!("failed to fetch metrics: {}", e))
}

/// Save sync config to Android SharedPreferences for the background worker
#[tauri::command]
pub fn p2p_sync_update_worker_config(
    _app_handle: tauri::AppHandle,
    _vault_path: String,
    _server_addr: String,
    _server_id_hex: String,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;
        
        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.unwrap();
        let mut env = vm.attach_current_thread().unwrap();
        
        let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        
        let prefs_name = env.new_string("SynabitPrefs").unwrap();
        
        let shared_prefs = env.call_method(
            &context,
            "getSharedPreferences",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
            &[JValue::Object(&prefs_name), JValue::Int(0)]
        ).unwrap().l().unwrap();
        
        let editor = env.call_method(&shared_prefs, "edit", "()Landroid/content/SharedPreferences$Editor;", &[]).unwrap().l().unwrap();
        
        let k_vault = env.new_string("vaultPath").unwrap();
        let v_vault = env.new_string(&vault_path).unwrap();
        env.call_method(&editor, "putString", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;", &[JValue::Object(&k_vault), JValue::Object(&v_vault)]).unwrap();
        
        let k_addr = env.new_string("p2pServerAddr").unwrap();
        let v_addr = env.new_string(&server_addr).unwrap();
        env.call_method(&editor, "putString", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;", &[JValue::Object(&k_addr), JValue::Object(&v_addr)]).unwrap();
        
        let k_id = env.new_string("p2pServerIdHex").unwrap();
        let v_id = env.new_string(&server_id_hex).unwrap();
        env.call_method(&editor, "putString", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;", &[JValue::Object(&k_id), JValue::Object(&v_id)]).unwrap();
        
        env.call_method(&editor, "apply", "()V", &[]).unwrap();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Pairing commands
// ---------------------------------------------------------------------------

/// Initiate a new pairing session and return a pairing code.
///
/// The code is valid for 5 minutes. The session is stored in `PairingState`
/// managed state. Only one session can be active at a time.
#[tauri::command]
pub fn p2p_pair_initiate(app_handle: tauri::AppHandle) -> Result<PairingInfo, String> {
    // Get node_id from KV store
    let node_id_hex = {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("device_node_id")
            .ok()
            .flatten()
            .unwrap_or_else(|| "unknown".to_string())
    };

    let device_name = local_device_name();
    let session = PairingSession::new(node_id_hex, device_name);
    let info = session.pairing_info();

    let pairing_state = app_handle.state::<PairingState>();
    let mut state = pairing_state.lock().unwrap_or_else(|e| e.into_inner());
    *state = Some(session);

    log::info!("Pairing initiated, code={}", info.code);
    Ok(info)
}

/// Cancel any active pairing session.
#[tauri::command]
pub fn p2p_pair_cancel(app_handle: tauri::AppHandle) -> Result<(), String> {
    let pairing_state = app_handle.state::<PairingState>();
    let mut state = pairing_state.lock().unwrap_or_else(|e| e.into_inner());
    *state = None;

    log::info!("Pairing session cancelled");
    Ok(())
}

// ---------------------------------------------------------------------------
// Device commands
// ---------------------------------------------------------------------------

/// Accept a pairing code entered by the user.
///
/// Normalizes and validates the code, creates a `PairedDevice` record,
/// and stores it in the device registry. In a full implementation this
/// would initiate a P2P connection to the peer.
#[tauri::command]
pub fn p2p_pair_accept(
    app_handle: tauri::AppHandle,
    code: String,
) -> Result<PairedDevice, String> {
    let normalized = normalize_code(&code);
    validate_code(&normalized).map_err(|e| e.to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // For now, create a device entry with the code as identifier.
    // In a full implementation, this would initiate P2P connection.
    let device = PairedDevice {
        device_name: format!("Device-{}", &normalized[..4]),
        node_id_hex: normalized.clone(),
        paired_at: now,
        last_seen: now,
    };

    DeviceRegistry::add(&app_handle, device.clone()).map_err(|e| e.to_string())?;

    log::info!("Pairing accepted via code, device_name={}", device.device_name);
    Ok(device)
}

// ---------------------------------------------------------------------------

/// List all paired devices.
#[tauri::command]
pub fn p2p_list_devices(app_handle: tauri::AppHandle) -> Result<Vec<PairedDevice>, String> {
    Ok(DeviceRegistry::list(&app_handle))
}

/// Remove a paired device by its node ID.
#[tauri::command]
pub fn p2p_remove_device(
    app_handle: tauri::AppHandle,
    node_id_hex: String,
) -> Result<(), String> {
    DeviceRegistry::remove(&app_handle, &node_id_hex).map_err(|e| e.to_string())
}


// ---------------------------------------------------------------------------
// Key rotation commands
// ---------------------------------------------------------------------------

/// Get the current E2EE epoch (0 = no rotation has occurred).
#[tauri::command]
pub fn p2p_current_epoch(app_handle: tauri::AppHandle) -> Result<u32, String> {
    Ok(crate::sync::key_rotation::KeyRotationManager::current_epoch(&app_handle))
}

/// Revoke a device by its node ID hex.
///
/// Removes the device from the local registry and increments the E2EE epoch.
/// Returns the new epoch number. The caller should subsequently tell the
/// server to rotate the mailbox token.
#[tauri::command]
pub fn p2p_revoke_device(
    app_handle: tauri::AppHandle,
    node_id_hex: String,
) -> Result<u32, String> {
    crate::sync::key_rotation::KeyRotationManager::revoke_device_local(
        &app_handle,
        &node_id_hex,
    )
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------


