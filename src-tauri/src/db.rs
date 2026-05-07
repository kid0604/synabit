use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::time::Instant;
use std::sync::Mutex;


use crate::models::file::{FileMetadata, FileSource};
use crate::models::whiteboard::WhiteboardMetadata;
use crate::error::{AppError, AppResult};
use crate::utils::graph_parser::GraphEdge;

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

        // ─── Drop Legacy Notes Table ───────────────────────────
        let _ = conn.execute("DROP TABLE IF EXISTS notes", []);

        // ─── Drop Legacy Events Table ──────────────────────────
        let _ = conn.execute("DROP TABLE IF EXISTS events", []);

        // ─── Drop Legacy Tasks Table ───────────────────────────
        let _ = conn.execute("DROP TABLE IF EXISTS tasks", []);

        // ─── Drop Legacy QuickCaps Table ───────────────────────────
        let _ = conn.execute("DROP TABLE IF EXISTS quickcaps", []);

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
        ).map_err(|e| AppError::General(format!("DB Schema Error (files): {}", e)))?;

        // ─── File Sources Table ────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS file_sources (
                id TEXT PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL
            )",
            [],
        ).map_err(|e| AppError::General(format!("DB Schema Error (file_sources): {}", e)))?;

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
        ).map_err(|e| AppError::General(format!("DB Schema Error (nodes): {}", e)))?;

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
        ).map_err(|e| AppError::General(format!("DB Schema Error (whiteboards): {}", e)))?;

        // ─── KV Store (for OAuth tokens and settings) ──────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv_store (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        ).map_err(|e| AppError::General(format!("DB Schema Error (kv_store): {}", e)))?;

        // ─── Graph Edges (for Nexus Knowledge Graph) ───────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS graph_edges (
                source_id TEXT NOT NULL,
                target_title_or_path TEXT NOT NULL,
                link_type TEXT NOT NULL,
                PRIMARY KEY (source_id, target_title_or_path, link_type)
            )",
            [],
        ).map_err(|e| AppError::General(format!("DB Schema Error (graph_edges): {}", e)))?;

        // ─── FTS5 Full-Text Search Index ────────────────────────
        // DROP + CREATE to ensure schema includes the `properties` column.
        // Data is repopulated by reindex_search() on every app startup.
        conn.execute_batch("DROP TABLE IF EXISTS search_index;")
            .map_err(|e| AppError::General(format!("DB Schema Error (drop search_index): {}", e)))?;
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
            );"
        ).map_err(|e| AppError::General(format!("DB Schema Error (search_index): {}", e)))?;

        Ok(Self { conn })
    }

    // ═══════════════════════════════════════════════════════════
    //  KV STORE
    // ═══════════════════════════════════════════════════════════

    pub fn set_kv(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO kv_store (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        ).map_err(|e| AppError::General(format!("DB Set KV Error: {}", e)))?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> AppResult<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM kv_store WHERE key = ?1")
            .map_err(|e| AppError::General(format!("DB Get KV Prepare Error: {}", e)))?;
        let mut rows = stmt.query(params![key])
            .map_err(|e| AppError::General(format!("DB Get KV Query Error: {}", e)))?;
        
        if let Some(row) = rows.next().unwrap_or(None) {
            Ok(Some(row.get(0).unwrap_or_default()))
        } else {
            Ok(None)
        }
    }

    pub fn delete_kv(&self, key: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM kv_store WHERE key = ?1", params![key])
            .map_err(|e| AppError::General(format!("DB Delete KV Error: {}", e)))?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════
    //  NOTES
    // ═══════════════════════════════════════════════════════════







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
        self.conn.execute("DELETE FROM whiteboards WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Whiteboard Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_whiteboard_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare("SELECT id, timestamp FROM whiteboards")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
        let mut map = HashMap::new();
        for r in rows.flatten() { map.insert(r.0, r.1); }
        Ok(map)
    }

    // ═══════════════════════════════════════════════════════════
    //  FILES & SOURCES
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_file_source(&self, source: &FileSource) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO file_sources (id, path, name) 
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET 
                name=excluded.name",
            params![source.id, source.path, source.name],
        ).map_err(|e| AppError::General(format!("DB Upsert File Source Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_file_sources(&self) -> AppResult<Vec<FileSource>> {
        let mut stmt = self.conn.prepare("SELECT id, path, name FROM file_sources")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            Ok(FileSource {
                id: row.get(0)?,
                path: row.get(1)?,
                name: row.get(2)?,
            })
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut sources = Vec::new();
        for s in rows.flatten() {
            sources.push(s);
        }
        Ok(sources)
    }

    pub fn delete_file_source(&self, id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM file_sources WHERE id = ?1", params![id])
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
                modified_at=excluded.modified_at,
                tags=excluded.tags",
            params![
                file.id, file.path, file.filename, file.extension, file.size, 
                file.created_at, file.modified_at, tags_json, file.source_type
            ],
        ).map_err(|e| AppError::General(format!("DB Upsert File Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_file_by_path(&self, path: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM files WHERE path = ?1", params![path])
            .map_err(|e| AppError::General(format!("DB Delete File Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_files_by_source_type(&self, source_type: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM files WHERE source_type = ?1", params![source_type])
            .map_err(|e| AppError::General(format!("DB Delete Files by Source Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_files(&self) -> AppResult<Vec<FileMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, filename, extension, size, created_at, modified_at, tags, source_type 
             FROM files ORDER BY modified_at DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt.query_map([], |row| {
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
                source_type: row.get(8)?,
            })
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut files = Vec::new();
        for f in rows.flatten() {
            files.push(f);
        }
        Ok(files)
    }

    pub fn update_file_tags(&self, path: &str, tags: Vec<String>) -> AppResult<()> {
        let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        self.conn.execute("UPDATE files SET tags = ?1 WHERE path = ?2", params![tags_json, path])
            .map_err(|e| AppError::General(format!("DB Update File Tags Error: {}", e)))?;
        Ok(())
    }

    pub fn update_file_path_and_name(&self, old_path: &str, new_path: &str, new_filename: &str, extension: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE files SET path = ?1, filename = ?2, extension = ?3 WHERE path = ?4",
            params![new_path, new_filename, extension, old_path],
        ).map_err(|e| AppError::General(format!("DB Rename File Error: {}", e)))?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════
    //  NEXUS — Unified search query (replaces 4x full scan)
    // ═══════════════════════════════════════════════════════════

    pub fn get_all_nexus_items(&self) -> AppResult<Vec<NexusRow>> {
        let mut items = Vec::new();

        // Files
        let mut stmt = self.conn.prepare(
            "SELECT id, path, filename, extension, size, modified_at, tags FROM files"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let tags_str: String = row.get(6)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let filename: String = row.get(2)?;
            let extension: String = row.get(3)?;
            let size: i64 = row.get(4)?;
            let size_mb = size as f64 / 1024.0 / 1024.0;
            let preview = format!("{} • {:.2}MB", extension, size_mb);
            
            Ok(NexusRow {
                id: row.get(0)?, item_type: "file".to_string(),
                title: filename.clone(), preview, tags,
                date: row.get(5)?, path: row.get(1)?,
                content: filename,
                status: None,
            })
        }).map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() { items.push(r); }

        // Whiteboards
        let mut stmt = self.conn.prepare(
            "SELECT id, title, tags, content, path, created_at, updated_at FROM whiteboards"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let tags_str: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let content_json: String = row.get(3)?;
            let node_count = content_json.matches("\"id\"").count();
            let preview = format!("Whiteboard • {} nodes", node_count);
            // Extract text labels from nodes for searchable content
            let node_texts: String = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content_json) {
                parsed.get("nodes")
                    .and_then(|n| n.as_array())
                    .map(|nodes| {
                        nodes.iter()
                            .filter_map(|n| n.get("data").and_then(|d| d.get("label").and_then(|l| l.as_str())))
                            .collect::<Vec<_>>()
                            .join(" • ")
                    })
                    .unwrap_or_default()
            } else { String::new() };
            Ok(NexusRow {
                id: row.get(0)?, item_type: "whiteboard".to_string(),
                title: row.get(1)?, preview, tags,
                date: row.get(6)?, path: row.get(4)?,
                content: node_texts,
                status: None,
            })
        }).map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() { items.push(r); }

        // Nodes (Universal Architecture)
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let props_str: String = row.get(4)?;
            let mut tags = Vec::new();
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&props_str) {
                if let Some(t) = json_val.get("tags").and_then(|v| v.as_array()) {
                    tags = t.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                }
            }
            let content: String = row.get(3)?;
            let preview: String = content.chars().take(150).collect();
            let node_type: String = row.get(1)?;
            Ok(NexusRow {
                id: row.get(0)?, item_type: node_type,
                title: row.get(2)?, preview, tags,
                date: row.get(5)?, path: row.get(0)?,
                content,
                status: None,
            })
        }).map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() { items.push(r); }

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
            let mut rows = stmt.query_map(params![id], |row| {
                let tags_str: String = row.get(2)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let content_json: String = row.get(3)?;
                let node_count = content_json.matches("\"id\"").count();
                let preview = format!("Whiteboard • {} nodes", node_count);
                let node_texts: String = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content_json) {
                    parsed.get("nodes")
                        .and_then(|n| n.as_array())
                        .map(|nodes| {
                            nodes.iter()
                                .filter_map(|n| n.get("data").and_then(|d| d.get("label").and_then(|l| l.as_str())))
                                .collect::<Vec<_>>()
                                .join(" • ")
                        })
                        .unwrap_or_default()
                } else { String::new() };
                Ok(NexusRow {
                    id: row.get(0)?, item_type: "whiteboard".to_string(),
                    title: row.get(1)?, preview, tags,
                    date: row.get(6)?, path: row.get(4)?,
                    content: node_texts,
                    status: None,
                })
            }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
            return Ok(rows.next().and_then(|r| r.ok()));
        }
        // Files (UUID-based IDs)
        {
            let mut stmt = self.conn
                .prepare("SELECT id, path, filename, extension, size, modified_at, tags FROM files WHERE id = ?1")
                .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
            let mut rows = stmt.query_map(params![id], |row| {
                let tags_str: String = row.get(6)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let filename: String = row.get(2)?;
                let extension: String = row.get(3)?;
                let size: i64 = row.get(4)?;
                let size_mb = size as f64 / 1024.0 / 1024.0;
                let preview = format!("{} • {:.2}MB", extension, size_mb);
                Ok(NexusRow {
                    id: row.get(0)?, item_type: "file".to_string(),
                    title: filename.clone(), preview, tags,
                    date: row.get(5)?, path: row.get(1)?,
                    content: filename, status: None,
                })
            }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
            if let Some(Ok(row)) = rows.next() {
                return Ok(Some(row));
            }
        }
        
        // Fallback to Universal Nodes table
        {
            let mut stmt = self.conn
                .prepare("SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE id = ?1")
                .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
            let mut rows = stmt.query_map(params![id], |row| {
                let props_str: String = row.get(4)?;
                let mut tags = Vec::new();
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&props_str) {
                    if let Some(t) = json_val.get("tags").and_then(|v| v.as_array()) {
                        tags = t.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    }
                }
                let content: String = row.get(3)?;
                let preview: String = content.chars().take(150).collect();
                let node_type: String = row.get(1)?;
                Ok(NexusRow {
                    id: row.get(0)?, item_type: node_type,
                    title: row.get(2)?, preview, tags,
                    date: row.get(5)?, path: row.get(0)?,
                    content, status: None,
                })
            }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
            if let Some(Ok(row)) = rows.next() {
                return Ok(Some(row));
            }
        }
        
        Ok(None)
    }

    pub fn clear_all(&self) -> AppResult<()> {
        self.conn.execute_batch(
            ""
        ).map_err(|e| AppError::General(format!("DB Clear Error: {}", e)))?;
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════
    //  GRAPH EDGES
    // ═══════════════════════════════════════════════════════════

    pub fn update_edges(&self, source_id: &str, edges: Vec<GraphEdge>) -> AppResult<()> {
        let mut stmt = self.conn.prepare("DELETE FROM graph_edges WHERE source_id = ?1")
            .map_err(|e| AppError::General(format!("DB Error prepare delete edges: {}", e)))?;
        stmt.execute(params![source_id]).map_err(|e| AppError::General(format!("DB Error deleting edges: {}", e)))?;

        let mut insert_stmt = self.conn.prepare(
            "INSERT INTO graph_edges (source_id, target_title_or_path, link_type) VALUES (?1, ?2, ?3)"
        ).map_err(|e| AppError::General(format!("DB Error preparing edge insert: {}", e)))?;
        
        for edge in edges {
            let _ = insert_stmt.execute(params![
                edge.source_id,
                edge.target_title_or_path,
                edge.link_type
            ]); // Ignore individual insert errors (like duplicates)
        }
        
        Ok(())
    }

    pub fn delete_edges(&self, source_id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM graph_edges WHERE source_id = ?1", params![source_id])
            .map_err(|e| AppError::General(format!("DB Error deleting edges: {}", e)))?;
        Ok(())
    }

    pub fn get_all_edges(&self) -> AppResult<Vec<GraphEdge>> {
        let mut stmt = self.conn.prepare("SELECT source_id, target_title_or_path, link_type FROM graph_edges")
            .map_err(|e| AppError::General(format!("DB Error preparing edges query: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            Ok(GraphEdge {
                source_id: row.get(0)?,
                target_title_or_path: row.get(1)?,
                link_type: row.get(2)?,
            })
        }).map_err(|e| AppError::General(format!("DB Error mapping edges: {}", e)))?;

        let mut edges = Vec::new();
        for edge in rows.flatten() {
            edges.push(edge);
        }
        Ok(edges)
    }

    // ═══════════════════════════════════════════════════════════
    //  FULL-TEXT SEARCH (FTS5)
    // ═══════════════════════════════════════════════════════════

    /// Rebuild the entire FTS5 search index from all data tables.
    /// Called on app startup or when the user requests a reindex.
    pub fn reindex_search(&self) -> AppResult<()> {
        // Clear existing index
        self.conn.execute("DELETE FROM search_index", [])
            .map_err(|e| AppError::General(format!("FTS Clear Error: {}", e)))?;


        // Index files (with properties)
        let mut stmt = self.conn.prepare(
            "SELECT id, filename, tags, extension, modified_at, path, source_type FROM files"
        ).map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
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
        let mut stmt = self.conn.prepare(
            "SELECT id, title, tags, path, updated_at FROM whiteboards"
        ).map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
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
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes"
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
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&properties) {
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
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?1)",
                params![id, node_type, title, tags_str, content, props_search, status.unwrap_or("".to_string()), date],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        Ok(())
    }

    /// Insert or update a single entry in the FTS5 search index.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_search_entry(&self, item_id: &str, item_type: &str, title: &str, tags: &str, content: &str, properties: &str, status: Option<&str>, date: &str, path: &str) {
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
    pub fn search_fts(&self, parsed: &crate::search::ParsedQuery, page: u32, per_page: u32) -> AppResult<crate::search::SearchResponse> {
        let start = Instant::now();
        let offset = (page.saturating_sub(1)) * per_page;

        let has_fts_terms = !parsed.fts_terms.is_empty();
        let has_exclude = !parsed.exclude_terms.is_empty();
        let has_tag_filter = !parsed.tag_filters.is_empty();
        let has_type_filter = parsed.type_filter.is_some();
        let has_status_filter = parsed.status_filter.is_some();

        // Build the SQL query dynamically
        let mut sql;
        let mut count_sql;
        let mut param_values: Vec<String> = Vec::new();

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

            // Main query with BM25 ranking
            // bm25(search_index, weight_item_id, weight_item_type, weight_title, weight_tags, weight_content)
            // We want: title=10.0, tags=5.0, content=1.0, item_id and item_type have 0 weight
            // bm25 weights: item_id=0, item_type=0, title=10, tags=5, content=1, properties=3
            sql = "SELECT item_id, item_type, title, snippet(search_index, 4, '<mark>', '</mark>', '…', 48) as snip, tags, date, path, bm25(search_index, 0.0, 0.0, 10.0, 5.0, 1.0, 3.0) as rank, status FROM search_index WHERE search_index MATCH ?1".to_string();
            count_sql = "SELECT COUNT(*) FROM search_index WHERE search_index MATCH ?1".to_string();
        } else {
            // No FTS terms — browse mode (filter only)
            sql = "SELECT item_id, item_type, title, substr(content, 1, 200) as snip, tags, date, path, 0.0 as rank, status FROM search_index WHERE 1=1".to_string();
            count_sql = "SELECT COUNT(*) FROM search_index WHERE 1=1".to_string();
        }

        // Apply filters
        if has_type_filter {
            let type_val = parsed.type_filter.as_ref().unwrap();
            sql.push_str(&format!(" AND item_type = '{}'", type_val));
            count_sql.push_str(&format!(" AND item_type = '{}'", type_val));
        }

        if has_tag_filter {
            for tag in &parsed.tag_filters {
                let escaped = tag.replace('\'', "''");
                sql.push_str(&format!(" AND tags LIKE '%{}%'", escaped));
                count_sql.push_str(&format!(" AND tags LIKE '%{}%'", escaped));
            }
        }

        if has_status_filter {
            let status_val = parsed.status_filter.as_ref().unwrap();
            sql.push_str(&format!(" AND status = '{}'", status_val));
            count_sql.push_str(&format!(" AND status = '{}'", status_val));
        }

        // Apply generic property filters
        for (key, val) in &parsed.property_filters {
            let filter = format!("{}:{}", key, val).replace('\'', "''");
            sql.push_str(&format!(" AND properties LIKE '%{}%'", filter));
            count_sql.push_str(&format!(" AND properties LIKE '%{}%'", filter));
        }

        // Ordering
        if has_fts_terms {
            sql.push_str(" ORDER BY rank"); // BM25 returns negative values, lower = better
        } else {
            sql.push_str(" ORDER BY date DESC");
        }

        sql.push_str(&format!(" LIMIT {} OFFSET {}", per_page, offset));

        // Execute count query
        let total_count: u32 = if !param_values.is_empty() {
            self.conn.query_row(&count_sql, params![param_values[0]], |row| row.get(0))
                .unwrap_or(0)
        } else {
            self.conn.query_row(&count_sql, [], |row| row.get(0))
                .unwrap_or(0)
        };

        // Execute search query
        let mut results = Vec::new();

        if !param_values.is_empty() {
            let mut stmt = self.conn.prepare(&sql)
                .map_err(|e| AppError::General(format!("FTS Search Prepare Error: {}", e)))?;
            let rows = stmt.query_map(params![param_values[0]], |row| {
                let tags_str: String = row.get(4)?;
                let tags: Vec<String> = tags_str.split_whitespace()
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
            }).map_err(|e| AppError::General(format!("FTS Search Map Error: {}", e)))?;

            for row in rows.flatten() {
                results.push(row);
            }
        } else {
            let mut stmt = self.conn.prepare(&sql)
                .map_err(|e| AppError::General(format!("FTS Search Prepare Error: {}", e)))?;
            let rows = stmt.query_map([], |row| {
                let tags_str: String = row.get(4)?;
                let tags: Vec<String> = tags_str.split_whitespace()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                Ok(crate::search::SearchResult {
                    id: row.get(0)?,
                    item_type: row.get(1)?,
                    title: row.get(2)?,
                    snippet: row.get(3)?,
                    tags,
                    date: row.get(5)?,
                    path: row.get(6)?,
                    score: 0.0,
                    status: row.get(8)?,
                })
            }).map_err(|e| AppError::General(format!("FTS Search Map Error: {}", e)))?;

            for row in rows.flatten() {
                results.push(row);
            }
        }

        let elapsed = start.elapsed().as_millis() as u64;

        // Case-sensitive post-filter: FTS5 is case-insensitive, so we filter results here
        if parsed.case_sensitive && !parsed.fts_terms.is_empty() {
            let original_terms: Vec<&str> = parsed.fts_terms.iter()
                .map(|t| t.trim_matches('"'))
                .filter(|t| !t.is_empty())
                .collect();

            results.retain(|r| {
                let haystack = format!("{} {} {}", r.title, r.snippet.replace("<mark>", "").replace("</mark>", ""), r.tags.join(" "));
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
        self.conn.execute("DELETE FROM nodes WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Node Error: {}", e)))?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> AppResult<Option<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE id = ?1")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        
        let mut rows = stmt.query_map(params![id], |row| {
            let props_str: String = row.get(4)?;
            let properties: serde_json::Value = serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
            Ok(crate::models::node::NodeMetadata {
                id: row.get(0)?,
                node_type: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                properties,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                timestamp: row.get(7)?,
            })
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn get_nodes_by_type(&self, node_type: &str) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE node_type = ?1 ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        
        let rows = stmt.query_map(params![node_type], |row| {
            let props_str: String = row.get(4)?;
            let properties: serde_json::Value = serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
            Ok(crate::models::node::NodeMetadata {
                id: row.get(0)?,
                node_type: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                properties,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                timestamp: row.get(7)?,
            })
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_linked_nodes(&self, target_title: &str, target_id: &str) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.node_type, n.title, n.content, n.properties, n.created_at, n.updated_at, n.timestamp 
             FROM nodes n 
             JOIN graph_edges e ON n.id = e.source_id 
             WHERE LOWER(e.target_title_or_path) = LOWER(?1) OR LOWER(e.target_title_or_path) = LOWER(?2)
             ORDER BY n.updated_at DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_linked_nodes): {}", e)))?;
        
        let rows = stmt.query_map(params![target_title, target_id], |row| {
            let props_str: String = row.get(4)?;
            let properties: serde_json::Value = serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
            Ok(crate::models::node::NodeMetadata {
                id: row.get(0)?,
                node_type: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                properties,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                timestamp: row.get(7)?,
            })
        }).map_err(|e| AppError::General(format!("DB Map Error (get_linked_nodes): {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_node_title(&self, node_id: &str) -> Option<String> {
        let mut stmt = self.conn.prepare("SELECT title FROM nodes WHERE id = ?1").ok()?;
        stmt.query_row(params![node_id], |row| row.get::<_, String>(0)).ok()
    }

    pub fn get_all_nodes(&self) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        
        let rows = stmt.query_map([], |row| {
            let props_str: String = row.get(4)?;
            let properties: serde_json::Value = serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
            Ok(crate::models::node::NodeMetadata {
                id: row.get(0)?,
                node_type: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                properties,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                timestamp: row.get(7)?,
            })
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut nodes = Vec::new();
        for n in rows.flatten() {
            nodes.push(n);
        }
        Ok(nodes)
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
