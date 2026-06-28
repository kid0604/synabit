use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SyncMetrics {
    pub date: String,
    pub cellular_bytes_tx: u64,
    pub cellular_bytes_rx: u64,
    pub wifi_bytes_tx: u64,
    pub wifi_bytes_rx: u64,
}

impl DbBridge {
    pub fn record_sync_metric(&self, date: &str, is_cellular: bool, tx_bytes: u64, rx_bytes: u64) -> AppResult<()> {
        let query = if is_cellular {
            "INSERT INTO sync_metrics (date, cellular_bytes_tx, cellular_bytes_rx, wifi_bytes_tx, wifi_bytes_rx)
             VALUES (?1, ?2, ?3, 0, 0)
             ON CONFLICT(date) DO UPDATE SET
             cellular_bytes_tx = cellular_bytes_tx + excluded.cellular_bytes_tx,
             cellular_bytes_rx = cellular_bytes_rx + excluded.cellular_bytes_rx"
        } else {
            "INSERT INTO sync_metrics (date, cellular_bytes_tx, cellular_bytes_rx, wifi_bytes_tx, wifi_bytes_rx)
             VALUES (?1, 0, 0, ?2, ?3)
             ON CONFLICT(date) DO UPDATE SET
             wifi_bytes_tx = wifi_bytes_tx + excluded.wifi_bytes_tx,
             wifi_bytes_rx = wifi_bytes_rx + excluded.wifi_bytes_rx"
        };

        self.conn
            .execute(query, params![date, tx_bytes as i64, rx_bytes as i64])
            .map_err(|e| AppError::General(format!("DB Record Sync Metric Error: {}", e)))?;
        Ok(())
    }

    pub fn get_sync_metrics(&self, date: &str) -> AppResult<SyncMetrics> {
        let mut stmt = self
            .conn
            .prepare("SELECT date, cellular_bytes_tx, cellular_bytes_rx, wifi_bytes_tx, wifi_bytes_rx FROM sync_metrics WHERE date = ?1")
            .map_err(|e| AppError::General(format!("DB Get Metric Prepare Error: {}", e)))?;
        let mut rows = stmt
            .query(params![date])
            .map_err(|e| AppError::General(format!("DB Get Metric Query Error: {}", e)))?;

        if let Some(row) = rows.next().unwrap_or(None) {
            Ok(SyncMetrics {
                date: row.get(0).unwrap_or_default(),
                cellular_bytes_tx: row.get::<_, i64>(1).unwrap_or(0) as u64,
                cellular_bytes_rx: row.get::<_, i64>(2).unwrap_or(0) as u64,
                wifi_bytes_tx: row.get::<_, i64>(3).unwrap_or(0) as u64,
                wifi_bytes_rx: row.get::<_, i64>(4).unwrap_or(0) as u64,
            })
        } else {
            Ok(SyncMetrics {
                date: date.to_string(),
                ..Default::default()
            })
        }
    }
}
