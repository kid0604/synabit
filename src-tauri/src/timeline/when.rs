//! When something happened, to the precision anybody knows it.
//!
//! A life is rarely dated to the day: *"in April"*, *"the last year of
//! university"*, *"around 2012"*. So a time on the timeline is always a span
//! with a precision, and questions are answered by whether two spans overlap,
//! never by whether two dates are equal. Asking about May 2016 finds
//! `2016-05-14`, `2016-05` and `2016` alike. See `docs/timeline-2026-09-17.md`
//! §4.4.
//!
//! | written | span | precision |
//! | --- | --- | --- |
//! | `2016-05-14`, `2016-05-14T09:00` | that day | day |
//! | `2018-07` | the month | month |
//! | `2012` | the year | year |
//! | `2009-09..2014-06` | first day of one to last day of the other | range |
//! | `2009-09/2014-06` | the same, the older spelling | range |
//! | `today`, `last-year`, `this-month` | read against the day it is | — |
//! | `~2012` | widened by one unit either side | approx |

use chrono::{Datelike, Months, NaiveDate};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Precision {
    Day,
    Month,
    Year,
    Range,
    Approx,
}

impl Precision {
    pub fn as_str(self) -> &'static str {
        match self {
            Precision::Day => "day",
            Precision::Month => "month",
            Precision::Year => "year",
            Precision::Range => "range",
            Precision::Approx => "approx",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub precision: Precision,
}

impl Span {
    pub fn day(date: NaiveDate) -> Self {
        Span {
            from: date,
            to: date,
            precision: Precision::Day,
        }
    }

    /// How wide it is, in days, counting both ends. This is what a view's zoom
    /// is read from: see [`super::magnitude::room_for`].
    pub fn days(&self) -> i64 {
        (self.to - self.from).num_days().max(0) + 1
    }
}

/// The far end of something still going on, such as a job held today.
///
/// Stored as a date so that overlap is one comparison with no special case.
/// Nothing that really happened ends on it.
pub fn open_end() -> NaiveDate {
    NaiveDate::from_ymd_opt(9999, 12, 31).expect("a valid date")
}

/// The month and day a text names, with the year taken off: `11-05`.
///
/// §6.2. `same-day-as(today)` is the whole of "this day in other years", and
/// it is a **day of the year**, not a span — which is why it cannot be
/// answered by the same comparison every other `when:` uses.
pub fn same_day_as(text: &str) -> Option<String> {
    Some(parse(text)?.from.format("%m-%d").to_string())
}

/// What to write instead, when what was written could not be read.
///
/// One string in one place: three commands used to each keep their own list of
/// examples, and two of them still offered `2016-05-01/2016-06-30` and
/// `"last year"` — a separator §11 replaced and a spelling that never parsed.
/// An error message that teaches the wrong syntax is worse than one that
/// teaches none.
pub const HOW_TO_WRITE_ONE: &str =
    "Use 2016-05-14, 2016-05, 2016, 2016-05..2016-06, ~2012, or today / this-month / last-year.";

/// A date as the timeline stores it.
pub fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// The span `text` names, or `None` when it names none.
pub fn parse(text: &str) -> Option<Span> {
    parse_on(text, chrono::Local::now().date_naive())
}

/// The same, told what day it is.
///
/// `today` is an argument rather than a clock read inside, so that a test of
/// `last-year` is a test of arithmetic instead of a test that happens to pass
/// until December.
pub fn parse_on(text: &str, today: NaiveDate) -> Option<Span> {
    relative(&text.trim().to_lowercase(), today).or_else(|| parse_written(text))
}

/// The span `text` names outright, with no reference to what day it is.
///
/// This is what a **stored decision** must be written in. A seal and a hush
/// outlive the day they were made: keep the word `last-year` in one and every
/// January it slides off the year it was meant to cover and onto a different
/// one — silently, because nothing re-reads it until something is hidden that
/// should not have been. So `timeline::seal` and `timeline::quiet` read their
/// periods through this and refuse a word that moves.
pub fn parse_written(text: &str) -> Option<Span> {
    let text = text.trim();

    if let Some(inner) = text.strip_prefix('~') {
        return widen(parse_point(inner.trim())?);
    }

    // `..` first, and `/` still after it: §11 renames the range separator
    // because `2016-05/2016-06` reads as a day-and-month to most of the world,
    // but a query somebody saved last week is still a question they meant.
    let halves = text
        .split_once("..")
        .or_else(|| text.split_once('/'));
    if let Some((start, end)) = halves {
        let (start, end) = (parse_point(start.trim())?, parse_point(end.trim())?);
        return (start.from <= end.to).then_some(Span {
            from: start.from,
            to: end.to,
            precision: Precision::Range,
        });
    }

    parse_point(text)
}

/// The spans that are named rather than written out.
///
/// These are what `date:today` promised and never delivered — it parsed three
/// of these words into a field no runner ever read, so `date:today` matched
/// the entire vault. §11 points that spelling at `when:`, which means `when:`
/// has to actually answer it.
///
/// # There is a second reader of words like these, on purpose
///
/// [`super::asked::span_in`] finds a time phrase inside a whole sentence, in
/// Vietnamese and English, for the chat harness. This reads one written
/// *value*. They are kept apart because they answer different questions, and
/// they differ in one visible way worth naming: `span_in` reads "this month"
/// as the 1st **to today**, because it is describing what has already
/// happened in an answer. Here `this-month` is the whole month, so that it
/// means the same thing as `when:2026-09` — a query about a month is a query
/// about a month, including the part of it still to come.
fn relative(word: &str, today: NaiveDate) -> Option<Span> {
    let day = |date: NaiveDate| Some(Span::day(date));
    let from_to = |from: NaiveDate, to: NaiveDate, precision| Some(Span { from, to, precision });
    let month_of = |date: NaiveDate| {
        let first = date.with_day(1)?;
        from_to(
            first,
            first.checked_add_months(Months::new(1))?.pred_opt()?,
            Precision::Month,
        )
    };
    let year_of = |year: i32| {
        from_to(
            NaiveDate::from_ymd_opt(year, 1, 1)?,
            NaiveDate::from_ymd_opt(year, 12, 31)?,
            Precision::Year,
        )
    };

    match word {
        "today" => day(today),
        "yesterday" => day(today.pred_opt()?),
        "tomorrow" => day(today.succ_opt()?),
        // The week starts on Monday, which is what a Vietnamese calendar and
        // ISO 8601 both say.
        "this-week" => {
            let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            from_to(monday, monday + chrono::Duration::days(6), Precision::Range)
        }
        "last-week" => {
            let monday = today
                - chrono::Duration::days(today.weekday().num_days_from_monday() as i64)
                - chrono::Duration::days(7);
            from_to(monday, monday + chrono::Duration::days(6), Precision::Range)
        }
        "this-month" => month_of(today),
        "last-month" => month_of(today.with_day(1)?.checked_sub_months(Months::new(1))?),
        "this-year" => year_of(today.year()),
        "last-year" => year_of(today.year() - 1),
        _ => None,
    }
}

fn parse_point(text: &str) -> Option<Span> {
    let bytes = text.as_bytes();
    let digits = |at: std::ops::Range<usize>| {
        bytes
            .get(at)
            .is_some_and(|run| run.iter().all(u8::is_ascii_digit))
    };

    // A day, possibly followed by a time, which the timeline does not keep.
    if bytes.len() >= 10
        && digits(0..4)
        && bytes[4] == b'-'
        && digits(5..7)
        && bytes[7] == b'-'
        && digits(8..10)
        && (bytes.len() == 10 || matches!(bytes[10], b'T' | b' '))
    {
        let date = NaiveDate::parse_from_str(&text[..10], "%Y-%m-%d").ok()?;
        return Some(Span::day(date));
    }

    if bytes.len() == 7 && digits(0..4) && bytes[4] == b'-' && digits(5..7) {
        let from = NaiveDate::from_ymd_opt(text[..4].parse().ok()?, text[5..7].parse().ok()?, 1)?;
        let to = from.checked_add_months(Months::new(1))?.pred_opt()?;
        return Some(Span {
            from,
            to,
            precision: Precision::Month,
        });
    }

    if bytes.len() == 4 && digits(0..4) {
        let year: i32 = text.parse().ok()?;
        return Some(Span {
            from: NaiveDate::from_ymd_opt(year, 1, 1)?,
            to: NaiveDate::from_ymd_opt(year, 12, 31)?,
            precision: Precision::Year,
        });
    }

    None
}

fn widen(point: Span) -> Option<Span> {
    let (from, to) = match point.precision {
        Precision::Day => (point.from.pred_opt()?, point.to.succ_opt()?),
        Precision::Month => (
            point.from.checked_sub_months(Months::new(1))?,
            point.from.checked_add_months(Months::new(2))?.pred_opt()?,
        ),
        Precision::Year => (
            NaiveDate::from_ymd_opt(point.from.year() - 1, 1, 1)?,
            NaiveDate::from_ymd_opt(point.from.year() + 1, 12, 31)?,
        ),
        Precision::Range | Precision::Approx => return None,
    };
    Some(Span {
        from,
        to,
        precision: Precision::Approx,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn span(text: &str) -> (NaiveDate, NaiveDate, Precision) {
        let s = parse(text).unwrap_or_else(|| panic!("'{text}' should parse"));
        (s.from, s.to, s.precision)
    }

    #[test]
    fn a_day_with_or_without_a_time_is_that_day() {
        assert_eq!(span("2016-05-14"), (day(2016, 5, 14), day(2016, 5, 14), Precision::Day));
        assert_eq!(span("2016-05-14T09:30:00"), (day(2016, 5, 14), day(2016, 5, 14), Precision::Day));
        assert_eq!(span("2016-05-14 09:30"), (day(2016, 5, 14), day(2016, 5, 14), Precision::Day));
    }

    #[test]
    fn a_month_runs_to_its_own_last_day() {
        assert_eq!(span("2018-07"), (day(2018, 7, 1), day(2018, 7, 31), Precision::Month));
        assert_eq!(span("2024-02"), (day(2024, 2, 1), day(2024, 2, 29), Precision::Month));
    }

    #[test]
    fn a_year_is_the_whole_year() {
        assert_eq!(span("2012"), (day(2012, 1, 1), day(2012, 12, 31), Precision::Year));
    }

    #[test]
    fn a_range_runs_from_the_start_of_one_to_the_end_of_the_other() {
        assert_eq!(
            span("2009-09/2014-06"),
            (day(2009, 9, 1), day(2014, 6, 30), Precision::Range)
        );
        assert_eq!(parse("2014-06/2009-09"), None, "a range that ends before it starts");
    }

    #[test]
    fn around_a_time_widens_it_by_one_unit_either_side() {
        assert_eq!(span("~2012"), (day(2011, 1, 1), day(2013, 12, 31), Precision::Approx));
        assert_eq!(span("~2018-07"), (day(2018, 6, 1), day(2018, 8, 31), Precision::Approx));
        assert_eq!(span("~2016-05-14"), (day(2016, 5, 13), day(2016, 5, 15), Precision::Approx));
    }

    #[test]
    fn anything_else_is_not_a_time() {
        for text in ["", "May 2016", "2016-13", "2016-02-30", "16-05-14", "hôm qua", "~2009/2010"] {
            assert_eq!(parse(text), None, "'{text}'");
        }
    }

    /// §11 points `date:today` at `when:today`, so `when:` has to answer it.
    /// Told what day it is rather than asking, so this is arithmetic and not a
    /// test that happens to pass until December.
    #[test]
    fn the_words_that_name_a_time_are_read_against_the_day_it_is() {
        let today = day(2026, 9, 20); // a Sunday
        let on = |text: &str| parse_on(text, today).map(|s| (s.from, s.to));

        assert_eq!(on("today"), Some((day(2026, 9, 20), day(2026, 9, 20))));
        assert_eq!(on("yesterday"), Some((day(2026, 9, 19), day(2026, 9, 19))));
        // Monday to Sunday: the week a Vietnamese calendar and ISO 8601 agree on.
        assert_eq!(on("this-week"), Some((day(2026, 9, 14), day(2026, 9, 20))));
        assert_eq!(on("last-week"), Some((day(2026, 9, 7), day(2026, 9, 13))));
        // The whole month, not the part of it that has happened — so that
        // `this-month` and `2026-09` are the same question.
        assert_eq!(on("this-month"), Some((day(2026, 9, 1), day(2026, 9, 30))));
        assert_eq!(on("last-month"), Some((day(2026, 8, 1), day(2026, 8, 31))));
        assert_eq!(on("last-year"), Some((day(2025, 1, 1), day(2025, 12, 31))));
        assert_eq!(on("LAST-YEAR"), on("last-year"), "the words are not shouted at");
    }

    /// §11 renames the range separator, and keeps reading the old one: a query
    /// somebody saved last week is still a question they meant.
    #[test]
    fn a_range_is_written_with_two_dots_and_still_read_with_a_slash() {
        assert_eq!(span("2009-09..2014-06"), span("2009-09/2014-06"));
        assert_eq!(parse("2014-06..2009-09"), None, "a range that ends before it starts");
    }

    /// A seal and a hush outlive the day they were made, so the words that
    /// move are not a spelling they may be written in.
    #[test]
    fn a_stored_decision_cannot_be_written_in_a_word_that_moves() {
        let today = day(2026, 9, 20);
        for moving in ["today", "yesterday", "last-year", "this-month"] {
            assert!(parse_on(moving, today).is_some(), "'{moving}' reads as a time");
            assert_eq!(parse_written(moving), None, "but '{moving}' may not be stored");
        }
        // What can be written down still can be.
        assert!(parse_written("2019-11-05").is_some());
        assert!(parse_written("2019-02..2019-06").is_some());
    }
}
