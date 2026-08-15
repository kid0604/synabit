use super::*;
use rusqlite::{params, Connection};

fn fresh_v7_connection() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    run_sync_schema_migrations(&mut conn).unwrap();
    conn.execute(
        "INSERT INTO sync_vaults
         (vault_id, canonical_root, metadata_version, created_at, updated_at)
         VALUES ('v1', '/v1', 1, 100, 100)",
        [],
    )
    .unwrap();
    conn
}

fn historical_v6_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
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
             CHECK (incarnation_id IS NULL OR length(incarnation_id) = 16),
             CHECK (remote_vault_id IS NULL OR length(remote_vault_id) > 0),
             CHECK (created_at >= 0),
             CHECK (updated_at >= created_at)
         );

         INSERT INTO sync_vaults VALUES ('v1', '/v1', 1, 100, 100);
         INSERT INTO sync_provider_state
             (vault_id, provider_id, remote_vault_id, created_at, updated_at)
         VALUES ('v1', 'legacy', 'legacy-remote-id', 100, 100);",
    )
    .unwrap();
    conn
}

fn assert_identity_constraints_are_exact(conn: &Connection, suffix: &str) {
    let valid_provider = format!("valid-{suffix}");
    conn.execute(
        "INSERT INTO sync_provider_state
         (vault_id, provider_id, incarnation_id, remote_vault_id, created_at, updated_at)
         VALUES ('v1', ?1, ?2, ?3, 100, 100)",
        params![valid_provider, vec![1_u8; 16], vec![2_u8; 32]],
    )
    .unwrap();

    let invalid_cases: Vec<(&str, rusqlite::types::Value, rusqlite::types::Value)> = vec![
        (
            "incarnation-blob-15",
            vec![1_u8; 15].into(),
            rusqlite::types::Value::Null,
        ),
        (
            "incarnation-text-16",
            "0123456789abcdef".to_string().into(),
            rusqlite::types::Value::Null,
        ),
        (
            "remote-blob-10",
            rusqlite::types::Value::Null,
            vec![2_u8; 10].into(),
        ),
        (
            "remote-blob-31",
            rusqlite::types::Value::Null,
            vec![2_u8; 31].into(),
        ),
        (
            "remote-blob-33",
            rusqlite::types::Value::Null,
            vec![2_u8; 33].into(),
        ),
        (
            "remote-text-short",
            rusqlite::types::Value::Null,
            "legacy-remote-id".to_string().into(),
        ),
        (
            "remote-text-32",
            rusqlite::types::Value::Null,
            "0123456789abcdef0123456789abcdef".to_string().into(),
        ),
    ];

    for (label, incarnation, remote) in invalid_cases {
        let provider_id = format!("{label}-{suffix}");
        let result = conn.execute(
            "INSERT INTO sync_provider_state
             (vault_id, provider_id, incarnation_id, remote_vault_id, created_at, updated_at)
             VALUES ('v1', ?1, ?2, ?3, 100, 100)",
            params![provider_id, incarnation, remote],
        );
        assert!(result.is_err(), "invalid identity case {label} was accepted");

        let persisted: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sync_provider_state WHERE provider_id = ?1",
                params![provider_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted, 0, "invalid identity case {label} persisted");
    }
}

fn assert_backward_update_is_rejected(conn: &Connection, provider_id: &str) {
    let before: (i64, i64) = conn
        .query_row(
            "SELECT created_at, updated_at FROM sync_provider_state WHERE provider_id = ?1",
            params![provider_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    let result = conn.execute(
        "UPDATE sync_provider_state SET updated_at = ?1 WHERE provider_id = ?2",
        params![before.0 - 1, provider_id],
    );
    assert!(result.is_err(), "backward updated_at was accepted");

    let after: (i64, i64) = conn
        .query_row(
            "SELECT created_at, updated_at FROM sync_provider_state WHERE provider_id = ?1",
            params![provider_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(after, before, "failed backward update mutated timestamps");
}

#[test]
fn c2b_arch_v6_fresh_schema_rejects_identity_storage_loopholes() {
    let conn = fresh_v7_connection();
    assert_identity_constraints_are_exact(&conn, "fresh");
}

#[test]
fn c2b_arch_v6_rebuilt_schema_rejects_identity_storage_loopholes() {
    let mut conn = historical_v6_connection();
    run_sync_schema_migrations(&mut conn).unwrap();

    let storage: String = conn
        .query_row(
            "SELECT typeof(remote_vault_id) FROM sync_provider_state WHERE provider_id = 'legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(storage, "null");
    assert_identity_constraints_are_exact(&conn, "rebuilt");
}

#[test]
fn c2b_arch_v6_backward_updates_fail_without_rewriting_created_at() {
    let fresh = fresh_v7_connection();
    fresh
        .execute(
            "INSERT INTO sync_provider_state
             (vault_id, provider_id, created_at, updated_at)
             VALUES ('v1', 'fresh-time', 100, 100)",
            [],
        )
        .unwrap();
    assert_backward_update_is_rejected(&fresh, "fresh-time");

    let mut rebuilt = historical_v6_connection();
    run_sync_schema_migrations(&mut rebuilt).unwrap();
    assert_backward_update_is_rejected(&rebuilt, "legacy");
}
