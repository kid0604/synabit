//! Two copies of a whiteboard, item by item.
//!
//! A board used to be settled whole: whichever copy carried the later
//! `metadata.updated_at` replaced the other, so two devices that each added a
//! box to the same board kept one box between them. Every item on a board
//! carries the time it last changed (`updated`, in milliseconds), and every
//! save records what was deleted and when (`metadata.deleted`), so the two
//! copies can be combined instead:
//!
//! - an item on one side only is kept — unless the other side deleted it
//!   after it last changed;
//! - an item on both sides is taken from whichever changed it last;
//! - a line survives only while both of its ends do.
//!
//! There is no common ancestor to compare against here — the stored document
//! is rewritten on push as well as on pull, so it is the last copy sent or
//! received, not the copy both sides grew from. The stamps are what make the
//! merge possible without one.
//!
//! The board's own fields (title, tags, each key of `metadata`) are settled
//! one by one, by when each last changed (`metadata.stamps`, written by the
//! app when it saves). A field neither copy has a stamp for comes from the
//! copy saved last, as it always did — except a `metadata` key only one copy
//! has, which is kept: a project linked on one device is not unlinked by the
//! other device saving later.

use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// How long a deletion is remembered. A device away for longer than this may
/// bring a deleted item back, which is the safer way to be wrong.
const FORGET_DELETIONS_AFTER_MS: i64 = 180 * 24 * 60 * 60 * 1000;

/// The board format this build understands; see `boardFile.ts`.
pub const BOARD_SCHEMA_VERSION: i64 = 1;

pub fn is_board(rel_path: &str) -> bool {
    rel_path.ends_with(".whiteboard.json")
}

/// What became of a merge.
pub enum BoardMerge {
    /// The combined board.
    Merged(Value),
    /// The copies cannot be combined — one was written by a newer build. The
    /// newer copy is kept as the board; the other is kept beside it, as a
    /// conflict copy, so nothing is lost.
    KeepWithConflictCopy { keep: String, copy: String },
    /// One side is not a board at all; the other is taken as it is.
    Take(String),
}

fn stamp(item: &Value) -> i64 {
    item.get("updated").and_then(Value::as_i64).unwrap_or(0)
}

fn version(board: &Value) -> i64 {
    board.get("schemaVersion").and_then(Value::as_i64).unwrap_or(0)
}

fn saved_at(board: &Value) -> String {
    board
        .get("metadata")
        .and_then(|m| m.get("updated_at"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Every deletion either copy remembers, the later time for each id.
fn deletions(local: &Value, remote: &Value, now_ms: i64) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    for board in [local, remote] {
        let Some(map) = board.get("metadata").and_then(|m| m.get("deleted")).and_then(Value::as_object) else {
            continue;
        };
        for (id, at) in map {
            let at = at.as_i64().unwrap_or(0);
            if now_ms - at > FORGET_DELETIONS_AFTER_MS {
                continue;
            }
            let entry = out.entry(id.clone()).or_insert(at);
            *entry = (*entry).max(at);
        }
    }
    out
}

/// The items of one kind (`nodes` or `edges`), combined.
fn merge_items(local: &[Value], remote: &[Value], deleted: &HashMap<String, i64>, remote_wins_ties: bool) -> Vec<Value> {
    let id_of = |v: &Value| v.get("id").and_then(Value::as_str).map(str::to_string);
    let remote_by_id: HashMap<String, &Value> = remote.iter().filter_map(|v| id_of(v).map(|id| (id, v))).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    let alive = |item: &Value, id: &str| deleted.get(id).is_none_or(|at| stamp(item) > *at);

    // This copy's order first, then what only the other copy has.
    for mine in local {
        let Some(id) = id_of(mine) else { continue };
        seen.insert(id.clone());
        let chosen = match remote_by_id.get(&id) {
            Some(theirs) => {
                let (a, b) = (stamp(mine), stamp(theirs));
                if b > a || (b == a && remote_wins_ties) { *theirs } else { mine }
            }
            None => mine,
        };
        if alive(chosen, &id) {
            out.push(chosen.clone());
        }
    }
    for theirs in remote {
        let Some(id) = id_of(theirs) else { continue };
        if seen.contains(&id) {
            continue;
        }
        if alive(theirs, &id) {
            out.push(theirs.clone());
        }
    }
    out
}

/// Combine this device's copy of a board with the one that arrived.
pub fn merge_boards(local_text: &str, remote_text: &str, now_ms: i64) -> BoardMerge {
    let local: Option<Value> = serde_json::from_str(local_text).ok().filter(Value::is_object);
    let remote: Option<Value> = serde_json::from_str(remote_text).ok().filter(Value::is_object);
    let (local, remote) = match (local, remote) {
        (Some(l), Some(r)) => (l, r),
        // A copy that is not a board — half a file — loses to one that is.
        (Some(_), None) => return BoardMerge::Take(local_text.to_string()),
        (None, _) => return BoardMerge::Take(remote_text.to_string()),
    };

    // A board from a newer build may carry what this build would drop by
    // rewriting it. That copy is kept whole; the other is set beside it.
    let (lv, rv) = (version(&local), version(&remote));
    if lv != rv && lv.max(rv) > BOARD_SCHEMA_VERSION {
        return if rv > lv {
            BoardMerge::KeepWithConflictCopy { keep: remote_text.to_string(), copy: local_text.to_string() }
        } else {
            BoardMerge::KeepWithConflictCopy { keep: local_text.to_string(), copy: remote_text.to_string() }
        };
    }

    let remote_newer = saved_at(&remote) >= saved_at(&local);
    let deleted = deletions(&local, &remote, now_ms);
    let list = |board: &Value, key: &str| board.get(key).and_then(Value::as_array).cloned().unwrap_or_default();

    let nodes = merge_items(&list(&local, "nodes"), &list(&remote, "nodes"), &deleted, remote_newer);
    let ids: HashSet<&str> = nodes.iter().filter_map(|n| n.get("id").and_then(Value::as_str)).collect();
    let edges: Vec<Value> = merge_items(&list(&local, "edges"), &list(&remote, "edges"), &deleted, remote_newer)
        .into_iter()
        .filter(|e| {
            let end = |k: &str| e.get(k).and_then(Value::as_str).is_some_and(|id| ids.contains(id));
            end("source") && end("target")
        })
        .collect();

    // The board's own fields, field by field; the deletions both remember,
    // still current.
    let fields = settle_fields(&local, &remote, remote_newer);
    let mut merged = if remote_newer { remote } else { local };
    let obj = merged.as_object_mut().expect("checked above");
    for (key, value) in fields.top {
        match value {
            Some(v) => obj.insert(key, v),
            None => obj.remove(&key),
        };
    }
    let meta = obj.entry("metadata").or_insert_with(|| Value::Object(Map::new()));
    if let Some(meta) = meta.as_object_mut() {
        for (key, value) in fields.meta {
            match value {
                Some(v) => meta.insert(key, v),
                None => meta.remove(&key),
            };
        }
        if fields.stamps.is_empty() {
            meta.remove("stamps");
        } else {
            meta.insert("stamps".into(), Value::Object(fields.stamps));
        }
    }
    obj.insert("nodes".into(), Value::Array(nodes));
    obj.insert("edges".into(), Value::Array(edges));
    let metadata = obj.entry("metadata").or_insert_with(|| Value::Object(Map::new()));
    if let Some(meta) = metadata.as_object_mut() {
        if deleted.is_empty() {
            meta.remove("deleted");
        } else {
            let map: Map<String, Value> = deleted.into_iter().map(|(k, v)| (k, Value::from(v))).collect();
            meta.insert("deleted".into(), Value::Object(map));
        }
    }
    BoardMerge::Merged(merged)
}

/// The board's own fields as settled: what to set (or, `None`, remove) at the
/// top of the board and in its `metadata`, and the stamps that go with them.
struct Fields {
    top: Vec<(String, Option<Value>)>,
    meta: Vec<(String, Option<Value>)>,
    stamps: Map<String, Value>,
}

const BOOKKEEPING: [&str; 3] = ["updated_at", "deleted", "stamps"];

fn settle_fields(local: &Value, remote: &Value, remote_newer: bool) -> Fields {
    let stamps_of = |b: &Value| -> Map<String, Value> {
        b.get("metadata").and_then(|m| m.get("stamps")).and_then(Value::as_object).cloned().unwrap_or_default()
    };
    let (ls, rs) = (stamps_of(local), stamps_of(remote));
    let at = |stamps: &Map<String, Value>, key: &str| stamps.get(key).and_then(Value::as_i64).unwrap_or(0);

    // Which copy a field comes from: the one that changed it last, or, with
    // no stamp on either side, the one saved last.
    let take_remote = |key: &str| {
        let (l, r) = (at(&ls, key), at(&rs, key));
        if l == 0 && r == 0 { remote_newer } else { r > l }
    };

    let mut top = Vec::new();
    for key in ["title", "tags"] {
        let from = if take_remote(key) { remote } else { local };
        top.push((key.to_string(), from.get(key).cloned()));
    }

    let meta_of = |b: &Value| b.get("metadata").and_then(Value::as_object).cloned().unwrap_or_default();
    let (lm, rm) = (meta_of(local), meta_of(remote));
    let mut keys: Vec<&String> = lm.keys().chain(rm.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut meta = Vec::new();
    for key in keys {
        if BOOKKEEPING.contains(&key.as_str()) {
            continue;
        }
        let stamp_key = format!("metadata.{key}");
        let unstamped = at(&ls, &stamp_key) == 0 && at(&rs, &stamp_key) == 0;
        let value = match (lm.get(key), rm.get(key)) {
            // On one side only, and nothing says it was removed on the other:
            // kept.
            (Some(v), None) | (None, Some(v)) if unstamped => Some(v.clone()),
            _ => {
                if take_remote(&stamp_key) { rm.get(key).cloned() } else { lm.get(key).cloned() }
            }
        };
        meta.push((key.clone(), value));
    }

    let mut stamps = ls.clone();
    for (key, value) in rs {
        let later = value.as_i64().unwrap_or(0).max(at(&ls, &key));
        stamps.insert(key, Value::from(later));
    }
    Fields { top, meta, stamps }
}

/// Where a conflict copy of a board goes: beside it, still a board.
///
/// `sync::core::asset::conflict_path` names `x.whiteboard.json` as
/// `x.whiteboard (conflict …).json`, which is no longer a board to anything
/// that reads boards by their suffix.
pub fn board_conflict_path(rel_path: &str, discriminator: &str) -> String {
    let short: String = discriminator.chars().take(8).collect();
    match rel_path.strip_suffix(".whiteboard.json") {
        Some(stem) => format!("{stem} (conflict {short}).whiteboard.json"),
        None => format!("{rel_path} (conflict {short})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_790_000_000_000;

    fn board(saved: &str, nodes: Value, edges: Value, deleted: Value) -> String {
        json!({
            "schemaVersion": 1,
            "title": format!("saved {saved}"),
            "metadata": { "updated_at": saved, "deleted": deleted },
            "nodes": nodes,
            "edges": edges,
        })
        .to_string()
    }

    fn item(id: &str, label: &str, updated: i64) -> Value {
        json!({ "id": id, "type": "shape", "position": { "x": 0, "y": 0 }, "data": { "label": label }, "updated": updated })
    }

    fn labels(v: &Value) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| (n["id"].as_str().unwrap().to_string(), n["data"]["label"].as_str().unwrap().to_string()))
            .collect();
        out.sort();
        out
    }

    fn merged(local: &str, remote: &str) -> Value {
        match merge_boards(local, remote, NOW) {
            BoardMerge::Merged(v) => v,
            _ => panic!("expected a merge"),
        }
    }

    #[test]
    fn both_devices_additions_survive() {
        let a = board("2026-10-01T10:00:00Z", json!([item("x", "x", 1), item("from-a", "A", 5)]), json!([]), json!({}));
        let b = board("2026-10-01T11:00:00Z", json!([item("x", "x", 1), item("from-b", "B", 6)]), json!([]), json!({}));
        assert_eq!(
            labels(&merged(&a, &b)),
            vec![("from-a".into(), "A".into()), ("from-b".into(), "B".into()), ("x".into(), "x".into())]
        );
    }

    #[test]
    fn the_later_change_to_an_item_wins_whichever_copy_was_saved_last() {
        let a = board("2026-10-01T12:00:00Z", json!([item("x", "old here", 10)]), json!([]), json!({}));
        let b = board("2026-10-01T11:00:00Z", json!([item("x", "newer there", 20)]), json!([]), json!({}));
        assert_eq!(labels(&merged(&a, &b)), vec![("x".into(), "newer there".into())]);
    }

    #[test]
    fn a_deletion_holds_unless_the_item_changed_after_it() {
        let at = NOW - 5_000;
        let a = board("2026-10-01T12:00:00Z", json!([item("keep", "k", 1)]), json!([]), json!({ "gone": at, "edited": at }));
        let b = board(
            "2026-10-01T11:00:00Z",
            json!([item("keep", "k", 1), item("gone", "g", at - 10), item("edited", "e", at + 10)]),
            json!([]),
            json!({}),
        );
        let out = merged(&a, &b);
        assert_eq!(labels(&out), vec![("edited".into(), "e".into()), ("keep".into(), "k".into())]);
        assert_eq!(out["metadata"]["deleted"]["gone"], at);
    }

    #[test]
    fn a_line_goes_with_its_end() {
        let a = board(
            "2026-10-01T12:00:00Z",
            json!([item("p", "p", 1)]),
            json!([]),
            json!({ "q": NOW - 1_000 }),
        );
        let b = board(
            "2026-10-01T11:00:00Z",
            json!([item("p", "p", 1), item("q", "q", 2)]),
            json!([{ "id": "e", "source": "p", "target": "q", "type": "default", "updated": 3 }]),
            json!({}),
        );
        assert_eq!(merged(&a, &b)["edges"], json!([]));
    }

    #[test]
    fn the_board_fields_come_from_the_copy_saved_last() {
        let a = board("2026-10-01T12:00:00Z", json!([]), json!([]), json!({}));
        let b = board("2026-10-01T11:00:00Z", json!([]), json!([]), json!({}));
        assert_eq!(merged(&a, &b)["title"], "saved 2026-10-01T12:00:00Z");
    }

    #[test]
    fn a_board_from_a_newer_build_is_kept_whole_with_the_other_beside_it() {
        let ours = board("2026-10-01T12:00:00Z", json!([item("x", "ours", 5)]), json!([]), json!({}));
        let newer = json!({ "schemaVersion": 2, "nodes": [], "edges": [] }).to_string();
        match merge_boards(&ours, &newer, NOW) {
            BoardMerge::KeepWithConflictCopy { keep, copy } => {
                assert_eq!(keep, newer);
                assert_eq!(copy, ours);
            }
            _ => panic!("expected a conflict copy"),
        }
    }

    #[test]
    fn half_a_file_loses_to_a_board() {
        let ours = board("2026-10-01T12:00:00Z", json!([]), json!([]), json!({}));
        assert!(matches!(merge_boards("{\"nodes\": [", &ours, NOW), BoardMerge::Take(t) if t == ours));
        assert!(matches!(merge_boards(&ours, "garbage", NOW), BoardMerge::Take(t) if t == ours));
    }

    #[test]
    fn old_deletions_are_forgotten() {
        let a = board("2026-10-01T12:00:00Z", json!([]), json!([]), json!({ "ancient": 1 }));
        let out = merged(&a, &a);
        assert!(out["metadata"].get("deleted").is_none());
    }

    #[test]
    fn a_conflict_copy_is_still_a_board() {
        assert_eq!(
            board_conflict_path("Whiteboards/plan.whiteboard.json", "abcdef123456"),
            "Whiteboards/plan (conflict abcdef12).whiteboard.json"
        );
    }

    /// A rename on one device survives the other device saving later.
    #[test]
    fn a_rename_survives_a_later_save_elsewhere() {
        let a = json!({ "schemaVersion": 1, "title": "Roadmap 2027",
            "metadata": { "updated_at": "2026-10-03T10:00:00Z", "stamps": { "title": NOW - 1000 } },
            "nodes": [], "edges": [] }).to_string();
        let b = json!({ "schemaVersion": 1, "title": "Roadmap",
            "metadata": { "updated_at": "2026-10-03T11:00:00Z" },
            "nodes": [item("x", "moved", NOW)], "edges": [] }).to_string();
        let m = merged(&a, &b);
        assert_eq!(m["title"], "Roadmap 2027");
        assert_eq!(labels(&m), vec![("x".to_string(), "moved".to_string())]);
        assert_eq!(m["metadata"]["stamps"]["title"], json!(NOW - 1000));
    }

    /// A metadata key one copy has and the other never had is kept.
    #[test]
    fn a_project_link_made_on_one_device_is_kept() {
        let a = json!({ "schemaVersion": 1, "title": "B",
            "metadata": { "updated_at": "2026-10-03T10:00:00Z", "linked_projects": ["P"] },
            "nodes": [], "edges": [] }).to_string();
        let b = json!({ "schemaVersion": 1, "title": "B",
            "metadata": { "updated_at": "2026-10-03T11:00:00Z" }, "nodes": [], "edges": [] }).to_string();
        assert_eq!(merged(&a, &b)["metadata"]["linked_projects"], json!(["P"]));
    }

    /// A key removed on purpose (stamped) stays removed.
    #[test]
    fn a_link_removed_on_purpose_stays_removed() {
        let a = json!({ "schemaVersion": 1, "title": "B",
            "metadata": { "updated_at": "2026-10-03T10:00:00Z", "linked_projects": ["P"], "stamps": { "metadata.linked_projects": NOW - 5000 } },
            "nodes": [], "edges": [] }).to_string();
        let b = json!({ "schemaVersion": 1, "title": "B",
            "metadata": { "updated_at": "2026-10-03T09:00:00Z", "stamps": { "metadata.linked_projects": NOW - 100 } },
            "nodes": [], "edges": [] }).to_string();
        assert!(merged(&a, &b)["metadata"].get("linked_projects").is_none());
    }
}
