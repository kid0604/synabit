//! A JSON document the app keeps in the vault on the user's behalf.
//!
//! For what used to live only in the cache and is the user's own — feed
//! highlights, calendar subscriptions. Two rules, both from losing data:
//!
//! * **A file that cannot be read is never written over.** [`read`] answers
//!   an error for it rather than an empty document, so the read-modify-write
//!   every caller does stops instead of replacing somebody's highlights with
//!   one new one. A file that is not there yet is empty, which is different.
//! * **What the sync layer stamped is kept.** [`write`] carries the file's
//!   `metadata` over and dates it, the way `syn::vault_json` does for Syn's
//!   files, then writes through `path_utils::write_atomic`.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{AppError, AppResult};

/// Held across a read-modify-write of any of these documents. One lock for
/// all of them: they are small, written by hand, and rarely at the same time.
static WRITING: Mutex<()> = Mutex::new(());

pub fn writing() -> MutexGuard<'static, ()> {
    WRITING.lock().unwrap_or_else(|e| e.into_inner())
}

/// The document at `path`, or an empty one if there is no file yet.
pub fn read<T: DeserializeOwned + Default>(path: &Path) -> AppResult<T> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            AppError::General(format!("{} cannot be read ({e}); it was left as it is", path.display()))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(AppError::General(format!("Failed to read {}: {e}", path.display()))),
    }
}

/// Write `value` to `path`, keeping and stamping the sync layer's `metadata`.
pub fn write<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let existing = std::fs::read_to_string(path).ok();
    let rendered = crate::syn::vault_json::render(
        existing.as_deref(),
        serde_json::to_value(value)?,
        &crate::syn::vault_json::now_stamp(),
    )?;
    crate::path_utils::write_atomic(path, rendered.as_bytes())
        .map_err(|e| AppError::General(format!("Failed to write {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, serde::Deserialize, Default, Debug, PartialEq)]
    struct Doc {
        #[serde(default)]
        items: Vec<String>,
    }

    #[test]
    fn a_missing_file_is_empty_and_a_broken_one_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Feeds/highlights.json");
        assert_eq!(read::<Doc>(&path).unwrap(), Doc::default());

        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ half a fi").unwrap();
        assert!(read::<Doc>(&path).is_err(), "a caller must not mistake this for nothing");
    }

    #[test]
    fn the_sync_layers_node_id_survives_a_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Calendar/subscriptions.json");
        write(&path, &Doc { items: vec!["a".into()] }).unwrap();
        let mut stamped: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        stamped["metadata"]["node_id"] = "n-1".into();
        std::fs::write(&path, stamped.to_string()).unwrap();

        write(&path, &Doc { items: vec!["a".into(), "b".into()] }).unwrap();
        let back: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["metadata"]["node_id"], "n-1");
        assert_eq!(back["items"].as_array().unwrap().len(), 2);
        assert_eq!(read::<Doc>(&path).unwrap().items, ["a", "b"]);
    }
}
