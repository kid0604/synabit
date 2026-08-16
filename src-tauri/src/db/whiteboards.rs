//! Whiteboards as nodes.
//!
//! A board used to have a table of its own, keyed — like `nodes` — by the
//! board's path inside the vault. Since the vault scan indexes every file it
//! finds, including `.whiteboard.json`, each board ended up written twice under
//! one key, by two code paths that disagreed about what to store: the table
//! held a summary of the board's labels, `nodes` held the raw file. Nexus read
//! both and listed every board twice, once with a preview of braces.
//!
//! There is one row per board now. What is left here is the reading a board
//! needs that a generic node query does not express.

use super::DbBridge;
use crate::error::{AppError, AppResult};
use std::collections::HashMap;

impl DbBridge {
    /// Board id → the modification time already recorded for it, so a scan can
    /// tell which boards on disk it has seen before.
    pub fn get_all_whiteboard_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, timestamp FROM nodes WHERE node_type = 'whiteboard'")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
        let mut map = HashMap::new();
        for r in rows.flatten() {
            map.insert(r.0, r.1);
        }
        Ok(map)
    }
}
