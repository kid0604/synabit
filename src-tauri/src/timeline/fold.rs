//! Notes become boxes, pictures become evidence.
//!
//! A daily note is not a thing that happened; it is where several things that
//! happened were written down. A photograph is not a thing that happened
//! either; it is what shows one did. Listing all three side by side is why
//! fifty pictures of one trip were fifty rows and the trip was none
//! (`docs/su-kien-2026-09-16.md` §1).
//!
//! # What this pass does, and what it refuses to do
//!
//! Measured on the real vault on 2026-09-16, before any of this was written:
//! 243 rows over 97 days, and **45 of those days held nothing but notes and
//! pictures**. Extraction was off, so nothing else had been read out of the
//! writing. Deleting notes outright would have taken 46% of the days off the
//! strip — every one of them a day the person had actually written on.
//!
//! So the rule is not "a note is never an event". It is:
//!
//! - a day that has something else on it: the note steps back to
//!   [`container_node`] and stops being a row of its own;
//! - a day that has nothing else: the note keeps its row, which says no more
//!   than "there is writing from this day", and the day stays on the strip.
//!
//! Pictures attach to the largest event of their day, whichever of the two
//! kinds of row that turns out to be.
//!
//! # Marking, not deleting
//!
//! A folded row is marked `folded_into = <the event it went into>` and stays
//! in the table; the reading queries hide it. Deleting was tried first and was
//! wrong in three ways at once: catch-up derives only the nodes whose text
//! changed, so a row deleted on one pass was never rebuilt on the next — and a
//! day whose only other event was later removed left the timeline for good. It
//! took the picture gallery with it, which reads rows of kind `media`. And
//! turning folding off meant deriving the whole vault again to get the rows
//! back. Marking costs one column and none of that.
//!
//! # Getting back
//!
//! `Timeline/timeline.json` in the vault holds `{ "fold": false }` for anyone
//! who wants the old flat list. It is mirrored into `meta` in `timeline.db`
//! because deriving happens without a vault path in hand; the vault file is
//! the one that syncs, and the mirror is refreshed from it on every read that
//! knows where the vault is.
//!
//! [`container_node`]: super::store::Event::container_node

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::magnitude::{self, Signals};
use crate::error::{AppError, AppResult};

pub const CONFIG_FILE: &str = "Timeline/timeline.json";

/// The key the vault's answer is mirrored under, inside `timeline.db`.
const MIRROR: &str = "fold_days";

/// Rows that are not events of their own: what this pass folds away.
const CONTAINERS: &str = "'note'";
const EVIDENCE: &str = "'media'";

/// Rows nothing folds into, and rows folding never reads: a proposal is not
/// on the timeline until somebody accepts it.
const VISIBLE: &str = "superseded_by IS NULL AND sealed = 0 AND source != 'extract'";

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("timeline fold: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    /// On unless turned off. Off gives back the flat list of every dated node.
    #[serde(default = "on")]
    pub fold: bool,
    #[serde(flatten, default)]
    pub rest: Map<String, Value>,
}

fn on() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config { fold: true, rest: Map::new() }
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

/// Bring the device's copy of the answer in line with the vault's.
pub fn mirror(conn: &Connection, vault_path: &str) -> AppResult<bool> {
    let on = read_config(vault_path).fold;
    let was = folding(conn);
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params![MIRROR, if on { "1" } else { "0" }],
    )
    .map_err(sql)?;
    // Turning it off has to put back rows this pass deleted, so the caller is
    // told to derive the whole thing again rather than catch up.
    Ok(was != on)
}

/// What the device last heard. On when it has heard nothing.
pub fn folding(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![MIRROR],
        |r| r.get::<_, String>(0),
    )
    .map(|value| value != "0")
    .unwrap_or(true)
}

/// Fold a day's notes and pictures into what happened that day.
///
/// Runs inside the catch-up transaction, after every node has been derived and
/// after pictures have been given the day of the note they sit in.
pub fn fold_days(tx: &Transaction, folding: bool) -> AppResult<()> {
    // This pass is the whole truth about what is inside what, so it starts
    // from nothing folded. That is also what makes it idempotent: running it
    // twice says the same thing, and running it after the switch went off
    // leaves every row standing on its own again.
    tx.execute("UPDATE events SET folded_into = NULL WHERE folded_into IS NOT NULL", [])
        .map_err(sql)?;
    if !folding {
        return resize(tx);
    }

    // A note steps back to being the box: the events of that day gain it as
    // where they were written down.
    tx.execute(
        &format!(
            "UPDATE events SET container_node = (
                SELECT n.node_id FROM events n
                WHERE n.kind IN ({CONTAINERS}) AND n.happened_from = events.happened_from AND n.{VISIBLE}
                ORDER BY n.id LIMIT 1)
             WHERE kind NOT IN ({CONTAINERS}, {EVIDENCE}) AND container_node IS NULL AND {VISIBLE}
               AND EXISTS (SELECT 1 FROM events n WHERE n.kind IN ({CONTAINERS})
                           AND n.happened_from = events.happened_from AND n.{VISIBLE})"
        ),
        [],
    )
    .map_err(sql)?;

    // …and stops being a row of its own, on a day that holds something else.
    tx.execute(
        &format!(
            "UPDATE events SET folded_into = (
                SELECT e.id FROM events e
                WHERE e.happened_from = events.happened_from
                  AND e.kind NOT IN ({CONTAINERS}, {EVIDENCE}) AND e.{VISIBLE}
                ORDER BY e.magnitude DESC, e.id LIMIT 1)
             WHERE kind IN ({CONTAINERS}) AND {VISIBLE}
               AND EXISTS (SELECT 1 FROM events e
                           WHERE e.happened_from = events.happened_from
                             AND e.kind NOT IN ({CONTAINERS}, {EVIDENCE}) AND e.{VISIBLE})"
        ),
        [],
    )
    .map_err(sql)?;

    // A picture joins the largest thing left standing on its day. Largest, not
    // first, so a wedding takes the photographs rather than the lunch before
    // it; and a day whose only row is the note's mark gives them to that.
    let pictures: Vec<(String, String, String)> = {
        let mut stmt = tx
            .prepare(&format!(
                "SELECT id, node_id, happened_from FROM events
                 WHERE kind IN ({EVIDENCE}) AND {VISIBLE} ORDER BY id"
            ))
            .map_err(sql)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(sql)?;
        rows.flatten().collect()
    };

    for (id, node_id, day) in pictures {
        let host: Option<String> = tx
            .query_row(
                &format!(
                    "SELECT id FROM events
                     WHERE happened_from = ?1 AND kind NOT IN ({EVIDENCE})
                       AND folded_into IS NULL AND {VISIBLE}
                     ORDER BY magnitude DESC, id LIMIT 1"
                ),
                params![day],
                |r| r.get(0),
            )
            .ok();
        let Some(host) = host else {
            // Nothing else is known about that day. The picture is all there
            // is, so it keeps standing on its own.
            continue;
        };
        tx.execute(
            "INSERT OR REPLACE INTO event_links (event_id, node_id, role, label)
             VALUES (?1, ?2, 'evidence', NULL)",
            params![host, node_id],
        )
        .map_err(sql)?;
        tx.execute(
            "UPDATE events SET folded_into = ?2 WHERE id = ?1",
            params![id, host],
        )
        .map_err(sql)?;
    }

    resize(tx)
}

/// Work out every event's size again, now that folding has changed who names
/// what. An event that took a day's photographs is larger than it was.
///
/// The whole table, because it is small: 243 rows on the vault this was
/// measured against. If it ever is not, narrow it to the days folding touched.
fn resize(tx: &Transaction) -> AppResult<()> {
    let rows: Vec<(String, String, String, String, Option<String>, String)> = {
        let mut stmt = tx
            .prepare("SELECT id, happened_from, happened_to, title, label, source FROM events")
            .map_err(sql)?;
        let found = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })
            .map_err(sql)?;
        found.flatten().collect()
    };
    for (id, from, to, title, label, source) in rows {
        let count = |role: &str| -> usize {
            tx.query_row(
                "SELECT COUNT(*) FROM event_links WHERE event_id = ?1 AND role = ?2",
                params![id, role],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as usize
        };
        let text = match &label {
            Some(label) => format!("{title} {label}"),
            None => title.clone(),
        };
        let size = magnitude::of(Signals {
            from: &from,
            to: &to,
            people: count("with"),
            evidence: count("evidence"),
            text: &text,
            source: &source,
        });
        tx.execute("UPDATE events SET magnitude = ?2 WHERE id = ?1", params![id, size])
            .map_err(sql)?;
    }
    Ok(())
}
