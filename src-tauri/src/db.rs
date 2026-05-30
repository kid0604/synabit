use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::error::{AppError, AppResult};
use crate::models::file::{FileMetadata, FileSource};
use crate::models::whiteboard::WhiteboardMetadata;

/// New ID-based edge for the knowledge graph
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String, // 'wikilink' | 'internal_link' | 'embed' | 'manual'
    pub relation: Option<String>, // 'references' | 'attachment' | 'related' | custom...
    pub created_at: String,
}

pub struct DbBridge {
    conn: Connection,
}

/// Thread-safe wrapper for Tauri managed state.
pub type DbState = Mutex<DbBridge>;

impl DbBridge {
    /// Initialize the database once at app startup. Runs all migrations.
    pub fn init(app_handle: &tauri::AppHandle) -> AppResult<Self> {
        use tauri::Manager;
        let app_data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| AppError::General(format!("Could not determine app data dir: {}", e)))?;

        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| AppError::General(format!("Failed to create app data dir: {}", e)))?;

        let db_path = app_data_dir.join("vault_cache.db");
        let conn = Connection::open(db_path)
            .map_err(|e| AppError::General(format!("DB Open Error: {}", e)))?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();

        // ─── One-time Legacy Cleanup ────────────────────────────
        // These tables were migrated to Universal Node Core in v0.2.x.
        // Only drop them once, then set a flag so we skip on future startups.
        {
            let already_cleaned: bool = conn
                .query_row(
                    "SELECT value FROM kv_store WHERE key = 'legacy_tables_cleaned'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map(|v| v == "1")
                .unwrap_or(false);

            if !already_cleaned {
                let _ = conn.execute("DROP TABLE IF EXISTS notes", []);
                let _ = conn.execute("DROP TABLE IF EXISTS events", []);
                let _ = conn.execute("DROP TABLE IF EXISTS tasks", []);
                let _ = conn.execute("DROP TABLE IF EXISTS quickcaps", []);
                // Flag will be set after kv_store table is created below
            }
        }

        // ─── Files Table ───────────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                filename TEXT NOT NULL,
                extension TEXT NOT NULL,
                size INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                modified_at TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '[]',
                source_type TEXT NOT NULL DEFAULT 'local'
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (files): {}", e)))?;

        // ─── File Sources Table ────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS file_sources (
                id TEXT PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (file_sources): {}", e)))?;

        // ─── Nodes Table (Universal Core) ────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS nodes (
                id TEXT PRIMARY KEY,
                node_type TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                properties TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (nodes): {}", e)))?;

        // ─── Nodes Indexes (for performance at scale) ────────────
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);
             CREATE INDEX IF NOT EXISTS idx_nodes_type_updated ON nodes(node_type, updated_at);
             CREATE INDEX IF NOT EXISTS idx_nodes_timestamp ON nodes(timestamp);",
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (nodes indexes): {}", e)))?;

        // ─── Node Blocks (for Block-Level Referencing) ──────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS node_blocks (
                block_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                content TEXT NOT NULL,
                PRIMARY KEY (block_id, node_id)
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (node_blocks): {}", e)))?;

        // ─── Whiteboards Table ─────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS whiteboards (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '[]',
                content TEXT NOT NULL DEFAULT '',
                path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                timestamp INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (whiteboards): {}", e)))?;

        // ─── KV Store (for OAuth tokens and settings) ──────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv_store (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (kv_store): {}", e)))?;

        // Mark legacy cleanup as done (now that kv_store exists)
        let _ = conn.execute(
            "INSERT OR IGNORE INTO kv_store (key, value) VALUES ('legacy_tables_cleaned', '1')",
            [],
        );

        // ─── Node Edges (NEW — ID-based knowledge graph) ────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS node_edges (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                edge_type TEXT NOT NULL,
                relation TEXT DEFAULT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(source_id, target_id, edge_type)
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (node_edges): {}", e)))?;

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_node_edges_source ON node_edges(source_id);
             CREATE INDEX IF NOT EXISTS idx_node_edges_target ON node_edges(target_id);
             CREATE INDEX IF NOT EXISTS idx_node_edges_type ON node_edges(edge_type);",
        )
        .map_err(|e| AppError::General(format!("DB Index Error (node_edges): {}", e)))?;

        // ─── CRDT Core Tables (Synabit V2) ──────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS crdt_documents (
                doc_id TEXT PRIMARY KEY,
                snapshot BLOB NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (crdt_documents): {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS crdt_updates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                doc_id TEXT NOT NULL,
                delta BLOB NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (crdt_updates): {}", e)))?;

        // ─── FTS5 Full-Text Search Index ────────────────────────
        // DROP + CREATE to ensure schema includes the `properties` column.
        // Data is repopulated by reindex_search() on every app startup.
        conn.execute_batch("DROP TABLE IF EXISTS search_index;")
            .map_err(|e| {
                AppError::General(format!("DB Schema Error (drop search_index): {}", e))
            })?;
        conn.execute_batch(
            "CREATE VIRTUAL TABLE search_index USING fts5(
                item_id,
                item_type,
                title,
                tags,
                content,
                properties,
                status UNINDEXED,
                date UNINDEXED,
                path UNINDEXED,
                tokenize = 'unicode61 remove_diacritics 0'
            );",
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (search_index): {}", e)))?;

        // ─── One-time: Migrate legacy `files` table → `nodes` ─────
        // Previous frontend-driven migration may have set the flag but created 0 nodes.
        // Re-run if nodes table has zero file entries despite files table having data.
        {
            let legacy_file_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
                .unwrap_or(0);
            let node_file_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE node_type = 'file'",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            if legacy_file_count > 0 && node_file_count == 0 {
                log::info!(
                    "Migrating {} legacy files to nodes table (SQL batch)...",
                    legacy_file_count
                );
                // Single SQL statement — no Rust iteration needed
                let result = conn.execute(
                    "INSERT OR IGNORE INTO nodes (id, node_type, title, content, properties, created_at, updated_at, timestamp)
                     SELECT id, 'file', filename, '',
                       json_object('path', path, 'extension', extension, 'size', size, 'source_type', source_type, 'tags', json(tags)),
                       created_at, modified_at, strftime('%s','now')
                     FROM files",
                    [],
                );
                match result {
                    Ok(count) => log::info!("Migrated {} files to nodes table.", count),
                    Err(e) => log::error!("Failed to migrate files to nodes: {}", e),
                }
            }
        }

        Ok(Self { conn })
    }

    // ═══════════════════════════════════════════════════════════
    //  KV STORE
    // ═══════════════════════════════════════════════════════════

    pub fn set_kv(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO kv_store (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )
            .map_err(|e| AppError::General(format!("DB Set KV Error: {}", e)))?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> AppResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM kv_store WHERE key = ?1")
            .map_err(|e| AppError::General(format!("DB Get KV Prepare Error: {}", e)))?;
        let mut rows = stmt
            .query(params![key])
            .map_err(|e| AppError::General(format!("DB Get KV Query Error: {}", e)))?;

        if let Some(row) = rows.next().unwrap_or(None) {
            Ok(Some(row.get(0).unwrap_or_default()))
        } else {
            Ok(None)
        }
    }

    pub fn delete_kv(&self, key: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM kv_store WHERE key = ?1", params![key])
            .map_err(|e| AppError::General(format!("DB Delete KV Error: {}", e)))?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════
    //  NODE BLOCKS
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_node_blocks(
        &self,
        node_id: &str,
        blocks: Vec<(String, String)>,
    ) -> AppResult<()> {
        // Use INSERT OR REPLACE to keep old block_ids from previous content versions.
        // This ensures that transclusion references to old block hashes still resolve,
        // even after the source content has been edited.
        let mut insert_stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO node_blocks (block_id, node_id, content) VALUES (?1, ?2, ?3)"
        ).map_err(|e| AppError::General(format!("DB Error preparing block upsert: {}", e)))?;

        let mut insert_fts_stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'block', ?2, '', ?3, '', '', '', ?4)"
        ).map_err(|e| AppError::General(format!("DB Error preparing block fts upsert: {}", e)))?;

        for (block_id, content) in blocks {
            let _ = insert_stmt.execute(params![&block_id, node_id, &content]);
            let item_id = format!("{}#{}", node_id, block_id);
            let _ = insert_fts_stmt.execute(params![item_id, block_id, content, node_id]);
        }

        Ok(())
    }

    pub fn delete_node_blocks(&self, node_id: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM node_blocks WHERE node_id = ?1",
                params![node_id],
            )
            .map_err(|e| AppError::General(format!("DB Error deleting blocks: {}", e)))?;
        let _ = self.conn.execute(
            "DELETE FROM search_index WHERE item_type = 'block' AND path = ?1",
            params![node_id],
        );
        Ok(())
    }

    pub fn get_node_block(&self, node_id: &str, block_id: &str) -> AppResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content FROM node_blocks WHERE node_id = ?1 AND block_id = ?2")
            .map_err(|e| AppError::General(format!("DB Error prepare get block: {}", e)))?;

        let mut rows = stmt
            .query(params![node_id, block_id])
            .map_err(|e| AppError::General(format!("DB Error querying block: {}", e)))?;

        if let Some(row) = rows.next().unwrap_or(None) {
            Ok(Some(row.get(0).unwrap_or_default()))
        } else {
            Ok(None)
        }
    }

    // ═══════════════════════════════════════════════════════════
    //  WHITEBOARDS
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_whiteboard(&self, wb: &WhiteboardMetadata) -> AppResult<()> {
        let tags_json = serde_json::to_string(&wb.tags)?;
        let timestamp = chrono::NaiveDateTime::parse_from_str(&wb.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO whiteboards (id, title, tags, content, path, created_at, updated_at, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title, tags=excluded.tags, content=excluded.content,
                path=excluded.path, updated_at=excluded.updated_at, timestamp=excluded.timestamp",
            params![wb.id, wb.title, tags_json, wb.content, wb.path, wb.created_at, wb.updated_at, timestamp],
        ).map_err(|e| AppError::General(format!("DB Upsert Whiteboard Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_whiteboard(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM whiteboards WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Whiteboard Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_whiteboard_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, timestamp FROM whiteboards")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
        let mut map = HashMap::new();
        for r in rows.flatten() {
            map.insert(r.0, r.1);
        }
        Ok(map)
    }

    // ═══════════════════════════════════════════════════════════
    //  FILES & SOURCES
    // ═══════════════════════════════════════════════════════════

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

    pub fn upsert_file(&self, file: &FileMetadata) -> AppResult<()> {
        let tags_json = serde_json::to_string(&file.tags)?;
        self.conn.execute(
            "INSERT INTO files (id, path, filename, extension, size, created_at, modified_at, tags, source_type) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET 
                filename=excluded.filename,
                extension=excluded.extension,
                size=excluded.size,
                modified_at=excluded.modified_at",
            params![
                file.id, file.path, file.filename, file.extension, file.size, 
                file.created_at, file.modified_at, tags_json, file.source_type
            ],
        ).map_err(|e| AppError::General(format!("DB Upsert File Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_file_by_path(&self, path: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM files WHERE path = ?1", params![path])
            .map_err(|e| AppError::General(format!("DB Delete File Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_files_by_source_type(&self, source_type: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM files WHERE source_type = ?1",
                params![source_type],
            )
            .map_err(|e| AppError::General(format!("DB Delete Files by Source Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_files(&self) -> AppResult<Vec<FileMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, filename, extension, size, created_at, modified_at, tags, source_type 
             FROM files ORDER BY modified_at DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let tags_str: String = row.get(7)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

                Ok(FileMetadata {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    extension: row.get(3)?,
                    size: row.get(4)?,
                    created_at: row.get(5)?,
                    modified_at: row.get(6)?,
                    tags,
                    people: vec![],
                    source_type: row.get(8)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut files = Vec::new();
        for f in rows.flatten() {
            files.push(f);
        }
        Ok(files)
    }

    pub fn update_file_tags(&self, path: &str, tags: Vec<String>) -> AppResult<()> {
        let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        self.conn
            .execute(
                "UPDATE files SET tags = ?1 WHERE path = ?2",
                params![tags_json, path],
            )
            .map_err(|e| AppError::General(format!("DB Update File Tags Error: {}", e)))?;
        Ok(())
    }

    /// Read tags from the legacy `files` table for a given path.
    /// Returns None if the file doesn't exist or the table is gone.
    pub fn get_legacy_file_tags(&self, path: &str) -> Option<Vec<String>> {
        let tags_str: String = self
            .conn
            .query_row(
                "SELECT tags FROM files WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .ok()?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        if tags.is_empty() {
            None
        } else {
            Some(tags)
        }
    }

    pub fn update_file_path_and_name(
        &self,
        old_path: &str,
        new_path: &str,
        new_filename: &str,
        extension: &str,
    ) -> AppResult<()> {
        self.conn
            .execute(
                "UPDATE files SET path = ?1, filename = ?2, extension = ?3 WHERE path = ?4",
                params![new_path, new_filename, extension, old_path],
            )
            .map_err(|e| AppError::General(format!("DB Rename File Error: {}", e)))?;
        Ok(())
    }

    /// Search all node content (notes, tasks, events, whiteboards) for references to a filename.
    /// Returns (id, node_type, title) for each referencing node.
    pub fn find_nodes_referencing_file(
        &self,
        filename: &str,
    ) -> AppResult<Vec<(String, String, String)>> {
        let pattern = format!("%{}%", filename);
        let mut stmt = self
            .conn
            .prepare("SELECT id, node_type, title FROM nodes WHERE content LIKE ?1")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    // ═══════════════════════════════════════════════════════════
    //  NEXUS — Unified search query (replaces 4x full scan)
    // ═══════════════════════════════════════════════════════════

    pub fn get_all_nexus_items(&self) -> AppResult<Vec<NexusRow>> {
        let mut items = Vec::new();

        // Note: Files are now stored in the `nodes` table (Universal Architecture)
        // and are queried below along with other node types.

        // Whiteboards
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, tags, content, path, created_at, updated_at FROM whiteboards",
            )
            .map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                let tags_str: String = row.get(2)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let content_json: String = row.get(3)?;
                let node_count = content_json.matches("\"id\"").count();
                let preview = format!("Whiteboard • {} nodes", node_count);
                // Extract text labels from nodes for searchable content
                let node_texts: String =
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content_json) {
                        parsed
                            .get("nodes")
                            .and_then(|n| n.as_array())
                            .map(|nodes| {
                                nodes
                                    .iter()
                                    .filter_map(|n| {
                                        n.get("data")
                                            .and_then(|d| d.get("label").and_then(|l| l.as_str()))
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" • ")
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                Ok(NexusRow {
                    id: row.get(0)?,
                    item_type: "whiteboard".to_string(),
                    title: row.get(1)?,
                    preview,
                    tags,
                    date: row.get(6)?,
                    path: row.get(4)?,
                    content: node_texts,
                    status: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() {
            items.push(r);
        }

        // Nodes (Universal Architecture)
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE node_type NOT LIKE 'finance_%'"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(4)?;
                let mut tags = Vec::new();
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&props_str) {
                    if let Some(t) = json_val.get("tags").and_then(|v| v.as_array()) {
                        tags = t
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                }
                let content: String = row.get(3)?;
                let preview: String = content.chars().take(150).collect();
                let node_type: String = row.get(1)?;
                Ok(NexusRow {
                    id: row.get(0)?,
                    item_type: node_type,
                    title: row.get(2)?,
                    preview,
                    tags,
                    date: row.get(5)?,
                    path: row.get(0)?,
                    content,
                    status: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() {
            items.push(r);
        }

        Ok(items)
    }

    /// Fast single-item lookup: determines the correct table from the ID prefix
    /// and runs a targeted `WHERE id = ?` query instead of scanning all tables.
    pub fn get_nexus_item_by_id(&self, id: &str) -> AppResult<Option<NexusRow>> {
        // Whiteboards
        if id.starts_with("Whiteboards/") || id.starts_with("whiteboard-") {
            let mut stmt = self.conn
                .prepare("SELECT id, title, tags, content, path, created_at, updated_at FROM whiteboards WHERE id = ?1")
                .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
            let mut rows = stmt
                .query_map(params![id], |row| {
                    let tags_str: String = row.get(2)?;
                    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                    let content_json: String = row.get(3)?;
                    let node_count = content_json.matches("\"id\"").count();
                    let preview = format!("Whiteboard • {} nodes", node_count);
                    let node_texts: String = if let Ok(parsed) =
                        serde_json::from_str::<serde_json::Value>(&content_json)
                    {
                        parsed
                            .get("nodes")
                            .and_then(|n| n.as_array())
                            .map(|nodes| {
                                nodes
                                    .iter()
                                    .filter_map(|n| {
                                        n.get("data")
                                            .and_then(|d| d.get("label").and_then(|l| l.as_str()))
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" • ")
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    Ok(NexusRow {
                        id: row.get(0)?,
                        item_type: "whiteboard".to_string(),
                        title: row.get(1)?,
                        preview,
                        tags,
                        date: row.get(6)?,
                        path: row.get(4)?,
                        content: node_texts,
                        status: None,
                    })
                })
                .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
            return Ok(rows.next().and_then(|r| r.ok()));
        }
        // Universal Nodes table
        {
            let mut stmt = self.conn
                .prepare("SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE id = ?1 AND node_type NOT LIKE 'finance_%'")
                .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
            let mut rows = stmt
                .query_map(params![id], |row| {
                    let props_str: String = row.get(4)?;
                    let mut tags = Vec::new();
                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&props_str) {
                        if let Some(t) = json_val.get("tags").and_then(|v| v.as_array()) {
                            tags = t
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    let content: String = row.get(3)?;
                    let preview: String = content.chars().take(150).collect();
                    let node_type: String = row.get(1)?;
                    Ok(NexusRow {
                        id: row.get(0)?,
                        item_type: node_type,
                        title: row.get(2)?,
                        preview,
                        tags,
                        date: row.get(5)?,
                        path: row.get(0)?,
                        content,
                        status: None,
                    })
                })
                .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
            if let Some(Ok(row)) = rows.next() {
                return Ok(Some(row));
            }
        }

        Ok(None)
    }

    // ═══════════════════════════════════════════════════════════
    //  NODE EDGES (ID-based knowledge graph)
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_node_edge(&self, edge: &NodeEdge) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO node_edges (id, source_id, target_id, edge_type, relation, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(source_id, target_id, edge_type) DO UPDATE SET
                relation = COALESCE(excluded.relation, node_edges.relation),
                id = excluded.id",
                params![
                    edge.id,
                    edge.source_id,
                    edge.target_id,
                    edge.edge_type,
                    edge.relation,
                    edge.created_at
                ],
            )
            .map_err(|e| AppError::General(format!("DB Error upserting node_edge: {}", e)))?;
        Ok(())
    }

    pub fn delete_node_edges_by_source(&self, source_id: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM node_edges WHERE source_id = ?1",
                params![source_id],
            )
            .map_err(|e| AppError::General(format!("DB Error deleting node_edges: {}", e)))?;
        Ok(())
    }

    pub fn delete_node_edge(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM node_edges WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Error deleting node_edge: {}", e)))?;
        Ok(())
    }

    /// Get all edges connected to a node (both incoming and outgoing)
    pub fn get_node_edges_for_node(&self, node_id: &str) -> AppResult<Vec<NodeEdge>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_id, target_id, edge_type, relation, created_at
             FROM node_edges
             WHERE source_id = ?1 OR target_id = ?1
             ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Error querying node_edges: {}", e)))?;

        let rows = stmt
            .query_map(params![node_id], |row| {
                Ok(NodeEdge {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    edge_type: row.get(3)?,
                    relation: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Error mapping node_edges: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    pub fn get_all_node_edges(&self) -> AppResult<Vec<NodeEdge>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_id, target_id, edge_type, relation, created_at FROM node_edges",
            )
            .map_err(|e| AppError::General(format!("DB Error querying all node_edges: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(NodeEdge {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    edge_type: row.get(3)?,
                    relation: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Error mapping all node_edges: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    // ═══════════════════════════════════════════════════════════
    //  FULL-TEXT SEARCH (FTS5)
    // ═══════════════════════════════════════════════════════════

    /// Rebuild the entire FTS5 search index from all data tables.
    /// Called on app startup or when the user requests a reindex.
    pub fn reindex_search(&self) -> AppResult<()> {
        // Clear existing index
        self.conn
            .execute("DELETE FROM search_index", [])
            .map_err(|e| AppError::General(format!("FTS Clear Error: {}", e)))?;

        // Index files (with properties)
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, filename, tags, extension, modified_at, path, source_type FROM files",
            )
            .map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let filename: String = row.get(1)?;
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let extension: String = row.get(3)?;
            let date: String = row.get(4)?;
            let path: String = row.get(5)?;
            let source_type: String = row.get::<_, String>(6).unwrap_or_default();
            let props = format!("ext:{} source:{}", extension, source_type);
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'file', ?2, ?3, ?4, ?5, NULL, ?6, ?7)",
                params![id, filename, tags.join(" "), extension, props, date, path],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        // Index whiteboards
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, tags, path, updated_at FROM whiteboards")
            .map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let path: String = row.get(3)?;
            let date: String = row.get(4)?;
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'whiteboard', ?2, ?3, ?2, '', NULL, ?4, ?5)",
                params![id, title, tags.join(" "), date, path],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        // Index nodes (Universal Core)
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE node_type NOT LIKE 'finance_%'"
        ).map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let node_type: String = row.get(1)?;
            let title: String = row.get(2)?;
            let content: String = row.get(3)?;
            let properties: String = row.get(4)?;
            let date: String = row.get(5)?;
            // Attempt to extract tags, status, and priority from properties if present
            let mut tags_str = String::new();
            let mut status = None;
            let mut props_search = properties.clone();
            let mut search_path = id.clone();
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&properties) {
                if let Some(p) = json_val.get("path").and_then(|v| v.as_str()) {
                    search_path = p.to_string();
                }
                if let Some(tags) = json_val.get("tags").and_then(|v| v.as_array()) {
                    let tags_vec: Vec<String> = tags.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    tags_str = tags_vec.join(" ");
                }
                if let Some(s) = json_val.get("status").and_then(|v| v.as_str()) {
                    status = Some(s.to_string());
                }
                // Extract priority to append to properties text for BM25 search
                if let Some(p) = json_val.get("priority").and_then(|v| v.as_str()) {
                    props_search = format!("{} priority:{}", properties, p);
                }
            }
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, node_type, title, tags_str, content, props_search, status.unwrap_or("".to_string()), date, search_path],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        // Index node blocks
        let mut stmt = self
            .conn
            .prepare("SELECT block_id, node_id, content FROM node_blocks")
            .map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let block_id: String = row.get(0)?;
            let node_id: String = row.get(1)?;
            let content: String = row.get(2)?;
            let item_id = format!("{}#{}", node_id, block_id);
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'block', ?2, '', ?3, '', '', '', ?4)",
                params![item_id, block_id, content, node_id],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        Ok(())
    }

    /// Insert or update a single entry in the FTS5 search index.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_search_entry(
        &self,
        item_id: &str,
        item_type: &str,
        title: &str,
        tags: &str,
        content: &str,
        properties: &str,
        status: Option<&str>,
        date: &str,
        path: &str,
    ) {
        if item_type.starts_with("finance_") {
            return;
        }
        // FTS5 doesn't support ON CONFLICT, so delete + insert
        let _ = self.conn.execute(
            "DELETE FROM search_index WHERE item_id = ?1",
            params![item_id],
        );
        let _ = self.conn.execute(
            "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![item_id, item_type, title, tags, content, properties, status.unwrap_or(""), date, path],
        );
    }

    /// Remove an entry from the FTS5 search index.
    pub fn delete_search_entry(&self, item_id: &str) {
        let _ = self.conn.execute(
            "DELETE FROM search_index WHERE item_id = ?1",
            params![item_id],
        );
    }

    /// Perform a full-text search using FTS5 with BM25 ranking.
    pub fn search_fts(
        &self,
        parsed: &crate::search::ParsedQuery,
        page: u32,
        per_page: u32,
    ) -> AppResult<crate::search::SearchResponse> {
        let start = Instant::now();
        let offset = (page.saturating_sub(1)) * per_page;

        let has_fts_terms = !parsed.fts_terms.is_empty();
        let has_exclude = !parsed.exclude_terms.is_empty();

        // All parameter values collected here; SQL uses numbered placeholders ?N
        let mut param_values: Vec<String> = Vec::new();
        // Tracks the next available placeholder index (1-based for SQLite)
        let mut param_idx: usize = 1;

        // Build the SQL query dynamically
        let mut sql;
        let mut count_sql;

        if has_fts_terms || has_exclude {
            // Build FTS5 MATCH expression
            let mut match_parts: Vec<String> = Vec::new();
            for term in &parsed.fts_terms {
                if term.starts_with('"') && term.ends_with('"') {
                    // Phrase query — pass directly
                    match_parts.push(term.clone());
                } else if parsed.title_only {
                    // Restrict to title column
                    match_parts.push(format!("title : \"{}\"", term));
                } else {
                    // Search across title (boosted), tags, content with column weighting
                    // FTS5: {col1 col2} : term
                    match_parts.push(format!("\"{}\"", term));
                }
            }
            for term in &parsed.exclude_terms {
                match_parts.push(format!("NOT \"{}\"", term));
            }

            let fts_expr = match_parts.join(" AND ");
            param_values.push(fts_expr);
            param_idx += 1; // ?1 is used for the MATCH expression

            // Main query with BM25 ranking
            // bm25 weights: item_id=0, item_type=0, title=10, tags=5, content=1, properties=3
            sql = "SELECT item_id, item_type, title, snippet(search_index, 4, '<mark>', '</mark>', '…', 48) as snip, tags, date, path, bm25(search_index, 0.0, 0.0, 10.0, 5.0, 1.0, 3.0) as rank, status FROM search_index WHERE search_index MATCH ?1".to_string();
            count_sql = "SELECT COUNT(*) FROM search_index WHERE search_index MATCH ?1".to_string();
        } else {
            // No FTS terms — browse mode (filter only)
            sql = "SELECT item_id, item_type, title, substr(content, 1, 200) as snip, tags, date, path, 0.0 as rank, status FROM search_index WHERE 1=1".to_string();
            count_sql = "SELECT COUNT(*) FROM search_index WHERE 1=1".to_string();
        }

        // Apply filters — all use parameterized placeholders
        if let Some(type_val) = &parsed.type_filter {
            sql.push_str(&format!(" AND item_type = ?{}", param_idx));
            count_sql.push_str(&format!(" AND item_type = ?{}", param_idx));
            param_values.push(type_val.clone());
            param_idx += 1;
        }

        for tag in &parsed.tag_filters {
            sql.push_str(&format!(" AND tags LIKE ?{}", param_idx));
            count_sql.push_str(&format!(" AND tags LIKE ?{}", param_idx));
            param_values.push(format!("%{}%", tag));
            param_idx += 1;
        }

        if let Some(status_val) = &parsed.status_filter {
            sql.push_str(&format!(" AND status = ?{}", param_idx));
            count_sql.push_str(&format!(" AND status = ?{}", param_idx));
            param_values.push(status_val.clone());
            param_idx += 1;
        }

        // Apply generic property filters
        for (key, val) in &parsed.property_filters {
            sql.push_str(&format!(" AND properties LIKE ?{}", param_idx));
            count_sql.push_str(&format!(" AND properties LIKE ?{}", param_idx));
            param_values.push(format!("%{}:{}%", key, val));
            param_idx += 1;
        }

        // Ordering
        if has_fts_terms {
            sql.push_str(" ORDER BY rank"); // BM25 returns negative values, lower = better
        } else {
            sql.push_str(" ORDER BY date DESC");
        }

        // LIMIT and OFFSET as parameters
        sql.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_idx, param_idx + 1));
        param_values.push(per_page.to_string());
        param_values.push(offset.to_string());

        // Execute count query (uses only the filter params, not LIMIT/OFFSET)
        let count_params: Vec<&str> = param_values
            .iter()
            .take(param_values.len() - 2) // exclude LIMIT and OFFSET
            .map(|s| s.as_str())
            .collect();
        let total_count: u32 = self
            .conn
            .query_row(
                &count_sql,
                rusqlite::params_from_iter(count_params.iter()),
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Execute search query (all params including LIMIT/OFFSET)
        let all_params: Vec<&str> = param_values.iter().map(|s| s.as_str()).collect();
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| AppError::General(format!("FTS Search Prepare Error: {}", e)))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(all_params.iter()), |row| {
                let tags_str: String = row.get(4)?;
                let tags: Vec<String> = tags_str
                    .split_whitespace()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                let rank: f64 = row.get(7)?;
                Ok(crate::search::SearchResult {
                    id: row.get(0)?,
                    item_type: row.get(1)?,
                    title: row.get(2)?,
                    snippet: row.get(3)?,
                    tags,
                    date: row.get(5)?,
                    path: row.get(6)?,
                    score: -rank, // BM25 returns negative; negate for display
                    status: row.get(8)?,
                })
            })
            .map_err(|e| AppError::General(format!("FTS Search Map Error: {}", e)))?;

        let mut results = Vec::new();
        for row in rows.flatten() {
            results.push(row);
        }

        let elapsed = start.elapsed().as_millis() as u64;

        // Case-sensitive post-filter: FTS5 is case-insensitive, so we filter results here
        if parsed.case_sensitive && !parsed.fts_terms.is_empty() {
            let original_terms: Vec<&str> = parsed
                .fts_terms
                .iter()
                .map(|t| t.trim_matches('"'))
                .filter(|t| !t.is_empty())
                .collect();

            results.retain(|r| {
                let haystack = format!(
                    "{} {} {}",
                    r.title,
                    r.snippet.replace("<mark>", "").replace("</mark>", ""),
                    r.tags.join(" ")
                );
                original_terms.iter().all(|term| haystack.contains(term))
            });
            let filtered_count = results.len() as u32;
            return Ok(crate::search::SearchResponse {
                results,
                total_count: filtered_count,
                query_time_ms: elapsed,
            });
        }

        Ok(crate::search::SearchResponse {
            results,
            total_count,
            query_time_ms: elapsed,
        })
    }

    // ═══════════════════════════════════════════════════════════
    //  NODES (UNIVERSAL ARCHITECTURE)
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_node(&self, node: &crate::models::node::NodeMetadata) -> AppResult<()> {
        let properties_json = serde_json::to_string(&node.properties)?;
        self.conn.execute(
            "INSERT INTO nodes (id, node_type, title, content, properties, created_at, updated_at, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                node_type=excluded.node_type,
                title=excluded.title,
                content=excluded.content,
                properties=excluded.properties,
                updated_at=excluded.updated_at,
                timestamp=excluded.timestamp",
            params![node.id, node.node_type, node.title, node.content, properties_json, node.created_at, node.updated_at, node.timestamp],
        ).map_err(|e| AppError::General(format!("DB Upsert Node Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_node(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM nodes WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Node Error: {}", e)))?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> AppResult<Option<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE id = ?1")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn get_nodes_by_type(
        &self,
        node_type: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE node_type = ?1 ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map(params![node_type], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_active_tasks_and_events(
        &self,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp 
             FROM nodes 
             WHERE node_type IN ('task', 'event', 'person') 
             AND (
                 (node_type = 'task' AND json_extract(properties, '$.status') NOT IN ('done', 'canceled') AND json_extract(properties, '$.due_date') IS NOT NULL AND json_extract(properties, '$.due_date') != '')
                 OR (node_type = 'event' AND json_extract(properties, '$.start_at') IS NOT NULL AND json_extract(properties, '$.start_at') != '')
                 OR (node_type = 'person' AND json_extract(properties, '$.birthday') IS NOT NULL AND json_extract(properties, '$.birthday') != '')
             )"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_active_tasks_and_events): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (get_active_tasks_and_events): {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_linked_nodes(
        &self,
        _target_title: &str,
        target_id: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        if target_id.is_empty() {
            return Ok(Vec::new());
        }

        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.node_type, n.title, n.content, n.properties, n.created_at, n.updated_at, n.timestamp 
             FROM nodes n 
             JOIN node_edges e ON n.id = e.source_id 
             WHERE e.target_id = ?1
             ORDER BY n.updated_at DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_linked_nodes): {}", e)))?;

        let rows = stmt
            .query_map(params![target_id], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (get_linked_nodes): {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_node_title(&self, node_id: &str) -> Option<String> {
        let mut stmt = self
            .conn
            .prepare("SELECT title FROM nodes WHERE id = ?1")
            .ok()?;
        stmt.query_row(params![node_id], |row| row.get::<_, String>(0))
            .ok()
    }

    pub fn get_all_nodes(&self) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut nodes = Vec::new();
        for n in rows.flatten() {
            nodes.push(n);
        }
        Ok(nodes)
    }

    pub fn get_all_tags_with_counts(&self) -> AppResult<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT json_each.value, COUNT(*) 
             FROM nodes, json_each(nodes.properties, '$.tags') 
             GROUP BY json_each.value 
             ORDER BY COUNT(*) DESC, json_each.value ASC"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_all_tags): {}", e)))?;

        let rows = stmt.query_map([], |row| {
            let tag: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((tag, count))
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for row in rows.flatten() {
            results.push(row);
        }
        Ok(results)
    }

    pub fn get_nodes_by_tag(&self, target_tag: &str) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp 
             FROM nodes 
             WHERE EXISTS (
                 SELECT 1 FROM json_each(nodes.properties, '$.tags') WHERE value = ?1
             )"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_nodes_by_tag): {}", e)))?;

        let rows = stmt
            .query_map(params![target_tag], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut nodes = Vec::new();
        for n in rows.flatten() {
            nodes.push(n);
        }
        Ok(nodes)
    }

    // ═══════════════════════════════════════════════════════════
    //  CRDT STORAGE LOGIC (Phase 1)
    // ═══════════════════════════════════════════════════════════

    pub fn get_crdt_doc(&self, doc_id: &str) -> AppResult<loro::LoroDoc> {
        let mut stmt = self.conn.prepare("SELECT snapshot FROM crdt_documents WHERE doc_id = ?1")
            .map_err(|e| AppError::General(format!("DB Error prepare get_crdt_doc: {}", e)))?;
        let mut rows = stmt.query(params![doc_id])
            .map_err(|e| AppError::General(format!("DB Error querying crdt_documents: {}", e)))?;
        
        let doc = loro::LoroDoc::new();
        
        if let Some(row) = rows.next().unwrap_or(None) {
            let snapshot: Vec<u8> = row.get(0).unwrap_or_default();
            if !snapshot.is_empty() {
                doc.import(&snapshot).map_err(|e| AppError::General(format!("Failed to import Loro snapshot: {:?}", e)))?;
            }
        }
        
        // Load pending deltas
        let mut delta_stmt = self.conn.prepare("SELECT delta FROM crdt_updates WHERE doc_id = ?1 ORDER BY id ASC")
            .map_err(|e| AppError::General(format!("DB Error prepare crdt_updates: {}", e)))?;
        let delta_rows = delta_stmt.query_map(params![doc_id], |row| {
            let delta: Vec<u8> = row.get(0)?;
            Ok(delta)
        }).map_err(|e| AppError::General(format!("DB Error querying crdt_updates: {}", e)))?;
        
        for delta in delta_rows.flatten() {
            if !delta.is_empty() {
                doc.import(&delta).map_err(|e| AppError::General(format!("Failed to import Loro delta: {:?}", e)))?;
            }
        }
        
        Ok(doc)
    }

    pub fn save_crdt_delta(&self, doc_id: &str, delta: Vec<u8>) -> AppResult<()> {
        if delta.is_empty() {
            return Ok(());
        }
        let timestamp = chrono::Utc::now().timestamp_millis();
        self.conn.execute(
            "INSERT INTO crdt_updates (doc_id, delta, timestamp) VALUES (?1, ?2, ?3)",
            params![doc_id, delta, timestamp]
        ).map_err(|e| AppError::General(format!("DB Error saving crdt_delta: {}", e)))?;
        Ok(())
    }

    pub fn save_crdt_snapshot(&self, doc_id: &str, snapshot: Vec<u8>) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO crdt_documents (doc_id, snapshot) VALUES (?1, ?2)
             ON CONFLICT(doc_id) DO UPDATE SET snapshot=excluded.snapshot",
            params![doc_id, snapshot]
        ).map_err(|e| AppError::General(format!("DB Error saving crdt_snapshot: {}", e)))?;
        Ok(())
    }

    pub fn compact_crdt_history(&self, doc_id: &str) -> AppResult<()> {
        let doc = self.get_crdt_doc(doc_id)?;
        let snapshot = doc.export_snapshot();
        self.save_crdt_snapshot(doc_id, snapshot)?;
        
        self.conn.execute(
            "DELETE FROM crdt_updates WHERE doc_id = ?1",
            params![doc_id]
        ).map_err(|e| AppError::General(format!("DB Error compacting crdt_updates: {}", e)))?;
        Ok(())
    }

    pub fn compact_all_crdt(&self) -> AppResult<()> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT doc_id FROM crdt_updates")
            .map_err(|e| AppError::General(format!("DB Error getting docs for compaction: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let doc_id: String = row.get(0)?;
            Ok(doc_id)
        }).map_err(|e| AppError::General(format!("DB Map error in compaction: {}", e)))?;
        
        for doc_id in rows.flatten() {
            let _ = self.compact_crdt_history(&doc_id);
        }
        Ok(())
    }
}

/// Lightweight row struct for Nexus unified queries
#[derive(Debug)]
pub struct NexusRow {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub preview: String,
    pub tags: Vec<String>,
    pub date: String,
    pub path: String,
    pub content: String,
    pub status: Option<String>,
}
