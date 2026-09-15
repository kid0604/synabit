//! One shape for every timestamp the index holds.
//!
//! # The problem, measured
//!
//! On a real vault of 963 nodes the `created_at` column held two formats: 633
//! rows of `2026-08-15 23:54:33`, the file's time in the device's own zone as
//! `parse_file_to_node` wrote it, and 330 of `2026-09-14T03:35:42.538489+00:00`,
//! which is what every Rust writer puts into frontmatter. Both are sorted as
//! plain strings throughout the app. Between two days that is harmless. Within
//! one day it is not: a space (0x20) sorts before `T` (0x54), so every
//! local-format row of a day came before every RFC 3339 row of that day,
//! whatever the clock said.
//!
//! The space format costs something else on macOS. WKWebView will not parse
//! it — `new Date("2026-08-15 23:54:33")` is an Invalid Date — and QuickCap
//! calls `toISOString()` on the result, which throws.
//!
//! # The shape
//!
//! RFC 3339 in UTC with milliseconds: `2026-09-14T03:35:42.538Z`. It is exactly
//! what `Date.prototype.toISOString()` produces, so a stamp minted in the
//! webview and a stamp read from the index sort together; every WebView parses
//! it; and it is always 24 characters, so string order is time order.
//!
//! A value with no offset is wall-clock time in this device's zone, because
//! that is what wrote it (`chrono::Local` in the parser). Reading it back in
//! the same zone recovers the instant it meant.
//!
//! A value that is not a timestamp at all comes back unchanged. Normalising is
//! a courtesy to whoever reads the column; it must never be the reason a value
//! was lost.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};

/// `stamp` in the index's one shape, or unchanged when it is not a timestamp.
pub fn normalize(stamp: &str) -> String {
    let s = stamp.trim();

    if let Ok(moment) = DateTime::parse_from_rfc3339(s) {
        return canonical(moment.with_timezone(&Utc));
    }
    // An offset after a space rather than a `T`.
    if let Ok(moment) = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%:z") {
        return canonical(moment.with_timezone(&Utc));
    }

    for format in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, format) {
            if let Some(moment) = from_local(naive) {
                return canonical(moment);
            }
        }
    }

    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(moment) = date.and_hms_opt(0, 0, 0).and_then(from_local) {
            return canonical(moment);
        }
    }

    stamp.to_string()
}

/// A moment in the index's shape.
pub fn canonical(moment: DateTime<Utc>) -> String {
    moment.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Wall-clock time in this device's zone, as an instant.
///
/// `earliest` settles the hour a daylight-saving change repeats. The hour it
/// skips never existed, so a stamp inside it is not a time and is kept as
/// written by the caller.
fn from_local(naive: NaiveDateTime) -> Option<DateTime<Utc>> {
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|moment| moment.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
        let naive = NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, s)
            .unwrap();
        from_local(naive).unwrap()
    }

    #[test]
    fn an_rfc3339_stamp_becomes_utc_with_milliseconds() {
        assert_eq!(
            normalize("2026-09-14T03:35:42.538489+00:00"),
            "2026-09-14T03:35:42.538Z"
        );
        assert_eq!(
            normalize("2026-09-14T10:35:42+07:00"),
            "2026-09-14T03:35:42.000Z"
        );
        assert_eq!(
            normalize("2026-09-14T03:35:42.538Z"),
            "2026-09-14T03:35:42.538Z"
        );
    }

    #[test]
    fn a_stamp_without_an_offset_is_read_in_this_devices_zone() {
        let expected = canonical(local(2026, 8, 15, 23, 54, 33));
        assert_eq!(normalize("2026-08-15 23:54:33"), expected);
        assert_eq!(normalize("2026-08-15T23:54:33"), expected);
    }

    #[test]
    fn normalising_twice_changes_nothing() {
        for stamp in [
            "2026-08-15 23:54:33",
            "2026-09-14T03:35:42.538489+00:00",
            "2026-01-01",
            "last Tuesday",
        ] {
            let once = normalize(stamp);
            assert_eq!(normalize(&once), once, "{stamp}");
        }
    }

    #[test]
    fn something_that_is_not_a_timestamp_is_kept_as_written() {
        assert_eq!(normalize("last Tuesday"), "last Tuesday");
        assert_eq!(normalize(""), "");
    }

    /// The bug this module closes: one day, two formats, the wrong order.
    #[test]
    fn two_stamps_from_one_day_sort_in_the_order_they_happened() {
        let earlier = normalize("2026-09-14T01:00:00+00:00");
        let later_written_locally = Utc
            .with_ymd_and_hms(2026, 9, 14, 2, 0, 0)
            .unwrap()
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let later = normalize(&later_written_locally);

        assert!(earlier < later, "{earlier} should sort before {later}");
        assert_eq!(earlier.len(), later.len());
    }
}
