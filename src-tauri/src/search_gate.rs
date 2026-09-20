//! What the query grammar answers today, written down so a change has to own
//! up to itself.
//!
//! The plan is §15.1 of `docs/query-grammar-2026-09-20.md`.
//!
//! # Why a golden file and not more unit tests
//!
//! §16 Bước 9 of the Timeline document did this and it earned its keep: a
//! 1,307-line snapshot of the index caught a regression that 2,340 unit tests
//! did not — 302 pictures quietly changing which event they belonged to. The
//! tests all passed because each one asked about the thing it was written for,
//! and nobody had written one about that.
//!
//! The grammar is about to be rebuilt from a flat struct into a tree, over
//! several steps, and every step has the same danger: a query that used to
//! mean one thing quietly meaning another. So this records what every shape of
//! question answers **now**, against one fixed vault. A step that changes a
//! line has to change the line here too, in the same commit, which makes the
//! change visible in review instead of invisible in production.
//!
//! # It records refusals as well as answers
//!
//! Half of what is wrong with the grammar today is that it answers a different
//! question instead of refusing (§9). Those lines are in here as they stand —
//! wrong — so that fixing them shows up as a diff rather than as silence.

#![cfg(test)]

use std::sync::Mutex;

use serde_json::json;

use crate::db::DbBridge;
use crate::models::node::NodeMetadata;
use crate::search::parse_query;
use crate::timeline::store::{catch_up, TimelineStore};

fn node(id: &str, node_type: &str, title: &str, properties: serde_json::Value) -> NodeMetadata {
    NodeMetadata {
        id: id.into(),
        node_type: node_type.into(),
        title: title.into(),
        content: String::new(),
        properties,
        created_at: "2026-01-01T00:00:00.000Z".into(),
        updated_at: "2026-01-01T00:00:00.000Z".into(),
        timestamp: 0,
        blocks: None,
    }
}

/// One small vault holding a little of everything the grammar can ask about.
fn a_vault() -> (Mutex<DbBridge>, TimelineStore) {
    let db = DbBridge::new_in_memory_full().unwrap();
    for n in [
        node("People/khanh.md", "person", "Khánh", json!({ "node_id": "uuid-khanh" })),
        node("People/minh.md", "person", "Minh", json!({ "node_id": "uuid-minh" })),
        node(
            "Notes/2019-11-05.md",
            "note",
            "2019-11-05",
            json!({
                "date": "2019-11-05",
                "tags": ["gia-đình"],
                "moments": [
                    { "title": "Gặp Khánh ở quán quen", "happened": "2019-11-05", "people": ["uuid-khanh"], "where": "Hà Nội" },
                    { "title": "Ăn tối với Minh", "happened": "2019-11-05", "people": ["uuid-minh"] }
                ]
            }),
        ),
        node(
            "Notes/2021-03-14.md",
            "note",
            "2021-03-14",
            json!({ "date": "2021-03-14", "tags": ["công-việc"] }),
        ),
        node(
            "Tasks/a.md",
            "task",
            "Gửi báo cáo quý",
            json!({ "status": "done", "completed_at": "2019-11-05", "priority": 3 }),
        ),
        node("Tasks/b.md", "task", "Làm công văn", json!({ "priority": 5 })),
        node("Books/x.md", "book", "Sách hay", json!({ "rating": 4, "author": "Nguyễn" })),
    ] {
        db.upsert_node(&n).unwrap();
    }
    // Free-word search goes through FTS, which `upsert_node` does not fill —
    // without this every bare word answers 0 and the gate would be recording
    // a hole in the harness rather than the behaviour of the grammar.
    db.reindex_search().unwrap();
    let cache = Mutex::new(db);
    let mut timeline = TimelineStore::open_in_memory().unwrap();
    catch_up(&cache, &mut timeline).unwrap();
    (cache, timeline)
}

/// Every shape of question the grammar can be asked, including the ones that
/// are wrong today.
const ASKED: &[&str] = &[
    // ── the plain filters ──
    "is:note",
    "type:task",
    "is:book rating:>3",
    "#gia-đình",
    "tag:công-việc",
    "status:done",
    "-status:done",
    "priority:3",
    "priority:>3",
    "author:Nguyễn",
    "báo cáo",
    "-báo",
    "\"công văn\"",
    // ── shaping ──
    "is:task sort:priority",
    "is:task sort:-priority",
    "is:task sort:title",
    "is:task columns:title,priority",
    "is:task limit:1",
    // ── the timeline's words ──
    "when:2019",
    "when:2019-11-05",
    "when:2019..2021",
    "with:khánh",
    "with:Khánh",
    "with:khánh with:minh",
    "where:\"Hà Nội\"",
    "shape:occasion",
    "shape:chore",
    "magnitude:>2",
    "magnitude:2",
    "when:2019 columns:when,title,who",
    "when:2016..2026 gặp",
    "when:2016..2026 -gặp",
    // ── §9: what answers a different question today ──
    "date:today",
    "date:2026-06",
    "is:note date:2026-06",
    "is:note when:2019",
    "#gia-đình when:2019",
    "status:done when:2019",
    "limit:abc",
    "-#gia-đình",
    "-with:khánh",
    "-when:2019",
    "sort:tiêu_đề",
    "columns:tiêu_đề",
    "in:title gặp",
    // ── nothing to ask ──
    "",
    "   ",
];

/// How one question answers, in one line.
fn answer(cache: &Mutex<DbBridge>, timeline: &TimelineStore, q: &str) -> String {
    let parsed = parse_query(q);
    // Refusals first, the way both runners read them: a query that carries one
    // is refused for that reason, not for being empty — and saying "nothing to
    // match on" for `limit:abc` would hide the actual complaint.
    if let Some(why) = parsed.refused.first() {
        return format!("{q:<34} → refused: {why}");
    }
    if parsed.is_empty {
        return format!("{q:<34} → refused (nothing to match on)");
    }

    let to_timeline = parsed.asks_the_timeline();
    let result = if to_timeline {
        let db = cache.lock().unwrap();
        let resolve = |names: &Vec<String>| -> Vec<String> {
            names
                .iter()
                .map(|name| {
                    crate::timeline::store::node_for(&db, name)
                        .and_then(|id| {
                            db.conn()
                                .query_row(
                                    "SELECT COALESCE(NULLIF(json_extract(properties, '$.node_id'), ''), stable_id, id) FROM nodes WHERE id = ?1",
                                    [&id],
                                    |r| r.get::<_, String>(0),
                                )
                                .ok()
                        })
                        .unwrap_or_else(|| name.clone())
                })
                .collect()
        };
        crate::timeline::query::run(
            timeline,
            &parsed,
            &crate::timeline::query::Named {
                with: resolve(&parsed.with),
                place: resolve(&parsed.place),
                about: resolve(&parsed.about),
            },
        )
    } else {
        cache.lock().unwrap().run_node_query(&parsed)
    };

    let source = if to_timeline { "events" } else { "notes " };
    match result {
        Ok(found) => {
            // In the order they came back, deliberately: sorting them here
            // would hide every difference `sort:` makes, which is most of what
            // there is to see about `sort:`.
            let titles: Vec<&str> =
                found.rows.iter().take(3).map(|r| r.title.as_str()).collect();
            format!("{q:<34} → {source} {:>3} [{}]", found.total, titles.join(" · "))
        }
        Err(e) => format!("{q:<34} → {source} refused: {e}"),
    }
}

/// The whole grammar, as it answers today.
///
/// When this fails, read the diff before anything else: it is telling you that
/// a question now means something it did not mean before. That is sometimes
/// the point of the change — and then the expected text is updated in the same
/// commit, so the change is in the review rather than in production.
#[test]
fn what_the_grammar_answers_today() {
    let (cache, timeline) = a_vault();
    let lines: Vec<String> = ASKED.iter().map(|q| answer(&cache, &timeline, q)).collect();
    let got = lines.join("\n");

    let expected = include_str!("search_gate.txt").trim_end();
    if got.trim_end() != expected {
        // Printed rather than only asserted: a 45-line diff is unreadable in
        // an assertion message, and the usual next step is to copy this over
        // the file after reading it.
        eprintln!("\n─── what it answers now ───\n{got}\n");
        panic!("the grammar answers differently than `search_gate.txt` records");
    }
}
