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
    let registry = Arc::new(discovery::PeerDiscovery::new());
    app.manage(registry.clone());
    
    // Attempt to start mDNS
    let node_id_hex = {
        let db_state = app.state::<crate::db::DbState>();
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
    
    let device_name = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "Desktop".to_string());
        
    if node_id_hex != "unknown" {
        // Use a default port for now until PersistentEndpoint is wired
        if let Ok(mdns) = mdns::MdnsDiscovery::start(node_id_hex, 11204, device_name, registry) {
            app.manage(Arc::new(mdns));
        }
    }
    
    Ok(())
}
