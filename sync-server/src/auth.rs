//! Authentication logic for the Synabit Mailbox protocol.
//!
//! Auth is token-based with no accounts:
//! - Token = `blake3::derive_key("synabit-mailbox-v1", &e2ee_key)`
//! - First device to connect with a new `vault_hash` implicitly *registers* the vault.
//! - Subsequent connections must present the same token (constant-time comparison).
//!
//! This design means all devices sharing the same E2EE key derive the same token
//! and can access the vault without any signup flow.

use anyhow::Result;
use subtle::ConstantTimeEq;
use tracing::{info, warn};

use crate::db::Database;

/// Result of an authentication attempt.
#[derive(Debug)]
pub enum AuthResult {
    /// New vault registered — this is the first device.
    Registered,
    /// Existing vault, token matched.
    Authenticated,
    /// Token mismatch for an existing vault.
    Rejected(String),
}

/// Authenticate (or register) a device for a vault.
///
/// # Security
/// - Uses constant-time comparison (`subtle::ConstantTimeEq`) so timing
///   side-channels cannot be used to guess the token byte-by-byte.
/// - The token is never logged; only the vault_hash (which is already a hash)
///   is included in log messages.
pub fn authenticate(
    db: &Database,
    vault_hash_hex: &str,
    mailbox_token: &[u8; 32],
    device_id: &str,
    default_max_vault_bytes: u64,
) -> Result<AuthResult> {
    match db.get_vault_token(vault_hash_hex)? {
        Some(stored_token) => {
            // Existing vault — verify token with constant-time comparison.
            if stored_token.ct_eq(mailbox_token).into() {
                // Update the device's last-seen timestamp.
                db.touch_device(vault_hash_hex, device_id)?;
                info!(
                    vault = vault_hash_hex,
                    device = device_id,
                    "device authenticated"
                );
                Ok(AuthResult::Authenticated)
            } else {
                warn!(
                    vault = vault_hash_hex,
                    device = device_id,
                    "authentication failed: token mismatch"
                );
                Ok(AuthResult::Rejected("token mismatch".to_string()))
            }
        }
        None => {
            // New vault — register it.
            db.register_vault(vault_hash_hex, mailbox_token, default_max_vault_bytes)?;
            db.touch_device(vault_hash_hex, device_id)?;
            info!(
                vault = vault_hash_hex,
                device = device_id,
                "new vault registered"
            );
            Ok(AuthResult::Registered)
        }
    }
}
