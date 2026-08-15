//! Turning a file on disk into encrypted chunks, and back again.
//!
//! Documents are carried as CRDT text, which cannot represent an image. Files
//! that are not text travel a separate path: split into fixed-size pieces, each
//! encrypted on its own, published as a small reference that names the pieces.
//! The bytes never enter the mailbox entry, and never enter the database — the
//! copy in the vault is the copy.
//!
//! ## Chunk identity
//!
//! A chunk is addressed by a hash of its *plaintext*, keyed with a secret
//! derived from the vault key. Keying it matters: a plain content hash would let
//! anyone holding the server's storage test whether a vault contains a file they
//! already have, simply by hashing their copy. With the key in the hash they
//! cannot, while identical content within one vault still deduplicates.

use crate::error::{AppError, AppResult};
use synabit_protocol::{AssetChunkRef, AssetRef};

/// Size of a plaintext chunk before encryption.
///
/// Small enough that a failed transfer costs little and a large file can resume
/// part-way; large enough that a photograph is a handful of pieces rather than
/// hundreds. The server accepts up to 10MB per chunk, so this leaves ample room
/// for the encryption overhead.
pub const CHUNK_BYTES: usize = 1024 * 1024;

fn chunk_id_key(vault_key: &[u8; 32]) -> [u8; 32] {
    blake3::derive_key("synabit-asset-chunk-id-v1", vault_key)
}

/// Address a chunk by its content, unlinkably to anyone without the vault key.
pub fn chunk_id(vault_key: &[u8; 32], plaintext: &[u8]) -> [u8; 32] {
    *blake3::keyed_hash(&chunk_id_key(vault_key), plaintext).as_bytes()
}

/// Guess a MIME type from the file extension.
///
/// Only used as a hint for the receiving side; nothing depends on it being
/// right, and the bytes are verified against their hash regardless.
pub fn mime_for(rel_path: &str) -> String {
    let ext = std::path::Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// One chunk ready to be uploaded.
pub struct PreparedChunk {
    pub reference: AssetChunkRef,
    pub encrypted: Vec<u8>,
}

/// Split a file's contents into encrypted chunks and describe them.
///
/// Returns the reference to publish and the chunks to upload. An empty file
/// still produces one chunk, so that reassembly has something to verify against
/// and the receiver creates the file rather than skipping it.
pub fn prepare(
    vault_key: &[u8; 32],
    rel_path: &str,
    node_id: &str,
    contents: &[u8],
) -> AppResult<(AssetRef, Vec<PreparedChunk>)> {
    let mut chunks = Vec::new();
    let mut references = Vec::new();

    let pieces: Vec<&[u8]> = if contents.is_empty() {
        vec![&[][..]]
    } else {
        contents.chunks(CHUNK_BYTES).collect()
    };

    for piece in pieces {
        let id = chunk_id(vault_key, piece);
        // Compression is left off: these are already-compressed formats far more
        // often than not, and spending time to grow a JPEG helps nobody.
        let encrypted = crate::sync::core::crypto::encrypt_v5(vault_key, piece, false)
            .map_err(|e| AppError::General(format!("Failed to encrypt asset chunk: {}", e)))?;

        let compressed_len = u32::try_from(encrypted.len()).map_err(|_| {
            AppError::General("Encrypted asset chunk exceeds the addressable size".into())
        })?;

        references.push(AssetChunkRef {
            chunk_id: id,
            chunk_hash: *blake3::hash(&encrypted).as_bytes(),
            compressed_len,
        });
        chunks.push(PreparedChunk {
            reference: references[references.len() - 1].clone(),
            encrypted,
        });
    }

    let asset = AssetRef {
        asset_id: *blake3::keyed_hash(&chunk_id_key(vault_key), contents).as_bytes(),
        rel_path: rel_path.to_string(),
        node_id: node_id.to_string(),
        mime_type: mime_for(rel_path),
        total_bytes: contents.len() as u64,
        plaintext_hash: *blake3::hash(contents).as_bytes(),
        chunks: references,
    };

    Ok((asset, chunks))
}

/// Rebuild a file from chunks that have already been fetched.
///
/// Every stage is checked: each encrypted chunk against the hash in the
/// reference, then the reassembled file against its own hash and length. A file
/// that fails any of these is refused rather than written, because a
/// half-correct attachment on disk is worse than a missing one.
pub fn reassemble(
    vault_key: &[u8; 32],
    asset: &AssetRef,
    fetched: &[(AssetChunkRef, Vec<u8>)],
) -> AppResult<Vec<u8>> {
    if fetched.len() != asset.chunks.len() {
        return Err(AppError::SyncError(format!(
            "asset {} expected {} chunk(s), got {}",
            asset.rel_path,
            asset.chunks.len(),
            fetched.len()
        )));
    }

    let mut out = Vec::with_capacity(asset.total_bytes as usize);
    for (expected, (reference, encrypted)) in asset.chunks.iter().zip(fetched.iter()) {
        if reference.chunk_id != expected.chunk_id {
            return Err(AppError::SyncError(format!(
                "asset {} received chunks out of order",
                asset.rel_path
            )));
        }
        if *blake3::hash(encrypted).as_bytes() != expected.chunk_hash {
            return Err(AppError::SyncError(format!(
                "asset {} has a corrupt chunk",
                asset.rel_path
            )));
        }
        let plain = crate::sync::core::crypto::decrypt_v5(vault_key, &encrypted[1..])
            .map_err(|e| AppError::SyncError(format!("asset {}: {}", asset.rel_path, e)))?;
        out.extend_from_slice(&plain);
    }

    if out.len() as u64 != asset.total_bytes {
        return Err(AppError::SyncError(format!(
            "asset {} reassembled to {} bytes, expected {}",
            asset.rel_path,
            out.len(),
            asset.total_bytes
        )));
    }
    if *blake3::hash(&out).as_bytes() != asset.plaintext_hash {
        return Err(AppError::SyncError(format!(
            "asset {} failed its content check after reassembly",
            asset.rel_path
        )));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [3u8; 32];

    fn round_trip(contents: &[u8]) {
        let (asset, prepared) = prepare(&KEY, "assets/file.bin", "assets/file.bin", contents)
            .expect("prepare");
        let fetched: Vec<(AssetChunkRef, Vec<u8>)> = prepared
            .into_iter()
            .map(|c| (c.reference, c.encrypted))
            .collect();
        let rebuilt = reassemble(&KEY, &asset, &fetched).expect("reassemble");
        assert_eq!(rebuilt, contents, "the file did not survive the round trip");
    }

    #[test]
    fn files_survive_being_split_and_put_back() {
        round_trip(b"");
        round_trip(b"small");
        round_trip(&vec![0xABu8; CHUNK_BYTES - 1]);
        round_trip(&vec![0xCDu8; CHUNK_BYTES]);
        round_trip(&vec![0xEFu8; CHUNK_BYTES + 1]);
        round_trip(&(0..250_000u32).map(|i| i as u8).collect::<Vec<u8>>());
    }

    #[test]
    fn a_large_file_becomes_several_chunks() {
        let contents = vec![7u8; CHUNK_BYTES * 3 + 10];
        let (asset, prepared) = prepare(&KEY, "a.bin", "a.bin", &contents).unwrap();
        assert_eq!(prepared.len(), 4);
        assert_eq!(asset.chunks.len(), 4);
        assert_eq!(asset.total_bytes, contents.len() as u64);
    }

    #[test]
    fn identical_content_shares_a_chunk_address() {
        let (a, _) = prepare(&KEY, "one.bin", "one.bin", b"same bytes").unwrap();
        let (b, _) = prepare(&KEY, "two.bin", "two.bin", b"same bytes").unwrap();
        assert_eq!(
            a.chunks[0].chunk_id, b.chunks[0].chunk_id,
            "the same content should deduplicate within a vault"
        );
    }

    #[test]
    fn another_vault_cannot_recognise_our_content() {
        // A plain content hash would let anyone holding the server's storage
        // test whether a vault contains a file they already have.
        let ours = prepare(&KEY, "a.bin", "a.bin", b"a known photograph").unwrap().0;
        let theirs = prepare(&[9u8; 32], "a.bin", "a.bin", b"a known photograph")
            .unwrap()
            .0;
        assert_ne!(ours.chunks[0].chunk_id, theirs.chunks[0].chunk_id);
        assert_ne!(ours.asset_id, theirs.asset_id);
    }

    #[test]
    fn a_corrupt_chunk_is_refused_rather_than_written() {
        let contents = vec![1u8; 5_000];
        let (asset, prepared) = prepare(&KEY, "a.bin", "a.bin", &contents).unwrap();
        let mut fetched: Vec<(AssetChunkRef, Vec<u8>)> = prepared
            .into_iter()
            .map(|c| (c.reference, c.encrypted))
            .collect();
        fetched[0].1.push(0xFF);

        let err = reassemble(&KEY, &asset, &fetched).unwrap_err();
        assert!(
            err.to_string().contains("corrupt chunk"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn a_missing_chunk_is_refused() {
        let contents = vec![2u8; CHUNK_BYTES + 5];
        let (asset, prepared) = prepare(&KEY, "a.bin", "a.bin", &contents).unwrap();
        let fetched: Vec<(AssetChunkRef, Vec<u8>)> = prepared
            .into_iter()
            .take(1)
            .map(|c| (c.reference, c.encrypted))
            .collect();
        assert!(reassemble(&KEY, &asset, &fetched).is_err());
    }

    #[test]
    fn a_wrong_key_cannot_rebuild_the_file() {
        let (asset, prepared) = prepare(&KEY, "a.bin", "a.bin", b"private").unwrap();
        let fetched: Vec<(AssetChunkRef, Vec<u8>)> = prepared
            .into_iter()
            .map(|c| (c.reference, c.encrypted))
            .collect();
        assert!(reassemble(&[9u8; 32], &asset, &fetched).is_err());
    }
}
