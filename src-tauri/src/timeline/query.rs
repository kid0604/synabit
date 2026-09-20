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
use crate::query::{Expr, Field, Query, Term, Value};
use crate::refusal::Refusal;

/// What a `with:`/`place:`/`about:` name turned out to be.
///
/// Resolving needs the vault cache and running needs the timeline, and they
/// are different connections — so the caller resolves first and hands the
/// answers in. It also means this function has no opinion about what a name
/// is, which is the only way `with:khánh` and `with:People/khanh.md` can mean
/// the same thing.
///
/// A map from the written name rather than three lists in order: with a tree,
/// a name can sit anywhere — inside a bracket, under a `NOT` — so there is no
/// position to line up against. A name nobody resolved is used as written,
/// which is how a question naming an identity directly still works.
#[derive(Debug, Default, Clone)]
pub struct Named(pub std::collections::HashMap<String, String>);

impl Named {
    fn of(&self, written: &str) -> String {
        self.0.get(written).cloned().unwrap_or_else(|| written.to_string())
    }
}

/// Every name a question hands to the vault to resolve.
pub fn names_in(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(expr: &Expr, out: &mut Vec<String>) {
        match expr {
            Expr::Term(Term {
                field: Field::With | Field::Place | Field::About,
                value: Value::Text(name),
            }) => out.push(name.clone()),
            Expr::Term(_) => {}
            Expr::Not(inner) => walk(inner, out),
            Expr::And(branches) | Expr::Or(branches) => {
                for branch in branches {
                    walk(branch, out);
                }
            }
        }
    }
    walk(expr, &mut out);
    out
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

/// The three roles that can be asked for as a column, and the link role each
/// reads.
///
/// All three, not just `who`: `| seq gaps by about` — what has gone quiet on a
/// project — is the same question as the one about a person, and leaving the
/// column out would have made it a question the language could not ask.
///
/// The fourth role, `evidence`, is not here for the reason it is not a
/// keyword either: nobody looks for "events a photograph belongs to", they
/// look at the photograph.
fn role_column(name: &str) -> Option<&'static str> {
    match name {
        "who" => Some("with"),
        "place" => Some("where"),
        "about" => Some("about"),
        _ => None,
    }
}

/// What a timeline query shows when it was not told.
const BY_DEFAULT: &[&str] = &["when", "title", "who"];

/// Rows this asks for at once when the query did not say.
const A_PAGEFUL: u32 = 200;

/// Nothing may ask for the whole index in one go.
const AT_MOST: u32 = 1000;

/// Building the `WHERE` clause of a timeline question, one branch at a time.
struct Where<'a> {
    params: Vec<Sql>,
    named: &'a Named,
}

impl Where<'_> {
    fn bind(&mut self, value: Sql) -> String {
        self.params.push(value);
        format!("?{}", self.params.len())
    }

    fn condition(&mut self, expr: &Expr) -> AppResult<String> {
        Ok(match expr {
            Expr::Term(term) => self.term(term)?,
            // "Unknown" counts as "not true", the same rule the node runner
            // uses and for the same reason: `NOT NULL` is NULL in SQL, which
            // the WHERE clause throws away, so an event with nothing recorded
            // would fail "not with Khánh".
            Expr::Not(inner) => {
                let inner = self.condition(inner)?;
                format!("COALESCE({inner}, 0) = 0")
            }
            Expr::And(branches) if branches.is_empty() => "1".to_string(),
            Expr::And(branches) => self.join(branches, " AND ")?,
            Expr::Or(branches) if branches.is_empty() => "0".to_string(),
            Expr::Or(branches) => self.join(branches, " OR ")?,
        })
    }

    fn join(&mut self, branches: &[Expr], with: &str) -> AppResult<String> {
        let mut parts = Vec::with_capacity(branches.len());
        for branch in branches {
            parts.push(self.condition(branch)?);
        }
        Ok(format!("({})", parts.join(with)))
    }

    fn term(&mut self, term: &Term) -> AppResult<String> {
        Ok(match (&term.field, &term.value) {
            // `when:` — anything `timeline::when` can read, which is every
            // shape a date takes in this app. An unreadable one is refused
            // rather than ignored: silently answering a different question
            // than the one asked is worse than saying no.
            (Field::When, Value::Text(text)) => {
                let span = when::parse(text).ok_or_else(|| {
                    AppError::Refused(Refusal::not_a_time(text, when::HOW_TO_WRITE_ONE))
                })?;
                let to = self.bind(Sql::Text(when::iso(span.to)));
                let from = self.bind(Sql::Text(when::iso(span.from)));
                format!("(e.happened_from <= {to} AND e.happened_to >= {from})")
            }
            // This day in other years. `strftime` on the start of the event
            // rather than on both ends: a thing that ran for a week has one
            // anniversary, the day it began, the way a birthday is a day and
            // not a stretch.
            (Field::SameDay, Value::Text(text)) => {
                let day = when::same_day_as(text).ok_or_else(|| {
                    AppError::Refused(Refusal::not_an_anniversary(text, when::HOW_TO_WRITE_ONE))
                })?;
                let at = self.bind(Sql::Text(day));
                format!("strftime('%m-%d', e.happened_from) = {at}")
            }
            (Field::With | Field::Place | Field::About, Value::Text(name)) => {
                let role = match term.field {
                    Field::With => "with",
                    Field::Place => "where",
                    _ => "about",
                };
                let who = self.named.of(name);
                let (role, who) = (self.bind(Sql::Text(role.into())), self.bind(Sql::Text(who)));
                format!(
                    "EXISTS (SELECT 1 FROM event_links l
                             WHERE l.event_id = e.id AND l.role = {role} AND l.node_id = {who})"
                )
            }
            // `is:note when:2019` used to answer as though `is:note` had not
            // been written. An event knows the type of the node it came from,
            // so this is a question the timeline can actually answer.
            (Field::Kind, Value::Text(kind)) => {
                let at = self.bind(Sql::Text(kind.clone()));
                format!("e.node_type = {at}")
            }
            (Field::Shape, Value::Text(shape)) => {
                let at = self.bind(Sql::Text(shape.to_lowercase()));
                format!("e.shape = {at}")
            }
            (Field::Size, Value::Number(op, size)) => {
                // The operator comes from a fixed set, never from the text.
                let at = self.bind(Sql::Real(*size));
                format!("e.magnitude {} {at}", op.as_sql())
            }
            // A word with no `key:` in front of it matches the event's own
            // name.
            //
            // Not FTS: the search index holds nodes, and an event's title is
            // often a sentence the extractor wrote out that no node carries.
            // Matching a few hundred titles directly is honest here, and it
            // keeps the two indexes from having to agree about anything.
            //
            // But it has to match a WORD, not a run of letters. The real vault
            // said so immediately: `ăn` found fifteen events, and the first
            // three were «công **văn**» and «Bùi **Văn** Phương». Vietnamese is
            // written in syllables separated by spaces, so a substring test
            // turns every short word into a wildcard.
            //
            // `vwords` is what pads and flattens it — and it is what lowercases
            // it, which SQLite's own `lower()` could not do. Until it did, no
            // word that begins a title was findable at all: titles are
            // sentences, so their first word is capitalised, and `ăn` never
            // found «Ăn tối với Minh». See `db::text`.
            (Field::Text, Value::Text(written)) => {
                let word = written.trim_matches('"').trim();
                if word.is_empty() {
                    return Ok("1".to_string());
                }
                let at = self.bind(Sql::Text(format!("%{}%", crate::db::text::words_in(word))));
                format!("vwords(e.title) LIKE {at}")
            }
            // Everything a question carries has to be either answered or
            // refused. These are fields of a *node*, and an event is not one —
            // so asking `#gia-đình when:2019` used to drop the tag on the floor
            // and answer a question about the whole year instead. Dropping half
            // a question is the one thing §9 will not have.
            (field, _) => {
                return Err(AppError::Refused(Refusal::node_field_on_events(
                    field.written(),
                )))
            }
        })
    }
}

pub fn run(store: &TimelineStore, query: &Query, named: &Named) -> AppResult<QueryResult> {
    let started = std::time::Instant::now();

    if let Some(why) = query.refused.first() {
        return Err(AppError::Refused(why.clone()));
    }
    if query.title_only {
        return Err(AppError::Refused(Refusal::node_field_on_events("in:title")));
    }

    let mut build = Where {
        params: Vec::new(),
        named,
    };
    let condition = build.condition(&query.filter)?;
    let mut params = build.params;
    let next = params.len() + 1;

    let sql = format!(
        "FROM events e \
         WHERE e.superseded_by IS NULL AND e.source != 'extract' AND e.folded_into IS NULL \
           AND ({condition})"
    );

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
    let (order_by, descending) = match &query.sort {
        Some(order) => (
            column_sql(&order.key).unwrap_or("e.happened_from"),
            order.descending,
        ),
        None => ("e.happened_from", true),
    };
    let direction = if descending { "DESC" } else { "ASC" };

    // §8: with a pipeline, `limit:` is about the **final** answer, so the
    // filter half runs to the internal ceiling instead.
    let limit = if query.stages.is_empty() {
        query.limit.unwrap_or(A_PAGEFUL).min(AT_MOST)
    } else {
        crate::pipeline::CEILING
    };
    let wanted: Vec<String> = {
        let asked: Vec<&str> = query.columns.iter().map(String::as_str).collect();
        let asked = if asked.is_empty() { BY_DEFAULT.to_vec() } else { asked };
        asked
            .into_iter()
            .filter(|name| role_column(name).is_some() || column_sql(name).is_some())
            .map(str::to_string)
            .collect()
    };

    let reads: Vec<String> = wanted
        .iter()
        .map(|name| {
            // These are not columns of `events`; they are what the event
            // names, gathered from the links in the same statement so a page
            // of rows is one query rather than one query per row.
            match role_column(name) {
                Some(role) => format!(
                    "(SELECT group_concat(l.node_id, ', ') FROM event_links l \
                      WHERE l.event_id = e.id AND l.role = '{role}')"
                ),
                None => column_sql(name).unwrap_or("''").to_string(),
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
    params.push(Sql::Integer(query.offset as i64));

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

    let rows: Vec<QueryRow> = rows.flatten().collect();
    // Reaching the ceiling is admitted, not hidden.
    let note = crate::pipeline::hit_the_ceiling(query, rows.len(), total as usize);

    Ok(QueryResult {
        columns: wanted,
        rows,
        total: total as usize,
        query_time_ms: started.elapsed().as_millis() as u64,
        note,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
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
    fn named(cache: &Mutex<DbBridge>, asked: &Query) -> Named {
        let db = cache.lock().unwrap();
        Named(
            names_in(&asked.filter)
                .into_iter()
                .map(|name| {
                    let found = crate::timeline::store::node_for(&db, &name)
                        .and_then(|id| identity(&db, &id))
                        .unwrap_or_else(|| name.clone());
                    (name, found)
                })
                .collect(),
        )
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
        let asked = crate::query::parse(q);
        assert_eq!(asked.source_of(), crate::query::Source::Events, "'{q}' is not a timeline question");
        run(timeline, &asked, &named(cache, &asked)).expect("the query runs")
    }

    #[test]
    fn a_question_without_the_timeline_s_words_is_still_a_question_about_notes() {
        for q in ["is:task", "#family", "status:done", "báo cáo"] {
            assert_eq!(crate::query::parse(q).source_of(), crate::query::Source::Nodes, "{q}");
        }
        for q in ["when:2019", "with:khánh", "place:hanoi", "about:synabit", "shape:occasion", "size:>4"] {
            assert_eq!(crate::query::parse(q).source_of(), crate::query::Source::Events, "{q}");
        }
        // And saying it out loud beats guessing from the words, both ways.
        assert_eq!(crate::query::parse("events #family").source_of(), crate::query::Source::Events);
        assert_eq!(crate::query::parse("nodes when:2019").source_of(), crate::query::Source::Nodes);
    }

    /// The two words §11 renamed say so, rather than quietly becoming a filter
    /// on a frontmatter key of that name and answering 0.
    #[test]
    fn the_old_spellings_say_what_they_are_now_called() {
        for (old, now) in [("where:hanoi", "place"), ("magnitude:>4", "size")] {
            let refused = crate::query::parse(old).refused;
            assert!(
                refused.first().is_some_and(|why| why.to_string().contains(now)),
                "{old} → {refused:?}"
            );
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
        let parsed = crate::query::parse("when:hôm-nào-đó");
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
        let big = ask(&cache, &timeline, "when:2016/2026 size:>5");
        assert!(big.total >= 1, "the job is the big thing here");
        assert!(big.rows.iter().all(|r| r.title.contains("MDP")), "{:?}", big.rows);
        assert_eq!(ask(&cache, &timeline, "when:2016/2026 size:>99").total, 0);
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
                    { "title": "Ăn tối với Minh", "happened": "2026-06-03" },
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
        let mut titles: Vec<&str> = found.rows.iter().map(|r| r.title.as_str()).collect();
        titles.sort_unstable();
        assert_eq!(
            titles,
            ["Ăn tối với Minh", "ăn trưa: cơm cá kho"],
            "«công văn» is not «ăn», and «Ăn» is"
        );
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
            "when:2026 size:>4",
            "when:2026 ăn",
            "when:2026 văn",
            "when:2026 họp",
            "when:2026 shape:occasion -ăn limit:5",
            "shape:spell",
        ] {
            let parsed = crate::query::parse(q);
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

