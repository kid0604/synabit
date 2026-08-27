mod blocks;
pub mod crdt;
mod node_query;
pub use crdt::StatCacheEntry;
pub use node_query::{QueryResult, QueryRow};
pub mod edges;
mod people_brief;
mod files;
mod kv;
pub mod legacy_sync_migration;
pub mod metrics;
mod nexus;
mod nodes;
mod rag;
mod reminders;
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
