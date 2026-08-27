//! The line between how this app stores a day and how iCalendar names one.
//!
//! # Why the vault does not simply store what iCalendar stores
//!
//! RFC 5545 makes `DTEND` *exclusive*: an all-day event on the 10th ends on
//! the 11th. This app stores the last day the event covers — the 10th — and
//! always has. Those are the same event written two ways, and exactly one day
//! apart.
//!
//! Flipping the vault to the RFC's convention was on the plan for this phase.
//! It is not done, and deliberately:
//!
//! * Nothing in a file says which convention wrote it. Migrating means
//!   stamping every event with a version marker and reading two shapes
//!   forever — the cost is permanent, not one-off.
//! * Getting it wrong moves every multi-day event in a real vault by a day,
//!   silently, with nothing on screen to show for it.
//! * It buys exactly one thing: correct import and export. And that can be
//!   bought here instead, at the boundary, for the two functions below.
//!
//! So the convention is written down rather than changed, and everything that
//! speaks to the outside world converts as it crosses. The rule for anyone
//! adding an exporter or a CalDAV client later: **never put a stored `end_at`
//! into a `DTEND`, and never put a `DTEND` into a stored `end_at`.** Call
//! these.

use chrono::NaiveDate;

/// The `DTEND` for an all-day event this app stores as ending on `end_date`.
///
/// One day later, because the RFC names the first day the event no longer
/// covers.
pub fn dtend_from_stored_all_day(end_date: &str) -> Option<String> {
    let d = NaiveDate::parse_from_str(end_date, "%Y-%m-%d").ok()?;
    d.succ_opt().map(|next| next.format("%Y-%m-%d").to_string())
}

/// The `end_at` to store for an all-day event whose `DTEND` is `dtend`.
///
/// One day earlier. A `DTEND` on or before the start is treated as a
/// single-day event rather than a negative one: some exporters write
/// `DTEND == DTSTART`, which is not legal but is common.
pub fn stored_all_day_from_dtend(dtstart: &str, dtend: &str) -> Option<String> {
    let start = NaiveDate::parse_from_str(dtstart, "%Y-%m-%d").ok()?;
    let end = NaiveDate::parse_from_str(dtend, "%Y-%m-%d").ok()?;
    let last = end.pred_opt()?;
    Some(if last < start { start } else { last }.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one-day gap, in both directions, on the shapes that trip people up.
    #[test]
    fn a_single_day_event_ends_the_next_morning_in_ical_terms() {
        assert_eq!(dtend_from_stored_all_day("2026-03-10").as_deref(), Some("2026-03-11"));
        assert_eq!(stored_all_day_from_dtend("2026-03-10", "2026-03-11").as_deref(), Some("2026-03-10"));
    }

    #[test]
    fn a_multi_day_event_keeps_its_length_across_the_boundary() {
        let dtend = dtend_from_stored_all_day("2026-03-13").unwrap();
        assert_eq!(dtend, "2026-03-14");
        assert_eq!(stored_all_day_from_dtend("2026-03-10", &dtend).as_deref(), Some("2026-03-13"));
    }

    #[test]
    fn month_and_year_boundaries_are_not_special() {
        assert_eq!(dtend_from_stored_all_day("2026-02-28").as_deref(), Some("2026-03-01"));
        assert_eq!(dtend_from_stored_all_day("2028-02-28").as_deref(), Some("2028-02-29"));
        assert_eq!(dtend_from_stored_all_day("2026-12-31").as_deref(), Some("2027-01-01"));
        assert_eq!(stored_all_day_from_dtend("2026-12-31", "2027-01-01").as_deref(), Some("2026-12-31"));
    }

    /// Not legal, but common enough that refusing it would lose the event.
    #[test]
    fn an_exporter_that_writes_dtend_equal_to_dtstart_still_gives_one_day() {
        assert_eq!(stored_all_day_from_dtend("2026-03-10", "2026-03-10").as_deref(), Some("2026-03-10"));
        assert_eq!(stored_all_day_from_dtend("2026-03-10", "2026-03-01").as_deref(), Some("2026-03-10"));
    }

    #[test]
    fn nonsense_comes_back_as_nothing_rather_than_as_a_wrong_date() {
        assert!(dtend_from_stored_all_day("").is_none());
        assert!(dtend_from_stored_all_day("tomorrow").is_none());
        assert!(stored_all_day_from_dtend("2026-03-10", "").is_none());
    }

    /// The property that matters: a round trip is the identity, so an event
    /// exported and re-imported is the same event.
    #[test]
    fn a_round_trip_changes_nothing() {
        for (start, end) in [
            ("2026-03-10", "2026-03-10"),
            ("2026-03-10", "2026-03-13"),
            ("2026-02-27", "2026-03-02"),
            ("2028-02-28", "2028-02-29"),
        ] {
            let dtend = dtend_from_stored_all_day(end).unwrap();
            assert_eq!(
                stored_all_day_from_dtend(start, &dtend).as_deref(),
                Some(end),
                "round trip of {}..{}",
                start,
                end
            );
        }
    }
}
