//! What a file *is*, as distinct from where it happens to be.
//!
//! The Files app used to answer both questions with one string: the absolute
//! path was the conflict key when indexing, the lookup key when reading, and
//! the identity a tag was attached to. A path cannot carry all three.
//!
//! - Rename a file in Finder and the path changes, so the node behind it is
//!   orphaned and every tag on it is lost.
//! - `/Users/anh/…` means nothing on another machine, so metadata could never
//!   travel: a tag applied on the laptop was invisible to the phone.
//! - Two copies of the same photo are two unrelated items, so tagging one says
//!   nothing about the other.
//!
//! Identity here is a BLAKE3 digest of the contents. All three problems
//! dissolve at once: a moved file hashes the same, a file copied to another
//! device hashes the same, and duplicates *are* the same item — which is how
//! DEVONthink and Zotero have always modelled it.
//!
//! # Why the node id is a path anyway
//!
//! The id is `Files/<digest>.md`, which looks like a step backwards until you
//! notice what it buys: it is the path the node's metadata file will occupy in
//! the vault *if the user ever attaches anything to it*. A file nobody has
//! tagged has a row in the index and no file on disk, so it costs nothing and
//! syncs nowhere. The moment a tag is added, `write_node_file` writes that exact
//! path and the same row becomes a synced document.
//!
//! One identity, one row, and the choice of whether it travels is made by the
//! user rather than by the scanner.

use std::path::Path;

/// Files at or below this size are digested whole.
///
/// Above it, reading every byte to notice that a 40GB video has not changed is
/// a bad trade — see `tiered_key`.
const FULL_HASH_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// How much is read from each end of a file too large to digest whole.
const TIER_WINDOW: usize = 1024 * 1024;

/// A modification time this recent is re-hashed even when size and mtime match
/// what was cached.
///
/// Filesystems record modification times at limited resolution, so a write
/// landing in the same tick as the one recorded is invisible to a metadata
/// comparison. Waiting until the tick has demonstrably passed closes that
/// window and costs nothing in practice: a file touched two seconds ago is
/// exactly the file worth hashing.
///
/// Lifted from `sync::core::change`, which learned it the hard way.
const STAT_SETTLE_MS: i64 = 2_000;

/// The vault path a file's metadata occupies, derived from its contents.
pub fn node_id_for(content_hash: &str) -> String {
    format!("Files/{content_hash}.md")
}

/// The digest back out of a node id, for a node that has one.
pub fn hash_from_node_id(node_id: &str) -> Option<&str> {
    node_id
        .strip_prefix("Files/")
        .and_then(|rest| rest.strip_suffix(".md"))
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Size and modification time, in the units the cache stores.
pub fn stat_of(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some((meta.len(), mtime))
}

/// May a cached digest stand in for reading this file again?
///
/// Only when size and modification time both match *and* that timestamp is old
/// enough that no write could still be hiding inside the same filesystem tick.
/// Getting this wrong in the permissive direction means an edited file keeps its
/// old identity — and with it, another file's tags — so every uncertain case
/// answers `false`.
pub fn cache_is_usable(observed: (u64, i64), cached: (u64, i64), now_ms: i64) -> bool {
    observed == cached && now_ms.saturating_sub(cached.1) > STAT_SETTLE_MS
}

/// The content digest of a file, read from disk.
pub fn content_key(path: &Path, size: u64) -> Option<String> {
    if size > FULL_HASH_LIMIT {
        tiered_key(path, size)
    } else {
        full_key(path)
    }
}

fn full_key(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 65536];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return None,
        };
    }
    Some(hasher.finalize().to_hex().to_string())
}

/// A digest of a very large file's size and both of its ends.
///
/// Reading 40GB of video to notice nothing has changed would dominate every
/// scan, and the cases this misses are not ones that occur: for two distinct
/// files to collide here they must have identical lengths, identical first
/// megabytes and identical last megabytes. Container formats put their headers
/// at one end and their indexes at the other, so a genuine edit moves at least
/// one of them.
///
/// The size is mixed in first so that a truncation with untouched ends still
/// changes the answer.
fn tiered_key(path: &Path, size: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&size.to_le_bytes());

    let mut head = vec![0u8; TIER_WINDOW];
    file.read_exact(&mut head).ok()?;
    hasher.update(&head);

    file.seek(SeekFrom::End(-(TIER_WINDOW as i64))).ok()?;
    let mut tail = vec![0u8; TIER_WINDOW];
    file.read_exact(&mut tail).ok()?;
    hasher.update(&tail);

    Some(hasher.finalize().to_hex().to_string())
}

/// Moving a vault from path identity to content identity.
///
/// Every file node written before this change has a UUID for an id and an
/// absolute path in its properties. Both have to go, and the tags hanging off
/// them must not.
///
/// # Why nothing is destroyed when a file cannot be found
///
/// A legacy node whose file is missing cannot be re-identified: identity now
/// comes from contents, and there are no contents to read. The tempting move is
/// to drop it. The right one is to leave it exactly as it is and try again
/// later — an external drive that is unplugged today is plugged in tomorrow,
/// and the tags on those files are precisely what this whole change exists to
/// protect. Leaving them costs a row; deleting them costs the user's work.
///
/// That also makes this safe to re-run, which is why it needs no flag: a
/// migrated node no longer looks legacy, so a second pass sees only what the
/// first could not resolve.
pub mod migration {
    use super::*;
    use crate::db::DbBridge;
    use crate::error::AppResult;

    /// What a migration would do, without doing it.
    #[derive(Debug, Default, Clone, serde::Serialize)]
    pub struct MigrationPlan {
        /// Nodes still carrying a UUID identity.
        pub legacy: usize,
        /// Of those, ones whose file is present and can be re-identified.
        pub resolvable: usize,
        /// Ones whose file is missing, which are left untouched for another day.
        pub unresolvable: usize,
        /// Ones carrying tags or people — what is actually at stake.
        pub carrying_metadata: usize,
        /// Legacy nodes that turn out to be copies of one another.
        pub merges: usize,
    }

    /// The fields a rescan cannot recover, and so must be carried by hand.
    const USER_FIELDS: [&str; 3] = ["tags", "people", "linked_projects"];

    fn is_legacy(node: &crate::models::node::NodeMetadata) -> bool {
        hash_from_node_id(&node.id).is_none()
    }

    fn path_of(node: &crate::models::node::NodeMetadata) -> Option<String> {
        node.properties
            .get("path")
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    fn has_metadata(node: &crate::models::node::NodeMetadata) -> bool {
        USER_FIELDS.iter().any(|f| {
            node.properties
                .get(*f)
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty())
        })
    }

    /// Report what a migration would change. Reads only.
    pub fn plan(db: &DbBridge) -> AppResult<MigrationPlan> {
        let mut plan = MigrationPlan::default();
        let mut targets = std::collections::HashSet::new();

        for node in db.get_nodes_by_type("file")? {
            if !is_legacy(&node) {
                continue;
            }
            plan.legacy += 1;
            if has_metadata(&node) {
                plan.carrying_metadata += 1;
            }

            let resolved = path_of(&node)
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_file())
                .and_then(|p| stat_of(&p).map(|(size, _)| (p, size)))
                .and_then(|(p, size)| content_key(&p, size));

            match resolved {
                Some(hash) => {
                    plan.resolvable += 1;
                    if !targets.insert(hash) {
                        plan.merges += 1;
                    }
                }
                None => plan.unresolvable += 1,
            }
        }
        Ok(plan)
    }

    /// Re-identify every legacy file node whose file can still be read.
    ///
    /// Returns how many were migrated.
    pub fn apply(db: &DbBridge) -> AppResult<usize> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut migrated = 0;

        for node in db.get_nodes_by_type("file")? {
            if !is_legacy(&node) {
                continue;
            }
            let Some(path) = path_of(&node) else { continue };
            let file = std::path::Path::new(&path);
            let Some((size, mtime_ms)) = stat_of(file) else {
                continue;
            };
            if !file.is_file() {
                continue;
            }
            let Some(hash) = content_key(file, size) else {
                continue;
            };

            let node_id = node_id_for(&hash);
            let mut carried = db.get_node(&node_id).ok().flatten().unwrap_or_else(|| {
                let mut fresh = node.clone();
                fresh.id = node_id.clone();
                fresh
            });

            // Union rather than overwrite. Two legacy nodes may be two copies
            // of one file, each tagged differently, and arriving second is no
            // reason to lose the first one's tags.
            if let Some(props) = carried.properties.as_object_mut() {
                props.insert("path".into(), serde_json::json!(path));
                for field in USER_FIELDS {
                    let merged = union_of(
                        carried_values(&node, field),
                        props.get(field).map(|v| v.clone()).unwrap_or(serde_json::Value::Null),
                    );
                    if !merged.is_empty() {
                        props.insert(field.into(), serde_json::json!(merged));
                    }
                }
            }
            carried.id = node_id.clone();

            db.upsert_node(&carried)?;
            db.upsert_file_location(
                &crate::db::FileLocation {
                    abs_path: path.clone(),
                    node_id: node_id.clone(),
                    size: size as i64,
                    mtime_ms,
                },
                now_ms,
            )?;
            db.remember_content_hash(&path, size, mtime_ms, &hash, now_ms)?;

            // The old row and everything keyed to it. Edges are cleared by the
            // identity they were recorded under, which for a legacy node is its
            // own id; the new node's links are rebuilt by the next scan.
            db.delete_node(&node.id)?;
            db.delete_search_entry(&node.id);
            db.delete_node_edges_by_source(node.stable_id())?;
            migrated += 1;
        }

        Ok(migrated)
    }

    fn carried_values(
        node: &crate::models::node::NodeMetadata,
        field: &str,
    ) -> serde_json::Value {
        node.properties
            .get(field)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    }

    fn union_of(a: serde_json::Value, b: serde_json::Value) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for value in [a, b] {
            let Some(items) = value.as_array() else { continue };
            for item in items {
                let Some(text) = item.as_str() else { continue };
                if !out.iter().any(|existing| existing == text) {
                    out.push(text.to_string());
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_contents_at_different_paths_are_one_identity() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("hoa-don.pdf");
        let b = dir.path().join("sao-luu/hoa-don-copy.pdf");
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        std::fs::write(&a, "cùng một nội dung").unwrap();
        std::fs::write(&b, "cùng một nội dung").unwrap();

        assert_eq!(
            content_key(&a, 24).unwrap(),
            content_key(&b, 24).unwrap(),
            "two copies of one file are one item, which is the whole point"
        );
    }

    #[test]
    fn editing_a_file_changes_its_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ghi-chu.txt");
        std::fs::write(&path, "bản đầu").unwrap();
        let before = content_key(&path, 8).unwrap();
        std::fs::write(&path, "bản sau").unwrap();

        assert_ne!(before, content_key(&path, 8).unwrap());
    }

    #[test]
    fn a_node_id_round_trips_through_its_digest() {
        let digest = "a".repeat(64);
        let id = node_id_for(&digest);
        assert_eq!(id, format!("Files/{digest}.md"));
        assert_eq!(hash_from_node_id(&id), Some(digest.as_str()));
    }

    /// Ids from every other part of the app pass through here — a note is
    /// `Notes/x.md` — and must not be mistaken for a file identity.
    #[test]
    fn only_a_real_digest_is_read_back_as_one() {
        assert_eq!(hash_from_node_id("Notes/nhat-ky.md"), None);
        assert_eq!(hash_from_node_id("Files/khong-phai-hash.md"), None);
        assert_eq!(hash_from_node_id("Files/abc.md"), None);
        assert_eq!(hash_from_node_id(&format!("Files/{}.md", "z".repeat(64))), None);
    }

    // ── The stat cache ────────────────────────────────────────

    /// The bug this rule exists to prevent: a file written in the same
    /// filesystem tick as the one recorded looks unchanged, so it would keep
    /// the identity — and the tags — of what it used to be.
    #[test]
    fn a_file_touched_a_moment_ago_is_never_taken_from_cache() {
        let now = 1_000_000_i64;
        let just_now = (500u64, now - 500);
        assert!(!cache_is_usable(just_now, just_now, now));
    }

    #[test]
    fn a_settled_file_with_matching_stats_is_taken_from_cache() {
        let now = 1_000_000_i64;
        let settled = (500u64, now - 60_000);
        assert!(cache_is_usable(settled, settled, now));
    }

    #[test]
    fn a_changed_size_or_time_defeats_the_cache() {
        let now = 1_000_000_i64;
        let cached = (500u64, now - 60_000);
        assert!(!cache_is_usable((501, cached.1), cached, now));
        assert!(!cache_is_usable((500, cached.1 + 1), cached, now));
    }

    // ── Very large files ──────────────────────────────────────

    /// A truncation that leaves both ends untouched still has to register, or
    /// two different videos share one identity.
    #[test]
    fn the_tiered_digest_notices_a_change_in_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("phim.bin");
        let body = vec![7u8; TIER_WINDOW * 2 + 4096];
        std::fs::write(&path, &body).unwrap();

        let long = tiered_key(&path, body.len() as u64).unwrap();
        let short = tiered_key(&path, body.len() as u64 - 1).unwrap();

        assert_ne!(long, short);
    }

    /// Both digests are BLAKE3 hex, so nothing downstream needs to know which
    /// route a given file took.
    #[test]
    fn both_routes_produce_the_same_shape_of_digest() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("nho.txt");
        std::fs::write(&small, "x").unwrap();
        let big = dir.path().join("to.bin");
        std::fs::write(&big, vec![0u8; TIER_WINDOW * 2 + 1]).unwrap();

        let a = content_key(&small, 1).unwrap();
        let b = tiered_key(&big, (TIER_WINDOW * 2 + 1) as u64).unwrap();

        assert_eq!(a.len(), 64);
        assert_eq!(b.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(b.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
