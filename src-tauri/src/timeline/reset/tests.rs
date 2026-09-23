use std::path::Path;
use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::models::node::NodeMetadata;
use crate::timeline::TimelineStore;

fn vault() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
    (dir, path)
}

fn node(id: &str, node_type: &str, properties: serde_json::Value) -> NodeMetadata {
    NodeMetadata {
        id: id.into(),
        node_type: node_type.into(),
        title: properties.get("title").and_then(|t| t.as_str()).unwrap_or("x").into(),
        content: String::new(),
        properties,
        created_at: "2026-07-01T08:00:00.000Z".into(),
        updated_at: "2026-07-01T08:00:00.000Z".into(),
        timestamp: 0,
        blocks: None,
    }
}

/// A vault with a moment kept, a month file holding a proposal, a reading and
/// a transcript, a decision, and a note that has nothing to do with any of it.
fn a_vault_with_a_timeline() -> (tempfile::TempDir, String, DbState) {
    let (dir, vault_path) = vault();
    let db = DbBridge::new_in_memory_full().expect("schema");
    for n in [
        node(
            "Moments/6f3c.md",
            "moment",
            json!({ "type": "moment", "title": "Ăn trưa bún chả", "happened": "2026-07-21" }),
        ),
        node("Notes/2026-07-21.md", "note", json!({ "title": "2026-07-21", "date": "2026-07-21" })),
    ] {
        db.upsert_node(&n).expect("seeded");
    }
    let write = |rel: &str, text: &str| {
        let path = Path::new(&vault_path).join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("Moments/6f3c.md", "---\ntype: moment\ntitle: Ăn trưa bún chả\nhappened: 2026-07-21\n---\n");
    write(
        "Timeline/2026/2026-07.dev.json",
        &json!({
            "format": 1, "month": "2026-07", "device": "dev",
            "items": [{
                "id": "x1", "kind": "moment", "happened_from": "2026-07-21", "happened_to": "2026-07-21",
                "precision": "day", "recorded": "2026-07-21", "source": "extract",
                "evidence": [{ "node": "Notes/2026-07-21.md", "hash": "b1" }],
                "extractor": { "version": 3, "model": "m" }, "confidence": 0.9,
                "payload": { "title": "Họp UAT", "quote": "Chiều họp UAT" }
            }],
            "sources": [{
                "node": "day:2026-07-21", "hash": "bag", "version": 3, "model": "m",
                "at": "2026-07-22T00:00:00.000Z", "items": ["x1"], "dropped": 0, "chars": 40, "ms": 1,
                "blocks": ["b1"]
            }],
            "surrogates": [{
                "node": "Files/a.md", "hash": "h", "kind": "caption", "version": 1, "model": "m",
                "at": "2026-07-22T00:00:00.000Z", "text": "một bức ảnh", "segments": [], "ms": 0
            }]
        })
        .to_string(),
    );
    write(
        "Timeline/reviews/dev.json",
        &json!({ "format": 1, "device": "dev", "decisions": [
            { "item": "x0", "decision": "declined", "node": "Notes/2026-07-21.md", "at": "2026-07-22T00:00:00.000Z" }
        ]})
        .to_string(),
    );
    write("Timeline/extract.json", &json!({ "enabled": true, "categories": ["ăn uống"] }).to_string());
    write("Timeline/ledger/dev/2026-07.json", "{}");
    (dir, vault_path, Mutex::new(db))
}

#[test]
fn what_starting_again_would_take_is_counted_first() {
    let (_dir, vault_path, state) = a_vault_with_a_timeline();
    let store = TimelineStore::open_in_memory().unwrap();
    extract::load(store.conn(), &vault_path).unwrap();

    let db = state.lock().unwrap();
    let plan = plan(&db, store.conn(), &vault_path).unwrap();
    drop(db);
    assert_eq!(
        plan,
        Plan { moments: 1, proposals: 1, readings: 1, decisions: 1, month_files: 1, surrogates: 1 }
    );
}

#[test]
fn starting_again_empties_the_timeline_and_keeps_everything_else() {
    let (_dir, vault_path, state) = a_vault_with_a_timeline();
    let store = TimelineStore::open_in_memory().unwrap();
    extract::load(store.conn(), &vault_path).unwrap();

    let done = reset(&state, store.conn(), &vault_path, Some(1)).unwrap();
    assert_eq!(done.failed, Vec::<String>::new());
    assert_eq!((done.moments, done.month_files, done.review_files, done.surrogates_kept), (1, 1, 1, 1));

    let at = |rel: &str| Path::new(&vault_path).join(rel);
    // The moment went to the trash, so it can be brought back.
    assert!(!at("Moments/6f3c.md").exists());
    assert!(at(".trash/Moments/6f3c.md").exists(), "{:?}", std::fs::read_dir(at(".trash")).map(|d| d.count()));
    assert!(state.lock().unwrap().get_node("Moments/6f3c.md").unwrap().is_none(), "and out of the index");

    // The month file kept its transcript and nothing else.
    let month: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(at("Timeline/2026/2026-07.dev.json")).unwrap()).unwrap();
    assert_eq!(month["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(month["sources"].as_array().map(Vec::len), Some(0));
    assert_eq!(month["surrogates"][0]["text"], "một bức ảnh", "a transcript is minutes of a model's time");

    // The decisions went with the proposals they were about.
    assert!(!at("Timeline/reviews/dev.json").exists());
    assert!(extract::reviewed(&vault_path).is_empty());

    // What was never about moments is untouched.
    assert!(at("Notes/2026-07-21.md").exists() || state.lock().unwrap().get_node("Notes/2026-07-21.md").unwrap().is_some());
    assert!(at("Timeline/extract.json").exists(), "the settings are not reconfigured by starting again");
    let config = extract::read_config(&vault_path);
    assert_eq!(config.categories, vec!["ăn uống".to_string()]);
    assert!(at("Timeline/ledger/dev/2026-07.json").exists(), "the evidence ledger is about files, not moments");

    // And the index says nothing about any of it.
    let count = |table: &str| -> i64 {
        store.conn().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    };
    assert_eq!((count("events"), count("extract_runs"), count("event_links")), (0, 0, 0));
}

/// A moment arriving by sync between the question and the answer changes what
/// the answer means, so it is refused rather than done anyway.
#[test]
fn it_refuses_when_the_vault_is_not_what_the_person_was_shown() {
    let (_dir, vault_path, state) = a_vault_with_a_timeline();
    let store = TimelineStore::open_in_memory().unwrap();
    let refused = reset(&state, store.conn(), &vault_path, Some(7));
    assert!(refused.is_err(), "{refused:?}");
    assert!(Path::new(&vault_path).join("Moments/6f3c.md").exists(), "nothing was touched");
}
