//! Article images, fetched by the app rather than by the page.
//!
//! Rendering `<img src="https://publisher.example/...">` inside the reader
//! means the publisher learns when the article was opened, from which address,
//! and — through a one-pixel image that exists for exactly this purpose —
//! that it was opened at all. It also means an article read on a train has no
//! pictures in it.
//!
//! So the images are fetched here, once, into the app's own data directory,
//! and the page is given local paths. The cache lives outside the vault: these
//! are copies of somebody else's files, not the reader's documents, and there
//! is no reason to sync them between devices.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Images larger than this are left alone; a reader is not a photo library.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// Where cached images live under the app data directory.
pub const CACHE_DIR_NAME: &str = "feed_images";

/// The file an image URL maps to. Content-addressed by URL, so the same
/// picture used across several articles is fetched once.
fn cache_path(cache_dir: &Path, url: &str, extension: &str) -> PathBuf {
    let digest = hex::encode(Sha256::digest(url.as_bytes()));
    cache_dir.join(format!("{}.{}", &digest[..32], extension))
}

/// The file extension to store an image under, from what the server called it.
///
/// The extension is for the webview's benefit — the asset protocol infers a
/// content type from it — so an unrecognised type is stored as `.bin` and
/// simply will not render, rather than being guessed at.
fn extension_for(content_type: &str) -> Option<&'static str> {
    let base = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/avif" => Some("avif"),
        "image/svg+xml" => Some("svg"),
        _ => None,
    }
}

/// Fetch one image into the cache, or return where it already is.
///
/// Returns `None` for anything that is not a usable image — a 404, a tracking
/// pixel served as `text/gif`, something too large, a private address. The
/// caller leaves the original markup alone in that case, and the picture is
/// simply missing rather than the article failing to open.
pub async fn cache_image(cache_dir: &Path, url: &str) -> Option<PathBuf> {
    if super::fetcher::guard_url(url).is_err() {
        return None;
    }

    // A hit costs one stat call, which is why the extension is part of the
    // name rather than something to look up.
    for extension in ["jpg", "png", "gif", "webp", "avif", "svg"] {
        let candidate = cache_path(cache_dir, url, extension);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let client = super::fetcher::build_client(std::time::Duration::from_secs(20)).ok()?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }

    let extension = extension_for(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default(),
    )?;

    if response
        .content_length()
        .is_some_and(|len| len as usize > MAX_IMAGE_BYTES)
    {
        return None;
    }

    let bytes = response.bytes().await.ok()?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return None;
    }

    std::fs::create_dir_all(cache_dir).ok()?;
    let path = cache_path(cache_dir, url, extension);
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

/// Delete cached images not touched in `max_age_days`.
///
/// The cache is disposable by construction: anything removed here is fetched
/// again the next time an article that uses it is opened.
pub fn prune(cache_dir: &Path, max_age_days: i64) -> usize {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return 0;
    };
    let cutoff = std::time::SystemTime::now()
        - std::time::Duration::from_secs((max_age_days.max(1) as u64) * 24 * 60 * 60);

    let mut removed = 0;
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .is_some_and(|modified| modified < cutoff);
        if stale && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_url_always_names_the_same_file() {
        let dir = Path::new("/tmp/cache");
        let a = cache_path(dir, "https://example.com/a.png", "png");
        let b = cache_path(dir, "https://example.com/a.png", "png");
        assert_eq!(a, b);

        let other = cache_path(dir, "https://example.com/b.png", "png");
        assert_ne!(a, other, "different pictures, different files");
    }

    #[test]
    fn a_cached_name_is_a_plain_filename() {
        // The URL must not be able to steer where the file lands.
        let dir = Path::new("/tmp/cache");
        let path = cache_path(dir, "https://example.com/../../etc/passwd?x=/y", "png");
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.ends_with(".png"));
        assert!(!name.contains('/') && !name.contains(".."));
        assert_eq!(path.parent(), Some(dir));
    }

    #[test]
    fn only_things_that_are_actually_images_get_stored() {
        assert_eq!(extension_for("image/jpeg"), Some("jpg"));
        assert_eq!(extension_for("image/png; charset=binary"), Some("png"));
        assert_eq!(extension_for("IMAGE/WEBP"), Some("webp"));
        assert_eq!(extension_for("text/html"), None);
        assert_eq!(extension_for(""), None);
    }

    #[test]
    fn pruning_removes_the_old_and_keeps_the_recent() {
        let dir = tempfile::tempdir().expect("temp cache");
        let fresh = dir.path().join("fresh.png");
        std::fs::write(&fresh, b"x").unwrap();

        // Nothing is old yet, so nothing goes.
        assert_eq!(prune(dir.path(), 30), 0);
        assert!(fresh.exists());

        // A zero-day cutoff is clamped to one day, so today's file survives.
        assert_eq!(prune(dir.path(), 0), 0);
        assert!(fresh.exists());
    }

    #[test]
    fn pruning_a_directory_that_is_not_there_is_not_an_error() {
        assert_eq!(prune(Path::new("/nonexistent/feed_images"), 30), 0);
    }
}
