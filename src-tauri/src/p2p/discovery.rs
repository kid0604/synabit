//! Peer discovery for P2P sync.
//!
//! Tracks known online peers and their connection information. Currently
//! a manual registry — actual mDNS/LAN discovery will be added in a
//! future milestone.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let discovery = PeerDiscovery::new();
//! discovery.add_peer(PeerInfo {
//!     node_id: peer_public_key,
//!     is_lan: true,
//!     last_seen: now_secs,
//! });
//!
//! for peer in discovery.online_peers() {
//!     // connect and sync
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Information about a known peer device.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// The peer's Iroh public key / EndpointId.
    pub node_id: iroh::PublicKey,
    /// Whether the peer was discovered on the local network.
    pub is_lan: bool,
    /// Unix timestamp (seconds) of when the peer was last seen.
    pub last_seen: u64,
    /// The local IP address if discovered via mDNS.
    pub lan_address: Option<std::net::SocketAddr>,
    /// Device name (optional, from mDNS TXT record).
    pub device_name: Option<String>,
    /// Pairing code (optional, from mDNS TXT record).
    pub pairing_code: Option<String>,
}

/// Registry of known peer devices.
///
/// Thread-safe — can be shared across Tauri command handlers and
/// background sync tasks.
pub struct PeerDiscovery {
    /// Map from hex-encoded `PublicKey` to `PeerInfo`.
    known_peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
}

impl PeerDiscovery {
    /// Create an empty peer registry.
    pub fn new() -> Self {
        Self {
            known_peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register or update a peer.
    pub fn add_peer(&self, peer: PeerInfo) {
        let key = hex::encode(peer.node_id.as_bytes());
        self.known_peers.write().unwrap().insert(key, peer);
    }

    /// Remove a peer from the registry.
    pub fn remove_peer(&self, node_id_hex: &str) {
        self.known_peers.write().unwrap().remove(node_id_hex);
    }

    /// Get all currently known peers.
    pub fn online_peers(&self) -> Vec<PeerInfo> {
        self.known_peers
            .read()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    /// Get a specific peer by hex-encoded node ID.
    pub fn get_peer(&self, node_id_hex: &str) -> Option<PeerInfo> {
        self.known_peers.read().unwrap().get(node_id_hex).cloned()
    }

    /// Number of known peers.
    pub fn peer_count(&self) -> usize {
        self.known_peers.read().unwrap().len()
    }

    /// Remove peers that haven't been seen since `cutoff_secs` (Unix timestamp).
    pub fn prune_stale(&self, cutoff_secs: u64) {
        self.known_peers
            .write()
            .unwrap()
            .retain(|_, info| info.last_seen >= cutoff_secs);
    }
}

impl Default for PeerDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a dummy PeerInfo for testing.
    fn test_peer(last_seen: u64) -> PeerInfo {
        let key = iroh::SecretKey::generate();
        PeerInfo {
            node_id: key.public(),
            is_lan: false,
            last_seen,
            lan_address: None,
            device_name: None,
            pairing_code: None,
        }
    }

    #[test]
    fn test_add_and_list() {
        let disc = PeerDiscovery::new();
        assert_eq!(disc.peer_count(), 0);

        disc.add_peer(test_peer(100));
        disc.add_peer(test_peer(200));
        assert_eq!(disc.peer_count(), 2);
        assert_eq!(disc.online_peers().len(), 2);
    }

    #[test]
    fn test_remove_peer() {
        let disc = PeerDiscovery::new();
        let peer = test_peer(100);
        let hex_id = hex::encode(peer.node_id.as_bytes());

        disc.add_peer(peer);
        assert_eq!(disc.peer_count(), 1);

        disc.remove_peer(&hex_id);
        assert_eq!(disc.peer_count(), 0);
    }

    #[test]
    fn test_prune_stale() {
        let disc = PeerDiscovery::new();
        disc.add_peer(test_peer(100)); // stale
        disc.add_peer(test_peer(300)); // fresh

        disc.prune_stale(200);
        assert_eq!(disc.peer_count(), 1);
    }

    #[test]
    fn test_default() {
        let disc = PeerDiscovery::default();
        assert_eq!(disc.peer_count(), 0);
    }
}
