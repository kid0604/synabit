//! Giving a daily note the date it is named after.
//!
//! A daily note has only ever carried its day in its title, written under
//! whatever pattern the user set (`dailyNoteFormat`, default `YYYY-MM-DD`).
//! Anything that wanted to know which day a note was about had to parse the
//! title back under the pattern in force *now*, so changing the setting quietly
//! un-dated every note written before it. See `docs/timeline-2026-09-17.md` §5.1.
//!
//! New daily notes are written with `date: "YYYY-MM-DD"` beside the title. The
//! ones already in a vault are given it once, through the silent migration path.
//!
//! # Not guessing
//!
//! That path runs on each device separately and tells sync nothing
//! (`commands::migration`). So two devices must reach the same answer from the
//! same title, or both leave it alone; they must never reach two different
//! answers. A title is dated only when reading it is not a judgement call:
//!
//! - it reads back exactly under the pattern, or under `YYYY-MM-DD`, which is
//!   read year-month-day everywhere;
//! - and, for a pattern that does not start with the year, swapping day and
//!   month does not give a *different* valid date. `03/04/2026` is left alone;
//!   `25/04/2026` is not.
//!
//! A title that fails either test keeps having no date, which is what it had.

use chrono::NaiveDate;

/// The user's pattern in chrono's terms, or `None` when chrono cannot read it.
///
/// `date_string_from_pattern` renders today's title through this same
/// translation. A title written by one translation and read back by another is
/// how a round trip stops being one.
pub fn chrono_pattern(pattern: &str) -> Option<String> {
    let translated = pattern
        .replace("YYYY", "%Y")
        .replace("YY", "%y")
        .replace("MM", "%m")
        .replace("M", "%-m")
        .replace("DD", "%d")
        .replace("D", "%-d");

    let readable = !chrono::format::StrftimeItems::new(&translated)
        .any(|item| matches!(item, chrono::format::Item::Error));
    readable.then_some(translated)
}

/// The day `title` names under `pattern`, when no device could read it otherwise.
pub fn date_from_title(title: &str, pattern: &str) -> Option<NaiveDate> {
    let title = title.trim();
    let mut found: Option<NaiveDate> = None;

    for candidate in [pattern, "YYYY-MM-DD"] {
        let Some(format) = chrono_pattern(candidate) else {
            continue;
        };
        let Some(date) = read_exactly(title, &format) else {
            continue;
        };
        // A two-digit year reads as a day as easily as a year, and nothing in
        // the title says which. Refused, as an ambiguous title is.
        if format.contains("%y") {
            continue;
        }
        // Only ISO order cannot be misread. Every other pattern, year first or
        // not, has a day and a month that can trade places: `2026/3/4`.
        if format != "%Y-%m-%d" {
            if let Some(other) = read_exactly(title, &swap_day_and_month(&format)) {
                if other != date {
                    return None;
                }
            }
        }
        match found {
            Some(earlier) if earlier != date => return None,
            _ => found = Some(date),
        }
    }

    found
}

/// `title` under `format`, only if writing the date back gives the same text.
///
/// Parsing alone is too forgiving: `2026-2-3` parses under `%Y-%m-%d`, and a
/// title nobody wrote under that pattern is not evidence of anything.
fn read_exactly(title: &str, format: &str) -> Option<NaiveDate> {
    let date = NaiveDate::parse_from_str(title, format).ok()?;
    (date.format(format).to_string() == title).then_some(date)
}

fn swap_day_and_month(format: &str) -> String {
    format
        .replace("%-d", "\u{0}-d")
        .replace("%-m", "%-d")
        .replace("\u{0}-d", "%-m")
        .replace("%d", "\u{0}d")
        .replace("%m", "%d")
        .replace("\u{0}d", "%m")
}

/// `file` with `date:` added to its frontmatter, or `None` when there is
/// nothing to add: no frontmatter, or a `date` already in it.
///
/// Text in, text out, rather than parsing the YAML and writing it back. A trip
/// through a serializer reorders keys and restyles quoting, and a migration
/// that rewrites lines it had no reason to touch produces a change nobody can
/// review. One line goes in: after `type:` if there is one, else after
/// `title:`, else just before the closing fence.
pub fn with_date(file: &str, date: NaiveDate) -> Option<String> {
    let newline = if file.starts_with("---\r\n") {
        "\r\n"
    } else if file.starts_with("---\n") {
        "\n"
    } else {
        return None;
    };
    let start = 3 + newline.len();

    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut closing = None;
    let mut offset = start;
    for line in file[start..].split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            closing = Some(offset);
            break;
        }
        lines.push((offset, line));
        offset += line.len();
    }
    let closing = closing?;

    if lines.iter().any(|(_, line)| line.starts_with("date:")) {
        return None;
    }

    let after = lines
        .iter()
        .find(|(_, line)| line.starts_with("type:"))
        .or_else(|| lines.iter().find(|(_, line)| line.starts_with("title:")));
    let at = match after {
        Some((line_start, line)) => line_start + line.len(),
        None => closing,
    };

    let inserted = format!("date: \"{}\"{}", date.format("%Y-%m-%d"), newline);
    let mut out = String::with_capacity(file.len() + inserted.len());
    out.push_str(&file[..at]);
    out.push_str(&inserted);
    out.push_str(&file[at..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_year_first_pattern_whose_day_and_month_can_trade_places_is_not_read() {
        assert_eq!(date_from_title("2026/3/4", "YYYY/D/M"), None);
        assert_eq!(date_from_title("2026/3/14", "YYYY/M/D"), Some(day(2026, 3, 14)), "14 cannot be a month");
        assert_eq!(date_from_title("2026-03-04", "YYYY/D/M"), Some(day(2026, 3, 4)), "ISO is always read");
    }

    #[test]
    fn a_two_digit_year_is_never_read() {
        assert_eq!(date_from_title("12.05.16", "YY.MM.DD"), None);
        assert_eq!(date_from_title("16.05.12", "DD.MM.YY"), None);
        assert_eq!(date_from_title("2016-05-12", "DD.MM.YY"), Some(day(2016, 5, 12)));
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn a_title_in_the_default_pattern_is_its_date() {
        assert_eq!(date_from_title("2026-02-12", "YYYY-MM-DD"), Some(day(2026, 2, 12)));
    }

    #[test]
    fn a_title_under_the_users_own_pattern_is_read_with_it() {
        assert_eq!(date_from_title("25/04/2026", "DD/MM/YYYY"), Some(day(2026, 4, 25)));
        assert_eq!(date_from_title("4/25/2026", "M/D/YYYY"), Some(day(2026, 4, 25)));
    }

    /// The gate in the doc: a user who changed the setting still has their old
    /// notes dated, because `YYYY-MM-DD` cannot be read two ways.
    #[test]
    fn notes_written_before_the_pattern_changed_are_still_read() {
        assert_eq!(date_from_title("2026-02-12", "DD.MM.YYYY"), Some(day(2026, 2, 12)));
    }

    #[test]
    fn a_title_that_could_be_two_days_is_left_alone() {
        assert_eq!(date_from_title("03/04/2026", "DD/MM/YYYY"), None);
        assert_eq!(date_from_title("3/4/2026", "D/M/YYYY"), None);
    }

    #[test]
    fn a_title_that_is_not_only_a_date_is_not_a_daily_note() {
        assert_eq!(date_from_title("2026-02-12 họp nhóm", "YYYY-MM-DD"), None);
        assert_eq!(date_from_title("Họp nhóm", "YYYY-MM-DD"), None);
        assert_eq!(date_from_title("2026-2-12", "YYYY-MM-DD"), None);
    }

    #[test]
    fn a_pattern_chrono_cannot_read_still_leaves_the_unambiguous_one() {
        assert_eq!(chrono_pattern("YYYY-MM-DD (100%)"), None);
        assert_eq!(date_from_title("2026-02-12", "YYYY-MM-DD (100%)"), Some(day(2026, 2, 12)));
    }

    #[test]
    fn the_date_goes_in_after_the_type_and_nothing_else_moves() {
        let file = "---\nnode_id: abc\ntitle: \"2026-02-12\"\ntype: \"note\"\ntags:\n  - daily\n---\n\nHôm nay.\n";
        let dated = with_date(file, day(2026, 2, 12)).unwrap();
        assert_eq!(
            dated,
            "---\nnode_id: abc\ntitle: \"2026-02-12\"\ntype: \"note\"\ndate: \"2026-02-12\"\ntags:\n  - daily\n---\n\nHôm nay.\n"
        );
    }

    #[test]
    fn without_a_type_the_date_follows_the_title() {
        let file = "---\ntitle: \"2026-02-12\"\n---\nbody\n";
        assert_eq!(
            with_date(file, day(2026, 2, 12)).unwrap(),
            "---\ntitle: \"2026-02-12\"\ndate: \"2026-02-12\"\n---\nbody\n"
        );
    }

    #[test]
    fn windows_line_endings_are_kept() {
        let file = "---\r\ntitle: \"2026-02-12\"\r\n---\r\nbody\r\n";
        assert_eq!(
            with_date(file, day(2026, 2, 12)).unwrap(),
            "---\r\ntitle: \"2026-02-12\"\r\ndate: \"2026-02-12\"\r\n---\r\nbody\r\n"
        );
    }

    #[test]
    fn a_note_that_already_has_a_date_or_no_frontmatter_is_not_touched() {
        let dated = "---\ntitle: x\ndate: \"2020-01-01\"\n---\n";
        assert_eq!(with_date(dated, day(2026, 2, 12)), None);
        assert_eq!(with_date("no frontmatter here", day(2026, 2, 12)), None);
        assert_eq!(with_date("---\ntitle: x\nnever closed\n", day(2026, 2, 12)), None);
    }

    #[test]
    fn running_it_twice_adds_one_date() {
        let file = "---\ntitle: \"2026-02-12\"\ntype: note\n---\n";
        let once = with_date(file, day(2026, 2, 12)).unwrap();
        assert_eq!(with_date(&once, day(2026, 2, 12)), None);
    }
}
