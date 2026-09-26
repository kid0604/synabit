//! Work Syn does at a time the person chose, without being asked each time.
//!
//! # What this is and is not
//!
//! A routine is a question the person has asked Syn to ask itself on a
//! schedule — "every weekday at 7:30, tell me what today holds". It is written
//! by the person, in their words, on a screen that shows exactly when it will
//! run. **Syn never creates, changes or enables a routine.** An assistant that
//! schedules its own work has started deciding what to spend the person's
//! attention, money and data on, and that is not a line this app crosses by
//! degrees.
//!
//! `syn::notice` is the other kind of initiative, and the first: noticing a
//! fact arithmetic can see, with no model and no permissions. A routine is the
//! second kind — a real run, with a model and tools — and so it comes with the
//! contract that kind needs:
//!
//! * **It reads, and makes new things.** A routine's run is on its own surface
//!   (`Surface::Routine`), offered reading, looking things up and creating new
//!   notes, and nothing that changes or removes existing work. Nobody is
//!   watching it happen, so nothing it does may need watching.
//! * **It grants itself nothing.** What needs permission is decided by the
//!   ledger the person keeps. A routine that reaches for something they have
//!   not allowed stops and waits for them, listed under "waiting for you".
//! * **It reports.** Every run lands in the routine's own conversation, is
//!   listed in Syn's work, and — when the person asked — is sent to the phone.
//!
//! # Where it runs
//!
//! On the computer, from the loop that already wakes every minute for
//! reminders (`chat_engine`). A phone does not keep that loop running in the
//! background, and a model run is not something to hand to the operating
//! system's scheduler the way a reminder is; so a routine that came due while
//! the app was closed runs the next time it is open, if that is still within
//! `CATCH_UP_HOURS`, and is skipped — not run late and stale — after that.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDateTime, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// How late a routine may still run after its time. Twelve hours: a morning
/// brief read at lunch is still worth something; one read the next morning is
/// yesterday's.
pub const CATCH_UP_HOURS: i64 = 12;

/// At most this many routines, so the list stays something a person reads.
pub const MOST_ROUTINES: usize = 20;

/// When a routine runs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// Local time of day, "HH:MM".
    pub at: String,
    /// Days of the week it runs, Monday = 1 … Sunday = 7. Empty means every day.
    #[serde(default)]
    pub weekdays: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Routine {
    pub id: String,
    /// What the person calls it. Also the conversation's title.
    pub name: String,
    /// What Syn is asked, in the person's words.
    pub ask: String,
    pub schedule: Schedule,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Also send the result to the paired Telegram chat.
    #[serde(default)]
    pub to_phone: bool,
    /// The conversation its runs are written into, made on the first run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}

fn yes() -> bool {
    true
}

/// What is kept about routines: the routines, and which slot each last ran.
///
/// The slot is kept beside them, in the vault, rather than on this device: a
/// vault open on two computers would otherwise run every routine twice. Sync is
/// not instant, so two machines waking in the same minute can still both run
/// one — rare, visible in the conversation, and preferable to a lock that
/// could leave a routine never running at all.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Book {
    #[serde(default)]
    pub routines: Vec<Routine>,
    /// Routine id → the slot it last ran for, as `YYYY-MM-DDTHH:MM`.
    #[serde(default)]
    pub last_slot: BTreeMap<String, String>,
}

fn path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join("Syn").join("routines.json")
}

/// Read what is kept. An unreadable file is an empty one — no routine runs
/// rather than a wrong one.
pub fn load(vault_path: &str) -> Book {
    std::fs::read_to_string(path(vault_path))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(vault_path: &str, book: &Book) -> AppResult<()> {
    let file = path(vault_path);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| AppError::General(format!("Failed to create the Syn folder: {e}")))?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(book)?)?;
    std::fs::rename(&tmp, &file).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::General(format!("Failed to save routines: {e}"))
    })
}

/// Check a routine as the person wrote it, and say what is wrong in words.
pub fn check(routine: &Routine) -> Result<(), String> {
    if routine.name.trim().is_empty() {
        return Err("A routine needs a name.".into());
    }
    if routine.ask.trim().is_empty() {
        return Err("A routine needs something to ask.".into());
    }
    if time_of(&routine.schedule.at).is_none() {
        return Err(format!("`{}` is not a time of day; use HH:MM.", routine.schedule.at));
    }
    if routine.schedule.weekdays.iter().any(|d| !(1..=7).contains(d)) {
        return Err("Weekdays are 1 (Monday) to 7 (Sunday).".into());
    }
    Ok(())
}

fn time_of(at: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(at.trim(), "%H:%M").ok()
}

fn slot_key(slot: NaiveDateTime) -> String {
    slot.format("%Y-%m-%dT%H:%M").to_string()
}

/// The most recent time this routine was meant to run, at or before `now`.
fn latest_slot(schedule: &Schedule, now: NaiveDateTime) -> Option<NaiveDateTime> {
    let at = time_of(&schedule.at)?;
    // A week back is always enough to find one, when any day is allowed.
    (0..8).find_map(|back| {
        let day = now.date() - chrono::Duration::days(back);
        let when = day.and_time(at);
        let weekday = day.weekday().number_from_monday() as u8;
        let runs_that_day = schedule.weekdays.is_empty() || schedule.weekdays.contains(&weekday);
        (runs_that_day && when <= now).then_some(when)
    })
}

/// The slot this routine should run for now, if it should.
///
/// Due when its latest slot has passed, is no more than `CATCH_UP_HOURS` old,
/// and is not the slot it last ran for. Pure: the clock and the record come in
/// as arguments, which is what lets the "not twice" and "not stale" rules be
/// tested at all.
pub fn due(routine: &Routine, last: Option<&str>, now: NaiveDateTime) -> Option<String> {
    if !routine.enabled {
        return None;
    }
    let slot = latest_slot(&routine.schedule, now)?;
    if now - slot > chrono::Duration::hours(CATCH_UP_HOURS) {
        return None;
    }
    let key = slot_key(slot);
    (last != Some(key.as_str())).then_some(key)
}

/// Routines due now, with the slot each is due for.
pub fn all_due(book: &Book, now: NaiveDateTime) -> Vec<(Routine, String)> {
    book.routines
        .iter()
        .filter_map(|r| due(r, book.last_slot.get(&r.id).map(String::as_str), now).map(|slot| (r.clone(), slot)))
        .collect()
}

/// When it will next run, for the screen: "YYYY-MM-DDTHH:MM", or `None` when
/// it is off.
pub fn next_run(routine: &Routine, now: NaiveDateTime) -> Option<String> {
    if !routine.enabled {
        return None;
    }
    let at = time_of(&routine.schedule.at)?;
    (0..8).find_map(|ahead| {
        let day = now.date() + chrono::Duration::days(ahead);
        let when = day.and_time(at);
        let weekday = day.weekday().number_from_monday() as u8;
        let runs_that_day = routine.schedule.weekdays.is_empty() || routine.schedule.weekdays.contains(&weekday);
        (runs_that_day && when > now).then(|| slot_key(when))
    })
}

/// What the run is told, on top of the routine's own words.
///
/// The routine's text is the question; this says when and why it is being
/// asked, so "today" means the day it ran and not the day it was written.
pub fn question(routine: &Routine, now: NaiveDateTime) -> String {
    format!(
        "{}\n\n[Routine \"{}\", run at {:02}:{:02} on {}. Nobody is waiting on this as it runs; \
         write the result to be read later.]",
        routine.ask.trim(),
        routine.name.trim(),
        now.hour(),
        now.minute(),
        now.format("%A %Y-%m-%d"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn brief(weekdays: Vec<u8>) -> Routine {
        Routine {
            id: "r1".into(),
            name: "Sáng nay".into(),
            ask: "Hôm nay có gì?".into(),
            schedule: Schedule { at: "07:30".into(), weekdays },
            enabled: true,
            to_phone: false,
            conversation_id: None,
        }
    }

    #[test]
    fn it_runs_once_its_time_has_come_and_not_before() {
        let r = brief(vec![]);
        // A minute early, the latest slot is yesterday's — too old to run.
        assert_eq!(due(&r, None, at("2026-09-28 07:29")), None);
        assert_eq!(due(&r, None, at("2026-09-28 07:30")), Some("2026-09-28T07:30".into()));
        assert_eq!(due(&r, Some("2026-09-27T07:30"), at("2026-09-28 07:30")), Some("2026-09-28T07:30".into()));
    }

    /// Not twice for the same slot, however many ticks see it.
    #[test]
    fn a_slot_that_ran_does_not_run_again() {
        let r = brief(vec![]);
        assert_eq!(due(&r, Some("2026-09-28T07:30"), at("2026-09-28 07:31")), None);
        assert_eq!(due(&r, Some("2026-09-28T07:30"), at("2026-09-28 18:00")), None);
    }

    /// A morning brief read at lunch is still worth something; one read the
    /// next morning is yesterday's.
    #[test]
    fn a_slot_missed_for_too_long_is_skipped_not_run_stale() {
        let r = brief(vec![]);
        assert_eq!(due(&r, Some("2026-09-27T07:30"), at("2026-09-28 19:00")), Some("2026-09-28T07:30".into()));
        assert_eq!(due(&r, Some("2026-09-27T07:30"), at("2026-09-28 20:00")), None);
    }

    #[test]
    fn weekdays_are_honoured() {
        // 2026-09-26 is a Saturday.
        let weekdays = brief(vec![1, 2, 3, 4, 5]);
        assert_eq!(due(&weekdays, None, at("2026-09-26 08:00")), None, "no run on a Saturday, and Friday's is a day old");
        assert_eq!(next_run(&weekdays, at("2026-09-26 08:00")), Some("2026-09-28T07:30".into()), "Monday");
    }

    #[test]
    fn an_off_routine_never_runs() {
        let mut r = brief(vec![]);
        r.enabled = false;
        assert_eq!(due(&r, None, at("2026-09-28 07:30")), None);
        assert_eq!(next_run(&r, at("2026-09-28 07:00")), None);
    }

    #[test]
    fn a_routine_as_written_is_checked_in_words() {
        let mut r = brief(vec![]);
        assert!(check(&r).is_ok());
        r.schedule.at = "7h30".into();
        assert!(check(&r).unwrap_err().contains("HH:MM"));
        r.schedule.at = "07:30".into();
        r.schedule.weekdays = vec![0];
        assert!(check(&r).is_err());
    }

    #[test]
    fn the_run_is_told_when_it_is() {
        let q = question(&brief(vec![]), at("2026-09-28 07:30"));
        assert!(q.starts_with("Hôm nay có gì?"));
        assert!(q.contains("2026-09-28") && q.contains("07:30"));
    }

    #[test]
    fn what_is_kept_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        assert!(load(vault).routines.is_empty(), "nothing yet is an empty book");
        let mut book = Book::default();
        book.routines.push(brief(vec![1, 3]));
        book.last_slot.insert("r1".into(), "2026-09-28T07:30".into());
        save(vault, &book).unwrap();
        let back = load(vault);
        assert_eq!(back.routines, book.routines);
        assert_eq!(back.last_slot.get("r1").map(String::as_str), Some("2026-09-28T07:30"));
    }
}
