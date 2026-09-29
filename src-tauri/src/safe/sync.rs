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
//! header** alone — which Safe, which item, which revision — and from which
//! versions this device has held. No key is needed, so it works while the
//! Safe is locked, which is most of the time sync runs.
//!
//! # The rule (section 5.6 of the design, as revised in section 24)
//!
//! * A different Safe, a header that does not match the file name, bytes that
//!   do not parse, or a revision that jumps implausibly far: **refused**, and
//!   the local file is left alone.
//! * A version this device has already held — by the hash of its bytes: an old
//!   one handed back. **Refused.** That is a replay, and the heart of why this
//!   module exists.
//! * A higher revision: **written**, and ours **set aside**. Revisions are
//!   counted per device, so "higher" does not mean "made after seeing ours":
//!   the open Safe folds ours back in (`ItemBody::absorb`, newest edit wins,
//!   every other password kept in history) and finds nothing to add when theirs
//!   did come after.
//! * A lower revision we never held: another device's edit made without seeing
//!   ours. Ours stays; **theirs is set aside** to be folded in the same way —
//!   never thrown away, which is how two devices once kept different passwords
//!   for good.
//! * The same revision with different bytes: both devices keep the version
//!   whose bytes hash higher, so they agree without talking, and set the other
//!   aside.
//!
//! Folding happens when the Safe is next open (`Unlocked::resolve_conflicts`);
//! its result is a new revision above both, which settles the other device
//! the same way. An item deleted on one device and edited on the other at the
//! same time comes back — losing an edit is worse than a delete to redo — but
//! a delete made after the edit arrived is a delete.
//!
//! # Where "what this device has seen" lives
//!
//! `.synabit/safe/seen.json`, per device and never synced — a dotdir. In the
//! clear, unlike the design's first draft: it has to be read while the Safe
//! is locked, and it holds item ids, revision numbers and hashes of files
//! that sit encrypted beside it. Losing it costs only rollback protection
//! until the items are next written or read.

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
    rel.starts_with("Safe/conflicts/") || (rel.starts_with(PREFIX) && (rel.ends_with(".tmp") || rel.ends_with(".new")))
}

// ─── what this device has seen ──────────────────────────

/// How many versions of each file are remembered as held.
const KNOWN_PER_FILE: usize = 64;

/// A revision further than this above anything this device has seen for the
/// file is not a real one: nobody edits an item a million times offline. A
/// forged header claiming `u64::MAX` would otherwise pin the mark there and
/// every real revision after it would look like a replay.
pub const MAX_JUMP: u64 = 1_000_000;

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Seen {
    #[serde(default)]
    pub keyset: u64,
    #[serde(default)]
    pub items: BTreeMap<String, u64>,
    /// Short hashes of the versions held, newest last, by item id or `keyset`.
    #[serde(default)]
    pub known: BTreeMap<String, Vec<String>>,
}

fn seen_path(vault: &Path) -> PathBuf {
    vault.join(".synabit").join("safe").join("seen.json")
}

/// One writer of `seen.json` at a time in this process: sync and the open
/// Safe both move its marks.
static SEEN_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn version_tag(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex()[..32].to_string()
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

    /// Whether this device has held exactly these bytes for `key`.
    pub fn held(&self, key: &str, bytes: &[u8]) -> bool {
        self.known.get(key).is_some_and(|tags| tags.contains(&version_tag(bytes)))
    }

    fn hold(&mut self, key: &str, bytes: &[u8]) -> bool {
        let tag = version_tag(bytes);
        let tags = self.known.entry(key.to_string()).or_default();
        if tags.contains(&tag) {
            return false;
        }
        tags.push(tag);
        if tags.len() > KNOWN_PER_FILE {
            tags.remove(0);
        }
        true
    }
}

/// Read, change and write `seen.json` as one step.
fn update(vault: &Path, change: impl FnOnce(&mut Seen) -> bool) {
    let _one = SEEN_WRITE.lock().unwrap_or_else(|p| p.into_inner());
    let mut seen = Seen::load(vault);
    if change(&mut seen) {
        seen.save(vault);
    }
}

/// Remember that `id` has been seen at `revision`. Never lowers a mark.
pub fn saw_item(vault: &Path, id: &ItemId, revision: u64) {
    update(vault, |seen| {
        let mark = seen.items.entry(hex::encode(id)).or_default();
        if revision > *mark {
            *mark = revision;
            true
        } else {
            false
        }
    });
}

pub fn saw_keyset(vault: &Path, revision: u64) {
    update(vault, |seen| {
        if revision > seen.keyset {
            seen.keyset = revision;
            true
        } else {
            false
        }
    });
}

/// Remember holding these bytes of a file: written here, or taken from sync.
/// `key` is the item's id in hex, or `keyset`.
pub fn held(vault: &Path, key: &str, bytes: &[u8]) {
    update(vault, |seen| seen.hold(key, bytes));
}

/// Remember holding whatever is on disk at `rel_path` now.
pub fn held_file(vault: &Path, key: &str, rel_path: &str) {
    if let Ok(bytes) = std::fs::read(vault.join(rel_path)) {
        held(vault, key, &bytes);
    }
}

// ─── the decision ────────────────────────────────────────

/// What to do with another device's version of a file under `Safe/`.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming {
    /// Write theirs where there was nothing.
    Write,
    /// Leave ours; say why in the log.
    Refuse(String),
    /// Two versions, both kept: `write` says whether theirs takes the file's
    /// place; the other goes to `aside`, to be folded in when the Safe opens.
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

/// The revision in a Safe file's cleartext header, if it is one.
pub fn revision_of(bytes: &[u8]) -> Option<u64> {
    match header(bytes)? {
        Header::Keyset { revision, .. } | Header::Item { revision, .. } => Some(revision),
    }
}

/// Where a version is set aside. `.over` marks one that a higher revision
/// replaced — for a delete, that says the delete came after it.
fn aside_path(vault: &Path, stem: &str, loser: &[u8], replaced_by_higher: bool) -> PathBuf {
    let tag = &blake3::hash(loser).to_hex()[..16];
    let kind = if replaced_by_higher { ".over" } else { "" };
    vault.join(PREFIX).join(CONFLICTS_DIR).join(format!("{stem}.{tag}{kind}.safe"))
}

/// Whether a version set aside was replaced by a higher revision, rather than
/// arriving beside ours.
pub fn replaced_by_higher(aside: &Path) -> bool {
    aside.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".over.safe"))
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

    let (safe_id, revision, floor, stem, is_keyset) = match (&their_header, rel.as_str()) {
        (Header::Keyset { safe_id, revision }, "Safe/keyset.safe") => (*safe_id, *revision, seen.keyset, "keyset".to_string(), true),
        (Header::Item { safe_id, item_id, revision }, path) if path.starts_with("Safe/items/") => {
            let named = path.strip_prefix("Safe/items/").and_then(|n| n.strip_suffix(".safe"));
            if named != Some(hex::encode(item_id).as_str()) {
                return Incoming::Refuse(format!("{rel}: holds a different item than its name says"));
            }
            (*safe_id, *revision, seen.item(item_id), hex::encode(item_id), false)
        }
        _ => return Incoming::Refuse(format!("{rel}: not a place Safe files arrive")),
    };

    if let Some(ours_id) = local_safe_id {
        if ours_id != safe_id {
            return Incoming::Refuse(format!("{rel}: belongs to a different Safe"));
        }
    }
    let our_revision = ours.as_deref().and_then(revision_of);
    let highest = floor.max(our_revision.unwrap_or(0));
    if revision > highest.saturating_add(MAX_JUMP) {
        return Incoming::Refuse(format!("{rel}: revision {revision} is too far above {highest} to be real"));
    }
    if ours.as_deref() == Some(theirs) {
        return Incoming::Refuse(format!("{rel}: already here"));
    }
    if seen.held(&stem, theirs) {
        return Incoming::Refuse(format!("{rel}: a version this device already had — refused as a replay"));
    }

    let Some(ours) = ours else {
        // Nothing here to fold into: below what this device has seen, it is
        // an old version handed back.
        if revision < floor {
            return Incoming::Refuse(format!("{rel}: revision {revision} is older than {floor}, already seen here — refused as a replay"));
        }
        return Incoming::Write;
    };
    let Some(our_revision) = our_revision else {
        // Ours does not parse: theirs, which does, is the better copy — but
        // ours is kept aside rather than destroyed.
        return Incoming::Conflict { write: true, aside: aside_path(vault, &stem, &ours, true) };
    };

    match revision.cmp(&our_revision) {
        std::cmp::Ordering::Greater => Incoming::Conflict { write: true, aside: aside_path(vault, &stem, &ours, true) },
        // A keyset is not folded: an older one we never held is a password
        // changed there before a change here, and the newer change stands.
        std::cmp::Ordering::Less if is_keyset => {
            Incoming::Refuse(format!("{rel}: ours is newer ({our_revision} > {revision})"))
        }
        std::cmp::Ordering::Less => Incoming::Conflict { write: false, aside: aside_path(vault, &stem, theirs, false) },
        std::cmp::Ordering::Equal => {
            // Both devices run this with the two versions swapped and must
            // reach the same answer: keep the higher hash.
            let theirs_wins = blake3::hash(theirs).as_bytes() > blake3::hash(&ours).as_bytes();
            let loser: &[u8] = if theirs_wins { &ours } else { theirs };
            Incoming::Conflict { write: theirs_wins, aside: aside_path(vault, &stem, loser, false) }
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
            log::info!("[Safe] sync: two versions of {rel_path}; one kept aside to fold in when the Safe opens");
            *write
        }
    };
    if !matches!(decision, Incoming::Refuse(_)) {
        // Held now, written or set aside: the same bytes again are a replay.
        match header(theirs) {
            Some(Header::Keyset { .. }) => held(vault, "keyset", theirs),
            Some(Header::Item { item_id, .. }) => held(vault, &hex::encode(item_id), theirs),
            None => {}
        }
    }
    if wrote {
        match header(theirs) {
            Some(Header::Keyset { revision, .. }) => saw_keyset(vault, revision),
            Some(Header::Item { item_id, revision, .. }) => saw_item(vault, &item_id, revision),
            None => {}
        }
    }
    Ok(wrote)
}

/// Keysets set aside — replaced by another device's, or tied with one — newest
/// first. Kept so that a password changed elsewhere, or a keyset a peer
/// forged, can be undone by the user at unlock.
pub fn keyset_asides(vault: &Path) -> Vec<PathBuf> {
    let dir = vault.join(PREFIX).join(CONFLICTS_DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("keyset."))
        .map(|e| (e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), e.path()))
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().map(|(_, p)| p).collect()
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

    /// Revisions are counted per device: a higher one replaces ours with ours
    /// kept aside, a lower one we never held is kept aside beside ours — both
    /// folded when the Safe opens. Neither is thrown away.
    #[test]
    fn a_newer_revision_replaces_ours_and_an_older_unknown_one_is_kept_aside() {
        let v = vault();
        let ours = item(SAFE, 3, b"{}");
        store::write_atomic(&v.path().join(PATH), &ours).unwrap();
        assert!(matches!(decide(v.path(), PATH, &item(SAFE, 4, b"{}")), Incoming::Conflict { write: true, aside } if replaced_by_higher(&aside)));
        let older = item(SAFE, 2, b"{}");
        let d = decide(v.path(), PATH, &older);
        assert!(matches!(&d, Incoming::Conflict { write: false, aside } if !replaced_by_higher(aside)), "{d:?}");
        apply(v.path(), PATH, &older, &d).unwrap();
        assert_eq!(std::fs::read(v.path().join(PATH)).unwrap(), ours, "ours stays");
        assert_eq!(conflicts_for(v.path()).len(), 1);
        // The same version again is one this device has held: a replay.
        assert!(matches!(decide(v.path(), PATH, &older), Incoming::Refuse(r) if r.contains("replay")));
    }

    /// A forged header cannot pin the mark: a revision far past anything seen
    /// is not taken at all.
    #[test]
    fn an_implausible_revision_is_refused() {
        let v = vault();
        store::write_atomic(&v.path().join(PATH), &item(SAFE, 3, b"{}")).unwrap();
        assert!(matches!(decide(v.path(), PATH, &item(SAFE, u64::MAX, b"{}")), Incoming::Refuse(r) if r.contains("too far")));
        assert!(matches!(decide(v.path(), PATH, &item(SAFE, 3 + MAX_JUMP, b"{}")), Incoming::Conflict { write: true, .. }));
        assert!(matches!(decide(v.path(), "Safe/keyset.safe", &keyset(SAFE, u64::MAX)), Incoming::Refuse(_)));
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
        assert!(is_local_only("Safe/keyset.safe.1a2b3c4d.new"), "a crashed create's leftover");
        assert!(!is_local_only("Notes/draft.tmp"), "a note of the user's is not Safe's leftover");
        assert!(!is_local_only("Projects/idea.new"));
        assert!(is_safe_path("Safe\\items\\x.safe"));
        assert!(!is_safe_path("Projects/Safe/x.md"));
    }
}
