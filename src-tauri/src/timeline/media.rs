//! Tier 1 for pictures and recordings: words that stand in for them.
//!
//! The design is §4.8.5 of `docs/timeline-2026-09-17.md`. A recording
//! gets a transcript with times; a photograph gets a sentence or two. Both are
//! what make a picture findable, readable by Syn, and shown on a device that
//! does not hold the file.
//!
//! # What they are not
//!
//! Evidence. A transcript is what a model heard and a caption is what a model
//! saw, and either can be wrong. The bytes' fingerprint in the file's own id
//! (`Files/<blake3>.md`) is the record; these are kept under `Timeline/`, which
//! the evidence ledger does not read (`ledger::is_bookkeeping`), and every page
//! Syn reads of them says who wrote it.
//!
//! Nor copies. Nothing of a file's bytes is written under `Timeline/`.
//!
//! # Where they run, and on what
//!
//! Only on a computer, and only on it. Transcripts go to a transcription server
//! the person runs on the same machine — any that speaks the OpenAI shape,
//! `/v1/audio/transcriptions` — and an address that is not the machine itself
//! is refused. Captions go to Ollama, and to nothing else. No library was added
//! for either (§4.8.5: every crate is a size review for the Android build).
//!
//! # No faces
//!
//! Nothing here, or anywhere in the app, detects or recognises a face (§8.2):
//! `tests::nothing_in_the_app_detects_or_recognises_a_face` looks. The caption
//! prompt tells the model not to say who anyone is.

use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use regex::Regex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::seal::Seals;
use super::store::Event;
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::syn::provider::{ChatMessage, ChatProvider, ChatRequest};

/// Bump when a transcript or caption made now would be worth making again.
pub const SURROGATE_VERSION: u32 = 1;

pub const CONFIG_FILE: &str = "Timeline/media.json";

pub const AUDIO_EXTENSIONS: &[&str] = &["m4a", "mp3", "ogg", "oga", "opus", "wav", "aac", "flac", "webm"];
/// What Ollama's vision models read. HEIC is not among them, and converting it
/// would be a library.
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Past this a recording is an hour-long meeting, and uploading it to a server
/// on the same machine still costs the memory of holding it twice.
pub const MAX_AUDIO_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;

/// Pictures further apart than this are two moments (§4.8.5).
pub const MOMENT_GAP_HOURS: i64 = 3;

/// The most files one automatic pass reads.
pub const AUTO_LIMIT: usize = 4;

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("media: {e}"))
}

// ─── Configuration ───────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub transcripts: bool,
    /// The transcription server on this machine, e.g. `http://127.0.0.1:8000`.
    #[serde(default)]
    pub transcribe_url: String,
    #[serde(default)]
    pub transcribe_model: String,
    #[serde(default)]
    pub captions: bool,
    /// An Ollama model that reads images, e.g. `llava` or `gemma3`.
    #[serde(default)]
    pub caption_model: String,
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
    let url = config.transcribe_url.trim();
    if !url.is_empty() && !is_loopback(url) {
        return Err(AppError::General(
            "The transcription server must run on this machine: use an address on localhost or 127.0.0.1".into(),
        ));
    }
    super::extract::stamp(&mut config.rest, now);
    super::extract::write_json(&Path::new(vault_path).join(CONFIG_FILE), config)
}

/// Whether an address is this machine. Anything else would send a recording
/// of somebody's life somewhere they did not see it go (§8.6).
pub fn is_loopback(address: &str) -> bool {
    let Ok(url) = url::Url::parse(address.trim()) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host() {
        // Only the name itself. `whisper.localhost` is resolved like any other
        // name, and a resolver that answers everything sends it elsewhere.
        Some(url::Host::Domain(domain)) => domain == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Whether Syn's model runs on this machine: Ollama, at an address that is
/// this machine. Ollama pointed at another computer, or a hosted one, is not.
pub fn runs_here(settings: &crate::models::syn::SynSettings) -> bool {
    settings.provider.is_local() && is_loopback(&settings.ollama_url)
}

/// The key a failure is remembered by. See `extract::resting`.
pub fn failure_key(input: &MediaInput) -> String {
    format!("media:{}:{}", input.hash, input.kind)
}

/// Transcripts and captions are made on a computer. A phone shows what a
/// computer made (§4.8.5).
pub fn refuse_on_phone(mobile: bool) -> AppResult<()> {
    if mobile {
        return Err(AppError::General(
            "Transcripts and captions are made on a computer; this device shows the ones made there".into(),
        ));
    }
    Ok(())
}

// ─── What a surrogate is ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Segment {
    /// Seconds from the start.
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// A transcript or a caption, as it is written in a month file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Surrogate {
    /// The file node, `Files/<hash>.md`.
    pub node: String,
    /// The fingerprint of the bytes that were read. The key: a file read on
    /// one device is not read again on another.
    pub hash: String,
    /// `transcript` or `caption`.
    pub kind: String,
    pub version: u32,
    pub model: String,
    pub at: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub ms: u64,
}

/// The fingerprint a file node's id is made of.
pub fn hash_of(node_id: &str) -> Option<&str> {
    node_id.strip_prefix("Files/")?.strip_suffix(".md").filter(|hash| !hash.is_empty())
}

fn extension_of<'a>(properties: &'a Value, name: &'a str) -> String {
    properties
        .get("extension")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| name.rsplit_once('.').map(|(_, ext)| ext.to_string()))
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_lowercase()
}

fn name_of<'a>(properties: &'a Value, title: &'a str) -> &'a str {
    properties
        .get("path")
        .and_then(Value::as_str)
        .map(|path| path.rsplit(['/', '\\']).next().unwrap_or(path))
        .filter(|name| !name.is_empty())
        .unwrap_or(title)
}

/// What is made of a file with this extension, under this configuration.
pub fn kind_for(extension: &str, config: &Config) -> Option<&'static str> {
    let extension = extension.to_lowercase();
    if config.transcripts && AUDIO_EXTENSIONS.contains(&extension.as_str()) {
        return Some("transcript");
    }
    if config.captions && IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Some("caption");
    }
    None
}

static TIME_IN_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9])(\d{4})-?(\d{2})-?(\d{2})[ _T-](\d{2})[.:\-]?(\d{2})[.:\-]?(\d{2})(?:[^0-9]|$)").expect("pattern")
});

/// When a picture or recording was made, to the second when anything says,
/// and whether it said the time or only the day.
pub fn when_made(properties: &Value, name: &str) -> Option<(NaiveDateTime, bool)> {
    if let Some(shot) = properties.get("shot_at").and_then(Value::as_str) {
        if let Ok(at) = NaiveDateTime::parse_from_str(shot.trim(), "%Y-%m-%d %H:%M:%S") {
            return Some((at, true));
        }
    }
    if let Some(c) = TIME_IN_NAME.captures(name) {
        let number = |i: usize| c[i].parse::<u32>().ok();
        let at = NaiveDate::from_ymd_opt(c[1].parse().ok()?, number(2)?, number(3)?)
            .and_then(|day| day.and_hms_opt(number(4)?, number(5)?, number(6)?));
        if let Some(at) = at {
            return Some((at, true));
        }
    }
    super::derive::date_in_filename(name)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .map(|at| (at, false))
}

// ─── What is left to read ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct MediaInput {
    pub node_id: String,
    pub hash: String,
    pub kind: &'static str,
    pub path: String,
    pub extension: String,
    pub size: u64,
}

/// Every file this configuration would read, on this device, less what is
/// sealed; and how many were passed over for size.
///
/// `locate` answers where the bytes are on this device and how many there
/// are. A file with no bytes here is somebody else's to read.
pub fn inputs(
    db: &DbBridge,
    config: &Config,
    seals: &Seals,
    locate: impl Fn(&str) -> Option<(String, u64)>,
) -> AppResult<(Vec<MediaInput>, usize)> {
    let mut stmt = db
        .conn()
        .prepare("SELECT id, title, properties FROM nodes WHERE node_type = 'file' ORDER BY id")
        .map_err(sql)?;
    let rows: Vec<(String, String, Value)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                serde_json::from_str(&r.get::<_, Option<String>>(2)?.unwrap_or_default()).unwrap_or(Value::Null),
            ))
        })
        .map_err(sql)?
        .flatten()
        .collect();

    let mut out = Vec::new();
    let mut too_large = 0;
    for (id, title, properties) in rows {
        let name = name_of(&properties, &title);
        let extension = extension_of(&properties, name);
        let (Some(kind), Some(hash)) = (kind_for(&extension, config), hash_of(&id)) else {
            continue;
        };
        if seals.hides(&id) || properties.get("sealed").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if let Some((at, _)) = when_made(&properties, name) {
            let day = at.format("%Y-%m-%d").to_string();
            if seals.periods().iter().any(|p| p.from.as_str() <= day.as_str() && day.as_str() <= p.to.as_str()) {
                continue;
            }
        }
        let Some((path, size)) = locate(&id) else {
            continue;
        };
        let limit = if kind == "transcript" { MAX_AUDIO_BYTES } else { MAX_IMAGE_BYTES };
        if size > limit {
            too_large += 1;
            continue;
        }
        out.push(MediaInput {
            node_id: id.clone(),
            hash: hash.to_string(),
            kind,
            path,
            extension,
            size,
        });
    }
    Ok((out, too_large))
}

#[derive(Debug, Default)]
pub struct Plan {
    pub pending: Vec<MediaInput>,
    pub done: usize,
}

/// What no device has read yet, by fingerprint and kind.
pub fn plan(conn: &Connection, inputs: Vec<MediaInput>) -> AppResult<Plan> {
    super::extract::ensure_schema(conn)?;
    let read: HashSet<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT hash, kind FROM media_surrogates WHERE version = ?1")
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![SURROGATE_VERSION], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql)?;
        rows.flatten().collect()
    };
    let mut plan = Plan::default();
    for input in inputs {
        if read.contains(&(input.hash.clone(), input.kind.to_string())) {
            plan.done += 1;
        } else {
            plan.pending.push(input);
        }
    }
    Ok(plan)
}

// ─── Transcripts ─────────────────────────────────────────────────

/// Where to send a recording, from the address the person gave.
pub fn endpoint(address: &str) -> String {
    let base = address.trim().trim_end_matches('/');
    if base.ends_with("/audio/transcriptions") || base.ends_with("/inference") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/audio/transcriptions")
    } else {
        format!("{base}/v1/audio/transcriptions")
    }
}

pub struct Transcript {
    pub text: String,
    pub segments: Vec<Segment>,
    pub language: Option<String>,
    pub duration: Option<f64>,
}

/// A `verbose_json` reply, as OpenAI, faster-whisper-server and whisper.cpp's
/// server write it.
pub fn parse_transcript(value: &Value) -> Option<Transcript> {
    let segments: Vec<Segment> = value
        .get("segments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|segment| {
            let text = segment.get("text")?.as_str()?.trim().to_string();
            let start = segment.get("start")?.as_f64()?;
            let end = segment.get("end").and_then(Value::as_f64).unwrap_or(start);
            (!text.is_empty() && start >= 0.0).then_some(Segment { start, end: end.max(start), text })
        })
        .collect();
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(String::from)
        .unwrap_or_else(|| segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" "));
    if text.is_empty() {
        return None;
    }
    Some(Transcript {
        language: value.get("language").and_then(Value::as_str).map(String::from),
        duration: value
            .get("duration")
            .and_then(Value::as_f64)
            .or_else(|| segments.last().map(|s| s.end)),
        text,
        segments,
    })
}

pub async fn transcribe(input: &MediaInput, address: &str, model: &str, now: DateTime<Utc>) -> AppResult<Surrogate> {
    if !is_loopback(address) {
        return Err(AppError::General(format!("{address} is not this machine, so nothing was sent")));
    }
    let bytes = tokio::fs::read(&input.path)
        .await
        .map_err(|e| AppError::General(format!("{}: {e}", input.path)))?;
    let name = Path::new(&input.path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("recording.{}", input.extension));
    let mut form = reqwest::multipart::Form::new()
        .part("file", reqwest::multipart::Part::bytes(bytes).file_name(name))
        .text("response_format", "verbose_json")
        .text("timestamp_granularities[]", "segment");
    if !model.trim().is_empty() {
        form = form.text("model", model.trim().to_string());
    }

    let started = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30 * 60))
        .build()
        .map_err(|e| AppError::General(format!("transcription: {e}")))?;
    let response = client
        .post(endpoint(address))
        .multipart(form)
        .send()
        .await
        .map_err(|e| AppError::General(format!("the transcription server did not answer: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        let body: String = response.text().await.unwrap_or_default().chars().take(300).collect();
        return Err(AppError::General(format!("the transcription server answered {status}: {body}")));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| AppError::General(format!("the transcription server's reply was not JSON: {e}")))?;
    let transcript = parse_transcript(&value)
        .ok_or_else(|| AppError::General("the transcription server heard nothing".into()))?;

    Ok(Surrogate {
        node: input.node_id.clone(),
        hash: input.hash.clone(),
        kind: "transcript".into(),
        version: SURROGATE_VERSION,
        model: if model.trim().is_empty() { "transcription server".into() } else { model.trim().into() },
        at: crate::utils::timestamp::canonical(now),
        text: transcript.text,
        segments: transcript.segments,
        language: transcript.language,
        duration: transcript.duration,
        ms: started.elapsed().as_millis() as u64,
    })
}

// ─── Captions ────────────────────────────────────────────────────

pub fn caption_prompt(language: &str) -> String {
    format!(
        "Describe this photograph in one or two plain sentences, in {language}, for the person who took it, \
looking back on it years later: the place, what is happening, the things in it, the time of day if it shows.\n\n\
Do not say who anyone is, even if they look famous or familiar. Do not guess anyone's name, age, sex, \
ethnicity, feelings, or relationship to anyone else: say \"a person\", \"two people\", \"a child\". Do not \
read out documents, screens or cards. Reply with the description only."
    )
}

/// A reply, tidied into a caption, or `None` when there is nothing in it.
pub fn tidy_caption(reply: &str) -> Option<String> {
    let text = reply
        .trim()
        .trim_matches(|c: char| matches!(c, '"' | '“' | '”' | '\''))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let text: String = text.chars().take(500).collect();
    (!text.is_empty()).then_some(text)
}

pub async fn caption(
    provider: &dyn ChatProvider,
    model: &str,
    num_ctx: u32,
    input: &MediaInput,
    language: &str,
    now: DateTime<Utc>,
) -> AppResult<Surrogate> {
    use base64::Engine;
    let bytes = tokio::fs::read(&input.path)
        .await
        .map_err(|e| AppError::General(format!("{}: {e}", input.path)))?;
    let mut message = ChatMessage::new("user", caption_prompt(language));
    message.images = Some(vec![base64::engine::general_purpose::STANDARD.encode(&bytes)]);
    let messages = vec![message];
    let started = std::time::Instant::now();
    let reply = provider
        .chat(ChatRequest {
            model,
            messages: &messages,
            temperature: Some(0.2),
            num_ctx,
            tools: None,
        })
        .await?;
    let text = tidy_caption(&reply.content)
        .ok_or_else(|| AppError::General(format!("{model} said nothing about {}", input.node_id)))?;
    Ok(Surrogate {
        node: input.node_id.clone(),
        hash: input.hash.clone(),
        kind: "caption".into(),
        version: SURROGATE_VERSION,
        model: model.to_string(),
        at: crate::utils::timestamp::canonical(now),
        text,
        segments: Vec::new(),
        language: None,
        duration: None,
        ms: reply.duration_ms.unwrap_or_else(|| started.elapsed().as_millis() as u64),
    })
}

#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct MediaRun {
    pub read: usize,
    pub failed: Vec<String>,
    pub remaining: usize,
    pub skipped: Option<String>,
}

// ─── Tier 3 ──────────────────────────────────────────────────────

fn surrogate_from_row(node: &str, r: &rusqlite::Row<'_>) -> rusqlite::Result<Surrogate> {
    Ok(Surrogate {
        node: node.to_string(),
        kind: r.get(0)?,
        hash: r.get(1)?,
        version: r.get(2)?,
        model: r.get(3)?,
        at: r.get(4)?,
        text: r.get(5)?,
        segments: serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default(),
        language: r.get(7)?,
        duration: r.get(8)?,
        ms: 0,
    })
}

/// The newest caption and transcript of a file, from any device.
pub fn latest(conn: &Connection, node_id: &str) -> AppResult<(Option<Surrogate>, Option<Surrogate>)> {
    super::extract::ensure_schema(conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT kind, hash, version, model, at, text, segments, language, duration
             FROM media_surrogates WHERE node_id = ?1 ORDER BY version DESC, at DESC",
        )
        .map_err(sql)?;
    let rows: Vec<Surrogate> = stmt
        .query_map(params![node_id], |r| surrogate_from_row(node_id, r))
        .map_err(sql)?
        .flatten()
        .collect();
    let caption = rows.iter().find(|s| s.kind == "caption").cloned();
    let transcript = rows.iter().find(|s| s.kind == "transcript").cloned();
    Ok((caption, transcript))
}

/// `3:12`, or `1:03:12` past the hour.
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Lines a transcript page holds before the next starts.
const LINES_PER_PAGE: usize = 40;

/// What of a file's surrogates is put where search and Syn read file text.
///
/// Each page says a model wrote it, and a transcript says how to cite a moment
/// in it: `Files/<hash>.md#t=START,END`, which opens the recording there.
pub fn search_pages(node_id: &str, caption: Option<&Surrogate>, transcript: Option<&Surrogate>) -> Vec<String> {
    let mut pages = Vec::new();
    if let Some(caption) = caption {
        pages.push(format!(
            "Caption written by {}, a model's description and not a record: {}",
            caption.model, caption.text
        ));
    }
    if let Some(transcript) = transcript {
        let header = format!(
            "Transcript heard by {}; it may be wrong. Times are [minutes:seconds]. To point at a moment, cite {node_id}#t=START,END in seconds, as shown after each line.",
            transcript.model
        );
        if transcript.segments.is_empty() {
            pages.push(format!("{header}\n{}", transcript.text));
        }
        for (i, chunk) in transcript.segments.chunks(LINES_PER_PAGE).enumerate() {
            let lines: Vec<String> = chunk
                .iter()
                .map(|s| format!("[{}] {} (t={:.0},{:.0})", clock(s.start), s.text, s.start.floor(), s.end.ceil()))
                .collect();
            pages.push(if i == 0 { format!("{header}\n{}", lines.join("\n")) } else { lines.join("\n") });
        }
    }
    pages
}

/// Put surrogates loaded since last time where file search and Syn's
/// `read_file_text` find them. A file whose node is not on this device yet
/// waits for it.
pub fn mirror_into_search(conn: &Connection, db: &DbBridge) -> AppResult<usize> {
    super::extract::ensure_schema(conn)?;
    // Waiting: never put there, or put there and since forgotten, as a file
    // trashed and added back is.
    let indexed: HashSet<String> = {
        let mut stmt = db
            .conn()
            .prepare("SELECT node_id FROM file_text_state WHERE status = 'indexed'")
            .map_err(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(sql)?;
        rows.flatten().collect()
    };
    let waiting: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT node_id, MIN(mirrored) FROM media_surrogates GROUP BY node_id")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(sql)?;
        rows.flatten()
            .filter(|(node, mirrored)| *mirrored == 0 || !indexed.contains(node))
            .map(|(node, _)| node)
            .collect()
    };
    let mut mirrored = 0;
    for node_id in waiting {
        let Some(node) = db.get_node(&node_id)? else {
            continue;
        };
        let (caption, transcript) = latest(conn, &node_id)?;
        let pages = search_pages(&node_id, caption.as_ref(), transcript.as_ref());
        if pages.is_empty() {
            continue;
        }
        db.store_file_text(&node_id, &pages)?;
        let chars = pages.iter().map(String::len).sum();
        db.record_text_status(&node_id, crate::db::TextStatus::Indexed, pages.len(), chars, Utc::now().timestamp())?;
        if let Some(meta) = crate::models::file::FileMetadata::from_node(&node) {
            crate::commands::files::index_for_search(db, &node_id, &meta);
        }
        conn.execute("UPDATE media_surrogates SET mirrored = 1 WHERE node_id = ?1", params![node_id])
            .map_err(sql)?;
        mirrored += 1;
    }
    Ok(mirrored)
}

// ─── Moments ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TranscriptView {
    pub text: String,
    pub segments: Vec<Segment>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MediaEntry {
    pub node_id: String,
    pub title: String,
    pub extension: String,
    /// `image`, `audio`, `video` or `other`.
    pub kind: String,
    /// `YYYY-MM-DD HH:MM`, or `YYYY-MM-DD` when only the day is known.
    pub at: String,
    /// The bytes are on this device.
    pub present: bool,
    pub path: Option<String>,
    pub caption: Option<String>,
    pub caption_model: Option<String>,
    pub transcript: Option<TranscriptView>,
}

/// Pictures and recordings close together in time, shown as one (§4.8.5).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MediaMoment {
    pub from: String,
    pub to: String,
    pub count: usize,
    pub cover: MediaEntry,
    pub members: Vec<MediaEntry>,
}

pub fn media_kind(extension: &str) -> &'static str {
    match extension.to_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "webp" | "gif" => "image",
        "m4a" | "mp3" | "ogg" | "oga" | "opus" | "wav" | "aac" | "flac" => "audio",
        "mp4" | "mov" | "m4v" | "mkv" | "webm" => "video",
        _ => "other",
    }
}

fn instant(entry: &MediaEntry) -> Option<(NaiveDateTime, bool)> {
    NaiveDateTime::parse_from_str(&entry.at, "%Y-%m-%d %H:%M")
        .map(|at| (at, true))
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(&entry.at, "%Y-%m-%d")
                .ok()
                .and_then(|day| day.and_hms_opt(12, 0, 0))
                .map(|at| (at, false))
        })
}

/// Group entries into moments: a gap of more than three hours starts a new
/// one. Something known only by its day is grouped with the rest of that
/// day's undated things instead: three hours means nothing without a time,
/// and placing it at noon would let it join, or bridge, moments it was not in.
pub fn group(entries: Vec<MediaEntry>) -> Vec<MediaMoment> {
    let mut placed: Vec<(NaiveDateTime, bool, MediaEntry)> = entries
        .into_iter()
        .filter_map(|entry| instant(&entry).map(|(at, exact)| (at, exact, entry)))
        .collect();
    placed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.node_id.cmp(&b.2.node_id)));

    let mut groups: Vec<Vec<(NaiveDateTime, bool, MediaEntry)>> = Vec::new();
    let (timed, by_day): (Vec<_>, Vec<_>) = placed.into_iter().partition(|item| item.1);
    for item in timed {
        let joins = groups
            .last()
            .and_then(|group| group.last())
            .is_some_and(|last| item.0 - last.0 <= chrono::Duration::hours(MOMENT_GAP_HOURS));
        if joins {
            groups.last_mut().expect("a group").push(item);
        } else {
            groups.push(vec![item]);
        }
    }
    let mut days: Vec<Vec<(NaiveDateTime, bool, MediaEntry)>> = Vec::new();
    for item in by_day {
        match days.last_mut() {
            Some(day) if day[0].0.date() == item.0.date() => day.push(item),
            _ => days.push(vec![item]),
        }
    }
    groups.extend(days);
    groups.sort_by(|a, b| a[0].0.cmp(&b[0].0).then_with(|| a[0].2.node_id.cmp(&b[0].2.node_id)));

    groups
        .into_iter()
        .map(|group| {
            let from = group.first().map(|g| g.2.at.clone()).unwrap_or_default();
            let to = group.last().map(|g| g.2.at.clone()).unwrap_or_default();
            let members: Vec<MediaEntry> = group.into_iter().map(|g| g.2).collect();
            let cover = members
                .iter()
                .find(|m| m.kind == "image" && (m.present || m.caption.is_some()))
                .or_else(|| members.iter().find(|m| m.kind == "image"))
                .unwrap_or(&members[0])
                .clone();
            MediaMoment {
                from,
                to,
                count: members.len(),
                cover,
                members,
            }
        })
        .collect()
}

/// The moments among these timeline items, with what stands in for each file.
pub fn moments(conn: &Connection, db: &DbBridge, seals: &Seals, items: &[Event]) -> AppResult<Vec<MediaMoment>> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    // A file node derives nothing but its own picture or recording, so asking
    // what the node is answers this without a `kind` list (§16 Bước 9).
    for item in items.iter().filter(|item| item.node_type == "file") {
        if seals.hides_item(item) || !seen.insert(item.node_id.clone()) {
            continue;
        }
        let Some(node) = db.get_node(&item.node_id)? else {
            continue;
        };
        let name = name_of(&node.properties, &node.title).to_string();
        let extension = extension_of(&node.properties, &name);
        let at = match when_made(&node.properties, &name) {
            Some((at, true)) => at.format("%Y-%m-%d %H:%M").to_string(),
            _ => item.happened_from.clone(),
        };
        let path = db
            .file_locations_for_node(&item.node_id)
            .unwrap_or_default()
            .into_iter()
            .map(|location| location.abs_path)
            .find(|path| Path::new(path).is_file());
        let (caption, transcript) = latest(conn, &item.node_id)?;
        entries.push(MediaEntry {
            node_id: item.node_id.clone(),
            title: node.title.clone(),
            kind: media_kind(&extension).to_string(),
            extension,
            at,
            present: path.is_some(),
            path,
            caption_model: caption.as_ref().map(|c| c.model.clone()),
            caption: caption.map(|c| c.text),
            transcript: transcript.map(|t| TranscriptView { text: t.text, segments: t.segments, model: t.model }),
        });
    }
    Ok(group(entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::node::NodeMetadata;
    use crate::timeline::TimelineStore;
    use serde_json::json;

    #[test]
    fn only_this_machine_may_hear_a_recording() {
        for here in ["http://127.0.0.1:8000", "http://localhost:9000/v1", "https://[::1]:8443"] {
            assert!(is_loopback(here), "{here}");
        }
        for elsewhere in ["https://api.openai.com/v1", "http://192.168.1.20:8000", "http://localhost.evil.com", "http://whisper.localhost", "http://localhost@evil.com", "ftp://127.0.0.1", "127.0.0.1:8000", ""] {
            assert!(!is_loopback(elsewhere), "{elsewhere}");
        }
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let mut config = Config { transcripts: true, transcribe_url: "https://api.openai.com".into(), ..Config::default() };
        assert!(write_config(&vault, &mut config, Utc::now()).is_err());
        assert!(!Path::new(&vault).join(CONFIG_FILE).exists());
    }

    /// Gate: transcripts and captions run only on a computer.
    #[test]
    fn a_phone_makes_neither() {
        assert!(refuse_on_phone(true).is_err());
        assert!(refuse_on_phone(false).is_ok());
    }

    #[test]
    fn the_address_given_becomes_the_endpoint() {
        assert_eq!(endpoint("http://127.0.0.1:8000"), "http://127.0.0.1:8000/v1/audio/transcriptions");
        assert_eq!(endpoint("http://127.0.0.1:8000/v1/"), "http://127.0.0.1:8000/v1/audio/transcriptions");
        assert_eq!(endpoint("http://127.0.0.1:8080/inference"), "http://127.0.0.1:8080/inference");
    }

    #[test]
    fn a_verbose_reply_keeps_its_times() {
        let reply = json!({
            "text": " Hôm nay họp về hợp đồng. ",
            "language": "vi",
            "duration": 230.4,
            "segments": [
                { "id": 0, "start": 0.0, "end": 4.2, "text": " Hôm nay họp" },
                { "id": 1, "start": 192.0, "end": 230.4, "text": " về hợp đồng." },
                { "id": 2, "start": 231.0, "end": 232.0, "text": "   " }
            ]
        });
        let transcript = parse_transcript(&reply).unwrap();
        assert_eq!(transcript.text, "Hôm nay họp về hợp đồng.");
        assert_eq!(transcript.segments.len(), 2);
        assert_eq!(transcript.segments[1], Segment { start: 192.0, end: 230.4, text: "về hợp đồng.".into() });
        assert_eq!(transcript.language.as_deref(), Some("vi"));

        let plain = parse_transcript(&json!({ "text": "chỉ có chữ" })).unwrap();
        assert!(plain.segments.is_empty());
        assert!(parse_transcript(&json!({ "text": "" })).is_none());
    }

    #[test]
    fn when_a_file_was_made_comes_from_the_camera_then_its_name() {
        let (at, exact) = when_made(&json!({ "shot_at": "2024-09-12 10:10:10" }), "a.jpg").unwrap();
        assert_eq!((at.to_string(), exact), ("2024-09-12 10:10:10".into(), true));
        assert_eq!(when_made(&json!({}), "IMG_20240912_153012.jpg").unwrap(), (NaiveDateTime::parse_from_str("2024-09-12 15:30:12", "%Y-%m-%d %H:%M:%S").unwrap(), true));
        assert_eq!(when_made(&json!({}), "photo_2024-09-12_22-10-33.jpg").unwrap().1, true);
        assert_eq!(when_made(&json!({}), "Screenshot 2024-09-12 at 10.10.10.png").unwrap().1, false, "the day only");
        assert!(when_made(&json!({}), "3f9a1b2c-2024-0912.jpg").is_none());
    }

    fn entry(id: &str, at: &str, kind: &str) -> MediaEntry {
        MediaEntry {
            node_id: id.into(),
            title: id.into(),
            extension: if kind == "image" { "jpg".into() } else { "m4a".into() },
            kind: kind.into(),
            at: at.into(),
            present: true,
            path: None,
            caption: None,
            caption_model: None,
            transcript: None,
        }
    }

    #[test]
    fn pictures_hours_apart_are_two_moments_and_a_day_keeps_its_own() {
        let moments = group(vec![
            entry("c", "2024-09-12 11:30", "image"),
            entry("a", "2024-09-12 09:00", "audio"),
            entry("b", "2024-09-12 10:00", "image"),
            entry("d", "2024-09-12 18:00", "image"),
            entry("e", "2024-09-12", "image"),
            entry("f", "2024-09-14", "image"),
        ]);
        let shape: Vec<(Vec<&str>, &str)> = moments
            .iter()
            .map(|m| (m.members.iter().map(|e| e.node_id.as_str()).collect(), m.cover.node_id.as_str()))
            .collect();
        assert_eq!(
            shape,
            vec![(vec!["a", "b", "c"], "b"), (vec!["e"], "e"), (vec!["d"], "d"), (vec!["f"], "f")],
            "the cover of a morning is its first picture, not the recording"
        );
    }

    fn surrogate(node: &str, kind: &str) -> Surrogate {
        Surrogate {
            node: node.into(),
            hash: hash_of(node).unwrap().into(),
            kind: kind.into(),
            version: SURROGATE_VERSION,
            model: "whisper-small".into(),
            at: "2026-09-15T00:00:00.000Z".into(),
            text: "về hợp đồng".into(),
            segments: vec![Segment { start: 192.0, end: 230.4, text: "về hợp đồng.".into() }],
            language: Some("vi".into()),
            duration: Some(230.4),
            ms: 1000,
        }
    }

    const MEMO: &str = "Files/9f3a0000.md";

    /// Same key as Nhát E: a file read on one device is not read on another.
    #[test]
    fn a_file_read_on_another_device_is_not_read_again() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        crate::timeline::extract::record_surrogate(&vault, "device-a", surrogate(MEMO, "transcript"), Utc::now()).unwrap();

        let store = TimelineStore::open_in_memory().unwrap();
        crate::timeline::extract::load(store.conn(), &vault).unwrap();
        let input = |kind: &'static str| MediaInput {
            node_id: MEMO.into(),
            hash: "9f3a0000".into(),
            kind,
            path: "/elsewhere/memo.m4a".into(),
            extension: "m4a".into(),
            size: 10,
        };
        let on_b = plan(store.conn(), vec![input("transcript")]).unwrap();
        assert_eq!((on_b.pending.len(), on_b.done), (0, 1));
        let caption = plan(store.conn(), vec![input("caption")]).unwrap();
        assert_eq!(caption.pending.len(), 1, "a caption is its own reading");

        // Refused: copying media into `Timeline/`.
        for file in walkdir::WalkDir::new(Path::new(&vault).join("Timeline")).into_iter().flatten().filter(|e| e.file_type().is_file()) {
            assert_eq!(file.path().extension().and_then(|e| e.to_str()), Some("json"), "{}", file.path().display());
        }
    }

    #[test]
    fn what_is_read_leaves_out_the_sealed_the_absent_and_the_huge() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let db = DbBridge::new_in_memory_full().unwrap();
        let file = |hash: &str, name: &str, extra: Value| {
            let mut properties = json!({ "path": format!("/vault/assets/{name}"), "extension": name.rsplit('.').next().unwrap() });
            if let (Value::Object(map), Value::Object(more)) = (&mut properties, extra) {
                map.extend(more);
            }
            NodeMetadata {
                id: format!("Files/{hash}.md"),
                node_type: "file".into(),
                title: name.into(),
                content: String::new(),
                properties,
                created_at: String::new(),
                updated_at: String::new(),
                timestamp: 0,
                blocks: None,
            }
        };
        for node in [
            file("aa", "memo.m4a", json!({})),
            file("bb", "IMG_20240912_101010.jpg", json!({})),
            file("cc", "sealed.m4a", json!({ "sealed": true })),
            file("dd", "IMG_20190210_101010.jpg", json!({})),
            file("ee", "elsewhere.m4a", json!({})),
            file("ff", "huge.m4a", json!({})),
            file("gg", "photo.heic", json!({})),
            file("hh", "notes.pdf", json!({})),
        ] {
            db.upsert_node(&node).unwrap();
        }
        crate::timeline::seal::write_period(&vault, "2019-02", "2019-02").unwrap();
        let seals = Seals::read(&db, &vault).unwrap();
        let config = Config { transcripts: true, captions: true, ..Config::default() };
        let locate = |id: &str| match id {
            "Files/ee.md" => None,
            "Files/ff.md" => Some(("/vault/assets/huge.m4a".to_string(), MAX_AUDIO_BYTES + 1)),
            other => Some((format!("/vault/{other}"), 1_000)),
        };
        let (read, too_large) = inputs(&db, &config, &seals, locate).unwrap();
        let kinds: Vec<(&str, &str)> = read.iter().map(|i| (i.node_id.as_str(), i.kind)).collect();
        assert_eq!(kinds, vec![("Files/aa.md", "transcript"), ("Files/bb.md", "caption")]);
        assert_eq!(too_large, 1);

        let only_captions = Config { captions: true, ..Config::default() };
        assert_eq!(inputs(&db, &only_captions, &seals, locate).unwrap().0.len(), 1);
    }

    /// A transcript is found by search, read by Syn, and says how to cite a moment.
    #[test]
    fn a_transcript_is_searchable_and_says_how_to_cite_a_moment() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&NodeMetadata {
            id: MEMO.into(),
            node_type: "file".into(),
            title: "memo.m4a".into(),
            content: String::new(),
            properties: json!({ "path": "/vault/assets/memo.m4a", "extension": "m4a", "size": 10, "source_type": "local" }),
            created_at: String::new(),
            updated_at: String::new(),
            timestamp: 0,
            blocks: None,
        })
        .unwrap();
        crate::timeline::extract::record_surrogate(&vault, "device-a", surrogate(MEMO, "transcript"), Utc::now()).unwrap();
        let store = TimelineStore::open_in_memory().unwrap();
        crate::timeline::extract::load(store.conn(), &vault).unwrap();

        assert_eq!(mirror_into_search(store.conn(), &db).unwrap(), 1);
        assert_eq!(mirror_into_search(store.conn(), &db).unwrap(), 0, "once");
        let text = db.file_text_joined(MEMO).unwrap();
        assert!(text.contains("[3:12] về hợp đồng. (t=192,231)"), "{text}");
        assert!(text.contains(&format!("{MEMO}#t=START,END")), "{text}");
        assert!(text.contains("may be wrong"), "a transcript is not a record: {text}");

        // Trashed and added back: its text is forgotten, and put back.
        db.forget_file_text(MEMO).unwrap();
        assert_eq!(mirror_into_search(store.conn(), &db).unwrap(), 1);
        assert!(db.file_text_joined(MEMO).unwrap().contains("về hợp đồng"));
    }

    fn source_files(root: &Path) -> Vec<std::path::PathBuf> {
        walkdir::WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !matches!(e.file_name().to_str(), Some("node_modules" | "target" | ".git" | "build" | "dist")))
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                matches!(
                    e.path().extension().and_then(|x| x.to_str()),
                    Some("rs" | "ts" | "vue" | "js" | "kt" | "java" | "swift" | "toml" | "json")
                )
            })
            .map(|e| e.into_path())
            .collect()
    }

    /// Gate: no call anywhere detects or recognises a face (§8.2).
    #[test]
    fn nothing_in_the_app_detects_or_recognises_a_face() {
        // Written in halves so that this file does not match itself.
        let calls: Vec<String> = [
            ["vndetect", "face"], ["vnface", "observation"], ["cidetectortype", "face"], ["face", "detector"],
            ["face_", "recognition"], ["face-", "api"], ["face", "mesh"], ["detect", "faces"], ["detect_", "faces"],
            ["vision.", "face"], ["face_", "landmarks"], ["face", "landmarks"], ["insight", "face"],
        ]
        .iter()
        .map(|halves| halves.concat())
        .collect();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let mut found = Vec::new();
        for dir in [root.join("src"), root.join("src-tauri/src"), root.join("src-tauri/gen/android/app/src")] {
            for path in source_files(&dir) {
                let text = std::fs::read_to_string(&path).unwrap_or_default().to_lowercase();
                for call in &calls {
                    if text.contains(call.as_str()) {
                        found.push(format!("{} contains {call}", path.display()));
                    }
                }
            }
        }
        let manifests = std::fs::read_to_string(root.join("package.json")).unwrap_or_default().to_lowercase()
            + &std::fs::read_to_string(root.join("src-tauri/Cargo.toml")).unwrap_or_default().to_lowercase();
        found.extend(calls.iter().filter(|call| manifests.contains(call.as_str())).map(|call| format!("a manifest names {call}")));
        assert!(found.is_empty(), "{found:#?}");
    }

    /// The Rust crates that reach the Android build, as of the last size review.
    const REVIEWED_CRATES: &[&str] = &[
        "ammonia", "argon2", "async-trait", "base64", "bip39", "blake3", "chacha20poly1305", "chrono", "chrono-tz",
        "ctor", "dirs", "ed25519-dalek", "feed-rs", "futures", "gray_matter", "hex", "iana-time-zone", "infer",
        "iroh", "jni", "keyring", "log", "loro", "lopdf", "lz4_flex", "md-5", "ndk-context", "notify", "opener",
        "opml", "postcard", "pulldown-cmark", "rand", "regex", "reqwest", "rusqlite", "rustls", "scraper",
        "serde", "serde_json", "serde_yaml", "sha2", "similar", "synabit_protocol", "sysinfo", "tauri",
        "tauri-plugin-deep-link", "tauri-plugin-dialog", "tauri-plugin-fs", "tauri-plugin-log",
        "tauri-plugin-notification", "tauri-plugin-opener", "tauri-plugin-os", "tauri-plugin-process",
        "tauri-plugin-store", "tauri-plugin-updater", "thiserror", "time", "tokio", "url", "urlencoding",
        "uuid", "walkdir", "zeroize", "zip",
    ];

    /// The npm packages bundled into every build, Android's included.
    const REVIEWED_PACKAGES: &[&str] = &[
        "@tailwindcss/typography", "@tailwindcss/vite", "@tauri-apps/api", "@tauri-apps/plugin-autostart",
        "@tauri-apps/plugin-deep-link", "@tauri-apps/plugin-dialog", "@tauri-apps/plugin-fs", "@tauri-apps/plugin-log",
        "@tauri-apps/plugin-notification", "@tauri-apps/plugin-opener", "@tauri-apps/plugin-os",
        "@tauri-apps/plugin-process", "@tauri-apps/plugin-store", "@tauri-apps/plugin-updater",
        "@tiptap/extension-blockquote", "@tiptap/extension-code-block-lowlight", "@tiptap/extension-color",
        "@tiptap/extension-highlight", "@tiptap/extension-image", "@tiptap/extension-link",
        "@tiptap/extension-placeholder", "@tiptap/extension-table", "@tiptap/extension-table-cell",
        "@tiptap/extension-table-header", "@tiptap/extension-table-row", "@tiptap/extension-task-item",
        "@tiptap/extension-task-list", "@tiptap/extension-text-align", "@tiptap/extension-text-style",
        "@tiptap/extension-underline", "@tiptap/pm", "@tiptap/starter-kit", "@tiptap/suggestion", "@tiptap/vue-3",
        "@vue-flow/background", "@vue-flow/controls", "@vue-flow/core", "@vue-flow/node-resizer", "@vueuse/core",
        "d3", "dompurify", "highlight.js", "html-to-image", "html2pdf.js", "katex", "leaflet", "lowlight",
        "lucide-vue-next", "marked", "marked-highlight", "markmap-common", "markmap-lib", "markmap-toolbar",
        "markmap-view", "mermaid", "pdfjs-dist", "perfect-freehand", "pinia", "tailwindcss", "tippy.js",
        "tiptap-markdown", "vue", "vue-i18n", "vue-router",
    ];

    /// A manifest line with its comment taken off, `#` inside a string kept.
    fn without_comment(line: &str) -> &str {
        let mut quoted = false;
        for (i, c) in line.char_indices() {
            match c {
                '"' => quoted = !quoted,
                '#' if !quoted => return &line[..i],
                _ => {}
            }
        }
        line
    }

    /// The crates a manifest names in any section that can reach an Android
    /// build: `[dependencies]`, `[dependencies.foo]`, and every target section
    /// but those that name only desktop systems.
    fn android_crates(manifest: &str) -> Vec<String> {
        let reaches_android = |spec: &str| {
            let spec = spec.to_lowercase();
            ["android", "unix", "mobile", "not("].iter().any(|word| spec.contains(word))
                || !["windows", "macos", "linux", "ios"].iter().any(|word| spec.contains(word))
        };
        // `None`: not a dependency section. `Some(None)`: a table of them.
        // `Some(Some(name))`: one crate's own table.
        let section_of = |header: &str| -> Option<Option<String>> {
            let tail = if let Some(rest) = header.strip_prefix("target.") {
                let at = rest.rfind(".dependencies")?;
                if !reaches_android(&rest[..at]) {
                    return None;
                }
                &rest[at + 1..]
            } else {
                header
            };
            match tail.strip_prefix("dependencies") {
                Some("") => Some(None),
                Some(name) => name.strip_prefix('.').map(|n| Some(n.trim_matches('"').to_string())),
                None => None,
            }
        };

        let mut names = Vec::new();
        let mut section: Option<Option<String>> = None;
        let mut depth = 0i32;
        for raw in manifest.lines() {
            let line = without_comment(raw).trim();
            if depth == 0 && line.starts_with('[') {
                section = section_of(line.trim_start_matches('[').trim_end_matches(']').trim());
                if let Some(Some(name)) = &section {
                    names.push(name.clone());
                }
                continue;
            }
            if depth == 0 && matches!(section, Some(None)) {
                if let Some((name, _)) = line.split_once('=') {
                    let name = name.trim().trim_matches('"');
                    if !name.is_empty() {
                        names.push(name.to_string());
                    }
                }
            }
            depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        }
        names.sort();
        names.dedup();
        names
    }

    /// Gate: nothing reaches the Android build without a size review
    /// (`docs/android-google-play-readiness-audit-2026-07-22.md`, §4.6 of the
    /// Tua lại design). A failure here is the review being asked for, not a
    /// test to update on the way past.
    #[test]
    fn nothing_reaches_the_android_build_without_a_size_review() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let manifest = std::fs::read_to_string(root.join("src-tauri/Cargo.toml")).unwrap();
        let crates = android_crates(&manifest);
        let mut reviewed: Vec<String> = REVIEWED_CRATES.iter().map(|c| c.to_string()).collect();
        reviewed.sort();
        assert_eq!(crates, reviewed, "a crate joined or left the Android build");

        let package: Value = serde_json::from_str(&std::fs::read_to_string(root.join("package.json")).unwrap()).unwrap();
        let mut packages: Vec<String> = package["dependencies"].as_object().unwrap().keys().cloned().collect();
        packages.sort();
        let mut reviewed: Vec<String> = REVIEWED_PACKAGES.iter().map(|p| p.to_string()).collect();
        reviewed.sort();
        assert_eq!(packages, reviewed, "an npm package joined or left the bundle");
    }

    #[test]
    fn every_way_a_crate_can_reach_android_is_read() {
        let manifest = r#"
[dependencies]
serde = { version = "1",
  features = ["derive"] }
# not = "a crate"
foo = "1" # trailing

[dependencies.bar]
version = "2"

[target.'cfg(unix)'.dependencies]
baz = "1"

[target.'cfg(any(target_os = "windows", target_os = "linux"))'.dependencies]
desk = "1"

[target.'cfg(target_os = "android")'.dependencies.qux]
version = "1"

[dev-dependencies]
tempfile = "3"

[build-dependencies]
tauri-build = "2"
"#;
        assert_eq!(android_crates(manifest), vec!["bar", "baz", "foo", "qux", "serde"]);
    }

    #[test]
    fn ollama_elsewhere_is_not_this_machine() {
        let mut settings = crate::models::syn::SynSettings::default();
        settings.provider = crate::models::syn::SynProvider::Ollama;
        settings.ollama_url = "http://localhost:11434".into();
        assert!(runs_here(&settings));
        settings.ollama_url = "http://gpu-box.lan:11434".into();
        assert!(!runs_here(&settings));
    }

    #[test]
    fn a_camera_date_places_a_picture_whose_name_has_none() {
        let properties = json!({ "path": "/vault/assets/IMG_1234.JPG", "shot_at": "2024-09-12 10:10:10" });
        let derived = crate::timeline::derive::derive(
            &crate::timeline::derive::NodeView { id: "Files/ab.md", node_type: "file", title: "IMG_1234.JPG", properties: &properties },
            &std::collections::HashMap::new(),
        );
        assert_eq!(derived.len(), 1);
        assert_eq!((derived[0].kind, derived[0].time_source), ("media", "exif"));
        assert_eq!(crate::timeline::when::iso(derived[0].span.from), "2024-09-12");
    }

    #[test]
    fn a_caption_is_tidied_and_the_prompt_asks_for_no_identities() {
        assert_eq!(tidy_caption("  \"Hai người   ngồi bên hồ lúc chiều.\" "), Some("Hai người ngồi bên hồ lúc chiều.".into()));
        assert_eq!(tidy_caption("   "), None);
        let prompt = caption_prompt("Vietnamese");
        assert!(prompt.contains("Do not say who anyone is"));
        assert!(prompt.contains("Vietnamese"));
    }
}
