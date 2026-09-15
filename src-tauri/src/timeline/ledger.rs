//! Sổ bằng chứng: when each file was first recorded, what it held, and every
//! time that changed.
//!
//! The design is §7.3 of `docs/tua-lai-2026-09-14.md`, and this is Nhát C.
//!
//! # What it is for
//!
//! A memory may be corrected; evidence may not. Both live in the same files,
//! and the files stay the person's to edit, so this blocks nothing. It notices.
//! A file's content is fingerprinted when it is first seen and again every
//! time it changes, into a record that shows if the record itself was altered.
//!
//! # The record
//!
//! `Timeline/ledger/<device>/<YYYY-MM>.json`: one chain per device, entries
//! appended in order. Each entry carries the fingerprint of the entry before
//! it and a fingerprint of itself, so changing an entry, or removing one,
//! breaks every entry after it. One writer per file, as with
//! `Feeds/state/<device>.json`: two devices never write the same ledger, so
//! sync has nothing to merge.
//!
//! # What it can show, and what it cannot
//!
//! It shows that a record was altered after it was written, and from which
//! entry. It cannot show that a whole chain was rewritten consistently from
//! its first entry by someone holding the vault; that takes the chain's
//! fingerprint kept somewhere outside the vault, such as an RFC 3161
//! timestamp (§7.3), which is not done here.
//!
//! # What is fingerprinted
//!
//! A Markdown file's body, not its frontmatter. The app rewrites frontmatter
//! on its own account (an identity on first sync, `updated_at` on every save,
//! `pinned`, `sealed`, tags), and none of that changes what was written.
//! Every other file is taken whole, bytes as they are.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

pub const LEDGER_DIR: &str = "Timeline/ledger";
const FORMAT: u32 = 1;

/// The app's own bookkeeping, which is not something a person kept.
fn is_bookkeeping(rel: &str) -> bool {
    super::is_timeline_path(rel) || rel.starts_with("Syn/") || rel.starts_with("Feeds/state/")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerEntry {
    /// Position in this device's chain, from 0, across every month.
    pub seq: u64,
    /// When this device recorded it, RFC 3339 UTC.
    pub at: String,
    pub path: String,
    /// `recorded` the first time this device saw the file, `changed` when its
    /// content differs from what was last recorded, `removed` when it is gone.
    pub change: String,
    /// The content's fingerprint; absent for `removed`.
    pub hash: Option<String>,
    /// Bytes fingerprinted: the body for Markdown, the file otherwise.
    pub size: Option<u64>,
    /// The fingerprint of the entry before this one.
    pub prev: String,
    /// The fingerprint of this entry, `prev` included.
    pub entry: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LedgerFile {
    format: u32,
    device: String,
    month: String,
    entries: Vec<LedgerEntry>,
    /// Anything else in the file, kept as it was. Sync adds a
    /// `metadata.node_id` to a JSON document, and that is not an alteration.
    #[serde(flatten)]
    rest: Map<String, Value>,
}

fn genesis(device: &str) -> String {
    format!("genesis:{device}")
}

fn entry_hash(e: &LedgerEntry) -> String {
    let seq = e.seq.to_string();
    let size = e.size.map(|s| s.to_string()).unwrap_or_default();
    let mut hasher = blake3::Hasher::new();
    for part in [
        e.prev.as_str(),
        seq.as_str(),
        e.at.as_str(),
        e.path.as_str(),
        e.change.as_str(),
        e.hash.as_deref().unwrap_or(""),
        size.as_str(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    format!("blake3:{}", hasher.finalize().to_hex())
}

fn io(e: std::io::Error) -> AppError {
    AppError::Io(e)
}

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("ledger: {e}"))
}

/// A Markdown file's body: what follows the closing fence, or all of it when
/// there is no frontmatter.
fn body_of(bytes: &[u8]) -> &[u8] {
    let rest = if let Some(rest) = bytes.strip_prefix(b"---\n") {
        rest
    } else if let Some(rest) = bytes.strip_prefix(b"---\r\n") {
        rest
    } else {
        return bytes;
    };
    let mut offset = 0;
    for line in rest.split_inclusive(|b| *b == b'\n') {
        offset += line.len();
        let bare = line.strip_suffix(b"\n").unwrap_or(line);
        let bare = bare.strip_suffix(b"\r").unwrap_or(bare);
        if bare == b"---" {
            return &rest[offset..];
        }
    }
    bytes
}

/// A file's content fingerprint and the number of bytes it covers.
pub fn fingerprint(path: &Path) -> std::io::Result<(String, u64)> {
    let markdown = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md"));
    let mut hasher = blake3::Hasher::new();
    let size = if markdown {
        let bytes = std::fs::read(path)?;
        let body = body_of(&bytes);
        hasher.update(body);
        body.len() as u64
    } else {
        let mut file = std::fs::File::open(path)?;
        let mut buffer = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            total += n as u64;
        }
        total
    };
    Ok((format!("blake3:{}", hasher.finalize().to_hex()), size))
}

fn ledger_root(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(LEDGER_DIR)
}

/// A device's ledger files, oldest month first. A file that cannot be read is
/// kept as its error, because an unreadable ledger is itself something to say.
fn read_chain(dir: &Path) -> Vec<(String, Result<LedgerFile, String>)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, Result<LedgerFile, String>)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".json") {
                return None;
            }
            let parsed = std::fs::read_to_string(entry.path())
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str::<LedgerFile>(&text).map_err(|e| e.to_string()));
            Some((name, parsed))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// The ledger's own table in `timeline.db`: what each file looked like the
/// last time it was fingerprinted, so an unchanged file is not read again.
pub fn ensure_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ledger_stat (
            vault     TEXT NOT NULL,
            path      TEXT NOT NULL,
            size      INTEGER NOT NULL,
            modified  INTEGER NOT NULL,
            hash      TEXT NOT NULL,
            covered   INTEGER NOT NULL,
            PRIMARY KEY (vault, path)
         );
         -- Where each device's chain ended, last time this device looked. A
         -- chain shorter than that has lost its end.
         CREATE TABLE IF NOT EXISTS ledger_tip (
            vault  TEXT NOT NULL,
            device TEXT NOT NULL,
            seq    INTEGER NOT NULL,
            entry  TEXT NOT NULL,
            PRIMARY KEY (vault, device)
         );",
    )
    .map_err(sql)
}

#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct SweepReport {
    pub recorded: usize,
    pub changed: usize,
    pub removed: usize,
    /// Files read and fingerprinted this time, as opposed to known unchanged.
    pub hashed: usize,
}

/// Record every file this device has not yet recorded, or whose content has
/// changed since it did, and every file that has gone.
pub fn sweep(conn: &Connection, vault_path: &str, device: &str, now: DateTime<Utc>) -> AppResult<SweepReport> {
    ensure_schema(conn)?;
    let dir = ledger_root(vault_path).join(device);

    // What this device last recorded of each path, and where its chain ends.
    let mut last: HashMap<String, Option<String>> = HashMap::new();
    let mut tail: Option<(u64, String)> = None;
    for (_, file) in read_chain(&dir) {
        if let Ok(file) = file {
            for e in file.entries {
                tail = Some((e.seq, e.entry.clone()));
                last.insert(e.path, e.hash);
            }
        }
    }
    let mut next_seq = tail.as_ref().map(|(seq, _)| seq + 1).unwrap_or(0);
    let mut prev = tail.map(|(_, entry)| entry).unwrap_or_else(|| genesis(device));
    let at = now.to_rfc3339_opts(SecondsFormat::Millis, true);

    let mut report = SweepReport::default();
    let mut added: Vec<LedgerEntry> = Vec::new();
    let mut push = |path: String, change: &str, hash: Option<String>, size: Option<u64>| {
        let mut entry = LedgerEntry {
            seq: next_seq,
            at: at.clone(),
            path,
            change: change.to_string(),
            hash,
            size,
            prev: prev.clone(),
            entry: String::new(),
        };
        entry.entry = entry_hash(&entry);
        prev = entry.entry.clone();
        next_seq += 1;
        added.push(entry);
    };

    let mut files = crate::sync::utils::collect_local_files(vault_path);
    files.sort();
    let mut present: HashSet<String> = HashSet::with_capacity(files.len());

    for rel in files {
        if is_bookkeeping(&rel) {
            continue;
        }
        // Here, whether or not it can be read this moment. A file another
        // program holds for a second has not been removed, and the ledger
        // only ever adds: a false removal could never be taken back.
        present.insert(rel.clone());
        let abs = Path::new(vault_path).join(&rel);
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue;
        };
        let size = meta.len() as i64;
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);

        let cached: Option<(i64, i64, String, i64)> = conn
            .query_row(
                "SELECT size, modified, hash, covered FROM ledger_stat WHERE vault = ?1 AND path = ?2",
                params![vault_path, rel],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(sql)?;
        let (hash, covered) = match cached {
            Some((s, m, hash, covered)) if s == size && m == modified => (hash, covered as u64),
            _ => {
                let Ok((hash, covered)) = fingerprint(&abs) else {
                    continue;
                };
                report.hashed += 1;
                conn.execute(
                    "INSERT INTO ledger_stat (vault, path, size, modified, hash, covered)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(vault, path) DO UPDATE SET
                        size = excluded.size, modified = excluded.modified,
                        hash = excluded.hash, covered = excluded.covered",
                    params![vault_path, rel, size, modified, hash, covered as i64],
                )
                .map_err(sql)?;
                (hash, covered)
            }
        };

        match last.get(&rel) {
            None => {
                report.recorded += 1;
                push(rel, "recorded", Some(hash), Some(covered));
            }
            Some(Some(known)) if *known == hash => {}
            Some(_) => {
                report.changed += 1;
                push(rel, "changed", Some(hash), Some(covered));
            }
        }
    }

    let mut gone: Vec<String> = last
        .iter()
        .filter(|(path, hash)| hash.is_some() && !present.contains(path.as_str()))
        .map(|(path, _)| path.clone())
        .collect();
    gone.sort();
    for path in gone {
        report.removed += 1;
        push(path, "removed", None, None);
    }

    if added.is_empty() {
        return Ok(report);
    }

    let month = now.format("%Y-%m").to_string();
    std::fs::create_dir_all(&dir).map_err(io)?;
    let path = dir.join(format!("{month}.json"));
    let mut file = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<LedgerFile>(&text).map_err(|e| {
            // Writing over an unreadable ledger would destroy the very thing
            // that shows it was tampered with.
            AppError::General(format!("{} cannot be read, so nothing was added to it: {e}", path.display()))
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LedgerFile {
            format: FORMAT,
            device: device.to_string(),
            month: month.clone(),
            entries: Vec::new(),
            rest: Map::new(),
        },
        Err(e) => return Err(io(e)),
    };
    file.entries.extend(added);

    // Sync settles two copies of a JSON document by this stamp. Only this
    // device writes this file, so it only ever moves forward.
    let metadata = file
        .rest
        .entry("metadata")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(metadata) = metadata.as_object_mut() {
        metadata.insert("updated_at".into(), Value::String(at));
    }

    let temp = dir.join(format!(".{month}.json.tmp"));
    std::fs::write(&temp, serde_json::to_string_pretty(&file)?).map_err(io)?;
    std::fs::rename(&temp, &path).map_err(io)?;
    Ok(report)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChainReport {
    pub device: String,
    pub entries: usize,
    pub intact: bool,
    /// The first entry that does not follow from those before it. Nothing
    /// from here on in this chain can be relied on.
    pub broken_at: Option<u64>,
    pub reason: Option<String>,
}

/// Check every device's chain.
pub fn verify(vault_path: &str) -> Vec<ChainReport> {
    verify_with(None, vault_path)
}

/// Check every device's chain, and, given this device's connection, that no
/// chain ends before where it ended last time: an entry cut off the end
/// leaves every remaining link intact, so only a memory of the end shows it.
pub fn verify_with(conn: Option<&Connection>, vault_path: &str) -> Vec<ChainReport> {
    if let Some(conn) = conn {
        if ensure_schema(conn).is_err() {
            return verify_with(None, vault_path);
        }
    }
    let Ok(devices) = std::fs::read_dir(ledger_root(vault_path)) else {
        return Vec::new();
    };
    let mut devices: Vec<(String, PathBuf)> = devices
        .flatten()
        .filter(|d| d.path().is_dir())
        .map(|d| (d.file_name().to_string_lossy().to_string(), d.path()))
        .filter(|(name, _)| !name.starts_with('.'))
        .collect();
    devices.sort();

    devices
        .into_iter()
        .map(|(device, dir)| {
            let mut expected = 0u64;
            let mut prev = genesis(&device);
            let mut count = 0usize;
            let mut broken: Option<(u64, String)> = None;
            let mut entries: HashMap<u64, String> = HashMap::new();
            let mut end: Option<(u64, String)> = None;

            for (name, file) in read_chain(&dir) {
                let file = match file {
                    Ok(file) => file,
                    Err(e) => {
                        broken.get_or_insert((expected, format!("{name} cannot be read: {e}")));
                        continue;
                    }
                };
                for e in file.entries {
                    count += 1;
                    if broken.is_some() {
                        continue;
                    }
                    if e.seq != expected {
                        broken = Some((expected.min(e.seq), format!("entry {expected} is missing")));
                    } else if e.prev != prev {
                        broken = Some((e.seq, format!("entry {} does not follow the one before it", e.seq)));
                    } else if entry_hash(&e) != e.entry {
                        broken = Some((e.seq, format!("entry {} was altered after it was written", e.seq)));
                    }
                    entries.insert(e.seq, e.entry.clone());
                    end = Some((e.seq, e.entry.clone()));
                    prev = e.entry.clone();
                    expected = e.seq + 1;
                }
            }

            if let Some(conn) = conn {
                let seen: Option<(u64, String)> = conn
                    .query_row(
                        "SELECT seq, entry FROM ledger_tip WHERE vault = ?1 AND device = ?2",
                        params![vault_path, device],
                        |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?)),
                    )
                    .optional()
                    .ok()
                    .flatten();
                if broken.is_none() {
                    if let Some((seq, entry)) = &seen {
                        match entries.get(seq) {
                            None => broken = Some((*seq, format!("the chain ends before entry {seq}, which this device has seen"))),
                            Some(found) if found != entry => {
                                broken = Some((*seq, format!("entry {seq} is not the one this device saw")))
                            }
                            _ => {}
                        }
                    }
                }
                if let (None, Some((seq, entry))) = (&broken, &end) {
                    if seen.as_ref().is_none_or(|(known, _)| seq > known) {
                        let _ = conn.execute(
                            "INSERT INTO ledger_tip (vault, device, seq, entry) VALUES (?1, ?2, ?3, ?4)
                             ON CONFLICT(vault, device) DO UPDATE SET seq = excluded.seq, entry = excluded.entry",
                            params![vault_path, device, *seq as i64, entry],
                        );
                    }
                }
            }

            ChainReport {
                device,
                entries: count,
                intact: broken.is_none(),
                broken_at: broken.as_ref().map(|(seq, _)| *seq),
                reason: broken.map(|(_, reason)| reason),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileHistory {
    pub path: String,
    /// Whether any device has recorded this file.
    pub recorded: bool,
    pub first_recorded_at: Option<String>,
    pub first_hash: Option<String>,
    pub current_hash: Option<String>,
    /// How many times the content has changed since it was first recorded,
    /// counting a change the ledger has not looked at yet.
    pub times_changed: usize,
    /// When the last recorded change was recorded. A change not yet recorded
    /// has no date anybody can vouch for.
    pub last_changed_at: Option<String>,
    /// The content on disk differs from the last record of it.
    pub changed_since_record: bool,
    /// Every chain these records come from is intact up to them.
    pub trusted: bool,
}

/// One ledger record of a file: when, which device, its place in that
/// device's chain, the content's fingerprint, and whether its chain holds.
type Record = (String, String, u64, Option<String>, bool);

/// What every device's ledger says about one file.
pub fn history(vault_path: &str, rel_path: &str) -> FileHistory {
    history_with(None, vault_path, rel_path)
}

/// The same, checking chain ends against what this device has seen.
pub fn history_with(conn: Option<&Connection>, vault_path: &str, rel_path: &str) -> FileHistory {
    let broken: HashMap<String, Option<u64>> = verify_with(conn, vault_path)
        .into_iter()
        .map(|report| (report.device, report.broken_at))
        .collect();

    let mut records: Vec<(String, String, u64, Option<String>, bool)> = Vec::new();
    for (device, broken_at) in &broken {
        for (_, file) in read_chain(&ledger_root(vault_path).join(device)) {
            let Ok(file) = file else { continue };
            for e in file.entries.into_iter().filter(|e| e.path == rel_path) {
                let trusted = broken_at.is_none_or(|b| e.seq < b);
                records.push((e.at, device.clone(), e.seq, e.hash, trusted));
            }
        }
    }
    records.sort();

    // Two devices recording the same content are one state, not two.
    let mut states: Vec<(String, Option<String>)> = Vec::new();
    for (at, _, _, hash, _) in &records {
        if states.last().map(|(_, h)| h) != Some(hash) {
            states.push((at.clone(), hash.clone()));
        }
    }

    // Changes are counted along each device's own chain, as moves from one
    // content to another. Two devices that recorded the same edit recorded one
    // edit, whatever order their clocks put them in; a file removed and put
    // back unchanged was not edited.
    let mut transitions: HashSet<(String, String)> = HashSet::new();
    let mut last_changed_at: Option<String> = None;
    let mut by_device: HashMap<&str, Vec<&Record>> = HashMap::new();
    for record in &records {
        by_device.entry(record.1.as_str()).or_default().push(record);
    }
    for chain in by_device.values_mut() {
        chain.sort_by_key(|record| record.2);
        let mut before: Option<&String> = None;
        for (at, _, _, hash, _) in chain.iter().copied() {
            let Some(hash) = hash else { continue };
            if let Some(before) = before.filter(|before| *before != hash) {
                transitions.insert((before.clone(), hash.clone()));
                if last_changed_at.as_ref().is_none_or(|last| at > last) {
                    last_changed_at = Some(at.clone());
                }
            }
            before = Some(hash);
        }
    }

    let abs = Path::new(vault_path).join(rel_path);
    let current_hash = abs.is_file().then(|| fingerprint(&abs).ok().map(|(h, _)| h)).flatten();
    let changed_since_record = !states.is_empty()
        && states.last().map(|(_, h)| h.as_ref()) != Some(current_hash.as_ref());

    FileHistory {
        path: rel_path.to_string(),
        recorded: !records.is_empty(),
        first_recorded_at: records.first().map(|r| r.0.clone()),
        first_hash: records.first().and_then(|r| r.3.clone()),
        current_hash,
        times_changed: transitions.len() + usize::from(changed_since_record),
        last_changed_at,
        changed_since_record,
        // Records before a break were checked and still hold. But a file with
        // no records left while some chain is broken may have lost them to the
        // break: a record deleted leaves nothing behind to be untrusted.
        trusted: records.iter().all(|r| r.4) && (!records.is_empty() || broken.values().all(Option::is_none)),
    }
}

/// The ledger's own connection to `timeline.db`, apart from the timeline's.
///
/// A first sweep fingerprints every file in the vault, which on a vault full
/// of pictures takes minutes. On the timeline's connection it held the lock
/// every timeline read waits on, a question to Syn that named a time among
/// them.
pub struct LedgerDb(pub std::sync::Mutex<Connection>);

impl LedgerDb {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path).map_err(sql)?;
        conn.busy_timeout(std::time::Duration::from_secs(10)).map_err(sql)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
        ensure_schema(&conn)?;
        Ok(LedgerDb(std::sync::Mutex::new(conn)))
    }

    pub fn in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("an in-memory ledger connection");
        ensure_schema(&conn).expect("the ledger schema");
        LedgerDb(std::sync::Mutex::new(conn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_cut_short_at_its_end_is_caught() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("Notes")).unwrap();
        let vault_path = vault.canonicalize().unwrap().to_string_lossy().to_string();
        let conn = Connection::open_in_memory().unwrap();
        use chrono::TimeZone;
        let now = |minute: u32| chrono::Utc.with_ymd_and_hms(2026, 9, 15, 10, minute, 0).unwrap();

        std::fs::write(vault.join("Notes/a.md"), "one").unwrap();
        sweep(&conn, &vault_path, "dev", now(0)).unwrap();
        std::fs::write(vault.join("Notes/b.md"), "two").unwrap();
        sweep(&conn, &vault_path, "dev", now(1)).unwrap();
        assert!(verify_with(Some(&conn), &vault_path).iter().all(|c| c.intact));

        let month = vault.join(LEDGER_DIR).join("dev/2026-09.json");
        let mut file: LedgerFile = serde_json::from_str(&std::fs::read_to_string(&month).unwrap()).unwrap();
        file.entries.pop();
        std::fs::write(&month, serde_json::to_string(&file).unwrap()).unwrap();

        assert!(verify(&vault_path).iter().all(|c| c.intact), "without a memory of the end it looks whole");
        let chains = verify_with(Some(&conn), &vault_path);
        assert!(!chains[0].intact, "{chains:?}");
        let lost = history_with(Some(&conn), &vault_path, "Notes/b.md");
        assert!(!lost.recorded && !lost.trusted, "the record cut off was b's: {lost:?}");
        assert!(history_with(Some(&conn), &vault_path, "Notes/a.md").trusted, "a's record is before the cut");
    }

    #[test]
    fn one_edit_seen_by_two_devices_in_any_order_is_one_change() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("Notes")).unwrap();
        let vault_path = vault.canonicalize().unwrap().to_string_lossy().to_string();
        use chrono::TimeZone;
        let now = |minute: u32| chrono::Utc.with_ymd_and_hms(2026, 9, 15, 10, minute, 0).unwrap();
        let note = vault.join("Notes/a.md");
        let (a, b) = (Connection::open_in_memory().unwrap(), Connection::open_in_memory().unwrap());

        std::fs::write(&note, "first").unwrap();
        sweep(&a, &vault_path, "dev-a", now(0)).unwrap();
        std::fs::write(&note, "second, longer").unwrap();
        sweep(&a, &vault_path, "dev-a", now(1)).unwrap();
        // Device B had not synced the edit yet when it first looked.
        std::fs::write(&note, "first").unwrap();
        sweep(&b, &vault_path, "dev-b", now(2)).unwrap();
        std::fs::write(&note, "second, longer").unwrap();
        sweep(&b, &vault_path, "dev-b", now(3)).unwrap();

        let history = history(&vault_path, "Notes/a.md");
        assert_eq!(history.times_changed, 1, "{history:?}");
        assert!(!history.changed_since_record);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_read_this_moment_is_not_removed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("Notes")).unwrap();
        let vault_path = vault.canonicalize().unwrap().to_string_lossy().to_string();
        let conn = Connection::open_in_memory().unwrap();
        let note = vault.join("Notes/a.md");
        std::fs::write(&note, "kept").unwrap();
        sweep(&conn, &vault_path, "dev", chrono::Utc::now()).unwrap();

        std::fs::write(&note, "kept, and changed").unwrap();
        std::fs::set_permissions(&note, std::fs::Permissions::from_mode(0o000)).unwrap();
        let report = sweep(&conn, &vault_path, "dev", chrono::Utc::now()).unwrap();
        std::fs::set_permissions(&note, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(report.removed, 0, "{report:?}");
    }

    fn conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    /// A vault under a directory whose name does not start with a dot:
    /// `tempfile` names start with one, and the vault walk skips dotted names.
    fn vault() -> (tempfile::TempDir, String) {
        let holder = tempfile::tempdir().unwrap();
        let root = holder.path().join("vault");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.to_string_lossy().to_string();
        (holder, path)
    }

    fn write(vault: &str, rel: &str, text: &str) {
        let path = Path::new(vault).join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn at(stamp: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(stamp).unwrap().with_timezone(&Utc)
    }

    fn ledger_file(vault: &str, device: &str, month: &str) -> PathBuf {
        ledger_root(vault).join(device).join(format!("{month}.json"))
    }

    #[test]
    fn a_file_is_recorded_once_and_each_change_to_what_was_written_once() {
        let (_holder, vault) = vault();
        let db = conn();
        write(&vault, "Notes/a.md", "---\ntitle: A\n---\nfirst\n");

        let first = sweep(&db, &vault, "mac", at("2026-09-15T08:00:00Z")).unwrap();
        assert_eq!((first.recorded, first.changed), (1, 0));

        let again = sweep(&db, &vault, "mac", at("2026-09-15T08:01:00Z")).unwrap();
        assert_eq!(again, SweepReport::default(), "nothing changed, nothing read, nothing written");

        // Pinning rewrites the frontmatter. It is not an edit.
        write(&vault, "Notes/a.md", "---\ntitle: A\npinned: true\n---\nfirst\n");
        let pinned = sweep(&db, &vault, "mac", at("2026-09-15T08:02:00Z")).unwrap();
        assert_eq!((pinned.hashed, pinned.changed), (1, 0));

        write(&vault, "Notes/a.md", "---\ntitle: A\npinned: true\n---\nfirst, then changed\n");
        let edited = sweep(&db, &vault, "mac", at("2026-09-15T08:03:00Z")).unwrap();
        assert_eq!(edited.changed, 1);

        let history = history(&vault, "Notes/a.md");
        assert!(history.recorded && history.trusted);
        assert_eq!(history.times_changed, 1);
        assert_eq!(history.first_recorded_at.as_deref(), Some("2026-09-15T08:00:00.000Z"));
        assert_eq!(history.last_changed_at.as_deref(), Some("2026-09-15T08:03:00.000Z"));
        assert_ne!(history.first_hash, history.current_hash);
        assert!(!history.changed_since_record);
    }

    /// The first gate for Nhát C: an edit the ledger has not looked at yet is
    /// still found, by comparing the file with its last record.
    #[test]
    fn an_edit_not_yet_recorded_is_still_found() {
        let (_holder, vault) = vault();
        write(&vault, "Notes/a.md", "agreed: 10 million\n");
        sweep(&conn(), &vault, "mac", at("2026-09-15T08:00:00Z")).unwrap();

        write(&vault, "Notes/a.md", "agreed: 1 million\n");
        let history = history(&vault, "Notes/a.md");
        assert!(history.changed_since_record);
        assert_eq!(history.times_changed, 1);
        assert_eq!(history.last_changed_at, None, "no one can vouch for when");
    }

    #[test]
    fn a_file_that_goes_is_recorded_as_gone() {
        let (_holder, vault) = vault();
        let db = conn();
        write(&vault, "assets/photo.jpg", "not really a jpeg");
        sweep(&db, &vault, "mac", at("2026-09-15T08:00:00Z")).unwrap();
        std::fs::remove_file(Path::new(&vault).join("assets/photo.jpg")).unwrap();

        let report = sweep(&db, &vault, "mac", at("2026-09-15T09:00:00Z")).unwrap();
        assert_eq!(report.removed, 1);
        assert_eq!(sweep(&db, &vault, "mac", at("2026-09-15T10:00:00Z")).unwrap().removed, 0, "gone once");
    }

    #[test]
    fn the_apps_own_bookkeeping_is_not_recorded() {
        let (_holder, vault) = vault();
        write(&vault, "Timeline/2016/2016-05.mac.json", "{}");
        write(&vault, "Syn/runs/r.json", "{}");
        write(&vault, "Feeds/state/mac.json", "{}");
        let report = sweep(&conn(), &vault, "mac", at("2026-09-15T08:00:00Z")).unwrap();
        assert_eq!(report.recorded, 0);
    }

    /// The second gate for Nhát C: alter one entry, and every entry from it
    /// on is reported, whichever way it was altered.
    #[test]
    fn altering_an_entry_breaks_the_chain_from_that_entry_on() {
        let (_holder, vault) = vault();
        let db = conn();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            write(&vault, &format!("Notes/{name}.md"), name);
            sweep(&db, &vault, "mac", at(&format!("2026-09-15T08:0{i}:00Z"))).unwrap();
        }
        let path = ledger_file(&vault, "mac", "2026-09");
        let original = std::fs::read_to_string(&path).unwrap();
        assert!(verify(&vault)[0].intact);

        let tamper = |edit: &dyn Fn(&mut Vec<Value>)| {
            let mut doc: Value = serde_json::from_str(&original).unwrap();
            edit(doc["entries"].as_array_mut().unwrap());
            std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
            verify(&vault).remove(0)
        };

        // Its content changed, its own fingerprint left as it was.
        let report = tamper(&|entries| entries[1]["hash"] = Value::from("blake3:0000"));
        assert_eq!((report.intact, report.broken_at), (false, Some(1)), "{report:?}");

        // Its content changed and its fingerprint recomputed to match: the
        // next entry no longer follows from it.
        let report = tamper(&|entries| {
            entries[1]["hash"] = Value::from("blake3:0000");
            let rewritten: LedgerEntry = serde_json::from_value(entries[1].clone()).unwrap();
            entries[1]["entry"] = Value::from(entry_hash(&rewritten));
        });
        assert_eq!(report.broken_at, Some(2), "{report:?}");

        // Removed outright.
        let report = tamper(&|entries| {
            entries.remove(1);
        });
        assert_eq!(report.broken_at, Some(1), "{report:?}");

        // And the history of what that entry was about is no longer trusted.
        tamper(&|entries| entries[1]["hash"] = Value::from("blake3:0000"));
        assert!(!history(&vault, "Notes/b.md").trusted);
        assert!(history(&vault, "Notes/a.md").trusted, "entries before the break still hold");

        // The only record about a file cut off the end: nothing about it
        // remains to distrust, and every link left is whole. Only this
        // device's memory of where the chain ended can say it.
        std::fs::write(&path, &original).unwrap();
        assert!(verify_with(Some(&db), &vault)[0].intact);
        tamper(&|entries| {
            entries.remove(2);
        });
        let lost = history_with(Some(&db), &vault, "Notes/c.md");
        assert!(!lost.recorded && !lost.trusted, "{lost:?}");
    }

    /// Sync adds an identity to every JSON document it carries.
    #[test]
    fn what_sync_adds_to_a_ledger_file_is_not_an_alteration() {
        let (_holder, vault) = vault();
        let db = conn();
        write(&vault, "Notes/a.md", "a");
        sweep(&db, &vault, "mac", at("2026-09-15T08:00:00Z")).unwrap();

        let path = ledger_file(&vault, "mac", "2026-09");
        let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        doc["metadata"]["node_id"] = Value::from("uuid-from-sync");
        std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

        write(&vault, "Notes/b.md", "b");
        sweep(&db, &vault, "mac", at("2026-09-15T09:00:00Z")).unwrap();

        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after["metadata"]["node_id"], "uuid-from-sync");
        assert!(verify(&vault)[0].intact);
    }

    #[test]
    fn a_chain_runs_on_across_months() {
        let (_holder, vault) = vault();
        let db = conn();
        write(&vault, "Notes/a.md", "a");
        sweep(&db, &vault, "mac", at("2026-09-30T23:00:00Z")).unwrap();
        write(&vault, "Notes/b.md", "b");
        sweep(&db, &vault, "mac", at("2026-10-01T01:00:00Z")).unwrap();

        assert!(ledger_file(&vault, "mac", "2026-10").exists());
        let report = verify(&vault).remove(0);
        assert_eq!((report.entries, report.intact), (2, true));
    }

    #[test]
    fn a_files_history_reads_every_devices_ledger() {
        let (_holder, vault) = vault();
        write(&vault, "Notes/a.md", "what was agreed");
        sweep(&conn(), &vault, "mac", at("2026-09-15T08:00:00Z")).unwrap();
        sweep(&conn(), &vault, "phone", at("2026-09-15T09:00:00Z")).unwrap();

        let history = history(&vault, "Notes/a.md");
        assert_eq!(history.first_recorded_at.as_deref(), Some("2026-09-15T08:00:00.000Z"));
        assert_eq!(history.times_changed, 0, "two devices seeing the same content is not a change");
        assert_eq!(verify(&vault).len(), 2);
    }
}
