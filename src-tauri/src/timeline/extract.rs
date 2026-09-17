//! Tier 1: what a model reads out of the person's own words.
//!
//! The design is §4.5 and §4.7.B of `docs/tua-lai-2026-09-14.md`, and this is
//! Nhát E of it. Derived items (`derive`) are what a field already says; these
//! are what a sentence says — "hôm qua đưa mẹ đi khám mắt" — which no field
//! holds and only a model can read.
//!
//! # Where results live, and why there
//!
//! A reading costs a model call, so it is kept in the vault and synced rather
//! than done again on every device: `Timeline/<year>/<YYYY-MM>.<device>.json`.
//! One writer per file, like the evidence ledger, so sync never merges two
//! edits of one file. An item sits in the month its `happened_from` falls in;
//! the record that a note was read (`sources`) sits in the month it was read.
//! A device that finds a note's fingerprint already read, by anyone, does not
//! read it again.
//!
//! # What a reading is not
//!
//! It is a proposal. Nothing here writes outside `Timeline/`; the person's
//! notes change only when they accept a proposal in the tray, which writes a
//! `moments` entry into the note it came from (tier 2). Until then the
//! proposals are left out of every answer the timeline gives — to Syn, to
//! Nexus, to People — because a guess is not a record (§6).
//!
//! # What is read
//!
//! §4.7.B, and nothing else: daily notes, interactions, person notes, quick
//! captures and events by default; other notes only by folder or tag; Syn
//! conversations only when turned on, and then only what the person wrote,
//! never what Syn answered. `timeline: false` in a note's frontmatter keeps it
//! out, `timeline: true` lets it in. Nothing sealed is read.
//!
//! # Edited notes
//!
//! A note edited after it was read keeps its proposals, marked stale, and is
//! not read again until asked. Reading again on every edit would spend a model
//! call on each save of a note somebody is still writing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::asked;
use super::seal::Seals;
use super::magnitude::{self, Signals};
use super::store::Event;
use super::when::{self, Precision, Span};
use super::FOLDER;
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::syn::provider::{ChatMessage, ChatProvider, ChatRequest};

/// Bump when the prompt or the reading of a reply changes enough that an old
/// reading is worth replacing. Nothing is read again on its own when it does;
/// the tray offers it (§4.5.3).
pub const EXTRACTOR_VERSION: u32 = 2;

/// Whether reading is on, for this vault. Synced, so it is decided once.
pub const CONFIG_FILE: &str = "Timeline/extract.json";

/// Who accepted or declined what, one file per device.
pub const REVIEWS_DIR: &str = "Timeline/reviews";

/// A note is read automatically only once it has sat unchanged this long. A
/// daily note is saved every few seconds while it is written, and a reading
/// of the first paragraph is stale by the second.
pub const SETTLE_MINUTES: i64 = 120;

/// The most notes one automatic pass reads, so a vault switched on for the
/// first time is read a little at a time rather than in one sitting.
pub const AUTO_LIMIT: usize = 10;

/// Shorter than this there is nothing to read.
const MIN_CHARS: usize = 40;

/// What is sent of one note. A journal entry is far shorter; a pasted essay
/// in a person note is not, and is not what this is for.
const MAX_INPUT_CHARS: usize = 6_000;

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("timeline extract: {e}"))
}

fn io(path: &Path, e: impl std::fmt::Display) -> AppError {
    AppError::General(format!("{}: {e}", path.display()))
}

// ─── Configuration ───────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// Off until the person turns it on, after reading what it sends where
    /// and how often it was right (`docs/eval-timeline-extract-2026-09-15.md`).
    #[serde(default)]
    pub enabled: bool,
    /// A cloud provider may read notes. Off unless chosen for this vault (§7.6).
    #[serde(default)]
    pub allow_cloud: bool,
    /// Ordinary notes under these folders are read too.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Ordinary notes with these tags are read too.
    #[serde(default)]
    pub tags: Vec<String>,
    /// What the person wrote to Syn is read too.
    #[serde(default)]
    pub conversations: bool,
    /// Whatever else the file holds, sync's `metadata` among it, kept as found.
    #[serde(flatten, default)]
    pub rest: Map<String, Value>,
}

pub fn read_config(vault_path: &str) -> Config {
    std::fs::read_to_string(Path::new(vault_path).join(CONFIG_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn write_config(vault_path: &str, config: &mut Config, now: DateTime<Utc>) -> AppResult<()> {
    stamp(&mut config.rest, now);
    write_json(&Path::new(vault_path).join(CONFIG_FILE), config)
}

/// Sync resolves a JSON document whole by `metadata.updated_at`.
pub(crate) fn stamp(rest: &mut Map<String, Value>, now: DateTime<Utc>) {
    let metadata = rest
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(metadata) = metadata {
        metadata.insert(
            "updated_at".into(),
            Value::from(crate::utils::timestamp::canonical(now)),
        );
    }
}

/// Written beside the file and renamed over it, so a crash leaves the old
/// file or the new one and never half of each. The temporary name starts with
/// a dot, which sync skips.
pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> AppResult<()> {
    let dir = path
        .parent()
        .ok_or_else(|| AppError::General(format!("{} has no folder", path.display())))?;
    std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = dir.join(format!(".{name}.tmp"));
    let text = serde_json::to_string_pretty(value).map_err(|e| io(path, e))?;
    std::fs::write(&temporary, text).map_err(|e| io(&temporary, e))?;
    std::fs::rename(&temporary, path).map_err(|e| io(path, e))
}

// ─── What is read ────────────────────────────────────────────────

/// One thing to read: a note, or one day of what the person wrote to Syn.
#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    /// The note's path, or `Syn/<file>#<day>` for a day of a conversation.
    pub node_id: String,
    pub node_type: String,
    pub title: String,
    /// The day it was written, which relative times are counted from.
    pub recorded: NaiveDate,
    /// Whether it is about that one day, so a moment with no time of its own
    /// happened then. True of a daily note; not of a note about a person.
    pub dated_by_day: bool,
    pub text: String,
    /// blake3 of everything that would be read, frontmatter excluded: sync
    /// giving a note its identity, or accepting a proposal into it, is not an
    /// edit of what it says.
    pub hash: String,
    /// The person an interaction was with, who took part whether named or not.
    pub person_id: Option<String>,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Types that are never anybody's life, whatever their frontmatter says.
fn never(node_type: &str) -> bool {
    node_type.starts_with("syn_")
        || node_type.starts_with("finance_")
        || node_type.starts_with("pdf_")
        || matches!(
            node_type,
            "whiteboard" | "filter" | "view" | "schema" | "canvas" | "json" | "file" | "task" | "project"
        )
}

/// §4.7.B, and the frontmatter switch that overrides it for one note.
pub fn wanted(node_type: &str, properties: &Value, id: &str, config: &Config) -> bool {
    if never(node_type) {
        return false;
    }
    match properties.get("timeline") {
        Some(Value::Bool(false)) => return false,
        Some(Value::Bool(true)) => return true,
        _ => {}
    }
    match node_type {
        "interaction" | "person" | "quickcap" | "event" => true,
        "note" => {
            text(properties, "date").is_some()
                || config.folders.iter().any(|folder| {
                    let folder = folder.trim().trim_matches('/');
                    !folder.is_empty() && id.starts_with(&format!("{folder}/"))
                })
                || tagged(properties, &config.tags)
        }
        _ => false,
    }
}

fn tagged(properties: &Value, wanted: &[String]) -> bool {
    if wanted.is_empty() {
        return false;
    }
    let norm = |tag: &str| tag.trim().trim_start_matches('#').to_lowercase();
    let wanted: HashSet<String> = wanted.iter().map(|t| norm(t)).collect();
    match properties.get("tags") {
        Some(Value::Array(tags)) => tags
            .iter()
            .filter_map(Value::as_str)
            .any(|tag| wanted.contains(&norm(tag))),
        Some(Value::String(tags)) => tags.split(',').any(|tag| wanted.contains(&norm(tag))),
        _ => false,
    }
}

fn local_day(stamp: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|d| d.with_timezone(&Local).date_naive())
        .or_else(|| when::parse(stamp).filter(|s| s.precision == Precision::Day).map(|s| s.from))
}

/// The day a node was written, and whether it is about that day.
fn recorded(node_type: &str, properties: &Value, created_at: &str) -> Option<(NaiveDate, bool)> {
    let field = match node_type {
        "note" | "interaction" => text(properties, "date"),
        "event" => text(properties, "start_at").or_else(|| text(properties, "event_date")),
        _ => None,
    };
    if let Some(day) = field
        .and_then(when::parse)
        .filter(|span| span.precision == Precision::Day)
    {
        return Some((day.from, true));
    }
    let by_day = node_type == "quickcap";
    local_day(created_at).map(|day| (day, by_day))
}

fn in_sealed_period(seals: &Seals, day: NaiveDate) -> bool {
    let day = when::iso(day);
    seals
        .periods()
        .iter()
        .any(|period| period.from.as_str() <= day.as_str() && day.as_str() <= period.to.as_str())
}

pub(crate) fn fingerprint(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

#[allow(clippy::too_many_arguments)]
fn make_input(
    node_id: String,
    node_type: String,
    title: String,
    content: &str,
    recorded: NaiveDate,
    dated_by_day: bool,
    person_id: Option<String>,
) -> Option<Input> {
    let whole = content.trim();
    if whole.chars().count() < MIN_CHARS {
        return None;
    }
    Some(Input {
        hash: fingerprint(whole),
        text: whole.chars().take(MAX_INPUT_CHARS).collect(),
        node_id,
        node_type,
        title,
        recorded,
        dated_by_day,
        person_id,
    })
}

/// Everything in the vault this configuration reads, less what is sealed.
///
/// `settled_before` leaves out what was changed after it, for automatic runs.
pub fn inputs(
    db: &DbBridge,
    vault_path: &str,
    config: &Config,
    seals: &Seals,
    today: NaiveDate,
    settled_before: Option<DateTime<Utc>>,
) -> AppResult<Vec<Input>> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at
             FROM nodes ORDER BY id",
        )
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            ))
        })
        .map_err(sql)?;

    let settled = |updated_at: &str| match settled_before {
        None => true,
        Some(before) => DateTime::parse_from_rfc3339(updated_at)
            .map(|at| at.with_timezone(&Utc) <= before)
            .unwrap_or(true),
    };

    let mut out = Vec::new();
    for (id, node_type, title, content, properties, created_at, updated_at) in rows.flatten() {
        if super::is_timeline_path(&id) {
            continue;
        }
        let properties: Value = serde_json::from_str(&properties).unwrap_or(Value::Null);
        if !wanted(&node_type, &properties, &id, config) || seals.hides(&id) || !settled(&updated_at) {
            continue;
        }
        let person_id = text(&properties, "person_id").map(String::from);
        if person_id.as_deref().is_some_and(|person| seals.hides(person)) {
            continue;
        }
        let Some((day, by_day)) = recorded(&node_type, &properties, &created_at) else {
            continue;
        };
        if day > today || in_sealed_period(seals, day) {
            continue;
        }
        out.extend(make_input(id, node_type, title, &content, day, by_day, person_id));
    }

    if config.conversations {
        out.extend(conversation_inputs(vault_path, seals, today, settled_before));
    }
    Ok(out)
}

/// What the person wrote to Syn, one input per conversation per day.
///
/// Only `user` turns. What Syn said is not a fact about anybody's life, and
/// reading it would put the assistant's guesses back in as the person's
/// memories (§4.7, rule 3).
fn conversation_inputs(
    vault_path: &str,
    seals: &Seals,
    today: NaiveDate,
    settled_before: Option<DateTime<Utc>>,
) -> Vec<Input> {
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join("Syn")) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();

    let mut out = Vec::new();
    for path in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let rel = format!("Syn/{name}");
        if seals.hides(&rel) {
            continue;
        }
        let Some(file) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            continue;
        };
        let Some(messages) = file.get("messages").and_then(Value::as_array) else {
            continue;
        };
        let title = text(&file, "title").unwrap_or("Syn").to_string();

        let mut days: BTreeMap<NaiveDate, (Vec<&str>, String)> = BTreeMap::new();
        for message in messages {
            if message.get("role").and_then(Value::as_str) != Some("user") {
                continue;
            }
            let (Some(content), Some(stamp)) = (text(message, "content"), text(message, "timestamp")) else {
                continue;
            };
            let Some(day) = local_day(stamp) else {
                continue;
            };
            let entry = days.entry(day).or_default();
            entry.0.push(content);
            if stamp > entry.1.as_str() {
                entry.1 = stamp.to_string();
            }
        }

        for (day, (said, last)) in days {
            if day > today || in_sealed_period(seals, day) {
                continue;
            }
            if let Some(before) = settled_before {
                if DateTime::parse_from_rfc3339(&last).is_ok_and(|at| at.with_timezone(&Utc) > before) {
                    continue;
                }
            }
            out.extend(make_input(
                format!("{rel}#{}", when::iso(day)),
                "syn_conversation".into(),
                title.clone(),
                &said.join("\n\n"),
                day,
                true,
                None,
            ));
        }
    }
    out
}

// ─── Asking ──────────────────────────────────────────────────────

fn kind_words(input: &Input) -> &'static str {
    match input.node_type.as_str() {
        "note" if input.dated_by_day => "daily journal entry",
        "interaction" => "record of time spent with someone",
        "person" => "note about a person they know",
        "quickcap" => "quick note jotted down in the moment",
        "event" => "calendar event, with notes",
        "syn_conversation" => "set of messages they wrote to an assistant",
        _ => "note",
    }
}

/// What the model is asked about one input.
pub fn prompt(input: &Input) -> String {
    format!(
        "Read one entry from a person's own notes and list what HAPPENED in their life: something \
they did, went through or took part in — a meeting, a trip, a change of job or home, an \
illness, a milestone.\n\n\
Leave out: plans, intentions and anything that has not happened yet; to-do items; facts, \
knowledge and opinions; what happened only to someone else, told to them second-hand.\n\n\
The entry is a {kind}, titled \"{title}\", written on {day} ({weekday}).\n\n\
Reply with JSON only, in this shape:\n\
{{\"moments\": [{{\"what\": \"a short title, in the entry's language\", \"when\": \"\", \"people\": [], \"where\": \"\", \"quote\": \"\", \"confidence\": 0.0}}]}}\n\n\
- when: the time as the entry gives it. Copy relative words exactly (\"hôm qua\", \"tuần trước\", \
\"last week\"); they are counted from the day it was written. If the entry leaves the year \
implied (\"tháng 4\" in an entry from 2024), you may complete it as \"2024-04\". Leave it \"\" \
when the entry gives no time at all. Do not guess one.\n\
- people: the others who took part, by the name the entry uses. Not the writer.\n\
- where: the place, in the entry's own words. Leave it \"\" when the entry does not say. \
Do not infer a place from what happened.\n\
- quote: the words of the entry the moment rests on, copied exactly, one sentence at most.\n\
- confidence: how sure you are it happened, 0 to 1.\n\
- If nothing in it happened, reply {{\"moments\": []}}.\n\n\
Entry:\n\"\"\"\n{text}\n\"\"\"",
        kind = kind_words(input),
        title = input.title,
        day = when::iso(input.recorded),
        weekday = input.recorded.format("%A"),
        text = input.text,
    )
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RawMoment {
    #[serde(default)]
    pub what: String,
    #[serde(default)]
    pub when: String,
    #[serde(default)]
    pub people: Vec<String>,
    /// Named `where` in the reply; a keyword here.
    #[serde(default, rename = "where")]
    pub place: String,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// The moments in a reply, or `None` when the reply is not the JSON asked for.
pub fn parse_reply(reply: &str) -> Option<Vec<RawMoment>> {
    #[derive(Deserialize)]
    struct Shape {
        #[serde(default)]
        moments: Vec<RawMoment>,
    }
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    (start < end)
        .then(|| serde_json::from_str::<Shape>(&reply[start..=end]).ok())
        .flatten()
        .map(|shape| shape.moments)
}

// ─── People ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct People {
    by_name: HashMap<String, String>,
    /// The last word of a Vietnamese name is the one people use. `None` when
    /// two people share it, since guessing between them is worse than not.
    by_given: HashMap<String, Option<String>>,
    titles: HashMap<String, String>,
}

const HONORIFICS: &[&str] = &["anh ", "chị ", "em ", "bạn ", "cô ", "chú ", "bác ", "ông ", "bà ", "thầy "];

impl People {
    pub fn read(db: &DbBridge) -> AppResult<Self> {
        let mut stmt = db
            .conn()
            .prepare(
                "SELECT COALESCE(NULLIF(stable_id, ''), id), title FROM nodes WHERE node_type = 'person'",
            )
            .map_err(sql)?;
        let pairs: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(0)?)))
            .map_err(sql)?
            .flatten()
            .collect();
        Ok(Self::from_pairs(&pairs))
    }

    /// `(title, identity)` for each person.
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut people = People::default();
        for (title, id) in pairs {
            let name = title.trim().to_lowercase();
            if name.is_empty() {
                continue;
            }
            people.titles.insert(id.clone(), title.trim().to_string());
            people.by_name.insert(name.clone(), id.clone());
            if let Some(given) = name.split_whitespace().last() {
                people
                    .by_given
                    .entry(given.to_string())
                    .and_modify(|found| {
                        if found.as_deref() != Some(id.as_str()) {
                            *found = None;
                        }
                    })
                    .or_insert_with(|| Some(id.clone()));
            }
        }
        people
    }

    pub fn find(&self, name: &str) -> Option<&str> {
        let mut name = name.trim().to_lowercase();
        for honorific in HONORIFICS {
            if let Some(rest) = name.strip_prefix(honorific) {
                name = rest.trim().to_string();
                break;
            }
        }
        if let Some(id) = self.by_name.get(&name) {
            return Some(id);
        }
        let given = name.split_whitespace().last()?;
        self.by_given.get(given)?.as_deref()
    }

    pub fn title(&self, id: &str) -> Option<&str> {
        self.titles.get(id).map(String::as_str)
    }
}

// ─── Settling a reply into items ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evidence {
    pub node: String,
    pub hash: String,
    /// Where the quote sits in what was read, in characters, when it was
    /// found exactly rather than only after tidying whitespace and case.
    #[serde(default)]
    pub span: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Extractor {
    pub version: u32,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Payload {
    pub title: String,
    /// People found in the vault, by identity.
    #[serde(default)]
    pub people: Vec<String>,
    /// Names the model gave that match nobody, kept as written.
    #[serde(default)]
    pub names: Vec<String>,
    /// Where it happened, in the words the entry used. A node only if somebody
    /// later decides it deserves one (§10, question 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(default)]
    pub quote: String,
}

/// One proposed moment, as it is written in a month file (§4.5.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Extracted {
    pub id: String,
    pub kind: String,
    pub happened_from: String,
    pub happened_to: String,
    pub precision: String,
    pub recorded: String,
    pub source: String,
    pub evidence: Vec<Evidence>,
    pub extractor: Extractor,
    pub confidence: f64,
    pub payload: Payload,
    #[serde(default)]
    pub superseded_by: Option<String>,
}

/// What was left out of a reply, and why.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct Dropped {
    /// The quote is not in the note: nothing shows the moment came from it.
    pub no_quote: usize,
    /// No time the code can read, or none at all in a note not about one day.
    pub undated: usize,
    /// After the day it was written: a plan.
    pub not_yet: usize,
    pub untitled: usize,
}

impl Dropped {
    pub fn total(&self) -> usize {
        self.no_quote + self.undated + self.not_yet + self.untitled
    }
}

static LAST_NIGHT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:t(?:ố|o)i\s*qua|(?:đ|d)(?:ê|e)m\s*qua|last night)\b").expect("pattern")
});
static SAME_DAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:h(?:ô|o)m\s*nay|s(?:á|a)ng\s*nay|tr(?:ư|u)a\s*nay|chi(?:ề|e)u\s*nay|t(?:ố|o)i\s*nay|v(?:ừ|u)a\s*(?:xong|m(?:ớ|o)i)|today|tonight|this (?:morning|afternoon|evening)|just now)\b",
    )
    .expect("pattern")
});

/// The span a moment's `when` names, counted from the day the input was written.
pub fn resolve(when_text: &str, input: &Input) -> Option<Span> {
    let words = when_text.trim();
    if words.is_empty() {
        return input.dated_by_day.then(|| Span::day(input.recorded));
    }
    if let Some(span) = when::parse(words) {
        return Some(span);
    }
    if LAST_NIGHT.is_match(words) {
        return Some(Span::day(input.recorded - Duration::days(1)));
    }
    if SAME_DAY.is_match(words) {
        return Some(Span::day(input.recorded));
    }
    asked::span_in(words, input.recorded).map(|asked| asked.span)
}

/// Where `quote` is in `text`: `Some(Some(span))` found exactly, `Some(None)`
/// found once whitespace and case are set aside, `None` not there.
fn find_quote(text: &str, quote: &str) -> Option<Option<[usize; 2]>> {
    let quote = quote
        .trim()
        .trim_matches(|c: char| matches!(c, '"' | '“' | '”' | '\'' | '«' | '»'))
        .trim();
    if quote.chars().count() < 4 {
        return None;
    }
    if let Some(at) = text.find(quote) {
        let start = text[..at].chars().count();
        return Some(Some([start, start + quote.chars().count()]));
    }
    let tidy = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    tidy(text).contains(&tidy(quote)).then_some(None)
}

/// The day a span ends, cut back to the day the input was written.
fn up_to(span: Span, day: NaiveDate) -> Span {
    if span.to <= day {
        return span;
    }
    Span {
        from: span.from,
        to: day,
        precision: if span.from == day { Precision::Day } else { Precision::Range },
    }
}

/// A model's moments, checked, dated and joined to people.
pub fn settle(input: &Input, raw: Vec<RawMoment>, people: &People, model: &str) -> (Vec<Extracted>, Dropped) {
    let mut items = Vec::new();
    let mut dropped = Dropped::default();

    for (n, moment) in raw.into_iter().enumerate() {
        let title = moment.what.trim();
        if title.is_empty() {
            dropped.untitled += 1;
            continue;
        }
        let Some(span) = resolve(&moment.when, input) else {
            dropped.undated += 1;
            continue;
        };
        if span.from > input.recorded {
            dropped.not_yet += 1;
            continue;
        }
        let span = up_to(span, input.recorded);
        let Some(found) = find_quote(&input.text, &moment.quote) else {
            dropped.no_quote += 1;
            continue;
        };

        let mut ids: Vec<String> = input.person_id.iter().cloned().collect();
        let mut names = Vec::new();
        for name in &moment.people {
            match people.find(name) {
                Some(id) if !ids.iter().any(|known| known == id) => ids.push(id.to_string()),
                Some(_) => {}
                None if !name.trim().is_empty() => names.push(name.trim().to_string()),
                None => {}
            }
        }

        let id = blake3::hash(
            format!("{}\0{}\0{}\0{}", input.node_id, input.hash, EXTRACTOR_VERSION, n).as_bytes(),
        )
        .to_hex();
        items.push(Extracted {
            id: format!("x{}", &id[..20]),
            kind: "moment".into(),
            happened_from: when::iso(span.from),
            happened_to: when::iso(span.to),
            precision: span.precision.as_str().into(),
            recorded: when::iso(input.recorded),
            source: "extract".into(),
            evidence: vec![Evidence {
                node: input.node_id.clone(),
                hash: input.hash.clone(),
                span: found,
            }],
            extractor: Extractor {
                version: EXTRACTOR_VERSION,
                model: model.to_string(),
            },
            confidence: moment.confidence.unwrap_or(0.5).clamp(0.0, 1.0),
            payload: Payload {
                title: title.to_string(),
                people: ids,
                names,
                place: Some(moment.place.trim().to_string()).filter(|p| !p.is_empty()),
                quote: moment.quote.trim().to_string(),
            },
            superseded_by: None,
        });
    }
    (items, dropped)
}

// ─── Month files ─────────────────────────────────────────────────

/// That an input was read, whatever it yielded. Without it a note with nothing
/// in it would be read again on every device, forever.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceRun {
    pub node: String,
    pub hash: String,
    pub version: u32,
    pub model: String,
    pub at: String,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub dropped: usize,
    /// How much was read, and how long it took, so the next estimate is a
    /// measurement of this machine rather than a guess.
    #[serde(default)]
    pub chars: usize,
    #[serde(default)]
    pub ms: u64,
}

impl SourceRun {
    pub fn new(input: &Input, model: &str, items: &[Extracted], dropped: usize, ms: u64, now: DateTime<Utc>) -> Self {
        SourceRun {
            node: input.node_id.clone(),
            hash: input.hash.clone(),
            version: EXTRACTOR_VERSION,
            model: model.to_string(),
            at: crate::utils::timestamp::canonical(now),
            items: items.iter().map(|item| item.id.clone()).collect(),
            dropped,
            chars: input.text.chars().count(),
            ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MonthFile {
    format: u32,
    month: String,
    device: String,
    #[serde(default)]
    items: Vec<Extracted>,
    #[serde(default)]
    sources: Vec<SourceRun>,
    /// Transcripts and captions, in the month they were made. See `timeline::media`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    surrogates: Vec<super::media::Surrogate>,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

static MONTH_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})-(\d{2})\.[^/\\]+\.json$").expect("pattern"));

fn month_path(vault_path: &str, month: &str, device: &str) -> PathBuf {
    Path::new(vault_path)
        .join(FOLDER)
        .join(&month[..4])
        .join(format!("{month}.{device}.json"))
}

/// Every month file in the vault, from every device: `(vault-relative, absolute)`.
fn month_files(vault_path: &str) -> Vec<(String, PathBuf)> {
    let root = Path::new(vault_path).join(FOLDER);
    let Ok(years) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for year in years.flatten() {
        let year_name = year.file_name().to_string_lossy().to_string();
        if year_name.len() != 4 || !year_name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(year.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            if MONTH_FILE.captures(&name).is_some_and(|c| c[1] == year_name) {
                out.push((format!("{FOLDER}/{year_name}/{name}"), file.path()));
            }
        }
    }
    out.sort();
    out
}

/// One read-change-write of a file under `Timeline/` at a time, in this
/// process. A reading and a media run write the same month file, and two
/// writers unguarded each keep only their own change.
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn writing() -> std::sync::MutexGuard<'static, ()> {
    WRITING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Readings that failed, in this process: how many times in a row, and when last.
static FAILED: LazyLock<std::sync::Mutex<HashMap<String, (u32, std::time::Instant)>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// The key a failure is remembered by: what was read, not where it lives.
pub fn failure_key(input: &Input) -> String {
    format!("extract:{}", input.hash)
}

pub fn note_failure(key: &str) {
    let mut failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
    let entry = failed.entry(key.to_string()).or_insert((0, std::time::Instant::now()));
    *entry = (entry.0 + 1, std::time::Instant::now());
}

pub fn clear_failure(key: &str) {
    FAILED.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
}

/// How long an automatic run leaves alone something that failed this many
/// times in a row: half an hour, doubling, at most a day.
pub fn backoff(failures: u32) -> std::time::Duration {
    let half_hours = 1u64 << failures.saturating_sub(1).min(8);
    std::time::Duration::from_secs((half_hours * 30 * 60).min(24 * 60 * 60))
}

/// Whether an automatic run should leave this alone for now. A note whose
/// reply is never the JSON asked for would otherwise head the queue for good,
/// costing a model call on every pass and holding back every note behind it.
pub fn resting(key: &str) -> bool {
    FAILED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(key)
        .is_some_and(|(failures, at)| at.elapsed() < backoff(*failures))
}

fn update_month(
    vault_path: &str,
    month: &str,
    device: &str,
    now: DateTime<Utc>,
    change: impl FnOnce(&mut MonthFile),
) -> AppResult<()> {
    let _writing = writing();
    let path = month_path(vault_path, month, device);
    let mut file = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<MonthFile>(&text).map_err(|e| {
            // Never replaced: a file this device cannot read may hold readings
            // that cost hours, and writing over it would lose them for good.
            AppError::General(format!(
                "{} cannot be read ({e}), so nothing was added to it",
                path.display()
            ))
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => MonthFile {
            format: 1,
            month: month.to_string(),
            device: device.to_string(),
            items: Vec::new(),
            sources: Vec::new(),
            surrogates: Vec::new(),
            rest: Map::new(),
        },
        Err(e) => return Err(io(&path, e)),
    };
    change(&mut file);
    stamp(&mut file.rest, now);
    write_json(&path, &file)
}

/// Keep one reading: its items in the months they happened, then the record
/// that it was read. In that order, so a reading cut short is read again
/// rather than recorded as done with nothing to show for it.
pub fn record(vault_path: &str, device: &str, run: SourceRun, items: &[Extracted], now: DateTime<Utc>) -> AppResult<()> {
    let mut by_month: BTreeMap<String, Vec<&Extracted>> = BTreeMap::new();
    for item in items {
        by_month.entry(item.happened_from[..7].to_string()).or_default().push(item);
    }
    for (month, items) in by_month {
        update_month(vault_path, &month, device, now, |file| {
            for item in items {
                if !file.items.iter().any(|kept| kept.id == item.id) {
                    file.items.push(item.clone());
                }
            }
        })?;
    }
    let month = now.format("%Y-%m").to_string();
    update_month(vault_path, &month, device, now, |file| file.sources.push(run))
}

/// Keep a transcript or caption, in the month it was made. See `timeline::media`.
pub fn record_surrogate(
    vault_path: &str,
    device: &str,
    surrogate: super::media::Surrogate,
    now: DateTime<Utc>,
) -> AppResult<()> {
    let month = now.format("%Y-%m").to_string();
    update_month(vault_path, &month, device, now, |file| {
        file.surrogates.retain(|kept| {
            !(kept.hash == surrogate.hash && kept.kind == surrogate.kind && kept.version == surrogate.version)
        });
        file.surrogates.push(surrogate);
    })
}

// ─── Reviews (tier 2) ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub item: String,
    /// `accepted` or `declined`.
    pub decision: String,
    #[serde(default)]
    pub node: String,
    pub at: String,
    /// The moment itself, for one accepted from something that is not a note
    /// and so has no frontmatter to keep it in: a day of a conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moment: Option<Extracted>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewFile {
    format: u32,
    device: String,
    #[serde(default)]
    decisions: Vec<Decision>,
    #[serde(flatten)]
    rest: Map<String, Value>,
}

/// Every decision on every device, by item.
pub fn reviewed(vault_path: &str) -> HashMap<String, Decision> {
    let Ok(files) = std::fs::read_dir(Path::new(vault_path).join(REVIEWS_DIR)) else {
        return HashMap::new();
    };
    let mut out: HashMap<String, Decision> = HashMap::new();
    for file in files.flatten() {
        let path = file.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Some(review) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<ReviewFile>(&text).ok())
        else {
            continue;
        };
        for decision in review.decisions {
            match out.get(&decision.item) {
                Some(kept) if kept.at >= decision.at => {}
                _ => {
                    out.insert(decision.item.clone(), decision);
                }
            }
        }
    }
    out
}

pub fn decide(vault_path: &str, device: &str, decision: Decision, now: DateTime<Utc>) -> AppResult<()> {
    let _writing = writing();
    let path = Path::new(vault_path).join(REVIEWS_DIR).join(format!("{device}.json"));
    let mut file = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<ReviewFile>(&text).map_err(|e| {
            AppError::General(format!("{} cannot be read ({e}), so nothing was added to it", path.display()))
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => ReviewFile {
            format: 1,
            device: device.to_string(),
            decisions: Vec::new(),
            rest: Map::new(),
        },
        Err(e) => return Err(io(&path, e)),
    };
    file.decisions.retain(|kept| kept.item != decision.item);
    file.decisions.push(decision);
    stamp(&mut file.rest, now);
    write_json(&path, &file)
}

/// How a moment's time is written into frontmatter, in the form `when::parse` reads.
pub fn happened_text(item: &Extracted) -> String {
    match item.precision.as_str() {
        "day" => item.happened_from.clone(),
        "month" => item.happened_from[..7].to_string(),
        "year" => item.happened_from[..4].to_string(),
        _ => format!("{}/{}", item.happened_from, item.happened_to),
    }
}

/// The `moments` entry an accepted proposal becomes. See `derive::moments`.
pub fn moment_entry(item: &Extracted) -> Value {
    let mut entry = json!({
        "title": item.payload.title,
        "happened": happened_text(item),
        "people": item.payload.people,
        "extract": item.id,
    });
    if let (Value::Object(fields), Some(place)) = (&mut entry, &item.payload.place) {
        fields.insert("where".into(), Value::String(place.clone()));
    }
    entry
}

// ─── Tier 3 ──────────────────────────────────────────────────────

pub fn ensure_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS extract_runs (
            month_file TEXT NOT NULL,
            device     TEXT NOT NULL,
            node_id    TEXT NOT NULL,
            hash       TEXT NOT NULL,
            version    INTEGER NOT NULL,
            model      TEXT NOT NULL,
            at         TEXT NOT NULL,
            items      TEXT NOT NULL,
            dropped    INTEGER NOT NULL,
            chars      INTEGER NOT NULL,
            ms         INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_extract_runs_node ON extract_runs(node_id);
        CREATE TABLE IF NOT EXISTS media_surrogates (
            month_file TEXT NOT NULL,
            device     TEXT NOT NULL,
            node_id    TEXT NOT NULL,
            hash       TEXT NOT NULL,
            kind       TEXT NOT NULL,
            version    INTEGER NOT NULL,
            model      TEXT NOT NULL,
            at         TEXT NOT NULL,
            text       TEXT NOT NULL,
            segments   TEXT NOT NULL,
            language   TEXT,
            duration   REAL,
            -- Put where file search reads text, on this device. See `media::mirror_into_search`.
            mirrored   INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_media_surrogates_node ON media_surrogates(node_id);",
    )
    .map_err(sql)
}

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct Loaded {
    pub files_read: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    /// Month files that could not be read, and why. Their earlier contents are
    /// kept rather than dropped.
    pub unreadable: Vec<String>,
}

/// Bring `timeline.db` in line with the month files and reviews in the vault.
///
/// Only a file whose bytes changed is read again, so a sync that brings one
/// device's May costs one file (§4.5.2). No model is involved: losing
/// `timeline.db` costs this and nothing more.
pub fn load(conn: &Connection, vault_path: &str) -> AppResult<Loaded> {
    ensure_schema(conn)?;
    let known: HashMap<String, String> = {
        let mut stmt = conn
            .prepare("SELECT path, content_hash FROM month_files")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?;
        rows.flatten().collect()
    };

    let tx = conn.unchecked_transaction().map_err(sql)?;
    let mut report = Loaded::default();
    let mut present = HashSet::new();
    let now = crate::utils::timestamp::canonical(Utc::now());

    for (rel, abs) in month_files(vault_path) {
        present.insert(rel.clone());
        let Ok(bytes) = std::fs::read(&abs) else {
            continue;
        };
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        if known.get(&rel) == Some(&content_hash) {
            report.files_unchanged += 1;
            continue;
        }
        let file: MonthFile = match serde_json::from_slice(&bytes) {
            Ok(file) => file,
            Err(e) => {
                report.unreadable.push(format!("{rel}: {e}"));
                continue;
            }
        };
        forget(&tx, &rel)?;
        for item in &file.items {
            tx.execute(
                "INSERT OR REPLACE INTO events
                    (id, kind, happened_from, happened_to, precision, time_source, recorded_at,
                     node_id, node_type, title, label, related_id, source, confidence, evidence, month_file,
                     magnitude, container_node)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'extract', ?6, ?7, '', ?8, NULL, ?9, 'extract', ?10, ?11, ?12, ?13, ?7)",
                // Per file: two devices, or a sync conflict copy, can hold the
                // same item, and forgetting one file must not take the other's row.
                params![
                    format!("{}@{}", item.id, rel),
                    item.kind,
                    item.happened_from,
                    item.happened_to,
                    item.precision,
                    item.recorded,
                    item.evidence.first().map(|e| e.node.as_str()).unwrap_or(""),
                    item.payload.title,
                    item.payload.people.first(),
                    item.confidence,
                    serde_json::to_string(item).unwrap_or_default(),
                    rel,
                    magnitude::of(Signals {
                        from: &item.happened_from,
                        to: &item.happened_to,
                        people: item.payload.people.len(),
                        evidence: item.evidence.len(),
                        text: &item.payload.title,
                        source: "extract",
                    }),
                ],
            )
            .map_err(sql)?;
            // Everyone it named, and what it was read from. See
            // `docs/su-kien-2026-09-16.md` §3.2.
            let row_id = format!("{}@{}", item.id, rel);
            tx.execute("DELETE FROM event_links WHERE event_id = ?1", params![row_id])
                .map_err(sql)?;
            for person in &item.payload.people {
                tx.execute(
                    "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label) VALUES (?1, ?2, 'with', NULL)",
                    params![row_id, person],
                )
                .map_err(sql)?;
            }
            for evidence in &item.evidence {
                tx.execute(
                    "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label) VALUES (?1, ?2, 'evidence', NULL)",
                    params![row_id, evidence.node],
                )
                .map_err(sql)?;
            }
            if let Some(place) = &item.payload.place {
                tx.execute(
                    "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label) VALUES (?1, ?2, 'where', NULL)",
                    params![row_id, place],
                )
                .map_err(sql)?;
            }
        }
        for run in &file.sources {
            tx.execute(
                "INSERT INTO extract_runs (month_file, device, node_id, hash, version, model, at, items, dropped, chars, ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    rel,
                    file.device,
                    run.node,
                    run.hash,
                    run.version,
                    run.model,
                    run.at,
                    serde_json::to_string(&run.items).unwrap_or_default(),
                    run.dropped as i64,
                    run.chars as i64,
                    run.ms as i64,
                ],
            )
            .map_err(sql)?;
        }
        for surrogate in &file.surrogates {
            tx.execute(
                "INSERT INTO media_surrogates (month_file, device, node_id, hash, kind, version, model, at, text, segments, language, duration)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    rel,
                    file.device,
                    surrogate.node,
                    surrogate.hash,
                    surrogate.kind,
                    surrogate.version,
                    surrogate.model,
                    surrogate.at,
                    surrogate.text,
                    serde_json::to_string(&surrogate.segments).unwrap_or_default(),
                    surrogate.language,
                    surrogate.duration,
                ],
            )
            .map_err(sql)?;
        }
        tx.execute(
            "INSERT INTO month_files (path, content_hash, loaded_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET content_hash = excluded.content_hash, loaded_at = excluded.loaded_at",
            params![rel, content_hash, now],
        )
        .map_err(sql)?;
        report.files_read += 1;
    }

    for gone in known.keys().filter(|path| !present.contains(*path)) {
        forget(&tx, gone)?;
        tx.execute("DELETE FROM month_files WHERE path = ?1", params![gone])
            .map_err(sql)?;
        report.files_removed += 1;
    }

    // Moments accepted from something with no frontmatter of its own. Few,
    // and read whole each time.
    tx.execute(
        "DELETE FROM event_links WHERE event_id IN
            (SELECT id FROM events WHERE source = 'user' AND time_source = 'review')",
        [],
    )
    .map_err(sql)?;
    tx.execute(
        "DELETE FROM events WHERE source = 'user' AND time_source = 'review'",
        [],
    )
    .map_err(sql)?;
    for decision in reviewed(vault_path).into_values() {
        let (Some(moment), "accepted") = (decision.moment, decision.decision.as_str()) else {
            continue;
        };
        tx.execute(
            "INSERT OR REPLACE INTO events
                (id, kind, happened_from, happened_to, precision, time_source, recorded_at,
                 node_id, node_type, title, label, related_id, source, confidence, magnitude, container_node)
             VALUES (?1, 'moment', ?2, ?3, ?4, 'review', ?5, ?6, 'syn_conversation', ?7, ?7, ?8, 'user', ?9, ?10, ?6)",
            params![
                format!("{}#accepted", moment.id),
                moment.happened_from,
                moment.happened_to,
                moment.precision,
                moment.recorded,
                decision.node,
                moment.payload.title,
                moment.payload.people.first(),
                moment.confidence,
                magnitude::of(Signals {
                    from: &moment.happened_from,
                    to: &moment.happened_to,
                    people: moment.payload.people.len(),
                    evidence: moment.evidence.len(),
                    text: &moment.payload.title,
                    source: "user",
                }),
            ],
        )
        .map_err(sql)?;

        // Everyone it named, not just the first. A moment kept from a
        // conversation cannot be derived from the vault again, so this is the
        // only place these links are written — and `Seals::hides_item` reads
        // them to decide whether the moment may be shown at all.
        let row_id = format!("{}#accepted", moment.id);
        for person in &moment.payload.people {
            tx.execute(
                "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label) VALUES (?1, ?2, 'with', NULL)",
                params![row_id, person],
            )
            .map_err(sql)?;
        }
        for evidence in &moment.evidence {
            tx.execute(
                "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label) VALUES (?1, ?2, 'evidence', NULL)",
                params![row_id, evidence.node],
            )
            .map_err(sql)?;
        }
        if let Some(place) = &moment.payload.place {
            tx.execute(
                "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label) VALUES (?1, ?2, 'where', NULL)",
                params![row_id, place],
            )
            .map_err(sql)?;
        }
    }

    tx.commit().map_err(sql)?;
    Ok(report)
}

fn forget(conn: &Connection, rel: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM event_links WHERE event_id IN
            (SELECT id FROM events WHERE month_file = ?1 AND source = 'extract')",
        params![rel],
    )
    .map_err(sql)?;
    conn.execute(
        "DELETE FROM events WHERE month_file = ?1 AND source = 'extract'",
        params![rel],
    )
    .map_err(sql)?;
    conn.execute("DELETE FROM extract_runs WHERE month_file = ?1", params![rel])
        .map_err(sql)?;
    conn.execute("DELETE FROM media_surrogates WHERE month_file = ?1", params![rel])
        .map_err(sql)?;
    Ok(())
}

/// One proposed moment, by id.
pub fn item(conn: &Connection, id: &str) -> AppResult<Option<Extracted>> {
    ensure_schema(conn)?;
    let found = conn
        .query_row(
            "SELECT evidence FROM events
             WHERE substr(id, 1, length(?1) + 1) = ?1 || '@' AND source = 'extract'
             LIMIT 1",
            params![id],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten();
    Ok(found.and_then(|json| serde_json::from_str(&json).ok()))
}

/// What is left to read, and what was read before.
#[derive(Debug, Default)]
pub struct Plan {
    /// Never read by any device.
    pub pending: Vec<Input>,
    /// Read, then edited. Its proposals stand, marked; nothing reads it again
    /// until asked.
    pub stale: Vec<Input>,
    /// Read as it is now, by an older version of the extractor.
    pub old_version: Vec<Input>,
    pub done: usize,
}

pub fn plan(conn: &Connection, inputs: Vec<Input>) -> AppResult<Plan> {
    ensure_schema(conn)?;
    let mut read_at: HashSet<String> = HashSet::new();
    let mut by_hash: HashMap<String, Vec<u32>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT node_id, hash, version FROM extract_runs")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, u32>(2)?)))
            .map_err(sql)?;
        for (node, hash, version) in rows.flatten() {
            read_at.insert(node);
            by_hash.entry(hash).or_default().push(version);
        }
    }

    let mut plan = Plan::default();
    for input in inputs {
        // By what was read, wherever the note lives now: moving or renaming a
        // note is no reason to read it again, on this device or any other.
        match by_hash.get(&input.hash) {
            Some(versions) if versions.contains(&EXTRACTOR_VERSION) => plan.done += 1,
            Some(_) => plan.old_version.push(input),
            None if read_at.contains(&input.node_id) => plan.stale.push(input),
            None => plan.pending.push(input),
        }
    }
    Ok(plan)
}

/// Roughly how long reading `chars` characters takes, and whether that is
/// measured on this device or only a starting guess.
///
/// The guess is deliberately slow for a local model: an estimate that runs
/// short is how "a few minutes" becomes a laptop fan for an hour.
pub fn estimate_ms(conn: &Connection, device: &str, chars: usize, local: bool) -> AppResult<(u64, bool)> {
    ensure_schema(conn)?;
    let (ms, read): (i64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(ms), 0), COALESCE(SUM(chars), 0) FROM extract_runs
             WHERE device = ?1 AND ms > 0 AND chars > 0",
            params![device],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql)?;
    if read >= 2_000 {
        return Ok(((chars as f64 * ms as f64 / read as f64) as u64, true));
    }
    let per_char = if local { 12.0 } else { 3.0 };
    Ok(((chars as f64 * per_char) as u64, false))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PersonRef {
    pub id: String,
    pub title: String,
}

/// One reading of a note: fingerprint, extractor version, when, item ids.
type Reading = (String, u32, String, Vec<String>);

/// What makes two proposals the same moment: the same title over the same
/// days, or the same words quoted.
fn same_moment_keys(item: &Extracted) -> Vec<String> {
    let tidy = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let mut keys = vec![format!("t:{}|{}|{}", tidy(&item.payload.title), item.happened_from, item.happened_to)];
    if !item.payload.quote.trim().is_empty() {
        keys.push(format!("q:{}", tidy(&item.payload.quote)));
    }
    keys
}

/// Whether a note still says what a proposal was read from.
pub fn still_reads_as_read(content: &str, item: &Extracted) -> bool {
    item.evidence.first().is_none_or(|e| e.hash == fingerprint(content.trim()))
}

/// A note's `moments` after keeping a proposal into it, from the frontmatter
/// on disk. Keeping the same moment twice, by id or by what it says, adds it once.
pub fn moments_after_keeping(on_disk: &Map<String, Value>, item: &Extracted) -> Vec<Value> {
    let mut moments = on_disk.get("moments").and_then(Value::as_array).cloned().unwrap_or_default();
    let entry = moment_entry(item);
    let lower = |v: Option<&Value>| v.and_then(Value::as_str).map(|t| t.trim().to_lowercase());
    let already = moments.iter().any(|m| {
        m.get("extract").and_then(Value::as_str) == Some(item.id.as_str())
            || (lower(m.get("title")) == lower(entry.get("title")) && m.get("happened") == entry.get("happened"))
    });
    if !already {
        moments.push(entry);
    }
    moments
}

/// A proposal as the tray shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Proposal {
    pub id: String,
    pub node_id: String,
    pub node_title: String,
    pub node_type: String,
    pub recorded: String,
    pub happened_from: String,
    pub happened_to: String,
    pub precision: String,
    pub title: String,
    pub people: Vec<PersonRef>,
    pub names: Vec<String>,
    pub quote: String,
    pub confidence: f64,
    pub model: String,
    /// The note was edited after this was read from it.
    pub stale: bool,
}

/// What waits for a decision: for each input read as it is still eligible,
/// the items of its most fitting reading, less what was decided or sealed.
pub fn proposals(
    conn: &Connection,
    inputs: &[Input],
    decided: &HashMap<String, Decision>,
    seals: &Seals,
    people: &People,
) -> AppResult<Vec<Proposal>> {
    ensure_schema(conn)?;
    let mut runs: HashMap<String, Vec<Reading>> = HashMap::new();
    let mut by_hash: HashMap<String, Vec<Reading>> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT node_id, hash, version, at, items FROM extract_runs")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(sql)?;
        for (node, hash, version, at, items) in rows.flatten() {
            let items: Vec<String> = serde_json::from_str(&items).unwrap_or_default();
            by_hash.entry(hash.clone()).or_default().push((hash.clone(), version, at.clone(), items.clone()));
            runs.entry(node).or_default().push((hash, version, at, items));
        }
    }

    // What was decided about each note, by what it said. A moment read again
    // from an edited note, or by a newer extractor, has a new id and the same
    // words, and was decided already.
    let mut decided_about: HashMap<String, HashSet<String>> = HashMap::new();
    for id in decided.keys() {
        if let Some(earlier) = item(conn, id)? {
            let node = earlier.evidence.first().map(|e| e.node.clone()).unwrap_or_default();
            decided_about.entry(node).or_default().extend(same_moment_keys(&earlier));
        }
    }

    let mut out = Vec::new();
    for input in inputs {
        let mut read: Vec<&Reading> = runs.get(&input.node_id).into_iter().flatten().collect();
        read.extend(by_hash.get(&input.hash).into_iter().flatten());
        if read.is_empty() {
            continue;
        }
        let current = read
            .iter()
            .copied()
            .filter(|(hash, ..)| *hash == input.hash)
            .max_by(|a, b| (a.1, &a.2).cmp(&(b.1, &b.2)));
        let (run, stale) = match current {
            Some(run) => (run, false),
            None => match read.iter().copied().max_by(|a, b| a.2.cmp(&b.2)) {
                Some(run) => (run, true),
                None => continue,
            },
        };
        for id in &run.3 {
            if decided.contains_key(id) {
                continue;
            }
            let Some(extracted) = item(conn, id)? else {
                continue;
            };
            let evidence_node = extracted.evidence.first().map(|e| e.node.as_str()).unwrap_or_default();
            let keys = same_moment_keys(&extracted);
            if [input.node_id.as_str(), evidence_node]
                .iter()
                .any(|node| decided_about.get(*node).is_some_and(|known| keys.iter().any(|k| known.contains(k))))
            {
                continue;
            }
            let as_item = Event {
                id: extracted.id.clone(),
                kind: "moment".into(),
                node_id: input.node_id.split('#').next().unwrap_or_default().to_string(),
                node_type: input.node_type.clone(),
                title: extracted.payload.title.clone(),
                label: None,
                related_id: extracted.payload.people.first().cloned(),
                links: Vec::new(),
                magnitude: 0.0,
                container_node: None,
                props: serde_json::Value::Null,
                happened_from: extracted.happened_from.clone(),
                happened_to: extracted.happened_to.clone(),
                precision: extracted.precision.clone(),
                time_source: "extract".into(),
                source: "extract".into(),
            };
            if seals.hides_item(&as_item) || extracted.payload.people.iter().any(|p| seals.hides(p)) {
                continue;
            }
            out.push(Proposal {
                id: extracted.id.clone(),
                node_id: input.node_id.clone(),
                node_title: input.title.clone(),
                node_type: input.node_type.clone(),
                recorded: extracted.recorded.clone(),
                happened_from: extracted.happened_from.clone(),
                happened_to: extracted.happened_to.clone(),
                precision: extracted.precision.clone(),
                title: extracted.payload.title.clone(),
                people: extracted
                    .payload
                    .people
                    .iter()
                    .map(|id| PersonRef {
                        id: id.clone(),
                        title: people.title(id).unwrap_or(id).to_string(),
                    })
                    .collect(),
                names: extracted.payload.names.clone(),
                quote: extracted.payload.quote.clone(),
                confidence: extracted.confidence,
                model: extracted.extractor.model.clone(),
                stale,
            });
        }
    }
    out.sort_by(|a, b| b.happened_from.cmp(&a.happened_from).then_with(|| a.id.cmp(&b.id)));
    Ok(out)
}

// ─── Running ─────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct ExtractRun {
    /// Inputs read and recorded.
    pub read: usize,
    pub items: usize,
    pub dropped: usize,
    pub failed: Vec<String>,
    /// Left for a later run by the limit.
    pub remaining: usize,
    /// Why nothing was read, when an automatic run chose not to.
    pub skipped: Option<String>,
}

/// Read one input.
pub async fn extract_one(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    input: &Input,
    people: &People,
    now: DateTime<Utc>,
) -> AppResult<(Vec<Extracted>, SourceRun)> {
    let started = std::time::Instant::now();
    let messages = vec![ChatMessage::new("user", prompt(input))];
    let reply = provider
        .chat(ChatRequest {
            model,
            messages: &messages,
            temperature: Some(0.0),
            num_ctx,
            tools: None,
        })
        .await?;
    let raw = parse_reply(&reply.content).ok_or_else(|| {
        AppError::General(format!("the reply for {} was not the JSON asked for", input.node_id))
    })?;
    let (items, dropped) = settle(input, raw, people, model);
    let ms = reply
        .duration_ms
        .unwrap_or_else(|| started.elapsed().as_millis() as u64);
    let run = SourceRun::new(input, model, &items, dropped.total(), ms, now);
    Ok((items, run))
}

/// Read each input in turn and keep what each yields as soon as it is read,
/// so stopping halfway loses one reading at most.
pub async fn extract_all(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    inputs: &[Input],
    people: &People,
    vault_path: &str,
    device: &str,
) -> ExtractRun {
    let mut report = ExtractRun::default();
    for input in inputs {
        match extract_one(provider, model, num_ctx, input, people, Utc::now()).await {
            Ok((items, run)) => {
                let (count, dropped) = (items.len(), run.dropped);
                match record(vault_path, device, run, &items, Utc::now()) {
                    Ok(()) => {
                        clear_failure(&failure_key(input));
                        report.read += 1;
                        report.items += count;
                        report.dropped += dropped;
                    }
                    Err(e) => {
                        note_failure(&failure_key(input));
                        report.failed.push(format!("{}: {e}", input.node_id))
                    }
                }
            }
            Err(e) => {
                note_failure(&failure_key(input));
                report.failed.push(format!("{}: {e}", input.node_id))
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_that_keeps_failing_rests_between_automatic_runs() {
        let key = "extract:test-failing";
        assert!(!resting(key));
        note_failure(key);
        assert!(resting(key));
        clear_failure(key);
        assert!(!resting(key));
        assert_eq!(backoff(1).as_secs(), 30 * 60);
        assert_eq!(backoff(2).as_secs(), 60 * 60);
        assert_eq!(backoff(20).as_secs(), 24 * 60 * 60);
    }

    #[test]
    fn two_writers_to_one_month_file_both_keep_their_change() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let now = Utc::now();
        let writers: Vec<_> = (0..8)
            .map(|n| {
                let vault = vault.clone();
                std::thread::spawn(move || {
                    let surrogate = crate::timeline::media::Surrogate {
                        node: format!("Files/{n:08}.md"),
                        hash: format!("{n:08}"),
                        kind: "caption".into(),
                        version: 1,
                        model: "m".into(),
                        at: String::new(),
                        text: format!("picture {n}"),
                        segments: Vec::new(),
                        language: None,
                        duration: None,
                        ms: 0,
                    };
                    record_surrogate(&vault, "dev", surrogate, now).unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let path = month_path(&vault, &now.format("%Y-%m").to_string(), "dev");
        let file: MonthFile = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(file.surrogates.len(), 8);
    }

    #[test]
    fn a_moved_note_is_not_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let read = input("note", "2024-06-02", true, DAILY);
        let (items, _) = settle(&read, parse_reply(REPLY).unwrap(), &People::default(), "m");
        record(&vault, "dev", SourceRun::new(&read, "m", &items, 0, 1, Utc::now()), &items, Utc::now()).unwrap();
        let store = TimelineStore::open_in_memory().unwrap();
        load(store.conn(), &vault).unwrap();

        let moved = Input { node_id: "Journal/2024-06-02.md".into(), ..read.clone() };
        let plan = plan(store.conn(), vec![moved.clone()]).unwrap();
        assert_eq!((plan.pending.len(), plan.done), (0, 1));
        let seals = Seals::read(&DbBridge::new_in_memory_full().unwrap(), &vault).unwrap();
        let shown = proposals(store.conn(), &[moved], &HashMap::new(), &seals, &People::default()).unwrap();
        assert_eq!(shown.len(), 1, "its proposals follow it");
        assert_eq!(shown[0].node_id, "Journal/2024-06-02.md", "and are kept into it where it is now");
    }

    #[test]
    fn a_moment_decided_before_is_not_offered_again_after_an_edit() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let first = input("note", "2024-06-02", true, DAILY);
        let (items, _) = settle(&first, parse_reply(REPLY).unwrap(), &People::default(), "m");
        record(&vault, "dev", SourceRun::new(&first, "m", &items, 0, 1, Utc::now()), &items, Utc::now()).unwrap();
        decide(&vault, "dev", Decision { item: items[0].id.clone(), decision: "declined".into(), node: first.node_id.clone(), at: "2026-09-15T00:00:00.000Z".into(), moment: None }, Utc::now()).unwrap();

        let edited = input("note", "2024-06-02", true, &format!("{DAILY} Tối về ngủ sớm."));
        let (again, _) = settle(&edited, parse_reply(REPLY).unwrap(), &People::default(), "m");
        assert_ne!(again[0].id, items[0].id);
        record(&vault, "dev", SourceRun::new(&edited, "m", &again, 0, 1, Utc::now()), &again, Utc::now()).unwrap();

        let store = TimelineStore::open_in_memory().unwrap();
        load(store.conn(), &vault).unwrap();
        let seals = Seals::read(&DbBridge::new_in_memory_full().unwrap(), &vault).unwrap();
        let shown = proposals(store.conn(), &[edited], &reviewed(&vault), &seals, &People::default()).unwrap();
        assert!(shown.is_empty(), "{shown:?}");
    }

    #[test]
    fn forgetting_a_conflict_copy_keeps_the_rows_of_the_file_it_copied() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let read = input("note", "2024-06-02", true, DAILY);
        let (items, _) = settle(&read, parse_reply(REPLY).unwrap(), &People::default(), "m");
        let now = DateTime::parse_from_rfc3339("2024-06-03T00:00:00Z").unwrap().with_timezone(&Utc);
        record(&vault, "dev", SourceRun::new(&read, "m", &items, 0, 1, now), &items, now).unwrap();
        let original = Path::new(&vault).join("Timeline/2024/2024-06.dev.json");
        let copy = Path::new(&vault).join("Timeline/2024/2024-06.dev (conflict 1).json");
        std::fs::copy(&original, &copy).unwrap();

        let store = TimelineStore::open_in_memory().unwrap();
        load(store.conn(), &vault).unwrap();
        std::fs::remove_file(&copy).unwrap();
        load(store.conn(), &vault).unwrap();
        assert!(item(store.conn(), &items[0].id).unwrap().is_some());
    }

    #[test]
    fn keeping_reads_the_note_as_it_is_now() {
        let read = input("note", "2024-06-02", true, DAILY);
        let (items, _) = settle(&read, parse_reply(REPLY).unwrap(), &People::default(), "m");
        assert!(still_reads_as_read(DAILY, &items[0]));
        assert!(!still_reads_as_read("Chỉ còn một câu khác hẳn.", &items[0]));

        let arrived: Map<String, Value> = serde_json::from_value(json!({
            "title": "2024-06-02",
            "moments": [{ "title": "Chuyến đi Huế", "happened": "2024-05", "extract": "x-other-device" }]
        }))
        .unwrap();
        let once = moments_after_keeping(&arrived, &items[0]);
        assert_eq!(once.len(), 2, "what arrived from another device stays");
        let mut again = arrived.clone();
        again.insert("moments".into(), Value::Array(once.clone()));
        let reread = Extracted { id: "x-reread".into(), ..items[0].clone() };
        assert_eq!(moments_after_keeping(&again, &reread).len(), 2, "the same moment read again is not added twice");
    }
    use crate::models::node::NodeMetadata;
    use crate::models::syn::{ModelInfo, ProviderStatus, SynProvider};
    use crate::syn::provider::{ChatReply, StreamSink};
    use crate::timeline::TimelineStore;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn input(node_type: &str, recorded: &str, by_day: bool, text: &str) -> Input {
        make_input(
            format!("Notes/{recorded}.md"),
            node_type.into(),
            recorded.into(),
            text,
            day(recorded),
            by_day,
            None,
        )
        .expect("long enough")
    }

    fn moment(what: &str, when_text: &str, people: &[&str], quote: &str) -> RawMoment {
        RawMoment {
            what: what.into(),
            when: when_text.into(),
            people: people.iter().map(|p| p.to_string()).collect(),
            quote: quote.into(),
            confidence: Some(0.8),
            ..RawMoment::default()
        }
    }

    fn spans(items: &[Extracted]) -> Vec<(String, String, String)> {
        items
            .iter()
            .map(|i| (i.happened_from.clone(), i.happened_to.clone(), i.precision.clone()))
            .collect()
    }

    fn node(id: &str, node_type: &str, content: &str, properties: Value) -> NodeMetadata {
        NodeMetadata {
            id: id.into(),
            node_type: node_type.into(),
            title: id.rsplit('/').next().unwrap_or(id).trim_end_matches(".md").into(),
            content: content.into(),
            properties,
            created_at: "2026-09-01T08:00:00.000Z".into(),
            updated_at: "2026-09-01T08:00:00.000Z".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn vault() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        (dir, path)
    }

    const DAILY: &str = "Hôm qua đưa mẹ đi khám mắt ở bệnh viện, bác sĩ bảo phải mổ. Tuần trước ký hợp đồng nhà mới. Sáng nay chạy 5km.";

    /// Gate: a relative `happened` is counted from the day it was written.
    #[test]
    fn relative_time_is_counted_from_the_day_it_was_written() {
        // 2024-06-02 is a Sunday.
        let daily = input("note", "2024-06-02", true, DAILY);
        let raw = vec![
            moment("Khám mắt", "hôm qua", &[], "Hôm qua đưa mẹ đi khám mắt"),
            moment("Ký hợp đồng", "tuần trước", &[], "Tuần trước ký hợp đồng nhà mới"),
            moment("Chạy bộ", "sáng nay", &[], "Sáng nay chạy 5km"),
            moment("Chạy bộ", "", &[], "Sáng nay chạy 5km"),
            moment("Cả tháng", "tháng này", &[], "Sáng nay chạy 5km"),
            moment("Năm 2024", "2024", &[], "Sáng nay chạy 5km"),
        ];
        let (items, dropped) = settle(&daily, raw, &People::default(), "m");
        assert_eq!(dropped, Dropped::default());
        assert_eq!(
            spans(&items),
            vec![
                ("2024-06-01".into(), "2024-06-01".into(), "day".into()),
                ("2024-05-20".into(), "2024-05-26".into(), "range".into()),
                ("2024-06-02".into(), "2024-06-02".into(), "day".into()),
                ("2024-06-02".into(), "2024-06-02".into(), "day".into()),
                ("2024-06-01".into(), "2024-06-02".into(), "range".into()),
                // A whole year is cut back to the day it was written: the rest had not happened.
                ("2024-01-01".into(), "2024-06-02".into(), "range".into()),
            ]
        );
    }

    #[test]
    fn a_time_that_cannot_be_read_or_has_not_come_is_left_out() {
        let about_someone = input("person", "2024-01-01", false, DAILY);
        let raw = vec![
            moment("No time", "", &[], "Sáng nay chạy 5km"),
            moment("School", "hồi cấp 3", &[], "Sáng nay chạy 5km"),
            moment("Plan", "2025-03", &[], "Sáng nay chạy 5km"),
            moment("", "2020", &[], "Sáng nay chạy 5km"),
        ];
        let (items, dropped) = settle(&about_someone, raw, &People::default(), "m");
        assert!(items.is_empty());
        assert_eq!(dropped, Dropped { no_quote: 0, undated: 2, not_yet: 1, untitled: 1 });
    }

    #[test]
    fn a_quote_the_entry_does_not_hold_is_no_evidence() {
        let daily = input("note", "2024-06-02", true, DAILY);
        let raw = vec![
            moment("Invented", "hôm qua", &[], "Hôm qua đi Đà Lạt với cả nhà"),
            moment("Tidied", "hôm qua", &[], "  hôm qua   ĐƯA mẹ đi khám mắt "),
            moment("Exact", "hôm qua", &[], "\"Hôm qua đưa mẹ đi khám mắt\""),
        ];
        let (items, dropped) = settle(&daily, raw, &People::default(), "m");
        assert_eq!(dropped.no_quote, 1);
        assert_eq!(items[0].evidence[0].span, None, "found only after tidying");
        assert_eq!(items[1].evidence[0].span, Some([0, 26]), "counted in characters, not bytes");
    }

    #[test]
    fn names_are_joined_to_people_and_kept_as_written_when_nobody_matches() {
        let people = People::from_pairs(&[
            ("Nguyễn Thu Hà".into(), "uuid-ha".into()),
            ("Trần Minh Tuấn".into(), "uuid-tuan".into()),
            ("Lê Văn Tuấn".into(), "uuid-tuan-2".into()),
        ]);
        assert_eq!(people.find("chị Hà"), Some("uuid-ha"));
        assert_eq!(people.find("trần minh tuấn"), Some("uuid-tuan"));
        assert_eq!(people.find("Tuấn"), None, "two Tuấns: not a guess");

        let mut coffee = input("interaction", "2024-06-02", true, DAILY);
        coffee.person_id = Some("uuid-tuan".into());
        let (items, _) = settle(
            &coffee,
            vec![moment("Khám mắt", "hôm qua", &["Hà", "Tuấn", "bác sĩ Long"], "Hôm qua đưa mẹ")],
            &people,
            "m",
        );
        assert_eq!(items[0].payload.people, vec!["uuid-tuan", "uuid-ha"], "the interaction's person took part");
        assert_eq!(items[0].payload.names, vec!["Tuấn", "bác sĩ Long"]);
    }

    #[test]
    fn a_reply_is_read_through_fences_and_chatter() {
        let reply = "Here you go:\n```json\n{\"moments\": [{\"what\": \"Chạy\", \"when\": \"sáng nay\", \"people\": [], \"quote\": \"chạy 5km\"}]}\n```";
        assert_eq!(parse_reply(reply).unwrap()[0].what, "Chạy");
        assert_eq!(parse_reply("{\"moments\": []}"), Some(vec![]));
        assert_eq!(parse_reply("I could not find anything."), None);
    }

    #[test]
    fn what_is_read_by_default_follows_the_matrix() {
        let off = Config::default();
        let dated = json!({ "date": "2024-06-02" });
        assert!(wanted("note", &dated, "Daily/2024-06-02.md", &off), "daily note");
        for t in ["interaction", "person", "quickcap", "event"] {
            assert!(wanted(t, &json!({}), "x.md", &off), "{t}");
        }
        assert!(!wanted("note", &json!({}), "Reading/essay.md", &off), "ordinary note");
        assert!(!wanted("syn_memory", &json!({ "timeline": true }), "m.md", &off), "never Syn's own records");
        assert!(!wanted("task", &json!({}), "t.md", &off));

        assert!(wanted("note", &json!({ "timeline": true }), "Reading/essay.md", &off));
        assert!(!wanted("note", &json!({ "date": "2024-06-02", "timeline": false }), "d.md", &off));

        let on = Config {
            folders: vec!["Journal/".into()],
            tags: vec!["#life".into()],
            ..Config::default()
        };
        assert!(wanted("note", &json!({}), "Journal/trip.md", &on));
        assert!(!wanted("note", &json!({}), "JournalOld/trip.md", &on));
        assert!(wanted("note", &json!({ "tags": ["Life"] }), "Other/x.md", &on));
    }

    /// Gate: no assistant turn and nothing sealed reaches the extraction
    /// prompt. Checked on what is sent, not on what comes back.
    #[test]
    fn the_extraction_prompt_never_holds_what_syn_said_or_anything_sealed() {
        let (_dir, vault_path) = vault();
        let db = DbBridge::new_in_memory_full().expect("schema");
        let long = "đủ dài để được đọc, vì một ghi chép ngắn hơn thì không có gì để đọc cả";
        for n in [
            node("Notes/open.md", "note", &format!("Đi ăn tối với Tuấn ở quán cũ, {long}."), json!({ "date": "2026-09-10" })),
            node("Notes/sealed.md", "note", &format!("MARK-SEALED-NOTE {long}"), json!({ "date": "2026-09-11", "sealed": true })),
            node("Notes/in-period.md", "note", &format!("MARK-SEALED-PERIOD {long}"), json!({ "date": "2019-02-10" })),
            node("Notes/off.md", "note", &format!("MARK-TIMELINE-FALSE {long}"), json!({ "date": "2026-09-12", "timeline": false })),
            node("Notes/essay.md", "note", &format!("MARK-ORDINARY-NOTE {long}"), json!({})),
        ] {
            db.upsert_node(&n).expect("seeded");
        }
        crate::timeline::seal::write_period(&vault_path, "2019-02", "2019-02").expect("period sealed");
        std::fs::create_dir_all(Path::new(&vault_path).join("Syn")).unwrap();
        std::fs::write(
            Path::new(&vault_path).join("Syn/c1.json"),
            json!({
                "id": "c1",
                "title": "Nhà mới",
                "messages": [
                    { "id": "1", "role": "user", "content": format!("Hôm qua tao chuyển sang nhà mới ở Cầu Giấy, {long}."), "timestamp": "2026-09-14T03:00:00Z" },
                    { "id": "2", "role": "assistant", "content": format!("MARK-WHAT-SYN-SAID chúc mừng {long}"), "timestamp": "2026-09-14T03:00:05Z" },
                    { "id": "3", "role": "system", "content": format!("MARK-SYSTEM {long}"), "timestamp": "2026-09-14T03:00:00Z" }
                ]
            })
            .to_string(),
        )
        .unwrap();

        let seals = Seals::read(&db, &vault_path).expect("seals");
        let config = Config { conversations: true, ..Config::default() };
        let read = inputs(&db, &vault_path, &config, &seals, day("2026-09-15"), None).expect("inputs");
        let sent: String = read.iter().map(prompt).collect::<Vec<_>>().join("\n");

        assert!(sent.contains("Tuấn") && sent.contains("Cầu Giấy"), "what should be read is: {sent}");
        for mark in ["MARK-WHAT-SYN-SAID", "MARK-SYSTEM", "MARK-SEALED-NOTE", "MARK-SEALED-PERIOD", "MARK-TIMELINE-FALSE", "MARK-ORDINARY-NOTE"] {
            assert!(!sent.contains(mark), "{mark} reached the prompt");
        }

        let without = inputs(&db, &vault_path, &Config::default(), &seals, day("2026-09-15"), None).unwrap();
        assert!(without.iter().all(|i| !i.node_id.starts_with("Syn/")), "conversations are off by default");
    }

    /// A provider that fails the test if it is asked anything.
    struct Refusing;

    #[async_trait::async_trait]
    impl ChatProvider for Refusing {
        fn id(&self) -> SynProvider {
            SynProvider::Ollama
        }
        async fn check_status(&self) -> AppResult<ProviderStatus> {
            unreachable!()
        }
        async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
            unreachable!()
        }
        async fn chat(&self, _: ChatRequest<'_>) -> AppResult<ChatReply> {
            panic!("a model was asked")
        }
        async fn chat_streaming(&self, _: ChatRequest<'_>, _: &StreamSink<'_>) -> AppResult<ChatReply> {
            panic!("a model was asked")
        }
    }

    /// A provider that always gives the same reply.
    struct Replying(&'static str);

    #[async_trait::async_trait]
    impl ChatProvider for Replying {
        fn id(&self) -> SynProvider {
            SynProvider::Ollama
        }
        async fn check_status(&self) -> AppResult<ProviderStatus> {
            unreachable!()
        }
        async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
            unreachable!()
        }
        async fn chat(&self, _: ChatRequest<'_>) -> AppResult<ChatReply> {
            Ok(ChatReply {
                content: self.0.to_string(),
                tool_calls: Vec::new(),
                usage: Default::default(),
                duration_ms: Some(1200),
            })
        }
        async fn chat_streaming(&self, req: ChatRequest<'_>, _: &StreamSink<'_>) -> AppResult<ChatReply> {
            self.chat(req).await
        }
    }

    const REPLY: &str = r#"{"moments": [{"what": "Khám mắt cho mẹ", "when": "hôm qua", "people": [], "quote": "Hôm qua đưa mẹ đi khám mắt ở bệnh viện", "confidence": 0.9}]}"#;

    fn files_outside_timeline(vault_path: &str) -> BTreeMap<String, Vec<u8>> {
        crate::sync::utils::collect_local_files(vault_path)
            .into_iter()
            .filter(|rel| !super::super::is_timeline_path(rel))
            .map(|rel| {
                let bytes = std::fs::read(Path::new(vault_path).join(&rel)).unwrap_or_default();
                (rel, bytes)
            })
            .collect()
    }

    /// Gate: nothing is written into the vault's own notes until accepted.
    #[tokio::test]
    async fn reading_writes_nothing_outside_the_timeline_folder() {
        let (_dir, vault_path) = vault();
        let daily = format!("---\ntitle: 2024-06-02\ndate: 2024-06-02\n---\n{DAILY}\n");
        std::fs::create_dir_all(Path::new(&vault_path).join("Notes")).unwrap();
        std::fs::write(Path::new(&vault_path).join("Notes/2024-06-02.md"), &daily).unwrap();
        let before = files_outside_timeline(&vault_path);

        let read = vec![input("note", "2024-06-02", true, DAILY)];
        let report = extract_all(&Replying(REPLY), "m", 8192, &read, &People::default(), &vault_path, "dev-a").await;
        assert_eq!((report.read, report.items, report.failed.len()), (1, 1, 0), "{report:?}");

        assert_eq!(files_outside_timeline(&vault_path), before, "a note changed before anything was accepted");
        let month = std::fs::read_to_string(Path::new(&vault_path).join("Timeline/2024/2024-06.dev-a.json")).expect("the item's month");
        assert!(month.contains("Khám mắt cho mẹ"), "{month}");
    }

    /// Gate: an edited note is marked stale, and no model is asked.
    #[tokio::test]
    async fn an_edited_note_is_stale_and_no_model_is_asked() {
        let (_dir, vault_path) = vault();
        let original = input("note", "2024-06-02", true, DAILY);
        extract_all(&Replying(REPLY), "m", 8192, std::slice::from_ref(&original), &People::default(), &vault_path, "dev-a").await;

        let store = TimelineStore::open_in_memory().unwrap();
        load(store.conn(), &vault_path).unwrap();
        let unchanged = plan(store.conn(), vec![original.clone()]).unwrap();
        assert_eq!((unchanged.pending.len(), unchanged.done), (0, 1));

        let edited = input("note", "2024-06-02", true, &format!("{DAILY} Tối về ngủ sớm."));
        let after = plan(store.conn(), vec![edited.clone()]).unwrap();
        assert_eq!((after.pending.len(), after.stale.len()), (0, 1));

        // What an automatic run reads is the pending list, and nothing else.
        let report = extract_all(&Refusing, "m", 8192, &after.pending, &People::default(), &vault_path, "dev-a").await;
        assert_eq!(report.read, 0);

        let shown = proposals(store.conn(), &[edited], &HashMap::new(), &Seals::read(&DbBridge::new_in_memory_full().unwrap(), &vault_path).unwrap(), &People::default()).unwrap();
        assert_eq!(shown.len(), 1, "the reading stands");
        assert!(shown[0].stale, "and says the note has changed since");
    }

    #[tokio::test]
    async fn a_decision_takes_a_proposal_out_of_the_tray() {
        let (_dir, vault_path) = vault();
        let read = input("note", "2024-06-02", true, DAILY);
        extract_all(&Replying(REPLY), "m", 8192, std::slice::from_ref(&read), &People::default(), &vault_path, "dev-a").await;
        let store = TimelineStore::open_in_memory().unwrap();
        load(store.conn(), &vault_path).unwrap();
        let seals = Seals::read(&DbBridge::new_in_memory_full().unwrap(), &vault_path).unwrap();

        let shown = proposals(store.conn(), std::slice::from_ref(&read), &reviewed(&vault_path), &seals, &People::default()).unwrap();
        assert_eq!(shown.len(), 1);
        assert!(!shown[0].stale);

        // Not in any answer the timeline gives until it is accepted.
        let answered = store.query(when::parse("2024-06").unwrap(), day("2026-09-15")).unwrap();
        assert!(answered.is_empty(), "{answered:?}");

        decide(
            &vault_path,
            "dev-b",
            Decision { item: shown[0].id.clone(), decision: "declined".into(), node: read.node_id.clone(), at: "2026-09-15T00:00:00.000Z".into(), moment: None },
            Utc::now(),
        )
        .unwrap();
        let shown = proposals(store.conn(), std::slice::from_ref(&read), &reviewed(&vault_path), &seals, &People::default()).unwrap();
        assert!(shown.is_empty(), "declined on another device is declined");
    }

    #[test]
    fn a_month_file_that_cannot_be_read_is_never_overwritten() {
        let (_dir, vault_path) = vault();
        let path = Path::new(&vault_path).join("Timeline/2024/2024-06.dev-a.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ half a file").unwrap();

        let read = input("note", "2024-06-02", true, DAILY);
        let (items, _) = settle(&read, parse_reply(REPLY).unwrap(), &People::default(), "m");
        let now = DateTime::parse_from_rfc3339("2024-06-03T00:00:00Z").unwrap().with_timezone(&Utc);
        let run = SourceRun::new(&read, "m", &items, 0, 1, now);
        assert!(record(&vault_path, "dev-a", run, &items, now).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ half a file");

        let store = TimelineStore::open_in_memory().unwrap();
        let loaded = load(store.conn(), &vault_path).unwrap();
        assert_eq!(loaded.unreadable.len(), 1, "{loaded:?}");
    }

    #[test]
    fn an_accepted_moment_is_read_back_from_the_note_it_was_written_into() {
        let read = input("note", "2024-06-02", true, DAILY);
        let (items, _) = settle(&read, parse_reply(REPLY).unwrap(), &People::default(), "m");
        let entry = moment_entry(&items[0]);
        assert_eq!(entry["happened"], "2024-06-01");

        let properties = json!({ "date": "2024-06-02", "moments": [entry] });
        let derived = crate::timeline::derive::derive(
            &crate::timeline::derive::NodeView { id: "Notes/2024-06-02.md", node_type: "note", title: "2024-06-02", properties: &properties },
            &HashMap::new(),
        );
        let moment = derived.iter().find(|d| d.kind == "moment").expect("a moment");
        assert_eq!(moment.label.as_deref(), Some("Khám mắt cho mẹ"));
        assert_eq!(when::iso(moment.span.from), "2024-06-01");
        assert_eq!(moment.time_source, "user");
    }

    /// The place goes the whole way: out of the model, through the guards,
    /// into the note's frontmatter, and back out as a `where` link.
    #[test]
    fn a_place_the_model_read_reaches_the_timeline_as_a_place() {
        let read = input("note", "2024-06-02", true, DAILY);
        let raw = vec![RawMoment {
            what: "Khám mắt cho mẹ".into(),
            when: "hôm qua".into(),
            people: vec![],
            place: "bệnh viện".into(),
            quote: "Hôm qua đưa mẹ đi khám mắt ở bệnh viện".into(),
            confidence: Some(0.9),
        }];
        let (items, dropped) = settle(&read, raw, &People::default(), "m");
        assert_eq!(dropped.total(), 0, "{dropped:?}");
        assert_eq!(items[0].payload.place.as_deref(), Some("bệnh viện"));

        let entry = moment_entry(&items[0]);
        assert_eq!(entry["where"], "bệnh viện");

        let properties = json!({ "date": "2024-06-02", "moments": [entry] });
        let derived = crate::timeline::derive::derive(
            &crate::timeline::derive::NodeView {
                id: "Notes/2024-06-02.md",
                node_type: "note",
                title: "2024-06-02",
                properties: &properties,
            },
            &HashMap::new(),
        );
        let moment = derived.iter().find(|d| d.kind == "moment").expect("a moment");
        assert!(
            moment.links.contains(&crate::timeline::derive::Link::at("bệnh viện")),
            "{:?}",
            moment.links
        );
    }

    /// A moment kept from a conversation cannot be derived from the vault
    /// again, so the links it names are written when it is loaded. Sealing
    /// reads those links: without them, a sealed person standing second in the
    /// list is shown, because `related_id` only ever held the first.
    #[test]
    fn an_accepted_moment_names_everyone_it_named() {
        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_string_lossy().to_string();
        let read = input("note", "2024-06-02", true, DAILY);
        let (mut items, _) = settle(&read, parse_reply(REPLY).unwrap(), &People::default(), "m");
        let mut moment = items.remove(0);
        moment.payload.people = vec!["uuid-a".into(), "uuid-sealed".into(), "uuid-c".into()];

        let now = Utc::now();
        decide(
            &vault_path,
            "macbook",
            Decision {
                item: moment.id.clone(),
                decision: "accepted".into(),
                node: "Syn/chat.md".into(),
                at: crate::utils::timestamp::canonical(now),
                moment: Some(moment.clone()),
            },
            now,
        )
        .unwrap();

        let timeline = crate::timeline::TimelineStore::open_in_memory().unwrap();
        load(timeline.conn(), &vault_path).unwrap();

        let row = format!("{}#accepted", moment.id);
        let mut stmt = timeline
            .conn()
            .prepare("SELECT node_id FROM event_links WHERE event_id = ?1 AND role = 'with' ORDER BY node_id")
            .unwrap();
        let named: Vec<String> = stmt.query_map([&row], |r| r.get(0)).unwrap().flatten().collect();
        assert_eq!(
            named,
            vec!["uuid-a", "uuid-c", "uuid-sealed"],
            "everyone it named has to be a link, or the seal cannot see them"
        );
    }

    /// The live half of the eval, on hand-labelled notes. Spends real credit.
    ///
    /// ```bash
    /// cargo test --lib timeline::extract::tests::live -- --ignored --nocapture
    /// ```
    mod live {
        use super::*;

        struct Case {
            node_type: &'static str,
            recorded: &'static str,
            by_day: bool,
            person: Option<&'static str>,
            text: &'static str,
            /// `(from, to, people by title)`.
            labels: &'static [(&'static str, &'static str, &'static [&'static str])],
        }

        const PEOPLE: &[(&str, &str)] = &[
            ("Nguyễn Thu Hà", "p-ha"),
            ("Trần Minh Tuấn", "p-tuan"),
            ("Lê Hoàng Minh", "p-minh"),
            ("Phạm Ngọc Lan", "p-lan"),
            ("Mẹ", "p-me"),
        ];

        const CASES: &[Case] = &[
            Case { node_type: "note", recorded: "2024-03-10", by_day: true, person: None,
                text: "Sáng nay chạy 5km quanh hồ Tây, lần đầu tiên chạy hết mà không nghỉ. Chiều cà phê với Tuấn, nó kể sắp chuyển vào Sài Gòn làm việc.",
                labels: &[("2024-03-10", "2024-03-10", &[]), ("2024-03-10", "2024-03-10", &["Trần Minh Tuấn"])] },
            Case { node_type: "note", recorded: "2024-03-18", by_day: true, person: None,
                text: "Hôm qua đưa mẹ đi khám mắt ở bệnh viện Mắt Trung ương, bác sĩ bảo phải mổ đục thuỷ tinh thể. Tuần sau đặt lịch mổ.",
                labels: &[("2024-03-17", "2024-03-17", &["Mẹ"])] },
            Case { node_type: "note", recorded: "2024-06-02", by_day: true, person: None,
                text: "Tuần trước ký hợp đồng thuê căn hộ mới ở Cầu Giấy. Hôm nay dọn đồ sang, Minh với Lan qua phụ khiêng tủ.",
                labels: &[("2024-05-20", "2024-05-26", &[]), ("2024-06-02", "2024-06-02", &["Lê Hoàng Minh", "Phạm Ngọc Lan"])] },
            Case { node_type: "note", recorded: "2024-09-01", by_day: true, person: None,
                text: "Việc cần làm tuần này: gọi cho ngân hàng về khoản vay, viết xong báo cáo quý, mua quà sinh nhật cho Lan, đặt vé máy bay đi Đà Nẵng.",
                labels: &[] },
            Case { node_type: "note", recorded: "2024-12-28", by_day: true, person: None,
                text: "Kế hoạch tuần sau: thứ hai họp với Minh chốt thiết kế, thứ tư đưa mẹ đi tái khám, cuối tuần đi Ninh Bình với Lan.",
                labels: &[] },
            Case { node_type: "interaction", recorded: "2023-11-05", by_day: true, person: Some("p-ha"),
                text: "Gặp Hà ở quán quen. Hà kể năm 2019 từng bỏ học một kỳ để chăm bố ốm. Giờ Hà đã mở được tiệm bánh riêng.",
                labels: &[("2023-11-05", "2023-11-05", &["Nguyễn Thu Hà"])] },
            Case { node_type: "interaction", recorded: "2024-02-14", by_day: true, person: Some("p-tuan"),
                text: "Ăn tối với Tuấn nhân dịp nó được thăng chức trưởng phòng. Tuấn trả tiền, hẹn lần sau tới lượt mình.",
                labels: &[("2024-02-14", "2024-02-14", &["Trần Minh Tuấn"])] },
            Case { node_type: "interaction", recorded: "2024-08-20", by_day: true, person: Some("p-lan"),
                text: "Gọi video với Lan gần một tiếng. Lan mới sinh con gái hôm 15/8, hai mẹ con khoẻ.",
                labels: &[("2024-08-20", "2024-08-20", &["Phạm Ngọc Lan"])] },
            Case { node_type: "person", recorded: "2024-01-01", by_day: false, person: None,
                text: "Bạn cùng phòng thời đại học. Quen nhau năm 2009 khi cùng vào ký túc xá Bách Khoa. Năm 2015 làm phù rể trong đám cưới của Minh.",
                labels: &[("2009-01-01", "2009-12-31", &["Lê Hoàng Minh"]), ("2015-01-01", "2015-12-31", &["Lê Hoàng Minh"])] },
            Case { node_type: "person", recorded: "2024-01-01", by_day: false, person: None,
                text: "Mẹ sinh năm 1962 ở Nam Định. Tết năm 2020 cả nhà về quê ngoại ăn Tết cùng mẹ, lần cuối trước dịch.",
                labels: &[("2020-01-01", "2020-12-31", &["Mẹ"])] },
            Case { node_type: "quickcap", recorded: "2024-10-05", by_day: true, person: None,
                text: "Vừa nhận giấy báo trúng tuyển thạc sĩ Đại học Quốc gia Hà Nội, không tin nổi!!!",
                labels: &[("2024-10-05", "2024-10-05", &[])] },
            Case { node_type: "quickcap", recorded: "2024-10-07", by_day: true, person: None,
                text: "Mẹo: dùng git worktree để làm song song nhiều nhánh mà không phải stash thay đổi.",
                labels: &[] },
        ];

        #[tokio::test]
        #[ignore = "spends real API credit and needs a network; run by hand"]
        async fn precision_on_hand_labelled_notes() {
            let vault_path = std::env::var("SYN_EVAL_VAULT")
                .unwrap_or_else(|_| format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default()));
            let settings = crate::syn::settings::load_settings(&vault_path).expect("the real Syn settings");
            let model = settings.default_model.clone().expect("a default model must be configured");
            let provider = crate::syn::provider::for_settings(
                &settings,
                crate::secrets::SecretManager::get_syn_api_key(None, settings.provider.key_slot()),
            );
            let pairs: Vec<(String, String)> = PEOPLE.iter().map(|(t, i)| (t.to_string(), i.to_string())).collect();
            let people = People::from_pairs(&pairs);

            let (mut extracted, mut correct, mut labels, mut found) = (0, 0, 0, 0);
            let (mut empty_notes, mut empty_with_items) = (0, 0);
            let (mut names_given, mut names_joined) = (0, 0);
            eprintln!("\n═══ extraction on hand-labelled notes ═══  model {model}\n");

            for (n, case) in CASES.iter().enumerate() {
                let mut read = make_input(
                    format!("Eval/{n}.md"), case.node_type.into(), format!("case {n}"),
                    case.text, day(case.recorded), case.by_day, case.person.map(String::from),
                )
                .expect("long enough");
                read.title = format!("case {n}");
                let raw = match provider
                    .chat(ChatRequest { model: &model, messages: &[ChatMessage::new("user", prompt(&read))], temperature: Some(0.0), num_ctx: settings.num_ctx, tools: None })
                    .await
                    .map(|reply| parse_reply(&reply.content))
                {
                    Ok(Some(raw)) => raw,
                    other => {
                        eprintln!("── case {n}: no usable reply: {other:?}");
                        continue;
                    }
                };
                for m in &raw {
                    names_given += m.people.len();
                    names_joined += m.people.iter().filter(|name| people.find(name).is_some()).count();
                }
                let (items, dropped) = settle(&read, raw, &people, &model);

                let group = |titles: &[&str]| -> Vec<String> {
                    let mut group: Vec<String> = titles.iter().map(|t| t.to_lowercase()).collect();
                    group.sort();
                    group
                };
                let people_of = |item: &Extracted| {
                    let mut group: Vec<String> = item.payload.people.iter()
                        .map(|id| people.title(id).unwrap_or(id).to_lowercase())
                        .chain(item.payload.names.iter().map(|name| name.to_lowercase()))
                        .collect();
                    group.sort();
                    group
                };
                let fits = |item: &Extracted, (from, to, who): &(&str, &str, &[&str])| {
                    *from <= item.happened_from.as_str() && item.happened_to.as_str() <= *to && people_of(item) == group(who)
                };

                extracted += items.len();
                let right = items.iter().filter(|item| case.labels.iter().any(|label| fits(item, label))).count();
                correct += right;
                labels += case.labels.len();
                found += case.labels.iter().filter(|label| items.iter().any(|item| fits(item, label))).count();
                if case.labels.is_empty() {
                    empty_notes += 1;
                    if !items.is_empty() {
                        empty_with_items += 1;
                    }
                }

                eprintln!("── case {n} ({}): {right}/{} right, {} of {} labels found, dropped {dropped:?}",
                    case.node_type, items.len(),
                    case.labels.iter().filter(|label| items.iter().any(|item| fits(item, label))).count(),
                    case.labels.len());
                for item in &items {
                    eprintln!("   {} → {} · {} · {:?} {:?}", item.happened_from, item.happened_to, item.payload.title, people_of(item), item.payload.quote);
                }
            }

            let ratio = |a: usize, b: usize| if b == 0 { 0.0 } else { a as f64 / b as f64 };
            eprintln!(
                "\nprecision {correct}/{extracted} = {:.2}   recall {found}/{labels} = {:.2}   \
                 empty notes with items {empty_with_items}/{empty_notes}   names joined {names_joined}/{names_given}\n",
                ratio(correct, extracted), ratio(found, labels)
            );
        }
    }
}
