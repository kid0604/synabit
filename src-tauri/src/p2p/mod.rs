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
