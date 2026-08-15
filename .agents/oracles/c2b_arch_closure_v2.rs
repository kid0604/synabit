use super::*;
use rusqlite::{params, Connection};

const NOW: i64 = 30_000;

fn historical_v6_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS sync_document_paths (
             vault_id TEXT NOT NULL,
             doc_id TEXT NOT NULL,
             rel_path TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, doc_id),
             UNIQUE (vault_id, rel_path)
         );
         CREATE TABLE IF NOT EXISTS sync_crdt_documents (
             vault_id TEXT NOT NULL,
             doc_id TEXT NOT NULL,
             snapshot BLOB NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, doc_id)
         );
         CREATE TABLE IF NOT EXISTS sync_crdt_updates (
             vault_id TEXT NOT NULL,
             doc_id TEXT NOT NULL,
             update_id INTEGER NOT NULL,
             delta BLOB NOT NULL,
             timestamp INTEGER NOT NULL,
             PRIMARY KEY (vault_id, update_id)
         );
         CREATE TABLE IF NOT EXISTS sync_outbox (
             vault_id TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             operation_id BLOB NOT NULL,
             entry_kind TEXT NOT NULL,
             node_id TEXT NOT NULL,
             rel_path TEXT,
             source_hash BLOB,
             original_timestamp INTEGER NOT NULL,
             encrypted_payload BLOB,
             payload_hash BLOB,
             asset_ref_blob BLOB,
             state TEXT NOT NULL DEFAULT 'prepared',
             retry_count INTEGER NOT NULL DEFAULT 0,
             next_retry_at INTEGER,
             last_error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, provider_id, operation_id)
         );
         CREATE TABLE IF NOT EXISTS sync_inbox (
             vault_id TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             page_cursor TEXT NOT NULL DEFAULT '',
             remote_position TEXT NOT NULL,
             remote_seq INTEGER,
             operation_id BLOB NOT NULL,
             doc_hash BLOB NOT NULL,
             entry_kind TEXT NOT NULL,
             encrypted_payload BLOB,
             payload_hash BLOB,
             source_device TEXT,
             state TEXT NOT NULL DEFAULT 'pending',
             last_error TEXT,
             received_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             applied_at INTEGER,
             PRIMARY KEY (vault_id, provider_id, operation_id)
         );
         CREATE TABLE sync_schema_meta (
             singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
             version INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );
         INSERT INTO sync_schema_meta VALUES (1, 6, 6000);

         CREATE TABLE sync_vaults (
             vault_id TEXT PRIMARY KEY,
             canonical_root TEXT NOT NULL UNIQUE,
             metadata_version INTEGER NOT NULL DEFAULT 1,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             CHECK (created_at >= 0),
             CHECK (updated_at >= created_at)
         );

         CREATE TABLE sync_provider_state (
             vault_id TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             cursor TEXT NOT NULL DEFAULT '',
             ack_cursor TEXT,
             sync_state TEXT NOT NULL DEFAULT 'ready',
             incarnation_id BLOB,
             remote_vault_id TEXT,
             last_error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, provider_id),
             FOREIGN KEY (vault_id) REFERENCES sync_vaults(vault_id) ON DELETE CASCADE,
             CHECK (length(incarnation_id) = 16 OR incarnation_id IS NULL),
             CHECK (remote_vault_id IS NULL OR (length(remote_vault_id) > 0 AND length(remote_vault_id) <= 128)),
             CHECK (created_at >= 0),
             CHECK (updated_at >= created_at)
         );

         CREATE TABLE sync_provider_child_guard (
             vault_id TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             child_id TEXT NOT NULL,
             PRIMARY KEY (vault_id, provider_id, child_id),
             FOREIGN KEY (vault_id, provider_id)
                 REFERENCES sync_provider_state(vault_id, provider_id)
                 ON DELETE CASCADE
         );

         INSERT INTO sync_vaults VALUES ('v1', '/v1', 1, 1, 1);",
    )
    .unwrap();

    let rows = [
        ("ready-text", "ready", 100_i64),
        ("disabled-text", "disabled", 200_i64),
        ("bad-blob", "ready", 300_i64),
        ("valid-blob", "ready", 400_i64),
        ("future-text", "ready", i64::MAX),
    ];
    for (provider, state, updated_at) in rows {
        conn.execute(
            "INSERT INTO sync_provider_state
             (vault_id, provider_id, cursor, sync_state, created_at, updated_at)
             VALUES ('v1', ?1, '', ?2, 1, ?3)",
            params![provider, state, updated_at],
        )
        .unwrap();
    }

    conn.execute(
        "UPDATE sync_provider_state SET remote_vault_id = 'legacy-ready' WHERE provider_id = 'ready-text'",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE sync_provider_state SET remote_vault_id = 'legacy-disabled' WHERE provider_id = 'disabled-text'",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE sync_provider_state SET remote_vault_id = ?1 WHERE provider_id = 'bad-blob'",
        params![vec![3_u8; 31]],
    )
    .unwrap();
    conn.execute(
        "UPDATE sync_provider_state SET remote_vault_id = ?1 WHERE provider_id = 'valid-blob'",
        params![vec![4_u8; 32]],
    )
    .unwrap();
    conn.execute(
        "UPDATE sync_provider_state SET remote_vault_id = 'legacy-future' WHERE provider_id = 'future-text'",
        [],
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO sync_provider_child_guard VALUES ('v1', 'ready-text', 'child-ready');
         INSERT INTO sync_provider_child_guard VALUES ('v1', 'valid-blob', 'child-valid');",
    )
    .unwrap();

    conn
}

fn pragma_foreign_keys(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap()
}

fn declared_remote_type(conn: &Connection) -> String {
    conn.query_row(
        "SELECT type FROM pragma_table_info('sync_provider_state') WHERE name = 'remote_vault_id'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

fn provider_observation(
    conn: &Connection,
    provider_id: &str,
) -> (String, String, Option<String>, String, i64) {
    conn.query_row(
        "SELECT typeof(remote_vault_id), sync_state, last_error, typeof(updated_at), updated_at
         FROM sync_provider_state WHERE vault_id = 'v1' AND provider_id = ?1",
        params![provider_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )
    .unwrap()
}

fn assert_exact_provider_constraints(conn: &Connection) {
    let bad_incarnation = conn.execute(
        "INSERT INTO sync_provider_state
         (vault_id, provider_id, incarnation_id, created_at, updated_at)
         VALUES ('v1', 'bad-incarnation-new', ?1, 1, 1)",
        params![vec![1_u8; 15]],
    );
    assert!(bad_incarnation.is_err());

    let bad_remote = conn.execute(
        "INSERT INTO sync_provider_state
         (vault_id, provider_id, remote_vault_id, created_at, updated_at)
         VALUES ('v1', 'bad-remote-new', ?1, 1, 1)",
        params![vec![2_u8; 31]],
    );
    assert!(bad_remote.is_err());

    let backward_timestamp = conn.execute(
        "INSERT INTO sync_provider_state
         (vault_id, provider_id, created_at, updated_at)
         VALUES ('v1', 'bad-time-new', 2, 1)",
        [],
    );
    assert!(backward_timestamp.is_err());
}

#[test]
fn c2b_arch_v5_fresh_schema_enforces_exact_identity_and_timestamp_constraints() {
    let mut conn = Connection::open_in_memory().unwrap();
    run_sync_schema_migrations(&mut conn).unwrap();
    conn.execute(
        "INSERT INTO sync_vaults
         (vault_id, canonical_root, metadata_version, created_at, updated_at)
         VALUES ('v1', '/v1', 1, 1, 1)",
        [],
    )
    .unwrap();

    assert_eq!(declared_remote_type(&conn).to_ascii_uppercase(), "BLOB");
    assert_exact_provider_constraints(&conn);
}

#[test]
fn c2b_arch_v5_real_v6_text_rebuild_normalizes_and_preserves_foreign_keys() {
    let mut conn = historical_v6_connection();
    assert_eq!(declared_remote_type(&conn).to_ascii_uppercase(), "TEXT");

    run_sync_schema_migrations(&mut conn).unwrap();

    assert_eq!(declared_remote_type(&conn).to_ascii_uppercase(), "BLOB");
    let version: i64 = conn
        .query_row(
            "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, LATEST_SYNC_SCHEMA_VERSION);
    assert_eq!(pragma_foreign_keys(&conn), 1);

    let foreign_key_errors: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(foreign_key_errors, 0);
    let child_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sync_provider_child_guard",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(child_count, 2);

    for provider in ["ready-text", "bad-blob"] {
        let (storage, state, error, updated_type, _) = provider_observation(&conn, provider);
        assert_eq!(storage, "null");
        assert_eq!(state, "bootstrap_required");
        assert!(error
            .as_deref()
            .unwrap_or_default()
            .contains("legacy remote_vault_id"));
        assert_eq!(updated_type, "integer");
    }

    let (storage, state, error, _, _) = provider_observation(&conn, "disabled-text");
    assert_eq!(storage, "null");
    assert_eq!(state, "disabled");
    assert!(error
        .as_deref()
        .unwrap_or_default()
        .contains("legacy remote_vault_id"));

    let (storage, state, error, _, _) = provider_observation(&conn, "valid-blob");
    assert_eq!(storage, "blob");
    assert_eq!(state, "ready");
    assert_eq!(error, None);
    let valid_remote: Vec<u8> = conn
        .query_row(
            "SELECT remote_vault_id FROM sync_provider_state WHERE provider_id = 'valid-blob'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(valid_remote, vec![4_u8; 32]);

    let (_, state, _, updated_type, updated_at) = provider_observation(&conn, "future-text");
    assert_eq!(state, "bootstrap_required");
    assert_eq!(updated_type, "integer");
    assert_eq!(updated_at, i64::MAX);

    assert_exact_provider_constraints(&conn);

    run_sync_schema_migrations(&mut conn).unwrap();
    assert_eq!(pragma_foreign_keys(&conn), 1);
    assert_eq!(declared_remote_type(&conn).to_ascii_uppercase(), "BLOB");
}

#[test]
fn c2b_arch_v5_rebuild_failure_rolls_back_and_restores_foreign_keys() {
    let mut conn = historical_v6_connection();
    conn.execute_batch("CREATE TABLE sync_provider_state_v7 (blocker TEXT);")
        .unwrap();

    let error = migrate_sync_schema_v7(&mut conn).unwrap_err();
    assert!(error.to_string().contains("rebuild sync_provider_state"));
    assert_eq!(pragma_foreign_keys(&conn), 1);
    assert_eq!(declared_remote_type(&conn).to_ascii_uppercase(), "TEXT");

    let version: i64 = conn
        .query_row(
            "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 6);
    let (storage, state, error, _, updated_at) = provider_observation(&conn, "ready-text");
    assert_eq!(storage, "text");
    assert_eq!(state, "ready");
    assert_eq!(error, None);
    assert_eq!(updated_at, 100);
}

#[test]
fn c2b_arch_v5_missing_identity_column_fails_before_mutation() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS sync_document_paths (
             vault_id TEXT NOT NULL,
             doc_id TEXT NOT NULL,
             rel_path TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, doc_id),
             UNIQUE (vault_id, rel_path)
         );
         CREATE TABLE IF NOT EXISTS sync_crdt_documents (
             vault_id TEXT NOT NULL,
             doc_id TEXT NOT NULL,
             snapshot BLOB NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, doc_id)
         );
         CREATE TABLE IF NOT EXISTS sync_crdt_updates (
             vault_id TEXT NOT NULL,
             doc_id TEXT NOT NULL,
             update_id INTEGER NOT NULL,
             delta BLOB NOT NULL,
             timestamp INTEGER NOT NULL,
             PRIMARY KEY (vault_id, update_id)
         );
         CREATE TABLE IF NOT EXISTS sync_outbox (
             vault_id TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             operation_id BLOB NOT NULL,
             entry_kind TEXT NOT NULL,
             node_id TEXT NOT NULL,
             rel_path TEXT,
             source_hash BLOB,
             original_timestamp INTEGER NOT NULL,
             encrypted_payload BLOB,
             payload_hash BLOB,
             asset_ref_blob BLOB,
             state TEXT NOT NULL DEFAULT 'prepared',
             retry_count INTEGER NOT NULL DEFAULT 0,
             next_retry_at INTEGER,
             last_error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, provider_id, operation_id)
         );
         CREATE TABLE IF NOT EXISTS sync_inbox (
             vault_id TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             page_cursor TEXT NOT NULL DEFAULT '',
             remote_position TEXT NOT NULL,
             remote_seq INTEGER,
             operation_id BLOB NOT NULL,
             doc_hash BLOB NOT NULL,
             entry_kind TEXT NOT NULL,
             encrypted_payload BLOB,
             payload_hash BLOB,
             source_device TEXT,
             state TEXT NOT NULL DEFAULT 'pending',
             last_error TEXT,
             received_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             applied_at INTEGER,
             PRIMARY KEY (vault_id, provider_id, operation_id)
         );
         CREATE TABLE sync_schema_meta (
             singleton_id INTEGER PRIMARY KEY,
             version INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );
         INSERT INTO sync_schema_meta VALUES (1, 6, 6000);
         CREATE TABLE sync_provider_state (
             vault_id TEXT NOT NULL,
             provider_id TEXT NOT NULL,
             sync_state TEXT NOT NULL,
             last_error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             PRIMARY KEY (vault_id, provider_id)
         );
         INSERT INTO sync_provider_state VALUES ('v1', 'server', 'ready', NULL, 1, 1);",
    )
    .unwrap();

    let error = migrate_sync_schema_v7(&mut conn).unwrap_err();
    assert!(error
        .to_string()
        .to_ascii_lowercase()
        .contains("remote_vault_id schema inspection"));
    assert_eq!(pragma_foreign_keys(&conn), 1);
    let version: i64 = conn
        .query_row(
            "SELECT version FROM sync_schema_meta WHERE singleton_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, 6);
    let state: String = conn
        .query_row(
            "SELECT sync_state FROM sync_provider_state WHERE provider_id = 'server'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "ready");
}
