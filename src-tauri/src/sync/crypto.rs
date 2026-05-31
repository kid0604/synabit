use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use zeroize::Zeroize;

/// Wire format version for auto-key encryption (no Argon2, no salt)
const FORMAT_V3: u8 = 0x03;
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

/// Decrypt payload encrypted with v3 format (raw key, no Argon2).
/// Wire format: [0x03][24-byte nonce][ciphertext+tag]
pub fn decrypt(key: &[u8; 32], encrypted_payload: &[u8]) -> Result<Vec<u8>, String> {
    if encrypted_payload.len() < 1 + 24 {
        return Err("Payload too short".to_string());
    }
    if encrypted_payload[0] != FORMAT_V3 {
        return Err("Not a v3 encrypted payload".to_string());
    }

    let nonce_bytes = &encrypted_payload[1..25];
    let ciphertext = &encrypted_payload[25..];

    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XNonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))
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
