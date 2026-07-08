import re

with open('src/sync/crypto.rs', 'r') as f:
    content = f.read()

v5 = """pub const FORMAT_V5: u8 = 0x05;

/// Encrypt payload with optional LZ4 compression (v5 format)
pub fn encrypt_v5(key: &[u8; 32], payload: &[u8], compress: bool) -> Result<Vec<u8>, String> {
    let payload_to_encrypt = if compress {
        lz4_flex::compress_size_prepended(payload)
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
        lz4_flex::decompress_size_prepended(&decrypted)
            .map_err(|e| format!("decompression failed: {}", e))
    } else {
        Ok(decrypted)
    }
}
"""

content = content.replace('pub const FORMAT_V4: u8 = 0x04;', 'pub const FORMAT_V4: u8 = 0x04;\n' + v5)

decrypt_old = """        FORMAT_V4 => decrypt_v4(key, &encrypted_payload[1..]),
        other => Err(format!("Unknown wire format version: 0x{:02x}", other)),"""

decrypt_new = """        FORMAT_V4 => decrypt_v4(key, &encrypted_payload[1..]),
        FORMAT_V5 => decrypt_v5(key, &encrypted_payload[1..]),
        other => Err(format!("Unknown wire format version: 0x{:02x}", other)),"""

content = content.replace(decrypt_old, decrypt_new)

with open('src/sync/crypto.rs', 'w') as f:
    f.write(content)

with open('src/sync/engine.rs', 'r') as f:
    engine = f.read()

# Update engine to use encrypt_v5
engine = re.sub(r'crate::sync::crypto::encrypt\(&e2ee_key, &file_content\)', 'crate::sync::crypto::encrypt_v5(&e2ee_key, &file_content, false)', engine)
engine = re.sub(r'crate::sync::crypto::encrypt\(&e2ee_key, &serialized\)', 'crate::sync::crypto::encrypt_v5(&e2ee_key, &serialized, true)', engine)

with open('src/sync/engine.rs', 'w') as f:
    f.write(engine)

