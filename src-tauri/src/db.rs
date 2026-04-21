use rusqlite::{params, Connection};
use std::path::Path;
use std::collections::HashMap;

use crate::models::note::NoteMetadata;
use crate::models::task::TaskMetadata;
use crate::models::event::EventMetadata;
use crate::models::quickcap::QuickCapMetadata;
use crate::models::file::{FileMetadata, FileSource};
use crate::error::{AppError, AppResult};

pub struct DbBridge {
    conn: Connection,
}

impl DbBridge {
    pub fn new(vault_path: &str) -> AppResult<Self> {
        let db_path = Path::new(vault_path).join("vault_cache.db");
        let conn = Connection::open(db_path).map_err(|e| AppError::General(format!("DB Open Error: {}", e)))?;
        
        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();

        // ─── Notes Table ───────────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                date TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                summary TEXT NOT NULL,
                tags TEXT NOT NULL,
                pinned BOOLEAN NOT NULL DEFAULT 0,
                content TEXT NOT NULL DEFAULT '',
                is_task BOOLEAN NOT NULL DEFAULT 0,
                is_event BOOLEAN NOT NULL DEFAULT 0,
                has_reminder BOOLEAN NOT NULL DEFAULT 0,
                is_done BOOLEAN NOT NULL DEFAULT 0,
                raw_frontmatter TEXT NOT NULL
            )",
            [],
        ).map_err(|e| AppError::General(format!("DB Schema Error (notes): {}", e)))?;

        // Migration: add columns if missing (for existing vaults)
        let _ = conn.execute("ALTER TABLE notes ADD COLUMN pinned BOOLEAN NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE notes ADD COLUMN content TEXT NOT NULL DEFAULT ''", []);

        // ─── Tasks Table ───────────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'todo',
                priority TEXT NOT NULL DEFAULT '',
                start_date TEXT NOT NULL DEFAULT '',
                due_date TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                content TEXT NOT NULL DEFAULT '',
                path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT NOT NULL DEFAULT '',
                timestamp INTEGER NOT NULL DEFAULT 0
            )",
            [],
        ).map_err(|e| AppError::General(format!("DB Schema Error (tasks): {}", e)))?;

        // ─── Events Table ──────────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                event_date TEXT NOT NULL DEFAULT '',
                event_time TEXT NOT NULL DEFAULT '',
                location TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                content TEXT NOT NULL DEFAULT '',
                path TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT '',
                timestamp INTEGER NOT NULL DEFAULT 0
            )",
            [],
        ).map_err(|e| AppError::General(format!("DB Schema Error (events): {}", e)))?;

        // ─── QuickCaps Table ───────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS quickcaps (
                id TEXT PRIMARY KEY,
                date TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                path TEXT NOT NULL,
                timestamp INTEGER NOT NULL DEFAULT 0
            )",
            [],
        ).map_err(|e| AppError::General(format!("DB Schema Error (quickcaps): {}", e)))?;

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

        Ok(Self { conn })
    }

    // ═══════════════════════════════════════════════════════════
    //  NOTES
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_note(&self, note: &NoteMetadata) -> AppResult<()> {
        let tags_json = serde_json::to_string(&note.tags)?;
        self.conn.execute(
            "INSERT INTO notes (id, title, date, timestamp, summary, tags, pinned, content, is_task, is_event, has_reminder, is_done, raw_frontmatter) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET 
                title=excluded.title,
                date=excluded.date,
                timestamp=excluded.timestamp,
                summary=excluded.summary,
                tags=excluded.tags,
                pinned=excluded.pinned,
                content=excluded.content,
                is_task=excluded.is_task,
                is_event=excluded.is_event,
                has_reminder=excluded.has_reminder,
                is_done=excluded.is_done,
                raw_frontmatter=excluded.raw_frontmatter",
            params![
                note.id,
                note.title,
                note.date,
                note.timestamp,
                note.summary,
                tags_json,
                note.pinned,
                note.content,
                note.is_task,
                note.is_event,
                note.has_reminder,
                note.is_done,
                note.raw_frontmatter
            ],
        ).map_err(|e| AppError::General(format!("DB Upsert Note Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_note(&self, id: &str) -> AppResult<()> {
         self.conn.execute(
            "DELETE FROM notes WHERE id = ?1",
            params![id],
         ).map_err(|e| AppError::General(format!("DB Delete Error: {}", e)))?;
         Ok(())
    }

    pub fn get_all_note_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare("SELECT id, timestamp FROM notes")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut map = HashMap::new();
        for r in rows.flatten() {
            map.insert(r.0, r.1);
        }
        Ok(map)
    }

    pub fn get_all_notes(&self) -> AppResult<Vec<NoteMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, date, timestamp, summary, tags, pinned, content, is_task, is_event, has_reminder, is_done, raw_frontmatter 
             FROM notes ORDER BY timestamp DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let note_iter = stmt.query_map([], |row| {
            let tags_str: String = row.get(5)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let id: String = row.get(0)?;
            
            Ok(NoteMetadata {
                id: id.clone(),
                title: row.get(1)?,
                date: row.get(2)?,
                timestamp: row.get(3)?,
                summary: row.get(4)?,
                tags,
                path: id, // path == id (relative path)
                pinned: row.get(6)?,
                content: row.get(7)?,
                is_task: row.get(8)?,
                is_event: row.get(9)?,
                has_reminder: row.get(10)?,
                is_done: row.get(11)?,
                raw_frontmatter: row.get(12)?,
            })
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut out = Vec::new();
        for n in note_iter.flatten() {
            out.push(n);
        }
        Ok(out)
    }

    // ═══════════════════════════════════════════════════════════
    //  TASKS
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_task(&self, task: &TaskMetadata) -> AppResult<()> {
        let tags_json = serde_json::to_string(&task.tags)?;
        let timestamp = chrono::NaiveDateTime::parse_from_str(&task.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO tasks (id, title, status, priority, start_date, due_date, tags, content, path, created_at, updated_at, completed_at, timestamp) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET 
                title=excluded.title, status=excluded.status, priority=excluded.priority,
                start_date=excluded.start_date, due_date=excluded.due_date, tags=excluded.tags,
                content=excluded.content, path=excluded.path, updated_at=excluded.updated_at,
                completed_at=excluded.completed_at, timestamp=excluded.timestamp",
            params![task.id, task.title, task.status, task.priority, task.start_date, task.due_date,
                    tags_json, task.content, task.path, task.created_at, task.updated_at, task.completed_at, timestamp],
        ).map_err(|e| AppError::General(format!("DB Upsert Task Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_task(&self, id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Task Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_task_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare("SELECT id, timestamp FROM tasks")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
        let mut map = HashMap::new();
        for r in rows.flatten() { map.insert(r.0, r.1); }
        Ok(map)
    }

    // ═══════════════════════════════════════════════════════════
    //  EVENTS
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_event(&self, event: &EventMetadata) -> AppResult<()> {
        let tags_json = serde_json::to_string(&event.tags)?;
        let timestamp = chrono::NaiveDateTime::parse_from_str(&event.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO events (id, title, event_date, event_time, location, tags, content, path, created_at, timestamp) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET 
                title=excluded.title, event_date=excluded.event_date, event_time=excluded.event_time,
                location=excluded.location, tags=excluded.tags, content=excluded.content,
                path=excluded.path, created_at=excluded.created_at, timestamp=excluded.timestamp",
            params![event.id, event.title, event.event_date, event.event_time, event.location,
                    tags_json, event.content, event.path, event.created_at, timestamp],
        ).map_err(|e| AppError::General(format!("DB Upsert Event Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_event(&self, id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM events WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Event Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_event_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare("SELECT id, timestamp FROM events")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
        let mut map = HashMap::new();
        for r in rows.flatten() { map.insert(r.0, r.1); }
        Ok(map)
    }

    // ═══════════════════════════════════════════════════════════
    //  QUICKCAPS
    // ═══════════════════════════════════════════════════════════

    pub fn upsert_quickcap(&self, qc: &QuickCapMetadata) -> AppResult<()> {
        let timestamp = chrono::NaiveDateTime::parse_from_str(&qc.date, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO quickcaps (id, date, content, path, timestamp) 
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET 
                date=excluded.date, content=excluded.content, path=excluded.path, timestamp=excluded.timestamp",
            params![qc.id, qc.date, qc.content, qc.path, timestamp],
        ).map_err(|e| AppError::General(format!("DB Upsert QuickCap Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_quickcap(&self, id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM quickcaps WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete QuickCap Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_quickcap_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self.conn.prepare("SELECT id, timestamp FROM quickcaps")
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

        // Notes
        let mut stmt = self.conn.prepare(
            "SELECT id, title, summary, tags, date, id, content FROM notes"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let tags_str: String = row.get(3)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(NexusRow {
                id: row.get(0)?, item_type: "note".to_string(),
                title: row.get(1)?, preview: row.get(2)?,
                tags, date: row.get(4)?, path: row.get(5)?,
                content: row.get(6)?,
                status: None,
            })
        }).map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() { items.push(r); }

        // Tasks
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, tags, created_at, path, status FROM tasks"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let tags_str: String = row.get(3)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let content: String = row.get(2)?;
            let preview: String = content.chars().take(150).collect();
            Ok(NexusRow {
                id: row.get(0)?, item_type: "task".to_string(),
                title: row.get(1)?, preview, tags,
                date: row.get(4)?, path: row.get(5)?,
                content,
                status: Some(row.get(6)?),
            })
        }).map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() { items.push(r); }

        // Events
        let mut stmt = self.conn.prepare(
            "SELECT id, title, content, tags, event_date, path FROM events"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let tags_str: String = row.get(3)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            let content: String = row.get(2)?;
            let preview: String = content.chars().take(150).collect();
            Ok(NexusRow {
                id: row.get(0)?, item_type: "event".to_string(),
                title: row.get(1)?, preview, tags,
                date: row.get(4)?, path: row.get(5)?,
                content,
                status: None,
            })
        }).map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() { items.push(r); }

        // QuickCaps
        let mut stmt = self.conn.prepare(
            "SELECT id, content, date, path FROM quickcaps"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let content: String = row.get(1)?;
            let preview: String = content.chars().take(150).collect();
            let extracted_tags: Vec<String> = content.split_whitespace()
                .filter(|w| w.starts_with('#') && w.len() > 1)
                .map(|w| w[1..].to_string())
                .collect();
            Ok(NexusRow {
                id: row.get(0)?, item_type: "quickcap".to_string(),
                title: "⚡ QuickCap".to_string(), preview, tags: extracted_tags,
                date: row.get(2)?, path: row.get(3)?,
                content,
                status: None,
            })
        }).map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() { items.push(r); }

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

        Ok(items)
    }

    pub fn clear_all(&self) -> AppResult<()> {
        self.conn.execute_batch(
            "DELETE FROM notes; DELETE FROM tasks; DELETE FROM events; DELETE FROM quickcaps;"
        ).map_err(|e| AppError::General(format!("DB Clear Error: {}", e)))?;
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
