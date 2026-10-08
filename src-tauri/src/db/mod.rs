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
