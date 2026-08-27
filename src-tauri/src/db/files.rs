//! Where files are, what is written in them, and which folders to look in.
//!
//! Nothing here touches the legacy `files` table any more. It has one reader
//! left in the whole app — the schema migration that copies its rows into
//! `nodes` once, on the first launch after upgrading — and every method that
//! wrote to it went with the Drive browser that was its only writer.

use super::DbBridge;
use crate::error::{AppError, AppResult};
use crate::models::file::{FileMetadata, FileSource};
use rusqlite::params;

impl DbBridge {
    pub fn upsert_file_source(&self, source: &FileSource) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO file_sources (id, path, name) 
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET 
                name=excluded.name",
                params![source.id, source.path, source.name],
            )
            .map_err(|e| AppError::General(format!("DB Upsert File Source Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_file_sources(&self) -> AppResult<Vec<FileSource>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, name FROM file_sources")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(FileSource {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    name: row.get(2)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut sources = Vec::new();
        for s in rows.flatten() {
            sources.push(s);
        }
        Ok(sources)
    }

    pub fn delete_file_source(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM file_sources WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Source Error: {}", e)))?;
        Ok(())
    }








}

/// One place a copy of an indexed file was last seen.
#[derive(Debug, Clone, PartialEq)]
pub struct FileLocation {
    pub abs_path: String,
    pub node_id: String,
    pub size: i64,
    pub mtime_ms: i64,
}

impl DbBridge {
    /// Record where a file is, and which node it belongs to.
    ///
    /// Keyed by path: re-scanning the same path updates the row rather than
    /// adding a second one. A file whose contents changed therefore *moves* to
    /// a different node here, which is exactly right — it is no longer the same
    /// item.
    pub fn upsert_file_location(&self, loc: &FileLocation, seen_at: i64) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO file_locations (abs_path, node_id, size, mtime_ms, seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(abs_path) DO UPDATE SET
                    node_id=excluded.node_id,
                    size=excluded.size,
                    mtime_ms=excluded.mtime_ms,
                    seen_at=excluded.seen_at",
                params![loc.abs_path, loc.node_id, loc.size, loc.mtime_ms, seen_at],
            )
            .map_err(|e| AppError::General(format!("DB Upsert File Location Error: {}", e)))?;
        Ok(())
    }

    /// Every place this node's contents can currently be found.
    pub fn file_locations_for_node(&self, node_id: &str) -> AppResult<Vec<FileLocation>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT abs_path, node_id, size, mtime_ms FROM file_locations
                 WHERE node_id = ?1 ORDER BY abs_path",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (locations): {}", e)))?;
        let rows = stmt
            .query_map(params![node_id], |row| {
                Ok(FileLocation {
                    abs_path: row.get(0)?,
                    node_id: row.get(1)?,
                    size: row.get(2)?,
                    mtime_ms: row.get(3)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (locations): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    /// The node a given path currently belongs to, if it is indexed at all.
    pub fn file_location_at(&self, abs_path: &str) -> AppResult<Option<FileLocation>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT abs_path, node_id, size, mtime_ms FROM file_locations WHERE abs_path = ?1",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (location): {}", e)))?;
        let mut rows = stmt
            .query_map(params![abs_path], |row| {
                Ok(FileLocation {
                    abs_path: row.get(0)?,
                    node_id: row.get(1)?,
                    size: row.get(2)?,
                    mtime_ms: row.get(3)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (location): {}", e)))?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn delete_file_location(&self, abs_path: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM file_locations WHERE abs_path = ?1",
                params![abs_path],
            )
            .map_err(|e| AppError::General(format!("DB Delete Location Error: {}", e)))?;
        Ok(())
    }

    /// Paths recorded under a folder, so a scan can tell what has gone.
    pub fn file_locations_under(&self, prefix: &str) -> AppResult<Vec<FileLocation>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT abs_path, node_id, size, mtime_ms FROM file_locations
                 WHERE abs_path LIKE ?1 ESCAPE '\\'",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (locations under): {}", e)))?;
        // `_` and `%` in a folder name would otherwise act as wildcards.
        let pattern = format!(
            "{}%",
            prefix
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok(FileLocation {
                    abs_path: row.get(0)?,
                    node_id: row.get(1)?,
                    size: row.get(2)?,
                    mtime_ms: row.get(3)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (locations under): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    /// Does any copy of this node's contents still exist on this device?
    pub fn node_has_locations(&self, node_id: &str) -> bool {
        self.conn
            .query_row(
                "SELECT 1 FROM file_locations WHERE node_id = ?1 LIMIT 1",
                params![node_id],
                |_| Ok(()),
            )
            .is_ok()
    }

    // ─── Stat cache ────────────────────────────────────────────

    /// The digest recorded for a path, with the stats it was taken under.
    pub fn cached_content_hash(&self, abs_path: &str) -> Option<(u64, i64, String)> {
        self.conn
            .query_row(
                "SELECT size, mtime_ms, content_hash FROM file_stat_cache WHERE abs_path = ?1",
                params![abs_path],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)? as u64,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .ok()
    }

    pub fn remember_content_hash(
        &self,
        abs_path: &str,
        size: u64,
        mtime_ms: i64,
        content_hash: &str,
        now: i64,
    ) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO file_stat_cache (abs_path, size, mtime_ms, content_hash, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(abs_path) DO UPDATE SET
                    size=excluded.size,
                    mtime_ms=excluded.mtime_ms,
                    content_hash=excluded.content_hash,
                    updated_at=excluded.updated_at",
                params![abs_path, size as i64, mtime_ms, content_hash, now],
            )
            .map_err(|e| AppError::General(format!("DB Stat Cache Error: {}", e)))?;
        Ok(())
    }

    pub fn forget_content_hash(&self, abs_path: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM file_stat_cache WHERE abs_path = ?1",
                params![abs_path],
            )
            .map_err(|e| AppError::General(format!("DB Stat Cache Delete Error: {}", e)))?;
        Ok(())
    }
}

impl DbBridge {
    /// Every copy of every indexed file that is on this device right now,
    /// each paired with the node describing what it is.
    ///
    /// One row per *location*, not per node. Two copies of one photo are one
    /// item as far as tags are concerned and two entries as far as a file
    /// browser is concerned, and both of those are true at once — so the list
    /// shows both, and they share an id because they share an identity.
    pub fn indexed_files(&self) -> AppResult<Vec<(FileLocation, crate::models::node::NodeMetadata)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT l.abs_path, l.node_id, l.size, l.mtime_ms,
                        n.node_type, n.title, n.content, n.properties, n.created_at, n.updated_at, n.timestamp
                 FROM file_locations l
                 JOIN nodes n ON n.id = l.node_id
                 ORDER BY n.updated_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (indexed files): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(7)?;
                Ok((
                    FileLocation {
                        abs_path: row.get(0)?,
                        node_id: row.get(1)?,
                        size: row.get(2)?,
                        mtime_ms: row.get(3)?,
                    },
                    crate::models::node::NodeMetadata {
                        id: row.get(1)?,
                        node_type: row.get(4)?,
                        title: row.get(5)?,
                        content: row.get(6)?,
                        properties: serde_json::from_str(&props_str)
                            .unwrap_or(serde_json::Value::Null),
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                        timestamp: row.get(10)?,
                        blocks: None,
                    },
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (indexed files): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    /// Nodes with more than one copy on this device, newest first.
    ///
    /// Duplicate detection used to mean hashing the whole library on demand.
    /// Now that identity *is* the digest, two copies of a file already share a
    /// node id and this is a `GROUP BY`.
    pub fn duplicate_locations(&self) -> AppResult<Vec<(String, Vec<FileLocation>)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT abs_path, node_id, size, mtime_ms FROM file_locations
                 WHERE node_id IN (
                    SELECT node_id FROM file_locations GROUP BY node_id HAVING COUNT(*) > 1
                 )
                 ORDER BY node_id, abs_path",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (duplicates): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(FileLocation {
                    abs_path: row.get(0)?,
                    node_id: row.get(1)?,
                    size: row.get(2)?,
                    mtime_ms: row.get(3)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (duplicates): {}", e)))?;

        let mut grouped: Vec<(String, Vec<FileLocation>)> = Vec::new();
        for loc in rows.flatten() {
            match grouped.last_mut() {
                Some((node_id, group)) if *node_id == loc.node_id => group.push(loc),
                _ => grouped.push((loc.node_id.clone(), vec![loc])),
            }
        }
        Ok(grouped)
    }
}

/// How reading a file's words turned out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextStatus {
    /// Words were found and stored.
    Indexed,
    /// The format has none to give — an image, a video, an archive.
    Unsupported,
    /// Something went wrong that trying again will not fix.
    Failed,
}

impl TextStatus {
    fn as_str(self) -> &'static str {
        match self {
            TextStatus::Indexed => "indexed",
            TextStatus::Unsupported => "unsupported",
            TextStatus::Failed => "failed",
        }
    }
}

impl DbBridge {
    /// File nodes whose words have not been read yet, newest first.
    ///
    /// The queue is defined by absence: anything with a row in
    /// `file_text_state` is settled, whatever the outcome was. That keeps the
    /// job monotonic — it can only ever shrink — and means a crash halfway
    /// through costs the batch in flight and nothing else.
    ///
    /// Newest first because the file somebody just added is the one they are
    /// most likely to search for next.
    pub fn files_awaiting_text(&self, limit: usize) -> AppResult<Vec<(String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT n.id,
                        json_extract(n.properties, '$.extension'),
                        (SELECT l.abs_path FROM file_locations l WHERE l.node_id = n.id LIMIT 1)
                 FROM nodes n
                 WHERE n.node_type = 'file'
                   AND n.id NOT IN (SELECT node_id FROM file_text_state)
                 ORDER BY n.updated_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (text queue): {}", e)))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (text queue): {}", e)))?;

        // A node with no location has no file on this device to read.
        Ok(rows
            .flatten()
            .filter(|(_, _, path)| !path.is_empty())
            .collect())
    }

    /// How many file nodes are still waiting.
    pub fn files_awaiting_text_count(&self) -> AppResult<usize> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM nodes n
                 WHERE n.node_type = 'file'
                   AND n.id NOT IN (SELECT node_id FROM file_text_state)
                   AND EXISTS (SELECT 1 FROM file_locations l WHERE l.node_id = n.id)",
                [],
                |row| row.get::<_, i64>(0).map(|n| n as usize),
            )
            .map_err(|e| AppError::General(format!("DB Query Error (text queue count): {}", e)))
    }

    /// Store the words of one file, replacing anything held for it before.
    pub fn store_file_text(&self, node_id: &str, pages: &[String]) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM file_text WHERE node_id = ?1", params![node_id])
            .map_err(|e| AppError::General(format!("DB Delete File Text Error: {}", e)))?;

        for (index, text) in pages.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            // One-based, because it is shown to a person. A format with no
            // pages hands over a single entry, which is then page 1 — the only
            // page it has.
            self.conn
                .execute(
                    "INSERT INTO file_text (node_id, page, text) VALUES (?1, ?2, ?3)",
                    params![node_id, (index + 1) as i64, text],
                )
                .map_err(|e| AppError::General(format!("DB Insert File Text Error: {}", e)))?;
        }
        Ok(())
    }

    pub fn record_text_status(
        &self,
        node_id: &str,
        status: TextStatus,
        pages: usize,
        chars: usize,
        now: i64,
    ) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO file_text_state (node_id, status, pages, chars, extracted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(node_id) DO UPDATE SET
                    status=excluded.status,
                    pages=excluded.pages,
                    chars=excluded.chars,
                    extracted_at=excluded.extracted_at",
                params![node_id, status.as_str(), pages as i64, chars as i64, now],
            )
            .map_err(|e| AppError::General(format!("DB Text Status Error: {}", e)))?;
        Ok(())
    }

    /// Everything held for one file, joined for the search index.
    pub fn file_text_joined(&self, node_id: &str) -> AppResult<String> {
        let mut stmt = self
            .conn
            .prepare("SELECT text FROM file_text WHERE node_id = ?1 ORDER BY page")
            .map_err(|e| AppError::General(format!("DB Query Error (file text): {}", e)))?;
        let rows = stmt
            .query_map(params![node_id], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::General(format!("DB Map Error (file text): {}", e)))?;
        Ok(rows.flatten().collect::<Vec<_>>().join(" "))
    }

    /// The first page of a document containing every one of these words.
    ///
    /// Deliberately not an FTS query. The index answers "which document", and
    /// this answers "whereabouts in it" — a much smaller question, over one
    /// document's rows, where a scan is cheaper than a second index would be.
    pub fn first_page_matching(&self, node_id: &str, words: &[String]) -> AppResult<Option<i64>> {
        if words.is_empty() {
            return Ok(None);
        }
        let mut stmt = self
            .conn
            .prepare("SELECT page, text FROM file_text WHERE node_id = ?1 ORDER BY page")
            .map_err(|e| AppError::General(format!("DB Query Error (page match): {}", e)))?;
        let rows = stmt
            .query_map(params![node_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (page match): {}", e)))?;

        let needles: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        for (page, text) in rows.flatten() {
            let haystack = text.to_lowercase();
            if needles.iter().all(|w| haystack.contains(w)) {
                return Ok(Some(page));
            }
        }
        Ok(None)
    }

    pub fn forget_file_text(&self, node_id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM file_text WHERE node_id = ?1", params![node_id])
            .map_err(|e| AppError::General(format!("DB Delete File Text Error: {}", e)))?;
        self.conn
            .execute(
                "DELETE FROM file_text_state WHERE node_id = ?1",
                params![node_id],
            )
            .map_err(|e| AppError::General(format!("DB Delete Text State Error: {}", e)))?;
        Ok(())
    }
}

impl DbBridge {
    /// A short passage from a document, around the first place a term appears.
    ///
    /// For telling somebody *why* a file matched. The search index has
    /// `snippet()` for the same job and does it better, with the matched words
    /// marked up; this is for the callers that go through `search_files_filtered`
    /// rather than through FTS — the assistant's file tool, mainly — and needs
    /// no query grammar to work.
    pub fn file_text_excerpt(&self, node_id: &str, needle: &str, radius: usize) -> Option<String> {
        if needle.is_empty() {
            return None;
        }
        let mut stmt = self
            .conn
            .prepare("SELECT text FROM file_text WHERE node_id = ?1 ORDER BY page")
            .ok()?;
        let rows = stmt
            .query_map(params![node_id], |row| row.get::<_, String>(0))
            .ok()?;

        let lowered = needle.to_lowercase();
        for text in rows.flatten() {
            let Some(at) = text.to_lowercase().find(&lowered) else {
                continue;
            };
            // Widened to character boundaries, which in a Vietnamese document
            // are rarely where a byte offset lands.
            let mut start = at.saturating_sub(radius);
            while start > 0 && !text.is_char_boundary(start) {
                start -= 1;
            }
            let mut end = (at + needle.len() + radius).min(text.len());
            while end < text.len() && !text.is_char_boundary(end) {
                end += 1;
            }
            let mut excerpt = String::new();
            if start > 0 {
                excerpt.push('…');
            }
            excerpt.push_str(&text[start..end]);
            if end < text.len() {
                excerpt.push('…');
            }
            return Some(excerpt);
        }
        None
    }
}

impl DbBridge {
    /// The distinct values a property takes across indexed files, sorted.
    ///
    /// For building a filter's options out of what is actually there, rather
    /// than showing the user a list of cameras they do not own.
    pub fn distinct_file_property(&self, field: &str) -> AppResult<Vec<String>> {
        // The field is chosen by this crate, never by a caller, but the value
        // still goes through a bound parameter rather than into the SQL text.
        let path = format!("$.{field}");
        let mut stmt = self
            .conn
            .prepare(
                "SELECT DISTINCT json_extract(properties, ?1) AS value
                 FROM nodes
                 WHERE node_type = 'file' AND value IS NOT NULL AND value != ''
                 ORDER BY value",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (distinct): {}", e)))?;
        let rows = stmt
            .query_map(params![path], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::General(format!("DB Map Error (distinct): {}", e)))?;
        Ok(rows.flatten().collect())
    }
}

/// A file the vault can see but does not hold.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteEntry {
    pub node_id: String,
    pub provider: String,
    pub remote_id: String,
    pub account: String,
    pub size: i64,
    pub modified_at: String,
    pub web_url: String,
}

impl DbBridge {
    pub fn upsert_remote_file(&self, entry: &RemoteEntry, seen_at: i64) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO remote_files (node_id, provider, remote_id, account, size, modified_at, web_url, seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(node_id) DO UPDATE SET
                    provider=excluded.provider,
                    remote_id=excluded.remote_id,
                    account=excluded.account,
                    size=excluded.size,
                    modified_at=excluded.modified_at,
                    web_url=excluded.web_url,
                    seen_at=excluded.seen_at",
                params![
                    entry.node_id, entry.provider, entry.remote_id, entry.account,
                    entry.size, entry.modified_at, entry.web_url, seen_at
                ],
            )
            .map_err(|e| AppError::General(format!("DB Upsert Remote File Error: {}", e)))?;
        Ok(())
    }

    /// Every remote file, paired with the node describing it.
    pub fn remote_files(&self) -> AppResult<Vec<(RemoteEntry, crate::models::node::NodeMetadata)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT r.node_id, r.provider, r.remote_id, r.account, r.size, r.modified_at, r.web_url,
                        n.node_type, n.title, n.content, n.properties, n.created_at, n.updated_at, n.timestamp
                 FROM remote_files r
                 JOIN nodes n ON n.id = r.node_id
                 ORDER BY n.updated_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (remote files): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let props: String = row.get(10)?;
                Ok((
                    RemoteEntry {
                        node_id: row.get(0)?,
                        provider: row.get(1)?,
                        remote_id: row.get(2)?,
                        account: row.get(3)?,
                        size: row.get(4)?,
                        modified_at: row.get(5)?,
                        web_url: row.get(6)?,
                    },
                    crate::models::node::NodeMetadata {
                        id: row.get(0)?,
                        node_type: row.get(7)?,
                        title: row.get(8)?,
                        content: row.get(9)?,
                        properties: serde_json::from_str(&props)
                            .unwrap_or(serde_json::Value::Null),
                        created_at: row.get(11)?,
                        updated_at: row.get(12)?,
                        timestamp: row.get(13)?,
                        blocks: None,
                    },
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (remote files): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    pub fn remote_file(&self, node_id: &str) -> AppResult<Option<RemoteEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT node_id, provider, remote_id, account, size, modified_at, web_url
                 FROM remote_files WHERE node_id = ?1",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (remote file): {}", e)))?;
        let mut rows = stmt
            .query_map(params![node_id], |row| {
                Ok(RemoteEntry {
                    node_id: row.get(0)?,
                    provider: row.get(1)?,
                    remote_id: row.get(2)?,
                    account: row.get(3)?,
                    size: row.get(4)?,
                    modified_at: row.get(5)?,
                    web_url: row.get(6)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (remote file): {}", e)))?;
        Ok(rows.next().and_then(|r| r.ok()))
    }

    /// Forget what a provider no longer lists, and the nodes behind it.
    ///
    /// Anything the listing did not mention has gone from the account, and a
    /// remote entry describes nothing on this device — so unlike a local file,
    /// there is nothing worth keeping around in case it comes back.
    pub fn prune_remote_files(&self, provider: &str, keep: &[String]) -> AppResult<usize> {
        let existing: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT node_id FROM remote_files WHERE provider = ?1")
                .map_err(|e| AppError::General(format!("DB Query Error (prune): {}", e)))?;
            let rows = stmt
                .query_map(params![provider], |row| row.get::<_, String>(0))
                .map_err(|e| AppError::General(format!("DB Map Error (prune): {}", e)))?;
            rows.flatten().collect()
        };

        let keeping: std::collections::HashSet<&String> = keep.iter().collect();
        let mut dropped = 0;
        for node_id in existing {
            if keeping.contains(&node_id) {
                continue;
            }
            self.conn
                .execute("DELETE FROM remote_files WHERE node_id = ?1", params![node_id])
                .map_err(|e| AppError::General(format!("DB Delete Remote Error: {}", e)))?;
            self.delete_node(&node_id)?;
            self.delete_search_entry(&node_id);
            dropped += 1;
        }
        Ok(dropped)
    }

    /// Drop everything a provider contributed — disconnecting an account.
    pub fn forget_provider(&self, provider: &str) -> AppResult<usize> {
        self.prune_remote_files(provider, &[])
    }
}

/// What the list is currently showing, as a question the database can answer.
///
/// Every field is a narrowing. All of them are optional, and an empty filter
/// means the whole library.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFilter {
    /// Files under one registered folder.
    pub source_path: Option<String>,
    /// Files from one provider — `local`, `gdrive`.
    pub source_kind: Option<String>,
    /// The extensions a category covers. The category → extensions mapping
    /// lives in the front end, which is where the category list is defined;
    /// duplicating it here would give the app two answers to one question.
    pub extensions: Option<Vec<String>>,
    pub tag: Option<String>,
    pub camera: Option<String>,
    pub label: Option<String>,
    /// A substring of the filename.
    ///
    /// What the list falls back to while a full-text search is still being
    /// answered, and what it uses if that search fails. Without it, typing into
    /// the search box showed the unfiltered library until the index replied —
    /// which reads as the search doing nothing.
    pub name_contains: Option<String>,
    /// Node ids from a full-text search, in rank order.
    pub search_ids: Option<Vec<String>>,
}

/// How the list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileSort {
    Modified,
    Name,
    Size,
    Shot,
    Pixels,
    /// The order a search returned, which is the ranking it computed.
    Relevance,
}

impl Default for FileSort {
    fn default() -> Self {
        Self::Modified
    }
}

/// A slice of the list, and how long the list is.
#[derive(Debug, serde::Serialize)]
pub struct FilePage {
    pub files: Vec<FileMetadata>,
    /// Rows matching the filter, not rows returned.
    pub total: usize,
}

impl DbBridge {
    /// One window onto the filtered library.
    ///
    /// The whole list used to come back in a single array, which the front end
    /// then filtered in the browser. It worked until it did not: fifty thousand
    /// files is fourteen megabytes of JSON across the IPC bridge before the
    /// webview has parsed a byte of it, on every open. Narrowing and ordering
    /// belong where the rows are.
    ///
    /// One row per *location*, as before — two copies of a photo are two things
    /// to browse and one thing to tag.
    pub fn query_file_page(
        &self,
        filter: &FileFilter,
        sort: FileSort,
        descending: bool,
        offset: usize,
        limit: usize,
    ) -> AppResult<FilePage> {
        let (where_sql, params) = self.build_filter(filter)?;

        let total: i64 = {
            let sql = format!(
                "SELECT COUNT(*) FROM file_locations l
                 JOIN nodes n ON n.id = l.node_id
                 WHERE {where_sql}"
            );
            let mut stmt = self
                .conn
                .prepare(&sql)
                .map_err(|e| AppError::General(format!("DB Query Error (count): {}", e)))?;
            stmt.query_row(rusqlite::params_from_iter(params.iter()), |row| row.get(0))
                .map_err(|e| AppError::General(format!("DB Count Error: {}", e)))?
        };

        let direction = if descending { "DESC" } else { "ASC" };
        // A search's own ranking is the order it computed; re-sorting by name
        // would throw that away. `search_ids` arrives in rank order, and the
        // position of each id is carried in the ordering column below.
        let order = match sort {
            FileSort::Relevance => "rank_position ASC".to_string(),
            FileSort::Name => format!("n.title COLLATE NOCASE {direction}"),
            FileSort::Size => format!("l.size {direction}"),
            FileSort::Shot => format!(
                "COALESCE(json_extract(n.properties, '$.shot_at'), '') {direction}"
            ),
            FileSort::Pixels => format!(
                "(COALESCE(json_extract(n.properties, '$.width'), 0) *
                  COALESCE(json_extract(n.properties, '$.height'), 0)) {direction}"
            ),
            FileSort::Modified => format!("n.updated_at {direction}"),
        };

        let rank_column = match &filter.search_ids {
            Some(ids) if !ids.is_empty() => {
                // A CASE ladder rather than a temporary table: the list is one
                // page of search results, not the whole library.
                let arms: String = ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| format!("WHEN {} THEN {i}", quote(id)))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("CASE n.id {arms} ELSE {} END", ids.len())
            }
            _ => "0".to_string(),
        };

        let sql = format!(
            "SELECT l.abs_path, l.size, n.id, n.title, n.properties, n.created_at, n.updated_at,
                    {rank_column} AS rank_position
             FROM file_locations l
             JOIN nodes n ON n.id = l.node_id
             WHERE {where_sql}
             ORDER BY {order}, l.abs_path
             LIMIT {limit} OFFSET {offset}"
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| AppError::General(format!("DB Query Error (page): {}", e)))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                let props: String = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    crate::models::node::NodeMetadata {
                        id: row.get(2)?,
                        node_type: "file".to_string(),
                        title: row.get(3)?,
                        content: String::new(),
                        properties: serde_json::from_str(&props)
                            .unwrap_or(serde_json::Value::Null),
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                        timestamp: 0,
                        blocks: None,
                    },
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (page): {}", e)))?;

        let files = rows
            .flatten()
            .filter_map(|(abs_path, size, node)| {
                let mut meta = FileMetadata::from_node(&node)?;
                meta.path = abs_path;
                meta.size = size;
                Some(meta)
            })
            .collect();

        Ok(FilePage {
            files,
            total: total as usize,
        })
    }

    /// Every identity the filter matches, deduplicated.
    ///
    /// For "select all", which is about the whole filtered set rather than the
    /// page on screen — and for tagging, which works on identities, so two
    /// copies of one photo count once.
    pub fn query_file_ids(&self, filter: &FileFilter, limit: usize) -> AppResult<Vec<String>> {
        let (where_sql, params) = self.build_filter(filter)?;
        let sql = format!(
            "SELECT DISTINCT n.id FROM file_locations l
             JOIN nodes n ON n.id = l.node_id
             WHERE {where_sql} LIMIT {limit}"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| AppError::General(format!("DB Query Error (ids): {}", e)))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| AppError::General(format!("DB Map Error (ids): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    /// The tags in use, with how many files carry each.
    ///
    /// Its own query now. The sidebar used to derive this by walking the whole
    /// list in the browser, which is exactly the thing the list stopped being.
    pub fn file_tag_counts(&self) -> AppResult<Vec<(String, usize)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT tag.value AS name, COUNT(*) AS uses
                 FROM nodes n, json_each(n.properties, '$.tags') AS tag
                 WHERE n.node_type = 'file'
                 GROUP BY name
                 ORDER BY name",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (tags): {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (tags): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    /// The `WHERE` clause for a filter, and the values it binds.
    fn build_filter(&self, filter: &FileFilter) -> AppResult<(String, Vec<String>)> {
        let mut clauses = vec!["n.node_type = 'file'".to_string()];
        let mut params: Vec<String> = Vec::new();

        if let Some(prefix) = filter.source_path.as_ref().filter(|p| !p.is_empty()) {
            clauses.push(format!("l.abs_path LIKE ?{} ESCAPE '\\'", params.len() + 1));
            params.push(format!(
                "{}%",
                prefix
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            ));
        }
        if let Some(kind) = filter.source_kind.as_ref().filter(|k| !k.is_empty()) {
            clauses.push(format!(
                "json_extract(n.properties, '$.source_type') = ?{}",
                params.len() + 1
            ));
            params.push(kind.clone());
        }
        if let Some(tag) = filter.tag.as_ref().filter(|t| !t.is_empty()) {
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM json_each(n.properties, '$.tags') WHERE value = ?{})",
                params.len() + 1
            ));
            params.push(tag.clone());
        }
        if let Some(needle) = filter.name_contains.as_ref().filter(|n| !n.is_empty()) {
            clauses.push(format!("n.title LIKE ?{} ESCAPE '\\'", params.len() + 1));
            params.push(format!(
                "%{}%",
                needle
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            ));
        }
        if let Some(camera) = filter.camera.as_ref().filter(|c| !c.is_empty()) {
            clauses.push(format!(
                "json_extract(n.properties, '$.camera') = ?{}",
                params.len() + 1
            ));
            params.push(camera.clone());
        }
        if let Some(label) = filter.label.as_ref().filter(|l| !l.is_empty()) {
            clauses.push(format!(
                "json_extract(n.properties, '$.label') = ?{}",
                params.len() + 1
            ));
            params.push(label.clone());
        }
        if let Some(extensions) = filter.extensions.as_ref().filter(|e| !e.is_empty()) {
            let placeholders: Vec<String> = extensions
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", params.len() + 1 + i))
                .collect();
            clauses.push(format!(
                "LOWER(json_extract(n.properties, '$.extension')) IN ({})",
                placeholders.join(", ")
            ));
            params.extend(extensions.iter().map(|e| e.to_lowercase()));
        }
        if let Some(ids) = filter.search_ids.as_ref() {
            if ids.is_empty() {
                // A search that matched nothing matches nothing. Without this
                // an empty result set would widen to the whole library.
                clauses.push("0 = 1".to_string());
            } else {
                let list: Vec<String> = ids.iter().map(|id| quote(id)).collect();
                clauses.push(format!("n.id IN ({})", list.join(", ")));
            }
        }

        Ok((clauses.join(" AND "), params))
    }
}

/// A string literal safe to put straight into SQL.
///
/// Used only for node ids, which this crate mints and which are already
/// constrained — but an id can reach here from a search result, so it is
/// quoted properly rather than trusted. Doubling the quote is SQLite's own
/// escape, and the surrounding quotes make the result a literal whatever is
/// inside it.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
