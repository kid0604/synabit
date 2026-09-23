//! Whether a question is about a time, and which.
//!
//! A question about a time is answered from the timeline before the model is
//! asked, instead of waiting for the model to think of a tool: `recall` went
//! uncalled in fifteen runs out of fifteen (`docs/adr-memory-shape-2026-09-04.md`).
//! That means reading the time off the question without a model. Asking a model
//! whether to make a model call is the cost `tempo` exists to remove.
//!
//! # What is read
//!
//! A day, a month or a year written out ("14/5/2016", "tháng 5/2016", "May
//! 2016", "năm 2019"), and the relative ones people say ("hôm qua", "tuần
//! trước", "năm ngoái", "3 năm trước", "last month", "dạo này"), with or
//! without Vietnamese diacritics. A date with slashes is read day first, the
//! way it is written in Vietnamese.
//!
//! # What is not
//!
//! A time with no anchor ("hồi đó", "dạo ấy") is not a time. Guessing one would
//! put the wrong days in front of the model with the authority of having been
//! looked up.
//!
//! How well this does is measured, and the measurement was predicted before it
//! was taken: `docs/eval-timeline-questions-2026-09-15.md`.

use std::sync::LazyLock;

use chrono::{Datelike, Duration, Months, NaiveDate};
use regex::{Captures, Regex};

use super::store::{self, Event};
use super::when::{self, Precision, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub span: Span,
    /// The words the time was read from, as they appear lowercased.
    pub phrase: String,
}

type Resolve = fn(&Captures, NaiveDate) -> Option<Span>;

const THANG: &str = "th(?:á|a)ng";
const NAM: &str = "n(?:ă|a)m";
const TRUOC: &str = "tr(?:ư|u)(?:ớ|o)c";
const NGOAI: &str = "ngo(?:á|a)i";
const TUAN: &str = "tu(?:ầ|a)n";
const NAY: &str = "n(?:à|a)y";
const HOM: &str = "h(?:ô|o)m";
const NGAY: &str = "ng(?:à|a)y";
const COUNT_WORD: &str = "one|two|three|four|five|six|seven|eight|nine|ten";
/// Vietnamese counts, with and without marks. `năm` is five as well as year:
/// "năm năm trước" is five years ago.
const COUNT_VI: &str = "một|mot|hai|ba|bốn|bon|tư|năm|nam|sáu|sau|bảy|bay|tám|tam|chín|chin|mười|muoi";

fn number(text: &str) -> Option<u32> {
    let vietnamese = |word: &str| match word {
        "một" | "mot" => Some(1),
        "hai" => Some(2),
        "ba" => Some(3),
        "bốn" | "bon" | "tư" => Some(4),
        "năm" | "nam" => Some(5),
        "sáu" | "sau" => Some(6),
        "bảy" | "bay" => Some(7),
        "tám" | "tam" => Some(8),
        "chín" | "chin" => Some(9),
        "mười" | "muoi" => Some(10),
        _ => None,
    };
    text.parse().ok().or_else(|| vietnamese(text)).or_else(|| {
        COUNT_WORD
            .split('|')
            .position(|word| word == text)
            .map(|i| i as u32 + 1)
    })
}

fn day(y: &str, m: &str, d: &str) -> Option<Span> {
    Some(Span::day(NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)?))
}

fn month(y: i32, m: u32) -> Option<Span> {
    if !(1900..=2200).contains(&y) {
        return None;
    }
    when::parse(&format!("{y:04}-{m:02}"))
}

fn year(y: i32) -> Option<Span> {
    if !(1900..=2200).contains(&y) {
        return None;
    }
    when::parse(&format!("{y:04}"))
}

/// A stretch of days, cut off at today: the timeline is history.
fn range(from: NaiveDate, to: NaiveDate, today: NaiveDate) -> Option<Span> {
    let to = to.min(today);
    (from <= to).then_some(Span {
        from,
        to,
        precision: Precision::Range,
    })
}

fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).expect("every month has a first")
}

fn monday_of(date: NaiveDate) -> NaiveDate {
    date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

fn month_number(name: &str) -> Option<u32> {
    let names = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    names.iter().position(|n| name.starts_with(n)).map(|i| i as u32 + 1)
}

static VAGUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:mấy|may|vài|vai|nhiều|nhieu|several|a few|many)\s+(?:năm|nam|tháng|thang|tuần|tuan|ngày|ngay|years?|months?|weeks?|days?)\b")
        .expect("a valid pattern")
});

/// The patterns, most specific first: "3 năm trước" must be read before
/// "năm trước" finds the second half of it.
static RULES: LazyLock<Vec<(Regex, Resolve)>> = LazyLock::new(|| {
    let rule = |pattern: String, resolve: Resolve| (Regex::new(&pattern).expect("a valid pattern"), resolve);
    vec![
        rule(r"\b(\d{4})-(\d{2})-(\d{2})\b".into(), |c, _| day(&c[1], &c[2], &c[3])),
        rule(r"\b(\d{1,2})[/.\-](\d{1,2})[/.\-](\d{4})\b".into(), |c, _| day(&c[3], &c[2], &c[1])),
        rule(
            format!(r"\b{NGAY}\s*(\d{{1,2}})\s*{THANG}\s*(\d{{1,2}})\s*(?:{NAM}\s*)?(\d{{4}})\b"),
            |c, _| day(&c[3], &c[2], &c[1]),
        ),
        rule(
            format!(r"\b{THANG}\s*(\d{{1,2}})\s*(?:[/,\-]\s*|\s+{NAM}\s+|\s+)(\d{{4}})\b"),
            |c, _| month(c[2].parse().ok()?, c[1].parse().ok()?),
        ),
        rule(r"\b(\d{1,2})/(\d{4})\b".into(), |c, _| month(c[2].parse().ok()?, c[1].parse().ok()?)),
        rule(r"\b(\d{4})-(\d{2})\b".into(), |c, _| month(c[1].parse().ok()?, c[2].parse().ok()?)),
        rule(
            r"\b(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+(?:of\s+)?(\d{4})\b".into(),
            |c, _| month(c[2].parse().ok()?, month_number(&c[1])?),
        ),
        rule(
            format!(r"\b(\d{{1,2}}|{COUNT_VI})\s*{NAM}\s*{TRUOC}|\b(\d{{1,2}}|{COUNT_WORD})\s+years?\s+ago\b"),
            |c, today| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                year(today.year() - n as i32)
            },
        ),
        rule(
            format!(r"\b(\d{{1,2}}|{COUNT_VI})\s*{THANG}\s*{TRUOC}|\b(\d{{1,2}}|{COUNT_WORD})\s+months?\s+ago\b"),
            |c, today| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                let target = first_of_month(today).checked_sub_months(Months::new(n))?;
                month(target.year(), target.month())
            },
        ),
        rule(
            format!(r"\b(\d{{1,2}}|{COUNT_VI})\s*{TUAN}\s*{TRUOC}|\b(\d{{1,2}}|{COUNT_WORD})\s+weeks?\s+ago\b"),
            |c, today| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                let monday = monday_of(today) - Duration::weeks(i64::from(n));
                range(monday, monday + Duration::days(6), today)
            },
        ),
        rule(
            format!(r"\b(\d{{1,2}}|{COUNT_VI})\s*{NGAY}\s*{TRUOC}|\b(\d{{1,2}}|{COUNT_WORD})\s+days?\s+ago\b"),
            |c, today| {
                let n = number(c.get(1).or(c.get(2))?.as_str())?;
                Some(Span::day(today - Duration::days(i64::from(n))))
            },
        ),
        rule(
            format!(r"\b{NAM}\s*(\d{{4}})\b|\b(?:in|during|since|back in)\s+(\d{{4}})\b"),
            |c, _| year(c.get(1).or(c.get(2))?.as_str().parse().ok()?),
        ),
        rule(format!(r"\b{HOM}\s*kia\b|\bday before yesterday\b"), |_, today| {
            Some(Span::day(today - Duration::days(2)))
        }),
        rule(format!(r"\b{HOM}\s*qua\b|\byesterday\b"), |_, today| {
            Some(Span::day(today - Duration::days(1)))
        }),
        rule(format!(r"\b{TUAN}\s*{TRUOC}\b|\blast week\b"), |_, today| {
            let monday = monday_of(today);
            range(monday - Duration::days(7), monday - Duration::days(1), today)
        }),
        rule(format!(r"\b{TUAN}\s*{NAY}\b|\bthis week\b"), |_, today| {
            range(monday_of(today), today, today)
        }),
        rule(format!(r"\b{THANG}\s*{TRUOC}\b|\blast month\b"), |_, today| {
            let last = first_of_month(today).checked_sub_months(Months::new(1))?;
            month(last.year(), last.month())
        }),
        rule(format!(r"\b{THANG}\s*{NAY}\b|\bthis month\b"), |_, today| {
            range(first_of_month(today), today, today)
        }),
        rule(format!(r"\b{NAM}\s*(?:{NGOAI}|{TRUOC})\b|\blast year\b"), |_, today| {
            year(today.year() - 1)
        }),
        rule(format!(r"\b{NAM}\s*nay\b|\bthis year\b"), |_, today| {
            range(NaiveDate::from_ymd_opt(today.year(), 1, 1)?, today, today)
        }),
        rule(
            format!(r"\bd(?:ạ|a)o\s*{NAY}\b|\bg(?:ầ|a)n\s*(?:đ|d)(?:â|a)y\b|\brecently\b|\blately\b|\bthese days\b"),
            |_, today| range(today - Duration::days(30), today, today),
        ),
        // A bare year only where a time is being spoken of: after a word that
        // places things in time, or ending the question. "ngân sách 2000 đô"
        // and "2000 notes" are amounts.
        rule(
            r"\b(?:hồi|hoi|từ|đến|den|tới|trong|vào|vao|cuối|cuoi|giữa|giua|khoảng|khoang|until|by|before|after|since|around)\s+(19[5-9]\d|20\d{2})\b|\b(19[5-9]\d|20\d{2})\s*[?.!]?\s*$"
                .into(),
            |c, today| {
            let y: i32 = c.get(1).or(c.get(2))?.as_str().parse().ok()?;
            (y <= today.year()).then(|| year(y)).flatten()
            },
        ),
    ]
});

/// The time a question is about, if it names one.
pub fn span_in(question: &str, today: NaiveDate) -> Option<Asked> {
    let text = question.to_lowercase();
    // "mấy năm trước" is some years ago, which is no span at all, and must not
    // fall through to the rule for last year.
    if VAGUE.is_match(&text) {
        return None;
    }
    RULES.iter().find_map(|(pattern, resolve)| {
        let captures = pattern.captures(&text)?;
        let span = resolve(&captures, today)?;
        Some(Asked {
            span,
            phrase: captures[0].trim().to_string(),
        })
    })
}

fn kind_words(kind: &str) -> &str {
    match kind {
        "moment" => "happened",
        "note" => "note",
        "event" => "event",
        "task_done" => "task finished",
        "interaction" => "met",
        "experience" => "job",
        "connection" => "relationship",
        "birthday" => "born",
        "important_date" => "date to remember",
        "death" => "died",
        "media" => "picture",
        "project_start" => "project began",
        _ => "date",
    }
}

/// The most items put in front of the model. Past this the block says how many
/// more there are and where to read them, rather than growing without limit.
const SHOWN: usize = 40;

/// The prompt section for a question about a time.
pub fn block(
    asked: &Asked,
    items: &[Event],
    names: &std::collections::HashMap<String, String>,
) -> String {
    let open = when::iso(when::open_end());
    // The first forty as given, which is the store's order: most precisely
    // known first. Sorted by date before the cut, a vault with fifty
    // relationships going on for years would fill the list with them and
    // leave out what happened on the days asked about.
    let mut ordered: Vec<&Event> = items.iter().take(SHOWN).collect();
    ordered.sort_by(|a, b| a.happened_from.cmp(&b.happened_from).then_with(|| a.title.cmp(&b.title)));

    let mut out = format!(
        "=== FROM THE TIMELINE: \"{}\", {} to {} ===\n",
        asked.phrase,
        when::iso(asked.span.from),
        when::iso(asked.span.to)
    );
    out.push_str(
        "Looked up before you were asked. This is every dated thing the vault holds for these \
         days: daily notes, events, finished tasks, meetings with people, jobs and relationships \
         that were going on, pictures by their day. Each line ends with who was there and where, \
         when the vault says. It is complete for what the vault dates to these days: if it is \
         short or empty, say so plainly rather than filling the gap. Anything else the question \
         needs may still come from the rest of the context. Cite what you use as [[Title]].\n",
    );
    if ordered.is_empty() {
        out.push_str("The vault dates nothing to these days.\n");
    }
    for item in &ordered {
        let when_text = if item.happened_from == item.happened_to {
            item.happened_from.clone()
        } else if item.happened_to == open {
            format!("{} → now", item.happened_from)
        } else {
            format!("{} → {}", item.happened_from, item.happened_to)
        };

        // Who was there and where. This is the half that was missing: a
        // question like "họp với ai hồi tháng 5" cannot be answered off a list
        // that never says who was at anything.
        let cast = |role: &str, word: &str| {
            let found = store::named(&item.links, role, names);
            if found.is_empty() {
                String::new()
            } else {
                format!(" · {word} {}", found.join(", "))
            }
        };
        out.push_str(&format!(
            "- {when_text} · {} · [[{}]]{}{}\n",
            kind_words(&item.kind),
            item.title,
            cast("with", "with"),
            cast("where", "at"),
        ));
    }
    if items.len() > SHOWN {
        out.push_str(&format!(
            "…and {} more. The `timeline` tool with this range lists them, sixty at a time.\n",
            items.len() - SHOWN
        ));
    }
    out.push_str("=== END TIMELINE ===\n\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        // A Tuesday.
        NaiveDate::from_ymd_opt(2026, 9, 15).unwrap()
    }

    fn d(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn read(question: &str) -> Option<(String, String)> {
        span_in(question, today()).map(|a| (when::iso(a.span.from), when::iso(a.span.to)))
    }

    #[test]
    fn a_written_date_is_read_day_first() {
        assert_eq!(read("hồi 14/5/2016 tao ở đâu"), Some(("2016-05-14".into(), "2016-05-14".into())));
        assert_eq!(read("ngày 14 tháng 5 năm 2016"), Some(("2016-05-14".into(), "2016-05-14".into())));
        assert_eq!(read("what happened on 2016-05-14"), Some(("2016-05-14".into(), "2016-05-14".into())));
    }

    #[test]
    fn a_relative_time_is_counted_back_from_today() {
        assert_eq!(read("hôm qua có chuyện gì"), Some(("2026-09-14".into(), "2026-09-14".into())));
        assert_eq!(read("tuần trước tao làm gì"), Some(("2026-09-07".into(), "2026-09-13".into())));
        assert_eq!(read("tháng này"), Some(("2026-09-01".into(), "2026-09-15".into())));
        assert_eq!(read("6 tháng trước tao gặp ai"), Some(("2026-03-01".into(), "2026-03-31".into())));
        assert_eq!(read("3 năm trước"), Some(("2023-01-01".into(), "2023-12-31".into())), "not 'last year'");
    }

    #[test]
    fn a_count_in_words_or_a_vague_one_is_read_as_said() {
        assert_eq!(read("hai năm trước tao ở đâu"), Some(("2024-01-01".into(), "2024-12-31".into())));
        assert_eq!(read("năm năm trước"), Some(("2021-01-01".into(), "2021-12-31".into())));
        assert_eq!(read("ba tháng trước"), Some(("2026-06-01".into(), "2026-06-30".into())));
        assert_eq!(read("2 tuần trước tao làm gì"), Some(("2026-08-31".into(), "2026-09-06".into())));
        assert_eq!(read("3 ngày trước"), Some(("2026-09-12".into(), "2026-09-12".into())));
        assert_eq!(read("mấy năm trước tao hay đi đâu"), None, "not last year");
        assert_eq!(read("vài tuần trước"), None);
    }

    #[test]
    fn an_amount_is_not_a_year() {
        assert_eq!(read("ngân sách 2000 đô có đủ không"), None);
        assert_eq!(read("summary of 2000 notes"), None);
        assert_eq!(read("tỉ lệ 3/1000 là bao nhiêu"), None);
        assert_eq!(read("hồi 2012 tao học ở đâu"), Some(("2012-01-01".into(), "2012-12-31".into())));
        assert_eq!(read("chuyện gì xảy ra 2016?"), Some(("2016-01-01".into(), "2016-12-31".into())));
    }

    #[test]
    fn the_list_keeps_the_days_asked_about_when_long_relationships_would_fill_it() {
        let asked = span_in("tuần trước", today()).unwrap();
        let long = |n: usize| Event {
            id: format!("c{n}"),
            kind: "connection".into(),
            node_id: format!("People/p{n}.md"),
            node_type: "person".into(),
            title: format!("Người {n}"),
            node_title: String::new(),
            links: Vec::new(),
            magnitude: 0.0,
            container_node: None,
            props: serde_json::Value::Null,
            happened_from: "2009-09-01".into(),
            happened_to: "9999-12-31".into(),
            precision: "range".into(),
            time_source: "frontmatter".into(),
            source: "derived".into(),
            shape: crate::timeline::derive::Shape::Occasion,
        };
        let day_note = Event {
            id: "n".into(),
            kind: "note".into(),
            node_id: "Notes/2026-09-09.md".into(),
            node_type: "note".into(),
            title: "Đi khám".into(),
            node_title: String::new(),
            links: Vec::new(),
            magnitude: 0.0,
            container_node: None,
            props: serde_json::Value::Null,
            happened_from: "2026-09-09".into(),
            happened_to: "2026-09-09".into(),
            precision: "day".into(),
            time_source: "frontmatter".into(),
            source: "derived".into(),
            shape: crate::timeline::derive::Shape::Occasion,
        };
        // The store's order: the day first, then the long spans.
        let mut items = vec![day_note];
        items.extend((0..50).map(long));
        let written = block(&asked, &items, &Default::default());
        assert!(written.contains("[[Đi khám]]"), "{written}");
        assert!(written.contains("…and 11 more"), "{written}");
    }

    #[test]
    fn a_time_with_no_anchor_is_not_a_time() {
        assert_eq!(read("hồi đó tao nghĩ gì"), None);
        assert_eq!(read("dạo ấy thế nào"), None);
    }

    #[test]
    fn the_block_says_when_there_is_nothing_and_cites_by_title() {
        let asked = span_in("tháng 2/2017 có gì", today()).unwrap();
        assert!(block(&asked, &[], &Default::default()).contains("The vault dates nothing to these days."));

        let item = Event {
            id: "x".into(),
            kind: "note".into(),
            node_id: "Notes/d.md".into(),
            node_type: "note".into(),
            title: "Đám cưới".into(),
            node_title: String::new(),
            links: Vec::new(),
            magnitude: 0.0,
            container_node: None,
            props: serde_json::Value::Null,
            happened_from: "2017-02-14".into(),
            happened_to: "2017-02-14".into(),
            precision: "day".into(),
            time_source: "frontmatter".into(),
            source: "derived".into(),
            shape: crate::timeline::derive::Shape::Occasion,
        };
        let written = block(&asked, &[item], &Default::default());
        assert!(written.contains("- 2017-02-14 · note · [[Đám cưới]]"), "{written}");
        assert!(written.ends_with("=== END TIMELINE ===\n\n"));
    }

    /// The gate for Bước 6. "họp với ai hồi tháng 5" can only be answered off
    /// a list that says who was at things — which, until the event model, it
    /// never did: one meeting held one name, and most held none.
    #[test]
    fn the_block_says_who_was_there_and_where_it_was() {
        let asked = span_in("họp với ai hồi tháng 5/2016", today()).expect("a time");
        let meeting = Event {
            id: "Notes/2016-05-14.md#moment#0".into(),
            kind: "moment".into(),
            node_id: "Notes/2016-05-14.md".into(),
            node_type: "note".into(),
            title: "Họp dự án".into(),
            node_title: "2016-05-14".into(),
            links: vec![
                store::EventLink { node_id: "uuid-khanh".into(), role: "with".into(), label: None },
                store::EventLink { node_id: "uuid-hai".into(), role: "with".into(), label: None },
                store::EventLink { node_id: "Tuần Châu".into(), role: "where".into(), label: None },
            ],
            happened_from: "2016-05-14".into(),
            happened_to: "2016-05-14".into(),
            precision: "day".into(),
            time_source: "user".into(),
            source: "derived".into(),
            shape: crate::timeline::derive::Shape::Occasion,
            magnitude: 4.0,
            container_node: Some("Notes/2016-05-14.md".into()),
            props: serde_json::Value::Null,
        };
        let names = std::collections::HashMap::from([
            ("uuid-khanh".to_string(), "Khánh".to_string()),
            ("uuid-hai".to_string(), "Hải".to_string()),
        ]);

        let written = block(&asked, &[meeting], &names);
        assert!(written.contains("with Khánh, Hải"), "{written}");
        assert!(
            written.contains("at Tuần Châu"),
            "a place nobody made a node for is still a place: {written}"
        );
    }

    /// What a question is about, if anything, and what counts as right.
    enum Expect {
        /// A time: the span a person meant.
        Time(&'static str, &'static str),
        /// Not a question about a time.
        Nothing,
    }

    /// The questions in `docs/eval-timeline-questions-2026-09-15.md`.
    const QUESTIONS: &[(&str, Expect)] = &[
        ("tháng 5/2016 tao làm gì?", Expect::Time("2016-05-01", "2016-05-31")),
        ("Tháng 5 năm 2016 có chuyện gì", Expect::Time("2016-05-01", "2016-05-31")),
        ("hồi 14/5/2016 tao ở đâu", Expect::Time("2016-05-14", "2016-05-14")),
        ("năm 2019 thế nào", Expect::Time("2019-01-01", "2019-12-31")),
        ("năm ngoái tao gặp những ai", Expect::Time("2025-01-01", "2025-12-31")),
        ("năm trước có gì đáng nhớ", Expect::Time("2025-01-01", "2025-12-31")),
        ("tuần trước tao làm gì", Expect::Time("2026-09-07", "2026-09-13")),
        ("tháng trước tao chi tiêu những gì", Expect::Time("2026-08-01", "2026-08-31")),
        ("hôm qua có chuyện gì", Expect::Time("2026-09-14", "2026-09-14")),
        ("3 năm trước giờ này tao đang làm gì", Expect::Time("2023-01-01", "2023-12-31")),
        ("6 tháng trước tao gặp ai", Expect::Time("2026-03-01", "2026-03-31")),
        ("dạo này tao hay gặp ai", Expect::Time("2026-08-16", "2026-09-15")),
        ("what happened in May 2016?", Expect::Time("2016-05-01", "2016-05-31")),
        ("who did I meet last year", Expect::Time("2025-01-01", "2025-12-31")),
        ("what did I do last week", Expect::Time("2026-09-07", "2026-09-13")),
        ("remind me what happened on 2016-05-14", Expect::Time("2016-05-14", "2016-05-14")),
        ("what was going on in 2012", Expect::Time("2012-01-01", "2012-12-31")),
        ("2 years ago what was I doing", Expect::Time("2024-01-01", "2024-12-31")),
        ("thang 5/2016 co gi", Expect::Time("2016-05-01", "2016-05-31")),
        ("nam ngoai toi da lam gi", Expect::Time("2025-01-01", "2025-12-31")),
        ("tháng này tao đã xong những task nào", Expect::Time("2026-09-01", "2026-09-15")),
        ("năm nay tao đã đi đâu", Expect::Time("2026-01-01", "2026-09-15")),
        ("đầu năm 2020 tao làm gì", Expect::Time("2020-01-01", "2020-12-31")),
        ("mùa hè năm ngoái tao đi đâu", Expect::Time("2025-06-01", "2025-08-31")),
        ("có bao nhiêu task chưa xong", Expect::Nothing),
        ("viết lại đoạn này cho gọn", Expect::Nothing),
        ("iPhone 17 ra năm 2025 giá bao nhiêu", Expect::Nothing),
        ("tóm tắt note về pricing", Expect::Nothing),
        ("hồi đó tao nghĩ gì về chuyện này", Expect::Nothing),
        ("summarize this article", Expect::Nothing),
        ("what is the wifi password in the Hanoi office", Expect::Nothing),
        ("tạo task gọi cho mẹ lúc 5 giờ", Expect::Nothing),
        ("đọc hết feed tuần này rồi tổng hợp", Expect::Nothing),
        ("Mai nói gì về pricing", Expect::Nothing),
        ("tao có bao nhiêu tiền trong tài khoản", Expect::Nothing),
        ("draw a diagram of the network", Expect::Nothing),
    ];

    /// The offline half of the eval: does the harness look the time up.
    ///
    /// ```bash
    /// cargo test --lib timeline::asked::tests::which_questions_are_about_a_time -- --nocapture
    /// ```
    #[test]
    fn which_questions_are_about_a_time() {
        let (mut exact, mut partial, mut wrong, mut missed) = (0, 0, 0, 0);
        let (mut quiet, mut false_alarm) = (0, 0);
        eprintln!("\n{:<44} {:<24} {:<24} verdict", "question", "wanted", "read");
        for (question, expect) in QUESTIONS {
            let got = read(question);
            let shown = got.as_ref().map(|(f, t)| format!("{f}..{t}")).unwrap_or_else(|| "-".into());
            let (wanted, verdict) = match expect {
                Expect::Time(from, to) => {
                    let verdict = match &got {
                        Some((f, t)) if f == from && t == to => {
                            exact += 1;
                            "exact"
                        }
                        Some((f, t)) if f.as_str() <= *from && t.as_str() >= *to => {
                            partial += 1;
                            "partial (wider)"
                        }
                        Some(_) => {
                            wrong += 1;
                            "WRONG"
                        }
                        None => {
                            missed += 1;
                            "MISSED"
                        }
                    };
                    (format!("{from}..{to}"), verdict)
                }
                Expect::Nothing => {
                    let verdict = if got.is_some() {
                        false_alarm += 1;
                        "FALSE ALARM"
                    } else {
                        quiet += 1;
                        "quiet"
                    };
                    ("-".to_string(), verdict)
                }
            };
            eprintln!("{:<44} {:<24} {:<24} {verdict}", question.chars().take(43).collect::<String>(), wanted, shown);
        }
        let about_time = exact + partial + wrong + missed;
        eprintln!(
            "\nabout a time: {exact} exact, {partial} partial, {wrong} wrong, {missed} missed of {about_time}\n\
             not about a time: {quiet} quiet, {false_alarm} false alarm(s) of {}\n",
            quiet + false_alarm
        );

        // The gate: the timeline is looked up for at least nine in ten
        // questions about a time, and a wrong span is not looking it up.
        assert!((exact + partial) * 10 >= about_time * 9, "looked up {} of {about_time}", exact + partial);
        // Not the gate. The prediction was at most one false alarm, and the
        // first measurement found two: "đọc hết feed tuần này" names a time
        // that is not a time in the user's life. The prediction stays wrong in
        // the eval doc; this line only stops it getting worse. A false alarm
        // costs one extra section, not a wrong answer.
        assert!(false_alarm <= 2, "{false_alarm} questions not about a time were read as one");
    }

    /// The live half of the eval: given the block, does the model say what the
    /// vault holds for that time and nothing else. Spends real credit.
    ///
    /// ```bash
    /// cargo test --lib timeline::asked::tests::live -- --ignored --nocapture
    /// ```
    mod live {
        use super::super::*;
        use crate::db::DbBridge;
        use crate::models::node::NodeMetadata;
        use crate::syn::prompt::{ChatPrompt, PromptPlan};
        use crate::syn::provider::{ChatMessage, ChatRequest};
        use crate::timeline::store::{catch_up, TimelineStore};
        use serde_json::json;

        fn node(id: &str, node_type: &str, title: &str, properties: serde_json::Value) -> NodeMetadata {
            NodeMetadata {
                id: id.into(),
                node_type: node_type.into(),
                title: title.into(),
                content: String::new(),
                properties,
                created_at: "2026-01-01T12:00:00.000Z".into(),
                updated_at: "2026-01-01T12:00:00.000Z".into(),
                timestamp: 0,
                blocks: None,
            }
        }

        const TITLES: &[&str] = &["Đám cưới", "Cà phê với Tuấn", "Nộp hồ sơ", "Chuyển nhà"];

        /// (question, must mention, must not mention). An empty `must` is a time
        /// the vault holds nothing for.
        const CASES: &[(&str, &[&str], &[&str])] = &[
            ("tháng 5/2016 tao làm gì?", &["Đám cưới", "Tuấn"], &["Chuyển nhà", "Nộp hồ sơ"]),
            ("năm 2019 có chuyện gì?", &["Chuyển nhà"], &["Đám cưới", "Nộp hồ sơ"]),
            ("ngày 14/5/2016 có gì?", &["Đám cưới"], &["Chuyển nhà", "Nộp hồ sơ"]),
            ("tháng 6/2016 tao xong việc gì?", &["Nộp hồ sơ"], &["Đám cưới", "Chuyển nhà"]),
            ("what happened in May 2016?", &["Đám cưới"], &["Chuyển nhà"]),
            ("năm 2016 tao gặp những ai?", &["Tuấn"], &["Chuyển nhà"]),
            ("in 2019 what did I do", &["Chuyển nhà"], &["Đám cưới"]),
            ("tháng 2/2017 có gì?", &[], TITLES),
            ("năm 2021 tao làm gì?", &[], TITLES),
            ("what happened in March 2018?", &[], TITLES),
        ];

        #[tokio::test]
        #[ignore = "spends real API credit and needs a network; run by hand"]
        async fn answers_about_a_time_from_the_timeline_block() {
            let settings = crate::syn::settings::load_settings(
                &std::env::var("SYN_EVAL_VAULT").unwrap_or_else(|_| {
                    format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default())
                }),
            )
            .expect("the real Syn settings");
            let model = settings.default_model.clone().expect("a default model must be configured");
            let provider = crate::syn::provider::for_settings(
                &settings,
                crate::secrets::SecretManager::get_syn_api_key(None, settings.provider.key_slot()),
            );

            let db = DbBridge::new_in_memory_full().expect("schema");
            for n in [
                node("Notes/wedding.md", "note", "Đám cưới", json!({ "date": "2016-05-14" })),
                node("People/tuan.md", "person", "Tuấn", json!({ "node_id": "uuid-tuan" })),
                node("People/Interactions/c.md", "interaction", "Cà phê với Tuấn", json!({ "date": "2016-05-03", "person_id": "uuid-tuan", "interaction_type": "coffee" })),
                node("Tasks/apply.md", "task", "Nộp hồ sơ", json!({ "completed_at": "2016-06-02" })),
                node("Notes/move.md", "note", "Chuyển nhà", json!({ "date": "2019-03-11" })),
            ] {
                db.upsert_node(&n).expect("seeded");
            }
            let cache = std::sync::Mutex::new(db);
            let mut store = TimelineStore::open_in_memory().expect("store");
            catch_up(&cache, &mut store).expect("caught up");
            let today = chrono::Local::now().date_naive();

            let (mut complete, mut invented, mut said_nothing, mut empty_cases) = (0, 0, 0, 0);
            eprintln!("\n═══ answers from the timeline block ═══  model {model}\n");
            for (question, must, must_not) in CASES {
                let asked = span_in(question, today).expect("every case names a time");
                let items = store.query(asked.span, today).expect("query");
                let timeline = block(&asked, &items, &Default::default());
                let system = PromptPlan::for_chat(ChatPrompt {
                    context: "",
                    custom: None,
                    skills: None,
                    memory: None,
                    focus: None,
                    thread: None,
                    counted: None,
                    timeline: Some(&timeline),
                    budget_chars: 60_000,
                })
                .render();
                let messages = vec![ChatMessage::new("system", system), ChatMessage::new("user", question.to_string())];
                let reply = provider
                    .chat(ChatRequest { model: &model, messages: &messages, temperature: Some(settings.temperature), num_ctx: settings.num_ctx, tools: None , json_schema: None})
                    .await
                    .expect("the model answers");
                let answer = reply.content;
                let has_all = must.iter().all(|w| answer.contains(w));
                let has_invented = must_not.iter().any(|w| answer.contains(w));
                if must.is_empty() {
                    empty_cases += 1;
                    if !has_invented {
                        said_nothing += 1;
                    }
                } else if has_all {
                    complete += 1;
                }
                if has_invented {
                    invented += 1;
                }
                eprintln!(
                    "── {question}\n   complete={has_all} invented={has_invented}\n   {}\n",
                    answer.replace('\n', " ").chars().take(300).collect::<String>()
                );
            }
            eprintln!(
                "complete {complete}/{}   invented {invented}/{}   said nothing when there was nothing {said_nothing}/{empty_cases}",
                CASES.len() - empty_cases,
                CASES.len()
            );
        }
    }
}
