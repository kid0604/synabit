use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;

impl DbBridge {
    pub fn upsert_node_blocks(
        &self,
        node_id: &str,
        blocks: Vec<(String, String)>,
    ) -> AppResult<()> {
        // Use INSERT OR REPLACE to keep old block_ids from previous content versions.
        // This ensures that transclusion references to old block hashes still resolve,
        // even after the source content has been edited.
        let mut insert_stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO node_blocks (block_id, node_id, content) VALUES (?1, ?2, ?3)"
        ).map_err(|e| AppError::General(format!("DB Error preparing block upsert: {}", e)))?;

        let mut insert_fts_stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'block', ?2, '', ?3, '', '', '', ?4)"
        ).map_err(|e| AppError::General(format!("DB Error preparing block fts upsert: {}", e)))?;

        for (block_id, content) in blocks {
            let _ = insert_stmt.execute(params![&block_id, node_id, &content]);
            let item_id = format!("{}#{}", node_id, block_id);
            let _ = insert_fts_stmt.execute(params![item_id, block_id, content, node_id]);
        }

        Ok(())
    }

    pub fn delete_node_blocks(&self, node_id: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM node_blocks WHERE node_id = ?1",
                params![node_id],
            )
            .map_err(|e| AppError::General(format!("DB Error deleting blocks: {}", e)))?;
        let _ = self.conn.execute(
            "DELETE FROM search_index WHERE item_type = 'block' AND path = ?1",
            params![node_id],
        );
        Ok(())
    }

    pub fn get_node_block(&self, node_id: &str, block_id: &str) -> AppResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content FROM node_blocks WHERE node_id = ?1 AND block_id = ?2")
            .map_err(|e| AppError::General(format!("DB Error prepare get block: {}", e)))?;

        let mut rows = stmt
            .query(params![node_id, block_id])
            .map_err(|e| AppError::General(format!("DB Error querying block: {}", e)))?;

        if let Some(row) = rows.next().unwrap_or(None) {
            Ok(Some(row.get(0).unwrap_or_default()))
        } else {
            Ok(None)
        }
    }
}
