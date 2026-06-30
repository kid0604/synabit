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
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::MailboxEntry;

/// Thread-safe database handle.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    /// Open (or create) the SQLite database at `path` and apply migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database at {}", path.display()))?;

        // Enable WAL mode for better concurrent-read performance.
        conn.execute_batch("PRAGMA journal_mode = WAL;")?;
        // Set a busy timeout so concurrent writers don't fail immediately.
        conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
        // Enable foreign-key enforcement.
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Run schema migrations (idempotent — uses IF NOT EXISTS).
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS vaults (
                vault_hash    TEXT PRIMARY KEY,
                mailbox_token BLOB NOT NULL,
                created_at    INTEGER NOT NULL,
                max_storage_bytes INTEGER NOT NULL DEFAULT 1073741824
            );

            CREATE TABLE IF NOT EXISTS mailbox (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                vault_hash    TEXT NOT NULL REFERENCES vaults(vault_hash),
                doc_hash      TEXT NOT NULL,
                source_device TEXT NOT NULL,
                seq           INTEGER NOT NULL,
                blob_path     TEXT NOT NULL,
                blob_size     INTEGER NOT NULL,
                payload_hash  TEXT NOT NULL,
                created_at    INTEGER NOT NULL,
                is_delete     INTEGER NOT NULL DEFAULT 0,
                UNIQUE(vault_hash, seq)
            );

            CREATE TABLE IF NOT EXISTS cursors (
                vault_hash  TEXT NOT NULL REFERENCES vaults(vault_hash),
                device_id   TEXT NOT NULL,
                last_seq    INTEGER NOT NULL DEFAULT 0,
                last_seen   INTEGER NOT NULL,
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
                       COALESCE((SELECT MAX(last_seq) FROM cursors c WHERE c.vault_hash = v.vault_hash), 0)
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
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
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
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO vaults (vault_hash, mailbox_token, created_at, max_storage_bytes)
             VALUES (?1, ?2, ?3, ?4)",
            params![vault_hash, mailbox_token, now, max_storage_bytes as i64],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Mailbox entry operations
    // -----------------------------------------------------------------------

    /// Get the current max sequence number for a vault (0 if empty).
    pub fn current_seq(&self, vault_hash: &str) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
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

    /// Insert a new mailbox entry and return the assigned sequence number.
    pub fn push_entry(
        &self,
        vault_hash: &str,
        doc_hash: &str,
        source_device: &str,
        blob_path: &str,
        blob_size: u64,
        payload_hash: &str,
        is_delete: bool,
    ) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = unix_now();

        // Ensure vault_sequences exists for this vault
        conn.execute(
            "INSERT OR IGNORE INTO vault_sequences (vault_hash, seq) VALUES (?1, 0)",
            params![vault_hash],
        )?;

        let next_seq: i64 = conn
            .query_row(
                "UPDATE vault_sequences SET seq = seq + 1 WHERE vault_hash = ?1 RETURNING seq",
                params![vault_hash],
                |row| row.get(0),
            )?;

        conn.execute(
            "INSERT INTO mailbox (vault_hash, doc_hash, source_device, seq, blob_path, blob_size, payload_hash, created_at, is_delete)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                vault_hash,
                doc_hash,
                source_device,
                next_seq,
                blob_path,
                blob_size as i64,
                payload_hash,
                now,
                is_delete
            ],
        )?;
        Ok(next_seq as u64)
    }

    /// Pull all entries for a vault with `seq > since_seq`.
    pub fn pull_entries(&self, vault_hash: &str, since_seq: u64) -> Result<Vec<MailboxEntry>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT seq, doc_hash, source_device, blob_path, payload_hash, created_at, is_delete, blob_size
             FROM mailbox
             WHERE vault_hash = ?1 AND seq > ?2
             ORDER BY seq ASC",
        )?;

        let entries = stmt
            .query_map(params![vault_hash, since_seq as i64], |row| {
                let seq: i64 = row.get(0)?;
                let doc_hash_hex: String = row.get(1)?;
                let source_device: String = row.get(2)?;
                let blob_path: String = row.get(3)?;
                let payload_hash_hex: String = row.get(4)?;
                let timestamp: i64 = row.get(5)?;
                let is_delete: bool = row.get(6)?;
                let _blob_size: i64 = row.get(7)?;

                Ok(PullRow {
                    seq: seq as u64,
                    doc_hash_hex,
                    source_device,
                    blob_path,
                    payload_hash_hex,
                    timestamp,
                    is_delete,
                })
            })?
            .collect::<std::result::Result<Vec<PullRow>, _>>()?;

        // Convert rows to MailboxEntry, loading blob data from disk.
        let mut result = Vec::with_capacity(entries.len());
        for row in entries {
            let doc_hash = hex_to_hash(&row.doc_hash_hex)?;
            let payload_hash = hex_to_hash(&row.payload_hash_hex)?;
            let encrypted_payload = if row.is_delete {
                Vec::new()
            } else {
                std::fs::read(&row.blob_path).with_context(|| {
                    format!("failed to read blob at {}", row.blob_path)
                })?
            };

            result.push(MailboxEntry {
                seq: row.seq,
                doc_hash,
                source_device: row.source_device,
                encrypted_payload,
                payload_hash,
                timestamp: row.timestamp,
                is_delete: row.is_delete,
            });
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Cursor operations
    // -----------------------------------------------------------------------

    /// Update (upsert) the ACK cursor for a device.
    pub fn update_cursor(
        &self,
        vault_hash: &str,
        device_id: &str,
        last_seq: u64,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO cursors (vault_hash, device_id, last_seq, last_seen)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(vault_hash, device_id)
             DO UPDATE SET last_seq = MAX(last_seq, excluded.last_seq), last_seen = excluded.last_seen",
            params![vault_hash, device_id, last_seq as i64, now],
        )?;
        Ok(())
    }

    /// Touch the `last_seen` timestamp for a device (called on Auth).
    pub fn touch_device(&self, vault_hash: &str, device_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = unix_now();
        conn.execute(
            "INSERT INTO cursors (vault_hash, device_id, last_seq, last_seen)
             VALUES (?1, ?2, 0, ?3)
             ON CONFLICT(vault_hash, device_id)
             DO UPDATE SET last_seen = excluded.last_seen",
            params![vault_hash, device_id, now],
        )?;
        Ok(())
    }

    /// Get the minimum `last_seq` across all devices for a vault.
    /// Entries at or below this seq have been ACKed by everyone and can be GC'd.
    pub fn min_cursor(&self, vault_hash: &str) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let min: Option<i64> = conn
            .query_row(
                "SELECT MIN(last_seq) FROM cursors WHERE vault_hash = ?1",
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
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let now = unix_now();
        conn.execute(
            "INSERT OR REPLACE INTO assets (vault_hash, asset_hash, blob_path, blob_size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![vault_hash, asset_hash, blob_path, blob_size as i64, now],
        )?;
        Ok(())
    }

    /// Look up the blob path for an asset. Returns `None` if not found.
    pub fn get_asset_path(
        &self,
        vault_hash: &str,
        asset_hash: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
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
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        conn.execute(
            "INSERT OR REPLACE INTO trash_meta (vault_hash, doc_hash, meta_encrypted, deleted_at, is_purged)
             VALUES (?1, ?2, ?3, ?4, 0)",
            params![vault_hash, doc_hash, meta_encrypted, deleted_at],
        )?;
        Ok(())
    }

    /// Get all trash metadata entries for a vault.
    pub fn get_trash_meta(&self, vault_hash: &str) -> Result<Vec<TrashMetaRow>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
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
    pub fn mark_trash_purged(&self, vault_hash: &str, doc_hash: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        conn.execute(
            "UPDATE trash_meta SET is_purged = 1 WHERE vault_hash = ?1 AND doc_hash = ?2",
            params![vault_hash, doc_hash],
        )?;
        Ok(())
    }

    /// Remove a trash metadata entry (used when restoring a document).
    pub fn remove_trash_meta(&self, vault_hash: &str, doc_hash: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
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
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;

        // Collect blob paths first.
        let mut stmt = conn.prepare(
            "SELECT blob_path FROM mailbox WHERE vault_hash = ?1 AND seq <= ?2",
        )?;
        let paths: Vec<String> = stmt
            .query_map(params![vault_hash, min_seq as i64], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;

        // Delete the rows.
        conn.execute(
            "DELETE FROM mailbox WHERE vault_hash = ?1 AND seq <= ?2",
            params![vault_hash, min_seq as i64],
        )?;

        Ok(paths)
    }

    /// Delete mailbox entries older than `max_age_secs`, regardless of ACK state.
    /// Returns blob paths for cleanup.
    pub fn gc_old_entries(&self, max_age_secs: u64) -> Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let cutoff = unix_now() - max_age_secs as i64;

        let mut stmt = conn.prepare(
            "SELECT blob_path FROM mailbox WHERE created_at < ?1",
        )?;
        let paths: Vec<String> = stmt
            .query_map(params![cutoff], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;

        conn.execute("DELETE FROM mailbox WHERE created_at < ?1", params![cutoff])?;
        Ok(paths)
    }

    /// List all vault hashes (for cleanup iteration).
    pub fn list_vault_hashes(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
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
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM vaults", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Count total mailbox entries across all vaults.
    pub fn entry_count(&self) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM mailbox", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Count total assets across all vaults.
    pub fn asset_count(&self) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Total blob storage used (mailbox entries + assets) in bytes.
    pub fn total_storage_bytes(&self) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
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
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let mailbox_bytes: i64 = conn.query_row(
            "SELECT COALESCE(SUM(blob_size), 0) FROM mailbox WHERE vault_hash = ?1",
            params![vault_hash],
            |row| row.get(0),
        )?;
        let asset_bytes: i64 = conn.query_row(
            "SELECT COALESCE(SUM(blob_size), 0) FROM assets WHERE vault_hash = ?1",
            params![vault_hash],
            |row| row.get(0),
        )?;
        Ok((mailbox_bytes + asset_bytes) as u64)
    }

    /// Get the storage limit for a vault.
    pub fn get_vault_limit(&self, vault_hash: &str) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let limit: i64 = conn.query_row(
            "SELECT max_storage_bytes FROM vaults WHERE vault_hash = ?1",
            params![vault_hash],
            |row| row.get(0),
        )?;
        Ok(limit as u64)
    }

    /// Delete assets older than cutoff and return blob paths for file removal.
    pub fn gc_old_assets(&self, max_age_secs: u64) -> Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let cutoff = unix_now() - max_age_secs as i64;
        let mut stmt = conn.prepare(
            "SELECT blob_path FROM assets WHERE created_at < ?1",
        )?;
        let paths: Vec<String> = stmt
            .query_map(params![cutoff], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        conn.execute("DELETE FROM assets WHERE created_at < ?1", params![cutoff])?;
        Ok(paths)
    }

    // -----------------------------------------------------------------------
    // Key rotation / device revocation
    // -----------------------------------------------------------------------

    /// Replace the mailbox token for a vault (called after epoch rotation).
    pub fn update_vault_token(&self, vault_hash: &str, new_token: &[u8]) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        conn.execute(
            "UPDATE vaults SET mailbox_token = ?1 WHERE vault_hash = ?2",
            params![new_token, vault_hash],
        )?;
        Ok(())
    }

    /// Delete the cursor for a specific device in a vault (device revocation).
    pub fn delete_cursor(&self, vault_hash: &str, device_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        conn.execute(
            "DELETE FROM cursors WHERE vault_hash = ?1 AND device_id = ?2",
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
    source_device: String,
    blob_path: String,
    payload_hash_hex: String,
    timestamp: i64,
    is_delete: bool,
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
    let bytes = hex::decode(hex_str)
        .with_context(|| format!("invalid hex hash: {hex_str}"))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|v: Vec<u8>| anyhow::anyhow!("hash has wrong length: {} (expected 32)", v.len()))?;
    Ok(arr)
}

/// Current Unix timestamp in seconds.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
