use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;

/// Which `kv_store` a key lives in: this device's own state in `state.db`,
/// everything else in the cache. Decided by the key, so no caller has to know
/// there are two — see `db::local_state`.
fn table(key: &str) -> &'static str {
    if super::local_state::is_device_key(key) {
        "state.kv_store"
    } else {
        "main.kv_store"
    }
}

impl DbBridge {
    pub fn set_kv(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn
            .execute(
                &format!(
                    "INSERT INTO {} (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    table(key)
                ),
                params![key, value],
            )
            .map_err(|e| AppError::General(format!("DB Set KV Error: {}", e)))?;
        Ok(())
    }

    pub fn get_kv(&self, key: &str) -> AppResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT value FROM {} WHERE key = ?1", table(key)))
            .map_err(|e| AppError::General(format!("DB Get KV Prepare Error: {}", e)))?;
        let mut rows = stmt
            .query(params![key])
            .map_err(|e| AppError::General(format!("DB Get KV Query Error: {}", e)))?;

        if let Some(row) = rows
            .next()
            .map_err(|e| AppError::General(format!("DB Get KV Iteration Error: {}", e)))?
        {
            let val: String = row
                .get(0)
                .map_err(|e| AppError::General(format!("DB Get KV Decode Error: {}", e)))?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    pub fn delete_kv(&self, key: &str) -> AppResult<()> {
        self.conn
            .execute(&format!("DELETE FROM {} WHERE key = ?1", table(key)), params![key])
            .map_err(|e| AppError::General(format!("DB Delete KV Error: {}", e)))?;
        Ok(())
    }

    /// Every key starting with `prefix`. Prefixes are only ever asked for
    /// within one namespace (`telegram:inbox:`, `capture:pending:`), so the
    /// prefix decides the store the way a key does.
    pub fn get_kv_prefix(&self, prefix: &str) -> AppResult<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT key, value FROM {} WHERE key LIKE ?1", table(prefix)))
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
