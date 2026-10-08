//! Item-by-item merge for the few JSON files that whole-file LWW breaks.
//!
//! JSON documents are resolved whole, last writer wins on
//! `metadata.updated_at` (see `apply::pull_json_impl`). That is right for a
//! document one person edits in one place, and wrong for a file that is a
//! *list* each device appends to on its own: whichever copy wins, the other
//! device's additions are gone. For Syn that meant a routine that ran on one
//! computer lost its `last_slot` and ran again on the other, and a memory
//! suggestion declined on the phone came back on the desktop.
//!
//! The files named here are merged instead: the union of both lists by a key,
//! and for a key both sides hold, the copy with the newer per-item stamp.
//! Deletions travel as stamped tombstones (a `removed` map, or a `removed_at`
//! on the item) because a plain union can never forget anything.
//!
//! The merge works on `serde_json::Value`, not on Syn's types, so the sync
//! layer stays ignorant of what the lists mean, and fields it does not know
//! about are carried through untouched.
//!
//! # Convergence
//!
//! Two devices merging each other's copies must land on the same bytes, or
//! they push the difference back and forth for ever. So every choice is
//! deterministic: newer stamp wins, a tie goes to the larger serialisation,
//! and order is the remote copy's with local-only items after it — which,
//! once one side holds everything, is exactly the order it already has.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value};

/// How the items of one list are told apart and dated.
struct ListRule {
    key: fn(&Value) -> Option<String>,
    stamp: fn(&Value) -> String,
}

fn str_field(item: &Value, field: &str) -> String {
    item.get(field).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn by_id(item: &Value) -> Option<String> {
    item.get("id").and_then(Value::as_str).map(str::to_string)
}

/// A routine is dated by its last edit, stamped by `syn::routine::save`.
const ROUTINES: ListRule = ListRule {
    key: by_id,
    stamp: |item| str_field(item, "updated_at"),
};

/// A proposal never changes once made; it is only taken out of the tray, and
/// that is recorded as `removed_at`, which is always the later of the two.
const PROPOSALS: ListRule = ListRule {
    key: by_id,
    stamp: |item| str_field(item, "proposed_at").max(str_field(item, "removed_at")),
};

/// A decline has no id; it is the body, folded the way `syn::proposal` folds
/// it when it checks for one.
const DECLINED: ListRule = ListRule {
    key: |item| item.get("body").and_then(Value::as_str).map(|b| b.trim().to_lowercase()),
    stamp: |item| str_field(item, "at"),
};

/// A whiteboard shape library's pieces, dated by when each was added. Pieces
/// are added on one device or another and never edited in place, so the
/// union is what both devices hold.
const LIBRARY_PIECES: ListRule = ListRule {
    key: by_id,
    stamp: |item| str_field(item, "added_at"),
};

/// Feed highlights and calendar subscriptions: made on one device or another,
/// changed now and then, removed with a stamped `removed_at` that the newer
/// stamp carries. See `feed_engine::highlights`, `calendar::subscriptions_file`.
const STAMPED_ENTRIES: ListRule = ListRule {
    key: by_id,
    stamp: |item| str_field(item, "updated_at").max(str_field(item, "removed_at")),
};

/// Files that are `{ <list>: [...] }` of [`STAMPED_ENTRIES`], and which list.
fn stamped_list(path: &str) -> Option<&'static str> {
    match path {
        "Feeds/highlights.json" => Some("highlights"),
        "Calendar/subscriptions.json" => Some("subscriptions"),
        _ => None,
    }
}

/// A whiteboard shape library: `Whiteboards/Libraries/<name>.boardlib.json`.
fn is_library(path: &str) -> bool {
    path.strip_prefix("Whiteboards/Libraries/")
        .is_some_and(|name| name.ends_with(".boardlib.json") && !name.contains('/'))
}

/// Is this file merged item by item rather than resolved whole?
pub fn is_merged(rel_path: &str) -> bool {
    let path = rel_path.replace('\\', "/");
    matches!(path.as_str(), "Syn/routines.json" | "Syn/proposals.json" | "Syn/declined.json")
        || is_library(&path)
        || stamped_list(&path).is_some()
}

/// Merge two copies of a file named by [`is_merged`].
///
/// `None` when the path is not one of them, or either side does not parse as
/// the expected shape — the caller then falls back to last writer wins, which
/// is what these files had before.
pub fn merge(rel_path: &str, local: &str, remote: &str) -> Option<Value> {
    let local: Value = serde_json::from_str(local).ok()?;
    let remote: Value = serde_json::from_str(remote).ok()?;
    match rel_path.replace('\\', "/").as_str() {
        "Syn/routines.json" => merge_routines(&local, &remote),
        "Syn/proposals.json" => Some(Value::Array(merge_list(local.as_array()?, remote.as_array()?, &PROPOSALS))),
        "Syn/declined.json" => Some(Value::Array(merge_list(local.as_array()?, remote.as_array()?, &DECLINED))),
        path if is_library(path) => merge_library(&local, &remote),
        path => merge_stamped(&local, &remote, stamped_list(path)?),
    }
}

/// `routines.json`: `{ routines: [...], last_slot: {id: slot}, removed: {id: stamp} }`.
///
/// `last_slot` takes the later slot for each routine — a slot key is
/// `YYYY-MM-DDTHH:MM`, so later is larger — because a slot either device ran
/// is a slot that must not run again anywhere.
fn merge_routines(local: &Value, remote: &Value) -> Option<Value> {
    let local = local.as_object()?;
    let remote = remote.as_object()?;

    let list = |o: &Map<String, Value>| o.get("routines").and_then(Value::as_array).cloned().unwrap_or_default();
    let removed = max_map(local.get("removed"), remote.get("removed"));
    let routines: Vec<Value> = merge_list(&list(local), &list(remote), &ROUTINES)
        .into_iter()
        .filter(|r| {
            let id = by_id(r).unwrap_or_default();
            removed.get(&id).is_none_or(|gone| *gone < (ROUTINES.stamp)(r))
        })
        .collect();
    let alive: std::collections::HashSet<String> = routines.iter().filter_map(by_id).collect();
    let last_slot: BTreeMap<String, String> = max_map(local.get("last_slot"), remote.get("last_slot"))
        .into_iter()
        .filter(|(id, _)| alive.contains(id))
        .collect();

    // Everything else — `metadata` above all — is the remote's, which is what
    // whole-file resolution would have kept had the remote won.
    let mut merged = remote.clone();
    merged.insert("routines".into(), Value::Array(routines));
    merged.insert("last_slot".into(), to_object(last_slot));
    if removed.is_empty() {
        merged.remove("removed");
    } else {
        merged.insert("removed".into(), to_object(removed));
    }
    Some(Value::Object(merged))
}

/// A shape library: `{ type, version, name, items: [...] }`. The pieces are
/// the union of both copies; everything else is the remote's. Adding a piece
/// on two devices before either had synced used to keep one device's copy
/// whole and lose the other's piece.
fn merge_library(local: &Value, remote: &Value) -> Option<Value> {
    let local = local.as_object()?;
    let remote = remote.as_object()?;
    let items = |o: &Map<String, Value>| o.get("items").and_then(Value::as_array).cloned();
    let merged_items = merge_list(&items(local)?, &items(remote)?, &LIBRARY_PIECES);
    let mut merged = remote.clone();
    merged.insert("items".into(), Value::Array(merged_items));
    Some(Value::Object(merged))
}

/// `{ <field>: [...] }`, the list merged by [`STAMPED_ENTRIES`] and everything
/// else the remote's. Tombstones are kept in the list: dropping them would let
/// a third device that has not heard of the removal bring the entry back.
fn merge_stamped(local: &Value, remote: &Value, field: &str) -> Option<Value> {
    let local = local.as_object()?;
    let remote = remote.as_object()?;
    let list = |o: &Map<String, Value>| match o.get(field) {
        None => Some(Vec::new()),
        Some(v) => v.as_array().cloned(),
    };
    let merged_items = merge_list(&list(local)?, &list(remote)?, &STAMPED_ENTRIES);
    let mut merged = remote.clone();
    merged.insert(field.into(), Value::Array(merged_items));
    Some(Value::Object(merged))
}

fn to_object(map: BTreeMap<String, String>) -> Value {
    Value::Object(map.into_iter().map(|(k, v)| (k, Value::String(v))).collect())
}

/// Two `{key: string}` maps, keeping the larger value for each key.
fn max_map(a: Option<&Value>, b: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for map in [a, b].into_iter().flatten().filter_map(Value::as_object) {
        for (k, v) in map {
            let Some(v) = v.as_str() else { continue };
            let slot = out.entry(k.clone()).or_insert_with(String::new);
            if v > slot.as_str() {
                *slot = v.to_string();
            }
        }
    }
    out
}

/// The union of two lists by key, the newer copy of each item winning.
fn merge_list(local: &[Value], remote: &[Value], rule: &ListRule) -> Vec<Value> {
    // An item with no key is keyed by its whole text, so identical ones fold
    // together and different ones are both kept.
    let key = |item: &Value| (rule.key)(item).unwrap_or_else(|| format!("\u{0}{item}"));

    let mut order: Vec<String> = Vec::new();
    let mut chosen: HashMap<String, Value> = HashMap::new();
    for item in remote.iter().chain(local) {
        let k = key(item);
        match chosen.get(&k) {
            None => {
                order.push(k.clone());
                chosen.insert(k, item.clone());
            }
            Some(held) if newer(item, held, rule) => {
                chosen.insert(k, item.clone());
            }
            Some(_) => {}
        }
    }
    order.into_iter().filter_map(|k| chosen.remove(&k)).collect()
}

/// Is `a` the copy to keep over `b`? Newer stamp, and on a tie the larger
/// serialisation, so both devices pick the same one.
fn newer(a: &Value, b: &Value, rule: &ListRule) -> bool {
    let (sa, sb) = ((rule.stamp)(a), (rule.stamp)(b));
    if sa != sb {
        return sa > sb;
    }
    let (ta, tb) = (a.to_string(), b.to_string());
    ta > tb
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn routine(id: &str, name: &str, updated_at: &str) -> Value {
        json!({"id": id, "name": name, "ask": "?", "schedule": {"at": "07:30", "weekdays": []},
               "enabled": true, "to_phone": false, "updated_at": updated_at})
    }

    fn merged(path: &str, a: &Value, b: &Value) -> Value {
        merge(path, &a.to_string(), &b.to_string()).expect("merges")
    }

    /// The case that sent a Telegram message twice: each computer ran a routine
    /// and recorded the slot, and one copy won whole.
    #[test]
    fn two_divergent_routine_books_keep_both_routines_and_the_latest_slot() {
        let desktop = json!({
            "routines": [routine("a", "Sáng nay", "2026-09-27T00:00:00.000Z")],
            "last_slot": {"a": "2026-09-28T07:30"},
            "metadata": {"node_id": "n1", "updated_at": "2026-09-28T07:30:05.000Z"},
        });
        let laptop = json!({
            "routines": [
                routine("a", "Sáng nay", "2026-09-27T00:00:00.000Z"),
                routine("b", "Tối nay", "2026-09-28T09:00:00.000Z"),
            ],
            "last_slot": {"a": "2026-09-27T07:30", "b": "2026-09-27T21:00"},
            "metadata": {"node_id": "n1", "updated_at": "2026-09-28T09:00:00.000Z"},
        });

        for (local, remote) in [(&desktop, &laptop), (&laptop, &desktop)] {
            let book = merged("Syn/routines.json", local, remote);
            let ids: Vec<&str> = book["routines"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
            let mut sorted = ids.clone();
            sorted.sort();
            assert_eq!(sorted, ["a", "b"], "both routines survive");
            assert_eq!(book["last_slot"]["a"], "2026-09-28T07:30", "the later slot wins, so it does not run again");
            assert_eq!(book["last_slot"]["b"], "2026-09-27T21:00");
            assert_eq!(book["metadata"]["node_id"], "n1");
        }
    }

    #[test]
    fn the_newer_edit_of_a_routine_wins_whichever_side_it_is_on() {
        let old = json!({"routines": [routine("a", "cũ", "2026-09-27T00:00:00.000Z")], "last_slot": {}});
        let new = json!({"routines": [routine("a", "mới", "2026-09-28T00:00:00.000Z")], "last_slot": {}});
        assert_eq!(merged("Syn/routines.json", &old, &new)["routines"][0]["name"], "mới");
        assert_eq!(merged("Syn/routines.json", &new, &old)["routines"][0]["name"], "mới");
    }

    /// Without a tombstone a union can never forget: the other device's copy
    /// would bring a deleted routine back on every sync.
    #[test]
    fn a_deleted_routine_stays_deleted_unless_edited_after() {
        let kept = json!({"routines": [routine("a", "x", "2026-09-27T00:00:00.000Z")], "last_slot": {"a": "2026-09-27T07:30"}});
        let deleted = json!({"routines": [], "last_slot": {}, "removed": {"a": "2026-09-28T00:00:00.000Z"}});
        for (l, r) in [(&kept, &deleted), (&deleted, &kept)] {
            let book = merged("Syn/routines.json", l, r);
            assert!(book["routines"].as_array().unwrap().is_empty());
            assert!(book["last_slot"].get("a").is_none());
            assert_eq!(book["removed"]["a"], "2026-09-28T00:00:00.000Z");
        }

        let edited_later = json!({"routines": [routine("a", "x", "2026-09-29T00:00:00.000Z")], "last_slot": {}});
        assert_eq!(merged("Syn/routines.json", &edited_later, &deleted)["routines"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_taken_proposal_stays_taken_and_new_ones_from_both_sides_are_kept() {
        let p = |id: &str, removed: Option<&str>| {
            let mut v = json!({"id": id, "body": id, "proposed_at": "2026-09-27T00:00:00Z"});
            if let Some(r) = removed {
                v["removed_at"] = json!(r);
            }
            v
        };
        let phone = json!([p("x", Some("2026-09-28T00:00:00Z")), p("y", None)]);
        let desktop = json!([p("x", None), p("z", None)]);
        for (l, r) in [(&phone, &desktop), (&desktop, &phone)] {
            let out = merged("Syn/proposals.json", l, r);
            let mut ids: Vec<(String, bool)> = out
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v["id"].as_str().unwrap().to_string(), v.get("removed_at").is_some()))
                .collect();
            ids.sort();
            assert_eq!(ids, [("x".into(), true), ("y".into(), false), ("z".into(), false)]);
        }
    }

    #[test]
    fn declines_from_both_devices_are_kept_once_each() {
        let phone = json!([{"body": "Thích trà", "at": "2026-09-28T00:00:00Z"}]);
        let desktop = json!([{"body": "thích trà ", "at": "2026-09-27T00:00:00Z"}, {"body": "Dậy sớm", "at": "2026-09-26T00:00:00Z"}]);
        let out = merged("Syn/declined.json", &phone, &desktop);
        assert_eq!(out.as_array().unwrap().len(), 2);
    }

    /// Once one side holds the merge, merging it again changes nothing — the
    /// property that stops two devices pushing the same file back and forth.
    #[test]
    fn merging_the_merge_is_a_fixed_point() {
        let a = json!({"routines": [routine("a", "x", "1"), routine("c", "z", "3")], "last_slot": {"a": "s1"}});
        let b = json!({"routines": [routine("b", "y", "2"), routine("a", "x2", "1")], "last_slot": {"a": "s2"}, "removed": {"c": "4"}});
        let m = merged("Syn/routines.json", &a, &b);
        assert_eq!(merged("Syn/routines.json", &b, &m), m);
        assert_eq!(merged("Syn/routines.json", &a, &m), m);
        assert_eq!(merged("Syn/routines.json", &m, &m), m);
    }

    #[test]
    fn other_files_and_unreadable_copies_are_left_to_last_writer_wins() {
        assert!(merge("Syn/connectors.json", "{}", "{}").is_none());
        assert!(merge("Syn/routines.json", "{ not json", "{}").is_none());
        assert!(merge("Syn/proposals.json", "{}", "[]").is_none(), "wrong shape");
        assert!(is_merged("Syn/declined.json") && !is_merged("Notes/declined.json"));
        assert!(is_merged("Whiteboards/Libraries/Kit.boardlib.json"));
        assert!(!is_merged("Whiteboards/Libraries/sub/Kit.boardlib.json") && !is_merged("Whiteboards/a.whiteboard.json"));
    }

    /// Two devices highlighting before either had synced each keep theirs; a
    /// removal on one is not undone by the other's older copy.
    #[test]
    fn feed_highlights_from_both_devices_are_kept_and_a_removal_holds() {
        let h = |id: &str, at: &str, removed: Option<&str>| {
            let mut v = json!({"id": id, "source_id": "f", "guid": "g", "text": id, "created_at": at, "updated_at": removed.unwrap_or(at)});
            if let Some(r) = removed {
                v["removed_at"] = json!(r);
                v["text"] = json!("");
            }
            v
        };
        let phone = json!({"highlights": [h("a", "2026-10-01T00:00:00Z", None), h("p", "2026-10-02T00:00:00Z", None)],
                           "metadata": {"node_id": "n-h"}});
        let desk = json!({"highlights": [h("a", "2026-10-01T00:00:00Z", Some("2026-10-03T00:00:00Z")), h("d", "2026-10-02T01:00:00Z", None)],
                          "metadata": {"node_id": "n-h"}});
        for (l, r) in [(&phone, &desk), (&desk, &phone)] {
            let out = merged("Feeds/highlights.json", l, r);
            let mut got: Vec<(String, bool)> = out["highlights"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v["id"].as_str().unwrap().to_string(), v.get("removed_at").is_some()))
                .collect();
            got.sort();
            assert_eq!(got, [("a".into(), true), ("d".into(), false), ("p".into(), false)]);
            assert_eq!(out["metadata"]["node_id"], "n-h");
            let again = merged("Feeds/highlights.json", r, &out);
            assert_eq!(again["highlights"].as_array().unwrap().len(), 3, "and merging the merge changes nothing");
        }
    }

    #[test]
    fn calendar_subscriptions_take_the_newer_edit_of_each() {
        let s = |name: &str, at: &str| json!({"id": "s1", "url": "https://x/a.ics", "name": name, "updated_at": at});
        let older = json!({"subscriptions": [s("Cũ", "2026-10-01T00:00:00.000Z")]});
        let newer = json!({"subscriptions": [s("Mới", "2026-10-02T00:00:00.000Z"),
                                             {"id": "s2", "url": "https://x/b.ics", "updated_at": "2026-10-02T00:00:00.000Z"}]});
        for (l, r) in [(&older, &newer), (&newer, &older)] {
            let out = merged("Calendar/subscriptions.json", l, r);
            let subs = out["subscriptions"].as_array().unwrap();
            assert_eq!(subs.len(), 2);
            assert_eq!(subs.iter().find(|v| v["id"] == "s1").unwrap()["name"], "Mới");
        }
        assert!(is_merged("Calendar/subscriptions.json") && is_merged("Feeds/highlights.json"));
        assert!(!is_merged("Notes/Calendar/subscriptions.json"));
    }

    #[test]
    fn a_shape_library_keeps_the_pieces_both_devices_added() {
        let base = |items: &str| format!(r#"{{"type":"synabit-board-library","version":1,"name":"Kit","items":[{items}]}}"#);
        let shared = r#"{"id":"a","title":"A","added_at":"2026-10-01T00:00:00Z","nodes":[]}"#;
        let here = base(&format!(r#"{shared},{{"id":"b","title":"Mine","added_at":"2026-10-05T10:00:00Z","nodes":[]}}"#));
        let there = base(&format!(r#"{shared},{{"id":"c","title":"Theirs","added_at":"2026-10-05T10:01:00Z","nodes":[]}}"#));
        let merged = merge("Whiteboards/Libraries/Kit.boardlib.json", &here, &there).expect("merged");
        let ids: Vec<&str> = merged["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["a", "c", "b"]);
        // And the other way round lands on the same pieces.
        let back = merge("Whiteboards/Libraries/Kit.boardlib.json", &there, &here).expect("merged");
        assert_eq!(back["items"].as_array().unwrap().len(), 3);
    }
}