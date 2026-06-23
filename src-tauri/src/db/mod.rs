mod schema;
mod kv;
mod blocks;
mod whiteboards;
mod files;
mod nexus;
mod edges;
mod search;
mod nodes;
mod crdt;
mod rag;

use std::sync::Mutex;
use rusqlite::Connection;

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
}

// Re-exports (Option A — consumers keep using crate::db::NodeEdge, etc.)
pub use edges::NodeEdge;
pub use nexus::NexusRow;
