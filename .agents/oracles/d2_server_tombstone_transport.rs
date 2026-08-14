use super::*;

async fn setup_d2_mailbox() -> (MailboxHandler, tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = Database::open(&dir.path().join("d2.db")).expect("open DB");
    let vault_hash = hex::encode([0xD2; 32]);
    db.register_vault(&vault_hash, &[0xA2; 32], 100 * 1024 * 1024)
        .expect("register vault");
    let config = crate::config::AppConfig {
        quic_port: 0,
        health_port: 0,
        bind_addr: "127.0.0.1".into(),
        data_dir: dir.path().to_path_buf(),
        default_max_vault_bytes: 100 * 1024 * 1024,
        cleanup_interval_secs: 3600,
        max_entry_age_secs: 86400 * 30,
    };
    let server = MailboxHandler::new(db, config)
        .await
        .expect("create MailboxHandler");
    (server, dir, vault_hash)
}

fn delete_item(
    operation_id: [u8; 16],
    doc_hash: [u8; 32],
    encrypted_payload: Vec<u8>,
    payload_hash: [u8; 32],
) -> synabit_protocol::PushBatchItem {
    synabit_protocol::PushBatchItem {
        operation_id,
        doc_hash,
        entry_kind: synabit_protocol::SyncEntryKind::Delete,
        encrypted_payload,
        payload_hash,
    }
}

async fn push_delete_batch(
    server: &MailboxHandler,
    vault_hash: &str,
    item: synabit_protocol::PushBatchItem,
) -> MailboxResponse {
    server
        .handle_request(
            vault_hash,
            "d2-source-device",
            MailboxRequest::PushBatch { items: vec![item] },
        )
        .await
        .expect("PushBatch request must produce a protocol response")
}

fn assert_empty_server_state(server: &MailboxHandler, vault_hash: &str) {
    assert_eq!(
        server.db.current_seq(vault_hash).expect("read sequence"),
        0,
        "a rejected tombstone must not consume a sequence"
    );
    assert_eq!(
        server
            .db
            .total_vault_storage(vault_hash)
            .expect("read quota usage"),
        0,
        "a rejected tombstone must not charge quota"
    );
    let blob_count = std::fs::read_dir(&server.blob_dir)
        .expect("read blob directory")
        .count();
    assert_eq!(
        blob_count, 0,
        "a rejected tombstone must not leave a blob or temp file"
    );
}

#[tokio::test]
async fn d2_tombstone_round_trip_and_retry_preserve_exact_ciphertext() {
    let (server, _dir, vault_hash) = setup_d2_mailbox().await;
    let operation_id = [0x21; 16];
    let doc_hash = [0x42; 32];
    let encrypted_payload = vec![0x91, 0x02, 0xA7, 0x00, 0x5E, 0xFF, 0x18];
    let payload_hash = *blake3::hash(&encrypted_payload).as_bytes();

    let first = push_delete_batch(
        &server,
        &vault_hash,
        delete_item(
            operation_id,
            doc_hash,
            encrypted_payload.clone(),
            payload_hash,
        ),
    )
    .await;
    match first {
        MailboxResponse::PushBatchOk { max_seq, results } => {
            assert_eq!(max_seq, 1);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].operation_id, operation_id);
            assert_eq!(results[0].error, None);
        }
        other => panic!("expected successful PushBatch, got {other:?}"),
    }

    assert_eq!(
        server
            .db
            .total_vault_storage(&vault_hash)
            .expect("read quota usage"),
        encrypted_payload.len() as u64,
        "the stored tombstone ciphertext must count toward quota"
    );
    let blob_paths: Vec<_> = std::fs::read_dir(&server.blob_dir)
        .expect("read blob directory")
        .map(|entry| entry.expect("blob directory entry").path())
        .collect();
    assert_eq!(blob_paths.len(), 1, "one logical tombstone stores one blob");
    assert_eq!(
        std::fs::read(&blob_paths[0]).expect("read tombstone blob"),
        encrypted_payload,
        "the durable blob must contain the exact opaque ciphertext"
    );

    let exact_retry = push_delete_batch(
        &server,
        &vault_hash,
        delete_item(
            operation_id,
            doc_hash,
            encrypted_payload.clone(),
            payload_hash,
        ),
    )
    .await;
    match exact_retry {
        MailboxResponse::PushBatchOk { max_seq, results } => {
            assert_eq!(
                max_seq, 1,
                "an exact retry must reuse the original sequence"
            );
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].error, None);
        }
        other => panic!("expected idempotent PushBatch success, got {other:?}"),
    }
    assert_eq!(
        server.db.current_seq(&vault_hash).expect("read sequence"),
        1
    );
    assert_eq!(
        server
            .db
            .total_vault_storage(&vault_hash)
            .expect("read quota usage"),
        encrypted_payload.len() as u64,
        "an exact retry must not double-charge quota"
    );
    assert_eq!(
        std::fs::read_dir(&server.blob_dir)
            .expect("read blob directory")
            .count(),
        1,
        "an exact retry must not create another blob"
    );

    let corrupted_retry = push_delete_batch(
        &server,
        &vault_hash,
        delete_item(
            operation_id,
            doc_hash,
            vec![0x91, 0x02, 0xA7, 0x00, 0x5E, 0xFE, 0x18],
            payload_hash,
        ),
    )
    .await;
    match corrupted_retry {
        MailboxResponse::PushBatchOk { max_seq, results } => {
            assert_eq!(max_seq, 0);
            assert_eq!(results.len(), 1);
            assert!(
                results[0].error.is_some(),
                "idempotency must not acknowledge bytes that fail their supplied hash"
            );
        }
        other => panic!("expected per-item PushBatch rejection, got {other:?}"),
    }
    assert_eq!(
        server.db.current_seq(&vault_hash).expect("read sequence"),
        1
    );

    let paged = server
        .handle_request(
            &vault_hash,
            "d2-reader",
            MailboxRequest::PullPage {
                after_seq: 0,
                until_seq: 1,
                max_entries: 10,
                max_bytes: 1024,
            },
        )
        .await
        .expect("paged pull request");
    match paged {
        MailboxResponse::PullPageResult(page) => {
            assert_eq!(page.entries.len(), 1);
            assert_eq!(page.next_seq, 1);
            assert!(!page.has_more);
            let entry = &page.entries[0];
            assert_eq!(entry.seq, 1);
            assert_eq!(entry.operation_id, operation_id);
            assert_eq!(entry.doc_hash, doc_hash);
            assert_eq!(entry.entry_kind, synabit_protocol::SyncEntryKind::Delete);
            assert_eq!(entry.source_device, "d2-source-device");
            assert_eq!(entry.encrypted_payload, encrypted_payload);
            assert_eq!(entry.payload_hash, payload_hash);
        }
        other => panic!("expected PullPageResult, got {other:?}"),
    }

    let legacy = server
        .handle_request(
            &vault_hash,
            "d2-reader",
            MailboxRequest::Pull { since_seq: 0 },
        )
        .await
        .expect("legacy pull request");
    match legacy {
        MailboxResponse::PullResult { entries } => {
            assert_eq!(entries.len(), 1);
            let entry = &entries[0];
            assert_eq!(entry.seq, 1);
            assert_eq!(entry.operation_id, operation_id);
            assert_eq!(entry.doc_hash, doc_hash);
            assert_eq!(entry.entry_kind, synabit_protocol::SyncEntryKind::Delete);
            assert_eq!(entry.encrypted_payload, encrypted_payload);
            assert_eq!(entry.payload_hash, payload_hash);
        }
        other => panic!("expected PullResult, got {other:?}"),
    }
}

#[tokio::test]
async fn d2_tombstone_rejects_invalid_or_payloadless_writes_without_mutation() {
    let (bad_hash_server, _bad_hash_dir, bad_hash_vault) = setup_d2_mailbox().await;
    let bad_hash_response = bad_hash_server
        .handle_request(
            &bad_hash_vault,
            "d2-source-device",
            MailboxRequest::Push {
                operation_id: [0x31; 16],
                entry_kind: synabit_protocol::SyncEntryKind::Delete,
                doc_hash: [0x32; 32],
                encrypted_payload: vec![1, 2, 3, 4],
                payload_hash: [0; 32],
            },
        )
        .await
        .expect("bad-hash request must produce a protocol response");
    assert!(
        matches!(bad_hash_response, MailboxResponse::Error { .. }),
        "a tombstone with mismatched ciphertext/hash must be rejected"
    );
    assert_empty_server_state(&bad_hash_server, &bad_hash_vault);

    let (empty_server, _empty_dir, empty_vault) = setup_d2_mailbox().await;
    let empty_payload = Vec::new();
    let empty_response = empty_server
        .handle_request(
            &empty_vault,
            "d2-source-device",
            MailboxRequest::Push {
                operation_id: [0x41; 16],
                entry_kind: synabit_protocol::SyncEntryKind::Delete,
                doc_hash: [0x42; 32],
                payload_hash: *blake3::hash(&empty_payload).as_bytes(),
                encrypted_payload: empty_payload,
            },
        )
        .await
        .expect("empty request must produce a protocol response");
    assert!(
        matches!(empty_response, MailboxResponse::Error { .. }),
        "a payloadless tombstone cannot carry the D1 typed encrypted identity"
    );
    assert_empty_server_state(&empty_server, &empty_vault);

    let (legacy_server, _legacy_dir, legacy_vault) = setup_d2_mailbox().await;
    let legacy_response = legacy_server
        .handle_request(
            &legacy_vault,
            "d2-source-device",
            MailboxRequest::PushDelete {
                operation_id: [0x51; 16],
                doc_hash: [0x52; 32],
            },
        )
        .await
        .expect("legacy request must produce a protocol response");
    assert!(
        matches!(legacy_response, MailboxResponse::Error { .. }),
        "the payloadless legacy PushDelete route must fail closed"
    );
    assert_empty_server_state(&legacy_server, &legacy_vault);
}

#[tokio::test]
async fn d2_tombstone_pull_bounds_and_missing_blob_fail_closed() {
    let (server, _dir, vault_hash) = setup_d2_mailbox().await;
    let encrypted_payload = vec![0xC1, 0x02, 0x03, 0x04, 0x05, 0x06];
    let payload_hash = *blake3::hash(&encrypted_payload).as_bytes();
    let operation_id = [0x61; 16];
    let doc_hash = [0x62; 32];

    let response = server
        .handle_request(
            &vault_hash,
            "d2-source-device",
            MailboxRequest::Push {
                operation_id,
                entry_kind: synabit_protocol::SyncEntryKind::Delete,
                doc_hash,
                encrypted_payload: encrypted_payload.clone(),
                payload_hash,
            },
        )
        .await
        .expect("valid push request");
    assert!(matches!(response, MailboxResponse::PushOk { seq: 1 }));

    let bounded = server
        .handle_request(
            &vault_hash,
            "d2-reader",
            MailboxRequest::PullPage {
                after_seq: 0,
                until_seq: 1,
                max_entries: 1,
                max_bytes: (encrypted_payload.len() - 1) as u32,
            },
        )
        .await
        .expect("bounded pull request");
    assert!(
        matches!(bounded, MailboxResponse::Error { .. }),
        "the first oversized tombstone must not be returned empty or advance a cursor"
    );

    let adequate = server
        .handle_request(
            &vault_hash,
            "d2-reader",
            MailboxRequest::PullPage {
                after_seq: 0,
                until_seq: 1,
                max_entries: 1,
                max_bytes: encrypted_payload.len() as u32,
            },
        )
        .await
        .expect("adequately bounded pull request");
    match adequate {
        MailboxResponse::PullPageResult(page) => {
            assert_eq!(page.next_seq, 1);
            assert_eq!(page.entries.len(), 1);
            assert_eq!(page.entries[0].encrypted_payload, encrypted_payload);
        }
        other => panic!("expected PullPageResult, got {other:?}"),
    }

    let blob_path = std::fs::read_dir(&server.blob_dir)
        .expect("read blob directory")
        .next()
        .expect("stored tombstone blob")
        .expect("blob directory entry")
        .path();
    std::fs::remove_file(&blob_path).expect("remove blob to simulate storage loss");

    let paged_missing = server
        .handle_request(
            &vault_hash,
            "d2-reader",
            MailboxRequest::PullPage {
                after_seq: 0,
                until_seq: 1,
                max_entries: 1,
                max_bytes: 1024,
            },
        )
        .await;
    assert!(
        paged_missing.is_err(),
        "paged pull must fail closed when durable tombstone bytes are unavailable"
    );

    let legacy_missing = server
        .handle_request(
            &vault_hash,
            "d2-reader",
            MailboxRequest::Pull { since_seq: 0 },
        )
        .await;
    assert!(
        legacy_missing.is_err(),
        "legacy pull must fail closed when durable tombstone bytes are unavailable"
    );
}
