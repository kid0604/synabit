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

/// The largest attachment this pipeline will carry.
///
/// Chunking holds the file and every one of its encrypted chunks in memory at
/// the same time, so a file costs roughly twice its own size in resident memory
/// while it is being prepared, and again while it is being reassembled. Without
/// a ceiling a single dropped-in video decides how much memory the app needs,
/// which on a phone means it is decided by being killed.
///
/// 100MB covers what a vault of notes actually holds — photographs, scans,
/// voice memos — and keeps the worst case survivable. Files above it stay
/// local, which is the same place they were before any of this existed.
pub const MAX_ASSET_BYTES: u64 = 100 * 1024 * 1024;

/// The most chunks any one attachment may claim.
///
/// Derived from the size ceiling rather than chosen, so the two cannot drift
/// apart. This bounds what a malformed or hostile entry can make us allocate
/// before a single byte has been verified: the chunk count arrives from the
/// network, and `Vec::with_capacity` believes whatever it is told.
pub const MAX_ASSET_CHUNKS: usize = (MAX_ASSET_BYTES / CHUNK_BYTES as u64) as usize + 1;

/// Is this attachment within the size the pipeline will carry?
///
/// One function so the sending and receiving sides cannot disagree about the
/// limit — a file we would refuse to send must also be one we refuse to be
/// handed, or the ceiling only binds the well-behaved.
pub fn check_size(rel_path: &str, total_bytes: u64) -> AppResult<()> {
    if total_bytes > MAX_ASSET_BYTES {
        return Err(AppError::AssetTooLarge(format!(
            "{} is {:.1}MB, over the {}MB limit for attachments",
            rel_path,
            total_bytes as f64 / (1024.0 * 1024.0),
            MAX_ASSET_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

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

/// Wire marker for a chunk, kept distinct from the document formats.
const CHUNK_FORMAT: u8 = 0x06;

/// Encrypt a chunk so that the same content always produces the same bytes.
///
/// The nonce is derived from the plaintext rather than drawn at random. That is
/// deliberate and it is what makes content addressing work: the reference
/// published to peers records the hash of the encrypted chunk, and the chunk is
/// re-derived from the file when it is uploaded, so a random nonce would produce
/// bytes that no longer match what was published.
///
/// Deriving a nonce from the message is safe here in the way that matters —
/// nonce reuse is dangerous when two *different* messages share one, and a nonce
/// that is a function of the message cannot do that. What it does reveal is that
/// two chunks are identical, which the chunk address already reveals and which
/// deduplication depends on.
fn encrypt_chunk(vault_key: &[u8; 32], plaintext: &[u8]) -> AppResult<Vec<u8>> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };

    let nonce_key = blake3::derive_key("synabit-asset-chunk-nonce-v1", vault_key);
    let digest = blake3::keyed_hash(&nonce_key, plaintext);
    let nonce_bytes: [u8; 24] = digest.as_bytes()[..24].try_into().expect("24 of 32 bytes");

    let cipher = XChaCha20Poly1305::new(vault_key.into());
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| AppError::General(format!("chunk encryption failed: {}", e)))?;

    let mut out = Vec::with_capacity(1 + 24 + ciphertext.len());
    out.push(CHUNK_FORMAT);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_chunk(vault_key: &[u8; 32], data: &[u8]) -> AppResult<Vec<u8>> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };

    if data.len() < 1 + 24 || data[0] != CHUNK_FORMAT {
        return Err(AppError::SyncError("chunk has an unknown format".into()));
    }
    let nonce_bytes: [u8; 24] = data[1..25].try_into().expect("checked length");
    let cipher = XChaCha20Poly1305::new(vault_key.into());
    cipher
        .decrypt(&XNonce::from(nonce_bytes), &data[25..])
        .map_err(|_| AppError::SyncError("chunk failed authentication".into()))
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
    check_size(rel_path, contents.len() as u64)?;

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
        let encrypted = encrypt_chunk(vault_key, piece)?;

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

/// Check an attachment description that arrived from another device.
///
/// Everything in an `AssetRef` is asserted by whoever sent it, and both the
/// size and the chunk count are used to size allocations. They are checked
/// before we act on them, so a corrupt or hostile entry costs a rejection
/// rather than the memory it asked for.
///
/// This runs before the first chunk is fetched. Verifying afterwards would mean
/// the damage — the allocation, the transfer — is already done.
pub fn validate_incoming(asset: &AssetRef) -> AppResult<()> {
    check_size(&asset.rel_path, asset.total_bytes)?;

    if asset.chunks.len() > MAX_ASSET_CHUNKS {
        return Err(AppError::AssetTooLarge(format!(
            "{} claims {} chunks, over the {} a valid attachment can have",
            asset.rel_path,
            asset.chunks.len(),
            MAX_ASSET_CHUNKS
        )));
    }

    // The two claims have to agree with each other. A short chunk list beside a
    // huge byte count is the shape of an entry built to make us allocate.
    let capacity = asset.chunks.len() as u64 * CHUNK_BYTES as u64;
    if asset.total_bytes > capacity {
        return Err(AppError::SyncError(format!(
            "{} claims {} bytes but only {} chunk(s) to hold them",
            asset.rel_path,
            asset.total_bytes,
            asset.chunks.len()
        )));
    }

    Ok(())
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
    validate_incoming(asset)?;

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
        let plain = decrypt_chunk(vault_key, encrypted)
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

    #[test]
    fn the_same_content_encrypts_to_the_same_bytes() {
        // The reference published to peers records the hash of the encrypted
        // chunk, and the chunk is re-derived from the file at upload time. If
        // encryption were randomised those bytes would no longer match and every
        // attachment would arrive "corrupt".
        let (_, first) = prepare(&KEY, "a.bin", "a.bin", b"identical content").unwrap();
        let (_, second) = prepare(&KEY, "a.bin", "a.bin", b"identical content").unwrap();
        assert_eq!(first[0].encrypted, second[0].encrypted);
        assert_eq!(first[0].reference.chunk_hash, second[0].reference.chunk_hash);
    }

    #[test]
    fn different_content_does_not_share_a_nonce() {
        let (_, a) = prepare(&KEY, "a.bin", "a.bin", b"one message").unwrap();
        let (_, b) = prepare(&KEY, "a.bin", "a.bin", b"another message").unwrap();
        assert_ne!(a[0].encrypted[1..25], b[0].encrypted[1..25], "nonces must differ");
    }

    #[test]
    fn a_tampered_chunk_fails_authentication() {
        let (asset, prepared) = prepare(&KEY, "a.bin", "a.bin", b"trusted bytes").unwrap();
        let mut fetched: Vec<(AssetChunkRef, Vec<u8>)> = prepared
            .into_iter()
            .map(|c| (c.reference, c.encrypted))
            .collect();
        // Flip a bit inside the ciphertext and repair the outer hash, so only
        // the cipher's own authentication can catch it.
        let last = fetched[0].1.len() - 1;
        fetched[0].1[last] ^= 0x01;
        let repaired = *blake3::hash(&fetched[0].1).as_bytes();
        let mut asset = asset;
        asset.chunks[0].chunk_hash = repaired;
        fetched[0].0.chunk_hash = repaired;

        let err = reassemble(&KEY, &asset, &fetched).unwrap_err();
        assert!(err.to_string().contains("authentication"), "unexpected: {err}");
    }
}

/// Where a file goes when it loses its position to another device's version.
///
/// Two devices can put different content at one path — importing files that
/// happen to share a name, or naming two notes identically. One of them has to
/// win the position, and without somewhere for the other to go, losing it means
/// losing the content.
///
/// The name is derived rather than chosen so that every device computing it
/// arrives at the same answer with nothing to co-ordinate. `discriminator` is a
/// hex digest of whatever identifies the losing side — its content for a file
/// with no identity of its own, its `node_id` for a document that has one.
///
/// Kept to ASCII on purpose: the vault syncs between machines whose filesystems
/// disagree about Unicode normalisation, and a filename is the last place to
/// discover that.
pub fn conflict_path(rel_path: &str, discriminator: &str) -> String {
    let short: String = discriminator.chars().take(8).collect();
    let path = std::path::Path::new(rel_path);

    let parent = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .filter(|p| !p.is_empty());
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let extension = path.extension().and_then(|e| e.to_str());

    let name = match extension {
        Some(ext) => format!("{stem} (conflict {short}).{ext}"),
        None => format!("{stem} (conflict {short})"),
    };

    match parent {
        Some(dir) => format!("{dir}/{name}"),
        None => name,
    }
}

#[cfg(test)]
mod conflict_path_tests {
    use super::conflict_path;

    #[test]
    fn the_name_keeps_its_folder_and_its_extension() {
        assert_eq!(
            conflict_path("assets/report.pdf", "3f2a91c8dead"),
            "assets/report (conflict 3f2a91c8).pdf"
        );
        assert_eq!(
            conflict_path("Notes/plan.md", "abcdef0123"),
            "Notes/plan (conflict abcdef01).md"
        );
    }

    #[test]
    fn a_file_at_the_root_or_without_an_extension_still_works() {
        assert_eq!(conflict_path("notes.md", "11112222"), "notes (conflict 11112222).md");
        assert_eq!(conflict_path("assets/LICENSE", "aaaabbbb"), "assets/LICENSE (conflict aaaabbbb)");
    }

    #[test]
    fn the_same_input_always_gives_the_same_name() {
        // Every device works this out alone. If two of them disagreed, the copy
        // would arrive twice under two names.
        let a = conflict_path("assets/photo.png", "deadbeefcafe");
        let b = conflict_path("assets/photo.png", "deadbeefcafe");
        assert_eq!(a, b);
    }

    #[test]
    fn different_losers_do_not_land_on_the_same_name() {
        assert_ne!(
            conflict_path("assets/photo.png", "aaaaaaaa"),
            conflict_path("assets/photo.png", "bbbbbbbb")
        );
    }
}
