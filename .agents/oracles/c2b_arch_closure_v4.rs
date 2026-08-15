use super::*;
use rusqlite::{params, Connection};

const PROVIDER_STATES: [&str; 5] = [
    "ready",
    "bootstrap_required",
    "bootstrapping",
    "error",
    "disabled",
];

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

fn rebuilt_v7_connection() -> Connection {
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
             CHECK (created_at >= 0),
             CHECK (updated_at >= created_at)
         );

         INSERT INTO sync_vaults VALUES ('v1', '/v1', 1, 100, 100);",
    )
    .unwrap();
    run_sync_schema_migrations(&mut conn).unwrap();
    conn
}

fn assert_text_remote_is_rejected_for_every_state(conn: &Connection, suffix: &str) {
    for state in PROVIDER_STATES {
        for with_error in [false, true] {
            let error_label = if with_error { "error" } else { "null" };
            let provider_id = format!("insert-{state}-{error_label}-{suffix}");
            let last_error = with_error.then_some("legacy error");
            let insert = conn.execute(
                "INSERT INTO sync_provider_state
                 (vault_id, provider_id, sync_state, remote_vault_id, last_error, created_at, updated_at)
                 VALUES ('v1', ?1, ?2, ?3, ?4, 100, 100)",
                params![provider_id, state, "legacy-text", last_error],
            );
            assert!(
                insert.is_err(),
                "TEXT remote_vault_id insert was accepted for state={state} last_error={error_label}"
            );

            let persisted: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sync_provider_state WHERE provider_id = ?1",
                    params![provider_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(persisted, 0, "rejected TEXT identity row persisted");
        }

        let provider_id = format!("update-{state}-{suffix}");
        conn.execute(
            "INSERT INTO sync_provider_state
             (vault_id, provider_id, sync_state, remote_vault_id, last_error, created_at, updated_at)
             VALUES ('v1', ?1, ?2, NULL, NULL, 100, 100)",
            params![provider_id, state],
        )
        .unwrap();

        let update = conn.execute(
            "UPDATE sync_provider_state
             SET remote_vault_id = ?1, last_error = ?2
             WHERE vault_id = 'v1' AND provider_id = ?3",
            params!["legacy-text", "legacy error", provider_id],
        );
        assert!(
            update.is_err(),
            "TEXT remote_vault_id update was accepted for state={state}"
        );

        let after: (String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT typeof(remote_vault_id), remote_vault_id, last_error
                 FROM sync_provider_state WHERE provider_id = ?1",
                params![provider_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(after, ("null".to_string(), None, None));
    }
}

#[test]
fn c2b_arch_v7_fresh_schema_has_no_state_dependent_text_identity_exception() {
    let conn = fresh_v7_connection();
    assert_text_remote_is_rejected_for_every_state(&conn, "fresh");
}

#[test]
fn c2b_arch_v7_rebuilt_schema_has_no_state_dependent_text_identity_exception() {
    let conn = rebuilt_v7_connection();
    assert_text_remote_is_rejected_for_every_state(&conn, "rebuilt");
}
