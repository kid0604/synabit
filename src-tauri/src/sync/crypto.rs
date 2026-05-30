use argon2::Argon2;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use zeroize::Zeroize;

const SALT: &[u8] = b"synabit_e2ee_salt_v1_1234567890";

pub fn derive_key(password: &mut String) -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    
    Argon2::default()
        .hash_password_into(password.as_bytes(), SALT, &mut key)
        .map_err(|e| format!("Failed to derive key: {}", e))?;
        
    password.zeroize();
    Ok(key)
}

pub fn encrypt_snapshot(key: &[u8; 32], payload: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce_bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    let mut result = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

pub fn decrypt_snapshot(key: &[u8; 32], encrypted_payload: &[u8]) -> Result<Vec<u8>, String> {
    if encrypted_payload.len() < 24 {
        return Err("Payload too short".to_string());
    }

    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XNonce::from_slice(&encrypted_payload[..24]);
    let ciphertext = &encrypted_payload[24..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))
}
