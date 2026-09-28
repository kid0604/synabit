//! Safe's files between devices: what to do when another device's version of
//! one arrives.
//!
//! Sync carries `Safe/` as attachments — opaque bytes, encrypted once by Safe
//! and again by sync. What it cannot do is decide between two versions of an
//! item, because it cannot read either. Its own rule for attachments is "the
//! later one in the mailbox wins, the loser is renamed beside it", and for a
//! Safe that is wrong twice: a sync peer could hand back last month's password
//! and it would win, and a renamed copy of an item file is a second item with
//! the same id.
//!
//! So `Safe/` gets a rule of its own, decided here from the **cleartext
//! header** alone — which Safe, which item, which revision. No key is needed,
//! so it works while the Safe is locked, which is most of the time sync runs.
//!
//! # The rule (section 5.6 of the design)
//!
//! * A different Safe, a header that does not match the file name, or bytes
//!   that do not parse: **refused**, and the local file is left alone.
//! * A revision lower than the highest this device has seen for the item:
//!   **refused**. That is a replay — an old password handed back — and the
//!   heart of why this module exists.
//! * A higher revision: **written**.
//! * The same revision with different bytes: two devices edited at once. Both
//!   devices keep the version whose bytes hash higher, so they agree without
//!   talking, and set the other **aside** in `Safe/conflicts/` for the open
//!   Safe to fold into the winner's history — see `Unlocked::resolve_conflicts`.
//!   Nothing is lost and no second copy of the item appears in the list.
//!
//! # Where "the highest revision seen" lives
//!
//! `.synabit/safe/seen.json`, per device and never synced — a dotdir. In the
//! clear, unlike the design's first draft: it has to be read while the Safe
//! is locked, and it holds item ids and revision numbers, which the file names
//! and headers beside it already show. Losing it costs only rollback
//! protection until the items are next written or read.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::format::{ItemFile, Keyset};
use super::store::{self, ItemId};

pub const PREFIX: &str = "Safe/";
pub const CONFLICTS_DIR: &str = "conflicts";

/// Whether a vault-relative path is one of Safe's.
pub fn is_safe_path(rel_path: &str) -> bool {
    rel_path.replace('\\', "/").starts_with(PREFIX)
}

/// Paths under `Safe/` that stay on the device: versions set aside to be
/// merged here, and anything a crashed write left behind.
pub fn is_local_only(rel_path: &str) -> bool {
    let rel = rel_path.replace('\\', "/");
    rel.starts_with("Safe/conflicts/") || rel.ends_with(".tmp") || rel.ends_with(".safe.new")
}

// ─── the highest revision seen ───────────────────────────

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Seen {
    #[serde(default)]
    pub keyset: u64,
    #[serde(default)]
    pub items: BTreeMap<String, u64>,
}

fn seen_path(vault: &Path) -> PathBuf {
    vault.join(".synabit").join("safe").join("seen.json")
}

impl Seen {
    pub fn load(vault: &Path) -> Seen {
        std::fs::read(seen_path(vault)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save(&self, vault: &Path) {
        let bytes = serde_json::to_vec(self).expect("seen serialises");
        if let Err(e) = store::write_atomic(&seen_path(vault), &bytes) {
            log::warn!("[Safe] could not record revisions seen: {e}");
        }
    }

    pub fn item(&self, id: &ItemId) -> u64 {
        self.items.get(&hex::encode(id)).copied().unwrap_or(0)
    }
}

/// Remember that `id` has been seen at `revision`. Never lowers a mark.
pub fn saw_item(vault: &Path, id: &ItemId, revision: u64) {
    let mut seen = Seen::load(vault);
    let mark = seen.items.entry(hex::encode(id)).or_default();
    if revision > *mark {
        *mark = revision;
        seen.save(vault);
    }
}

pub fn saw_keyset(vault: &Path, revision: u64) {
    let mut seen = Seen::load(vault);
    if revision > seen.keyset {
        seen.keyset = revision;
        seen.save(vault);
    }
}

// ─── the decision ────────────────────────────────────────

/// What to do with another device's version of a file under `Safe/`.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming {
    /// Write theirs over ours (or where there was nothing).
    Write,
    /// Leave ours; say why in the log.
    Refuse(String),
    /// Same revision, different bytes. `write` says whether theirs wins; the
    /// loser — theirs or ours — goes to `aside`.
    Conflict { write: bool, aside: PathBuf },
}

enum Header {
    Keyset { safe_id: [u8; 16], revision: u64 },
    Item { safe_id: [u8; 16], item_id: ItemId, revision: u64 },
}

fn header(bytes: &[u8]) -> Option<Header> {
    if let Ok(k) = Keyset::decode(bytes) {
        return Some(Header::Keyset { safe_id: k.header.safe_id, revision: k.header.keyset_revision });
    }
    ItemFile::decode(bytes).ok().map(|f| Header::Item {
        safe_id: f.header.safe_id,
        item_id: f.header.item_id,
        revision: f.header.revision,
    })
}

fn aside_path(vault: &Path, stem: &str, loser: &[u8]) -> PathBuf {
    let tag = &blake3::hash(loser).to_hex()[..16];
    vault.join(PREFIX).join(CONFLICTS_DIR).join(format!("{stem}.{tag}.safe"))
}

/// Decide about `theirs`, arriving for `rel_path`.
pub fn decide(vault: &Path, rel_path: &str, theirs: &[u8]) -> Incoming {
    let rel = rel_path.replace('\\', "/");
    let local_path = vault.join(&rel);
    let ours = std::fs::read(&local_path).ok();
    let seen = Seen::load(vault);
    let local_safe_id = super::keyset::read(vault).ok().map(|k| k.header.safe_id);

    let Some(their_header) = header(theirs) else {
        return Incoming::Refuse(format!("{rel}: not a Safe file"));
    };

    let (safe_id, revision, floor, stem) = match (&their_header, rel.as_str()) {
        (Header::Keyset { safe_id, revision }, "Safe/keyset.safe") => (*safe_id, *revision, seen.keyset, "keyset".to_string()),
        (Header::Item { safe_id, item_id, revision }, path) if path.starts_with("Safe/items/") => {
            let named = path.strip_prefix("Safe/items/").and_then(|n| n.strip_suffix(".safe"));
            if named != Some(hex::encode(item_id).as_str()) {
                return Incoming::Refuse(format!("{rel}: holds a different item than its name says"));
            }
            (*safe_id, *revision, seen.item(item_id), hex::encode(item_id))
        }
        _ => return Incoming::Refuse(format!("{rel}: not a place Safe files arrive")),
    };

    if let Some(ours_id) = local_safe_id {
        if ours_id != safe_id {
            return Incoming::Refuse(format!("{rel}: belongs to a different Safe"));
        }
    }
    if revision < floor {
        return Incoming::Refuse(format!("{rel}: revision {revision} is older than {floor}, already seen here — refused as a replay"));
    }

    let Some(ours) = ours else { return Incoming::Write };
    let our_revision = match header(&ours) {
        Some(Header::Keyset { revision, .. }) | Some(Header::Item { revision, .. }) => revision,
        // Ours does not parse: theirs, which does, is the better copy — but
        // ours is kept aside rather than destroyed.
        None => return Incoming::Conflict { write: true, aside: aside_path(vault, &stem, &ours) },
    };

    match revision.cmp(&our_revision) {
        std::cmp::Ordering::Greater => Incoming::Write,
        std::cmp::Ordering::Less => Incoming::Refuse(format!("{rel}: ours is newer ({our_revision} > {revision})")),
        std::cmp::Ordering::Equal if ours == theirs => Incoming::Refuse(format!("{rel}: already here")),
        std::cmp::Ordering::Equal => {
            // Both devices run this with the two versions swapped and must
            // reach the same answer: keep the higher hash.
            let theirs_wins = blake3::hash(theirs).as_bytes() > blake3::hash(&ours).as_bytes();
            let loser: &[u8] = if theirs_wins { &ours } else { theirs };
            Incoming::Conflict { write: theirs_wins, aside: aside_path(vault, &stem, loser) }
        }
    }
}

/// Carry out a decision: write theirs, set the loser aside, move the
/// high-water mark. Returns whether `rel_path` now holds theirs.
pub fn apply(vault: &Path, rel_path: &str, theirs: &[u8], decision: &Incoming) -> std::io::Result<bool> {
    let local_path = vault.join(rel_path.replace('\\', "/"));
    let wrote = match decision {
        Incoming::Refuse(reason) => {
            log::info!("[Safe] sync: {reason}");
            false
        }
        Incoming::Write => {
            store::write_atomic(&local_path, theirs)?;
            true
        }
        Incoming::Conflict { write, aside } => {
            let loser: Vec<u8> = if *write { std::fs::read(&local_path).unwrap_or_default() } else { theirs.to_vec() };
            if !aside.exists() {
                store::write_atomic(aside, &loser)?;
            }
            if *write {
                store::write_atomic(&local_path, theirs)?;
            }
            log::info!("[Safe] sync: two versions of {rel_path} at the same revision; one kept aside to merge");
            *write
        }
    };
    if wrote {
        match header(theirs) {
            Some(Header::Keyset { revision, .. }) => saw_keyset(vault, revision),
            Some(Header::Item { item_id, revision, .. }) => saw_item(vault, &item_id, revision),
            None => {}
        }
    }
    Ok(wrote)
}

/// Every version set aside for an item, oldest file first.
pub fn conflicts_for(vault: &Path) -> Vec<(ItemId, PathBuf)> {
    let dir = vault.join(PREFIX).join(CONFLICTS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<(ItemId, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.split('.').next()?;
            let id: ItemId = hex::decode(stem).ok()?.try_into().ok()?;
            Some((id, e.path()))
        })
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safe::crypto::{self, KdfParams, Key, SecretKey};
    use crate::safe::format::{ItemHeader, KeysetHeader};

    const SAFE: [u8; 16] = [7; 16];
    const ITEM: [u8; 16] = [9; 16];

    fn keyset(safe_id: [u8; 16], revision: u64) -> Vec<u8> {
        let kdf = KdfParams { m_kib: 64, t: 1, p: 1 };
        let auk = crypto::derive_auk(b"pw", &[0; 32], kdf, &SecretKey::from_bytes([1; 16])).unwrap();
        let header = KeysetHeader { safe_id, key_epoch: 1, keyset_revision: revision, kdf, kdf_salt: [0; 32] };
        Keyset::seal(header, &auk, &Key::from_bytes([3; 32]), crypto::random_bytes().unwrap()).unwrap().encode()
    }

    fn item(safe_id: [u8; 16], revision: u64, body: &[u8]) -> Vec<u8> {
        let header = ItemHeader { safe_id, item_id: ITEM, revision, key_epoch: 1, tombstone: false };
        ItemFile::seal(header, &Key::from_bytes([3; 32]), &Key::random().unwrap(), crypto::random_bytes().unwrap(), crypto::random_bytes().unwrap(), body)
            .unwrap()
            .encode()
    }

    const PATH: &str = "Safe/items/09090909090909090909090909090909.safe";

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        store::write_atomic(&dir.path().join("Safe/keyset.safe"), &keyset(SAFE, 1)).unwrap();
        dir
    }

    #[test]
    fn a_new_item_is_written_and_remembered() {
        let v = vault();
        let theirs = item(SAFE, 1, b"{}");
        let d = decide(v.path(), PATH, &theirs);
        assert_eq!(d, Incoming::Write);
        assert!(apply(v.path(), PATH, &theirs, &d).unwrap());
        assert_eq!(Seen::load(v.path()).item(&ITEM), 1);
    }

    #[test]
    fn a_newer_revision_replaces_ours_and_an_older_one_does_not() {
        let v = vault();
        store::write_atomic(&v.path().join(PATH), &item(SAFE, 3, b"{}")).unwrap();
        assert_eq!(decide(v.path(), PATH, &item(SAFE, 4, b"{}")), Incoming::Write);
        assert!(matches!(decide(v.path(), PATH, &item(SAFE, 2, b"{}")), Incoming::Refuse(_)));
    }

    /// The attack this module is for: the item file is gone here (deleted, or
    /// never written), and a peer hands back a revision older than one this
    /// device has already seen.
    #[test]
    fn a_replayed_old_revision_is_refused_even_with_no_local_file() {
        let v = vault();
        saw_item(v.path(), &ITEM, 5);
        assert!(matches!(decide(v.path(), PATH, &item(SAFE, 4, b"{}")), Incoming::Refuse(r) if r.contains("replay")));
        assert_eq!(decide(v.path(), PATH, &item(SAFE, 5, b"{}")), Incoming::Write);
    }

    #[test]
    fn marks_never_go_down() {
        let v = vault();
        saw_item(v.path(), &ITEM, 5);
        saw_item(v.path(), &ITEM, 2);
        assert_eq!(Seen::load(v.path()).item(&ITEM), 5);
    }

    #[test]
    fn another_safe_a_misnamed_item_and_junk_are_refused() {
        let v = vault();
        assert!(matches!(decide(v.path(), PATH, &item([8; 16], 1, b"{}")), Incoming::Refuse(r) if r.contains("different Safe")));
        let elsewhere = "Safe/items/00000000000000000000000000000000.safe";
        assert!(matches!(decide(v.path(), elsewhere, &item(SAFE, 1, b"{}")), Incoming::Refuse(r) if r.contains("different item")));
        assert!(matches!(decide(v.path(), PATH, b"not a safe file"), Incoming::Refuse(_)));
        assert!(matches!(decide(v.path(), "Safe/keyset.safe", &item(SAFE, 1, b"{}")), Incoming::Refuse(_)));
        assert!(matches!(decide(v.path(), "Safe/other.safe", &item(SAFE, 1, b"{}")), Incoming::Refuse(_)));
        assert!(matches!(decide(v.path(), "Safe/keyset.safe", &keyset([8; 16], 9)), Incoming::Refuse(_)));
    }

    /// Two devices, the same revision, different bytes. Each runs `decide`
    /// with the other's version; both must keep the same one and set the same
    /// one aside.
    #[test]
    fn a_tie_is_broken_the_same_way_on_both_devices() {
        let a_bytes = item(SAFE, 3, b"{\"a\":1}");
        let b_bytes = item(SAFE, 3, b"{\"b\":1}");

        let a = vault();
        store::write_atomic(&a.path().join(PATH), &a_bytes).unwrap();
        let b = vault();
        store::write_atomic(&b.path().join(PATH), &b_bytes).unwrap();

        let on_a = decide(a.path(), PATH, &b_bytes);
        let on_b = decide(b.path(), PATH, &a_bytes);
        apply(a.path(), PATH, &b_bytes, &on_a).unwrap();
        apply(b.path(), PATH, &a_bytes, &on_b).unwrap();

        let kept_a = std::fs::read(a.path().join(PATH)).unwrap();
        let kept_b = std::fs::read(b.path().join(PATH)).unwrap();
        assert_eq!(kept_a, kept_b, "the two devices kept different versions");
        let aside_a = conflicts_for(a.path());
        let aside_b = conflicts_for(b.path());
        assert_eq!(aside_a.len(), 1);
        assert_eq!(std::fs::read(&aside_a[0].1).unwrap(), std::fs::read(&aside_b[0].1).unwrap());
        assert_eq!(aside_a[0].0, ITEM);
    }

    #[test]
    fn identical_bytes_are_nothing_to_do() {
        let v = vault();
        let bytes = item(SAFE, 3, b"{}");
        store::write_atomic(&v.path().join(PATH), &bytes).unwrap();
        assert!(matches!(decide(v.path(), PATH, &bytes), Incoming::Refuse(r) if r.contains("already here")));
    }

    #[test]
    fn conflicts_and_leftovers_stay_on_the_device() {
        assert!(is_local_only("Safe/conflicts/09.abc.safe"));
        assert!(is_local_only("Safe/items/.x.safe.1a2b.tmp"));
        assert!(!is_local_only("Safe/items/09.safe"));
        assert!(!is_local_only("Safe/keyset.safe"));
        assert!(is_safe_path("Safe\\items\\x.safe"));
        assert!(!is_safe_path("Projects/Safe/x.md"));
    }
}
