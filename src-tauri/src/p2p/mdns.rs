//! LAN Discovery via UDP Broadcast
//!
//! Broadcasts this device's Iroh EndpointId over the local network using
//! simple UDP broadcast packets on a custom port (not 5353).
//! Listens for other Synabit devices and records their local IP addresses
//! for direct P2P connections.
//!
//! This avoids the macOS mDNSResponder conflict on port 5353 that breaks
//! the `mdns-sd` crate.

use std::net::{SocketAddr, UdpSocket, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use log::info;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::p2p::discovery::{PeerDiscovery, PeerInfo};
use crate::p2p::devices::DeviceRegistry;
use crate::p2p::pairing::PairedDevice;

/// Custom port for Synabit LAN discovery (avoids 5353 conflict)
const DISCOVERY_PORT: u16 = 41732;
/// How often to broadcast presence (seconds)
const BROADCAST_INTERVAL_SECS: u64 = 3;

/// A discovery announcement sent over UDP broadcast
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryPacket {
    /// Protocol identifier: "synabit-lan-v1" for discovery, "synabit-pair-accept" for pair confirmation
    proto: String,
    /// Hex-encoded PublicKey (64 chars)
    node_id_hex: String,
    /// Device name
    device_name: String,
    /// Optional pairing code (only set when pairing is active)
    pairing_code: Option<String>,
    /// Port the Iroh endpoint listens on
    iroh_port: u16,
}

/// Event emitted to frontend when a remote device accepts our pairing code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingAcceptedEvent {
    pub device_name: String,
    pub node_id_hex: String,
}

/// Manages LAN discovery broadcasting and listening.
pub struct MdnsDiscovery {
    my_node_id_hex: String,
    /// Shared state for updating the pairing code at runtime
    shared_state: Arc<std::sync::Mutex<SharedDiscoveryState>>,
    /// Handle to stop the broadcast thread
    _running: Arc<std::sync::atomic::AtomicBool>,
}

struct SharedDiscoveryState {
    device_name: String,
    pairing_code: Option<String>,
    iroh_port: u16,
}

impl MdnsDiscovery {
    /// Start the LAN discovery service.
    pub fn start(
        node_id_hex: String,
        port: u16,
        device_name: String,
        registry: Arc<PeerDiscovery>,
        app_handle: tauri::AppHandle,
    ) -> Result<Self, String> {
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        
        let shared_state = Arc::new(std::sync::Mutex::new(SharedDiscoveryState {
            device_name: device_name.clone(),
            pairing_code: None,
            iroh_port: port,
        }));

        // --- Listener thread ---
        let registry_clone = registry.clone();
        let my_id = node_id_hex.clone();
        let running_l = running.clone();
        let app_handle_l = app_handle.clone();
        
        std::thread::Builder::new()
            .name("lan-discovery-listener".into())
            .spawn(move || {
                info!("LAN Discovery listener starting on port {}...", DISCOVERY_PORT);
                
                let socket = match UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Failed to bind discovery listener on port {}: {}", DISCOVERY_PORT, e);
                        return;
                    }
                };
                
                // Non-blocking so we can check `running` flag
                socket.set_read_timeout(Some(Duration::from_secs(2))).ok();
                
                info!("LAN Discovery listener ready on port {}", DISCOVERY_PORT);
                
                let mut buf = [0u8; 2048];
                while running_l.load(std::sync::atomic::Ordering::Relaxed) {
                    match socket.recv_from(&mut buf) {
                        Ok((len, src_addr)) => {
                            if let Ok(packet) = serde_json::from_slice::<DiscoveryPacket>(&buf[..len]) {
                                // Ignore our own broadcasts
                                if packet.node_id_hex == my_id {
                                    continue;
                                }
                                
                                if packet.proto == "synabit-pair-accept" {
                                    // A remote device accepted our pairing code!
                                    info!("LAN Discovery: PAIR ACCEPTED by {} ({})", 
                                        packet.device_name,
                                        &packet.node_id_hex[..std::cmp::min(16, packet.node_id_hex.len())]);
                                    
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    
                                    let device = PairedDevice {
                                        device_name: packet.device_name.clone(),
                                        node_id_hex: packet.node_id_hex.clone(),
                                        paired_at: now,
                                        last_seen: now,
                                    };
                                    
                                    if let Err(e) = DeviceRegistry::add(&app_handle_l, device) {
                                        log::error!("Failed to save paired device: {}", e);
                                    }
                                    
                                    // Emit event to frontend so it can dismiss the waiting screen
                                    let event = PairingAcceptedEvent {
                                        device_name: packet.device_name,
                                        node_id_hex: packet.node_id_hex,
                                    };
                                    let _ = app_handle_l.emit("pairing-accepted", event);
                                    
                                    continue;
                                }
                                
                                if packet.proto != "synabit-lan-v1" {
                                    continue;
                                }
                                
                                // Parse the node_id
                                let node_id = match parse_public_key(&packet.node_id_hex) {
                                    Ok(id) => id,
                                    Err(e) => {
                                        info!("LAN Discovery: ignoring invalid node_id from {}: {}", src_addr, e);
                                        continue;
                                    }
                                };
                                
                                let lan_address = Some(SocketAddr::new(src_addr.ip(), packet.iroh_port));
                                
                                info!("LAN Discovery: found peer {} ({}) at {:?}, code={:?}", 
                                    &packet.node_id_hex[..std::cmp::min(16, packet.node_id_hex.len())],
                                    packet.device_name,
                                    lan_address,
                                    packet.pairing_code);
                                
                                let peer_info = PeerInfo {
                                    node_id,
                                    is_lan: true,
                                    last_seen: unix_now(),
                                    lan_address,
                                    device_name: Some(packet.device_name),
                                    pairing_code: packet.pairing_code,
                                };
                                
                                registry_clone.add_peer(peer_info);
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || 
                                      e.kind() == std::io::ErrorKind::TimedOut => {
                            continue;
                        }
                        Err(e) => {
                            info!("LAN Discovery listener error: {}", e);
                        }
                    }
                }
                info!("LAN Discovery listener stopped");
            })
            .map_err(|e| format!("Failed to spawn listener thread: {}", e))?;

        // --- Broadcaster thread ---
        let my_id2 = node_id_hex.clone();
        let shared2 = shared_state.clone();
        let running_b = running.clone();
        
        std::thread::Builder::new()
            .name("lan-discovery-broadcast".into())
            .spawn(move || {
                info!("LAN Discovery broadcaster starting...");
                
                let socket = match UdpSocket::bind("0.0.0.0:0") {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Failed to create broadcast socket: {}", e);
                        return;
                    }
                };
                socket.set_broadcast(true).ok();
                
                let broadcast_addr = SocketAddr::new(
                    std::net::IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
                    DISCOVERY_PORT,
                );
                
                info!("LAN Discovery broadcaster ready, broadcasting to {:?}", broadcast_addr);
                
                while running_b.load(std::sync::atomic::Ordering::Relaxed) {
                    let (device_name, pairing_code, iroh_port) = {
                        let state = shared2.lock().unwrap();
                        (state.device_name.clone(), state.pairing_code.clone(), state.iroh_port)
                    };
                    
                    let packet = DiscoveryPacket {
                        proto: "synabit-lan-v1".to_string(),
                        node_id_hex: my_id2.clone(),
                        device_name,
                        pairing_code,
                        iroh_port,
                    };
                    
                    if let Ok(data) = serde_json::to_vec(&packet) {
                        match socket.send_to(&data, broadcast_addr) {
                            Ok(_) => {}
                            Err(e) => {
                                info!("LAN Discovery broadcast failed: {}", e);
                            }
                        }
                    }
                    
                    std::thread::sleep(Duration::from_secs(BROADCAST_INTERVAL_SECS));
                }
                info!("LAN Discovery broadcaster stopped");
            })
            .map_err(|e| format!("Failed to spawn broadcaster thread: {}", e))?;

        let my_ip = get_local_ip().unwrap_or_else(|| "0.0.0.0".to_string());
        info!("LAN Discovery started: node={} ip={} port={}", 
            &node_id_hex[..std::cmp::min(16, node_id_hex.len())], my_ip, port);

        Ok(Self {
            my_node_id_hex: node_id_hex,
            shared_state,
            _running: running,
        })
    }
    
    /// Update the broadcasted pairing code (used when initiating pairing)
    pub fn update_pairing_code(&self, port: u16, device_name: String, code: Option<String>) {
        if let Ok(mut state) = self.shared_state.lock() {
            state.pairing_code = code.clone();
            state.device_name = device_name;
            state.iroh_port = port;
        }
        if code.is_some() {
            info!("LAN Discovery: broadcasting pairing code");
        } else {
            info!("LAN Discovery: cleared pairing code");
        }
    }
    
    /// Send a pairing acceptance packet directly to a peer's IP.
    /// Called by Machine A after it successfully pairs with Machine B.
    pub fn send_pair_accept(&self, peer_addr: SocketAddr, device_name: &str) {
        let packet = DiscoveryPacket {
            proto: "synabit-pair-accept".to_string(),
            node_id_hex: self.my_node_id_hex.clone(),
            device_name: device_name.to_string(),
            pairing_code: None,
            iroh_port: 11204,
        };
        
        if let Ok(data) = serde_json::to_vec(&packet) {
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to create socket for pair_accept: {}", e);
                    return;
                }
            };
            
            // Send to the peer's discovery port (not iroh port)
            let target = SocketAddr::new(peer_addr.ip(), DISCOVERY_PORT);
            
            // Send multiple times for reliability
            for _ in 0..3 {
                match socket.send_to(&data, target) {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Failed to send pair_accept: {}", e);
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            
            info!("LAN Discovery: sent pair_accept to {}", target);
        }
    }
}

/// Helper to get local IP address
fn get_local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn parse_public_key(hex_str: &str) -> Result<iroh::PublicKey, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex: {}", e))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "key must be exactly 32 bytes".to_string())?;
    iroh::PublicKey::try_from(&bytes[..])
        .map_err(|e| format!("invalid public key: {}", e))
}
