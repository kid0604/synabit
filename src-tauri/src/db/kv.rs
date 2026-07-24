use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;

impl DbBridge {
    pub fn set_kv(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO kv_store (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )
            .map_err(|e| AppError::General(format!("DB Set KV Error: {}", e)))?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> AppResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM kv_store WHERE key = ?1")
            .map_err(|e| AppError::General(format!("DB Get KV Prepare Error: {}", e)))?;
        let mut rows = stmt
            .query(params![key])
            .map_err(|e| AppError::General(format!("DB Get KV Query Error: {}", e)))?;

        if let Some(row) = rows.next().unwrap_or(None) {
            Ok(Some(row.get(0).unwrap_or_default()))
        } else {
            Ok(None)
        }
    }

    pub fn delete_kv(&self, key: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM kv_store WHERE key = ?1", params![key])
            .map_err(|e| AppError::General(format!("DB Delete KV Error: {}", e)))?;
        Ok(())
    }

    pub fn get_kv_prefix(&self, prefix: &str) -> AppResult<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM kv_store WHERE key LIKE ?1")
            .map_err(|e| AppError::General(format!("DB Get KV Prefix Prepare Error: {}", e)))?;
        
        let pattern = format!("{}%", prefix);
        let rows = stmt
            .query_map(params![pattern], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| AppError::General(format!("DB Get KV Prefix Query Error: {}", e)))?;

        let mut results = Vec::new();
        for pair in rows.flatten() {
            results.push(pair);
        }
        Ok(results)
    }
}
