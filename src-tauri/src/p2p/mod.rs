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

/// Direct protocol currently implemented by `direct` + `handler`.
/// Phase 0 deliberately keeps v1 behind a debug-only unsafe override.
pub const DIRECT_P2P_PROTOCOL_VERSION: u16 = 1;
pub const DIRECT_P2P_V2_FEATURE_KEY: &str = "feature:direct_p2p_v2";
pub const UNSAFE_DIRECT_P2P_V1_ENV: &str = "SYNABIT_ENABLE_UNSAFE_DIRECT_P2P_V1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectP2PMode {
    Disabled,
    V2,
    UnsafeLegacyV1,
}

impl DirectP2PMode {
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::V2 => "v2",
            Self::UnsafeLegacyV1 => "unsafe_legacy_v1",
        }
    }
}

fn parse_bool_flag(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

fn resolve_direct_p2p_mode(
    v2_requested: bool,
    protocol_version: u16,
    debug_build: bool,
    unsafe_v1_requested: bool,
) -> DirectP2PMode {
    if v2_requested && protocol_version >= 2 {
        DirectP2PMode::V2
    } else if debug_build && unsafe_v1_requested {
        DirectP2PMode::UnsafeLegacyV1
    } else {
        DirectP2PMode::Disabled
    }
}

/// Resolve the direct-sync rollout gate for both outgoing and incoming paths.
///
/// Release builds cannot enable protocol v1. The environment override exists
/// only so maintainers can run the Phase 0 two-device regression harness.
pub fn direct_p2p_mode(app_handle: &tauri::AppHandle) -> DirectP2PMode {
    let v2_requested = {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv(DIRECT_P2P_V2_FEATURE_KEY)
            .ok()
            .flatten()
            .as_deref()
            .map(|value| parse_bool_flag(Some(value)))
            .unwrap_or(false)
    };
    let unsafe_v1_requested = std::env::var(UNSAFE_DIRECT_P2P_V1_ENV)
        .ok()
        .as_deref()
        .map(|value| parse_bool_flag(Some(value)))
        .unwrap_or(false);

    let mode = resolve_direct_p2p_mode(
        v2_requested,
        DIRECT_P2P_PROTOCOL_VERSION,
        cfg!(debug_assertions),
        unsafe_v1_requested,
    );

    if v2_requested && DIRECT_P2P_PROTOCOL_VERSION < 2 {
        log::warn!(
            "Direct P2P v2 feature requested but protocol v{} is the only implementation; direct sync remains {}",
            DIRECT_P2P_PROTOCOL_VERSION,
            mode.label(),
        );
    }

    mode
}

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

#[cfg(test)]
mod rollout_tests {
    use super::*;

    #[test]
    fn release_build_never_enables_legacy_v1() {
        assert_eq!(
            resolve_direct_p2p_mode(false, 1, false, true),
            DirectP2PMode::Disabled,
        );
    }

    #[test]
    fn v2_requires_both_flag_and_protocol_support() {
        assert_eq!(
            resolve_direct_p2p_mode(true, 1, false, false),
            DirectP2PMode::Disabled,
        );
        assert_eq!(
            resolve_direct_p2p_mode(true, 2, false, false),
            DirectP2PMode::V2,
        );
    }

    #[test]
    fn debug_build_can_opt_into_legacy_reproduction() {
        assert_eq!(
            resolve_direct_p2p_mode(false, 1, true, true),
            DirectP2PMode::UnsafeLegacyV1,
        );
    }

    #[test]
    fn bool_flag_parser_is_explicit() {
        assert!(parse_bool_flag(Some("true")));
        assert!(parse_bool_flag(Some(" 1 ")));
        assert!(!parse_bool_flag(Some("enabled")));
        assert!(!parse_bool_flag(None));
    }
}
