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
//! | `2009-09/2014-06` | first day of one to last day of the other | range |
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

/// A date as the timeline stores it.
pub fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// The span `text` names, or `None` when it names none.
pub fn parse(text: &str) -> Option<Span> {
    let text = text.trim();

    if let Some(inner) = text.strip_prefix('~') {
        return widen(parse_point(inner.trim())?);
    }

    if let Some((start, end)) = text.split_once('/') {
        let (start, end) = (parse_point(start.trim())?, parse_point(end.trim())?);
        return (start.from <= end.to).then_some(Span {
            from: start.from,
            to: end.to,
            precision: Precision::Range,
        });
    }

    parse_point(text)
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
}
