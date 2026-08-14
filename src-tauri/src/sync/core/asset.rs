use crate::error::{AppError, AppResult};
use crate::sync::core::types::{AssetChunkRef, AssetRef};
use blake3::Hasher;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const ASSET_CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4 MiB
pub const INLINE_PAYLOAD_LIMIT: u64 = 8 * 1024 * 1024; // 8 MiB
pub const MAX_SYNC_ASSET_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

/// Calculate the BLAKE3 hash of a file using streaming reads.
pub fn hash_file_streaming(path: &Path) -> AppResult<(blake3::Hash, u64)> {
    let mut file = File::open(path)
        .map_err(|e| AppError::General(format!("Failed to open file for hashing: {}", e)))?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024]; // 64 KB buffer
    let mut total_size = 0u64;

    loop {
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| AppError::General(format!("Failed to read file for hashing: {}", e)))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        total_size += bytes_read as u64;
    }

    Ok((hasher.finalize(), total_size))
}

/// Derive asset-specific keys
pub fn derive_asset_key(vault_e2ee_key: &[u8; 32], asset_id: &[u8; 32]) -> [u8; 32] {
    let context = format!("synabit-asset-key-v1-{}", hex::encode(asset_id));
    blake3::derive_key(&context, vault_e2ee_key)
}

fn derive_asset_id(vault_e2ee_key: &[u8; 32], plaintext_hash: &[u8; 32]) -> [u8; 32] {
    let vault_asset_id_key = blake3::derive_key("synabit-asset-id-v1", vault_e2ee_key);
    let mut hasher = blake3::Hasher::new_keyed(&vault_asset_id_key);
    hasher.update(plaintext_hash);
    *hasher.finalize().as_bytes()
}

fn derive_chunk_id(
    vault_e2ee_key: &[u8; 32],
    asset_id: &[u8; 32],
    chunk_index: u32,
    chunk_hash: &[u8; 32],
) -> [u8; 32] {
    let vault_chunk_id_key = blake3::derive_key("synabit-chunk-id-v1", vault_e2ee_key);
    let mut hasher = blake3::Hasher::new_keyed(&vault_chunk_id_key);
    hasher.update(asset_id);
    hasher.update(&chunk_index.to_le_bytes());
    hasher.update(chunk_hash);
    *hasher.finalize().as_bytes()
}

/// Generate an AssetRef by reading the file in chunks
pub fn create_asset_ref(path: &Path, vault_e2ee_key: &[u8; 32]) -> AppResult<AssetRef> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| AppError::General(format!("Failed to read file metadata: {}", e)))?;

    let file_size = metadata.len();
    if file_size > MAX_SYNC_ASSET_SIZE {
        return Err(AppError::General(
            "File exceeds maximum asset size of 2 GiB".to_string(),
        ));
    }

    let (plaintext_hash, actual_size) = hash_file_streaming(path)?;
    if actual_size != file_size {
        return Err(AppError::General(
            "File size changed during hashing".to_string(),
        ));
    }

    let asset_id = derive_asset_id(vault_e2ee_key, plaintext_hash.as_bytes());
    let mut file = File::open(path)
        .map_err(|e| AppError::General(format!("Failed to open file for chunking: {}", e)))?;

    let mut chunks = Vec::new();
    let mut index = 0u32;
    let mut buffer = vec![0u8; ASSET_CHUNK_SIZE];

    loop {
        let mut chunk_bytes_read = 0;
        while chunk_bytes_read < ASSET_CHUNK_SIZE {
            match file.read(&mut buffer[chunk_bytes_read..]) {
                Ok(0) => break,
                Ok(n) => chunk_bytes_read += n,
                Err(e) => return Err(AppError::General(format!("Failed to read chunk: {}", e))),
            }
        }

        if chunk_bytes_read == 0 && index > 0 {
            break;
        }

        let chunk_data = &buffer[..chunk_bytes_read];
        let chunk_hash = *blake3::hash(chunk_data).as_bytes();
        let chunk_id = derive_chunk_id(vault_e2ee_key, &asset_id, index, &chunk_hash);

        chunks.push(AssetChunkRef {
            index,
            chunk_id,
            plaintext_hash: chunk_hash,
            plaintext_size: chunk_bytes_read as u32,
        });

        index += 1;
        if chunk_bytes_read < ASSET_CHUNK_SIZE {
            break;
        }
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime_type = match ext.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "png" => Some("image/png".to_string()),
        "gif" => Some("image/gif".to_string()),
        "pdf" => Some("application/pdf".to_string()),
        "mp4" => Some("video/mp4".to_string()),
        "webm" => Some("video/webm".to_string()),
        _ => None,
    };

    Ok(AssetRef {
        version: 2,
        asset_id,
        plaintext_hash: *plaintext_hash.as_bytes(),
        plaintext_size: actual_size,
        chunk_size: ASSET_CHUNK_SIZE as u32,
        chunks,
        mime_type,
    })
}

/// Encrypt a single chunk. Returns the encrypted bytes.
pub fn encrypt_chunk(
    plaintext: &[u8],
    asset_ref: &AssetRef,
    chunk_ref: &AssetChunkRef,
    vault_e2ee_key: &[u8; 32],
) -> AppResult<Vec<u8>> {
    let asset_key = derive_asset_key(vault_e2ee_key, &asset_ref.asset_id);
    let cipher = XChaCha20Poly1305::new((&asset_key).into());

    let mut nonce_bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    // AAD: version (1), asset_id (32), chunk_index (4), chunk_count (4), plaintext_size (8)
    let mut aad = Vec::with_capacity(1 + 32 + 4 + 4 + 8);
    aad.push(asset_ref.version);
    aad.extend_from_slice(&asset_ref.asset_id);
    aad.extend_from_slice(&chunk_ref.index.to_le_bytes());
    aad.extend_from_slice(&(asset_ref.chunks.len() as u32).to_le_bytes());
    aad.extend_from_slice(&asset_ref.plaintext_size.to_le_bytes());

    let payload = Payload {
        msg: plaintext,
        aad: &aad,
    };

    let ciphertext = cipher
        .encrypt(nonce, payload)
        .map_err(|e| AppError::General(format!("Chunk encryption failed: {}", e)))?;

    // Wire format: [24-byte nonce][ciphertext + tag]
    let mut out = Vec::with_capacity(24 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a single chunk.
pub fn decrypt_chunk(
    encrypted_data: &[u8],
    asset_ref: &AssetRef,
    chunk_ref: &AssetChunkRef,
    vault_e2ee_key: &[u8; 32],
) -> AppResult<Vec<u8>> {
    if encrypted_data.len() < 24 + 16 {
        return Err(AppError::General("Chunk payload too short".to_string()));
    }

    let nonce_bytes: [u8; 24] = encrypted_data[0..24].try_into().unwrap();
    let ciphertext = &encrypted_data[24..];

    let asset_key = derive_asset_key(vault_e2ee_key, &asset_ref.asset_id);
    let cipher = XChaCha20Poly1305::new((&asset_key).into());
    let nonce = XNonce::from_slice(&nonce_bytes);

    let mut aad = Vec::with_capacity(1 + 32 + 4 + 4 + 8);
    aad.push(asset_ref.version);
    aad.extend_from_slice(&asset_ref.asset_id);
    aad.extend_from_slice(&chunk_ref.index.to_le_bytes());
    aad.extend_from_slice(&(asset_ref.chunks.len() as u32).to_le_bytes());
    aad.extend_from_slice(&asset_ref.plaintext_size.to_le_bytes());

    let payload = Payload {
        msg: ciphertext,
        aad: &aad,
    };

    let plaintext = cipher
        .decrypt(nonce, payload)
        .map_err(|e| AppError::General(format!("Chunk decryption failed: {}", e)))?;

    let actual_hash = *blake3::hash(&plaintext).as_bytes();
    if actual_hash != chunk_ref.plaintext_hash {
        return Err(AppError::General(
            "Chunk hash verification failed after decryption".to_string(),
        ));
    }

    Ok(plaintext)
}

/// Helper to read exactly one chunk from a file
pub fn read_chunk(path: &Path, offset: u64, size: u32) -> AppResult<Vec<u8>> {
    let mut file = File::open(path)
        .map_err(|e| AppError::General(format!("Failed to open file to read chunk: {}", e)))?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| AppError::General(format!("Failed to seek file: {}", e)))?;

    let mut buffer = vec![0u8; size as usize];
    file.read_exact(&mut buffer)
        .map_err(|e| AppError::General(format!("Failed to read chunk bytes: {}", e)))?;

    Ok(buffer)
}
