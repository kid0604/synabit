//! The moment reader, third version: blocks, put on the day they tell about,
//! read a day at a time.
//!
//! The design is `docs/timeline-extract-v3-2026-09-22.md`. In short:
//!
//! 1. Every note the reader may read is split into **blocks** (`blocks.rs`).
//! 2. Each block gets a **frame day** — the day "today" means in it: the day of
//!    a daily note, the day of a capture or an event, or else the day its
//!    words first appeared in the note's history (§4.2).
//! 3. Blocks not read before go into the **bag** of their frame day, beside
//!    what else is known about that day — the calendar, tasks finished, the
//!    moments already kept — and the people likely to be named (§4.3, §5).
//! 4. A bag is one model call, under one fixed set of instructions and one
//!    JSON schema (§5.5, §6). The reply is checked (§7) and becomes proposals.
//!
//! What a reading read is recorded by block, so an edit elsewhere in a note
//! does not send the whole note back to the model, and a note moved to
//! another folder is not read again.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Local, NaiveDate, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::blocks::{self, Block, History};
use super::extract::{
    self, fingerprint, Amount, Config, Dropped, Evidence, Extracted, Extractor, Payload, SourceRun,
};
use super::when::{self, Precision, Span};
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::syn::provider::{ChatMessage, ChatProvider, ChatRequest};

/// What one bag may hand the model to read, in characters. Past this a day is
/// read in more than one call, split by note.
const MOST_READ: usize = 6_000;

/// One block, at most, as the model is given it. A longer one is cut: a
/// moment is told in a sentence, not in the fortieth paragraph of one block.
const MOST_BLOCK: usize = 2_000;

/// Context around each block read: this many blocks before it and after it.
const AROUND_BEFORE: usize = 2;
const AROUND_AFTER: usize = 1;
/// And at most this much context altogether.
const MOST_CONTEXT: usize = 3_000;

/// People named in a bag, at most. Family is always among them.
const MOST_PEOPLE: usize = 40;

/// The kinds a moment can be, until the vault says otherwise (§6).
///
/// A starting point, not a law: `Config::categories` is what a reading is
/// actually held to, and the person edits it.
pub const DEFAULT_CATEGORIES: &[&str] = &[
    "meal", "spending", "meeting", "work", "health", "trip", "family", "feeling", "thought", "milestone", "other",
];

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("timeline reader: {e}"))
}

// ─── Who is who ──────────────────────────────────────────────────

/// Words a relationship is written with that make someone family.
const FAMILY: &[&str] = &[
    "bố", "ba", "cha", "mẹ", "má", "vợ", "chồng", "con", "anh trai", "chị gái", "em trai", "em gái", "anh", "chị",
    "em", "ông", "bà", "cậu", "mợ", "dì", "chú", "thím", "bác", "cô", "cháu", "em họ", "anh họ", "chị họ", "mẹ vợ",
    "bố vợ", "mẹ chồng", "bố chồng", "father", "mother", "wife", "husband", "son", "daughter", "brother", "sister",
    "family",
];

/// Ways of addressing someone that come before their name.
const HONORIFICS: &[&str] = &["anh ", "chị ", "em ", "bạn ", "cô ", "chú ", "bác ", "ông ", "bà ", "thầy ", "sếp "];

/// One person in the vault, as the model is told about them.
#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    /// What a moment names them by: their `node_id`, or their path.
    pub id: String,
    pub name: String,
    /// Nickname, display name, and anything under `aliases`.
    pub aliases: Vec<String>,
    /// `relationship_type` as written: "Đồng Nghiệp", "Mẹ", "Con".
    pub relation: Option<String>,
    pub family: bool,
}

impl Person {
    fn terms(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.name.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

/// Everybody, and who is writing.
#[derive(Debug, Clone, Default)]
pub struct Directory {
    pub people: Vec<Person>,
    /// The person whose vault it is: the "tôi / mình" of every note.
    pub writer: Option<Person>,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(one)) => vec![one.trim().to_string()],
        Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).map(|s| s.trim().to_string()).collect(),
        _ => Vec::new(),
    }
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect()
}

fn without_honorific(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    HONORIFICS
        .iter()
        .find_map(|h| lower.strip_prefix(h).map(|rest| rest.trim().to_string()))
        .unwrap_or(lower)
}

impl Directory {
    pub fn read(db: &DbBridge) -> AppResult<Directory> {
        let mut stmt = db
            .conn()
            .prepare(
                "SELECT COALESCE(NULLIF(stable_id, ''), id), title, properties FROM nodes
                 WHERE node_type = 'person' ORDER BY id",
            )
            .map_err(sql)?;
        let rows: Vec<(String, String, String)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get::<_, Option<String>>(2)?.unwrap_or_default()))
            })
            .map_err(sql)?
            .flatten()
            .collect();
        let mut directory = Directory::default();
        for (id, title, properties) in rows {
            let properties: Value = serde_json::from_str(&properties).unwrap_or(Value::Null);
            let mut aliases: Vec<String> = ["nickname", "display_name", "custom_display", "aliases"]
                .iter()
                .flat_map(|key| strings(properties.get(*key)))
                .filter(|alias| alias.to_lowercase() != title.trim().to_lowercase())
                .collect();
            aliases.dedup();
            let relation = strings(properties.get("relationship_type")).join(", ");
            let relation = (!relation.is_empty()).then_some(relation);
            let family = relation.as_deref().is_some_and(|r| {
                r.to_lowercase().split(',').any(|part| FAMILY.contains(&part.trim()))
            });
            let person = Person { id, name: title.trim().to_string(), aliases, relation, family };
            if properties.get("is_owner").and_then(Value::as_bool) == Some(true) {
                directory.writer = Some(person);
            } else if !person.name.is_empty() {
                directory.people.push(person);
            }
        }
        // The name people use is the last word of a Vietnamese name. Offered
        // as an alias only when it is nobody else's too.
        let mut given: HashMap<String, usize> = HashMap::new();
        for person in &directory.people {
            if let Some(last) = person.name.split_whitespace().last().filter(|_| person.name.contains(' ')) {
                *given.entry(last.to_lowercase()).or_insert(0) += 1;
            }
        }
        for person in &mut directory.people {
            if let Some(last) = person.name.split_whitespace().last().filter(|_| person.name.contains(' ')) {
                if given.get(&last.to_lowercase()) == Some(&1) && !person.aliases.iter().any(|a| a.eq_ignore_ascii_case(last)) {
                    person.aliases.push(last.to_string());
                }
            }
        }
        Ok(directory)
    }

    /// Who a bag's text seems to name, and the family, who are named by what
    /// they are ("mẹ", "con") as often as by name.
    pub fn in_text(&self, text: &str) -> Vec<Person> {
        let lower = text.to_lowercase();
        let mut out: Vec<Person> = self
            .people
            .iter()
            .filter(|person| person.family || person.terms().any(|term| names_in(&lower, &term.to_lowercase())))
            .cloned()
            .collect();
        out.sort_by_key(|person| !person.family);
        out.truncate(MOST_PEOPLE);
        out
    }

    /// The person a name the model wrote belongs to, when exactly one does.
    pub fn find(&self, name: &str) -> Option<&Person> {
        let wanted = without_honorific(name);
        if wanted.is_empty() {
            return None;
        }
        let matching: Vec<&Person> = self
            .people
            .iter()
            .filter(|person| person.terms().any(|term| without_honorific(term) == wanted))
            .collect();
        match matching.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    pub fn title(&self, id: &str) -> Option<&str> {
        self.people.iter().find(|p| p.id == id).map(|p| p.name.as_str())
    }

    fn is_writer(&self, id_or_name: &str) -> bool {
        let lower = id_or_name.trim().to_lowercase();
        matches!(lower.as_str(), "tôi" | "mình" | "tao" | "tớ" | "i" | "me" | "the writer" | "writer")
            || self.writer.as_ref().is_some_and(|w| w.id == id_or_name || w.terms().any(|t| t.to_lowercase() == lower))
    }
}

/// Whether `term` appears in `text` as whole words.
fn names_in(text: &str, term: &str) -> bool {
    if term.chars().count() < 2 {
        return false;
    }
    let mut from = 0;
    while let Some(at) = text[from..].find(term) {
        let start = from + at;
        let end = start + term.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if before.is_none_or(|c| !c.is_alphanumeric()) && after.is_none_or(|c| !c.is_alphanumeric()) {
            return true;
        }
        from = start + term.chars().next().map_or(1, char::len_utf8);
    }
    false
}

// ─── What can be read ────────────────────────────────────────────

/// One note, split and dated.
#[derive(Debug, Clone)]
pub struct Source {
    pub node: String,
    pub node_type: String,
    pub title: String,
    /// Each block with its frame day, and whether that day is only a guess.
    pub blocks: Vec<(Block, NaiveDate, bool)>,
    /// Every block the note has had, for telling an edit from a new block.
    pub history: History,
    /// The whole body's fingerprint, as the second reader recorded it.
    pub fingerprint: String,
    /// When its history says each block first appeared, in unix ms.
    pub first_seen: HashMap<String, i64>,
}

/// How near the front of a day's reading a kind of node belongs.
fn told_by(node_type: &str) -> u8 {
    match node_type {
        "event" => 1,
        "task" => 2,
        _ => 0,
    }
}

/// A calendar entry's name with the time it was at, which is the part of it
/// that a note about the same hour has to be matched against.
fn title_with_time(node: &NodeRow) -> String {
    let at = (node.node_type == "event")
        .then(|| node.properties.get("start_at").and_then(Value::as_str))
        .flatten()
        .and_then(|start| DateTime::parse_from_rfc3339(start).ok())
        .map(|at| at.with_timezone(&Local).format("%H:%M").to_string())
        .filter(|time| time != "00:00");
    match at {
        Some(time) => format!("{} ({time})", node.title),
        None => node.title.to_string(),
    }
}

/// A node's row, as the reader needs it.
pub struct NodeRow<'a> {
    pub id: &'a str,
    pub node_type: &'a str,
    pub title: &'a str,
    pub content: &'a str,
    pub properties: &'a Value,
    pub created_at: &'a str,
}

fn day_of_millis(ms: i64) -> Option<NaiveDate> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|at| at.with_timezone(&Local).date_naive())
}

/// Split one node and give each block its frame day (§4.2).
pub fn source(node: &NodeRow, history: History, today: NaiveDate) -> Option<Source> {
    let (recorded, by_day) = extract::recorded(node.node_type, node.properties, node.created_at)?;
    // A daily note, a capture, an event, a task, a meeting written down:
    // everything in it is about its one day, however late it was typed.
    let one_day = by_day || matches!(node.node_type, "event" | "interaction" | "task");
    let mut out = Vec::new();
    // A task or a calendar entry says what it is in its name, and usually has
    // no body at all — so the name is the first thing read (§14, chốt
    // 2026-09-23: task và lịch hẹn được đọc như mọi node khác).
    let named = matches!(node.node_type, "task" | "event")
        .then(|| blocks::of_title(&title_with_time(node)))
        .flatten();
    for block in named.into_iter().chain(blocks::split(node.content)) {
        let (day, estimated) = if one_day {
            (recorded, false)
        } else {
            match history.first.get(&block.hash) {
                // There when the history began: it may be far older, and the
                // day the file was made is the better guess.
                Some(_) if history.from_the_start(&block.hash) => (recorded.min(today), true),
                Some(ms) => (day_of_millis(*ms).unwrap_or(recorded), false),
                None => (recorded, true),
            }
        };
        out.push((block, day.min(today), estimated));
    }
    Some(Source {
        node: node.id.to_string(),
        node_type: node.node_type.to_string(),
        title: node.title.to_string(),
        blocks: out,
        fingerprint: fingerprint(node.content.trim()),
        first_seen: history.first.clone(),
        history,
    })
}

/// Everything the configuration lets the reader read.
///
/// `history_of` hands back a note's Loro history, or an empty one where there
/// is none: a note never saved by this app has no history, and its blocks all
/// take the day it was made.
pub fn sources(
    db: &DbBridge,
    vault_path: &str,
    config: &Config,
    today: NaiveDate,
    settled_before: Option<DateTime<Utc>>,
    history_of: &dyn Fn(&str) -> History,
) -> AppResult<Vec<Source>> {
    let mut stmt = db
        .conn()
        .prepare("SELECT id, node_type, title, content, properties, created_at, updated_at FROM nodes ORDER BY id")
        .map_err(sql)?;
    let rows: Vec<(String, String, String, String, String, String, String)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                r.get::<_, Option<String>>(6)?.unwrap_or_default(),
            ))
        })
        .map_err(sql)?
        .flatten()
        .collect();

    let settled = |updated_at: &str| match settled_before {
        None => true,
        Some(before) => DateTime::parse_from_rfc3339(updated_at).map(|at| at.with_timezone(&Utc) <= before).unwrap_or(true),
    };

    let mut out = Vec::new();
    for (id, node_type, title, content, properties, created_at, updated_at) in rows {
        if super::is_timeline_path(&id) {
            continue;
        }
        let properties: Value = serde_json::from_str(&properties).unwrap_or(Value::Null);
        if !extract::wanted(&node_type, &properties, &id, config) || !settled(&updated_at) {
            continue;
        }
        let row = NodeRow { id: &id, node_type: &node_type, title: &title, content: &content, properties: &properties, created_at: &created_at };
        if let Some(mut read) = source(&row, history_of(&id), today) {
            read.blocks.retain(|(_, day, _)| *day <= today);
            if !read.blocks.is_empty() {
                out.push(read);
            }
        }
    }
    // What somebody wrote comes before what the app kept for them: the day's
    // notes read as a day, and the tasks and calendar entries under them are
    // the bookkeeping of it.
    out.sort_by_key(|source| (told_by(&source.node_type), source.node.clone()));
    if config.conversations {
        for (node, title, day, said) in extract::conversation_days(vault_path, today, settled_before) {
            let blocks: Vec<(Block, NaiveDate, bool)> =
                said.iter().flat_map(|message| blocks::split(message)).map(|block| (block, day, false)).collect();
            if !blocks.is_empty() {
                out.push(Source {
                    fingerprint: fingerprint(&said.join("\n\n")),
                    node,
                    node_type: "syn_conversation".into(),
                    title,
                    blocks,
                    history: History::default(),
                    first_seen: HashMap::new(),
                });
            }
        }
    }
    Ok(out)
}

// ─── What has been read ──────────────────────────────────────────

/// What earlier readings covered.
#[derive(Debug, Default, Clone)]
pub struct Read {
    /// Blocks this reader read, anywhere: a note moved is not a note unread.
    pub blocks: HashSet<String>,
    /// For the second reader, which read whole notes: the fingerprints it read,
    pub whole: HashSet<String>,
    /// and when it last read each note, in unix ms.
    pub notes: HashMap<String, i64>,
    /// Kept moments already asked about, as `moment:<path>\0<state>` (§15).
    pub changes: HashSet<String>,
}

impl Read {
    pub fn so_far(conn: &Connection) -> AppResult<Read> {
        extract::ensure_schema(conn)?;
        let mut read = Read::default();
        let mut stmt = conn.prepare("SELECT node_id, hash, version, at, blocks FROM extract_runs").map_err(sql)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, u32>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?))
            })
            .map_err(sql)?;
        for (node, hash, version, at, blocks) in rows.flatten() {
            if node.starts_with("moment:") {
                read.changes.insert(format!("{node}\0{hash}"));
                continue;
            }
            if version >= extract::EXTRACTOR_VERSION {
                let blocks: Vec<String> = serde_json::from_str(blocks.as_deref().unwrap_or("[]")).unwrap_or_default();
                read.blocks.extend(blocks);
            } else {
                read.whole.insert(hash);
                let at = DateTime::parse_from_rfc3339(&at).map(|at| at.timestamp_millis()).unwrap_or(0);
                let known = read.notes.entry(node).or_insert(at);
                *known = (*known).max(at);
            }
        }
        Ok(read)
    }
}

/// Where a block stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Read by this reader, or an edit of a block that was.
    Read,
    /// Read only by the reader before this one.
    ReadBefore,
    New,
}

pub fn standing(source: &Source, block: &Block, read: &Read) -> Standing {
    if read.blocks.contains(&block.hash) {
        return Standing::Read;
    }
    // An edit of a block already read is that block (§4.1). Only an older
    // block of the same note counts: the same sentence in two notes is two
    // things written.
    let edited = source.history.text.iter().any(|(hash, text)| {
        hash != &block.hash && read.blocks.contains(hash) && blocks::similar(text, &block.text) >= blocks::SAME_BLOCK
    });
    if edited {
        return Standing::Read;
    }
    if read.whole.contains(&source.fingerprint) {
        return Standing::ReadBefore;
    }
    if let Some(at) = read.notes.get(&source.node) {
        // Read as a whole by the reader before; this block was there by then,
        // or nobody can tell that it was not.
        match source.first_seen.get(&block.hash) {
            Some(first) if first > at => {}
            _ => return Standing::ReadBefore,
        }
    }
    Standing::New
}

// ─── Bags ────────────────────────────────────────────────────────

/// A block to read, with its code in the message.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBlock {
    pub code: String,
    pub node: String,
    pub node_title: String,
    pub block: Block,
    pub estimated: bool,
}

/// One day's reading: one model call.
#[derive(Debug, Clone, PartialEq)]
pub struct Bag {
    /// `2026-07-21`, or `2026-07-21#2` for the second call of a long day.
    pub key: String,
    pub day: NaiveDate,
    pub read: Vec<ReadBlock>,
    /// Blocks around them, only to understand them by: `(where, text)`.
    pub context: Vec<(String, String)>,
    /// Titles of moments already kept on this day.
    pub kept: Vec<String>,
    pub people: Vec<(String, Person)>,
    pub writer: Option<String>,
    /// The last few times the person corrected a reading (§8.2).
    pub corrections: Vec<Correction>,
    /// The kinds a moment can be, in this vault (`Config::categories`).
    pub categories: Vec<String>,
    /// What was read, as a whole: the blocks' hashes, sorted.
    pub hash: String,
}

/// One thing the person put right after a reading, kept so the next reading
/// is told about it. On this device: it is about how they write, and it
/// changes as they do.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    /// `title`, `category`, or `person` for a name given to somebody.
    pub field: String,
    pub before: String,
    pub after: String,
}

/// How many are worth telling a reading about.
pub const CORRECTIONS: usize = 20;

pub fn remember_correction(conn: &Connection, correction: &Correction) -> AppResult<()> {
    ensure_corrections(conn)?;
    conn.execute(
        "INSERT INTO reader_corrections (at, field, before_text, after_text) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            crate::utils::timestamp::canonical(Utc::now()),
            correction.field,
            correction.before,
            correction.after
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// The latest corrections, newest last, the way an example list reads.
pub fn corrections(conn: &Connection) -> AppResult<Vec<Correction>> {
    ensure_corrections(conn)?;
    let mut stmt = conn
        .prepare("SELECT field, before_text, after_text FROM reader_corrections ORDER BY at DESC, rowid DESC LIMIT ?1")
        .map_err(sql)?;
    let mut found: Vec<Correction> = stmt
        .query_map([CORRECTIONS as i64], |r| Ok(Correction { field: r.get(0)?, before: r.get(1)?, after: r.get(2)? }))
        .map_err(sql)?
        .flatten()
        .collect();
    found.reverse();
    Ok(found)
}

fn ensure_corrections(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS reader_corrections (
            at          TEXT NOT NULL,
            field       TEXT NOT NULL,
            before_text TEXT NOT NULL,
            after_text  TEXT NOT NULL
        );",
    )
    .map_err(sql)
}

impl Bag {
    /// Characters handed to the model to read.
    pub fn chars(&self) -> usize {
        self.read.iter().map(|b| b.block.text.chars().count()).sum()
    }

    pub fn block_hashes(&self) -> Vec<String> {
        self.read.iter().map(|b| b.block.hash.clone()).collect()
    }

    fn person(&self, code: &str) -> Option<&Person> {
        self.people.iter().find(|(c, _)| c == code).map(|(_, p)| p)
    }
}

/// What else is known about each day, for the bags.
#[derive(Debug, Clone, Default)]
pub struct Day {
    pub kept: Vec<String>,
}

/// The moments already kept, by day: what a reading must not propose again.
///
/// The calendar and the day's finished tasks used to be listed here too. They
/// are read as blocks of their own now (§14), so a reading sees them as it
/// sees everything else, and can quote them.
pub fn days(db: &DbBridge) -> AppResult<HashMap<NaiveDate, Day>> {
    let mut stmt = db
        .conn()
        .prepare("SELECT id, node_type, title, properties FROM nodes WHERE node_type = 'moment'")
        .map_err(sql)?;
    let rows: Vec<(String, String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default(), r.get::<_, Option<String>>(3)?.unwrap_or_default())))
        .map_err(sql)?
        .flatten()
        .collect();
    let mut out: HashMap<NaiveDate, Day> = HashMap::new();
    for (id, node_type, title, properties) in rows {
        let properties: Value = serde_json::from_str(&properties).unwrap_or(Value::Null);
        let field = |key: &str| properties.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
        let _ = (id, node_type);
        if let Some(span) = field("happened").and_then(when::parse) {
            out.entry(span.from).or_default().kept.push(title);
        }
    }
    Ok(out)
}

/// What is left to read, counted without reading anything.
///
/// # Why this is not `plan`
///
/// A plan says which **blocks** of which notes go in which model call. Working
/// one out reads every note the reader may look at, splits it, and replays its
/// whole edit history — measured at 750 ms on a vault of 1 016 nodes and
/// around fifteen seconds on one of 13 000. A screen that only wants to say
/// *"three days left, about four minutes"* cannot cost that, and at ten
/// thousand notes it cannot cost it at all.
///
/// So this counts **days**, from two cheap queries: which days the vault has
/// writing on, and which days a reading has already covered
/// (`extract_runs.node_id` is `day:2026-07-21`).
///
/// # What it gives up
///
/// A day is unread or it is not; a day the reader half-read is neither, and
/// this calls it read, because a reading covered it. And a note is counted on
/// the day it is *about*, where the plan would date each block from the note's
/// history and can scatter one note across several days. Both make this a
/// count of what is waiting, not a promise of what a reading will do. Press
/// "Read" and the plan is worked out in full; that is the moment worth
/// waiting for.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Left {
    /// Days with writing on them and no reading at this version.
    pub days: usize,
    /// How much writing is on those days, for the estimate.
    pub chars: usize,
    /// Days covered only by an older reader.
    pub old_version: usize,
    /// Blocks this reader has read.
    pub done: usize,
}

/// The day a reading covered, out of `day:2026-07-21` or `day:2026-07-21#2`.
fn day_of_key(node_id: &str) -> Option<NaiveDate> {
    let rest = node_id.strip_prefix("day:")?;
    let date = rest.split('#').next()?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

pub fn left_to_read(db: &DbBridge, conn: &Connection, config: &Config, today: NaiveDate) -> AppResult<Left> {
    // `length(content)` rather than the content: how much a note holds is all
    // this needs, and SQLite answers that without handing over the words.
    let mut stmt = db
        .conn()
        .prepare("SELECT id, node_type, properties, created_at, length(content) FROM nodes")
        .map_err(sql)?;
    let rows: Vec<(String, String, String, String, i64)> = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<i64>>(4)?.unwrap_or_default(),
            ))
        })
        .map_err(sql)?
        .flatten()
        .collect();

    let mut written: HashMap<NaiveDate, usize> = HashMap::new();
    for (id, node_type, properties, created_at, chars) in rows {
        if super::is_timeline_path(&id) {
            continue;
        }
        let properties: Value = serde_json::from_str(&properties).unwrap_or(Value::Null);
        if !extract::wanted(&node_type, &properties, &id, config) {
            continue;
        }
        let Some((recorded, _)) = extract::recorded(&node_type, &properties, &created_at) else {
            continue;
        };
        if recorded > today {
            continue;
        }
        *written.entry(recorded).or_default() += chars.max(0) as usize;
    }

    extract::ensure_schema(conn)?;
    let mut stmt = conn
        .prepare("SELECT node_id, version FROM extract_runs WHERE node_id LIKE 'day:%'")
        .map_err(sql)?;
    let runs: Vec<(String, u32)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))
        .map_err(sql)?
        .flatten()
        .collect();
    let mut read: HashSet<NaiveDate> = HashSet::new();
    let mut older: HashSet<NaiveDate> = HashSet::new();
    for (node_id, version) in runs {
        let Some(day) = day_of_key(&node_id) else { continue };
        if version >= extract::EXTRACTOR_VERSION {
            read.insert(day);
        } else {
            older.insert(day);
        }
    }

    let left: Vec<(&NaiveDate, &usize)> = written.iter().filter(|(day, _)| !read.contains(day)).collect();
    Ok(Left {
        days: left.len(),
        chars: left.iter().map(|(_, chars)| **chars).sum(),
        old_version: older.iter().filter(|day| !read.contains(day) && written.contains_key(day)).count(),
        done: Read::so_far(conn)?.blocks.len(),
    })
}

/// What to read: new blocks by day, and blocks only the older reader read.
#[derive(Debug, Default, Clone)]
pub struct Plan {
    pub pending: Vec<Bag>,
    pub old_version: Vec<Bag>,
    /// Kept moments whose source has changed under them (§15).
    pub changes: Vec<Change>,
    /// Blocks already read by this reader.
    pub done: usize,
}

pub fn plan(sources: &[Source], read: &Read, days: &HashMap<NaiveDate, Day>, directory: &Directory) -> Plan {
    let mut new: BTreeMap<NaiveDate, Vec<(usize, usize)>> = BTreeMap::new();
    let mut before: BTreeMap<NaiveDate, Vec<(usize, usize)>> = BTreeMap::new();
    let mut plan = Plan::default();
    for (s, source) in sources.iter().enumerate() {
        for (b, (block, day, _)) in source.blocks.iter().enumerate() {
            match standing(source, block, read) {
                Standing::Read => plan.done += 1,
                Standing::ReadBefore => before.entry(*day).or_default().push((s, b)),
                Standing::New => new.entry(*day).or_default().push((s, b)),
            }
        }
    }
    // Newest first: what was written this week matters more than a backlog.
    for (day, picked) in new.into_iter().rev() {
        plan.pending.extend(bags_of(day, &picked, sources, days, directory));
    }
    for (day, picked) in before.into_iter().rev() {
        plan.old_version.extend(bags_of(day, &picked, sources, days, directory));
    }
    plan
}

/// Everything a plan needs, from the vault's cache and the timeline's store.
#[allow(clippy::too_many_arguments)]
pub fn plan_in(
    db: &DbBridge,
    conn: &Connection,
    vault_path: &str,
    config: &Config,
    today: NaiveDate,
    settled_before: Option<DateTime<Utc>>,
    history_of: &dyn Fn(&str) -> History,
) -> AppResult<(Plan, Directory)> {
    let directory = Directory::read(db)?;
    let days = days(db)?;
    let sources = sources(db, vault_path, config, today, settled_before, history_of)?;
    let read = Read::so_far(conn)?;
    let mut plan = plan(&sources, &read, &days, &directory);
    plan.changes = changes(&sources, &super::moments::kept(db)?, &read, &directory);
    // What the person put right before, so a reading is told how they write,
    // and the kinds this vault keeps moments in.
    let learned = corrections(conn)?;
    let kinds = config.categories();
    for bag in plan.pending.iter_mut().chain(&mut plan.old_version) {
        bag.corrections.clone_from(&learned);
        bag.categories.clone_from(&kinds);
    }
    for change in &mut plan.changes {
        change.categories.clone_from(&kinds);
    }
    Ok((plan, directory))
}

/// One day's blocks as bags, split when there is too much to read at once.
fn bags_of(
    day: NaiveDate,
    picked: &[(usize, usize)],
    sources: &[Source],
    days: &HashMap<NaiveDate, Day>,
    directory: &Directory,
) -> Vec<Bag> {
    let mut groups: Vec<Vec<(usize, usize)>> = vec![Vec::new()];
    let mut size = 0;
    for &(s, b) in picked {
        let chars = sources[s].blocks[b].0.text.chars().count().min(MOST_BLOCK);
        // A new call once this one is full — inside a note too, when one note
        // is longer than a call. Picked in note order, so a note is read in
        // as few calls as it fits in.
        if size + chars > MOST_READ && size > 0 {
            groups.push(Vec::new());
            size = 0;
        }
        groups.last_mut().expect("a group").push((s, b));
        size += chars;
    }
    let count = groups.len();
    groups
        .into_iter()
        .enumerate()
        .map(|(n, group)| {
            let key = if count == 1 { when::iso(day) } else { format!("{}#{}", when::iso(day), n + 1) };
            bag(key, day, &group, sources, days.get(&day), directory)
        })
        .collect()
}

fn bag(key: String, day: NaiveDate, group: &[(usize, usize)], sources: &[Source], extras: Option<&Day>, directory: &Directory) -> Bag {
    let label = |source: &Source, block: &Block| {
        let what = match source.node_type.as_str() {
            "task" => "task, finished",
            "event" => "calendar",
            "quickcap" => "quick note",
            "interaction" => "time spent with somebody",
            "person" => "note about a person",
            "syn_conversation" => "written to the assistant",
            _ => "",
        };
        let name = match &block.heading {
            Some(h) => format!("{} › {h}", source.title),
            None => source.title.clone(),
        };
        if what.is_empty() { name } else { format!("{name} · {what}") }
    };
    let read: Vec<ReadBlock> = group
        .iter()
        .enumerate()
        .map(|(n, &(s, b))| {
            let (block, _, estimated) = &sources[s].blocks[b];
            let mut block = block.clone();
            if block.text.chars().count() > MOST_BLOCK {
                block.text = block.text.chars().take(MOST_BLOCK).collect();
            }
            ReadBlock {
                code: format!("b{}", n + 1),
                node: sources[s].node.clone(),
                node_title: label(&sources[s], &block),
                block,
                estimated: *estimated,
            }
        })
        .collect();

    // The blocks around each one read, to understand it by: which meeting
    // "that meeting" was.
    let reading: HashSet<(usize, usize)> = group.iter().copied().collect();
    let mut context = Vec::new();
    let mut context_size = 0;
    let mut seen = HashSet::new();
    for &(s, b) in group {
        let from = b.saturating_sub(AROUND_BEFORE);
        let to = (b + AROUND_AFTER).min(sources[s].blocks.len().saturating_sub(1));
        for around in from..=to {
            if reading.contains(&(s, around)) || !seen.insert((s, around)) {
                continue;
            }
            let block = &sources[s].blocks[around].0;
            let text: String = block.text.chars().take(400).collect();
            if context_size + text.len() > MOST_CONTEXT {
                break;
            }
            context_size += text.len();
            context.push((label(&sources[s], block), text));
        }
    }

    let mut all_text: String = read.iter().map(|b| b.block.text.as_str()).collect::<Vec<_>>().join("\n");
    all_text.push('\n');
    all_text.push_str(&context.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join("\n"));
    let people: Vec<(String, Person)> =
        directory.in_text(&all_text).into_iter().enumerate().map(|(n, p)| (format!("p{}", n + 1), p)).collect();

    let extras = extras.cloned().unwrap_or_default();
    let mut hashes: Vec<String> = read.iter().map(|b| b.block.hash.clone()).collect();
    hashes.sort();
    Bag {
        hash: fingerprint(&format!("{key}\0{}", hashes.join("\0"))),
        key,
        day,
        read,
        context,
        kept: extras.kept,
        people,
        writer: directory.writer.as_ref().map(|w| w.name.clone()),
        corrections: Vec::new(),
        categories: DEFAULT_CATEGORIES.iter().map(|kind| kind.to_string()).collect(),
    }
}

/// A bag of one line, for the compose box: the same reading as a day's, of
/// one sentence somebody typed (§9).
pub fn bag_of_line(line: &str, day: NaiveDate, directory: &Directory) -> Option<Bag> {
    let text = line.trim();
    if text.is_empty() {
        return None;
    }
    let block = Block { hash: blocks::key(text), raw: text.to_string(), text: text.to_string(), start: 0, heading: None, from_title: false };
    let source = Source {
        node: "compose".into(),
        node_type: "quickcap".into(),
        title: "compose".into(),
        blocks: vec![(block, day, false)],
        history: History::default(),
        fingerprint: fingerprint(text),
        first_seen: HashMap::new(),
    };
    Some(bag(when::iso(day), day, &[(0, 0)], std::slice::from_ref(&source), None, directory))
}

// ─── When the source changes under a kept moment (§15) ──────────

/// What became of the words a kept moment was read from.
#[derive(Debug, Clone, PartialEq)]
pub enum Became {
    /// The block was edited; the model is asked what that does to the moment.
    Edited(ReadBlock),
    /// The words are gone, and nothing in the note is like them.
    Retracted,
    /// So is the note.
    NoteGone,
}

/// A kept moment whose source no longer says what it said.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub moment: super::moments::Kept,
    pub became: Became,
    /// What is being answered about: the new block, or what is left of the
    /// note. A reading is not offered twice for the same state.
    pub hash: String,
    pub day: NaiveDate,
    pub people: Vec<(String, Person)>,
    pub categories: Vec<String>,
}

impl Change {
    pub fn key(&self) -> String {
        format!("moment:{}", self.moment.path)
    }
}

/// How alike a changed block must still be to the words quoted, to be the
/// block they were in. Below this, nothing in the note is about it any more.
const STILL_THE_SAME_WORDS: f64 = 0.4;

/// Kept moments whose source has changed since, and what to ask about each.
///
/// Silent when the words are still there, whatever moved around them: a
/// reformatted paragraph says the same thing. One reading per state of the
/// source, so a moment whose note is edited twice is asked about twice, and
/// a moment nobody touched is never asked about at all.
pub fn changes(sources: &[Source], kept: &[super::moments::Kept], read: &Read, directory: &Directory) -> Vec<Change> {
    let by_node: HashMap<&str, &Source> = sources.iter().map(|s| (s.node.as_str(), s)).collect();
    let mut out = Vec::new();
    for moment in kept {
        let (Some(node), Some(quote)) = (moment.source_node.as_deref(), moment.quote.as_deref()) else {
            continue;
        };
        if quote.trim().is_empty() {
            continue;
        }
        let day = moment
            .fields
            .get("happened")
            .and_then(Value::as_str)
            .and_then(when::parse)
            .map(|span| span.from)
            .unwrap_or_else(|| Local::now().date_naive());

        let (became, hash) = match by_node.get(node) {
            None => (Became::NoteGone, "gone".to_string()),
            Some(source) => {
                // The block it was read from, by the block's own name. A
                // sentence that gained a clause is a different block, and
                // that is the question to ask; one that gained bold is the
                // same words, and so the same block (`blocks::key`).
                let unchanged = match moment.block.as_deref() {
                    Some(hash) => source.blocks.iter().any(|(block, ..)| block.hash == hash),
                    // Moved out of a note before blocks were recorded: the
                    // words themselves are all there is to go on.
                    None => source.blocks.iter().any(|(block, ..)| extract::find_quote(&block.raw, quote).is_some()),
                };
                if unchanged {
                    continue;
                }
                let closest = source
                    .blocks
                    .iter()
                    .map(|(block, ..)| (block, blocks::similar(&block.text, quote)))
                    .filter(|(_, alike)| *alike >= STILL_THE_SAME_WORDS)
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                match closest {
                    Some((block, _)) => {
                        let hash = block.hash.clone();
                        (
                            Became::Edited(ReadBlock {
                                code: "b1".into(),
                                node: source.node.clone(),
                                node_title: source.title.clone(),
                                block: block.clone(),
                                estimated: false,
                            }),
                            hash,
                        )
                    }
                    None => {
                        let left: Vec<&str> = source.blocks.iter().map(|(b, ..)| b.hash.as_str()).collect();
                        (Became::Retracted, fingerprint(&left.join("\0")))
                    }
                }
            }
        };
        let change = Change {
            categories: DEFAULT_CATEGORIES.iter().map(|kind| kind.to_string()).collect(),
            people: Vec::new(),
            hash,
            day,
            became,
            moment: moment.clone(),
        };
        // Asked once for each state of the source.
        if read.changes.contains(&format!("{}\0{}", change.key(), change.hash)) {
            continue;
        }
        let words = match &change.became {
            Became::Edited(block) => format!("{}\n{}", block.block.text, change.moment.title),
            _ => change.moment.title.clone(),
        };
        out.push(Change { people: directory.in_text(&words).into_iter().enumerate().map(|(n, p)| (format!("p{}", n + 1), p)).collect(), ..change });
    }
    out
}

/// What a reading is told about a kept moment whose source was edited.
pub const ABOUT_A_CHANGE: &str = r#"A person kept a moment in their timeline. It was read from a sentence in one
of their notes, and that sentence has since been edited. Say what the edit
does to the moment they kept.

Answer with one verdict:
- "unchanged": the edit does not change the moment (a typo, rewording, or
  something added that is a separate moment of its own).
- "changed": the moment is still there but some of its fields are now wrong.
  Give the whole moment as it should now read.
- "retracted": the edited sentence no longer says this happened — it says it
  did not, or says something else entirely.

Never change a field the edit does not speak to. The fields, and how to fill
them, are the same as for reading a day: title (3–8 words, the notes'
language), date (YYYY-MM-DD), date_to, time (HH:MM), people (the OTHERS, by
ref from PEOPLE or by name as written), place, about, category (meal,
spending, meeting, work, health, trip, family, feeling, thought, milestone,
other), amount, quote (the shortest span of the edited block that shows it,
copied exactly).

The person's own wording is not yours to improve: if the title still fits, keep
it as it is."#;

/// The shape a verdict must have.
pub fn change_schema(categories: &[String]) -> Value {
    let mut moment = schema(categories)["properties"]["moments"]["items"].clone();
    moment["required"] = json!(["title", "date", "quote"]);
    json!({
        "type": "object",
        "properties": {
            "verdict": { "type": "string", "enum": ["unchanged", "changed", "retracted"] },
            "moment": moment
        },
        "required": ["verdict"]
    })
}

/// The message for one changed moment.
pub fn change_message(change: &Change) -> String {
    let mut out = String::new();
    out.push_str("THE MOMENT AS IT IS KEPT:\n");
    for (key, value) in &change.moment.fields {
        if matches!(key.as_str(), "type" | "source" | "origin" | "hand" | "extract" | "node_id" | "created_at" | "updated_at") {
            continue;
        }
        out.push_str(&format!("  {key}: {value}\n"));
    }
    if !change.moment.hand.is_empty() {
        out.push_str(&format!(
            "  (the person wrote these themselves, leave them as they are: {})\n",
            change.moment.hand.join(", ")
        ));
    }
    out.push_str(&format!(
        "\nIT WAS READ FROM, in {}:\n  “{}”\n",
        change.moment.source_node.as_deref().unwrap_or("a note"),
        change.moment.quote.as_deref().unwrap_or("")
    ));

    out.push_str(&format!("\nKINDS (category): {}\n", change.categories.join(", ")));
    out.push_str("\nPEOPLE (use the ref):\n");
    for (code, person) in &change.people {
        out.push_str(&format!("  {code}  {}", person.name));
        if !person.aliases.is_empty() {
            out.push_str(&format!("   also: {}", person.aliases.join(", ")));
        }
        out.push('\n');
    }

    match &change.became {
        Became::Edited(block) => {
            out.push_str(&format!("\nTHAT PART OF THE NOTE NOW READS:\n[b1 · READ]    {}\n{}\n", block.node_title, block.block.text));
        }
        Became::Retracted => out.push_str("\nTHAT PART OF THE NOTE IS GONE.\n"),
        Became::NoteGone => out.push_str("\nTHE NOTE IS GONE.\n"),
    }
    out
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RawVerdict {
    #[serde(default)]
    verdict: String,
    #[serde(default)]
    moment: Option<RawMoment>,
}

/// Ask what an edit did to a kept moment, and turn the answer into a proposal
/// to change it — or into the card that says its words are gone.
///
/// The two verdicts nobody has to ask a model about are not asked: a note that
/// is gone, and words that nothing in the note is like any more.
pub async fn read_change(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    change: &Change,
    directory: &Directory,
    now: DateTime<Utc>,
) -> AppResult<(Vec<Extracted>, SourceRun, Dropped)> {
    let started = std::time::Instant::now();
    let (verdict, mut items, ms) = match &change.became {
        Became::Edited(block) => {
            let messages = vec![ChatMessage::new("system", ABOUT_A_CHANGE), ChatMessage::new("user", change_message(change))];
            let shape = change_schema(&change.categories);
            let request = |held| ChatRequest { model, messages: &messages, temperature: Some(0.0), num_ctx, tools: None, json_schema: held };
            let reply = match provider.chat(request(Some(&shape))).await {
                Ok(reply) => reply,
                Err(e) => {
                    log::info!("timeline reader: asked again without a schema after: {e}");
                    provider.chat(request(None)).await?
                }
            };
            let start = reply.content.find('{').unwrap_or(0);
            let end = reply.content.rfind('}').map(|at| at + 1).unwrap_or(0);
            let read: RawVerdict = serde_json::from_str(reply.content.get(start..end).unwrap_or("")).unwrap_or_default();
            let ms = reply.duration_ms.unwrap_or_else(|| started.elapsed().as_millis() as u64);
            match read.verdict.as_str() {
                "changed" => {
                    let bag = Bag {
                        key: change.moment.path.clone(),
                        day: change.day.max(Local::now().date_naive()),
                        read: vec![block.clone()],
                        context: Vec::new(),
                        kept: Vec::new(),
                        people: change.people.clone(),
                        writer: directory.writer.as_ref().map(|w| w.name.clone()),
                        corrections: Vec::new(),
                        categories: change.categories.clone(),
                        hash: change.hash.clone(),
                    };
                    let (items, _) = settle(&bag, read.moment.into_iter().collect(), directory, model);
                    ("changed", items, ms)
                }
                "retracted" => ("retracted", Vec::new(), ms),
                _ => ("unchanged", Vec::new(), ms),
            }
        }
        Became::Retracted => ("retracted", Vec::new(), 0),
        Became::NoteGone => ("gone", Vec::new(), 0),
    };

    // A verdict of its own is a card too: nothing to change, but the person
    // has to be told the moment is standing on nothing.
    if matches!(verdict, "retracted" | "gone") {
        items.push(the_card(change, verdict, model));
    }
    for item in &mut items {
        item.about_moment = Some(change.moment.path.clone());
        item.verdict = Some(verdict.to_string());
        item.recorded = when::iso(change.day);
    }
    let run = SourceRun {
        node: change.key(),
        hash: change.hash.clone(),
        version: extract::EXTRACTOR_VERSION,
        model: model.to_string(),
        at: crate::utils::timestamp::canonical(now),
        items: items.iter().map(|item| item.id.clone()).collect(),
        dropped: 0,
        chars: change.moment.quote.as_deref().map(|q| q.chars().count()).unwrap_or(0),
        ms,
        blocks: Vec::new(),
    };
    Ok((items, run, Dropped::default()))
}

/// The card for a moment whose words are gone: what it says now, unchanged,
/// so the person can keep it or let it go.
fn the_card(change: &Change, verdict: &str, model: &str) -> Extracted {
    let id = blake3::hash(format!("{}\0{}\0{verdict}", change.key(), change.hash).as_bytes()).to_hex();
    let text = |key: &str| change.moment.fields.get(key).and_then(Value::as_str).map(String::from);
    let span = change
        .moment
        .fields
        .get("happened")
        .and_then(Value::as_str)
        .and_then(when::parse)
        .unwrap_or_else(|| Span::day(change.day));
    Extracted {
        id: format!("x{}", &id[..20]),
        kind: "moment".into(),
        happened_from: when::iso(span.from),
        happened_to: when::iso(span.to),
        precision: span.precision.as_str().into(),
        recorded: when::iso(change.day),
        source: "extract".into(),
        evidence: vec![Evidence {
            node: change.moment.source_node.clone().unwrap_or_default(),
            hash: change.hash.clone(),
            span: None,
        }],
        extractor: Extractor { version: extract::EXTRACTOR_VERSION, model: model.to_string() },
        confidence: 1.0,
        payload: Payload {
            title: change.moment.title.clone(),
            people: Vec::new(),
            names: Vec::new(),
            place: text("where"),
            quote: change.moment.quote.clone().unwrap_or_default(),
            category: text("category"),
            amount: None,
            about: Vec::new(),
            time: text("time"),
            date_basis: None,
            also: Vec::new(),
        },
        superseded_by: None,
        about_moment: Some(change.moment.path.clone()),
        verdict: Some(verdict.to_string()),
    }
}

// ─── Asking ──────────────────────────────────────────────────────

/// What every reading is told, word for word as §5.5 writes it. Fixed, and
/// first, so a provider that caches a prompt's opening caches this.
pub const INSTRUCTIONS: &str = r#"You read a person's own notes for one day and list the MOMENTS in their life that day.

A moment is something that HAPPENED, that the writer took part in or was directly
affected by, that can be placed on a day (or a span of days), and that they would
want to see again when looking back at that day. All four must hold.

Never a moment:
- plans, intentions, to-dos, anything scheduled after the day you are given;
- knowledge, notes on content, opinions, summaries of what was discussed;
- things that happened to someone else, told second-hand — unless they are family,
  or the writer was affected;
- text copied from elsewhere: articles, pasted messages, templates, checklists;
- anything in a block marked CONTEXT. Only extract from blocks marked READ.
- anything already listed under ALREADY KEPT.

Routine without detail is not worth keeping ("had lunch", "replied to emails",
"daily standup"). Routine WITH a detail is: who with, where, how much it cost, how it
turned out, how it felt.

One moment = one thing done, with one group of people, at one time. A meeting with
five agenda points is one moment. A meal and what it cost is one moment.

A block may be a sentence from a note, or the name of a finished task, or a
calendar entry — the label above each says which. A task ticked off and a
sentence about the same thing are ONE moment: propose it once, quote whichever
says it best, and put the other block's id in "also". A task name that is only
a piece of work with nothing else said about it ("Trả lời mail", "Cấp máy cho
L1") is not a moment at all.
A stretch of time (a trip of several days, years at a school, a job) is one moment
with "date" and "date_to", or "ongoing": true if it has not ended.

Fields:
- title: 3–8 words, in the notes' language, starting with what was done. Include
  who or where if the text says. No date, no "I". Add nothing the text does not say.
- date / date_to / precision / time / date_basis: when it HAPPENED, not when it was
  written. Dates are YYYY-MM-DD. Relative words count from the day given
  ("hôm qua" = the day before; "thứ Ba" = the latest Tuesday not after the day;
  "tuần trước" = Monday to Sunday of last week, precision "week"; "tháng 4" = the
  whole month, precision "month"). No time word = the day given, date_basis
  "the_day". A written date is "explicit"; a relative one "relative"; a guess from
  the content "inferred". A date after the day given means it is a plan: leave it
  out. time (HH:MM) only when the text gives a clock time. Never guess one.
- people: the OTHERS who took part, never the writer. Use {"ref": "p…"} from PEOPLE
  when the name, nickname or form of address ("mẹ", "con") matches exactly one
  person; otherwise {"name": "..."} as written, keeping "anh"/"chị". Groups are not
  people.
- place: only as written. Do not infer a place from the activity.
- about: named projects, organisations, topics in the text. At most three.
- category: one of the KINDS listed below, and nothing else. Unsure: "other".
- amount: only a sum written in the text: {"value": 50000, "unit": "VND"}.
  50k = 50000, 1tr2 = 1200000, 2 củ = 2000000. VND unless another currency is written.
- quote: {"source": "b…", "text": "..."}: the shortest continuous span of a READ
  block that shows it happened, copied exactly, at most one sentence.
- also: the ids (b…) of the other READ blocks that are this same moment.

Most days have few moments. Many blocks have none. {"moments": []} is a good answer.

Examples (one READ block each, the day given is 2026-07-22, a Wednesday):

[b1] Tối qua đưa Cam đi bơi.   (PEOPLE: p1 Cam, also: con)
→ {"moments": [{"title": "Đưa Cam đi bơi", "date": "2026-07-21", "precision": "day", "date_basis": "relative", "people": [{"ref": "p1"}], "category": "family", "quote": {"source": "b1", "text": "Tối qua đưa Cam đi bơi."}}]}

[b1] 21:03 — gọi cho mẹ, mẹ bảo đã đỡ đau lưng.   (PEOPLE: p1 Mẹ, family)
→ {"moments": [{"title": "Gọi điện cho mẹ", "date": "2026-07-22", "time": "21:03", "precision": "day", "date_basis": "the_day", "people": [{"ref": "p1"}], "category": "family", "quote": {"source": "b1", "text": "gọi cho mẹ, mẹ bảo đã đỡ đau lưng"}}]}

[b1] Họp UAT: - chốt ngày golive - thêm quyền admin - sửa báo cáo tuần - rà soát log - hẹn demo thứ 6
→ {"moments": [{"title": "Họp UAT", "date": "2026-07-22", "precision": "day", "date_basis": "the_day", "category": "meeting", "quote": {"source": "b1", "text": "Họp UAT"}}]}

[b1] Dạo này hay mất ngủ.
→ {"moments": []}

[b1] Tuần sau đi Đà Lạt.
→ {"moments": []}

[b1] Phan Hương Lê gọi trao đổi về hợp đồng.   (PEOPLE: p1 Phan Hương Lê)
→ {"moments": [{"title": "Trao đổi hợp đồng với Phan Hương Lê", "date": "2026-07-22", "precision": "day", "date_basis": "the_day", "people": [{"ref": "p1"}], "category": "work", "quote": {"source": "b1", "text": "Phan Hương Lê gọi trao đổi về hợp đồng."}}]}"#;

/// The message for one bag (§5.5).
pub fn message(bag: &Bag) -> String {
    let mut out = String::new();
    match &bag.writer {
        Some(name) => out.push_str(&format!("WRITER: {name} — the \"tôi / mình / tao / anh / em\" in the notes.\n")),
        None => out.push_str("WRITER: the \"tôi / mình / tao\" in the notes.\n"),
    }
    out.push_str(&format!("DAY: {}, {}.\n", when::iso(bag.day), bag.day.format("%A")));
    out.push_str(&format!("KINDS (category): {}\n", bag.categories.join(", ")));

    out.push_str("\nPEOPLE (use the ref):\n");
    if bag.people.is_empty() {
        out.push_str("  (none known)\n");
    }
    for (code, person) in &bag.people {
        let mut line = format!("  {code}  {}", person.name);
        if !person.aliases.is_empty() {
            line.push_str(&format!("   also: {}", person.aliases.join(", ")));
        }
        if let Some(relation) = &person.relation {
            line.push_str(&format!("   · {relation}"));
        }
        if person.family {
            line.push_str(" · family");
        }
        out.push_str(&line);
        out.push('\n');
    }

    if !bag.corrections.is_empty() {
        out.push_str("\nCORRECTIONS THE WRITER MADE BEFORE:\n");
        for correction in &bag.corrections {
            match correction.field.as_str() {
                "person" => out.push_str(&format!("  people \"{}\" → {}\n", correction.before, correction.after)),
                field => out.push_str(&format!("  {field} \"{}\" → \"{}\"\n", correction.before, correction.after)),
            }
        }
    }

    out.push_str("\nALREADY KEPT THIS DAY:\n");
    if bag.kept.is_empty() {
        out.push_str("  (none)\n");
    }
    for title in &bag.kept {
        out.push_str(&format!("  {title}\n"));
    }

    out.push('\n');
    for block in &bag.read {
        out.push_str(&format!("[{} · READ]    {}\n{}\n", block.code, block.node_title, block.block.text));
    }
    for (n, (label, text)) in bag.context.iter().enumerate() {
        out.push_str(&format!("[x{} · CONTEXT] {label}\n{text}\n", n + 1));
    }
    out
}

/// The shape a reply must have (§6), for providers that can be held to one.
///
/// The kinds come from the vault, so a kind somebody added is one the model
/// may answer with, and one it has never heard of is not.
pub fn schema(categories: &[String]) -> Value {
    let text = json!({ "type": "string" });
    let maybe_text = json!({ "type": ["string", "null"] });
    json!({
        "type": "object",
        "properties": {
            "moments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": text,
                        "date": text,
                        "date_to": maybe_text,
                        "ongoing": { "type": "boolean" },
                        "precision": { "type": "string", "enum": ["day", "week", "month", "year"] },
                        "time": maybe_text,
                        "date_basis": { "type": "string", "enum": ["explicit", "relative", "the_day", "inferred"] },
                        "people": {
                            "type": "array",
                            "items": { "type": "object", "properties": { "ref": text, "name": text } }
                        },
                        "place": maybe_text,
                        "about": { "type": "array", "items": text },
                        "category": { "type": "string", "enum": categories },
                        "amount": {
                            "type": ["object", "null"],
                            "properties": { "value": { "type": "number" }, "unit": text }
                        },
                        "quote": {
                            "type": "object",
                            "properties": { "source": text, "text": text },
                            "required": ["source", "text"]
                        },
                        "also": { "type": "array", "items": text }
                    },
                    "required": ["title", "date", "date_basis", "category", "quote"]
                }
            }
        },
        "required": ["moments"]
    })
}

/// One moment as the model wrote it. Everything optional: what is missing is
/// the checks' to judge, not the parser's to reject.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RawMoment {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub date_to: Option<String>,
    #[serde(default)]
    pub ongoing: bool,
    #[serde(default)]
    pub precision: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub date_basis: Option<String>,
    /// `{"ref": …}` or `{"name": …}` — or a bare name, which models write too.
    #[serde(default)]
    pub people: Vec<Value>,
    #[serde(default)]
    pub place: Option<String>,
    #[serde(default)]
    pub about: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub amount: Option<Value>,
    #[serde(default)]
    pub quote: Option<Quote>,
    #[serde(default)]
    pub also: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Quote {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub text: String,
}

/// The moments in a reply, or `None` when it is not the JSON asked for.
pub fn parse_reply(reply: &str) -> Option<Vec<RawMoment>> {
    #[derive(Deserialize)]
    struct Shape {
        #[serde(default)]
        moments: Vec<RawMoment>,
    }
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    (start < end).then(|| serde_json::from_str::<Shape>(&reply[start..=end]).ok()).flatten().map(|shape| shape.moments)
}

// ─── Checking (§7) ───────────────────────────────────────────────

fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The span a moment's dates name.
fn span_of(moment: &RawMoment) -> Option<Span> {
    let start = when::parse(moment.date.trim())?;
    let start = match moment.precision.as_deref() {
        // A week is not a unit `when` knows. Seven days from the day given.
        Some("week") => Span { from: start.from, to: start.from + chrono::Duration::days(6), precision: Precision::Range },
        _ => start,
    };
    match moment.date_to.as_deref().map(str::trim).filter(|t| !t.is_empty()).and_then(when::parse) {
        Some(end) if end.to >= start.from => Some(Span { from: start.from, to: end.to, precision: Precision::Range }),
        _ => Some(start),
    }
}

fn clock(time: Option<&str>) -> Option<String> {
    let time = time?.trim();
    let (h, m) = time.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.get(..2)?.parse().ok()?);
    (h < 24 && m < 60).then(|| format!("{h:02}:{m:02}"))
}

fn amount_of(value: Option<&Value>) -> Option<Amount> {
    let value = value?;
    let number = value.get("value").and_then(Value::as_f64).filter(|n| n.is_finite() && *n >= 0.0)?;
    let unit = value.get("unit").and_then(Value::as_str).map(str::trim).filter(|u| !u.is_empty()).unwrap_or("VND");
    Some(Amount { value: number, unit: unit.to_uppercase() })
}

/// A reply, checked, dated and joined to people.
pub fn settle(bag: &Bag, raw: Vec<RawMoment>, directory: &Directory, model: &str) -> (Vec<Extracted>, Dropped) {
    let mut items: Vec<Extracted> = Vec::new();
    let mut dropped = Dropped::default();
    let kept: HashSet<String> = bag.kept.iter().map(|t| tidy(t)).collect();
    // "the same moment, said twice": the codes of the other blocks it is also
    // in — the note's sentence, the task ticked off, the calendar entry.
    let also: HashMap<&str, &str> = bag.read.iter().map(|block| (block.code.as_str(), block.node.as_str())).collect();

    for (n, moment) in raw.into_iter().enumerate() {
        let title = moment.title.trim().to_string();
        if title.is_empty() {
            dropped.untitled += 1;
            dropped.each.push(("untitled", moment_as_value(&moment)));
            continue;
        }
        // Only a READ block is evidence. A quote from the context was read
        // before, and is somebody else's proposal already.
        let quote = moment.quote.clone().unwrap_or_default();
        let Some(block) = bag.read.iter().find(|b| b.code == quote.source.trim()) else {
            dropped.no_quote += 1;
            dropped.each.push(("no_quote", moment_as_value(&moment)));
            continue;
        };
        let Some(found) = extract::find_quote(&block.block.raw, &quote.text) else {
            dropped.no_quote += 1;
            dropped.each.push(("no_quote", moment_as_value(&moment)));
            continue;
        };
        let Some(span) = span_of(&moment) else {
            dropped.undated += 1;
            dropped.each.push(("undated", moment_as_value(&moment)));
            continue;
        };
        if span.from > bag.day {
            dropped.not_yet += 1;
            dropped.each.push(("not_yet", moment_as_value(&moment)));
            continue;
        }
        let span = if moment.ongoing {
            Span { from: span.from, to: bag.day, precision: if span.from == bag.day { Precision::Day } else { Precision::Range } }
        } else {
            extract::up_to(span, bag.day)
        };
        if kept.contains(&tidy(&title)) || items.iter().any(|item| tidy(&item.payload.title) == tidy(&title) && item.happened_from == when::iso(span.from)) {
            dropped.repeated += 1;
            dropped.each.push(("repeated", moment_as_value(&moment)));
            continue;
        }

        let mut ids: Vec<String> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut unmatched = 0;
        for who in &moment.people {
            let (reference, name) = match who {
                Value::String(name) => (None, Some(name.as_str())),
                Value::Object(fields) => (fields.get("ref").and_then(Value::as_str), fields.get("name").and_then(Value::as_str)),
                _ => (None, None),
            };
            let person = reference.and_then(|code| bag.person(code.trim())).or_else(|| name.and_then(|n| directory.find(n)));
            match (person, name.map(str::trim).filter(|n| !n.is_empty())) {
                (Some(person), _) if !directory.is_writer(&person.id) => {
                    if !ids.contains(&person.id) {
                        ids.push(person.id.clone());
                    }
                }
                (None, Some(name)) if !directory.is_writer(name) => {
                    if !names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
                        names.push(name.to_string());
                        unmatched += 1;
                    }
                }
                _ => {}
            }
        }

        let basis = moment.date_basis.as_deref().unwrap_or("the_day").to_string();
        let category = moment
            .category
            .as_deref()
            .map(str::trim)
            .map(str::to_lowercase)
            .filter(|kind| bag.categories.iter().any(|known| known == kind))
            .unwrap_or_else(|| "other".into());
        // How sure the app is, not the model: how the moment stands on the
        // page. For ordering the review, never for leaving anything out.
        let mut sure: f64 = 1.0;
        if found.is_none() {
            sure -= 0.2;
        }
        match basis.as_str() {
            "inferred" => sure -= 0.4,
            "relative" => sure -= 0.1,
            _ => {}
        }
        if block.estimated {
            sure -= 0.2;
        }
        sure -= 0.05 * unmatched as f64;

        let id = blake3::hash(format!("{}\0{}\0{}", bag.hash, extract::EXTRACTOR_VERSION, n).as_bytes()).to_hex();
        items.push(Extracted {
            id: format!("x{}", &id[..20]),
            kind: "moment".into(),
            happened_from: when::iso(span.from),
            happened_to: when::iso(span.to),
            precision: span.precision.as_str().into(),
            recorded: when::iso(bag.day),
            source: "extract".into(),
            evidence: vec![Evidence {
                node: block.node.clone(),
                hash: block.block.hash.clone(),
                // A node's own name is not anywhere in its body, so there is
                // nothing in the body to mark.
                span: found
                    .filter(|_| !block.block.from_title)
                    .map(|[from, to]| [block.block.start + from, block.block.start + to]),
            }],
            extractor: Extractor { version: extract::EXTRACTOR_VERSION, model: model.to_string() },
            confidence: sure.clamp(0.0, 1.0),
            payload: Payload {
                title,
                people: ids,
                names,
                place: moment.place.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(String::from),
                quote: quote.text.trim().to_string(),
                category: Some(category),
                amount: amount_of(moment.amount.as_ref()),
                about: moment.about.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).take(3).collect(),
                time: clock(moment.time.as_deref()),
                date_basis: Some(basis),
                also: moment.also.iter().filter_map(|code| also.get(code.trim()).map(|node| node.to_string())).collect(),
            },
            superseded_by: None,
            about_moment: None,
            verdict: None,
        });
    }
    (items, dropped)
}

fn moment_as_value(moment: &RawMoment) -> Value {
    serde_json::to_value(moment).unwrap_or(Value::Null)
}

// ─── Running ─────────────────────────────────────────────────────

/// Read one bag.
pub async fn read_bag(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    bag: &Bag,
    directory: &Directory,
    now: DateTime<Utc>,
) -> AppResult<(Vec<Extracted>, SourceRun, Dropped)> {
    let started = std::time::Instant::now();
    let messages = vec![ChatMessage::new("system", INSTRUCTIONS), ChatMessage::new("user", message(bag))];
    let shape = schema(&bag.categories);
    let request = |held| ChatRequest {
        model,
        messages: &messages,
        temperature: Some(0.0),
        num_ctx,
        tools: None,
        json_schema: held,
    };
    // Held to the schema where the provider can be. A server that does not
    // know the field refuses the whole request, so it is asked once more
    // without, and the reply is checked the same either way.
    let reply = match provider.chat(request(Some(&shape))).await {
        Ok(reply) => reply,
        Err(e) => {
            log::info!("timeline reader: asked again without a schema after: {e}");
            provider.chat(request(None)).await?
        }
    };
    let raw = parse_reply(&reply.content)
        .ok_or_else(|| AppError::General(format!("the reply for {} was not the JSON asked for", bag.key)))?;
    let (items, dropped) = settle(bag, raw, directory, model);
    let ms = reply.duration_ms.unwrap_or_else(|| started.elapsed().as_millis() as u64);
    let run = SourceRun {
        node: format!("day:{}", bag.key),
        hash: bag.hash.clone(),
        version: extract::EXTRACTOR_VERSION,
        model: model.to_string(),
        at: crate::utils::timestamp::canonical(now),
        items: items.iter().map(|item| item.id.clone()).collect(),
        dropped: dropped.total(),
        chars: bag.chars(),
        ms,
        blocks: bag.block_hashes(),
    };
    Ok((items, run, dropped))
}

/// The key a failure is remembered by: what was read.
pub fn failure_key(bag: &Bag) -> String {
    format!("extract:{}", bag.hash)
}

/// Ask about each changed moment in turn, keeping each answer as it comes.
pub async fn read_all_changes(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    changes: &[Change],
    directory: &Directory,
    vault_path: &str,
    device: &str,
) -> extract::ExtractRun {
    let mut report = extract::ExtractRun::default();
    for change in changes {
        match read_change(provider, model, num_ctx, change, directory, Utc::now()).await {
            Ok((items, run, _)) => {
                let count = items.len();
                match extract::record(vault_path, device, run, &items, Utc::now()) {
                    Ok(()) => {
                        report.read += 1;
                        report.items += count;
                    }
                    Err(e) => report.failed.push(format!("{}: {e}", change.moment.path)),
                }
            }
            Err(e) => report.failed.push(format!("{}: {e}", change.moment.path)),
        }
    }
    report
}

/// Read each bag in turn and keep what each yields as soon as it is read, so
/// stopping halfway loses one reading at most.
pub async fn read_all(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    bags: &[Bag],
    directory: &Directory,
    vault_path: &str,
    device: &str,
) -> extract::ExtractRun {
    let mut report = extract::ExtractRun::default();
    for bag in bags {
        match read_bag(provider, model, num_ctx, bag, directory, Utc::now()).await {
            Ok((items, run, gates)) => {
                let (count, dropped) = (items.len(), run.dropped);
                let at = run.at.clone();
                match extract::record(vault_path, device, run, &items, Utc::now()) {
                    Ok(()) => {
                        extract::clear_failure(&failure_key(bag));
                        report.read += 1;
                        report.items += count;
                        report.dropped += dropped;
                        if !gates.each.is_empty() {
                            report.drops.push(extract::Drops {
                                node: format!("day:{}", bag.key),
                                hash: bag.hash.clone(),
                                model: model.to_string(),
                                at,
                                each: gates.each,
                            });
                        }
                    }
                    Err(e) => {
                        extract::note_failure(&failure_key(bag));
                        report.failed.push(format!("{}: {e}", bag.key));
                    }
                }
            }
            Err(e) => {
                extract::note_failure(&failure_key(bag));
                report.failed.push(format!("{}: {e}", bag.key));
            }
        }
    }
    report
}

#[cfg(test)]
mod tests;
