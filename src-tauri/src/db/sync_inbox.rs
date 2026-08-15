use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension, Row};
use std::fmt;
use std::str::FromStr;
use synabit_protocol::SyncEntryKind;

pub const MAX_INBOX_STAGE_ENTRIES: usize = 1000;
pub const MAX_INBOX_STAGE_PAYLOAD_BYTES: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxState {
    Pending,
    Applying,
    PendingAsset,
    Applied,
    IgnoredOwnOperation,
    Failed,
    Quarantined,
}

impl InboxState {
    pub fn as_str(&self) -> &'static str {
        match self {
            InboxState::Pending => "pending",
            InboxState::Applying => "applying",
            InboxState::PendingAsset => "pending_asset",
            InboxState::Applied => "applied",
            InboxState::IgnoredOwnOperation => "ignored_own_operation",
            InboxState::Failed => "failed",
            InboxState::Quarantined => "quarantined",
        }
    }
}

impl fmt::Display for InboxState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for InboxState {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(InboxState::Pending),
            "applying" => Ok(InboxState::Applying),
            "pending_asset" => Ok(InboxState::PendingAsset),
            "applied" => Ok(InboxState::Applied),
            "ignored_own_operation" => Ok(InboxState::IgnoredOwnOperation),
            "failed" => Ok(InboxState::Failed),
            "quarantined" => Ok(InboxState::Quarantined),
            other => Err(AppError::General(format!(
                "Invalid sync_inbox state in DB: '{}'",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxRecord {
    pub vault_id: String,
    pub provider_id: String,
    pub page_cursor: String,
    pub remote_position: String,
    pub remote_seq: Option<u64>,
    pub operation_id: [u8; 16],
    pub doc_hash: [u8; 32],
    pub entry_kind: SyncEntryKind,
    pub encrypted_payload: Option<Vec<u8>>,
    pub payload_hash: Option<[u8; 32]>,
    pub source_device: Option<String>,
    pub state: InboxState,
    pub last_error: Option<String>,
    pub received_at: i64,
    pub updated_at: i64,
    pub applied_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEntryToStage {
    pub remote_position: String,
    pub remote_seq: Option<u64>,
    pub operation_id: [u8; 16],
    pub doc_hash: [u8; 32],
    pub entry_kind: SyncEntryKind,
    pub encrypted_payload: Option<Vec<u8>>,
    pub payload_hash: Option<[u8; 32]>,
    pub source_device: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxPageState {
    Staged,
    Applied,
    CursorCommitted,
}

impl InboxPageState {
    pub fn as_str(&self) -> &'static str {
        match self {
            InboxPageState::Staged => "staged",
            InboxPageState::Applied => "applied",
            InboxPageState::CursorCommitted => "cursor_committed",
        }
    }
}

impl fmt::Display for InboxPageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for InboxPageState {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "staged" => Ok(InboxPageState::Staged),
            "applied" => Ok(InboxPageState::Applied),
            "cursor_committed" => Ok(InboxPageState::CursorCommitted),
            other => Err(AppError::General(format!(
                "Invalid sync_inbox_pages state in DB: '{}'",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxPageRecord {
    pub vault_id: String,
    pub provider_id: String,
    pub start_cursor: String,
    pub next_cursor: String,
    pub has_more: bool,
    pub entry_count: u64,
    pub state: InboxPageState,
    pub received_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxPageEntryRecord {
    pub vault_id: String,
    pub provider_id: String,
    pub start_cursor: String,
    pub page_ordinal: u64,
    pub operation_id: [u8; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InboxStageResult {
    pub inserted_count: usize,
    pub duplicate_count: usize,
}

fn decode_inbox_row(row: &Row) -> Result<InboxRecord, rusqlite::Error> {
    let vault_id: String = row.get(0)?;
    let provider_id: String = row.get(1)?;
    let page_cursor: String = row.get(2)?;
    let remote_position: String = row.get(3)?;
    let remote_seq_i64: Option<i64> = row.get(4)?;
    let op_id_bytes: Vec<u8> = row.get(5)?;
    let doc_hash_bytes: Vec<u8> = row.get(6)?;
    let entry_kind_str: String = row.get(7)?;
    let encrypted_payload: Option<Vec<u8>> = row.get(8)?;
    let payload_hash_bytes: Option<Vec<u8>> = row.get(9)?;
    let source_device: Option<String> = row.get(10)?;
    let state_str: String = row.get(11)?;
    let last_error: Option<String> = row.get(12)?;
    let received_at: i64 = row.get(13)?;
    let updated_at: i64 = row.get(14)?;
    let applied_at: Option<i64> = row.get(15)?;

    let operation_id: [u8; 16] = op_id_bytes.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "operation_id length must be 16 bytes",
            )),
        )
    })?;

    let doc_hash: [u8; 32] = doc_hash_bytes.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "doc_hash length must be 32 bytes",
            )),
        )
    })?;

    let payload_hash = match payload_hash_bytes {
        Some(bytes) => Some(bytes.try_into().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                9,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "payload_hash length must be 32 bytes",
                )),
            )
        })?),
        None => None,
    };

    let remote_seq = match remote_seq_i64 {
        Some(seq) => Some(u64::try_from(seq).map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "remote_seq cannot be negative",
                )),
            )
        })?),
        None => None,
    };

    let entry_kind: SyncEntryKind = entry_kind_str.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid entry_kind: {:?}", e),
            )),
        )
    })?;

    let state: InboxState = state_str.parse().map_err(|e: AppError| {
        rusqlite::Error::FromSqlConversionFailure(
            11,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        )
    })?;

    Ok(InboxRecord {
        vault_id,
        provider_id,
        page_cursor,
        remote_position,
        remote_seq,
        operation_id,
        doc_hash,
        entry_kind,
        encrypted_payload,
        payload_hash,
        source_device,
        state,
        last_error,
        received_at,
        updated_at,
        applied_at,
    })
}
pub const MAX_INBOX_APPLY_BATCH: usize = 1000;

fn validate_inbox_transition(expected: InboxState, new_state: InboxState) -> bool {
    match (expected, new_state) {
        (InboxState::Pending, InboxState::Applying) => true,
        (InboxState::PendingAsset, InboxState::Applying) => true,
        (InboxState::Failed, InboxState::Applying) => true,
        (InboxState::Applying, InboxState::Pending) => true,
        (InboxState::Applying, InboxState::PendingAsset) => true,
        (InboxState::Applying, InboxState::Applied) => true,
        (InboxState::Applying, InboxState::IgnoredOwnOperation) => true,
        (InboxState::Applying, InboxState::Failed) => true,
        (InboxState::Applying, InboxState::Quarantined) => true,
        _ => false,
    }
}

impl DbBridge {
    pub fn decode_inbox_page_bool(val: i64) -> Result<bool, rusqlite::Error> {
        match val {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "invalid sync_inbox_pages.has_more value {}; expected 0 or 1",
                        val
                    ),
                )),
            )),
        }
    }

    pub fn inbox_record_matches_staged_entry(
        dh: &[u8; 32],
        kind: &SyncEntryKind,
        payload: &Option<Vec<u8>>,
        ph: &Option<[u8; 32]>,
        sdev: &Option<String>,
        rseq: Option<u64>,
        remote_position: &str,
        op_id: &[u8; 16],
        entry: &InboxEntryToStage,
    ) -> bool {
        *dh == entry.doc_hash
            && *kind == entry.entry_kind
            && *payload == entry.encrypted_payload
            && *ph == entry.payload_hash
            && *sdev == entry.source_device
            && rseq == entry.remote_seq
            && remote_position == entry.remote_position
            && *op_id == entry.operation_id
    }

    pub fn stage_inbox_page(
        &self,
        vault_id: &str,
        provider_id: &str,
        start_cursor: &str,
        next_cursor: &str,
        has_more: bool,
        entries: &[InboxEntryToStage],
        received_at: i64,
    ) -> AppResult<InboxStageResult> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }
        if vault_id.len() > 128 || provider_id.len() > 64 {
            return Err(AppError::General(
                "vault_id or provider_id exceeds length bounds".into(),
            ));
        }
        if start_cursor.len() > 16384 || next_cursor.len() > 16384 {
            return Err(AppError::General(
                "start_cursor or next_cursor exceeds length bounds".into(),
            ));
        }
        if received_at < 0 {
            return Err(AppError::General("received_at cannot be negative".into()));
        }
        if next_cursor.trim().is_empty() {
            return Err(AppError::General("next_cursor cannot be empty".into()));
        }
        if next_cursor == start_cursor {
            return Err(AppError::General(
                "Advancing page requires next_cursor != start_cursor".into(),
            ));
        }
        if entries.len() > MAX_INBOX_STAGE_ENTRIES {
            return Err(AppError::General(format!(
                "Page entries count {} exceeds maximum allowed {}",
                entries.len(),
                MAX_INBOX_STAGE_ENTRIES
            )));
        }
        let entries_len_u64 = u64::try_from(entries.len())
            .map_err(|_| AppError::General("entries count exceeds u64::MAX".into()))?;

        let mut has_some_seq = false;
        let mut has_none_seq = false;
        let mut prev_seq: Option<u64> = None;
        let mut total_bytes: usize = 0;

        for entry in entries {
            if entry.remote_position.trim().is_empty() {
                return Err(AppError::General(
                    "entry.remote_position cannot be empty".into(),
                ));
            }
            match entry.remote_seq {
                Some(seq) => {
                    has_some_seq = true;
                    if i64::try_from(seq).is_err() {
                        return Err(AppError::General(format!(
                            "remote_seq {} exceeds i64::MAX",
                            seq
                        )));
                    }
                    if let Some(prev) = prev_seq {
                        if seq <= prev {
                            return Err(AppError::General(
                                "Non-monotonic or duplicate remote_seq in stage_inbox_page".into(),
                            ));
                        }
                    }
                    prev_seq = Some(seq);
                }
                None => {
                    has_none_seq = true;
                }
            }
            if let Some(ref payload) = entry.encrypted_payload {
                total_bytes = total_bytes.checked_add(payload.len()).ok_or_else(|| {
                    AppError::General("Payload size overflow during stage page validation".into())
                })?;
            }
        }

        if has_some_seq && has_none_seq {
            return Err(AppError::General(
                "Mixed remote_seq in stage_inbox_page: entries must be all Some or all None".into(),
            ));
        }

        if total_bytes > MAX_INBOX_STAGE_PAYLOAD_BYTES {
            return Err(AppError::General(format!(
                "Total payload size {} bytes exceeds maximum allowed {}",
                total_bytes, MAX_INBOX_STAGE_PAYLOAD_BYTES
            )));
        }

        let tx = self.conn.unchecked_transaction().map_err(|e| {
            AppError::General(format!("Failed to start inbox stage transaction: {}", e))
        })?;

        let existing_page_opt: Option<(String, i64, i64)> = tx
            .query_row(
                "SELECT next_cursor, has_more, entry_count FROM sync_inbox_pages WHERE vault_id = ?1 AND provider_id = ?2 AND start_cursor = ?3",
                params![vault_id, provider_id, start_cursor],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| AppError::General(format!("DB error reading sync_inbox_pages: {}", e)))?;

        if let Some((ex_next, ex_has_more_raw, ex_count_raw)) = existing_page_opt {
            let ex_has_more = Self::decode_inbox_page_bool(ex_has_more_raw).map_err(|e| {
                AppError::General(format!(
                    "DB error decoding sync_inbox_pages.has_more during replay: {}",
                    e
                ))
            })?;
            let ex_count = u64::try_from(ex_count_raw)
                .map_err(|_| AppError::General("Invalid entry_count in sync_inbox_pages".into()))?;

            if ex_next != next_cursor || ex_has_more != has_more || ex_count != entries_len_u64 {
                return Err(AppError::General(
                    "operation_id collision: page metadata mismatch on replay".into(),
                ));
            }

            let mut stmt = tx
                .prepare(
                    "SELECT page_ordinal, operation_id FROM sync_inbox_page_entries WHERE vault_id = ?1 AND provider_id = ?2 AND start_cursor = ?3 ORDER BY page_ordinal ASC",
                )
                .map_err(|e| AppError::General(format!("DB Error preparing page entries replay stmt: {}", e)))?;

            let ex_entries: Vec<(i64, Vec<u8>)> = stmt
                .query_map(params![vault_id, provider_id, start_cursor], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(|e| {
                    AppError::General(format!("DB Error querying page entries replay: {}", e))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    AppError::General(format!("DB Error decoding page entries replay: {}", e))
                })?;

            if ex_entries.len() != entries.len() {
                return Err(AppError::General(
                    "operation_id collision: page entries count mismatch on replay".into(),
                ));
            }

            for (idx, (ord_raw, op_bytes)) in ex_entries.into_iter().enumerate() {
                let ord_usize = usize::try_from(ord_raw)
                    .map_err(|_| AppError::General("Invalid ordinal".into()))?;
                if ord_usize != idx || op_bytes != entries[idx].operation_id {
                    return Err(AppError::General(
                        "operation_id collision: page entry ordinal/op_id mismatch on replay"
                            .into(),
                    ));
                }

                let mut inb_stmt = tx
                    .prepare(
                        "SELECT doc_hash, entry_kind, encrypted_payload, payload_hash, source_device, remote_seq, remote_position
                         FROM sync_inbox WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3",
                    )
                    .map_err(|e| AppError::General(format!("DB Error preparing inbox replay stmt: {}", e)))?;

                let (dh_b, kind_s, payload, ph_b, sdev, rseq_raw, remote_pos): (
                    Vec<u8>,
                    String,
                    Option<Vec<u8>>,
                    Option<Vec<u8>>,
                    Option<String>,
                    Option<i64>,
                    String,
                ) = inb_stmt
                    .query_row(
                        params![vault_id, provider_id, entries[idx].operation_id.as_slice()],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                                row.get(6)?,
                            ))
                        },
                    )
                    .map_err(|e| {
                        AppError::General(format!("DB Error reading inbox row for replay: {}", e))
                    })?;

                let dh: [u8; 32] = dh_b
                    .try_into()
                    .map_err(|_| AppError::General("doc_hash decode err".into()))?;
                let kind = SyncEntryKind::from_str(&kind_s)
                    .map_err(|_| AppError::General("kind decode err".into()))?;
                let ph: Option<[u8; 32]> = match ph_b {
                    Some(b) => Some(
                        b.try_into()
                            .map_err(|_| AppError::General("ph decode err".into()))?,
                    ),
                    None => None,
                };
                let rseq = match rseq_raw {
                    Some(s) => Some(
                        u64::try_from(s)
                            .map_err(|_| AppError::General("rseq decode err".into()))?,
                    ),
                    None => None,
                };

                if !Self::inbox_record_matches_staged_entry(
                    &dh,
                    &kind,
                    &payload,
                    &ph,
                    &sdev,
                    rseq,
                    &remote_pos,
                    &entries[idx].operation_id,
                    &entries[idx],
                ) {
                    return Err(AppError::General(
                        "operation_id collision with conflicting content on replay".into(),
                    ));
                }
            }

            return Ok(InboxStageResult {
                inserted_count: 0,
                duplicate_count: entries.len(),
            });
        }

        let has_more_int = if has_more { 1i64 } else { 0i64 };
        let entry_count_i64 = i64::try_from(entries.len())
            .map_err(|_| AppError::General("entries count overflow".into()))?;

        tx.execute(
            "INSERT INTO sync_inbox_pages (
                vault_id, provider_id, start_cursor, next_cursor, has_more, entry_count, state, received_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, 'staged', ?7, ?7
            )",
            params![vault_id, provider_id, start_cursor, next_cursor, has_more_int, entry_count_i64, received_at],
        ).map_err(|e| AppError::General(format!("DB Error inserting sync_inbox_pages: {}", e)))?;

        let mut inserted_count: usize = 0;
        let mut duplicate_count: usize = 0;

        for (ordinal, entry) in entries.iter().enumerate() {
            let remote_seq_i64 = match entry.remote_seq {
                Some(seq) => Some(i64::try_from(seq).map_err(|_| {
                    AppError::General(format!("remote_seq {} exceeds i64::MAX", seq))
                })?),
                None => None,
            };

            let rows_affected = tx
                .execute(
                    "INSERT INTO sync_inbox (
                        vault_id, provider_id, page_cursor, remote_position, remote_seq,
                        operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash,
                        source_device, state, last_error, received_at, updated_at, applied_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5,
                        ?6, ?7, ?8, ?9, ?10,
                        ?11, 'pending', NULL, ?12, ?12, NULL
                    )
                    ON CONFLICT(vault_id, provider_id, operation_id) DO NOTHING",
                    params![
                        vault_id,
                        provider_id,
                        start_cursor,
                        entry.remote_position,
                        remote_seq_i64,
                        entry.operation_id.as_slice(),
                        entry.doc_hash.as_slice(),
                        entry.entry_kind.to_string(),
                        entry.encrypted_payload,
                        entry.payload_hash.map(|h| h.to_vec()),
                        entry.source_device,
                        received_at,
                    ],
                )
                .map_err(|e| AppError::General(format!("DB Error inserting inbox entry: {}", e)))?;

            if rows_affected == 1 {
                inserted_count = inserted_count
                    .checked_add(1)
                    .ok_or_else(|| AppError::General("Counter overflow".into()))?;
            } else {
                let mut stmt = tx
                    .prepare(
                        "SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq,
                                operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash,
                                source_device, state, last_error, received_at, updated_at, applied_at
                         FROM sync_inbox
                         WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3",
                    )
                    .map_err(|e| AppError::General(format!("DB Error preparing conflict check stmt: {}", e)))?;

                let existing = stmt
                    .query_row(
                        params![vault_id, provider_id, entry.operation_id.as_slice()],
                        decode_inbox_row,
                    )
                    .map_err(|e| {
                        AppError::General(format!(
                            "DB Error decoding existing inbox row for conflict check: {}",
                            e
                        ))
                    })?;

                if existing.operation_id != entry.operation_id {
                    return Err(AppError::General(
                        "Decoded existing inbox row operation_id mismatch".into(),
                    ));
                }

                if Self::inbox_record_matches_staged_entry(
                    &existing.doc_hash,
                    &existing.entry_kind,
                    &existing.encrypted_payload,
                    &existing.payload_hash,
                    &existing.source_device,
                    existing.remote_seq,
                    &existing.remote_position,
                    &existing.operation_id,
                    entry,
                ) {
                    duplicate_count = duplicate_count
                        .checked_add(1)
                        .ok_or_else(|| AppError::General("Counter overflow".into()))?;
                } else {
                    return Err(AppError::General(format!(
                        "Inbox operation_id collision with conflicting content for operation_id hex '{}'",
                        hex::encode(entry.operation_id)
                    )));
                }
            }

            let page_ordinal_i64 = i64::try_from(ordinal)
                .map_err(|_| AppError::General("page_ordinal overflow".into()))?;

            tx.execute(
                "INSERT INTO sync_inbox_page_entries (
                    vault_id, provider_id, start_cursor, page_ordinal, operation_id
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5
                )",
                params![
                    vault_id,
                    provider_id,
                    start_cursor,
                    page_ordinal_i64,
                    entry.operation_id.as_slice()
                ],
            )
            .map_err(|e| {
                AppError::General(format!("DB Error inserting sync_inbox_page_entries: {}", e))
            })?;
        }

        tx.commit().map_err(|e| {
            AppError::General(format!("Failed to commit inbox stage transaction: {}", e))
        })?;

        Ok(InboxStageResult {
            inserted_count,
            duplicate_count,
        })
    }

    pub fn get_inbox_page(
        &self,
        vault_id: &str,
        provider_id: &str,
        start_cursor: &str,
    ) -> AppResult<Option<InboxPageRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, provider_id, start_cursor, next_cursor, has_more, entry_count, state, received_at, updated_at
                 FROM sync_inbox_pages
                 WHERE vault_id = ?1 AND provider_id = ?2 AND start_cursor = ?3",
            )
            .map_err(|e| AppError::General(format!("DB Error preparing get_inbox_page stmt: {}", e)))?;

        stmt.query_row(params![vault_id, provider_id, start_cursor], |row| {
            let v_id: String = row.get(0)?;
            let p_id: String = row.get(1)?;
            let s_cur: String = row.get(2)?;
            let n_cur: String = row.get(3)?;
            let has_more_raw: i64 = row.get(4)?;
            let entry_count_raw: i64 = row.get(5)?;
            let state_str: String = row.get(6)?;
            let received_at: i64 = row.get(7)?;
            let updated_at: i64 = row.get(8)?;

            let has_more = Self::decode_inbox_page_bool(has_more_raw)?;

            let entry_count = u64::try_from(entry_count_raw).map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Integer,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "negative entry_count",
                    )),
                )
            })?;

            let state = InboxPageState::from_str(&state_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        e.to_string(),
                    )),
                )
            })?;

            Ok(InboxPageRecord {
                vault_id: v_id,
                provider_id: p_id,
                start_cursor: s_cur,
                next_cursor: n_cur,
                has_more,
                entry_count,
                state,
                received_at,
                updated_at,
            })
        })
        .optional()
        .map_err(|e| AppError::General(format!("DB Error reading inbox page: {}", e)))
    }

    pub fn get_inbox_page_entries(
        &self,
        vault_id: &str,
        provider_id: &str,
        start_cursor: &str,
        limit: usize,
    ) -> AppResult<Vec<(InboxPageEntryRecord, InboxRecord)>> {
        if limit == 0 || limit > MAX_INBOX_APPLY_BATCH {
            return Err(AppError::General(format!(
                "Invalid limit {}. Must be between 1 and {}",
                limit, MAX_INBOX_APPLY_BATCH
            )));
        }
        let effective_limit_i64 =
            i64::try_from(limit).map_err(|_| AppError::General("Limit exceeds i64::MAX".into()))?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT e.vault_id, e.provider_id, e.start_cursor, e.page_ordinal, e.operation_id,
                        i.vault_id, i.provider_id, i.page_cursor, i.remote_position, i.remote_seq,
                        i.operation_id, i.doc_hash, i.entry_kind, i.encrypted_payload, i.payload_hash,
                        i.source_device, i.state, i.last_error, i.received_at, i.updated_at, i.applied_at
                 FROM sync_inbox_page_entries e
                 JOIN sync_inbox i
                   ON e.vault_id = i.vault_id AND e.provider_id = i.provider_id AND e.operation_id = i.operation_id
                 WHERE e.vault_id = ?1 AND e.provider_id = ?2 AND e.start_cursor = ?3
                 ORDER BY e.page_ordinal ASC
                 LIMIT ?4",
            )
            .map_err(|e| AppError::General(format!("DB Error preparing get_inbox_page_entries stmt: {}", e)))?;

        let rows = stmt
            .query_map(
                params![vault_id, provider_id, start_cursor, effective_limit_i64],
                |row| {
                    let e_vault: String = row.get(0)?;
                    let e_prov: String = row.get(1)?;
                    let e_scur: String = row.get(2)?;
                    let e_ord_raw: i64 = row.get(3)?;
                    let e_op_bytes: Vec<u8> = row.get(4)?;

                    let e_ord = u64::try_from(e_ord_raw).map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Integer,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "negative page_ordinal",
                            )),
                        )
                    })?;

                    let e_op: [u8; 16] = e_op_bytes.try_into().map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Blob,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "invalid operation_id length",
                            )),
                        )
                    })?;

                    let entry_rec = InboxPageEntryRecord {
                        vault_id: e_vault,
                        provider_id: e_prov,
                        start_cursor: e_scur,
                        page_ordinal: e_ord,
                        operation_id: e_op,
                    };

                    let inb_vault: String = row.get(5)?;
                    let inb_prov: String = row.get(6)?;
                    let inb_pcur: String = row.get(7)?;
                    let inb_rpos: String = row.get(8)?;
                    let inb_rseq_raw: Option<i64> = row.get(9)?;
                    let inb_op_bytes: Vec<u8> = row.get(10)?;
                    let inb_dh_bytes: Vec<u8> = row.get(11)?;
                    let inb_kind_str: String = row.get(12)?;
                    let inb_payload: Option<Vec<u8>> = row.get(13)?;
                    let inb_ph_bytes: Option<Vec<u8>> = row.get(14)?;
                    let inb_sdev: Option<String> = row.get(15)?;
                    let inb_state_str: String = row.get(16)?;
                    let inb_lerr: Option<String> = row.get(17)?;
                    let inb_rec_at: i64 = row.get(18)?;
                    let inb_upd_at: i64 = row.get(19)?;
                    let inb_app_at: Option<i64> = row.get(20)?;

                    let inb_rseq = match inb_rseq_raw {
                        Some(s) => Some(u64::try_from(s).map_err(|_| {
                            rusqlite::Error::FromSqlConversionFailure(
                                9,
                                rusqlite::types::Type::Integer,
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "negative remote_seq",
                                )),
                            )
                        })?),
                        None => None,
                    };

                    let inb_op: [u8; 16] = inb_op_bytes.try_into().map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            10,
                            rusqlite::types::Type::Blob,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "invalid operation_id length",
                            )),
                        )
                    })?;

                    let inb_dh: [u8; 32] = inb_dh_bytes.try_into().map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            11,
                            rusqlite::types::Type::Blob,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "invalid doc_hash length",
                            )),
                        )
                    })?;

                    let inb_kind = SyncEntryKind::from_str(&inb_kind_str).map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            12,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "invalid entry_kind",
                            )),
                        )
                    })?;

                    let inb_ph: Option<[u8; 32]> = match inb_ph_bytes {
                        Some(b) => Some(b.try_into().map_err(|_| {
                            rusqlite::Error::FromSqlConversionFailure(
                                14,
                                rusqlite::types::Type::Blob,
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "invalid payload_hash length",
                                )),
                            )
                        })?),
                        None => None,
                    };

                    let inb_state = InboxState::from_str(&inb_state_str).map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            16,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "invalid state",
                            )),
                        )
                    })?;

                    let inb_rec = InboxRecord {
                        vault_id: inb_vault,
                        provider_id: inb_prov,
                        page_cursor: inb_pcur,
                        remote_position: inb_rpos,
                        remote_seq: inb_rseq,
                        operation_id: inb_op,
                        doc_hash: inb_dh,
                        entry_kind: inb_kind,
                        encrypted_payload: inb_payload,
                        payload_hash: inb_ph,
                        source_device: inb_sdev,
                        state: inb_state,
                        last_error: inb_lerr,
                        received_at: inb_rec_at,
                        updated_at: inb_upd_at,
                        applied_at: inb_app_at,
                    };

                    Ok((entry_rec, inb_rec))
                },
            )
            .map_err(|e| {
                AppError::General(format!("DB Error querying inbox page entries: {}", e))
            })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| {
                AppError::General(format!("DB Error decoding inbox page entry row: {}", e))
            })?);
        }
        Ok(result)
    }

    pub fn mark_inbox_page_applied_if_safe(
        &self,
        vault_id: &str,
        provider_id: &str,
        start_cursor: &str,
        now: i64,
    ) -> AppResult<()> {
        let tx = self.conn.unchecked_transaction().map_err(|e| {
            AppError::General(format!("Failed to start mark safe transaction: {}", e))
        })?;

        let page_opt: Option<(i64, String)> = tx
            .query_row(
                "SELECT entry_count, state FROM sync_inbox_pages WHERE vault_id = ?1 AND provider_id = ?2 AND start_cursor = ?3",
                params![vault_id, provider_id, start_cursor],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| AppError::General(format!("Failed to query inbox page for mark safe: {}", e)))?;

        let (entry_count_raw, state_str) = match page_opt {
            Some((ec, st)) => (ec, st),
            None => {
                return Err(AppError::General(
                    "Cannot mark page safe: page not found".into(),
                ))
            }
        };

        if state_str != "staged" {
            return Err(AppError::General(format!(
                "Cannot mark page safe: expected state 'staged', found '{}'",
                state_str
            )));
        }

        let expected_count = u64::try_from(entry_count_raw)
            .map_err(|_| AppError::General("Negative entry_count in sync_inbox_pages".into()))?;

        let members: Vec<(i64, Option<String>)> = tx
            .prepare(
                "SELECT e.page_ordinal, i.state
                 FROM sync_inbox_page_entries e
                 LEFT JOIN sync_inbox i
                   ON e.vault_id = i.vault_id AND e.provider_id = i.provider_id AND e.operation_id = i.operation_id
                 WHERE e.vault_id = ?1 AND e.provider_id = ?2 AND e.start_cursor = ?3
                 ORDER BY e.page_ordinal ASC",
            )
            .map_err(|e| AppError::General(format!("Failed to prepare member state query: {}", e)))?
            .query_map(params![vault_id, provider_id, start_cursor], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| AppError::General(format!("Failed to query member states: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::General(format!("Failed to decode page membership: {}", e)))?;

        let actual_count = u64::try_from(members.len())
            .map_err(|_| AppError::General("page membership count exceeds u64::MAX".into()))?;

        if actual_count != expected_count {
            return Err(AppError::General(format!(
                "Cannot mark page safe: Page member count mismatch: expected {}, found {}",
                expected_count, actual_count
            )));
        }

        for (expected_ordinal, (ordinal_raw, state_raw)) in members.into_iter().enumerate() {
            let ordinal = usize::try_from(ordinal_raw).map_err(|_| {
                AppError::General(format!(
                    "Cannot mark page safe: invalid negative page ordinal {}",
                    ordinal_raw
                ))
            })?;
            if ordinal != expected_ordinal {
                return Err(AppError::General(format!(
                    "Cannot mark page safe: non-contiguous page ordinal {}; expected {}",
                    ordinal, expected_ordinal
                )));
            }

            let state_text = state_raw.ok_or_else(|| {
                AppError::General(format!(
                    "Cannot mark page safe: membership ordinal {} has no durable inbox row",
                    ordinal
                ))
            })?;
            let state = InboxState::from_str(&state_text).map_err(|e| {
                AppError::General(format!(
                    "Cannot mark page safe: corrupt member state at ordinal {}: {}",
                    ordinal, e
                ))
            })?;

            match state {
                InboxState::Applied | InboxState::IgnoredOwnOperation => {}
                InboxState::Pending => {
                    return Err(AppError::General(
                        "Cannot mark page safe: member is pending".into(),
                    ))
                }
                InboxState::Applying => {
                    return Err(AppError::General(
                        "Cannot mark page safe: member is applying".into(),
                    ))
                }
                // Terminal, even though nothing was written. Holding the page
                // open for an entry we have decided not to apply is what stalled
                // a vault forever behind a single bad payload; the entry stays on
                // record and is reported, but it no longer gates the cursor.
                InboxState::PendingAsset | InboxState::Quarantined => {}
                InboxState::Failed => {
                    return Err(AppError::General(
                        "Cannot mark page safe: member is failed".into(),
                    ))
                }
            }
        }

        let rows_affected = tx
            .execute(
                "UPDATE sync_inbox_pages
                 SET state = 'applied', updated_at = ?1
                 WHERE vault_id = ?2 AND provider_id = ?3 AND start_cursor = ?4 AND state = 'staged'",
                params![now, vault_id, provider_id, start_cursor],
            )
            .map_err(|e| AppError::General(format!("Failed to update sync_inbox_pages state to applied: {}", e)))?;

        if rows_affected != 1 {
            return Err(AppError::General(
                "Concurrency conflict: failed to mark page applied".into(),
            ));
        }

        tx.commit().map_err(|e| {
            AppError::General(format!("Failed to commit mark safe transaction: {}", e))
        })?;

        Ok(())
    }

    pub fn commit_applied_inbox_page_cursor(
        &self,
        vault_id: &str,
        provider_id: &str,
        start_cursor: &str,
        now: i64,
    ) -> AppResult<()> {
        let tx = self.conn.unchecked_transaction().map_err(|e| {
            AppError::General(format!(
                "Failed to start page cursor commit transaction: {}",
                e
            ))
        })?;

        let page_opt: Option<(String, String)> = tx
            .query_row(
                "SELECT next_cursor, state FROM sync_inbox_pages WHERE vault_id = ?1 AND provider_id = ?2 AND start_cursor = ?3",
                params![vault_id, provider_id, start_cursor],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| AppError::General(format!("Failed to load page for cursor commit: {}", e)))?;

        let (next_cursor, state_str) = page_opt.ok_or_else(|| {
            AppError::General(format!(
                "Inbox page not found for cursor commit: start_cursor='{}'",
                start_cursor
            ))
        })?;

        if state_str != "applied" {
            return Err(AppError::General(format!(
                "Inbox page is in state '{}' (expected 'applied') for cursor commit",
                state_str
            )));
        }

        let rows_affected = tx
            .execute(
                "UPDATE sync_provider_state
                 SET cursor = ?1, updated_at = ?2
                 WHERE vault_id = ?3 AND provider_id = ?4 AND cursor = ?5",
                params![next_cursor, now, vault_id, provider_id, start_cursor],
            )
            .map_err(|e| AppError::General(format!("Failed to update provider cursor: {}", e)))?;

        if rows_affected != 1 {
            return Err(AppError::General(format!(
                "Failed to commit cursor: provider cursor CAS mismatch for start_cursor '{}'",
                start_cursor
            )));
        }

        let rows_affected = tx
            .execute(
                "UPDATE sync_inbox_pages
                 SET state = 'cursor_committed', updated_at = ?1
                 WHERE vault_id = ?2 AND provider_id = ?3 AND start_cursor = ?4 AND state = 'applied'",
                params![now, vault_id, provider_id, start_cursor],
            )
            .map_err(|e| AppError::General(format!("Failed to update inbox page state to cursor_committed: {}", e)))?;

        if rows_affected != 1 {
            return Err(AppError::General(format!(
                "Failed to commit cursor: inbox page state CAS mismatch for start_cursor '{}'",
                start_cursor
            )));
        }

        tx.commit().map_err(|e| {
            AppError::General(format!("Failed to commit page cursor transaction: {}", e))
        })?;

        Ok(())
    }

    pub fn get_inbox_by_id(
        &self,
        vault_id: &str,
        provider_id: &str,
        operation_id: &[u8; 16],
    ) -> AppResult<Option<InboxRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq,
                        operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash,
                        source_device, state, last_error, received_at, updated_at, applied_at
                 FROM sync_inbox
                 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3",
            )
            .map_err(|e| AppError::General(format!("DB Error preparing get_inbox_by_id: {}", e)))?;

        let record = stmt
            .query_row(
                params![vault_id, provider_id, operation_id.as_slice()],
                decode_inbox_row,
            )
            .optional()
            .map_err(|e| AppError::General(format!("DB Error executing get_inbox_by_id: {}", e)))?;

        Ok(record)
    }

    pub fn get_inbox_apply_candidates(
        &self,
        vault_id: &str,
        provider_id: &str,
        limit: usize,
    ) -> AppResult<Vec<InboxRecord>> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }
        if limit == 0 {
            return Err(AppError::General(
                "Apply candidates limit must be greater than 0".into(),
            ));
        }
        if limit > MAX_INBOX_APPLY_BATCH {
            return Err(AppError::General(format!(
                "Apply candidates limit {} exceeds maximum batch size {}",
                limit, MAX_INBOX_APPLY_BATCH
            )));
        }

        let limit_i64 = i64::try_from(limit).map_err(|_| {
            AppError::General(format!(
                "Apply candidates limit {} is out of bounds for i64",
                limit
            ))
        })?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq,
                        operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash,
                        source_device, state, last_error, received_at, updated_at, applied_at
                 FROM sync_inbox
                 WHERE vault_id = ?1 AND provider_id = ?2 AND state = 'pending'
                 ORDER BY remote_seq ASC, received_at ASC, page_cursor ASC, remote_position ASC, operation_id ASC
                 LIMIT ?3",
            )
            .map_err(|e| AppError::General(format!("DB Error preparing apply candidates query: {}", e)))?;

        let rows = stmt
            .query_map(params![vault_id, provider_id, limit_i64], decode_inbox_row)
            .map_err(|e| {
                AppError::General(format!("DB Error executing apply candidates query: {}", e))
            })?;

        let mut records = Vec::new();
        for row in rows {
            let record = row.map_err(|e| {
                AppError::General(format!(
                    "DB Error decoding inbox apply candidate record: {}",
                    e
                ))
            })?;
            records.push(record);
        }

        Ok(records)
    }

    /// Record one more failed apply attempt and return the new total.
    ///
    /// Kept separate from `transition_inbox_state` so the counter survives the
    /// Failed → Applying → Failed cycle that a retry goes through.
    pub fn increment_inbox_retry(
        &self,
        vault_id: &str,
        provider_id: &str,
        operation_id: &[u8; 16],
        now: i64,
    ) -> AppResult<u32> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| AppError::General(format!("DB tx error: {}", e)))?;

        let rows = tx
            .execute(
                "UPDATE sync_inbox SET retry_count = retry_count + 1, updated_at = ?4
                 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3",
                params![vault_id, provider_id, operation_id.as_slice(), now],
            )
            .map_err(|e| AppError::General(format!("DB Error bumping inbox retry: {}", e)))?;

        if rows != 1 {
            tx.rollback().ok();
            return Err(AppError::General(format!(
                "inbox entry {} not found while recording a retry",
                hex::encode(operation_id)
            )));
        }

        let count: i64 = tx
            .query_row(
                "SELECT retry_count FROM sync_inbox
                 WHERE vault_id = ?1 AND provider_id = ?2 AND operation_id = ?3",
                params![vault_id, provider_id, operation_id.as_slice()],
                |row| row.get(0),
            )
            .map_err(|e| AppError::General(format!("DB Error reading inbox retry: {}", e)))?;

        tx.commit()
            .map_err(|e| AppError::General(format!("DB commit error: {}", e)))?;

        Ok(count.max(0) as u32)
    }

    pub fn transition_inbox_state(
        &self,
        vault_id: &str,
        provider_id: &str,
        operation_id: &[u8; 16],
        expected_state: InboxState,
        new_state: InboxState,
        last_error: Option<&str>,
        now: i64,
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }
        if !validate_inbox_transition(expected_state, new_state) {
            return Err(AppError::General(format!(
                "Invalid inbox state transition from {:?} to {:?}",
                expected_state, new_state
            )));
        }

        let (applied_at, err_str) = match new_state {
            InboxState::Applied | InboxState::IgnoredOwnOperation => {
                if last_error.is_some() {
                    return Err(AppError::General(format!(
                        "last_error cannot be provided for transition to {:?}",
                        new_state
                    )));
                }
                (Some(now), None)
            }
            InboxState::Failed | InboxState::Quarantined => {
                let err = last_error.map(|s| s.trim()).unwrap_or("");
                if err.is_empty() {
                    return Err(AppError::General(format!(
                        "last_error must be non-empty for transition to {:?}",
                        new_state
                    )));
                }
                (None, Some(err.to_string()))
            }
            _ => {
                if last_error.is_some() {
                    return Err(AppError::General(format!(
                        "last_error cannot be provided for transition to {:?}",
                        new_state
                    )));
                }
                (None, None)
            }
        };

        let rows_affected = self
            .conn
            .execute(
                // Reaching a terminal state releases the payload. The row is
                // still needed — it records that this operation was handled, so
                // the same entry is not applied twice — but the bytes are not.
                // Retained, they left a full encrypted copy of every document
                // that ever arrived sitting in the database forever.
                "UPDATE sync_inbox
                 SET state = ?1,
                     last_error = ?2,
                     updated_at = ?3,
                     applied_at = ?4,
                     encrypted_payload = CASE
                         WHEN ?1 IN ('applied', 'ignored_own_operation') THEN NULL
                         ELSE encrypted_payload
                     END
                 WHERE vault_id = ?5
                   AND provider_id = ?6
                   AND operation_id = ?7
                   AND state = ?8",
                params![
                    new_state.as_str(),
                    err_str,
                    now,
                    applied_at,
                    vault_id,
                    provider_id,
                    operation_id.as_slice(),
                    expected_state.as_str(),
                ],
            )
            .map_err(|e| {
                AppError::General(format!("DB Error executing inbox state transition: {}", e))
            })?;

        if rows_affected == 0 {
            return Err(AppError::General(format!(
                "CAS transition failed: record not found in state {:?} for vault '{}', provider '{}', op hex '{}'",
                expected_state,
                vault_id,
                provider_id,
                hex::encode(operation_id)
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::schema::run_sync_schema_migrations;
    use rusqlite::types::Value;
    use rusqlite::Connection;

    type RawTable = Vec<Vec<Value>>;
    type C2aDurableSnapshot = (RawTable, RawTable, RawTable, RawTable);

    pub(crate) fn setup_test_db() -> DbBridge {
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

    fn snapshot_c2a_durable_scope_raw(db: &DbBridge, vault_id: &str) -> C2aDurableSnapshot {
        let fetch = |query: &str| -> Vec<Vec<rusqlite::types::Value>> {
            let mut stmt = db.conn.prepare(query).unwrap();
            let num_cols = stmt.column_count();
            let rows: Vec<_> = stmt
                .query_map([vault_id], |row| {
                    let mut vals = Vec::with_capacity(num_cols);
                    for i in 0..num_cols {
                        vals.push(row.get(i)?);
                    }
                    Ok(vals)
                })
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            rows
        };

        let providers = fetch("SELECT vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at FROM sync_provider_state WHERE vault_id = ?1 ORDER BY vault_id, provider_id");
        let pages = fetch("SELECT vault_id, provider_id, start_cursor, next_cursor, has_more, entry_count, state, received_at, updated_at FROM sync_inbox_pages WHERE vault_id = ?1 ORDER BY vault_id, provider_id, start_cursor");
        let entries = fetch("SELECT vault_id, provider_id, start_cursor, page_ordinal, operation_id FROM sync_inbox_page_entries WHERE vault_id = ?1 ORDER BY vault_id, provider_id, start_cursor, page_ordinal");
        let inbox = fetch("SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq, operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash, source_device, state, last_error, received_at, updated_at, applied_at FROM sync_inbox WHERE vault_id = ?1 ORDER BY vault_id, provider_id, page_cursor, operation_id");
        (providers, pages, entries, inbox)
    }

    fn raw_text(value: &str) -> Value {
        Value::Text(value.to_string())
    }

    fn raw_optional_text(value: Option<&str>) -> Value {
        value.map_or(Value::Null, raw_text)
    }

    fn raw_optional_integer(value: Option<i64>) -> Value {
        value.map_or(Value::Null, Value::Integer)
    }

    fn raw_optional_blob(value: Option<&[u8]>) -> Value {
        value.map_or(Value::Null, |bytes| Value::Blob(bytes.to_vec()))
    }

    fn expected_provider_row(
        vault_id: &str,
        provider_id: &str,
        cursor: &str,
        ack_cursor: Option<&str>,
        updated_at: i64,
    ) -> Vec<Value> {
        vec![
            raw_text(vault_id),
            raw_text(provider_id),
            raw_text(cursor),
            raw_optional_text(ack_cursor),
            raw_text("ready"),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Integer(100),
            Value::Integer(updated_at),
        ]
    }

    fn expected_page_row(
        vault_id: &str,
        provider_id: &str,
        start_cursor: &str,
        next_cursor: &str,
        has_more: bool,
        entry_count: i64,
        state: &str,
        received_at: i64,
        updated_at: i64,
    ) -> Vec<Value> {
        vec![
            raw_text(vault_id),
            raw_text(provider_id),
            raw_text(start_cursor),
            raw_text(next_cursor),
            Value::Integer(i64::from(has_more)),
            Value::Integer(entry_count),
            raw_text(state),
            Value::Integer(received_at),
            Value::Integer(updated_at),
        ]
    }

    fn expected_membership_row(
        vault_id: &str,
        provider_id: &str,
        start_cursor: &str,
        ordinal: i64,
        operation_id: &[u8; 16],
    ) -> Vec<Value> {
        vec![
            raw_text(vault_id),
            raw_text(provider_id),
            raw_text(start_cursor),
            Value::Integer(ordinal),
            Value::Blob(operation_id.to_vec()),
        ]
    }

    fn expected_inbox_row(
        vault_id: &str,
        provider_id: &str,
        page_cursor: &str,
        entry: &InboxEntryToStage,
        state: &str,
        last_error: Option<&str>,
        received_at: i64,
        updated_at: i64,
        applied_at: Option<i64>,
    ) -> Vec<Value> {
        let remote_seq = entry
            .remote_seq
            .map(|value| i64::try_from(value).expect("test remote_seq must fit i64"));
        vec![
            raw_text(vault_id),
            raw_text(provider_id),
            raw_text(page_cursor),
            raw_text(&entry.remote_position),
            raw_optional_integer(remote_seq),
            Value::Blob(entry.operation_id.to_vec()),
            Value::Blob(entry.doc_hash.to_vec()),
            raw_text(&entry.entry_kind.to_string()),
            raw_optional_blob(entry.encrypted_payload.as_deref()),
            raw_optional_blob(entry.payload_hash.as_ref().map(|hash| hash.as_slice())),
            raw_optional_text(entry.source_device.as_deref()),
            raw_text(state),
            raw_optional_text(last_error),
            Value::Integer(received_at),
            Value::Integer(updated_at),
            raw_optional_integer(applied_at),
        ]
    }

    fn snapshot_v5_provider_and_inbox_raw(
        conn: &Connection,
        vault_id: &str,
    ) -> (RawTable, RawTable) {
        let fetch = |query: &str| -> RawTable {
            let mut stmt = conn.prepare(query).unwrap();
            let column_count = stmt.column_count();
            stmt.query_map([vault_id], |row| {
                let mut values = Vec::with_capacity(column_count);
                for index in 0..column_count {
                    values.push(row.get(index)?);
                }
                Ok(values)
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect()
        };

        let providers = fetch("SELECT vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at FROM sync_provider_state WHERE vault_id = ?1 ORDER BY vault_id, provider_id");
        let inbox = fetch("SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq, operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash, source_device, state, last_error, received_at, updated_at, applied_at FROM sync_inbox WHERE vault_id = ?1 ORDER BY vault_id, provider_id, page_cursor, operation_id");
        (providers, inbox)
    }

    fn seed_complete_v5_provider_and_inbox(conn: &Connection, vault_id: &str) {
        conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES (?1, ?2, 100, 100)",
            params![vault_id, format!("/{}", vault_id)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_provider_state (
                vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id,
                remote_vault_id, last_error, created_at, updated_at
             ) VALUES (?1, 'gdrive', 'old_c', 'old_ack', 'error', ?2, ?3,
                       'legacy provider error', 100, 200)",
            params![vault_id, vec![3u8; 16], vec![9u8; 32]],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sync_inbox (
                vault_id, provider_id, page_cursor, remote_position, remote_seq,
                operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash,
                source_device, state, last_error, received_at, updated_at, applied_at
             ) VALUES (?1, 'gdrive', 'old_c', 'pos1', 7, ?2, ?3, 'upsert', ?4, ?5,
                       'legacy-device', 'failed', 'legacy inbox error', 1000, 1100, NULL)",
            params![
                vault_id,
                vec![1u8; 16],
                vec![2u8; 32],
                vec![5u8; 11],
                vec![4u8; 32]
            ],
        )
        .unwrap();
    }

    fn sample_entry(op_byte: u8, seq: u64) -> InboxEntryToStage {
        InboxEntryToStage {
            remote_position: format!("pos_{}", op_byte),
            remote_seq: Some(seq),
            operation_id: [op_byte; 16],
            doc_hash: [op_byte; 32],
            entry_kind: SyncEntryKind::Upsert,
            encrypted_payload: Some(vec![10, 20, op_byte]),
            payload_hash: Some([op_byte; 32]),
            source_device: Some(format!("device_{}", op_byte)),
        }
    }

    #[test]
    fn insert_and_read_round_trip() {
        let db = setup_test_db();
        let entry = sample_entry(1, 42);

        let res = db
            .stage_inbox_page(
                "v1",
                "gdrive",
                "cursor_1",
                "cursor_2",
                true,
                &[entry.clone()],
                1000,
            )
            .unwrap();

        assert_eq!(res.inserted_count, 1);
        assert_eq!(res.duplicate_count, 0);

        let fetched = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .expect("Inbox record must exist");

        assert_eq!(fetched.vault_id, "v1");
        assert_eq!(fetched.provider_id, "gdrive");
        assert_eq!(fetched.page_cursor, "cursor_1");
        assert_eq!(fetched.remote_position, "pos_1");
        assert_eq!(fetched.remote_seq, Some(42));
        assert_eq!(fetched.operation_id, [1; 16]);
        assert_eq!(fetched.doc_hash, [1; 32]);
        assert_eq!(fetched.entry_kind, SyncEntryKind::Upsert);
        assert_eq!(fetched.encrypted_payload, Some(vec![10, 20, 1]));
        assert_eq!(fetched.payload_hash, Some([1; 32]));
        assert_eq!(fetched.source_device, Some("device_1".to_string()));
        assert_eq!(fetched.state, InboxState::Pending);
        assert_eq!(fetched.last_error, None);
        assert_eq!(fetched.received_at, 1000);
        assert_eq!(fetched.updated_at, 1000);
        assert_eq!(fetched.applied_at, None);
    }

    #[test]
    fn transactional_page_staging_all_or_nothing() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);
        let e2 = sample_entry(2, 20);
        let e3 = sample_entry(3, 30);

        let res = db
            .stage_inbox_page(
                "v1",
                "gdrive",
                "cursor_batch",
                "cursor_batch_next",
                true,
                &[e1, e2, e3],
                1000,
            )
            .unwrap();

        assert_eq!(res.inserted_count, 3);
        assert_eq!(res.duplicate_count, 0);

        assert!(db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .is_some());
        assert!(db
            .get_inbox_by_id("v1", "gdrive", &[2; 16])
            .unwrap()
            .is_some());
        assert!(db
            .get_inbox_by_id("v1", "gdrive", &[3; 16])
            .unwrap()
            .is_some());
    }

    #[test]
    fn transactional_page_staging_rollback_on_failure() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);
        let mut e2 = sample_entry(2, 20);
        e2.remote_position = "".to_string(); // Invalid empty position causes failure
        let e3 = sample_entry(3, 30);

        let before = snapshot_c2a_durable_scope_raw(&db, "v1");

        let res = db.stage_inbox_page(
            "v1",
            "gdrive",
            "cursor_batch",
            "cursor_batch_next",
            true,
            &[e1, e2, e3],
            1000,
        );
        assert!(res.is_err());

        let after = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after, before);
    }

    #[test]
    fn idempotent_duplicate_page_staging() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        let res1 = db
            .stage_inbox_page(
                "v1",
                "gdrive",
                "cursor_1",
                "cursor_2",
                true,
                &[e1.clone()],
                1000,
            )
            .unwrap();
        assert_eq!(res1.inserted_count, 1);
        assert_eq!(res1.duplicate_count, 0);

        let res2 = db
            .stage_inbox_page(
                "v1",
                "gdrive",
                "cursor_1",
                "cursor_2",
                true,
                &[e1.clone()],
                2000,
            )
            .unwrap();
        assert_eq!(res2.inserted_count, 0);
        assert_eq!(res2.duplicate_count, 1);

        let fetched = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(fetched.received_at, 1000);
        assert_eq!(fetched.state, InboxState::Pending);
    }

    #[test]
    fn duplicate_applied_entry_not_reset_to_pending() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page(
            "v1",
            "gdrive",
            "cursor_1",
            "cursor_2",
            true,
            &[e1.clone()],
            1000,
        )
        .unwrap();

        // Directly set state = applied in DB
        db.conn
            .execute(
                "UPDATE sync_inbox SET state = 'applied', applied_at = 1500 WHERE vault_id = 'v1' AND provider_id = 'gdrive' AND operation_id = ?1",
                params![[1u8; 16]],
            )
            .unwrap();

        // Stage duplicate
        let res = db
            .stage_inbox_page(
                "v1",
                "gdrive",
                "cursor_1",
                "cursor_2",
                true,
                &[e1.clone()],
                2000,
            )
            .unwrap();
        assert_eq!(res.inserted_count, 0);
        assert_eq!(res.duplicate_count, 1);

        let fetched = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(fetched.state, InboxState::Applied);
        assert_eq!(fetched.applied_at, Some(1500));
    }

    #[test]
    fn duplicate_conflicting_content_causes_collision_error() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);
        let e2 = sample_entry(2, 20);

        db.stage_inbox_page(
            "v1",
            "gdrive",
            "cursor_1",
            "cursor_2",
            true,
            &[e1.clone()],
            1000,
        )
        .unwrap();

        let mut e1_conflicting = e1.clone();
        e1_conflicting.doc_hash = [99; 32];
        e1_conflicting.remote_seq = Some(30);

        // Batch containing e2 and conflicting e1
        let res = db.stage_inbox_page(
            "v1",
            "gdrive",
            "cursor_2",
            "cursor_3",
            true,
            &[e2.clone(), e1_conflicting],
            2000,
        );
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("collision"));

        // Entire page 2 rolled back: e2 must NOT exist in DB
        assert!(db
            .get_inbox_by_id("v1", "gdrive", &[2; 16])
            .unwrap()
            .is_none());
    }

    #[test]
    fn isolation_by_vault_and_provider() {
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

        let e1 = sample_entry(1, 10);

        // Same operation_id staged into (v1, gdrive), (v1, server), (v2, gdrive)
        db.stage_inbox_page("v1", "gdrive", "c1", "c1_next", true, &[e1.clone()], 1000)
            .unwrap();
        db.stage_inbox_page("v1", "server", "c2", "c2_next", true, &[e1.clone()], 1000)
            .unwrap();
        db.stage_inbox_page("v2", "gdrive", "c3", "c3_next", true, &[e1.clone()], 1000)
            .unwrap();

        assert!(db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .is_some());
        assert!(db
            .get_inbox_by_id("v1", "server", &[1; 16])
            .unwrap()
            .is_some());
        assert!(db
            .get_inbox_by_id("v2", "gdrive", &[1; 16])
            .unwrap()
            .is_some());
    }

    #[test]
    fn foreign_key_to_provider_state_enforced() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        let res = db.stage_inbox_page("nonexistent_vault", "gdrive", "c1", "c2", true, &[e1], 1000);
        assert!(res.is_err());
    }

    #[test]
    fn empty_page_staging_returns_zero_counts() {
        let db = setup_test_db();

        let res = db
            .stage_inbox_page(
                "v1",
                "gdrive",
                "cursor_empty",
                "cursor_empty_next",
                true,
                &[],
                1000,
            )
            .unwrap();
        assert_eq!(res.inserted_count, 0);
        assert_eq!(res.duplicate_count, 0);
    }

    #[test]
    fn stage_page_validations_reject_invalid_bounds() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        // Blank vault_id
        assert!(db
            .stage_inbox_page("", "gdrive", "c1", "c2", true, &[e1.clone()], 1000)
            .is_err());

        // Blank provider_id
        assert!(db
            .stage_inbox_page("v1", "", "c1", "c2", true, &[e1.clone()], 1000)
            .is_err());

        // Blank page_cursor for non-empty page
        assert!(db
            .stage_inbox_page("v1", "gdrive", "", "", true, &[e1.clone()], 1000)
            .is_err());

        // remote_seq > i64::MAX
        let mut e_seq_over = e1.clone();
        e_seq_over.remote_seq = Some(9_223_372_036_854_775_808u64);
        assert!(db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_seq_over], 1000)
            .is_err());

        // Empty remote_position
        let mut e_empty_pos = e1.clone();
        e_empty_pos.remote_position = "".to_string();
        assert!(db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_empty_pos], 1000)
            .is_err());

        // Over entry count limit
        let over_entries: Vec<_> = (0..=MAX_INBOX_STAGE_ENTRIES)
            .map(|i| {
                let seq_val: u64 = i.try_into().unwrap();
                let op_val: u8 = (i % 255).try_into().unwrap();
                sample_entry(op_val, seq_val)
            })
            .collect();
        assert!(db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &over_entries, 1000)
            .is_err());
    }

    #[test]
    fn corrupt_operation_id_length_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_inbox (
                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
                    entry_kind, state, received_at, updated_at
                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'upsert', 'pending', 100, 100)",
                params![vec![99u8; 10], vec![1u8; 32]],
            )
            .unwrap();

        let mut stmt = db
            .conn
            .prepare(
                "SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq,
                        operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash,
                        source_device, state, last_error, received_at, updated_at, applied_at
                 FROM sync_inbox",
            )
            .unwrap();
        let res = stmt.query_row([], decode_inbox_row);
        assert!(
            res.is_err(),
            "operation_id with 10 bytes must return Err when decoding"
        );
    }

    #[test]
    fn corrupt_doc_hash_length_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_inbox (
                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
                    entry_kind, state, received_at, updated_at
                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'upsert', 'pending', 100, 100)",
                params![vec![1u8; 16], vec![99u8; 10]],
            )
            .unwrap();

        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "doc_hash with 10 bytes must return Err when decoding"
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
                "INSERT INTO sync_inbox (
                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
                    payload_hash, entry_kind, state, received_at, updated_at
                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, ?3, 'upsert', 'pending', 100, 100)",
                params![vec![1u8; 16], vec![2u8; 32], vec![99u8; 10]],
            )
            .unwrap();

        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "payload_hash with 10 bytes must return Err when decoding"
        );
    }

    #[test]
    fn corrupt_negative_remote_seq_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_inbox (
                    vault_id, provider_id, page_cursor, remote_position, remote_seq, operation_id, doc_hash,
                    entry_kind, state, received_at, updated_at
                ) VALUES ('v1', 'gdrive', 'c1', 'p1', -5, ?1, ?2, 'upsert', 'pending', 100, 100)",
                params![vec![1u8; 16], vec![2u8; 32]],
            )
            .unwrap();

        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "negative remote_seq must return Err when decoding"
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
                "INSERT INTO sync_inbox (
                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
                    entry_kind, state, received_at, updated_at
                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'bogus_kind', 'pending', 100, 100)",
                params![vec![1u8; 16], vec![2u8; 32]],
            )
            .unwrap();

        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "invalid entry_kind string must return Err when decoding"
        );
    }

    #[test]
    fn corrupt_inbox_state_returns_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_inbox (
                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
                    entry_kind, state, received_at, updated_at
                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'upsert', 'bogus_state', 100, 100)",
                params![vec![1u8; 16], vec![2u8; 32]],
            )
            .unwrap();

        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
        assert!(
            res.is_err(),
            "invalid state string must return Err when decoding"
        );
    }

    #[test]
    fn duplicate_against_corrupt_existing_row_rolls_back_page() {
        let db = setup_test_db();
        let e_corrupt_op = sample_entry(2, 20);

        // Stage valid existing row first
        db.stage_inbox_page(
            "v1",
            "gdrive",
            "c1",
            "c1_next",
            true,
            &[e_corrupt_op.clone()],
            1000,
        )
        .unwrap();

        // Turn off check constraints and corrupt the state of existing row in DB
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        db.conn
            .execute(
                "UPDATE sync_inbox SET state = 'bogus_state' WHERE vault_id = 'v1' AND provider_id = 'gdrive' AND operation_id = ?1",
                params![[2u8; 16]],
            )
            .unwrap();

        // Stage a new page: new entry A (op 1) first, duplicate of corrupt existing row second
        let e1_new = sample_entry(1, 10);
        let res = db.stage_inbox_page(
            "v1",
            "gdrive",
            "c2",
            "c2_next",
            true,
            &[e1_new, e_corrupt_op],
            2000,
        );

        // Assert staging failed
        assert!(
            res.is_err(),
            "staging duplicate against corrupt existing row must fail"
        );

        // Assert entry A was rolled back and does NOT exist in DB
        assert!(
            db.get_inbox_by_id("v1", "gdrive", &[1; 16])
                .unwrap()
                .is_none(),
            "entry A must be rolled back when page staging fails due to corrupt existing row"
        );
    }

    #[test]
    fn stage_page_rejects_payload_over_byte_limit_before_write() {
        let db = setup_test_db();
        let mut e_over_payload = sample_entry(1, 10);
        e_over_payload.encrypted_payload = Some(vec![0u8; MAX_INBOX_STAGE_PAYLOAD_BYTES + 1]);

        let res = db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_over_payload], 1000);
        assert!(
            res.is_err(),
            "staging page with payload over byte limit must return Err"
        );

        // Assert no partial write occurred
        assert!(
            db.get_inbox_by_id("v1", "gdrive", &[1; 16])
                .unwrap()
                .is_none(),
            "operation_id must not exist in DB after over byte limit rejection"
        );
    }

    #[test]
    fn asset_reference_stages_and_reads_as_typed_kind() {
        let db = setup_test_db();
        let mut e_asset = sample_entry(1, 10);
        e_asset.entry_kind = SyncEntryKind::AssetReference;

        let res = db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_asset], 1000)
            .unwrap();
        assert_eq!(res.inserted_count, 1);
        assert_eq!(res.duplicate_count, 0);

        let fetched = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .expect("Inbox record must exist");

        assert_eq!(fetched.entry_kind, SyncEntryKind::AssetReference);
        assert_eq!(fetched.state, InboxState::Pending);
    }

    #[test]
    fn candidate_query_only_returns_pending() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);
        let e2 = sample_entry(2, 20);
        let e3 = sample_entry(3, 30);
        let e4 = sample_entry(4, 40);
        let e5 = sample_entry(5, 50);
        let e6 = sample_entry(6, 60);
        let e7 = sample_entry(7, 70);

        db.stage_inbox_page(
            "v1",
            "gdrive",
            "c1",
            "c2",
            true,
            &[
                e1.clone(),
                e2.clone(),
                e3.clone(),
                e4.clone(),
                e5.clone(),
                e6.clone(),
                e7.clone(),
            ],
            1000,
        )
        .unwrap();

        // e2 -> Applying
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[2; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        // e3 -> PendingAsset
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[3; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[3; 16],
            InboxState::Applying,
            InboxState::PendingAsset,
            None,
            1200,
        )
        .unwrap();

        // e4 -> Applied
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[4; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[4; 16],
            InboxState::Applying,
            InboxState::Applied,
            None,
            1200,
        )
        .unwrap();

        // e5 -> IgnoredOwnOperation
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[5; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[5; 16],
            InboxState::Applying,
            InboxState::IgnoredOwnOperation,
            None,
            1200,
        )
        .unwrap();

        // e6 -> Failed
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[6; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[6; 16],
            InboxState::Applying,
            InboxState::Failed,
            Some("error msg"),
            1200,
        )
        .unwrap();

        // e7 -> Quarantined
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[7; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[7; 16],
            InboxState::Applying,
            InboxState::Quarantined,
            Some("quarantine msg"),
            1200,
        )
        .unwrap();

        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].operation_id, [1; 16]);
        assert_eq!(candidates[0].state, InboxState::Pending);

        let non_pending_op_ids: Vec<[u8; 16]> =
            vec![[2; 16], [3; 16], [4; 16], [5; 16], [6; 16], [7; 16]];
        for op_id in non_pending_op_ids {
            assert!(
                !candidates.iter().any(|c| c.operation_id == op_id),
                "Candidates list must not contain non-pending operation_id hex '{}'",
                hex::encode(op_id)
            );
        }
    }

    #[test]
    fn candidate_query_and_cas_vault_and_provider_isolation() {
        let db = setup_test_db();
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'server', 100, 100)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v2', 'gdrive', 200, 200)",
                [],
            )
            .unwrap();

        let mut e_v1 = sample_entry(1, 10);
        let mut e_v2 = sample_entry(1, 10); // SAME operation_id [1; 16] in both vaults!
        e_v1.source_device = Some("device_v1".to_string());
        e_v2.source_device = Some("device_v2".to_string());

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_v1.clone()], 1000)
            .unwrap();
        db.stage_inbox_page("v2", "gdrive", "c2", "c3", true, &[e_v2.clone()], 2000)
            .unwrap();

        // Candidates for v1/gdrive
        let cand_v1 = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
        assert_eq!(cand_v1.len(), 1);
        assert_eq!(cand_v1[0].vault_id, "v1");
        assert_eq!(cand_v1[0].provider_id, "gdrive");
        assert_eq!(cand_v1[0].source_device, Some("device_v1".to_string()));

        // Candidates for v2/gdrive
        let cand_v2 = db.get_inbox_apply_candidates("v2", "gdrive", 10).unwrap();
        assert_eq!(cand_v2.len(), 1);
        assert_eq!(cand_v2[0].vault_id, "v2");
        assert_eq!(cand_v2[0].provider_id, "gdrive");
        assert_eq!(cand_v2[0].source_device, Some("device_v2".to_string()));

        // CAS transition v1/gdrive record to Applying
        let v2_before = db
            .get_inbox_by_id("v2", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1500,
        )
        .unwrap();

        // Verify v1 changed
        let v1_after = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(v1_after.state, InboxState::Applying);
        assert_eq!(v1_after.updated_at, 1500);

        // Verify v2 remains UNCHANGED
        let v2_after = db
            .get_inbox_by_id("v2", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(v2_after, v2_before);

        // CAS with wrong vault/provider returns Err
        assert!(db
            .transition_inbox_state(
                "v1",
                "non_existent_provider",
                &[1; 16],
                InboxState::Pending,
                InboxState::Applying,
                None,
                1600,
            )
            .is_err());
        assert!(db
            .transition_inbox_state(
                "non_existent_vault",
                "gdrive",
                &[1; 16],
                InboxState::Pending,
                InboxState::Applying,
                None,
                1600,
            )
            .is_err());
    }

    #[test]
    fn candidate_query_null_seq_ordering_regression() {
        let db = setup_test_db();
        let mut e_z = sample_entry(26, 0); // byte 26 => op_id [26; 16]
        let mut e_a = sample_entry(1, 0); // byte 1 => op_id [1; 16]

        e_z.remote_seq = None;
        e_a.remote_seq = None;

        // Equal received_at and page_cursor and remote_position
        e_z.remote_position = "pos_same".to_string();
        e_a.remote_position = "pos_same".to_string();

        // Stage in REVERSE order (e_z first, e_a second)
        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_z, e_a], 1000)
            .unwrap();

        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].remote_seq, None);
        assert_eq!(candidates[1].remote_seq, None);

        // Tie-breaker on operation_id ASC: [1; 16] before [26; 16]
        assert_eq!(candidates[0].operation_id, [1; 16]);
        assert_eq!(candidates[1].operation_id, [26; 16]);
    }

    #[test]
    fn candidate_query_bounded_by_limit() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);
        let e2 = sample_entry(2, 20);
        let e3 = sample_entry(3, 30);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1, e2, e3], 1000)
            .unwrap();

        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 2).unwrap();
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn candidate_query_sorts_numeric_seq_asc() {
        let db = setup_test_db();
        let e10 = sample_entry(1, 10);
        let e2 = sample_entry(2, 2);
        let e1 = sample_entry(3, 1);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e10], 1000)
            .unwrap();
        db.stage_inbox_page("v1", "gdrive", "c2", "c3", true, &[e2], 1000)
            .unwrap();
        db.stage_inbox_page("v1", "gdrive", "c3", "c4", true, &[e1], 1000)
            .unwrap();

        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].remote_seq, Some(1));
        assert_eq!(candidates[1].remote_seq, Some(2));
        assert_eq!(candidates[2].remote_seq, Some(10));
    }

    #[test]
    fn candidate_query_deterministic_tie_breaker() {
        let db = setup_test_db();
        let mut e1 = sample_entry(1, 5);
        let mut e2 = sample_entry(2, 5);
        e1.remote_seq = None;
        e2.remote_seq = None;
        e1.remote_position = "pos_a".to_string();
        e2.remote_position = "pos_b".to_string();

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1, e2], 1000)
            .unwrap();

        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].remote_position, "pos_a");
        assert_eq!(candidates[1].remote_position, "pos_b");
    }

    #[test]
    fn candidate_query_invalid_limits_reject() {
        let db = setup_test_db();

        assert!(db.get_inbox_apply_candidates("v1", "gdrive", 0).is_err());
        assert!(db
            .get_inbox_apply_candidates("v1", "gdrive", MAX_INBOX_APPLY_BATCH + 1)
            .is_err());
    }

    #[test]
    fn candidate_query_invalid_vault_provider_reject() {
        let db = setup_test_db();

        assert!(db.get_inbox_apply_candidates("", "gdrive", 10).is_err());
        assert!(db.get_inbox_apply_candidates("v1", "   ", 10).is_err());
    }

    #[test]
    fn candidate_query_corrupt_row_fails_entire_query() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_inbox (
                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
                    entry_kind, state, received_at, updated_at
                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'bogus_kind', 'pending', 100, 100)",
                params![vec![1u8; 16], vec![2u8; 32]],
            )
            .unwrap();

        let res = db.get_inbox_apply_candidates("v1", "gdrive", 10);
        assert!(res.is_err());
    }

    #[test]
    fn cas_transition_pending_to_applying_success() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();

        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        let record = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(record.state, InboxState::Applying);
        assert_eq!(record.updated_at, 1100);
        assert_eq!(record.last_error, None);
        assert_eq!(record.applied_at, None);
    }

    #[test]
    fn cas_transition_wrong_expected_state_fails() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();

        // Move to Applying
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        // Expect Pending again -> must fail because state is now Applying
        let res = db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1200,
        );
        assert!(res.is_err());

        // Row remains Applying
        let record = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(record.state, InboxState::Applying);
    }

    #[test]
    fn cas_transition_missing_operation_fails() {
        let db = setup_test_db();

        let res = db.transition_inbox_state(
            "v1",
            "gdrive",
            &[99; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1000,
        );
        assert!(res.is_err());
    }

    #[test]
    fn cas_transition_cross_vault_provider_fails() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();

        let res = db.transition_inbox_state(
            "v1",
            "other_provider",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        );
        assert!(res.is_err());
    }

    #[test]
    fn cas_transition_invalid_allowlist_transition_rejected() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();

        let before = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();

        // Pending -> Applied is NOT in allowlist (must go through Applying)
        let res = db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applied,
            None,
            1100,
        );
        assert!(res.is_err());

        let after = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(
            after, before,
            "Invalid allowlist transition must be rejected without mutating any record fields"
        );
    }

    #[test]
    fn cas_transition_same_state_rejected() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();

        let before = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();

        let res = db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Pending,
            None,
            1100,
        );
        assert!(res.is_err());

        let after = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(
            after, before,
            "Same state transition must be rejected without mutating any record fields"
        );
    }

    #[test]
    fn cas_transition_applying_to_applied_sets_applied_at_clears_error() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Applying,
            InboxState::Applied,
            None,
            1200,
        )
        .unwrap();

        let record = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(record.state, InboxState::Applied);
        assert_eq!(record.applied_at, Some(1200));
        assert_eq!(record.last_error, None);
        assert_eq!(record.updated_at, 1200);
    }

    #[test]
    fn cas_transition_applying_to_ignored_own_operation_sets_applied_at() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Applying,
            InboxState::IgnoredOwnOperation,
            None,
            1200,
        )
        .unwrap();

        let record = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(record.state, InboxState::IgnoredOwnOperation);
        assert_eq!(record.applied_at, Some(1200));
    }

    #[test]
    fn cas_transition_applying_to_failed_saves_error() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Applying,
            InboxState::Failed,
            Some("apply failed with syntax error"),
            1200,
        )
        .unwrap();

        let record = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(record.state, InboxState::Failed);
        assert_eq!(
            record.last_error,
            Some("apply failed with syntax error".to_string())
        );
        assert_eq!(record.applied_at, None);
    }

    #[test]
    fn cas_transition_applying_to_quarantined_saves_error() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Applying,
            InboxState::Quarantined,
            Some("corrupt payload quarantine"),
            1200,
        )
        .unwrap();

        let record = db
            .get_inbox_by_id("v1", "gdrive", &[1; 16])
            .unwrap()
            .unwrap();
        assert_eq!(record.state, InboxState::Quarantined);
        assert_eq!(
            record.last_error,
            Some("corrupt payload quarantine".to_string())
        );
    }

    #[test]
    fn cas_transition_failed_or_quarantined_with_empty_error_rejected() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1], 1000)
            .unwrap();
        db.transition_inbox_state(
            "v1",
            "gdrive",
            &[1; 16],
            InboxState::Pending,
            InboxState::Applying,
            None,
            1100,
        )
        .unwrap();

        // None error
        assert!(db
            .transition_inbox_state(
                "v1",
                "gdrive",
                &[1; 16],
                InboxState::Applying,
                InboxState::Failed,
                None,
                1200,
            )
            .is_err());

        // Empty error string
        assert!(db
            .transition_inbox_state(
                "v1",
                "gdrive",
                &[1; 16],
                InboxState::Applying,
                InboxState::Failed,
                Some("   "),
                1200,
            )
            .is_err());
    }

    #[test]
    fn legacy_v5_inbox_rows_survive_v6_page_ledger_migration_and_reopen() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("v5-upgrade.db");

        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE sync_schema_meta (
                 singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                 version INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );",
        )
        .unwrap();
        crate::db::schema::migrate_sync_schema_v1(&mut conn).unwrap();
        crate::db::schema::migrate_sync_schema_v2(&mut conn).unwrap();
        crate::db::schema::migrate_sync_schema_v3(&mut conn).unwrap();
        crate::db::schema::migrate_sync_schema_v4(&mut conn).unwrap();
        crate::db::schema::migrate_sync_schema_v5(&mut conn).unwrap();
        seed_complete_v5_provider_and_inbox(&conn, "v1");
        let before = snapshot_v5_provider_and_inbox_raw(&conn, "v1");
        drop(conn);

        let mut conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        crate::db::schema::migrate_sync_schema_v6(&mut conn).unwrap();
        let after_migration = snapshot_v5_provider_and_inbox_raw(&conn, "v1");
        assert_eq!(after_migration, before);
        let version: i64 = conn
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 6);
        drop(conn);

        let mut reopened = Connection::open(&db_path).unwrap();
        run_sync_schema_migrations(&mut reopened).unwrap();
        let after_reopen = snapshot_v5_provider_and_inbox_raw(&reopened, "v1");
        assert_eq!(after_reopen, before);
        drop(reopened);

        let failing_path = dir.path().join("v5-injected-failure.db");
        let mut failing = Connection::open(&failing_path).unwrap();
        failing
            .execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE sync_schema_meta (
                     singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
                     version INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL
                 );",
            )
            .unwrap();
        crate::db::schema::migrate_sync_schema_v1(&mut failing).unwrap();
        crate::db::schema::migrate_sync_schema_v2(&mut failing).unwrap();
        crate::db::schema::migrate_sync_schema_v3(&mut failing).unwrap();
        crate::db::schema::migrate_sync_schema_v4(&mut failing).unwrap();
        crate::db::schema::migrate_sync_schema_v5(&mut failing).unwrap();
        seed_complete_v5_provider_and_inbox(&failing, "v-fail");
        failing
            .execute_batch("CREATE TABLE sync_inbox_pages (incompatible TEXT NOT NULL);")
            .unwrap();
        let before_injected_failure = snapshot_v5_provider_and_inbox_raw(&failing, "v-fail");

        let injected = run_sync_schema_migrations(&mut failing);
        assert!(injected.is_err());
        let after_injected_failure = snapshot_v5_provider_and_inbox_raw(&failing, "v-fail");
        assert_eq!(after_injected_failure, before_injected_failure);
        let version_after_failure: i64 = failing
            .query_row(
                "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version_after_failure, 5);
        let membership_table_count: i64 = failing
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sync_inbox_page_entries'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(membership_table_count, 0);
    }

    #[test]
    fn stage_inbox_page_persists_page_entries_and_membership_atomically() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);

        let before = snapshot_c2a_durable_scope_raw(&db, "v1");

        db.conn.execute_batch(
            "CREATE TRIGGER fail_membership BEFORE INSERT ON sync_inbox_page_entries BEGIN SELECT RAISE(ABORT, 'injected_membership_error'); END;"
        ).unwrap();

        let res = db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1.clone()], 1000);
        assert!(res.is_err());

        let after_failure = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_failure, before);

        db.conn
            .execute_batch("DROP TRIGGER fail_membership;")
            .unwrap();

        let res2 = db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1.clone()], 1000);
        assert_eq!(
            res2.unwrap(),
            InboxStageResult {
                inserted_count: 1,
                duplicate_count: 0,
            }
        );

        let after_success = snapshot_c2a_durable_scope_raw(&db, "v1");
        let expected_success: C2aDurableSnapshot = (
            vec![expected_provider_row("v1", "gdrive", "", None, 100)],
            vec![expected_page_row(
                "v1", "gdrive", "c1", "c2", true, 1, "staged", 1000, 1000,
            )],
            vec![expected_membership_row(
                "v1",
                "gdrive",
                "c1",
                0,
                &e1.operation_id,
            )],
            vec![expected_inbox_row(
                "v1", "gdrive", "c1", &e1, "pending", None, 1000, 1000, None,
            )],
        );
        assert_eq!(after_success, expected_success);
    }

    #[test]
    fn stage_inbox_page_replay_is_exact_and_conflicts_roll_back() {
        let db = setup_test_db();
        let mut e1 = sample_entry(1, 10);
        let mut e2 = sample_entry(2, 20);
        e1.remote_seq = None;
        e2.remote_seq = None;
        let original_entries = vec![e1.clone(), e2.clone()];

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &original_entries, 1000)
            .unwrap();

        let before = snapshot_c2a_durable_scope_raw(&db, "v1");

        let res_replay = db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &original_entries, 1000)
            .unwrap();
        assert_eq!(res_replay.duplicate_count, 2);

        let after = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after, before);

        let test_conflict = |next_cursor: &str, has_more: bool, entries: &[InboxEntryToStage]| {
            let res =
                db.stage_inbox_page("v1", "gdrive", "c1", next_cursor, has_more, entries, 1000);
            assert!(res.is_err());
            let after_conflict = snapshot_c2a_durable_scope_raw(&db, "v1");
            assert_eq!(after_conflict, before);
        };

        test_conflict("c2_conflict", true, &original_entries);
        test_conflict("c2", false, &original_entries);

        let reversed_order = vec![e2.clone(), e1.clone()];
        test_conflict("c2", true, &reversed_order);

        let mut operation_id_conflict = original_entries.clone();
        operation_id_conflict[1].operation_id = [9; 16];
        test_conflict("c2", true, &operation_id_conflict);

        let mut remote_position_conflict = original_entries.clone();
        remote_position_conflict[0].remote_position = "pos_conflict".to_string();
        test_conflict("c2", true, &remote_position_conflict);

        let mut entry_kind_conflict = original_entries.clone();
        entry_kind_conflict[0].entry_kind = SyncEntryKind::Delete;
        test_conflict("c2", true, &entry_kind_conflict);

        let mut encrypted_payload_conflict = original_entries;
        encrypted_payload_conflict[0].encrypted_payload = Some(vec![99, 99]);
        test_conflict("c2", true, &encrypted_payload_conflict);

        let mut new_page_operation_collision = e1;
        new_page_operation_collision.remote_position = "different-cross-page-position".into();
        let collision = db.stage_inbox_page(
            "v1",
            "gdrive",
            "new-page",
            "new-next",
            true,
            &[new_page_operation_collision],
            1100,
        );
        assert!(collision.is_err());
        let after_new_page_collision = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_new_page_collision, before);
    }

    #[test]
    fn empty_page_high_watermark_is_durable() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let expected: C2aDurableSnapshot = (
            vec![expected_provider_row("v1", "gdrive", "", None, 100)],
            vec![expected_page_row(
                "v1", "gdrive", "c1", "c2", true, 0, "staged", 1000, 1000,
            )],
            Vec::new(),
            Vec::new(),
        );

        {
            let mut conn = Connection::open(&db_path).unwrap();
            crate::db::schema::run_sync_schema_migrations(&mut conn).unwrap();
            conn.execute("INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)", []).unwrap();
            conn.execute("INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'gdrive', 100, 100)", []).unwrap();

            let db = DbBridge { conn };
            db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[], 1000)
                .unwrap();

            let page = db.get_inbox_page("v1", "gdrive", "c1").unwrap().unwrap();
            assert_eq!(page.start_cursor, "c1");
            assert_eq!(page.next_cursor, "c2");
            assert_eq!(page.has_more, true);
            assert_eq!(page.entry_count, 0);
            assert_eq!(snapshot_c2a_durable_scope_raw(&db, "v1"), expected);

            drop(db);
        }

        let conn = Connection::open(&db_path).unwrap();
        let db = DbBridge { conn };
        let after_reopen = db.get_inbox_page("v1", "gdrive", "c1").unwrap().unwrap();

        assert_eq!(after_reopen.start_cursor, "c1");
        assert_eq!(after_reopen.next_cursor, "c2");
        assert_eq!(after_reopen.has_more, true);
        assert_eq!(after_reopen.entry_count, 0);
        assert_eq!(after_reopen.state, InboxPageState::Staged);
        assert_eq!(snapshot_c2a_durable_scope_raw(&db, "v1"), expected);
    }

    #[test]
    fn inbox_page_numeric_sequence_1_2_10_is_ordered_and_bounded() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 1);
        let e2 = sample_entry(2, 2);
        let e10 = sample_entry(10, 10);

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1, e2, e10], 1000)
            .unwrap();

        let entries = db
            .get_inbox_page_entries("v1", "gdrive", "c1", MAX_INBOX_APPLY_BATCH)
            .unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].1.remote_seq, Some(1));
        assert_eq!(entries[1].1.remote_seq, Some(2));
        assert_eq!(entries[2].1.remote_seq, Some(10));
    }

    #[test]
    fn inbox_page_rejects_mixed_or_non_monotonic_remote_sequence_without_mutation() {
        let db = setup_test_db();
        let before = snapshot_c2a_durable_scope_raw(&db, "v1");

        let e1 = sample_entry(1, 10);
        let mut e2 = sample_entry(2, 20);

        e2.remote_seq = None;
        let res1 = db.stage_inbox_page(
            "v1",
            "gdrive",
            "mixed",
            "c2",
            true,
            &[e1.clone(), e2.clone()],
            1000,
        );
        assert!(res1.is_err());
        let after_mixed = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_mixed, before);

        e2.remote_seq = Some(5);
        let res2 = db.stage_inbox_page(
            "v1",
            "gdrive",
            "non_monotonic",
            "c2",
            true,
            &[e1.clone(), e2.clone()],
            1000,
        );
        assert!(res2.is_err());
        let after_non_monotonic = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_non_monotonic, before);

        e2.remote_seq = Some(10);
        let res3 = db.stage_inbox_page(
            "v1",
            "gdrive",
            "c1",
            "c2",
            true,
            &[e1.clone(), e2.clone()],
            1000,
        );
        assert!(res3.is_err());

        let after = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after, before);
    }

    #[test]
    fn inbox_page_requires_all_members_safe_before_cursor_commit() {
        let assert_state_blocker = |blocking_state: InboxState| {
            let db = setup_test_db();
            let entry = sample_entry(1, 10);
            db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[entry], 1000)
                .unwrap();
            db.conn
                .execute(
                    "UPDATE sync_inbox SET state = ?1 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                    [blocking_state.as_str()],
                )
                .unwrap();
            let before = snapshot_c2a_durable_scope_raw(&db, "v1");
            let error = db
                .mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
                .unwrap_err();
            assert!(error.to_string().contains(blocking_state.as_str()));
            let after = snapshot_c2a_durable_scope_raw(&db, "v1");
            assert_eq!(after, before);
        };

        // Only states with work still outstanding may hold the page open.
        for blocking_state in [
            InboxState::Pending,
            InboxState::Applying,
            InboxState::Failed,
        ] {
            assert_state_blocker(blocking_state);
        }

        // PendingAsset and Quarantined are terminal decisions, not unfinished
        // work: the page must be able to commit past them, or one unusable
        // entry stalls the vault forever.
        for terminal_state in [InboxState::PendingAsset, InboxState::Quarantined] {
            let db = setup_test_db();
            let entry = sample_entry(1, 10);
            db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[entry], 1000)
                .unwrap();
            db.conn
                .execute(
                    "UPDATE sync_inbox SET state = ?1 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                    [terminal_state.as_str()],
                )
                .unwrap();
            db.mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
                .unwrap_or_else(|e| {
                    panic!("page must commit past a {} member: {e}", terminal_state.as_str())
                });
        }

        let missing_page_db = setup_test_db();
        let before_missing_page = snapshot_c2a_durable_scope_raw(&missing_page_db, "v1");
        let missing_page_error = missing_page_db
            .mark_inbox_page_applied_if_safe("v1", "gdrive", "missing", 1100)
            .unwrap_err();
        assert!(missing_page_error.to_string().contains("page not found"));
        let after_missing_page = snapshot_c2a_durable_scope_raw(&missing_page_db, "v1");
        assert_eq!(after_missing_page, before_missing_page);

        let wrong_state_db = setup_test_db();
        wrong_state_db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[], 1000)
            .unwrap();
        wrong_state_db
            .mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
            .unwrap();
        let before_wrong_state = snapshot_c2a_durable_scope_raw(&wrong_state_db, "v1");
        let wrong_state_error = wrong_state_db
            .mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1200)
            .unwrap_err();
        assert!(wrong_state_error
            .to_string()
            .contains("expected state 'staged'"));
        let after_wrong_state = snapshot_c2a_durable_scope_raw(&wrong_state_db, "v1");
        assert_eq!(after_wrong_state, before_wrong_state);

        let count_mismatch_db = setup_test_db();
        let entry = sample_entry(1, 10);
        count_mismatch_db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[entry], 1000)
            .unwrap();
        count_mismatch_db
            .conn
            .execute(
                "DELETE FROM sync_inbox_page_entries WHERE vault_id = 'v1' AND provider_id = 'gdrive' AND start_cursor = 'c1'",
                [],
            )
            .unwrap();
        let before_count_mismatch = snapshot_c2a_durable_scope_raw(&count_mismatch_db, "v1");
        let count_mismatch_error = count_mismatch_db
            .mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
            .unwrap_err();
        assert!(count_mismatch_error
            .to_string()
            .contains("member count mismatch"));
        let after_count_mismatch = snapshot_c2a_durable_scope_raw(&count_mismatch_db, "v1");
        assert_eq!(after_count_mismatch, before_count_mismatch);

        let missing_member_db = setup_test_db();
        let entry = sample_entry(1, 10);
        missing_member_db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[entry], 1000)
            .unwrap();
        missing_member_db
            .conn
            .execute_batch("PRAGMA foreign_keys=OFF;")
            .unwrap();
        missing_member_db
            .conn
            .execute(
                "DELETE FROM sync_inbox WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                [],
            )
            .unwrap();
        missing_member_db
            .conn
            .execute_batch("PRAGMA foreign_keys=ON;")
            .unwrap();
        let before_missing_member = snapshot_c2a_durable_scope_raw(&missing_member_db, "v1");
        let missing_member_error = missing_member_db
            .mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
            .unwrap_err();
        assert!(missing_member_error
            .to_string()
            .contains("no durable inbox row"));
        let after_missing_member = snapshot_c2a_durable_scope_raw(&missing_member_db, "v1");
        assert_eq!(after_missing_member, before_missing_member);

        let corrupt_state_db = setup_test_db();
        let entry = sample_entry(1, 10);
        corrupt_state_db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[entry], 1000)
            .unwrap();
        corrupt_state_db
            .conn
            .execute_batch(
                "PRAGMA ignore_check_constraints=ON;
                 UPDATE sync_inbox SET state = 'corrupt' WHERE vault_id = 'v1' AND provider_id = 'gdrive';
                 PRAGMA ignore_check_constraints=OFF;",
            )
            .unwrap();
        let before_corrupt_state = snapshot_c2a_durable_scope_raw(&corrupt_state_db, "v1");
        let corrupt_state_error = corrupt_state_db
            .mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
            .unwrap_err();
        assert!(corrupt_state_error
            .to_string()
            .contains("corrupt member state"));
        let after_corrupt_state = snapshot_c2a_durable_scope_raw(&corrupt_state_db, "v1");
        assert_eq!(after_corrupt_state, before_corrupt_state);

        let success_db = setup_test_db();
        let e1 = sample_entry(1, 10);
        let e2 = sample_entry(2, 20);
        success_db
            .stage_inbox_page(
                "v1",
                "gdrive",
                "c1",
                "c2",
                true,
                &[e1.clone(), e2.clone()],
                1000,
            )
            .unwrap();
        success_db
            .conn
            .execute(
                "UPDATE sync_inbox SET state = 'applied', updated_at = 1200, applied_at = 1200 WHERE operation_id = ?1",
                [e1.operation_id.as_slice()],
            )
            .unwrap();
        success_db
            .conn
            .execute(
                "UPDATE sync_inbox SET state = 'ignored_own_operation', updated_at = 1300, applied_at = 1300 WHERE operation_id = ?1",
                [e2.operation_id.as_slice()],
            )
            .unwrap();
        success_db
            .mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1400)
            .unwrap();

        let expected_success: C2aDurableSnapshot = (
            vec![expected_provider_row("v1", "gdrive", "", None, 100)],
            vec![expected_page_row(
                "v1", "gdrive", "c1", "c2", true, 2, "applied", 1000, 1400,
            )],
            vec![
                expected_membership_row("v1", "gdrive", "c1", 0, &e1.operation_id),
                expected_membership_row("v1", "gdrive", "c1", 1, &e2.operation_id),
            ],
            vec![
                expected_inbox_row(
                    "v1",
                    "gdrive",
                    "c1",
                    &e1,
                    "applied",
                    None,
                    1000,
                    1200,
                    Some(1200),
                ),
                expected_inbox_row(
                    "v1",
                    "gdrive",
                    "c1",
                    &e2,
                    "ignored_own_operation",
                    None,
                    1000,
                    1300,
                    Some(1300),
                ),
            ],
        );
        let after = snapshot_c2a_durable_scope_raw(&success_db, "v1");
        assert_eq!(after, expected_success);
    }

    #[test]
    fn inbox_page_cursor_commit_is_atomic_scoped_and_does_not_ack() {
        let db = setup_test_db();
        db.conn
            .execute(
                "UPDATE sync_provider_state SET cursor = 'c1', ack_cursor = 'ack-before' WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                [],
            )
            .unwrap();
        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[], 1000)
            .unwrap();
        db.mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
            .unwrap();

        let before = snapshot_c2a_durable_scope_raw(&db, "v1");

        db.conn
            .execute_batch(
                "CREATE TRIGGER fail_cursor_commit
             BEFORE UPDATE ON sync_inbox_pages
             WHEN NEW.state = 'cursor_committed'
             BEGIN SELECT RAISE(ABORT, 'injected_cursor_commit_error'); END;",
            )
            .unwrap();

        assert!(db
            .commit_applied_inbox_page_cursor("v1", "gdrive", "c1", 1300)
            .is_err());

        let after_failure = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_failure, before);

        db.conn
            .execute_batch("DROP TRIGGER fail_cursor_commit;")
            .unwrap();

        db.commit_applied_inbox_page_cursor("v1", "gdrive", "c1", 1300)
            .unwrap();

        let expected_success: C2aDurableSnapshot = (
            vec![expected_provider_row(
                "v1",
                "gdrive",
                "c2",
                Some("ack-before"),
                1300,
            )],
            vec![expected_page_row(
                "v1",
                "gdrive",
                "c1",
                "c2",
                true,
                0,
                "cursor_committed",
                1000,
                1300,
            )],
            Vec::new(),
            Vec::new(),
        );
        let after_success = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_success, expected_success);
        let ack_cursor: Option<String> = db
            .conn
            .query_row(
                "SELECT ack_cursor FROM sync_provider_state WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ack_cursor.as_deref(), Some("ack-before"));
    }

    #[test]
    fn inbox_page_ledger_isolated_by_vault_and_provider() {
        let db = setup_test_db();

        db.conn.execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 100, 100)",
            [],
        ).unwrap();
        db.conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, ack_cursor, created_at, updated_at) VALUES ('v2', 'gdrive', 'c1', 'ack-v2-gdrive', 100, 100)",
            [],
        ).unwrap();
        db.conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, ack_cursor, created_at, updated_at) VALUES ('v1', 'server', 'c1', 'ack-v1-server', 100, 100)",
            [],
        ).unwrap();
        db.conn.execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, ack_cursor, created_at, updated_at) VALUES ('v2', 'server', 'c1', 'ack-v2-server', 100, 100)",
            [],
        ).unwrap();
        db.conn
            .execute(
                "UPDATE sync_provider_state SET cursor = 'c1', ack_cursor = 'ack-v1-gdrive' WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                [],
            )
            .unwrap();

        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[], 1000)
            .unwrap();
        db.stage_inbox_page("v2", "gdrive", "c1", "c2", true, &[], 1000)
            .unwrap();
        db.stage_inbox_page("v1", "server", "c1", "c2", true, &[], 1000)
            .unwrap();
        db.stage_inbox_page("v2", "server", "c1", "c2", true, &[], 1000)
            .unwrap();
        db.mark_inbox_page_applied_if_safe("v1", "gdrive", "c1", 1100)
            .unwrap();

        let before = (
            snapshot_c2a_durable_scope_raw(&db, "v1"),
            snapshot_c2a_durable_scope_raw(&db, "v2"),
        );

        db.commit_applied_inbox_page_cursor("v1", "gdrive", "c1", 1300)
            .unwrap();

        let expected = (
            (
                vec![
                    expected_provider_row("v1", "gdrive", "c2", Some("ack-v1-gdrive"), 1300),
                    expected_provider_row("v1", "server", "c1", Some("ack-v1-server"), 100),
                ],
                vec![
                    expected_page_row(
                        "v1",
                        "gdrive",
                        "c1",
                        "c2",
                        true,
                        0,
                        "cursor_committed",
                        1000,
                        1300,
                    ),
                    expected_page_row("v1", "server", "c1", "c2", true, 0, "staged", 1000, 1000),
                ],
                Vec::new(),
                Vec::new(),
            ),
            (
                vec![
                    expected_provider_row("v2", "gdrive", "c1", Some("ack-v2-gdrive"), 100),
                    expected_provider_row("v2", "server", "c1", Some("ack-v2-server"), 100),
                ],
                vec![
                    expected_page_row("v2", "gdrive", "c1", "c2", true, 0, "staged", 1000, 1000),
                    expected_page_row("v2", "server", "c1", "c2", true, 0, "staged", 1000, 1000),
                ],
                Vec::new(),
                Vec::new(),
            ),
        );

        let after = (
            snapshot_c2a_durable_scope_raw(&db, "v1"),
            snapshot_c2a_durable_scope_raw(&db, "v2"),
        );
        assert_eq!(after, expected);
        assert_ne!(after, before);
    }

    #[test]
    fn stage_inbox_page_rejects_empty_next_cursor_and_corrupt_replay_metadata() {
        let db = setup_test_db();
        let e1 = sample_entry(1, 10);
        let before = snapshot_c2a_durable_scope_raw(&db, "v1");

        let res = db.stage_inbox_page("v1", "gdrive", "c1", "", true, &[e1], 1000);
        assert!(res.is_err());
        let after_empty_next_cursor = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_empty_next_cursor, before);

        let staged = sample_entry(1, 10);
        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[staged.clone()], 1000)
            .unwrap();
        db.conn
            .execute_batch(
                "PRAGMA ignore_check_constraints=ON;
                 UPDATE sync_inbox_pages SET has_more = 2 WHERE vault_id = 'v1' AND provider_id = 'gdrive' AND start_cursor = 'c1';
                 PRAGMA ignore_check_constraints=OFF;",
            )
            .unwrap();
        let before_corrupt_replay = snapshot_c2a_durable_scope_raw(&db, "v1");
        let replay_error = db
            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[staged], 1000)
            .unwrap_err();
        assert!(replay_error.to_string().contains("has_more"));
        let after_corrupt_replay = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after_corrupt_replay, before_corrupt_replay);

        let typed_read_error = db.get_inbox_page("v1", "gdrive", "c1").unwrap_err();
        assert!(typed_read_error.to_string().contains("has_more"));
        let after = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after, before_corrupt_replay);
    }

    #[test]
    fn get_inbox_page_entries_rejects_zero_and_oversized_limits() {
        let db = setup_test_db();
        let before = snapshot_c2a_durable_scope_raw(&db, "v1");

        let res1 = db.get_inbox_page_entries("v1", "gdrive", "c1", 0);
        assert!(res1.is_err());
        let res2 = db.get_inbox_page_entries("v1", "gdrive", "c1", MAX_INBOX_APPLY_BATCH + 1);
        assert!(res2.is_err());

        let after = snapshot_c2a_durable_scope_raw(&db, "v1");
        assert_eq!(after, before);
    }
}
