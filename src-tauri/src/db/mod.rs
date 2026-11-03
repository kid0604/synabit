pub mod internal;
pub mod text;
mod blocks;
pub mod crdt;
mod node_query;
pub use crdt::StatCacheEntry;
pub use node_query::{QueryResult, QueryRow};
pub mod edges;
mod people_brief;
mod files;
mod kv;
pub mod local_state;
pub mod legacy_sync_migration;
pub mod metrics;
mod nexus;
mod nodes;
mod rag;
mod read_pool;
pub use read_pool::{DbReadPool, READERS};
pub mod reminders;
pub mod subscriptions;
mod schema;
mod search;
pub mod sync_inbox;
pub mod sync_outbox;
pub mod sync_provider_state;
pub mod sync_vault;
mod whiteboards;

use rusqlite::Connection;
use std::sync::Mutex;

/// One connection to the cache database.
///
/// The app holds two kinds. `DbState` is the one writer: every write, and
/// every read that has not been moved, goes through its mutex, and with one
/// writer SQLite never has to arbitrate between two. `DbReadPool` holds a few
/// read-only connections beside it for the reads the interface waits on —
/// search, a node, a list, backlinks, a query — which WAL lets run while the
/// writer works. A read belongs on the pool only if it writes nothing and
/// caches nothing against its connection; see `read_pool`.
pub struct DbBridge {
    conn: Connection,
}

/// Thread-safe wrapper for Tauri managed state.
pub type DbState = Mutex<DbBridge>;

/// Taking the writer, whatever became of whoever held it last.
pub trait Hold {
    fn hold(&self) -> std::sync::MutexGuard<'_, DbBridge>;
}

impl Hold for DbState {
    /// The connection, even after a panic while somebody held it.
    ///
    /// A panic with the lock held poisons it, and every `lock()` after fails:
    /// on 2026-11-03 one panic left Syn answering every message with "DB lock
    /// error: poisoned lock" until the app was restarted. The connection is
    /// not the worse for it — a `Transaction` dropped in the unwind rolls back,
    /// and one opened some other way is rolled back here — so it is taken
    /// back and the poison cleared, which also lets the callers that still
    /// refuse a poisoned lock carry on.
    fn hold(&self) -> std::sync::MutexGuard<'_, DbBridge> {
        self.lock().unwrap_or_else(|poisoned| {
            log::warn!("[DB] A panic poisoned the database lock; taking it back");
            self.clear_poison();
            let guard = poisoned.into_inner();
            if !guard.conn.is_autocommit() {
                if let Err(e) = guard.conn.execute_batch("ROLLBACK") {
                    log::error!("[DB] Could not roll back what the panic left open: {e}");
                }
            }
            guard
        })
    }
}

impl DbBridge {
    /// Provide crate-internal access to the underlying SQLite connection.
    /// Used by feed_engine and feed commands for direct SQL operations.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

// Re-exports (Option A — consumers keep using crate::db::NodeEdge, etc.)
pub use edges::NodeEdge;
pub use files::{FileFilter, FileLocation, FilePage, FileSort, RemoteEntry, TextStatus};
pub use nexus::NexusRow;

#[cfg(test)]
pub(crate) use schema::run_sync_schema_migrations as run_sync_schema_migrations_for_test;

#[cfg(test)]
mod hold_tests {
    use super::{DbBridge, DbState, Hold};

    /// A panic inside, with a transaction open: the next caller gets the
    /// connection, with nothing of the transaction kept, and so does every
    /// plain `lock()` after it.
    #[test]
    fn a_panic_while_holding_the_writer_does_not_shut_everyone_else_out() {
        let state: DbState = std::sync::Mutex::new(DbBridge::new_in_memory().expect("db"));
        state.lock().unwrap().conn().execute_batch("CREATE TABLE t (x INTEGER)").expect("table");

        let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let db = state.lock().unwrap();
            db.conn().execute_batch("BEGIN; INSERT INTO t VALUES (1);").expect("half a write");
            panic!("a tool that panicked");
        }));
        assert!(crashed.is_err());
        assert!(state.is_poisoned());

        let db = state.hold();
        assert!(db.conn().is_autocommit(), "what the panic left open is rolled back");
        let rows: i64 = db.conn().query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0)).expect("count");
        assert_eq!(rows, 0);
        drop(db);
        assert!(state.lock().is_ok(), "the poison is cleared for everybody else");
    }
}
