//! "Ngày này, những năm trước".
//!
//! The design is §7.1 of `docs/timeline-2026-09-17.md`, and it is the first
//! thing in the app that speaks without being asked. It is deliberately small,
//! because it is the test of §6: if the literary contract cannot survive a
//! feature this simple, it will not survive the ones after it.
//!
//! # The one rule
//!
//! **Quote, do not retell.** Every line this produces is a sentence the person
//! wrote, with the day it was written and the note it came from. Nothing here
//! summarises, rewrites, softens or labels. There is no model in this module at
//! all, and that is on purpose: a model asked to "introduce" a memory will write
//! *"a lovely memory of your father"* over the sentence about the father, and
//! the sentence was the only thing worth having.
//!
//! So when there is nothing to quote, this returns nothing. §6.1's last row:
//! be silent rather than fill the space.
//!
//! # Never twice in one year, without remembering anything
//!
//! §7.1 forbids raising the same thing twice in a year. The obvious way is to
//! write down what was shown — and it is the wrong way, because that record is
//! not derivable from the vault, so it belongs to neither tier 2 nor tier 3
//! (§4.7): rebuild the index and the app starts repeating itself, or keep it in
//! the vault and every render writes a file.
//!
//! Instead the rule holds by arithmetic. A thing is offered only on the
//! anniversary of the day it *started*, and only when that day is exactly
//! known ([`super::store::TimelineStore::anniversaries`]). One anniversary a
//! year is one chance a year. Nothing is stored, nothing drifts, and opening
//! the panel twice on the same day shows the same thing both times.
//!
//! # What is refused, and why each one
//!
//! - **A day of nothing but finished tasks.** That is a work log, not a memory.
//!   §7.1 names it, and on this vault it is most days: 102 of 166 rows are
//!   tasks.
//! - **Anything sealed or hushed.** Through [`super::quiet::allow`], the one
//!   road, so that this feature cannot be the one that forgot.
//! - **Anything that cannot be quoted.** A line that leads nowhere should not
//!   appear at all (§10).

use chrono::{Datelike, NaiveDate};

use serde::Serialize;

use super::quiet::{self, Nudge, Quiet};
use super::store::{Event, TimelineStore};
use super::when;
use crate::db::DbBridge;
use crate::error::AppResult;

/// A day's writing is long enough to be a memory rather than a jotting. The
/// same floor the extractor uses to decide a note is worth reading at all.
const ENOUGH_WRITING: usize = 40;

/// The most a quote may run to before it is cut at a word.
///
/// §6.2 asks for one or two sentences; anything longer is for the person to
/// open and read themselves.
const LONGEST_QUOTE: usize = 220;

/// One thing worth handing back, in the person's own words.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Looking {
    /// The day it happened, `YYYY-MM-DD`.
    pub day: String,
    pub years_ago: i32,
    /// The event's name, as the timeline holds it.
    pub title: String,
    /// Verbatim. Never written by anything but the person.
    pub quote: String,
    /// Where to open, so every line leads back to its source (§10).
    pub node_id: String,
    pub kind: String,
}

/// What this day held in earlier years, less everything refused.
///
/// `today` is the day being looked at rather than the actual date, so a person
/// can walk back through the strip and the panel follows them.
pub fn look_back(
    store: &TimelineStore,
    cache: &DbBridge,
    today: NaiveDate,
    quiet: &Quiet,
) -> AppResult<Vec<Looking>> {
    let events = store.anniversaries(today)?;
    if events.is_empty() {
        return Ok(Vec::new());
    }

    // A day is offered whole or not at all: its events are weighed together,
    // because "a day of nothing but finished tasks" is a fact about the day.
    let mut days: Vec<(String, Vec<Event>)> = Vec::new();
    for event in events {
        match days.last_mut() {
            Some((day, held)) if *day == event.happened_from => held.push(event),
            _ => days.push((event.happened_from.clone(), vec![event])),
        }
    }

    let mut looking = Vec::new();
    for (day, held) in days {
        if !held.iter().any(|e| e.shape.is_a_memory()) {
            continue;
        }
        let Some(found) = quotable(cache, &held) else {
            continue;
        };
        let (event, quote) = found;
        let offered = quiet::allow(vec![Nudge::for_event(event)], quiet);
        if offered.is_empty() {
            continue;
        }
        let Some(years_ago) = years_between(&day, today) else {
            continue;
        };
        looking.push(Looking {
            day,
            years_ago,
            title: event.title.clone(),
            quote,
            node_id: event.container_node.clone().unwrap_or_else(|| event.node_id.clone()),
            kind: event.kind.clone(),
        });
    }

    // Nearest year first: last year is what a person came for.
    looking.sort_by(|a, b| b.day.cmp(&a.day));
    Ok(looking)
}

fn years_between(day: &str, today: NaiveDate) -> Option<i32> {
    let then = when::parse(day)?.from;
    Some(today.year() - then.year()).filter(|years| *years > 0)
}

/// The one event of this day that can be quoted, and its quote.
///
/// Whatever the day held, only something with words in it can be shown, so the
/// search is for a quote first and an event second.
fn quotable<'a>(cache: &DbBridge, held: &'a [Event]) -> Option<(&'a Event, String)> {
    let mut a_picture = None;
    for event in held.iter().filter(|e| e.shape.is_a_memory()) {
        let source = event.container_node.as_deref().unwrap_or(&event.node_id);
        if let Some(quote) = read_quote(cache, source) {
            return Some((event, quote));
        }
        // §7.1 admits a day on a photograph alone. Its caption is what the
        // person wrote about it, and a picture with nothing written on it has
        // nothing to say.
        if event.node_type == "file" && a_picture.is_none() {
            let caption = event.title.trim();
            if caption.chars().count() >= 12 {
                a_picture = Some((event, caption.to_string()));
            }
        }
    }
    a_picture
}

fn read_quote(cache: &DbBridge, node_id: &str) -> Option<String> {
    let content: String = cache
        .conn()
        .query_row(
            "SELECT COALESCE(content, '') FROM nodes WHERE id = ?1 OR stable_id = ?1",
            rusqlite::params![node_id],
            |row| row.get(0),
        )
        .ok()?;
    first_sentences(&content)
}

/// One or two sentences of the person's own prose, verbatim.
///
/// Markdown furniture is stepped over rather than cleaned up: a heading, a
/// checkbox, a quote marker or a bare link is not a sentence anybody wrote to
/// be read back to them. What is returned is a slice of the file, never
/// anything assembled — the moment this starts composing, §6.1 is broken.
pub(crate) fn first_sentences(content: &str) -> Option<String> {
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| is_prose(line))?;

    let mut out = take_sentence(line)?;
    if out.chars().count() < ENOUGH_WRITING {
        // One short sentence is a jotting; with the next one it is a thought.
        if let Some(more) = take_sentence(line[out.len()..].trim_start()) {
            let joined = format!("{out} {more}");
            if joined.chars().count() <= LONGEST_QUOTE {
                out = joined;
            }
        }
    }
    if !stands_alone(&out) {
        return None;
    }
    Some(cut(&out))
}

/// Whether a run of words can be handed back on its own.
///
/// A fragment with no full stop is a title line, a label or the stub of a list
/// unless it is long enough to be a thought. The real vault offered
/// «Nhận cảnh báo:» as a memory before this was here.
pub(crate) fn stands_alone(text: &str) -> bool {
    let ends_properly = text.ends_with(['.', '!', '?', '…', '"', '»', '”']);
    text.chars().count() >= 12 && (ends_properly || text.chars().count() >= ENOUGH_WRITING)
}

/// Every sentence of a note that is prose, in the order they were written.
///
/// The same reading of "prose" as [`first_sentences`], deliberately: two
/// features that disagreed about what counts as a sentence would quote the
/// same note differently, and the person would see the seam.
///
/// One thing differs, and it has to. [`first_sentences`] trims an over-long
/// quote and marks the trim with an ellipsis, which is honest when the app is
/// showing one line of a long day. Here an over-long sentence is **dropped**
/// instead, because §16 Bước 6 asks that every line of a year match the vault
/// byte for byte and an ellipsis is a character the person did not write. The
/// real vault caught this: a pasted SharePoint link came back trimmed, and so
/// matched nothing. A year is fifteen sentences out of hundreds; declining the
/// unwieldy ones costs nothing.
pub(crate) fn sentences(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines().map(str::trim).filter(|line| is_prose(line)) {
        let mut rest = line;
        while let Some(sentence) = take_sentence(rest) {
            rest = rest[sentence.len()..].trim_start();
            if stands_alone(&sentence) && sentence.chars().count() <= LONGEST_QUOTE {
                out.push(sentence);
            }
            if rest.is_empty() {
                break;
            }
        }
    }
    out
}

fn is_prose(line: &str) -> bool {
    if line.chars().count() < 12 {
        return false;
    }
    // `<` is here because a note written in the editor keeps its HTML: the
    // real vault offered a quote that began with an `<img>` tag, width and
    // all. Quoting verbatim means the line has to *be* prose to begin with.
    let furniture = ['#', '>', '|', '-', '*', '+', '`', '!', '[', '<'];
    if line.starts_with(|c| furniture.contains(&c)) || line.starts_with("---") {
        return false;
    }
    // A line that introduces a list is not a sentence about anybody's day,
    // and a pasted link is not a sentence at all.
    if line.ends_with(':') || line.starts_with("http") {
        return false;
    }
    // A line that is only a property, `key: value`, is frontmatter that
    // survived or a field, not prose.
    let looks_like_a_field = line
        .split_once(": ")
        .is_some_and(|(key, _)| !key.contains(' ') && key.chars().all(|c| c.is_alphanumeric() || c == '_'));
    !looks_like_a_field && line.chars().any(char::is_alphabetic)
}

/// The first sentence of `text`, with its full stop, or the whole of it.
fn take_sentence(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let end = text
        .char_indices()
        .find(|(i, c)| {
            matches!(c, '.' | '!' | '?' | '…')
                // Not a decimal point, an ellipsis of dots, or an abbreviation
                // run: the sentence ends where a space follows.
                && text[i + c.len_utf8()..].starts_with(|next: char| next.is_whitespace())
        })
        .map(|(i, c)| i + c.len_utf8());
    Some(text[..end.unwrap_or(text.len())].trim().to_string())
}

/// Cut an over-long quote at a word, marking that it was cut.
///
/// Trimming is still quoting; the mark is what keeps it honest.
fn cut(text: &str) -> String {
    if text.chars().count() <= LONGEST_QUOTE {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(LONGEST_QUOTE).collect();
    if let Some(space) = kept.rfind(char::is_whitespace) {
        kept.truncate(space);
    }
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::models::node::NodeMetadata;
    use crate::timeline::store::catch_up;

    fn note(id: &str, title: &str, content: &str, properties: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: "note".to_string(),
            title: title.to_string(),
            content: content.to_string(),
            properties,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn a_quote_is_a_slice_of_what_was_written() {
        let quote = first_sentences(
            "Sáng nay đi bộ với bố, ông kể chuyện năm 54. Trời lạnh hơn mọi khi.",
        )
        .unwrap();
        assert_eq!(quote, "Sáng nay đi bộ với bố, ông kể chuyện năm 54.");
    }

    #[test]
    fn a_short_sentence_brings_the_next_one_with_it() {
        let quote = first_sentences("Hôm nay mệt quá. Không làm được gì cả.").unwrap();
        assert_eq!(quote, "Hôm nay mệt quá. Không làm được gì cả.");
    }

    #[test]
    fn headings_checkboxes_and_fields_are_stepped_over() {
        let quote = first_sentences(
            "# Ngày 14 tháng 5\n\n- [x] gửi báo cáo quý\ntags: work, family\n> trích của ai đó\n\nCuối cùng cũng nói được với mẹ chuyện chuyển nhà.",
        )
        .unwrap();
        assert_eq!(quote, "Cuối cùng cũng nói được với mẹ chuyện chuyển nhà.");
    }

    #[test]
    fn a_line_that_introduces_a_list_is_not_a_memory() {
        // Straight from the real vault, where this was offered as one.
        assert!(first_sentences("Nhận cảnh báo:\n- CPU 91%\n- RAM 87%").is_none());
    }

    #[test]
    fn a_fragment_with_no_full_stop_has_to_earn_its_place() {
        assert!(first_sentences("Gửi báo cáo").is_none(), "too short and unfinished");
        assert!(
            first_sentences("Cam tập ăn miếng táo đầu tiên").is_none(),
            "a title line without a full stop is the note's name, not its prose"
        );
        assert!(
            first_sentences("Hôm nay cả nhà đi Times City và Cam thích cái hồ cá ở tầng hầm")
                .is_some(),
            "long enough to be a sentence somebody wrote"
        );
    }

    #[test]
    fn a_picture_tag_is_not_the_start_of_a_sentence() {
        // Straight from the real vault, where this was offered as a memory.
        let content = "<img src=\"assets/1777860790-image.png\" alt=\"image.png\" width=\"574.2\" height=\"842\" />`Echina`: đây là thực phẩm chức năng.\nHôm nay con đã chịu uống thuốc mà không khóc.";
        assert_eq!(
            first_sentences(content).unwrap(),
            "Hôm nay con đã chịu uống thuốc mà không khóc."
        );
    }

    #[test]
    fn a_note_with_nothing_but_furniture_has_nothing_to_quote() {
        assert!(first_sentences("# Thứ hai\n\n- [x] họp\n- [x] gửi mail\n").is_none());
        assert!(first_sentences("").is_none());
    }

    #[test]
    fn a_decimal_point_does_not_end_a_sentence() {
        let quote = first_sentences("Chạy được 5.2km sáng nay, lần đầu không phải nghỉ.").unwrap();
        assert_eq!(quote, "Chạy được 5.2km sáng nay, lần đầu không phải nghỉ.");
    }

    #[test]
    fn an_over_long_sentence_is_cut_at_a_word_and_says_so() {
        let long = format!("Hôm nay {}", "chuyện dài ".repeat(40));
        let quote = first_sentences(&long).unwrap();
        assert!(quote.ends_with('…'), "{quote}");
        assert!(quote.chars().count() <= LONGEST_QUOTE + 1);
        assert!(long.starts_with(quote.trim_end_matches('…').trim_end()), "still verbatim");
    }

    #[test]
    fn a_quote_is_never_assembled_from_two_places() {
        let content = "Câu đầu tiên ở đây và nó khá dài.\n\nMột đoạn khác hoàn toàn.";
        let quote = first_sentences(content).unwrap();
        assert!(content.contains(&quote), "the quote is a slice of the file: {quote}");
    }

    #[test]
    fn years_are_counted_only_backwards() {
        assert_eq!(years_between("2020-05-14", day("2026-05-14")), Some(6));
        assert_eq!(years_between("2026-05-14", day("2026-05-14")), None);
        assert_eq!(years_between("2030-05-14", day("2026-05-14")), None);
    }

    /// §16 Bước 4's gate, end to end: everything offered quotes a real
    /// sentence; nothing sealed or hushed gets through; and a day of nothing
    /// but finished tasks is never offered.
    #[test]
    fn nothing_reaches_the_screen_without_a_quote_and_without_consent() {
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        for n in [
            note(
                "Notes/2020-05-14.md",
                "2020-05-14",
                "Sáng nay đi bộ với bố, ông kể chuyện năm 54. Trời lạnh hơn mọi khi.",
                json!({ "date": "2020-05-14" }),
            ),
            // A day of nothing but work: §7.1 refuses it outright.
            note(
                "Notes/2021-05-14.md",
                "2021-05-14",
                "- [x] gửi báo cáo quý\n- [x] họp với khách",
                json!({ "date": "2021-05-14", "tasks_done": ["gửi báo cáo quý"] }),
            ),
            // Written, but nothing a person would want read back to them.
            note(
                "Notes/2022-05-14.md",
                "2022-05-14",
                "# Thứ bảy\n\n- [x] đi chợ\n",
                json!({ "date": "2022-05-14" }),
            ),
            note(
                "Notes/2023-05-14.md",
                "2023-05-14",
                "Hôm nay quyết định nghỉ việc, để được yên hơn một chút.",
                json!({ "date": "2023-05-14" }),
            ),
            note(
                "Notes/2024-05-14.md",
                "2024-05-14",
                "Cuối cùng cũng nói được với mẹ chuyện chuyển nhà.",
                json!({ "date": "2024-05-14" }),
            ),
        ] {
            db.upsert_node(&n).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        let today = day("2026-05-14");

        let db = cache.lock().unwrap();
        let open = look_back(&timeline, &db, today, &Quiet::default()).unwrap();

        let days: Vec<&str> = open.iter().map(|l| l.day.as_str()).collect();
        assert_eq!(
            days,
            ["2024-05-14", "2023-05-14", "2020-05-14"],
            "a day of only tasks and a day with nothing to quote are both out"
        );
        assert_eq!(open[0].years_ago, 2);
        for looking in &open {
            assert!(!looking.quote.is_empty(), "every line quotes: {looking:?}");
            assert!(!looking.node_id.is_empty(), "and leads back: {looking:?}");
        }

        // Now refuse two of them, one each way.
        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_str().unwrap();
        quiet::write_hush(
            vault_path,
            &quiet::Subject::Moment {
                node: "Notes/2024-05-14.md".into(),
                day: "2024-05-14".into(),
            },
            Some("2027-05-14"),
        )
        .unwrap();
        let quiet = Quiet::read(&db, vault_path, "2026-05-14").unwrap();

        let left = look_back(&timeline, &db, today, &quiet).unwrap();
        let days: Vec<&str> = left.iter().map(|l| l.day.as_str()).collect();
        assert!(!days.contains(&"2024-05-14"), "what was hushed is gone: {days:?}");
    }

    /// §16 Bước 4's gate on the real vault, read only: walk every day of the
    /// year and check the three promises against what a person actually wrote.
    ///
    ///   SYN_PROBE_CACHE=/path/to/a/copy/of/vault_cache.db \
    ///     cargo test --lib -- --ignored walking_a_real_year --nocapture
    #[test]
    #[ignore = "needs a copy of a real vault cache; run by hand"]
    fn walking_a_real_year() {
        let Ok(path) = std::env::var("SYN_PROBE_CACHE") else {
            panic!("set SYN_PROBE_CACHE to a copy of a vault_cache.db");
        };
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = crate::db::DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        let built = catch_up(&cache, &mut timeline).expect("built from the vault");
        let db = cache.lock().unwrap();

        // A vault only a year old has no earlier year, so walking *this* year
        // proves nothing. Walk the year after the last thing in it: that is
        // what the feature will say on this person's real writing, the first
        // time it has anything to say at all.
        let year: i32 = timeline
            .all_items(NaiveDate::from_ymd_opt(9999, 12, 31).unwrap())
            .unwrap()
            .iter()
            .filter_map(|e| e.happened_from.get(..4).and_then(|y| y.parse::<i32>().ok()))
            .max()
            .expect("something in the vault has a date")
            + 1;
        let (mut days_with, mut offered, mut without_quote) = (0, 0, 0);
        let mut shown: Vec<Looking> = Vec::new();
        let mut at = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
        while at <= end {
            let found =
                look_back(&timeline, &db, at, &Quiet::default()).unwrap();
            if !found.is_empty() {
                days_with += 1;
                offered += found.len();
                without_quote += found.iter().filter(|l| l.quote.trim().is_empty()).count();
                shown.extend(found);
            }
            at = at.succ_opt().unwrap();
        }

        eprintln!(
            "\n═══ on this day, across {year} ═══  {} nodes, {} events",
            built.nodes_read, built.items
        );
        eprintln!("  days that would say something: {days_with} of 365");
        eprintln!("  things offered:                {offered}");
        eprintln!("  offered without a quote:       {without_quote}");
        for looking in shown.iter().take(12) {
            eprintln!("  {} ({}y) «{}»", looking.day, looking.years_ago, looking.quote);
        }

        assert_eq!(without_quote, 0, "§7.1: everything shown quotes the source");
        assert!(
            shown.iter().all(|l| !l.node_id.is_empty()),
            "§10: every line leads back somewhere"
        );
    }

    #[test]
    fn a_day_is_not_its_own_anniversary() {
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&note(
            "Notes/2026-05-14.md",
            "2026-05-14",
            "Hôm nay trời đẹp và tao đã đi bộ rất lâu quanh hồ.",
            json!({ "date": "2026-05-14" }),
        ))
        .unwrap();
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let db = cache.lock().unwrap();
        let open =
            look_back(&timeline, &db, day("2026-05-14"), &Quiet::default())
                .unwrap();
        assert!(open.is_empty(), "{open:?}");
    }
}
