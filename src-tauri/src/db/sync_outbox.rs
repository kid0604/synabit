use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension, Row};
use std::fmt;
use std::str::FromStr;
use synabit_protocol::SyncEntryKind;

pub const MAX_OUTBOX_DISPATCH_BATCH: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxState {
    Prepared,
    UploadingAssets,
    Ready,
    Sent,
    Acknowledged,
    Failed,
}

impl OutboxState {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutboxState::Prepared => "prepared",
            OutboxState::UploadingAssets => "uploading_assets",
            OutboxState::Ready => "ready",
            OutboxState::Sent => "sent",
            OutboxState::Acknowledged => "acknowledged",
            OutboxState::Failed => "failed",
        }
    }
}

impl fmt::Display for OutboxState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for OutboxState {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prepared" => Ok(OutboxState::Prepared),
            "uploading_assets" => Ok(OutboxState::UploadingAssets),
            "ready" => Ok(OutboxState::Ready),
            "sent" => Ok(OutboxState::Sent),
            "acknowledged" => Ok(OutboxState::Acknowledged),
            "failed" => Ok(OutboxState::Failed),
            other => Err(AppError::General(format!(
                "Invalid sync_outbox state in DB: '{}'",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRecord {
    pub vault_id: String,
    pub provider_id: String,
    pub operation_id: [u8; 16],
    pub entry_kind: SyncEntryKind,
    pub node_id: String,
    pub rel_path: Option<String>,
    pub doc_hash: Option<[u8; 32]>,
    pub source_hash: Option<[u8; 32]>,
    pub original_timestamp: i64,
    pub encrypted_payload: Option<Vec<u8>>,
    pub payload_hash: Option<[u8; 32]>,
    pub asset_ref_blob: Option<Vec<u8>>,
    pub state: OutboxState,
    pub retry_count: u32,
    pub next_retry_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl OutboxRecord {
    pub fn validate_complete(&self) -> AppResult<()> {
        if self.vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if self.provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }
        if self.node_id.trim().is_empty() {
            return Err(AppError::General("node_id cannot be empty".into()));
        }
        if self.doc_hash.is_none() {
            return Err(AppError::General("Missing doc_hash".into()));
        }
        if self.rel_path.as_ref().map_or(true, |p| p.trim().is_empty()) {
            return Err(AppError::General("Missing or empty rel_path".into()));
        }
        if self.source_hash.is_none() {
            return Err(AppError::General("Missing source_hash".into()));
        }
        if self
            .encrypted_payload
            .as_ref()
            .map_or(true, |p| p.is_empty())
        {
            return Err(AppError::General(
                "Missing or empty encrypted_payload".into(),
            ));
        }
        if self.payload_hash.is_none() {
            return Err(AppError::General("Missing payload_hash".into()));
        }
        match self.entry_kind {
            SyncEntryKind::Upsert | SyncEntryKind::Delete => {
                if self.asset_ref_blob.is_some() {
                    return Err(AppError::General(
                        "Upsert and Delete entries must have asset_ref_blob=None".into(),
                    ));
                }
            }
            SyncEntryKind::AssetReference => {
                if self.asset_ref_blob.as_ref().map_or(true, |b| b.is_empty()) {
                    return Err(AppError::General(
                        "AssetReference entry must have non-empty asset_ref_blob".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn validate(&self) -> AppResult<()> {
        self.validate_complete()
    }
}

pub fn outbox_record_to_sync_operation(
    record: &OutboxRecord,
) -> AppResult<crate::sync::core::types::SyncOperation> {
    record.validate_complete()?;

    let doc_hash = record
        .doc_hash
        .ok_or_else(|| AppError::General("Missing doc_hash".into()))?;
    let rel_path = record
        .rel_path
        .clone()
        .ok_or_else(|| AppError::General("Missing rel_path".into()))?;
    let encrypted_payload = record
        .encrypted_payload
        .clone()
        .ok_or_else(|| AppError::General("Missing encrypted_payload".into()))?;
    let payload_hash = record
        .payload_hash
        .ok_or_else(|| AppError::General("Missing payload_hash".into()))?;

    Ok(crate::sync::core::types::SyncOperation {
        operation_id: record.operation_id,
        doc_hash,
        entry_kind: record.entry_kind.clone(),
        node_id: record.node_id.clone(),
        rel_path,
        encrypted_payload,
        payload_hash,
        timestamp: record.original_timestamp,
    })
}

fn decode_outbox_row(row: &Row) -> Result<OutboxRecord, rusqlite::Error> {
    let vault_id: String = row.get(0)?;
    let provider_id: String = row.get(1)?;
    let op_id_bytes: Vec<u8> = row.get(2)?;
    let entry_kind_str: String = row.get(3)?;
    let node_id: String = row.get(4)?;
    let rel_path: Option<String> = row.get(5)?;
    let doc_hash_bytes: Option<Vec<u8>> = row.get(6)?;
    let source_hash_bytes: Option<Vec<u8>> = row.get(7)?;
    let original_timestamp: i64 = row.get(8)?;
    let encrypted_payload: Option<Vec<u8>> = row.get(9)?;
    let payload_hash_bytes: Option<Vec<u8>> = row.get(10)?;
    let asset_ref_blob: Option<Vec<u8>> = row.get(11)?;
    let state_str: String = row.get(12)?;
    let retry_count_i64: i64 = row.get(13)?;
    let next_retry_at: Option<i64> = row.get(14)?;
    let last_error: Option<String> = row.get(15)?;
    let created_at: i64 = row.get(16)?;
    let updated_at: i64 = row.get(17)?;

    let operation_id: [u8; 16] = op_id_bytes.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "operation_id length must be 16 bytes",
            )),
        )
    })?;

    let doc_hash = match doc_hash_bytes {
        Some(bytes) => Some(bytes.try_into().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "doc_hash length must be 32 bytes",
                )),
            )
        })?),
        None => None,
    };

    let source_hash = match source_hash_bytes {
        Some(bytes) => Some(bytes.try_into().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "source_hash length must be 32 bytes",
                )),
            )
        })?),
        None => None,
    };

    let payload_hash = match payload_hash_bytes {
        Some(bytes) => Some(bytes.try_into().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "payload_hash length must be 32 bytes",
                )),
            )
        })?),
        None => None,
    };

    let entry_kind: SyncEntryKind = entry_kind_str.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid entry_kind: {:?}", e),
            )),
        )
    })?;

    let state: OutboxState = state_str.parse().map_err(|e: AppError| {
        rusqlite::Error::FromSqlConversionFailure(
            12,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        )
    })?;

    let retry_count = u32::try_from(retry_count_i64).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            13,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "retry_count cannot be negative",
            )),
        )
    })?;

    Ok(OutboxRecord {
        vault_id,
        provider_id,
        operation_id,
        entry_kind,
        node_id,
        rel_path,
        doc_hash,
        source_hash,
        original_timestamp,
        encrypted_payload,
        payload_hash,
        asset_ref_blob,
        state,
        retry_count,
        next_retry_at,
        last_error,
        created_at,
        updated_at,
    })
}

impl DbBridge {
    pub fn insert_outbox_record(&self, record: &OutboxRecord) -> AppResult<()> {
        record.validate_complete()?;

        let rows_affected = self
            .conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash,
                    source_hash, original_timestamp, encrypted_payload, payload_hash,
                    asset_ref_blob, state, retry_count, next_retry_at, last_error,
                    created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
                ON CONFLICT(vault_id, provider_id, operation_id) DO NOTHING",
                params![
                    record.vault_id,
                    record.provider_id,
                    record.operation_id.as_slice(),
                    record.entry_kind.to_string(),
                    record.node_id,
                    record.rel_path,
                    record.doc_hash.map(|h| h.to_vec()),
                    record.source_hash.map(|h| h.to_vec()),
                    record.original_timestamp,
                    record.encrypted_payload,
                    record.payload_hash.map(|h| h.to_vec()),
                    record.asset_ref_blob,
                    record.state.as_str(),
                    record.retry_count as i64,
                    record.next_retry_at,
                    record.last_error,
                    record.created_at,
                    record.updated_at,
                ],
            )
            .map_err(|e| AppError::General(format!("DB Error inserting outbox record: {}", e)))?;

        if rows_affected == 1 {
            return Ok(());
        }

        let existing = self
            .get_outbox_by_id(&record.vault_id, &record.provider_id, &record.operation_id)?
            .ok_or_else(|| {
                AppError::General(format!(
                    "Conflict occurred but row for operation_id hex '{}' could not be found",
                    hex::encode(record.operation_id)
                ))
            })?;

        if existing.entry_kind == record.entry_kind
            && existing.node_id == record.node_id
            && existing.rel_path == record.rel_path
            && existing.doc_hash == record.doc_hash
            && existing.source_hash == record.source_hash
            && existing.original_timestamp == record.original_timestamp
            && existing.encrypted_payload == record.encrypted_payload
            && existing.payload_hash == record.payload_hash
            && existing.asset_ref_blob == record.asset_ref_blob
        {
            Ok(())
        } else {
            Err(AppError::General(format!(
                "Outbox operation_id collision with conflicting content for operation_id hex '{}'",
                hex::encode(record.operation_id)
            )))
        }
    }

    pub fn enqueue_or_reuse_outbox_operation(
        &mut self,
        record: &OutboxRecord,
    ) -> AppResult<[u8; 16]> {
        record.validate_complete()?;

        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB Tx start error: {}", e)))?;
        let query_res = tx.query_row(
            "SELECT operation_id FROM sync_outbox
             WHERE vault_id = ?1 AND provider_id = ?2 AND node_id = ?3 AND rel_path IS ?4 
             AND entry_kind = ?5 AND source_hash IS ?6
             AND state IN ('prepared', 'uploading_assets', 'ready', 'sent', 'failed') -- explicitly excludes acknowledged
             ORDER BY created_at, operation_id LIMIT 1",
            params![
                record.vault_id,
                record.provider_id,
                record.node_id,
                record.rel_path,
                record.entry_kind.to_string(),
                record.source_hash.map(|h| h.to_vec()),
            ],
            |row| {
                let op_id: Vec<u8> = row.get(0)?;
                Ok(op_id)
            },
        );

        match query_res {
            Ok(op_id_vec) => {
                if let Ok(op_id) = op_id_vec.try_into() {
                    tx.commit()
                        .map_err(|e| AppError::General(format!("DB Tx commit error: {}", e)))?;
                    return Ok(op_id);
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {}
            Err(e) => return Err(AppError::General(format!("DB query error: {}", e))),
        }

        // No exact match, but there may be an earlier revision of this same
        // document still waiting to go out. Overwrite it rather than queueing
        // another entry: each payload carries a complete CRDT snapshot, so the
        // newer one already contains everything the older one did. Editing a
        // note fifty times while offline used to queue fifty full snapshots and
        // push every one of them.
        //
        // Rows already handed to the transport are left alone. A `sent`
        // operation may have reached the server, and its acknowledgement is
        // matched by operation id, so rewriting it underneath the push would
        // corrupt that exchange.
        let pending: Option<Vec<u8>> = tx
            .query_row(
                "SELECT operation_id FROM sync_outbox
                 WHERE vault_id = ?1 AND provider_id = ?2 AND node_id = ?3 AND entry_kind = ?4
                   AND state IN ('prepared', 'ready', 'failed')
                 ORDER BY created_at, operation_id LIMIT 1",
                params![
                    record.vault_id,
                    record.provider_id,
                    record.node_id,
                    record.entry_kind.to_string(),
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::General(format!("DB query error: {}", e)))?;

        if let Some(op_id_vec) = pending {
            if let Ok(existing_id) = <Vec<u8> as TryInto<[u8; 16]>>::try_into(op_id_vec) {
                tx.execute(
                    "UPDATE sync_outbox SET
                        rel_path = ?4, doc_hash = ?5, source_hash = ?6,
                        original_timestamp = ?7, encrypted_payload = ?8, payload_hash = ?9,
                        asset_ref_blob = ?10, state = 'ready', retry_count = 0,
                        next_retry_at = NULL, last_error = NULL, updated_at = ?11
                     WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3",
                    params![
                        record.vault_id,
                        record.provider_id,
                        existing_id.as_slice(),
                        record.rel_path,
                        record.doc_hash.map(|h| h.to_vec()),
                        record.source_hash.map(|h| h.to_vec()),
                        record.original_timestamp,
                        record.encrypted_payload,
                        record.payload_hash.map(|h| h.to_vec()),
                        record.asset_ref_blob,
                        record.updated_at,
                    ],
                )
                .map_err(|e| AppError::General(format!("DB Error coalescing outbox: {}", e)))?;

                tx.commit()
                    .map_err(|e| AppError::General(format!("DB Tx commit error: {}", e)))?;
                return Ok(existing_id);
            }
        }

        tx.execute(
            "INSERT INTO sync_outbox (
                vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash,
                source_hash, original_timestamp, encrypted_payload, payload_hash,
                asset_ref_blob, state, retry_count, next_retry_at, last_error,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                record.vault_id,
                record.provider_id,
                record.operation_id.as_slice(),
                record.entry_kind.to_string(),
                record.node_id,
                record.rel_path,
                record.doc_hash.map(|h| h.to_vec()),
                record.source_hash.map(|h| h.to_vec()),
                record.original_timestamp,
                record.encrypted_payload,
                record.payload_hash.map(|h| h.to_vec()),
                record.asset_ref_blob,
                record.state.as_str(),
                record.retry_count as i64,
                record.next_retry_at,
                record.last_error,
                record.created_at,
                record.updated_at,
            ],
        ).map_err(|e| AppError::General(format!("DB Error inserting outbox record: {}", e)))?;

        tx.commit()
            .map_err(|e| AppError::General(format!("DB Tx commit error: {}", e)))?;
        Ok(record.operation_id)
    }

    pub fn quarantine_incomplete_dispatchable_outbox(
        &mut self,
        vault_id: &str,
        provider_id: &str,
        now: i64,
    ) -> AppResult<Vec<[u8; 16]>> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB tx err: {}", e)))?;

        let candidates = {
            let mut stmt = tx
                .prepare(
                    "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash, source_hash, original_timestamp, encrypted_payload, payload_hash, asset_ref_blob, state, retry_count, next_retry_at, last_error, created_at, updated_at
                     FROM sync_outbox
                     WHERE vault_id = ?1 AND provider_id = ?2
                       AND (state IN ('ready', 'sent') OR (state = 'failed' AND next_retry_at IS NOT NULL AND next_retry_at <= ?3))
                     ORDER BY created_at, operation_id"
                )
                .map_err(|e| AppError::General(format!("DB prepare err: {}", e)))?;

            let rows = stmt
                .query_map(params![vault_id, provider_id, now], decode_outbox_row)
                .map_err(|e| AppError::General(format!("DB query err: {}", e)))?;

            let mut list = Vec::new();
            for r in rows {
                let rec: OutboxRecord =
                    r.map_err(|e| AppError::General(format!("Outbox decode error: {}", e)))?;
                list.push(rec);
            }
            list
        };

        let mut quarantined = Vec::new();
        for rec in candidates {
            if let Err(e) = rec.validate_complete() {
                let err_msg = e.to_string();
                let rows_affected = tx
                    .execute(
                        "UPDATE sync_outbox
                         SET state = 'failed', next_retry_at = NULL, last_error = ?1, updated_at = ?2
                         WHERE vault_id = ?3 AND provider_id = ?4 AND operation_id = ?5
                           AND state = ?6 AND next_retry_at IS ?7",
                        params![
                            err_msg,
                            now,
                            vault_id,
                            provider_id,
                            rec.operation_id.as_slice(),
                            rec.state.as_str(),
                            rec.next_retry_at
                        ],
                    )
                    .map_err(|e| AppError::General(format!("Quarantine update err: {}", e)))?;

                if rows_affected != 1 {
                    tx.rollback().ok();
                    return Err(AppError::General(format!(
                        "Quarantine CAS update failed for op {}",
                        hex::encode(rec.operation_id)
                    )));
                }
                quarantined.push(rec.operation_id);
            }
        }

        tx.commit()
            .map_err(|e| AppError::General(format!("DB commit err: {}", e)))?;

        Ok(quarantined)
    }

    pub fn mark_outbox_batch_sent(
        &mut self,
        vault_id: &str,
        provider_id: &str,
        operation_ids: &[[u8; 16]],
        now: i64,
    ) -> AppResult<()> {
        if operation_ids.is_empty() {
            return Ok(());
        }

        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for id in operation_ids {
            if !seen.insert(*id) {
                return Err(AppError::General(format!(
                    "Protocol violation: duplicate operation ID {} in sent batch",
                    hex::encode(id)
                )));
            }
        }

        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("Failed to start transaction: {}", e)))?;

        for op_id in operation_ids {
            let (state_str, next_retry_at): (String, Option<i64>) = {
                let mut stmt = tx
                    .prepare(
                        "SELECT state, next_retry_at FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3"
                    )
                    .map_err(|e| AppError::General(format!("DB prepare err: {}", e)))?;

                match stmt.query_row(params![vault_id, provider_id, op_id.as_slice()], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                }) {
                    Ok(tuple) => tuple,
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        drop(stmt);
                        tx.rollback().ok();
                        return Err(AppError::General(format!(
                            "Cannot mark sent: operation {} not found",
                            hex::encode(op_id)
                        )));
                    }
                    Err(e) => {
                        drop(stmt);
                        tx.rollback().ok();
                        return Err(AppError::General(format!("DB query err: {}", e)));
                    }
                }
            };

            let state: OutboxState = state_str.parse()?;
            let is_valid = match state {
                OutboxState::Ready | OutboxState::Sent => true,
                OutboxState::Failed => next_retry_at.map_or(false, |t| t <= now),
                _ => false,
            };

            if !is_valid {
                tx.rollback().ok();
                return Err(AppError::General(format!(
                    "Cannot mark sent: operation {} in invalid state {:?} (next_retry_at={:?})",
                    hex::encode(op_id),
                    state,
                    next_retry_at
                )));
            }
        }

        for op_id in operation_ids {
            let rows_affected = tx.execute(
                "UPDATE sync_outbox SET state = 'sent', updated_at = ?4 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state IN ('ready', 'sent', 'failed')",
                params![vault_id, provider_id, op_id.as_slice(), now],
            ).map_err(|e| AppError::General(format!("DB error updating sent state: {}", e)))?;

            if rows_affected != 1 {
                tx.rollback().ok();
                return Err(AppError::General(format!(
                    "CAS state transition failed for operation {}",
                    hex::encode(op_id)
                )));
            }
        }

        tx.commit()
            .map_err(|e| AppError::General(format!("Failed to commit transaction: {}", e)))?;
        Ok(())
    }

    /// Find the highest mailbox sequence this device has successfully published
    /// for a document.
    ///
    /// Used to tell whether an incoming tombstone predates work we have already
    /// put into the shared order for the same document.
    pub fn latest_acked_outbox_seq_for_node(
        &self,
        vault_id: &str,
        provider_id: &str,
        node_id: &str,
    ) -> AppResult<Option<u64>> {
        let seq: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(remote_seq) FROM sync_outbox
                 WHERE vault_id = ?1 AND provider_id = ?2 AND node_id = ?3
                   AND state = 'acknowledged' AND remote_seq IS NOT NULL",
                params![vault_id, provider_id, node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::General(format!("DB Error reading outbox seq: {}", e)))?
            .flatten();

        Ok(seq.and_then(|s| u64::try_from(s).ok()))
    }

    pub fn commit_accepted_outbox_operation(
        &mut self,
        record: &OutboxRecord,
        remote_seq: Option<u64>,
        now: i64,
    ) -> AppResult<()> {
        record.validate_complete()?;
        let is_delete = record.entry_kind == synabit_protocol::SyncEntryKind::Delete;
        let remote_seq_param = remote_seq.and_then(|s| i64::try_from(s).ok());

        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB Tx start error: {}", e)))?;

        let rows_affected = tx
            .execute(
                "UPDATE sync_outbox
                 SET state = 'acknowledged', updated_at = ?4, next_retry_at = NULL, last_error = NULL, remote_seq = ?5
                 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state = 'sent'",
                params![
                    record.vault_id,
                    record.provider_id,
                    record.operation_id.as_slice(),
                    now,
                    remote_seq_param
                ],
            )
            .map_err(|e| AppError::General(format!("DB Tx outbox error: {}", e)))?;

        if rows_affected != 1 {
            tx.rollback().ok();
            return Err(AppError::General(
                "CAS state transition failed: operation not found or not in 'sent' state".into(),
            ));
        }

        if is_delete {
            if let Some(ref rel_path) = record.rel_path {
                tx.execute(
                    "DELETE FROM sync_document_baselines WHERE vault_id = ?1 AND provider_id = ?2 AND rel_path = ?3",
                    params![record.vault_id, record.provider_id, rel_path],
                ).map_err(|e| AppError::General(format!("DB Tx baseline error: {}", e)))?;

                tx.execute(
                    "DELETE FROM sync_document_paths WHERE vault_id = ?1 AND doc_id = ?2",
                    params![record.vault_id, record.node_id],
                )
                .map_err(|e| AppError::General(format!("DB Tx path map error: {}", e)))?;
            }
        } else {
            let rel_path = record.rel_path.as_ref().unwrap();
            let source_hash = record.source_hash.as_ref().unwrap();
            let content_hash = hex::encode(source_hash);

            tx.execute(
                "INSERT INTO sync_document_baselines (vault_id, provider_id, rel_path, content_hash, updated_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(vault_id, provider_id, rel_path) DO UPDATE SET content_hash = excluded.content_hash, updated_at = excluded.updated_at",
                params![record.vault_id, record.provider_id, rel_path, content_hash, now],
            ).map_err(|e| AppError::General(format!("DB Tx baseline error: {}", e)))?;

            tx.execute(
                "INSERT INTO sync_document_paths (vault_id, doc_id, rel_path, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(vault_id, doc_id) DO UPDATE SET rel_path = excluded.rel_path, updated_at = excluded.updated_at",
                params![record.vault_id, record.node_id, rel_path, now],
            ).map_err(|e| AppError::General(format!("DB Tx path map error: {}", e)))?;
        }

        tx.commit()
            .map_err(|e| AppError::General(format!("DB Tx commit error: {}", e)))?;
        Ok(())
    }

    pub const MAX_OUTBOX_RETRY_DELAY: i64 = 86400;

    pub fn schedule_outbox_retry(
        &mut self,
        vault_id: &str,
        provider_id: &str,
        operation_id: &[u8; 16],
        error: &str,
        now: i64,
    ) -> AppResult<()> {
        if error.trim().is_empty() {
            return Err(AppError::General(
                "Failed transition requires a non-empty last_error message".into(),
            ));
        }

        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB Tx start err: {}", e)))?;

        let mut stmt = tx
            .prepare("SELECT retry_count FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state = 'sent'")
            .map_err(|e| AppError::General(format!("DB prepare err: {}", e)))?;
        let retry_count: i64 = match stmt.query_row(
            params![vault_id, provider_id, operation_id.as_slice()],
            |r| r.get(0),
        ) {
            Ok(c) => c,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(AppError::General(
                    "CAS state transition failed: operation not found or not in 'sent' state"
                        .into(),
                ));
            }
            Err(e) => return Err(AppError::General(format!("DB query error: {}", e))),
        };
        drop(stmt);

        let delay = (2_i64.saturating_pow(retry_count.min(30) as u32)).saturating_mul(60);
        let next_retry_at = now.saturating_add(delay.min(Self::MAX_OUTBOX_RETRY_DELAY));

        let rows_affected = tx.execute(
            "UPDATE sync_outbox SET state = 'failed', retry_count = retry_count + 1, next_retry_at = ?4, last_error = ?5, updated_at = ?6 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state = 'sent'",
            params![vault_id, provider_id, operation_id.as_slice(), next_retry_at, error, now],
        ).map_err(|e| AppError::General(format!("DB error: {}", e)))?;

        if rows_affected != 1 {
            return Err(AppError::General(
                "CAS state transition failed: concurrent modification".into(),
            ));
        }

        tx.commit()
            .map_err(|e| AppError::General(format!("DB Tx commit err: {}", e)))?;
        Ok(())
    }

    pub fn schedule_outbox_batch_retry(
        &mut self,
        vault_id: &str,
        provider_id: &str,
        operation_ids: &[[u8; 16]],
        error: &str,
        now: i64,
    ) -> AppResult<()> {
        if operation_ids.is_empty() {
            return Ok(());
        }
        if error.trim().is_empty() {
            return Err(AppError::General(
                "Batch retry requires non-empty last_error message".into(),
            ));
        }

        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB Tx start err: {}", e)))?;

        for op_id in operation_ids {
            let mut stmt = tx
                .prepare("SELECT retry_count FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state = 'sent'")
                .map_err(|e| AppError::General(format!("DB prepare err: {}", e)))?;
            let retry_count: i64 = match stmt
                .query_row(params![vault_id, provider_id, op_id.as_slice()], |r| {
                    r.get(0)
                }) {
                Ok(c) => c,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    return Err(AppError::General(
                        "Batch CAS retry failed: operation not found or not in 'sent' state".into(),
                    ));
                }
                Err(e) => return Err(AppError::General(format!("DB query error: {}", e))),
            };
            drop(stmt);

            let delay = (2_i64.saturating_pow(retry_count.min(30) as u32)).saturating_mul(60);
            let next_retry_at = now.saturating_add(delay.min(Self::MAX_OUTBOX_RETRY_DELAY));

            let rows_affected = tx
                .execute(
                    "UPDATE sync_outbox SET state = 'failed', retry_count = retry_count + 1, next_retry_at = ?4, last_error = ?5, updated_at = ?6 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state = 'sent'",
                    params![vault_id, provider_id, op_id.as_slice(), next_retry_at, error, now],
                )
                .map_err(|e| AppError::General(format!("DB error updating batch retry: {}", e)))?;

            if rows_affected != 1 {
                return Err(AppError::General(
                    "Batch CAS retry failed: state transition mismatch".into(),
                ));
            }
        }

        tx.commit()
            .map_err(|e| AppError::General(format!("DB Tx commit err: {}", e)))?;
        Ok(())
    }

    pub fn get_dispatchable_outbox(
        &self,
        vault_id: &str,
        provider_id: &str,
        now: i64,
        limit: usize,
    ) -> AppResult<Vec<OutboxRecord>> {
        if limit == 0 {
            return Err(AppError::General(
                "Dispatch limit must be greater than 0".into(),
            ));
        }
        if limit > MAX_OUTBOX_DISPATCH_BATCH {
            return Err(AppError::General(format!(
                "Dispatch limit {} exceeds maximum batch size {}",
                limit, MAX_OUTBOX_DISPATCH_BATCH
            )));
        }

        let limit_i64 = i64::try_from(limit).map_err(|_| {
            AppError::General(format!("Dispatch limit {} is out of bounds for i64", limit))
        })?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash,
                        source_hash, original_timestamp, encrypted_payload, payload_hash,
                        asset_ref_blob, state, retry_count, next_retry_at, last_error,
                        created_at, updated_at
                 FROM sync_outbox
                 WHERE vault_id = ?1 AND provider_id = ?2
                   AND (
                       state IN ('ready', 'sent')
                       OR (state = 'failed' AND next_retry_at IS NOT NULL AND next_retry_at <= ?3)
                   )
                 ORDER BY created_at ASC, operation_id ASC
                 LIMIT ?4",
            )
            .map_err(|e| AppError::General(format!("DB Error preparing dispatch query: {}", e)))?;

        let rows = stmt
            .query_map(
                params![vault_id, provider_id, now, limit_i64],
                decode_outbox_row,
            )
            .map_err(|e| AppError::General(format!("DB Error executing dispatch query: {}", e)))?;

        let mut records = Vec::new();
        for row in rows {
            let record = row.map_err(|e| {
                AppError::General(format!("DB Error decoding outbox record: {}", e))
            })?;
            records.push(record);
        }

        Ok(records)
    }

    #[cfg(test)]
    pub fn snapshot_all_scoped_outbox(
        &self,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<Vec<OutboxRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT operation_id FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY operation_id")
            .map_err(|e| AppError::General(format!("Prepare err: {}", e)))?;
        let rows = stmt
            .query_map(params![vault_id, provider_id], |r| {
                let blob: Vec<u8> = r.get(0)?;
                blob.try_into().map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Blob,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "operation_id length must be 16 bytes",
                        )),
                    )
                })
            })
            .map_err(|e| AppError::General(format!("Query map err: {}", e)))?;

        let mut list = Vec::new();
        for id_res in rows {
            let id: [u8; 16] = id_res.map_err(|e| {
                AppError::General(format!("operation_id length must be 16 bytes: {}", e))
            })?;
            let rec = self
                .get_outbox_by_id(vault_id, provider_id, &id)?
                .ok_or_else(|| AppError::General(format!("Record missing: {}", hex::encode(id))))?;
            list.push(rec);
        }
        Ok(list)
    }

    pub fn get_outbox_by_id(
        &self,
        vault_id: &str,
        provider_id: &str,
        operation_id: &[u8; 16],
    ) -> AppResult<Option<OutboxRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash,
                        source_hash, original_timestamp, encrypted_payload, payload_hash,
                        asset_ref_blob, state, retry_count, next_retry_at, last_error,
                        created_at, updated_at
                 FROM sync_outbox
                 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3",
            )
            .map_err(|e| {
                AppError::General(format!("DB Error preparing get_outbox_by_id: {}", e))
            })?;

        let record = stmt
            .query_row(
                params![vault_id, provider_id, operation_id.as_slice()],
                decode_outbox_row,
            )
            .optional()
            .map_err(|e| {
                AppError::General(format!("DB Error executing get_outbox_by_id: {}", e))
            })?;

        Ok(record)
    }

    pub fn transition_outbox_state(
        &self,
        vault_id: &str,
        provider_id: &str,
        operation_id: &[u8; 16],
        expected_state: OutboxState,
        new_state: OutboxState,
        last_error: Option<&str>,
        next_retry_at: Option<i64>,
        now: i64,
    ) -> AppResult<()> {
        if new_state == OutboxState::Failed {
            let err_msg = match last_error {
                Some(msg) if !msg.trim().is_empty() => msg,
                _ => {
                    return Err(AppError::General(
                        "Failed transition requires a non-empty last_error message".into(),
                    ))
                }
            };
            let retry_at = match next_retry_at {
                Some(ts) => ts,
                None => {
                    return Err(AppError::General(
                        "Failed transition requires a next_retry_at timestamp".into(),
                    ))
                }
            };
            if retry_at < now {
                return Err(AppError::General(format!(
                    "Failed transition next_retry_at timestamp {} cannot be in the past relative to now {}",
                    retry_at, now
                )));
            }

            let rows_affected = self
                .conn
                .execute(
                    "UPDATE sync_outbox
                     SET state = 'failed',
                         retry_count = retry_count + 1,
                         next_retry_at = ?5,
                         last_error = ?6,
                         updated_at = ?7
                     WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state = ?4",
                    params![
                        vault_id,
                        provider_id,
                        operation_id.as_slice(),
                        expected_state.as_str(),
                        retry_at,
                        err_msg,
                        now,
                    ],
                )
                .map_err(|e| {
                    AppError::General(format!("DB Error in failed CAS transition: {}", e))
                })?;

            if rows_affected == 0 {
                return Err(AppError::General(format!(
                    "CAS state transition failed: operation '{}' not found or state is not '{}'",
                    hex::encode(operation_id),
                    expected_state.as_str()
                )));
            }
        } else {
            let rows_affected = self
                .conn
                .execute(
                    "UPDATE sync_outbox
                     SET state = ?5,
                         last_error = ?6,
                         next_retry_at = ?7,
                         updated_at = ?8
                     WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3 AND state = ?4",
                    params![
                        vault_id,
                        provider_id,
                        operation_id.as_slice(),
                        expected_state.as_str(),
                        new_state.as_str(),
                        last_error,
                        next_retry_at,
                        now,
                    ],
                )
                .map_err(|e| AppError::General(format!("DB Error in CAS transition: {}", e)))?;

            if rows_affected == 0 {
                return Err(AppError::General(format!(
                    "CAS state transition failed: operation '{}' not found or state is not '{}'",
                    hex::encode(operation_id),
                    expected_state.as_str()
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db::schema::run_sync_schema_migrations;
    use rusqlite::Connection;

    pub fn setup_test_db() -> DbBridge {
        let mut conn = Connection::open_in_memory().unwrap();
        run_sync_schema_migrations(&mut conn).unwrap();

        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'gdrive', 100, 100)",
            [],
        )
        .unwrap();

        DbBridge { conn }
    }

    pub fn sample_record(vault_id: &str, provider_id: &str, op_byte: u8) -> OutboxRecord {
        OutboxRecord {
            vault_id: vault_id.to_string(),
            provider_id: provider_id.to_string(),
            operation_id: [op_byte; 16],
            entry_kind: SyncEntryKind::Upsert,
            node_id: format!("node_{}", op_byte),
            rel_path: Some(format!("path_{}.md", op_byte)),
            doc_hash: Some([op_byte; 32]),
            source_hash: Some([op_byte; 32]),
            original_timestamp: 1000,
            encrypted_payload: Some(vec![1, 2, 3, op_byte]),
            payload_hash: Some([op_byte; 32]),
            asset_ref_blob: None,
            state: OutboxState::Prepared,
            retry_count: 0,
            next_retry_at: None,
            last_error: None,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[test]
    fn insert_and_read_round_trip() {
        let db = setup_test_db();
        let rec = sample_record("v1", "gdrive", 1);

        db.insert_outbox_record(&rec).unwrap();

        let fetched = db
            .get_outbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .expect("record should exist");

        assert_eq!(fetched, rec);
    }

    #[test]
    fn query_is_scoped_by_vault_and_provider() {
        let db = setup_test_db();

        db.conn
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'server', 100, 100)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v2', 'gdrive', 200, 200)",
                [],
            )
            .unwrap();

        let mut r1 = sample_record("v1", "gdrive", 1);
        r1.state = OutboxState::Ready;
        db.insert_outbox_record(&r1).unwrap();

        let mut r2 = sample_record("v1", "server", 2);
        r2.state = OutboxState::Ready;
        db.insert_outbox_record(&r2).unwrap();

        let mut r3 = sample_record("v2", "gdrive", 3);
        r3.state = OutboxState::Ready;
        db.insert_outbox_record(&r3).unwrap();

        let dispatch_v1_gdrive = db
            .get_dispatchable_outbox("v1", "gdrive", 2000, 10)
            .unwrap();
        assert_eq!(dispatch_v1_gdrive.len(), 1);
        assert_eq!(dispatch_v1_gdrive[0].operation_id, [1; 16]);

        let dispatch_v1_server = db
            .get_dispatchable_outbox("v1", "server", 2000, 10)
            .unwrap();
        assert_eq!(dispatch_v1_server.len(), 1);
        assert_eq!(dispatch_v1_server[0].operation_id, [2; 16]);

        let dispatch_v2_gdrive = db
            .get_dispatchable_outbox("v2", "gdrive", 2000, 10)
            .unwrap();
        assert_eq!(dispatch_v2_gdrive.len(), 1);
        assert_eq!(dispatch_v2_gdrive[0].operation_id, [3; 16]);
    }

    #[test]
    fn duplicate_identical_insert_is_idempotent() {
        let db = setup_test_db();
        let rec = sample_record("v1", "gdrive", 1);

        db.insert_outbox_record(&rec).unwrap();

        db.transition_outbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            OutboxState::Prepared,
            OutboxState::Ready,
            None,
            None,
            2000,
        )
        .unwrap();

        let res = db.insert_outbox_record(&rec);
        assert!(res.is_ok(), "Re-inserting identical content must succeed");

        let fetched = db
            .get_outbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(fetched.state, OutboxState::Ready);
        assert_eq!(fetched.retry_count, 0);
        assert_eq!(fetched.updated_at, 2000);
    }

    #[test]
    fn duplicate_operation_id_with_different_content_is_rejected() {
        let db = setup_test_db();
        let rec = sample_record("v1", "gdrive", 1);

        db.insert_outbox_record(&rec).unwrap();

        let mut conflicting = rec.clone();
        conflicting.node_id = "differing_node".to_string();

        let res = db.insert_outbox_record(&conflicting);
        assert!(
            res.is_err(),
            "Inserting same operation_id with conflicting content must fail"
        );
        let err_msg = res.unwrap_err().to_string();
        assert!(
            err_msg.contains("collision"),
            "Error message should mention collision, got: {}",
            err_msg
        );
    }

    #[test]
    fn dispatch_query_bounded_deterministic_and_respects_retry() {
        let db = setup_test_db();

        let mut r1 = sample_record("v1", "gdrive", 1);
        r1.created_at = 100;
        r1.state = OutboxState::Ready;

        let mut r2 = sample_record("v1", "gdrive", 2);
        r2.created_at = 200;
        r2.state = OutboxState::Ready;

        let mut r3 = sample_record("v1", "gdrive", 3);
        r3.created_at = 150;
        r3.state = OutboxState::Ready;

        db.insert_outbox_record(&r1).unwrap();
        db.insert_outbox_record(&r2).unwrap();
        db.insert_outbox_record(&r3).unwrap();

        // Mutate r3 to Failed with valid parameters
        db.transition_outbox_state(
            "v1",
            "gdrive",
            &[3; 16],
            OutboxState::Ready,
            OutboxState::Failed,
            Some("error msg"),
            Some(500),
            200,
        )
        .unwrap();

        let mut r4 = sample_record("v1", "gdrive", 4);
        r4.created_at = 250;
        r4.state = OutboxState::Ready;
        db.insert_outbox_record(&r4).unwrap();

        db.transition_outbox_state(
            "v1",
            "gdrive",
            &[4; 16],
            OutboxState::Ready,
            OutboxState::Failed,
            Some("retry due"),
            Some(250),
            200,
        )
        .unwrap();

        // When now = 300, limit = 2:
        // Dispatchable candidates: r1 (created 100), r2 (created 200), r4 (retry 250 <= 300, created 250).
        // r3 (retry 500 > 300) is NOT dispatchable.
        let dispatch = db.get_dispatchable_outbox("v1", "gdrive", 300, 2).unwrap();
        assert_eq!(dispatch.len(), 2, "Limit 2 must bound output to 2 items");

        assert_eq!(dispatch[0].operation_id, [1; 16]);
        assert_eq!(dispatch[1].operation_id, [2; 16]);

        let dispatch_all = db.get_dispatchable_outbox("v1", "gdrive", 300, 10).unwrap();
        assert_eq!(dispatch_all.len(), 3);
        assert_eq!(dispatch_all[0].operation_id, [1; 16]);
        assert_eq!(dispatch_all[1].operation_id, [2; 16]);
        assert_eq!(dispatch_all[2].operation_id, [4; 16]);
    }

    #[test]
    fn dispatch_query_rejects_invalid_limits() {
        let db = setup_test_db();

        let zero_res = db.get_dispatchable_outbox("v1", "gdrive", 100, 0);
        assert!(zero_res.is_err(), "Limit = 0 must be rejected");

        let over_res =
            db.get_dispatchable_outbox("v1", "gdrive", 100, MAX_OUTBOX_DISPATCH_BATCH + 1);
        assert!(
            over_res.is_err(),
            "Limit > MAX_OUTBOX_DISPATCH_BATCH must be rejected"
        );

        let max_usize_res = db.get_dispatchable_outbox("v1", "gdrive", 100, usize::MAX);
        assert!(max_usize_res.is_err(), "usize::MAX limit must be rejected");

        let valid_max_res =
            db.get_dispatchable_outbox("v1", "gdrive", 100, MAX_OUTBOX_DISPATCH_BATCH);
        assert!(
            valid_max_res.is_ok(),
            "Valid maximum limit 1000 must succeed"
        );
    }

    #[test]
    fn cas_transition_with_wrong_expected_state_fails() {
        let db = setup_test_db();
        let rec = sample_record("v1", "gdrive", 1);

        db.insert_outbox_record(&rec).unwrap();

        let res = db.transition_outbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            OutboxState::Ready,
            OutboxState::Sent,
            None,
            None,
            2000,
        );

        assert!(
            res.is_err(),
            "CAS transition with mismatched expected state must fail"
        );

        let fetched = db
            .get_outbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(fetched.state, OutboxState::Prepared);
    }

    #[test]
    fn failed_transition_validates_inputs_before_sql() {
        let db = setup_test_db();
        let rec = sample_record("v1", "gdrive", 1);

        db.insert_outbox_record(&rec).unwrap();

        // 1. Missing error message
        let res1 = db.transition_outbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            OutboxState::Prepared,
            OutboxState::Failed,
            None,
            Some(1000),
            500,
        );
        assert!(res1.is_err(), "Missing error message must return Err");

        // 2. Blank whitespace error message
        let res2 = db.transition_outbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            OutboxState::Prepared,
            OutboxState::Failed,
            Some("   "),
            Some(1000),
            500,
        );
        assert!(res2.is_err(), "Blank error message must return Err");

        // 3. Missing next_retry_at timestamp
        let res3 = db.transition_outbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            OutboxState::Prepared,
            OutboxState::Failed,
            Some("valid error"),
            None,
            500,
        );
        assert!(res3.is_err(), "Missing next_retry_at must return Err");

        // 4. Past next_retry_at timestamp (ts = 400 < now = 500)
        let res4 = db.transition_outbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            OutboxState::Prepared,
            OutboxState::Failed,
            Some("valid error"),
            Some(400),
            500,
        );
        assert!(
            res4.is_err(),
            "next_retry_at in the past relative to now must return Err"
        );

        // Verify DB row remains completely untouched in state Prepared with retry_count 0
        let fetched = db
            .get_outbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(fetched.state, OutboxState::Prepared);
        assert_eq!(fetched.retry_count, 0);
        assert_eq!(fetched.last_error, None);
        assert_eq!(fetched.next_retry_at, None);
    }

    #[test]
    fn valid_failed_transition_increments_retry_count_and_saves_error() {
        let db = setup_test_db();
        let rec = sample_record("v1", "gdrive", 1);

        db.insert_outbox_record(&rec).unwrap();

        db.transition_outbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            OutboxState::Prepared,
            OutboxState::Failed,
            Some("connection timeout"),
            Some(1500),
            1000,
        )
        .unwrap();

        let fetched = db
            .get_outbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(fetched.state, OutboxState::Failed);
        assert_eq!(fetched.retry_count, 1);
        assert_eq!(fetched.last_error.as_deref(), Some("connection timeout"));
        assert_eq!(fetched.next_retry_at, Some(1500));
        assert_eq!(fetched.updated_at, 1000);
    }

    #[test]
    fn invalid_record_validation_rejects_empty_ids() {
        let mut rec = sample_record("v1", "gdrive", 1);

        rec.vault_id = "".to_string();
        assert!(rec.validate().is_err(), "Empty vault_id must fail");

        rec.vault_id = "v1".to_string();
        rec.provider_id = "".to_string();
        assert!(rec.validate().is_err(), "Empty provider_id must fail");

        rec.provider_id = "gdrive".to_string();
        rec.node_id = "".to_string();
        assert!(rec.validate().is_err(), "Empty node_id must fail");
    }

    #[test]
    fn corrupt_operation_id_length_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp,
                    state, retry_count, created_at, updated_at
                ) VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 100, 'ready', 0, 100, 100)",
                params![vec![99u8; 10]],
            )
            .unwrap();

        let res = db.get_dispatchable_outbox("v1", "gdrive", 200, 10);
        assert!(
            res.is_err(),
            "operation_id with 10 bytes must return Err when decoding during dispatch query"
        );
    }

    #[test]
    fn corrupt_source_hash_length_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, source_hash,
                    original_timestamp, state, retry_count, created_at, updated_at
                ) VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', ?2, 100, 'prepared', 0, 100, 100)",
                params![vec![1u8; 16], vec![99u8; 10]],
            )
            .unwrap();

        let res = db.get_outbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "source_hash with 10 bytes must return Err when decoding"
        );
    }

    #[test]
    fn corrupt_payload_hash_length_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, payload_hash,
                    original_timestamp, state, retry_count, created_at, updated_at
                ) VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', ?2, 100, 'prepared', 0, 100, 100)",
                params![vec![1u8; 16], vec![99u8; 10]],
            )
            .unwrap();

        let res = db.get_outbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "payload_hash with 10 bytes must return Err when decoding"
        );
    }

    #[test]
    fn corrupt_negative_retry_count_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp,
                    state, retry_count, created_at, updated_at
                ) VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 100, 'prepared', -5, 100, 100)",
                params![vec![1u8; 16]],
            )
            .unwrap();

        let res = db.get_outbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "negative retry_count must return Err when decoding"
        );
    }

    #[test]
    fn corrupt_entry_kind_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp,
                    state, retry_count, created_at, updated_at
                ) VALUES ('v1', 'gdrive', ?1, 'bogus_kind', 'n1', 100, 'prepared', 0, 100, 100)",
                params![vec![1u8; 16]],
            )
            .unwrap();

        let res = db.get_outbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "invalid entry_kind string must return Err when decoding"
        );
    }

    #[test]
    fn corrupt_outbox_state_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, original_timestamp,
                    state, retry_count, created_at, updated_at
                ) VALUES ('v1', 'gdrive', ?1, 'upsert', 'n1', 100, 'bogus_state', 0, 100, 100)",
                params![vec![1u8; 16]],
            )
            .unwrap();

        let res = db.get_outbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "invalid state string must return Err when decoding"
        );
    }

    #[test]
    fn foreign_key_to_provider_state_is_enforced() {
        let db = setup_test_db();
        let rec = sample_record("nonexistent_vault", "gdrive", 1);

        let res = db.insert_outbox_record(&rec);
        assert!(
            res.is_err(),
            "Inserting outbox record for non-existent vault must fail FK constraint"
        );
    }

    #[test]
    fn same_source_hash_reuses_exact_durable_outbox_operation() {
        let mut db = setup_test_db();
        let mut rec1 = sample_record("v1", "gdrive", 1);
        rec1.operation_id = [1; 16];
        rec1.source_hash = Some([9u8; 32]);

        let candidate_a = db.enqueue_or_reuse_outbox_operation(&rec1).unwrap();
        assert_eq!(candidate_a, rec1.operation_id);

        let mut rec2 = sample_record("v1", "gdrive", 2);
        rec2.operation_id = [2; 16];
        rec2.source_hash = Some([9u8; 32]);
        rec2.rel_path = rec1.rel_path.clone();
        rec2.node_id = rec1.node_id.clone();

        let candidate_b = db.enqueue_or_reuse_outbox_operation(&rec2).unwrap();
        assert_eq!(candidate_a, candidate_b);
        assert_eq!(candidate_b, rec1.operation_id);
    }

    type BaselineTuple = (String, String, String, String, i64);
    type PathTuple = (String, String, String, i64);

    // Helper for snapshot testing
    fn snapshot_outbox_baseline_and_path(
        db: &mut DbBridge,
        vault_id: &str,
        provider_id: &str,
        op_id: &[u8; 16],
        path: &str,
    ) -> (
        Option<OutboxRecord>,
        Option<BaselineTuple>,
        Option<PathTuple>,
    ) {
        let rec = db.get_outbox_by_id(vault_id, provider_id, op_id).unwrap();
        let baseline = db
            .conn
            .query_row(
                "SELECT vault_id, provider_id, rel_path, content_hash, updated_at FROM sync_document_baselines WHERE vault_id = ?1 AND provider_id = ?2 AND rel_path = ?3",
                params![vault_id, provider_id, path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()
            .unwrap();
        let path_tuple = db
            .conn
            .query_row(
                "SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1 AND rel_path = ?2",
                params![vault_id, path],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .unwrap();
        (rec, baseline, path_tuple)
    }

    #[test]
    fn accepted_outbox_commit_atomically_updates_baseline_and_ack_state() {
        let mut db = setup_test_db();
        let mut rec = sample_record("v1", "gdrive", 1);
        rec.state = OutboxState::Sent;
        rec.rel_path = Some("foo.md".into());
        rec.doc_hash = Some([7u8; 32]);
        rec.source_hash = Some([9u8; 32]);
        db.insert_outbox_record(&rec).unwrap();
        db.conn.execute("INSERT INTO sync_document_paths (vault_id, doc_id, rel_path, updated_at) VALUES ('v1', 'node_1', 'foo.md', 100)", []).unwrap();

        let before =
            snapshot_outbox_baseline_and_path(&mut db, "v1", "gdrive", &rec.operation_id, "foo.md");
        db.conn.execute_batch("CREATE TRIGGER fail_baseline BEFORE INSERT ON sync_document_baselines BEGIN SELECT RAISE(ABORT, 'injected DB failure'); END;").unwrap();

        let now = 2000i64;
        let _ = db.commit_accepted_outbox_operation(&rec, None, now);

        let after_failure =
            snapshot_outbox_baseline_and_path(&mut db, "v1", "gdrive", &rec.operation_id, "foo.md");
        assert_eq!(after_failure, before);

        db.conn
            .execute_batch("DROP TRIGGER fail_baseline;")
            .unwrap();
        db.commit_accepted_outbox_operation(&rec, None, now).unwrap();

        let after_success =
            snapshot_outbox_baseline_and_path(&mut db, "v1", "gdrive", &rec.operation_id, "foo.md");

        let source_hash = rec.source_hash.as_ref().unwrap();
        let expected_hash = hex::encode(source_hash);
        let expected_success = (
            Some({
                let mut r = rec.clone();
                r.state = OutboxState::Acknowledged;
                r.next_retry_at = None;
                r.last_error = None;
                r.updated_at = now;
                r
            }),
            Some((
                "v1".into(),
                "gdrive".into(),
                "foo.md".into(),
                expected_hash,
                now,
            )),
            Some(("v1".into(), "node_1".into(), "foo.md".into(), now)),
        );
        assert_eq!(after_success, expected_success);
    }

    #[test]
    fn accepted_delete_removes_baseline_and_path_atomically() {
        let mut db = setup_test_db();
        let mut rec = sample_record("v1", "gdrive", 1);
        rec.entry_kind = synabit_protocol::SyncEntryKind::Delete;
        rec.state = OutboxState::Sent;
        rec.rel_path = Some("del.md".into());
        db.insert_outbox_record(&rec).unwrap();
        db.conn.execute("INSERT INTO sync_document_paths (vault_id, doc_id, rel_path, updated_at) VALUES ('v1', 'node_1', 'del.md', 100)", []).unwrap();
        db.conn.execute("INSERT INTO sync_document_baselines (vault_id, provider_id, rel_path, content_hash, updated_at) VALUES ('v1', 'gdrive', 'del.md', 'hash1', 100)", []).unwrap();

        let before =
            snapshot_outbox_baseline_and_path(&mut db, "v1", "gdrive", &rec.operation_id, "del.md");
        assert!(before.1.is_some());
        assert!(before.2.is_some());

        db.conn.execute_batch("CREATE TRIGGER fail_delete_path BEFORE DELETE ON sync_document_paths BEGIN SELECT RAISE(ABORT, 'injected DB failure'); END;").unwrap();

        let now = 2000i64;
        let _ = db.commit_accepted_outbox_operation(&rec, None, now);

        let after_failure =
            snapshot_outbox_baseline_and_path(&mut db, "v1", "gdrive", &rec.operation_id, "del.md");
        assert_eq!(after_failure, before);

        db.conn
            .execute_batch("DROP TRIGGER fail_delete_path;")
            .unwrap();

        db.commit_accepted_outbox_operation(&rec, None, now).unwrap();

        let after_success =
            snapshot_outbox_baseline_and_path(&mut db, "v1", "gdrive", &rec.operation_id, "del.md");

        let expected_success = (
            Some({
                let mut r = rec.clone();
                r.state = OutboxState::Acknowledged;
                r.next_retry_at = None;
                r.last_error = None;
                r.updated_at = now;
                r
            }),
            None,
            None,
        );
        assert_eq!(after_success, expected_success);
    }

    #[test]
    fn snapshot_all_scoped_outbox_rejects_malformed_operation_id_without_aliasing_zero_id() {
        fn raw_outbox_snapshot(db: &DbBridge) -> Vec<Vec<rusqlite::types::Value>> {
            let mut stmt = db
                .conn
                .prepare(
                    "SELECT vault_id, provider_id, operation_id, entry_kind, node_id, rel_path,
                            doc_hash, source_hash, original_timestamp, encrypted_payload,
                            payload_hash, asset_ref_blob, state, retry_count, next_retry_at,
                            last_error, created_at, updated_at
                     FROM sync_outbox
                     WHERE vault_id = 'v1' AND provider_id = 'gdrive'
                     ORDER BY operation_id",
                )
                .unwrap();

            stmt.query_map([], |row| {
                (0..18)
                    .map(|index| row.get::<_, rusqlite::types::Value>(index))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
        }

        let db = setup_test_db();
        let zero_id = vec![0u8; 16];
        let mut zero_rec = sample_record("v1", "gdrive", 1);
        zero_rec.operation_id = zero_id.as_slice().try_into().unwrap();
        db.insert_outbox_record(&zero_rec).unwrap();

        db.conn
            .execute("PRAGMA ignore_check_constraints = ON;", [])
            .unwrap();

        let malformed_id = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        db.conn
            .execute(
                "INSERT INTO sync_outbox (
                    vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash,
                    source_hash, original_timestamp, encrypted_payload, payload_hash,
                    asset_ref_blob, state, retry_count, next_retry_at, last_error,
                    created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    "v1",
                    "gdrive",
                    malformed_id,
                    "upsert",
                    "node_2",
                    "file2.txt",
                    vec![1u8; 32],
                    vec![2u8; 32],
                    1000i64,
                    vec![3u8; 16],
                    vec![4u8; 32],
                    Option::<Vec<u8>>::None,
                    "ready",
                    0i64,
                    Option::<i64>::None,
                    Option::<String>::None,
                    1000i64,
                    1000i64,
                ],
            )
            .unwrap();

        let before = raw_outbox_snapshot(&db);
        let res = db.snapshot_all_scoped_outbox("v1", "gdrive");
        assert!(res.is_err());
        let err_str = res.unwrap_err().to_string();
        assert!(
            err_str.contains("operation_id length must be 16 bytes"),
            "Expected error context 'operation_id length must be 16 bytes', got: {}",
            err_str
        );
        let after = raw_outbox_snapshot(&db);
        assert_eq!(after, before);
    }

    #[test]
    fn outbox_validation_rejects_incomplete_new_rows_and_entry_kind_mismatch() {
        let mut db = setup_test_db();
        let before_validation = db.snapshot_all_scoped_outbox("v1", "gdrive").unwrap();

        let missing_doc_hash = {
            let mut r1 = sample_record("v1", "gdrive", 1);
            r1.doc_hash = None;
            r1
        };
        assert!(missing_doc_hash.validate_complete().is_err());
        assert!(db.insert_outbox_record(&missing_doc_hash).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&missing_doc_hash)
            .is_err());

        let missing_rel_path = {
            let mut r2 = sample_record("v1", "gdrive", 2);
            r2.rel_path = None;
            r2
        };
        assert!(missing_rel_path.validate_complete().is_err());
        assert!(db.insert_outbox_record(&missing_rel_path).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&missing_rel_path)
            .is_err());

        let missing_source_hash = {
            let mut r3 = sample_record("v1", "gdrive", 3);
            r3.source_hash = None;
            r3
        };
        assert!(missing_source_hash.validate_complete().is_err());
        assert!(db.insert_outbox_record(&missing_source_hash).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&missing_source_hash)
            .is_err());

        let missing_encrypted_payload = {
            let mut r4 = sample_record("v1", "gdrive", 4);
            r4.encrypted_payload = None;
            r4
        };
        assert!(missing_encrypted_payload.validate_complete().is_err());
        assert!(db.insert_outbox_record(&missing_encrypted_payload).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&missing_encrypted_payload)
            .is_err());

        let missing_payload_hash = {
            let mut r5 = sample_record("v1", "gdrive", 5);
            r5.payload_hash = None;
            r5
        };
        assert!(missing_payload_hash.validate_complete().is_err());
        assert!(db.insert_outbox_record(&missing_payload_hash).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&missing_payload_hash)
            .is_err());

        let upsert_with_asset_ref = {
            let mut r6 = sample_record("v1", "gdrive", 6);
            r6.entry_kind = SyncEntryKind::Upsert;
            r6.asset_ref_blob = Some(vec![1, 2, 3]);
            r6
        };
        assert!(upsert_with_asset_ref.validate_complete().is_err());
        assert!(db.insert_outbox_record(&upsert_with_asset_ref).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&upsert_with_asset_ref)
            .is_err());

        let asset_without_asset_ref = {
            let mut r7 = sample_record("v1", "gdrive", 7);
            r7.entry_kind = SyncEntryKind::AssetReference;
            r7.asset_ref_blob = None;
            r7
        };
        assert!(asset_without_asset_ref.validate_complete().is_err());
        assert!(db.insert_outbox_record(&asset_without_asset_ref).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&asset_without_asset_ref)
            .is_err());

        let incomplete_failed = {
            let mut r8 = sample_record("v1", "gdrive", 8);
            r8.state = OutboxState::Failed;
            r8.next_retry_at = Some(100);
            r8.doc_hash = None;
            r8
        };
        assert!(incomplete_failed.validate_complete().is_err());
        assert!(db.insert_outbox_record(&incomplete_failed).is_err());
        assert!(db
            .enqueue_or_reuse_outbox_operation(&incomplete_failed)
            .is_err());

        let after_validation = db.snapshot_all_scoped_outbox("v1", "gdrive").unwrap();
        assert_eq!(after_validation, before_validation);
    }

    #[test]
    fn sent_batch_rejects_duplicate_not_due_and_rolls_back_second_item() {
        let mut db = setup_test_db();
        let duplicate = [1u8; 16];
        let second_item = [2u8; 16];
        let wrong_state_id = [3u8; 16];

        let mut r1 = sample_record("v1", "gdrive", 1);
        r1.operation_id = duplicate;
        r1.state = OutboxState::Ready;
        db.insert_outbox_record(&r1).unwrap();

        let mut not_due = sample_record("v1", "gdrive", 2);
        not_due.operation_id = second_item;
        not_due.state = OutboxState::Failed;
        not_due.next_retry_at = Some(99999);
        db.insert_outbox_record(&not_due).unwrap();

        let mut wrong_state = sample_record("v1", "gdrive", 3);
        wrong_state.operation_id = wrong_state_id;
        wrong_state.state = OutboxState::Prepared;
        db.insert_outbox_record(&wrong_state).unwrap();

        let before = (
            db.get_outbox_by_id("v1", "gdrive", &duplicate)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &second_item)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &wrong_state_id)
                .unwrap()
                .unwrap(),
        );

        db.conn.execute_batch("CREATE TRIGGER fail_update_on_prevalidation UPDATE ON sync_outbox BEGIN SELECT RAISE(ABORT, 'prevalidation must fail before UPDATE'); END;").unwrap();

        let res_dup = db.mark_outbox_batch_sent("v1", "gdrive", &[duplicate, duplicate], 1000);
        assert!(res_dup.is_err());
        let after_dup = (
            db.get_outbox_by_id("v1", "gdrive", &duplicate)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &second_item)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &wrong_state_id)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(after_dup, before);

        let res_not_due =
            db.mark_outbox_batch_sent("v1", "gdrive", &[duplicate, second_item], 1000);
        assert!(res_not_due.is_err());
        let after_not_due = (
            db.get_outbox_by_id("v1", "gdrive", &duplicate)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &second_item)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &wrong_state_id)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(after_not_due, before);

        let res_wrong_state =
            db.mark_outbox_batch_sent("v1", "gdrive", &[duplicate, wrong_state_id], 1000);
        assert!(res_wrong_state.is_err());
        let after_wrong_state = (
            db.get_outbox_by_id("v1", "gdrive", &duplicate)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &second_item)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &wrong_state_id)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(after_wrong_state, before);

        db.conn
            .execute_batch("DROP TRIGGER fail_update_on_prevalidation;")
            .unwrap();

        let now = 1000i64;
        let res_valid = db.mark_outbox_batch_sent("v1", "gdrive", &[duplicate], now);
        assert!(res_valid.is_ok());

        let after_valid = (
            db.get_outbox_by_id("v1", "gdrive", &duplicate)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &second_item)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &wrong_state_id)
                .unwrap()
                .unwrap(),
        );
        let expected_valid = (
            {
                let mut r = before.0.clone();
                r.state = OutboxState::Sent;
                r.updated_at = now;
                r
            },
            before.1.clone(),
            before.2.clone(),
        );
        assert_eq!(after_valid, expected_valid);
    }

    #[test]
    fn outbox_roundtrip_reconstructs_exact_sync_operation() {
        let db = setup_test_db();
        let mut rec = sample_record("v1", "gdrive", 1);
        rec.operation_id = [12; 16];
        db.insert_outbox_record(&rec).unwrap();

        let read_rec = db
            .get_outbox_by_id("v1", "gdrive", &rec.operation_id)
            .unwrap()
            .unwrap();
        let actual = outbox_record_to_sync_operation(&read_rec).unwrap();

        let expected = crate::sync::core::types::SyncOperation {
            operation_id: rec.operation_id,
            doc_hash: rec.doc_hash.unwrap(),
            entry_kind: rec.entry_kind.clone(),
            node_id: rec.node_id.clone(),
            rel_path: rec.rel_path.clone().unwrap(),
            encrypted_payload: rec.encrypted_payload.clone().unwrap(),
            payload_hash: rec.payload_hash.unwrap(),
            timestamp: rec.original_timestamp,
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn pending_source_reuse_covers_sent_failed_and_exact_scope() {
        let mut db = setup_test_db();
        db.conn.execute("INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v2', 'root2', 1, 0, 0)", []).unwrap();
        db.conn.execute("INSERT INTO sync_provider_state (vault_id, provider_id, cursor, sync_state, created_at, updated_at) VALUES ('v1', 'server', '', 'ready', 0, 0), ('v2', 'gdrive', '', 'ready', 0, 0)", []).unwrap();

        let before = db
            .get_dispatchable_outbox("v1", "gdrive", 10000, 10)
            .unwrap();

        // 1. Sent state reuse
        let mut rec1 = sample_record("v1", "gdrive", 1);
        rec1.state = OutboxState::Sent;
        rec1.source_hash = Some([9u8; 32]);
        db.insert_outbox_record(&rec1).unwrap();
        let candidate1 = db.enqueue_or_reuse_outbox_operation(&rec1).unwrap();
        assert_eq!(candidate1, rec1.operation_id);

        // 2. Failed state reuse
        let mut rec2 = sample_record("v1", "gdrive", 2);
        rec2.state = OutboxState::Failed;
        rec2.source_hash = Some([8u8; 32]);
        db.insert_outbox_record(&rec2).unwrap();
        let candidate2 = db.enqueue_or_reuse_outbox_operation(&rec2).unwrap();
        assert_eq!(candidate2, rec2.operation_id);

        // 3. Acknowledged state MUST NOT be reused (new operation_id created)
        let mut rec3 = sample_record("v1", "gdrive", 3);
        rec3.operation_id = [3; 16];
        rec3.state = OutboxState::Acknowledged;
        rec3.source_hash = Some([7u8; 32]);
        db.insert_outbox_record(&rec3).unwrap();
        let mut rec3_new = sample_record("v1", "gdrive", 33);
        rec3_new.operation_id = [33; 16];
        rec3_new.source_hash = rec3.source_hash;
        rec3_new.rel_path = rec3.rel_path.clone();
        rec3_new.node_id = rec3.node_id.clone();
        let candidate3 = db.enqueue_or_reuse_outbox_operation(&rec3_new).unwrap();
        assert_eq!(candidate3, rec3_new.operation_id);

        // 4. Scope isolation: different vault "v2" or provider "server" or different path/hash
        let mut rec_v2 = sample_record("v2", "gdrive", 4);
        rec_v2.source_hash = rec1.source_hash;
        rec_v2.rel_path = rec1.rel_path.clone();
        rec_v2.node_id = rec1.node_id.clone();
        let candidate_v2 = db.enqueue_or_reuse_outbox_operation(&rec_v2).unwrap();
        assert_eq!(candidate_v2, rec_v2.operation_id);

        let mut rec_srv = sample_record("v1", "server", 5);
        rec_srv.source_hash = rec1.source_hash;
        rec_srv.rel_path = rec1.rel_path.clone();
        rec_srv.node_id = rec1.node_id.clone();
        let candidate_srv = db.enqueue_or_reuse_outbox_operation(&rec_srv).unwrap();
        assert_eq!(candidate_srv, rec_srv.operation_id);

        let mut rec_different = sample_record("v1", "gdrive", 6);
        rec_different.source_hash = Some([99u8; 32]); // different source_hash
        rec_different.rel_path = rec1.rel_path.clone();
        rec_different.node_id = rec1.node_id.clone();
        let candidate_diff = db
            .enqueue_or_reuse_outbox_operation(&rec_different)
            .unwrap();
        assert_eq!(candidate_diff, rec_different.operation_id);

        let after = db
            .get_dispatchable_outbox("v1", "gdrive", 10000, 10)
            .unwrap();
        assert_ne!(before, after);
    }

    #[test]
    fn outbox_state_mutations_are_scoped_cas_and_batch_atomic() {
        let mut db = setup_test_db();
        db.conn.execute("INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v2', 'root2', 1, 0, 0)", []).unwrap();
        db.conn.execute("INSERT INTO sync_provider_state (vault_id, provider_id, cursor, sync_state, created_at, updated_at) VALUES ('v1', 'server', '', 'ready', 0, 0), ('v2', 'gdrive', '', 'ready', 0, 0)", []).unwrap();

        let same_operation_id = [10; 16];

        let mut rec1 = sample_record("v1", "gdrive", 1);
        rec1.operation_id = same_operation_id;
        rec1.state = OutboxState::Ready;
        db.insert_outbox_record(&rec1).unwrap();

        let mut rec_v2 = sample_record("v2", "gdrive", 1);
        rec_v2.operation_id = same_operation_id;
        rec_v2.state = OutboxState::Ready;
        db.insert_outbox_record(&rec_v2).unwrap();

        let mut rec_srv = sample_record("v1", "server", 1);
        rec_srv.operation_id = same_operation_id;
        rec_srv.state = OutboxState::Ready;
        db.insert_outbox_record(&rec_srv).unwrap();

        let before = (
            db.get_outbox_by_id("v1", "gdrive", &same_operation_id)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v2", "gdrive", &same_operation_id)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "server", &same_operation_id)
                .unwrap()
                .unwrap(),
        );

        // Mutate v1/gdrive
        db.mark_outbox_batch_sent("v1", "gdrive", &[same_operation_id], 1000)
            .unwrap();

        let after = (
            db.get_outbox_by_id("v1", "gdrive", &same_operation_id)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v2", "gdrive", &same_operation_id)
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "server", &same_operation_id)
                .unwrap()
                .unwrap(),
        );

        assert_eq!(after.0.state, OutboxState::Sent);
        assert_eq!(after.1, before.1); // v2/gdrive untouched
        assert_eq!(after.2, before.2); // v1/server untouched
    }

    #[test]
    fn retry_batch_failure_is_atomic_and_backoff_is_capped() {
        let mut db = setup_test_db();
        let mut rec1 = sample_record("v1", "gdrive", 1);
        rec1.operation_id = [1; 16];
        rec1.state = OutboxState::Sent;
        rec1.retry_count = 0;
        db.insert_outbox_record(&rec1).unwrap();

        let mut rec2 = sample_record("v1", "gdrive", 2);
        rec2.operation_id = [2; 16];
        rec2.state = OutboxState::Ready; // Not sent! Will cause batch retry failure
        db.insert_outbox_record(&rec2).unwrap();

        let before = (
            db.get_outbox_by_id("v1", "gdrive", &[1; 16])
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &[2; 16])
                .unwrap()
                .unwrap(),
        );

        let res = db.schedule_outbox_batch_retry(
            "v1",
            "gdrive",
            &[rec1.operation_id, rec2.operation_id],
            "network error",
            1000,
        );
        assert!(res.is_err()); // Atomic rollback

        let after_failure = (
            db.get_outbox_by_id("v1", "gdrive", &[1; 16])
                .unwrap()
                .unwrap(),
            db.get_outbox_by_id("v1", "gdrive", &[2; 16])
                .unwrap()
                .unwrap(),
        );
        assert_eq!(after_failure, before);

        // High retry count to test capped backoff
        let mut rec3 = sample_record("v1", "gdrive", 3);
        rec3.operation_id = [3; 16];
        rec3.state = OutboxState::Sent;
        rec3.retry_count = 50;
        db.insert_outbox_record(&rec3).unwrap();

        db.schedule_outbox_batch_retry("v1", "gdrive", &[rec3.operation_id], "error", 1000)
            .unwrap();
        let r3 = db
            .get_outbox_by_id("v1", "gdrive", &[3; 16])
            .unwrap()
            .unwrap();
        assert_eq!(r3.state, OutboxState::Failed);
        assert_eq!(
            r3.next_retry_at,
            Some(1000 + DbBridge::MAX_OUTBOX_RETRY_DELAY)
        );
    }

    // -----------------------------------------------------------------------
    // Coalescing
    // -----------------------------------------------------------------------

    fn revision(vault: &str, provider: &str, op_byte: u8, node: &str, content: u8) -> OutboxRecord {
        let mut rec = sample_record(vault, provider, op_byte);
        rec.node_id = node.to_string();
        rec.rel_path = Some(format!("{node}.md"));
        rec.source_hash = Some([content; 32]);
        rec.encrypted_payload = Some(vec![content; 8]);
        rec.payload_hash = Some([content; 32]);
        rec.state = OutboxState::Ready;
        rec
    }

    fn outbox_rows(db: &DbBridge, node: &str) -> Vec<(Vec<u8>, String, Option<Vec<u8>>)> {
        let mut stmt = db
            .conn
            .prepare(
                "SELECT operation_id, state, encrypted_payload FROM sync_outbox
                 WHERE node_id = ?1 ORDER BY created_at, operation_id",
            )
            .unwrap();
        stmt.query_map([node], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    /// Revising the same document before it goes out must replace the queued
    /// entry, not add another. Every payload is a complete CRDT snapshot, so
    /// the newest one already contains the earlier ones.
    #[test]
    fn revisions_of_a_queued_document_collapse_into_one_entry() {
        let mut db = setup_test_db();

        for (op, content) in [(1u8, 10u8), (2, 20), (3, 30)] {
            db.enqueue_or_reuse_outbox_operation(&revision("v1", "gdrive", op, "note", content))
                .unwrap();
        }

        let rows = outbox_rows(&db, "note");
        assert_eq!(rows.len(), 1, "three revisions queued three entries");
        assert_eq!(
            rows[0].2,
            Some(vec![30u8; 8]),
            "the queued entry does not hold the newest revision"
        );
        assert_eq!(
            rows[0].0,
            vec![1u8; 16],
            "coalescing should keep the original operation id"
        );
    }

    /// An operation already handed to the transport must be left alone: the
    /// server may have it, and its acknowledgement is matched by operation id.
    #[test]
    fn a_sent_operation_is_never_rewritten_underneath_the_push() {
        let mut db = setup_test_db();

        db.enqueue_or_reuse_outbox_operation(&revision("v1", "gdrive", 1, "note", 10))
            .unwrap();
        db.mark_outbox_batch_sent("v1", "gdrive", &[[1u8; 16]], 2000)
            .unwrap();

        db.enqueue_or_reuse_outbox_operation(&revision("v1", "gdrive", 2, "note", 20))
            .unwrap();

        let rows = outbox_rows(&db, "note");
        assert_eq!(rows.len(), 2, "the in-flight entry should have been left in place");
        assert_eq!(rows[0].1, "sent");
        assert_eq!(rows[0].2, Some(vec![10u8; 8]), "the sent payload was modified");
        assert_eq!(rows[1].2, Some(vec![20u8; 8]));
    }

    /// A revision arriving after a failure clears the back-off, since the new
    /// content deserves a fresh attempt.
    #[test]
    fn coalescing_onto_a_failed_entry_clears_its_backoff() {
        let mut db = setup_test_db();

        db.enqueue_or_reuse_outbox_operation(&revision("v1", "gdrive", 1, "note", 10))
            .unwrap();
        db.mark_outbox_batch_sent("v1", "gdrive", &[[1u8; 16]], 2000)
            .unwrap();
        db.schedule_outbox_retry("v1", "gdrive", &[1u8; 16], "network down", 2000)
            .unwrap();

        db.enqueue_or_reuse_outbox_operation(&revision("v1", "gdrive", 9, "note", 30))
            .unwrap();

        let rows = outbox_rows(&db, "note");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "ready", "a fresh revision should be dispatchable");

        let (retry_count, next_retry, last_error): (i64, Option<i64>, Option<String>) = db
            .conn
            .query_row(
                "SELECT retry_count, next_retry_at, last_error FROM sync_outbox WHERE node_id = 'note'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(retry_count, 0);
        assert_eq!(next_retry, None);
        assert_eq!(last_error, None);
    }
}
