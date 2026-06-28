//! P2P Device Pairing — code-based pairing protocol.
//!
//! Implements a simplified pairing flow using 8-character alphanumeric codes.
//! A device initiates pairing by generating a code, which a second device
//! enters to complete the pairing. The pairing session is time-limited
//! (5 minutes) and produces a `PairedDevice` record stored in the KV store.
//!
//! This is a simplified version — actual peer-to-peer connection establishment
//! will use the sync server as a relay for pairing messages.

use serde::{Deserialize, Serialize};

use log::info;

use crate::error::AppResult;

/// 8-character alphanumeric pairing code.
const CODE_LENGTH: usize = 8;
/// How long a pairing code is valid (5 minutes).
const CODE_EXPIRY_SECS: u64 = 300;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Information about a paired device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_name: String,
    /// Hex-encoded `iroh::PublicKey`.
    pub node_id_hex: String,
    /// Unix timestamp when pairing was established.
    pub paired_at: u64,
    /// Unix timestamp of the last time this device was seen.
    pub last_seen: u64,
}

/// Information returned when initiating pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingInfo {
    /// 8-char code formatted as "XXXX-XXXX".
    pub code: String,
    /// This device's node ID (hex).
    pub node_id_hex: String,
    /// Unix timestamp when the code expires.
    pub expires_at: u64,
}

/// Information returned when joining a pairing session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResult {
    pub peer_node_id_hex: String,
    pub peer_device_name: String,
}

// ---------------------------------------------------------------------------
// Pairing session
// ---------------------------------------------------------------------------

/// Active pairing session state.
pub struct PairingSession {
    pub code: String,
    pub my_node_id_hex: String,
    pub my_device_name: String,
    pub created_at: u64,
    pub peer_info: Option<PeerPairingInfo>,
}

/// Information received from the peer during pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerPairingInfo {
    pub node_id_hex: String,
    pub device_name: String,
}

impl PairingSession {
    /// Generate a new pairing session with a random code.
    pub fn new(my_node_id_hex: String, my_device_name: String) -> Self {
        let code = generate_code();
        let now = unix_now();

        info!(
            "Pairing session created, code={}, device={}",
            format_code(&code),
            my_device_name
        );

        Self {
            code,
            my_node_id_hex,
            my_device_name,
            created_at: now,
            peer_info: None,
        }
    }

    /// Whether the session's code has expired.
    pub fn is_expired(&self) -> bool {
        unix_now() - self.created_at > CODE_EXPIRY_SECS
    }

    /// Build a `PairingInfo` suitable for returning to the frontend.
    pub fn pairing_info(&self) -> PairingInfo {
        PairingInfo {
            code: format_code(&self.code),
            node_id_hex: self.my_node_id_hex.clone(),
            expires_at: self.created_at + CODE_EXPIRY_SECS,
        }
    }

    /// Accept a peer's pairing request, producing a `PairedDevice` record.
    pub fn accept_peer(&mut self, peer_info: PeerPairingInfo) -> PairedDevice {
        let now = unix_now();
        self.peer_info = Some(peer_info.clone());

        info!(
            "Pairing accepted: peer_node={}, peer_device={}",
            peer_info.node_id_hex, peer_info.device_name
        );

        PairedDevice {
            device_name: peer_info.device_name,
            node_id_hex: peer_info.node_id_hex,
            paired_at: now,
            last_seen: now,
        }
    }
}

// ---------------------------------------------------------------------------
// Code generation & formatting
// ---------------------------------------------------------------------------

/// Generate an 8-character alphanumeric code (uppercase + digits).
///
/// Uses safe characters — no `0`/`O`, `1`/`I`/`L` confusion.
fn generate_code() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    const CHARS: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

    let mut code = String::with_capacity(CODE_LENGTH);
    for i in 0..CODE_LENGTH {
        let state = RandomState::new();
        let mut hasher = state.build_hasher();
        hasher.write_usize(i);
        let idx = (hasher.finish() as usize) % CHARS.len();
        code.push(CHARS[idx] as char);
    }
    code
}

/// Format code as "XXXX-XXXX" for display.
fn format_code(code: &str) -> String {
    if code.len() >= 8 {
        format!("{}-{}", &code[..4], &code[4..8])
    } else {
        code.to_string()
    }
}

/// Normalize a user-input code (remove dashes, uppercase).
pub fn normalize_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .collect()
}

/// Validate that a normalized code has the expected format.
pub fn validate_code(code: &str) -> AppResult<()> {
    if code.len() != CODE_LENGTH {
        return Err(crate::error::AppError::General(format!(
            "pairing code must be {} characters, got {}",
            CODE_LENGTH,
            code.len()
        )));
    }
    if !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(crate::error::AppError::General(
            "pairing code must be alphanumeric".to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Current unix timestamp in seconds.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_length() {
        let code = generate_code();
        assert_eq!(code.len(), CODE_LENGTH);
    }

    #[test]
    fn test_generate_code_charset() {
        let code = generate_code();
        for ch in code.chars() {
            assert!(
                ch.is_ascii_uppercase() || ch.is_ascii_digit(),
                "unexpected char: {}",
                ch
            );
            // Should not contain confusing characters
            assert!(!['O', 'I', 'L', '0', '1'].contains(&ch));
        }
    }

    #[test]
    fn test_format_code() {
        assert_eq!(format_code("ABCD1234"), "ABCD-1234");
        assert_eq!(format_code("SHORT"), "SHORT");
    }

    #[test]
    fn test_normalize_code() {
        assert_eq!(normalize_code("abcd-1234"), "ABCD1234");
        assert_eq!(normalize_code("AXBR-M4KP"), "AXBRM4KP");
        assert_eq!(normalize_code("  ab cd "), "ABCD");
    }

    #[test]
    fn test_validate_code() {
        assert!(validate_code("ABCD1234").is_ok());
        assert!(validate_code("SHORT").is_err());
        assert!(validate_code("ABCDEFGHIJ").is_err());
    }

    #[test]
    fn test_session_not_immediately_expired() {
        let session = PairingSession::new("abc123".to_string(), "TestDevice".to_string());
        assert!(!session.is_expired());
        assert_eq!(session.code.len(), CODE_LENGTH);
    }

    #[test]
    fn test_accept_peer() {
        let mut session = PairingSession::new("abc123".to_string(), "TestDevice".to_string());
        let peer = PeerPairingInfo {
            node_id_hex: "deadbeef".to_string(),
            device_name: "PeerDevice".to_string(),
        };
        let device = session.accept_peer(peer);
        assert_eq!(device.device_name, "PeerDevice");
        assert_eq!(device.node_id_hex, "deadbeef");
        assert!(device.paired_at > 0);
        assert!(session.peer_info.is_some());
    }
}
