//! What `sync_provider_state` refuses to store, on a schema built from scratch.
//!
//! The identity columns are what tie a local vault to a remote one. A value of
//! the wrong shape there is not a cosmetic problem: it is a vault that syncs
//! against the wrong remote, or against none, without saying so. So the
//! constraints are checked at the storage layer rather than trusted to every
//! caller, and these tests hold them there.

use super::*;
use rusqlite::types::Value;
use rusqlite::{params, Connection};

const PROVIDER_STATES: [&str; 5] = [
    "ready",
    "bootstrap_required",
    "bootstrapping",
    "error",
    "disabled",
];

fn migrated() -> Connection {
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

fn persisted(conn: &Connection, provider_id: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM sync_provider_state WHERE provider_id = ?1",
        params![provider_id],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn identity_columns_hold_only_blobs_of_the_exact_length() {
    let conn = migrated();

    let declared: String = conn
        .query_row(
            "SELECT type FROM pragma_table_info('sync_provider_state') WHERE name = 'remote_vault_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(declared.to_ascii_uppercase(), "BLOB");

    conn.execute(
        "INSERT INTO sync_provider_state
         (vault_id, provider_id, incarnation_id, remote_vault_id, created_at, updated_at)
         VALUES ('v1', 'valid', ?1, ?2, 100, 100)",
        params![vec![1_u8; 16], vec![2_u8; 32]],
    )
    .unwrap();

    let invalid: Vec<(&str, Value, Value)> = vec![
        ("incarnation-blob-15", vec![1_u8; 15].into(), Value::Null),
        (
            "incarnation-text-16",
            "0123456789abcdef".to_string().into(),
            Value::Null,
        ),
        ("remote-blob-10", Value::Null, vec![2_u8; 10].into()),
        ("remote-blob-31", Value::Null, vec![2_u8; 31].into()),
        ("remote-blob-33", Value::Null, vec![2_u8; 33].into()),
        (
            "remote-text-short",
            Value::Null,
            "legacy-remote-id".to_string().into(),
        ),
        (
            "remote-text-32",
            Value::Null,
            "0123456789abcdef0123456789abcdef".to_string().into(),
        ),
    ];
    for (label, incarnation, remote) in invalid {
        let result = conn.execute(
            "INSERT INTO sync_provider_state
             (vault_id, provider_id, incarnation_id, remote_vault_id, created_at, updated_at)
             VALUES ('v1', ?1, ?2, ?3, 100, 100)",
            params![label, incarnation, remote],
        );
        assert!(result.is_err(), "{label} was accepted");
        assert_eq!(persisted(&conn, label), 0, "{label} persisted");
    }
}

/// A text identity is what an old schema allowed. No provider state, however
/// broken, is an exception to refusing it — neither on insert nor on update.
#[test]
fn a_text_identity_is_refused_in_every_provider_state() {
    let conn = migrated();

    for state in PROVIDER_STATES {
        for last_error in [None, Some("legacy error")] {
            let provider_id = format!("insert-{state}-{}", last_error.is_some());
            let insert = conn.execute(
                "INSERT INTO sync_provider_state
                 (vault_id, provider_id, sync_state, remote_vault_id, last_error, created_at, updated_at)
                 VALUES ('v1', ?1, ?2, 'legacy-text', ?3, 100, 100)",
                params![provider_id, state, last_error],
            );
            assert!(insert.is_err(), "text identity inserted in state {state}");
            assert_eq!(persisted(&conn, &provider_id), 0);
        }

        let provider_id = format!("update-{state}");
        conn.execute(
            "INSERT INTO sync_provider_state
             (vault_id, provider_id, sync_state, created_at, updated_at)
             VALUES ('v1', ?1, ?2, 100, 100)",
            params![provider_id, state],
        )
        .unwrap();
        let update = conn.execute(
            "UPDATE sync_provider_state
             SET remote_vault_id = 'legacy-text', last_error = 'legacy error'
             WHERE provider_id = ?1",
            params![provider_id],
        );
        assert!(
            update.is_err(),
            "text identity written by update in state {state}"
        );

        let after: (String, Option<String>) = conn
            .query_row(
                "SELECT typeof(remote_vault_id), last_error FROM sync_provider_state WHERE provider_id = ?1",
                params![provider_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(after, ("null".to_string(), None));
    }
}

#[test]
fn updated_at_cannot_go_behind_created_at() {
    let conn = migrated();

    let inserted = conn.execute(
        "INSERT INTO sync_provider_state
         (vault_id, provider_id, created_at, updated_at)
         VALUES ('v1', 'backward-insert', 200, 100)",
        [],
    );
    assert!(inserted.is_err());

    conn.execute(
        "INSERT INTO sync_provider_state
         (vault_id, provider_id, created_at, updated_at)
         VALUES ('v1', 'backward-update', 100, 100)",
        [],
    )
    .unwrap();
    let updated = conn.execute(
        "UPDATE sync_provider_state SET updated_at = 99 WHERE provider_id = 'backward-update'",
        [],
    );
    assert!(updated.is_err());

    let after: (i64, i64) = conn
        .query_row(
            "SELECT created_at, updated_at FROM sync_provider_state WHERE provider_id = 'backward-update'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(after, (100, 100), "a refused update still changed the row");
}
