use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension, Row};

pub struct PendingAssetRow {
    pub vault_id: String,
    pub provider_id: String,
    pub remote_seq: u64,
    pub asset_id: Vec<u8>,
    pub node_id: String,
    pub rel_path: String,
    pub asset_ref_blob: Vec<u8>,
    pub status: String,
    pub retry_count: i64,
}

impl DbBridge {
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_pending_asset(
        &self,
        vault_id: &str,
        provider_id: &str,
        remote_seq: u64,
        asset_id: &[u8],
        node_id: &str,
        rel_path: &str,
        asset_ref_blob: &[u8],
    ) -> AppResult<()> {
        let now = chrono::Utc::now().timestamp();

        // Mark existing pending assets for this node/path as superseded to avoid redundant downloads
        self.conn.execute(
            "UPDATE sync_pending_assets 
             SET status = 'superseded', updated_at = ?1 
             WHERE vault_id = ?2 AND provider_id = ?3 AND node_id = ?4 AND rel_path = ?5 AND status IN ('pending', 'failed')",
            params![now, vault_id, provider_id, node_id, rel_path],
        ).map_err(|e| AppError::General(format!("Failed to supersede old assets: {}", e)))?;

        self.conn.execute(
            "INSERT INTO sync_pending_assets (
                vault_id, provider_id, remote_seq, asset_id, node_id, rel_path, asset_ref_blob, status, retry_count, created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, ?8, ?8
            ) ON CONFLICT(vault_id, provider_id, remote_seq, asset_id) DO UPDATE SET
                status = 'pending',
                asset_ref_blob = excluded.asset_ref_blob,
                updated_at = excluded.updated_at",
            params![
                vault_id,
                provider_id,
                remote_seq as i64,
                asset_id,
                node_id,
                rel_path,
                asset_ref_blob,
                now
            ],
        ).map_err(|e| AppError::General(format!("Failed to insert pending asset: {}", e)))?;

        Ok(())
    }

    pub fn get_pending_assets(
        &self,
        vault_id: &str,
        provider_id: &str,
        max_items: u32,
    ) -> AppResult<Vec<PendingAssetRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT vault_id, provider_id, remote_seq, asset_id, node_id, rel_path, asset_ref_blob, status, retry_count
             FROM sync_pending_assets
             WHERE vault_id = ? AND provider_id = ? AND status IN ('pending', 'failed') AND retry_count < 5
             ORDER BY remote_seq ASC, created_at ASC
             LIMIT ?"
        ).map_err(|e| AppError::General(format!("Failed to prepare get_pending_assets: {}", e)))?;

        let rows = stmt
            .query_map(params![vault_id, provider_id, max_items], |row: &Row| {
                let remote_seq_i64: i64 = row.get(2)?;
                Ok(PendingAssetRow {
                    vault_id: row.get(0)?,
                    provider_id: row.get(1)?,
                    remote_seq: remote_seq_i64 as u64,
                    asset_id: row.get(3)?,
                    node_id: row.get(4)?,
                    rel_path: row.get(5)?,
                    asset_ref_blob: row.get(6)?,
                    status: row.get(7)?,
                    retry_count: row.get(8)?,
                })
            })
            .map_err(|e| AppError::General(format!("Failed to query get_pending_assets: {}", e)))?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row.unwrap());
        }
        Ok(results)
    }

    pub fn update_pending_asset_status(
        &self,
        vault_id: &str,
        provider_id: &str,
        remote_seq: u64,
        asset_id: &[u8],
        status: &str,
        error_msg: Option<&str>,
    ) -> AppResult<()> {
        let now = chrono::Utc::now().timestamp();

        let mut retry_increment = 0;
        if status == "failed" {
            retry_increment = 1;
        }

        self.conn
            .execute(
                "UPDATE sync_pending_assets
             SET status = ?1, last_error = ?2, retry_count = retry_count + ?3, updated_at = ?4
             WHERE vault_id = ?5 AND provider_id = ?6 AND remote_seq = ?7 AND asset_id = ?8",
                params![
                    status,
                    error_msg,
                    retry_increment,
                    now,
                    vault_id,
                    provider_id,
                    remote_seq as i64,
                    asset_id
                ],
            )
            .map_err(|e| AppError::General(format!("Failed to update asset status: {}", e)))?;

        Ok(())
    }
}
