//! Chiêm nghiệm: a decision, what was expected of it, and what happened.
//!
//! The design is Nhát F of `docs/tua-lai-2026-09-14.md`. A decision is a note
//! of `type: decision` in `Decisions/`: the reasoning in its body, and in its
//! frontmatter the day it was made (`decided_on`), what was expected
//! (`expected`), the day to look at it again (`review_on`), and each time it
//! was looked at (`reviews: [{on, happened, outcome}]`).
//!
//! # What the app does with one
//!
//! On `review_on` it asks what actually happened, through the reminders every
//! other date already goes through (`calendar::reminders::plan_decision`), and
//! asks again a week later for as long as nobody answers.
//!
//! # Patterns
//!
//! Only across decisions sharing a tag, only once at least three of them have
//! been looked back on, and only as an observation. Every sentence Syn writes
//! must cite a decision, the sentences together must cite at least three, and
//! a sentence that tells the person what to do is removed: that would be
//! advice, which this is not. All of it checked in code (`check_pattern`), the
//! same way `syn::narrative` checks "Syn kể lại". Nothing sealed is a case.
//!
//! # Off
//!
//! `Timeline/reflect.json`, per vault. Off means off: no question on the
//! review day, on the desktop, the phone or Telegram, and no pattern. The
//! decisions themselves are the person's notes and stay where they are.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use chrono::{DateTime, Local, NaiveDate, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::seal::Seals;
use super::when::{self, Precision};
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::models::node::NodeMetadata;
use crate::syn::narrative::{self, Sentence, Source};

pub const CONFIG_FILE: &str = "Timeline/reflect.json";

/// Where a decision is written. Kept in step with `nodeRoutes.ts`.
pub const FOLDER: &str = "Decisions";

/// No pattern rests on fewer.
pub const MIN_CASES: usize = 3;

/// How what happened compared with what was expected.
pub const OUTCOMES: &[&str] = &["as_expected", "partly", "otherwise"];

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("reflect: {e}"))
}

// ─── The switch ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// On unless turned off. It asks only about decisions the person wrote
    /// down with a date to look again, which is asking to be asked.
    #[serde(default = "on")]
    pub enabled: bool,
    #[serde(flatten, default)]
    pub rest: Map<String, Value>,
}

fn on() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            rest: Map::new(),
        }
    }
}

pub fn read_config(vault_path: &str) -> Config {
    std::fs::read_to_string(Path::new(vault_path).join(CONFIG_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn write_config(vault_path: &str, config: &mut Config, now: DateTime<Utc>) -> AppResult<()> {
    super::extract::stamp(&mut config.rest, now);
    super::extract::write_json(&Path::new(vault_path).join(CONFIG_FILE), config)
}

/// The nodes the reminder planner is given, less every decision when
/// reflection is off for the vault, or when there is no vault to ask.
pub fn keep_if_on(vault_path: Option<&str>, nodes: Vec<NodeMetadata>) -> Vec<NodeMetadata> {
    if vault_path.is_some_and(|vault| read_config(vault).enabled) {
        return nodes;
    }
    nodes.into_iter().filter(|node| node.node_type != "decision").collect()
}

// ─── Reading decisions ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Review {
    pub on: String,
    pub happened: String,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Decision {
    pub id: String,
    pub title: String,
    pub decided_on: String,
    pub expected: String,
    pub review_on: Option<String>,
    pub reasoning: String,
    pub tags: Vec<String>,
    pub reviews: Vec<Review>,
    /// The day to look again has come and nobody has.
    pub due: bool,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn day_of(value: &Value, key: &str) -> Option<String> {
    text(value, key)
        .and_then(when::parse)
        .filter(|span| span.precision == Precision::Day)
        .map(|span| when::iso(span.from))
}

fn tags_of(properties: &Value) -> Vec<String> {
    let clean = |tag: &str| tag.trim().trim_start_matches('#').trim().to_string();
    match properties.get("tags") {
        Some(Value::Array(tags)) => tags.iter().filter_map(Value::as_str).map(clean).filter(|t| !t.is_empty()).collect(),
        Some(Value::String(tags)) => tags.split(',').map(clean).filter(|t| !t.is_empty()).collect(),
        _ => Vec::new(),
    }
}

/// Whether a review written on `on` answers the question asked on `review_on`.
fn answers(reviews: &[Review], review_on: &str) -> bool {
    reviews.iter().any(|review| review.on.as_str() >= review_on)
}

pub fn read_decision(node: &NodeMetadata, today: NaiveDate) -> Option<Decision> {
    if node.node_type != "decision" {
        return None;
    }
    let p = &node.properties;
    let decided_on = day_of(p, "decided_on").or_else(|| {
        DateTime::parse_from_rfc3339(&node.created_at)
            .ok()
            .map(|at| when::iso(at.with_timezone(&Local).date_naive()))
    })?;
    let reviews: Vec<Review> = p
        .get("reviews")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|review| {
            Some(Review {
                on: day_of(review, "on")?,
                happened: text(review, "happened").unwrap_or_default().to_string(),
                outcome: text(review, "outcome")
                    .filter(|outcome| OUTCOMES.contains(outcome))
                    .map(String::from),
            })
        })
        .collect();
    let review_on = day_of(p, "review_on");
    let due = review_on
        .as_deref()
        .is_some_and(|on| on <= when::iso(today).as_str() && !answers(&reviews, on));
    Some(Decision {
        id: node.id.clone(),
        title: node.title.clone(),
        decided_on,
        expected: text(p, "expected").unwrap_or_default().to_string(),
        review_on,
        reasoning: node.content.trim().to_string(),
        tags: tags_of(p),
        reviews,
        due,
    })
}

fn in_sealed_period(seals: &Seals, day: &str) -> bool {
    seals
        .periods()
        .iter()
        .any(|period| period.from.as_str() <= day && day <= period.to.as_str())
}

/// Every decision in the vault, newest first, less what is sealed: the note,
/// or a decision made or looked back on inside a sealed period.
pub fn decisions(db: &DbBridge, seals: &Seals, today: NaiveDate) -> AppResult<Vec<Decision>> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp
             FROM nodes WHERE node_type = 'decision'",
        )
        .map_err(sql)?;
    let nodes: Vec<NodeMetadata> = stmt
        .query_map([], |r| {
            Ok(NodeMetadata {
                id: r.get(0)?,
                node_type: r.get(1)?,
                title: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                content: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                properties: serde_json::from_str(&r.get::<_, Option<String>>(4)?.unwrap_or_default())
                    .unwrap_or(Value::Null),
                created_at: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                updated_at: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                timestamp: r.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                blocks: None,
            })
        })
        .map_err(sql)?
        .flatten()
        .collect();

    let mut out: Vec<Decision> = nodes
        .iter()
        .filter(|node| node.properties.get("sealed").and_then(Value::as_bool) != Some(true))
        .filter(|node| !seals.hides(&node.id))
        .filter_map(|node| read_decision(node, today))
        .filter(|decision| {
            !in_sealed_period(seals, &decision.decided_on)
                && !decision.reviews.iter().any(|review| in_sealed_period(seals, &review.on))
        })
        .collect();
    out.sort_by(|a, b| b.decided_on.cmp(&a.decided_on).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

// ─── Writing ─────────────────────────────────────────────────────

fn parse_day(text: &str, what: &str) -> AppResult<NaiveDate> {
    when::parse(text)
        .filter(|span| span.precision == Precision::Day)
        .map(|span| span.from)
        .ok_or_else(|| AppError::General(format!("'{text}' is not a day for {what}: use YYYY-MM-DD")))
}

/// Frontmatter for a decision made today.
pub fn new_decision(today: NaiveDate, expected: &str, review_on: Option<&str>, tags: &[String]) -> AppResult<Value> {
    let mut properties = json!({
        "decided_on": when::iso(today),
        "expected": expected.trim(),
        "reviews": [],
        "tags": tags.iter().map(|t| t.trim().trim_start_matches('#')).filter(|t| !t.is_empty()).collect::<Vec<_>>(),
    });
    if let Some(on) = review_on.map(str::trim).filter(|on| !on.is_empty()) {
        let on = parse_day(on, "review_on")?;
        if on < today {
            return Err(AppError::General("The day to look again cannot be before the decision".into()));
        }
        properties["review_on"] = Value::from(when::iso(on));
    }
    Ok(properties)
}

/// A decision's properties with one more look back, and the next day to look
/// again if one was given.
pub fn with_review(
    properties: &Value,
    on: NaiveDate,
    happened: &str,
    outcome: Option<&str>,
    next_review_on: Option<&str>,
) -> AppResult<Map<String, Value>> {
    let happened = happened.trim();
    if happened.is_empty() {
        return Err(AppError::General("What happened is the point of looking back".into()));
    }
    if let Some(outcome) = outcome {
        if !OUTCOMES.contains(&outcome) {
            return Err(AppError::General(format!("'{outcome}' is not one of {OUTCOMES:?}")));
        }
    }
    let mut map = match properties {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    let mut reviews = map.get("reviews").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut entry = json!({ "on": when::iso(on), "happened": happened });
    if let Some(outcome) = outcome {
        entry["outcome"] = Value::from(outcome);
    }
    reviews.push(entry);
    map.insert("reviews".into(), Value::Array(reviews));
    if let Some(next) = next_review_on.map(str::trim).filter(|n| !n.is_empty()) {
        let next = parse_day(next, "the next look")?;
        if next <= on {
            return Err(AppError::General("The next look has to be after today".into()));
        }
        map.insert("review_on".into(), Value::from(when::iso(next)));
    }
    Ok(map)
}

// ─── Patterns ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Tally {
    pub as_expected: usize,
    pub partly: usize,
    pub otherwise: usize,
    pub unrated: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Group {
    pub tag: String,
    /// Decisions with this tag that have been looked back on.
    pub cases: Vec<String>,
    /// How the latest look at each compared with what was expected. Counted,
    /// not interpreted.
    pub tally: Tally,
    pub enough: bool,
}

/// Decisions that share a tag and have been looked back on, grouped.
pub fn groups(decisions: &[Decision]) -> Vec<Group> {
    let mut by_tag: BTreeMap<String, (String, Vec<&Decision>)> = BTreeMap::new();
    for decision in decisions.iter().filter(|d| !d.reviews.is_empty()) {
        for tag in &decision.tags {
            let entry = by_tag
                .entry(tag.to_lowercase())
                .or_insert_with(|| (tag.clone(), Vec::new()));
            if !entry.1.iter().any(|kept| kept.id == decision.id) {
                entry.1.push(decision);
            }
        }
    }
    let mut out: Vec<Group> = by_tag
        .into_values()
        .map(|(tag, cases)| {
            let mut tally = Tally::default();
            for case in &cases {
                match case.reviews.iter().max_by(|a, b| a.on.cmp(&b.on)).and_then(|r| r.outcome.as_deref()) {
                    Some("as_expected") => tally.as_expected += 1,
                    Some("partly") => tally.partly += 1,
                    Some("otherwise") => tally.otherwise += 1,
                    _ => tally.unrated += 1,
                }
            }
            Group {
                enough: cases.len() >= MIN_CASES,
                cases: cases.iter().map(|case| case.id.clone()).collect(),
                tag,
                tally,
            }
        })
        .collect();
    out.sort_by(|a, b| b.enough.cmp(&a.enough).then(b.cases.len().cmp(&a.cases.len())).then_with(|| a.tag.cmp(&b.tag)));
    out
}

/// The decisions a pattern may rest on, numbered, oldest first.
pub fn sources(cases: &[&Decision]) -> Vec<Source> {
    let mut ordered: Vec<&&Decision> = cases.iter().collect();
    ordered.sort_by(|a, b| a.decided_on.cmp(&b.decided_on));
    ordered
        .iter()
        .enumerate()
        .map(|(i, decision)| {
            let reasoning: String = decision.reasoning.chars().take(400).collect();
            let looks = decision
                .reviews
                .iter()
                .map(|review| {
                    format!(
                        "on {} it had turned out: \"{}\"{}",
                        review.on,
                        review.happened,
                        review.outcome.as_deref().map(|o| format!(" ({})", o.replace('_', " "))).unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            Source {
                n: i + 1,
                node_id: decision.id.clone(),
                node_type: "decision".into(),
                title: decision.title.clone(),
                date: decision.decided_on.clone(),
                what: format!("reasoning: \"{reasoning}\"; expected: \"{}\"; {looks}", decision.expected),
            }
        })
        .collect()
}

/// The words a pattern is refused on when nothing three decisions share.
pub const NO_PATTERN: &str = "NO PATTERN";

pub fn pattern_prompt(tag: &str, sources: &[Source], language: &str) -> String {
    let mut out = format!(
        "Below are {count} decisions a person made, all tagged \"{tag}\": why they made each, what they \
expected, and what they later wrote had actually happened.\n\n\
In {language}, write two to four plain sentences on what these decisions have in common, if anything: \
in what was expected, in what happened, or in the gap between the two.\n\n\
This is an observation of their own records, and nothing more. Do not advise, do not tell them what to \
do or avoid, do not judge them, do not predict. After every sentence put the numbers of the records it \
rests on in square brackets, like [1][3]; a sentence without a number is deleted. Taken together the \
sentences must rest on at least three different records. If no three of them share anything, reply \
with exactly: {NO_PATTERN}\n\nRecords:\n",
        count = sources.len(),
    );
    for source in sources {
        out.push_str(&format!("[{}] {} · {} · {}\n", source.n, source.date, source.title, source.what));
    }
    out
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Pattern {
    pub tag: String,
    pub sentences: Vec<Sentence>,
    pub sources: Vec<Source>,
    /// Sentences that cited no decision.
    pub dropped: usize,
    /// Sentences that told the person what to do.
    pub advice_dropped: usize,
    /// How many different decisions what is shown rests on.
    pub cited: usize,
    /// Why nothing is shown: `no_pattern`, `fewer_than_three`, `not_enough_cases`.
    pub withheld: Option<String>,
}

/// A sentence telling the person what to do. "nên" alone is not one: in
/// Vietnamese it is also "so", as in "nên tao nghỉ".
static ADVICE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        // A sentence that opens telling: "Nên chờ…", "Có lẽ cần cân nhắc…".
        r"(?i)^\s*(?:có lẽ\s+|maybe\s+|perhaps\s+)?(?:nên|cần|hãy|đừng)\b",
        // Told to someone. Not "anh phải chuyển nhà": of the past that is "had to".
        r"|\b(?:mày|bạn|anh|chị|em|you|we)\s+(?:nên|cần|should|must|need to|ought to|could consider|might consider)\b",
        r"|\bhãy\b|\bđừng\b",
        // "Lần tới", and "lần sau" only when it goes on to tell: "lần sau đó" is a past.
        r"|\blần tới\b|\blần sau\s+(?:nên|hãy|cần|thử|đừng)\b|\bnext time\b",
        r"|\bthử\s+(?:nghĩ|cân nhắc|xem)\b|\b(?:nên|cần)\s+cân nhắc\b",
        r"|^\s*consider\b|\btry to\b|\bi (?:suggest|recommend)\b",
        r"|\bit (?:might|may|would|could) (?:help|be (?:wise|worth|better|good))\b",
    ))
    .expect("pattern")
});

/// What of a reply may be shown as a pattern.
pub fn check_pattern(tag: &str, reply: &str, sources: Vec<Source>) -> Pattern {
    let mut pattern = Pattern {
        tag: tag.to_string(),
        ..Pattern::default()
    };
    if reply.trim().contains(NO_PATTERN) {
        pattern.sources = sources;
        pattern.withheld = Some("no_pattern".into());
        return pattern;
    }
    let (sentences, dropped) = narrative::cited_sentences(reply, sources.len());
    pattern.dropped = dropped;
    let (kept, advice): (Vec<Sentence>, Vec<Sentence>) =
        sentences.into_iter().partition(|sentence| !ADVICE.is_match(&sentence.text));
    pattern.advice_dropped = advice.len();

    let mut cited: Vec<usize> = kept.iter().flat_map(|s| s.sources.iter().copied()).collect();
    cited.sort_unstable();
    cited.dedup();
    pattern.cited = cited.len();
    pattern.sources = sources;
    if cited.len() < MIN_CASES {
        pattern.withheld = Some("fewer_than_three".into());
    } else {
        pattern.sentences = kept;
    }
    pattern
}

/// What the Chiêm nghiệm panel shows.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Overview {
    pub enabled: bool,
    pub decisions: Vec<Decision>,
    pub groups: Vec<Group>,
}

/// Switched off, the panel is given nothing to show but the switch.
pub fn overview(config: &Config, decisions: Vec<Decision>) -> Overview {
    if !config.enabled {
        return Overview {
            enabled: false,
            decisions: Vec::new(),
            groups: Vec::new(),
        };
    }
    Overview {
        enabled: true,
        groups: groups(&decisions),
        decisions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::reminders;
    use chrono::NaiveDateTime;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M").unwrap()
    }

    fn decision_node(id: &str, properties: Value, body: &str) -> NodeMetadata {
        NodeMetadata {
            id: id.into(),
            node_type: "decision".into(),
            title: id.trim_start_matches("Decisions/").trim_end_matches(".md").into(),
            content: body.into(),
            properties,
            created_at: "2020-01-01T00:00:00.000Z".into(),
            updated_at: "2020-01-01T00:00:00.000Z".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn reviewed(id: &str, decided_on: &str, outcome: &str, extra: Value) -> NodeMetadata {
        let mut properties = json!({
            "decided_on": decided_on,
            "expected": format!("expected of {id}"),
            "tags": ["đổi việc"],
            "reviews": [{ "on": "2025-06-01", "happened": format!("what happened after {id}"), "outcome": outcome }],
        });
        if let (Value::Object(map), Value::Object(more)) = (&mut properties, extra) {
            map.extend(more);
        }
        decision_node(id, properties, &format!("why {id}"))
    }

    #[test]
    fn a_decision_is_on_the_timeline_when_it_was_made_and_each_time_it_was_looked_at() {
        let node = reviewed("Decisions/leave.md", "2021-03-01", "partly", json!({}));
        let derived = crate::timeline::derive::derive(
            &crate::timeline::derive::NodeView {
                id: &node.id,
                node_type: "decision",
                title: &node.title,
                properties: &node.properties,
            },
            &std::collections::HashMap::new(),
        );
        let kinds: Vec<(&str, String)> = derived.iter().map(|d| (d.kind, when::iso(d.span.from))).collect();
        assert_eq!(kinds, vec![("decision", "2021-03-01".into()), ("decision_review", "2025-06-01".into())]);
    }

    #[test]
    fn the_question_comes_on_its_day_and_again_a_week_later_until_answered() {
        let waiting = decision_node("Decisions/move.md", json!({ "decided_on": "2026-01-01", "review_on": "2026-09-01" }), "");
        let answered = decision_node(
            "Decisions/car.md",
            json!({ "decided_on": "2026-01-01", "review_on": "2026-09-01", "reviews": [{ "on": "2026-09-02", "happened": "fine" }] }),
            "",
        );
        let sealed = decision_node("Decisions/ex.md", json!({ "decided_on": "2026-01-01", "review_on": "2026-09-01", "sealed": true }), "");

        let asked = reminders::plan(&[waiting.clone(), answered, sealed], at("2026-09-01T00:00"), at("2026-09-15T23:59"), "");
        let when_asked: Vec<(String, bool)> = asked.iter().map(|r| (r.trigger_at.to_string(), r.overdue)).collect();
        assert_eq!(
            when_asked,
            vec![("2026-09-01 09:00:00".into(), false), ("2026-09-08 09:00:00".into(), true), ("2026-09-15 09:00:00".into(), true)]
        );
        assert!(asked.iter().all(|r| r.target_id == waiting.id && r.target_type == "decision"));
    }

    /// Gate: reflection can be switched off completely.
    #[test]
    fn switched_off_nothing_asks_and_nothing_is_offered() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let waiting = decision_node("Decisions/move.md", json!({ "decided_on": "2026-01-01", "review_on": "2026-09-01" }), "");
        let task = NodeMetadata { node_type: "task".into(), ..decision_node("Tasks/t.md", json!({ "due_date": "2026-09-01", "status": "todo" }), "") };
        let window = (at("2026-09-01T00:00"), at("2026-09-01T23:59"));

        let on = keep_if_on(Some(&vault), vec![waiting.clone(), task.clone()]);
        assert_eq!(reminders::plan(&on, window.0, window.1, "").iter().filter(|r| r.target_type == "decision").count(), 1);

        let mut config = read_config(&vault);
        config.enabled = false;
        write_config(&vault, &mut config, Utc::now()).unwrap();

        let off = keep_if_on(Some(&vault), vec![waiting.clone(), task.clone()]);
        let asked = reminders::plan(&off, window.0, window.1, "");
        assert!(asked.iter().all(|r| r.target_type != "decision"), "{asked:?}");
        assert!(asked.iter().any(|r| r.target_type == "task"), "only reflection is off");
        assert!(keep_if_on(None, vec![waiting.clone()]).is_empty(), "no vault to ask is not permission");

        let shown = overview(&read_config(&vault), vec![read_decision(&waiting, day("2026-09-15")).unwrap()]);
        assert_eq!(shown, Overview { enabled: false, decisions: vec![], groups: vec![] });
    }

    #[test]
    fn a_pattern_waits_for_three_decisions_looked_back_on() {
        let today = day("2026-09-15");
        let mut read: Vec<Decision> = ["a", "b"]
            .iter()
            .map(|id| read_decision(&reviewed(&format!("Decisions/{id}.md"), "2020-01-01", "otherwise", json!({})), today).unwrap())
            .collect();
        read.push(read_decision(&decision_node("Decisions/unreviewed.md", json!({ "decided_on": "2024-01-01", "tags": ["Đổi việc"] }), ""), today).unwrap());
        assert!(!groups(&read)[0].enough, "an unreviewed decision is not a case");

        read.push(read_decision(&reviewed("Decisions/c.md", "2023-01-01", "as_expected", json!({ "tags": ["#Đổi việc"] })), today).unwrap());
        let group = &groups(&read)[0];
        assert!(group.enough);
        assert_eq!(group.cases.len(), 3);
        assert_eq!(group.tally, Tally { as_expected: 1, partly: 0, otherwise: 2, unrated: 0 });
    }

    fn three_sources() -> Vec<Source> {
        (1..=4)
            .map(|n| Source { n, node_id: format!("Decisions/{n}.md"), node_type: "decision".into(), title: format!("d{n}"), date: "2020-01-01".into(), what: String::new() })
            .collect()
    }

    /// Gate: a pattern cites all three, and says nothing uncited.
    #[test]
    fn a_pattern_resting_on_fewer_than_three_decisions_is_not_shown() {
        let two = check_pattern("đổi việc", "Cả hai lần mày đều kỳ vọng lương cao hơn [1][2]. Lần nào cũng thất vọng.", three_sources());
        assert!(two.sentences.is_empty());
        assert_eq!((two.withheld.as_deref(), two.cited, two.dropped), (Some("fewer_than_three"), 2, 1));

        let three = check_pattern("đổi việc", "Ba lần đều quyết trong vòng một tuần [1][2][4]. Kỳ vọng xoay quanh thu nhập [2].", three_sources());
        assert_eq!((three.withheld, three.cited, three.sentences.len()), (None, 3, 2));

        let none = check_pattern("đổi việc", "NO PATTERN", three_sources());
        assert_eq!(none.withheld.as_deref(), Some("no_pattern"));
    }

    #[test]
    fn advice_in_other_shapes_is_caught_and_a_past_is_not_advice() {
        let advice = [
            "Nên chờ thêm một tháng [1][2][3].",
            "Có lẽ cần cân nhắc kỹ hơn [1][2][3].",
            "Lần tới nên hỏi gia đình [1][2][3].",
            "It might help to wait a month [1][2][3].",
        ];
        for sentence in advice {
            assert_eq!(check_pattern("t", sentence, three_sources()).advice_dropped, 1, "{sentence}");
        }
        let observations = [
            "Lần sau đó mày lại đổi việc [1][2][3].",
            "Cả ba lần anh phải chuyển nhà [1][2][3].",
            "Each time you did not consider the cost [1][2][3].",
        ];
        for sentence in observations {
            assert_eq!(check_pattern("t", sentence, three_sources()).advice_dropped, 0, "{sentence}");
        }
    }

    #[test]
    fn a_decision_written_after_its_hour_is_not_asked_about_that_morning() {
        use chrono::TimeZone;
        let written = chrono::Local.with_ymd_and_hms(2026, 9, 1, 15, 0, 0).single().unwrap();
        let node = NodeMetadata {
            created_at: crate::utils::timestamp::canonical(written.with_timezone(&Utc)),
            ..decision_node("Decisions/today.md", json!({ "decided_on": "2026-09-01", "review_on": "2026-09-01" }), "")
        };
        let asked = reminders::plan(&[node], at("2026-09-01T00:00"), at("2026-09-08T23:59"), "");
        let days: Vec<String> = asked.iter().map(|r| r.occurrence_date.clone()).collect();
        assert_eq!(days, vec!["2026-09-08".to_string()]);
    }

    #[test]
    fn telling_the_person_what_to_do_is_not_an_observation() {
        let reply = "Ba lần đều quyết vội [1][2][3]. Lần sau mày nên chờ thêm một tháng [1][2][3]. Hãy hỏi ý kiến gia đình [3]. Thu nhập tăng, nên tao nghĩ mọi thứ ổn [2].";
        let pattern = check_pattern("đổi việc", reply, three_sources());
        assert_eq!(pattern.advice_dropped, 2);
        assert_eq!(pattern.sentences.len(), 2, "{:?}", pattern.sentences);
        assert!(pattern.sentences[1].text.contains("nên tao nghĩ"), "'nên' meaning 'so' stays");
    }

    /// Gate: a pattern never rests on anything sealed. Checked on what is
    /// sent to the model.
    #[test]
    fn nothing_sealed_is_a_case() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let db = DbBridge::new_in_memory_full().unwrap();
        for node in [
            reviewed("Decisions/a.md", "2018-05-01", "otherwise", json!({})),
            reviewed("Decisions/b.md", "2020-05-01", "partly", json!({})),
            reviewed("Decisions/c.md", "2022-05-01", "otherwise", json!({})),
            reviewed("Decisions/MARK-SEALED-NOTE.md", "2021-05-01", "otherwise", json!({ "sealed": true })),
            reviewed("Decisions/MARK-SEALED-PERIOD.md", "2019-02-10", "otherwise", json!({})),
        ] {
            db.upsert_node(&node).unwrap();
        }
        crate::timeline::seal::write_period(&vault, "2019-02", "2019-02").unwrap();
        let seals = Seals::read(&db, &vault).unwrap();

        let read = decisions(&db, &seals, day("2026-09-15")).unwrap();
        assert_eq!(read.len(), 3, "{:?}", read.iter().map(|d| &d.id).collect::<Vec<_>>());
        let group = &groups(&read)[0];
        assert!(group.enough);

        let cases: Vec<&Decision> = read.iter().filter(|d| group.cases.contains(&d.id)).collect();
        let sent = pattern_prompt(&group.tag, &sources(&cases), "Vietnamese");
        assert!(!sent.contains("MARK-"), "{sent}");
        assert!(sent.contains("[3] 2022-05-01"), "{sent}");
    }

    #[test]
    fn looking_back_adds_a_review_and_can_set_the_next_look() {
        let properties = new_decision(day("2026-01-10"), "a calmer year", Some("2026-09-01"), &["#đổi việc".into()]).unwrap();
        assert_eq!(properties["tags"], json!(["đổi việc"]));
        assert!(new_decision(day("2026-01-10"), "x", Some("2025-01-01"), &[]).is_err());

        let map = with_review(&properties, day("2026-09-02"), "calmer, poorer", Some("partly"), Some("2027-09-01")).unwrap();
        assert_eq!(map["reviews"], json!([{ "on": "2026-09-02", "happened": "calmer, poorer", "outcome": "partly" }]));
        assert_eq!(map["review_on"], "2027-09-01");
        assert!(with_review(&properties, day("2026-09-02"), "  ", None, None).is_err());
        assert!(with_review(&properties, day("2026-09-02"), "x", Some("great"), None).is_err());

        let node = decision_node("Decisions/d.md", Value::Object(map), "");
        assert!(!read_decision(&node, day("2026-09-10")).unwrap().due, "answered, and the next look is next year");
    }
}
