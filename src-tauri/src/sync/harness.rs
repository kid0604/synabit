//! Two-device integration harness for the sync engine.
//!
//! Everything below drives the *real* `SyncCoordinator` against the *real*
//! `ProductionInboxEntryApplier`, so files genuinely land on disk and the
//! durable outbox/inbox tables are genuinely written. The only substitution is
//! the transport: `InMemoryMailbox` stands in for `sync-server`, implementing
//! the same append-only, sequence-numbered mailbox semantics.
//!
//! The unit tests elsewhere in this crate check state transitions in isolation.
//! This harness answers the question none of them can: *do two devices actually
//! converge?*

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tauri::test::MockRuntime;
use tauri::Manager;

use crate::db::{DbBridge, DbState};
use crate::error::{AppError, AppResult};
use crate::sync::adapter::{
    AdapterPullPage, AdapterSyncMode, AdapterSyncPlan, PullLimits, PushAck, PushResult,
    RemoteEntry, SyncAdapter,
};
use crate::sync::core::types::{SyncOperation, SyncResult, SyncRunContext};
use synabit_protocol::SyncEntryKind;

/// E2EE key shared by every device in a harness vault.
pub const HARNESS_E2EE_KEY: [u8; 32] = [7u8; 32];

const HARNESS_INCARNATION: [u8; 16] = [42u8; 16];
const HARNESS_REMOTE_VAULT_ID: [u8; 32] = [9u8; 32];

// ---------------------------------------------------------------------------
// In-memory mailbox (stands in for sync-server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct StoredEntry {
    seq: u64,
    operation_id: [u8; 16],
    doc_hash: [u8; 32],
    entry_kind: SyncEntryKind,
    encrypted_payload: Vec<u8>,
    payload_hash: [u8; 32],
    source_device: String,
    timestamp: i64,
}

#[derive(Default)]
struct MailboxInner {
    entries: Vec<StoredEntry>,
    acks: HashMap<String, u64>,
    /// Operation ids the mailbox should reject instead of accept, so tests can
    /// exercise the retry path without breaking the transport.
    reject: Vec<[u8; 16]>,
}

/// Append-only mailbox shared by every device in one harness vault.
///
/// Sequence numbers start at 1 and are assigned on push, matching the server.
#[derive(Clone, Default)]
pub struct InMemoryMailbox {
    inner: Arc<Mutex<MailboxInner>>,
}

impl InMemoryMailbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn head_seq(&self) -> u64 {
        self.lock().entries.last().map(|e| e.seq).unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entry kinds currently in the mailbox, oldest first. Useful for asserting
    /// what a device actually emitted (e.g. one upsert vs. delete + upsert).
    pub fn kinds(&self) -> Vec<SyncEntryKind> {
        self.lock()
            .entries
            .iter()
            .map(|e| e.entry_kind.clone())
            .collect()
    }

    /// Corrupt a stored entry's payload so it fails hash verification on pull.
    /// Models a bad actor, a truncated write, or a bit flip in transit.
    pub fn corrupt_entry_at(&self, seq: u64) {
        let mut inner = self.lock();
        if let Some(entry) = inner.entries.iter_mut().find(|e| e.seq == seq) {
            entry.encrypted_payload.push(0xFF);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MailboxInner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

// ---------------------------------------------------------------------------
// Adapter (one per device, all sharing a mailbox)
// ---------------------------------------------------------------------------

pub struct HarnessAdapter {
    mailbox: InMemoryMailbox,
    device_id: String,
}

impl HarnessAdapter {
    pub fn new(mailbox: &InMemoryMailbox, device_id: &str) -> Self {
        Self {
            mailbox: mailbox.clone(),
            device_id: device_id.to_string(),
        }
    }
}

fn parse_cursor(cursor: &str) -> AppResult<u64> {
    if cursor.is_empty() {
        return Ok(0);
    }
    cursor
        .parse::<u64>()
        .map_err(|_| AppError::SyncError(format!("Invalid harness cursor: '{}'", cursor)))
}

#[async_trait]
impl SyncAdapter for HarnessAdapter {
    fn name(&self) -> &str {
        "harness"
    }

    fn adapter_id(&self) -> String {
        "harness".to_string()
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
        let mut inner = self.mailbox.lock();
        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let mut tx_bytes = 0u64;

        for op in operations {
            tx_bytes += op.encrypted_payload.len() as u64;

            if inner.reject.contains(&op.operation_id) {
                rejected.push(PushAck {
                    operation_id: op.operation_id,
                    remote_position: "rejected by harness".to_string(),
                    remote_seq: None,
                });
                continue;
            }

            // Idempotency: a re-pushed operation_id keeps its original seq.
            if let Some(existing) = inner
                .entries
                .iter()
                .find(|e| e.operation_id == op.operation_id)
            {
                accepted.push(PushAck {
                    operation_id: op.operation_id,
                    remote_position: existing.seq.to_string(),
                    remote_seq: Some(existing.seq),
                });
                continue;
            }

            let seq = inner.entries.last().map(|e| e.seq).unwrap_or(0) + 1;
            inner.entries.push(StoredEntry {
                seq,
                operation_id: op.operation_id,
                doc_hash: op.doc_hash,
                entry_kind: op.entry_kind.clone(),
                encrypted_payload: op.encrypted_payload,
                payload_hash: op.payload_hash,
                source_device: self.device_id.clone(),
                timestamp: op.timestamp,
            });
            accepted.push(PushAck {
                operation_id: op.operation_id,
                remote_position: seq.to_string(),
                remote_seq: Some(seq),
            });
        }

        Ok(PushResult {
            accepted,
            rejected,
            tx_bytes,
        })
    }

    async fn get_sync_plan(
        &self,
        _cursor: &str,
        _client_incarnation_id: Option<[u8; 16]>,
    ) -> AppResult<AdapterSyncPlan> {
        let head = self.mailbox.head_seq();
        Ok(AdapterSyncPlan {
            mode: AdapterSyncMode::Delta {
                until_cursor: Some(head.to_string()),
            },
            incarnation_id: Some(HARNESS_INCARNATION),
            remote_vault_id: Some(HARNESS_REMOTE_VAULT_ID),
        })
    }

    async fn pull_page(
        &self,
        cursor: &str,
        until_cursor: Option<&str>,
        limits: PullLimits,
    ) -> AppResult<AdapterPullPage> {
        let after = parse_cursor(cursor)?;
        let until = match until_cursor {
            Some(u) => parse_cursor(u)?,
            None => self.mailbox.head_seq(),
        };

        let inner = self.mailbox.lock();
        let mut entries = Vec::new();
        let mut rx_bytes = 0u64;
        let mut next = after;

        for stored in inner.entries.iter().filter(|e| e.seq > after && e.seq <= until) {
            if entries.len() >= limits.max_entries as usize
                || rx_bytes + stored.encrypted_payload.len() as u64 > limits.max_bytes as u64
            {
                break;
            }
            rx_bytes += stored.encrypted_payload.len() as u64;
            next = stored.seq;
            entries.push(RemoteEntry {
                remote_position: stored.seq.to_string(),
                remote_seq: Some(stored.seq),
                doc_hash: stored.doc_hash,
                source_device: stored.source_device.clone(),
                encrypted_payload: stored.encrypted_payload.clone(),
                payload_hash: stored.payload_hash,
                timestamp: stored.timestamp,
                operation_id: stored.operation_id,
                entry_kind: stored.entry_kind.clone(),
            });
        }

        let has_more = inner.entries.iter().any(|e| e.seq > next && e.seq <= until);

        Ok(AdapterPullPage {
            entries,
            next_cursor: next.to_string(),
            has_more,
            rx_bytes,
        })
    }

    async fn ack(&self, cursor: &str) -> AppResult<()> {
        let seq = parse_cursor(cursor)?;
        self.mailbox.lock().acks.insert(self.device_id.clone(), seq);
        Ok(())
    }

    async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> {
        Err(AppError::UnsupportedCapability("push_asset".into()))
    }

    async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
        Err(AppError::UnsupportedCapability("pull_asset".into()))
    }
}

// ---------------------------------------------------------------------------
// Device
// ---------------------------------------------------------------------------

/// One simulated device: its own app handle, database, vault directory and
/// device id, sharing a mailbox with its peers.
pub struct HarnessDevice {
    pub name: String,
    pub device_id: String,
    /// Held for the device's whole life. Dropping the `App` and keeping only a
    /// handle lets Tauri tear the mock runtime down underneath us, which showed
    /// up as rare, parallelism-dependent failures.
    _app: tauri::App<MockRuntime>,
    handle: tauri::AppHandle<MockRuntime>,
    /// Held so the directory outlives the device; the vault itself is the
    /// `vault/` subdirectory inside it.
    _tmp: tempfile::TempDir,
    vault_root: std::path::PathBuf,
    adapter: Arc<HarnessAdapter>,
}

impl HarnessDevice {
    pub fn new(name: &str, mailbox: &InMemoryMailbox) -> Self {
        let device_id = format!("device-{}", name);
        Self::new_with_device_id(name, mailbox, &device_id)
    }

    /// Build a device with an explicit device id, so tests can reproduce the
    /// case where two installs report the same identity.
    pub fn new_with_device_id(name: &str, mailbox: &InMemoryMailbox, device_id: &str) -> Self {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock app");
        let handle = app.handle().clone();

        let db = DbBridge::new_in_memory_full().expect("build full in-memory schema");
        handle.manage(DbState::new(db));

        // The vault lives in a *named* subdirectory: `tempfile` prefixes its
        // directories with a dot, and `collect_local_files` skips any entry
        // whose name starts with one — including the walk root, which would
        // silently make the whole vault invisible to change detection.
        let vault = tempfile::tempdir().expect("create temp dir");
        std::fs::create_dir_all(vault.path().join("vault")).expect("create vault dir");

        let adapter = Arc::new(HarnessAdapter::new(mailbox, device_id));

        let vault_root = vault.path().join("vault");
        Self {
            name: name.to_string(),
            device_id: device_id.to_string(),
            _app: app,
            handle,
            _tmp: vault,
            vault_root,
            adapter,
        }
    }

    pub fn vault_path(&self) -> &Path {
        &self.vault_root
    }

    // ── Filesystem helpers ──────────────────────────────────

    pub fn write(&self, rel_path: &str, content: &str) {
        let full = self.vault_root.join(rel_path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(&full, content).expect("write file");
    }

    pub fn read(&self, rel_path: &str) -> Option<String> {
        std::fs::read_to_string(self.vault_root.join(rel_path)).ok()
    }

    pub fn exists(&self, rel_path: &str) -> bool {
        self.vault_root.join(rel_path).exists()
    }

    pub fn delete(&self, rel_path: &str) {
        std::fs::remove_file(self.vault_root.join(rel_path)).expect("delete file");
    }

    pub fn rename(&self, from_rel: &str, to_rel: &str) {
        let to = self.vault_root.join(to_rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::rename(self.vault_root.join(from_rel), to).expect("rename file");
    }

    /// Body text with the frontmatter stripped, so assertions do not depend on
    /// the `node_id` the engine injects.
    pub fn body(&self, rel_path: &str) -> Option<String> {
        let text = self.read(rel_path)?;
        Some(strip_frontmatter(&text))
    }

    // ── The thing under test ────────────────────────────────

    /// Run one full sync, exactly as the `sync_full` command does.
    pub async fn sync(&self) -> AppResult<SyncResult> {
        let vault_path = self.vault_root.to_string_lossy().to_string();
        let identity =
            crate::sync::core::identity::load_or_register_vault_identity(&self.handle, &vault_path)?;

        let mut coordinator = crate::sync::coordinator::SyncCoordinator::new();
        coordinator.set_adapter(self.adapter.clone()).await?;

        let ctx = SyncRunContext::new(&vault_path, Some("manual"));
        coordinator
            .sync(&identity, &self.device_id, &HARNESS_E2EE_KEY, &ctx, &self.handle)
            .await
    }

    /// Sync and panic with context on failure — the common case in scenarios.
    pub async fn sync_ok(&self) -> SyncResult {
        match self.sync().await {
            Ok(r) => r,
            Err(e) => panic!("[{}] sync failed: {}", self.name, e),
        }
    }
}

pub fn strip_frontmatter(text: &str) -> String {
    if !text.starts_with("---") {
        return text.to_string();
    }
    let mut parts = text.splitn(3, "---");
    let _ = parts.next();
    let _ = parts.next();
    parts.next().unwrap_or("").trim_start().to_string()
}

/// Build `count` devices sharing one mailbox.
pub fn vault_with_devices(names: &[&str]) -> (InMemoryMailbox, Vec<HarnessDevice>) {
    let mailbox = InMemoryMailbox::new();
    let devices = names
        .iter()
        .map(|n| HarnessDevice::new(n, &mailbox))
        .collect();
    (mailbox, devices)
}
