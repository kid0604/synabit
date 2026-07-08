use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use zeroize::Zeroize;

/// Wire format version for auto-key encryption (no Argon2, no salt)
const FORMAT_V3: u8 = 0x03;
/// Wire format version for epoch-based encryption (key rotation)
const FORMAT_V4: u8 = 0x04;
const FORMAT_V5: u8 = 0x05;
/// Wire format version for password-based encryption (Argon2 + random salt)
const FORMAT_V2: u8 = 0x02;
/// Legacy hardcoded salt (for v1 password-based decryption)
const LEGACY_SALT: &[u8] = b"synabit_e2ee_salt_v1_1234567890";
const SALT_LEN: usize = 32;

// ─── Key Generation ──────────────────────────────────────

/// Generate a cryptographically secure 256-bit key that is
/// round-trip compatible with the 12-word BIP39 mnemonic.
/// Only 16 random bytes are generated; the other 16 are derived
/// deterministically via BLAKE3 so that mnemonic recovery always
/// reproduces the same 32-byte key.
pub fn generate_key() -> [u8; 32] {
    let mut entropy = [0u8; 16];
    rand::rng().fill_bytes(&mut entropy);

    let mut key = [0u8; 32];
    key[..16].copy_from_slice(&entropy);
    let expanded = blake3::hash(&entropy);
    key[16..].copy_from_slice(&expanded.as_bytes()[..16]);
    entropy.zeroize();
    key
}

/// Convert a 32-byte key to a 12-word BIP39 mnemonic (128-bit entropy).
/// We use the first 16 bytes of the key for the mnemonic (128 bits = 12 words).
/// The full 32-byte key is stored in keychain; the mnemonic is a recovery backup.
pub fn key_to_mnemonic(key: &[u8; 32]) -> Result<String, String> {
    // Use first 16 bytes (128 bits) for 12-word mnemonic
    let mnemonic = bip39::Mnemonic::from_entropy(&key[..16])
        .map_err(|e| format!("Failed to generate mnemonic: {}", e))?;
    Ok(mnemonic.to_string())
}

/// Convert a 12-word BIP39 mnemonic back to a 32-byte key.
/// The mnemonic encodes 16 bytes; we derive the remaining 16 bytes
/// using BLAKE3 hash of the first 16 bytes for deterministic expansion.
pub fn mnemonic_to_key(phrase: &str) -> Result<[u8; 32], String> {
    let mnemonic = bip39::Mnemonic::parse(phrase)
        .map_err(|e| format!("Invalid recovery phrase: {}", e))?;
    let mut entropy = mnemonic.to_entropy();
    if entropy.len() != 16 {
        entropy.zeroize();
        return Err("Recovery phrase must be 12 words".to_string());
    }
    
    let mut key = [0u8; 32];
    key[..16].copy_from_slice(&entropy);
    // Deterministically derive second half from first half
    let expanded = blake3::hash(&entropy);
    key[16..].copy_from_slice(&expanded.as_bytes()[..16]);
    entropy.zeroize();
    Ok(key)
}

// ─── Epoch Key Derivation ────────────────────────────────

/// Derive an epoch-specific encryption key from the master key.
///
/// Each epoch produces a completely independent key via BLAKE3 keyed
/// derivation, so revoking a device (incrementing the epoch) makes all
/// future ciphertext unreadable to holders of only the old epoch key.
pub fn derive_epoch_key(master_key: &[u8; 32], epoch: u32) -> [u8; 32] {
    let context = format!("synabit-e2ee-epoch-{}", epoch);
    blake3::derive_key(&context, master_key)
}

// ─── Encryption (v3 format: no Argon2, direct key) ───────

/// Encrypt payload with a raw 256-bit key.
/// Wire format: [0x03][24-byte nonce][ciphertext+tag]
/// No Argon2, no salt — key is already high-entropy random.
pub fn encrypt(key: &[u8; 32], payload: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let mut result = Vec::with_capacity(1 + 24 + ciphertext.len());
    result.push(FORMAT_V3);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

// ─── Encryption (v4 format: epoch-based key rotation) ────

/// Encrypt with epoch-based v4 wire format.
/// Format: [0x04][epoch: u32 LE][nonce: 24 bytes][ciphertext + poly1305 tag]
///
/// The master key is never used directly — an epoch-specific key is derived
/// first, so rotating the epoch invalidates all future ciphertext for
/// devices that only know a previous epoch's key.
pub fn encrypt_v4(master_key: &[u8; 32], epoch: u32, plaintext: &[u8]) -> Vec<u8> {
    let epoch_key = derive_epoch_key(master_key, epoch);
    let cipher = XChaCha20Poly1305::new((&epoch_key).into());
    let mut nonce_bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext).expect("encryption failed");

    let mut out = Vec::with_capacity(1 + 4 + 24 + ciphertext.len());
    out.push(FORMAT_V4); // version marker
    out.extend_from_slice(&epoch.to_le_bytes());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    out
}

/// Decrypt v4: extract epoch from header, derive key, decrypt.
/// `data` layout after stripping the version byte:
///   [epoch: 4 bytes LE][nonce: 24 bytes][ciphertext + tag]
fn decrypt_v4(master_key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 4 + 24 + 16 {
        return Err("v4 ciphertext too short".into());
    }
    let epoch = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let nonce_bytes: [u8; 24] = data[4..28].try_into().unwrap();
    let ciphertext = &data[28..];

    let epoch_key = derive_epoch_key(master_key, epoch);
    let cipher = XChaCha20Poly1305::new((&epoch_key).into());
    let nonce = XNonce::from(nonce_bytes);

    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| format!("v4 decryption failed (epoch={})", epoch))
}

pub fn encrypt_v5(key: &[u8; 32], payload: &[u8], compress: bool) -> Result<Vec<u8>, String> {
    let payload_to_encrypt = if compress {
        lz4_flex::block::compress_prepend_size(payload)
    } else {
        payload.to_vec()
    };

    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, payload_to_encrypt.as_slice())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let flags = if compress { 0x01 } else { 0x00 };
    let mut result = Vec::with_capacity(1 + 1 + 24 + ciphertext.len());
    result.push(FORMAT_V5);
    result.push(flags);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

fn decrypt_v5(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 1 + 24 {
        return Err("v5 ciphertext too short".into());
    }
    let flags = data[0];
    let nonce_bytes: [u8; 24] = data[1..25].try_into().unwrap();
    let ciphertext = &data[25..];

    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XNonce::from(nonce_bytes);

    let decrypted = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| format!("v5 decryption failed: {}", e))?;

    if flags & 0x01 != 0 {
        lz4_flex::block::decompress_size_prepended(&decrypted)
            .map_err(|e| format!("decompression failed: {}", e))
    } else {
        Ok(decrypted)
    }
}


// ─── Unified Decrypt (v3 + v4) ───────────────────────────

/// Decrypt payload — auto-detects wire format version.
///
/// Supported formats:
/// - `0x03` — v3 (direct key, no epoch)
/// - `0x04` — v4 (epoch-based key rotation)
pub fn decrypt(key: &[u8; 32], encrypted_payload: &[u8]) -> Result<Vec<u8>, String> {
    if encrypted_payload.is_empty() {
        return Err("Payload too short".to_string());
    }

    match encrypted_payload[0] {
        FORMAT_V3 => {
            if encrypted_payload.len() < 1 + 24 {
                return Err("Payload too short".to_string());
            }
            let nonce_bytes = &encrypted_payload[1..25];
            let ciphertext = &encrypted_payload[25..];

            let cipher = XChaCha20Poly1305::new(key.into());
            let nonce = XNonce::from_slice(nonce_bytes);

            cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| format!("Decryption failed: {}", e))
        }
        FORMAT_V4 => decrypt_v4(key, &encrypted_payload[1..]),
        FORMAT_V5 => decrypt_v5(key, &encrypted_payload[1..]),
        other => Err(format!("Unknown wire format version: 0x{:02x}", other)),
    }
}

// ─── Legacy Decryption (for migration) ───────────────────

/// Decrypt payload that was encrypted with the old password-based system.
/// Supports both v2 (random salt + Argon2 strong) and v1 (hardcoded salt + Argon2 default).
/// This function is SLOW (calls Argon2) and only used during migration.
pub fn decrypt_legacy_password(password: &str, encrypted_payload: &[u8]) -> Result<Vec<u8>, String> {
    if encrypted_payload.is_empty() {
        return Err("Payload is empty".to_string());
    }

    // Try v2 format: [0x02][32-byte salt][24-byte nonce][ciphertext]
    if encrypted_payload[0] == FORMAT_V2 && encrypted_payload.len() > 1 + SALT_LEN + 24 {
        let salt = &encrypted_payload[1..1 + SALT_LEN];
        let nonce_bytes = &encrypted_payload[1 + SALT_LEN..1 + SALT_LEN + 24];
        let ciphertext = &encrypted_payload[1 + SALT_LEN + 24..];

        let key = derive_key_v2(password, salt)?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let nonce = XNonce::from_slice(nonce_bytes);

        match cipher.decrypt(nonce, ciphertext) {
            Ok(plaintext) => return Ok(plaintext),
            Err(_) => {} // Fall through to v1
        }
    }

    // Try v1 format: [24-byte nonce][ciphertext]
    if encrypted_payload.len() < 24 {
        return Err("Payload too short".to_string());
    }

    let key = derive_key_v1(password)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let nonce = XNonce::from_slice(&encrypted_payload[..24]);
    let ciphertext = &encrypted_payload[24..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))
}

/// Detect if a payload uses the new v3 auto-key format.
pub fn is_v3_format(payload: &[u8]) -> bool {
    !payload.is_empty() && payload[0] == FORMAT_V3
}

// ─── Internal: Argon2 key derivation (legacy only) ───────

fn derive_key_v2(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    use argon2::{Argon2, Params};
    let mut key = [0u8; 32];
    let params = Params::new(65536, 3, 1, Some(32)).expect("valid Argon2 params");
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut pwd = password.to_string();
    argon2
        .hash_password_into(pwd.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Argon2 v2 derivation failed: {}", e))?;
    pwd.zeroize();
    Ok(key)
}

fn derive_key_v1(password: &str) -> Result<[u8; 32], String> {
    use argon2::Argon2;
    let mut key = [0u8; 32];
    let mut pwd = password.to_string();
    Argon2::default()
        .hash_password_into(pwd.as_bytes(), LEGACY_SALT, &mut key)
        .map_err(|e| format!("Argon2 v1 derivation failed: {}", e))?;
    pwd.zeroize();
    Ok(key)
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_v4_roundtrip() {
        let master_key = generate_key();
        let plaintext = b"hello epoch-based encryption";
        let epoch = 42u32;

        let ciphertext = encrypt_v4(&master_key, epoch, plaintext);

        // Version byte should be 0x04
        assert_eq!(ciphertext[0], 0x04);

        let decrypted = decrypt(&master_key, &ciphertext).expect("decrypt should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_v4_different_epochs_different_ciphertext() {
        let master_key = generate_key();
        let plaintext = b"same plaintext, different epochs";

        let ct_epoch0 = encrypt_v4(&master_key, 0, plaintext);
        let ct_epoch1 = encrypt_v4(&master_key, 1, plaintext);

        // Ciphertext should differ (different derived keys + random nonces)
        assert_ne!(ct_epoch0, ct_epoch1);

        // Both should decrypt correctly
        let pt0 = decrypt(&master_key, &ct_epoch0).unwrap();
        let pt1 = decrypt(&master_key, &ct_epoch1).unwrap();
        assert_eq!(pt0, plaintext);
        assert_eq!(pt1, plaintext);
    }

    #[test]
    fn test_v4_wrong_epoch_fails() {
        let master_key = generate_key();
        let plaintext = b"secret data";

        let ciphertext = encrypt_v4(&master_key, 5, plaintext);

        // Tamper with the epoch field (bytes 1..5) to a different epoch
        let mut tampered = ciphertext.clone();
        tampered[1..5].copy_from_slice(&99u32.to_le_bytes());

        let result = decrypt(&master_key, &tampered);
        assert!(result.is_err(), "decryption with wrong epoch should fail");
    }

    #[test]
    fn test_v3_still_works() {
        let key = generate_key();
        let plaintext = b"backward compat test for v3";

        let ciphertext = encrypt(&key, plaintext).expect("v3 encrypt should succeed");
        assert_eq!(ciphertext[0], 0x03);

        let decrypted = decrypt(&key, &ciphertext).expect("v3 decrypt should succeed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_derive_epoch_key_deterministic() {
        let master_key = [0xABu8; 32];
        let k1 = derive_epoch_key(&master_key, 7);
        let k2 = derive_epoch_key(&master_key, 7);
        assert_eq!(k1, k2, "same epoch should derive the same key");

        let k3 = derive_epoch_key(&master_key, 8);
        assert_ne!(k1, k3, "different epochs should derive different keys");
    }
}
