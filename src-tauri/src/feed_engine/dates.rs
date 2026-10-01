//! Turning whatever a web page calls a date into one we can sort by.
//!
//! RSS and Atom dates come parsed (`parser.rs` writes RFC 3339). Scraped pages
//! and article meta tags do not: the scraper used to store whatever text sat in
//! an element whose class mentioned "date" or "time", and on real Vietnamese
//! news sites that was "21' trước", "1h trước" — or the author's name. Stored
//! as the publish date, those broke the reader three ways: every card said
//! "Invalid Date", the list (ordered by this column as text) put those feeds
//! first, and cleanup's "older than" comparison could never match them.
//!
//! So a date is normalised once, where it enters: to RFC 3339 if it can be
//! read, and to nothing if it cannot. An empty date is honest — the card shows
//! none and the article sorts by when it was fetched — and garbage is not.

use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// `raw` as RFC 3339, or `""` when it is not a date.
///
/// A date that carries its own offset keeps it ("17:46+07:00" stays that, not
/// "10:46+00:00"): Syn reads these to people, and the hour the page printed is
/// the one they recognise. Only zone-less and relative dates are converted.
///
/// `now` anchors relative dates ("2h trước", "3 days ago"): the moment the page
/// was fetched, which for a stored row is its `fetched_at`. Dates without a
/// zone are read in the machine's local zone, which is the zone a local news
/// site writes them in far more often than UTC.
pub fn normalize_published(raw: &str, now: DateTime<Utc>) -> String {
    let text = raw.trim();
    if text.is_empty() || text.len() > 64 {
        return String::new();
    }
    if let Some(dt) = absolute(text) {
        return dt.to_rfc3339();
    }
    relative(text, now).map(|dt| dt.to_rfc3339()).unwrap_or_default()
}

/// Whether a stored value is already in the form this module writes.
pub fn is_normalized(stored: &str) -> bool {
    stored.is_empty() || DateTime::parse_from_rfc3339(stored).is_ok()
}

fn absolute(text: &str) -> Option<DateTime<FixedOffset>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(dt);
    }
    if let Ok(dt) = DateTime::parse_from_rfc2822(text) {
        return Some(dt);
    }
    // Zoned forms that are not quite RFC 3339.
    for fmt in ["%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%d %H:%M:%S%z", "%Y-%m-%dT%H:%M%z"] {
        if let Ok(dt) = DateTime::parse_from_str(text, fmt) {
            return Some(dt);
        }
    }
    // Pages often add words around the date ("Thứ hai, 29/9/2026, 10:05 (GMT+7)").
    let cleaned = clean(text);
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%d/%m/%Y %H:%M:%S",
        "%d/%m/%Y %H:%M",
        "%H:%M %d/%m/%Y",
        "%d-%m-%Y %H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&cleaned, fmt) {
            return local(naive);
        }
    }
    for fmt in ["%Y-%m-%d", "%d/%m/%Y", "%d-%m-%Y"] {
        if let Ok(day) = NaiveDate::parse_from_str(&cleaned, fmt) {
            return local(day.and_hms_opt(0, 0, 0)?);
        }
    }
    None
}

/// Strip the words and punctuation news sites put around a date, keeping only
/// what the formats above read: digits, `/ - :`, `T`, and single spaces.
fn clean(text: &str) -> String {
    let lowered = text.to_lowercase();
    // Drop a weekday before the first comma ("thứ hai, 29/9/2026, 10:05").
    let body = match lowered.split_once(',') {
        Some((head, rest)) if !head.chars().any(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => lowered,
    };
    // "(gmt+7)" and the like.
    let body = match body.find('(') {
        Some(i) => body[..i].to_string(),
        None => body,
    };
    let kept: String = body
        .chars()
        .map(|c| if c.is_ascii_digit() || matches!(c, '/' | '-' | ':' | 'T') { c } else { ' ' })
        .collect();
    let mut parts: Vec<&str> = kept.split_whitespace().collect();
    // "29/9/2026 10:05" and "10:05 29/9/2026" both read; anything more is noise.
    parts.truncate(2);
    parts.join(" ")
}

fn local(naive: NaiveDateTime) -> Option<DateTime<FixedOffset>> {
    Local.from_local_datetime(&naive).earliest().map(|dt| dt.fixed_offset())
}

/// "21' trước", "2h trước", "3 giờ trước", "hôm qua", "5 minutes ago", "yesterday".
fn relative(text: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let t = text.to_lowercase();
    let t = t.trim();
    if ["vừa xong", "vừa đăng", "just now", "now"].contains(&t) {
        return Some(now);
    }
    if t == "hôm qua" || t == "yesterday" {
        return Some(now - Duration::days(1));
    }
    let is_ago = t.ends_with("trước") || t.ends_with("ago");
    if !is_ago {
        return None;
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: i64 = digits.parse().ok()?;
    let unit = t[digits.len()..].trim_start();
    let span = if unit.starts_with('\'') || unit.starts_with("phút") || unit.starts_with("min") || unit.starts_with('m') && !unit.starts_with("mo") {
        Duration::minutes(n)
    } else if unit.starts_with('h') || unit.starts_with("giờ") || unit.starts_with("tiếng") {
        Duration::hours(n)
    } else if unit.starts_with("ngày") || unit.starts_with('d') {
        Duration::days(n)
    } else if unit.starts_with("tuần") || unit.starts_with('w') {
        Duration::weeks(n)
    } else {
        return None;
    };
    Some(now - span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-30T10:00:00+00:00").unwrap().with_timezone(&Utc)
    }

    #[test]
    fn keeps_dates_that_are_already_dates_in_their_own_zone() {
        assert_eq!(normalize_published("2026-09-01T10:00:00+00:00", now()), "2026-09-01T10:00:00+00:00");
        assert_eq!(normalize_published("2026-09-09T17:46:00+07:00", now()), "2026-09-09T17:46:00+07:00");
        assert_eq!(normalize_published("Tue, 29 Sep 2026 03:05:00 +0700", now()), "2026-09-29T03:05:00+07:00");
        assert_eq!(normalize_published("2026-09-29T10:05:00+0700", now()), "2026-09-29T10:05:00+07:00");
    }

    #[test]
    fn reads_the_relative_times_vietnamese_sites_print() {
        assert_eq!(normalize_published("21' trước", now()), (now() - Duration::minutes(21)).to_rfc3339());
        assert_eq!(normalize_published("1h trước", now()), (now() - Duration::hours(1)).to_rfc3339());
        assert_eq!(normalize_published("3 giờ trước", now()), (now() - Duration::hours(3)).to_rfc3339());
        assert_eq!(normalize_published("2 ngày trước", now()), (now() - Duration::days(2)).to_rfc3339());
        assert_eq!(normalize_published("hôm qua", now()), (now() - Duration::days(1)).to_rfc3339());
        assert_eq!(normalize_published("5 minutes ago", now()), (now() - Duration::minutes(5)).to_rfc3339());
    }

    #[test]
    fn reads_local_day_month_year_with_the_words_around_it() {
        for raw in ["29/9/2026 10:05", "Thứ hai, 29/9/2026, 10:05 (GMT+7)", "10:05 29/09/2026"] {
            let got = normalize_published(raw, now());
            assert!(!got.is_empty(), "{raw} was not read");
            let local_dt = DateTime::parse_from_rfc3339(&got).unwrap().with_timezone(&Local);
            assert_eq!(local_dt.format("%Y-%m-%d %H:%M").to_string(), "2026-09-29 10:05", "{raw}");
        }
        assert!(!normalize_published("29/09/2026", now()).is_empty());
    }

    /// The bug itself: author names were stored as publish dates.
    #[test]
    fn stores_nothing_rather_than_something_that_is_not_a_date() {
        for raw in ["Đỗ Duy Thọ", "Trần Kiên, Hoàng Minh Đức", "", "   ", "Nguyễn Hồng Hải Đăng", "12 comments"] {
            assert_eq!(normalize_published(raw, now()), "", "{raw}");
        }
    }

    #[test]
    fn knows_what_it_already_wrote() {
        assert!(is_normalized(""));
        assert!(is_normalized("2026-09-01T10:00:00+00:00"));
        assert!(!is_normalized("21' trước"));
        assert!(!is_normalized("Đỗ Duy Thọ"));
    }
}
