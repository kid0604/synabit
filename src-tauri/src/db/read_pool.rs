//! Read-only connections beside the one writer, for the reads the interface
//! waits on.
//!
//! `DbState` is one connection behind one mutex, so every read queued behind
//! every write: a search typed while a scan batch or a sync apply held the lock
//! waited for it, and so did opening a note. The database is in WAL mode, where
//! readers do not block the writer or each other and each sees the last commit
//! as of its first statement, so the reads that matter most can go around the
//! lock instead.
//!
//! Only reads come here, and only ones that are reads all the way down: each
//! connection is opened read-only and set `query_only`, so a write routed here
//! by mistake fails loudly instead of racing the writer. Anything whose answer
//! is cached against a particular connection's `total_changes` — the timeline's
//! `quiet::current`, its store — must stay on the writer, because a reader's
//! count never moves.

use super::DbBridge;
use rusqlite::{Connection, OpenFlags};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

/// How many readers. The interface rarely has more than a couple of reads in
/// flight at once (a list, a search, a backlink panel); more connections would
/// only be memory.
pub const READERS: usize = 3;

pub struct DbReadPool {
    readers: Vec<Mutex<DbBridge>>,
    next: AtomicUsize,
}

impl DbReadPool {
    /// A pool with no readers: every read goes to the writer, as before.
    ///
    /// What an in-memory database gets — tests above all — since a second
    /// connection to `:memory:` is a second, empty database rather than a view
    /// of the first.
    pub fn empty() -> Self {
        DbReadPool {
            readers: Vec::new(),
            next: AtomicUsize::new(0),
        }
    }

    /// Up to `size` readers on the file the writer has open.
    ///
    /// Opened after the writer, so the schema exists and the WAL files are in
    /// place. A reader that fails to open is logged and the pool is smaller;
    /// none at all is the writer-only behaviour, never an error.
    pub fn open_beside(writer: &DbBridge, size: usize) -> Self {
        let Some(path) = writer.conn().path().filter(|p| !p.is_empty()) else {
            return Self::empty();
        };
        // Outside WAL a reader's shared lock blocks the writer's commit, which
        // would make the pool the thing the writer waits on. `init_with_conn`
        // only warns when WAL cannot be had, so check what it got.
        let mode: String = writer
            .conn()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap_or_default();
        if !mode.eq_ignore_ascii_case("wal") {
            log::warn!("database is in {mode:?} journal mode, not WAL; reads share the writer");
            return Self::empty();
        }
        let mut readers = Vec::with_capacity(size);
        for _ in 0..size {
            match open_reader(path) {
                Ok(conn) => readers.push(Mutex::new(DbBridge { conn })),
                Err(e) => {
                    log::warn!("database reader could not be opened, reads share the writer: {e}");
                    break;
                }
            }
        }
        DbReadPool {
            readers,
            next: AtomicUsize::new(0),
        }
    }

    /// Run `read` on a free reader, or on the writer when there are none.
    ///
    /// A free reader if one is free; otherwise wait for the next in turn
    /// rather than falling back to the writer, which is the lock this exists
    /// to stay out of.
    pub fn read<T>(&self, writer: &super::DbState, read: impl FnOnce(&DbBridge) -> T) -> T {
        if self.readers.is_empty() {
            let db = writer.lock().unwrap_or_else(|e| e.into_inner());
            return read(&db);
        }
        let db = self.checkout();
        read(&db)
    }

    fn checkout(&self) -> MutexGuard<'_, DbBridge> {
        let start = self.next.fetch_add(1, Ordering::Relaxed);
        let n = self.readers.len();
        for i in 0..n {
            if let Ok(guard) = self.readers[(start + i) % n].try_lock() {
                return guard;
            }
        }
        self.readers[start % n]
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.readers.len()
    }
}

/// Everything a reader needs to see what the writer sees. The one place a
/// reader connection is set up, so anything the writer's connection gains at
/// open time has to be added here too.
///
/// That includes `state.db`: device-local tables and kv keys live in the
/// attached `state` schema, and a reader without it would answer those reads
/// with "nothing" rather than an error. An attached database opens with this
/// connection's flags, so it is read-only here as well.
fn open_reader(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // A reader waits only for a checkpoint resetting the WAL, which is brief;
    // without a timeout it would fail on one instead.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA query_only = ON;")?;
    // The same SQL functions the writer has, or a query using `vlower` works
    // on one connection and fails on the other.
    super::text::teach(&conn)?;
    let state = std::path::Path::new(path).with_file_name(super::local_state::FILE_NAME);
    super::local_state::attach_read_only(&conn, &state)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbState;

    #[test]
    fn an_in_memory_database_reads_through_the_writer() {
        let writer = DbState::new(DbBridge::new_in_memory_full().unwrap());
        let pool = DbReadPool::open_beside(&writer.lock().unwrap(), READERS);
        assert_eq!(pool.len(), 0);
        writer.lock().unwrap().set_kv("k", "v").unwrap();
        assert_eq!(pool.read(&writer, |db| db.get_kv("k").unwrap()), Some("v".into()));
    }

    /// The point of the pool: a read that does not wait for the writer's lock,
    /// sees what the writer committed, and cannot itself write.
    #[test]
    fn a_reader_sees_commits_without_the_writer_lock_and_cannot_write() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("cache.db")).unwrap();
        // As the app opens it: `state.db` attached before the schema is built.
        crate::db::local_state::attach(&conn, &dir.path().join(crate::db::local_state::FILE_NAME), 0)
            .unwrap();
        let writer = DbState::new(DbBridge::init_with_conn(conn).unwrap());
        let pool = DbReadPool::open_beside(&writer.lock().unwrap(), READERS);
        assert_eq!(pool.len(), READERS);

        writer.lock().unwrap().set_kv("k", "v1").unwrap();
        let held = writer.lock().unwrap();
        // The writer's lock is held for the whole of this read.
        assert_eq!(pool.read(&writer, |db| db.get_kv("k").unwrap()), Some("v1".into()));
        assert_eq!(
            pool.read(&writer, |db| db.conn().query_row("SELECT vlower('ĐÀ')", [], |r| r.get::<_, String>(0)).unwrap()),
            "đà",
            "the custom functions are there too"
        );
        assert!(pool.read(&writer, |db| db.set_kv("k", "v2")).is_err());
        drop(held);
        // A device-local key lives in the attached `state.db`; a reader must
        // see it as the writer does.
        writer.lock().unwrap().set_kv("device_id", "dev-1").unwrap();
        let held = writer.lock().unwrap();
        assert_eq!(pool.read(&writer, |db| db.get_kv("device_id").unwrap()), Some("dev-1".into()));
        drop(held);
        assert_eq!(pool.read(&writer, |db| db.get_kv("k").unwrap()), Some("v1".into()));
    }
}
