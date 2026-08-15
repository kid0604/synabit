//! SQLite database layer for the Synabit Mailbox.
//!
//! Uses a single WAL-mode SQLite database to track:
//! - Registered vaults (vault_hash → mailbox_token)
//! - Mailbox entries (encrypted doc snapshots, sequenced per vault)
//! - Per-device cursors (last-ACKed sequence number)
//! - Asset metadata (content-addressed encrypted blobs)
//!
//! All write operations use parameterized queries to prevent injection.
//! The database handle is wrapped in `Arc<Mutex<..>>` so it can be shared
//! across async tasks safely (rusqlite::Connection is !Send without Mutex).

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension};
use std::path::Path;

use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::MailboxEntry;

/// Thread-safe database handle.
#[derive(Clone)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum IdempotencyResult {
    Existing { seq: u64 },
    Conflict,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Created(u64),
    Existing(u64),
    Conflict,
}

impl Database {
    /// Open (or create) the SQLite database at `path` and apply migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path).with_init(|c| {
            c.execute_batch("PRAGMA journal_mode = WAL;")?;
            c.execute_batch("PRAGMA busy_timeout = 5000;")?;
            c.execute_batch("PRAGMA foreign_keys = ON;")
        });
        let pool = Pool::new(manager).context("failed to create connection pool")?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Run schema migrations
    fn migrate(&self) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS vaults (
                vault_hash    TEXT PRIMARY KEY,
                mailbox_token BLOB NOT NULL,
                created_at    INTEGER NOT NULL,
                max_storage_bytes INTEGER NOT NULL DEFAULT 1073741824,
                used_storage_bytes INTEGER NOT NULL DEFAULT 0,
                incarnation_id BLOB,
                bootstrap_state TEXT DEFAULT 'not_ready',
                committed_bytes INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS mailbox (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                vault_hash    TEXT NOT NULL REFERENCES vaults(vault_hash),
                doc_hash      TEXT NOT NULL,
                operation_id  BLOB NOT NULL,
                entry_kind    TEXT NOT NULL,
                source_device TEXT NOT NULL,
                seq           INTEGER NOT NULL,
                blob_path     TEXT NOT NULL,
                blob_size     INTEGER NOT NULL,
                payload_hash  TEXT NOT NULL,
                created_at    INTEGER NOT NULL,
                UNIQUE(vault_hash, seq)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_mailbox_vault_op_id ON mailbox(vault_hash, operation_id);

            CREATE TABLE IF NOT EXISTS devices (
                vault_hash  TEXT NOT NULL REFERENCES vaults(vault_hash),
                device_id   TEXT NOT NULL,
                last_seq    INTEGER NOT NULL DEFAULT 0,
                last_seen   INTEGER NOT NULL,
                status      TEXT NOT NULL DEFAULT 'active',
                bootstrap_required INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(vault_hash, device_id)
            );

            CREATE TABLE IF NOT EXISTS assets (
                vault_hash  TEXT NOT NULL REFERENCES vaults(vault_hash),
                asset_hash  TEXT NOT NULL,
                blob_path   TEXT NOT NULL,
                blob_size   INTEGER NOT NULL,
                created_at  INTEGER NOT NULL,
                PRIMARY KEY(vault_hash, asset_hash)
            );

            CREATE INDEX IF NOT EXISTS idx_mailbox_vault_seq
                ON mailbox(vault_hash, seq);

            CREATE INDEX IF NOT EXISTS idx_mailbox_vault_doc
                ON mailbox(vault_hash, doc_hash);

            CREATE TABLE IF NOT EXISTS vault_sequences (
                vault_hash TEXT PRIMARY KEY REFERENCES vaults(vault_hash),
                seq INTEGER NOT NULL DEFAULT 0
            );

            -- Populate vault_sequences with the maximum seq known so far across mailbox and cursors
            INSERT OR IGNORE INTO vault_sequences (vault_hash, seq)
            SELECT v.vault_hash, 
                   MAX(
                       COALESCE((SELECT MAX(seq) FROM mailbox m WHERE m.vault_hash = v.vault_hash), 0),
                       COALESCE((SELECT MAX(last_seq) FROM devices c WHERE c.vault_hash = v.vault_hash), 0)
                   )
            FROM vaults v;

            CREATE TABLE IF NOT EXISTS trash_meta (
                vault_hash     TEXT NOT NULL,
                doc_hash       BLOB NOT NULL,
                meta_encrypted BLOB NOT NULL,
                deleted_at     INTEGER NOT NULL,
                is_purged      INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(vault_hash, doc_hash)
            );
            ",
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Vault operations
    // -----------------------------------------------------------------------

    /// Look up the stored mailbox_token for a vault. Returns `None` if the
    /// vault has never been registered.
    pub fn get_vault_token(&self, vault_hash: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.pool.get().context("pool get")?;
        let token: Option<Vec<u8>> = conn
            .query_row(
                "SELECT mailbox_token FROM vaults WHERE vault_hash = ?1",
                params![vault_hash],
                |row| row.get(0),
            )
            .optional()?;
        Ok(token)
    }

    /// Register a new vault. Called when the first device connects.
    pub fn register_vault(
        &self,
        vault_hash: &str,
        mailbox_token: &[u8],
        max_storage_bytes: u64,
    ) -> Result<()> {
        let mut conn = self.pool.get().context("pool get")?;
        let tx = conn.transaction()?;
        let now = unix_now();
        let incarnation_id = uuid::Uuid::new_v4().into_bytes();

        tx.execute(
            "INSERT INTO vaults (vault_hash, mailbox_token, created_at, max_storage_bytes, incarnation_id, bootstrap_state, committed_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, 'not_ready', 0)",
            rusqlite::params![vault_hash, mailbox_token, now, max_storage_bytes as i64, incarnation_id.as_slice()],
        )?;

        tx.execute(
            "INSERT INTO vault_sequences (vault_hash, seq) VALUES (?1, 0)",
            rusqlite::params![vault_hash],
        )?;

        tx.commit()?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Mailbox entry operations
    // -----------------------------------------------------------------------

    /// Get the current max sequence number for a vault (0 if empty).
    #[allow(dead_code)]
    pub fn current_seq(&self, vault_hash: &str) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let seq: Option<i64> = conn
            .query_row(
                "SELECT seq FROM vault_sequences WHERE vault_hash = ?1",
                params![vault_hash],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(seq.unwrap_or(0) as u64)
    }

    pub fn get_entry_by_operation_id(
        &self,
        vault_hash: &str,
        operation_id: &[u8; 16],
        doc_hash: &str,
        entry_kind: synabit_protocol::SyncEntryKind,
        payload_hash: &str,
    ) -> Result<IdempotencyResult> {
        let conn = self.pool.get().context("pool get")?;
        let row: Option<(i64, String, String, String)> = conn
            .query_row(
                "SELECT seq, doc_hash, entry_kind, payload_hash FROM mailbox WHERE vault_hash = ? AND operation_id = ?",
                rusqlite::params![vault_hash, operation_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;

        match row {
            Some((seq, existing_doc, existing_kind, existing_payload)) => {
                if existing_doc == doc_hash
                    && existing_kind == entry_kind.to_string()
                    && existing_payload == payload_hash
                {
                    Ok(IdempotencyResult::Existing { seq: seq as u64 })
                } else {
                    Ok(IdempotencyResult::Conflict)
                }
            }
            None => Ok(IdempotencyResult::NotFound),
        }
    }

    /// Insert a new mailbox entry and return the assigned sequence number or existing idempotency outcome.
    #[allow(clippy::too_many_arguments)]
    pub fn push_entry(
        &self,
        vault_hash: &str,
        doc_hash: &str,
        operation_id: &[u8; 16],
        entry_kind: synabit_protocol::SyncEntryKind,
        source_device: &str,
        blob_path: &str,
        blob_size: u64,
        payload_hash: &str,
    ) -> Result<PushOutcome> {
        let mut conn = self.pool.get().context("pool get")?;
        let now = unix_now();
        let tx = conn.transaction()?;

        // Ensure vault_sequences exists for this vault
        tx.execute(
            "INSERT OR IGNORE INTO vault_sequences (vault_hash, seq) VALUES (?1, 0)",
            params![vault_hash],
        )?;

        let next_seq: i64 = tx.query_row(
            "UPDATE vault_sequences SET seq = seq + 1 WHERE vault_hash = ?1 RETURNING seq",
            params![vault_hash],
            |row| row.get(0),
        )?;

        let insert_res = tx.execute(
            "INSERT INTO mailbox (vault_hash, doc_hash, operation_id, entry_kind, source_device, seq, blob_path, blob_size, payload_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                vault_hash,
                doc_hash,
                operation_id.as_slice(),
                entry_kind.to_string(),
                source_device,
                next_seq,
                blob_path,
                blob_size as i64,
                payload_hash,
                now,
            ],
        );

        if let Err(err) = insert_res {
            drop(tx);
            match self.get_entry_by_operation_id(
                vault_hash,
                operation_id,
                doc_hash,
                entry_kind,
                payload_hash,
            )? {
                IdempotencyResult::Existing { seq } => return Ok(PushOutcome::Existing(seq)),
                IdempotencyResult::Conflict => return Ok(PushOutcome::Conflict),
                IdempotencyResult::NotFound => return Err(err.into()),
            }
        }

        tx.execute(
            "UPDATE vaults SET used_storage_bytes = used_storage_bytes + ? WHERE vault_hash = ?",
            params![blob_size as i64, vault_hash],
        )?;

        tx.commit()?;
        Ok(PushOutcome::Created(next_seq as u64))
    }

    /// Pull all entries for a vault with `seq > since_seq`.
    pub fn has_more_entries(
        &self,
        vault_hash: &str,
        after_seq: u64,
        until_seq: u64,
    ) -> Result<bool> {
        let conn = self.pool.get()?;
        let after_i64 = after_seq.min(i64::MAX as u64) as i64;
        let until_i64 = until_seq.min(i64::MAX as u64) as i64;
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM mailbox WHERE vault_hash = ? AND seq > ? AND seq <= ?",
            rusqlite::params![vault_hash, after_i64, until_i64],
            |row: &rusqlite::Row| row.get(0),
        )?;
        Ok(count > 0)
    }







    pub fn get_sync_plan_info(&self, vault_hash: &str) -> Result<(u64, Option<[u8; 16]>, String)> {
        let conn = self.pool.get()?;
        let (seq, inc_id_vec, state): (u64, Option<Vec<u8>>, String) = conn.query_row(
            "SELECT (SELECT seq FROM vault_sequences WHERE vault_hash = ?1), incarnation_id, bootstrap_state FROM vaults WHERE vault_hash = ?1",
            rusqlite::params![vault_hash],
            |row: &rusqlite::Row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        let mut inc_id = None;
        if let Some(vec) = inc_id_vec {
            if vec.len() == 16 {
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&vec);
                inc_id = Some(arr);
            }
        }

        Ok((seq, inc_id, state))
    }

    #[allow(clippy::type_complexity)]
    pub fn pull_page_metadata(
        &self,
        vault_hash: &str,
        after_seq: u64,
        until_seq: u64,
        max_entries: u16,
    ) -> Result<
        Vec<(
            u64,
            [u8; 16],
            synabit_protocol::SyncEntryKind,
            String,
            String,
            String,
            String,
            i64,
        )>,
    > {
        let conn = self.pool.get()?;
        let after_i64 = after_seq.min(i64::MAX as u64) as i64;
        let until_i64 = until_seq.min(i64::MAX as u64) as i64;
        let mut stmt = conn.prepare(
            "SELECT m.seq, m.operation_id, m.entry_kind, 
                    m.doc_hash, m.source_device, m.blob_path, m.payload_hash, m.created_at
             FROM mailbox m
             WHERE m.vault_hash = ?1 AND m.seq > ?2 AND m.seq <= ?3
             ORDER BY m.seq ASC
             LIMIT ?4",
        )?;

        let mut results = Vec::new();
        let rows = stmt.query_map(
            rusqlite::params![vault_hash, after_i64, until_i64, max_entries],
            |row: &rusqlite::Row| {
                let op_id_vec: Vec<u8> = row.get(1)?;
                let mut op_id = [0u8; 16];
                if op_id_vec.len() == 16 {
                    op_id.copy_from_slice(&op_id_vec);
                } else {
                    return Err(rusqlite::Error::InvalidQuery);
                }

                let entry_kind_str: String = row.get(2)?;
                let entry_kind = entry_kind_str
                    .parse()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;

                Ok((
                    row.get(0)?,
                    op_id,
                    entry_kind,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )?;

        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    pub fn pull_entries(&self, vault_hash: &str, since_seq: u64) -> Result<Vec<MailboxEntry>> {
        let conn = self.pool.get().context("pool get")?;
        let mut stmt = conn.prepare(
            "SELECT seq, doc_hash, operation_id, entry_kind, source_device, blob_path, payload_hash, created_at
             FROM mailbox
             WHERE vault_hash = ?1 AND seq > ?2
             ORDER BY seq ASC",
        )?;

        let entries = stmt
            .query_map(params![vault_hash, since_seq as i64], |row| {
                let seq: i64 = row.get(0)?;
                let doc_hash_hex: String = row.get(1)?;
                let op_id_vec: Vec<u8> = row.get(2)?;
                let mut operation_id = [0u8; 16];
                if op_id_vec.len() == 16 {
                    operation_id.copy_from_slice(&op_id_vec);
                } else {
                    return Err(rusqlite::Error::InvalidQuery);
                }

                let entry_kind_str: String = row.get(3)?;
                let entry_kind: synabit_protocol::SyncEntryKind = entry_kind_str
                    .parse()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                let source_device: String = row.get(4)?;
                let blob_path: String = row.get(5)?;
                let payload_hash_hex: String = row.get(6)?;
                let timestamp: i64 = row.get(7)?;

                Ok(PullRow {
                    seq: seq as u64,
                    doc_hash_hex,
                    operation_id,
                    entry_kind,
                    source_device,
                    blob_path,
                    payload_hash_hex,
                    timestamp,
                })
            })?
            .collect::<std::result::Result<Vec<PullRow>, _>>()?;

        let mut result = Vec::with_capacity(entries.len());
        for row in entries {
            let doc_hash = hex_to_hash(&row.doc_hash_hex)?;
            let payload_hash = hex_to_hash(&row.payload_hash_hex)?;
            let encrypted_payload = std::fs::read(&row.blob_path)
                .with_context(|| format!("failed to read blob at {}", row.blob_path))?;

            result.push(MailboxEntry {
                seq: row.seq,
                doc_hash,
                operation_id: row.operation_id,
                entry_kind: row.entry_kind,
                source_device: row.source_device,
                encrypted_payload,
                payload_hash,
                timestamp: row.timestamp,
            });
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Cursor operations
    // -----------------------------------------------------------------------

    /// Update (upsert) the ACK cursor for a device.
    pub fn update_cursor(&self, vault_hash: &str, device_id: &str, last_seq: u64) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO devices (vault_hash, device_id, last_seq, last_seen, status)
             VALUES (?1, ?2, ?3, ?4, 'active')
             ON CONFLICT(vault_hash, device_id)
             DO UPDATE SET last_seq = MAX(last_seq, excluded.last_seq), last_seen = excluded.last_seen",
            params![vault_hash, device_id, last_seq as i64, now],
        )?;
        Ok(())
    }

    /// Touch the `last_seen` timestamp for a device (called on Auth).
    pub fn touch_device(&self, vault_hash: &str, device_id: &str) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        let now = unix_now();
        // Register the device so its status can be read back at the next
        // authentication. Without this the `devices` table stayed empty, so
        // `get_device_status` always answered "unknown" and revocation could
        // never take effect. `status` is deliberately not touched here: a
        // revoked device reconnecting must not re-activate itself.
        conn.execute(
            "INSERT INTO devices (vault_hash, device_id, last_seq, last_seen, status)
             VALUES (?1, ?2, 0, ?3, 'active')
             ON CONFLICT(vault_hash, device_id)
             DO UPDATE SET last_seen = excluded.last_seen",
            params![vault_hash, device_id, now],
        )?;
        Ok(())
    }

    /// Get the minimum `last_seq` across all devices for a vault.
    /// Entries at or below this seq have been ACKed by everyone and can be GC'd.
    pub fn min_cursor(&self, vault_hash: &str) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let min: Option<i64> = conn
            .query_row(
                "SELECT MIN(last_seq) FROM devices WHERE vault_hash = ?1 AND status != 'revoked'",
                params![vault_hash],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(min.unwrap_or(0) as u64)
    }

    // -----------------------------------------------------------------------
    // Asset operations
    // -----------------------------------------------------------------------

    /// Store asset metadata.
    pub fn store_asset(
        &self,
        vault_hash: &str,
        asset_hash: &str,
        blob_path: &str,
        blob_size: u64,
    ) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        let now = unix_now();
        conn.execute(
            "INSERT OR REPLACE INTO assets (vault_hash, asset_hash, blob_path, blob_size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![vault_hash, asset_hash, blob_path, blob_size as i64, now],
        )?;
        Ok(())
    }

    /// Look up the blob path for an asset. Returns `None` if not found.
    pub fn asset_exists(&self, vault_hash: &str, asset_hash: &str) -> Result<Option<u64>> {
        let conn = self.pool.get()?;
        let size: Option<i64> = conn
            .query_row(
                "SELECT blob_size FROM assets WHERE vault_hash = ? AND asset_hash = ?",
                rusqlite::params![vault_hash, asset_hash],
                |row: &rusqlite::Row| row.get(0),
            )
            .optional()?;
        Ok(size.map(|s| s as u64))
    }

    pub fn get_asset_path(&self, vault_hash: &str, asset_hash: &str) -> Result<Option<String>> {
        let conn = self.pool.get().context("pool get")?;
        let path: Option<String> = conn
            .query_row(
                "SELECT blob_path FROM assets WHERE vault_hash = ?1 AND asset_hash = ?2",
                params![vault_hash, asset_hash],
                |row| row.get(0),
            )
            .optional()?;
        Ok(path)
    }

    // -----------------------------------------------------------------------
    // Trash metadata operations
    // -----------------------------------------------------------------------

    /// Store (or replace) trash metadata for a document in a vault.
    pub fn store_trash_meta(
        &self,
        vault_hash: &str,
        doc_hash: &str,
        meta_encrypted: &[u8],
        deleted_at: i64,
    ) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        conn.execute(
            "INSERT OR REPLACE INTO trash_meta (vault_hash, doc_hash, meta_encrypted, deleted_at, is_purged)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![vault_hash, doc_hash, meta_encrypted, deleted_at],
        )?;
        Ok(())
    }

    /// Get all trash metadata entries for a vault.
    pub fn get_trash_meta(&self, vault_hash: &str) -> Result<Vec<TrashMetaRow>> {
        let conn = self.pool.get().context("pool get")?;
        let mut stmt = conn.prepare(
            "SELECT doc_hash, meta_encrypted, deleted_at, is_purged
             FROM trash_meta
             WHERE vault_hash = ?1
             ORDER BY deleted_at ASC",
        )?;
        let rows = stmt
            .query_map(params![vault_hash], |row| {
                Ok(TrashMetaRow {
                    doc_hash: row.get(0)?,
                    meta_encrypted: row.get(1)?,
                    deleted_at: row.get(2)?,
                    is_purged: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<TrashMetaRow>, _>>()?;
        Ok(rows)
    }

    /// Mark a trash metadata entry as purged.
    #[allow(dead_code)]
    pub fn mark_trash_purged(&self, vault_hash: &str, doc_hash: &str) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        conn.execute(
            "UPDATE trash_meta SET is_purged = 1 WHERE vault_hash = ?1 AND doc_hash = ?2",
            params![vault_hash, doc_hash],
        )?;
        Ok(())
    }

    /// Remove a trash metadata entry (used when restoring a document).
    pub fn remove_trash_meta(&self, vault_hash: &str, doc_hash: &str) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        conn.execute(
            "DELETE FROM trash_meta WHERE vault_hash = ?1 AND doc_hash = ?2",
            params![vault_hash, doc_hash],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Cleanup / garbage-collection queries
    // -----------------------------------------------------------------------

    /// Delete all mailbox entries that have been ACKed by all devices (seq ≤ min_cursor)
    /// and return their blob paths so the caller can delete the files.
    pub fn gc_acked_entries(&self, vault_hash: &str, min_seq: u64) -> Result<Vec<String>> {
        let mut conn = self.pool.get().context("pool get")?;
        let tx = conn.transaction()?;

        // Collectable = acknowledged by every device *and* superseded by a later
        // entry for the same document.
        //
        // The second half is what makes the mailbox a usable source of truth. An
        // entry that is still the head of its document is the only remaining
        // record of that document; dropping it because the current devices have
        // acknowledged it leaves the vault reconstructible only by the devices
        // that already hold a copy. A device that joins afterwards replays from
        // the beginning and silently receives an incomplete vault — no error,
        // just missing notes. Keeping heads means a replay from sequence zero
        // always yields the whole vault.
        const COLLECTABLE: &str = "vault_hash = ?1 AND seq <= ?2 AND seq < (
                 SELECT MAX(newer.seq) FROM mailbox AS newer
                 WHERE newer.vault_hash = mailbox.vault_hash
                   AND newer.doc_hash = mailbox.doc_hash
             )";

        let mut stmt = tx.prepare(&format!(
            "SELECT blob_path FROM mailbox WHERE {COLLECTABLE}"
        ))?;
        let paths: Vec<String> = stmt
            .query_map(params![vault_hash, min_seq as i64], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        drop(stmt);

        // Delete the rows and subtract storage quota.
        let freed_bytes: i64 = tx.query_row(
            &format!("SELECT COALESCE(SUM(blob_size), 0) FROM mailbox WHERE {COLLECTABLE}"),
            params![vault_hash, min_seq as i64],
            |row| row.get(0),
        )?;

        tx.execute(
            &format!("DELETE FROM mailbox WHERE {COLLECTABLE}"),
            params![vault_hash, min_seq as i64],
        )?;

        if freed_bytes > 0 {
            tx.execute(
                "UPDATE vaults SET used_storage_bytes = MAX(0, used_storage_bytes - ?) WHERE vault_hash = ?",
                params![freed_bytes, vault_hash],
            )?;
        }

        tx.commit()?;
        Ok(paths)
    }

    /// Delete mailbox entries older than `max_age_secs`, regardless of ACK state.
    /// Returns blob paths for cleanup.
    pub fn gc_old_entries(&self, _max_age_secs: u64) -> Result<Vec<String>> {
        // Disabled until reference graph is tested
        Ok(vec![])
    }

    /// List all vault hashes (for cleanup iteration).
    pub fn list_vault_hashes(&self) -> Result<Vec<String>> {
        let conn = self.pool.get().context("pool get")?;
        let mut stmt = conn.prepare("SELECT vault_hash FROM vaults")?;
        let hashes: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(hashes)
    }

    // -----------------------------------------------------------------------
    // Stats (for health endpoint)
    // -----------------------------------------------------------------------

    /// Count the number of registered vaults.
    pub fn vault_count(&self) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM vaults", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Count total mailbox entries across all vaults.
    pub fn entry_count(&self) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM mailbox", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Count total assets across all vaults.
    pub fn asset_count(&self) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Total blob storage used (mailbox entries + assets) in bytes.
    pub fn total_storage_bytes(&self) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let mailbox_bytes: Option<i64> = conn
            .query_row(
                "SELECT COALESCE(SUM(blob_size), 0) FROM mailbox",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let asset_bytes: Option<i64> = conn
            .query_row(
                "SELECT COALESCE(SUM(blob_size), 0) FROM assets",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok((mailbox_bytes.unwrap_or(0) + asset_bytes.unwrap_or(0)) as u64)
    }

    /// Get total storage used by a specific vault (mailbox + assets) in bytes.
    pub fn total_vault_storage(&self, vault_hash: &str) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let used_bytes: i64 = conn.query_row(
            "SELECT used_storage_bytes FROM vaults WHERE vault_hash = ?1",
            params![vault_hash],
            |row| row.get(0),
        )?;
        Ok(used_bytes as u64)
    }

    /// Get the storage limit for a vault.
    pub fn get_vault_limit(&self, vault_hash: &str) -> Result<u64> {
        let conn = self.pool.get().context("pool get")?;
        let limit: i64 = conn.query_row(
            "SELECT max_storage_bytes FROM vaults WHERE vault_hash = ?1",
            params![vault_hash],
            |row| row.get(0),
        )?;
        Ok(limit as u64)
    }

    /// Delete assets older than cutoff and return blob paths for file removal.
    /// Collect attachment chunks that nothing needs any more.
    ///
    /// Disabled, and it has to be. This deleted every chunk older than the
    /// retention window regardless of whether a live entry still pointed at it,
    /// so a month after an image was shared the chunks vanished while the
    /// reference remained: any device that had not already fetched it — one
    /// joining later, one that had been offline — would find a reference to
    /// nothing, permanently.
    ///
    /// It cannot simply be repaired here. Which chunks a vault still needs is
    /// recorded inside encrypted payloads, and the server cannot read them; it
    /// has no way to tell a referenced chunk from an abandoned one. Collecting
    /// them safely needs the clients, who can read their own references, to say
    /// what is still in use. Until then chunks live for the life of the vault
    /// and the storage quota is what bounds them.
    ///
    /// Superseded chunks do accumulate: editing an image leaves the old one
    /// behind. That is wasted space, which is the right way to be wrong here.
    #[allow(dead_code)]
    pub fn gc_old_assets(&self, _max_age_secs: u64) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    // -----------------------------------------------------------------------
    // Key rotation / device revocation
    // -----------------------------------------------------------------------

    /// Replace the mailbox token for a vault (called after epoch rotation).
    pub fn update_vault_token(&self, vault_hash: &str, new_token: &[u8]) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        conn.execute(
            "UPDATE vaults SET mailbox_token = ?1 WHERE vault_hash = ?2",
            params![new_token, vault_hash],
        )?;
        Ok(())
    }

    /// Rewind a device's cursor to the start of the mailbox.
    ///
    /// The row itself is kept: it carries the device's status, and dropping it
    /// would erase a revocation we had just recorded.
    pub fn reset_device_cursor(&self, vault_hash: &str, device_id: &str) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
        conn.execute(
            "UPDATE devices SET last_seq = 0 WHERE vault_hash = ?1 AND device_id = ?2",
            params![vault_hash, device_id],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Intermediate row type for pull queries (avoids holding the lock while
/// reading blobs from disk).
struct PullRow {
    seq: u64,
    doc_hash_hex: String,
    operation_id: [u8; 16],
    entry_kind: synabit_protocol::SyncEntryKind,
    source_device: String,
    blob_path: String,
    payload_hash_hex: String,
    timestamp: i64,
}

/// Row type for trash metadata queries.
pub struct TrashMetaRow {
    pub doc_hash: String,
    pub meta_encrypted: Vec<u8>,
    pub deleted_at: i64,
    pub is_purged: bool,
}

/// Convert a hex-encoded hash string back to a `[u8; 32]`.
fn hex_to_hash(hex_str: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_str).with_context(|| format!("invalid hex hash: {hex_str}"))?;
    let arr: [u8; 32] = bytes.try_into().map_err(|v: Vec<u8>| {
        anyhow::anyhow!("hash has wrong length: {} (expected 32)", v.len())
    })?;
    Ok(arr)
}

/// Current Unix timestamp in seconds.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl Database {
    pub fn get_device_status(&self, vault_hash: &str, device_id: &str) -> Result<Option<String>> {
        let conn = self.pool.get()?;
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM devices WHERE vault_hash = ? AND device_id = ?",
                rusqlite::params![vault_hash, device_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(status)
    }

    /// Set a device's status, creating the row if the device has never been
    /// seen. Revoking a device that has not connected yet must still stick.
    pub fn set_device_status(&self, vault_hash: &str, device_id: &str, status: &str) -> Result<()> {
        let conn = self.pool.get()?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO devices (vault_hash, device_id, last_seq, last_seen, status)
             VALUES (?2, ?3, 0, ?4, ?1)
             ON CONFLICT(vault_hash, device_id)
             DO UPDATE SET status = excluded.status",
            rusqlite::params![status, vault_hash, device_id, now],
        )?;
        Ok(())
    }
}
