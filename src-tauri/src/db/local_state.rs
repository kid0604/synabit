//! `state.db`: what this device keeps for itself, apart from the cache.
//!
//! `vault_cache.db` is meant to be disposable — an index of the vault that the
//! next scan rebuilds. A few things in it were not that: which reminders have
//! already been announced (lose it and every reminder of the last day fires
//! again), the folders the Files app watches, this device's id (which names
//! its files in `Feeds/state/` and `Timeline/`), the Telegram pairing, the peer
//! a P2P sync talks to, and captures waiting for a vault. None of them belongs
//! in the vault — they are about this machine, or about a moment before there
//! was a vault — and none of them can be rebuilt by scanning one.
//!
//! So they live in a second file beside the cache, attached to the cache's own
//! connection as the schema `state`. Attached rather than opened separately,
//! because every caller already holds a `DbBridge`: an unqualified table name
//! resolves to `main` first and to `state` after, so `reminder_deliveries` and
//! `file_sources` are found in `state` by the same SQL that used to find them
//! in the cache, once the cache no longer has them. The key-value store is
//! split by key instead — see [`is_device_key`] and `db::kv`.
//!
//! Any further connection opened on `vault_cache.db` must call [`attach`] too,
//! or it will not see these tables at all.

use std::path::Path;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};

/// The file, beside `vault_cache.db` in the app's data directory.
pub const FILE_NAME: &str = "state.db";

/// Set in `state.kv_store` once the cache's copies have been moved across.
const MOVED_FLAG: &str = "state:moved-from-cache";

/// Keys that are this device's own state rather than a cache.
const DEVICE_KEYS: &[&str] = &[
    // Names this device's files in the vault (`Feeds/state/<id>.json`,
    // `Timeline/reviews/<id>.json`). A new one after a cache wipe would leave
    // the old files orphaned and start new ones beside them.
    "device_id",
    // The vault the app last had open, so it opens there again.
    "vault_path",
    // The paired P2P server. Losing it means pairing again.
    "p2p_server_addr",
    "p2p_server_id_hex",
];

/// Prefixes of keys that are this device's own state.
const DEVICE_PREFIXES: &[&str] = &[
    // The Telegram pairing, its offset, its inbox and the reminders queued for it.
    "telegram:",
    // Captures waiting for a vault — see `commands::capture`.
    "capture:",
];

/// Whether `key` (or every key under the prefix `key`) lives in `state.db`.
///
/// `device_peer_id` is deliberately not here. It is the CRDT peer id, and a
/// peer id that outlived its own history — kept here while the cache holding
/// its operations was wiped — would mint operation ids the other devices have
/// already seen from it. It must live and die with the cache.
pub fn is_device_key(key: &str) -> bool {
    DEVICE_KEYS.contains(&key) || DEVICE_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// Attach `state.db` at `path` to `conn` as `state`, and build its schema.
///
/// A damaged `state.db` is set aside as `state.db.corrupt-<now>` and a fresh
/// one started, the way the cache is: what is in it is worth keeping for
/// `sqlite3 .recover`, and not worth refusing to start over.
pub fn attach(conn: &Connection, path: &Path, now: u64) -> AppResult<()> {
    match attach_checked(conn, path) {
        Ok(()) => {}
        Err(first) => {
            log::error!("{} could not be opened: {first}", path.display());
            let _ = conn.execute_batch("DETACH DATABASE state");
            for suffix in ["", "-wal", "-shm"] {
                let mut from = path.as_os_str().to_owned();
                from.push(suffix);
                let from = std::path::PathBuf::from(from);
                if from.exists() {
                    let mut to = from.as_os_str().to_owned();
                    to.push(format!(".corrupt-{now}"));
                    std::fs::rename(&from, std::path::PathBuf::from(to)).map_err(|e| {
                        AppError::General(format!("{first}, and it could not be moved aside: {e}"))
                    })?;
                }
            }
            attach_checked(conn, path)?;
        }
    }
    ensure_schema(conn)
}

fn attach_checked(conn: &Connection, path: &Path) -> AppResult<()> {
    let open = |e: rusqlite::Error| AppError::General(format!("State DB Open Error: {e}"));
    conn.execute("ATTACH DATABASE ?1 AS state", [path.to_string_lossy()])
        .map_err(open)?;
    let verdict: String = conn
        .query_row("PRAGMA state.quick_check", [], |row| row.get(0))
        .map_err(open)?;
    if verdict != "ok" {
        return Err(AppError::General(format!("State DB Integrity Check Failed: {verdict}")));
    }
    if let Err(e) = conn.execute_batch("PRAGMA state.journal_mode=WAL;") {
        log::warn!("Could not enable WAL mode on state.db: {e}");
    }
    Ok(())
}

/// Whether `state` is already attached to `conn`.
fn attached(conn: &Connection) -> bool {
    conn.prepare("PRAGMA database_list")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()
        })
        .is_ok_and(|names| names.iter().any(|n| n == "state"))
}

/// An in-memory `state`, for a connection nobody attached a file to — the
/// in-memory databases tests run against. Production attaches the real file
/// before the schema is built, so this never stands in for it there.
pub fn ensure_attached(conn: &Connection) -> AppResult<()> {
    if attached(conn) {
        return Ok(());
    }
    conn.execute_batch("ATTACH DATABASE ':memory:' AS state")
        .map_err(|e| AppError::General(format!("State DB Open Error: {e}")))?;
    ensure_schema(conn)
}

fn ensure_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS state.kv_store (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        -- Which reminders have already been announced. It used to be rebuilt
        -- every sixty seconds by parsing every message file in the vault; a
        -- primary key answers it instantly, and losing it fires the last day's
        -- reminders again.
        CREATE TABLE IF NOT EXISTS state.reminder_deliveries (
            delivery_key TEXT PRIMARY KEY,
            delivered_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS state.idx_reminder_deliveries_at
            ON reminder_deliveries(delivered_at);
        -- Folders outside the vault the Files app indexes. Absolute paths on
        -- this machine, so they mean nothing on another one and stay here.
        CREATE TABLE IF NOT EXISTS state.file_sources (
            id TEXT PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            name TEXT NOT NULL
        );",
    )
    .map_err(|e| AppError::General(format!("State DB Schema Error: {e}")))
}

fn main_has_table(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM main.sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// Move what an older cache held into `state.db`, once.
///
/// Copied before it is dropped, with `OR IGNORE` so a row already in
/// `state.db` — written by this version — wins over the cache's older copy. An
/// interruption between the copy and the drop is put right by the next launch:
/// the copy is ignored and the drop happens. The flag only spares the key
/// scan; the tables are moved whenever the cache still has them, which is what
/// lets an old cache restored from a backup give its rows up too.
pub fn move_from_cache(conn: &Connection) -> AppResult<()> {
    let fail = |e: rusqlite::Error| AppError::General(format!("State DB Migration Error: {e}"));

    for (table, columns) in [
        ("reminder_deliveries", "delivery_key, delivered_at"),
        ("file_sources", "id, path, name"),
    ] {
        if !main_has_table(conn, table) {
            continue;
        }
        conn.execute_batch(&format!(
            "BEGIN;
             INSERT OR IGNORE INTO state.{table} ({columns}) SELECT {columns} FROM main.{table};
             DROP TABLE main.{table};
             COMMIT;"
        ))
        .map_err(|e| {
            let _ = conn.execute_batch("ROLLBACK");
            fail(e)
        })?;
        log::info!("moved {table} from the cache into {FILE_NAME}");
    }

    let done = conn
        .query_row("SELECT 1 FROM state.kv_store WHERE key = ?1", [MOVED_FLAG], |_| Ok(()))
        .is_ok();
    if done || !main_has_table(conn, "kv_store") {
        return conn
            .execute(
                "INSERT OR IGNORE INTO state.kv_store (key, value) VALUES (?1, '1')",
                [MOVED_FLAG],
            )
            .map(|_| ())
            .map_err(fail);
    }

    let keys: Vec<(String, String)> = conn
        .prepare("SELECT key, value FROM main.kv_store")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()
        })
        .map_err(fail)?;
    let moving: Vec<&(String, String)> = keys.iter().filter(|(k, _)| is_device_key(k)).collect();

    conn.execute_batch("BEGIN").map_err(fail)?;
    let moved = (|| {
        for (key, value) in &moving {
            conn.execute(
                "INSERT OR IGNORE INTO state.kv_store (key, value) VALUES (?1, ?2)",
                [key, value],
            )?;
            conn.execute("DELETE FROM main.kv_store WHERE key = ?1", [key])?;
        }
        conn.execute(
            "INSERT OR REPLACE INTO state.kv_store (key, value) VALUES (?1, '1')",
            [MOVED_FLAG],
        )?;
        Ok::<_, rusqlite::Error>(())
    })();
    match moved {
        Ok(()) => conn.execute_batch("COMMIT").map_err(fail)?,
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(fail(e));
        }
    }
    if !moving.is_empty() {
        log::info!("moved {} device key(s) from the cache into {FILE_NAME}", moving.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;

    /// The cache and the state file side by side in one directory, the way
    /// the app lays them out.
    fn open(dir: &Path) -> DbBridge {
        DbBridge::open_or_recover(&dir.join("vault_cache.db"), 1).expect("opens")
    }

    #[test]
    fn device_keys_are_told_apart_from_cache_keys() {
        for key in ["device_id", "vault_path", "telegram:offset", "telegram:inbox:12", "capture:pending:000000000001", "capture:next-seq"] {
            assert!(is_device_key(key), "{key}");
        }
        for key in ["device_peer_id", "fts_schema_version", "migration:quickcap", "feeds_state_fingerprint", "link_schema_version"] {
            assert!(!is_device_key(key), "{key}");
        }
    }

    /// The whole point: delete the cache, and what this device keeps for
    /// itself is still there when the app opens a fresh one.
    #[test]
    fn device_state_survives_the_cache_being_deleted() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut db = open(dir.path());
            db.set_kv("device_id", "macbook").unwrap();
            db.set_kv("fts_schema_version", "cache-only").unwrap();
            db.record_reminder_deliveries(&["Events/a.md_2026-10-08_15m".to_string()], 1_000).unwrap();
            db.upsert_file_source(&crate::models::file::FileSource {
                id: "src-1".into(),
                path: "/Users/me/Documents".into(),
                name: "Documents".into(),
            })
            .unwrap();
            crate::commands::capture::enqueue(
                &db,
                &crate::commands::capture::QueuedCaptureInput { text: "ý tưởng".into(), source: None },
            )
            .unwrap();
        }
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(dir.path().join(format!("vault_cache.db{suffix}")));
        }

        let db = open(dir.path());
        assert_eq!(db.get_kv("device_id").unwrap().as_deref(), Some("macbook"));
        assert_ne!(db.get_kv("fts_schema_version").unwrap().as_deref(), Some("cache-only"), "the cache did start again");
        assert_eq!(db.delivered_reminders(0).unwrap().len(), 1, "the reminder does not fire twice");
        assert_eq!(db.get_all_file_sources().unwrap().len(), 1);
        let waiting = crate::commands::capture::queued(&db).unwrap();
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].text, "ý tưởng");
    }

    /// A cache from before `state.db` existed: its rows are moved across on
    /// the first launch, and the cache no longer holds them afterwards.
    #[test]
    fn an_old_cache_gives_its_device_state_up_once() {
        let dir = tempfile::tempdir().unwrap();
        {
            let conn = Connection::open(dir.path().join("vault_cache.db")).unwrap();
            conn.execute_batch(
                "CREATE TABLE kv_store (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO kv_store VALUES ('device_id', 'old-mac'), ('telegram:offset', '42'),
                                             ('capture:pending:000000000000', '{\"text\":\"chưa lưu\",\"source\":null,\"received_at\":\"2026-10-01T00:00:00Z\"}'),
                                             ('capture:next-seq', '1'),
                                             ('device_peer_id', '7');
                 CREATE TABLE reminder_deliveries (delivery_key TEXT PRIMARY KEY, delivered_at INTEGER NOT NULL);
                 INSERT INTO reminder_deliveries VALUES ('k1', 100), ('k2', 200);
                 CREATE TABLE file_sources (id TEXT PRIMARY KEY, path TEXT UNIQUE NOT NULL, name TEXT NOT NULL);
                 INSERT INTO file_sources VALUES ('f1', '/data', 'Data');",
            )
            .unwrap();
        }

        let db = open(dir.path());
        assert_eq!(db.get_kv("device_id").unwrap().as_deref(), Some("old-mac"));
        assert_eq!(db.get_kv("telegram:offset").unwrap().as_deref(), Some("42"));
        assert_eq!(crate::commands::capture::queued(&db).unwrap()[0].text, "chưa lưu");
        assert_eq!(db.delivered_reminders(0).unwrap().len(), 2);
        assert_eq!(db.get_all_file_sources().unwrap()[0].path, "/data");
        assert!(!main_has_table(db.conn(), "reminder_deliveries"));
        assert!(!main_has_table(db.conn(), "file_sources"));
        let left_in_cache: Vec<String> = db
            .conn()
            .prepare("SELECT key FROM main.kv_store ORDER BY key")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .flatten()
            .collect();
        assert!(left_in_cache.contains(&"device_peer_id".to_string()), "the peer id stays with its history");
        assert!(!left_in_cache.iter().any(|k| is_device_key(k)), "{left_in_cache:?}");
        drop(db);

        // A second launch finds nothing to move and changes nothing.
        let db = open(dir.path());
        assert_eq!(db.get_kv("device_id").unwrap().as_deref(), Some("old-mac"));
        assert_eq!(db.delivered_reminders(0).unwrap().len(), 2);

        // And with the cache gone altogether, it is all still there.
        drop(db);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(dir.path().join(format!("vault_cache.db{suffix}")));
        }
        let db = open(dir.path());
        assert_eq!(db.get_kv("device_id").unwrap().as_deref(), Some("old-mac"));
        assert_eq!(db.get_all_file_sources().unwrap().len(), 1);
    }

    /// A row this version already wrote is not overwritten by the old cache's.
    #[test]
    fn the_newer_copy_wins_a_move() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = open(dir.path());
            db.set_kv("device_id", "current").unwrap();
        }
        {
            let conn = Connection::open(dir.path().join("vault_cache.db")).unwrap();
            conn.execute("INSERT OR REPLACE INTO kv_store VALUES ('device_id', 'stale')", []).unwrap();
        }
        // The flag is set, so the key scan does not run again: the stale copy
        // stays in the cache, unread, and the current one is what is answered.
        let db = open(dir.path());
        assert_eq!(db.get_kv("device_id").unwrap().as_deref(), Some("current"));
    }

    #[test]
    fn a_damaged_state_file_is_set_aside_rather_than_stopping_the_app() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE_NAME), b"this is not a database at all, not even close").unwrap();
        let db = open(dir.path());
        db.set_kv("device_id", "fresh").unwrap();
        assert_eq!(db.get_kv("device_id").unwrap().as_deref(), Some("fresh"));
        assert!(dir.path().join(format!("{FILE_NAME}.corrupt-1")).exists());
    }
}
