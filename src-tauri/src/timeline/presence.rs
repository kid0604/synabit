//! When something was around: the stretches of a life an object was part of.
//!
//! A person is not a point on the timeline and never was. They are in it for a
//! while, then they are not, and perhaps they come back. The same is true of a
//! place, a system, a company. Those stretches are worked out from the events
//! that name the object rather than declared anywhere, which is the whole
//! point of `docs/timeline-2026-09-17.md` §9: a relationship stops being a
//! special kind of edge carrying dates and becomes **the shape of two
//! worldlines running close together for a while**.
//!
//! # Where a stretch comes from
//!
//! - every event that names the object in any role — met, was there, was about
//!   it, is evidence of it;
//! - events the object itself implies, for the objects that are nodes in their
//!   own right: a job held, a relationship entered, a day somebody was born.
//!
//! Nothing else. An object no event mentions has no stretches at all, not an
//! empty one and not a guessed one. Inventing presence is worse than admitting
//! ignorance: it would let the app say somebody was around in a year the vault
//! says nothing about.
//!
//! # Worked out when asked, not kept
//!
//! There is no table. A stretch is computed from the events that name the
//! object, at the moment somebody asks — which is the only way it can pass
//! through the same seal filter as everything else. Kept in a table it went
//! stale, and worse: sealing is applied when the timeline is *read*, from the
//! vault, so a stored stretch had no way to know that half the events it was
//! built from are not to be shown.
//!
//! # The gap
//!
//! Two events far enough apart are two stretches, not one long one. How far is
//! "far enough" is a guess, like the weights in [`super::magnitude`], and is
//! written here so it can be measured and changed: half a year with no trace
//! at all reads as absence.

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use super::when;

/// Silence longer than this breaks a worldline in two. A first guess.
const GAP_DAYS: i64 = 180;

/// A stretch of time something was part of the life, and how much happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Presence {
    pub from_day: String,
    pub to_day: String,
    pub events: u32,
}

/// The stretches one object's events fall into, oldest first.
///
/// `spans` need not be sorted. Anything unreadable as a date is dropped rather
/// than guessed at, and nothing runs past `today`: a job still held says it
/// ends in the year 9999, which is a way of writing "no end known" and not a
/// claim about the next eight thousand years. Left alone it swallowed every
/// later stretch into one, because no gap can follow a span that never ends.
pub fn merge(spans: &[(&str, &str)], today: NaiveDate) -> Vec<Presence> {
    let day = |text: &str| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok();
    // Three dates per span: where it starts, where it ends for the purpose of
    // measuring a silence, and where it ends as the vault wrote it. The last
    // two differ for something still going on, which says it ends in the year
    // 9999 — that is how "no end known" is written, and the reader shows it as
    // an open arrow. Measuring a gap from the year 9999 would be nonsense, so
    // that side is clamped and the written side is kept.
    let mut sorted: Vec<(NaiveDate, NaiveDate, NaiveDate)> = spans
        .iter()
        .filter_map(|(from, to)| {
            let from = day(from)?;
            // A date still to come is a plan, and the timeline is history
            // (§4.4 of the older doc). Nobody was anywhere in 2027 yet.
            if from > today {
                return None;
            }
            let written = day(to).filter(|to| *to >= from).unwrap_or(from);
            Some((from, written.min(today.max(from)), written))
        })
        .collect();
    sorted.sort();

    let mut out: Vec<(NaiveDate, NaiveDate, NaiveDate, u32)> = Vec::new();
    for (from, measured, written) in sorted {
        match out.last_mut() {
            // Close enough to the last stretch to be the same one. A span that
            // ends inside it only widens it if it reaches further.
            Some(last) if from <= last.1 + Duration::days(GAP_DAYS) => {
                last.1 = last.1.max(measured);
                last.2 = last.2.max(written);
                last.3 += 1;
            }
            _ => out.push((from, measured, written, 1)),
        }
    }
    out.into_iter()
        .map(|(from, _, written, events)| Presence {
            from_day: when::iso(from),
            to_day: when::iso(written),
            events,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 17).unwrap()
    }

    fn stretches(spans: &[(&str, &str)]) -> Vec<(String, String, u32)> {
        merge(spans, today())
            .into_iter()
            .map(|p| (p.from_day, p.to_day, p.events))
            .collect()
    }

    #[test]
    fn meeting_often_through_a_year_is_one_stretch_not_twenty() {
        let days: Vec<String> = (1..=20)
            .map(|n| format!("2019-{:02}-{:02}", (n % 12) + 1, (n % 27) + 1))
            .collect();
        let mut spans: Vec<(&str, &str)> = days.iter().map(|d| (d.as_str(), d.as_str())).collect();
        spans.sort();

        let found = stretches(&spans);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].0.starts_with("2019"), "{found:?}");
        assert!(found[0].1.starts_with("2019"), "{found:?}");
        assert_eq!(found[0].2, 20);
    }

    #[test]
    fn somebody_who_went_away_and_came_back_has_two_stretches() {
        let found = stretches(&[
            ("2015-03-01", "2015-03-01"),
            ("2015-06-01", "2015-06-01"),
            // Four years of nothing at all.
            ("2019-08-02", "2019-08-04"),
        ]);
        assert_eq!(
            found,
            vec![
                ("2015-03-01".into(), "2015-06-01".into(), 2),
                ("2019-08-02".into(), "2019-08-04".into(), 1),
            ]
        );
    }

    #[test]
    fn a_long_span_carries_the_stretch_along_with_it() {
        // A job held for years is one stretch, however little else happened.
        let found = stretches(&[("2014-07-01", "2019-01-31"), ("2018-05-05", "2018-05-05")]);
        assert_eq!(found, vec![("2014-07-01".into(), "2019-01-31".into(), 2)]);
    }

    /// A job still held says it ends in the year 9999. That is how "no end
    /// known" is written, and it must survive: the reader draws it as an open
    /// arrow. What must not survive is measuring a silence from the year 9999.
    #[test]
    fn something_still_going_on_stays_open_at_the_far_end() {
        let found = stretches(&[("2009-09-01", "9999-12-31"), ("2019-08-02", "2019-08-04")]);
        assert_eq!(
            found,
            vec![("2009-09-01".into(), "9999-12-31".into(), 2)],
            "they held it the whole time, so they were around the whole time — and still are"
        );
    }

    /// The timeline is history. An anniversary in 2027 is not evidence that
    /// anybody was anywhere.
    #[test]
    fn a_date_still_to_come_places_nobody_anywhere() {
        let found = stretches(&[("2015-03-01", "2015-03-01"), ("2027-05-14", "2027-05-14")]);
        assert_eq!(found, vec![("2015-03-01".into(), "2015-03-01".into(), 1)]);
    }

    #[test]
    fn nothing_known_is_nothing_claimed() {
        assert!(stretches(&[]).is_empty());
        assert!(stretches(&[("sometime", "sometime")]).is_empty(), "a date nobody can read is not a year");
    }
}
