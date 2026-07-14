//! LAN Discovery via True mDNS (Bonjour/ZeroConf)
//!
//! Uses mdns-sd crate to register and browse services on the local network.
//! This seamlessly bypasses VPN/Docker default-route hijacking.

use std::sync::Arc;
use std::time::Duration;

use log::info;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::p2p::discovery::{PeerDiscovery, PeerInfo};
use crate::p2p::devices::DeviceRegistry;
use crate::p2p::pairing::PairedDevice;

const SERVICE_TYPE_DISCOVERY: &str = "_synabit._udp.local.";
const SERVICE_TYPE_ACCEPT: &str = "_synabit-accept._udp.local.";

/// Event emitted to frontend when a remote device accepts our pairing code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingAcceptedEvent {
    pub device_name: String,
    pub node_id_hex: String,
}

pub struct MdnsDiscovery {
    my_node_id_hex: String,
    shared_state: Arc<std::sync::Mutex<SharedDiscoveryState>>,
    _running: Arc<std::sync::atomic::AtomicBool>,
    daemon: ServiceDaemon,
}

struct SharedDiscoveryState {
    device_name: String,
    pairing_code: Option<String>,
    iroh_port: u16,
    daemon: ServiceDaemon,
    my_node_id_hex: String,
    active_service_fullname: Option<String>,
}

impl SharedDiscoveryState {
    fn register_service(&mut self, code: Option<String>) -> Result<(), String> {
        if let Some(ref fullname) = self.active_service_fullname {
            let _ = self.daemon.unregister(fullname);
        }

        // Create a safe string for instance name
        let safe_name = self.device_name.replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        let suffix = if code.is_some() { "pairing" } else { "idle" };
        let instance_name = format!("{}-{}-{}", safe_name, &self.my_node_id_hex[..8], suffix);
        let host_name = format!("{}.local.", instance_name);
        
        let mut props = vec![
            ("node_id".to_string(), self.my_node_id_hex.clone()),
            ("device_name".to_string(), self.device_name.clone()),
            ("iroh_port".to_string(), self.iroh_port.to_string()),
        ];
        
        if let Some(c) = code {
            props.push(("pairing_code".to_string(), c));
        }

        let service_info = ServiceInfo::new(
            SERVICE_TYPE_DISCOVERY,
            &instance_name,
            &host_name,
            "",
            self.iroh_port,
            &props[..],
        ).map_err(|e| e.to_string())?;

        self.active_service_fullname = Some(service_info.get_fullname().to_string());
        self.daemon.register(service_info).map_err(|e| e.to_string())?;

        Ok(())
    }
}

impl MdnsDiscovery {
    pub fn start(
        node_id_hex: String,
        port: u16,
        device_name: String,
        registry: Arc<PeerDiscovery>,
        app_handle: tauri::AppHandle,
    ) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| format!("Failed to start mDNS daemon: {}", e))?;
        
        let shared_state = Arc::new(std::sync::Mutex::new(SharedDiscoveryState {
            device_name: device_name.clone(),
            pairing_code: None,
            iroh_port: port,
            daemon: daemon.clone(),
            my_node_id_hex: node_id_hex.clone(),
            active_service_fullname: None,
        }));

        let my_id = node_id_hex.clone();
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_t = running.clone();
        
        // Start browsing discovery
        let receiver_discovery = daemon.browse(SERVICE_TYPE_DISCOVERY).map_err(|e| e.to_string())?;
        // Start browsing accept
        let receiver_accept = daemon.browse(SERVICE_TYPE_ACCEPT).map_err(|e| e.to_string())?;

        let registry_clone = registry.clone();
        let my_id_clone = my_id.clone();
        let app_handle_clone = app_handle.clone();
        
        std::thread::spawn(move || {
            info!("LAN mDNS Discovery started!");
            while running_t.load(std::sync::atomic::Ordering::Relaxed) {
                while let Ok(event) = receiver_discovery.try_recv() {
                    if let ServiceEvent::ServiceResolved(info) = event {
                        let props = info.get_properties();
                        let node_id = props.get_property_val_str("node_id").unwrap_or("").to_string();
                        let name = props.get_property_val_str("device_name").unwrap_or("").to_string();
                        let code = props.get_property_val_str("pairing_code").map(|s| s.to_string());
                        let p = props.get_property_val_str("iroh_port").and_then(|s| s.parse::<u16>().ok()).unwrap_or(11204);
                        
                        info!("LAN Discovery: Found service name={}, node_id={:?}, code={:?}", name, node_id, code);

                        if !node_id.is_empty() && node_id != my_id_clone {
                            match parse_public_key(&node_id) {
                                Ok(pub_key) => {
                                    let lan_addr = info.get_addresses_v4().iter().next().map(|ip| std::net::SocketAddr::new(std::net::IpAddr::V4(*ip), p));
                                    
                                    registry_clone.add_peer(PeerInfo {
                                        node_id: pub_key,
                                        is_lan: true,
                                        lan_address: lan_addr,
                                        pairing_code: code.clone(),
                                        device_name: Some(name.clone()),
                                        last_seen: unix_now(),
                                    });
                                    info!("LAN Discovery: Added peer {} with code {:?}", name, code);
                                }
                                Err(e) => {
                                    log::error!("LAN Discovery: Failed to parse node_id '{}': {}", node_id, e);
                                }
                            }
                        }
                    }
                }

                while let Ok(event) = receiver_accept.try_recv() {
                    if let ServiceEvent::ServiceResolved(info) = event {
                        let props = info.get_properties();
                        let node_id = props.get_property_val_str("node_id").unwrap_or("").to_string();
                        let target_node = props.get_property_val_str("target_node_id").unwrap_or("").to_string();
                        let name = props.get_property_val_str("device_name").unwrap_or("").to_string();
                        
                        if target_node == my_id_clone && !node_id.is_empty() && node_id != my_id_clone {
                            info!("LAN Discovery: PAIR ACCEPTED by {} ({})", name, &node_id[..std::cmp::min(16, node_id.len())]);
                            let now = unix_now();
                            let device = PairedDevice {
                                device_name: name.clone(),
                                node_id_hex: node_id.clone(),
                                paired_at: now,
                                last_seen: now,
                            };
                            
                            if let Err(e) = DeviceRegistry::add(&app_handle_clone, device) {
                                log::error!("Failed to save paired device: {}", e);
                            }
                            
                            let _ = app_handle_clone.emit("pairing-accepted", PairingAcceptedEvent {
                                device_name: name,
                                node_id_hex: node_id,
                            });
                        }
                    }
                }

                std::thread::sleep(Duration::from_millis(100));
            }
        });

        // Register default discovery service
        let _ = shared_state.lock().unwrap().register_service(None);

        Ok(Self {
            my_node_id_hex: node_id_hex,
            shared_state,
            _running: running,
            daemon,
        })
    }

    pub fn update_pairing_code(&self, port: u16, device_name: String, code: Option<String>) {
        if let Ok(mut state) = self.shared_state.lock() {
            state.pairing_code = code.clone();
            state.device_name = device_name.clone();
            state.iroh_port = port;
            let _ = state.register_service(code.clone());
        }
        info!("LAN Discovery: updated pairing code to {:?}", code);
    }

    pub fn send_pair_accept(&self, target_node_id: &str, device_name: &str) {
        let safe_name = device_name.replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        let instance_name = format!("{}-accept-{}", safe_name, &self.my_node_id_hex[..8]);
        let host_name = format!("{}.local.", instance_name);
        
        let props = vec![
            ("node_id".to_string(), self.my_node_id_hex.clone()),
            ("device_name".to_string(), device_name.to_string()),
            ("target_node_id".to_string(), target_node_id.to_string()),
        ];
        
        if let Ok(service_info) = ServiceInfo::new(
            SERVICE_TYPE_ACCEPT,
            &instance_name,
            &host_name,
            "",
            11204,
            &props[..],
        ) {
            let daemon = self.daemon.clone();
            let fullname = service_info.get_fullname().to_string();
            
            let _ = daemon.register(service_info);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(5));
                let _ = daemon.unregister(&fullname);
            });
            info!("LAN Discovery: registered pair_accept for {}", target_node_id);
        }
    }
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
