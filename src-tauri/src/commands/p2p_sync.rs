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
pub type P2pSyncState = Mutex<Option<(P2pSyncConfig, std::sync::Arc<crate::p2p::transport::SynabitServerTransport>)>>;

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
) -> Result<std::sync::Arc<crate::p2p::transport::SynabitServerTransport>, String> {
    let e2ee_key = SecretManager::get_e2ee_key(Some(app_handle))
        .ok_or_else(|| "E2EE key not configured — set up encryption first".to_string())?;

    let device_id = ensure_device_id(app_handle)?;
    let server_id = parse_server_id(&config.server_id_hex)?;

    let t = SynabitServerTransport::new(&config.server_addr, server_id, &e2ee_key, &device_id, Some(app_handle.clone()))
        .await
        .map_err(|e| format!("failed to create transport: {}", e))?;
    Ok(std::sync::Arc::new(t))
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
        *guard = Some((config, transport.clone()));
    }

    // The connection is now persistent and stored in state.
    // It will remain active for push notifications and subsequent syncs.
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

    // 2. Read Server config from managed state (optional now)
    let server_transport_arc = {
        let state = app_handle.state::<P2pSyncState>();
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().map(|(_, t)| t.clone())
    };

    // 3. Collect P2P Transports only when the rollout safety gate allows it.
    // Protocol v1 is unsafe and is never enabled in release builds.
    let mut p2p_transports_owned: Vec<crate::p2p::direct::DirectP2PTransport> = Vec::new();
    let direct_mode = crate::p2p::direct_p2p_mode(&app_handle);
    let paired_devices = crate::p2p::devices::DeviceRegistry::list(&app_handle);

    let persistent_endpoint = app_handle.try_state::<std::sync::Arc<crate::p2p::endpoint::PersistentEndpoint>>();
    let discovery = app_handle.try_state::<std::sync::Arc<crate::p2p::discovery::PeerDiscovery>>();

    if direct_mode.is_enabled() {
        if let (Some(ep), Some(disc)) = (persistent_endpoint, discovery) {
            for device in &paired_devices {
                if let Ok(peer_id) = parse_server_id(&device.node_id_hex) {
                    let lan_address = disc
                        .get_peer(&device.node_id_hex)
                        .and_then(|p| p.lan_address);
                    let p2p_t = crate::p2p::direct::DirectP2PTransport::new(
                        app_handle.clone(),
                        ep.endpoint_cloned(),
                        peer_id,
                        &e2ee_key,
                        &device_id,
                        lan_address,
                    );
                    p2p_transports_owned.push(p2p_t);
                }
            }
        }
    } else if !paired_devices.is_empty() {
        log::warn!(
            "sync_run run_id={} trigger={} vault_tag={} scope=direct_gate state=blocked mode={} protocol=v{} paired_peers={}",
            run_context.run_id,
            run_context.trigger_reason,
            run_context.vault_tag,
            direct_mode.label(),
            crate::p2p::DIRECT_P2P_PROTOCOL_VERSION,
            paired_devices.len(),
        );
    }

    if server_transport_arc.is_none() && !paired_devices.is_empty() && !direct_mode.is_enabled() {
        log::warn!(
            "sync_run run_id={} trigger={} vault_tag={} scope=command state=blocked reason=direct_v1_safety_gate",
            run_context.run_id,
            run_context.trigger_reason,
            run_context.vault_tag,
        );
        return Err(
            "Direct P2P v1 is safety-disabled until protocol v2 is available. Connect the Sync Server, or use the debug-only unsafe legacy override for regression testing."
                .to_string(),
        );
    }

    let mut p2p_transport_refs: Vec<&dyn crate::sync::SyncTransport> = Vec::new();
    for t in &p2p_transports_owned {
        p2p_transport_refs.push(t as &dyn crate::sync::SyncTransport);
    }

    let server_transport_dyn = server_transport_arc.as_ref().map(|t| t.as_ref() as &dyn crate::sync::SyncTransport);

    let result = crate::p2p::hybrid::hybrid_sync(
        &app_handle,
        server_transport_dyn,
        &p2p_transport_refs,
        &vault_path,
        &device_id,
        &run_context,
    )
    .await
    .map_err(|e| {
        log::warn!(
            "sync_run run_id={} trigger={} vault_tag={} scope=command state=failed error_kind=hybrid_sync",
            run_context.run_id,
            run_context.trigger_reason,
            run_context.vault_tag,
        );
        format!("hybrid sync failed: {}", e)
    })?;

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
        "sync_run run_id={} trigger={} vault_tag={} scope=command state=complete direct_mode={} pulled={} pushed={} deleted={} errors={} tx_bytes={} rx_bytes={}",
        run_context.run_id,
        run_context.trigger_reason,
        run_context.vault_tag,
        direct_mode.label(),
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
pub async fn p2p_sync_disconnect(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let transport = {
        let state = app_handle.state::<P2pSyncState>();
        let mut guard = state.lock().unwrap_or_else(|e| e.into_inner());
        guard.take() // This leaves None in the state and returns the Option
    };

    if let Some((_, t)) = transport {
        use crate::sync::SyncTransport;
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
pub fn p2p_sync_status(
    app_handle: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let state = app_handle.state::<P2pSyncState>();
    let guard = state.lock().unwrap_or_else(|e| e.into_inner());

    let connected = guard.is_some();
    let direct_mode = crate::p2p::direct_p2p_mode(&app_handle);
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
        "direct_p2p_mode": direct_mode.label(),
        "direct_p2p_protocol_version": crate::p2p::DIRECT_P2P_PROTOCOL_VERSION,
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
const COORDINATOR_URL: &str = "https://coordinator.synabit.net";

#[tauri::command]
pub async fn p2p_pair_initiate(app_handle: tauri::AppHandle) -> Result<PairingInfo, String> {
    let node_id_hex = {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        match db.get_kv("device_node_id") {
            Ok(Some(id)) if !id.is_empty() => id,
            _ => {
                let secret_key = iroh::SecretKey::generate();
                let id = hex::encode(secret_key.public().as_bytes());
                let _ = db.set_kv("device_node_id", &id);
                id
            }
        }
    };

    let device_name = local_device_name();
    let session = PairingSession::new(node_id_hex.clone(), device_name.clone());
    let info = session.pairing_info();

    let pairing_state = app_handle.state::<PairingState>();
    let mut state = pairing_state.lock().unwrap_or_else(|e| e.into_inner());
    *state = Some(session);

    let mdns_state = app_handle.try_state::<std::sync::Arc<crate::p2p::mdns::MdnsDiscovery>>();
    if let Some(mdns) = mdns_state {
        log::info!("mDNS state found, broadcasting pairing code: {}", info.code);
        mdns.update_pairing_code(11204, device_name.clone(), Some(info.code.clone()));
    } else {
        log::warn!("mDNS state NOT found — mDNS was not initialized");
    }

    let code_clone = info.code.clone();
    let code_normalized = crate::p2p::pairing::normalize_code(&code_clone);
    let node_id_clone = node_id_hex.clone();
    let device_name_clone = device_name.clone();
    let app_handle_clone = app_handle.clone();
    
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "code": code_normalized,
            "node_id_hex": node_id_clone,
            "device_name": device_name_clone,
        });
        
        if let Err(e) = client.post(format!("{}/pair/register", COORDINATOR_URL))
            .json(&body)
            .send()
            .await 
        {
            log::warn!("Failed to register with Coordinator: {}", e);
        } else {
            log::info!("Registered pairing code with Coordinator");
            
            for _ in 0..150 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                
                let still_active = {
                    let st = app_handle_clone.state::<PairingState>();
                    let x = if let Ok(l) = st.try_lock() {
                        l.is_some()
                    } else { false };
                    x
                };
                if !still_active { break; }
                
                if let Ok(res) = client.get(format!("{}/pair/poll/{}", COORDINATOR_URL, code_normalized)).send().await {
                    if res.status().is_success() {
                        if let Ok(Some(data)) = res.json::<Option<serde_json::Value>>().await {
                            if let (Some(acc_node), Some(acc_name)) = (data.get("acceptor_node_id_hex").and_then(|v| v.as_str()), data.get("acceptor_device_name").and_then(|v| v.as_str())) {
                                log::info!("Coordinator returned accept from {}", acc_name);
                                
                                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                                let device = PairedDevice {
                                    device_name: acc_name.to_string(),
                                    node_id_hex: acc_node.to_string(),
                                    paired_at: now,
                                    last_seen: now,
                                };
                                
                                if let Err(e) = DeviceRegistry::add(&app_handle_clone, device) {
                                    log::error!("Failed to save paired device: {}", e);
                                }
                                
                                use tauri::Emitter;
                                let _ = app_handle_clone.emit("pairing-accepted", crate::p2p::mdns::PairingAcceptedEvent {
                                    device_name: acc_name.to_string(),
                                    node_id_hex: acc_node.to_string(),
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    log::info!("Pairing initiated, code={}", info.code);
    Ok(info)
}

/// Cancel any active pairing session.
#[tauri::command]
pub fn p2p_pair_cancel(app_handle: tauri::AppHandle) -> Result<(), String> {
    let pairing_state = app_handle.state::<PairingState>();
    let mut state = pairing_state.lock().unwrap_or_else(|e| e.into_inner());
    *state = None;

    let mdns_state = app_handle.try_state::<std::sync::Arc<crate::p2p::mdns::MdnsDiscovery>>();
    let device_name = local_device_name();
    if let Some(mdns) = mdns_state {
        mdns.update_pairing_code(11204, device_name, None);
    }

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
pub async fn p2p_pair_accept(
    app_handle: tauri::AppHandle,
    code: String,
) -> Result<PairedDevice, String> {
    let normalized = crate::p2p::pairing::normalize_code(&code);
    crate::p2p::pairing::validate_code(&normalized).map_err(|e| e.to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut real_node_id = None;
    let mut peer_name = format!("Device-{}", &normalized[..4]);

    let coord_task = {
        let code_norm = normalized.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            if let Ok(res) = client.get(format!("{}/pair/lookup/{}", COORDINATOR_URL, code_norm)).send().await {
                if res.status().is_success() {
                    if let Ok(Some(data)) = res.json::<Option<serde_json::Value>>().await {
                        if let (Some(node), Some(name)) = (data.get("node_id_hex").and_then(|v| v.as_str()), data.get("device_name").and_then(|v| v.as_str())) {
                            return Some((node.to_string(), name.to_string()));
                        }
                    }
                }
            }
            None
        })
    };

    let discovery = app_handle.try_state::<std::sync::Arc<crate::p2p::discovery::PeerDiscovery>>();
    if let Some(disc) = discovery {
        for _ in 0..25 {
            for peer in disc.online_peers() {
                if let Some(ref c) = peer.pairing_code {
                    if crate::p2p::pairing::normalize_code(c) == normalized {
                        real_node_id = Some(hex::encode(peer.node_id.as_bytes()));
                        if let Some(n) = peer.device_name {
                            peer_name = n;
                        }
                        break;
                    }
                }
            }
            if real_node_id.is_some() { break; }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    if real_node_id.is_none() {
        if let Ok(Some((node, name))) = coord_task.await {
            log::info!("Found peer via Coordinator!");
            real_node_id = Some(node);
            peer_name = name;
        }
    } else {
        coord_task.abort();
        log::info!("Found peer via LAN mDNS!");
    }

    let node_id_hex = match real_node_id {
        Some(id) => id,
        None => return Err("No device found with this pairing code. Make sure the code is correct.".to_string()),
    };

    let device = PairedDevice {
        device_name: peer_name,
        node_id_hex: node_id_hex.clone(),
        paired_at: now,
        last_seen: now,
    };

    DeviceRegistry::add(&app_handle, device.clone()).map_err(|e| e.to_string())?;

    let my_device_name = local_device_name();
    
    let mdns_state = app_handle.try_state::<std::sync::Arc<crate::p2p::mdns::MdnsDiscovery>>();
    if let Some(mdns) = mdns_state {
        mdns.send_pair_accept(&node_id_hex, &my_device_name);
    }

    let code_norm = normalized.clone();
    let my_node_id = {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("device_node_id").unwrap_or_default().unwrap_or_default()
    };
    
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "code": code_norm,
            "acceptor_node_id_hex": my_node_id,
            "acceptor_device_name": my_device_name,
        });
        let _ = client.post(format!("{}/pair/accept", COORDINATOR_URL))
            .json(&body)
            .send()
            .await;
    });

    log::info!("Pairing accepted via code, device_name={}", device.device_name);
    Ok(device)
}

// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct PairedDeviceResponse {
    pub device_name: String,
    pub node_id_hex: String,
    pub paired_at: u64,
    pub last_seen: u64,
    pub is_online: bool,
}

/// List all paired devices.
#[tauri::command]
pub fn p2p_list_devices(app_handle: tauri::AppHandle) -> Result<Vec<PairedDeviceResponse>, String> {
    let devices = DeviceRegistry::list(&app_handle);
    
    // Check mDNS for online peers
    let mut online_nodes = std::collections::HashSet::new();
    if let Some(discovery) = app_handle.try_state::<std::sync::Arc<crate::p2p::discovery::PeerDiscovery>>() {
        for peer in discovery.online_peers() {
            online_nodes.insert(hex::encode(peer.node_id.as_bytes()));
        }
    }

    let response = devices.into_iter().map(|d| PairedDeviceResponse {
        is_online: online_nodes.contains(&d.node_id_hex),
        device_name: d.device_name,
        node_id_hex: d.node_id_hex,
        paired_at: d.paired_at,
        last_seen: d.last_seen,
    }).collect();

    Ok(response)
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
