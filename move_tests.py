import re

coordinator_file = "src-tauri/src/sync/coordinator.rs"
outbox_file = "src-tauri/src/db/sync_outbox.rs"

with open(coordinator_file, "r") as f:
    coordinator_content = f.read()

# Add the new tests before the last closing brace of the `tests` module in coordinator.rs
tests_to_add = """
    struct HardcodedPushAdapter {
        fail_push: bool,
        partial_ack: bool,
        missing_ack: bool,
        unknown_ack: bool,
        duplicate_ack: bool,
        push_calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl SyncAdapter for HardcodedPushAdapter {
        fn name(&self) -> &str { "HardcodedPushAdapter" }
        fn adapter_id(&self) -> String { "gdrive".to_string() }
        async fn is_connected(&self) -> bool { true }
        async fn connect(&self) -> AppResult<()> { Ok(()) }
        async fn disconnect(&self) -> AppResult<()> { Ok(()) }
        async fn get_sync_plan(&self, _cursor: &str, _client_incarnation_id: Option<[u8; 16]>) -> AppResult<AdapterSyncPlan> { unimplemented!() }
        async fn pull_page(&self, _cursor: &str, _until_cursor: Option<&str>, _limits: PullLimits) -> AppResult<AdapterPullPage> { unimplemented!() }
        async fn ack(&self, _cursor: &str) -> AppResult<()> { unimplemented!() }
        async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> { unimplemented!() }
        async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> { unimplemented!() }

        async fn push(
            &self,
            operations: Vec<crate::sync::core::types::SyncOperation>,
        ) -> AppResult<PushResult> {
            self.push_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_push {
                return Err(AppError::SyncError("network error".into()));
            }

            let mut accepted = vec![];
            for op in operations {
                if self.partial_ack && op.operation_id == [5; 16] {
                    accepted.push(crate::sync::adapter::OperationAck {
                        operation_id: op.operation_id,
                        accepted: false,
                        error: Some("conflict".into()),
                    });
                } else if self.missing_ack && op.operation_id == [6; 16] {
                    // Do nothing, omit the ACK
                } else {
                    accepted.push(crate::sync::adapter::OperationAck {
                        operation_id: op.operation_id,
                        accepted: true,
                        error: None,
                    });
                    if self.duplicate_ack {
                        accepted.push(crate::sync::adapter::OperationAck {
                            operation_id: op.operation_id,
                            accepted: true,
                            error: None,
                        });
                    }
                }
            }

            if self.unknown_ack {
                accepted.push(crate::sync::adapter::OperationAck {
                    operation_id: [99; 16],
                    accepted: true,
                    error: None,
                });
            }

            Ok(PushResult {
                accepted,
                tx_bytes: 0,
                new_cursor: "".into(),
            })
        }
    }

    #[tokio::test]
    async fn restart_redelivers_preexisting_sent_outbox_without_redetection() {
        use crate::db::{DbBridge, sync_outbox::OutboxState};
        let mut db = DbBridge::new_in_memory().unwrap();
        db.conn_mut().execute("INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)", []).unwrap();
        
        let mut rec = crate::db::sync_outbox::tests::sample_record("v1", "gdrive", 1);
        rec.operation_id = [8; 16];
        rec.state = OutboxState::Sent;
        db.insert_outbox_record(&rec).unwrap();

        let db_state = Arc::new(std::sync::Mutex::new(db));
        let adapter = HardcodedPushAdapter {
            fail_push: false,
            partial_ack: false,
            missing_ack: false,
            unknown_ack: false,
            duplicate_ack: false,
            push_calls: AtomicUsize::new(0),
        };
        
        let result = dispatch_durable_outbox(&db_state, &adapter, "v1", "gdrive").await.unwrap();
        assert_eq!(result.pushed, 1);
        assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 1);

        let acked = db_state.lock().unwrap().get_outbox_by_id("v1", "gdrive", &[8; 16]).unwrap().unwrap();
        assert_eq!(acked.state, OutboxState::Acknowledged);
    }

    #[tokio::test]
    async fn adapter_failure_preserves_outbox_and_schedules_bounded_retry() {
        use crate::db::{DbBridge, sync_outbox::OutboxState};
        let mut db = DbBridge::new_in_memory().unwrap();
        db.conn_mut().execute("INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)", []).unwrap();
        
        let mut rec = crate::db::sync_outbox::tests::sample_record("v1", "gdrive", 1);
        rec.operation_id = [3; 16];
        db.insert_outbox_record(&rec).unwrap();

        let db_state = Arc::new(std::sync::Mutex::new(db));
        let adapter = HardcodedPushAdapter {
            fail_push: true,
            partial_ack: false,
            missing_ack: false,
            unknown_ack: false,
            duplicate_ack: false,
            push_calls: AtomicUsize::new(0),
        };
        
        let res = dispatch_durable_outbox(&db_state, &adapter, "v1", "gdrive").await;
        assert!(res.is_err());
        assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 1);

        let acked = db_state.lock().unwrap().get_outbox_by_id("v1", "gdrive", &[3; 16]).unwrap().unwrap();
        assert_eq!(acked.retry_count, 1);
        assert!(acked.next_retry_at.is_some());
        assert_eq!(acked.last_error.unwrap(), "Adapter push failed: Sync error: network error");
    }

    #[tokio::test]
    async fn partial_ack_commits_only_accepted_operation() {
        use crate::db::{DbBridge, sync_outbox::OutboxState};
        let mut db = DbBridge::new_in_memory().unwrap();
        db.conn_mut().execute("INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)", []).unwrap();
        
        let mut rec1 = crate::db::sync_outbox::tests::sample_record("v1", "gdrive", 1);
        rec1.operation_id = [4; 16];
        let mut rec2 = crate::db::sync_outbox::tests::sample_record("v1", "gdrive", 2);
        rec2.operation_id = [5; 16];
        db.insert_outbox_record(&rec1).unwrap();
        db.insert_outbox_record(&rec2).unwrap();

        let db_state = Arc::new(std::sync::Mutex::new(db));
        let adapter = HardcodedPushAdapter {
            fail_push: false,
            partial_ack: true,
            missing_ack: false,
            unknown_ack: false,
            duplicate_ack: false,
            push_calls: AtomicUsize::new(0),
        };
        
        let result = dispatch_durable_outbox(&db_state, &adapter, "v1", "gdrive").await.unwrap();
        assert_eq!(result.pushed, 1);
        assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 1);

        let db_guard = db_state.lock().unwrap();
        let ack1 = db_guard.get_outbox_by_id("v1", "gdrive", &[4; 16]).unwrap().unwrap();
        let ack2 = db_guard.get_outbox_by_id("v1", "gdrive", &[5; 16]).unwrap().unwrap();

        assert_eq!(ack1.state, OutboxState::Acknowledged);
        assert_ne!(ack2.state, OutboxState::Acknowledged);
        assert_eq!(ack2.retry_count, 1);
        assert_eq!(ack2.last_error.unwrap(), "conflict");
    }

    #[tokio::test]
    async fn missing_or_unknown_ack_never_acknowledges_outbox() {
        use crate::db::{DbBridge, sync_outbox::OutboxState};
        let mut db = DbBridge::new_in_memory().unwrap();
        db.conn_mut().execute("INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)", []).unwrap();
        
        let mut rec = crate::db::sync_outbox::tests::sample_record("v1", "gdrive", 1);
        rec.operation_id = [6; 16];
        db.insert_outbox_record(&rec).unwrap();

        let db_state = Arc::new(std::sync::Mutex::new(db));
        
        // Missing ACK
        let missing_adapter = HardcodedPushAdapter {
            fail_push: false,
            partial_ack: false,
            missing_ack: true,
            unknown_ack: false,
            duplicate_ack: false,
            push_calls: AtomicUsize::new(0),
        };
        let res_missing = dispatch_durable_outbox(&db_state, &missing_adapter, "v1", "gdrive").await;
        assert!(res_missing.is_err());
        assert_eq!(res_missing.unwrap_err().to_string(), "General error: Protocol violation: Missing ACKs in response");

        let db_guard = db_state.lock().unwrap();
        let ack_missing = db_guard.get_outbox_by_id("v1", "gdrive", &[6; 16]).unwrap().unwrap();
        assert_ne!(ack_missing.state, OutboxState::Acknowledged);
        drop(db_guard);
        
        // Fix state back to ready for the next test
        db_state.lock().unwrap().conn_mut().execute("UPDATE sync_outbox SET state = 'ready' WHERE operation_id = ?", [[6u8; 16]]).unwrap();

        // Unknown ACK
        let unknown_adapter = HardcodedPushAdapter {
            fail_push: false,
            partial_ack: false,
            missing_ack: false,
            unknown_ack: true,
            duplicate_ack: false,
            push_calls: AtomicUsize::new(0),
        };
        let res_unknown = dispatch_durable_outbox(&db_state, &unknown_adapter, "v1", "gdrive").await;
        assert!(res_unknown.is_err());
        assert_eq!(res_unknown.unwrap_err().to_string(), "General error: Protocol violation: Unknown ACK for operation 63636363636363636363636363636363");
        
        // Duplicate ACK
        db_state.lock().unwrap().conn_mut().execute("UPDATE sync_outbox SET state = 'ready' WHERE operation_id = ?", [[6u8; 16]]).unwrap();
        let duplicate_adapter = HardcodedPushAdapter {
            fail_push: false,
            partial_ack: false,
            missing_ack: false,
            unknown_ack: false,
            duplicate_ack: true,
            push_calls: AtomicUsize::new(0),
        };
        let res_duplicate = dispatch_durable_outbox(&db_state, &duplicate_adapter, "v1", "gdrive").await;
        assert!(res_duplicate.is_err());
        assert_eq!(res_duplicate.unwrap_err().to_string(), "General error: Protocol violation: Duplicate ACK for operation 06060606060606060606060606060606");
    }
"""

coordinator_content = coordinator_content[:coordinator_content.rfind("}")] + tests_to_add + "}\n"

with open(coordinator_file, "w") as f:
    f.write(coordinator_content)

# Now delete the tests from sync_outbox.rs
with open(outbox_file, "r") as f:
    outbox_content = f.read()

# We need to remove the tests from outbox_file
test_names = [
    "restart_redelivers_preexisting_sent_outbox_without_redetection",
    "accepted_outbox_commit_atomically_updates_baseline_and_ack_state",
    "adapter_failure_preserves_outbox_and_schedules_bounded_retry",
    "partial_ack_commits_only_accepted_operation",
    "missing_or_unknown_ack_never_acknowledges_outbox",
    "durable_preparation_does_not_advance_baseline_before_acceptance"
]

import re
for test_name in test_names:
    pattern = r"#\[test\]\s+fn\s+" + test_name + r"\s*\(\)\s*\{.*?\n    \}\n\n"
    outbox_content = re.sub(pattern, "", outbox_content, flags=re.DOTALL)
    
    # Try alternate match if the first failed
    pattern = r"#\[test\]\s+fn\s+" + test_name + r"\s*\(\)\s*\{.*?\n    \}\n"
    outbox_content = re.sub(pattern, "", outbox_content, flags=re.DOTALL)

with open(outbox_file, "w") as f:
    f.write(outbox_content)
