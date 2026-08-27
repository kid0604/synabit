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

    /// Forget whatever vault was registered at this directory.
    ///
    /// Used after a restore. The directory held a freshly created vault with an
    /// identity of its own; the archive replaced that identity with the one it
    /// was backed up under. Registration would then find a different vault
    /// already claiming this root and refuse, so the stale claim is removed
    /// first and the restored vault registers as itself.
    ///
    /// The foreign keys cascade, which is the point: the provider state,
    /// outbox, inbox and CRDT history of the discarded vault go with it. None
    /// of it described the restored vault, and a restore has to re-establish
    /// its position with the server anyway.
    ///
    /// Returns whether there was anything to forget.
    pub fn forget_sync_vault_at_root(&self, canonical_root: &str) -> AppResult<bool> {
        if canonical_root.trim().is_empty() {
            return Err(AppError::General(
                "canonical_root cannot be empty or whitespace".into(),
            ));
        }

        let removed = self
            .conn
            .execute(
                "DELETE FROM sync_vaults WHERE canonical_root = ?1",
                params![canonical_root],
            )
            .map_err(|e| AppError::General(format!("DB Error forgetting vault: {e}")))?;

        Ok(removed > 0)
    }

    /// Point a vault that already exists at the directory it has moved to.
    ///
    /// The mapping is otherwise immutable, and deliberately so: one vault id
    /// appearing under two roots normally means the folder was *copied*, and
    /// adopting the copy would silently merge two vaults into one. That is why
    /// `insert_sync_vault_mapping` refuses it.
    ///
    /// A move is the one case where the same id under a new root is correct —
    /// there is still exactly one vault — and only the code that performed the
    /// move can tell the two apart. So it says so here rather than the
    /// registration path guessing.
    ///
    /// Returns `false` when there was no such vault to rebind, which is not an
    /// error: a first run has nothing recorded yet and registration will create
    /// the row with the right root anyway.
    pub fn rebind_sync_vault_canonical_root(
        &self,
        vault_id: &str,
        new_canonical_root: &str,
        now: i64,
    ) -> AppResult<bool> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        if new_canonical_root.trim().is_empty() {
            return Err(AppError::General(
                "canonical_root cannot be empty or whitespace".into(),
            ));
        }
        if new_canonical_root.len() > MAX_CANONICAL_ROOT_BYTES {
            return Err(AppError::General(format!(
                "canonical_root length {} exceeds maximum allowed {} bytes",
                new_canonical_root.len(),
                MAX_CANONICAL_ROOT_BYTES
            )));
        }

        if self.get_sync_vault_by_id(vault_id)?.is_none() {
            return Ok(false);
        }

        // Somebody else is already living there. Rebinding would either fail on
        // the unique index or, worse, describe two vaults as one directory.
        if let Some(occupant) = self.get_sync_vault_by_canonical_root(new_canonical_root)? {
            if occupant.vault_id != vault_id {
                return Err(AppError::General(format!(
                    "'{new_canonical_root}' is already registered to a different vault \
                     ({}), so vault {vault_id} was not moved onto it",
                    occupant.vault_id
                )));
            }
            return Ok(true);
        }

        // `updated_at >= created_at` is enforced by the table. Taking the later
        // of the two keeps a clock that has gone backwards from failing the
        // move outright — the timestamp is for observability, and losing a
        // little accuracy is better than refusing to record where the vault is.
        self.conn
            .execute(
                "UPDATE sync_vaults
                 SET canonical_root = ?2,
                     updated_at = MAX(?3, created_at)
                 WHERE vault_id = ?1",
                params![vault_id, new_canonical_root, now],
            )
            .map_err(|e| {
                AppError::General(format!("DB Error rebinding vault canonical_root: {e}"))
            })?;

        Ok(true)
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

    /// Moving the vault on Android changes its canonical root. Registration
    /// runs on every scan and every sync and rejects a second root for the same
    /// id, so without a rebind the app moves the vault and then cannot open it.
    mod rebind {
        use super::*;

        fn vault(id: &str, root: &str) -> SyncVaultRecord {
            SyncVaultRecord {
                vault_id: id.to_string(),
                canonical_root: root.to_string(),
                metadata_version: 1,
                created_at: 1000,
                updated_at: 1000,
            }
        }

        #[test]
        fn a_moved_vault_is_registerable_at_its_new_root() {
            let db = setup_test_db();
            db.insert_sync_vault_mapping(&vault("v1", "/old/vault"))
                .unwrap();

            assert!(db
                .rebind_sync_vault_canonical_root("v1", "/new/vault", 2000)
                .unwrap());

            // The check that matters: the ordinary registration path now
            // succeeds where it previously reported a mapping collision.
            db.insert_sync_vault_mapping(&vault("v1", "/new/vault"))
                .expect("registering the vault at its new root must succeed after a rebind");
            assert_eq!(
                db.get_sync_vault_by_id("v1")
                    .unwrap()
                    .unwrap()
                    .canonical_root,
                "/new/vault"
            );
            assert!(db
                .get_sync_vault_by_canonical_root("/old/vault")
                .unwrap()
                .is_none());
        }

        #[test]
        fn rebinding_a_vault_that_was_never_recorded_is_not_an_error() {
            let db = setup_test_db();
            assert!(!db
                .rebind_sync_vault_canonical_root("absent", "/new/vault", 2000)
                .unwrap());
        }

        #[test]
        fn rebinding_onto_its_own_root_is_harmless() {
            let db = setup_test_db();
            db.insert_sync_vault_mapping(&vault("v1", "/vault"))
                .unwrap();

            assert!(db
                .rebind_sync_vault_canonical_root("v1", "/vault", 2000)
                .unwrap());
            assert_eq!(
                db.get_sync_vault_by_id("v1")
                    .unwrap()
                    .unwrap()
                    .canonical_root,
                "/vault"
            );
        }

        /// Two vaults must never be described as one directory.
        #[test]
        fn a_root_another_vault_already_holds_is_refused() {
            let db = setup_test_db();
            db.insert_sync_vault_mapping(&vault("v1", "/one")).unwrap();
            db.insert_sync_vault_mapping(&vault("v2", "/two")).unwrap();

            let err = db
                .rebind_sync_vault_canonical_root("v1", "/two", 2000)
                .unwrap_err();

            assert!(err.to_string().contains("v2"), "unhelpful error: {err}");
            assert_eq!(
                db.get_sync_vault_by_id("v1")
                    .unwrap()
                    .unwrap()
                    .canonical_root,
                "/one",
                "a refused rebind must not have moved anything"
            );
            assert_eq!(
                db.get_sync_vault_by_id("v2")
                    .unwrap()
                    .unwrap()
                    .canonical_root,
                "/two"
            );
        }

        /// The table enforces `updated_at >= created_at`. A clock that has gone
        /// backwards must not make the vault unopenable — this codebase has
        /// already lost a sync run to exactly that.
        #[test]
        fn a_clock_that_went_backwards_does_not_fail_the_move() {
            let db = setup_test_db();
            db.insert_sync_vault_mapping(&vault("v1", "/old")).unwrap();

            assert!(db
                .rebind_sync_vault_canonical_root("v1", "/new", 1)
                .unwrap());

            let row = db.get_sync_vault_by_id("v1").unwrap().unwrap();
            assert_eq!(row.canonical_root, "/new");
            assert!(row.updated_at >= row.created_at);
        }

        /// After a restore the directory must be free for the archive's own
        /// identity to claim, or registration reports a collision and the
        /// restored vault cannot be opened.
        #[test]
        fn forgetting_a_root_lets_a_restored_vault_claim_it() {
            let db = setup_test_db();
            db.insert_sync_vault_mapping(&vault("fresh", "/vault"))
                .unwrap();

            assert!(db.forget_sync_vault_at_root("/vault").unwrap());

            db.insert_sync_vault_mapping(&vault("restored", "/vault"))
                .expect("the restored vault must be able to register at that root");
            assert_eq!(
                db.get_sync_vault_by_canonical_root("/vault")
                    .unwrap()
                    .unwrap()
                    .vault_id,
                "restored"
            );
        }

        #[test]
        fn forgetting_a_root_nothing_is_registered_at_is_not_an_error() {
            let db = setup_test_db();
            assert!(!db.forget_sync_vault_at_root("/never-used").unwrap());
            assert!(db.forget_sync_vault_at_root("   ").is_err());
        }

        /// Only the named root goes. A restore must not disturb another vault
        /// the same install is tracking.
        #[test]
        fn forgetting_one_root_leaves_other_vaults_alone() {
            let db = setup_test_db();
            db.insert_sync_vault_mapping(&vault("v1", "/one")).unwrap();
            db.insert_sync_vault_mapping(&vault("v2", "/two")).unwrap();

            db.forget_sync_vault_at_root("/one").unwrap();

            assert!(db.get_sync_vault_by_id("v1").unwrap().is_none());
            assert_eq!(
                db.get_sync_vault_by_id("v2")
                    .unwrap()
                    .unwrap()
                    .canonical_root,
                "/two"
            );
        }

        #[test]
        fn empty_inputs_are_refused() {
            let db = setup_test_db();
            db.insert_sync_vault_mapping(&vault("v1", "/old")).unwrap();

            assert!(db
                .rebind_sync_vault_canonical_root("", "/new", 2000)
                .is_err());
            assert!(db
                .rebind_sync_vault_canonical_root("v1", "  ", 2000)
                .is_err());
            assert_eq!(
                db.get_sync_vault_by_id("v1")
                    .unwrap()
                    .unwrap()
                    .canonical_root,
                "/old"
            );
        }
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
