//! mDNS Local LAN Discovery
//!
//! Broadcasts this device's Iroh EndpointId over the local network using mDNS.
//! Listens for other Synabit devices and records their local IP addresses for direct P2P connections.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, warn};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::p2p::discovery::{PeerDiscovery, PeerInfo};

pub const MDNS_SERVICE_TYPE: &str = "_synabit._udp.local.";

/// Manages mDNS broadcasting and browsing.
pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    my_node_id_hex: String,
}

impl MdnsDiscovery {
    /// Start the mDNS discovery service.
    ///
    /// It broadcasts the local device's presence and listens for other devices.
    pub fn start(
        node_id_hex: String,
        port: u16,
        device_name: String,
        registry: Arc<PeerDiscovery>,
    ) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| format!("Failed to start mDNS: {}", e))?;

        // 1. Get local IP to broadcast
        let my_ip = get_local_ip().unwrap_or_else(|| "0.0.0.0".to_string());
        
        // 2. Set up properties (TXT records)
        let mut properties = HashMap::new();
        properties.insert("device_name".to_string(), device_name);
        
        let host_name = format!("{}.local.", node_id_hex);

        let service_info = ServiceInfo::new(
            MDNS_SERVICE_TYPE,
            &node_id_hex,
            &host_name,
            &my_ip,
            port,
            Some(properties),
        );

        // 3. Register our service
        daemon
            .register(service_info)
            .map_err(|e| format!("Failed to register mDNS service: {}", e))?;
            
        info!("mDNS Broadcast started: {} at {}:{}", node_id_hex, my_ip, port);

        // 4. Start browsing for other devices
        let receiver = daemon
            .browse(MDNS_SERVICE_TYPE)
            .map_err(|e| format!("Failed to browse mDNS: {}", e))?;

        let my_id = node_id_hex.clone();
        
        tauri::async_runtime::spawn(async move {
            info!("mDNS Browser task started...");
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let peer_id_hex = info.get_fullname().split('.').next().unwrap_or("").to_string();
                        
                        // Ignore ourselves
                        if peer_id_hex == my_id || peer_id_hex.is_empty() {
                            continue;
                        }

                        // Try to parse the hex ID into a PublicKey
                        let node_id = match parse_server_id(&peer_id_hex) {
                            Ok(id) => id,
                            Err(e) => {
                                debug!("mDNS: Ignore invalid NodeId {}: {}", peer_id_hex, e);
                                continue;
                            }
                        };

                        let ips = info.get_addresses();
                        let port = info.get_port();
                        
                        let lan_address = ips.iter().next()
                            .and_then(|ip| format!("{}:{}", ip, port).parse::<SocketAddr>().ok());
                        
                        let device_name = info.get_properties().get("device_name")
                            .map(|v| v.to_string());
                            
                        let pairing_code = info.get_properties().get("pairing_code")
                            .map(|v| v.to_string());

                        let peer_info = PeerInfo {
                            node_id,
                            is_lan: true,
                            last_seen: unix_now(),
                            lan_address,
                            device_name,
                            pairing_code,
                        };

                        info!("mDNS Discovered Peer: {} at {:?}", peer_id_hex, lan_address);
                        registry.add_peer(peer_info);
                    }
                    ServiceEvent::ServiceRemoved(service_type, fullname) => {
                        let peer_id_hex = fullname.split('.').next().unwrap_or("").to_string();
                        debug!("mDNS Service removed: {}", peer_id_hex);
                        // Optionally prune from registry, but we usually keep it until it's stale
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            daemon,
            my_node_id_hex: node_id_hex,
        })
    }
    
    /// Update the broadcasted pairing code (used when initiating pairing)
    pub fn update_pairing_code(&self, port: u16, device_name: String, code: Option<String>) {
        let my_ip = get_local_ip().unwrap_or_else(|| "0.0.0.0".to_string());
        
        let mut properties = HashMap::new();
        properties.insert("device_name".to_string(), device_name);
        if let Some(c) = code {
            properties.insert("pairing_code".to_string(), c);
            info!("mDNS broadcasting pairing code...");
        }
        
        let host_name = format!("{}.local.", self.my_node_id_hex);
        
        let service_info = ServiceInfo::new(
            MDNS_SERVICE_TYPE,
            &self.my_node_id_hex,
            &host_name,
            &my_ip,
            port,
            Some(properties),
        );
        
        // Force peers to re-resolve by unregistering first
        let fullname = format!("{}.{}", self.my_node_id_hex, MDNS_SERVICE_TYPE);
        let _ = self.daemon.unregister(&fullname);
        
        std::thread::sleep(std::time::Duration::from_millis(50));
        
        let _ = self.daemon.register(service_info);
    }
}

/// Helper to get local IP address using a dummy UDP connection
fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_server_id(hex_str: &str) -> Result<iroh::PublicKey, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex: {}", e))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "server ID must be exactly 32 bytes".to_string())?;
    iroh::PublicKey::try_from(&bytes[..])
        .map_err(|e| format!("invalid server public key: {}", e))
}
