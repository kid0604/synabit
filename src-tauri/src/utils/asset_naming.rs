//! Naming an attachment after its contents, so the vault stops keeping
//! copies of the same picture.
//!
//! Assets used to be named with a fresh UUID on every save. Pasting one
//! screenshot into five caps produced five identical files — and every one
//! of them was published separately over end-to-end encrypted sync, so the
//! user paid for the same bytes five times on a connection they may be
//! paying for by the megabyte.
//!
//! A name derived from the bytes makes the second copy free: the path is
//! already there, already synced, and the save turns into a no-op.
//!
//! # Why the hash is truncated
//!
//! 128 bits of BLAKE3. Two different images colliding is not a risk anyone
//! will meet — and unlike a UUID, the name says something true about the
//! file, which is what makes the deduplication possible at all.
//!
//! # Why old assets are left alone
//!
//! Existing UUID-named files stay exactly where they are. Their names are
//! written into the Markdown of every note that uses them, and rewriting
//! those to save a little disk would be a vault-wide edit in exchange for
//! nothing the user asked for. Deduplication applies from here on.

use std::io::Read;
use std::path::Path;

/// How much of the hash goes into the filename: 32 hex characters, 128 bits.
const HASH_CHARS: usize = 32;

/// The extension to store a file under: lower-cased, and never empty.
///
/// Case is normalised because `IMG.PNG` and `img.png` are the same picture
/// on a case-insensitive filesystem and two different names on a
/// case-sensitive one — which is exactly the sort of difference that makes
/// two devices disagree about whether a file already exists.
pub fn normalised_extension(filename: &str) -> String {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext.is_empty() || ext.len() > 12 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return "png".to_string();
    }
    ext
}

/// The name an asset with these bytes gets.
pub fn content_name(bytes: &[u8], filename: &str) -> String {
    let hash = blake3::hash(bytes);
    format!(
        "{}.{}",
        &hash.to_hex()[..HASH_CHARS],
        normalised_extension(filename)
    )
}

/// The same, for a file on disk, without reading it all into memory.
///
/// The picker hands over whatever the user chose, which may be a photo
/// straight off a phone camera. Hashing in chunks keeps a large attachment
/// from being loaded twice — once to hash and once to copy.
pub fn content_name_of_file(source: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(source)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let name = source.file_name().and_then(|n| n.to_str()).unwrap_or("");
    Ok(format!(
        "{}.{}",
        &hasher.finalize().to_hex()[..HASH_CHARS],
        normalised_extension(name)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole change exists for.
    #[test]
    fn the_same_bytes_get_the_same_name() {
        let bytes = b"the same screenshot";
        assert_eq!(
            content_name(bytes, "a.png"),
            content_name(bytes, "b.png"),
            "a name derived from contents cannot depend on what the file was called"
        );
    }

    #[test]
    fn different_bytes_get_different_names() {
        assert_ne!(
            content_name(b"one picture", "a.png"),
            content_name(b"another picture", "a.png")
        );
    }

    /// Two devices must agree on the name, or each one saves its own copy
    /// and sync carries both.
    #[test]
    fn the_name_is_a_function_of_the_bytes_alone() {
        let first = content_name(b"stable", "x.png");
        for _ in 0..5 {
            assert_eq!(content_name(b"stable", "x.png"), first);
        }
    }

    #[test]
    fn the_extension_survives() {
        assert!(content_name(b"x", "photo.jpg").ends_with(".jpg"));
        assert!(content_name(b"x", "drawing.webp").ends_with(".webp"));
    }

    #[test]
    fn case_is_normalised() {
        assert_eq!(normalised_extension("IMG.PNG"), "png");
        assert_eq!(normalised_extension("photo.JpEg"), "jpeg");
    }

    /// A filename is user input arriving from a clipboard or a share sheet.
    /// None of these may become part of a path.
    #[test]
    fn a_hostile_extension_falls_back_rather_than_being_used() {
        assert_eq!(normalised_extension("evil.../../etc/passwd"), "png");
        assert_eq!(normalised_extension("no-extension"), "png");
        assert_eq!(normalised_extension(""), "png");
        assert_eq!(normalised_extension("x."), "png");
        assert_eq!(normalised_extension("x.wayyytoolongextension"), "png");
    }

    #[test]
    fn the_name_is_a_plain_filename_with_no_path_in_it() {
        let name = content_name(b"x", "../../escape.png");
        assert!(!name.contains('/'), "{name}");
        assert!(!name.contains('\\'), "{name}");
        assert!(!name.contains(".."), "{name}");
    }

    #[test]
    fn hashing_a_file_agrees_with_hashing_its_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        let bytes = b"pretend this is a very large photograph";
        std::fs::write(&path, bytes).unwrap();

        assert_eq!(
            content_name_of_file(&path).unwrap(),
            content_name(bytes, "photo.png")
        );
    }

    /// The chunked reader has to give the same answer as the one-shot one
    /// for input larger than its buffer.
    #[test]
    fn a_file_larger_than_the_read_buffer_hashes_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.jpg");
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();

        assert_eq!(
            content_name_of_file(&path).unwrap(),
            content_name(&bytes, "big.jpg")
        );
    }
}
