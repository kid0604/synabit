//! "Chuyện gì đã xảy ra với…" — §7.4 of `docs/timeline-2026-09-17.md`.
//!
//! A project, a person or a place that ran through a few months of someone's
//! writing and then stopped, with no ending written down anywhere.
//!
//! # It asks. It does not conclude.
//!
//! > *"«Synabit 1.0» xuất hiện 30 lần từ tháng 4 đến tháng 7, rồi thôi. Nó
//! > xong, hay nó dừng?"*
//!
//! The counts are measured and the question is a question. §7.4 forbids calling
//! any of this **dở dang** — unfinished — and the ban is not politeness. The app
//! does not know whether a thing ended well, ended badly, or is still going
//! somewhere it cannot see. "Unfinished" is a verdict on somebody's life
//! delivered by a program that counted rows, and it would be wrong often enough
//! to poison the ones it got right.
//!
//! For the same reason none of this is counted into anything. A question is not
//! a statistic about how much somebody leaves undone.
//!
//! # Asked once a year, and why that needs a file
//!
//! §7.1's "never twice in a year" was kept by arithmetic, with nothing stored
//! ([`super::onthisday`]). That trick does not work here: a question has no
//! anniversary to hang on, and §7.4 says a thing is not asked about again even
//! when the person simply walks away without answering. Walking away leaves no
//! trace to compute from, so there has to be a record.
//!
//! It goes in the vault, one file per question, because it is neither
//! derivable (tier 3 would lose it on the next rebuild, breaking §4.7's promise
//! that an index rebuilds to exactly what it was) nor local to a device (asking
//! on the laptop must not re-ask on the phone). The cost is small and bounded:
//! a question is a rare, discrete thing — a handful a year — not something
//! written on every render.
//!
//! So [`ask`] writes, and it is named for that. There is no way to see a
//! question without it counting as having been asked, which is the only reading
//! of §7.4's rule that a person would recognise.

use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{json, Value};

use super::quiet::{Nudge, Quiet, self as hush};
use super::store::TimelineStore;
use super::when;
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};

/// Where the record of having asked lives, one file per question.
pub const ASKED_DIR: &str = "Timeline/asked";

/// How many separate days a thing has to have been written about before its
/// going quiet is worth a question.
const ENOUGH_MENTIONS: usize = 8;

/// And across how long. A fortnight of heavy mention is one piece of work, not
/// a thing that ran through a life.
const ENOUGH_DAYS: i64 = 60;

/// How long it has to have been quiet. Six months is long enough that the
/// question is fair and short enough that the answer is still rememberable.
const QUIET_ENOUGH: i64 = 183;

/// And once asked, not again for this long.
const NOT_AGAIN_FOR: i64 = 365;

/// Properties that say a thing already has an ending, whoever wrote it. Asking
/// what happened to something whose end is recorded is asking a question the
/// vault already answered.
const ALREADY_ENDED: &[&str] =
    &["ended_on", "ended_at", "end", "completed_at", "closed_on", "died_on"];

/// Node types that end on their own terms and never need asking about: a task
/// carries its own state, and the app's own records are not anybody's life.
fn worth_asking_about(node_type: &str) -> bool {
    !(super::derive::is_the_apps_own(node_type) || matches!(node_type, "task" | "file"))
}

/// Something that was written about and then was not.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Question {
    /// The thing, by whichever name the timeline links it under.
    pub about: String,
    /// What to call it.
    pub name: String,
    pub node_type: String,
    /// How many separate days it was written about.
    pub times: usize,
    pub first: String,
    pub last: String,
    /// Days since the last time.
    pub quiet_for: i64,
}

/// Everything that could be asked about, most-written-about first.
///
/// Read only. [`ask`] is what turns one of these into a question actually put
/// to somebody.
pub fn open_questions(
    store: &TimelineStore,
    cache: &DbBridge,
    today: NaiveDate,
    quiet: &Quiet,
) -> AppResult<Vec<Question>> {
    let mut days: HashMap<String, Vec<String>> = HashMap::new();
    for (about, day) in store.days_about_things(today)? {
        days.entry(about).or_default().push(day);
    }
    if days.is_empty() {
        return Ok(Vec::new());
    }

    let known = describe(cache, &days.keys().map(String::as_str).collect::<Vec<_>>());
    let mut out: Vec<Question> = days
        .into_iter()
        .filter_map(|(about, days)| {
            let (node_type, name, properties) = known.get(&about)?;
            if !worth_asking_about(node_type) {
                return None;
            }
            if ALREADY_ENDED.iter().any(|key| {
                properties.get(*key).and_then(Value::as_str).is_some_and(|v| !v.trim().is_empty())
            }) {
                return None;
            }
            measure(about, name.clone(), node_type.clone(), &days, today)
        })
        .filter(|question| {
            let nudge =
                Nudge::on(question.last.clone()).naming([question.about.clone()]);
            !hush::allow(vec![nudge], quiet).is_empty()
        })
        .collect();

    // The thing written about most is the thing whose silence is loudest.
    out.sort_by(|a, b| b.times.cmp(&a.times).then_with(|| a.about.cmp(&b.about)));
    Ok(out)
}

fn measure(
    about: String,
    name: String,
    node_type: String,
    days: &[String],
    today: NaiveDate,
) -> Option<Question> {
    let dates: Vec<NaiveDate> =
        days.iter().filter_map(|d| when::parse(d)).map(|span| span.from).collect();
    if dates.len() < ENOUGH_MENTIONS {
        return None;
    }
    let (first, last) = (*dates.first()?, *dates.last()?);
    if (last - first).num_days() < ENOUGH_DAYS {
        return None;
    }
    let quiet_for = (today - last).num_days();
    if quiet_for < QUIET_ENOUGH {
        return None;
    }
    Some(Question {
        about,
        name,
        node_type,
        times: dates.len(),
        first: when::iso(first),
        last: when::iso(last),
        quiet_for,
    })
}

/// Type, title and properties for each thing, under any of its names.
fn describe(
    cache: &DbBridge,
    names: &[&str],
) -> HashMap<String, (String, String, Value)> {
    let mut out = HashMap::new();
    let Ok(mut stmt) = cache.conn().prepare(
        "SELECT id, COALESCE(stable_id, id), node_type, COALESCE(title, ''), properties FROM nodes",
    ) else {
        return out;
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    });
    let Ok(rows) = rows else { return out };
    for (id, stable_id, node_type, title, properties) in rows.flatten() {
        let properties: Value =
            serde_json::from_str(&properties.unwrap_or_default()).unwrap_or(Value::Null);
        let identity =
            properties.get("node_id").and_then(Value::as_str).unwrap_or_default().to_string();
        for name in [id.clone(), stable_id.clone(), identity] {
            if !name.is_empty() && names.contains(&name.as_str()) {
                out.insert(name, (node_type.clone(), title.clone(), properties.clone()));
            }
        }
    }
    out
}

// ─── Having asked ────────────────────────────────────────────────────

/// When each thing was last asked about, by whichever name it was asked under.
pub fn already_asked(vault_path: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join(ASKED_DIR)) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
        let (Some(about), Some(on)) = (
            value.get("about").and_then(Value::as_str),
            value.get("on").and_then(Value::as_str),
        ) else {
            continue;
        };
        // Two devices may both have asked; the later date is the one that holds.
        let on = on.to_string();
        out.entry(about.to_string())
            .and_modify(|kept: &mut String| {
                if on > *kept {
                    *kept = on.clone();
                }
            })
            .or_insert(on);
    }
    out
}

/// The one question to put now, if there is one — and the record that it was
/// put.
///
/// Writes before returning, on purpose: §7.4's rule holds whether or not the
/// person answers, so seeing the question has to be what counts.
pub fn ask(
    store: &TimelineStore,
    cache: &DbBridge,
    vault_path: &str,
    today: NaiveDate,
    quiet: &Quiet,
) -> AppResult<Option<Question>> {
    let asked = already_asked(vault_path);
    let cutoff = when::iso(today - chrono::Duration::days(NOT_AGAIN_FOR));
    let open = open_questions(store, cache, today, quiet)?;
    let Some(question) = open
        .into_iter()
        .find(|q| asked.get(&q.about).is_none_or(|on| on.as_str() <= cutoff.as_str()))
    else {
        return Ok(None);
    };
    record(vault_path, &question.about, today)?;
    Ok(Some(question))
}

fn record(vault_path: &str, about: &str, today: NaiveDate) -> AppResult<()> {
    let dir = Path::new(vault_path).join(ASKED_DIR);
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    let body = json!({
        "about": about,
        "on": when::iso(today),
        // Sync settles two copies of a JSON file by this stamp.
        "metadata": { "updated_at": chrono::Utc::now().to_rfc3339() },
    });
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_string_pretty(&body)?)
        .map_err(AppError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn every(days: i64, times: usize, from: &str) -> Vec<String> {
        let start = day(from);
        (0..times).map(|n| when::iso(start + chrono::Duration::days(days * n as i64))).collect()
    }

    #[test]
    fn something_written_about_for_months_and_then_not_at_all_is_worth_a_question() {
        let mentions = every(7, 14, "2025-04-01");
        let found =
            measure("p".into(), "Synabit 1.0".into(), "project".into(), &mentions, day("2026-09-19"))
                .expect("asked");
        assert_eq!(found.times, 14);
        assert_eq!(found.first, "2025-04-01");
        assert!(found.quiet_for > QUIET_ENOUGH);
    }

    #[test]
    fn a_fortnight_of_heavy_mention_is_one_piece_of_work() {
        let mentions = every(1, 14, "2025-04-01");
        assert_eq!(
            measure("p".into(), "Sprint".into(), "project".into(), &mentions, day("2026-09-19")),
            None
        );
    }

    #[test]
    fn something_still_being_written_about_is_not_asked_after() {
        let mentions = every(7, 14, "2026-04-01");
        assert_eq!(
            measure("p".into(), "Nay".into(), "project".into(), &mentions, day("2026-09-19")),
            None,
            "quiet for under six months"
        );
    }

    #[test]
    fn a_handful_of_mentions_is_not_a_thing_that_ran_through_a_life() {
        let mentions = every(30, 5, "2024-01-01");
        assert_eq!(
            measure("p".into(), "Ít".into(), "project".into(), &mentions, day("2026-09-19")),
            None
        );
    }

    #[test]
    fn the_app_s_own_records_are_not_anybody_s_life() {
        assert!(worth_asking_about("project"));
        assert!(worth_asking_about("person"));
        assert!(worth_asking_about("place"));
        for its_own in ["task", "json", "schema", "file", "lens", "finance_month", "syn_chat"] {
            assert!(!worth_asking_about(its_own), "{its_own}");
        }
    }

    #[test]
    fn asking_is_remembered_across_a_restart_and_forgotten_after_a_year() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        assert!(already_asked(vault).is_empty());

        record(vault, "Projects/synabit.md", day("2026-09-19")).unwrap();
        let asked = already_asked(vault);
        assert_eq!(asked.get("Projects/synabit.md").map(String::as_str), Some("2026-09-19"));
    }

    #[test]
    fn two_devices_that_both_asked_keep_the_later_date() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        record(vault, "Projects/synabit.md", day("2025-01-01")).unwrap();
        record(vault, "Projects/synabit.md", day("2026-03-04")).unwrap();
        assert_eq!(
            already_asked(vault).get("Projects/synabit.md").map(String::as_str),
            Some("2026-03-04"),
            "the later asking is the one the year runs from"
        );
    }

    // ─── The gate, on the road the real feature takes ────────────────

    use std::sync::Mutex;

    use serde_json::json;

    use crate::models::node::NodeMetadata;
    use crate::timeline::store::catch_up;

    fn node(id: &str, node_type: &str, title: &str, properties: Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            content: String::new(),
            properties,
            created_at: "2025-01-01T00:00:00.000Z".to_string(),
            updated_at: "2025-01-01T00:00:00.000Z".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    /// A note that writes about a thing on one day.
    fn about(day: &str, thing: &str, n: usize) -> NodeMetadata {
        node(
            &format!("Notes/{day}-{n}.md"),
            "note",
            day,
            json!({
                "date": day,
                "moments": [{ "title": "Làm việc", "happened": day, "people": [thing] }]
            }),
        )
    }

    fn a_vault() -> (Mutex<crate::db::DbBridge>, TimelineStore) {
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        for thing in [
            node("Projects/synabit.md", "project", "Synabit 1.0", json!({ "node_id": "uuid-syn" })),
            node(
                "Projects/mdp.md",
                "project",
                "MDP",
                // Somebody wrote down how it ended, so there is nothing to ask.
                json!({ "node_id": "uuid-mdp", "ended_on": "2025-09-01" }),
            ),
        ] {
            db.upsert_node(&thing).unwrap();
        }
        // Both written about weekly for three months, then nothing.
        for week in 0..14 {
            let day = when::iso(day("2025-04-01") + chrono::Duration::days(7 * week));
            db.upsert_node(&about(&day, "uuid-syn", 1)).unwrap();
            db.upsert_node(&about(&day, "uuid-mdp", 2)).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        (cache, timeline)
    }

    /// §16 Bước 7's gate: nothing is asked about twice in a year, and nothing
    /// whose ending is written is asked about at all.
    #[test]
    fn nothing_is_asked_about_twice_in_a_year() {
        let (cache, timeline) = a_vault();
        let db = cache.lock().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_str().unwrap();
        let today = day("2026-09-19");

        let open =
            open_questions(&timeline, &db, today, &Quiet::default()).unwrap();
        let named: Vec<&str> = open.iter().map(|q| q.name.as_str()).collect();
        assert_eq!(named, ["Synabit 1.0"], "the one whose ending nobody wrote: {open:?}");
        assert_eq!(open[0].times, 14);

        let first = ask(&timeline, &db, vault_path, today, &Quiet::default())
            .unwrap()
            .expect("asked once");
        assert_eq!(first.name, "Synabit 1.0");

        // Same day, next week, eleven months on: still not again.
        for later in ["2026-09-19", "2026-09-26", "2027-08-01"] {
            let again = ask(
                &timeline,
                &db,
                vault_path,
                day(later),
                &Quiet::default(),
            )
            .unwrap();
            assert!(again.is_none(), "asked again on {later}: {again:?}");
        }

        // A year and a day later it may be asked once more.
        let after = ask(
            &timeline,
            &db,
            vault_path,
            day("2027-09-20"),
            &Quiet::default(),
        )
        .unwrap();
        assert_eq!(after.map(|q| q.name), Some("Synabit 1.0".into()));
    }

    #[test]
    fn a_hushed_thing_is_never_asked_about() {
        let (cache, timeline) = a_vault();
        let db = cache.lock().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_str().unwrap();
        let today = day("2026-09-19");

        hush::write_hush(vault_path, &hush::Subject::Person { who: "uuid-syn".into() }, None)
            .unwrap();
        let quiet = Quiet::read(&db, vault_path, "2026-09-19").unwrap();

        assert!(open_questions(&timeline, &db, today, &quiet).unwrap().is_empty());
        assert!(ask(&timeline, &db, vault_path, today, &quiet).unwrap().is_none());
        // And nothing was written down about having asked, because nothing was.
        assert!(already_asked(vault_path).is_empty());
    }

    /// What would be asked about on the real vault, read only — `ask` is not
    /// called, so nothing is written and nothing counts as asked.
    ///
    ///   SYN_PROBE_CACHE=... cargo test --lib -- --ignored what_the_real_vault_would_ask --nocapture
    #[test]
    #[ignore = "needs a copy of a real vault cache; run by hand"]
    fn what_the_real_vault_would_ask() {
        let path = std::env::var("SYN_PROBE_CACHE").expect("a copy of a vault_cache.db");
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = crate::db::DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        let built = catch_up(&cache, &mut timeline).expect("built from the vault");
        let db = cache.lock().unwrap();
        let today = chrono::Local::now().date_naive();

        let things = timeline.days_about_things(today).unwrap();
        let mut per_thing: HashMap<&str, usize> = HashMap::new();
        for (about, _) in &things {
            *per_thing.entry(about.as_str()).or_default() += 1;
        }
        let open =
            open_questions(&timeline, &db, today, &Quiet::default()).unwrap();

        eprintln!("\n═══ what happened to… ═══  {} events", built.items);
        eprintln!("  things written about:        {}", per_thing.len());
        eprintln!("  written about {ENOUGH_MENTIONS}+ days:      {}",
            per_thing.values().filter(|n| **n >= ENOUGH_MENTIONS).count());
        eprintln!("  would be asked about:        {}", open.len());
        // Every thing appearing exactly once means the links are missing and
        // what is being counted is each event's own node — see Bước 2.
        eprintln!(
            "  most days any one thing has: {}",
            per_thing.values().max().copied().unwrap_or(0)
        );
        for question in open.iter().take(10) {
            eprintln!(
                "  «{}» ({}) — {} lần, {} → {}, im {} ngày",
                question.name, question.node_type, question.times,
                question.first, question.last, question.quiet_for
            );
        }
    }
}
