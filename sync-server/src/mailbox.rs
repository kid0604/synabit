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
use crate::protocol::{
    read_message, write_message, MailboxRequest, MailboxResponse,
};

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
    active_subscriptions: Arc<tokio::sync::RwLock<HashMap<String, Vec<tokio::sync::mpsc::Sender<u64>>>>>,
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

        // --- Step 1: Authenticate ---
        let first_msg: MailboxRequest = match read_message(&mut recv).await? {
            Some(msg) => msg,
            None => {
                debug!(remote = %remote, "client closed stream before auth");
                let _ = send.finish();
                return Ok(());
            }
        };

        let (vault_hash_hex, device_id) = match first_msg {
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
                        (vault_hash_hex, device_id)
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
                        message: "first message must be Auth".to_string(),
                    },
                )
                .await?;
                let _ = send.finish();
                return Ok(());
            }
        };

        // --- Rate limiting: check and increment concurrent connection count ---
        let rate_limited = {
            let mut counts = self.concurrent_connections.lock()
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
            subs.entry(vault_hash_hex.clone()).or_default().push(notify_tx);
        }

        // Spawn a task to read requests, because read_message is not cancellation-safe.
        let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<Result<Option<MailboxRequest>, anyhow::Error>>(10);
        let mut recv_task = tokio::spawn(async move {
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
                list.retain(|tx| !tx.is_closed());
                if list.is_empty() {
                    subs.remove(&vault_hash_hex);
                }
            }
        }

        Ok(())
    }

    /// Dispatch a single request within an authenticated session.
    
    async fn notify_subscribers(&self, vault_hash: &str, seq: u64) {
        let subs = self.active_subscriptions.read().await;
        if let Some(list) = subs.get(vault_hash) {
            for tx in list {
                let _ = tx.send(seq).await;
            }
        }
    }
    async fn handle_request(
        &self,
        vault_hash: &str,
        device_id: &str,
        request: MailboxRequest,
    ) -> Result<MailboxResponse> {
        match request {
            MailboxRequest::Hello { .. } => {
                Ok(MailboxResponse::Error {
                    message: "hello already sent".to_string(),
                })
            }

            MailboxRequest::Auth { .. } => {
                // Re-auth on the same stream is not allowed.
                Ok(MailboxResponse::Error {
                    message: "already authenticated".to_string(),
                })
            }

            MailboxRequest::Push {
                doc_hash,
                encrypted_payload,
                payload_hash,
            } => {
                self.handle_push(vault_hash, device_id, doc_hash, encrypted_payload, payload_hash).await
            }

            MailboxRequest::Pull { since_seq } => self.handle_pull(vault_hash, since_seq),

            MailboxRequest::PushBatch { items } => {
                let mut max_seq = 0;
                for item in items {
                    match self.handle_push(
                        vault_hash,
                        device_id,
                        item.doc_hash,
                        item.encrypted_payload,
                        item.payload_hash,
                    ).await {
                        Ok(MailboxResponse::PushOk { seq }) => {
                            max_seq = max_seq.max(seq);
                        }
                        Ok(resp) => {
                            tracing::warn!("PushBatch item unexpected response: {:?}", resp);
                        }
                        Err(e) => {
                            tracing::warn!("PushBatch item failed: {}", e);
                        }
                    }
                }

                Ok(MailboxResponse::PushBatchOk { max_seq })
            }

            MailboxRequest::Ack { up_to_seq } => {
                self.handle_ack(vault_hash, device_id, up_to_seq)
            }

            MailboxRequest::PushAsset {
                asset_hash,
                encrypted_data,
            } => self.handle_push_asset(vault_hash, asset_hash, encrypted_data).await,

            MailboxRequest::PullAsset { asset_hash } => {
                self.handle_pull_asset(vault_hash, asset_hash).await
            }

            MailboxRequest::PushDelete { doc_hash } => {
                self.handle_push_delete(vault_hash, device_id, doc_hash).await
            }

            MailboxRequest::PushTrashMeta { entries } => {
                self.handle_push_trash_meta(vault_hash, device_id, entries)
            }

            MailboxRequest::PullTrashMeta => {
                self.handle_pull_trash_meta(vault_hash)
            }

            MailboxRequest::PushRestore { doc_hash } => {
                self.handle_push_restore(vault_hash, device_id, doc_hash).await
            }

            MailboxRequest::RevokeDevice { device_id: revoked_device_id } => {
                self.handle_revoke_device(vault_hash, &revoked_device_id)
            }

            MailboxRequest::RotateToken { new_mailbox_token } => {
                self.handle_rotate_token(vault_hash, &new_mailbox_token)
            }

            MailboxRequest::Ping => {
                Ok(MailboxResponse::Pong)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Individual request handlers
    // -----------------------------------------------------------------------

    async fn handle_push(
        &self,
        vault_hash: &str,
        device_id: &str,
        doc_hash: [u8; 32],
        encrypted_payload: Vec<u8>,
        payload_hash: [u8; 32],
    ) -> Result<MailboxResponse> {
        // Verify payload integrity.
        let computed = blake3::hash(&encrypted_payload);
        if computed.as_bytes() != &payload_hash {
            return Ok(MailboxResponse::Error {
                message: "payload hash mismatch".to_string(),
            });
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

        let doc_hash_hex = hex::encode(doc_hash);
        let payload_hash_hex = hex::encode(payload_hash);

        // Write blob to disk.
        let blob_filename = format!("{vault_hash}_{doc_hash_hex}_{payload_hash_hex}.blob");
        let blob_path = self.blob_dir.join(&blob_filename);
        tokio::fs::write(&blob_path, &encrypted_payload)
            .await
            .with_context(|| format!("failed to write blob file to {}", blob_path.display()))?;

        let blob_path_str = blob_path
            .to_str()
            .context("blob path is not valid UTF-8")?
            .to_string();

        let seq = self.db.push_entry(
            vault_hash,
            &doc_hash_hex,
            device_id,
            &blob_path_str,
            blob_size,
            &payload_hash_hex,
            false,
        )?;

        info!(
            vault = vault_hash,
            device = device_id,
            seq = seq,
            doc = doc_hash_hex,
            size = blob_size,
            "document pushed"
        );

        self.notify_subscribers(vault_hash, seq).await;
        self.notify_subscribers(vault_hash, seq).await;
        Ok(MailboxResponse::PushOk { seq })
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

        // Write asset blob to disk.
        let blob_filename = format!("{vault_hash}_asset_{asset_hash_hex}.blob");
        let blob_path = self.blob_dir.join(&blob_filename);

        tokio::fs::write(&blob_path, &encrypted_data)
            .await
            .with_context(|| format!("failed to write asset blob to {}", blob_path.display()))?;

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
                let data = tokio::fs::read(&blob_path).await.with_context(|| {
                    format!("failed to read asset blob from {}", blob_path)
                })?;
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
        vault_hash: &str,
        device_id: &str,
        doc_hash: [u8; 32],
    ) -> Result<MailboxResponse> {
        let doc_hash_hex = hex::encode(doc_hash);
        let payload_hash_hex = hex::encode([0u8; 32]); // Tombstone has no payload.

        // Tombstone entry — no blob on disk.
        let tombstone_path = "(tombstone)";
        let seq = self.db.push_entry(
            vault_hash,
            &doc_hash_hex,
            device_id,
            tombstone_path,
            0,
            &payload_hash_hex,
            true,
        )?;

        info!(
            vault = vault_hash,
            device = device_id,
            seq = seq,
            doc = doc_hash_hex,
            "delete tombstone pushed"
        );

        self.notify_subscribers(vault_hash, seq).await;
        Ok(MailboxResponse::DeleteOk { seq })
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

    fn handle_pull_trash_meta(
        &self,
        vault_hash: &str,
    ) -> Result<MailboxResponse> {
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
        let seq = self.db.push_entry(
            vault_hash,
            &doc_hash_hex,
            device_id,
            "(restore)",
            0,
            &payload_hash_hex,
            false,
        )?;

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
        self.db.delete_cursor(vault_hash, revoked_device_id)?;

        info!(
            vault = vault_hash,
            revoked_device = revoked_device_id,
            "device cursor revoked"
        );

        Ok(MailboxResponse::RevokeOk)
    }

    fn handle_rotate_token(
        &self,
        vault_hash: &str,
        new_mailbox_token: &[u8],
    ) -> Result<MailboxResponse> {
        self.db.update_vault_token(vault_hash, new_mailbox_token)?;

        info!(
            vault = vault_hash,
            "mailbox token rotated"
        );

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
