use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};

/// Bump this version whenever the FTS5 `search_index` schema changes
/// (e.g. adding/removing columns, changing tokenizer).
/// The index will only be dropped and rebuilt when this version differs
/// from the stored value in `kv_store`.
const FTS_SCHEMA_VERSION: &str = "3";

impl DbBridge {
    /// Create an in-memory database with sync schema initialized (useful for testing).
    pub fn new_in_memory() -> AppResult<Self> {
        let mut conn = Connection::open_in_memory()
            .map_err(|e| AppError::General(format!("DB Open Error: {}", e)))?;
        run_sync_schema_migrations(&mut conn)?;
        Ok(DbBridge { conn })
    }

    /// Create an in-memory database with the *full* schema (main tables + sync
    /// migrations), matching what `init` produces at app startup.
    ///
    /// `new_in_memory` only builds the sync tables, which is enough for unit
    /// tests that touch the outbox/inbox in isolation. Anything exercising the
    /// real sync path also needs `nodes`, `crdt_documents`, `document_paths`
    /// and `kv_store`, so integration tests use this instead.
    pub fn new_in_memory_full() -> AppResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::General(format!("DB Open Error: {}", e)))?;
        Self::init_with_conn(conn)
    }

    /// Initialize the database once at app startup. Runs all migrations.
    pub fn init<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) -> AppResult<Self> {
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

        Self::init_with_conn(conn)
    }

    /// Build the full schema on an already-open connection.
    ///
    /// Split out from `init` so tests can run the exact production schema
    /// against an in-memory connection instead of a real app data directory.
    pub fn init_with_conn(mut conn: Connection) -> AppResult<Self> {
        // Enable WAL mode for better concurrent read performance and enable foreign keys
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .ok();

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
            "CREATE INDEX IF NOT EXISTS idx_crdt_updates_doc_id ON crdt_updates(doc_id);",
        )
        .map_err(|e| AppError::General(format!("DB Index Error (crdt_updates): {}", e)))?;

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
            "CREATE INDEX IF NOT EXISTS idx_document_paths_rel_path ON document_paths(rel_path);",
        )
        .map_err(|e| AppError::General(format!("DB Index Error (document_paths): {}", e)))?;

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
                    if stored_version.is_empty() {
                        "none"
                    } else {
                        &stored_version
                    },
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

        // ─── Versioned Sync Schema Migrations ─────────────────────
        run_sync_schema_migrations(&mut conn)?;

        Ok(Self { conn })
    }
}

const LATEST_SYNC_SCHEMA_VERSION: i64 = 9;

pub(crate) fn run_sync_schema_migrations(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|e| AppError::General(format!("DB Pragma Error (foreign_keys): {}", e)))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kv_store (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sync_schema_meta (
            singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
            version INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync_schema_meta): {}", e)))?;

    let stored_version: Option<i64> = conn
        .query_row(
            "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| AppError::General(format!("Failed to read sync schema version: {}", e)))?;

    let mut current_version = stored_version.unwrap_or(0);

    if current_version < 0 {
        return Err(AppError::General(format!(
            "Invalid negative sync schema version {} in database",
            current_version
        )));
    }

    if current_version > LATEST_SYNC_SCHEMA_VERSION {
        return Err(AppError::General(format!(
            "Database sync schema version {} is newer than binary supported version {}",
            current_version, LATEST_SYNC_SCHEMA_VERSION
        )));
    }

    while current_version < LATEST_SYNC_SCHEMA_VERSION {
        let next_version = current_version + 1;
        match next_version {
            1 => migrate_sync_schema_v1(conn)?,
            2 => migrate_sync_schema_v2(conn)?,
            3 => migrate_sync_schema_v3(conn)?,
            4 => migrate_sync_schema_v4(conn)?,
            5 => migrate_sync_schema_v5(conn)?,
            6 => migrate_sync_schema_v6(conn)?,
            7 => migrate_sync_schema_v7(conn)?,
            8 => migrate_sync_schema_v8(conn)?,
            9 => migrate_sync_schema_v9(conn)?,
            _ => {
                return Err(AppError::General(format!(
                    "No migration defined for version {}",
                    next_version
                )))
            }
        }
        current_version = next_version;
    }

    Ok(())
}

pub(crate) fn migrate_sync_schema_v1(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    tx.execute_batch(
        "CREATE TABLE sync_vaults (
            vault_id TEXT PRIMARY KEY,
            canonical_root TEXT NOT NULL UNIQUE,
            metadata_version INTEGER NOT NULL DEFAULT 1,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            CHECK (length(vault_id) > 0 AND length(vault_id) <= 128),
            CHECK (length(canonical_root) > 0 AND length(canonical_root) <= 16384),
            CHECK (metadata_version > 0 AND metadata_version <= 4294967295),
            CHECK (created_at >= 0),
            CHECK (updated_at >= created_at)
        );

        CREATE TABLE sync_provider_state (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            cursor TEXT NOT NULL DEFAULT '',
            ack_cursor TEXT,
            sync_state TEXT NOT NULL DEFAULT 'ready',
            incarnation_id BLOB,
            remote_vault_id BLOB,
            last_error TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (vault_id, provider_id),
            FOREIGN KEY (vault_id) REFERENCES sync_vaults(vault_id) ON DELETE CASCADE,
            CHECK (length(vault_id) > 0 AND length(vault_id) <= 128),
            CHECK (length(provider_id) > 0 AND length(provider_id) <= 64),
            CHECK (length(cursor) <= 16384),
            CHECK (ack_cursor IS NULL OR length(ack_cursor) <= 16384),
            CHECK (sync_state IN ('ready', 'bootstrap_required', 'bootstrapping', 'error', 'disabled')),
            CHECK (incarnation_id IS NULL OR (typeof(incarnation_id) = 'blob' AND length(incarnation_id) = 16)),
            CHECK (remote_vault_id IS NULL OR (typeof(remote_vault_id) = 'blob' AND length(remote_vault_id) = 32)),
            CHECK (created_at >= 0),
            CHECK (updated_at >= created_at)
        );

        CREATE INDEX idx_sync_provider_state_status ON sync_provider_state(vault_id, provider_id, sync_state);",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync v1 migration): {}", e)))?;

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 1, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v1): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v1: {}", e)))?;

    Ok(())
}

pub(crate) fn migrate_sync_schema_v2(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    tx.execute_batch(
        "CREATE TABLE sync_outbox (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            operation_id BLOB NOT NULL,
            entry_kind TEXT NOT NULL,
            node_id TEXT NOT NULL,
            rel_path TEXT,
            source_hash BLOB,
            original_timestamp INTEGER NOT NULL,
            encrypted_payload BLOB,
            payload_hash BLOB,
            asset_ref_blob BLOB,
            state TEXT NOT NULL DEFAULT 'prepared',
            retry_count INTEGER NOT NULL DEFAULT 0,
            next_retry_at INTEGER,
            last_error TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (vault_id, provider_id, operation_id),
            FOREIGN KEY (vault_id, provider_id)
                REFERENCES sync_provider_state(vault_id, provider_id)
                ON DELETE CASCADE,
            CHECK (length(operation_id) = 16),
            CHECK (source_hash IS NULL OR length(source_hash) = 32),
            CHECK (payload_hash IS NULL OR length(payload_hash) = 32),
            CHECK (retry_count >= 0),
            CHECK (entry_kind IN ('upsert', 'delete', 'asset_reference')),
            CHECK (state IN ('prepared', 'uploading_assets', 'ready', 'sent', 'acknowledged', 'failed'))
        );

        CREATE INDEX idx_sync_outbox_dispatch ON sync_outbox(vault_id, provider_id, state, next_retry_at, created_at);
        CREATE INDEX idx_sync_outbox_source ON sync_outbox(vault_id, provider_id, node_id, source_hash);

        CREATE TABLE sync_inbox (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            page_cursor TEXT NOT NULL DEFAULT '',
            remote_position TEXT NOT NULL,
            remote_seq INTEGER,
            operation_id BLOB NOT NULL,
            doc_hash BLOB NOT NULL,
            entry_kind TEXT NOT NULL,
            encrypted_payload BLOB,
            payload_hash BLOB,
            source_device TEXT,
            state TEXT NOT NULL DEFAULT 'pending',
            last_error TEXT,
            received_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            applied_at INTEGER,
            PRIMARY KEY (vault_id, provider_id, operation_id),
            FOREIGN KEY (vault_id, provider_id)
                REFERENCES sync_provider_state(vault_id, provider_id)
                ON DELETE CASCADE,
            CHECK (length(operation_id) = 16),
            CHECK (length(doc_hash) = 32),
            CHECK (payload_hash IS NULL OR length(payload_hash) = 32),
            CHECK (remote_seq IS NULL OR remote_seq >= 0),
            CHECK (entry_kind IN ('upsert', 'delete', 'asset_reference')),
            CHECK (state IN ('pending', 'applying', 'pending_asset', 'applied', 'ignored_own_operation', 'failed', 'quarantined'))
        );

        CREATE INDEX idx_sync_inbox_apply ON sync_inbox(vault_id, provider_id, state, remote_seq, received_at);
        CREATE INDEX idx_sync_inbox_page ON sync_inbox(vault_id, provider_id, page_cursor, remote_position);",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync v2 migration): {}", e)))?;

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 2, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v2): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v2: {}", e)))?;

    Ok(())
}

pub(crate) fn migrate_sync_schema_v3(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    tx.execute_batch(
        "CREATE TABLE sync_pending_assets (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            asset_id BLOB NOT NULL,
            operation_id BLOB NOT NULL,
            doc_hash BLOB NOT NULL,
            mime_type TEXT NOT NULL DEFAULT '',
            total_bytes INTEGER NOT NULL,
            plaintext_hash BLOB NOT NULL,
            asset_ref BLOB NOT NULL,
            state TEXT NOT NULL DEFAULT 'pending',
            retry_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (vault_id, provider_id, asset_id),
            FOREIGN KEY (vault_id, provider_id)
                REFERENCES sync_provider_state(vault_id, provider_id)
                ON DELETE CASCADE,
            CHECK (length(asset_id) = 32),
            CHECK (length(operation_id) = 16),
            CHECK (length(doc_hash) = 32),
            CHECK (length(plaintext_hash) = 32),
            CHECK (total_bytes >= 0),
            CHECK (retry_count >= 0),
            CHECK (state IN ('pending', 'downloading', 'ready', 'failed'))
        );

        CREATE INDEX idx_sync_pending_assets_fetch ON sync_pending_assets(vault_id, provider_id, state, created_at);

        CREATE TABLE sync_bootstrap_sessions (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            session_id BLOB NOT NULL,
            incarnation_id BLOB NOT NULL,
            base_seq INTEGER NOT NULL,
            item_count INTEGER NOT NULL,
            downloaded_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (vault_id, provider_id, session_id),
            FOREIGN KEY (vault_id, provider_id)
                REFERENCES sync_provider_state(vault_id, provider_id)
                ON DELETE CASCADE,
            CHECK (length(session_id) = 16),
            CHECK (length(incarnation_id) = 16),
            CHECK (base_seq >= 0),
            CHECK (item_count >= 0),
            CHECK (downloaded_count >= 0)
        );

        CREATE TABLE sync_bootstrap_items (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            session_id BLOB NOT NULL,
            position INTEGER NOT NULL,
            doc_hash BLOB NOT NULL,
            head_seq INTEGER NOT NULL,
            operation_id BLOB NOT NULL,
            entry_kind TEXT NOT NULL,
            payload_hash BLOB NOT NULL,
            source_device TEXT NOT NULL,
            encrypted_payload BLOB,
            timestamp INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (vault_id, provider_id, session_id, position),
            FOREIGN KEY (vault_id, provider_id, session_id)
                REFERENCES sync_bootstrap_sessions(vault_id, provider_id, session_id)
                ON DELETE CASCADE,
            CHECK (length(session_id) = 16),
            CHECK (length(doc_hash) = 32),
            CHECK (length(operation_id) = 16),
            CHECK (length(payload_hash) = 32),
            CHECK (position >= 0),
            CHECK (head_seq >= 0),
            CHECK (entry_kind IN ('upsert', 'delete', 'asset_reference'))
        );",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync v3 migration): {}", e)))?;

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 3, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v3): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v3: {}", e)))?;

    Ok(())
}

pub(crate) fn migrate_sync_schema_v4(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    tx.execute_batch(
        "CREATE TABLE sync_crdt_documents (
            vault_id TEXT NOT NULL,
            doc_id TEXT NOT NULL,
            snapshot BLOB NOT NULL,
            updated_at INTEGER NOT NULL,
            -- PRIMARY KEY vault_id, doc_id)
            PRIMARY KEY (vault_id, doc_id),
            -- FOREIGN KEY vault_id REFERENCES sync_vaults
            FOREIGN KEY (vault_id) REFERENCES sync_vaults(vault_id) ON DELETE CASCADE
        );

        CREATE TABLE sync_crdt_updates (
            vault_id TEXT NOT NULL,
            doc_id TEXT NOT NULL,
            update_id INTEGER NOT NULL,
            delta BLOB NOT NULL,
            timestamp INTEGER NOT NULL,
            -- PRIMARY KEY vault_id, update_id)
            PRIMARY KEY (vault_id, update_id),
            -- FOREIGN KEY vault_id REFERENCES sync_vaults
            FOREIGN KEY (vault_id) REFERENCES sync_vaults(vault_id) ON DELETE CASCADE
        );

        CREATE TABLE sync_document_paths (
            vault_id TEXT NOT NULL,
            doc_id TEXT NOT NULL,
            rel_path TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            -- PRIMARY KEY vault_id, doc_id)
            PRIMARY KEY (vault_id, doc_id),
            UNIQUE (vault_id, rel_path),
            -- FOREIGN KEY vault_id REFERENCES sync_vaults
            FOREIGN KEY (vault_id) REFERENCES sync_vaults(vault_id) ON DELETE CASCADE
        );

        CREATE TABLE sync_document_baselines (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            rel_path TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (vault_id, provider_id, rel_path),
            FOREIGN KEY (vault_id, provider_id) REFERENCES sync_provider_state(vault_id, provider_id) ON DELETE CASCADE
        );

        CREATE TABLE sync_legacy_backup_rows (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            migration_version INTEGER NOT NULL,
            source_order INTEGER NOT NULL,
            source_table TEXT NOT NULL,
            source_key TEXT NOT NULL,
            raw_payload BLOB NOT NULL,
            backed_up_at INTEGER NOT NULL,
            UNIQUE (migration_version, source_table, source_key)
        );

        CREATE TABLE sync_legacy_migration_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            migration_version INTEGER NOT NULL,
            status TEXT NOT NULL,
            decision TEXT NOT NULL CHECK (decision IN ('Migrated', 'BootstrapRequired', 'AlreadyComplete')),
            vault_id TEXT,
            completed_at INTEGER,
            last_error TEXT,
            migrated_at INTEGER NOT NULL
        );",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync v4 migration): {}", e)))?;

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 4, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v4): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v4: {}", e)))?;

    Ok(())
}

pub(crate) fn migrate_sync_schema_v5(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    let column_exists = tx
        .prepare("SELECT doc_hash FROM sync_outbox LIMIT 1")
        .is_ok();
    if !column_exists {
        tx.execute_batch(
            "ALTER TABLE sync_outbox ADD COLUMN doc_hash BLOB CHECK (doc_hash IS NULL OR length(doc_hash) = 32);",
        ).map_err(|e| AppError::General(format!("Failed to add doc_hash column: {}", e)))?;
    }

    let mut stmt = tx
        .prepare("SELECT vault_id, provider_id, operation_id, rel_path, doc_hash FROM sync_outbox")
        .map_err(|e| AppError::General(format!("Failed to prepare query: {}", e)))?;

    let rows: Vec<(String, String, Vec<u8>, Option<String>, Option<Vec<u8>>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|e| AppError::General(format!("Failed to execute query: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::General(format!("Failed to read rows: {}", e)))?;

    drop(stmt);

    let mut update_doc_hash_stmt = tx.prepare(
        "UPDATE sync_outbox SET doc_hash = ?1 WHERE vault_id = ?2 AND provider_id = ?3 AND operation_id = ?4"
    ).map_err(|e| AppError::General(format!("Failed to prepare update: {}", e)))?;

    let mut quarantine_stmt = tx.prepare(
        "UPDATE sync_outbox SET state = 'failed', next_retry_at = NULL, last_error = 'Legacy row is missing reconstruction metadata' WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3"
    ).map_err(|e| AppError::General(format!("Failed to prepare quarantine: {}", e)))?;

    for (vault_id, provider_id, operation_id, rel_path, doc_hash) in rows {
        if doc_hash.is_some() {
            continue;
        }

        if let Some(path) = rel_path {
            if !path.trim().is_empty() {
                let hash = blake3::hash(path.as_bytes());
                update_doc_hash_stmt
                    .execute(params![
                        hash.as_bytes().to_vec(),
                        vault_id,
                        provider_id,
                        operation_id
                    ])
                    .map_err(|e| AppError::General(format!("Failed to execute update: {}", e)))?;
                continue;
            }
        }

        quarantine_stmt
            .execute(params![vault_id, provider_id, operation_id])
            .map_err(|e| AppError::General(format!("Failed to execute quarantine: {}", e)))?;
    }

    drop(update_doc_hash_stmt);
    drop(quarantine_stmt);

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 5, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v5): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v5: {}", e)))?;

    Ok(())
}

pub(crate) fn migrate_sync_schema_v6(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    tx.execute_batch(
        "-- CREATE TABLE sync_inbox_pages
        CREATE TABLE sync_inbox_pages (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            start_cursor TEXT NOT NULL,
            next_cursor TEXT NOT NULL,
            has_more INTEGER NOT NULL CHECK (has_more IN (0, 1)),
            entry_count INTEGER NOT NULL CHECK (entry_count >= 0 AND entry_count <= 1000),
            state TEXT NOT NULL DEFAULT 'staged',
            received_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (vault_id, provider_id, start_cursor),
            FOREIGN KEY (vault_id, provider_id)
                REFERENCES sync_provider_state(vault_id, provider_id)
                ON DELETE CASCADE,
            CHECK (length(vault_id) > 0 AND length(vault_id) <= 128),
            CHECK (length(provider_id) > 0 AND length(provider_id) <= 64),
            CHECK (length(start_cursor) <= 16384),
            CHECK (length(next_cursor) <= 16384),
            CHECK (state IN ('staged', 'applied', 'cursor_committed')),
            CHECK (received_at >= 0),
            CHECK (updated_at >= received_at)
        );

        -- CREATE TABLE sync_inbox_page_entries
        CREATE TABLE sync_inbox_page_entries (
            vault_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            start_cursor TEXT NOT NULL,
            page_ordinal INTEGER NOT NULL CHECK (page_ordinal >= 0),
            operation_id BLOB NOT NULL,
            PRIMARY KEY (vault_id, provider_id, start_cursor, page_ordinal),
            UNIQUE (vault_id, provider_id, start_cursor, operation_id),
            FOREIGN KEY (vault_id, provider_id, start_cursor)
                REFERENCES sync_inbox_pages(vault_id, provider_id, start_cursor)
                ON DELETE CASCADE,
            FOREIGN KEY (vault_id, provider_id, operation_id)
                REFERENCES sync_inbox(vault_id, provider_id, operation_id)
                ON DELETE CASCADE,
            CHECK (length(operation_id) = 16)
        );

        CREATE INDEX IF NOT EXISTS idx_sync_inbox_page_entries_lookup ON sync_inbox_page_entries(vault_id, provider_id, start_cursor, page_ordinal);",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync v6 migration): {}", e)))?;

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 6, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v6): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v6: {}", e)))?;

    Ok(())
}

/// Remember where each acknowledged operation landed in the mailbox.
///
/// The server's sequence is the only total order every device agrees on.
/// Recording it lets a device tell that a tombstone arriving from a peer is
/// older than work it has already published for the same document, and so must
/// not be applied — without it, the device that made the newer edit is the one
/// device that ends up deleting the file.
pub(crate) fn migrate_sync_schema_v9(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    tx.execute_batch(
        "ALTER TABLE sync_outbox ADD COLUMN remote_seq INTEGER;
         CREATE INDEX IF NOT EXISTS idx_sync_outbox_node_seq
             ON sync_outbox(vault_id, provider_id, node_id, remote_seq);",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync v9 migration): {}", e)))?;

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 9, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v9): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v9: {}", e)))?;

    Ok(())
}

/// Give inbox entries a retry budget.
///
/// Without one, an entry that fails to apply can only either block the whole
/// page forever or be dropped on its first hiccup. Counting attempts lets a
/// transient failure be retried a few times and a permanent one be quarantined
/// so it stops holding up every entry behind it.
pub(crate) fn migrate_sync_schema_v8(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();
    let tx = conn
        .transaction()
        .map_err(|e| AppError::General(format!("Failed to start sync schema tx: {}", e)))?;

    tx.execute_batch(
        "ALTER TABLE sync_inbox ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;",
    )
    .map_err(|e| AppError::General(format!("DB Schema Error (sync v8 migration): {}", e)))?;

    tx.execute(
        "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
         VALUES (1, 8, ?1)
         ON CONFLICT(singleton_id) DO UPDATE SET
             version = excluded.version,
             updated_at = excluded.updated_at;",
        params![now],
    )
    .map_err(|e| {
        AppError::General(format!(
            "DB Schema Error (sync_schema_meta update v8): {}",
            e
        ))
    })?;

    tx.commit()
        .map_err(|e| AppError::General(format!("Failed to commit sync schema v8: {}", e)))?;

    Ok(())
}

pub(crate) fn migrate_sync_schema_v7(conn: &mut Connection) -> AppResult<()> {
    let now = chrono::Utc::now().timestamp();

    let declared_type: Option<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT type FROM pragma_table_info('sync_provider_state') WHERE name = 'remote_vault_id'",
            )
            .map_err(|e| {
                AppError::General(format!(
                    "remote_vault_id schema inspection prepare error: {}",
                    e
                ))
            })?;
        stmt.query_row([], |row| row.get(0))
            .optional()
            .map_err(|e| {
                AppError::General(format!(
                    "remote_vault_id schema inspection query error: {}",
                    e
                ))
            })?
    };

    let declared_type = match declared_type {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            return Err(AppError::General(
                "remote_vault_id schema inspection failed: missing column".into(),
            ));
        }
    };

    let needs_table_rebuild = declared_type.to_ascii_uppercase() != "BLOB";

    if needs_table_rebuild {
        conn.execute_batch("PRAGMA foreign_keys = OFF;")
            .map_err(|e| AppError::General(format!("Failed to disable foreign keys: {}", e)))?;
    }

    let rebuild_res: AppResult<()> = (|| {
        let tx = conn.transaction().map_err(|e| {
            AppError::General(format!("rebuild sync_provider_state tx start error: {}", e))
        })?;

        if needs_table_rebuild {
            tx.execute_batch(
                "CREATE TABLE sync_provider_state_v7 (
                    vault_id TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    cursor TEXT NOT NULL DEFAULT '',
                    ack_cursor TEXT,
                    sync_state TEXT NOT NULL DEFAULT 'ready',
                    incarnation_id BLOB,
                    remote_vault_id BLOB,
                    last_error TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    PRIMARY KEY (vault_id, provider_id),
                    FOREIGN KEY (vault_id) REFERENCES sync_vaults(vault_id) ON DELETE CASCADE,
                    CHECK (length(vault_id) > 0 AND length(vault_id) <= 128),
                    CHECK (length(provider_id) > 0 AND length(provider_id) <= 64),
                    CHECK (length(cursor) <= 16384),
                    CHECK (ack_cursor IS NULL OR length(ack_cursor) <= 16384),
                    CHECK (sync_state IN ('ready', 'bootstrap_required', 'bootstrapping', 'error', 'disabled')),
                    CHECK (incarnation_id IS NULL OR (typeof(incarnation_id) = 'blob' AND length(incarnation_id) = 16)),
                    CHECK (remote_vault_id IS NULL OR (typeof(remote_vault_id) = 'blob' AND length(remote_vault_id) = 32)),
                    CHECK (created_at >= 0),
                    CHECK (updated_at >= created_at)
                );",
            )
            .map_err(|e| {
                AppError::General(format!("rebuild sync_provider_state table error: {}", e))
            })?;

            tx.execute(
                "INSERT INTO sync_provider_state_v7 (
                    vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id,
                    remote_vault_id, last_error, created_at, updated_at
                )
                SELECT
                    vault_id,
                    provider_id,
                    cursor,
                    ack_cursor,
                    CASE
                        WHEN remote_vault_id IS NOT NULL AND (typeof(remote_vault_id) != 'blob' OR length(remote_vault_id) != 32)
                        THEN (CASE WHEN sync_state = 'disabled' THEN 'disabled' ELSE 'bootstrap_required' END)
                        ELSE sync_state
                    END,
                    CASE
                        WHEN incarnation_id IS NOT NULL AND (typeof(incarnation_id) != 'blob' OR length(incarnation_id) != 16)
                        THEN NULL
                        ELSE incarnation_id
                    END,
                    CASE
                        WHEN remote_vault_id IS NOT NULL AND (typeof(remote_vault_id) != 'blob' OR length(remote_vault_id) != 32)
                        THEN NULL
                        ELSE remote_vault_id
                    END,
                    CASE
                        WHEN remote_vault_id IS NOT NULL AND (typeof(remote_vault_id) != 'blob' OR length(remote_vault_id) != 32)
                        THEN 'legacy remote_vault_id: incompatible storage class or length'
                        ELSE last_error
                    END,
                    created_at,
                    CASE
                        WHEN remote_vault_id IS NOT NULL AND (typeof(remote_vault_id) != 'blob' OR length(remote_vault_id) != 32)
                        THEN (CASE WHEN updated_at >= ?1 THEN updated_at ELSE ?1 END)
                        ELSE updated_at
                    END
                FROM sync_provider_state;",
                params![now],
            )
            .map_err(|e| {
                AppError::General(format!("rebuild sync_provider_state data copy error: {}", e))
            })?;

            tx.execute_batch(
                "DROP TABLE sync_provider_state;
                ALTER TABLE sync_provider_state_v7 RENAME TO sync_provider_state;
                CREATE INDEX IF NOT EXISTS idx_sync_provider_state_status ON sync_provider_state(vault_id, provider_id, sync_state);",
            )
            .map_err(|e| {
                AppError::General(format!("rebuild sync_provider_state finalize error: {}", e))
            })?;
        } else {
            tx.execute(
                "UPDATE sync_provider_state
                 SET
                     sync_state = CASE WHEN sync_state = 'disabled' THEN 'disabled' ELSE 'bootstrap_required' END,
                     remote_vault_id = NULL,
                     last_error = 'legacy remote_vault_id: incompatible storage class or length',
                     updated_at = CASE WHEN updated_at >= ?1 THEN updated_at ELSE ?1 END
                 WHERE remote_vault_id IS NOT NULL AND (typeof(remote_vault_id) != 'blob' OR length(remote_vault_id) != 32)",
                params![now],
            )
            .map_err(|e| AppError::General(format!("rebuild sync_provider_state normalize error: {}", e)))?;
        }

        let fk_violations: i64 = tx
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .map_err(|e| {
                AppError::General(format!(
                    "rebuild sync_provider_state foreign key check query error: {}",
                    e
                ))
            })?;
        if fk_violations > 0 {
            return Err(AppError::General(format!(
                "rebuild sync_provider_state foreign key check failed with {} violations",
                fk_violations
            )));
        }

        tx.execute(
            "INSERT INTO sync_schema_meta (singleton_id, version, updated_at)
             VALUES (1, 7, ?1)
             ON CONFLICT(singleton_id) DO UPDATE SET
                 version = excluded.version,
                 updated_at = excluded.updated_at;",
            params![now],
        )
        .map_err(|e| {
            AppError::General(format!(
                "DB Schema Error (sync_schema_meta update v7): {}",
                e
            ))
        })?;

        tx.commit().map_err(|e| {
            AppError::General(format!("rebuild sync_provider_state commit error: {}", e))
        })?;

        Ok(())
    })();

    if needs_table_rebuild {
        let restore_res = conn.execute_batch("PRAGMA foreign_keys = ON;");
        if let Err(e) = rebuild_res {
            if let Err(fk_err) = restore_res {
                return Err(AppError::General(format!(
                    "rebuild sync_provider_state error: {}, additionally failed to re-enable foreign keys: {}",
                    e, fk_err
                )));
            }
            return Err(AppError::General(format!(
                "rebuild sync_provider_state error: {}",
                e
            )));
        }
        if let Err(fk_err) = restore_res {
            return Err(AppError::General(format!(
                "Failed to re-enable foreign keys: {}",
                fk_err
            )));
        }
    } else if let Err(e) = rebuild_res {
        return Err(e);
    }

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn fresh_sync_schema_v5_creates_vault_scoped_legacy_targets() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).expect("v5 migration should succeed");

        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, LATEST_SYNC_SCHEMA_VERSION);

        let tables = vec![
            "sync_crdt_documents",
            "sync_crdt_updates",
            "sync_document_paths",
            "sync_document_baselines",
            "sync_legacy_backup_rows",
            "sync_legacy_migration_state",
        ];
        for t in tables {
            let count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'",
                        t
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "Table {} must exist", t);
        }
    }

    #[test]
    fn fresh_sync_schema_creates_vault_and_provider_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).expect("migration should succeed");

        let meta_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_schema_meta'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(meta_exists, 1);

        let vaults_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_vaults'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(vaults_exists, 1);

        let provider_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_provider_state'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(provider_exists, 1);

        let index_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_sync_provider_state_status'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(index_exists, 1);

        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, LATEST_SYNC_SCHEMA_VERSION);

        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN (
                    'sync_vaults', 'sync_provider_state', 'sync_outbox', 'sync_inbox',
                    'sync_pending_assets', 'sync_bootstrap_sessions', 'sync_bootstrap_items'
                )",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            table_count, 7,
            "All 7 required sync state tables must be created by v3 migration"
        );
    }

    #[test]
    fn sync_schema_migration_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/vault1', 100, 100)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v1', 'gdrive', 'cur_1', 100, 100)",
            [],
        )
        .unwrap();

        run_sync_schema_migrations(&mut conn).unwrap();

        let vault_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_vaults WHERE vault_id = 'v1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(vault_count, 1);

        let provider_cursor: String = conn
            .query_row(
                "SELECT cursor FROM sync_provider_state WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(provider_cursor, "cur_1");

        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, LATEST_SYNC_SCHEMA_VERSION);
    }

    #[test]
    fn provider_state_rejects_unknown_vault() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).unwrap();

        let res = conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('unknown_vault', 'gdrive', 'cur', 100, 100)",
            [],
        );
        assert!(
            res.is_err(),
            "Inserting provider_state without matching sync_vaults row must fail FK constraint"
        );
    }

    #[test]
    fn failed_sync_schema_migration_does_not_advance_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Pre-create conflicting table sync_provider_state ONLY with a dummy column.
        // Do NOT pre-create sync_vaults!
        conn.execute("CREATE TABLE sync_provider_state (dummy TEXT);", [])
            .unwrap();

        let result = run_sync_schema_migrations(&mut conn);
        assert!(
            result.is_err(),
            "Migration must fail when conflicting sync_provider_state exists"
        );

        // Verify sync_vaults created inside transaction was rolled back
        let vaults_exist: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_vaults'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            vaults_exist, 0,
            "sync_vaults created inside transaction must be rolled back on failure"
        );

        // Verify conflicting sync_provider_state created outside transaction remains with dummy column
        let dummy_col_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sync_provider_state') WHERE name = 'dummy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            dummy_col_count, 1,
            "Pre-existing table outside transaction remains"
        );

        // Verify version in sync_schema_meta is 0 or row not present (not 1)
        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(
            version, 0,
            "Version must not advance to 1 on failed migration"
        );
    }

    #[test]
    fn sync_schema_rejects_newer_version() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Bootstrap sync_schema_meta and insert version 99 (newer than binary supported version 2)
        conn.execute_batch(
            "CREATE TABLE sync_schema_meta (
                singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                version INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            INSERT INTO sync_schema_meta (singleton_id, version, updated_at) VALUES (1, 99, 1000);",
        )
        .unwrap();

        let result = run_sync_schema_migrations(&mut conn);
        assert!(
            result.is_err(),
            "Runner must fail when database version is newer than binary"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("99") && err_msg.contains(&LATEST_SYNC_SCHEMA_VERSION.to_string()),
            "Error message must contain stored version 99 and binary latest version {}, got: {}",
            LATEST_SYNC_SCHEMA_VERSION,
            err_msg
        );

        // Ensure no sync_vaults or sync_provider_state tables were created
        let tables_exist: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('sync_vaults', 'sync_provider_state')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            tables_exist, 0,
            "No tables should be created when version is rejected"
        );
    }

    #[test]
    fn sync_schema_meta_query_errors_are_not_treated_as_zero() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Create malformed sync_schema_meta table without the 'version' column
        conn.execute_batch(
            "CREATE TABLE sync_schema_meta (
                singleton_id INTEGER PRIMARY KEY,
                bad_column TEXT
            );",
        )
        .unwrap();

        let result = run_sync_schema_migrations(&mut conn);
        assert!(
            result.is_err(),
            "Runner must fail when querying malformed metadata table"
        );

        // Ensure migration v1 was NOT executed
        let tables_exist: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('sync_vaults', 'sync_provider_state')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            tables_exist, 0,
            "Migration v1 must not run on metadata query error"
        );
    }

    #[test]
    fn provider_state_is_scoped_by_vault_and_provider() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('vA', '/vaultA', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('vB', '/vaultB', 200, 200)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('vA', 'gdrive', 'cur_A_gdrive', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('vA', 'server', 'cur_A_server', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('vB', 'gdrive', 'cur_B_gdrive', 200, 200)",
            [],
        )
        .unwrap();

        let cur_a_gdrive: String = conn
            .query_row(
                "SELECT cursor FROM sync_provider_state WHERE vault_id = 'vA' AND provider_id = 'gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cur_a_gdrive, "cur_A_gdrive");

        let cur_a_server: String = conn
            .query_row(
                "SELECT cursor FROM sync_provider_state WHERE vault_id = 'vA' AND provider_id = 'server'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cur_a_server, "cur_A_server");

        let cur_b_gdrive: String = conn
            .query_row(
                "SELECT cursor FROM sync_provider_state WHERE vault_id = 'vB' AND provider_id = 'gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cur_b_gdrive, "cur_B_gdrive");
    }

    #[test]
    fn fresh_sync_schema_v2_creates_outbox_and_inbox() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).expect("migration should succeed");

        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, LATEST_SYNC_SCHEMA_VERSION);

        let outbox_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_outbox'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(outbox_exists, 1);

        let inbox_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_inbox'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(inbox_exists, 1);

        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('idx_sync_outbox_dispatch', 'idx_sync_outbox_source', 'idx_sync_inbox_apply', 'idx_sync_inbox_page')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(index_count, 4);

        // Check key columns in outbox
        let outbox_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('sync_outbox')")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        for required_col in &[
            "operation_id",
            "entry_kind",
            "original_timestamp",
            "asset_ref_blob",
            "state",
        ] {
            assert!(
                outbox_cols.contains(&required_col.to_string()),
                "sync_outbox missing column {}",
                required_col
            );
        }

        // Check key columns in inbox
        let inbox_cols: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('sync_inbox')")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        for required_col in &[
            "page_cursor",
            "remote_position",
            "remote_seq",
            "operation_id",
            "state",
        ] {
            assert!(
                inbox_cols.contains(&required_col.to_string()),
                "sync_inbox missing column {}",
                required_col
            );
        }
    }

    #[test]
    fn upgrade_v1_to_v2_preserves_vault_and_provider_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Bootstrap metadata
        conn.execute_batch(
            "CREATE TABLE sync_schema_meta (
                singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                version INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .unwrap();

        // Run v1 migration directly
        migrate_sync_schema_v1(&mut conn).unwrap();

        // Insert a vault and provider state row
        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/vault1', 100, 100)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v1', 'gdrive', 'cur_v1', 100, 100)",
            [],
        )
        .unwrap();

        // Run runner to upgrade v1 -> v2
        run_sync_schema_migrations(&mut conn).unwrap();

        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, LATEST_SYNC_SCHEMA_VERSION);

        let cursor: String = conn
            .query_row(
                "SELECT cursor FROM sync_provider_state WHERE vault_id='v1' AND provider_id='gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cursor, "cur_v1");

        let vault_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_vaults WHERE vault_id='v1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(vault_count, 1);
    }

    #[test]
    fn outbox_is_scoped_and_deduplicated_by_operation_id() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'gdrive', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'server', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v2', 'gdrive', 200, 200)",
            [],
        )
        .unwrap();

        let op_id = vec![1u8; 16];

        // Insert into (v1, gdrive, op_id) -> OK
        conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at)
             VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 100, 100, 100)",
            params![op_id],
        )
        .unwrap();

        // Duplicate insert into same (v1, gdrive, op_id) -> FAIL
        let dup = conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at)
             VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 100, 100, 100)",
            params![op_id],
        );
        assert!(
            dup.is_err(),
            "Duplicate operation_id in same vault+provider scope must fail PK"
        );

        // Insert same op_id into (v1, server) -> OK
        conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at)
             VALUES ('v1', 'server', ?1, 'upsert', 'n1', 100, 100, 100)",
            params![op_id],
        )
        .unwrap();

        // Insert same op_id into (v2, gdrive) -> OK
        conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at)
             VALUES ('v2', 'gdrive', ?1, 'upsert', 'n1', 100, 100, 100)",
            params![op_id],
        )
        .unwrap();

        // Unknown provider state parent -> FAIL FK
        let bad_fk = conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at)
             VALUES ('unknown', 'gdrive', ?1, 'upsert', 'n1', 100, 100, 100)",
            params![op_id],
        );
        assert!(bad_fk.is_err(), "Non-existent vault_id must fail FK");

        // Invalid operation_id length -> FAIL CHECK
        let bad_op_len = conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at)
             VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 100, 100, 100)",
            params![vec![1u8; 10]],
        );
        assert!(
            bad_op_len.is_err(),
            "operation_id length != 16 must fail CHECK"
        );

        // Invalid entry_kind -> FAIL CHECK
        let bad_kind = conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at)
             VALUES ('v1', 'gdrive', ?1, 'invalid_kind', 'n1', 100, 100, 100)",
            params![vec![2u8; 16]],
        );
        assert!(bad_kind.is_err(), "Invalid entry_kind must fail CHECK");

        // Invalid state -> FAIL CHECK
        let bad_state = conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, state, original_timestamp, created_at, updated_at)
             VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 'invalid_state', 100, 100, 100)",
            params![vec![2u8; 16]],
        );
        assert!(bad_state.is_err(), "Invalid state must fail CHECK");
    }

    #[test]
    fn inbox_is_scoped_and_uses_numeric_remote_seq() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'server', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v2', 'server', 200, 200)",
            [],
        )
        .unwrap();

        let doc_hash = vec![2u8; 32];

        // Insert remote_seq 1, 10, 2 out of order into inbox
        conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, received_at, updated_at)
             VALUES ('v1', 'server', '10', 10, ?1, ?2, 'upsert', 100, 100)",
            params![vec![10u8; 16], doc_hash],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, received_at, updated_at)
             VALUES ('v1', 'server', '1', 1, ?1, ?2, 'upsert', 100, 100)",
            params![vec![1u8; 16], doc_hash],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, received_at, updated_at)
             VALUES ('v1', 'server', '2', 2, ?1, ?2, 'upsert', 100, 100)",
            params![vec![2u8; 16], doc_hash],
        )
        .unwrap();

        // Query ORDER BY remote_seq
        let seqs: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT remote_seq FROM sync_inbox WHERE vault_id='v1' AND provider_id='server' ORDER BY remote_seq")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert_eq!(
            seqs,
            vec![1, 2, 10],
            "remote_seq ordering must be numeric [1, 2, 10]"
        );

        // Duplicate operation_id in same scope -> FAIL
        let dup = conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, received_at, updated_at)
             VALUES ('v1', 'server', '1', 1, ?1, ?2, 'upsert', 100, 100)",
            params![vec![1u8; 16], doc_hash],
        );
        assert!(
            dup.is_err(),
            "Duplicate operation_id in same scope must fail PK"
        );

        // Same operation_id in different scope (v2, server) -> OK
        conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, received_at, updated_at)
             VALUES ('v2', 'server', '1', 1, ?1, ?2, 'upsert', 100, 100)",
            params![vec![1u8; 16], doc_hash],
        )
        .unwrap();

        // Invalid doc_hash length -> FAIL CHECK
        let bad_doc_hash = conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, received_at, updated_at)
             VALUES ('v1', 'server', '99', 99, ?1, ?2, 'upsert', 100, 100)",
            params![vec![99u8; 16], vec![1u8; 10]],
        );
        assert!(
            bad_doc_hash.is_err(),
            "doc_hash length != 32 must fail CHECK"
        );

        // Invalid entry_kind -> FAIL CHECK
        let bad_kind = conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, received_at, updated_at)
             VALUES ('v1', 'server', '99', 99, ?1, ?2, 'invalid', 100, 100)",
            params![vec![99u8; 16], doc_hash],
        );
        assert!(bad_kind.is_err(), "Invalid entry_kind must fail CHECK");

        // Invalid state -> FAIL CHECK
        let bad_state = conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, remote_seq, operation_id, doc_hash, entry_kind, state, received_at, updated_at)
             VALUES ('v1', 'server', '99', 99, ?1, ?2, 'upsert', 'invalid_state', 100, 100)",
            params![vec![99u8; 16], doc_hash],
        );
        assert!(bad_state.is_err(), "Invalid state must fail CHECK");
    }

    #[test]
    fn failed_sync_schema_v2_migration_rolls_back_without_damaging_v1() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Bootstrap metadata
        conn.execute_batch(
            "CREATE TABLE sync_schema_meta (
                singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                version INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .unwrap();

        // Run v1 migration directly and insert vault/provider rows
        migrate_sync_schema_v1(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v1', 'gdrive', 'cur_v1', 100, 100)",
            [],
        )
        .unwrap();

        // Pre-create conflicting sync_inbox table ONLY with a dummy column.
        // Do NOT pre-create sync_outbox!
        conn.execute("CREATE TABLE sync_inbox (dummy TEXT);", [])
            .unwrap();

        // Run migration runner (attempts v2 upgrade)
        let result = run_sync_schema_migrations(&mut conn);
        assert!(
            result.is_err(),
            "Migration v2 must fail when conflicting sync_inbox table exists"
        );

        // Verify sync_outbox created inside v2 transaction was rolled back
        let outbox_exist: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_outbox'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            outbox_exist, 0,
            "sync_outbox created inside v2 transaction must be rolled back on failure"
        );

        // Verify conflicting sync_inbox created outside transaction remains with dummy column
        let dummy_col_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sync_inbox') WHERE name = 'dummy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            dummy_col_count, 1,
            "Pre-existing sync_inbox table outside transaction remains"
        );

        // Verify version in sync_schema_meta remains 1
        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 1, "Version must remain 1 on failed v2 migration");

        // Verify existing v1 vault and provider state rows remain intact
        let cursor: String = conn
            .query_row(
                "SELECT cursor FROM sync_provider_state WHERE vault_id='v1' AND provider_id='gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cursor, "cur_v1", "v1 provider state data must be undamaged");
    }

    #[test]
    fn upgrade_v2_to_v3_preserves_vault_provider_outbox_inbox_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_schema_meta (singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1), version INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
            [],
        ).unwrap();
        migrate_sync_schema_v1(&mut conn).unwrap();
        migrate_sync_schema_v2(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults VALUES ('v1', '/vault1', 1, 100, 100);",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v1', 'gdrive', 'cur_v2', 100, 100);",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at) VALUES ('v1', 'gdrive', x'01010101010101010101010101010101', 'upsert', 'node1', 100, 100, 100);",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, operation_id, doc_hash, entry_kind, received_at, updated_at) VALUES ('v1', 'gdrive', 'pos1', x'01010101010101010101010101010101', x'0202020202020202020202020202020202020202020202020202020202020202', 'upsert', 100, 100);",
            [],
        ).unwrap();

        // Run V3 migration directly
        migrate_sync_schema_v3(&mut conn).unwrap();

        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 3);

        let cursor: String = conn
            .query_row(
                "SELECT cursor FROM sync_provider_state WHERE vault_id='v1' AND provider_id='gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cursor, "cur_v2");

        let outbox_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_outbox WHERE vault_id='v1' AND provider_id='gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(outbox_count, 1, "V2 outbox row must be preserved");

        let inbox_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_inbox WHERE vault_id='v1' AND provider_id='gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(inbox_count, 1, "V2 inbox row must be preserved");
    }

    #[test]
    fn failed_sync_schema_v3_migration_rolls_back_without_damaging_v2() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_schema_meta (singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1), version INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
            [],
        ).unwrap();
        migrate_sync_schema_v1(&mut conn).unwrap();
        migrate_sync_schema_v2(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults VALUES ('v1', '/vault1', 1, 100, 100);",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v1', 'gdrive', 'cur_v2', 100, 100);",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp, created_at, updated_at) VALUES ('v1', 'gdrive', x'01010101010101010101010101010101', 'upsert', 'node1', 100, 100, 100);",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sync_inbox (vault_id, provider_id, remote_position, operation_id, doc_hash, entry_kind, received_at, updated_at) VALUES ('v1', 'gdrive', 'pos1', x'01010101010101010101010101010101', x'0202020202020202020202020202020202020202020202020202020202020202', 'upsert', 100, 100);",
            [],
        ).unwrap();

        // Create dummy sync_bootstrap_sessions table with incompatible schema to force v3 migration error
        conn.execute_batch("CREATE TABLE sync_bootstrap_sessions (dummy TEXT PRIMARY KEY);")
            .unwrap();

        let res = migrate_sync_schema_v3(&mut conn);
        assert!(res.is_err());

        // Verify version in sync_schema_meta remains 2
        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 2, "Version must remain 2 on failed v3 migration");

        // Verify all V2 data remains intact
        let outbox_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_outbox WHERE vault_id='v1' AND provider_id='gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            outbox_count, 1,
            "V2 outbox row must remain undamaged on failed v3 migration"
        );

        let inbox_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_inbox WHERE vault_id='v1' AND provider_id='gdrive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            inbox_count, 1,
            "V2 inbox row must remain undamaged on failed v3 migration"
        );

        // Verify sync_pending_assets (created first during V3 migration) was rolled back
        let pending_assets_table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_pending_assets'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            pending_assets_table_count, 0,
            "sync_pending_assets table created before migration failure must be rolled back"
        );

        // Verify intentional conflicting table remains
        let conflicting_table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sync_bootstrap_sessions'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            conflicting_table_count, 1,
            "Pre-existing conflicting table must remain present"
        );
    }

    #[test]
    fn legacy_outbox_v5_migration_preserves_rows_and_quarantines_unreconstructable_records() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sync_schema_meta (singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1), version INTEGER NOT NULL, updated_at INTEGER NOT NULL);",
            [],
        ).unwrap();
        migrate_sync_schema_v1(&mut conn).unwrap();
        migrate_sync_schema_v2(&mut conn).unwrap();
        migrate_sync_schema_v3(&mut conn).unwrap();
        migrate_sync_schema_v4(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at)
             VALUES ('v1', 'root1', 1, 0, 0)",
            []
        ).unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, sync_state, created_at, updated_at)
             VALUES ('v1', 'gdrive', '', 'ready', 0, 0)",
            []
        ).unwrap();

        let op1 = vec![1u8; 16];
        conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, original_timestamp, state, retry_count, created_at, updated_at)
             VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 'foo/bar.md', 100, 'prepared', 0, 100, 100)",
            params![op1]
        ).unwrap();

        let op2 = vec![2u8; 16];
        conn.execute(
            "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, original_timestamp, state, retry_count, created_at, updated_at)
             VALUES ('v1', 'gdrive', ?1, 'delete', 'n2', NULL, 200, 'prepared', 0, 200, 200)",
            params![op2]
        ).unwrap();

        let before_rows: Vec<(Vec<u8>, String, Option<String>)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT operation_id, state, rel_path FROM sync_outbox ORDER BY operation_id",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(before_rows.len(), 2);

        migrate_sync_schema_v5(&mut conn).expect("v5 migration should succeed");

        let after_rows: Vec<(Vec<u8>, String, Option<String>, Option<Vec<u8>>)> = {
            let mut stmt = conn.prepare("SELECT operation_id, state, rel_path, doc_hash FROM sync_outbox ORDER BY operation_id").unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };

        let after_reopen: Vec<(Vec<u8>, String, Option<String>, Option<Vec<u8>>)> =
            after_rows.clone();
        assert_eq!(after_reopen, after_rows);

        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, 5);

        let row1_hash: Option<Vec<u8>> = conn
            .query_row(
                "SELECT doc_hash FROM sync_outbox WHERE operation_id = ?1",
                params![op1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            row1_hash,
            Some(blake3::hash(b"foo/bar.md").as_bytes().to_vec())
        );
        let row1_state: String = conn
            .query_row(
                "SELECT state FROM sync_outbox WHERE operation_id = ?1",
                params![op1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row1_state, "prepared");

        let row2_state: String = conn
            .query_row(
                "SELECT state FROM sync_outbox WHERE operation_id = ?1",
                params![op2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row2_state, "failed");
        let row2_retry: Option<i64> = conn
            .query_row(
                "SELECT next_retry_at FROM sync_outbox WHERE operation_id = ?1",
                params![op2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row2_retry, None);
        let row2_error: String = conn
            .query_row(
                "SELECT last_error FROM sync_outbox WHERE operation_id = ?1",
                params![op2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row2_error, "Legacy row is missing reconstruction metadata");

        migrate_sync_schema_v5(&mut conn).expect("v5 migration should be idempotent");
    }

    #[test]
    fn legacy_outbox_v5_migration_complete_snapshot_survives_real_reopen() {
        let tempdir = tempfile::tempdir().unwrap();
        let db_path = tempdir.path().join("test_migration.db");

        let op1 = vec![1u8; 16];
        let op2 = vec![2u8; 16];

        #[derive(Debug, PartialEq, Eq)]
        struct OutboxV4Snapshot {
            vault_id: String,
            provider_id: String,
            operation_id: Vec<u8>,
            entry_kind: String,
            node_id: String,
            rel_path: Option<String>,
            source_hash: Option<Vec<u8>>,
            original_timestamp: i64,
            encrypted_payload: Option<Vec<u8>>,
            payload_hash: Option<Vec<u8>>,
            asset_ref_blob: Option<Vec<u8>>,
            state: String,
            retry_count: i64,
            next_retry_at: Option<i64>,
            last_error: Option<String>,
            created_at: i64,
            updated_at: i64,
        }

        #[derive(Debug, PartialEq, Eq)]
        struct OutboxV5Snapshot {
            vault_id: String,
            provider_id: String,
            operation_id: Vec<u8>,
            entry_kind: String,
            node_id: String,
            rel_path: Option<String>,
            doc_hash: Option<Vec<u8>>,
            source_hash: Option<Vec<u8>>,
            original_timestamp: i64,
            encrypted_payload: Option<Vec<u8>>,
            payload_hash: Option<Vec<u8>>,
            asset_ref_blob: Option<Vec<u8>>,
            state: String,
            retry_count: i64,
            next_retry_at: Option<i64>,
            last_error: Option<String>,
            created_at: i64,
            updated_at: i64,
        }

        let after_rows: Vec<OutboxV5Snapshot> = {
            let mut conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS sync_schema_meta (
                    singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                    version INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );",
            )
            .unwrap();
            migrate_sync_schema_v1(&mut conn).unwrap();
            migrate_sync_schema_v2(&mut conn).unwrap();
            migrate_sync_schema_v3(&mut conn).unwrap();
            migrate_sync_schema_v4(&mut conn).unwrap();

            conn.execute("INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v1', 'root1', 1, 0, 0)", []).unwrap();
            conn.execute("INSERT INTO sync_provider_state (vault_id, provider_id, cursor, sync_state, created_at, updated_at) VALUES ('v1', 'gdrive', '', 'ready', 0, 0)", []).unwrap();

            conn.execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, rel_path,
                    source_hash, original_timestamp, encrypted_payload, payload_hash,
                    asset_ref_blob, state, retry_count, next_retry_at, last_error,
                    created_at, updated_at
                ) VALUES (
                    'v1', 'gdrive', ?1, 'upsert', 'node1', 'foo/bar.md',
                    ?2, 1000, ?3, ?4,
                    NULL, 'prepared', 1, 500, 'some error',
                    100, 200
                )",
                params![op1, vec![9u8; 32], vec![7u8; 10], vec![8u8; 32]],
            )
            .unwrap();

            conn.execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, rel_path,
                    source_hash, original_timestamp, encrypted_payload, payload_hash,
                    asset_ref_blob, state, retry_count, next_retry_at, last_error,
                    created_at, updated_at
                ) VALUES (
                    'v1', 'gdrive', ?1, 'delete', 'node2', NULL,
                    NULL, 2000, NULL, NULL,
                    NULL, 'ready', 0, NULL, NULL,
                    300, 400
                )",
                params![op2],
            )
            .unwrap();

            let before_rows: Vec<OutboxV4Snapshot> = {
                let mut stmt = conn
                    .prepare(
                        "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path,
                                source_hash, original_timestamp, encrypted_payload, payload_hash,
                                asset_ref_blob, state, retry_count, next_retry_at, last_error,
                                created_at, updated_at
                         FROM sync_outbox ORDER BY operation_id",
                    )
                    .unwrap();
                stmt.query_map([], |r| {
                    Ok(OutboxV4Snapshot {
                        vault_id: r.get(0)?,
                        provider_id: r.get(1)?,
                        operation_id: r.get(2)?,
                        entry_kind: r.get(3)?,
                        node_id: r.get(4)?,
                        rel_path: r.get(5)?,
                        source_hash: r.get(6)?,
                        original_timestamp: r.get(7)?,
                        encrypted_payload: r.get(8)?,
                        payload_hash: r.get(9)?,
                        asset_ref_blob: r.get(10)?,
                        state: r.get(11)?,
                        retry_count: r.get(12)?,
                        next_retry_at: r.get(13)?,
                        last_error: r.get(14)?,
                        created_at: r.get(15)?,
                        updated_at: r.get(16)?,
                    })
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
            };
            assert_eq!(before_rows.len(), 2);

            migrate_sync_schema_v5(&mut conn).unwrap();

            let after_rows: Vec<OutboxV5Snapshot> = {
                let mut stmt = conn
                    .prepare(
                        "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path,
                                doc_hash, source_hash, original_timestamp, encrypted_payload, payload_hash,
                                asset_ref_blob, state, retry_count, next_retry_at, last_error,
                                created_at, updated_at
                         FROM sync_outbox ORDER BY operation_id",
                    )
                    .unwrap();
                stmt.query_map([], |r| {
                    Ok(OutboxV5Snapshot {
                        vault_id: r.get(0)?,
                        provider_id: r.get(1)?,
                        operation_id: r.get(2)?,
                        entry_kind: r.get(3)?,
                        node_id: r.get(4)?,
                        rel_path: r.get(5)?,
                        doc_hash: r.get(6)?,
                        source_hash: r.get(7)?,
                        original_timestamp: r.get(8)?,
                        encrypted_payload: r.get(9)?,
                        payload_hash: r.get(10)?,
                        asset_ref_blob: r.get(11)?,
                        state: r.get(12)?,
                        retry_count: r.get(13)?,
                        next_retry_at: r.get(14)?,
                        last_error: r.get(15)?,
                        created_at: r.get(16)?,
                        updated_at: r.get(17)?,
                    })
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
            };
            assert_eq!(after_rows.len(), 2);
            let expected_after: Vec<OutboxV5Snapshot> = vec![
                OutboxV5Snapshot {
                    vault_id: "v1".into(),
                    provider_id: "gdrive".into(),
                    operation_id: op1.clone(),
                    entry_kind: "upsert".into(),
                    node_id: "node1".into(),
                    rel_path: Some("foo/bar.md".into()),
                    doc_hash: Some(blake3::hash(b"foo/bar.md").as_bytes().to_vec()),
                    source_hash: Some(vec![9u8; 32]),
                    original_timestamp: 1000,
                    encrypted_payload: Some(vec![7u8; 10]),
                    payload_hash: Some(vec![8u8; 32]),
                    asset_ref_blob: None,
                    state: "prepared".into(),
                    retry_count: 1,
                    next_retry_at: Some(500),
                    last_error: Some("some error".into()),
                    created_at: 100,
                    updated_at: 200,
                },
                OutboxV5Snapshot {
                    vault_id: "v1".into(),
                    provider_id: "gdrive".into(),
                    operation_id: op2.clone(),
                    entry_kind: "delete".into(),
                    node_id: "node2".into(),
                    rel_path: None,
                    doc_hash: None,
                    source_hash: None,
                    original_timestamp: 2000,
                    encrypted_payload: None,
                    payload_hash: None,
                    asset_ref_blob: None,
                    state: "failed".into(),
                    retry_count: 0,
                    next_retry_at: None,
                    last_error: Some("Legacy row is missing reconstruction metadata".into()),
                    created_at: 300,
                    updated_at: 400,
                },
            ];
            assert_eq!(after_rows, expected_after);
            drop(conn);
            after_rows
        };

        {
            let mut conn = Connection::open(&db_path).unwrap();

            let after_reopen: Vec<OutboxV5Snapshot> = {
                let mut stmt = conn
                    .prepare(
                        "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path,
                                doc_hash, source_hash, original_timestamp, encrypted_payload, payload_hash,
                                asset_ref_blob, state, retry_count, next_retry_at, last_error,
                                created_at, updated_at
                         FROM sync_outbox ORDER BY operation_id",
                    )
                    .unwrap();
                stmt.query_map([], |r| {
                    Ok(OutboxV5Snapshot {
                        vault_id: r.get(0)?,
                        provider_id: r.get(1)?,
                        operation_id: r.get(2)?,
                        entry_kind: r.get(3)?,
                        node_id: r.get(4)?,
                        rel_path: r.get(5)?,
                        doc_hash: r.get(6)?,
                        source_hash: r.get(7)?,
                        original_timestamp: r.get(8)?,
                        encrypted_payload: r.get(9)?,
                        payload_hash: r.get(10)?,
                        asset_ref_blob: r.get(11)?,
                        state: r.get(12)?,
                        retry_count: r.get(13)?,
                        next_retry_at: r.get(14)?,
                        last_error: r.get(15)?,
                        created_at: r.get(16)?,
                        updated_at: r.get(17)?,
                    })
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
            };

            assert_eq!(after_reopen, after_rows);

            migrate_sync_schema_v5(&mut conn).unwrap();
            drop(conn);
        }

        {
            let mut conn = Connection::open(&db_path).unwrap();

            let after_second_reopen: Vec<OutboxV5Snapshot> = {
                let mut stmt = conn
                    .prepare(
                        "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path,
                                doc_hash, source_hash, original_timestamp, encrypted_payload, payload_hash,
                                asset_ref_blob, state, retry_count, next_retry_at, last_error,
                                created_at, updated_at
                         FROM sync_outbox ORDER BY operation_id",
                    )
                    .unwrap();
                stmt.query_map([], |r| {
                    Ok(OutboxV5Snapshot {
                        vault_id: r.get(0)?,
                        provider_id: r.get(1)?,
                        operation_id: r.get(2)?,
                        entry_kind: r.get(3)?,
                        node_id: r.get(4)?,
                        rel_path: r.get(5)?,
                        doc_hash: r.get(6)?,
                        source_hash: r.get(7)?,
                        original_timestamp: r.get(8)?,
                        encrypted_payload: r.get(9)?,
                        payload_hash: r.get(10)?,
                        asset_ref_blob: r.get(11)?,
                        state: r.get(12)?,
                        retry_count: r.get(13)?,
                        next_retry_at: r.get(14)?,
                        last_error: r.get(15)?,
                        created_at: r.get(16)?,
                        updated_at: r.get(17)?,
                    })
                })
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
            };

            assert_eq!(after_second_reopen, after_rows);
        }
    }
}

#[cfg(test)]
#[rustfmt::skip]
#[path = "../../../.agents/oracles/c2b_arch_closure_v2.rs"]
mod c2b_arch_closure_v2;

#[cfg(test)]
#[rustfmt::skip]
#[path = "../../../.agents/oracles/c2b_arch_closure_v3.rs"]
mod c2b_arch_closure_v3;

#[cfg(test)]
#[rustfmt::skip]
#[path = "../../../.agents/oracles/c2b_arch_closure_v4.rs"]
mod c2b_arch_closure_v4;
