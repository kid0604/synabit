//! "Khoảng lặng" — someone who was in a life and then was not.
//!
//! The design is §7.2 of `docs/timeline-2026-09-17.md`, which calls this the
//! most dangerous feature in the document and is right to.
//!
//! # What it is allowed to say
//!
//! One sentence, made of numbers it counted:
//!
//! > *"Khánh xuất hiện 20 lần trong 2019. Lần cuối: 14/3/2021."*
//!
//! And then nothing. It may not suggest getting in touch, and it may not
//! reach for a reason. People stop seeing each other because of a row, a move,
//! a divorce, a death. The app cannot tell which, and a guess costs far more
//! when it is wrong than it gains when it is right. So this module counts, and
//! the sentence it hands over is arithmetic with a name attached.
//!
//! There is no model here for the same reason there is none in
//! [`super::onthisday`]: nothing in this needs writing, and anything that wrote
//! would be writing about somebody's life.
//!
//! # "Longer than their usual rhythm" cannot be read literally
//!
//! §7.2 asks for silence "longer than the pattern's own everyday gap". Taken as
//! written — longer than the median gap — the rule fires on half of all
//! ordinary lulls, because half of any set of gaps is above its median by
//! definition. A rule that is wrong half the time is not a threshold, and on
//! this feature being wrong means asking somebody about a person who died.
//!
//! So the measure is the **longest** gap that friendship has ever had. The app
//! speaks only when this quiet is unlike anything in the record, which is what
//! "something changed" actually means, and is still read from the pair's own
//! history rather than from a number picked here.
//!
//! # Who is never mentioned
//!
//! Everything goes through [`super::quiet::allow`], so the dead are out by
//! default (§7.3), the sealed are out, and someone waved away once stays away
//! for months — that last one is the whole of §7.2's final "must never", and it
//! is an ordinary hush with an expiry rather than anything new.

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::Serialize;

use super::quiet::{self, Nudge, Quiet};
use super::seal::Seals;
use super::store::TimelineStore;
use super::when;
use crate::error::AppResult;

/// How many times somebody has to have been there before there is a pattern to
/// miss. Fewer than this is a coincidence, not a rhythm.
const ENOUGH_TIMES: usize = 5;

/// And over how long. Five evenings in one week is a holiday, not a friendship.
const ENOUGH_MONTHS: i64 = 183;

/// A quiet shorter than this is not worth anybody's attention however tight
/// the rhythm was, and without it a pair who met daily would be reported after
/// a fortnight away.
const SHORTEST_WORTH_SAYING: i64 = 90;

/// Someone who was there often, and then stopped being there.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Missing {
    /// The person, by whichever name the timeline links them under.
    pub who: String,
    /// How many separate days they were there.
    pub times: usize,
    /// The first and last of those days.
    pub first: String,
    pub last: String,
    /// Days since the last one.
    pub quiet_for: i64,
    /// The longest they had ever been apart before this.
    pub longest_before: i64,
}

/// Everyone whose absence is unlike anything in their own record.
///
/// Returns nothing when there is no pattern to speak of, which on most vaults
/// is most of the time. That is the correct output, not a failure.
pub fn who_went_quiet(
    store: &TimelineStore,
    today: NaiveDate,
    quiet: &Quiet,
    seals: &Seals,
) -> AppResult<Vec<Missing>> {
    let mut days: HashMap<String, Vec<String>> = HashMap::new();
    for (who, day) in store.days_with_people(today)? {
        days.entry(who).or_default().push(day);
    }

    let mut missing: Vec<Missing> = days
        .into_iter()
        .filter_map(|(who, days)| measure(who, &days, today))
        .filter(|found| {
            // The day being judged is the last day they were there: if that
            // time is sealed, this quiet is about something the person asked
            // not to be brought back.
            let nudge = Nudge::on(found.last.clone()).naming([found.who.clone()]);
            !quiet::allow(vec![nudge], quiet, seals).is_empty()
        })
        .collect();

    // Longest quiet first: the person furthest away is the one being asked about.
    missing.sort_by(|a, b| b.quiet_for.cmp(&a.quiet_for).then_with(|| a.who.cmp(&b.who)));
    Ok(missing)
}

/// One person's rhythm, and whether this quiet is outside it.
fn measure(who: String, days: &[String], today: NaiveDate) -> Option<Missing> {
    if days.len() < ENOUGH_TIMES {
        return None;
    }
    let dates: Vec<NaiveDate> = days.iter().filter_map(|d| when::parse(d)).map(|s| s.from).collect();
    if dates.len() < ENOUGH_TIMES {
        return None;
    }
    let (first, last) = (*dates.first()?, *dates.last()?);
    if (last - first).num_days() < ENOUGH_MONTHS {
        return None;
    }

    let longest_before = dates
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_days())
        .max()
        .unwrap_or(0);
    let quiet_for = (today - last).num_days();
    if quiet_for <= longest_before.max(SHORTEST_WORTH_SAYING) {
        return None;
    }

    Some(Missing {
        who,
        times: dates.len(),
        first: when::iso(first),
        last: when::iso(last),
        quiet_for,
        longest_before,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    /// Days at a steady interval, starting from `from`.
    fn every(days: i64, times: usize, from: &str) -> Vec<String> {
        let start = day(from);
        (0..times).map(|n| when::iso(start + chrono::Duration::days(days * n as i64))).collect()
    }

    #[test]
    fn somebody_seen_often_and_then_not_at_all_is_noticed() {
        // Once a month for a year, then gone.
        let seen = every(30, 12, "2024-01-05");
        let found = measure("uuid-khanh".into(), &seen, day("2026-05-14")).expect("noticed");
        assert_eq!(found.times, 12);
        assert_eq!(found.first, "2024-01-05");
        assert_eq!(found.last, "2024-11-30");
        assert_eq!(found.longest_before, 30);
        assert!(found.quiet_for > 500, "{found:?}");
    }

    #[test]
    fn somebody_seen_once_a_year_is_not_missing_at_ten_months() {
        let seen = every(365, 6, "2019-05-14");
        // Ten months after the last: long, but nothing new for this pair.
        assert_eq!(measure("uuid-bac".into(), &seen, day("2025-03-14")), None);
        // Two and a half years is outside anything they have had.
        assert!(measure("uuid-bac".into(), &seen, day("2026-11-14")).is_some());
    }

    #[test]
    fn a_handful_of_times_is_not_a_rhythm() {
        let seen = every(60, 4, "2024-01-01");
        assert_eq!(measure("uuid-x".into(), &seen, day("2026-05-14")), None);
    }

    #[test]
    fn a_week_of_seeing_each_other_daily_is_a_holiday_not_a_friendship() {
        let seen = every(1, 7, "2024-07-01");
        assert_eq!(measure("uuid-y".into(), &seen, day("2026-05-14")), None, "under six months");
    }

    #[test]
    fn people_who_met_daily_are_not_reported_after_a_fortnight() {
        let seen = every(1, 200, "2024-01-01");
        let a_fortnight = day("2024-07-31");
        assert_eq!(measure("uuid-z".into(), &seen, a_fortnight), None);
        assert!(
            measure("uuid-z".into(), &seen, day("2024-11-01")).is_some(),
            "three months of nothing, after two hundred days running, is worth a line"
        );
    }

    #[test]
    fn the_measure_is_the_longest_gap_they_ever_had_not_the_middle_one() {
        // Mostly weekly, but they once went five months without meeting.
        let mut seen = every(7, 20, "2024-01-01");
        seen.push("2024-10-01".into());
        let months_later = day("2025-01-15");
        // Half of any set of gaps is above its median, so a median rule would
        // have spoken here. The longest gap is 155 days and this is 106.
        assert_eq!(measure("uuid-w".into(), &seen, months_later), None);
    }

    // ─── The gate, on the road the real feature takes ────────────────

    use std::sync::Mutex;

    use serde_json::json;

    use crate::models::node::NodeMetadata;
    use crate::timeline::store::catch_up;

    fn node(id: &str, node_type: &str, title: &str, properties: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            content: String::new(),
            properties,
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    /// A note of one evening with somebody.
    fn evening(day: &str, who: &str) -> NodeMetadata {
        node(
            &format!("Notes/{day}-{who}.md"),
            "note",
            day,
            json!({
                "date": day,
                "moments": [{ "title": "Gặp nhau", "happened": day, "people": [who] }]
            }),
        )
    }

    /// §16 Bước 5's gate: somebody met regularly and then not at all is said;
    /// somebody met once a year is not; somebody who died is not.
    #[test]
    fn only_a_quiet_unlike_their_own_record_is_spoken_of() {
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        for person in [
            node("People/khanh.md", "person", "Khánh", json!({ "node_id": "uuid-khanh" })),
            node("People/bac.md", "person", "Bác Sơn", json!({ "node_id": "uuid-bac" })),
            node(
                "People/ba.md",
                "person",
                "Bà",
                json!({ "node_id": "uuid-ba", "died_on": "2024-12-01" }),
            ),
        ] {
            db.upsert_node(&person).unwrap();
        }

        // Khánh: once a month through 2024, then nothing.
        // Bà: the same rhythm, and then she died.
        for month in 1..=11 {
            let day = format!("2024-{month:02}-05");
            db.upsert_node(&evening(&day, "uuid-khanh")).unwrap();
            db.upsert_node(&evening(&day, "uuid-ba")).unwrap();
        }
        // Bác Sơn: Tết, once a year, for six years.
        for year in 2019..=2024 {
            db.upsert_node(&evening(&format!("{year}-02-10"), "uuid-bac")).unwrap();
        }

        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        let db = cache.lock().unwrap();

        // Chosen so the three cases separate: Khánh has been quiet 95 days
        // against a rhythm of 30, while Bác Sơn is at 364 against a rhythm of
        // a year — long, but nothing new for that pair.
        let today = day("2025-02-08");
        let quiet = Quiet::read(&db, "/nowhere", "2025-02-08").unwrap();
        let found = who_went_quiet(&timeline, today, &quiet, &Seals::default()).unwrap();

        let named: Vec<&str> = found.iter().map(|m| m.who.as_str()).collect();
        assert_eq!(named, ["uuid-khanh"], "{found:?}");
        assert_eq!(found[0].times, 11);
        assert_eq!(found[0].first, "2024-01-05");
        assert_eq!(found[0].last, "2024-11-05");
        assert!(found[0].quiet_for > found[0].longest_before);
    }

    #[test]
    fn waving_somebody_away_keeps_them_away() {
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node(
            "People/khanh.md",
            "person",
            "Khánh",
            json!({ "node_id": "uuid-khanh" }),
        ))
        .unwrap();
        for month in 1..=11 {
            db.upsert_node(&evening(&format!("2024-{month:02}-05"), "uuid-khanh")).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        let db = cache.lock().unwrap();
        let today = day("2025-06-01");

        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_str().unwrap();
        let open = Quiet::read(&db, vault_path, "2025-06-01").unwrap();
        assert_eq!(who_went_quiet(&timeline, today, &open, &Seals::default()).unwrap().len(), 1);

        // §7.2's last "must never": waved away once, gone for months.
        quiet::write_hush(
            vault_path,
            &quiet::Subject::Person { who: "uuid-khanh".into() },
            Some("2025-12-01"),
        )
        .unwrap();
        let after = Quiet::read(&db, vault_path, "2025-06-01").unwrap();
        assert!(who_went_quiet(&timeline, today, &after, &Seals::default()).unwrap().is_empty());

        // And back once it has run out, because this is a pause, not a seal.
        let later = Quiet::read(&db, vault_path, "2025-12-02").unwrap();
        assert_eq!(who_went_quiet(&timeline, today, &later, &Seals::default()).unwrap().len(), 1);
    }

    /// What this would say on the real vault today, read only.
    ///
    ///   SYN_PROBE_CACHE=/path/to/a/copy/of/vault_cache.db \\
    ///     cargo test --lib -- --ignored who_is_quiet_in_the_real_vault --nocapture
    #[test]
    #[ignore = "needs a copy of a real vault cache; run by hand"]
    fn who_is_quiet_in_the_real_vault() {
        let path = std::env::var("SYN_PROBE_CACHE").expect("a copy of a vault_cache.db");
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = crate::db::DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        let built = catch_up(&cache, &mut timeline).expect("built from the vault");
        let db = cache.lock().unwrap();
        let today = chrono::Local::now().date_naive();

        let appearances = timeline.days_with_people(today).unwrap();
        let mut per_person: HashMap<&str, usize> = HashMap::new();
        for (who, _) in &appearances {
            *per_person.entry(who.as_str()).or_default() += 1;
        }
        let found =
            who_went_quiet(&timeline, today, &Quiet::default(), &Seals::default()).unwrap();

        eprintln!("\n═══ who went quiet ═══  {} events", built.items);
        eprintln!("  events naming somebody: {}", appearances.len());
        eprintln!("  people with any at all: {}", per_person.len());
        eprintln!(
            "  people seen {ENOUGH_TIMES}+ times: {}",
            per_person.values().filter(|n| **n >= ENOUGH_TIMES).count()
        );
        eprintln!("  would be spoken of:     {}", found.len());
        for missing in found.iter().take(10) {
            eprintln!(
                "  {} — {} times, {} → {}, quiet {}d against a longest of {}d",
                missing.who,
                missing.times,
                missing.first,
                missing.last,
                missing.quiet_for,
                missing.longest_before
            );
        }
    }
}
