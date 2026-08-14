use super::*;
use crate::db::sync_vault::SyncVaultRecord;
use rusqlite::params;
use std::sync::{Arc, Mutex};

const ARCH_VAULT: &str = "c2b-arch-vault";
const ARCH_PROVIDER: &str = "server";
const ARCH_NOW: i64 = 20_000;

fn seeded_provider_db() -> DbBridge {
    let mut db = DbBridge::new_in_memory().unwrap();
    db.insert_sync_vault_mapping(&SyncVaultRecord {
        vault_id: ARCH_VAULT.to_string(),
        canonical_root: "/tmp/c2b-arch-vault".to_string(),
        metadata_version: 1,
        created_at: ARCH_NOW,
        updated_at: ARCH_NOW,
    })
    .unwrap();
    db.ensure_sync_provider_state(ARCH_VAULT, ARCH_PROVIDER)
        .unwrap();
    db
}

#[test]
fn c2b_arch_snapshot_roundtrips_non_null_provider_identity_blobs() {
    let mut db = seeded_provider_db();
    let incarnation_id = [41u8; 16];
    let remote_vault_id = [42u8; 32];
    db.reconcile_sync_provider_plan(
        ARCH_VAULT,
        ARCH_PROVIDER,
        Some(incarnation_id),
        Some(remote_vault_id),
        false,
        ARCH_NOW + 1,
    )
    .unwrap();

    let state = Arc::new(Mutex::new(db));
    let snapshot = snapshot_c2b_runtime_raw(&state, ARCH_VAULT, ARCH_PROVIDER).unwrap();
    assert_eq!(snapshot.provider_state.len(), 1);
    assert_eq!(snapshot.provider_state[0].incarnation_id, Some(incarnation_id));
    assert_eq!(
        snapshot.provider_state[0].remote_vault_id,
        Some(remote_vault_id)
    );
}

#[test]
fn c2b_arch_snapshot_rejects_malformed_provider_incarnation_blob() {
    let db = seeded_provider_db();
    db.conn()
        .execute_batch("PRAGMA ignore_check_constraints = ON;")
        .unwrap();
    db.conn()
        .execute(
            "UPDATE sync_provider_state SET incarnation_id = ?1 WHERE vault_id = ?2 AND provider_id = ?3",
            params![vec![7u8; 15], ARCH_VAULT, ARCH_PROVIDER],
        )
        .unwrap();

    let state = Arc::new(Mutex::new(db));
    let error = snapshot_c2b_runtime_raw(&state, ARCH_VAULT, ARCH_PROVIDER).unwrap_err();
    assert!(error.to_string().contains("malformed incarnation_id"));
}

#[test]
fn c2b_arch_snapshot_rejects_malformed_provider_remote_vault_blob() {
    let db = seeded_provider_db();
    db.conn()
        .execute_batch("PRAGMA ignore_check_constraints = ON;")
        .unwrap();
    db.conn()
        .execute(
            "UPDATE sync_provider_state SET remote_vault_id = ?1 WHERE vault_id = ?2 AND provider_id = ?3",
            params![vec![8u8; 31], ARCH_VAULT, ARCH_PROVIDER],
        )
        .unwrap();

    let state = Arc::new(Mutex::new(db));
    let error = snapshot_c2b_runtime_raw(&state, ARCH_VAULT, ARCH_PROVIDER).unwrap_err();
    assert!(error.to_string().contains("malformed remote_vault_id"));
}

#[test]
fn c2b_arch_schema_upgrade_normalizes_legacy_text_remote_identity() {
    let mut db = seeded_provider_db();
    db.conn()
        .execute(
            "UPDATE sync_provider_state SET remote_vault_id = 'legacy-remote-id', sync_state = 'ready', last_error = NULL WHERE vault_id = ?1 AND provider_id = ?2",
            params![ARCH_VAULT, ARCH_PROVIDER],
        )
        .unwrap();
    db.conn()
        .execute(
            "UPDATE sync_schema_meta SET version = 6 WHERE singleton_id = 1",
            [],
        )
        .unwrap();

    crate::db::run_sync_schema_migrations_for_test(db.conn_mut()).unwrap();

    let (storage_type, state, last_error): (String, String, Option<String>) = db
        .conn()
        .query_row(
            "SELECT typeof(remote_vault_id), sync_state, last_error FROM sync_provider_state WHERE vault_id = ?1 AND provider_id = ?2",
            params![ARCH_VAULT, ARCH_PROVIDER],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(storage_type, "null");
    assert_eq!(state, "bootstrap_required");
    assert!(last_error
        .as_deref()
        .unwrap_or_default()
        .contains("legacy remote_vault_id"));

    let version: i64 = db
        .conn()
        .query_row(
            "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 7);

    let declared_type: String = db
        .conn()
        .query_row(
            "SELECT type FROM pragma_table_info('sync_provider_state') WHERE name = 'remote_vault_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(declared_type.to_ascii_uppercase(), "BLOB");
}
