//! "Syn kể lại": a few sentences about a relationship, every one resting on a
//! record.
//!
//! The design is §6 of `docs/timeline-2026-09-17.md`: Syn tells, it is not the
//! source, and a memory it made up is worse than a gap.
//!
//! # The rule, enforced by code
//!
//! The model is given numbered records and asked to put the numbers after
//! every sentence. That is a request. This module is the check: a sentence
//! that cites nothing is not shown, and neither is one whose only numbers name
//! no record. How many were left out is reported, so the screen can say so.
//! A reply with nothing left after the check shows nothing, rather than the
//! parts of it that had no source.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::models::node::NodeMetadata;
use crate::timeline::store::{self, Event};
use crate::timeline::when;

/// The most records put in front of the model. The most recent are kept.
pub const MAX_SOURCES: usize = 40;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Source {
    /// The number the model cites it by, from 1.
    pub n: usize,
    pub node_id: String,
    pub node_type: String,
    pub title: String,
    pub date: String,
    pub what: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Sentence {
    /// The sentence, with its citation marks taken out.
    pub text: String,
    pub sources: Vec<usize>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Narrative {
    pub sentences: Vec<Sentence>,
    pub sources: Vec<Source>,
    /// Sentences the model wrote that rested on no record, and were removed.
    pub dropped: usize,
    /// Nothing was asked.
    pub withheld: bool,
}

fn kind_words(kind: &str) -> &str {
    match kind {
        "experience" => "job",
        "connection" => "relationship",
        "moment" => "happened",
        "important_date" => "date to remember",
        "death" => "died",
        "birthday" => "born",
        other => other,
    }
}

/// The records a narrative may rest on, numbered, oldest first.
///
/// Interactions come from their own notes rather than from the timeline,
/// because the note holds what was said and the timeline only that it happened.
pub fn sources_for(
    items: &[Event],
    interactions: &[NodeMetadata],
    names: &std::collections::HashMap<String, String>,
) -> Vec<Source> {
    let open = when::iso(when::open_end());
    let mut rows: Vec<(String, String, String, String, String)> = Vec::new();

    for item in items.iter().filter(|item| item.kind != "interaction") {
        let date = if item.happened_from == item.happened_to {
            item.happened_from.clone()
        } else if item.happened_to == open {
            format!("{} → now", item.happened_from)
        } else {
            format!("{} → {}", item.happened_from, item.happened_to)
        };
        let mut what = if item.title.trim().is_empty() {
            kind_words(&item.kind).to_string()
        } else {
            format!("{}: {}", kind_words(&item.kind), item.title)
        };
        // Who else was in it, and where. A retelling that cannot name the
        // people in a story has to write around them.
        for (role, word) in [("with", "with"), ("where", "at")] {
            let found = store::named(&item.links, role, names);
            if !found.is_empty() {
                what.push_str(&format!(" · {word} {}", found.join(", ")));
            }
        }
        rows.push((date, item.node_id.clone(), item.node_type.clone(), item.title.clone(), what));
    }

    for node in interactions {
        let date = node.properties.get("date").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let kind = node
            .properties
            .get("interaction_type")
            .and_then(|v| v.as_str())
            .unwrap_or("interaction");
        let said: String = node.content.trim().chars().take(280).collect();
        let what = if said.is_empty() { kind.to_string() } else { format!("{kind}: \"{said}\"") };
        rows.push((date, node.id.clone(), node.node_type.clone(), node.title.clone(), what));
    }

    rows.sort();
    let skip = rows.len().saturating_sub(MAX_SOURCES);
    rows.into_iter()
        .skip(skip)
        .enumerate()
        .map(|(i, (date, node_id, node_type, title, what))| Source {
            n: i + 1,
            node_id,
            node_type,
            title,
            date,
            what,
        })
        .collect()
}

/// What the model is asked.
pub fn prompt(name: &str, sources: &[Source], language: &str) -> String {
    let mut out = format!(
        "Write, for the user themself, a short account of their relationship with {name}, in {language}.\n\n\
         Use ONLY the numbered records below. After every sentence, put the numbers of the records it \
         rests on in square brackets, like [2] or [2][5]. A sentence without a number is deleted before \
         anyone reads it, so do not write one. Say nothing no record says: no guesses about feelings, no \
         advice, no summary of what the relationship \"means\". Three to six plain sentences, fewer if \
         the records are few. No heading and no list.\n\nRecords:\n"
    );
    for source in sources {
        out.push_str(&format!("[{}] {} · {} · {}\n", source.n, source.date, source.title, source.what));
    }
    out
}

static CITATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(\d+(?:\s*[,–-]\s*\d+)*)\]").expect("citation pattern"));
/// The end of a sentence: its mark, any closing quote or bracket, and the
/// citations that follow it.
static SENTENCE_END: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[.!?…]["”’)\]]*(?:\s*\[\d+(?:\s*[,–-]\s*\d+)*\])*(?:\s+|$)"#).expect("sentence pattern")
});
static SPACE_BEFORE_MARK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+([.!?…,;:])").expect("mark pattern"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").expect("space pattern"));

/// Words whose full stop does not end a sentence: "TP. HCM", "Dr. Mai".
const ABBREVIATIONS: &[&str] = &[
    "TP", "Tp", "Q", "P", "TX", "TT", "ThS", "TS", "PGS", "GS", "BS", "KS", "Mr", "Mrs", "Ms", "Dr", "St", "Jr", "vs",
    "etc", "e.g", "i.e", "No",
];

fn ends_in_abbreviation(before: &str) -> bool {
    let word = before
        .rsplit(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("");
    ABBREVIATIONS.contains(&word)
}

/// Every number a citation names, ranges included: `[2, 5]`, `[1–3]`.
fn numbers_cited(inside: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for part in inside.split(',') {
        let part = part.trim();
        match part.split_once(['–', '-']) {
            Some((from, to)) => {
                if let (Ok(from), Ok(to)) = (from.trim().parse::<usize>(), to.trim().parse::<usize>()) {
                    if from <= to && to - from <= 50 {
                        out.extend(from..=to);
                    }
                }
            }
            None => out.extend(part.parse::<usize>().ok()),
        }
    }
    out
}

/// The sentences of a reply that cite a record, and how many did not.
///
/// A line break ends a sentence as a full stop does: a model that writes an
/// uncited line and a cited one below it has written two sentences, and only
/// one of them has a source.
pub fn cited_sentences(reply: &str, source_count: usize) -> (Vec<Sentence>, usize) {
    let mut pieces: Vec<&str> = Vec::new();
    for line in reply.split(['\n', '\r']) {
        let mut start = 0;
        for end in SENTENCE_END.find_iter(line) {
            if line[end.start()..].starts_with('.') && ends_in_abbreviation(&line[..end.start()]) {
                continue;
            }
            pieces.push(&line[start..end.end()]);
            start = end.end();
        }
        if start < line.len() {
            pieces.push(&line[start..]);
        }
    }

    let mut kept = Vec::new();
    let mut dropped = 0;
    for piece in pieces {
        if !piece.chars().any(char::is_alphabetic) {
            continue;
        }
        let mut sources: Vec<usize> = CITATION
            .captures_iter(piece)
            .flat_map(|c| numbers_cited(&c[1]))
            .filter(|n| (1..=source_count).contains(n))
            .collect();
        sources.sort_unstable();
        sources.dedup();
        if sources.is_empty() {
            dropped += 1;
            continue;
        }
        let bare = CITATION.replace_all(piece, "");
        let bare = SPACE_BEFORE_MARK.replace_all(&bare, "$1");
        let bare = SPACES.replace_all(&bare, " ");
        kept.push(Sentence {
            text: bare.trim().to_string(),
            sources,
        });
    }
    (kept, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_line_without_a_full_stop_is_a_sentence_of_its_own() {
        let (kept, dropped) = cited_sentences("Họ là bạn thân nhất của nhau\n\nTuấn dự đám cưới năm 2016 [2].", 3);
        assert_eq!(dropped, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "Tuấn dự đám cưới năm 2016.");
    }

    #[test]
    fn a_closing_quote_or_bracket_does_not_join_two_sentences() {
        let (kept, dropped) = cited_sentences("Anh ấy rất buồn (và giận dữ.) Họ gặp nhau [1].", 1);
        assert_eq!((kept.len(), dropped), (1, 1));
        let (kept, dropped) = cited_sentences("Tuấn nói “đi thôi.” Họ đi Đà Lạt [1].", 1);
        assert_eq!((kept.len(), dropped), (1, 1));
        assert_eq!(kept[0].text, "Họ đi Đà Lạt.");
    }

    #[test]
    fn an_abbreviation_and_a_range_of_sources_are_read_as_meant() {
        let (kept, dropped) = cited_sentences("Họ gặp nhau ở TP. HCM năm 2016 [1]. Rồi cùng làm ở Mây [2–3].", 3);
        assert_eq!(dropped, 0);
        assert_eq!(kept[0].text, "Họ gặp nhau ở TP. HCM năm 2016.");
        assert_eq!(kept[1].sources, vec![2, 3]);
    }

    /// The gate for Nhát D: not one sentence without a source is kept.
    #[test]
    fn a_sentence_that_cites_nothing_is_not_kept() {
        let reply = "Hai người quen nhau năm 2013 [1]. Họ rất thân với nhau. Tuấn dự đám cưới năm 2016 [2][3].";
        let (kept, dropped) = cited_sentences(reply, 3);
        assert_eq!(dropped, 1);
        assert_eq!(
            kept,
            vec![
                Sentence { text: "Hai người quen nhau năm 2013.".into(), sources: vec![1] },
                Sentence { text: "Tuấn dự đám cưới năm 2016.".into(), sources: vec![2, 3] },
            ]
        );
    }

    #[test]
    fn a_number_that_names_no_record_is_no_source() {
        let (kept, dropped) = cited_sentences("Họ đi Đà Lạt [7]. Họ gặp nhau [2, 9].", 3);
        assert_eq!(dropped, 1);
        assert_eq!(kept[0].sources, vec![2]);
    }

    #[test]
    fn citations_after_the_full_stop_belong_to_that_sentence() {
        let (kept, dropped) = cited_sentences("They met in 2013. [1] They still talk [2]", 2);
        assert_eq!(dropped, 0);
        assert_eq!(kept.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["They met in 2013.", "They still talk"]);
        assert_eq!(kept[0].sources, vec![1]);
    }

    #[test]
    fn a_decimal_point_is_not_the_end_of_a_sentence() {
        let (kept, _) = cited_sentences("Tao cho vay 3.5 triệu [1].", 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "Tao cho vay 3.5 triệu.");
    }

    #[test]
    fn a_reply_with_no_sources_shows_nothing_at_all() {
        let (kept, dropped) = cited_sentences("Đây là một mối quan hệ đẹp. Hãy trân trọng nó!", 5);
        assert!(kept.is_empty());
        assert_eq!(dropped, 2);
        assert_eq!(cited_sentences("", 5), (vec![], 0));
    }

    #[test]
    fn records_are_numbered_oldest_first_and_interactions_carry_what_was_said() {
        let item = |kind: &str, from: &str, to: &str, label: Option<&str>| Event {
            id: format!("{kind}{from}"),
            kind: kind.into(),
            node_id: "People/tuan.md".into(),
            node_type: "person".into(),
            title: label.map(String::from).unwrap_or_default(),
            node_title: "Tuấn".into(),
            links: Vec::new(),
            magnitude: 0.0,
            container_node: None,
            props: serde_json::Value::Null,
            happened_from: from.into(),
            happened_to: to.into(),
            precision: "range".into(),
            time_source: "frontmatter".into(),
            source: "derived".into(),
            shape: crate::timeline::derive::Shape::Occasion,
        };
        let coffee = NodeMetadata {
            id: "People/Interactions/c.md".into(),
            node_type: "interaction".into(),
            title: "Cà phê".into(),
            content: "Nói chuyện về công việc mới.".into(),
            properties: json!({ "date": "2016-05-03", "interaction_type": "coffee" }),
            created_at: String::new(),
            updated_at: String::new(),
            timestamp: 0,
            blocks: None,
        };
        let sources = sources_for(
            &[
                item("experience", "2018-08-01", "9999-12-31", Some("Founder · Mây")),
                item("interaction", "2016-05-03", "2016-05-03", None),
                item("connection", "2009-09-01", "9999-12-31", Some("friend")),
            ],
            &[coffee],
            &Default::default(),
        );
        assert_eq!(sources.len(), 3, "the interaction is read from its note, once");
        assert_eq!(sources[0].date, "2009-09-01 → now");
        assert_eq!(sources[0].what, "relationship: friend");
        assert_eq!(sources[1].what, "coffee: \"Nói chuyện về công việc mới.\"");
        assert_eq!(sources[2].n, 3);

        let asked = prompt("Tuấn", &sources, "Vietnamese");
        assert!(asked.contains("[2] 2016-05-03 · Cà phê · coffee"), "{asked}");
    }
}
