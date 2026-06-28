//! LAN Discovery via UDP Broadcast
//!
//! Uses a SINGLE UDP socket for both broadcasting and listening on port 41732.
//! This is critical for macOS: when an app sends FROM a port, the firewall
//! automatically allows incoming traffic ON that same port. Using separate
//! sockets (one for send, one for receive) causes the receiver to be blocked
//! by macOS firewall on unsigned apps.

use std::net::{SocketAddr, UdpSocket, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use log::info;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::p2p::discovery::{PeerDiscovery, PeerInfo};
use crate::p2p::devices::DeviceRegistry;
use crate::p2p::pairing::PairedDevice;

/// Custom port for Synabit LAN discovery
const DISCOVERY_PORT: u16 = 41732;
/// How often to broadcast presence (seconds)
const BROADCAST_INTERVAL_SECS: u64 = 2;

/// A discovery packet sent over UDP broadcast
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiscoveryPacket {
    /// "synabit-lan-v1" for discovery, "synabit-pair-accept" for pair confirmation
    proto: String,
    /// Hex-encoded PublicKey (64 chars)
    node_id_hex: String,
    /// Device name
    device_name: String,
    /// Optional pairing code
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
    shared_state: Arc<std::sync::Mutex<SharedDiscoveryState>>,
    _running: Arc<std::sync::atomic::AtomicBool>,
}

struct SharedDiscoveryState {
    device_name: String,
    pairing_code: Option<String>,
    iroh_port: u16,
    /// Queue of pair_accept packets to send (peer_addr, device_name)
    pair_accept_queue: Vec<(SocketAddr, String)>,
}

impl MdnsDiscovery {
    /// Start the LAN discovery service.
    ///
    /// Uses a SINGLE socket bound to DISCOVERY_PORT for both sending and receiving.
    /// This ensures macOS firewall allows incoming packets since we also send from this port.
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
            pair_accept_queue: Vec::new(),
        }));

        let my_id = node_id_hex.clone();
        let shared = shared_state.clone();
        let running_t = running.clone();
        
        std::thread::Builder::new()
            .name("lan-discovery".into())
            .spawn(move || {
                info!("LAN Discovery starting on port {}...", DISCOVERY_PORT);
                
                // Single socket for both send and receive
                let socket = match UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Failed to bind discovery socket on port {}: {}", DISCOVERY_PORT, e);
                        return;
                    }
                };
                
                socket.set_broadcast(true)
                    .unwrap_or_else(|e| log::error!("Failed to set broadcast: {}", e));
                // Short timeout so we alternate between send and receive
                socket.set_read_timeout(Some(Duration::from_millis(500))).ok();
                
                let broadcast_addr = SocketAddr::new(
                    std::net::IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
                    DISCOVERY_PORT,
                );
                
                info!("LAN Discovery ready: send+recv on port {}", DISCOVERY_PORT);
                
                let mut buf = [0u8; 2048];
                let mut last_broadcast = std::time::Instant::now() - Duration::from_secs(10); // broadcast immediately
                
                while running_t.load(std::sync::atomic::Ordering::Relaxed) {
                    // --- SEND phase: broadcast if interval elapsed ---
                    if last_broadcast.elapsed() >= Duration::from_secs(BROADCAST_INTERVAL_SECS) {
                        let (dev_name, pairing_code, iroh_port, pair_accepts) = {
                            let mut state = shared.lock().unwrap();
                            let accepts = std::mem::take(&mut state.pair_accept_queue);
                            (state.device_name.clone(), state.pairing_code.clone(), state.iroh_port, accepts)
                        };
                        
                        // Send discovery broadcast
                        let packet = DiscoveryPacket {
                            proto: "synabit-lan-v1".to_string(),
                            node_id_hex: my_id.clone(),
                            device_name: dev_name,
                            pairing_code,
                            iroh_port,
                        };
                        
                        if let Ok(data) = serde_json::to_vec(&packet) {
                            let _ = socket.send_to(&data, broadcast_addr);
                        }
                        
                        // Send any queued pair_accept packets
                        for (peer_addr, dev_name) in pair_accepts {
                            let accept_packet = DiscoveryPacket {
                                proto: "synabit-pair-accept".to_string(),
                                node_id_hex: my_id.clone(),
                                device_name: dev_name,
                                pairing_code: None,
                                iroh_port: 11204,
                            };
                            if let Ok(data) = serde_json::to_vec(&accept_packet) {
                                // Send multiple times for reliability
                                let target = SocketAddr::new(peer_addr.ip(), DISCOVERY_PORT);
                                for _ in 0..3 {
                                    let _ = socket.send_to(&data, target);
                                    std::thread::sleep(Duration::from_millis(50));
                                }
                                // Also broadcast it so even if unicast is blocked, peer sees it
                                let _ = socket.send_to(&data, broadcast_addr);
                                info!("LAN Discovery: sent pair_accept to {} + broadcast", target);
                            }
                        }
                        
                        last_broadcast = std::time::Instant::now();
                    }
                    
                    // --- RECV phase: listen for incoming packets ---
                    match socket.recv_from(&mut buf) {
                        Ok((len, src_addr)) => {
                            if let Ok(packet) = serde_json::from_slice::<DiscoveryPacket>(&buf[..len]) {
                                // Ignore our own packets
                                if packet.node_id_hex == my_id {
                                    continue;
                                }
                                
                                if packet.proto == "synabit-pair-accept" {
                                    info!("LAN Discovery: PAIR ACCEPTED by {} ({})", 
                                        packet.device_name,
                                        &packet.node_id_hex[..std::cmp::min(16, packet.node_id_hex.len())]);
                                    
                                    let now = unix_now();
                                    let device = PairedDevice {
                                        device_name: packet.device_name.clone(),
                                        node_id_hex: packet.node_id_hex.clone(),
                                        paired_at: now,
                                        last_seen: now,
                                    };
                                    
                                    if let Err(e) = DeviceRegistry::add(&app_handle, device) {
                                        log::error!("Failed to save paired device: {}", e);
                                    }
                                    
                                    let event = PairingAcceptedEvent {
                                        device_name: packet.device_name,
                                        node_id_hex: packet.node_id_hex,
                                    };
                                    let _ = app_handle.emit("pairing-accepted", event);
                                    continue;
                                }
                                
                                if packet.proto != "synabit-lan-v1" {
                                    continue;
                                }
                                
                                // Parse node_id
                                let node_id = match parse_public_key(&packet.node_id_hex) {
                                    Ok(id) => id,
                                    Err(_) => continue,
                                };
                                
                                let lan_address = Some(SocketAddr::new(src_addr.ip(), packet.iroh_port));
                                
                                info!("LAN Discovery: peer {} ({}) at {:?} code={:?}", 
                                    &packet.node_id_hex[..std::cmp::min(16, packet.node_id_hex.len())],
                                    packet.device_name, lan_address, packet.pairing_code);
                                
                                registry.add_peer(PeerInfo {
                                    node_id,
                                    is_lan: true,
                                    last_seen: unix_now(),
                                    lan_address,
                                    device_name: Some(packet.device_name),
                                    pairing_code: packet.pairing_code,
                                });
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || 
                                      e.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(e) => {
                            info!("LAN Discovery recv error: {}", e);
                        }
                    }
                }
                info!("LAN Discovery stopped");
            })
            .map_err(|e| format!("Failed to spawn discovery thread: {}", e))?;

        let my_ip = get_local_ip().unwrap_or_else(|| "0.0.0.0".to_string());
        info!("LAN Discovery started: node={} ip={} port={}", 
            &node_id_hex[..std::cmp::min(16, node_id_hex.len())], my_ip, port);

        Ok(Self {
            my_node_id_hex: node_id_hex,
            shared_state,
            _running: running,
        })
    }
    
    /// Update the broadcasted pairing code
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
    
    /// Queue a pairing acceptance packet to be sent to a peer
    pub fn send_pair_accept(&self, peer_addr: SocketAddr, device_name: &str) {
        if let Ok(mut state) = self.shared_state.lock() {
            state.pair_accept_queue.push((peer_addr, device_name.to_string()));
        }
        info!("LAN Discovery: queued pair_accept for {}", peer_addr);
    }
}

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
