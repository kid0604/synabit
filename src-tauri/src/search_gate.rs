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
use crate::timeline::store::{catch_up, TimelineStore};

fn node(id: &str, node_type: &str, title: &str, properties: serde_json::Value) -> NodeMetadata {
    written(id, node_type, title, properties, "")
}

/// The same, with words in it — so `explode sentences` has something to open.
fn written(
    id: &str,
    node_type: &str,
    title: &str,
    properties: serde_json::Value,
    content: &str,
) -> NodeMetadata {
    NodeMetadata {
        id: id.into(),
        node_type: node_type.into(),
        title: title.into(),
        content: content.into(),
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
        written(
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
            "Hôm nay gặp Khánh ở quán quen. Nói chuyện hai tiếng. Về muộn.",
        ),
        written(
            "Notes/2021-03-14.md",
            "note",
            "2021-03-14",
            json!({ "date": "2021-03-14", "tags": ["công-việc"] }),
            "Nộp báo cáo quý. Sếp không nói gì.",
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
    // ── naming the table (§4) ──
    "nodes",
    "events",
    "events limit:5",
    "nodes when:2019",
    "events when:2019",
    "nodes with:khánh",
    "events #gia-đình",
    "nodes #gia-đình",
    "events is:note",
    // ── the timeline's words ──
    "when:2019",
    "when:2019-11-05",
    "when:2019..2021",
    "with:khánh",
    "with:Khánh",
    "with:khánh with:minh",
    "place:\"Hà Nội\"",
    "shape:occasion",
    "shape:chore",
    "size:>2",
    "size:2",
    "when:today",
    "when:2019 columns:when,title,who",
    "when:2016..2026 gặp",
    "when:2016..2026 -gặp",
    // ── §3: either, not-this, and brackets ──
    "#gia-đình OR #công-việc",
    "is:task OR is:book",
    "báo OR văn",
    "is:task AND status:done",
    "with:khánh OR with:minh",
    "(with:khánh OR with:minh) when:2019",
    "when:2019 (gặp OR ăn)",
    // BROKEN, and recorded so it stays visible: the vault holds «Ăn tối với
    // Minh» in 2019 and this answers 0. SQLite's `lower()` is ASCII only, so
    // `Ă` never becomes `ă` and a word that starts a Vietnamese sentence can
    // never be matched on the timeline. See §20 of the grammar document.
    "when:2016..2026 ăn",
    "NOT #gia-đình",
    "-shape:chore",
    "events -with:khánh",
    "is:note (#gia-đình OR #công-việc)",
    // lower case is a word, not an operator — a vault holds sentences
    "#gia-đình or #công-việc",
    // ── brackets that do not close, and a dangling OR ──
    "(#gia-đình",
    "#gia-đình)",
    "#gia-đình OR",
    // ── §7: the pipeline ──
    "events when:2016..2026 | stats count by month",
    "events when:2016..2026 | stats count by year",
    "events when:2016..2026 | stats count by shape",
    "events when:2016..2026 | stats count by month | sort count desc",
    "events when:2016..2026 | stats count by month | head 1",
    "events when:2016..2026 | stats count by month | top 1 by count",
    "nodes columns:title,date | stats count by year",
    "events when:2016..2026 columns:when,shape | stats count by shape",
    "is:task columns:title,status | stats count by status",
    // a heap to gather by that is not there, and a day that is not there
    "is:task | stats count by status",
    "is:note | stats count by month",
    // the shapes of a broken pipeline
    "events when:2016..2026 |",
    "events when:2016..2026 | stats",
    "events when:2016..2026 | wibble",
    "events when:2016..2026 | head abc",
    // ── §5.3 of the lens design: the panels, written as questions ──
    // The gate for this step. Each of these is a hand-written panel today.
    "events when:same-day-as(2019-11-05)",
    "events when:same-day-as(2019-11-05) shape:occasion",
    "nodes when:same-day-as(2019-11-05)",
    "events columns:when,who | seq gaps by who",
    "events columns:when,who | seq gaps by who | where quiet > longest",
    "events columns:when,who | seq gaps by who | where times > 1",
    "events columns:when,who | seq gaps by who | where quiet > 6mo",
    // `timeline::silence`'s own rule, written out as a question
    "events | seq gaps by who | where times >= 5 and span >= 183d and quiet > longest and quiet > 90d",
    "events columns:when,who | seq gaps by who | sort times desc | head 1",
    // the third panel: what has gone quiet on a project
    "events columns:when,about | seq gaps by about | where quiet > 6mo",
    "events columns:when,place,who",
    // and where it will not answer
    "events | seq gaps by who",
    "events columns:when,title | seq gaps by who",
    "events columns:when,who | seq wibble by who",
    "events columns:when,who | seq gaps by who | where quiet ~ longest",
    // ── §7: the one stage that spends money, and the wall in front of it ──
    // The gate has no vault to read words from and no model to ask, so every
    // one of these is a refusal — which is the point: this is the shape of a
    // saved lens being opened.
    // the fourth panel: a year in your own words
    "events when:2019 | explode sentences",
    "events when:2019 | explode sentences | ask 1",
    "is:note | explode sentences",
    "is:note | explode sentences | ask 15",
    "is:note | explode sentences | ask 1",
    "is:note | ask 1",
    "is:note | explode sentences | ask",
    "is:note | explode wibble",
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
    // ── §11: the words that were renamed ──
    "where:\"Hà Nội\"",
    "magnitude:>2",
    "notes when:2019",
    // and on its own it is still the word somebody is searching for
    "notes",
    "in:title gặp",
    // ── nothing to ask ──
    "",
    "   ",
];

/// How one question answers, in one line.
fn answer(cache: &Mutex<DbBridge>, timeline: &TimelineStore, q: &str) -> String {
    let asked = crate::query::parse(q);
    // Refusals first, the way both runners read them: a query that carries one
    // is refused for that reason, not for being empty — and saying "nothing to
    // match on" for `limit:abc` would hide the actual complaint.
    if let Some(why) = asked.refused.first() {
        return format!("{q:<34} → refused: {why}");
    }
    if asked.asks_nothing() {
        return format!("{q:<34} → refused (nothing to match on)");
    }

    let source = asked.source_of();
    let result = if source == crate::query::Source::Events {
        let db = cache.lock().unwrap();
        let named = crate::timeline::query::Named(
            crate::timeline::query::names_in(&asked.filter)
                .into_iter()
                .map(|name| {
                    let found = crate::timeline::store::node_for(&db, &name)
                        .and_then(|id| {
                            db.conn()
                                .query_row(
                                    "SELECT COALESCE(NULLIF(json_extract(properties, '$.node_id'), ''), stable_id, id) FROM nodes WHERE id = ?1",
                                    [&id],
                                    |r| r.get::<_, String>(0),
                                )
                                .ok()
                        })
                        .unwrap_or_else(|| name.clone());
                    (name, found)
                })
                .collect(),
        );
        crate::timeline::query::run(timeline, &asked, &named).map(|mut found| {
            // The command names the people before the pipeline runs; so does
            // this, or the gate would be recording a path nothing takes.
            if let Some(at) = found.columns.iter().position(|c| c == "who") {
                let ids: Vec<String> = found
                    .rows
                    .iter()
                    .filter_map(|row| row.cells.get(at))
                    .flat_map(|cell| cell.split(',').map(|id| id.trim().to_string()))
                    .filter(|id| !id.is_empty())
                    .collect();
                let borrowed: Vec<&str> = ids.iter().map(String::as_str).collect();
                let names = crate::timeline::store::names_for(&db, &borrowed);
                for row in &mut found.rows {
                    if let Some(cell) = row.cells.get_mut(at) {
                        *cell = cell
                            .split(',')
                            .map(str::trim)
                            .filter(|id| !id.is_empty())
                            .map(|id| names.get(id).cloned().unwrap_or_else(|| id.to_string()))
                            .collect::<Vec<_>>()
                            .join(", ");
                    }
                }
            }
            found
        })
    } else {
        cache.lock().unwrap().run_node_query(&asked)
    };

    // The pipeline runs where the command runs it, with what the command gives
    // it: the vault's own words, and **no asker** — which is the shape of a
    // saved lens being opened.
    let db = cache.lock().unwrap();
    let words = crate::commands::nexus::VaultWords::of(&db, "");
    let result = match (result, words) {
        (Ok(found), Ok(words)) => crate::pipeline::run_around(
            &asked,
            found,
            &crate::pipeline::Around {
                today: chrono::NaiveDate::from_ymd_opt(2026, 9, 20).expect("a real day"),
                words: Some(&words),
                asker: None,
            },
        ),
        (result, _) => result,
    };

    let source = if source == crate::query::Source::Events { "events" } else { "nodes " };
    match result {
        Ok(found) => {
            // In the order they came back, deliberately: sorting them here
            // would hide every difference `sort:` makes, which is most of what
            // there is to see about `sort:`.
            let titles: Vec<&str> =
                found.rows.iter().take(3).map(|r| r.title.as_str()).collect();
            let note = found.note.map(|n| format!(" — {n}")).unwrap_or_default();
            format!(
                "{q:<34} → {source} {:>3} [{}]{note}",
                found.total,
                titles.join(" · ")
            )
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
