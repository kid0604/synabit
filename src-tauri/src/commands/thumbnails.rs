//! Small copies of image attachments, for the card grid.
//!
//! A cap's card shows its picture at most 256px tall, but the file behind it
//! is whatever came off the camera. The webview decodes the full thing to
//! paint that strip: a 12-megapixel photo costs about 48MB of decoded pixels
//! to display at the size of a postage stamp, and a grid of them costs that
//! several times over.
//!
//! # Why there is no image decoder here
//!
//! There is no image crate in this project, and adding one means shipping a
//! set of codecs inside an Android package that already goes through a
//! release-size review. The webview has those codecs already — it is a
//! browser — so the resizing happens there, on a `<canvas>`, and this module
//! only stores the result.
//!
//! # Why they live under a dot
//!
//! `assets/.thumbs/` is derived data: delete it and the app is unharmed.
//! `sync::utils::collect_local_files` skips every directory whose name begins
//! with a dot, so putting them there keeps regenerable bytes off the user's
//! sync connection rather than paying to carry a picture twice.
//!
//! Thumbnails inherit their name from the asset, which is a hash of that
//! asset's contents, so they deduplicate for free along with it.

use std::path::{Path, PathBuf};

use crate::error::AppResult;

const THUMB_DIR: &str = "assets/.thumbs";

/// The thumbnail file for an asset, or `None` if the name is not one.
///
/// The asset name arrives from the front end, which got it from a clipboard
/// or a share sheet, so it is untrusted input on its way to becoming a path.
/// Only a plain filename is accepted — no separators, no parent hops.
fn thumb_name(asset_name: &str) -> Option<String> {
    if asset_name.is_empty()
        || asset_name.contains('/')
        || asset_name.contains('\\')
        || asset_name.contains("..")
    {
        return None;
    }
    let stem = Path::new(asset_name).file_stem()?.to_str()?;
    if stem.is_empty() {
        return None;
    }
    Some(format!("{stem}.webp"))
}

fn thumb_dir(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(THUMB_DIR)
}

/// Store a thumbnail the webview rendered. Returns its filename.
#[tauri::command]
pub fn save_thumbnail(vault_path: String, asset_name: String, bytes: Vec<u8>) -> AppResult<String> {
    let Some(name) = thumb_name(&asset_name) else {
        return Err(crate::error::AppError::InvalidPath(format!(
            "'{asset_name}' is not a plain asset filename"
        )));
    };

    let dir = thumb_dir(&vault_path);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(&name), bytes)?;
    Ok(name)
}

/// Every thumbnail this vault already has.
///
/// The card grid needs to know which images have one before it renders, and
/// asking per image would be a filesystem call per card per paint. One list
/// at load, held as a set, costs a single call.
#[tauri::command]
pub fn list_thumbnails(vault_path: String) -> AppResult<Vec<String>> {
    let dir = thumb_dir(&vault_path);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thumbnail_is_named_after_its_asset() {
        assert_eq!(thumb_name("abc123.png").unwrap(), "abc123.webp");
        assert_eq!(thumb_name("abc123.jpeg").unwrap(), "abc123.webp");
    }

    /// The asset name comes from a clipboard or a share sheet. None of these
    /// may reach the filesystem.
    #[test]
    fn a_name_that_is_really_a_path_is_refused() {
        assert!(thumb_name("../../etc/passwd").is_none());
        assert!(thumb_name("nested/name.png").is_none());
        assert!(thumb_name("nested\\name.png").is_none());
        assert!(thumb_name("..").is_none());
        assert!(thumb_name("").is_none());
    }

    #[test]
    fn saving_refuses_a_hostile_name_rather_than_writing_it() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();

        assert!(save_thumbnail(vault.clone(), "../escape.png".into(), vec![1, 2, 3]).is_err());
        assert!(!dir.path().parent().unwrap().join("escape.webp").exists());
    }

    #[test]
    fn saving_then_listing_finds_it() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();

        let name = save_thumbnail(vault.clone(), "abc123.png".into(), vec![1, 2, 3]).unwrap();

        assert_eq!(name, "abc123.webp");
        assert_eq!(list_thumbnails(vault).unwrap(), vec!["abc123.webp"]);
    }

    /// A vault nobody has pasted a picture into yet is not an error.
    #[test]
    fn listing_an_empty_vault_is_empty_rather_than_failing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_thumbnails(dir.path().to_string_lossy().to_string())
            .unwrap()
            .is_empty());
    }

    /// Two caps holding the same picture share one asset, so they share one
    /// thumbnail too — the second save overwrites identical bytes.
    #[test]
    fn the_same_asset_yields_one_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();

        save_thumbnail(vault.clone(), "abc123.png".into(), vec![1]).unwrap();
        save_thumbnail(vault.clone(), "abc123.png".into(), vec![1]).unwrap();

        assert_eq!(list_thumbnails(vault).unwrap().len(), 1);
    }

    /// The directory has to start with a dot, or sync carries a copy of every
    /// picture the app made for itself.
    #[test]
    fn thumbnails_live_somewhere_sync_ignores() {
        assert!(
            THUMB_DIR.split('/').any(|part| part.starts_with('.')),
            "collect_local_files only skips dot-directories"
        );
    }
}
