use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;

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

        for (block_id, content) in blocks {
            let _ = insert_stmt.execute(params![&block_id, node_id, &content]);

            // `INSERT OR REPLACE` did not replace anything here: the FTS table
            // has no unique constraint to conflict on, so every save of a note
            // appended another copy of each of its blocks. A note edited fifty
            // times contributed fifty copies, and since the index was scanned
            // in full on every write, the duplicates made every later write
            // slower. Routing through the upsert replaces by item_id instead.
            let item_id = format!("{}#{}", node_id, block_id);
            self.upsert_search_entry(
                &item_id, "block", &block_id, "", &content, "", None, "", node_id,
            );
        }

        Ok(())
    }

    pub fn delete_node_blocks(&self, node_id: &str) -> AppResult<()> {
        // Collect the ids before dropping the rows that name them. Clearing the
        // index by `path = node_id` instead would have to read every entry in
        // it; going through each block's own id uses the rowid map.
        let block_ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare("SELECT block_id FROM node_blocks WHERE node_id = ?1")
                .map_err(|e| AppError::General(format!("DB Error listing blocks: {}", e)))?;
            let rows = stmt
                .query_map(params![node_id], |row| row.get::<_, String>(0))
                .map_err(|e| AppError::General(format!("DB Error mapping blocks: {}", e)))?;
            rows.flatten().collect()
        };

        self.conn
            .execute(
                "DELETE FROM node_blocks WHERE node_id = ?1",
                params![node_id],
            )
            .map_err(|e| AppError::General(format!("DB Error deleting blocks: {}", e)))?;

        for block_id in block_ids {
            self.delete_search_entry(&format!("{}#{}", node_id, block_id));
        }

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
