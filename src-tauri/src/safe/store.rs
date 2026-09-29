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
    #[error("{path} opened but its contents are not an item: {reason}")]
    Json { path: String, reason: String },
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
///
/// Owner-only (0600) on Unix, from the moment the temporary file exists:
/// everything written through here is a Safe file, an export, or the
/// Emergency Kit — and the plaintext CSV export is all of the user's secrets.
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
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&tmp)?;
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

/// The Safe Key, and the ones it replaced — kept so that a file sealed
/// before a rotation, or by a device that had not seen it yet, still opens.
pub struct Keys<'a> {
    pub current: &'a Key,
    pub older: &'a [Key],
}

impl<'a> Keys<'a> {
    pub fn only(current: &'a Key) -> Self {
        Keys { current, older: &[] }
    }

    /// Open `file` with whichever key sealed it: the current one first. A
    /// wrong key fails its authentication tag, which costs nothing to try.
    pub fn open(&self, file: &ItemFile, safe_id: &[u8; 16], id: &ItemId) -> Result<ItemContent, super::format::OpenError> {
        let first = file.open(self.current, safe_id, id);
        if !matches!(first, Err(super::format::OpenError::Corrupt)) {
            return first;
        }
        self.older.iter().find_map(|k| file.open(k, safe_id, id).ok()).ok_or(super::format::OpenError::Corrupt)
    }
}

/// The reserved id of the file that keeps replaced Safe Keys. Never an item:
/// not listed, not exported, not given to sync's merge as one.
pub const KEYS_ID: ItemId = [0xff; 16];

fn read_file(vault: &Path, keyset: &Keyset, keys: &Keys, id: &ItemId) -> Result<Loaded, StoreError> {
    let path = item_path(vault, id);
    let shown = path.display().to_string();
    let bytes = std::fs::read(&path).map_err(|e| io("read", &path, e))?;
    let file = ItemFile::decode(&bytes).map_err(|source| StoreError::Format { path: shown.clone(), source })?;
    let revision = file.header.revision;
    let body = match keys
        .open(&file, &keyset.header.safe_id, id)
        .map_err(|source| StoreError::Open { path: shown.clone(), source })?
    {
        ItemContent::Tombstone => None,
        ItemContent::Body(json) => {
            Some(ItemBody::from_json(&json).map_err(|e| StoreError::Json { path: shown, reason: super::item::json_problem(&e) })?)
        }
    };
    Ok(Loaded { id: *id, revision, body })
}

pub fn load(vault: &Path, keyset: &Keyset, keys: &Keys, id: &ItemId) -> Result<Loaded, StoreError> {
    read_file(vault, keyset, keys, id)
}

/// Every item in the Safe, and every file that would not open.
pub fn load_all(vault: &Path, keyset: &Keyset, keys: &Keys) -> Result<(Vec<Loaded>, Vec<Unreadable>), StoreError> {
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
        if id == KEYS_ID {
            continue;
        }
        match read_file(vault, keyset, keys, &id) {
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
    loop {
        let id: ItemId = crypto::random_bytes()?;
        if id != KEYS_ID {
            return Ok(id);
        }
    }
}

// ─── replaced Safe Keys ──────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct KeyHistory {
    /// Each replaced Safe Key, hex, oldest first.
    #[serde(default)]
    keys: Vec<String>,
}

/// The Safe Keys this Safe used before, from `items/ffff…ff.safe`. Empty when
/// it has never been rotated.
pub fn load_key_history(vault: &Path, keyset: &Keyset, keys: &Keys) -> Result<Vec<Key>, StoreError> {
    let path = item_path(vault, &KEYS_ID);
    if !path.exists() {
        return Ok(Vec::new());
    }
    key_history_in(&path, keyset, keys)
}

/// The replaced keys held in the key-history file at `path` — the Safe's
/// own, or a version of it sync set aside.
pub fn key_history_in(path: &Path, keyset: &Keyset, keys: &Keys) -> Result<Vec<Key>, StoreError> {
    let shown = path.display().to_string();
    let bytes = std::fs::read(path).map_err(|e| io("read", path, e))?;
    let file = ItemFile::decode(&bytes).map_err(|source| StoreError::Format { path: shown.clone(), source })?;
    let json = match keys.open(&file, &keyset.header.safe_id, &KEYS_ID).map_err(|source| StoreError::Open { path: shown.clone(), source })? {
        ItemContent::Body(json) => json,
        ItemContent::Tombstone => return Ok(Vec::new()),
    };
    let history: KeyHistory =
        serde_json::from_slice(&json).map_err(|e| StoreError::Json { path: shown.clone(), reason: super::item::json_problem(&e) })?;
    history
        .keys
        .iter()
        .map(|h| {
            let bytes = zeroize::Zeroizing::new(hex::decode(h).ok().filter(|b| b.len() == 32).ok_or_else(|| StoreError::Json {
                path: shown.clone(),
                reason: "a replaced key is malformed".into(),
            })?);
            let mut array = [0u8; 32];
            array.copy_from_slice(&bytes);
            let key = Key::from_bytes(array);
            zeroize::Zeroize::zeroize(&mut array);
            Ok(key)
        })
        .collect()
}

/// Write the replaced Safe Keys, sealed under the current one.
pub fn save_key_history(vault: &Path, keyset: &Keyset, current: &Key, revision: u64, older: &[Key]) -> Result<(), StoreError> {
    let history = KeyHistory { keys: older.iter().map(|k| hex::encode(k.as_bytes())).collect() };
    let json = zeroize::Zeroizing::new(serde_json::to_vec(&history).expect("key history serialises"));
    let mut history = history;
    history.keys.iter_mut().for_each(zeroize::Zeroize::zeroize);
    seal_and_write(vault, keyset, current, &KEYS_ID, revision, Some(&json))
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
                totp: Default::default(),
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

        let (items, bad) = load_all(dir.path(), &ks, &Keys::only(&key)).unwrap();
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
        let loaded = load(dir.path(), &ks, &Keys::only(&key), &id).unwrap();
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

        let (items, unreadable) = load_all(dir.path(), &ks, &Keys::only(&key)).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(unreadable.len(), 1);
        assert_eq!(std::fs::read(&bad_path).unwrap(), bytes, "the damaged file was touched");
    }

    #[test]
    fn an_empty_or_missing_items_folder_is_an_empty_safe() {
        let dir = tempfile::tempdir().unwrap();
        let (ks, key) = keyset();
        let (items, bad) = load_all(dir.path(), &ks, &Keys::only(&key)).unwrap();
        assert!(items.is_empty() && bad.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn what_is_written_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("export.csv");
        write_atomic(&path, b"name,password\n").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
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
