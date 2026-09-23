//! Tier 1: what a model reads out of the person's own words.
//!
//! The design is §4.7 and §4.8.2 of `docs/timeline-2026-09-17.md`, and this is
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
//! notes are never changed by it. Keeping a proposal in the tray writes the
//! moment to a file of its own (`timeline::moments`). Until then the
//! proposals are left out of every answer the timeline gives — to Syn, to
//! Nexus, to People — because a guess is not a record (§6).
//!
//! # How it is read
//!
//! By block and by day: `timeline::reader`, and
//! `docs/timeline-extract-v3-2026-09-22.md`. This module keeps what every
//! reading shares — the configuration, the month files, the decisions, and
//! the proposals waiting for one.
//!
//! # What is read
//!
//! Notes, interactions, person notes, quick captures and events; Syn
//! conversations only when turned on, and then only what the person wrote,
//! never what Syn answered. `timeline: false` in a note's frontmatter keeps it
//! out. A moment is not read back: that is what the reader writes.
//!
//! # Edited notes
//!
//! An edit is read as blocks: a block changed only a little is the block it
//! was, and is not read again; a block that is new is read with its day.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::{DateTime, Local, NaiveDate, Utc};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::magnitude::{self, Signals};
use super::when::{self, Precision, Span};
use super::FOLDER;
use crate::error::{AppError, AppResult};

/// Bump when the prompt or the reading of a reply changes enough that an old
/// reading is worth replacing. Nothing is read again on its own when it does;
/// the tray offers it (§4.8.2).
///
/// 3: blocks and bags of a day (`timeline::reader`). What the second reader
/// read is not read again on its own; the tray offers it.
pub const EXTRACTOR_VERSION: u32 = 3;

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
    /// A cloud provider may read notes. Off unless chosen for this vault (§8.6).
    #[serde(default)]
    pub allow_cloud: bool,
    /// Model đọc nhật ký, khi nó khác model của trợ lý.
    ///
    /// §8.6 hứa hai điều cùng lúc: nhật ký chỉ được đọc bởi model mà vault này
    /// đã chọn cho việc đọc, **và** câu hỏi thường ngày gửi trợ lý thì không
    /// đổi gì. Hai vế đó chỉ đứng cùng nhau được nếu chỗ này có model riêng —
    /// nếu không, muốn đổi model đọc nhật ký là phải đổi cả trợ lý theo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Model ấy chạy ở đâu. Bỏ trống thì dùng đúng provider của trợ lý.
    ///
    /// Tách khỏi `model` vì hai thứ đổi độc lập: đổi sang một model khác của
    /// cùng nhà thì chỉ cần `model`, còn đọc nhật ký bằng máy này trong khi
    /// trợ lý vẫn ở trên mây thì mới cần tới đây. Provider nào không chạy trên
    /// máy này thì vẫn phải qua cửa `allow_cloud` như mọi khi.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<crate::models::syn::SynProvider>,
    /// Ordinary notes under these folders are read too.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Ordinary notes with these tags are read too.
    #[serde(default)]
    pub tags: Vec<String>,
    /// What the person wrote to Syn is read too.
    #[serde(default)]
    pub conversations: bool,
    /// The kinds a moment can be, in this vault.
    ///
    /// In the vault rather than in the code because a life is not a list
    /// somebody else wrote: the first reading of the real vault put 60% of
    /// what was kept in `meal` and had no kind at all for *"Hà Nội nắng nóng
    /// kỷ lục"*. The defaults are `reader::DEFAULT_CATEGORIES`; the person
    /// adds to them in the review, and the model is held to whatever the list
    /// says — it never invents one.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Whatever else the file holds, sync's `metadata` among it, kept as found.
    #[serde(flatten, default)]
    pub rest: Map<String, Value>,
}

/// Thiết lập để gọi model đọc nhật ký, và tên model đó.
///
/// Vault có thể chọn một model riêng cho việc đọc — tên model, nơi nó chạy,
/// hoặc cả hai. Chỗ nào không chọn thì lấy đúng của trợ lý. Kết quả đi qua
/// cùng một cửa như trước: `runs_here` hỏi model có chạy trên máy này không,
/// và nếu không thì vault phải đã bật `allow_cloud` (§8.6).
impl Config {
    /// The kinds a moment can be: the vault's own list, or the defaults while
    /// it has none.
    pub fn categories(&self) -> Vec<String> {
        let mine: Vec<String> = self
            .categories
            .iter()
            .map(|kind| kind.trim().to_lowercase())
            .filter(|kind| !kind.is_empty())
            .collect();
        if mine.is_empty() {
            return super::reader::DEFAULT_CATEGORIES.iter().map(|kind| kind.to_string()).collect();
        }
        // Whatever the list says, there is always somewhere to put what fits
        // nowhere. A reading with no way to say "I do not know" says something
        // else instead.
        let mut mine = mine;
        if !mine.iter().any(|kind| kind == "other") {
            mine.push("other".into());
        }
        mine.dedup();
        mine
    }
}

pub fn reader(
    config: &Config,
    settings: &crate::models::syn::SynSettings,
) -> (crate::models::syn::SynSettings, Option<String>) {
    let mut here = settings.clone();
    if let Some(provider) = config.provider {
        here.provider = provider;
    }
    if let Some(model) = config.model.clone().map(|m| m.trim().to_string()).filter(|m| !m.is_empty())
    {
        here.default_model = Some(model);
    }
    let model = here.default_model.clone();
    (here, model)
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
            "whiteboard" | "filter" | "view" | "schema" | "canvas" | "json" | "file" | "project"
                // What the reader wrote. Read back, every kept moment would
                // come back as a proposal of itself.
                | "moment"
        )
}

/// §4.8.2, and the frontmatter switch that overrides it for one note.
pub fn wanted(node_type: &str, properties: &Value, id: &str, config: &Config) -> bool {
    if never(node_type) {
        return false;
    }
    match properties.get("timeline") {
        Some(Value::Bool(false)) => return false,
        Some(Value::Bool(true)) => return true,
        _ => {}
    }
    // Every note, not only the ones with a day in their name. A project's
    // note says "hôm nay golive" as surely as a daily note does, and the
    // block it is in knows which day that was (§4.2). The folders and tags
    // still say so for the other kinds.
    match node_type {
        // A finished task and a calendar entry are read like anything else
        // (§14, chốt 2026-09-23): what was done that day is written in them
        // as much as in a note.
        "interaction" | "person" | "quickcap" | "event" | "note" | "task" => true,
        _ => {
            config.folders.iter().any(|folder| {
                let folder = folder.trim().trim_matches('/');
                !folder.is_empty() && id.starts_with(&format!("{folder}/"))
            }) || tagged(properties, &config.tags)
        }
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

pub(crate) fn local_day(stamp: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|d| d.with_timezone(&Local).date_naive())
        .or_else(|| when::parse(stamp).filter(|s| s.precision == Precision::Day).map(|s| s.from))
}

/// The day a node was written, and whether it is about that day.
pub(crate) fn recorded(node_type: &str, properties: &Value, created_at: &str) -> Option<(NaiveDate, bool)> {
    let field = match node_type {
        "note" | "interaction" => text(properties, "date"),
        "event" => text(properties, "start_at").or_else(|| text(properties, "event_date")),
        // The day a task happened is the day it was finished. One still open
        // is a plan, and a plan is nobody's day.
        "task" => text(properties, "completed_at"),
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

pub(crate) fn fingerprint(text: &str) -> String {
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

/// What the person wrote to Syn, one day of one conversation at a time:
/// `(node, title, day, messages)`.
///
/// Only `user` turns. What Syn said is not a fact about anybody's life, and
/// reading it would put the assistant's guesses back in as the person's
/// memories (§4.8.2, rule 3).
pub(crate) fn conversation_days(
    vault_path: &str,
    today: NaiveDate,
    settled_before: Option<DateTime<Utc>>,
) -> Vec<(String, String, NaiveDate, Vec<String>)> {
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
            if day > today {
                continue;
            }
            if let Some(before) = settled_before {
                if DateTime::parse_from_rfc3339(&last).is_ok_and(|at| at.with_timezone(&Utc) > before) {
                    continue;
                }
            }
            out.push((
                format!("{rel}#{}", when::iso(day)),
                title.clone(),
                day,
                said.iter().map(|s| s.to_string()).collect(),
            ));
        }
    }
    out
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
    /// later decides it deserves one (§14, question 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(default)]
    pub quote: String,
    /// One of `reader::CATEGORIES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// A sum the words named. The first number a moment carries (§6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<Amount>,
    /// Projects, organisations, topics named.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub about: Vec<String>,
    /// `HH:MM`, when the words gave a clock time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// How the day was worked out: `explicit`, `relative`, `the_day`, `inferred`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_basis: Option<String>,
    /// Calendar entries and tasks that are the same moment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
}

/// A sum of money, as written: `50k` is `{ value: 50000, unit: "VND" }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Amount {
    pub value: f64,
    pub unit: String,
}

/// One proposed moment, as it is written in a month file (§4.7).
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
    /// The moment this is a change to, when it is one: `Moments/<uuid>.md`.
    /// A proposal for something new has none. See §15 of
    /// `docs/timeline-extract-v3-2026-09-22.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about_moment: Option<String>,
    /// For a change: `changed`, `retracted` (the words are gone) or `gone`
    /// (the whole note is).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
}

/// What was left out of a reply, and why.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Dropped {
    /// The quote is not in the note: nothing shows the moment came from it.
    pub no_quote: usize,
    /// No time the code can read, or none at all in a note not about one day.
    pub undated: usize,
    /// After the day it was written: a plan.
    pub not_yet: usize,
    pub untitled: usize,
    /// The same moment twice in one reply, or one already kept that day.
    pub repeated: usize,
    /// Each one left out, with the gate that stopped it and what the model
    /// said, as it said it.
    ///
    /// The counts alone were all that was kept, and a golive the model found
    /// on 06-06 was lost without anybody being able to say which gate it fell
    /// at. See `remember_drops`.
    #[serde(skip)]
    pub each: Vec<(&'static str, Value)>,
}

impl Dropped {
    pub fn total(&self) -> usize {
        self.no_quote + self.undated + self.not_yet + self.untitled + self.repeated
    }
}

/// Where `quote` is in `text`: `Some(Some(span))` found exactly, `Some(None)`
/// found once whitespace and case are set aside, `None` not there.
///
/// Looked for in the note as written, then in the note as it reads (see
/// [`super::blocks::plain`]), so a quote that left out a link's markup is still found — and
/// still found *where it is*, for the source view to mark.
pub(crate) fn find_quote(text: &str, quote: &str) -> Option<Option<[usize; 2]>> {
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
    let (reads, from) = super::blocks::plain(text);
    let (quote, _) = super::blocks::plain(quote);
    let quote = quote.trim();
    if quote.chars().count() < 4 {
        return None;
    }
    if let Some(at) = reads.find(quote) {
        let first = reads[..at].chars().count();
        let last = first + quote.chars().count() - 1;
        return Some(Some([from[first], from[last] + 1]));
    }
    let tidy = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    tidy(&reads).contains(&tidy(quote)).then_some(None)
}

/// The day a span ends, cut back to the day the input was written.
pub(crate) fn up_to(span: Span, day: NaiveDate) -> Span {
    if span.to <= day {
        return span;
    }
    Span {
        from: span.from,
        to: day,
        precision: if span.from == day { Precision::Day } else { Precision::Range },
    }
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
    /// The blocks it read (`timeline::blocks::key`). A reading of whole notes,
    /// before blocks, has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
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
pub(crate) fn month_files(vault_path: &str) -> Vec<(String, PathBuf)> {
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

/// Take the moments and the readings out of one month file, and say how many
/// transcripts it kept.
///
/// For starting the timeline again (`timeline::reset`). The file itself stays
/// — emptied if that is all it held — because another device may be writing to
/// it, and a file that comes back empty is easier to merge than one that comes
/// back missing. Transcripts and captions are not moments and are left alone.
pub(crate) fn forget_readings(vault_path: &str, rel: &str) -> AppResult<usize> {
    let _writing = writing();
    let path = Path::new(vault_path).join(rel);
    let text = std::fs::read_to_string(&path).map_err(|e| io(&path, e))?;
    let mut file: MonthFile = serde_json::from_str(&text)
        .map_err(|e| AppError::General(format!("{rel} cannot be read ({e}), so nothing was taken out of it")))?;
    file.items.clear();
    file.sources.clear();
    let kept = file.surrogates.len();
    write_json(&path, &file)?;
    Ok(kept)
}

/// Every file holding decisions, one per device: vault-relative.
pub(crate) fn review_files(vault_path: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join(REVIEWS_DIR)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".json"))
        .map(|name| format!("{REVIEWS_DIR}/{name}"))
        .collect();
    out.sort();
    out
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

/// What the person changed in the review before keeping it.
///
/// Every field of a proposal is theirs to put right — a nearly right one used
/// to be either kept wrong or thrown away, because only the sentence could be
/// edited. What they do not send is left as the reading had it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Edits {
    pub title: Option<String>,
    pub happened_from: Option<String>,
    pub happened_to: Option<String>,
    pub precision: Option<String>,
    /// `HH:MM`, or empty to take the clock time off.
    pub time: Option<String>,
    /// Who took part, already resolved: an identity, or a name as written.
    pub people: Option<Vec<String>>,
    pub place: Option<String>,
    pub category: Option<String>,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub about: Option<Vec<String>>,
}

/// The proposal as the person decided to keep it, and which fields are now
/// theirs rather than the reader's.
///
/// # Why the quote cannot be edited
///
/// The **quote** is the line in the note this was read from, and it is what
/// makes the whole of extraction answerable — §16 Bước 6 calls a sentence that
/// does not match the vault a ship-blocking error. A person rewriting the
/// evidence would be writing the note's past.
///
/// What is kept carries the proposal's `extract` id either way, so the two can
/// always be held up against each other, and the fields in `hand` are the ones
/// a later reading must never propose over (§15.2).
pub fn as_kept(item: &Extracted, edits: &Edits) -> AppResult<(Extracted, Vec<String>)> {
    let mut kept = item.clone();
    let mut hand = Vec::new();
    let by_hand = |field: &str, hand: &mut Vec<String>| hand.push(field.to_string());

    if let Some(title) = edits.title.as_deref() {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::General(
                "A moment with no title is not a moment. Write one, or discard it.".into(),
            ));
        }
        if title != item.payload.title {
            kept.payload.title = title.to_string();
            by_hand("title", &mut hand);
        }
    }
    if let Some(from) = edits.happened_from.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        let to = edits.happened_to.as_deref().map(str::trim).filter(|d| !d.is_empty()).unwrap_or(from);
        let (from, to) = if from <= to { (from, to) } else { (to, from) };
        if from != item.happened_from || to != item.happened_to {
            kept.happened_from = from.to_string();
            kept.happened_to = to.to_string();
            kept.precision = edits
                .precision
                .clone()
                .unwrap_or_else(|| if from == to { "day".into() } else { "range".into() });
            by_hand("happened", &mut hand);
        }
    }
    let changed = |now: Option<&str>, before: Option<&str>| now.map(str::trim) != before.map(str::trim) && now.is_some();
    if changed(edits.time.as_deref(), item.payload.time.as_deref()) {
        kept.payload.time = edits.time.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(String::from);
        by_hand("time", &mut hand);
    }
    if changed(edits.place.as_deref(), item.payload.place.as_deref()) {
        kept.payload.place = edits.place.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(String::from);
        by_hand("where", &mut hand);
    }
    if changed(edits.category.as_deref(), item.payload.category.as_deref()) {
        kept.payload.category = edits.category.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(String::from);
        by_hand("category", &mut hand);
    }
    if let Some(people) = &edits.people {
        let people: Vec<String> = people.iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        let before: Vec<&String> = item.payload.people.iter().chain(&item.payload.names).collect();
        if people.iter().collect::<Vec<_>>() != before {
            // An identity is a path or a uuid; anything else is a name still
            // waiting for somebody to say who it is.
            let (ids, names): (Vec<String>, Vec<String>) =
                people.into_iter().partition(|who| who.contains('/') || who.len() == 36);
            kept.payload.people = ids;
            kept.payload.names = names;
            by_hand("people", &mut hand);
        }
    }
    if let Some(about) = &edits.about {
        let about: Vec<String> = about.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
        if about != item.payload.about {
            kept.payload.about = about;
            by_hand("about", &mut hand);
        }
    }
    let amount = edits.amount.filter(|value| value.is_finite() && *value > 0.0).map(|value| Amount {
        value,
        unit: edits
            .unit
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .unwrap_or("VND")
            .to_uppercase(),
    });
    if edits.amount.is_some() && amount != item.payload.amount {
        kept.payload.amount = amount;
        by_hand("amount", &mut hand);
    }
    Ok((kept, hand))
}

pub fn moment_entry(item: &Extracted) -> Value {
    moment_entry_with(item, &[])
}

/// The same, saying which fields the person wrote themselves (§15.2).
pub fn moment_entry_with(item: &Extracted, hand: &[String]) -> Value {
    let people: Vec<&String> = item.payload.people.iter().chain(&item.payload.names).collect();
    let mut entry = json!({
        "id": super::moments::settled_id(&["kept", &item.id]),
        "title": item.payload.title,
        "happened": happened_text(item),
        "people": people,
        "extract": item.id,
    });
    if let Value::Object(fields) = &mut entry {
        let payload = &item.payload;
        if let Some(place) = &payload.place {
            fields.insert("where".into(), Value::String(place.clone()));
        }
        if !payload.about.is_empty() {
            fields.insert("about".into(), json!(payload.about));
        }
        if let Some(category) = &payload.category {
            fields.insert("category".into(), Value::String(category.clone()));
        }
        if let Some(amount) = &payload.amount {
            fields.insert("amount".into(), json!({ "value": amount.value, "unit": amount.unit }));
        }
        if let Some(time) = &payload.time {
            fields.insert("time".into(), Value::String(time.clone()));
        }
        if !hand.is_empty() {
            fields.insert("hand".into(), json!(hand));
        }
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
            ms         INTEGER NOT NULL,
            -- The blocks a reading read, as JSON. Empty for readings of whole notes.
            blocks     TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_extract_runs_node ON extract_runs(node_id);
        -- What a reading left out, and at which gate. This device only; see `remember_drops`.
        CREATE TABLE IF NOT EXISTS extract_drops (
            node_id TEXT NOT NULL,
            hash    TEXT NOT NULL,
            model   TEXT NOT NULL,
            at      TEXT NOT NULL,
            gate    TEXT NOT NULL,
            raw     TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_extract_drops_node ON extract_drops(node_id);
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
    .map_err(sql)?;
    // A `timeline.db` made before readings were by block.
    let has_blocks = conn
        .prepare("SELECT 1 FROM pragma_table_info('extract_runs') WHERE name = 'blocks'")
        .and_then(|mut stmt| stmt.exists([]))
        .map_err(sql)?;
    if !has_blocks {
        conn.execute("ALTER TABLE extract_runs ADD COLUMN blocks TEXT NOT NULL DEFAULT '[]'", [])
            .map_err(sql)?;
    }
    Ok(())
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
/// device's May costs one file (§4.7). No model is involved: losing
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
                     node_id, node_type, title, node_title, source, confidence, evidence, month_file,
                     magnitude, container_node)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'extract', ?6, ?7, '', ?8, '', 'extract', ?9, ?10, ?11, ?12, ?7)",
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
            // `docs/timeline-2026-09-17.md` §4.2.
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
                "INSERT INTO extract_runs (month_file, device, node_id, hash, version, model, at, items, dropped, chars, ms, blocks)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                    serde_json::to_string(&run.blocks).unwrap_or_else(|_| "[]".into()),
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
                 node_id, node_type, title, node_title, source, confidence, magnitude, container_node)
             VALUES (?1, 'moment', ?2, ?3, ?4, 'review', ?5, ?6, 'syn_conversation', ?7, ?7, 'user', ?8, ?9, ?6)",
            params![
                format!("{}#accepted", moment.id),
                moment.happened_from,
                moment.happened_to,
                moment.precision,
                moment.recorded,
                decision.node,
                moment.payload.title,
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
///
/// The words quoted, not the whole file. Checked against a fingerprint of
/// the file, a letter added at the bottom of a note locked Keep on every
/// proposal read from it — each of them still standing on words that had not
/// changed.
pub fn still_reads_as_read(content: &str, item: &Extracted) -> bool {
    item.evidence.first().is_none_or(|e| e.hash == fingerprint(content.trim()))
        || find_quote(content, &item.payload.quote).is_some()
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
    /// The words it stands on are no longer in the note.
    pub stale: bool,
    pub category: Option<String>,
    pub amount: Option<Amount>,
    pub about: Vec<String>,
    pub time: Option<String>,
    pub place: Option<String>,
    pub date_basis: Option<String>,
    /// The kept moment this would change, and how (§15).
    pub about_moment: Option<String>,
    pub verdict: Option<String>,
}

/// Finds a note by the path it was read at, or by the words quoted from it,
/// and says where it is now. See [`proposals`].
pub type FindNote<'a> = dyn Fn(&str, &str) -> Option<(String, NoteNow)> + 'a;

/// A note as it is now, for judging what was read from it.
pub struct NoteNow {
    pub title: String,
    pub node_type: String,
    pub content: String,
}

/// One reading, as `extract_runs` holds it.
struct Reading {
    node: String,
    hash: String,
    version: u32,
    at: String,
    items: Vec<String>,
    blocks: Vec<String>,
}

/// What waits for a decision.
///
/// Every item of every reading, less: what was decided; what a later reading
/// of the same words replaced; and what was read from a note
/// that is gone. A proposal whose words are no longer in its note stays, marked
/// stale, so the person sees it went rather than having it vanish.
///
/// What replaces what: a reading of blocks replaces an earlier reading of the
/// same block. A reading of a whole note (before blocks) is replaced by a later
/// whole reading of that note, or by a reading of any block it still has.
///
/// `note` finds a note by the path it was read at and, failing that, by the
/// words quoted from it — a note moved or renamed since is still where the
/// proposal came from, and is kept into where it is now. It answers with the
/// path the note has now.
pub fn proposals(
    conn: &Connection,
    note: &FindNote,
    decided: &HashMap<String, Decision>,
    title_of: &dyn Fn(&str) -> Option<String>,
) -> AppResult<Vec<Proposal>> {
    ensure_schema(conn)?;
    let readings: Vec<Reading> = {
        let mut stmt = conn
            .prepare("SELECT node_id, hash, version, at, items, blocks FROM extract_runs ORDER BY at")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(sql)?;
        rows.flatten()
            .map(|(node, hash, version, at, items, blocks)| Reading {
                node,
                hash,
                version,
                at,
                items: serde_json::from_str(&items).unwrap_or_default(),
                blocks: serde_json::from_str(blocks.as_deref().unwrap_or("[]")).unwrap_or_default(),
            })
            .collect()
    };

    // The latest reading of each block, and of each whole note.
    let mut block_read_at: HashMap<&str, &str> = HashMap::new();
    let mut whole_read: HashMap<&str, Vec<&Reading>> = HashMap::new();
    for reading in &readings {
        for block in &reading.blocks {
            let at = block_read_at.entry(block.as_str()).or_insert(reading.at.as_str());
            if reading.at.as_str() > *at {
                *at = reading.at.as_str();
            }
        }
        if reading.blocks.is_empty() {
            whole_read.entry(reading.node.as_str()).or_default().push(reading);
        }
    }

    // What was decided about, by what it said: a moment read again — from an
    // edited note, or by a newer reader — has a new id and the same words.
    let mut decided_about: HashSet<String> = HashSet::new();
    for id in decided.keys() {
        if let Some(earlier) = item(conn, id)? {
            decided_about.extend(same_moment_keys(&earlier));
        }
    }

    let mut out = Vec::new();
    for reading in &readings {
        for id in &reading.items {
            if decided.contains_key(id) {
                continue;
            }
            let Some(extracted) = item(conn, id)? else {
                continue;
            };
            let Some(evidence) = extracted.evidence.first() else {
                continue;
            };
            let read_at = evidence.node.split('#').next().unwrap_or_default().to_string();
            let is_conversation = read_at.starts_with("Syn/");
            // A change to a kept moment stands or falls with the moment, not
            // with what its note says now — saying the words are gone is the
            // whole point of one.
            let is_a_change = extracted.about_moment.is_some();
            let found = if is_conversation { None } else { note(&read_at, &extracted.payload.quote) };
            if found.is_none() && !is_conversation && !is_a_change {
                // The note is gone, and with it what this was read from.
                continue;
            }
            let (base, now) = match found {
                Some((path, now)) => (path, Some(now)),
                None => (read_at.clone(), None),
            };

            let replaced = if reading.blocks.is_empty() {
                let later_whole = whole_read.get(reading.node.as_str()).is_some_and(|all| {
                    let current = now.as_ref().map(|n| fingerprint(n.content.trim()));
                    // The reading of the note as it is now wins; failing that, the newest.
                    let best = all
                        .iter()
                        .filter(|r| Some(&r.hash) == current.as_ref())
                        .max_by(|a, b| (a.version, &a.at).cmp(&(b.version, &b.at)))
                        .or_else(|| all.iter().max_by(|a, b| a.at.cmp(&b.at)));
                    best.is_some_and(|best| !std::ptr::eq(*best, reading))
                });
                let later_blocks = now.as_ref().is_some_and(|n| {
                    super::blocks::split(&n.content)
                        .iter()
                        .any(|block| block_read_at.get(block.hash.as_str()).is_some_and(|at| *at > reading.at.as_str()))
                });
                later_whole || later_blocks
            } else {
                block_read_at.get(evidence.hash.as_str()).is_some_and(|at| *at > reading.at.as_str())
            };
            if replaced {
                continue;
            }
            if same_moment_keys(&extracted).iter().any(|key| decided_about.contains(key)) {
                continue;
            }
            let stale = !is_a_change && now.as_ref().is_some_and(|n| !still_reads_as_read(&n.content, &extracted));
            let payload = &extracted.payload;
            out.push(Proposal {
                id: extracted.id.clone(),
                node_id: if is_conversation { evidence.node.clone() } else { base.clone() },
                node_title: now
                    .as_ref()
                    .map(|n| n.title.clone())
                    .unwrap_or_else(|| if is_conversation { "Syn".into() } else { base.clone() }),
                node_type: now
                    .as_ref()
                    .map(|n| n.node_type.clone())
                    .unwrap_or_else(|| if is_conversation { "syn_conversation".into() } else { String::new() }),
                recorded: extracted.recorded.clone(),
                happened_from: extracted.happened_from.clone(),
                happened_to: extracted.happened_to.clone(),
                precision: extracted.precision.clone(),
                title: payload.title.clone(),
                people: payload
                    .people
                    .iter()
                    .map(|id| PersonRef { id: id.clone(), title: title_of(id).unwrap_or_else(|| id.clone()) })
                    .collect(),
                names: payload.names.clone(),
                quote: payload.quote.clone(),
                confidence: extracted.confidence,
                model: extracted.extractor.model.clone(),
                stale,
                category: payload.category.clone(),
                amount: payload.amount.clone(),
                about: payload.about.clone(),
                time: payload.time.clone(),
                place: payload.place.clone(),
                date_basis: payload.date_basis.clone(),
                about_moment: extracted.about_moment.clone(),
                verdict: extracted.verdict.clone(),
            });
        }
    }
    // The same proposal from two devices' copies of one reading is one proposal.
    let mut seen = HashSet::new();
    out.retain(|proposal| seen.insert(proposal.id.clone()));
    out.sort_by(|a, b| b.happened_from.cmp(&a.happened_from).then_with(|| b.confidence.total_cmp(&a.confidence)).then_with(|| a.id.cmp(&b.id)));
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
    /// What each reading left out, for `remember_drops`. Not sent to the
    /// window: it is for finding out why, not for showing.
    #[serde(skip)]
    pub drops: Vec<Drops>,
}

/// What one reading left out.
#[derive(Debug, Clone, PartialEq)]
pub struct Drops {
    pub node: String,
    pub hash: String,
    pub model: String,
    pub at: String,
    pub each: Vec<(&'static str, Value)>,
}

/// Keep what readings left out, and why, on this device.
///
/// In `timeline.db` rather than the month files: it is for finding out what
/// a gate is costing, on the machine where the reading ran, and has no
/// business syncing to every other one. The latest reading of a note replaces
/// the one before, so this holds what is true now rather than growing.
pub fn remember_drops(conn: &Connection, drops: &[Drops]) -> AppResult<()> {
    ensure_schema(conn)?;
    let tx = conn.unchecked_transaction().map_err(sql)?;
    for reading in drops {
        tx.execute("DELETE FROM extract_drops WHERE node_id = ?1", params![reading.node]).map_err(sql)?;
        for (gate, raw) in &reading.each {
            tx.execute(
                "INSERT INTO extract_drops (node_id, hash, model, at, gate, raw) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![reading.node, reading.hash, reading.model, reading.at, gate, serde_json::to_string(raw).unwrap_or_default()],
            )
            .map_err(sql)?;
        }
    }
    tx.commit().map_err(sql)
}

#[cfg(test)]
mod tests;
