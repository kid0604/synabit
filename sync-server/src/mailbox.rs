//! Mailbox protocol handler.
//!
//! Implements `iroh::protocol::ProtocolHandler` so the Iroh `Router` can
//! dispatch incoming connections with ALPN `b"synabit/mailbox/1"` to this
//! handler.
//!
//! Each accepted connection opens a bidirectional QUIC stream. The client
//! sends a sequence of `MailboxRequest` messages (length-prefixed postcard)
//! and the server responds with `MailboxResponse` messages on the same stream.
//!
//! The first message MUST be `Auth`; all subsequent messages operate within
//! the authenticated vault context.

use anyhow::{Context, Result};
use iroh::endpoint::Connection;
use iroh::protocol::ProtocolHandler;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::auth::{self, AuthResult};
use crate::config::AppConfig;
use crate::db::Database;
use crate::protocol::{read_message, write_message, MailboxRequest, MailboxResponse};

/// Maximum number of concurrent connections per vault.
const MAX_CONNECTIONS_PER_VAULT: u32 = 10;

/// Shared state for the mailbox protocol handler.
#[derive(Debug)]
pub struct MailboxHandler {
    db: Database,
    config: AppConfig,
    blob_dir: PathBuf,
    endpoint_id: RwLock<String>,
    /// Per-vault concurrent connection counter for basic rate limiting.
    concurrent_connections: Arc<Mutex<HashMap<String, u32>>>,
    /// Registry of active connections waiting for push notifications.
    /// Maps vault_hash (hex) to a list of channels.
    /// Per-vault push channels, each tagged with the device it belongs to so a
    /// device is never told about its own writes.
    active_subscriptions:
        Arc<tokio::sync::RwLock<HashMap<String, Vec<(String, tokio::sync::mpsc::Sender<u64>)>>>>,
}

impl MailboxHandler {
    /// Create a new mailbox handler.
    pub async fn new(db: Database, config: AppConfig) -> Result<Self> {
        let blob_dir = config.data_dir.join("blobs");
        tokio::fs::create_dir_all(&blob_dir)
            .await
            .with_context(|| format!("failed to create blob dir: {}", blob_dir.display()))?;
        Ok(Self {
            db,
            config,
            blob_dir,
            endpoint_id: RwLock::new(String::new()),
            concurrent_connections: Arc::new(Mutex::new(HashMap::new())),
            active_subscriptions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        })
    }

    /// Public accessor for the database (used by cleanup and health).
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Public accessor for config.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Set the endpoint ID (called after Iroh endpoint is bound).
    pub fn set_endpoint_id(&self, id: String) {
        *self.endpoint_id.write().unwrap() = id;
    }

    /// Get the endpoint ID as a hex string.
    pub fn endpoint_id(&self) -> String {
        self.endpoint_id.read().unwrap().clone()
    }

    /// Handle a single authenticated session on one QUIC bidirectional stream.
    async fn handle_connection(&self, connection: Connection) -> Result<()> {
        let remote = connection.remote_id();
        info!(remote = %remote, "new mailbox connection");

        // Accept a bidirectional stream from the client.
        let (mut send, mut recv) = connection
            .accept_bi()
            .await
            .context("failed to accept bidirectional stream")?;

        // --- Step 1: Handshake and Authenticate ---
        let mut vault_hash_hex_opt = None;
        let mut device_id_opt = None;
        let mut is_v3_opt = None;

        while vault_hash_hex_opt.is_none() {
            let msg: MailboxRequest = match read_message(&mut recv).await? {
                Some(msg) => msg,
                None => {
                    debug!(remote = %remote, "client closed stream before auth");
                    let _ = send.finish();
                    return Ok(());
                }
            };

            match msg {
                MailboxRequest::Hello { version } => {
                    if version < 3 {
                        write_message(
                            &mut send,
                            &MailboxResponse::UpgradeRequired {
                                supported_versions: vec![3],
                                message: "Only V3 or later is supported".to_string(),
                            },
                        )
                        .await?;
                        let _ = send.finish();
                        return Ok(());
                    }
                    let hello_resp = synabit_protocol::ServerHello {
                        protocol_version: 3,
                        server_incarnation: [0; 16], // TODO: use real incarnation ID
                        // Only what this server actually implements. BootstrapV1
                        // and AssetChunksV1 were advertised but unimplemented,
                        // which would mislead any client that branched on them.
                        capabilities: vec![
                            synabit_protocol::Capability::PagedPull,
                            synabit_protocol::Capability::DurableIdempotency,
                            synabit_protocol::Capability::DeviceLifecycleV1,
                            synabit_protocol::Capability::QuotaV1,
                        ],
                        max_message_bytes: synabit_protocol::MAX_MESSAGE_SIZE as u64,
                        max_page_bytes: 5 * 1024 * 1024,
                        max_asset_chunk_bytes: 10 * 1024 * 1024,
                    };
                    write_message(&mut send, &MailboxResponse::HelloOk(hello_resp)).await?;
                }
                MailboxRequest::Auth {
                    vault_hash,
                    mailbox_token,
                    device_id,
                } => {
                    let vault_hash_hex = hex::encode(vault_hash);
                    match auth::authenticate(
                        &self.db,
                        &vault_hash_hex,
                        &mailbox_token,
                        &device_id,
                        self.config.default_max_vault_bytes,
                    )? {
                        AuthResult::Registered | AuthResult::Authenticated => {
                            write_message(&mut send, &MailboxResponse::AuthOk).await?;
                            vault_hash_hex_opt = Some(vault_hash_hex);
                            device_id_opt = Some(device_id);
                            is_v3_opt = Some(false);
                        }
                        AuthResult::Rejected(reason) => {
                            write_message(
                                &mut send,
                                &MailboxResponse::AuthFailed {
                                    reason: reason.clone(),
                                },
                            )
                            .await?;
                            warn!(
                                remote = %remote,
                                vault = vault_hash_hex,
                                "auth rejected: {reason}"
                            );
                            let _ = send.finish();
                            return Ok(());
                        }
                    }
                }
                MailboxRequest::AuthV3 {
                    version: _,
                    capabilities: _,
                    vault_hash,
                    mailbox_token,
                    device_id,
                } => {
                    let vault_hash_hex = hex::encode(vault_hash);
                    match auth::authenticate(
                        &self.db,
                        &vault_hash_hex,
                        &mailbox_token,
                        &device_id,
                        self.config.default_max_vault_bytes,
                    )? {
                        AuthResult::Registered | AuthResult::Authenticated => {
                            write_message(&mut send, &MailboxResponse::AuthOk).await?;
                            vault_hash_hex_opt = Some(vault_hash_hex);
                            device_id_opt = Some(device_id);
                            is_v3_opt = Some(true);
                        }
                        AuthResult::Rejected(reason) => {
                            write_message(
                                &mut send,
                                &MailboxResponse::AuthFailed {
                                    reason: reason.clone(),
                                },
                            )
                            .await?;
                            warn!(
                                remote = %remote,
                                vault = vault_hash_hex,
                                "auth rejected: {reason}"
                            );
                            let _ = send.finish();
                            return Ok(());
                        }
                    }
                }
                _ => {
                    write_message(
                        &mut send,
                        &MailboxResponse::Error {
                            message: "first message must be Hello or Auth".to_string(),
                        },
                    )
                    .await?;
                    let _ = send.finish();
                    return Ok(());
                }
            }
        }

        let vault_hash_hex = vault_hash_hex_opt.unwrap();
        let device_id = device_id_opt.unwrap();
        let _is_v3 = is_v3_opt.unwrap();

        // --- Rate limiting: check and increment concurrent connection count ---
        let rate_limited = {
            let mut counts = self
                .concurrent_connections
                .lock()
                .map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
            let count = counts.entry(vault_hash_hex.clone()).or_insert(0);
            if *count >= MAX_CONNECTIONS_PER_VAULT {
                warn!(
                    vault = vault_hash_hex,
                    count = *count,
                    "rate limit: too many concurrent connections"
                );
                true
            } else {
                *count += 1;
                false
            }
        }; // MutexGuard dropped here

        if rate_limited {
            let _ = write_message(
                &mut send,
                &MailboxResponse::Error {
                    message: "too many concurrent connections for this vault".to_string(),
                },
            )
            .await;
            return Ok(());
        }

        // Ensure the counter is decremented when this connection ends.
        let _guard = ConnectionGuard {
            vault_hash: vault_hash_hex.clone(),
            concurrent_connections: self.concurrent_connections.clone(),
        };

        // --- Step 2: Message loop ---
        // Create an mpsc channel for this connection to receive push notifications
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<u64>(100);

        // Register the channel in active_subscriptions
        {
            let mut subs = self.active_subscriptions.write().await;
            subs.entry(vault_hash_hex.clone())
                .or_default()
                .push((device_id.clone(), notify_tx));
        }

        // Spawn a task to read requests, because read_message is not cancellation-safe.
        let (req_tx, mut req_rx) =
            tokio::sync::mpsc::channel::<Result<Option<MailboxRequest>, anyhow::Error>>(10);
        let recv_task = tokio::spawn(async move {
            loop {
                let req = read_message(&mut recv).await;
                if req_tx.send(req).await.is_err() {
                    break;
                }
            }
        });

        loop {
            tokio::select! {
                req_opt = req_rx.recv() => {
                    let request = match req_opt {
                        Some(Ok(Some(msg))) => msg,
                        Some(Ok(None)) | None => {
                            debug!(vault = vault_hash_hex, device = device_id, "client closed stream");
                            break;
                        }
                        Some(Err(e)) => {
                            warn!(vault = vault_hash_hex, device = device_id, error = %e, "error reading from stream");
                            break;
                        }
                    };

                    let response = self
                        .handle_request(&vault_hash_hex, &device_id, request)
                        .await;

                    match response {
                        Ok(resp) => {
                            if let Err(e) = write_message(&mut send, &resp).await {
                                warn!(vault = vault_hash_hex, device = device_id, error = %e, "error writing response");
                                break;
                            }
                        }
                        Err(e) => {
                            error!(vault = vault_hash_hex, device = device_id, error = %e, "internal error handling request");
                            let _ = write_message(&mut send, &MailboxResponse::Error { message: "internal server error".to_string() }).await;
                            break;
                        }
                    }
                }
                Some(trigger_seq) = notify_rx.recv() => {
                    debug!(vault = vault_hash_hex, device = device_id, "pushing NotifyNewData");
                    let response = MailboxResponse::NotifyNewData { trigger_seq };
                    if let Err(e) = write_message(&mut send, &response).await {
                        warn!(vault = vault_hash_hex, device = device_id, error = %e, "error writing NotifyNewData");
                        break;
                    }
                }
            }
        }

        recv_task.abort();

        // Cleanup subscription
        {
            let mut subs = self.active_subscriptions.write().await;
            if let Some(list) = subs.get_mut(&vault_hash_hex) {
                // Since Sender doesn't have an ID, we just remove all closed channels
                list.retain(|(_, tx)| !tx.is_closed());
                if list.is_empty() {
                    subs.remove(&vault_hash_hex);
                }
            }
        }

        Ok(())
    }

    /// Notify subscribers when new sequence data is available.
    /// Tell a vault's other devices that new data has arrived.
    ///
    /// The device that produced the change is skipped. Notifying it made it sync
    /// again immediately, which pushed again, which notified it again — a device
    /// syncing alone spun at machine speed and never went idle.
    async fn notify_subscribers(&self, vault_hash: &str, seq: u64, source_device: &str) {
        let subs = self.active_subscriptions.read().await;
        if let Some(list) = subs.get(vault_hash) {
            for (device, tx) in list {
                if device == source_device {
                    continue;
                }
                let _ = tx.send(seq).await;
            }
        }
    }
    pub async fn handle_request(
        &self,
        vault_hash: &str,
        device_id: &str,
        request: MailboxRequest,
    ) -> Result<MailboxResponse> {
        match request {
            MailboxRequest::Hello { .. } => Ok(MailboxResponse::Error {
                message: "hello already sent".to_string(),
            }),

            MailboxRequest::Auth { .. } | MailboxRequest::AuthV3 { .. } => {
                // Re-auth on the same stream is not allowed.
                Ok(MailboxResponse::Error {
                    message: "already authenticated".to_string(),
                })
            }

            MailboxRequest::Push {
                doc_hash,
                encrypted_payload,
                payload_hash,
                operation_id,
                entry_kind,
            } => {
                self.handle_push(
                    vault_hash,
                    device_id,
                    doc_hash,
                    operation_id,
                    entry_kind,
                    encrypted_payload,
                    payload_hash,
                )
                .await
            }

            MailboxRequest::Pull { since_seq } => self.handle_pull(vault_hash, since_seq),

            MailboxRequest::PushBatch { items } => {
                let mut max_seq = 0;
                let mut results = Vec::with_capacity(items.len());
                for item in items {
                    match self
                        .handle_push(
                            vault_hash,
                            device_id,
                            item.doc_hash,
                            item.operation_id,
                            item.entry_kind,
                            item.encrypted_payload,
                            item.payload_hash,
                        )
                        .await
                    {
                        Ok(MailboxResponse::PushOk { seq }) => {
                            max_seq = max_seq.max(seq);
                            results.push(synabit_protocol::BatchResultItem {
                                operation_id: item.operation_id,
                                error: None,
                            });
                        }
                        Ok(resp) => {
                            let err_msg = match resp {
                                MailboxResponse::Error { message } => message,
                                MailboxResponse::QuotaExceeded {
                                    current_bytes,
                                    limit_bytes,
                                } => format!("QuotaExceeded: {} / {}", current_bytes, limit_bytes),
                                MailboxResponse::AuthFailed { reason } => {
                                    format!("AuthFailed: {}", reason)
                                }
                                other => format!("unexpected response: {:?}", other),
                            };
                            results.push(synabit_protocol::BatchResultItem {
                                operation_id: item.operation_id,
                                error: Some(err_msg),
                            });
                        }
                        Err(e) => {
                            tracing::warn!("PushBatch item failed: {}", e);
                            results.push(synabit_protocol::BatchResultItem {
                                operation_id: item.operation_id,
                                error: Some(e.to_string()),
                            });
                        }
                    }
                }

                Ok(MailboxResponse::PushBatchOk { max_seq, results })
            }

            MailboxRequest::Ack { up_to_seq } => self.handle_ack(vault_hash, device_id, up_to_seq),

            MailboxRequest::PushAsset {
                asset_hash,
                encrypted_data,
            } => {
                self.handle_push_asset(vault_hash, asset_hash, encrypted_data)
                    .await
            }

            MailboxRequest::PullAsset { asset_hash } => {
                self.handle_pull_asset(vault_hash, asset_hash).await
            }

            MailboxRequest::HasAsset { chunk_id } => {
                self.handle_has_asset(vault_hash, chunk_id).await
            }

            MailboxRequest::PushDelete {
                doc_hash,
                operation_id,
            } => {
                self.handle_push_delete(vault_hash, device_id, doc_hash, operation_id)
                    .await
            }

            MailboxRequest::PushTrashMeta { entries } => {
                self.handle_push_trash_meta(vault_hash, device_id, entries)
            }

            MailboxRequest::PullTrashMeta => self.handle_pull_trash_meta(vault_hash),

            MailboxRequest::PushRestore { doc_hash } => {
                self.handle_push_restore(vault_hash, device_id, doc_hash)
                    .await
            }

            MailboxRequest::RevokeDevice {
                device_id: revoked_device_id,
            } => self.handle_revoke_device(vault_hash, &revoked_device_id),

            MailboxRequest::RotateToken { new_mailbox_token } => {
                self.handle_rotate_token(vault_hash, &new_mailbox_token)
            }

            MailboxRequest::Ping => Ok(MailboxResponse::Pong),

            MailboxRequest::GetSyncPlan {
                client_incarnation_id: _,
                cursor,
            } => self.handle_get_sync_plan(vault_hash, cursor),

            MailboxRequest::PullPage {
                after_seq,
                until_seq,
                max_entries,
                max_bytes,
            } => self.handle_pull_page(vault_hash, after_seq, until_seq, max_entries, max_bytes),

            // Bootstrap is not offered. Collection keeps the head entry of
            // every document, so replaying the mailbox from any cursor already
            // reconstructs the whole vault and there is nothing left for a
            // bootstrap session to recover. The handlers that used to sit here
            // could never have run: they queried bootstrap_sessions,
            // bootstrap_items, bootstrap_item_assets, document_heads and
            // blob_objects, none of which this schema creates.
            MailboxRequest::BeginBootstrap { .. }
            | MailboxRequest::PullBootstrapPage { .. }
            | MailboxRequest::KeepBootstrapAlive { .. }
            | MailboxRequest::CompleteBootstrap { .. } => Ok(MailboxResponse::BootstrapUnavailable),


            _ => Ok(MailboxResponse::Error {
                message: "Not implemented in this version".to_string(),
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Individual request handlers
    // -----------------------------------------------------------------------

    /// Tell a client how to catch up.
    ///
    /// The answer is always a delta replay, and deliberately so. Collection
    /// keeps the head entry of every document (see `gc_acked_entries`), so
    /// replaying from any cursor — including zero, for a device that has never
    /// connected — always reconstructs the whole vault. There is nothing a
    /// separate bootstrap exchange would recover that a replay does not.
    ///
    /// This previously had a `BootstrapRequired` branch guarded by
    /// `cursor < compacted_through_seq`, where both sides were the same
    /// variable, so it could never be taken. The branch is gone rather than
    /// repaired: with heads preserved there is no case that needs it.
    fn handle_get_sync_plan(&self, vault_hash: &str, client_cursor: u64) -> Result<MailboxResponse> {
        let (max_seq, server_inc_id, _bootstrap_state) = self.db.get_sync_plan_info(vault_hash)?;

        Ok(MailboxResponse::SyncPlan(synabit_protocol::SyncPlan {
            mode: synabit_protocol::SyncMode::Delta { until_seq: max_seq },
            incarnation_id: server_inc_id.unwrap_or([0; 16]),
            head_seq: max_seq,
            compacted_through_seq: client_cursor.min(max_seq),
        }))
    }

    fn handle_pull_page(
        &self,
        vault_hash: &str,
        after_seq: u64,
        until_seq: u64,
        max_entries: u16,
        max_bytes: u32,
    ) -> Result<MailboxResponse> {
        let metadata_page =
            self.db
                .pull_page_metadata(vault_hash, after_seq, until_seq, max_entries)?;

        let mut entries = Vec::new();
        let mut current_bytes = 0;
        let mut next_seq = after_seq;
        let mut has_more = false;

        if max_entries == 0 || max_bytes == 0 {
            let has_more = self.db.has_more_entries(vault_hash, after_seq, until_seq)?;
            return Ok(MailboxResponse::PullPageResult(
                synabit_protocol::PullPageResult {
                    entries: vec![],
                    next_seq: after_seq,
                    until_seq,
                    has_more,
                },
            ));
        }

        for (seq, op_id, entry_kind, doc_hash, source_device, blob_path, payload_hash, timestamp) in
            metadata_page
        {
            let encrypted_payload = std::fs::read(&blob_path).map_err(|e| {
                anyhow::anyhow!("missing blob for doc {doc_hash} at {blob_path}: {e}")
            })?;

            if encrypted_payload.len() > max_bytes as usize {
                if entries.is_empty() {
                    return Ok(MailboxResponse::Error {
                        message: format!(
                            "Oversized entry: payload size {} exceeds max_bytes {}",
                            encrypted_payload.len(),
                            max_bytes
                        ),
                    });
                } else {
                    has_more = true;
                    break;
                }
            }

            if current_bytes + encrypted_payload.len() > max_bytes as usize {
                has_more = true;
                break;
            }

            current_bytes += encrypted_payload.len();
            next_seq = seq;

            let mut doc_hash_arr = [0u8; 32];
            if let Ok(bytes) = hex::decode(&doc_hash) {
                if bytes.len() == 32 {
                    doc_hash_arr.copy_from_slice(&bytes);
                }
            }
            let mut payload_hash_arr = [0u8; 32];
            if let Ok(bytes) = hex::decode(&payload_hash) {
                if bytes.len() == 32 {
                    payload_hash_arr.copy_from_slice(&bytes);
                }
            }

            entries.push(synabit_protocol::MailboxEntryV3 {
                seq,
                operation_id: op_id,
                entry_kind,
                doc_hash: doc_hash_arr,
                source_device,
                encrypted_payload,
                payload_hash: payload_hash_arr,
                timestamp,
            });
        }

        if !has_more {
            has_more = self.db.has_more_entries(vault_hash, next_seq, until_seq)?;
        }

        Ok(MailboxResponse::PullPageResult(
            synabit_protocol::PullPageResult {
                entries,
                next_seq,
                until_seq,
                has_more,
            },
        ))
    }



    #[allow(clippy::too_many_arguments)]
    async fn handle_push(
        &self,
        vault_hash: &str,
        device_id: &str,
        doc_hash: [u8; 32],
        operation_id: [u8; 16],
        entry_kind: synabit_protocol::SyncEntryKind,
        encrypted_payload: Vec<u8>,
        payload_hash: [u8; 32],
    ) -> Result<MailboxResponse> {
        if encrypted_payload.is_empty() {
            return Ok(MailboxResponse::Error {
                message: "payload is empty".to_string(),
            });
        }

        let computed = blake3::hash(&encrypted_payload);
        if computed.as_bytes() != &payload_hash {
            return Ok(MailboxResponse::Error {
                message: "payload hash mismatch".to_string(),
            });
        }

        let doc_hash_hex = hex::encode(doc_hash);
        let payload_hash_hex = hex::encode(payload_hash);

        // 1. Check Durable Idempotency AFTER payload validation
        match self.db.get_entry_by_operation_id(
            vault_hash,
            &operation_id,
            &doc_hash_hex,
            entry_kind.clone(),
            &payload_hash_hex,
        )? {
            crate::db::IdempotencyResult::Existing { seq } => {
                return Ok(MailboxResponse::PushOk { seq });
            }
            crate::db::IdempotencyResult::Conflict => {
                return Ok(MailboxResponse::Error {
                    message:
                        "idempotency conflict: operation_id reused with different payload/kind"
                            .into(),
                });
            }
            crate::db::IdempotencyResult::NotFound => {}
        }

        let blob_size = encrypted_payload.len() as u64;

        // Check storage quota before writing.
        let current_usage = self.db.total_vault_storage(vault_hash)?;
        let vault_limit = self.db.get_vault_limit(vault_hash)?;
        if current_usage + blob_size > vault_limit {
            return Ok(MailboxResponse::QuotaExceeded {
                current_bytes: current_usage,
                limit_bytes: vault_limit,
            });
        }

        let blob_filename = format!("{vault_hash}_{doc_hash_hex}_{payload_hash_hex}.blob");
        let blob_path = self.blob_dir.join(&blob_filename);
        if !blob_path.exists() {
            let tmp_path =
                self.blob_dir
                    .join(format!("{}.tmp.{}", blob_filename, uuid::Uuid::new_v4()));
            tokio::fs::write(&tmp_path, &encrypted_payload)
                .await
                .with_context(|| format!("failed to write temp blob to {}", tmp_path.display()))?;

            if let Err(e) = tokio::fs::rename(&tmp_path, &blob_path).await {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                if !blob_path.exists() {
                    return Err(anyhow::anyhow!(
                        "failed to rename temp blob to {}: {e}",
                        blob_path.display()
                    ));
                }
            }
        }

        let blob_path_str = blob_path
            .to_str()
            .context("blob path is not valid UTF-8")?
            .to_string();

        match self.db.push_entry(
            vault_hash,
            &doc_hash_hex,
            &operation_id,
            entry_kind,
            device_id,
            &blob_path_str,
            blob_size,
            &payload_hash_hex,
        )? {
            crate::db::PushOutcome::Created(seq) | crate::db::PushOutcome::Existing(seq) => {
                info!(
                    vault = vault_hash,
                    device = device_id,
                    seq = seq,
                    doc = doc_hash_hex,
                    size = blob_size,
                    "document pushed"
                );
                self.notify_subscribers(vault_hash, seq, device_id).await;
                Ok(MailboxResponse::PushOk { seq })
            }
            crate::db::PushOutcome::Conflict => Ok(MailboxResponse::Error {
                message: "idempotency conflict: operation_id reused with different payload/kind"
                    .into(),
            }),
        }
    }

    fn handle_pull(&self, vault_hash: &str, since_seq: u64) -> Result<MailboxResponse> {
        let entries = self.db.pull_entries(vault_hash, since_seq)?;
        debug!(
            vault = vault_hash,
            since_seq = since_seq,
            count = entries.len(),
            "pull completed"
        );
        Ok(MailboxResponse::PullResult { entries })
    }

    fn handle_ack(
        &self,
        vault_hash: &str,
        device_id: &str,
        up_to_seq: u64,
    ) -> Result<MailboxResponse> {
        self.db.update_cursor(vault_hash, device_id, up_to_seq)?;
        debug!(
            vault = vault_hash,
            device = device_id,
            up_to_seq = up_to_seq,
            "ack recorded"
        );
        Ok(MailboxResponse::AckOk)
    }

    async fn handle_has_asset(
        &self,
        vault_hash: &str,
        chunk_id: [u8; 32],
    ) -> Result<MailboxResponse> {
        let asset_hash_hex = hex::encode(chunk_id);
        let size_opt = self.db.asset_exists(vault_hash, &asset_hash_hex)?;
        if let Some(size) = size_opt {
            Ok(MailboxResponse::AssetExists {
                encrypted_size: size,
            })
        } else {
            Ok(MailboxResponse::AssetNotFound)
        }
    }

    async fn handle_push_asset(
        &self,
        vault_hash: &str,
        asset_hash: [u8; 32],
        encrypted_data: Vec<u8>,
    ) -> Result<MailboxResponse> {
        let asset_hash_hex = hex::encode(asset_hash);
        let blob_size = encrypted_data.len() as u64;

        // Check storage quota before writing.
        let current_usage = self.db.total_vault_storage(vault_hash)?;
        let vault_limit = self.db.get_vault_limit(vault_hash)?;
        if current_usage + blob_size > vault_limit {
            return Ok(MailboxResponse::QuotaExceeded {
                current_bytes: current_usage,
                limit_bytes: vault_limit,
            });
        }

        // Write asset blob to disk using a temp file + atomic rename.
        let blob_filename = format!("{vault_hash}_asset_{asset_hash_hex}.blob");
        let blob_path = self.blob_dir.join(&blob_filename);
        let tmp_path = self.blob_dir.join(format!("{}.tmp", blob_filename));

        tokio::fs::write(&tmp_path, &encrypted_data)
            .await
            .with_context(|| {
                format!("failed to write temp asset blob to {}", tmp_path.display())
            })?;

        tokio::fs::rename(&tmp_path, &blob_path)
            .await
            .with_context(|| {
                format!(
                    "failed to rename temp asset blob to {}",
                    blob_path.display()
                )
            })?;

        let blob_path_str = blob_path
            .to_str()
            .context("blob path is not valid UTF-8")?
            .to_string();

        self.db
            .store_asset(vault_hash, &asset_hash_hex, &blob_path_str, blob_size)?;

        info!(
            vault = vault_hash,
            asset = asset_hash_hex,
            size = blob_size,
            "asset stored"
        );

        Ok(MailboxResponse::AssetOk)
    }

    async fn handle_pull_asset(
        &self,
        vault_hash: &str,
        asset_hash: [u8; 32],
    ) -> Result<MailboxResponse> {
        let asset_hash_hex = hex::encode(asset_hash);

        match self.db.get_asset_path(vault_hash, &asset_hash_hex)? {
            Some(blob_path) => {
                let data = tokio::fs::read(&blob_path)
                    .await
                    .with_context(|| format!("failed to read asset blob from {}", blob_path))?;
                debug!(
                    vault = vault_hash,
                    asset = asset_hash_hex,
                    size = data.len(),
                    "asset retrieved"
                );
                Ok(MailboxResponse::AssetData { data })
            }
            None => {
                debug!(
                    vault = vault_hash,
                    asset = asset_hash_hex,
                    "asset not found"
                );
                Ok(MailboxResponse::AssetNotFound)
            }
        }
    }

    async fn handle_push_delete(
        &self,
        _vault_hash: &str,
        _device_id: &str,
        _doc_hash: [u8; 32],
        _operation_id: [u8; 16],
    ) -> Result<MailboxResponse> {
        Ok(MailboxResponse::Error {
            message: "PushDelete is deprecated; use Push with Delete entry kind and typed payload"
                .to_string(),
        })
    }

    fn handle_push_trash_meta(
        &self,
        vault_hash: &str,
        device_id: &str,
        entries: Vec<crate::protocol::TrashMetaEntry>,
    ) -> Result<MailboxResponse> {
        for entry in &entries {
            let doc_hash_hex = hex::encode(entry.doc_hash);
            self.db.store_trash_meta(
                vault_hash,
                &doc_hash_hex,
                &entry.original_path_encrypted,
                entry.deleted_at as i64,
            )?;
        }

        info!(
            vault = vault_hash,
            device = device_id,
            count = entries.len(),
            "trash metadata pushed"
        );

        Ok(MailboxResponse::AckOk)
    }

    fn handle_pull_trash_meta(&self, vault_hash: &str) -> Result<MailboxResponse> {
        let rows = self.db.get_trash_meta(vault_hash)?;
        let entries: Vec<crate::protocol::TrashMetaEntry> = rows
            .into_iter()
            .filter_map(|row| {
                let bytes = hex::decode(&row.doc_hash).ok()?;
                let arr: [u8; 32] = bytes.try_into().ok()?;
                Some(crate::protocol::TrashMetaEntry {
                    doc_hash: arr,
                    original_path_encrypted: row.meta_encrypted,
                    deleted_at: row.deleted_at as u64,
                    deleted_by_device: String::new(),
                    is_purged: row.is_purged,
                })
            })
            .collect();

        debug!(
            vault = vault_hash,
            count = entries.len(),
            "trash metadata pulled"
        );

        Ok(MailboxResponse::TrashMetaResult { entries })
    }

    async fn handle_push_restore(
        &self,
        vault_hash: &str,
        device_id: &str,
        doc_hash: [u8; 32],
    ) -> Result<MailboxResponse> {
        let doc_hash_hex = hex::encode(doc_hash);
        self.db.remove_trash_meta(vault_hash, &doc_hash_hex)?;

        // Also push a regular entry so other devices know to restore
        let payload_hash_hex = hex::encode([0u8; 32]);
        let mut op_id = [0u8; 16];
        op_id.copy_from_slice(
            &blake3::hash(format!("{}:{}:restore", vault_hash, doc_hash_hex).as_bytes()).as_bytes()
                [..16],
        );
        let outcome = self.db.push_entry(
            vault_hash,
            &doc_hash_hex,
            &op_id,
            synabit_protocol::SyncEntryKind::Upsert,
            device_id,
            "(restore)",
            0,
            &payload_hash_hex,
        )?;

        let seq = match outcome {
            crate::db::PushOutcome::Created(s) | crate::db::PushOutcome::Existing(s) => s,
            crate::db::PushOutcome::Conflict => 0,
        };

        info!(
            vault = vault_hash,
            device = device_id,
            seq = seq,
            doc = doc_hash_hex,
            "document restored from trash"
        );

        Ok(MailboxResponse::RestoreOk { seq })
    }

    fn handle_revoke_device(
        &self,
        vault_hash: &str,
        revoked_device_id: &str,
    ) -> Result<MailboxResponse> {
        // Mark first, then drop the cursor. Only the status stops the device
        // from authenticating again; deleting the cursor on its own merely made
        // it re-sync from the beginning, which is why revocation previously had
        // no effect at all.
        self.db
            .set_device_status(vault_hash, revoked_device_id, "revoked")?;
        self.db.reset_device_cursor(vault_hash, revoked_device_id)?;

        info!(
            vault = vault_hash,
            revoked_device = revoked_device_id,
            "device revoked"
        );

        Ok(MailboxResponse::RevokeOk)
    }

    fn handle_rotate_token(
        &self,
        vault_hash: &str,
        new_mailbox_token: &[u8],
    ) -> Result<MailboxResponse> {
        self.db.update_vault_token(vault_hash, new_mailbox_token)?;

        info!(vault = vault_hash, "mailbox token rotated");

        Ok(MailboxResponse::TokenRotated)
    }
}

// ---------------------------------------------------------------------------
// ProtocolHandler implementation for Iroh Router integration
// ---------------------------------------------------------------------------

impl ProtocolHandler for MailboxHandler {
    /// Called by the Iroh Router for each incoming connection on our ALPN.
    /// Runs on a freshly spawned tokio task.
    async fn accept(&self, connection: Connection) -> Result<(), iroh::protocol::AcceptError> {
        // Delegate to the instance method. Errors are logged but don't crash
        // the server — each connection is independent.
        if let Err(e) = self.handle_connection(connection).await {
            error!(error = %e, "connection handler failed");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Connection guard for rate-limit counter cleanup
// ---------------------------------------------------------------------------

/// RAII guard that decrements the per-vault connection counter on drop.
struct ConnectionGuard {
    vault_hash: String,
    concurrent_connections: Arc<Mutex<HashMap<String, u32>>>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Ok(mut counts) = self.concurrent_connections.lock() {
            if let Some(count) = counts.get_mut(&self.vault_hash) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    counts.remove(&self.vault_hash);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn setup_test_mailbox() -> (MailboxHandler, tempfile::TempDir, String) {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");

        let db = Database::open(&db_path).expect("open db");
        let vault_hash = hex::encode([1u8; 32]);
        let token = [2u8; 32];
        db.register_vault(&vault_hash, &token, 100 * 1024 * 1024)
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

    #[tokio::test]
    async fn test_push_batch_all_entry_kinds() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        let item1 = synabit_protocol::PushBatchItem {
            operation_id: [10; 16],
            doc_hash: [1; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            encrypted_payload: b"upsert payload".to_vec(),
            payload_hash: *blake3::hash(b"upsert payload").as_bytes(),
        };

        let item2 = synabit_protocol::PushBatchItem {
            operation_id: [20; 16],
            doc_hash: [2; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Delete,
            encrypted_payload: b"delete payload".to_vec(),
            payload_hash: *blake3::hash(b"delete payload").as_bytes(),
        };

        let item3 = synabit_protocol::PushBatchItem {
            operation_id: [30; 16],
            doc_hash: [3; 32],
            entry_kind: synabit_protocol::SyncEntryKind::AssetReference,
            encrypted_payload: b"asset ref payload".to_vec(),
            payload_hash: *blake3::hash(b"asset ref payload").as_bytes(),
        };

        let req = MailboxRequest::PushBatch {
            items: vec![item1, item2, item3],
        };

        let resp = server
            .handle_request(&vault_hash, "dev1", req)
            .await
            .unwrap();

        if let MailboxResponse::PushBatchOk { max_seq, results } = resp {
            assert!(max_seq > 0);
            assert_eq!(results.len(), 3);
            assert_eq!(results[0].operation_id, [10; 16]);
            assert!(results[0].error.is_none());
            assert_eq!(results[1].operation_id, [20; 16]);
            assert!(results[1].error.is_none());
            assert_eq!(results[2].operation_id, [30; 16]);
            assert!(results[2].error.is_none());
        } else {
            panic!("Expected PushBatchOk response");
        }
    }

    #[tokio::test]
    async fn test_pull_page_respects_bounds_and_preserves_kinds() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        let item1 = synabit_protocol::PushBatchItem {
            operation_id: [10; 16],
            doc_hash: [1; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            encrypted_payload: b"payload 1".to_vec(),
            payload_hash: *blake3::hash(b"payload 1").as_bytes(),
        };
        let item2 = synabit_protocol::PushBatchItem {
            operation_id: [20; 16],
            doc_hash: [2; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Delete,
            encrypted_payload: b"payload 2 delete".to_vec(),
            payload_hash: *blake3::hash(b"payload 2 delete").as_bytes(),
        };

        let req = MailboxRequest::PushBatch {
            items: vec![item1, item2],
        };
        server
            .handle_request(&vault_hash, "dev1", req)
            .await
            .unwrap();

        // Pull max 1 entry
        let pull_req = MailboxRequest::PullPage {
            after_seq: 0,
            until_seq: u64::MAX,
            max_entries: 1,
            max_bytes: 1024 * 1024,
        };

        let resp = server
            .handle_request(&vault_hash, "dev1", pull_req)
            .await
            .unwrap();
        if let MailboxResponse::PullPageResult(page) = resp {
            assert_eq!(page.entries.len(), 1);
            assert!(page.has_more);
            assert_eq!(page.entries[0].operation_id, [10; 16]);
            assert_eq!(
                page.entries[0].entry_kind,
                synabit_protocol::SyncEntryKind::Upsert
            );

            // Pull second page
            let pull_req2 = MailboxRequest::PullPage {
                after_seq: page.next_seq,
                until_seq: u64::MAX,
                max_entries: 10,
                max_bytes: 1024 * 1024,
            };
            let resp2 = server
                .handle_request(&vault_hash, "dev1", pull_req2)
                .await
                .unwrap();
            if let MailboxResponse::PullPageResult(page2) = resp2 {
                assert_eq!(page2.entries.len(), 1);
                assert!(!page2.has_more);
                assert_eq!(page2.entries[0].operation_id, [20; 16]);
                assert_eq!(
                    page2.entries[0].entry_kind,
                    synabit_protocol::SyncEntryKind::Delete
                );
            } else {
                panic!("Expected PullPageResult");
            }
        } else {
            panic!("Expected PullPageResult");
        }
    }

    #[tokio::test]
    async fn test_get_sync_plan_bounds() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        let req = MailboxRequest::GetSyncPlan {
            client_incarnation_id: None,
            cursor: 0,
        };

        let resp = server
            .handle_request(&vault_hash, "dev1", req)
            .await
            .unwrap();
        if let MailboxResponse::SyncPlan(plan) = resp {
            if let synabit_protocol::SyncMode::Delta { until_seq } = plan.mode {
                assert_eq!(until_seq, 0);
            } else {
                panic!("Expected Delta mode");
            }
        } else {
            panic!("Expected SyncPlan");
        }
    }

    #[tokio::test]
    async fn test_restore_flow_creates_valid_op() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        let resp = server
            .handle_push_restore(&vault_hash, "dev1", [7; 32])
            .await
            .unwrap();
        if let MailboxResponse::RestoreOk { seq } = resp {
            assert!(seq > 0);
        } else {
            panic!("Expected RestoreOk from restore");
        }
    }

    #[tokio::test]
    async fn test_idempotent_retry_and_conflict() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        let payload1 = b"idempotent payload 1".to_vec();
        let payload1_hash = *blake3::hash(&payload1).as_bytes();
        let op_id = [10; 16];

        let req1 = MailboxRequest::Push {
            operation_id: op_id,
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            doc_hash: [1; 32],
            encrypted_payload: payload1.clone(),
            payload_hash: payload1_hash,
        };

        // First push
        let resp1 = server
            .handle_request(&vault_hash, "dev1", req1.clone())
            .await
            .unwrap();
        let seq1 = match resp1 {
            MailboxResponse::PushOk { seq } => seq,
            _ => panic!("Expected PushOk on first push"),
        };
        assert_eq!(seq1, 1);

        // Retry same operation ID & same payload
        let resp2 = server
            .handle_request(&vault_hash, "dev1", req1)
            .await
            .unwrap();
        let seq2 = match resp2 {
            MailboxResponse::PushOk { seq } => seq,
            _ => panic!("Expected PushOk on retry"),
        };
        assert_eq!(seq2, 1); // Sequence number unchanged!

        // Verify storage size and entry count
        let usage = server.db.total_vault_storage(&vault_hash).unwrap();
        assert_eq!(usage, payload1.len() as u64);

        // Push same operation ID with DIFFERENT payload -> conflict error
        let payload2 = b"idempotent payload CONFLICT".to_vec();
        let payload2_hash = *blake3::hash(&payload2).as_bytes();
        let req_conflict = MailboxRequest::Push {
            operation_id: op_id,
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            doc_hash: [1; 32],
            encrypted_payload: payload2,
            payload_hash: payload2_hash,
        };
        let resp_conflict = server
            .handle_request(&vault_hash, "dev1", req_conflict)
            .await
            .unwrap();
        assert!(matches!(resp_conflict, MailboxResponse::Error { .. }));
    }

    #[tokio::test]
    async fn test_pull_page_oversized_first_entry_rejected() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        let payload = b"this payload is 32 bytes long!!".to_vec();
        let payload_hash = *blake3::hash(&payload).as_bytes();
        let req = MailboxRequest::Push {
            operation_id: [10; 16],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            doc_hash: [1; 32],
            encrypted_payload: payload,
            payload_hash,
        };
        server
            .handle_request(&vault_hash, "dev1", req)
            .await
            .unwrap();

        // Pull with max_bytes smaller than payload (e.g., 10 bytes)
        let pull_req = MailboxRequest::PullPage {
            after_seq: 0,
            until_seq: u64::MAX,
            max_entries: 10,
            max_bytes: 10,
        };
        let resp = server
            .handle_request(&vault_hash, "dev1", pull_req)
            .await
            .unwrap();
        assert!(matches!(resp, MailboxResponse::Error { .. }));
    }

    #[tokio::test]
    async fn test_concurrent_identical_push_idempotency() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let (server, _dir, vault_hash) = setup_test_mailbox().await;
        let server = Arc::new(server);
        let barrier = Arc::new(Barrier::new(2));

        let payload = b"concurrent identical payload".to_vec();
        let payload_hash = *blake3::hash(&payload).as_bytes();
        let op_id = [42; 16];
        let doc_hash = [7; 32];

        let req1 = MailboxRequest::Push {
            operation_id: op_id,
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            doc_hash,
            encrypted_payload: payload.clone(),
            payload_hash,
        };
        let req2 = req1.clone();

        let s1 = server.clone();
        let b1 = barrier.clone();
        let v1 = vault_hash.clone();
        let handle1 = tokio::spawn(async move {
            b1.wait().await;
            s1.handle_request(&v1, "dev1", req1).await
        });

        let s2 = server.clone();
        let b2 = barrier.clone();
        let v2 = vault_hash.clone();
        let handle2 = tokio::spawn(async move {
            b2.wait().await;
            s2.handle_request(&v2, "dev2", req2).await
        });

        let (res1, res2) = tokio::join!(handle1, handle2);
        let resp1 = res1.unwrap().unwrap();
        let resp2 = res2.unwrap().unwrap();

        let seq1 = match resp1 {
            MailboxResponse::PushOk { seq } => seq,
            _ => panic!("Expected PushOk for task 1"),
        };
        let seq2 = match resp2 {
            MailboxResponse::PushOk { seq } => seq,
            _ => panic!("Expected PushOk for task 2"),
        };

        // Both tasks received the EXACT SAME sequence number
        assert_eq!(seq1, seq2);

        // Database must contain only 1 entry
        let usage = server.db.total_vault_storage(&vault_hash).unwrap();
        assert_eq!(usage, payload.len() as u64);
    }

    #[tokio::test]
    async fn test_concurrent_conflicting_push() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let (server, _dir, vault_hash) = setup_test_mailbox().await;
        let server = Arc::new(server);
        let barrier = Arc::new(Barrier::new(2));

        let op_id = [99; 16];
        let doc_hash = [8; 32];

        let payload1 = b"conflicting payload A".to_vec();
        let payload1_hash = *blake3::hash(&payload1).as_bytes();
        let req1 = MailboxRequest::Push {
            operation_id: op_id,
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            doc_hash,
            encrypted_payload: payload1,
            payload_hash: payload1_hash,
        };

        let payload2 = b"conflicting payload B".to_vec();
        let payload2_hash = *blake3::hash(&payload2).as_bytes();
        let req2 = MailboxRequest::Push {
            operation_id: op_id,
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            doc_hash,
            encrypted_payload: payload2,
            payload_hash: payload2_hash,
        };

        let s1 = server.clone();
        let b1 = barrier.clone();
        let v1 = vault_hash.clone();
        let handle1 = tokio::spawn(async move {
            b1.wait().await;
            s1.handle_request(&v1, "dev1", req1).await
        });

        let s2 = server.clone();
        let b2 = barrier.clone();
        let v2 = vault_hash.clone();
        let handle2 = tokio::spawn(async move {
            b2.wait().await;
            s2.handle_request(&v2, "dev2", req2).await
        });

        let (res1, res2) = tokio::join!(handle1, handle2);
        let resp1 = res1.unwrap().unwrap();
        let resp2 = res2.unwrap().unwrap();

        let ok_count = [resp1.clone(), resp2.clone()]
            .iter()
            .filter(|r| matches!(r, MailboxResponse::PushOk { .. }))
            .count();
        let err_count = [resp1, resp2]
            .iter()
            .filter(|r| matches!(r, MailboxResponse::Error { .. }))
            .count();

        assert_eq!(ok_count, 1, "Exactly one task must win race");
        assert_eq!(
            err_count, 1,
            "Exactly one task must lose with conflict error"
        );
    }

    #[tokio::test]
    async fn test_mixed_push_batch_error_preservation() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        let payload = b"batch item 1".to_vec();
        let payload_hash = *blake3::hash(&payload).as_bytes();

        let item1 = synabit_protocol::PushBatchItem {
            operation_id: [1; 16],
            doc_hash: [10; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            encrypted_payload: payload.clone(),
            payload_hash,
        };

        // Retry of item1 -> idempotent success
        let item2 = item1.clone();

        // Conflict of item1 -> error preserved
        let item3 = synabit_protocol::PushBatchItem {
            operation_id: [1; 16],
            doc_hash: [10; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            encrypted_payload: b"different payload".to_vec(),
            payload_hash: *blake3::hash(b"different payload").as_bytes(),
        };

        let item4_payload = b"delete payload item4".to_vec();
        let item4_hash = *blake3::hash(&item4_payload).as_bytes();
        let item4 = synabit_protocol::PushBatchItem {
            operation_id: [4; 16],
            doc_hash: [20; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Delete,
            encrypted_payload: item4_payload,
            payload_hash: item4_hash,
        };

        let batch_req = MailboxRequest::PushBatch {
            items: vec![item1, item2, item3, item4],
        };

        let resp = server
            .handle_request(&vault_hash, "dev1", batch_req)
            .await
            .unwrap();
        if let MailboxResponse::PushBatchOk { results, .. } = resp {
            assert_eq!(results.len(), 4);
            assert!(results[0].error.is_none(), "Item 1 should succeed");
            assert!(
                results[1].error.is_none(),
                "Item 2 should succeed idempotently"
            );
            assert!(
                results[2].error.is_some(),
                "Item 3 should fail with conflict"
            );
            assert!(results[2].error.as_ref().unwrap().contains("conflict"));
            assert!(results[3].error.is_none(), "Item 4 should succeed");
        } else {
            panic!("Expected PushBatchOk response");
        }
    }

    use crate::auth::{authenticate, AuthResult};


    /// A revoked device must be turned away at authentication.
    ///
    /// Revocation used to delete the device's cursor and nothing else, which
    /// only made it re-sync from the beginning. Meanwhile `devices` was never
    /// written to, so the status check in `authenticate` had nothing to read
    /// and always let the device back in.
    #[tokio::test]
    async fn revoked_device_cannot_authenticate_again() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;
        let token = [2u8; 32];

        // First contact registers the device.
        let first = authenticate(&server.db, &vault_hash, &token, "laptop", 0).expect("auth");
        assert!(matches!(first, AuthResult::Authenticated));

        server
            .handle_revoke_device(&vault_hash, "laptop")
            .expect("revoke");

        let after = authenticate(&server.db, &vault_hash, &token, "laptop", 0).expect("auth");
        match after {
            AuthResult::Rejected(reason) => assert!(
                reason.contains("revoked"),
                "unexpected rejection reason: {reason}"
            ),
            other => panic!("revoked device was allowed back in: {other:?}"),
        }

        // Other devices in the same vault are unaffected.
        let sibling = authenticate(&server.db, &vault_hash, &token, "phone", 0).expect("auth");
        assert!(matches!(sibling, AuthResult::Authenticated));
    }

    /// Revoking a device that has never connected must still stick.
    #[tokio::test]
    async fn revoking_an_unseen_device_still_denies_its_first_connection() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;
        let token = [2u8; 32];

        server
            .handle_revoke_device(&vault_hash, "never-seen")
            .expect("revoke");

        let result = authenticate(&server.db, &vault_hash, &token, "never-seen", 0).expect("auth");
        assert!(
            matches!(result, AuthResult::Rejected(_)),
            "a pre-emptively revoked device was let in"
        );
    }

    // -----------------------------------------------------------------------
    // Garbage collection
    // -----------------------------------------------------------------------

    async fn push_doc(
        server: &MailboxHandler,
        vault_hash: &str,
        device: &str,
        op: u8,
        doc: u8,
        body: &[u8],
    ) {
        let item = synabit_protocol::PushBatchItem {
            operation_id: [op; 16],
            doc_hash: [doc; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            encrypted_payload: body.to_vec(),
            payload_hash: *blake3::hash(body).as_bytes(),
        };
        server
            .handle_request(vault_hash, device, MailboxRequest::PushBatch { items: vec![item] })
            .await
            .expect("push");
    }

    fn seqs_in_mailbox(server: &MailboxHandler, vault_hash: &str) -> Vec<(u64, String)> {
        let entries = server.db.pull_entries(vault_hash, 0).expect("pull");
        entries
            .into_iter()
            .map(|e| (e.seq, hex::encode(e.doc_hash)))
            .collect()
    }

    /// Collecting acknowledged history must never remove the last remaining
    /// record of a document, or a device that joins later replays from the
    /// beginning and silently ends up with an incomplete vault.
    #[tokio::test]
    async fn gc_keeps_the_head_of_every_document() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        push_doc(&server, &vault_hash, "dev1", 1, 0xAA, b"A first").await;
        push_doc(&server, &vault_hash, "dev1", 2, 0xBB, b"B only").await;
        push_doc(&server, &vault_hash, "dev1", 3, 0xAA, b"A second").await;

        let before = seqs_in_mailbox(&server, &vault_hash);
        assert_eq!(before.len(), 3, "precondition: three entries stored");

        // Everything has been acknowledged by every device.
        server
            .db
            .update_cursor(&vault_hash, "dev1", 3)
            .expect("ack");
        let min_seq = server.db.min_cursor(&vault_hash).expect("min cursor");
        assert_eq!(min_seq, 3);
        server
            .db
            .gc_acked_entries(&vault_hash, min_seq)
            .expect("gc");

        let after = seqs_in_mailbox(&server, &vault_hash);
        let docs: Vec<String> = after.iter().map(|(_, d)| d.clone()).collect();

        assert_eq!(
            after.len(),
            2,
            "only the superseded entry should be collected, got {after:?}"
        );
        assert!(
            docs.contains(&hex::encode([0xAAu8; 32])),
            "document A lost its head"
        );
        assert!(
            docs.contains(&hex::encode([0xBBu8; 32])),
            "document B lost its only entry"
        );
        assert_eq!(after[0].0, 2, "B's entry kept at its original sequence");
        assert_eq!(after[1].0, 3, "A's head kept, its earlier revision dropped");
    }

    /// The point of keeping heads: a device starting from nothing still sees
    /// every document after aggressive collection.
    #[tokio::test]
    async fn a_new_device_replays_the_whole_vault_after_gc() {
        let (server, _dir, vault_hash) = setup_test_mailbox().await;

        for (op, doc) in [(1u8, 0xA1u8), (2, 0xA2), (3, 0xA3)] {
            push_doc(&server, &vault_hash, "old-device", op, doc, b"content").await;
        }
        // The old device edits one of them again, then acknowledges everything.
        push_doc(&server, &vault_hash, "old-device", 4, 0xA2, b"edited").await;
        server
            .db
            .update_cursor(&vault_hash, "old-device", 4)
            .expect("ack");
        let min_seq = server.db.min_cursor(&vault_hash).expect("min cursor");
        server
            .db
            .gc_acked_entries(&vault_hash, min_seq)
            .expect("gc");

        // A device that has never connected replays from the very beginning.
        let replay = seqs_in_mailbox(&server, &vault_hash);
        let docs: std::collections::HashSet<String> =
            replay.into_iter().map(|(_, d)| d).collect();

        for doc in [0xA1u8, 0xA2, 0xA3] {
            assert!(
                docs.contains(&hex::encode([doc; 32])),
                "a new device would never learn about document {doc:#x}"
            );
        }
    }
}
