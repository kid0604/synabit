//! Persistent Iroh Endpoint for P2P sync.
//!
//! Manages a long-lived Iroh Endpoint with a stable keypair persisted to disk.
//! This ensures the device has a consistent identity (NodeId) that other
//! paired devices can use to find and connect to it.

use log::{info, warn};
use std::path::Path;

use crate::error::{AppError, AppResult};

/// Manages a long-lived Iroh Endpoint with persistent identity.
///
/// The endpoint uses the N0 preset for relay + DNS discovery, enabling
/// devices to find each other even across NATs. The secret key is
/// persisted to `{data_dir}/device.key` so the device maintains a
/// stable `EndpointId` across restarts.
pub struct PersistentEndpoint {
    endpoint: iroh::Endpoint,
    node_id: iroh::PublicKey,
}

impl PersistentEndpoint {
    /// Start the persistent endpoint.
    ///
    /// Loads or creates a keypair from `{data_dir}/device.key`.
    /// Uses N0 preset for relay + DNS discovery.
    pub async fn start(data_dir: &Path) -> AppResult<Self> {
        let key = load_or_create_key(&data_dir.join("device.key"))?;
        let node_id = key.public();

        let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(key)
            .bind()
            .await
            .map_err(|e| AppError::General(format!("failed to bind persistent endpoint: {}", e)))?;

        info!(
            "Persistent endpoint started, node_id={}",
            node_id.fmt_short()
        );

        Ok(Self { endpoint, node_id })
    }

    /// The device's public key / node identifier.
    pub fn node_id(&self) -> iroh::PublicKey {
        self.node_id
    }

    /// The underlying Iroh endpoint (for creating transports or handlers).
    pub fn endpoint(&self) -> &iroh::Endpoint {
        &self.endpoint
    }

    /// Clone the endpoint for sharing with transports.
    ///
    /// `iroh::Endpoint` is internally `Arc`-wrapped, so cloning is cheap.
    pub fn endpoint_cloned(&self) -> iroh::Endpoint {
        self.endpoint.clone()
    }

    /// Get this device's full address for sharing with peers.
    ///
    /// The `EndpointAddr` contains the public key plus relay/direct
    /// address hints that help other devices connect.
    pub fn addr(&self) -> iroh::EndpointAddr {
        self.endpoint.addr()
    }

    /// The device's `EndpointId` (alias for the public key).
    pub fn id(&self) -> iroh::EndpointId {
        self.endpoint.id()
    }

    /// Shut down the endpoint, closing all connections.
    pub async fn shutdown(self) -> AppResult<()> {
        self.endpoint.close().await;
        info!("Persistent endpoint shut down");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Key persistence
// ---------------------------------------------------------------------------

/// Load or create a persistent secret key.
///
/// Follows the same pattern as the Synabit Sync Server's key persistence:
/// hex-encoded 32-byte secret key stored in a file with mode 0o600.
fn load_or_create_key(path: &Path) -> AppResult<iroh::SecretKey> {
    if path.exists() {
        let hex_str = std::fs::read_to_string(path)
            .map_err(|e| AppError::General(format!("failed to read key {}: {}", path.display(), e)))?;
        let hex_str = hex_str.trim();
        let bytes: [u8; 32] = hex::decode(hex_str)
            .map_err(|e| AppError::General(format!("invalid hex in key file: {}", e)))?
            .try_into()
            .map_err(|_| AppError::General("key must be 32 bytes".to_string()))?;
        let key = iroh::SecretKey::from_bytes(&bytes);
        info!("Loaded device key from {}", path.display());
        Ok(key)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let key = iroh::SecretKey::generate();
        let hex_str = hex::encode(key.to_bytes());
        std::fs::write(path, &hex_str)
            .map_err(|e| AppError::General(format!("failed to write key {}: {}", path.display(), e)))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).ok();
        }
        warn!("Generated NEW device key at {}", path.display());
        Ok(key)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_key_roundtrip() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test_keys");
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("test_roundtrip.key");
        // Clean up from previous runs
        let _ = std::fs::remove_file(&path);

        // Generate
        let key1 = load_or_create_key(&path).unwrap();
        assert!(path.exists());

        // Reload
        let key2 = load_or_create_key(&path).unwrap();
        assert_eq!(key1.to_bytes(), key2.to_bytes());

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_key_file_format() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test_keys");
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("test_format.key");
        let _ = std::fs::remove_file(&path);

        let key = load_or_create_key(&path).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();

        // Should be 64 hex characters (32 bytes)
        assert_eq!(contents.len(), 64);
        assert!(contents.chars().all(|c| c.is_ascii_hexdigit()));

        // Verify roundtrip via hex
        let decoded = hex::decode(&contents).unwrap();
        assert_eq!(decoded.len(), 32);
        assert_eq!(&decoded[..], &key.to_bytes()[..]);

        let _ = std::fs::remove_file(&path);
    }
}
