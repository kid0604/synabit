//! Tauri commands for P2P sync via the Synabit Sync Server.
//!
//! This module exposes `#[tauri::command]` functions that the frontend calls
//! to connect, sync, disconnect, and query P2P sync status.
//!
//! ## State model
//!
//! We store only the lightweight `P2pSyncConfig` (server address + server ID hex)
//! in Tauri managed state. The actual `SynabitServerAdapter` is created on
//! demand for each sync operation because the QUIC connection is connection-
//! oriented and should not be held across idle periods.

use crate::error::logged;
use std::sync::Mutex;

use serde_json::json;
use tauri::Manager;

use crate::secrets::SecretManager;
use crate::sync::SyncRunContext;

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
pub type P2pSyncState = Mutex<
    Option<(
        P2pSyncConfig,
        std::sync::Arc<crate::sync::adapter::server::SynabitServerAdapter>,
    )>,
>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a hex-encoded 32-byte server ID into an `iroh::EndpointId`.
fn parse_server_id(hex_str: &str) -> Result<iroh::EndpointId, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex: {}", e))?;

    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "server ID must be exactly 32 bytes (64 hex chars)".to_string())?;

    iroh::PublicKey::try_from(&bytes[..]).map_err(|e| format!("invalid server public key: {}", e))
}

/// Get (or generate + persist) the stable device ID from the KV store.
pub(crate) fn ensure_device_id<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<String, String> {
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

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

/// Build a `SynabitServerAdapter` from the current config + secrets.
async fn build_adapter(
    app_handle: &tauri::AppHandle,
    config: &P2pSyncConfig,
) -> Result<std::sync::Arc<crate::sync::adapter::server::SynabitServerAdapter>, String> {
    let server_id = parse_server_id(&config.server_id_hex)?;

    let e2ee_key_opt = SecretManager::get_e2ee_key(Some(app_handle));
    if e2ee_key_opt.is_none() {
        return Err("E2EE key not set up. Please set up encryption first.".to_string());
    }
    let e2ee_key = e2ee_key_opt.unwrap();

    let device_id = ensure_device_id(app_handle)?;

    let adapter = crate::sync::adapter::server::SynabitServerAdapter::new(
        &config.server_addr,
        server_id,
        &e2ee_key,
        &device_id,
        Some(app_handle.clone()),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(std::sync::Arc::new(adapter))
}

// ---------------------------------------------------------------------------
// Sync commands
// ---------------------------------------------------------------------------

/// Connect to a Synabit Sync Server.
///
/// Validates the server ID, ensures E2EE + device ID are available, creates an
/// adapter, authenticates, and stores the config in managed state.
///
/// Returns `"connected"` on success.
#[tauri::command]
pub async fn sync_connect(
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

    // Validate hex parse before creating adapter
    let _ = parse_server_id(&server_id_hex)?;

    let config = P2pSyncConfig {
        server_addr: server_addr.clone(),
        server_id_hex: server_id_hex.clone(),
    };

    // Create adapter and test connection
    let adapter = build_adapter(&app_handle, &config).await?;

    use crate::sync::adapter::SyncAdapter;
    adapter
        .connect()
        .await
        .map_err(|e| format!("connection failed: {}", e))?;

    // Store config in managed state
    {
        let state = app_handle.state::<P2pSyncState>();
        let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some((config, adapter.clone()));
    }

    // The connection is now persistent and stored in state.
    // It will remain active for push notifications and subsequent syncs.
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let now = chrono::Utc::now().to_rfc3339();
        logged("record last connection", "p2p_last_connected", db.set_kv("p2p_last_connected", &now));
        logged("record peer address", "p2p_server_addr", db.set_kv("p2p_server_addr", &server_addr));
        logged("record peer id", "p2p_server_id_hex", db.set_kv("p2p_server_id_hex", &server_id_hex));
    }

    log::info!("P2P sync connected to {}", server_addr);
    Ok("connected".to_string())
}

/// Run a full bidirectional sync against the connected Sync Server.
///
/// This is the main sync entry point — it pushes local changes and pulls
/// remote changes, merging CRDT documents as needed.
#[tauri::command]
pub async fn sync_full(
    app_handle: tauri::AppHandle,
    vault_path: String,
    is_cellular: bool,
    trigger_reason: Option<String>,
) -> Result<crate::sync::SyncResult, String> {
    if vault_path.is_empty() {
        return Err("vault_path must not be empty".to_string());
    }

    let run_context = SyncRunContext::new(&vault_path, trigger_reason.as_deref());
    log::info!(
        "sync_run run_id={} trigger={} vault_tag={} scope=command state=start cellular={}",
        run_context.run_id,
        run_context.trigger_reason,
        run_context.vault_tag,
        is_cellular,
    );

    let device_id = ensure_device_id(&app_handle)?;

    // 1. Get E2EE key (Required for both Server and P2P)
    let e2ee_key_opt = crate::secrets::SecretManager::get_e2ee_key(Some(&app_handle));
    if e2ee_key_opt.is_none() {
        return Err("E2EE key not set up. Please set up encryption first.".to_string());
    }
    let e2ee_key = e2ee_key_opt.unwrap();

    // 2. Read Server config from managed state, reconnecting if it is not there.
    //
    // The adapter lives only in memory, so a sync triggered before the UI has
    // finished calling `sync_connect` — on a cold start the timer and the
    // watcher can both fire first — used to fail with "No sync adapter
    // configured". Rebuilding it from the stored configuration makes the order
    // of those calls stop mattering.
    let mut server_adapter_arc = {
        let state = app_handle.state::<P2pSyncState>();
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|(_, t)| t.clone())
    };

    if server_adapter_arc.is_none() {
        let stored = {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            match (
                db.get_kv("p2p_server_addr").ok().flatten(),
                db.get_kv("p2p_server_id_hex").ok().flatten(),
            ) {
                (Some(addr), Some(id)) if !addr.is_empty() && !id.is_empty() => Some((addr, id)),
                _ => None,
            }
        };

        if let Some((addr, id)) = stored {
            let config = P2pSyncConfig {
                server_addr: addr.clone(),
                server_id_hex: id,
            };
            match build_adapter(&app_handle, &config).await {
                Ok(adapter) => {
                    log::info!("Rebuilding sync adapter for {} from stored config", addr);
                    let state = app_handle.state::<P2pSyncState>();
                    let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = Some((config, adapter.clone()));
                    server_adapter_arc = Some(adapter);
                }
                Err(e) => log::warn!("Could not rebuild sync adapter from stored config: {}", e),
            }
        }
    }

    let vault_identity =
        crate::sync::core::identity::load_or_register_vault_identity(&app_handle, &vault_path)
            .map_err(|e| e.to_string())?;

    let mut coordinator = crate::sync::coordinator::SyncCoordinator::new();
    if let Some(adapter) = server_adapter_arc {
        coordinator
            .set_adapter(adapter.clone())
            .await
            .map_err(|e| e.to_string())?;
    }

    // 3. Run sync
    let result = match coordinator
        .sync(
            &vault_identity,
            &device_id,
            &e2ee_key,
            &run_context,
            &app_handle,
        )
        .await
    {
        Ok(result) => {
            log::info!(
                "sync_run run_id={} trigger={} vault_tag={} scope=command state=success pulled={} pushed={} deleted={}",
                run_context.run_id,
                run_context.trigger_reason,
                run_context.vault_tag,
                result.pulled,
                result.pushed,
                result.deleted
            );
            result
        }
        Err(e) => {
            log::error!(
                "sync_run run_id={} trigger={} vault_tag={} scope=command state=error msg=\"{}\"",
                run_context.run_id,
                run_context.trigger_reason,
                run_context.vault_tag,
                e
            );
            return Err(e.to_string());
        }
    };

    // Record last sync time & metrics
    {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let now = chrono::Utc::now();
        logged("record sync time", "p2p_last_sync_time", db.set_kv("p2p_last_sync_time", &now.to_rfc3339()));

        // Record data usage metrics
        let today = now.format("%Y-%m-%d").to_string();
        logged("record sync metric", &today, db.record_sync_metric(&today, is_cellular, result.tx_bytes, result.rx_bytes));
    }

    log::info!(
        "sync_run run_id={} trigger={} vault_tag={} scope=command state=complete pulled={} pushed={} deleted={} errors={} tx_bytes={} rx_bytes={}",
        run_context.run_id,
        run_context.trigger_reason,
        run_context.vault_tag,
        result.pulled,
        result.pushed,
        result.deleted,
        result.errors.len(),
        result.tx_bytes,
        result.rx_bytes,
    );

    Ok(result)
}

/// Disconnect from the Sync Server by clearing the stored config.
#[tauri::command]
pub async fn sync_disconnect(app_handle: tauri::AppHandle) -> Result<(), String> {
    let transport = {
        let state = app_handle.state::<P2pSyncState>();
        let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
        guard.take() // This leaves None in the state and returns the Option
    };

    if let Some((_, t)) = transport {
        use crate::sync::adapter::SyncAdapter;
        let _ = t.disconnect().await;
    }

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
pub async fn sync_status(app_handle: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<P2pSyncState>();
    let guard = state.lock().unwrap_or_else(|e| e.into_inner());

    let connected = guard.is_some();
    let server_addr = guard
        .as_ref()
        .map(|c| c.0.server_addr.clone())
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
pub async fn sync_metrics(
    app_handle: tauri::AppHandle,
    date: String,
) -> Result<crate::db::metrics::SyncMetrics, String> {
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

    db.get_sync_metrics(&date)
        .map_err(|e| format!("failed to fetch metrics: {}", e))
}

/// Save sync config to Android SharedPreferences for the background worker
#[allow(unused_variables)]
#[tauri::command]
pub async fn sync_update_worker_config(
    app_handle: tauri::AppHandle,
    vault_path: String,
    server_addr: String,
    server_id_hex: String,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        use jni::objects::JValue;

        let ctx = ndk_context::android_context();
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.unwrap();
        let mut env = vm.attach_current_thread().unwrap();

        let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

        let prefs_name = env.new_string("SynabitPrefs").unwrap();

        let shared_prefs = env
            .call_method(
                &context,
                "getSharedPreferences",
                "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
                &[JValue::Object(&prefs_name), JValue::Int(0)],
            )
            .unwrap()
            .l()
            .unwrap();

        let editor = env
            .call_method(
                &shared_prefs,
                "edit",
                "()Landroid/content/SharedPreferences$Editor;",
                &[],
            )
            .unwrap()
            .l()
            .unwrap();

        let k_vault = env.new_string("vaultPath").unwrap();
        let v_vault = env.new_string(&vault_path).unwrap();
        env.call_method(
            &editor,
            "putString",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            &[JValue::Object(&k_vault), JValue::Object(&v_vault)],
        )
        .unwrap();

        let k_addr = env.new_string("p2pServerAddr").unwrap();
        let v_addr = env.new_string(&server_addr).unwrap();
        env.call_method(
            &editor,
            "putString",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            &[JValue::Object(&k_addr), JValue::Object(&v_addr)],
        )
        .unwrap();

        let k_id = env.new_string("p2pServerIdHex").unwrap();
        let v_id = env.new_string(&server_id_hex).unwrap();
        env.call_method(
            &editor,
            "putString",
            "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
            &[JValue::Object(&k_id), JValue::Object(&v_id)],
        )
        .unwrap();

        env.call_method(&editor, "apply", "()V", &[]).unwrap();
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Key rotation commands
// ---------------------------------------------------------------------------

/// Get the current E2EE epoch (0 = no rotation has occurred).
#[tauri::command]
pub async fn sync_current_epoch(app_handle: tauri::AppHandle) -> Result<u32, String> {
    Ok(crate::sync::key_rotation::KeyRotationManager::current_epoch(&app_handle))
}

/// Revoke another device's access to the sync server.
///
/// This is *access* revocation and nothing more. Every device shares one vault
/// key derived from the recovery phrase, so a revoked device that still holds
/// that phrase can enrol again under a different device id, and can still read
/// any copy of the data it already has. Genuinely cutting off a device you no
/// longer control means generating a new recovery phrase for the vault.
///
/// Returns the epoch counter after the local bump, kept for the existing UI.
#[tauri::command]
pub async fn sync_revoke_device(
    app_handle: tauri::AppHandle,
    node_id_hex: String,
) -> Result<u32, String> {
    let adapter = {
        let state = app_handle.state::<P2pSyncState>();
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|(_, a)| a.clone())
    };

    let adapter = adapter.ok_or_else(|| {
        "Not connected to a sync server, so there is nothing to revoke against.".to_string()
    })?;

    adapter
        .revoke_device(&node_id_hex)
        .await
        .map_err(|e| e.to_string())?;

    crate::sync::key_rotation::KeyRotationManager::revoke_device_local(&app_handle, &node_id_hex)
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
