//! Item files on disk: `{vault}/Safe/items/<id>.safe`.
//!
//! Every write goes to a temporary name beside the file and is renamed over
//! it, so a crash leaves the old item or the new one, never half of either.
//! Nothing here ever deletes an item file it could not read — a file that does
//! not open is reported and left exactly where it is.

use std::path::{Path, PathBuf};

use super::crypto::{self, Key};
use super::format::{FormatError, ItemContent, ItemFile, ItemHeader, Keyset, OpenError};
use super::item::ItemBody;
use super::keyset::safe_dir;

pub const ITEMS_DIR: &str = "items";
const EXT: &str = "safe";

pub type ItemId = [u8; 16];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not {what} {path}: {source}")]
    Io { what: &'static str, path: String, source: std::io::Error },
    #[error("{path} is unreadable: {source}")]
    Format { path: String, source: FormatError },
    #[error("{path} does not open: {source}")]
    Open { path: String, source: OpenError },
    #[error("{path} opened but its contents are not an item: {source}")]
    Json { path: String, source: serde_json::Error },
    #[error(transparent)]
    Crypto(#[from] crypto::CryptoError),
}

fn io(what: &'static str, path: &Path, source: std::io::Error) -> StoreError {
    StoreError::Io { what, path: path.display().to_string(), source }
}

pub fn items_dir(vault: &Path) -> PathBuf {
    safe_dir(vault).join(ITEMS_DIR)
}

pub fn item_path(vault: &Path, id: &ItemId) -> PathBuf {
    items_dir(vault).join(format!("{}.{EXT}", hex::encode(id)))
}

/// The id an item file's name claims, if the name is one Safe writes. Anything
/// else in the folder — a temporary file, a stray `.DS_Store` — is not an item.
fn id_from_name(name: &str) -> Option<ItemId> {
    let stem = name.strip_suffix(".safe")?;
    let bytes = hex::decode(stem).ok()?;
    bytes.try_into().ok()
}

/// Write `bytes` to `path` via a temporary file in the same folder, synced
/// before the rename so the rename never publishes an empty file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("safe"),
        hex::encode(crypto::random_bytes::<4>().map_err(std::io::Error::other)?)
    ));
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// An item as read from disk.
pub struct Loaded {
    pub id: ItemId,
    pub revision: u64,
    /// `None` for a tombstone.
    pub body: Option<ItemBody>,
}

/// A file in `items/` that did not open, and why. Shown to the user; never
/// acted on.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Unreadable {
    pub file: String,
    pub reason: String,
}

fn read_file(vault: &Path, keyset: &Keyset, safe_key: &Key, id: &ItemId) -> Result<Loaded, StoreError> {
    let path = item_path(vault, id);
    let shown = path.display().to_string();
    let bytes = std::fs::read(&path).map_err(|e| io("read", &path, e))?;
    let file = ItemFile::decode(&bytes).map_err(|source| StoreError::Format { path: shown.clone(), source })?;
    let revision = file.header.revision;
    let body = match file
        .open(safe_key, &keyset.header.safe_id, id)
        .map_err(|source| StoreError::Open { path: shown.clone(), source })?
    {
        ItemContent::Tombstone => None,
        ItemContent::Body(json) => {
            Some(ItemBody::from_json(&json).map_err(|source| StoreError::Json { path: shown, source })?)
        }
    };
    Ok(Loaded { id: *id, revision, body })
}

pub fn load(vault: &Path, keyset: &Keyset, safe_key: &Key, id: &ItemId) -> Result<Loaded, StoreError> {
    read_file(vault, keyset, safe_key, id)
}

/// Every item in the Safe, and every file that would not open.
pub fn load_all(vault: &Path, keyset: &Keyset, safe_key: &Key) -> Result<(Vec<Loaded>, Vec<Unreadable>), StoreError> {
    let dir = items_dir(vault);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), Vec::new())),
        Err(e) => return Err(io("list", &dir, e)),
    };
    let mut loaded = Vec::new();
    let mut unreadable = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = id_from_name(&name) else { continue };
        match read_file(vault, keyset, safe_key, &id) {
            Ok(item) => loaded.push(item),
            Err(e) => {
                log::error!("[Safe] {e}");
                unreadable.push(Unreadable { file: name, reason: e.to_string() });
            }
        }
    }
    Ok((loaded, unreadable))
}

fn seal_and_write(
    vault: &Path,
    keyset: &Keyset,
    safe_key: &Key,
    id: &ItemId,
    revision: u64,
    plaintext: Option<&[u8]>,
) -> Result<(), StoreError> {
    let header = ItemHeader {
        safe_id: keyset.header.safe_id,
        item_id: *id,
        revision,
        key_epoch: keyset.header.key_epoch,
        tombstone: plaintext.is_none(),
    };
    // A fresh item key on every write. The design keeps one per item so that
    // sharing an item later means handing over one key; until sharing exists,
    // a fresh key costs nothing and leaves no key in use longer than one
    // revision.
    let item_key = Key::random()?;
    let file = ItemFile::seal(
        header,
        safe_key,
        &item_key,
        crypto::random_bytes()?,
        crypto::random_bytes()?,
        plaintext.unwrap_or(&[]),
    )?;
    let path = item_path(vault, id);
    write_atomic(&path, &file.encode()).map_err(|e| io("write", &path, e))
}

pub fn save(vault: &Path, keyset: &Keyset, safe_key: &Key, id: &ItemId, revision: u64, body: &ItemBody) -> Result<(), StoreError> {
    seal_and_write(vault, keyset, safe_key, id, revision, Some(&body.to_json()))
}

/// Replace an item with a tombstone: its contents are gone, its id and a
/// higher revision stay, so no device can sync the old item back.
pub fn bury(vault: &Path, keyset: &Keyset, safe_key: &Key, id: &ItemId, revision: u64) -> Result<(), StoreError> {
    seal_and_write(vault, keyset, safe_key, id, revision, None)
}

pub fn new_id() -> Result<ItemId, StoreError> {
    Ok(crypto::random_bytes()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safe::crypto::{KdfParams, SecretKey};
    use crate::safe::format::KeysetHeader;
    use crate::safe::item::{EditValue, FieldEdit, FieldKind, ItemEdit, ItemKind, SecretString};

    fn keyset() -> (Keyset, Key) {
        let kdf = KdfParams { m_kib: 64, t: 1, p: 1 };
        let auk = crypto::derive_auk(b"pw", &[0; 32], kdf, &SecretKey::from_bytes([1; 16])).unwrap();
        let header = KeysetHeader { safe_id: [7; 16], key_epoch: 1, keyset_revision: 1, kdf, kdf_salt: [0; 32] };
        let ks = Keyset::seal(header, &auk, &Key::from_bytes([3; 32]), [0; 24]).unwrap();
        (ks, Key::from_bytes([3; 32]))
    }

    fn body(password: &str) -> ItemBody {
        ItemBody::new_from(
            ItemEdit {
                kind: ItemKind::Login,
                title: "Bank".into(),
                fields: vec![FieldEdit {
                    id: None,
                    label: "password".into(),
                    kind: FieldKind::Password,
                    value: EditValue::Set { v: SecretString::new(password.into()) },
                }],
                urls: vec![],
                tags: vec![],
                favorite: false,
                notes: String::new(),
                expires_at: None,
            },
            1,
        )
        .unwrap()
    }

    #[test]
    fn items_round_trip_and_no_plaintext_reaches_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (ks, key) = keyset();
        let id = new_id().unwrap();
        save(dir.path(), &ks, &key, &id, 1, &body("canary-3f9a1c")).unwrap();

        let on_disk = std::fs::read(item_path(dir.path(), &id)).unwrap();
        for needle in [&b"canary-3f9a1c"[..], b"Bank", b"password"] {
            assert!(!on_disk.windows(needle.len()).any(|w| w == needle), "plaintext on disk");
        }

        let (items, bad) = load_all(dir.path(), &ks, &key).unwrap();
        assert!(bad.is_empty());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].revision, 1);
        assert_eq!(items[0].body.as_ref().unwrap().fields[0].value.expose(), "canary-3f9a1c");
    }

    #[test]
    fn a_buried_item_stays_as_a_tombstone() {
        let dir = tempfile::tempdir().unwrap();
        let (ks, key) = keyset();
        let id = new_id().unwrap();
        save(dir.path(), &ks, &key, &id, 1, &body("x")).unwrap();
        bury(dir.path(), &ks, &key, &id, 2).unwrap();
        let loaded = load(dir.path(), &ks, &key, &id).unwrap();
        assert_eq!(loaded.revision, 2);
        assert!(loaded.body.is_none());
    }

    /// A damaged file is reported, left in place, and does not stop the rest
    /// of the Safe from opening. Stray files are not items at all.
    #[test]
    fn a_damaged_file_is_reported_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let (ks, key) = keyset();
        let good = new_id().unwrap();
        let bad = new_id().unwrap();
        save(dir.path(), &ks, &key, &good, 1, &body("x")).unwrap();
        save(dir.path(), &ks, &key, &bad, 1, &body("y")).unwrap();
        let bad_path = item_path(dir.path(), &bad);
        let mut bytes = std::fs::read(&bad_path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&bad_path, &bytes).unwrap();
        std::fs::write(items_dir(dir.path()).join(".DS_Store"), b"junk").unwrap();

        let (items, unreadable) = load_all(dir.path(), &ks, &key).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(unreadable.len(), 1);
        assert_eq!(std::fs::read(&bad_path).unwrap(), bytes, "the damaged file was touched");
    }

    #[test]
    fn an_empty_or_missing_items_folder_is_an_empty_safe() {
        let dir = tempfile::tempdir().unwrap();
        let (ks, key) = keyset();
        let (items, bad) = load_all(dir.path(), &ks, &key).unwrap();
        assert!(items.is_empty() && bad.is_empty());
    }

    #[test]
    fn writes_leave_no_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let (ks, key) = keyset();
        let id = new_id().unwrap();
        for rev in 1..=5 {
            save(dir.path(), &ks, &key, &id, rev, &body("x")).unwrap();
        }
        let names: Vec<_> = std::fs::read_dir(items_dir(dir.path())).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names.len(), 1, "{names:?}");
    }
}
