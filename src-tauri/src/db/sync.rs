use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension, Row};

pub struct BootstrapSessionRow {
    pub vault_id: String,
    pub session_id: String,
    pub provider_id: String,
    pub incarnation_id: String,
    pub base_seq: u64,
    pub item_count: u64,
    pub downloaded_count: u64,
}

pub struct BootstrapItemRow {
    pub vault_id: String,
    pub provider_id: String,
    pub position: u64,
    pub doc_hash: String,
    pub head_seq: u64,
    pub operation_id: String,
    pub entry_kind: synabit_protocol::SyncEntryKind,
    pub payload_hash: String,
    pub source_device: String,
    pub encrypted_payload: Vec<u8>,
    pub timestamp: i64,
}

impl DbBridge {
    pub fn upsert_bootstrap_session(&self, session: BootstrapSessionRow) -> AppResult<()> {
        let session_id_bytes = hex::decode(&session.session_id).unwrap_or_default();
        let incarnation_id_bytes = hex::decode(&session.incarnation_id).unwrap_or_default();

        self.conn.execute(
            "INSERT OR REPLACE INTO sync_bootstrap_sessions 
            (vault_id, provider_id, session_id, incarnation_id, base_seq, item_count, downloaded_count, created_at, updated_at) 
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%s', 'now'), strftime('%s', 'now'))",
            params![
                session.vault_id,
                session.provider_id,
                session_id_bytes,
                incarnation_id_bytes,
                session.base_seq as i64,
                session.item_count as i64,
                session.downloaded_count as i64,
            ],
        ).map_err(|e| AppError::General(format!("DB Error: {}", e)))?;
        Ok(())
    }

    pub fn insert_bootstrap_item(&self, item: BootstrapItemRow) -> AppResult<()> {
        let doc_hash_bytes = hex::decode(&item.doc_hash).unwrap_or_default();
        let payload_hash_bytes = hex::decode(&item.payload_hash).unwrap_or_default();
        let op_id_bytes = hex::decode(&item.operation_id).unwrap_or_default();

        self.conn.execute(
            "INSERT OR REPLACE INTO sync_bootstrap_items 
            (vault_id, provider_id, position, doc_hash, head_seq, operation_id, entry_kind, payload_hash, source_device, encrypted_payload, timestamp) 
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                item.vault_id,
                item.provider_id,
                item.position as i64,
                doc_hash_bytes,
                item.head_seq as i64,
                op_id_bytes,
                item.entry_kind.to_string(),
                payload_hash_bytes,
                item.source_device,
                item.encrypted_payload,
                item.timestamp,
            ],
        ).map_err(|e| AppError::General(format!("DB Error: {}", e)))?;
        Ok(())
    }

    pub fn get_bootstrap_items(
        &self,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<Vec<BootstrapItemRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT position, doc_hash, head_seq, operation_id, entry_kind, payload_hash, source_device, encrypted_payload, timestamp 
            FROM sync_bootstrap_items WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY position ASC"
        ).map_err(|e| AppError::General(format!("DB Error: {}", e)))?;

        let rows = stmt
            .query_map(params![vault_id, provider_id], |row: &Row| {
                let doc_hash_bytes: Vec<u8> = row.get(1)?;
                let op_id_bytes: Vec<u8> = row.get(3)?;
                let payload_hash_bytes: Vec<u8> = row.get(5)?;
                Ok(BootstrapItemRow {
                    vault_id: vault_id.to_string(),
                    provider_id: provider_id.to_string(),
                    position: row.get::<_, i64>(0)? as u64,
                    doc_hash: hex::encode(doc_hash_bytes),
                    head_seq: row.get::<_, i64>(2)? as u64,
                    operation_id: hex::encode(op_id_bytes),
                    entry_kind: row
                        .get::<_, String>(4)?
                        .parse()
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    payload_hash: hex::encode(payload_hash_bytes),
                    source_device: row.get(6)?,
                    encrypted_payload: row.get(7)?,
                    timestamp: row.get(8)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Error: {}", e)))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| AppError::General(format!("DB Error: {}", e)))?);
        }
        Ok(items)
    }

    pub fn get_active_bootstrap_session(
        &self,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<Option<BootstrapSessionRow>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT session_id, incarnation_id, base_seq, item_count, downloaded_count
            FROM sync_bootstrap_sessions 
            WHERE vault_id = ?1 AND provider_id = ?2",
            )
            .map_err(|e| AppError::General(format!("DB Error: {}", e)))?;

        let row = stmt
            .query_row(params![vault_id, provider_id], |row: &Row| {
                let session_id_bytes: Vec<u8> = row.get(0)?;
                let inc_id_bytes: Vec<u8> = row.get(1)?;
                Ok(BootstrapSessionRow {
                    vault_id: vault_id.to_string(),
                    provider_id: provider_id.to_string(),
                    session_id: hex::encode(session_id_bytes),
                    incarnation_id: hex::encode(inc_id_bytes),
                    base_seq: row.get::<_, i64>(2)? as u64,
                    item_count: row.get::<_, i64>(3)? as u64,
                    downloaded_count: row.get::<_, i64>(4)? as u64,
                })
            })
            .optional()
            .map_err(|e| AppError::General(format!("DB Error: {}", e)))?;
        Ok(row)
    }

    pub fn mark_bootstrap_completed(&self, vault_id: &str, provider_id: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM sync_bootstrap_sessions WHERE vault_id = ?1 AND provider_id = ?2",
                params![vault_id, provider_id],
            )
            .map_err(|e| AppError::General(format!("DB Error: {}", e)))?;
        Ok(())
    }

    pub fn count_bootstrap_items(&self, vault_id: &str, provider_id: &str) -> AppResult<u64> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sync_bootstrap_items WHERE vault_id = ?1 AND provider_id = ?2",
                params![vault_id, provider_id],
                |row| row.get(0),
            )
            .map_err(|e| AppError::General(format!("DB Error: {}", e)))?;
        Ok(count as u64)
    }
}
