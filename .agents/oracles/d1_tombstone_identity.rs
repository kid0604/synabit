use super::{validate_and_parse_remote_entry, InboxApplyFailureKind};
use crate::sync::core::change::{
    encode_sync_payload_v5, prepare_durable_outbox_operations, LocalChange,
};
use crate::sync::core::types::SyncPayload;
use std::sync::Mutex;
use synabit_protocol::{DeletePayload, SyncEntryKind};

fn encode_for_validation(key: &[u8; 32], payload: &SyncPayload) -> (Vec<u8>, [u8; 32]) {
    encode_sync_payload_v5(key, payload, true).unwrap()
}

fn delete_change() -> LocalChange {
    LocalChange {
        rel_path: "notes/dead.md".into(),
        is_delete: true,
        new_hash: String::new(),
    }
}

#[test]
fn d1_tombstone_identity_preparation_is_exact_and_retry_stable() {
    let vault = tempfile::tempdir().unwrap();
    let db = crate::db::sync_outbox::tests::setup_test_db();
    db.upsert_document_path("v1", "node-delete", "notes/dead.md")
        .unwrap();
    let db_state = Mutex::new(db);
    let key = [41u8; 32];
    prepare_durable_outbox_operations(
        &db_state,
        vault.path(),
        vec![delete_change()],
        &key,
        "v1",
        "gdrive",
    )
    .unwrap();

    let before = db_state
        .lock()
        .unwrap()
        .snapshot_all_scoped_outbox("v1", "gdrive")
        .unwrap();
    assert_eq!(before.len(), 1);
    let record = &before[0];
    assert_eq!(record.entry_kind, SyncEntryKind::Delete);
    assert_eq!(record.node_id, "node-delete");
    assert_eq!(record.rel_path.as_deref(), Some("notes/dead.md"));
    assert_eq!(
        record.doc_hash,
        Some(*blake3::hash(b"notes/dead.md").as_bytes())
    );

    let encrypted = record.encrypted_payload.as_ref().unwrap();
    let plaintext = crate::sync::core::crypto::decrypt(&key, encrypted).unwrap();
    let (decoded, remainder) = postcard::take_from_bytes::<SyncPayload>(&plaintext).unwrap();
    assert!(remainder.is_empty());
    assert_eq!(
        decoded,
        SyncPayload::Delete(DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "notes/dead.md".into(),
        })
    );

    prepare_durable_outbox_operations(
        &db_state,
        vault.path(),
        vec![delete_change()],
        &key,
        "v1",
        "gdrive",
    )
    .unwrap();
    let after = db_state
        .lock()
        .unwrap()
        .snapshot_all_scoped_outbox("v1", "gdrive")
        .unwrap();
    assert_eq!(
        after, before,
        "retry must reuse the exact durable tombstone"
    );
}

#[test]
fn d1_tombstone_validation_is_typed_exact_and_rejects_unsafe_identity() {
    let key = [52u8; 32];
    let valid = SyncPayload::Delete(DeletePayload {
        node_id: "node-delete".into(),
        rel_path: "notes/dead.md".into(),
    });
    let (encrypted, hash) = encode_for_validation(&key, &valid);
    assert_eq!(
        validate_and_parse_remote_entry(&encrypted, &hash, &key, &SyncEntryKind::Delete),
        Ok(valid.clone())
    );
    assert_eq!(
        validate_and_parse_remote_entry(&encrypted, &hash, &key, &SyncEntryKind::Upsert),
        Err(InboxApplyFailureKind::Corrupt)
    );

    let invalid = vec![
        DeletePayload {
            node_id: String::new(),
            rel_path: "notes/dead.md".into(),
        },
        DeletePayload {
            node_id: "   ".into(),
            rel_path: "notes/dead.md".into(),
        },
        DeletePayload {
            node_id: "node\0delete".into(),
            rel_path: "notes/dead.md".into(),
        },
        DeletePayload {
            node_id: "n".repeat(129),
            rel_path: "notes/dead.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: String::new(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "   ".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "../escape.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "/absolute.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "notes\\windows-escape.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "C:/drive-escape.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "notes//empty-segment.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "notes/./dot-segment.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "notes/\0nul.md".into(),
        },
        DeletePayload {
            node_id: "node-delete".into(),
            rel_path: "p".repeat(16_385),
        },
    ];
    for tombstone in invalid {
        let payload = SyncPayload::Delete(tombstone.clone());
        let (encrypted, hash) = encode_for_validation(&key, &payload);
        assert_eq!(
            validate_and_parse_remote_entry(&encrypted, &hash, &key, &SyncEntryKind::Delete),
            Err(InboxApplyFailureKind::Corrupt),
            "unsafe tombstone was accepted: {tombstone:?}"
        );
    }

    let mut trailing = postcard::to_stdvec(&valid).unwrap();
    trailing.push(0x7f);
    let encrypted = crate::sync::core::crypto::encrypt_v5(&key, &trailing, true).unwrap();
    let hash = *blake3::hash(&encrypted).as_bytes();
    assert_eq!(
        validate_and_parse_remote_entry(&encrypted, &hash, &key, &SyncEntryKind::Delete),
        Err(InboxApplyFailureKind::Corrupt)
    );
}
