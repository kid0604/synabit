//! A note's history meeting a second, unrelated history of the same note.
//!
//! A note's CRDT history lives only in the database. Delete `vault_cache.db`,
//! restore an archive into a new vault, or copy the vault folder to another
//! machine, and the files arrive with no history behind them. The scan (or
//! sync, whichever reaches the file first) then starts one from the bytes: a
//! single insert of the whole text, by this device's peer.
//!
//! That history shares nothing with the one every other device has been
//! keeping. Loro merges two unrelated inserts of the same text as two pieces of
//! text, so importing one into the other puts the note in twice — and the
//! doubled note is then published, and every device gets it.
//!
//! What this module does instead is adopt the vault's history and work out what,
//! if anything, the file on this device adds to it:
//!
//! - the file already says what the vault's history says → nothing to add;
//! - the file says what that history said at some earlier point (a stale copy,
//!   an old archive, a device that missed some syncs) → nothing to add either,
//!   and the newer text is taken;
//! - the file holds edits made on top of a version the vault's history also
//!   passed through → those edits are replayed from that version, which is a
//!   true three-way merge with the vault's later changes;
//! - none of those → there is no common ancestor to merge against, and any
//!   automatic answer would drop one side. The caller keeps the vault's text in
//!   place and sets this device's text aside as a conflict copy.

use std::collections::HashMap;

use loro::{Frontiers, LoroDoc, VersionVector, ID};

use super::crdt::{apply_node_update, node_text, read_fields};
use super::node_document;

/// More versions than this and the search for a common one is skipped, which
/// costs a conflict copy rather than a wrong merge. Each version is one
/// checkout; this bounds the work on a note with an enormous history.
const MAX_VERSIONS_SEARCHED: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adoption {
    /// The file already says what the arriving history says.
    Same,
    /// The file says what the arriving history said at an earlier point.
    Stale,
    /// The file's edits were replayed onto the arriving history from a version
    /// both had. The document now holds something the vault does not have yet.
    Merged,
    /// The file holds text the arriving history never had, with no version in
    /// common to merge from.
    Diverged,
}

pub struct Adopted {
    /// The document to keep: the arriving history, plus this device's edits
    /// when the outcome is [`Adoption::Merged`].
    pub doc: LoroDoc,
    pub outcome: Adoption,
    /// The file as the arriving history has it, before anything of ours.
    pub remote_text: String,
}

/// Two histories that have not a single operation's author in common.
///
/// Every device that has ever received a note carries the peers of whoever
/// wrote it, so two copies of one note that have been through sync always
/// overlap. An empty history is related to everything — it has nothing to
/// duplicate.
pub fn unrelated(local: &VersionVector, remote: &VersionVector) -> bool {
    let authors = |vv: &VersionVector| -> Vec<u64> {
        vv.iter().filter(|(_, c)| **c > 0).map(|(p, _)| *p).collect()
    };
    let ours = authors(local);
    let theirs = authors(remote);
    !ours.is_empty() && !theirs.is_empty() && !ours.iter().any(|p| theirs.contains(p))
}

/// Adopt `remote_snapshot` in place of `local`'s history, if the two are
/// unrelated. `None` means they are related and an ordinary import is right.
///
/// `local_file` is the file as it is on disk now — what this device would lose
/// — or `None` when there is no file. `peer` is the peer any replayed edits are
/// written under.
pub fn adopt_unrelated_history(
    local: &LoroDoc,
    remote_snapshot: &[u8],
    local_file: Option<&str>,
    peer: u64,
) -> Result<Option<Adopted>, String> {
    let remote = LoroDoc::new();
    remote
        .import(remote_snapshot)
        .map_err(|e| format!("CRDT import error: {e:?}"))?;

    if !unrelated(&local.oplog_vv(), &remote.oplog_vv()) {
        return Ok(None);
    }

    let remote_text = node_text(&remote);
    let done = |doc: LoroDoc, outcome| Ok(Some(Adopted { doc, outcome, remote_text: remote_text.clone() }));

    let Some(file) = local_file else {
        return done(remote, Adoption::Stale);
    };
    let wanted = file_fingerprint(file);
    if doc_fingerprint(&remote) == wanted {
        return done(remote, Adoption::Same);
    }

    let Some(remote_versions) = index_versions(&remote) else {
        return done(remote, Adoption::Diverged);
    };
    if remote_versions.contains_key(&wanted) {
        return done(remote, Adoption::Stale);
    }

    // Our edits were made on top of something. If the vault's history went
    // through that same something, it is the common ancestor a merge needs.
    let Some(base) = latest_shared_version(local, &remote_versions) else {
        return done(remote, Adoption::Diverged);
    };

    // A peer the vault has already written under would have its counters
    // reused by the replay. Not something this device's own peer can be once
    // the histories are unrelated, but cheap to refuse rather than corrupt.
    if remote.oplog_vv().get(&peer).is_some_and(|c| *c > 0) {
        return done(remote, Adoption::Diverged);
    }

    let base_vv = remote
        .frontiers_to_vv(&base)
        .ok_or_else(|| "a version found in the history is not in it".to_string())?;
    let at_base = LoroDoc::new();
    at_base
        .import_json_updates(remote.export_json_updates(&VersionVector::default(), &base_vv))
        .map_err(|e| format!("CRDT import error: {e:?}"))?;
    at_base
        .set_peer_id(peer)
        .map_err(|e| format!("set_peer_id error: {e:?}"))?;
    apply_node_update(&at_base, file)?;
    remote
        .import(&at_base.export_from(&base_vv))
        .map_err(|e| format!("CRDT import error: {e:?}"))?;

    done(remote, Adoption::Merged)
}

/// What a file says, as the two things the document keeps: its fields and its
/// body. Compared this way rather than byte for byte because a device that
/// received a note rebuilds its frontmatter, and may lay it out differently from
/// the device that wrote it while saying exactly the same thing.
fn fingerprint(fields: &std::collections::BTreeMap<String, String>, body: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for (key, value) in fields {
        hasher.update(&(key.len() as u64).to_le_bytes());
        hasher.update(key.as_bytes());
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.update(b"\0body\0");
    hasher.update(body.as_bytes());
    *hasher.finalize().as_bytes()
}

fn file_fingerprint(text: &str) -> [u8; 32] {
    let parts = node_document::split(text);
    fingerprint(&parts.fields, &parts.body)
}

fn doc_fingerprint(doc: &LoroDoc) -> [u8; 32] {
    let fields = read_fields(doc);
    let body = doc.get_text("content").to_string();
    if fields.is_empty() {
        // Not migrated yet: the text is the whole file, frontmatter included.
        return file_fingerprint(&body);
    }
    fingerprint(&fields, &body)
}

/// Every version a history has passed through at the end of one of its
/// operations, or `None` when there are too many to look at.
///
/// Changes are no use for this: Loro folds consecutive commits by one peer
/// into a single change, so a whole afternoon of saves can be one change.
/// Operations are the finest grain there is.
fn versions(doc: &LoroDoc) -> Option<Vec<Frontiers>> {
    let vv = doc.oplog_vv();
    let json = doc.export_json_updates(&VersionVector::default(), &vv);

    let mut starts: HashMap<u64, Vec<i32>> = HashMap::new();
    for change in &json.changes {
        // Change ids in the JSON form name their peer by index.
        let peer = *json.peers.get(change.id.peer as usize)?;
        let list = starts.entry(peer).or_default();
        list.extend(change.ops.iter().map(|op| op.counter));
    }

    let mut ends = Vec::new();
    for (peer, mut list) in starts {
        list.sort_unstable();
        list.dedup();
        for pair in list.windows(2) {
            ends.push((peer, pair[1] - 1));
        }
        if !list.is_empty() {
            ends.push((peer, vv.get(&peer).copied().unwrap_or(0) - 1));
        }
        if ends.len() > MAX_VERSIONS_SEARCHED {
            return None;
        }
    }
    // Latest first within each peer, which is the order a caller looking for
    // the most recent shared version wants.
    ends.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Some(
        ends.into_iter()
            .filter(|(_, counter)| *counter >= 0)
            .map(|(peer, counter)| Frontiers::from(ID::new(peer, counter)))
            .collect(),
    )
}

/// What the history said at each of its versions, keyed by fingerprint.
fn index_versions(doc: &LoroDoc) -> Option<HashMap<[u8; 32], Frontiers>> {
    let probe = doc.fork();
    let mut index = HashMap::new();
    for version in versions(doc)? {
        if probe.checkout(&version).is_ok() {
            index.entry(doc_fingerprint(&probe)).or_insert(version);
        }
    }
    Some(index)
}

/// The most recent version of `local` that the other history also passed
/// through, as that history's own version.
fn latest_shared_version(
    local: &LoroDoc,
    other: &HashMap<[u8; 32], Frontiers>,
) -> Option<Frontiers> {
    let probe = local.fork();
    for version in versions(local)? {
        if probe.checkout(&version).is_ok() {
            if let Some(found) = other.get(&doc_fingerprint(&probe)) {
                return Some(found.clone());
            }
        }
    }
    None
}

/// `text` with its `node_id` taken out, for a copy that must not claim to be
/// the note it was copied from.
pub fn without_identity(text: &str) -> String {
    let mut parts = node_document::split(text);
    if parts.fields.remove("node_id").is_none() {
        return text.to_string();
    }
    parts.order.retain(|k| k != "node_id");
    node_document::rebuild(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(peer: u64, file: &str) -> LoroDoc {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).unwrap();
        apply_node_update(&doc, file).unwrap();
        doc
    }

    fn edit(doc: &LoroDoc, file: &str) {
        apply_node_update(doc, file).unwrap();
    }

    const V1: &str = "---\nnode_id: n1\ntitle: Plan\n---\n# Plan\n\nHello world.\n";
    const V2: &str = "---\nnode_id: n1\ntitle: Plan\n---\n# Plan\n\nHello world.\nFrom A.\n";

    #[test]
    fn the_failure_this_exists_for() {
        // Two unrelated inserts of the same text merge into the text twice.
        let a = doc_with(1, V1);
        let b = doc_with(2, V1);
        b.import(&a.export_snapshot()).unwrap();
        assert_ne!(node_text(&b), V1, "Loro no longer doubles unrelated inserts");
    }

    #[test]
    fn a_shared_history_is_left_to_the_ordinary_import() {
        let a = doc_with(1, V1);
        let b = LoroDoc::new();
        b.import(&a.export_snapshot()).unwrap();
        b.set_peer_id(2).unwrap();
        edit(&b, V2);
        assert!(adopt_unrelated_history(&b, &a.export_snapshot(), Some(V2), 2)
            .unwrap()
            .is_none());
    }

    #[test]
    fn the_same_text_adopts_without_change() {
        let a = doc_with(1, V1);
        let b = doc_with(2, V1);
        let got = adopt_unrelated_history(&b, &a.export_snapshot(), Some(V1), 2)
            .unwrap()
            .unwrap();
        assert_eq!(got.outcome, Adoption::Same);
        assert_eq!(node_text(&got.doc), V1);
    }

    #[test]
    fn an_older_copy_takes_the_newer_text() {
        let a = doc_with(1, V1);
        edit(&a, V2);
        let b = doc_with(2, V1);
        let got = adopt_unrelated_history(&b, &a.export_snapshot(), Some(V1), 2)
            .unwrap()
            .unwrap();
        assert_eq!(got.outcome, Adoption::Stale);
        assert_eq!(node_text(&got.doc), V2);
    }

    #[test]
    fn edits_on_an_older_copy_merge_with_the_newer_text() {
        let a = doc_with(1, V1);
        edit(&a, V2);
        let b = doc_with(2, V1);
        let ours = V1.replace("# Plan", "# Plan, revised");
        edit(&b, &ours);
        let got = adopt_unrelated_history(&b, &a.export_snapshot(), Some(&ours), 2)
            .unwrap()
            .unwrap();
        assert_eq!(got.outcome, Adoption::Merged);
        assert_eq!(node_text(&got.doc), V2.replace("# Plan", "# Plan, revised"));
        assert_eq!(got.remote_text, V2);
    }

    #[test]
    fn text_with_no_common_version_is_reported_as_diverged() {
        let a = doc_with(1, V1);
        let other = "---\nnode_id: n1\n---\nSomething else entirely.\n";
        let b = doc_with(2, other);
        let got = adopt_unrelated_history(&b, &a.export_snapshot(), Some(other), 2)
            .unwrap()
            .unwrap();
        assert_eq!(got.outcome, Adoption::Diverged);
        assert_eq!(node_text(&got.doc), V1, "the vault's text stays in place");
    }

    #[test]
    fn a_copy_set_aside_does_not_claim_the_note() {
        let copy = without_identity(V2);
        assert!(!copy.contains("node_id"), "{copy}");
        assert!(copy.contains("title: Plan") && copy.contains("From A."), "{copy}");
    }
}
