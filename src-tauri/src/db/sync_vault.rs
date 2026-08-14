use rusqlite::{params, OptionalExtension, Row};

use crate::db::DbBridge;
use crate::error::{AppError, AppResult};

pub const MAX_SYNC_VAULT_ID_BYTES: usize = 128;
pub const MAX_CANONICAL_ROOT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncVaultRecord {
    pub vault_id: String,
    pub canonical_root: String,
    pub metadata_version: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

fn decode_vault_row(row: &Row<'_>) -> rusqlite::Result<SyncVaultRecord> {
    let vault_id: String = row.get(0)?;
    let canonical_root: String = row.get(1)?;
    let raw_meta_ver: i64 = row.get(2)?;
    let created_at: i64 = row.get(3)?;
    let updated_at: i64 = row.get(4)?;

    if vault_id.trim().is_empty() {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "vault_id cannot be empty or whitespace",
            )),
        ));
    }
    if vault_id.len() > MAX_SYNC_VAULT_ID_BYTES {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "vault_id length {} exceeds maximum allowed {} bytes",
                    vault_id.len(),
                    MAX_SYNC_VAULT_ID_BYTES
                ),
            )),
        ));
    }

    if canonical_root.trim().is_empty() {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "canonical_root cannot be empty or whitespace",
            )),
        ));
    }
    if canonical_root.len() > MAX_CANONICAL_ROOT_BYTES {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "canonical_root length {} exceeds maximum allowed {} bytes",
                    canonical_root.len(),
                    MAX_CANONICAL_ROOT_BYTES
                ),
            )),
        ));
    }

    let metadata_version = u32::try_from(raw_meta_ver).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid metadata_version integer '{}': {}", raw_meta_ver, e),
            )),
        )
    })?;

    if metadata_version == 0 {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "metadata_version must be greater than zero",
            )),
        ));
    }

    if created_at < 0 {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("created_at timestamp '{}' cannot be negative", created_at),
            )),
        ));
    }
    if updated_at < created_at {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Integer,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "updated_at timestamp '{}' cannot be earlier than created_at timestamp '{}'",
                    updated_at, created_at
                ),
            )),
        ));
    }

    Ok(SyncVaultRecord {
        vault_id,
        canonical_root,
        metadata_version,
        created_at,
        updated_at,
    })
}

impl DbBridge {
    pub fn get_sync_vault_by_id(&self, vault_id: &str) -> AppResult<Option<SyncVaultRecord>> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id cannot be empty or whitespace".into(),
            ));
        }
        if vault_id.len() > MAX_SYNC_VAULT_ID_BYTES {
            return Err(AppError::General(format!(
                "vault_id length {} exceeds maximum allowed {} bytes",
                vault_id.len(),
                MAX_SYNC_VAULT_ID_BYTES
            )));
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, canonical_root, metadata_version, created_at, updated_at
                 FROM sync_vaults
                 WHERE vault_id = ?1",
            )
            .map_err(|e| {
                AppError::General(format!("DB Error preparing get_sync_vault_by_id: {}", e))
            })?;

        let record = stmt
            .query_row(params![vault_id], decode_vault_row)
            .optional()
            .map_err(|e| {
                AppError::General(format!("DB Error executing get_sync_vault_by_id: {}", e))
            })?;

        Ok(record)
    }

    pub fn get_sync_vault_by_canonical_root(
        &self,
        canonical_root: &str,
    ) -> AppResult<Option<SyncVaultRecord>> {
        if canonical_root.trim().is_empty() {
            return Err(AppError::General(
                "canonical_root cannot be empty or whitespace".into(),
            ));
        }
        if canonical_root.len() > MAX_CANONICAL_ROOT_BYTES {
            return Err(AppError::General(format!(
                "canonical_root length {} exceeds maximum allowed {} bytes",
                canonical_root.len(),
                MAX_CANONICAL_ROOT_BYTES
            )));
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT vault_id, canonical_root, metadata_version, created_at, updated_at
                 FROM sync_vaults
                 WHERE canonical_root = ?1",
            )
            .map_err(|e| {
                AppError::General(format!(
                    "DB Error preparing get_sync_vault_by_canonical_root: {}",
                    e
                ))
            })?;

        let record = stmt
            .query_row(params![canonical_root], decode_vault_row)
            .optional()
            .map_err(|e| {
                AppError::General(format!(
                    "DB Error executing get_sync_vault_by_canonical_root: {}",
                    e
                ))
            })?;

        Ok(record)
    }

    pub fn insert_sync_vault_mapping(&self, record: &SyncVaultRecord) -> AppResult<()> {
        if record.vault_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id cannot be empty or whitespace".into(),
            ));
        }
        if record.vault_id.len() > MAX_SYNC_VAULT_ID_BYTES {
            return Err(AppError::General(format!(
                "vault_id length {} exceeds maximum allowed {} bytes",
                record.vault_id.len(),
                MAX_SYNC_VAULT_ID_BYTES
            )));
        }
        if record.canonical_root.trim().is_empty() {
            return Err(AppError::General(
                "canonical_root cannot be empty or whitespace".into(),
            ));
        }
        if record.canonical_root.len() > MAX_CANONICAL_ROOT_BYTES {
            return Err(AppError::General(format!(
                "canonical_root length {} exceeds maximum allowed {} bytes",
                record.canonical_root.len(),
                MAX_CANONICAL_ROOT_BYTES
            )));
        }
        if record.metadata_version == 0 {
            return Err(AppError::General(
                "metadata_version must be greater than zero".into(),
            ));
        }
        if record.created_at < 0 {
            return Err(AppError::General(
                "created_at timestamp cannot be negative".into(),
            ));
        }
        if record.updated_at < record.created_at {
            return Err(AppError::General(
                "updated_at timestamp cannot be earlier than created_at".into(),
            ));
        }

        let raw_meta_ver = i64::from(record.metadata_version);

        let rows_affected = self
            .conn
            .execute(
                "INSERT INTO sync_vaults (
                    vault_id,
                    canonical_root,
                    metadata_version,
                    created_at,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ON CONFLICT DO NOTHING",
                params![
                    record.vault_id,
                    record.canonical_root,
                    raw_meta_ver,
                    record.created_at,
                    record.updated_at,
                ],
            )
            .map_err(|e| {
                AppError::General(format!(
                    "DB Error executing insert_sync_vault_mapping: {}",
                    e
                ))
            })?;

        if rows_affected == 1 {
            return Ok(());
        }

        // Conflict handling: read by vault_id and canonical_root to verify idempotent equality
        let by_id = self.get_sync_vault_by_id(&record.vault_id)?;
        let by_root = self.get_sync_vault_by_canonical_root(&record.canonical_root)?;

        match (by_id, by_root) {
            (Some(id_rec), Some(root_rec)) => {
                if id_rec == root_rec
                    && id_rec.vault_id == record.vault_id
                    && id_rec.canonical_root == record.canonical_root
                    && id_rec.metadata_version == record.metadata_version
                {
                    Ok(())
                } else {
                    Err(AppError::General(format!(
                        "Vault mapping collision for vault_id '{}', canonical_root '{}'",
                        record.vault_id, record.canonical_root
                    )))
                }
            }
            _ => Err(AppError::General(format!(
                "Vault mapping collision or missing existing record during conflict for vault_id '{}', canonical_root '{}'",
                record.vault_id, record.canonical_root
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> DbBridge {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::run_sync_schema_migrations(&mut conn).unwrap();
        DbBridge { conn }
    }

    #[test]
    fn insert_and_read_round_trip_all_5_fields() {
        let db = setup_test_db();
        let rec = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/vault1".to_string(),
            metadata_version: 2,
            created_at: 1000,
            updated_at: 1100,
        };

        db.insert_sync_vault_mapping(&rec).unwrap();

        let by_id = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        assert_eq!(by_id, rec);

        let by_root = db
            .get_sync_vault_by_canonical_root("/vault1")
            .unwrap()
            .unwrap();
        assert_eq!(by_root, rec);
    }

    #[test]
    fn read_missing_vault_id_or_root_returns_none() {
        let db = setup_test_db();

        assert!(db.get_sync_vault_by_id("non_existent").unwrap().is_none());
        assert!(db
            .get_sync_vault_by_canonical_root("/non_existent")
            .unwrap()
            .is_none());
    }

    #[test]
    fn lookup_by_id_and_canonical_root_return_same_record() {
        let db = setup_test_db();
        let rec = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/vault1".to_string(),
            metadata_version: 1,
            created_at: 500,
            updated_at: 600,
        };

        db.insert_sync_vault_mapping(&rec).unwrap();

        let rec_id = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        let rec_root = db
            .get_sync_vault_by_canonical_root("/vault1")
            .unwrap()
            .unwrap();
        assert_eq!(rec_id, rec_root);
    }

    #[test]
    fn two_independent_vaults() {
        let db = setup_test_db();
        let r1 = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 100,
            updated_at: 100,
        };
        let r2 = SyncVaultRecord {
            vault_id: "v2".to_string(),
            canonical_root: "/v2".to_string(),
            metadata_version: 1,
            created_at: 200,
            updated_at: 200,
        };

        db.insert_sync_vault_mapping(&r1).unwrap();
        db.insert_sync_vault_mapping(&r2).unwrap();

        assert_eq!(db.get_sync_vault_by_id("v1").unwrap().unwrap(), r1);
        assert_eq!(db.get_sync_vault_by_id("v2").unwrap().unwrap(), r2);
        assert_eq!(
            db.get_sync_vault_by_canonical_root("/v1").unwrap().unwrap(),
            r1
        );
        assert_eq!(
            db.get_sync_vault_by_canonical_root("/v2").unwrap().unwrap(),
            r2
        );
    }

    #[test]
    fn identical_insert_is_idempotent_and_does_not_reset_timestamps() {
        let db = setup_test_db();
        let r1 = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 1000,
            updated_at: 1100,
        };

        db.insert_sync_vault_mapping(&r1).unwrap();

        // Attempt second insert with different timestamps but same IDs and version
        let r1_dup = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 9999,
            updated_at: 9999,
        };

        db.insert_sync_vault_mapping(&r1_dup).unwrap();

        let stored = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        assert_eq!(stored.created_at, 1000);
        assert_eq!(stored.updated_at, 1100);
    }

    #[test]
    fn same_vault_id_different_canonical_root_rejected_with_zero_mutation() {
        let db = setup_test_db();
        let r1 = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 1000,
            updated_at: 1100,
        };

        db.insert_sync_vault_mapping(&r1).unwrap();
        let before = db.get_sync_vault_by_id("v1").unwrap().unwrap();

        let r1_conflict = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/different_root".to_string(),
            metadata_version: 1,
            created_at: 1200,
            updated_at: 1200,
        };

        let res = db.insert_sync_vault_mapping(&r1_conflict);
        assert!(res.is_err());

        let after = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn same_canonical_root_different_vault_id_rejected_with_zero_mutation() {
        let db = setup_test_db();
        let r1 = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 1000,
            updated_at: 1100,
        };

        db.insert_sync_vault_mapping(&r1).unwrap();
        let before = db.get_sync_vault_by_id("v1").unwrap().unwrap();

        let r2_conflict = SyncVaultRecord {
            vault_id: "v2".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 1200,
            updated_at: 1200,
        };

        let res = db.insert_sync_vault_mapping(&r2_conflict);
        assert!(res.is_err());

        let after = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn different_metadata_version_on_same_mapping_rejected_with_zero_mutation() {
        let db = setup_test_db();
        let r1 = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 1000,
            updated_at: 1100,
        };

        db.insert_sync_vault_mapping(&r1).unwrap();
        let before = db.get_sync_vault_by_id("v1").unwrap().unwrap();

        let r1_ver_conflict = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 2,
            created_at: 1200,
            updated_at: 1200,
        };

        let res = db.insert_sync_vault_mapping(&r1_ver_conflict);
        assert!(res.is_err());

        let after = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn empty_or_whitespace_vault_id_or_root_rejected_with_zero_mutation() {
        let db = setup_test_db();
        let r1 = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 1000,
            updated_at: 1100,
        };

        db.insert_sync_vault_mapping(&r1).unwrap();
        let before = db.get_sync_vault_by_id("v1").unwrap().unwrap();

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "".to_string(),
                canonical_root: "/root".to_string(),
                metadata_version: 1,
                created_at: 100,
                updated_at: 100,
            })
            .is_err());

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "   ".to_string(),
                canonical_root: "/root".to_string(),
                metadata_version: 1,
                created_at: 100,
                updated_at: 100,
            })
            .is_err());

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "v2".to_string(),
                canonical_root: "".to_string(),
                metadata_version: 1,
                created_at: 100,
                updated_at: 100,
            })
            .is_err());

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "v2".to_string(),
                canonical_root: "   ".to_string(),
                metadata_version: 1,
                created_at: 100,
                updated_at: 100,
            })
            .is_err());

        assert!(db.get_sync_vault_by_id("").is_err());
        assert!(db.get_sync_vault_by_id("   ").is_err());
        assert!(db.get_sync_vault_by_canonical_root("").is_err());
        assert!(db.get_sync_vault_by_canonical_root("   ").is_err());

        let after = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn oversized_vault_id_or_root_rejected_with_zero_mutation() {
        let db = setup_test_db();
        let r1 = SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 1000,
            updated_at: 1100,
        };

        db.insert_sync_vault_mapping(&r1).unwrap();
        let before = db.get_sync_vault_by_id("v1").unwrap().unwrap();

        let big_id = "v".repeat(MAX_SYNC_VAULT_ID_BYTES + 1);
        let big_root = "/r".repeat(MAX_CANONICAL_ROOT_BYTES + 1);

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: big_id.clone(),
                canonical_root: "/root".to_string(),
                metadata_version: 1,
                created_at: 100,
                updated_at: 100,
            })
            .is_err());

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "v2".to_string(),
                canonical_root: big_root.clone(),
                metadata_version: 1,
                created_at: 100,
                updated_at: 100,
            })
            .is_err());

        assert!(db.get_sync_vault_by_id(&big_id).is_err());
        assert!(db.get_sync_vault_by_canonical_root(&big_root).is_err());

        let after = db.get_sync_vault_by_id("v1").unwrap().unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn metadata_version_zero_rejected() {
        let db = setup_test_db();

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "v1".to_string(),
                canonical_root: "/v1".to_string(),
                metadata_version: 0,
                created_at: 100,
                updated_at: 100,
            })
            .is_err());
    }

    #[test]
    fn negative_timestamps_or_updated_before_created_rejected() {
        let db = setup_test_db();

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "v1".to_string(),
                canonical_root: "/v1".to_string(),
                metadata_version: 1,
                created_at: -1,
                updated_at: 100,
            })
            .is_err());

        assert!(db
            .insert_sync_vault_mapping(&SyncVaultRecord {
                vault_id: "v1".to_string(),
                canonical_root: "/v1".to_string(),
                metadata_version: 1,
                created_at: 100,
                updated_at: 99,
            })
            .is_err());
    }

    #[test]
    fn corrupt_db_row_causes_read_error() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .ok();

        // 1. Negative metadata_version
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v_neg', '/v_neg', -1, 100, 100)",
                [],
            )
            .unwrap();
        assert!(
            db.get_sync_vault_by_id("v_neg").is_err(),
            "Negative metadata_version in DB must cause read Error"
        );

        // 2. metadata_version > u32::MAX (4294967296)
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v_huge', '/v_huge', 4294967296, 100, 100)",
                [],
            )
            .unwrap();
        assert!(
            db.get_sync_vault_by_id("v_huge").is_err(),
            "metadata_version > u32::MAX in DB must cause read Error"
        );

        // 3. Blank vault_id in DB
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('', '/v_blank_id', 1, 100, 100)",
                [],
            )
            .unwrap();
        assert!(
            db.get_sync_vault_by_canonical_root("/v_blank_id").is_err(),
            "Blank vault_id in DB must cause read Error"
        );

        // 4. Blank or whitespace canonical_root in DB
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v_blank_root', '', 1, 100, 100)",
                [],
            )
            .unwrap();
        assert!(
            db.get_sync_vault_by_id("v_blank_root").is_err(),
            "Empty canonical_root in DB must cause read Error"
        );

        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v_ws_root', '   ', 1, 100, 100)",
                [],
            )
            .unwrap();
        assert!(
            db.get_sync_vault_by_id("v_ws_root").is_err(),
            "Whitespace canonical_root in DB must cause read Error"
        );

        // 5. Oversized vault_id in DB
        let big_id = "v".repeat(MAX_SYNC_VAULT_ID_BYTES + 1);
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, '/v_oversized_id', 1, 100, 100)",
                params![big_id],
            )
            .unwrap();
        assert!(
            db.get_sync_vault_by_canonical_root("/v_oversized_id")
                .is_err(),
            "Oversized vault_id in DB must cause read Error"
        );

        // 6. Oversized canonical_root in DB
        let big_root = "/r".repeat(MAX_CANONICAL_ROOT_BYTES + 1);
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v_oversized_root', ?1, 1, 100, 100)",
                params![big_root],
            )
            .unwrap();
        assert!(
            db.get_sync_vault_by_id("v_oversized_root").is_err(),
            "Oversized canonical_root in DB must cause read Error"
        );
    }

    #[test]
    fn conflict_with_corrupt_existing_row_returns_err_and_zero_mutation() {
        let db = setup_test_db();
        db.conn
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .ok();
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES ('v1', '/v1', -1, 100, 200)",
                [],
            )
            .unwrap();

        // Snapshot raw 5 columns before
        let before: (String, String, i64, i64, i64) = db
            .conn
            .query_row(
                "SELECT vault_id, canonical_root, metadata_version, created_at, updated_at FROM sync_vaults WHERE vault_id = 'v1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();

        // Attempting idempotent insert on a corrupt row must fail
        let res = db.insert_sync_vault_mapping(&SyncVaultRecord {
            vault_id: "v1".to_string(),
            canonical_root: "/v1".to_string(),
            metadata_version: 1,
            created_at: 300,
            updated_at: 400,
        });
        assert!(
            res.is_err(),
            "Conflict with corrupt existing row must return Err"
        );

        // Snapshot raw 5 columns after
        let after: (String, String, i64, i64, i64) = db
            .conn
            .query_row(
                "SELECT vault_id, canonical_root, metadata_version, created_at, updated_at FROM sync_vaults WHERE vault_id = 'v1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();

        assert_eq!(
            after, before,
            "Corrupt row must not be mutated by conflict attempt"
        );

        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sync_vaults", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "Database must still contain exactly one row");
    }
}
