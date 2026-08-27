use super::*;
use crate::db::sync_inbox::InboxState;
use crate::db::sync_vault::SyncVaultRecord;
use crate::sync::adapter::{AdapterPullPage, RemoteEntry};
use crate::sync::core::types::AssetRef;
use async_trait::async_trait;
use rusqlite::params;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

const VAULT: &str = "c2b-oracle-vault";
const PROVIDER: &str = "c2b-oracle-provider";
const NOW: i64 = 10_000;

fn seed_vault(db: &mut DbBridge, vault_id: &str) {
    db.insert_sync_vault_mapping(&SyncVaultRecord {
        vault_id: vault_id.to_string(),
        canonical_root: format!("/tmp/{vault_id}"),
        metadata_version: 1,
        created_at: NOW,
        updated_at: NOW,
    })
    .unwrap();
}

fn seeded_db() -> Arc<DbState> {
    let mut db = DbBridge::new_in_memory().unwrap();
    seed_vault(&mut db, VAULT);
    db.ensure_sync_provider_state(VAULT, PROVIDER).unwrap();
    Arc::new(Mutex::new(db))
}

fn valid_outbox(operation_id: [u8; 16], state: OutboxState) -> OutboxRecord {
    let encrypted_payload = vec![operation_id[0], 2, 3, 4];
    OutboxRecord {
        vault_id: VAULT.to_string(),
        provider_id: PROVIDER.to_string(),
        operation_id,
        entry_kind: SyncEntryKind::Upsert,
        node_id: format!("node-{}", operation_id[0]),
        rel_path: Some(format!("doc-{}.md", operation_id[0])),
        doc_hash: Some([operation_id[0]; 32]),
        source_hash: Some([operation_id[0].wrapping_add(1); 32]),
        original_timestamp: NOW,
        payload_hash: Some(*blake3::hash(&encrypted_payload).as_bytes()),
        encrypted_payload: Some(encrypted_payload),
        asset_ref_blob: None,
        state,
        retry_count: 0,
        next_retry_at: None,
        last_error: None,
        created_at: NOW + i64::from(operation_id[0]),
        updated_at: NOW + i64::from(operation_id[0]),
    }
}

fn insert_outbox(db_state: &Arc<DbState>, records: &[OutboxRecord]) {
    let db = db_state.lock().unwrap();
    for record in records {
        db.insert_outbox_record(record).unwrap();
    }
}

fn outbox(db_state: &Arc<DbState>, operation_id: [u8; 16]) -> OutboxRecord {
    db_state
        .lock()
        .unwrap()
        .get_outbox_by_id(VAULT, PROVIDER, &operation_id)
        .unwrap()
        .unwrap()
}

#[derive(Clone)]
enum PushBehavior {
    AcceptAll { tx_bytes: u64 },
    Reply(PushResult),
    Fail(String),
}

struct HarnessAdapter {
    push_behavior: PushBehavior,
    plan_mode: AdapterSyncMode,
    pages: Mutex<VecDeque<AdapterPullPage>>,
    captured_pushes: Mutex<Vec<Vec<SyncOperation>>>,
    events: Arc<Mutex<Vec<String>>>,
    push_calls: AtomicU32,
    pull_calls: AtomicU32,
    ack_calls: AtomicU32,
    ack_failures_remaining: AtomicU32,
    db_observed_by_ack: Option<Arc<DbState>>,
}

impl HarnessAdapter {
    fn new(push_behavior: PushBehavior) -> Self {
        Self {
            push_behavior,
            plan_mode: AdapterSyncMode::Delta {
                until_cursor: Some(String::new()),
            },
            pages: Mutex::new(VecDeque::new()),
            captured_pushes: Mutex::new(Vec::new()),
            events: Arc::new(Mutex::new(Vec::new())),
            push_calls: AtomicU32::new(0),
            pull_calls: AtomicU32::new(0),
            ack_calls: AtomicU32::new(0),
            ack_failures_remaining: AtomicU32::new(0),
            db_observed_by_ack: None,
        }
    }

    fn with_pages(mut self, pages: Vec<AdapterPullPage>) -> Self {
        self.pages = Mutex::new(pages.into());
        self
    }

    fn with_db_observation(mut self, db_state: Arc<DbState>) -> Self {
        self.db_observed_by_ack = Some(db_state);
        self
    }

    fn with_ack_failures(mut self, count: u32) -> Self {
        self.ack_failures_remaining = AtomicU32::new(count);
        self
    }
}

#[async_trait]
impl SyncAdapter for HarnessAdapter {
    fn name(&self) -> &str {
        "C2B immutable oracle adapter"
    }

    fn adapter_id(&self) -> String {
        PROVIDER.to_string()
    }

    async fn is_connected(&self) -> bool {
        true
    }

    async fn connect(&self) -> AppResult<()> {
        Ok(())
    }

    async fn disconnect(&self) -> AppResult<()> {
        Ok(())
    }

    async fn push(&self, operations: Vec<SyncOperation>) -> AppResult<PushResult> {
        self.push_calls.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push("push".to_string());
        self.captured_pushes
            .lock()
            .unwrap()
            .push(operations.clone());

        match &self.push_behavior {
            PushBehavior::AcceptAll { tx_bytes } => Ok(PushResult {
                accepted: operations
                    .iter()
                    .enumerate()
                    .map(|(index, operation)| PushAck {
                        operation_id: operation.operation_id,
                        remote_position: format!("native-ack-{index}"),
                        remote_seq: Some(index as u64 + 1),
                    })
                    .collect(),
                rejected: Vec::new(),
                tx_bytes: *tx_bytes,
            }),
            PushBehavior::Reply(result) => Ok(result.clone()),
            PushBehavior::Fail(message) => Err(AppError::General(message.clone())),
        }
    }

    async fn get_sync_plan(
        &self,
        _cursor: &str,
        _client_incarnation_id: Option<[u8; 16]>,
    ) -> AppResult<AdapterSyncPlan> {
        Ok(AdapterSyncPlan {
            mode: self.plan_mode.clone(),
            incarnation_id: None,
            remote_vault_id: None,
        })
    }

    async fn pull_page(
        &self,
        cursor: &str,
        _until_cursor: Option<&str>,
        _limits: PullLimits,
    ) -> AppResult<AdapterPullPage> {
        self.pull_calls.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push("pull".to_string());
        Ok(self
            .pages
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| AdapterPullPage {
                entries: Vec::new(),
                next_cursor: cursor.to_string(),
                has_more: false,
                rx_bytes: 0,
            }))
    }

    async fn ack(&self, cursor: &str) -> AppResult<()> {
        self.ack_calls.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push("ack".to_string());

        if let Some(db_state) = &self.db_observed_by_ack {
            let db = db_state.lock().unwrap();
            let state = db
                .get_sync_provider_state(VAULT, PROVIDER)?
                .ok_or_else(|| AppError::General("provider state missing at ACK".into()))?;
            if state.cursor != cursor {
                return Err(AppError::General(format!(
                    "remote ACK occurred before local cursor commit: local={} remote={cursor}",
                    state.cursor
                )));
            }
        }

        let remaining = self.ack_failures_remaining.load(Ordering::SeqCst);
        if remaining > 0 {
            self.ack_failures_remaining.fetch_sub(1, Ordering::SeqCst);
            return Err(AppError::General("injected remote ACK failure".into()));
        }
        Ok(())
    }

    async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> {
        Ok(())
    }

    async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
        Ok(None)
    }
}

fn ack(operation_id: [u8; 16], suffix: &str) -> PushAck {
    PushAck {
        operation_id,
        remote_position: format!("native-{suffix}"),
        remote_seq: Some(u64::from(operation_id[0])),
    }
}

#[tokio::test]
async fn frc2b_r1_01_adapter_failure_retries_the_whole_batch_once() {
    let db_state = seeded_db();
    let first = [1; 16];
    let second = [2; 16];
    insert_outbox(
        &db_state,
        &[
            valid_outbox(first, OutboxState::Ready),
            valid_outbox(second, OutboxState::Ready),
        ],
    );
    let adapter = HarnessAdapter::new(PushBehavior::Fail("wire unavailable".to_string()));

    let error = dispatch_durable_outbox_at(&db_state, VAULT, PROVIDER, &adapter, 100, NOW + 100)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("wire unavailable"));

    for operation_id in [first, second] {
        let record = outbox(&db_state, operation_id);
        assert_eq!(record.state, OutboxState::Failed);
        assert_eq!(record.retry_count, 1);
        assert!(record.next_retry_at.is_some());
        assert_eq!(record.last_error.as_deref(), Some("wire unavailable"));
    }
    assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn frc2b_r1_01_invalid_outcome_sets_retry_every_sent_member() {
    enum InvalidCase {
        Missing,
        Unknown,
        Duplicate,
    }

    for (case_index, case) in [
        InvalidCase::Missing,
        InvalidCase::Unknown,
        InvalidCase::Duplicate,
    ]
    .into_iter()
    .enumerate()
    {
        let db_state = seeded_db();
        let first = [10 + case_index as u8; 16];
        let second = [20 + case_index as u8; 16];
        insert_outbox(
            &db_state,
            &[
                valid_outbox(first, OutboxState::Ready),
                valid_outbox(second, OutboxState::Ready),
            ],
        );

        let accepted = match case {
            InvalidCase::Missing => vec![ack(first, "accepted")],
            InvalidCase::Unknown => vec![ack([99; 16], "unknown")],
            InvalidCase::Duplicate => vec![ack(first, "first"), ack(first, "duplicate")],
        };
        let adapter = HarnessAdapter::new(PushBehavior::Reply(PushResult {
            accepted,
            rejected: Vec::new(),
            tx_bytes: 55,
        }));

        let result =
            dispatch_durable_outbox_at(&db_state, VAULT, PROVIDER, &adapter, 100, NOW + 100).await;
        assert!(
            result.is_err(),
            "malformed outcome must be a protocol error"
        );
        for operation_id in [first, second] {
            let record = outbox(&db_state, operation_id);
            assert_eq!(record.state, OutboxState::Failed);
            assert_eq!(record.retry_count, 1);
            assert!(record
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("protocol"));
        }
    }
}

#[tokio::test]
async fn frc2b_r1_01_mixed_outcome_commits_accept_and_retries_reject() {
    let db_state = seeded_db();
    let accepted_id = [31; 16];
    let rejected_id = [32; 16];
    insert_outbox(
        &db_state,
        &[
            valid_outbox(accepted_id, OutboxState::Ready),
            valid_outbox(rejected_id, OutboxState::Ready),
        ],
    );
    let adapter = HarnessAdapter::new(PushBehavior::Reply(PushResult {
        accepted: vec![ack(accepted_id, "accepted")],
        rejected: vec![ack(rejected_id, "rejected")],
        tx_bytes: 777,
    }));

    let outcome = dispatch_durable_outbox_at(&db_state, VAULT, PROVIDER, &adapter, 100, NOW + 100)
        .await
        .unwrap();
    let serialized = serde_json::to_value(outcome).unwrap();
    assert_eq!(serialized.get("acknowledged"), Some(&Value::from(1)));
    assert_eq!(serialized.get("tx_bytes"), Some(&Value::from(777)));

    let accepted = outbox(&db_state, accepted_id);
    assert_eq!(accepted.state, OutboxState::Acknowledged);
    assert_eq!(accepted.retry_count, 0);
    let rejected = outbox(&db_state, rejected_id);
    assert_eq!(rejected.state, OutboxState::Failed);
    assert_eq!(rejected.retry_count, 1);
    assert!(rejected.last_error.as_deref().unwrap().contains("rejected"));
}

#[tokio::test]
async fn frc2b_r1_01_incomplete_row_is_quarantined_while_exact_wire_row_dispatches() {
    let db_state = seeded_db();
    let incomplete_id = [41; 16];
    let valid_id = [42; 16];
    insert_outbox(&db_state, &[valid_outbox(valid_id, OutboxState::Ready)]);
    {
        let db = db_state.lock().unwrap();
        db.conn()
            .execute(
                "INSERT INTO sync_outbox (vault_id, provider_id, operation_id, entry_kind, node_id, rel_path, doc_hash, source_hash, original_timestamp, encrypted_payload, payload_hash, asset_ref_blob, state, retry_count, next_retry_at, last_error, created_at, updated_at) VALUES (?1, ?2, ?3, 'upsert', 'bad-node', NULL, ?4, ?5, ?6, ?7, ?8, NULL, 'ready', 0, NULL, NULL, ?9, ?9)",
                params![
                    VAULT,
                    PROVIDER,
                    incomplete_id.as_slice(),
                    [1u8; 32].as_slice(),
                    [2u8; 32].as_slice(),
                    NOW,
                    vec![1u8],
                    [3u8; 32].as_slice(),
                    NOW - 1,
                ],
            )
            .unwrap();
    }

    let adapter = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 19 });
    dispatch_durable_outbox_at(&db_state, VAULT, PROVIDER, &adapter, 100, NOW + 100)
        .await
        .unwrap();

    let bad = outbox(&db_state, incomplete_id);
    assert_eq!(bad.state, OutboxState::Failed);
    assert_eq!(bad.retry_count, 0);
    assert_eq!(bad.next_retry_at, None);

    let good = outbox(&db_state, valid_id);
    assert_eq!(good.state, OutboxState::Acknowledged);
    let captured = adapter.captured_pushes.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].len(), 1);
    let wire = &captured[0][0];
    let source = valid_outbox(valid_id, OutboxState::Ready);
    assert_eq!(wire.operation_id, source.operation_id);
    assert_eq!(wire.doc_hash, source.doc_hash.unwrap());
    assert_eq!(wire.entry_kind, source.entry_kind);
    assert_eq!(wire.node_id, source.node_id);
    assert_eq!(wire.rel_path, source.rel_path.unwrap());
    assert_eq!(wire.encrypted_payload, source.encrypted_payload.unwrap());
    assert_eq!(wire.payload_hash, source.payload_hash.unwrap());
    assert_eq!(wire.timestamp, source.original_timestamp);
}

#[tokio::test]
async fn frc2b_r1_01_retry_persistence_error_keeps_network_and_database_context() {
    let db_state = seeded_db();
    let operation_id = [51; 16];
    insert_outbox(&db_state, &[valid_outbox(operation_id, OutboxState::Ready)]);
    {
        let db = db_state.lock().unwrap();
        db.conn()
            .execute_batch(
                "CREATE TRIGGER c2b_oracle_fail_retry
                 BEFORE UPDATE OF state ON sync_outbox
                 WHEN NEW.state = 'failed'
                 BEGIN
                   SELECT RAISE(ABORT, 'injected retry persistence failure');
                 END;",
            )
            .unwrap();
    }
    let adapter = HarnessAdapter::new(PushBehavior::Fail("original network failure".to_string()));

    let error = dispatch_durable_outbox_at(&db_state, VAULT, PROVIDER, &adapter, 100, NOW + 100)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("original network failure"), "{error}");
    assert!(
        error.contains("injected retry persistence failure"),
        "{error}"
    );
}

fn encrypted_typed_payload(payload: &SyncPayload, key: &[u8; 32]) -> (Vec<u8>, [u8; 32]) {
    let encoded = postcard::to_allocvec(payload).unwrap();
    let encrypted = crate::sync::core::crypto::encrypt(key, &encoded).unwrap();
    let hash = *blake3::hash(&encrypted).as_bytes();
    (encrypted, hash)
}

fn typed_delete(node_id: &str, rel_path: &str) -> SyncPayload {
    SyncPayload::Delete(synabit_protocol::DeletePayload {
        node_id: node_id.to_string(),
        rel_path: rel_path.to_string(),
    })
}

#[test]
fn frc2b_r1_02_kind_is_checked_only_after_exact_typed_decode() {
    let key = [7; 32];
    let garbage = crate::sync::core::crypto::encrypt(&key, b"not-a-sync-payload").unwrap();
    let garbage_hash = *blake3::hash(&garbage).as_bytes();

    assert_eq!(
        validate_and_parse_remote_entry(
            &garbage,
            &garbage_hash,
            &key,
            &SyncEntryKind::AssetReference,
        ),
        Err(InboxApplyFailureKind::Corrupt)
    );
    assert_eq!(
        validate_and_parse_remote_entry(&garbage, &garbage_hash, &key, &SyncEntryKind::Delete),
        Err(InboxApplyFailureKind::Corrupt)
    );
}

#[test]
fn frc2b_r1_02_kind_payload_mismatch_and_trailing_bytes_are_corrupt() {
    let key = [8; 32];
    let (delete_bytes, delete_hash) =
        encrypted_typed_payload(&typed_delete("typed-delete-node", "typed-delete.md"), &key);
    assert_eq!(
        validate_and_parse_remote_entry(&delete_bytes, &delete_hash, &key, &SyncEntryKind::Upsert,),
        Err(InboxApplyFailureKind::Corrupt)
    );

    let doc = DocSyncPayload {
        node_id: "typed-node".to_string(),
        rel_path: "typed.md".to_string(),
        snapshot: vec![1, 2, 3],
        is_json: false,
    };
    let mut encoded =
        postcard::to_allocvec(&SyncPayload::Upsert(postcard::to_allocvec(&doc).unwrap())).unwrap();
    encoded.extend_from_slice(&[0xde, 0xad]);
    let encrypted = crate::sync::core::crypto::encrypt(&key, &encoded).unwrap();
    let hash = *blake3::hash(&encrypted).as_bytes();
    assert_eq!(
        validate_and_parse_remote_entry(&encrypted, &hash, &key, &SyncEntryKind::Upsert),
        Err(InboxApplyFailureKind::Corrupt)
    );
}

#[test]
fn frc2b_r1_02_matching_asset_delete_and_upsert_have_distinct_results() {
    // Attachments used to be rejected here as PendingAsset, because nothing
    // could carry them. They are readable now; the bytes are fetched separately
    // before the page is applied.
    let key = [9; 32];
    let asset = AssetRef {
        asset_id: [1; 32],
        rel_path: "assets/x.bin".to_string(),
        node_id: "assets/x.bin".to_string(),
        mime_type: "application/octet-stream".to_string(),
        total_bytes: 0,
        plaintext_hash: [2; 32],
        chunks: Vec::new(),
    };
    let (asset_bytes, asset_hash) =
        encrypted_typed_payload(&SyncPayload::AssetReference(asset.clone()), &key);
    assert_eq!(
        validate_and_parse_remote_entry(
            &asset_bytes,
            &asset_hash,
            &key,
            &SyncEntryKind::AssetReference,
        ),
        Ok(SyncPayload::AssetReference(asset))
    );

    let (delete_bytes, delete_hash) =
        encrypted_typed_payload(&typed_delete("typed-delete-node", "typed-delete.md"), &key);
    assert_eq!(
        validate_and_parse_remote_entry(&delete_bytes, &delete_hash, &key, &SyncEntryKind::Delete),
        Ok(typed_delete("typed-delete-node", "typed-delete.md"))
    );
}
fn mock_app_handle() -> tauri::AppHandle<tauri::test::MockRuntime> {
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    app.handle().clone()
}

fn remote_upsert(
    operation_id: [u8; 16],
    position: &str,
    sequence: u64,
    snapshot: Vec<u8>,
    key: &[u8; 32],
) -> RemoteEntry {
    let doc = DocSyncPayload {
        node_id: format!("remote-node-{}", operation_id[0]),
        rel_path: format!("remote-{}.md", operation_id[0]),
        snapshot,
        is_json: false,
    };
    let (encrypted_payload, payload_hash) = encrypted_typed_payload(
        &SyncPayload::Upsert(postcard::to_allocvec(&doc).unwrap()),
        key,
    );
    RemoteEntry {
        remote_position: position.to_string(),
        remote_seq: Some(sequence),
        doc_hash: [operation_id[0]; 32],
        source_device: "remote-device".to_string(),
        encrypted_payload,
        payload_hash,
        timestamp: NOW + sequence as i64,
        operation_id,
        entry_kind: SyncEntryKind::Upsert,
    }
}

fn remote_typed(
    operation_id: [u8; 16],
    position: &str,
    sequence: u64,
    entry_kind: SyncEntryKind,
    payload: SyncPayload,
    key: &[u8; 32],
) -> RemoteEntry {
    let (encrypted_payload, payload_hash) = encrypted_typed_payload(&payload, key);
    RemoteEntry {
        remote_position: position.to_string(),
        remote_seq: Some(sequence),
        doc_hash: [operation_id[0]; 32],
        source_device: "remote-device".to_string(),
        encrypted_payload,
        payload_hash,
        timestamp: NOW + sequence as i64,
        operation_id,
        entry_kind,
    }
}

struct InspectingApplier {
    db_state: Arc<DbState>,
    page_cursor: String,
    snapshots: Mutex<Vec<Vec<u8>>>,
    events: Arc<Mutex<Vec<String>>>,
}

impl<R: tauri::Runtime> InboxEntryApplier<R> for InspectingApplier {
    fn apply(
        &self,
        _app_handle: &tauri::AppHandle<R>,
        _vault_path_obj: &Path,
        _vault_path: &str,
        payload: &DocSyncPayload,
        _result: &mut SyncResult,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<()> {
        let db = self.db_state.lock().unwrap();
        let page = db
            .get_inbox_page(vault_id, provider_id, &self.page_cursor)?
            .ok_or_else(|| AppError::General("apply ran before durable page stage".into()))?;
        if page.state.as_str() != "staged" {
            return Err(AppError::General(format!(
                "apply ran after unexpected page state {}",
                page.state.as_str()
            )));
        }
        drop(db);
        self.snapshots
            .lock()
            .unwrap()
            .push(payload.snapshot.clone());
        self.events.lock().unwrap().push("apply".to_string());
        Ok(())
    }

    fn apply_delete(
        &self,
        _app_handle: &tauri::AppHandle<R>,
        _vault_path_obj: &Path,
        payload: &synabit_protocol::DeletePayload,
        _remote_seq: Option<u64>,
        _result: &mut SyncResult,
        _vault_id: &str,
        _provider_id: &str,
    ) -> AppResult<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("apply_delete:{}", payload.rel_path));
        Ok(())
    }
}

fn pull_plan(until: &str) -> AdapterSyncPlan {
    AdapterSyncPlan {
        mode: AdapterSyncMode::Delta {
            until_cursor: Some(until.to_string()),
        },
        incarnation_id: None,
        remote_vault_id: None,
    }
}

fn pull_limits() -> PullLimits {
    PullLimits {
        max_entries: 20,
        max_bytes: 1_000_000,
    }
}

#[tokio::test]
async fn c2b_v3_pull_stage_apply_commit_ack_order_is_observable() {
    let db_state = seeded_db();
    let key = [11; 32];
    let page = AdapterPullPage {
        entries: vec![remote_upsert([61; 16], "server-seq-61", 61, vec![1], &key)],
        next_cursor: "cursor-1".to_string(),
        has_more: false,
        rx_bytes: 123,
    };
    let adapter = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 0 })
        .with_pages(vec![page])
        .with_db_observation(db_state.clone());
    let events = adapter.events.clone();
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events: events.clone(),
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();

    let rx = pull_pages_durable(
        &db_state,
        &adapter,
        VAULT,
        PROVIDER,
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &pull_plan("cursor-1"),
        pull_limits(),
        &mut result,
    )
    .await
    .unwrap();

    assert_eq!(rx, 123);
    assert_eq!(*events.lock().unwrap(), vec!["pull", "apply", "ack"]);
    let db = db_state.lock().unwrap();
    let state = db
        .get_sync_provider_state(VAULT, PROVIDER)
        .unwrap()
        .unwrap();
    assert_eq!(state.cursor, "cursor-1");
    assert_eq!(state.ack_cursor.as_deref(), Some("cursor-1"));
}

#[tokio::test]
async fn c2b_v3_pull_restart_resumes_staged_member_before_new_pull() {
    let db_state = seeded_db();
    let key = [12; 32];
    let remote = remote_upsert([62; 16], "server-seq-62", 62, vec![2], &key);
    {
        let db = db_state.lock().unwrap();
        db.stage_inbox_page(
            VAULT,
            PROVIDER,
            "",
            "cursor-staged",
            false,
            &[remote_entry_to_inbox_entry(&remote).unwrap()],
            NOW,
        )
        .unwrap();
    }
    let adapter = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 0 })
        .with_db_observation(db_state.clone());
    let events = adapter.events.clone();
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events: events.clone(),
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();

    pull_pages_durable(
        &db_state,
        &adapter,
        VAULT,
        PROVIDER,
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &pull_plan("cursor-staged"),
        pull_limits(),
        &mut result,
    )
    .await
    .unwrap();

    assert_eq!(*events.lock().unwrap(), vec!["apply", "ack", "pull"]);
}

#[test]
fn c2b_v3_pull_crash_left_applying_reapplies_once_to_terminal_state() {
    let db_state = seeded_db();
    let key = [13; 32];
    let operation_id = [63; 16];
    let remote = remote_upsert(operation_id, "server-seq-63", 63, vec![3], &key);
    {
        let db = db_state.lock().unwrap();
        db.stage_inbox_page(
            VAULT,
            PROVIDER,
            "",
            "cursor-applying",
            false,
            &[remote_entry_to_inbox_entry(&remote).unwrap()],
            NOW,
        )
        .unwrap();
        db.transition_inbox_state(
            VAULT,
            PROVIDER,
            &operation_id,
            InboxState::Pending,
            InboxState::Applying,
            None,
            NOW + 1,
        )
        .unwrap();
    }
    let events = Arc::new(Mutex::new(Vec::new()));
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events,
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();

    process_staged_inbox_page(
        &db_state,
        VAULT,
        PROVIDER,
        "",
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &mut result,
    )
    .unwrap();

    assert_eq!(applier.snapshots.lock().unwrap().as_slice(), &[vec![3]]);
    let inbox = db_state
        .lock()
        .unwrap()
        .get_inbox_by_id(VAULT, PROVIDER, &operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(inbox.state, InboxState::Applied);
    assert!(inbox.applied_at.is_some());
}

#[tokio::test]
async fn c2b_v3_pull_corrupt_middle_is_quarantined_and_later_members_still_apply() {
    // This previously asserted that a corrupt entry must stop everything behind
    // it and hold the cursor. That is what wedged a vault permanently: the bad
    // entry was re-encountered on every sync and nothing after it ever ran. A
    // corrupt payload is now quarantined and the page continues.
    let db_state = seeded_db();
    let key = [14; 32];
    let first = remote_upsert([64; 16], "p64", 64, vec![4], &key);
    let mut corrupt = remote_upsert([65; 16], "p65", 65, vec![5], &key);
    corrupt.payload_hash = [0xff; 32];
    let third = remote_upsert([66; 16], "p66", 66, vec![6], &key);
    let adapter = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 0 }).with_pages(vec![
        AdapterPullPage {
            entries: vec![first, corrupt, third],
            next_cursor: "cursor-corrupt".to_string(),
            has_more: false,
            rx_bytes: 99,
        },
    ]);
    let events = adapter.events.clone();
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events,
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();

    let result = pull_pages_durable(
        &db_state,
        &adapter,
        VAULT,
        PROVIDER,
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &pull_plan("cursor-corrupt"),
        pull_limits(),
        &mut result,
    )
    .await;
    assert!(
        result.is_ok(),
        "a corrupt entry must not fail the whole pull"
    );
    assert_eq!(
        applier.snapshots.lock().unwrap().as_slice(),
        &[vec![4], vec![6]],
        "the entry after the corrupt one must still be applied"
    );
    let db = db_state.lock().unwrap();
    assert_eq!(
        db.get_inbox_by_id(VAULT, PROVIDER, &[64; 16])
            .unwrap()
            .unwrap()
            .state,
        InboxState::Applied
    );
    assert_eq!(
        db.get_inbox_by_id(VAULT, PROVIDER, &[65; 16])
            .unwrap()
            .unwrap()
            .state,
        InboxState::Quarantined
    );
    assert_eq!(
        db.get_inbox_by_id(VAULT, PROVIDER, &[66; 16])
            .unwrap()
            .unwrap()
            .state,
        InboxState::Applied
    );
    assert_eq!(
        db.get_sync_provider_state(VAULT, PROVIDER)
            .unwrap()
            .unwrap()
            .cursor,
        "cursor-corrupt",
        "the cursor must advance past a quarantined entry"
    );
    assert!(adapter.ack_calls.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn c2b_v3_pull_ack_gap_is_retried_before_any_new_pull() {
    let db_state = seeded_db();
    let key = [15; 32];
    let adapter = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 0 })
        .with_pages(vec![AdapterPullPage {
            entries: vec![remote_upsert([67; 16], "p67", 67, vec![7], &key)],
            next_cursor: "cursor-ack-gap".to_string(),
            has_more: false,
            rx_bytes: 10,
        }])
        .with_db_observation(db_state.clone())
        .with_ack_failures(1);
    let events = adapter.events.clone();
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events: events.clone(),
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();

    let first = pull_pages_durable(
        &db_state,
        &adapter,
        VAULT,
        PROVIDER,
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &pull_plan("cursor-ack-gap"),
        pull_limits(),
        &mut result,
    )
    .await;
    assert!(first.is_err());
    {
        let db = db_state.lock().unwrap();
        let state = db
            .get_sync_provider_state(VAULT, PROVIDER)
            .unwrap()
            .unwrap();
        assert_eq!(state.cursor, "cursor-ack-gap");
        assert_eq!(state.ack_cursor, None);
    }

    events.lock().unwrap().clear();
    pull_pages_durable(
        &db_state,
        &adapter,
        VAULT,
        PROVIDER,
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &pull_plan("cursor-ack-gap"),
        pull_limits(),
        &mut result,
    )
    .await
    .unwrap();
    assert_eq!(*events.lock().unwrap(), vec!["ack", "pull"]);
}

#[test]
fn c2b_v3_pull_own_operation_evidence_is_scoped_and_unverified_source_applies() {
    let db_state = seeded_db();
    let key = [18; 32];
    let known_id = [70; 16];
    // A matching source_device label is not evidence of ownership. It is chosen
    // by the pusher, and installs that share an id (the Google Drive path used
    // the app bundle identifier) would otherwise discard every peer's work.
    // With nothing in our outbox, this operation is not ours.
    assert!(!is_verified_own_operation(
        &db_state,
        VAULT,
        PROVIDER,
        &known_id,
        Some("local-device"),
        "local-device",
    )
    .unwrap());
    assert!(!is_verified_own_operation(
        &db_state,
        VAULT,
        PROVIDER,
        &known_id,
        Some("other-device"),
        "local-device",
    )
    .unwrap());

    insert_outbox(&db_state, &[valid_outbox(known_id, OutboxState::Ready)]);
    assert!(
        is_verified_own_operation(&db_state, VAULT, PROVIDER, &known_id, None, "local-device",)
            .unwrap()
    );
    assert!(!is_verified_own_operation(
        &db_state,
        "different-vault",
        PROVIDER,
        &known_id,
        None,
        "local-device",
    )
    .unwrap());
    assert!(!is_verified_own_operation(
        &db_state,
        VAULT,
        "different-provider",
        &known_id,
        None,
        "local-device",
    )
    .unwrap());

    let unverified = remote_upsert([71; 16], "p71", 71, vec![9], &key);
    {
        let db = db_state.lock().unwrap();
        db.stage_inbox_page(
            VAULT,
            PROVIDER,
            "",
            "cursor-unverified",
            false,
            &[remote_entry_to_inbox_entry(&unverified).unwrap()],
            NOW,
        )
        .unwrap();
    }
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events: Arc::new(Mutex::new(Vec::new())),
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();
    process_staged_inbox_page(
        &db_state,
        VAULT,
        PROVIDER,
        "",
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &mut result,
    )
    .unwrap();
    assert_eq!(applier.snapshots.lock().unwrap().as_slice(), &[vec![9]]);
}

#[test]
fn c2b_v3_pull_asset_is_set_aside_while_delete_applies() {
    // Originally this asserted that a *valid* delete also had to end in
    // Failed/retryable. That froze a defect as a requirement: the coordinator
    // simply had no apply arm for SyncPayload::Delete. Deletes now apply, so
    // only the asset reference remains a genuine durable blocker.
    let key = [19; 32];
    let asset = AssetRef {
        asset_id: [72; 32],
        rel_path: "assets/x.bin".to_string(),
        node_id: "assets/x.bin".to_string(),
        mime_type: "application/pdf".to_string(),
        total_bytes: 123,
        plaintext_hash: [73; 32],
        chunks: Vec::new(),
    };
    let cases = [
        (
            remote_typed(
                [72; 16],
                "p72",
                72,
                SyncEntryKind::AssetReference,
                SyncPayload::AssetReference(asset),
                &key,
            ),
            InboxState::PendingAsset,
            None,
            false,
        ),
        (
            remote_typed(
                [73; 16],
                "p73",
                73,
                SyncEntryKind::Delete,
                typed_delete("typed-delete-node", "typed-delete.md"),
                &key,
            ),
            InboxState::Applied,
            None,
            false,
        ),
    ];

    for (entry, expected_state, expected_error, expect_err) in cases {
        let db_state = seeded_db();
        {
            let db = db_state.lock().unwrap();
            db.stage_inbox_page(
                VAULT,
                PROVIDER,
                "",
                format!("cursor-{}", entry.operation_id[0]).as_str(),
                false,
                &[remote_entry_to_inbox_entry(&entry).unwrap()],
                NOW,
            )
            .unwrap();
        }
        let applier = InspectingApplier {
            db_state: db_state.clone(),
            page_cursor: String::new(),
            snapshots: Mutex::new(Vec::new()),
            events: Arc::new(Mutex::new(Vec::new())),
        };
        let handle = mock_app_handle();
        let mut result = SyncResult::empty();
        let outcome = process_staged_inbox_page(
            &db_state,
            VAULT,
            PROVIDER,
            "",
            "local-device",
            &key,
            &applier,
            &handle,
            Path::new("/tmp"),
            "/tmp",
            &mut result,
        );
        assert_eq!(
            outcome.is_err(),
            expect_err,
            "unexpected outcome: {outcome:?}"
        );
        let record = db_state
            .lock()
            .unwrap()
            .get_inbox_by_id(VAULT, PROVIDER, &entry.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(record.state, expected_state);
        assert_eq!(record.last_error.as_deref(), expected_error);
        assert!(applier.snapshots.lock().unwrap().is_empty());
    }
}

#[test]
fn c2b_v3_pull_two_updates_preserve_exact_payload_order() {
    let db_state = seeded_db();
    let key = [16; 32];
    let first = remote_upsert([68; 16], "p68", 68, vec![8, 1], &key);
    let second = remote_upsert([69; 16], "p69", 69, vec![8, 2], &key);
    {
        let db = db_state.lock().unwrap();
        db.stage_inbox_page(
            VAULT,
            PROVIDER,
            "",
            "cursor-order",
            false,
            &[
                remote_entry_to_inbox_entry(&first).unwrap(),
                remote_entry_to_inbox_entry(&second).unwrap(),
            ],
            NOW,
        )
        .unwrap();
    }
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events: Arc::new(Mutex::new(Vec::new())),
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();
    process_staged_inbox_page(
        &db_state,
        VAULT,
        PROVIDER,
        "",
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &mut result,
    )
    .unwrap();
    assert_eq!(
        *applier.snapshots.lock().unwrap(),
        vec![vec![8, 1], vec![8, 2]]
    );
}

#[tokio::test]
async fn c2b_v3_pull_empty_advancing_page_and_terminal_noop_are_distinct() {
    let db_state = seeded_db();
    let key = [17; 32];
    let adapter = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 0 })
        .with_pages(vec![AdapterPullPage {
            entries: Vec::new(),
            next_cursor: "cursor-empty".to_string(),
            has_more: false,
            rx_bytes: 0,
        }])
        .with_db_observation(db_state.clone());
    let applier = InspectingApplier {
        db_state: db_state.clone(),
        page_cursor: String::new(),
        snapshots: Mutex::new(Vec::new()),
        events: adapter.events.clone(),
    };
    let handle = mock_app_handle();
    let mut result = SyncResult::empty();
    pull_pages_durable(
        &db_state,
        &adapter,
        VAULT,
        PROVIDER,
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &pull_plan("cursor-empty"),
        pull_limits(),
        &mut result,
    )
    .await
    .unwrap();
    assert_eq!(adapter.ack_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        db_state
            .lock()
            .unwrap()
            .get_sync_provider_state(VAULT, PROVIDER)
            .unwrap()
            .unwrap()
            .cursor,
        "cursor-empty"
    );

    let before = snapshot_c2b_runtime_raw(&db_state, VAULT, PROVIDER).unwrap();
    let terminal = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 0 });
    pull_pages_durable(
        &db_state,
        &terminal,
        VAULT,
        PROVIDER,
        "local-device",
        &key,
        &applier,
        &handle,
        Path::new("/tmp"),
        "/tmp",
        &pull_plan("cursor-empty"),
        pull_limits(),
        &mut result,
    )
    .await
    .unwrap();
    let after = snapshot_c2b_runtime_raw(&db_state, VAULT, PROVIDER).unwrap();
    assert_eq!(before, after);
    assert_eq!(terminal.ack_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn c2b_v3_pull_bootstrap_preflight_never_pushes_or_pulls() {
    let db_state = seeded_db();
    let mut adapter = HarnessAdapter::new(PushBehavior::AcceptAll { tx_bytes: 0 });
    adapter.plan_mode = AdapterSyncMode::BootstrapRequired;
    let result = preflight_provider_state(&db_state, VAULT, PROVIDER, &adapter).await;
    assert!(result.is_err());
    assert_eq!(adapter.push_calls.load(Ordering::SeqCst), 0);
    assert_eq!(adapter.pull_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn c2b_v3_provider_mappers_preserve_native_positions() {
    use crate::sync::protocol::{MailboxEntryV3, MailboxResponse, PullPageResult};

    let response = MailboxResponse::PullPageResult(PullPageResult {
        entries: vec![MailboxEntryV3 {
            seq: 901,
            operation_id: [91; 16],
            entry_kind: SyncEntryKind::Upsert,
            doc_hash: [92; 32],
            source_device: "server-device".to_string(),
            encrypted_payload: vec![1, 2],
            payload_hash: [93; 32],
            timestamp: NOW,
        }],
        next_seq: 902,
        until_seq: 999,
        has_more: true,
    });
    let server_page = crate::sync::adapter::server::map_pull_page_response(response, 999).unwrap();
    assert_eq!(server_page.entries[0].remote_position, "901");
    assert_eq!(server_page.entries[0].remote_seq, Some(901));

    // The Google Drive half of this oracle went with the provider it tested.
    // It asserted that an opaque native position — a Drive file id, carrying no
    // sequence — survived the mapper alongside the server's numeric one. The
    // server assertions above still cover the mapper that still exists.
}

#[test]
fn frc2b_r1_04_snapshot_rejects_malformed_operation_id_blob() {
    let db_state = seeded_db();
    {
        let db = db_state.lock().unwrap();
        db.conn()
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO sync_inbox (vault_id, provider_id, page_cursor, remote_position, remote_seq, operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash, source_device, state, last_error, received_at, updated_at, applied_at) VALUES (?1, ?2, '', 'native-bad-op', 1, ?3, ?4, 'upsert', ?5, ?6, 'remote', 'pending', NULL, ?7, ?7, NULL)",
                params![
                    VAULT,
                    PROVIDER,
                    vec![1u8; 15],
                    vec![2u8; 32],
                    vec![3u8],
                    vec![4u8; 32],
                    NOW,
                ],
            )
            .unwrap();
    }
    let error = snapshot_c2b_runtime_raw(&db_state, VAULT, PROVIDER).unwrap_err();
    assert!(error.to_string().contains("operation_id"));
}

#[test]
fn frc2b_r1_04_snapshot_rejects_malformed_hash_blob() {
    let db_state = seeded_db();
    {
        let db = db_state.lock().unwrap();
        db.conn()
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO sync_inbox (vault_id, provider_id, page_cursor, remote_position, remote_seq, operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash, source_device, state, last_error, received_at, updated_at, applied_at) VALUES (?1, ?2, '', 'native-bad-hash', 1, ?3, ?4, 'upsert', ?5, ?6, 'remote', 'pending', NULL, ?7, ?7, NULL)",
                params![
                    VAULT,
                    PROVIDER,
                    vec![5u8; 16],
                    vec![6u8; 31],
                    vec![7u8],
                    vec![8u8; 31],
                    NOW,
                ],
            )
            .unwrap();
    }
    let error = snapshot_c2b_runtime_raw(&db_state, VAULT, PROVIDER).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("doc_hash") || message.contains("payload_hash"));
}
