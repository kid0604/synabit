//! `timeline.db`: tier 3, one file on each device.
//!
//! # Why a file of its own
//!
//! `vault_cache.db` is treated as rebuildable, and it already holds things
//! that are not (§3.4 of `docs/tua-lai-2026-09-14.md`). The timeline is kept
//! apart so that losing the cache does not lose it, and losing it costs only
//! a re-read of the cache: every item in this nhát is derived, so rebuilding
//! is parsing, never a model call.
//!
//! # Catching up without hooks
//!
//! Twenty-odd call sites write nodes. Teaching each to tell the timeline would
//! be twenty chances to forget. Instead the timeline asks the cache's
//! connection how many rows it has changed since it opened
//! (`sqlite3_total_changes`), and reads the cache again only when that number
//! moved. Every write in the app goes through that one connection, so the
//! number cannot miss one. Each node's inputs are hashed, and only nodes whose
//! hash changed are derived again.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::NaiveDate;
use rusqlite::{params, Connection, Transaction};
use serde::Serialize;
use serde_json::Value;

use super::derive::{self, Derived, NodeView};
use super::when::{self, Span};
use crate::db::{DbBridge, DbState};
use crate::error::{AppError, AppResult};

pub const FILE_NAME: &str = "timeline.db";

/// Bump when [`derive`] would read an unchanged node differently, so every
/// device derives its timeline again on the next launch.
const DERIVE_VERSION: &str = "3";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimelineItem {
    pub id: String,
    pub kind: String,
    pub node_id: String,
    pub node_type: String,
    pub title: String,
    pub label: Option<String>,
    pub related_id: Option<String>,
    pub happened_from: String,
    pub happened_to: String,
    pub precision: String,
    pub time_source: String,
    pub source: String,
}

/// What a catch-up did, so a log can say it.
#[derive(Debug, Default, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct CatchUp {
    pub nodes_read: usize,
    pub nodes_derived: usize,
    pub nodes_removed: usize,
    pub items: usize,
}

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("timeline: {e}"))
}

struct SnapshotNode {
    id: String,
    node_type: String,
    title: String,
    properties_raw: String,
}

impl SnapshotNode {
    /// Parsed only when needed: a catch-up after an unrelated write reads every
    /// node, and most of them have not changed.
    fn properties(&self) -> Value {
        serde_json::from_str(&self.properties_raw).unwrap_or(Value::Null)
    }
}

/// The cache as the timeline needs it, read in one go so the cache's lock is
/// not held while the timeline writes.
pub struct Snapshot {
    changes: u64,
    nodes: Vec<SnapshotNode>,
    /// `(note id, file id)` for every embedded attachment, in a stable order.
    attachments: Vec<(String, String)>,
}

impl Snapshot {
    pub fn read(cache: &DbBridge) -> AppResult<Self> {
        let conn = cache.conn();
        let changes = conn.total_changes();

        let mut stmt = conn
            .prepare("SELECT id, node_type, title, properties FROM nodes")
            .map_err(sql)?;
        let nodes = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            })
            .map_err(sql)?
            .flatten()
            .filter(|(id, ..)| !super::is_timeline_path(id))
            .map(|(id, node_type, title, properties_raw)| SnapshotNode {
                id,
                node_type,
                title,
                properties_raw,
            })
            .collect();

        let mut stmt = conn
            .prepare(
                "SELECT s.id, t.id
                 FROM node_edges e
                 JOIN nodes s ON s.stable_id = e.source_id
                 JOIN nodes t ON t.stable_id = e.target_id
                 WHERE e.edge_type = 'attachment'
                 ORDER BY s.id, t.id",
            )
            .map_err(sql)?;
        let attachments = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(sql)?
            .flatten()
            .collect();

        Ok(Snapshot {
            changes,
            nodes,
            attachments,
        })
    }
}

pub struct TimelineStore {
    conn: Connection,
    /// The cache's change count when the timeline last matched it. `None`
    /// until the first catch-up, so a fresh launch always compares once.
    seen_changes: Option<u64>,
}

impl TimelineStore {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::init(Connection::open(path).map_err(sql)?)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory().map_err(sql)?)
    }

    /// `timeline.db` in the app's data directory, beside `vault_cache.db`.
    pub fn open_in_app_data<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AppResult<Self> {
        use tauri::Manager;
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::General(format!("Could not determine app data dir: {e}")))?;
        std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
        Self::open(&dir.join(FILE_NAME))
    }

    fn init(conn: Connection) -> AppResult<Self> {
        // The evidence ledger writes to the same file on a connection of its
        // own (`ledger::LedgerDb`); each waits for the other rather than failing.
        conn.busy_timeout(std::time::Duration::from_secs(10)).map_err(sql)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

             CREATE TABLE IF NOT EXISTS timeline_items (
                id             TEXT PRIMARY KEY,
                kind           TEXT NOT NULL,
                happened_from  TEXT NOT NULL,
                happened_to    TEXT NOT NULL,
                precision      TEXT NOT NULL,
                time_source    TEXT,
                recorded_at    TEXT,
                node_id        TEXT,
                node_type      TEXT,
                title          TEXT NOT NULL DEFAULT '',
                label          TEXT,
                related_id     TEXT,
                source         TEXT NOT NULL,
                confidence     REAL,
                evidence       TEXT,
                month_file     TEXT,
                superseded_by  TEXT,
                sealed         INTEGER NOT NULL DEFAULT 0
             );
             CREATE INDEX IF NOT EXISTS idx_timeline_range ON timeline_items(happened_from, happened_to);
             CREATE INDEX IF NOT EXISTS idx_timeline_node  ON timeline_items(node_id);
             CREATE INDEX IF NOT EXISTS idx_timeline_rel   ON timeline_items(related_id);

             CREATE TABLE IF NOT EXISTS node_sources (
                node_id    TEXT PRIMARY KEY,
                signature  TEXT NOT NULL
             );

             CREATE TABLE IF NOT EXISTS month_files (
                path          TEXT PRIMARY KEY,
                content_hash  TEXT NOT NULL,
                loaded_at     TEXT NOT NULL
             );",
        )
        .map_err(sql)?;

        let version: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'derive_version'", [], |r| r.get(0))
            .ok();
        if version.as_deref() != Some(DERIVE_VERSION) {
            conn.execute_batch(
                "DELETE FROM timeline_items WHERE source = 'derived';
                 DELETE FROM node_sources;",
            )
            .map_err(sql)?;
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('derive_version', ?1)",
                params![DERIVE_VERSION],
            )
            .map_err(sql)?;
        }

        Ok(TimelineStore {
            conn,
            seen_changes: None,
        })
    }

    /// The connection, for the evidence ledger's own table. See `timeline::ledger`.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Whether the timeline already matches a cache that has made this many changes.
    pub fn is_current(&self, cache_changes: u64) -> bool {
        self.seen_changes == Some(cache_changes)
    }

    /// Bring the timeline in line with `snapshot`, deriving only what changed.
    pub fn apply(&mut self, snapshot: &Snapshot) -> AppResult<CatchUp> {
        let date_fields: HashMap<String, Vec<String>> = snapshot
            .nodes
            .iter()
            .filter(|n| n.node_type == "schema")
            .filter_map(|n| derive::date_fields_from_schema(&n.title, &n.properties()))
            .collect();

        let tx = self.conn.transaction().map_err(sql)?;

        let known: HashMap<String, String> = {
            let mut stmt = tx
                .prepare("SELECT node_id, signature FROM node_sources")
                .map_err(sql)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(sql)?;
            rows.flatten().collect()
        };

        let mut report = CatchUp {
            nodes_read: snapshot.nodes.len(),
            ..CatchUp::default()
        };
        let mut present: HashSet<&str> = HashSet::with_capacity(snapshot.nodes.len());

        for node in &snapshot.nodes {
            present.insert(&node.id);
            let signature = signature(node, date_fields.get(&node.node_type));
            if known.get(&node.id) == Some(&signature) {
                continue;
            }

            tx.execute(
                "DELETE FROM timeline_items
                 WHERE node_id = ?1 AND source = 'derived' AND time_source IS NOT 'note'",
                params![node.id],
            )
            .map_err(sql)?;
            let properties = node.properties();
            let view = NodeView {
                id: &node.id,
                node_type: &node.node_type,
                title: &node.title,
                properties: &properties,
            };
            for (n, derived) in derive::derive(&view, &date_fields).iter().enumerate() {
                let id = format!("{}#{}#{}", node.id, derived.kind, n);
                insert_derived(&tx, &id, node, derived)?;
            }
            tx.execute(
                "INSERT INTO node_sources (node_id, signature) VALUES (?1, ?2)
                 ON CONFLICT(node_id) DO UPDATE SET signature = excluded.signature",
                params![node.id, signature],
            )
            .map_err(sql)?;
            report.nodes_derived += 1;
        }

        for gone in known.keys().filter(|id| !present.contains(id.as_str())) {
            tx.execute(
                "DELETE FROM timeline_items WHERE node_id = ?1 AND source = 'derived'",
                params![gone],
            )
            .map_err(sql)?;
            tx.execute("DELETE FROM node_sources WHERE node_id = ?1", params![gone])
                .map_err(sql)?;
            report.nodes_removed += 1;
        }

        place_media_by_note(&tx, snapshot)?;

        report.items = tx
            .query_row("SELECT COUNT(*) FROM timeline_items", [], |r| r.get::<_, i64>(0))
            .map_err(sql)? as usize;
        tx.commit().map_err(sql)?;

        self.seen_changes = Some(snapshot.changes);
        Ok(report)
    }

    /// Forget every derived item and read the whole snapshot again.
    pub fn rebuild(&mut self, snapshot: &Snapshot) -> AppResult<CatchUp> {
        self.conn
            .execute_batch(
                "DELETE FROM timeline_items WHERE source = 'derived';
                 DELETE FROM node_sources;",
            )
            .map_err(sql)?;
        self.seen_changes = None;
        self.apply(snapshot)
    }

    /// What overlaps `span`, up to `today`, most precisely known first.
    ///
    /// The question is cut off at today, because the timeline is history and a
    /// plan is not (§4.7.C). So asking about next year finds nothing, not even
    /// the job held now, whose span runs on to `open_end`; asking about this
    /// year finds that job, because it is still going.
    pub fn query(&self, span: Span, today: NaiveDate) -> AppResult<Vec<TimelineItem>> {
        let until = span.to.min(today);
        if span.from > until {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, kind, node_id, node_type, title, label, related_id,
                        happened_from, happened_to, precision, time_source, source
                 FROM timeline_items
                 WHERE happened_from <= ?1 AND happened_to >= ?2
                   AND superseded_by IS NULL AND sealed = 0 AND source != 'extract'
                 ORDER BY julianday(happened_to) - julianday(happened_from), happened_from, kind, id",
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(
                params![when::iso(until), when::iso(span.from)],
                |r| {
                    Ok(TimelineItem {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        node_id: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        node_type: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        title: r.get(4)?,
                        label: r.get(5)?,
                        related_id: r.get(6)?,
                        happened_from: r.get(7)?,
                        happened_to: r.get(8)?,
                        precision: r.get(9)?,
                        time_source: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
                        source: r.get(11)?,
                    })
                },
            )
            .map_err(sql)?;
        Ok(rows.flatten().collect())
    }

    /// Every item up to `today`, for a view that reads the whole timeline.
    pub fn all_items(&self, today: NaiveDate) -> AppResult<Vec<TimelineItem>> {
        self.select("happened_from <= ?1", vec![when::iso(today)])
    }

    /// What the timeline holds about a node, under any of its names: items it
    /// implies itself, and items that point at it, such as an interaction with
    /// a person or someone else's relationship with them.
    pub fn about(&self, names: &[&str], today: NaiveDate) -> AppResult<Vec<TimelineItem>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let marks = (0..names.len())
            .map(|i| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(", ");
        let mut values = vec![when::iso(today)];
        values.extend(names.iter().map(|name| name.to_string()));
        self.select(
            &format!("happened_from <= ?1 AND (node_id IN ({marks}) OR related_id IN ({marks}))"),
            values,
        )
    }

    fn select(&self, condition: &str, values: Vec<String>) -> AppResult<Vec<TimelineItem>> {
        let statement = format!(
            "SELECT id, kind, node_id, node_type, title, label, related_id,
                    happened_from, happened_to, precision, time_source, source
             FROM timeline_items
             WHERE {condition} AND superseded_by IS NULL AND sealed = 0 AND source != 'extract'
             ORDER BY happened_from, kind, id"
        );
        let mut stmt = self.conn.prepare(&statement).map_err(sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(values.iter()), read_item)
            .map_err(sql)?;
        Ok(rows.flatten().collect())
    }
}

fn read_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<TimelineItem> {
    Ok(TimelineItem {
        id: r.get(0)?,
        kind: r.get(1)?,
        node_id: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        node_type: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        title: r.get(4)?,
        label: r.get(5)?,
        related_id: r.get(6)?,
        happened_from: r.get(7)?,
        happened_to: r.get(8)?,
        precision: r.get(9)?,
        time_source: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
        source: r.get(11)?,
    })
}

fn is_media_node(file: &SnapshotNode) -> bool {
    let properties = file.properties();
    derive::is_media(derive::media_name(&NodeView {
        id: &file.id,
        node_type: &file.node_type,
        title: &file.title,
        properties: &properties,
    }))
}

/// The node `name` means: its own path, its identity, or the exact title of a
/// person. A name that is nobody and nothing is `None`, which is an answer.
pub fn node_for(cache: &DbBridge, name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    cache
        .conn()
        .query_row(
            "SELECT id FROM nodes
             WHERE id = ?1 OR stable_id = ?1
                OR (node_type = 'person' AND lower(title) = lower(?1))
             ORDER BY (id = ?1) DESC, (node_type = 'person') DESC
             LIMIT 1",
            [name],
            |r| r.get::<_, String>(0),
        )
        .ok()
}

/// Catch the timeline up with the cache, reading the cache only if it changed.
///
/// Locks the cache briefly, and never while the timeline writes.
pub fn catch_up(cache: &DbState, timeline: &mut TimelineStore) -> AppResult<CatchUp> {
    let snapshot = {
        let db = cache.lock().unwrap_or_else(|e| e.into_inner());
        if timeline.is_current(db.conn().total_changes()) {
            return Ok(CatchUp::default());
        }
        Snapshot::read(&db)?
    };
    timeline.apply(&snapshot)
}

/// Everything derivation reads from one node, hashed.
fn signature(node: &SnapshotNode, date_keys: Option<&Vec<String>>) -> String {
    let mut hasher = blake3::Hasher::new();
    for part in [
        DERIVE_VERSION,
        node.node_type.as_str(),
        node.title.as_str(),
        node.properties_raw.as_str(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }
    if let Some(keys) = date_keys {
        hasher.update(keys.join(",").as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn insert_derived(tx: &Transaction, id: &str, node: &SnapshotNode, derived: &Derived) -> AppResult<()> {
    tx.execute(
        "INSERT OR REPLACE INTO timeline_items
            (id, kind, happened_from, happened_to, precision, time_source,
             node_id, node_type, title, label, related_id, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'derived')",
        params![
            id,
            derived.kind,
            when::iso(derived.span.from),
            when::iso(derived.span.to),
            derived.span.precision.as_str(),
            derived.time_source,
            node.id,
            node.node_type,
            node.title,
            derived.label,
            derived.related_id,
        ],
    )
    .map_err(sql)?;
    Ok(())
}

/// A picture with no date in its name takes the day of the note it sits in.
///
/// On the vault this was checked against, 86 of 106 files in `assets/` were
/// embedded in a note, and pasted screenshots carry no camera date at all. The
/// earliest dated note wins: a picture first appears on the day it was taken,
/// and a later note that embeds it again is remembering it.
///
/// Recomputed whole on every catch-up. It is one pass over attachment edges,
/// and working out which pictures a changed note used to embed would cost more
/// than doing it again.
fn place_media_by_note(tx: &Transaction, snapshot: &Snapshot) -> AppResult<()> {
    tx.execute("DELETE FROM timeline_items WHERE time_source = 'note'", [])
        .map_err(sql)?;

    let dated_notes: HashMap<String, (String, String, String)> = {
        let mut stmt = tx
            .prepare("SELECT node_id, happened_from, happened_to, precision FROM timeline_items WHERE kind = 'note'")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))
            .map_err(sql)?;
        rows.flatten().collect()
    };
    let named: HashSet<String> = {
        let mut stmt = tx
            .prepare("SELECT node_id FROM timeline_items WHERE kind = 'media' AND time_source IN ('filename', 'exif')")
            .map_err(sql)?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(sql)?;
        rows.flatten().collect()
    };
    let nodes: HashMap<&str, &SnapshotNode> =
        snapshot.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut placed: HashMap<&str, (&str, &(String, String, String))> = HashMap::new();
    for (note_id, file_id) in &snapshot.attachments {
        let Some(file) = nodes.get(file_id.as_str()) else {
            continue;
        };
        if file.node_type != "file" || named.contains(file_id) || !is_media_node(file) {
            continue;
        }
        let Some(span) = dated_notes.get(note_id) else {
            continue;
        };
        match placed.get(file_id.as_str()) {
            Some((_, earlier)) if earlier.0 <= span.0 => {}
            _ => {
                placed.insert(file_id, (note_id, span));
            }
        }
    }

    for (file_id, (note_id, (from, to, precision))) in placed {
        let file = nodes[file_id];
        tx.execute(
            "INSERT OR REPLACE INTO timeline_items
                (id, kind, happened_from, happened_to, precision, time_source,
                 node_id, node_type, title, related_id, source)
             VALUES (?1, 'media', ?2, ?3, ?4, 'note', ?5, ?6, ?7, ?8, 'derived')",
            params![
                format!("{file_id}#media#note"),
                from,
                to,
                precision,
                file.id,
                file.node_type,
                file.title,
                note_id,
            ],
        )
        .map_err(sql)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_read_as_the_node_it_names_and_nothing_else() {
        let db = DbBridge::new_in_memory_full().unwrap();
        let person = crate::models::node::NodeMetadata {
            id: "People/mai.md".into(),
            node_type: "person".into(),
            title: "Nguyễn Thu Mai".into(),
            content: String::new(),
            properties: serde_json::json!({ "node_id": "uuid-mai" }),
            created_at: String::new(),
            updated_at: String::new(),
            timestamp: 0,
            blocks: None,
        };
        db.upsert_node(&person).unwrap();
        assert_eq!(node_for(&db, "People/mai.md").as_deref(), Some("People/mai.md"));
        assert_eq!(node_for(&db, "uuid-mai").as_deref(), Some("People/mai.md"));
        assert_eq!(node_for(&db, "nguyễn thu mai").as_deref(), Some("People/mai.md"));
        assert_eq!(node_for(&db, "nhà ông Thu"), None, "a description is not a node");
        assert_eq!(node_for(&db, "  "), None);
    }
    use crate::db::NodeEdge;
    use crate::models::node::NodeMetadata;
    use serde_json::json;
    use std::sync::Mutex;

    fn node(id: &str, node_type: &str, title: &str, properties: Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            content: String::new(),
            properties,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 14).unwrap()
    }

    fn a_life() -> DbState {
        let db = DbBridge::new_in_memory_full().unwrap();
        for n in [
            node("Notes/d.md", "note", "2016-05-14", json!({ "date": "2016-05-14", "node_id": "note-1" })),
            node("Notes/plain.md", "note", "Ý tưởng", json!({})),
            node("Events/wed.md", "event", "Đám cưới", json!({ "start_at": "2016-05-20T09:00:00", "end_at": "2016-05-20T12:00:00" })),
            node("Events/june.md", "event", "Tháng sáu", json!({ "start_at": "2016-06-02" })),
            node("Events/weekly.md", "event", "Họp tuần", json!({ "start_at": "2016-01-04T09:00", "rrule": "FREQ=WEEKLY" })),
            node("Events/future.md", "event", "Sau này", json!({ "start_at": "2099-01-01" })),
            node("People/Interactions/c.md", "interaction", "Cà phê", json!({ "date": "2016-05-03", "person_id": "People/tuan.md" })),
            node("Tasks/t.md", "task", "Nộp hồ sơ", json!({ "completed_at": "2016-05-30", "due_date": "2016-05-01" })),
            node("People/me.md", "person", "Minh", json!({ "experiences": [{ "company": "Công ty đầu", "start": "2014-07", "end": "", "current": true }] })),
            node("Files/pic.md", "file", "1777860790-image.png", json!({ "path": "/v/assets/1777860790-image.png", "node_id": "file-1" })),
        ] {
            db.upsert_node(&n).unwrap();
        }
        db.upsert_node_edge(&NodeEdge {
            id: "e1".into(),
            source_id: "note-1".into(),
            target_id: "file-1".into(),
            edge_type: "attachment".into(),
            relation: Some("attachment".into()),
            created_at: "2026-01-01T00:00:00.000Z".into(),
        })
        .unwrap();
        Mutex::new(db)
    }

    fn kinds(items: &[TimelineItem]) -> Vec<&str> {
        items.iter().map(|i| i.kind.as_str()).collect()
    }

    /// The first gate in the doc.
    #[test]
    fn what_happened_in_may_2016_comes_back_across_types() {
        let cache = a_life();
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let items = timeline.query(when::parse("2016-05").unwrap(), today()).unwrap();
        let found = kinds(&items);

        for kind in ["note", "event", "interaction", "task_done", "experience", "media"] {
            assert!(found.contains(&kind), "{kind} missing from {found:?}");
        }
        assert!(!items.iter().any(|i| i.title == "Tháng sáu"), "June is not May");
        assert!(!items.iter().any(|i| i.title == "Họp tuần"), "a series is not an item");
        assert_eq!(found.last(), Some(&"experience"), "the least precise comes last");

        let picture = items.iter().find(|i| i.kind == "media").unwrap();
        assert_eq!(picture.happened_from, "2016-05-14");
        assert_eq!(picture.related_id.as_deref(), Some("Notes/d.md"));
        assert_eq!(picture.time_source, "note");
    }

    #[test]
    fn the_future_is_not_history() {
        let cache = a_life();
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        assert!(timeline.query(when::parse("2099").unwrap(), today()).unwrap().is_empty());

        let this_year = timeline.query(when::parse("2026").unwrap(), today()).unwrap();
        assert_eq!(kinds(&this_year), ["experience"], "the job held now is still going");
    }

    #[test]
    fn nothing_is_read_again_until_the_cache_changes_and_then_only_what_changed() {
        let cache = a_life();
        let mut timeline = TimelineStore::open_in_memory().unwrap();

        let first = catch_up(&cache, &mut timeline).unwrap();
        assert_eq!(first.nodes_derived, first.nodes_read);

        assert_eq!(catch_up(&cache, &mut timeline).unwrap(), CatchUp::default());

        cache
            .lock()
            .unwrap()
            .upsert_node(&node("Tasks/t.md", "task", "Nộp hồ sơ", json!({ "completed_at": "2016-05-31" })))
            .unwrap();
        let second = catch_up(&cache, &mut timeline).unwrap();
        assert_eq!(second.nodes_derived, 1);

        let items = timeline.query(when::parse("2016-05-31").unwrap(), today()).unwrap();
        assert_eq!(kinds(&items), ["task_done", "experience"]);
    }

    #[test]
    fn a_node_that_is_gone_takes_its_items_with_it() {
        let cache = a_life();
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        cache.lock().unwrap().delete_node("Notes/d.md").unwrap();
        let report = catch_up(&cache, &mut timeline).unwrap();
        assert_eq!(report.nodes_removed, 1);

        let items = timeline.query(when::parse("2016-05-14").unwrap(), today()).unwrap();
        assert!(!items.iter().any(|i| i.node_id == "Notes/d.md"));
        assert!(!items.iter().any(|i| i.kind == "media"), "the picture was dated by that note");
    }

    #[test]
    fn a_schema_change_reaches_the_nodes_of_that_type() {
        let cache = a_life();
        {
            let db = cache.lock().unwrap();
            db.upsert_node(&node("Animal/mun.md", "animal", "Mun", json!({ "vaccinated_at": "2016-05-09" }))).unwrap();
        }
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        let may = when::parse("2016-05-09").unwrap();
        assert!(!timeline.query(may, today()).unwrap().iter().any(|i| i.kind == "field"));

        cache
            .lock()
            .unwrap()
            .upsert_node(&node(
                "Schema/animal.md",
                "schema",
                "animal",
                json!({ "fields": [{ "key": "vaccinated_at", "kind": "date" }] }),
            ))
            .unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        assert!(timeline.query(may, today()).unwrap().iter().any(|i| i.kind == "field"));
    }

    /// Two gates in the doc: losing the cache does not lose the timeline, and
    /// losing the timeline costs only a re-read of the cache.
    #[test]
    fn the_timeline_outlives_the_cache_and_comes_back_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let may = when::parse("2016-05").unwrap();

        let expected = {
            let cache = a_life();
            let mut timeline = TimelineStore::open(&path).unwrap();
            catch_up(&cache, &mut timeline).unwrap();
            timeline.query(may, today()).unwrap()
        };
        assert!(!expected.is_empty());

        // The cache is gone; the file is still there and still answers.
        let reopened = TimelineStore::open(&path).unwrap();
        assert_eq!(reopened.query(may, today()).unwrap(), expected);
        drop(reopened);

        // The timeline is gone; it is derived again from a cache alone.
        std::fs::remove_file(&path).unwrap();
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
        let cache = a_life();
        let mut rebuilt = TimelineStore::open(&path).unwrap();
        catch_up(&cache, &mut rebuilt).unwrap();
        assert_eq!(rebuilt.query(may, today()).unwrap(), expected);
    }

    #[test]
    fn a_rebuild_gives_the_same_timeline() {
        let cache = a_life();
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        let may = when::parse("2016-05").unwrap();
        let before = timeline.query(may, today()).unwrap();

        let snapshot = Snapshot::read(&cache.lock().unwrap()).unwrap();
        timeline.rebuild(&snapshot).unwrap();
        assert_eq!(timeline.query(may, today()).unwrap(), before);
    }

    #[test]
    fn what_is_known_about_a_person_includes_what_points_at_them() {
        let cache = a_life();
        {
            let db = cache.lock().unwrap();
            db.upsert_node(&node("People/tuan.md", "person", "Tuấn", json!({ "node_id": "uuid-tuan", "died_on": "2020-01-02" }))).unwrap();
            db.upsert_node(&node(
                "People/me.md",
                "person",
                "Minh",
                json!({ "connections": [{ "person_id": "uuid-tuan", "relation_type": "friend", "since": "2009-09" }] }),
            ))
            .unwrap();
        }
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let items = timeline.about(&["People/tuan.md", "uuid-tuan"], today()).unwrap();
        let found = kinds(&items);
        assert!(found.contains(&"death"), "{found:?}");
        assert!(found.contains(&"connection"), "someone else's relationship with them: {found:?}");
        assert!(found.contains(&"interaction"), "an interaction that names them by path: {found:?}");
        assert!(!found.contains(&"note"), "a note that does not mention them: {found:?}");
    }

    #[test]
    fn timeline_files_in_the_cache_are_never_read() {
        let cache = a_life();
        cache
            .lock()
            .unwrap()
            .upsert_node(&node("Timeline/2016/2016-05.macbook.json", "note", "x", json!({ "date": "2016-05-14" })))
            .unwrap();
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let items = timeline.query(when::parse("2016-05-14").unwrap(), today()).unwrap();
        assert!(!items.iter().any(|i| i.node_id.starts_with("Timeline/")));
    }
}
