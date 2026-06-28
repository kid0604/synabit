use rusqlite::{params, Connection};
use crate::error::{AppError, AppResult};
use super::DbBridge;

/// Bump this version whenever the FTS5 `search_index` schema changes
/// (e.g. adding/removing columns, changing tokenizer).
/// The index will only be dropped and rebuilt when this version differs
/// from the stored value in `kv_store`.
const FTS_SCHEMA_VERSION: &str = "3";

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

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_crdt_updates_doc_id ON crdt_updates(doc_id);"
        ).map_err(|e| AppError::General(format!("DB Index Error (crdt_updates): {}", e)))?;

        // ─── Identity Mapping (Phase 5) ─────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS document_paths (
                doc_id TEXT PRIMARY KEY,
                rel_path TEXT NOT NULL UNIQUE,
                path_updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (document_paths): {}", e)))?;

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_document_paths_rel_path ON document_paths(rel_path);"
        ).map_err(|e| AppError::General(format!("DB Index Error (document_paths): {}", e)))?;

        // ─── FTS5 Full-Text Search Index (versioned) ─────────────
        // Only DROP + CREATE when the schema version changes.
        // Incremental updates (upsert_search_entry / delete_search_entry)
        // keep the index in sync during normal operation.
        {
            let stored_version: String = conn
                .query_row(
                    "SELECT value FROM kv_store WHERE key = 'fts_schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap_or_default();

            if stored_version != FTS_SCHEMA_VERSION {
                log::info!(
                    "FTS schema version changed ({} → {}), rebuilding search_index...",
                    if stored_version.is_empty() { "none" } else { &stored_version },
                    FTS_SCHEMA_VERSION
                );
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

                // Persist new version + flag for reindex
                conn.execute(
                    "INSERT OR REPLACE INTO kv_store (key, value) VALUES ('fts_schema_version', ?1)",
                    params![FTS_SCHEMA_VERSION],
                ).map_err(|e| AppError::General(format!("DB KV Error (fts_schema_version): {}", e)))?;
                conn.execute(
                    "INSERT OR REPLACE INTO kv_store (key, value) VALUES ('fts_needs_reindex', '1')",
                    [],
                ).map_err(|e| AppError::General(format!("DB KV Error (fts_needs_reindex): {}", e)))?;
            }
        }

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

        // ─── Feed Articles Cache ───────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS feed_articles (
                id TEXT PRIMARY KEY,
                feed_source_id TEXT NOT NULL,
                guid TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                url TEXT NOT NULL DEFAULT '',
                author TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                summary TEXT NOT NULL DEFAULT '',
                published_at TEXT NOT NULL DEFAULT '',
                fetched_at TEXT NOT NULL DEFAULT '',
                thumbnail_url TEXT NOT NULL DEFAULT '',
                word_count INTEGER NOT NULL DEFAULT 0,
                read_time_minutes INTEGER NOT NULL DEFAULT 0,
                content_type TEXT NOT NULL DEFAULT 'text/html',
                is_read INTEGER NOT NULL DEFAULT 0,
                is_starred INTEGER NOT NULL DEFAULT 0,
                is_read_later INTEGER NOT NULL DEFAULT 0,
                UNIQUE(feed_source_id, guid)
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (feed_articles): {}", e)))?;

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_fa_source ON feed_articles(feed_source_id);
             CREATE INDEX IF NOT EXISTS idx_fa_unread ON feed_articles(is_read);
             CREATE INDEX IF NOT EXISTS idx_fa_starred ON feed_articles(is_starred);
             CREATE INDEX IF NOT EXISTS idx_fa_read_later ON feed_articles(is_read_later);
             CREATE INDEX IF NOT EXISTS idx_fa_published ON feed_articles(published_at);",
        )
        .map_err(|e| AppError::General(format!("DB Index Error (feed_articles): {}", e)))?;

        // ─── Feed Articles FTS5 ───────────────────────────────
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS feed_articles_fts USING fts5(
                title,
                author,
                content,
                summary,
                content='feed_articles',
                content_rowid='rowid',
                tokenize = 'unicode61 remove_diacritics 0'
            );",
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (feed_articles_fts): {}", e)))?;

        // ─── Feed Fetch Log ───────────────────────────────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS feed_fetch_log (
                id TEXT PRIMARY KEY,
                feed_source_id TEXT NOT NULL,
                fetched_at TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'ok',
                articles_found INTEGER NOT NULL DEFAULT 0,
                articles_new INTEGER NOT NULL DEFAULT 0,
                error_message TEXT
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (feed_fetch_log): {}", e)))?;

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_ffl_source ON feed_fetch_log(feed_source_id);
             CREATE INDEX IF NOT EXISTS idx_ffl_fetched ON feed_fetch_log(fetched_at);",
        )
        .map_err(|e| AppError::General(format!("DB Index Error (feed_fetch_log): {}", e)))?;

        // ─── Sync Metrics (Phase 4 Mobile Optimization) ───────────
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_metrics (
                date TEXT PRIMARY KEY,
                cellular_bytes_tx INTEGER NOT NULL DEFAULT 0,
                cellular_bytes_rx INTEGER NOT NULL DEFAULT 0,
                wifi_bytes_tx INTEGER NOT NULL DEFAULT 0,
                wifi_bytes_rx INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .map_err(|e| AppError::General(format!("DB Schema Error (sync_metrics): {}", e)))?;

        Ok(Self { conn })
    }
}
