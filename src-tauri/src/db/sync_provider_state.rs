use std::fmt;
use std::str::FromStr;

use rusqlite::{params, OptionalExtension, Row};

use crate::db::DbBridge;
use crate::error::{AppError, AppResult};

pub const MAX_SYNC_CURSOR_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSyncState {
    Ready,
    BootstrapRequired,
    Bootstrapping,
    Error,
    Disabled,
}

impl ProviderSyncState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::BootstrapRequired => "bootstrap_required",
            Self::Bootstrapping => "bootstrapping",
            Self::Error => "error",
            Self::Disabled => "disabled",
        }
    }
}

impl fmt::Display for ProviderSyncState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ProviderSyncState {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ready" => Ok(Self::Ready),
            "bootstrap_required" => Ok(Self::BootstrapRequired),
            "bootstrapping" => Ok(Self::Bootstrapping),
            "error" => Ok(Self::Error),
            "disabled" => Ok(Self::Disabled),
            _ => Err(AppError::General(format!(
                "Unknown provider sync state: '{}'",
                s
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProviderStateRecord {
    pub vault_id: String,
    pub provider_id: String,
    pub cursor: String,
    pub ack_cursor: Option<String>,
    pub incarnation_id: Option<[u8; 16]>,
    pub remote_vault_id: Option<[u8; 32]>,
    pub sync_state: ProviderSyncState,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn decode_provider_state_row(row: &Row) -> Result<SyncProviderStateRecord, rusqlite::Error> {
    let vault_id: String = row.get(0)?;
    let provider_id: String = row.get(1)?;
    let cursor: String = row.get(2)?;
    let ack_cursor: Option<String> = row.get(3)?;
    let incarnation_id_bytes: Option<Vec<u8>> = row.get(4)?;
    let remote_vault_id_bytes: Option<Vec<u8>> = row.get(5)?;
    let sync_state_str: String = row.get(6)?;
    let last_error: Option<String> = row.get(7)?;
    let created_at: i64 = row.get(8)?;
    let updated_at: i64 = row.get(9)?;

    if cursor.len() > MAX_SYNC_CURSOR_BYTES {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "cursor length {} exceeds maximum allowed {} bytes",
                    cursor.len(),
                    MAX_SYNC_CURSOR_BYTES
                ),
            )),
        ));
    }

    if let Some(ref ack) = ack_cursor {
        if ack.len() > MAX_SYNC_CURSOR_BYTES {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "ack_cursor length {} exceeds maximum allowed {} bytes",
                        ack.len(),
                        MAX_SYNC_CURSOR_BYTES
                    ),
                )),
            ));
        }
    }

    let incarnation_id = match incarnation_id_bytes {
        Some(bytes) => {
            let arr: [u8; 16] = bytes.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Blob,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "incarnation_id length must be 16 bytes",
                    )),
                )
            })?;
            Some(arr)
        }
        None => None,
    };

    let remote_vault_id = match remote_vault_id_bytes {
        Some(bytes) => {
            let arr: [u8; 32] = bytes.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Blob,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "remote_vault_id length must be 32 bytes",
                    )),
                )
            })?;
            Some(arr)
        }
        None => None,
    };

    let sync_state: ProviderSyncState = sync_state_str.parse().map_err(|e: AppError| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        )
    })?;

    Ok(SyncProviderStateRecord {
        vault_id,
        provider_id,
        cursor,
        ack_cursor,
        incarnation_id,
        remote_vault_id,
        sync_state,
        last_error,
        created_at,
        updated_at,
    })
}

impl DbBridge {
    pub fn ensure_sync_provider_state(&self, vault_id: &str, provider_id: &str) -> AppResult<()> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }
        let now = chrono::Utc::now().timestamp_millis();
        self.conn
            .execute(
                "INSERT INTO sync_provider_state (
                    vault_id, provider_id, cursor, ack_cursor, incarnation_id,
                    remote_vault_id, sync_state, last_error, created_at, updated_at
                ) VALUES (?1, ?2, '', NULL, NULL, NULL, 'ready', NULL, ?3, ?3)
                ON CONFLICT(vault_id, provider_id) DO NOTHING",
                params![vault_id, provider_id, now],
            )
            .map_err(|e| AppError::General(format!("DB Error ensuring provider state: {}", e)))?;
        Ok(())
    }

    pub fn reconcile_sync_provider_plan(
        &mut self,
        vault_id: &str,
        provider_id: &str,
        plan_incarnation: Option<[u8; 16]>,
        plan_remote_vault_id: Option<[u8; 32]>,
        plan_requires_bootstrap: bool,
        now: i64,
    ) -> AppResult<ProviderSyncState> {
        let tx = self
            .conn_mut()
            .transaction()
            .map_err(|e| AppError::General(format!("DB error starting transaction: {}", e)))?;

        let row = tx.query_row(
            "SELECT incarnation_id, remote_vault_id, sync_state, last_error 
             FROM sync_provider_state 
             WHERE vault_id = ?1 AND provider_id = ?2",
            params![vault_id, provider_id],
            |row| {
                let inc_bytes: Option<Vec<u8>> = row.get(0)?;
                let rv_bytes: Option<Vec<u8>> = row.get(1)?;
                let state_str: String = row.get(2)?;
                let last_error: Option<String> = row.get(3)?;
                Ok((inc_bytes, rv_bytes, state_str, last_error))
            },
        );

        let (inc_bytes, rv_bytes, state_str, last_error) = match row {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.rollback();
                return Err(AppError::General(format!(
                    "Provider state missing or corrupt: {}",
                    e
                )));
            }
        };

        let stored_inc = match inc_bytes {
            Some(b) => match b.try_into() {
                Ok(arr) => Some(arr),
                Err(_) => {
                    let _ = tx.rollback();
                    return Err(AppError::General("corrupt incarnation".into()));
                }
            },
            None => None,
        };
        let stored_rv = match rv_bytes {
            Some(b) => match b.try_into() {
                Ok(arr) => Some(arr),
                Err(_) => {
                    let _ = tx.rollback();
                    return Err(AppError::General("corrupt remote vault id".into()));
                }
            },
            None => None,
        };

        let current_state: ProviderSyncState = match state_str.parse() {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.rollback();
                return Err(AppError::General(format!("corrupt state string: {}", e)));
            }
        };

        if current_state == ProviderSyncState::Disabled || current_state == ProviderSyncState::Error
        {
            let _ = tx.rollback();
            return Ok(current_state);
        }

        let is_mismatch = (stored_inc.is_some() && stored_inc != plan_incarnation)
            || (stored_rv.is_some() && stored_rv != plan_remote_vault_id);

        let needs_bootstrap = current_state == ProviderSyncState::BootstrapRequired
            || current_state == ProviderSyncState::Bootstrapping
            || plan_requires_bootstrap
            || is_mismatch;

        let new_state = if needs_bootstrap {
            ProviderSyncState::BootstrapRequired
        } else {
            ProviderSyncState::Ready
        };

        let final_error = if current_state == ProviderSyncState::BootstrapRequired
            || current_state == ProviderSyncState::Bootstrapping
        {
            if is_mismatch {
                Some("Identity mismatch detected".to_string())
            } else {
                last_error.clone()
            }
        } else if needs_bootstrap {
            if is_mismatch {
                Some("Identity mismatch detected".to_string())
            } else {
                Some("Bootstrap requested by plan".to_string())
            }
        } else {
            None
        };

        let exact_match = current_state == new_state
            && stored_inc == plan_incarnation
            && stored_rv == plan_remote_vault_id
            && last_error == final_error;

        if exact_match {
            tx.commit().map_err(|e| {
                AppError::General(format!("DB Error committing transaction: {}", e))
            })?;
            return Ok(new_state);
        }

        let rows_res = tx.execute(
            "UPDATE sync_provider_state 
             SET sync_state = ?1, incarnation_id = ?2, remote_vault_id = ?3, last_error = ?4, updated_at = ?5
             WHERE vault_id = ?6 AND provider_id = ?7",
            params![
                new_state.as_str(),
                plan_incarnation.map(|x| x.to_vec()),
                plan_remote_vault_id.map(|x| x.to_vec()),
                final_error,
                now,
                vault_id,
                provider_id
            ]
        );
        let rows = match rows_res {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.rollback();
                return Err(AppError::General(format!(
                    "DB Error reconciling provider state: {}",
                    e
                )));
            }
        };

        if rows == 0 {
            let _ = tx.rollback();
            return Err(AppError::General("Reconcile failed: missing record".into()));
        }

        tx.commit()
            .map_err(|e| AppError::General(format!("DB Error committing transaction: {}", e)))?;
        Ok(new_state)
    }

    pub fn get_sync_provider_state(
        &self,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<Option<SyncProviderStateRecord>> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, provider_id, cursor, ack_cursor, incarnation_id,
                        remote_vault_id, sync_state, last_error, created_at, updated_at
                 FROM sync_provider_state
                 WHERE vault_id = ?1 AND provider_id = ?2",
            )
            .map_err(|e| {
                AppError::General(format!("DB Error preparing get_sync_provider_state: {}", e))
            })?;

        let record = stmt
            .query_row(params![vault_id, provider_id], decode_provider_state_row)
            .optional()
            .map_err(|e| {
                AppError::General(format!("DB Error executing get_sync_provider_state: {}", e))
            })?;

        Ok(record)
    }

    pub fn advance_sync_provider_cursor_cas(
        &self,
        vault_id: &str,
        provider_id: &str,
        expected_cursor: &str,
        new_cursor: &str,
        now: i64,
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }
        if new_cursor.is_empty() {
            return Err(AppError::General("new_cursor cannot be empty".into()));
        }
        if new_cursor == expected_cursor {
            return Err(AppError::General(
                "new_cursor must be different from expected_cursor".into(),
            ));
        }
        if expected_cursor.len() > MAX_SYNC_CURSOR_BYTES {
            return Err(AppError::General(format!(
                "expected_cursor length {} exceeds maximum allowed {} bytes",
                expected_cursor.len(),
                MAX_SYNC_CURSOR_BYTES
            )));
        }
        if new_cursor.len() > MAX_SYNC_CURSOR_BYTES {
            return Err(AppError::General(format!(
                "new_cursor length {} exceeds maximum allowed {} bytes",
                new_cursor.len(),
                MAX_SYNC_CURSOR_BYTES
            )));
        }
        if now < 0 {
            return Err(AppError::General("now timestamp cannot be negative".into()));
        }

        let rows_affected = self
            .conn
            .execute(
                "UPDATE sync_provider_state
                 SET cursor = ?1,
                     updated_at = ?2
                 WHERE vault_id = ?3
                   AND provider_id = ?4
                   AND cursor = ?5",
                params![new_cursor, now, vault_id, provider_id, expected_cursor],
            )
            .map_err(|e| {
                AppError::General(format!("DB Error executing cursor CAS update: {}", e))
            })?;

        if rows_affected == 0 {
            return Err(AppError::General(format!(
                "Cursor CAS advance failed: record not found or cursor mismatch for vault '{}', provider '{}'",
                vault_id, provider_id
            )));
        }

        Ok(())
    }

    pub fn mark_sync_provider_cursor_acked_cas(
        &self,
        vault_id: &str,
        provider_id: &str,
        expected_ack_cursor: Option<&str>,
        cursor_to_ack: &str,
        now: i64,
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if provider_id.trim().is_empty() {
            return Err(AppError::General("provider_id cannot be empty".into()));
        }
        if cursor_to_ack.is_empty() {
            return Err(AppError::General("cursor_to_ack cannot be empty".into()));
        }
        if cursor_to_ack.len() > MAX_SYNC_CURSOR_BYTES {
            return Err(AppError::General(format!(
                "cursor_to_ack length {} exceeds maximum allowed {} bytes",
                cursor_to_ack.len(),
                MAX_SYNC_CURSOR_BYTES
            )));
        }

        if let Some(expected_ack) = expected_ack_cursor {
            if expected_ack.is_empty() {
                return Err(AppError::General(
                    "expected_ack_cursor cannot be empty string".into(),
                ));
            }
            if expected_ack.len() > MAX_SYNC_CURSOR_BYTES {
                return Err(AppError::General(format!(
                    "expected_ack_cursor length {} exceeds maximum allowed {} bytes",
                    expected_ack.len(),
                    MAX_SYNC_CURSOR_BYTES
                )));
            }
            if expected_ack == cursor_to_ack {
                return Err(AppError::General(
                    "expected_ack_cursor cannot equal cursor_to_ack".into(),
                ));
            }
        }

        if now < 0 {
            return Err(AppError::General("now timestamp cannot be negative".into()));
        }

        let rows_affected = self
            .conn
            .execute(
                "UPDATE sync_provider_state
                 SET ack_cursor = ?1,
                     updated_at = ?2
                 WHERE vault_id = ?3
                   AND provider_id = ?4
                   AND cursor = ?1
                   AND ack_cursor IS ?5",
                params![
                    cursor_to_ack,
                    now,
                    vault_id,
                    provider_id,
                    expected_ack_cursor,
                ],
            )
            .map_err(|e| {
                AppError::General(format!("DB Error executing ACK cursor CAS update: {}", e))
            })?;

        if rows_affected == 0 {
            return Err(AppError::General(format!(
                "ACK cursor CAS update failed: record not found, local cursor mismatch, or ACK cursor mismatch for vault '{}', provider '{}'",
                vault_id, provider_id
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::run_sync_schema_migrations;
    use rusqlite::Connection;

    fn setup_test_db() -> DbBridge {
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

    #[test]
    fn typed_read_round_trip_all_10_fields() {
        let db = setup_test_db();
        let inc_id = [1u8; 16];
        let rem_vault_id = [2u8; 32];

        db.conn
            .execute(
                "UPDATE sync_provider_state
                 SET cursor = 'cur_100',
                     ack_cursor = 'ack_50',
                     incarnation_id = ?1,
                     remote_vault_id = ?2,
                     sync_state = 'bootstrapping',
                     last_error = 'init error',
                     updated_at = 500
                 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                params![inc_id.as_slice(), rem_vault_id.as_slice()],
            )
            .unwrap();

        let record = db
            .get_sync_provider_state("v1", "gdrive")
            .unwrap()
            .expect("Provider state record must exist");

        assert_eq!(record.vault_id, "v1");
        assert_eq!(record.provider_id, "gdrive");
        assert_eq!(record.cursor, "cur_100");
        assert_eq!(record.ack_cursor, Some("ack_50".to_string()));
        assert_eq!(record.incarnation_id, Some(inc_id));
        assert_eq!(record.remote_vault_id, Some(rem_vault_id));
        assert_eq!(record.sync_state, ProviderSyncState::Bootstrapping);
        assert_eq!(record.last_error, Some("init error".to_string()));
        assert_eq!(record.created_at, 100);
        assert_eq!(record.updated_at, 500);
    }

    #[test]
    fn missing_record_returns_none() {
        let db = setup_test_db();

        let record = db
            .get_sync_provider_state("v1", "non_existent_provider")
            .unwrap();
        assert!(record.is_none());
    }

    #[test]
    fn two_vaults_same_provider_read_independently() {
        let db = setup_test_db();
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v2', 'gdrive', 'cur_v2', 200, 200)",
                [],
            )
            .unwrap();

        let rec_v1 = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        let rec_v2 = db.get_sync_provider_state("v2", "gdrive").unwrap().unwrap();

        assert_eq!(rec_v1.vault_id, "v1");
        assert_eq!(rec_v1.cursor, "");

        assert_eq!(rec_v2.vault_id, "v2");
        assert_eq!(rec_v2.cursor, "cur_v2");
    }

    #[test]
    fn two_providers_same_vault_read_independently() {
        let db = setup_test_db();
        db.conn
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v1', 'server', 'cur_server', 100, 100)",
                [],
            )
            .unwrap();

        let rec_gdrive = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        let rec_server = db.get_sync_provider_state("v1", "server").unwrap().unwrap();

        assert_eq!(rec_gdrive.provider_id, "gdrive");
        assert_eq!(rec_gdrive.cursor, "");

        assert_eq!(rec_server.provider_id, "server");
        assert_eq!(rec_server.cursor, "cur_server");
    }

    #[test]
    fn invalid_vault_or_provider_input_returns_err() {
        let db = setup_test_db();

        assert!(db.get_sync_provider_state("", "gdrive").is_err());
        assert!(db.get_sync_provider_state("v1", "   ").is_err());
    }

    #[test]
    fn invalid_sync_state_in_db_returns_err() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .ok();
        db.conn
            .execute(
                "UPDATE sync_provider_state SET sync_state = 'bogus_state' WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                [],
            )
            .unwrap();

        let res = db.get_sync_provider_state("v1", "gdrive");
        assert!(res.is_err());
    }

    #[test]
    fn invalid_incarnation_id_length_returns_err() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .ok();
        db.conn
            .execute(
                "UPDATE sync_provider_state SET incarnation_id = ?1 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                params![vec![1u8; 10]],
            )
            .unwrap();

        let res = db.get_sync_provider_state("v1", "gdrive");
        assert!(res.is_err());
    }

    #[test]
    fn invalid_remote_vault_id_length_returns_err() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        db.conn
            .execute(
                "UPDATE sync_provider_state SET remote_vault_id = ?1 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                params![vec![2u8; 10]],
            )
            .unwrap();

        let res = db.get_sync_provider_state("v1", "gdrive");
        assert!(res.is_err());
    }

    #[test]
    fn oversized_stored_cursor_and_ack_cursor_returns_err() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .ok();
        let big_str = "x".repeat(MAX_SYNC_CURSOR_BYTES + 1);

        // Oversized cursor
        db.conn
            .execute(
                "UPDATE sync_provider_state SET cursor = ?1 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                params![big_str],
            )
            .unwrap();
        assert!(db.get_sync_provider_state("v1", "gdrive").is_err());

        // Restore cursor, set oversized ack_cursor
        db.conn
            .execute(
                "UPDATE sync_provider_state SET cursor = '', ack_cursor = ?1 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                params!["y".repeat(MAX_SYNC_CURSOR_BYTES + 1)],
            )
            .unwrap();
        assert!(db.get_sync_provider_state("v1", "gdrive").is_err());
    }

    #[test]
    fn cas_from_empty_cursor_to_first_cursor_success() {
        let db = setup_test_db();

        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "token_100", 1500)
            .unwrap();

        let rec = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(rec.cursor, "token_100");
        assert_eq!(rec.updated_at, 1500);
    }

    #[test]
    fn cas_saves_opaque_cursor_verbatim() {
        let db = setup_test_db();
        let opaque = "cur_val:123/abc?raw=true#end \t\n json{\"key\":\"val\"}";

        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", opaque, 1500)
            .unwrap();

        let rec = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(rec.cursor, opaque);
    }

    #[test]
    fn cas_only_updates_cursor_and_updated_at_other_fields_unchanged() {
        let db = setup_test_db();
        let inc_id = [1u8; 16];
        let rem_vault_id = [2u8; 32];

        db.conn
            .execute(
                "UPDATE sync_provider_state
                 SET cursor = 'c1',
                     ack_cursor = 'ack1',
                     incarnation_id = ?1,
                     remote_vault_id = ?2,
                     sync_state = 'bootstrapping',
                     last_error = 'some error',
                     created_at = 100,
                     updated_at = 200
                 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                params![inc_id.as_slice(), rem_vault_id.as_slice()],
            )
            .unwrap();

        db.advance_sync_provider_cursor_cas("v1", "gdrive", "c1", "c2", 300)
            .unwrap();

        let rec = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(rec.cursor, "c2");
        assert_eq!(rec.updated_at, 300);

        // Other fields strictly UNCHANGED
        assert_eq!(rec.ack_cursor, Some("ack1".to_string()));
        assert_eq!(rec.incarnation_id, Some(inc_id));
        assert_eq!(rec.remote_vault_id, Some(rem_vault_id));
        assert_eq!(rec.sync_state, ProviderSyncState::Bootstrapping);
        assert_eq!(rec.last_error, Some("some error".to_string()));
        assert_eq!(rec.created_at, 100);
    }

    #[test]
    fn wrong_expected_cursor_returns_err_and_zero_mutation() {
        let db = setup_test_db();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let res = db.advance_sync_provider_cursor_cas("v1", "gdrive", "wrong_expected", "c2", 1100);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after.vault_id, before.vault_id);
        assert_eq!(after.provider_id, before.provider_id);
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.ack_cursor, before.ack_cursor);
        assert_eq!(after.incarnation_id, before.incarnation_id);
        assert_eq!(after.remote_vault_id, before.remote_vault_id);
        assert_eq!(after.sync_state, before.sync_state);
        assert_eq!(after.last_error, before.last_error);
    }

    #[test]
    fn wrong_vault_or_provider_returns_err_and_zero_mutation() {
        let db = setup_test_db();
        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let res1 = db.advance_sync_provider_cursor_cas("v1", "wrong_provider", "", "c1", 1000);
        assert!(res1.is_err());

        let res2 = db.advance_sync_provider_cursor_cas("wrong_vault", "gdrive", "", "c1", 1000);
        assert!(res2.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn empty_new_cursor_rejected_before_sql() {
        let db = setup_test_db();
        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let res = db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "", 1000);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(
            after, before,
            "Empty new cursor rejection must not mutate record"
        );
    }

    #[test]
    fn same_cursor_transition_rejected_before_sql() {
        let db = setup_test_db();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 500)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let res = db.advance_sync_provider_cursor_cas("v1", "gdrive", "c1", "c1", 1000);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(
            after, before,
            "Same-cursor transition rejection must not mutate record"
        );
    }

    #[test]
    fn oversized_expected_or_new_cursor_rejected_before_sql() {
        let db = setup_test_db();
        let big_str = "x".repeat(MAX_SYNC_CURSOR_BYTES + 1);

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        // 1. Oversized expected_cursor
        let res1 = db.advance_sync_provider_cursor_cas("v1", "gdrive", &big_str, "c1", 1000);
        assert!(res1.is_err());

        let after_1 = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(
            after_1, before,
            "Oversized expected_cursor rejection must not mutate record"
        );

        // 2. Oversized new_cursor
        let res2 = db.advance_sync_provider_cursor_cas("v1", "gdrive", "", &big_str, 1000);
        assert!(res2.is_err());

        let after_2 = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(
            after_2, before,
            "Oversized new_cursor rejection must not mutate record"
        );
    }

    #[test]
    fn negative_now_timestamp_rejected_before_sql() {
        let db = setup_test_db();
        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let res = db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", -1);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(
            after, before,
            "Negative timestamp rejection must not mutate record"
        );
    }

    #[test]
    fn cas_blank_vault_or_provider_rejected_with_zero_mutation() {
        let db = setup_test_db();
        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        assert!(db
            .advance_sync_provider_cursor_cas("", "gdrive", "", "c1", 1000)
            .is_err());
        assert!(db
            .advance_sync_provider_cursor_cas("   ", "gdrive", "", "c1", 1000)
            .is_err());
        assert!(db
            .advance_sync_provider_cursor_cas("v1", "", "", "c1", 1000)
            .is_err());
        assert!(db
            .advance_sync_provider_cursor_cas("v1", "   ", "", "c1", 1000)
            .is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(
            after, before,
            "Blank/whitespace vault or provider rejection must not mutate target record"
        );
    }

    #[test]
    fn initial_ack_success() {
        let db = setup_test_db();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();

        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", 1100)
            .unwrap();

        let rec = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(rec.cursor, "c1");
        assert_eq!(rec.ack_cursor, Some("c1".to_string()));
        assert_eq!(rec.updated_at, 1100);
    }

    #[test]
    fn ack_progression_success() {
        let db = setup_test_db();

        // Advance to c1, ACK c1
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();
        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", 1100)
            .unwrap();

        // Advance to c2, ACK c2
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "c1", "c2", 1200)
            .unwrap();
        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", Some("c1"), "c2", 1300)
            .unwrap();

        let rec = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(rec.cursor, "c2");
        assert_eq!(rec.ack_cursor, Some("c2".to_string()));
        assert_eq!(rec.updated_at, 1300);
    }

    #[test]
    fn opaque_ack_cursor_saved_verbatim() {
        let db = setup_test_db();
        let opaque = "cur_val:123/abc?raw=true#end \t\n json{\"key\":\"val\"}";

        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", opaque, 1000)
            .unwrap();
        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, opaque, 1100)
            .unwrap();

        let rec = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(rec.ack_cursor, Some(opaque.to_string()));
    }

    #[test]
    fn successful_ack_only_mutates_ack_cursor_and_updated_at() {
        let db = setup_test_db();
        let inc_id = [1u8; 16];
        let rem_vault_id = [2u8; 32];

        db.conn
            .execute(
                "UPDATE sync_provider_state
                 SET cursor = 'c1',
                     incarnation_id = ?1,
                     remote_vault_id = ?2,
                     sync_state = 'bootstrapping',
                     last_error = 'error test',
                     created_at = 100,
                     updated_at = 200
                 WHERE vault_id = 'v1' AND provider_id = 'gdrive'",
                params![inc_id.as_slice(), rem_vault_id.as_slice()],
            )
            .unwrap();

        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", 300)
            .unwrap();

        let rec = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(rec.cursor, "c1");
        assert_eq!(rec.ack_cursor, Some("c1".to_string()));
        assert_eq!(rec.updated_at, 300);

        // All other fields strictly UNCHANGED
        assert_eq!(rec.incarnation_id, Some(inc_id));
        assert_eq!(rec.remote_vault_id, Some(rem_vault_id));
        assert_eq!(rec.sync_state, ProviderSyncState::Bootstrapping);
        assert_eq!(rec.last_error, Some("error test".to_string()));
        assert_eq!(rec.created_at, 100);
    }

    #[test]
    fn expected_ack_none_rejected_when_ack_exists() {
        let db = setup_test_db();

        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();
        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", 1100)
            .unwrap();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "c1", "c2", 1200)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        // Attempting None when ACK is already "c1" must fail
        let res = db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c2", 1300);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn wrong_expected_ack_rejected() {
        let db = setup_test_db();

        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();
        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", 1100)
            .unwrap();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "c1", "c2", 1200)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let res =
            db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", Some("wrong_ack"), "c2", 1300);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn cursor_to_ack_different_from_local_committed_cursor_rejected() {
        let db = setup_test_db();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        // Attempting to ACK uncommitted "c2" while local cursor is "c1" must fail
        let res = db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c2", 1100);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn ack_old_cursor_after_advance_rejected() {
        let db = setup_test_db();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "c1", "c2", 1100)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        // Attempting to ACK old "c1" after local cursor advanced to "c2" must fail
        let res = db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", 1200);
        assert!(res.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn missing_provider_state_row_ack_rejected() {
        let db = setup_test_db();

        let res =
            db.mark_sync_provider_cursor_acked_cas("v1", "non_existent_provider", None, "c1", 1000);
        assert!(res.is_err());
    }

    #[test]
    fn wrong_vault_or_provider_ack_rejected() {
        let db = setup_test_db();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let res1 = db.mark_sync_provider_cursor_acked_cas("v1", "wrong_provider", None, "c1", 1100);
        assert!(res1.is_err());

        let res2 =
            db.mark_sync_provider_cursor_acked_cas("wrong_vault", "gdrive", None, "c1", 1100);
        assert!(res2.is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn two_vaults_same_provider_same_local_cursor_ack_isolation() {
        let db = setup_test_db();
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
                [],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, created_at, updated_at) VALUES ('v2', 'gdrive', 'c1', 200, 200)",
                [],
            )
            .unwrap();

        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();

        let v2_before = db.get_sync_provider_state("v2", "gdrive").unwrap().unwrap();

        db.mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", 1100)
            .unwrap();

        let v1_after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(v1_after.ack_cursor, Some("c1".to_string()));

        let v2_after = db.get_sync_provider_state("v2", "gdrive").unwrap().unwrap();
        assert_eq!(v2_after, v2_before);
    }

    #[test]
    fn ack_input_validations_rejected_with_zero_mutation() {
        let db = setup_test_db();
        db.advance_sync_provider_cursor_cas("v1", "gdrive", "", "c1", 1000)
            .unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let big_str = "x".repeat(MAX_SYNC_CURSOR_BYTES + 1);

        assert!(db
            .mark_sync_provider_cursor_acked_cas("", "gdrive", None, "c1", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("   ", "gdrive", None, "c1", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "", None, "c1", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "   ", None, "c1", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "gdrive", Some(""), "c1", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "gdrive", Some(&big_str), "c1", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, &big_str, 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "gdrive", Some("c1"), "c1", 1100)
            .is_err());
        assert!(db
            .mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, "c1", -1)
            .is_err());

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn provider_state_ensure_preserves_existing_bootstrap_and_all_fields() {
        let db = setup_test_db();
        db.conn.execute("UPDATE sync_provider_state SET cursor='c1', ack_cursor='a1', sync_state='bootstrap_required', incarnation_id=zeroblob(16), remote_vault_id=zeroblob(32), last_error='some error', created_at=10, updated_at=20 WHERE vault_id='v1' AND provider_id='gdrive'", []).unwrap();

        let before = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        db.ensure_sync_provider_state("v1", "gdrive").unwrap();
        db.ensure_sync_provider_state("v1", "gdrive").unwrap();

        let after = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after.vault_id, before.vault_id);
        assert_eq!(after.provider_id, before.provider_id);
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.ack_cursor, before.ack_cursor);
        assert_eq!(after.incarnation_id, before.incarnation_id);
        assert_eq!(after.remote_vault_id, before.remote_vault_id);
        assert_eq!(after.sync_state, before.sync_state);
        assert_eq!(after.last_error, before.last_error);
        assert_eq!(after, before);
    }

    #[test]
    fn two_vault_runtime_cursor_and_ack_are_isolated() {
        let db = setup_test_db();
        let vault_a = "v1";
        let vault_b = "v2";
        db.conn.execute("INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 100, 100)", []).unwrap();
        db.ensure_sync_provider_state(vault_a, "gdrive").unwrap();
        db.ensure_sync_provider_state(vault_b, "gdrive").unwrap();

        let before_a = db
            .get_sync_provider_state(vault_a, "gdrive")
            .unwrap()
            .unwrap();
        let before_b = db
            .get_sync_provider_state(vault_b, "gdrive")
            .unwrap()
            .unwrap();

        db.advance_sync_provider_cursor_cas(vault_a, "gdrive", "", "c1", 100)
            .unwrap();
        db.mark_sync_provider_cursor_acked_cas(vault_a, "gdrive", None, "c1", 100)
            .unwrap();

        let after_a = db
            .get_sync_provider_state(vault_a, "gdrive")
            .unwrap()
            .unwrap();
        let after_b = db
            .get_sync_provider_state(vault_b, "gdrive")
            .unwrap()
            .unwrap();

        assert_eq!(after_b, before_b);

        let mut expected_a = before_a.clone();
        expected_a.cursor = "c1".to_string();
        expected_a.ack_cursor = Some("c1".to_string());
        expected_a.updated_at = after_a.updated_at;

        assert_eq!(after_a.vault_id, expected_a.vault_id);
        assert_eq!(after_a.provider_id, expected_a.provider_id);
        assert_eq!(after_a.cursor, expected_a.cursor);
        assert_eq!(after_a.ack_cursor, expected_a.ack_cursor);
        assert_eq!(after_a.incarnation_id, expected_a.incarnation_id);
        assert_eq!(after_a.remote_vault_id, expected_a.remote_vault_id);
        assert_eq!(after_a.sync_state, expected_a.sync_state);
        assert_eq!(after_a.last_error, expected_a.last_error);
        assert_eq!(after_a, expected_a);
    }

    #[test]
    fn reconcile_bootstrap_reason_and_idempotence_are_durable() {
        let mut db = setup_test_db();

        db.conn_mut().execute("UPDATE sync_provider_state SET cursor='c1', ack_cursor='a1', incarnation_id=NULL, remote_vault_id=NULL, sync_state='ready', last_error=NULL, created_at=100, updated_at=100 WHERE vault_id='v1' AND provider_id='gdrive'", []).unwrap();
        let before_first = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let inc1 = [1u8; 16];
        let rv1 = [2u8; 32];

        let state1 = db
            .reconcile_sync_provider_plan("v1", "gdrive", Some(inc1), Some(rv1), false, 200)
            .unwrap();
        assert_eq!(state1, ProviderSyncState::Ready);
        let after_first = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();

        let mut expected_first = before_first.clone();
        expected_first.incarnation_id = Some(inc1);
        expected_first.remote_vault_id = Some(rv1);
        expected_first.updated_at = 200;
        assert_eq!(after_first, expected_first);
        assert_eq!(after_first.cursor, "c1".to_string());
        assert_eq!(after_first.ack_cursor, Some("a1".to_string()));

        let state2 = db
            .reconcile_sync_provider_plan("v1", "gdrive", Some(inc1), Some(rv1), false, 300)
            .unwrap();
        assert_eq!(state2, ProviderSyncState::Ready);
        let second = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(second, after_first);

        db.conn_mut().execute("UPDATE sync_provider_state SET sync_state='bootstrap_required', last_error='existing reason' WHERE vault_id='v1'", []).unwrap();
        let before_boot = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        let state3 = db
            .reconcile_sync_provider_plan("v1", "gdrive", Some(inc1), Some(rv1), false, 400)
            .unwrap();
        assert_eq!(state3, ProviderSyncState::BootstrapRequired);
        let after3 = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(after3, before_boot);

        db.conn_mut().execute("UPDATE sync_provider_state SET sync_state='ready', last_error=NULL WHERE vault_id='v1'", []).unwrap();
        let state4 = db
            .reconcile_sync_provider_plan("v1", "gdrive", Some(inc1), Some(rv1), true, 500)
            .unwrap();
        assert_eq!(state4, ProviderSyncState::BootstrapRequired);
        let after4 = db.get_sync_provider_state("v1", "gdrive").unwrap().unwrap();
        assert_eq!(
            after4.last_error.as_deref(),
            Some("Bootstrap requested by plan")
        );
    }

    #[test]
    fn incarnation_or_remote_vault_mismatch_requires_bootstrap_without_cursor_advance() {
        // Fixture 1: Incarnation mismatch
        {
            let mut incarnation_db = setup_test_db();
            let inc1 = [1u8; 16];
            let rv1 = [2u8; 32];

            // persist valid old incarnation + remote ID by real reconcile
            incarnation_db
                .reconcile_sync_provider_plan("v1", "gdrive", Some(inc1), Some(rv1), false, 100)
                .unwrap();

            // seed cursor/ACK by real CAS
            let cursor = "c1";
            let ack_cursor = "c1";
            incarnation_db
                .advance_sync_provider_cursor_cas("v1", "gdrive", "", cursor, 110)
                .unwrap();
            incarnation_db
                .mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, ack_cursor, 110)
                .unwrap();

            let incarnation_before = incarnation_db
                .get_sync_provider_state("v1", "gdrive")
                .unwrap()
                .unwrap();

            // reconcile with a different non-zero incarnation
            let inc2 = [3u8; 16];
            let state = incarnation_db
                .reconcile_sync_provider_plan("v1", "gdrive", Some(inc2), Some(rv1), false, 120)
                .unwrap();
            let bootstrap_required = ProviderSyncState::BootstrapRequired;
            assert_eq!(state, bootstrap_required);

            let incarnation_after = incarnation_db
                .get_sync_provider_state("v1", "gdrive")
                .unwrap()
                .unwrap();

            // Build incarnation_expected and assert
            let mut incarnation_expected = incarnation_before.clone();
            incarnation_expected.incarnation_id = Some(inc2);
            incarnation_expected.remote_vault_id = Some(rv1);
            incarnation_expected.sync_state = bootstrap_required;
            incarnation_expected.last_error = Some("Identity mismatch detected".to_string());
            incarnation_expected.updated_at = 120;

            assert_eq!(incarnation_after, incarnation_expected);
            assert_eq!(incarnation_after.cursor, cursor.to_string());
            assert_eq!(incarnation_after.ack_cursor, Some(ack_cursor.to_string()));
        }

        // Fixture 2: Remote vault mismatch
        {
            let mut remote_db = setup_test_db();
            let inc1 = [1u8; 16];
            let rv1 = [2u8; 32];

            // persist valid incarnation + old remote ID by real reconcile
            remote_db
                .reconcile_sync_provider_plan("v1", "gdrive", Some(inc1), Some(rv1), false, 200)
                .unwrap();

            // seed cursor/ACK by real CAS
            let cursor = "c2";
            let ack_cursor = "c2";
            remote_db
                .advance_sync_provider_cursor_cas("v1", "gdrive", "", cursor, 210)
                .unwrap();
            remote_db
                .mark_sync_provider_cursor_acked_cas("v1", "gdrive", None, ack_cursor, 210)
                .unwrap();

            let remote_before = remote_db
                .get_sync_provider_state("v1", "gdrive")
                .unwrap()
                .unwrap();

            // reconcile with a different non-zero remote ID
            let rv2 = [4u8; 32];
            let state = remote_db
                .reconcile_sync_provider_plan("v1", "gdrive", Some(inc1), Some(rv2), false, 220)
                .unwrap();
            let bootstrap_required = ProviderSyncState::BootstrapRequired;
            assert_eq!(state, bootstrap_required);

            let remote_after = remote_db
                .get_sync_provider_state("v1", "gdrive")
                .unwrap()
                .unwrap();

            // Build remote_expected and assert
            let mut remote_expected = remote_before.clone();
            remote_expected.incarnation_id = Some(inc1);
            remote_expected.remote_vault_id = Some(rv2);
            remote_expected.sync_state = bootstrap_required;
            remote_expected.last_error = Some("Identity mismatch detected".to_string());
            remote_expected.updated_at = 220;

            assert_eq!(remote_after, remote_expected);
            assert_eq!(remote_after.cursor, cursor.to_string());
            assert_eq!(remote_after.ack_cursor, Some(ack_cursor.to_string()));
        }
    }
}
