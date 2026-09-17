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

use super::derive::{self, Derived, Link, NodeView};
use super::fold;
use super::magnitude::{self, Signals};
use super::when::{self, Span};
use crate::db::{DbBridge, DbState};
use crate::error::{AppError, AppResult};

pub const FILE_NAME: &str = "timeline.db";

/// Bump when [`derive`] would read an unchanged node differently, so every
/// device derives its timeline again on the next launch.
///
/// 5: the event model settled — links for everyone an event names, a size, the
/// free-form part, and folding as a mark rather than a deletion. A device that
/// built its index under any of the versions in between holds rows derived by
/// rules that no longer apply, and nothing else would ever ask it to look
/// again: `node_sources` still matches, so every node looks unchanged.
const DERIVE_VERSION: &str = "5";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventLink {
    pub node_id: String,
    pub role: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Event {
    pub id: String,
    pub kind: String,
    pub node_id: String,
    pub node_type: String,
    pub title: String,
    pub label: Option<String>,
    /// The first `with`, for readers that grew up asking for one name.
    pub related_id: Option<String>,
    /// Everyone and everything this event names.
    pub links: Vec<EventLink>,
    pub happened_from: String,
    pub happened_to: String,
    pub precision: String,
    pub time_source: String,
    pub source: String,
    /// How big it was, from the core alone. See [`super::magnitude`].
    pub magnitude: f64,
    /// The note that wrote it out, when the event is not a node of its own.
    pub container_node: Option<String>,
    /// What the person wrote that this version has no meaning for.
    pub props: Value,
}

/// Every column an [`Event`] is read from, in the order [`read_event`] wants.
const COLUMNS: &str = "id, kind, node_id, node_type, title, label, related_id, \
                       happened_from, happened_to, precision, time_source, source, \
                       magnitude, props, container_node";

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

        // `timeline_items` became `events` when the unit of the timeline became
        // a thing that happened rather than a node carrying a date
        // (`docs/su-kien-2026-09-16.md` §7). This runs before the CREATE below:
        // the other order would make an empty `events` beside the full old
        // table, and every row anybody had would be stranded in it.
        let table = |name: &str| {
            conn.query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![name],
                |r| r.get::<_, i64>(0),
            )
            .is_ok()
        };
        if table("timeline_items") {
            // `None` when the count could not be taken at all — the other
            // connection on this file (`ledger::LedgerDb`) may hold it. A
            // question that could not be answered must never read as "empty",
            // because the action on empty below is DROP TABLE.
            let rows = |name: &str| -> Option<i64> {
                conn.query_row(&format!("SELECT COUNT(*) FROM {name}"), [], |r| r.get(0))
                    .ok()
            };
            // An `events` with nothing in it is a table some other build made
            // on the way past; the rows are still under the old name. Seen for
            // real on 2026-09-16: 243 rows stranded in `timeline_items` beside
            // an empty `events`, because this guard only asked whether the new
            // name existed.
            if table("events") && rows("events") == Some(0) {
                conn.execute_batch("DROP TABLE events;").map_err(sql)?;
            }
            if !table("events") {
                conn.execute_batch("ALTER TABLE timeline_items RENAME TO events;")
                    .map_err(sql)?;
            } else {
                log::warn!(
                    "timeline: {} rows are under the old name `timeline_items` beside a table \
                     `events` that already has {} of its own; leaving both alone",
                    rows("timeline_items").unwrap_or(-1),
                    rows("events").unwrap_or(-1)
                );
            }
        }

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

             CREATE TABLE IF NOT EXISTS events (
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
                sealed         INTEGER NOT NULL DEFAULT 0,
                -- The event model, §5. `props` is one JSON object rather than a
                -- table of (key, value): see the doc for why not EAV.
                magnitude      REAL NOT NULL DEFAULT 0,
                props          TEXT NOT NULL DEFAULT '{}',
                container_node TEXT,
                -- The event this one was tucked inside by `timeline::fold`.
                -- Folding marks rather than deletes, so catch-up stays
                -- reproducible and turning it off costs a filter, not a rebuild.
                folded_into    TEXT
             );

             -- Everything an event names, and how it took part. One event has
             -- as many rows here as it has people, places and evidence; the
             -- `related_id` column above holds the first of them, for the
             -- readers that still ask for one. See `docs/su-kien-2026-09-16.md`.
             CREATE TABLE IF NOT EXISTS event_links (
                event_id TEXT NOT NULL,
                node_id  TEXT NOT NULL,
                role     TEXT NOT NULL,
                label    TEXT,
                PRIMARY KEY (event_id, node_id, role)
             );
             CREATE INDEX IF NOT EXISTS idx_event_links_node ON event_links(node_id);

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

        // A table that was renamed has the rows but not these.
        for (column, declaration) in [
            ("magnitude", "REAL NOT NULL DEFAULT 0"),
            ("props", "TEXT NOT NULL DEFAULT '{}'"),
            ("container_node", "TEXT"),
            ("folded_into", "TEXT"),
        ] {
            let present = conn
                .prepare("SELECT 1 FROM pragma_table_info('events') WHERE name = ?1")
                .and_then(|mut stmt| stmt.exists(params![column]))
                .unwrap_or(false);
            if !present {
                conn.execute_batch(&format!("ALTER TABLE events ADD COLUMN {column} {declaration};"))
                    .map_err(sql)?;
            }
        }

        // A worldline is worked out when asked now, not kept — the stored one
        // could not pass through the seal filter. See `timeline::presence`.
        conn.execute_batch("DROP TABLE IF EXISTS object_presence;").ok();

        // Indexes follow a renamed table but keep their old names.
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_timeline_range;
             DROP INDEX IF EXISTS idx_timeline_node;
             DROP INDEX IF EXISTS idx_timeline_rel;
             CREATE INDEX IF NOT EXISTS idx_events_range ON events(happened_from, happened_to);
             CREATE INDEX IF NOT EXISTS idx_events_node  ON events(node_id);
             CREATE INDEX IF NOT EXISTS idx_events_rel   ON events(related_id);
             DROP INDEX IF EXISTS idx_events_mag;",
        )
        .map_err(sql)?;

        let version: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key = 'derive_version'", [], |r| r.get(0))
            .ok();
        if version.as_deref() != Some(DERIVE_VERSION) {
            conn.execute_batch(
                // Links first: the subquery needs the rows it names to be
                // there. "Links outlive nothing" holds across a bump too.
                "DELETE FROM event_links WHERE event_id IN
                    (SELECT id FROM events WHERE source = 'derived');
                 DELETE FROM events WHERE source = 'derived';
                 DELETE FROM node_sources;
                 DELETE FROM month_files;",
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

            forget_events(
                &tx,
                "node_id = ?1 AND source = 'derived' AND time_source IS NOT 'note'",
                params![node.id],
            )?;
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
            forget_events(&tx, "node_id = ?1 AND source = 'derived'", params![gone])?;
            tx.execute("DELETE FROM node_sources WHERE node_id = ?1", params![gone])
                .map_err(sql)?;
            report.nodes_removed += 1;
        }

        place_media_by_note(&tx, snapshot)?;

        // Notes become the box, pictures become evidence — by being marked,
        // not removed. Off clears every mark, which is why it needs no
        // rebuild. See `timeline::fold`.
        fold::fold_days(&tx, fold::folding(&tx))?;

        // What the timeline shows, not what the table holds: folding tucks a
        // row inside another rather than removing it, and a count that ignored
        // that would say the list never got shorter.
        report.items = tx
            .query_row(
                "SELECT COUNT(*) FROM events WHERE folded_into IS NULL",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map_err(sql)? as usize;
        tx.commit().map_err(sql)?;

        self.seen_changes = Some(snapshot.changes);
        Ok(report)
    }

    /// Forget every derived item and read the whole snapshot again.
    pub fn rebuild(&mut self, snapshot: &Snapshot) -> AppResult<CatchUp> {
        self.conn
            .execute_batch(
                "DELETE FROM event_links WHERE event_id IN (SELECT id FROM events WHERE source = 'derived');
                 DELETE FROM events WHERE source = 'derived';
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
    pub fn query(&self, span: Span, today: NaiveDate) -> AppResult<Vec<Event>> {
        let until = span.to.min(today);
        if span.from > until {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(
                &format!(
                    "SELECT {COLUMNS}
                     FROM events
                     WHERE happened_from <= ?1 AND happened_to >= ?2
                       AND superseded_by IS NULL AND sealed = 0 AND source != 'extract' AND folded_into IS NULL
                     ORDER BY julianday(happened_to) - julianday(happened_from), happened_from, kind, id"
                ),
            )
            .map_err(sql)?;
        let rows = stmt
            .query_map(params![when::iso(until), when::iso(span.from)], read_event)
            .map_err(sql)?;
        let mut items: Vec<Event> = rows.flatten().collect();
        self.attach_links(&mut items)?;
        Ok(items)
    }

    /// What overlaps `span`, folded or not.
    ///
    /// The picture gallery shows a day's pictures whether or not they are now
    /// evidence of something: folding changed where a picture belongs, not
    /// whether it exists. See `timeline::fold`.
    pub fn including_folded(&self, span: Span, today: NaiveDate) -> AppResult<Vec<Event>> {
        let until = span.to.min(today);
        if span.from > until {
            return Ok(Vec::new());
        }
        let statement = format!(
            "SELECT {COLUMNS}
             FROM events
             WHERE happened_from <= ?1 AND happened_to >= ?2
               AND superseded_by IS NULL AND sealed = 0 AND source != 'extract'
             ORDER BY happened_from, kind, id"
        );
        let mut stmt = self.conn.prepare(&statement).map_err(sql)?;
        let rows = stmt
            .query_map(params![when::iso(until), when::iso(span.from)], read_event)
            .map_err(sql)?;
        let mut events: Vec<Event> = rows.flatten().collect();
        self.attach_links(&mut events)?;
        Ok(events)
    }

    /// Every item up to `today`, for a view that reads the whole timeline.
    pub fn all_items(&self, today: NaiveDate) -> AppResult<Vec<Event>> {
        self.select("happened_from <= ?1", vec![when::iso(today)])
    }

    /// What the timeline holds about a node, under any of its names: items it
    /// implies itself, and items that point at it, such as an interaction with
    /// a person or someone else's relationship with them.
    pub fn about(&self, names: &[&str], today: NaiveDate) -> AppResult<Vec<Event>> {
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
            &format!(
                "happened_from <= ?1 AND (node_id IN ({marks}) OR related_id IN ({marks}) \
                 OR id IN (SELECT event_id FROM event_links WHERE node_id IN ({marks})))"
            ),
            values,
        )
    }

    fn select(&self, condition: &str, values: Vec<String>) -> AppResult<Vec<Event>> {
        let statement = format!(
            "SELECT {COLUMNS}
             FROM events
             WHERE {condition} AND superseded_by IS NULL AND sealed = 0 AND source != 'extract' AND folded_into IS NULL AND folded_into IS NULL
             ORDER BY happened_from, kind, id"
        );
        let mut stmt = self.conn.prepare(&statement).map_err(sql)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(values.iter()), read_event)
            .map_err(sql)?;
        let mut items: Vec<Event> = rows.flatten().collect();
        self.attach_links(&mut items)?;
        Ok(items)
    }

    /// Give each item what it names, in one query rather than one per item.
    fn attach_links(&self, items: &mut [Event]) -> AppResult<()> {
        if items.is_empty() {
            return Ok(());
        }
        let marks = (0..items.len()).map(|i| format!("?{}", i + 1)).collect::<Vec<_>>().join(", ");
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT event_id, node_id, role, label FROM event_links WHERE event_id IN ({marks}) ORDER BY role, node_id"
            ))
            .map_err(sql)?;
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        let rows = stmt
            .query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                Ok((r.get::<_, String>(0)?, EventLink { node_id: r.get(1)?, role: r.get(2)?, label: r.get(3)? }))
            })
            .map_err(sql)?;
        let mut found: HashMap<String, Vec<EventLink>> = HashMap::new();
        for (event_id, link) in rows.flatten() {
            found.entry(event_id).or_default().push(link);
        }
        for item in items {
            if let Some(links) = found.remove(&item.id) {
                item.links = links;
            }
        }
        Ok(())
    }
}

fn read_event(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        id: r.get(0)?,
        kind: r.get(1)?,
        node_id: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        node_type: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        title: r.get(4)?,
        label: r.get(5)?,
        related_id: r.get(6)?,
        links: Vec::new(),
        happened_from: r.get(7)?,
        happened_to: r.get(8)?,
        precision: r.get(9)?,
        time_source: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
        source: r.get(11)?,
        magnitude: r.get::<_, Option<f64>>(12)?.unwrap_or_default(),
        props: r
            .get::<_, Option<String>>(13)?
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null),
        container_node: r.get(14)?,
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
/// The names an event's links go by in one role, ready to show.
///
/// An id nothing is known about answers for itself: a place that is only the
/// words somebody typed is already its own name (§3.2, and §10 question 4 —
/// free text first, a node when it earns one).
pub fn named(links: &[EventLink], role: &str, names: &HashMap<String, String>) -> Vec<String> {
    links
        .iter()
        .filter(|link| link.role == role)
        .map(|link| names.get(&link.node_id).cloned().unwrap_or_else(|| link.node_id.clone()))
        .filter(|name| !name.trim().is_empty())
        .collect()
}

/// One lookup covering everything a page of events names.
pub fn names_in(cache: &DbBridge, items: &[Event]) -> HashMap<String, String> {
    let ids: Vec<&str> = items
        .iter()
        .flat_map(|item| item.links.iter().map(|link| link.node_id.as_str()))
        .collect();
    names_for(cache, &ids)
}

/// What to call the nodes an event names. The twin of [`node_for`].
///
/// An event holds ids — `uuid-tuan`, `People/tuan.md` — and a person reading a
/// timeline needs "Tuấn". A place that is only words the person typed has no
/// node and no title, so it answers for itself and is left as written.
pub fn names_for(cache: &DbBridge, ids: &[&str]) -> HashMap<String, String> {
    let wanted: Vec<&str> = ids.iter().copied().filter(|id| !id.trim().is_empty()).collect();
    if wanted.is_empty() {
        return HashMap::new();
    }
    let marks = (0..wanted.len()).map(|i| format!("?{}", i + 1)).collect::<Vec<_>>().join(", ");
    let Ok(mut stmt) = cache.conn().prepare(&format!(
        "SELECT id, COALESCE(NULLIF(stable_id, ''), id), title FROM nodes
          WHERE id IN ({marks}) OR stable_id IN ({marks})"
    )) else {
        return HashMap::new();
    };
    let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(wanted.iter()), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    }) else {
        return HashMap::new();
    };
    let mut found = HashMap::new();
    for (id, identity, title) in rows.flatten() {
        if title.trim().is_empty() {
            continue;
        }
        found.insert(id, title.clone());
        found.insert(identity, title);
    }
    found
}

pub fn catch_up(cache: &DbState, timeline: &mut TimelineStore) -> AppResult<CatchUp> {
    catch_up_in(cache, timeline, None)
}

/// Catch up, and first ask the vault whether folding is still wanted.
///
/// Deriving happens with no vault path in hand, so the answer is mirrored into
/// `timeline.db`; every caller that knows where the vault is refreshes it.
/// Changing it changes which rows exist, so that is a rebuild, not a catch-up.
pub fn catch_up_in(
    cache: &DbState,
    timeline: &mut TimelineStore,
    vault: Option<&str>,
) -> AppResult<CatchUp> {
    let switched = match vault {
        Some(vault) => fold::mirror(timeline.conn(), vault)?,
        None => false,
    };
    let snapshot = {
        let db = cache.lock().unwrap_or_else(|e| e.into_inner());
        if !switched && timeline.is_current(db.conn().total_changes()) {
            return Ok(CatchUp::default());
        }
        Snapshot::read(&db)?
    };
    // Folding marks rows rather than deleting them, so changing the answer
    // only has to run the pass again — not derive the whole vault afresh.
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
    let (from, to) = (when::iso(derived.span.from), when::iso(derived.span.to));
    let played = |role: &str| derived.links.iter().filter(|link| link.role == role).count();
    let text = match &derived.label {
        Some(label) => format!("{} {label}", node.title),
        None => node.title.clone(),
    };
    let size = magnitude::of(Signals {
        from: &from,
        to: &to,
        people: played("with"),
        evidence: played("evidence"),
        text: &text,
        source: "derived",
    });
    tx.execute(
        "INSERT OR REPLACE INTO events
            (id, kind, happened_from, happened_to, precision, time_source,
             node_id, node_type, title, label, related_id, source,
             magnitude, props, container_node)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'derived', ?12, ?13, ?14)",
        params![
            id,
            derived.kind,
            from,
            to,
            derived.span.precision.as_str(),
            derived.time_source,
            node.id,
            node.node_type,
            node.title,
            derived.label,
            derived.links.first().filter(|link| link.role == "with").map(|link| &link.node),
            size,
            props_text(&derived.props),
            derived.container.then(|| node.id.clone()),
        ],
    )
    .map_err(sql)?;
    write_links(tx, id, &derived.links)?;
    Ok(())
}

/// Free-form fields as they will be stored. An event always has the column,
/// and an event with nothing unusual in it has an empty object.
fn props_text(props: &Value) -> String {
    match props {
        Value::Object(fields) if !fields.is_empty() => props.to_string(),
        _ => "{}".to_string(),
    }
}

/// Keep what an event names. Replaces whatever was there for that event.
fn write_links(tx: &Transaction, event_id: &str, links: &[Link]) -> AppResult<()> {
    tx.execute("DELETE FROM event_links WHERE event_id = ?1", params![event_id])
        .map_err(sql)?;
    for link in links {
        if link.node.trim().is_empty() {
            continue;
        }
        tx.execute(
            "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label) VALUES (?1, ?2, ?3, ?4)",
            params![event_id, link.node, link.role, link.label],
        )
        .map_err(sql)?;
    }
    Ok(())
}

/// Delete items and the links that belong to them. Links outlive nothing.
fn forget_events(tx: &Transaction, condition: &str, params: impl rusqlite::Params + Clone) -> AppResult<()> {
    tx.execute(
        &format!("DELETE FROM event_links WHERE event_id IN (SELECT id FROM events WHERE {condition})"),
        params.clone(),
    )
    .map_err(sql)?;
    tx.execute(&format!("DELETE FROM events WHERE {condition}"), params)
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
    forget_events(tx, "time_source = 'note'", [])?;

    let dated_notes: HashMap<String, (String, String, String)> = {
        let mut stmt = tx
            .prepare("SELECT node_id, happened_from, happened_to, precision FROM events WHERE kind = 'note'")
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))
            .map_err(sql)?;
        rows.flatten().collect()
    };
    let named: HashSet<String> = {
        let mut stmt = tx
            .prepare("SELECT node_id FROM events WHERE kind = 'media' AND time_source IN ('filename', 'exif')")
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
            "INSERT OR REPLACE INTO events
                (id, kind, happened_from, happened_to, precision, time_source,
                 node_id, node_type, title, related_id, source, magnitude)
             VALUES (?1, 'media', ?2, ?3, ?4, 'note', ?5, ?6, ?7, ?8, 'derived', ?9)",
            params![
                format!("{file_id}#media#note"),
                from,
                to,
                precision,
                file.id,
                file.node_type,
                file.title,
                note_id,
                magnitude::of(Signals {
                    from,
                    to,
                    evidence: 1,
                    text: &file.title,
                    source: "derived",
                    ..Signals::default()
                }),
            ],
        )
        .map_err(sql)?;
        // The note is what dates the picture, and what shows it was taken
        // that day. It did not take part in anything.
        write_links(
            tx,
            &format!("{file_id}#media#note"),
            &[Link { node: note_id.to_string(), role: "evidence", label: None }],
        )?;
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

    /// How many links the store is holding, to catch rows outliving their event.
    fn link_count(timeline: &TimelineStore) -> i64 {
        timeline
            .conn
            .query_row("SELECT COUNT(*) FROM event_links", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn a_meeting_is_found_under_everyone_who_was_at_it() {
        let db = DbBridge::new_in_memory_full().unwrap();
        for n in [
            node("People/tuan.md", "person", "Tuấn", json!({ "node_id": "uuid-tuan" })),
            node("People/thuy.md", "person", "Thuỳ", json!({ "node_id": "uuid-thuy" })),
            node("People/ha.md", "person", "Hà", json!({ "node_id": "uuid-ha" })),
            node(
                "Notes/2016-05-14.md",
                "note",
                "2016-05-14",
                json!({
                    "date": "2016-05-14",
                    "moments": [{ "title": "Đám cưới", "happened": "2016-05-14", "people": ["uuid-tuan", "uuid-thuy", "uuid-ha"] }]
                }),
            ),
        ] {
            db.upsert_node(&n).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        for person in ["uuid-tuan", "uuid-thuy", "uuid-ha"] {
            let found = timeline.about(&[person], today()).unwrap();
            assert!(
                found.iter().any(|item| item.kind == "moment"),
                "{person} was at it and cannot find it: {found:?}"
            );
        }
        let moment = timeline
            .query(when::parse("2016-05-14").unwrap(), today())
            .unwrap()
            .into_iter()
            .find(|item| item.kind == "moment")
            .expect("the moment");
        assert_eq!(moment.links.len(), 3, "{:?}", moment.links);
        assert_eq!(moment.related_id.as_deref(), Some("uuid-tuan"), "the old single name still answers");
    }

    #[test]
    fn what_an_event_named_goes_when_the_event_does() {
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node(
            "Notes/day.md",
            "note",
            "2016-05-14",
            json!({ "date": "2016-05-14", "moments": [{ "title": "Gặp", "happened": "2016-05-14", "people": ["uuid-a", "uuid-b"] }] }),
        ))
        .unwrap();
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        assert_eq!(link_count(&timeline), 2);

        // The same note, now naming one person: the other's link goes with it.
        {
            let db = cache.lock().unwrap();
            db.upsert_node(&node(
                "Notes/day.md",
                "note",
                "2016-05-14",
                json!({ "date": "2016-05-14", "moments": [{ "title": "Gặp", "happened": "2016-05-14", "people": ["uuid-a"] }] }),
            ))
            .unwrap();
        }
        catch_up(&cache, &mut timeline).unwrap();
        assert_eq!(link_count(&timeline), 1);

        {
            let db = cache.lock().unwrap();
            db.delete_node("Notes/day.md").unwrap();
        }
        catch_up(&cache, &mut timeline).unwrap();
        assert_eq!(link_count(&timeline), 0, "a link cannot outlive its event");
    }

    #[test]
    fn a_key_this_version_does_not_know_survives_the_trip_through_the_index() {
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node(
            "Notes/2026-06-18.md",
            "note",
            "2026-06-18",
            json!({
                "date": "2026-06-18",
                "moments": [{
                    "title": "Hệ thống ABC lỗi",
                    "happened": "2026-06-18",
                    "severity": "P1",
                    "downtime_minutes": 148
                }]
            }),
        ))
        .unwrap();
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let moment = timeline
            .query(when::parse("2026-06-18").unwrap(), today())
            .unwrap()
            .into_iter()
            .find(|event| event.kind == "moment")
            .expect("the moment");
        assert_eq!(moment.props, json!({ "severity": "P1", "downtime_minutes": 148 }));
        assert_eq!(
            moment.container_node.as_deref(),
            Some("Notes/2026-06-18.md"),
            "the note wrote it out; it is not the event"
        );
    }

    #[test]
    fn folding_shortens_the_list_and_never_takes_a_day_off_the_strip() {
        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_string_lossy().to_string();
        let cache = a_life();
        let days = |store: &TimelineStore| -> Vec<String> {
            let mut seen: Vec<String> = store
                .all_items(today())
                .unwrap()
                .into_iter()
                .map(|event| event.happened_from)
                .collect();
            seen.sort();
            seen.dedup();
            seen
        };

        let mut flat = TimelineStore::open_in_memory().unwrap();
        let mut off = fold::Config { fold: false, ..fold::Config::default() };
        fold::write_config(&vault_path, &mut off, chrono::Utc::now()).unwrap();
        let before = catch_up_in(&cache, &mut flat, Some(&vault_path)).unwrap();

        let mut folded = TimelineStore::open_in_memory().unwrap();
        let mut on = fold::Config::default();
        fold::write_config(&vault_path, &mut on, chrono::Utc::now()).unwrap();
        let after = catch_up_in(&cache, &mut folded, Some(&vault_path)).unwrap();

        assert_eq!(
            (before.items, after.items),
            (8, 7),
            "the picture folds into the day whose note dated it"
        );
        assert_eq!(days(&flat), days(&folded), "no day may leave the strip");
    }

    /// A device that has been running the released version has rows under the
    /// old name, and some of them cannot be derived again: a moment the person
    /// accepted from a conversation is a decision, not a reading of a field.
    #[test]
    fn a_timeline_from_the_released_version_keeps_its_rows_under_the_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.db");
        {
            let released = Connection::open(&path).unwrap();
            released
                .execute_batch(
                    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     CREATE TABLE timeline_items (
                        id TEXT PRIMARY KEY, kind TEXT NOT NULL, happened_from TEXT NOT NULL,
                        happened_to TEXT NOT NULL, precision TEXT NOT NULL, time_source TEXT,
                        recorded_at TEXT, node_id TEXT, node_type TEXT,
                        title TEXT NOT NULL DEFAULT '', label TEXT, related_id TEXT,
                        source TEXT NOT NULL, confidence REAL, evidence TEXT, month_file TEXT,
                        superseded_by TEXT, sealed INTEGER NOT NULL DEFAULT 0
                     );
                     CREATE INDEX idx_timeline_range ON timeline_items(happened_from, happened_to);
                     CREATE TABLE node_sources (node_id TEXT PRIMARY KEY, signature TEXT NOT NULL);
                     CREATE TABLE month_files (path TEXT PRIMARY KEY, content_hash TEXT NOT NULL, loaded_at TEXT NOT NULL);
                     INSERT INTO meta (key, value) VALUES ('derive_version', '3');
                     INSERT INTO timeline_items
                        (id, kind, happened_from, happened_to, precision, node_id, title, label, related_id, source)
                     VALUES ('kept#accepted', 'moment', '2016-05-14', '2016-05-14', 'day',
                             'Syn/chat.md', '', 'Đám cưới Tuấn và Thuỳ', 'uuid-tuan', 'user');
                     INSERT INTO timeline_items
                        (id, kind, happened_from, happened_to, precision, node_id, title, source)
                     VALUES ('Notes/a.md#note#0', 'note', '2016-05-14', '2016-05-14', 'day',
                             'Notes/a.md', '2016-05-14', 'derived');",
                )
                .unwrap();
        }

        let timeline = TimelineStore::open(&path).unwrap();
        let kept = timeline.all_items(today()).unwrap();
        assert_eq!(
            kept.iter().map(|event| event.id.as_str()).collect::<Vec<_>>(),
            vec!["kept#accepted"],
            "what a person accepted is kept; what was derived is derived again"
        );
        assert_eq!(kept[0].label.as_deref(), Some("Đám cưới Tuấn và Thuỳ"));
        assert_eq!(kept[0].related_id.as_deref(), Some("uuid-tuan"), "the old column still reads");
        assert_eq!(kept[0].magnitude, 0.0, "an old row has no size until it is written again");
        assert_eq!(kept[0].props, json!({}));
        assert!(
            timeline
                .conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'timeline_items'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .is_err(),
            "the old table was renamed, not left behind beside a copy"
        );
    }

    /// What an event points at, for the assertions below.
    fn evidence(node_id: &str) -> EventLink {
        EventLink { node_id: node_id.to_string(), role: "evidence".into(), label: None }
    }

    /// A day with a finished task on it: the note is where it was written, not
    /// a second thing that happened, and the picture is what shows it.
    #[test]
    fn a_day_that_holds_something_keeps_its_note_as_the_box_and_not_as_a_row() {
        let db = DbBridge::new_in_memory_full().unwrap();
        for n in [
            node("Notes/2026-06-18.md", "note", "2026-06-18", json!({ "date": "2026-06-18" })),
            node("Tasks/ship.md", "task", "Ship it", json!({ "completed_at": "2026-06-18" })),
            node("Files/2026-06-18 lunch.jpg", "file", "2026-06-18 lunch.jpg", json!({})),
        ] {
            db.upsert_node(&n).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let day = timeline.query(when::parse("2026-06-18").unwrap(), today()).unwrap();
        assert_eq!(
            day.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
            vec!["task_done"],
            "one thing happened that day: {day:?}"
        );
        assert_eq!(day[0].container_node.as_deref(), Some("Notes/2026-06-18.md"));
        assert_eq!(
            day[0].links,
            vec![evidence("Files/2026-06-18 lunch.jpg")],
            "the picture shows it; it did not attend it"
        );
    }

    /// The measured risk: on the real vault, 45 of 97 days held nothing but a
    /// note and its pictures. Those days must not leave the strip.
    #[test]
    fn a_day_whose_only_trace_is_writing_stays_on_the_strip() {
        let db = DbBridge::new_in_memory_full().unwrap();
        for n in [
            node("Notes/2026-06-19.md", "note", "2026-06-19", json!({ "date": "2026-06-19" })),
            node("Files/2026-06-19 walk.jpg", "file", "2026-06-19 walk.jpg", json!({})),
        ] {
            db.upsert_node(&n).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let day = timeline.query(when::parse("2026-06-19").unwrap(), today()).unwrap();
        assert_eq!(
            day.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
            vec!["note"],
            "the day has writing on it and must still be findable: {day:?}"
        );
        assert_eq!(day[0].links, vec![evidence("Files/2026-06-19 walk.jpg")]);
    }

    /// The way back, for anybody who wants the flat list of dated nodes.
    #[test]
    fn switching_the_fold_off_brings_every_row_back() {
        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_string_lossy().to_string();
        let db = DbBridge::new_in_memory_full().unwrap();
        for n in [
            node("Notes/2026-06-18.md", "note", "2026-06-18", json!({ "date": "2026-06-18" })),
            node("Tasks/ship.md", "task", "Ship it", json!({ "completed_at": "2026-06-18" })),
        ] {
            db.upsert_node(&n).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();

        catch_up_in(&cache, &mut timeline, Some(&vault_path)).unwrap();
        assert_eq!(timeline.query(when::parse("2026-06-18").unwrap(), today()).unwrap().len(), 1);

        let mut config = fold::Config { fold: false, ..fold::Config::default() };
        fold::write_config(&vault_path, &mut config, chrono::Utc::now()).unwrap();
        catch_up_in(&cache, &mut timeline, Some(&vault_path)).unwrap();

        let flat = timeline.query(when::parse("2026-06-18").unwrap(), today()).unwrap();
        assert_eq!(
            flat.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
            vec!["note", "task_done"],
            "off means the old flat list, notes and all: {flat:?}"
        );
    }

    /// The same gate, on a real vault instead of a fixture. §8, Bước 3.
    ///
    /// ```text
    /// SYNABIT_CACHE=/tmp/copy/vault_cache.db \
    ///   cargo test --lib -- --ignored --nocapture on_a_real_vault
    /// ```
    ///
    /// Point it at a **copy**. It runs migrations on whatever it opens, and
    /// the file the app is using is not ours to migrate.
    #[test]
    #[ignore = "measurement; needs SYNABIT_CACHE naming a copy of a real vault_cache.db"]
    fn on_a_real_vault_folding_keeps_every_day_that_has_writing() {
        let Ok(path) = std::env::var("SYNABIT_CACHE") else {
            return;
        };
        let conn = Connection::open(&path).expect("the copied cache");
        let cache: DbState = Mutex::new(DbBridge::init_with_conn(conn).expect("the schema"));
        let vault = tempfile::tempdir().unwrap();
        let vault_path = vault.path().to_string_lossy().to_string();
        let days = |store: &TimelineStore| -> Vec<String> {
            let mut seen: Vec<String> = store
                .all_items(today())
                .unwrap()
                .into_iter()
                .map(|event| event.happened_from)
                .collect();
            seen.sort();
            seen.dedup();
            seen
        };

        let mut flat = TimelineStore::open_in_memory().unwrap();
        let mut off = fold::Config { fold: false, ..fold::Config::default() };
        fold::write_config(&vault_path, &mut off, chrono::Utc::now()).unwrap();
        let before = catch_up_in(&cache, &mut flat, Some(&vault_path)).unwrap();
        let was = days(&flat);

        let mut folded = TimelineStore::open_in_memory().unwrap();
        let mut on = fold::Config::default();
        fold::write_config(&vault_path, &mut on, chrono::Utc::now()).unwrap();
        let after = catch_up_in(&cache, &mut folded, Some(&vault_path)).unwrap();
        let now = days(&folded);

        let tally = |store: &TimelineStore| -> Vec<(String, usize)> {
            let mut counted: std::collections::BTreeMap<String, usize> = Default::default();
            for event in store.all_items(today()).unwrap() {
                *counted.entry(event.kind).or_default() += 1;
            }
            counted.into_iter().collect()
        };
        println!("rows  {} -> {}", before.items, after.items);
        println!("days  {} -> {}", was.len(), now.len());
        println!("kinds before {:?}", tally(&flat));
        println!("kinds after  {:?}", tally(&folded));
        let lost: Vec<&String> = was.iter().filter(|day| !now.contains(day)).collect();
        assert!(lost.is_empty(), "{} days left the strip: {lost:?}", lost.len());
    }

    /// The gate for Bước 5. Meeting somebody twenty times through a year puts
    /// that year in both their worldlines — and somebody nothing mentions gets
    /// no worldline at all, rather than an empty or a guessed one.
    #[test]
    fn two_people_who_keep_meeting_share_the_time_they_kept_meeting_in() {
        let meetings: Vec<serde_json::Value> = (1..=20)
            .map(|n| {
                let day = format!("2019-{:02}-{:02}", (n % 12) + 1, (n % 27) + 1);
                json!({ "title": format!("gặp lần {n}"), "happened": day, "people": ["uuid-a", "uuid-b"] })
            })
            .collect();
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node(
            "Notes/2019.md",
            "note",
            "2019-01-01",
            json!({ "date": "2019-01-01", "moments": meetings }),
        ))
        .unwrap();
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();

        let worldline = |person: &str| {
            let events = timeline.about(&[person], today()).unwrap();
            let spans: Vec<(&str, &str)> = events
                .iter()
                .map(|event| (event.happened_from.as_str(), event.happened_to.as_str()))
                .collect();
            crate::timeline::presence::merge(&spans, today())
        };

        for person in ["uuid-a", "uuid-b"] {
            let worldline = worldline(person);
            assert_eq!(worldline.len(), 1, "{person} was around once, at length: {worldline:?}");
            assert!(worldline[0].from_day.starts_with("2019"), "{worldline:?}");
            assert!(worldline[0].to_day.starts_with("2019"), "{worldline:?}");
            assert_eq!(worldline[0].events, 20);
        }

        assert!(
            worldline("uuid-nobody").is_empty(),
            "nothing names them, so nothing is claimed about them"
        );
    }

    /// Folding is a way of reading the index, not a hole punched in it. The
    /// day still has writing on it when the thing it shared a day with goes.
    #[test]
    fn a_day_with_writing_stays_when_what_it_shared_the_day_with_goes() {
        let db = DbBridge::new_in_memory_full().unwrap();
        for n in [
            node("Notes/2026-06-18.md", "note", "2026-06-18", json!({ "date": "2026-06-18" })),
            node("Tasks/ship.md", "task", "Ship it", json!({ "completed_at": "2026-06-18" })),
        ] {
            db.upsert_node(&n).unwrap();
        }
        let cache = Mutex::new(db);
        let mut timeline = TimelineStore::open_in_memory().unwrap();
        catch_up(&cache, &mut timeline).unwrap();
        let day = when::parse("2026-06-18").unwrap();
        assert_eq!(kinds(&timeline.query(day, today()).unwrap()), vec!["task_done"]);

        {
            let db = cache.lock().unwrap();
            db.delete_node("Tasks/ship.md").unwrap();
        }
        catch_up(&cache, &mut timeline).unwrap();

        assert_eq!(
            kinds(&timeline.query(day, today()).unwrap()),
            vec!["note"],
            "the note is still in the vault, so the day is still a day that was written on"
        );
    }

    /// "Links outlive nothing" — including across a derive-version bump, which
    /// is a real path: every device takes it after an update that changes how
    /// nodes are read.
    #[test]
    fn deriving_everything_again_leaves_no_link_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("timeline.db");
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node(
            "Notes/day.md",
            "note",
            "2016-05-14",
            json!({ "date": "2016-05-14", "moments": [{ "title": "Gặp", "happened": "2016-05-14", "people": ["uuid-a", "uuid-b"] }] }),
        ))
        .unwrap();
        let cache = Mutex::new(db);
        {
            let mut timeline = TimelineStore::open(&path).unwrap();
            catch_up(&cache, &mut timeline).unwrap();
            assert_eq!(link_count(&timeline), 2);
            // What an update looks like from here: the version this device
            // last derived with is no longer the one the code carries.
            timeline
                .conn
                .execute("UPDATE meta SET value = 'older' WHERE key = 'derive_version'", [])
                .unwrap();
        }

        let reopened = TimelineStore::open(&path).unwrap();
        assert_eq!(
            reopened
                .conn
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0,
            "the bump forgets every derived event"
        );
        assert_eq!(link_count(&reopened), 0, "and takes their links with them");
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

    fn kinds(items: &[Event]) -> Vec<&str> {
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

        for kind in ["note", "event", "interaction", "task_done", "experience"] {
            assert!(found.contains(&kind), "{kind} missing from {found:?}");
        }
        assert!(
            !found.contains(&"media"),
            "a picture is what shows a day happened, not a row beside it: {found:?}"
        );
        assert!(!items.iter().any(|i| i.title == "Tháng sáu"), "June is not May");
        assert!(!items.iter().any(|i| i.title == "Họp tuần"), "a series is not an item");
        assert_eq!(found.last(), Some(&"experience"), "the least precise comes last");

        // The picture the note holds is now evidence of that day's own row.
        let day = items
            .iter()
            .find(|i| i.kind == "note" && i.happened_from == "2016-05-14")
            .expect("the day that was written on");
        assert_eq!(day.links, vec![evidence("Files/pic.md")]);
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
