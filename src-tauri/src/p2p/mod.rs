//! P2P module — Synabit sync transports and infrastructure.
//!
//! This module provides multiple sync transports:
//!
//! - `transport::SynabitServerTransport` — connects to the Synabit Sync Server
//!   over Iroh QUIC (primary sync path)
//! - `direct::DirectP2PTransport` — connects directly to a paired device
//!   for device-to-device sync (faster on LAN)
//!
//! Supporting infrastructure:
//!
//! - `endpoint::PersistentEndpoint` — long-lived Iroh endpoint with stable identity
//! - `handler::P2PSyncHandler` — accepts incoming P2P connections from paired devices
//! - `discovery::PeerDiscovery` — tracks known online peers
//! - `hybrid` — orchestrates sync across server + P2P peers

pub mod devices;
pub mod direct;
pub mod discovery;
pub mod endpoint;
pub mod handler;
pub mod hybrid;
pub mod pairing;
pub mod transport;
pub mod mdns;

use std::sync::Arc;
use tauri::Manager;

/// Initialize P2P managed state
pub fn init(app: &tauri::App) -> Result<(), String> {
    log::info!("P2P init starting...");
    
    let registry = Arc::new(discovery::PeerDiscovery::new());
    app.manage(registry.clone());
    
    let app_handle = app.handle().clone();
    
    tauri::async_runtime::spawn(async move {
        // 1. Start PersistentEndpoint
        let data_dir = app_handle.path().app_data_dir().unwrap_or_default();
        let endpoint = match endpoint::PersistentEndpoint::start(&data_dir).await {
            Ok(ep) => Arc::new(ep),
            Err(e) => {
                log::error!("Failed to start PersistentEndpoint: {}", e);
                return;
            }
        };
        
        let node_id_hex = hex::encode(endpoint.node_id().as_bytes());
        log::info!("PersistentEndpoint running with node_id_hex: {}", &node_id_hex[..16]);
        
        app_handle.manage(endpoint.clone());

        // Also save node_id to DB for backward compatibility / Coordinator usage
        {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let _ = db.set_kv("device_node_id", &node_id_hex);
        }

        // 2. Start P2PSyncHandler
        let handler = Arc::new(handler::P2PSyncHandler::new(app_handle.clone()));
        app_handle.manage(handler.clone());

        let ep_clone = endpoint.endpoint_cloned();
        tauri::async_runtime::spawn(async move {
            log::info!("P2PSyncHandler listening for incoming connections...");
            while let Some(incoming) = ep_clone.accept().await {
                let handler_ref = handler.clone();
                tauri::async_runtime::spawn(async move {
                    match incoming.accept() {
                        Ok(accepting) => {
                            match accepting.await {
                                Ok(conn) => {
                                    let alpn = conn.alpn();
                                    if alpn == handler::P2P_SYNC_ALPN {
                                        handler_ref.handle_connection(conn).await;
                                    } else {
                                        log::warn!("Rejected connection with unknown ALPN: {:?}", std::str::from_utf8(alpn).unwrap_or("invalid utf8"));
                                    }
                                }
                                Err(e) => log::warn!("Failed to establish connection: {}", e),
                            }
                        }
                        Err(e) => log::warn!("Failed to accept incoming connection: {}", e),
                    }
                });
            }
        });

        // 3. Start mDNS
        let device_name = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "Desktop".to_string());
        
        log::info!("P2P mDNS init: node_id={}, device_name={}", &node_id_hex[..16], device_name);
        
        match mdns::MdnsDiscovery::start(node_id_hex, 11204, device_name, registry, app_handle.clone()) {
            Ok(mdns) => {
                log::info!("mDNS started successfully");
                app_handle.manage(Arc::new(mdns));
            }
            Err(e) => {
                log::error!("mDNS failed to start: {}", e);
            }
        }
    });
    
    Ok(())
}
