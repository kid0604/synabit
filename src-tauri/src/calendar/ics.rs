//! Reading and writing iCalendar, so a vault is not a place events go to die.
//!
//! # What is deliberately not here
//!
//! **`VTIMEZONE`.** A zoned event is written as `DTSTART;TZID=Asia/Tokyo:...`
//! and no accompanying timezone component. Strictly the RFC wants one; in
//! practice Google, Apple and Outlook all resolve a well-known IANA name on
//! their own, and generating the alternative means synthesising daylight
//! saving transition rules for every zone in the file — a large amount of
//! code whose only job is to restate what both ends already know. A floating
//! event is written floating, which needs nothing and is exactly correct.
//!
//! **Everything except events.** No `VTODO`, no `VJOURNAL`, no `VFREEBUSY`.
//! Tasks in this app are not calendar entries and pretending otherwise would
//! export a shape nothing here can read back.
//!
//! # The one-day trap
//!
//! `DTEND` is exclusive: an all-day event on the 10th ends on the 11th. This
//! app stores the last day it covers. Everything crossing this boundary goes
//! through `super::ical`, and nothing here may take a stored `end_at` and put
//! it in a `DTEND` directly. See that module for why the vault was not simply
//! changed to match.

use crate::utils::contentline::{fold, param, prop, split_line, unescape, unfold};

use super::ical::{dtend_from_stored_all_day, stored_all_day_from_dtend};
use super::recurrence::EventSummary;
use super::rrule::RRule;

/// `2026-03-10` → `20260310`, `2026-03-10T09:00` → `20260310T090000`.
fn compact(stamp: &str) -> String {
    let stamp = stamp.trim();
    match stamp.split_once('T') {
        None => stamp.replace('-', ""),
        Some((date, time)) => {
            let mut hms: Vec<&str> = time.split(':').collect();
            while hms.len() < 3 {
                hms.push("00");
            }
            format!(
                "{}T{}{}{}",
                date.replace('-', ""),
                hms[0],
                hms[1],
                &hms[2][..2.min(hms[2].len())]
            )
        }
    }
}

fn write_stamp(name: &str, stamp: &str, all_day: bool, tzid: &str, out: &mut String) {
    if stamp.trim().is_empty() {
        return;
    }
    if all_day {
        fold(&format!("{};VALUE=DATE:{}", name, compact(stamp)), out);
    } else if tzid.trim().is_empty() {
        // Floating: the same wall clock wherever it is read, which is what
        // this app stores when no zone was chosen.
        fold(&format!("{}:{}", name, compact(stamp)), out);
    } else {
        fold(&format!("{};TZID={}:{}", name, tzid.trim(), compact(stamp)), out);
    }
}

/// One `VCALENDAR` holding every event given.
pub fn export(events: &[EventSummary], now_utc: &str) -> String {
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\n");
    out.push_str("VERSION:2.0\r\n");
    out.push_str("PRODID:-//Synabit//Calendar//EN\r\n");
    out.push_str("CALSCALE:GREGORIAN\r\n");

    for ev in events {
        if ev.start_at.trim().is_empty() {
            continue;
        }
        out.push_str("BEGIN:VEVENT\r\n");
        prop("UID", if ev.uid.is_empty() { &ev.id } else { &ev.uid }, &mut out);
        fold(&format!("DTSTAMP:{}", now_utc), &mut out);

        write_stamp("DTSTART", &ev.start_at, ev.is_all_day, &ev.tzid, &mut out);

        // The one-day trap. An all-day event's stored end is the last day it
        // covers; `DTEND` names the first day it does not.
        if ev.is_all_day {
            let last = if ev.end_at.trim().is_empty() { &ev.start_at } else { &ev.end_at };
            if let Some(dtend) = dtend_from_stored_all_day(last.split('T').next().unwrap_or(last)) {
                write_stamp("DTEND", &dtend, true, "", &mut out);
            }
        } else if !ev.end_at.trim().is_empty() {
            write_stamp("DTEND", &ev.end_at, false, &ev.tzid, &mut out);
        }

        prop("SUMMARY", &ev.title, &mut out);
        prop("LOCATION", &ev.location, &mut out);
        if !ev.tags.is_empty() {
            prop("CATEGORIES", &ev.tags.join(","), &mut out);
        }
        if let Some(rule) = ev.rule() {
            fold(&format!("RRULE:{}", rule.to_rrule_string()), &mut out);
        }
        if !ev.exceptions.is_empty() {
            let dates: Vec<String> = ev.exceptions.iter().map(|d| compact(d)).collect();
            let value = if ev.is_all_day {
                format!("EXDATE;VALUE=DATE:{}", dates.join(","))
            } else {
                // An exception names the occurrence's start, so it carries the
                // same time of day the series does.
                let time = ev.start_at.split_once('T').map(|(_, t)| t).unwrap_or("00:00");
                let stamps: Vec<String> = ev
                    .exceptions
                    .iter()
                    .map(|d| compact(&format!("{}T{}", d, time)))
                    .collect();
                if ev.tzid.trim().is_empty() {
                    format!("EXDATE:{}", stamps.join(","))
                } else {
                    format!("EXDATE;TZID={}:{}", ev.tzid.trim(), stamps.join(","))
                }
            };
            fold(&value, &mut out);
        }
        out.push_str("END:VEVENT\r\n");
    }

    out.push_str("END:VCALENDAR\r\n");
    out
}

// ─── Reading ────────────────────────────────────────────────

/// An event read out of a file, in the shapes this app stores.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportedEvent {
    pub uid: String,
    pub title: String,
    pub is_all_day: bool,
    pub start_at: String,
    pub end_at: String,
    pub tzid: String,
    pub rrule: String,
    pub exceptions: Vec<String>,
    pub location: String,
    pub description: String,
    pub tags: Vec<String>,
    /// The file said this repeats, in a way this app cannot reproduce
    /// faithfully. The event is here; its repeat is not. Surfaced so the
    /// import can say how many, rather than leaving it to be discovered.
    pub rrule_dropped: bool,
}

/// `20260310` → `2026-03-10`, `20260310T090000` → `2026-03-10T09:00`.
///
/// A trailing `Z` is kept as a marker for the caller; nothing else here
/// interprets it.
fn expand(value: &str) -> Option<(String, bool)> {
    let raw = value.trim();
    let utc = raw.ends_with('Z');
    let raw = raw.trim_end_matches('Z');

    let (date, time) = match raw.split_once('T') {
        None => (raw, None),
        Some((d, t)) => (d, Some(t)),
    };
    let date = date.replace('-', "");
    if date.len() != 8 || !date.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let iso_date = format!("{}-{}-{}", &date[..4], &date[4..6], &date[6..8]);

    match time {
        None => Some((iso_date, utc)),
        Some(t) => {
            let t = t.replace(':', "");
            if t.len() < 4 || !t[..4].chars().all(|c| c.is_ascii_digit()) {
                return Some((iso_date, utc));
            }
            Some((format!("{}T{}:{}", iso_date, &t[..2], &t[2..4]), utc))
        }
    }
}

/// Every `VEVENT` in `text`, in the shapes this app stores.
///
/// Deliberately forgiving. A file from another tool carries components and
/// properties this app has never heard of, and refusing the whole calendar
/// because one of them is unfamiliar would make the feature useless for
/// exactly the files it exists to read.
pub fn import(text: &str) -> Vec<ImportedEvent> {
    let mut out = Vec::new();
    let mut current: Option<ImportedEvent> = None;
    // `VALARM` and `VTIMEZONE` sit inside or beside a `VEVENT` and carry
    // properties with the same names. Reading those as the event's own is how
    // an alarm's trigger becomes the meeting's start time.
    let mut nested = 0usize;
    let mut duration: Option<String> = None;
    let mut saw_dtend = false;

    for line in unfold(text) {
        let Some((name, params, value)) = split_line(&line) else { continue };

        match (name.as_str(), value.trim().to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") => {
                current = Some(ImportedEvent::default());
                nested = 0;
                duration = None;
                saw_dtend = false;
                continue;
            }
            ("END", "VEVENT") => {
                if let Some(mut ev) = current.take() {
                    finish(&mut ev, duration.take(), saw_dtend);
                    if !ev.start_at.is_empty() {
                        out.push(ev);
                    }
                }
                continue;
            }
            ("BEGIN", _) if current.is_some() => {
                nested += 1;
                continue;
            }
            ("END", _) if current.is_some() => {
                nested = nested.saturating_sub(1);
                continue;
            }
            _ => {}
        }

        let Some(ev) = current.as_mut() else { continue };
        if nested > 0 {
            continue;
        }

        let is_date = param(&params, "VALUE").map(|v| v.eq_ignore_ascii_case("DATE")) == Some(true);

        match name.as_str() {
            "UID" => ev.uid = unescape(&value).trim().to_string(),
            "SUMMARY" => ev.title = unescape(&value),
            "LOCATION" => ev.location = unescape(&value),
            "DESCRIPTION" => ev.description = unescape(&value),
            "CATEGORIES" => {
                ev.tags = unescape(&value)
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
            }
            "RRULE" => {
                // Strictly, and on purpose. A rule carrying a part this app
                // cannot reproduce lands on different days than the file
                // meant, and storing it anyway would be a silent rewrite of
                // someone's calendar. See `RRule::parse_foreign`.
                match RRule::parse_foreign(&value) {
                    Some(rule) => ev.rrule = rule.to_rrule_string(),
                    None => ev.rrule_dropped = !value.trim().is_empty(),
                }
            }
            "DTSTART" => {
                if let Some((stamp, _)) = expand(&value) {
                    ev.is_all_day = is_date || !stamp.contains('T');
                    ev.start_at = stamp;
                    if let Some(tz) = param(&params, "TZID") {
                        ev.tzid = tz.to_string();
                    }
                }
            }
            "DTEND" => {
                saw_dtend = true;
                if let Some((stamp, _)) = expand(&value) {
                    ev.end_at = stamp;
                }
            }
            "DURATION" => duration = Some(value.trim().to_string()),
            "EXDATE" => {
                for part in value.split(',') {
                    if let Some((stamp, _)) = expand(part) {
                        let day = stamp.split('T').next().unwrap_or(&stamp).to_string();
                        if !ev.exceptions.contains(&day) {
                            ev.exceptions.push(day);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    out
}

/// `PT1H30M` / `P2D` → minutes.
fn duration_minutes(value: &str) -> Option<i64> {
    let raw = value.trim().to_ascii_uppercase();
    let raw = raw.strip_prefix('P')?;
    let (days_part, time_part) = match raw.split_once('T') {
        None => (raw, ""),
        Some((d, t)) => (d, t),
    };

    let mut minutes: i64 = 0;
    let mut number = String::new();
    for c in days_part.chars() {
        if c.is_ascii_digit() {
            number.push(c);
        } else {
            let n: i64 = number.parse().ok()?;
            number.clear();
            match c {
                'D' => minutes += n * 24 * 60,
                'W' => minutes += n * 7 * 24 * 60,
                _ => return None,
            }
        }
    }
    for c in time_part.chars() {
        if c.is_ascii_digit() {
            number.push(c);
        } else {
            let n: i64 = number.parse().ok()?;
            number.clear();
            match c {
                'H' => minutes += n * 60,
                'M' => minutes += n,
                'S' => {}
                _ => return None,
            }
        }
    }
    Some(minutes)
}

/// Turn what was read into what this app stores.
fn finish(ev: &mut ImportedEvent, duration: Option<String>, saw_dtend: bool) {
    if ev.is_all_day {
        // The one-day trap, in the other direction.
        let dtend = if saw_dtend && !ev.end_at.is_empty() {
            ev.end_at.clone()
        } else if let Some(d) = duration.as_deref().and_then(duration_minutes) {
            super::tz::parse_stamp(&format!("{}T00:00:00", ev.start_at))
                .map(|s| (s + chrono::Duration::minutes(d)).format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| ev.start_at.clone())
        } else {
            // No end at all means one day, whose exclusive end is the next.
            dtend_from_stored_all_day(&ev.start_at).unwrap_or_else(|| ev.start_at.clone())
        };
        ev.end_at = stored_all_day_from_dtend(&ev.start_at, &dtend)
            .unwrap_or_else(|| ev.start_at.clone());
        return;
    }

    if !saw_dtend || ev.end_at.is_empty() {
        ev.end_at = match duration.as_deref().and_then(duration_minutes) {
            Some(minutes) => super::tz::parse_stamp(&ev.start_at)
                .map(|s| (s + chrono::Duration::minutes(minutes)).format("%Y-%m-%dT%H:%M").to_string())
                .unwrap_or_else(|| ev.start_at.clone()),
            None => ev.start_at.clone(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: &str = "20260301T000000Z";

    fn event(props: serde_json::Value) -> EventSummary {
        EventSummary::from_properties("Events/a.md", "Design review", "", &props)
    }

    fn line_with(text: &str, prefix: &str) -> Option<String> {
        text.split("\r\n").find(|l| l.starts_with(prefix)).map(str::to_string)
    }

    // ─── Writing ────────────────────────────────────────────

    #[test]
    fn a_calendar_has_the_wrapper_every_reader_looks_for() {
        let ics = export(&[], NOW);
        assert!(ics.starts_with("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n"));
        assert!(ics.contains("PRODID:-//Synabit//Calendar//EN\r\n"));
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
    }

    /// The one-day trap. `DTEND` names the first day the event does *not*
    /// cover, so a single-day event on the 10th ends on the 11th. Writing the
    /// stored value straight through moves every all-day event a day short.
    #[test]
    fn an_all_day_event_ends_the_day_after_the_one_it_covers() {
        let ics = export(
            &[event(json!({ "start_at": "2026-03-10", "end_at": "2026-03-10", "is_all_day": true }))],
            NOW,
        );
        assert_eq!(line_with(&ics, "DTSTART").as_deref(), Some("DTSTART;VALUE=DATE:20260310"));
        assert_eq!(line_with(&ics, "DTEND").as_deref(), Some("DTEND;VALUE=DATE:20260311"));
    }

    #[test]
    fn a_multi_day_all_day_event_keeps_its_length() {
        let ics = export(
            &[event(json!({ "start_at": "2026-03-10", "end_at": "2026-03-13", "is_all_day": true }))],
            NOW,
        );
        assert_eq!(line_with(&ics, "DTEND").as_deref(), Some("DTEND;VALUE=DATE:20260314"));
    }

    /// No zone means floating — the same wall clock wherever it is read.
    /// Writing a `Z` would claim it is UTC, which would move it for everyone
    /// who is not.
    #[test]
    fn an_event_with_no_zone_is_written_floating() {
        let ics = export(
            &[event(json!({ "start_at": "2026-03-10T09:00", "end_at": "2026-03-10T10:30" }))],
            NOW,
        );
        assert_eq!(line_with(&ics, "DTSTART").as_deref(), Some("DTSTART:20260310T090000"));
        assert_eq!(line_with(&ics, "DTEND").as_deref(), Some("DTEND:20260310T103000"));
    }

    #[test]
    fn an_event_with_a_zone_says_which_one() {
        let ics = export(
            &[event(json!({ "start_at": "2026-03-10T09:00", "tzid": "Asia/Tokyo" }))],
            NOW,
        );
        assert_eq!(
            line_with(&ics, "DTSTART").as_deref(),
            Some("DTSTART;TZID=Asia/Tokyo:20260310T090000"),
        );
    }

    #[test]
    fn a_rule_is_written_as_a_rule() {
        let ics = export(
            &[event(json!({ "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE" }))],
            NOW,
        );
        assert_eq!(
            line_with(&ics, "RRULE").as_deref(),
            Some("RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE"),
        );
    }

    /// An older vault's `recurrence: weekly` is still a rule, and exporting
    /// it as nothing would turn a series into a single event.
    #[test]
    fn an_older_vaults_repeat_is_still_exported_as_a_rule() {
        let ics = export(
            &[event(json!({ "start_at": "2026-03-02T09:00", "recurrence": "weekly", "recurrence_end_at": "2026-12-31" }))],
            NOW,
        );
        assert_eq!(
            line_with(&ics, "RRULE").as_deref(),
            Some("RRULE:FREQ=WEEKLY;UNTIL=20261231"),
        );
    }

    #[test]
    fn a_cancelled_occurrence_is_written_as_an_exception() {
        let ics = export(
            &[event(json!({
                "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY",
                "exceptions": ["2026-03-16"],
            }))],
            NOW,
        );
        assert_eq!(line_with(&ics, "EXDATE").as_deref(), Some("EXDATE:20260316T090000"));
    }

    /// Semicolons and commas separate things in this format, so a title
    /// containing one has to be escaped or it becomes two properties.
    #[test]
    fn punctuation_in_a_title_does_not_become_syntax() {
        let mut ev = event(json!({ "start_at": "2026-03-10T09:00" }));
        ev.title = "Review: costs, risks; and timing\nsecond line".to_string();
        let ics = export(&[ev], NOW);
        assert!(ics.contains("SUMMARY:Review: costs\\, risks\\; and timing\\nsecond line\r\n"));
    }

    /// Lines are folded at 75 octets, and octets are not characters — a fold
    /// through the middle of a multi-byte character produces a file the other
    /// end cannot read as UTF-8.
    #[test]
    fn a_long_title_is_folded_without_cutting_a_character_in_half() {
        let mut ev = event(json!({ "start_at": "2026-03-10T09:00" }));
        let title = "Hội thảo về kiến trúc dữ liệu phân tán và đồng bộ hoá ngoại tuyến cho ứng dụng cục bộ".to_string();
        ev.title = title.clone();
        let ics = export(&[ev], NOW);

        for line in ics.split("\r\n") {
            assert!(line.len() <= 75, "line of {} octets: {:?}", line.len(), line);
        }
        // And it survives being read back.
        assert_eq!(import(&ics)[0].title, title);
    }

    #[test]
    fn an_event_with_no_start_is_not_exported_at_all() {
        assert!(!export(&[event(json!({ "title": "x" }))], NOW).contains("BEGIN:VEVENT"));
    }

    // ─── Reading ────────────────────────────────────────────

    #[test]
    fn an_all_day_event_read_back_covers_the_days_it_named() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\nSUMMARY:Trip\r\n\
                   DTSTART;VALUE=DATE:20260310\r\nDTEND;VALUE=DATE:20260314\r\n\
                   END:VEVENT\r\nEND:VCALENDAR\r\n";
        let got = import(ics);
        assert_eq!(got.len(), 1);
        assert!(got[0].is_all_day);
        assert_eq!(got[0].start_at, "2026-03-10");
        assert_eq!(got[0].end_at, "2026-03-13", "the 14th is the day it no longer covers");
    }

    /// Not legal, but common enough that refusing it would lose the event.
    #[test]
    fn an_exporter_that_omits_dtend_still_gives_a_day() {
        let ics = "BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20260310\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\n";
        let got = import(ics);
        assert_eq!(got[0].start_at, "2026-03-10");
        assert_eq!(got[0].end_at, "2026-03-10");
    }

    #[test]
    fn a_duration_is_read_when_there_is_no_end() {
        let ics = "BEGIN:VEVENT\r\nDTSTART:20260310T090000\r\nDURATION:PT1H30M\r\nSUMMARY:Call\r\nEND:VEVENT\r\n";
        assert_eq!(import(ics)[0].end_at, "2026-03-10T10:30");

        let days = "BEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20260310\r\nDURATION:P3D\r\nSUMMARY:Trip\r\nEND:VEVENT\r\n";
        assert_eq!(import(days)[0].end_at, "2026-03-12");
    }

    #[test]
    fn a_zone_on_the_start_is_kept() {
        let ics = "BEGIN:VEVENT\r\nDTSTART;TZID=Asia/Tokyo:20260310T090000\r\nSUMMARY:x\r\nEND:VEVENT\r\n";
        let got = import(ics);
        assert_eq!(got[0].tzid, "Asia/Tokyo");
        assert_eq!(got[0].start_at, "2026-03-10T09:00");
    }

    #[test]
    fn a_quoted_parameter_does_not_end_the_line_early() {
        let ics = "BEGIN:VEVENT\r\nDTSTART;TZID=\"Asia/Ho_Chi_Minh\":20260310T090000\r\nSUMMARY:x\r\nEND:VEVENT\r\n";
        assert_eq!(import(ics)[0].tzid, "Asia/Ho_Chi_Minh");
    }

    /// A `VALARM` sits inside the event and carries a `TRIGGER` and sometimes
    /// a `SUMMARY` of its own. Reading those as the event's is how a meeting
    /// ends up named after its alarm.
    #[test]
    fn an_alarm_inside_an_event_is_not_mistaken_for_the_event() {
        let ics = "BEGIN:VEVENT\r\nUID:x\r\nSUMMARY:Real meeting\r\nDTSTART:20260310T090000\r\n\
                   BEGIN:VALARM\r\nTRIGGER:-PT15M\r\nACTION:DISPLAY\r\nDESCRIPTION:Alarm text\r\n\
                   END:VALARM\r\nEND:VEVENT\r\n";
        let got = import(ics);
        assert_eq!(got[0].title, "Real meeting");
        assert_eq!(got[0].description, "", "the alarm's text is not the event's");
    }

    #[test]
    fn a_timezone_component_beside_the_events_is_skipped() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/London\r\n\
                   BEGIN:DAYLIGHT\r\nDTSTART:19700329T010000\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\n\
                   BEGIN:VEVENT\r\nUID:x\r\nSUMMARY:Real\r\nDTSTART:20260310T090000\r\nEND:VEVENT\r\n\
                   END:VCALENDAR\r\n";
        let got = import(ics);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].start_at, "2026-03-10T09:00");
    }

    #[test]
    fn a_folded_line_is_read_as_one_line() {
        let ics = "BEGIN:VEVENT\r\nDTSTART:20260310T090000\r\nSUMMARY:A very long title that\r\n  continues here\r\nEND:VEVENT\r\n";
        assert_eq!(import(ics)[0].title, "A very long title that continues here");
    }

    #[test]
    fn escapes_are_undone_on_the_way_in() {
        let ics = "BEGIN:VEVENT\r\nDTSTART:20260310T090000\r\nSUMMARY:costs\\, risks\\; and\\ntiming\r\nEND:VEVENT\r\n";
        assert_eq!(import(ics)[0].title, "costs, risks; and\ntiming");
    }

    /// The one that would rewrite someone's calendar quietly.
    ///
    /// `FREQ=MONTHLY;BYDAY=-1FR` is the last Friday of the month. Keeping the
    /// `FREQ=MONTHLY` and dropping the `-1FR` leaves "the 26th of every
    /// month" — a different series wearing the same name, and nothing on
    /// screen to say so. The event arrives without a repeat instead, which is
    /// visible.
    #[test]
    fn a_rule_this_app_cannot_reproduce_is_refused_rather_than_approximated() {
        let ics = "BEGIN:VEVENT\r\nDTSTART:20260626T170000\r\nSUMMARY:Last Friday\r\n\
                   RRULE:FREQ=MONTHLY;BYDAY=-1FR\r\nEND:VEVENT\r\n";
        let got = import(ics);
        assert_eq!(got[0].rrule, "", "no rule is better than the wrong rule");
        assert!(got[0].rrule_dropped, "and the import has to be able to say so");
        assert_eq!(got[0].start_at, "2026-06-26T17:00", "the event itself still arrives");
    }

    #[test]
    fn a_rule_this_app_can_reproduce_is_kept() {
        let ics = "BEGIN:VEVENT\r\nDTSTART:20260601T140000\r\n\
                   RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO;COUNT=8\r\nEND:VEVENT\r\n";
        let got = import(ics);
        assert_eq!(got[0].rrule, "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO;COUNT=8");
        assert!(!got[0].rrule_dropped);
    }

    #[test]
    fn an_unreadable_rule_is_no_rule_and_is_not_reported_as_a_loss() {
        let junk = "BEGIN:VEVENT\r\nDTSTART:20260302T090000\r\nRRULE:FREQ=FORTNIGHTLY\r\nEND:VEVENT\r\n";
        let got = import(junk);
        assert_eq!(got[0].rrule, "");
        assert!(got[0].rrule_dropped, "the file did claim it repeats");
    }

    #[test]
    fn exceptions_are_read_as_days_however_they_were_written() {
        let ics = "BEGIN:VEVENT\r\nDTSTART:20260302T090000\r\nRRULE:FREQ=WEEKLY\r\n\
                   EXDATE:20260316T090000,20260323T090000\r\nEXDATE;VALUE=DATE:20260330\r\nEND:VEVENT\r\n";
        assert_eq!(import(ics)[0].exceptions, ["2026-03-16", "2026-03-23", "2026-03-30"]);
    }

    #[test]
    fn an_empty_or_unrelated_file_yields_nothing_rather_than_failing() {
        assert!(import("").is_empty());
        assert!(import("hello").is_empty());
        assert!(import("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n").is_empty());
        assert!(import("BEGIN:VEVENT\r\nSUMMARY:no start\r\nEND:VEVENT\r\n").is_empty());
    }

    #[test]
    fn a_file_with_bare_newlines_is_still_read() {
        let ics = "BEGIN:VEVENT\nDTSTART:20260310T090000\nSUMMARY:Unix line endings\nEND:VEVENT\n";
        assert_eq!(import(ics)[0].title, "Unix line endings");
    }

    // ─── The gate ───────────────────────────────────────────

    /// What this phase is for: an event that leaves and comes back is the
    /// same event. Every shape that has ever been one cell off in a calendar
    /// import is in here.
    #[test]
    fn every_kind_of_event_survives_leaving_and_coming_back() {
        let events = vec![
            event(json!({ "node_id": "n1", "start_at": "2026-03-10T09:00", "end_at": "2026-03-10T10:30" })),
            event(json!({ "node_id": "n2", "start_at": "2026-03-10", "end_at": "2026-03-10", "is_all_day": true })),
            event(json!({ "node_id": "n3", "start_at": "2026-03-10", "end_at": "2026-03-13", "is_all_day": true })),
            event(json!({ "node_id": "n4", "start_at": "2026-03-10T22:00", "end_at": "2026-03-11T01:30" })),
            event(json!({ "node_id": "n5", "start_at": "2026-03-02T09:00", "end_at": "2026-03-02T09:15",
                          "rrule": "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10" })),
            event(json!({ "node_id": "n6", "start_at": "2026-03-02T09:00", "end_at": "2026-03-02T09:15",
                          "rrule": "FREQ=WEEKLY", "exceptions": ["2026-03-16"] })),
            event(json!({ "node_id": "n7", "start_at": "2026-03-10T09:00", "end_at": "2026-03-10T10:00",
                          "tzid": "Asia/Tokyo" })),
            event(json!({ "node_id": "n8", "start_at": "2028-02-29", "is_all_day": true,
                          "rrule": "FREQ=YEARLY;INTERVAL=4" })),
            event(json!({ "node_id": "n9", "start_at": "2026-12-31", "end_at": "2027-01-02", "is_all_day": true })),
        ];

        let ics = export(&events, NOW);
        let back = import(&ics);
        assert_eq!(back.len(), events.len(), "every event came back");

        for (before, after) in events.iter().zip(back.iter()) {
            let what = &before.uid;
            assert_eq!(&after.uid, what);
            assert_eq!(after.is_all_day, before.is_all_day, "{}: all-day", what);
            assert_eq!(after.start_at, before.start_at, "{}: start", what);
            assert_eq!(after.tzid, before.tzid, "{}: zone", what);
            assert_eq!(after.exceptions, before.exceptions, "{}: exceptions", what);
            assert_eq!(
                after.rrule,
                before.rule().map(|r| r.to_rrule_string()).unwrap_or_default(),
                "{}: rule",
                what,
            );

            // The end is the one that has ever been a day out.
            let expected_end = if before.end_at.trim().is_empty() {
                before.start_at.clone()
            } else {
                before.end_at.clone()
            };
            assert_eq!(after.end_at, expected_end, "{}: end", what);
        }
    }

    /// The identity has to survive the trip, or a calendar that took this
    /// event once makes a second copy the next time.
    #[test]
    fn the_identity_that_survives_a_rename_is_the_one_that_is_exported() {
        let renamed = event(json!({ "node_id": "stable-1", "start_at": "2026-03-10T09:00" }));
        assert_eq!(renamed.uid, "stable-1");
        assert_eq!(import(&export(&[renamed], NOW))[0].uid, "stable-1");

        // A file sync has not reached yet falls back to its path.
        let fresh = event(json!({ "start_at": "2026-03-10T09:00" }));
        assert_eq!(import(&export(&[fresh], NOW))[0].uid, "Events/a.md");
    }
}

/// Write a sample calendar covering every shape this app can store.
///
/// Used by `cargo test -- --ignored write_a_sample_calendar` so the file can
/// be handed to a parser that is not this one. A format is only interoperable
/// if something else agrees, and nothing in this crate is something else.
#[cfg(test)]
#[test]
#[ignore]
fn write_a_sample_calendar() {
    use serde_json::json;
    let make = |id: &str, title: &str, props: serde_json::Value| {
        EventSummary::from_properties(id, title, "", &props)
    };
    let events = vec![
        make("Events/1.md", "Timed, floating", json!({ "node_id": "n1",
            "start_at": "2026-03-10T09:00", "end_at": "2026-03-10T10:30", "location": "Room 3" })),
        make("Events/2.md", "All day, one", json!({ "node_id": "n2",
            "start_at": "2026-03-10", "end_at": "2026-03-10", "is_all_day": true })),
        make("Events/3.md", "All day, four", json!({ "node_id": "n3",
            "start_at": "2026-03-10", "end_at": "2026-03-13", "is_all_day": true })),
        make("Events/4.md", "Across midnight", json!({ "node_id": "n4",
            "start_at": "2026-03-10T22:00", "end_at": "2026-03-11T01:30" })),
        make("Events/5.md", "Every other Mon & Wed, ten times", json!({ "node_id": "n5",
            "start_at": "2026-03-02T09:00", "end_at": "2026-03-02T09:15",
            "rrule": "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10" })),
        make("Events/6.md", "Weekly with a cancellation", json!({ "node_id": "n6",
            "start_at": "2026-03-02T09:00", "end_at": "2026-03-02T09:15",
            "rrule": "FREQ=WEEKLY", "exceptions": ["2026-03-16"] })),
        make("Events/7.md", "In Tokyo", json!({ "node_id": "n7",
            "start_at": "2026-03-10T09:00", "end_at": "2026-03-10T10:00", "tzid": "Asia/Tokyo" })),
        make("Events/8.md", "Leap day, every four years", json!({ "node_id": "n8",
            "start_at": "2028-02-29", "is_all_day": true, "rrule": "FREQ=YEARLY;INTERVAL=4" })),
        make("Events/9.md", "Across new year", json!({ "node_id": "n9",
            "start_at": "2026-12-31", "end_at": "2027-01-02", "is_all_day": true })),
        make("Events/10.md", "Review: costs, risks; and timing", json!({ "node_id": "n10",
            "start_at": "2026-03-11T14:00", "end_at": "2026-03-11T15:00",
            "tags": ["work", "planning"] })),
        make("Events/11.md", "Hội thảo kiến trúc dữ liệu phân tán và đồng bộ hoá ngoại tuyến", json!({ "node_id": "n11",
            "start_at": "2026-03-12T08:00", "end_at": "2026-03-12T17:00" })),
    ];
    let path = std::env::var("ICS_OUT").unwrap_or_else(|_| "/tmp/synabit-sample.ics".to_string());
    std::fs::write(&path, export(&events, "20260301T000000Z")).expect("write the sample");
    println!("wrote {}", path);
}

/// Read a calendar this app did not write.
///
/// `cargo test -- --ignored read_a_foreign_calendar`, with `ICS_IN` pointing
/// at the file. Kept out of the normal run because it needs a file on disk,
/// but it is the half of interoperability the round-trip test cannot cover:
/// a round trip only proves this code agrees with itself.
#[cfg(test)]
#[test]
#[ignore]
fn read_a_foreign_calendar() {
    let path = std::env::var("ICS_IN").expect("set ICS_IN to a calendar file");
    let text = std::fs::read_to_string(&path).expect("read the file");
    let events = import(&text);
    println!("read {} events from {}", events.len(), path);
    for e in &events {
        println!(
            "  {:11} {:52} {} .. {}{}{}{}{}",
            e.uid,
            e.title.chars().take(50).collect::<String>(),
            e.start_at,
            e.end_at,
            if e.is_all_day { "  [all-day]" } else { "" },
            if e.tzid.is_empty() { String::new() } else { format!("  tz={}", e.tzid) },
            if e.rrule.is_empty() { String::new() } else { format!("  rrule={}", e.rrule) },
            if e.exceptions.is_empty() { String::new() } else { format!("  ex={:?}", e.exceptions) },
        );
        if !e.location.is_empty() { println!("              location: {:?}", e.location); }
        if !e.description.is_empty() { println!("              description: {:?}", e.description); }
        if !e.tags.is_empty() { println!("              tags: {:?}", e.tags); }
    }
}
