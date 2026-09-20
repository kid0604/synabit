//! Asking the timeline in the same words the vault is asked in.
//!
//! The design is `docs/nexus-lenses-2026-09-19.md`, step 1.
//!
//! # Why this exists
//!
//! `nodes` has had a query language for a long time — `is:`, `#tag`,
//! `status:`, `prop:>x`, `sort:`, `columns:` — a runner, and view primitives
//! with a written contract. `events` had none of it. Every question anybody
//! wanted to ask of the timeline therefore cost a Rust module, a Vue
//! component, two locale files and a command, which is why there were eight
//! panels and still not enough.
//!
//! So the same [`ParsedQuery`] now carries six more words, and this runs them.
//! Nothing about the old ones changes.
//!
//! # One result shape, on purpose
//!
//! This returns [`QueryResult`] — the very struct `run_node_query` returns, so
//! every view that can draw a node query can draw a timeline query without
//! being told about events at all. That is the whole reason a new question
//! will stop costing new code: the answer is a table of rows, and the views
//! already know what to do with one.
//!
//! # What a row's `id` is, and why `open` had to be added
//!
//! For a node query the row's id is the thing to open. An event's id is
//! `path#kind#n` and is *not* — opening it would open nothing. But the id must
//! stay unique, because a view keys rows by it and two events on one day come
//! from one note. So the id stays the event's, and `open` carries the node to
//! open. A node query leaves `open` empty, meaning "open the id".

use rusqlite::types::Value as Sql;
use rusqlite::ToSql;

use super::store::TimelineStore;
use super::when;
use crate::db::{QueryResult, QueryRow};
use crate::error::{AppError, AppResult};
use crate::search::ParsedQuery;

/// What a `with:`/`where:`/`about:` name turned out to be.
///
/// Resolving needs the vault cache and running needs the timeline, and they
/// are different connections — so the caller resolves first and hands the
/// answers in. It also means this function has no opinion about what a name
/// is, which is the only way `with:khánh` and `with:People/khanh.md` can mean
/// the same thing.
#[derive(Debug, Default, Clone)]
pub struct Named {
    pub with: Vec<String>,
    pub place: Vec<String>,
    pub about: Vec<String>,
}

/// The columns a timeline query can show, and what each reads.
///
/// A small closed set rather than anything in `props`: these are the facts
/// every event has. Asking for something else is not an error — the column is
/// dropped, the same way `run_node_query` drops a key no node carries.
fn column_sql(name: &str) -> Option<&'static str> {
    Some(match name {
        "when" | "from" => "e.happened_from",
        "to" => "e.happened_to",
        "title" => "e.title",
        "note" | "source" => "COALESCE(NULLIF(e.container_node, ''), e.node_id)",
        "kind" => "e.kind",
        "shape" => "e.shape",
        "magnitude" | "size" => "CAST(ROUND(e.magnitude, 2) AS TEXT)",
        "precision" => "e.precision",
        _ => return None,
    })
}

/// Every character that stands between two words here.
const BETWEEN_WORDS: &[&str] =
    &[",", ".", ";", ":", "!", "?", "(", ")", "\"", "'", "/", "\u{b7}", "\u{2014}", "\u{2013}", "\u{ab}", "\u{bb}"];

/// The title as one space-padded, punctuation-flattened string, so that
/// `LIKE '% word %'` matches a word rather than a run of letters.
///
/// Built once as a derived column rather than repeated in every term's
/// condition: with three words in a query the inline version wrote the same
/// sixteen nested `replace`s three times, which is unreadable in a log and
/// impossible to check by eye.
///
/// Not a tokenizer and not pretending to be one — a word glued to a character
/// outside [`BETWEEN_WORDS`] is still missed. That is a far smaller wrong than
/// `ăn` matching `văn`.
fn flattened_title() -> String {
    let mut read = String::from("lower(' ' || title || ' ')");
    for separator in BETWEEN_WORDS {
        // A literal `'` inside a SQL string is written twice. Missing this
        // made every query with a bare word a syntax error.
        let escaped = separator.replace('\'', "''");
        read = format!("replace({read}, '{escaped}', ' ')");
    }
    read
}

/// What a timeline query shows when it was not told.
const BY_DEFAULT: &[&str] = &["when", "title", "who"];

/// Rows this asks for at once when the query did not say.
const A_PAGEFUL: u32 = 200;

/// Nothing may ask for the whole index in one go.
const AT_MOST: u32 = 1000;

pub fn run(store: &TimelineStore, parsed: &ParsedQuery, named: &Named) -> AppResult<QueryResult> {
    let started = std::time::Instant::now();

    if let Some(why) = parsed.refused.first() {
        return Err(AppError::General(why.clone()));
    }

    // Everything a question carries has to be either answered or refused.
    //
    // These are fields of a *node*, and an event is not one — so asking
    // `#gia-đình when:2019` used to drop the tag on the floor and answer a
    // question about the whole year instead. Dropping half a question is the
    // one thing §9 of `docs/query-grammar-2026-09-20.md` will not have.
    //
    // `type:` is the exception, and it is answered rather than refused: an
    // event knows which kind of node wrote it.
    let unanswerable = [
        (!parsed.tag_filters.is_empty(), "#tag"),
        (!parsed.tag_exclusions.is_empty(), "-#tag"),
        (parsed.status_filter.is_some(), "status:"),
        (!parsed.property_filters.is_empty(), "a note's own fields"),
        (!parsed.property_ranges.is_empty(), "a note's own fields"),
        (!parsed.property_exclusions.is_empty(), "-field:value"),
        (parsed.title_only, "in:title"),
    ];
    if let Some((_, what)) = unanswerable.into_iter().find(|(carried, _)| *carried) {
        return Err(AppError::General(format!(
            "{what} asks about a note, and this question is about the timeline. \
             Ask it without the timeline's words, or drop it."
        )));
    }

    let mut sql = format!(
        "FROM (SELECT events.*, {} AS word_title FROM events) e \
         WHERE e.superseded_by IS NULL AND e.source != 'extract' AND e.folded_into IS NULL",
        flattened_title()
    );
    let mut params: Vec<Sql> = Vec::new();
    let mut next = 1usize;

    // `when:` — anything `timeline::when` can read, which is every shape a
    // date takes in this app. An unreadable one is refused rather than
    // ignored: silently answering a different question than the one asked is
    // worse than saying no.
    if let Some(text) = &parsed.when {
        let span = when::parse(text).ok_or_else(|| {
            AppError::General(format!(
                "'{text}' is not a time. Use 2016-05-14, 2016-05, 2016, \
                 2016-05-01/2016-06-30 or ~2012."
            ))
        })?;
        sql.push_str(&format!(
            " AND e.happened_from <= ?{next} AND e.happened_to >= ?{}",
            next + 1
        ));
        params.push(Sql::Text(when::iso(span.to)));
        params.push(Sql::Text(when::iso(span.from)));
        next += 2;
    }

    for (role, names) in
        [("with", &named.with), ("where", &named.place), ("about", &named.about)]
    {
        for name in names {
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM event_links l
                              WHERE l.event_id = e.id AND l.role = ?{next} AND l.node_id = ?{})",
                next + 1
            ));
            params.push(Sql::Text(role.to_string()));
            params.push(Sql::Text(name.clone()));
            next += 2;
        }
    }

    // `is:note when:2019` used to answer as though `is:note` had not been
    // written. An event knows the type of the node it came from, so this is a
    // question the timeline can actually answer.
    if let Some(node_type) = &parsed.type_filter {
        sql.push_str(&format!(" AND e.node_type = ?{next}"));
        params.push(Sql::Text(node_type.clone()));
        next += 1;
    }

    if let Some(shape) = &parsed.shape {
        sql.push_str(&format!(" AND e.shape = ?{next}"));
        params.push(Sql::Text(shape.to_lowercase()));
        next += 1;
    }

    if let Some((comparison, size)) = &parsed.magnitude {
        // The operator comes from a fixed set, never from the person's text.
        sql.push_str(&format!(" AND e.magnitude {} ?{next}", comparison.as_sql()));
        params.push(Sql::Real(*size));
        next += 1;
    }

    // Words with no `key:` in front of them match the event's own name.
    //
    // Not FTS: the search index holds nodes, and an event's title is often a
    // sentence the extractor wrote out that no node carries. Matching a few
    // hundred titles directly is honest here, and it keeps the two indexes
    // from having to agree about anything.
    //
    // But it has to match a WORD, not a run of letters. The real vault said so
    // immediately: `ăn` found fifteen events, and the first three were «công
    // **văn**» and «Bùi **Văn** Phương». Vietnamese is written in syllables
    // separated by spaces, so a substring test turns every short word into a
    // wildcard. Hence [`a_word_in`]: the title padded and with punctuation
    // turned to spaces, so `% ăn %` means the word and nothing else.
    for term in &parsed.fts_terms {
        let word = term.trim_matches('"').trim();
        if word.is_empty() {
            continue;
        }
        sql.push_str(&format!(" AND e.word_title LIKE ?{next}"));
        params.push(Sql::Text(format!("% {} %", word.to_lowercase())));
        next += 1;
    }
    for term in &parsed.exclude_terms {
        let word = term.trim();
        if word.is_empty() {
            continue;
        }
        sql.push_str(&format!(" AND e.word_title NOT LIKE ?{next}"));
        params.push(Sql::Text(format!("% {} %", word.to_lowercase())));
        next += 1;
    }

    let total: i64 = {
        let bound: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        store
            .conn()
            .query_row(&format!("SELECT COUNT(*) {sql}"), bound.as_slice(), |r| r.get(0))
            .map_err(|e| AppError::General(format!("timeline query: {e}")))?
    };

    // Which way round, and by what. `sort:-when` is the common one and the
    // default, because a timeline read newest-first is what a person means by
    // "show me".
    let (order_by, descending) = match &parsed.sort {
        Some(order) => (
            column_sql(&order.key).unwrap_or("e.happened_from"),
            order.descending,
        ),
        None => ("e.happened_from", true),
    };
    let direction = if descending { "DESC" } else { "ASC" };

    let limit = parsed.limit.unwrap_or(A_PAGEFUL).min(AT_MOST);
    let wanted: Vec<String> = {
        let asked: Vec<&str> = parsed.columns.iter().map(String::as_str).collect();
        let asked = if asked.is_empty() { BY_DEFAULT.to_vec() } else { asked };
        asked
            .into_iter()
            .filter(|name| *name == "who" || column_sql(name).is_some())
            .map(str::to_string)
            .collect()
    };

    let reads: Vec<String> = wanted
        .iter()
        .map(|name| {
            // `who` is not a column of `events`; it is everyone the event
            // names, gathered from the links in the same statement so a page
            // of rows is one query rather than one query per row.
            if name == "who" {
                "(SELECT group_concat(l.node_id, ', ') FROM event_links l \
                  WHERE l.event_id = e.id AND l.role = 'with')"
                    .to_string()
            } else {
                column_sql(name).unwrap_or("''").to_string()
            }
        })
        .collect();

    let statement = format!(
        "SELECT e.id, e.node_type, e.title, \
                COALESCE(NULLIF(e.container_node, ''), e.node_id){}{} \
         {sql} ORDER BY {order_by} {direction}, e.id LIMIT ?{next} OFFSET ?{}",
        if reads.is_empty() { "" } else { ", " },
        reads.join(", "),
        next + 1
    );
    params.push(Sql::Integer(limit as i64));
    params.push(Sql::Integer(parsed.offset as i64));

    let columns = wanted.len();
    let conn = store.conn();
    let mut stmt = conn
        .prepare(&statement)
        .map_err(|e| AppError::General(format!("timeline query: {e}")))?;
    let bound: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let rows = stmt
        .query_map(bound.as_slice(), |r| {
            let mut cells = Vec::with_capacity(columns);
            for n in 0..columns {
                cells.push(r.get::<_, Option<String>>(4 + n)?.unwrap_or_default());
            }
            Ok(QueryRow {
                id: r.get(0)?,
                node_type: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                title: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                cells,
                open: r.get::<_, Option<String>>(3)?,
            })
        })
        .map_err(|e| AppError::General(format!("timeline query: {e}")))?;

    Ok(QueryResult {
        columns: wanted,
        rows: rows.flatten().collect(),
        total: total as usize,
        query_time_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use crate::search::parse_query;
    use crate::timeline::store::catch_up;

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

    /// A vault with three days, two people and a job.
    fn a_vault() -> (Mutex<DbBridge>, TimelineStore) {
        let db = DbBridge::new_in_memory_full().unwrap();
        for n in [
            node("People/khanh.md", "person", "Khánh", json!({ "node_id": "uuid-khanh" })),
            node("People/minh.md", "person", "Minh", json!({ "node_id": "uuid-minh" })),
            node(
                "People/me.md",
                "person",
                "Tao",
                json!({
                    "node_id": "uuid-me",
                    "experiences": [{ "start": "2019-01-01", "end": "2021-06-30", "role": "Dev", "company": "MDP" }]
                }),
            ),
            node(
                "Notes/2019-11-05.md",
                "note",
                "2019-11-05",
                json!({
                    "date": "2019-11-05",
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
                json!({
                    "date": "2021-03-14",
                    "moments": [{ "title": "Cà phê sáng với Khánh", "happened": "2021-03-14", "people": ["uuid-khanh"] }]
                }),
            ),
            node("Tasks/a.md", "task", "Gửi báo cáo quý", json!({ "completed_at": "2019-11-05" })),
        ] {
            db.upsert_node(&n).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        (cache, timeline)
    }

    /// Resolve names the way the command does, so the tests exercise the
    /// same path.
    fn named(cache: &Mutex<DbBridge>, parsed: &ParsedQuery) -> Named {
        let db = cache.lock().unwrap();
        let resolve = |names: &Vec<String>| -> Vec<String> {
            names
                .iter()
                .map(|name| {
                    crate::timeline::store::node_for(&db, name)
                        .and_then(|id| identity(&db, &id))
                        .unwrap_or_else(|| name.clone())
                })
                .collect()
        };
        Named {
            with: resolve(&parsed.with),
            place: resolve(&parsed.place),
            about: resolve(&parsed.about),
        }
    }

    fn identity(db: &DbBridge, id: &str) -> Option<String> {
        db.conn()
            .query_row(
                "SELECT COALESCE(json_extract(properties, '$.node_id'), stable_id, id) FROM nodes WHERE id = ?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .ok()
    }

    fn ask(cache: &Mutex<DbBridge>, timeline: &TimelineStore, q: &str) -> QueryResult {
        let parsed = parse_query(q);
        assert!(parsed.asks_the_timeline(), "'{q}' did not read as a timeline question");
        run(timeline, &parsed, &named(cache, &parsed)).expect("the query runs")
    }

    #[test]
    fn a_question_without_the_timeline_s_words_is_still_a_question_about_notes() {
        for q in ["is:task", "#family", "status:done", "báo cáo"] {
            assert!(!parse_query(q).asks_the_timeline(), "{q}");
        }
        for q in ["when:2019", "with:khánh", "where:hanoi", "about:synabit", "shape:occasion", "magnitude:>4"] {
            assert!(parse_query(q).asks_the_timeline(), "{q}");
        }
    }

    #[test]
    fn asking_for_a_person_by_name_finds_what_they_were_at() {
        let (cache, timeline) = a_vault();
        let found = ask(&cache, &timeline, "with:khánh");
        let titles: Vec<&str> = found.rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Cà phê sáng với Khánh", "Gặp Khánh ở quán quen"], "newest first");
        assert_eq!(found.total, 2);
    }

    #[test]
    fn a_path_and_a_name_ask_the_same_thing() {
        let (cache, timeline) = a_vault();
        assert_eq!(
            ask(&cache, &timeline, "with:khánh").total,
            ask(&cache, &timeline, "with:People/khanh.md").total
        );
    }

    #[test]
    fn two_people_means_both_were_there_not_either() {
        let (cache, timeline) = a_vault();
        assert_eq!(ask(&cache, &timeline, "with:khánh with:minh").total, 0);
        assert_eq!(ask(&cache, &timeline, "with:minh").total, 1);
    }

    #[test]
    fn a_time_narrows_it() {
        let (cache, timeline) = a_vault();
        assert_eq!(ask(&cache, &timeline, "with:khánh when:2019").total, 1);
        assert_eq!(ask(&cache, &timeline, "with:khánh when:2019/2026").total, 2);
        assert_eq!(ask(&cache, &timeline, "when:2019-11-05").total > 1, true);
    }

    #[test]
    fn a_time_nobody_can_read_is_refused_rather_than_ignored() {
        let (cache, timeline) = a_vault();
        let parsed = parse_query("when:hôm-nào-đó");
        let refused = run(&timeline, &parsed, &named(&cache, &parsed));
        assert!(refused.is_err(), "answering a different question is worse than saying no");
    }

    #[test]
    fn shape_separates_a_job_from_an_evening() {
        let (cache, timeline) = a_vault();
        let jobs = ask(&cache, &timeline, "shape:spell");
        assert_eq!(jobs.total, 1);
        assert!(jobs.rows[0].title.contains("MDP"), "{:?}", jobs.rows[0]);

        let chores = ask(&cache, &timeline, "shape:chore");
        assert_eq!(chores.rows.len(), 1);
        assert_eq!(chores.rows[0].title, "Gửi báo cáo quý");
    }

    #[test]
    fn size_asks_for_the_big_things() {
        let (cache, timeline) = a_vault();
        let big = ask(&cache, &timeline, "when:2016/2026 magnitude:>5");
        assert!(big.total >= 1, "the job is the big thing here");
        assert!(big.rows.iter().all(|r| r.title.contains("MDP")), "{:?}", big.rows);
        assert_eq!(ask(&cache, &timeline, "when:2016/2026 magnitude:>99").total, 0);
    }

    #[test]
    fn a_bare_word_matches_the_event_s_own_name() {
        let (cache, timeline) = a_vault();
        assert_eq!(ask(&cache, &timeline, "when:2016/2026 cà phê").total, 1);
        assert_eq!(ask(&cache, &timeline, "when:2016/2026 -cà").total > 1, true);
    }

    /// The row a view is handed: unique key, a thing to open, and the columns
    /// it asked for.
    /// Straight from the real vault, where this was the first thing that
    /// went wrong: a bare word must match a word.
    #[test]
    fn a_short_word_does_not_match_the_inside_of_a_longer_one() {
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node(
            "Notes/2026-06-03.md",
            "note",
            "2026-06-03",
            json!({
                "date": "2026-06-03",
                "moments": [
                    { "title": "ăn trưa: cơm cá kho", "happened": "2026-06-03" },
                    { "title": "Làm công văn cấp chứng thư số", "happened": "2026-06-03" },
                    { "title": "Onboard Bùi Văn Phương", "happened": "2026-06-03" }
                ]
            }),
        ))
        .unwrap();
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let found = ask(&cache, &timeline, "when:2026 ăn");
        let titles: Vec<&str> = found.rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["ăn trưa: cơm cá kho"], "«công văn» is not «ăn»");
    }

    /// Punctuation is not a word boundary the eye sees, so it must not be one
    /// the query trips over.
    #[test]
    fn a_word_is_found_next_to_a_colon_or_a_full_stop() {
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node(
            "Notes/2026-06-04.md",
            "note",
            "2026-06-04",
            json!({
                "date": "2026-06-04",
                "moments": [{ "title": "Họp postmortem: oracle chết.", "happened": "2026-06-04" }]
            }),
        ))
        .unwrap();
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        assert_eq!(ask(&cache, &timeline, "when:2026 postmortem").total, 1);
        assert_eq!(ask(&cache, &timeline, "when:2026 chết").total, 1, "before a full stop");
    }

    #[test]
    fn a_row_keys_on_the_event_and_opens_the_note() {
        let (cache, timeline) = a_vault();
        let found = ask(&cache, &timeline, "with:khánh when:2019 columns:when,title,note");
        assert_eq!(found.columns, ["when", "title", "note"]);
        let row = &found.rows[0];
        assert!(row.id.starts_with("Notes/2019-11-05.md#"), "unique per event: {}", row.id);
        assert_eq!(row.open.as_deref(), Some("Notes/2019-11-05.md"), "what a click opens");
        assert_eq!(row.cells[0], "2019-11-05");
        assert_eq!(row.cells[2], "Notes/2019-11-05.md");
    }

    #[test]
    fn who_was_there_is_a_column_even_though_it_is_not_a_field() {
        let (cache, timeline) = a_vault();
        let found = ask(&cache, &timeline, "when:2019-11-05 columns:title,who");
        let evening = found
            .rows
            .iter()
            .find(|r| r.title.contains("Minh"))
            .expect("the evening with Minh");
        assert_eq!(evening.cells[1], "uuid-minh");
    }

    #[test]
    fn a_column_nobody_has_is_dropped_rather_than_failing_the_query() {
        let (cache, timeline) = a_vault();
        let found = ask(&cache, &timeline, "with:khánh columns:when,severity,title");
        assert_eq!(found.columns, ["when", "title"]);
    }

    #[test]
    fn the_count_is_of_everything_matching_not_of_the_page() {
        let (cache, timeline) = a_vault();
        let found = ask(&cache, &timeline, "with:khánh limit:1");
        assert_eq!(found.rows.len(), 1);
        assert_eq!(found.total, 2, "a view saying 'and more' compares these two");
    }

    #[test]
    fn sorting_the_other_way_is_asked_for_the_way_it_always_was() {
        let (cache, timeline) = a_vault();
        let oldest = ask(&cache, &timeline, "with:khánh sort:when");
        assert_eq!(oldest.rows[0].title, "Gặp Khánh ở quán quen");
    }

    /// What the real vault answers, for a handful of questions that used to
    /// need a panel each.
    ///
    ///   SYN_PROBE_CACHE=/path/to/a/copy/of/vault_cache.db \\
    ///     cargo test --lib -- --ignored asking_the_real_vault --nocapture
    #[test]
    #[ignore = "needs a copy of a real vault cache; run by hand"]
    fn asking_the_real_vault() {
        let path = std::env::var("SYN_PROBE_CACHE").expect("a copy of a vault_cache.db");
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).expect("built");

        for q in [
            "when:2026-06",
            "when:2026 shape:chore",
            "when:2026 magnitude:>4",
            "when:2026 ăn",
            "when:2026 văn",
            "when:2026 họp",
            "when:2026 shape:occasion -ăn limit:5",
            "shape:spell",
        ] {
            let parsed = parse_query(q);
            match run(&timeline, &parsed, &named(&cache, &parsed)) {
                Ok(found) => {
                    eprintln!("\n  {q}\n    {} khớp, {} ms", found.total, found.query_time_ms);
                    for row in found.rows.iter().take(3) {
                        eprintln!("      {} · {}", row.cells.first().cloned().unwrap_or_default(), row.title);
                    }
                }
                Err(e) => eprintln!("\n  {q}\n    từ chối: {e}"),
            }
        }
    }
}
