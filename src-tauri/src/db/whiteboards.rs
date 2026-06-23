use rusqlite::params;
use std::collections::HashMap;
use crate::error::{AppError, AppResult};
use crate::models::whiteboard::WhiteboardMetadata;
use super::DbBridge;

impl DbBridge {
    pub fn upsert_whiteboard(&self, wb: &WhiteboardMetadata) -> AppResult<()> {
        let tags_json = serde_json::to_string(&wb.tags)?;
        let timestamp = chrono::NaiveDateTime::parse_from_str(&wb.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| dt.and_utc().timestamp_millis())
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO whiteboards (id, title, tags, content, path, created_at, updated_at, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title, tags=excluded.tags, content=excluded.content,
                path=excluded.path, updated_at=excluded.updated_at, timestamp=excluded.timestamp",
            params![wb.id, wb.title, tags_json, wb.content, wb.path, wb.created_at, wb.updated_at, timestamp],
        ).map_err(|e| AppError::General(format!("DB Upsert Whiteboard Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_whiteboard(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM whiteboards WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Whiteboard Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_whiteboard_timestamps(&self) -> AppResult<HashMap<String, i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, timestamp FROM whiteboards")
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
