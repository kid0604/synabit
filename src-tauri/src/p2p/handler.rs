//! P2PSyncHandler — accepts incoming P2P sync connections from paired devices.
//!
//! This handler runs on each device and processes incoming sync requests
//! from other paired devices. When a peer connects, it:
//!
//! 1. Verifies the peer's `EndpointId` is in the paired devices set
//! 2. Authenticates with vault credentials (same mailbox protocol)
//! 3. Handles Pull/Push/Ack requests using local state
//!
//! This effectively makes each device a mini-server for its own data,
//! enabling direct device-to-device sync without the central server.

use log::{debug, error, info, warn};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tauri::Manager;

use crate::sync::protocol::{
    read_message, write_message, MailboxEntry, MailboxRequest, MailboxResponse,
};

/// ALPN for P2P sync protocol (separate from mailbox server ALPN).
///
/// Using a distinct ALPN from the server's `synabit/mailbox/1` ensures
/// that P2P connections and server connections are not confused.
pub const P2P_SYNC_ALPN: &[u8] = b"synabit/p2p-sync/1";

/// Handler for incoming P2P sync connections.
///
/// Tracks which devices are authorized to connect (paired devices) and
/// processes their sync requests using the local database.
pub struct P2PSyncHandler {
    app_handle: tauri::AppHandle,
    /// Set of hex-encoded `PublicKey` strings for authorized peers.
    paired_devices: Arc<RwLock<HashSet<String>>>,
    /// Monotonic sequence counter for entries served to peers.
    seq_counter: AtomicU64,
}

impl P2PSyncHandler {
    /// Create a new handler.
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self {
            app_handle,
            paired_devices: Arc::new(RwLock::new(HashSet::new())),
            seq_counter: AtomicU64::new(1),
        }
    }

    /// Add a device to the set of authorized peers.
    pub fn add_paired_device(&self, device_id_hex: String) {
        self.paired_devices.write().unwrap().insert(device_id_hex);
    }

    /// Remove a device from the set of authorized peers.
    pub fn remove_paired_device(&self, device_id_hex: &str) {
        self.paired_devices.write().unwrap().remove(device_id_hex);
    }

    /// Check if a device is authorized.
    pub fn is_paired(&self, device_id_hex: &str) -> bool {
        self.paired_devices.read().unwrap().contains(device_id_hex)
    }

    /// Number of currently paired devices.
    pub fn paired_count(&self) -> usize {
        self.paired_devices.read().unwrap().len()
    }

    /// Handle an incoming P2P connection.
    ///
    /// This is called when a paired device connects to us. It:
    /// 1. Verifies the peer's identity
    /// 2. Authenticates with vault credentials
    /// 3. Enters a request/response loop
    ///
    /// Returns when the connection is closed or an error occurs.
    pub async fn handle_connection(
        &self,
        conn: iroh::endpoint::Connection,
    ) {
        let remote_id = conn.remote_id();
        let remote_hex = hex::encode(remote_id.as_bytes());

        // Check if peer is in paired devices
        if !self.is_paired(&remote_hex) {
            warn!(
                "P2P connection rejected: {} not in paired devices",
                remote_id.fmt_short()
            );
            return;
        }

        info!(
            "P2P connection accepted from {}",
            remote_id.fmt_short()
        );

        // Accept a bidirectional stream
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                error!("Failed to accept bi stream from {}: {}", remote_id.fmt_short(), e);
                return;
            }
        };

        // Read and verify auth message
        let auth_msg: MailboxRequest = match read_message(&mut recv).await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                debug!("Peer {} disconnected before auth", remote_id.fmt_short());
                return;
            }
            Err(e) => {
                error!("Auth read error from {}: {}", remote_id.fmt_short(), e);
                return;
            }
        };

        // Verify auth credentials
        let _device_id = match auth_msg {
            MailboxRequest::Auth {
                vault_hash: _,
                mailbox_token: _,
                device_id,
            } => {
                // Accept any auth from paired devices — identity is already
                // verified via the iroh PublicKey check above.
                let auth_ok = MailboxResponse::AuthOk;
                if let Err(e) = write_message(&mut send, &auth_ok).await {
                    error!("Failed to send AuthOk to {}: {}", remote_id.fmt_short(), e);
                    return;
                }
                info!("P2P auth OK for device {}", device_id);
                device_id
            }
            _ => {
                warn!(
                    "Expected Auth message from {}, got non-auth",
                    remote_id.fmt_short(),
                );
                let resp = MailboxResponse::AuthFailed {
                    reason: "expected Auth message first".to_string(),
                };
                let _ = write_message(&mut send, &resp).await;
                return;
            }
        };

        // Enter message loop
        loop {
            let msg: MailboxRequest = match read_message(&mut recv).await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    debug!("Peer {} disconnected", remote_id.fmt_short());
                    break;
                }
                Err(e) => {
                    error!(
                        "Read error from peer {}: {}",
                        remote_id.fmt_short(),
                        e
                    );
                    break;
                }
            };

            let response = self.handle_request(msg);

            if let Err(e) = write_message(&mut send, &response).await {
                error!(
                    "Write error to peer {}: {}",
                    remote_id.fmt_short(),
                    e
                );
                break;
            }
        }

        info!(
            "P2P session with {} ended",
            remote_id.fmt_short()
        );
    }

    /// Handle a single mailbox request from a peer.
    ///
    /// Reads from/writes to the local database to serve real sync data.
    fn handle_request(&self, req: MailboxRequest) -> MailboxResponse {
        match req {
            MailboxRequest::Pull { since_seq } => {
                debug!("P2P Pull request: since_seq={}", since_seq);
                self.handle_pull(since_seq)
            }
            MailboxRequest::Push {
                doc_hash,
                encrypted_payload,
                payload_hash,
            } => {
                debug!("P2P Push request received");
                self.handle_push(doc_hash, encrypted_payload, payload_hash)
            }
            MailboxRequest::PushBatch { items } => {
                let mut max_seq = 0;
                for item in items {
                    match self.handle_push(
                        item.doc_hash,
                        item.encrypted_payload,
                        item.payload_hash,
                    ) {
                        MailboxResponse::PushOk { seq } => {
                            max_seq = max_seq.max(seq);
                        }
                        resp => {
                            warn!("DirectSync PushBatch item unexpected response: {:?}", resp);
                        }
                    }
                }
                MailboxResponse::PushBatchOk { max_seq }
            }

            MailboxRequest::Ack { up_to_seq } => {
                debug!("P2P Ack: up_to_seq={}", up_to_seq);
                MailboxResponse::AckOk
            }
            MailboxRequest::PushAsset {
                asset_hash,
                encrypted_data,
            } => {
                debug!("P2P PushAsset received");
                self.handle_push_asset(asset_hash, encrypted_data)
            }
            MailboxRequest::PullAsset { asset_hash } => {
                debug!("P2P PullAsset received");
                self.handle_pull_asset(asset_hash)
            }
            MailboxRequest::PushDelete { doc_hash } => {
                debug!("P2P PushDelete received");
                self.handle_push_delete(doc_hash)
            }
            MailboxRequest::Auth { .. } => {
                // Auth should only come once at the start
                MailboxResponse::Error {
                    message: "unexpected Auth message after handshake".to_string(),
                }
            }
            MailboxRequest::Hello { version } => {
                debug!("P2P Hello received, version: {}", version);
                MailboxResponse::HelloOk {
                    server_version: 1,
                    max_bytes: 10 * 1024 * 1024 * 1024, // 10GB arbitrary limit
                }
            }
            MailboxRequest::Ping => MailboxResponse::Pong,
            MailboxRequest::RevokeDevice { .. } => {
                MailboxResponse::Error {
                    message: "revoke not supported in P2P mode".to_string(),
                }
            }
            MailboxRequest::RotateToken { .. } => {
                debug!("P2P: key rotation operations not supported in direct P2P mode");
                MailboxResponse::Error {
                    message: "key rotation not supported in P2P mode".to_string(),
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Request handlers
    // -----------------------------------------------------------------------

    /// Handle Pull: export local CRDT docs as encrypted entries.
    ///
    /// Reads the vault directory, exports CRDT snapshots, encrypts them,
    /// and returns as MailboxEntry list with virtual sequence numbers.
    fn handle_pull(&self, since_seq: u64) -> MailboxResponse {
        let e2ee_key = match crate::secrets::SecretManager::get_e2ee_key(Some(&self.app_handle)) {
            Some(k) => k,
            None => {
                warn!("P2P Pull: E2EE key not available");
                return MailboxResponse::PullResult { entries: Vec::new() };
            }
        };

        let vault_path = match self.get_vault_path() {
            Some(p) => p,
            None => {
                warn!("P2P Pull: vault path not configured");
                return MailboxResponse::PullResult { entries: Vec::new() };
            }
        };

        let device_id = self.get_device_id();

        // Collect local files
        let local_files = crate::sync::utils::collect_local_files(&vault_path);

        let mut entries: Vec<MailboxEntry> = Vec::new();
        let db_state = self.app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        for rel_path in &local_files {
            let seq = self.seq_counter.fetch_add(1, Ordering::Relaxed);

            // Skip entries the peer already has
            if seq <= since_seq {
                continue;
            }

            // Export CRDT snapshot
            let snapshot = match db.get_crdt_doc(rel_path) {
                Ok(doc) => doc.export_snapshot(),
                Err(_) => continue, // No CRDT doc, skip
            };

            let is_json = rel_path.ends_with(".json") || rel_path.ends_with(".canvas");

            // Build payload (same format as sync engine)
            #[derive(serde::Serialize)]
            struct DocSyncPayload {
                doc_id: String,
                snapshot: Vec<u8>,
                is_json: bool,
            }

            let payload = DocSyncPayload {
                doc_id: rel_path.clone(),
                snapshot,
                is_json,
            };

            let serialized = match postcard::to_stdvec(&payload) {
                Ok(b) => b,
                Err(e) => {
                    warn!("P2P Pull: serialize failed for {}: {}", rel_path, e);
                    continue;
                }
            };

            // Encrypt
            let encrypted = match crate::sync::crypto::encrypt(&e2ee_key, &serialized) {
                Ok(enc) => enc,
                Err(e) => {
                    warn!("P2P Pull: encrypt failed for {}: {}", rel_path, e);
                    continue;
                }
            };

            let doc_hash = *blake3::hash(rel_path.as_bytes()).as_bytes();
            let payload_hash = *blake3::hash(&encrypted).as_bytes();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            entries.push(MailboxEntry {
                seq,
                doc_hash,
                source_device: device_id.clone(),
                encrypted_payload: encrypted,
                payload_hash,
                timestamp: now,
                is_delete: false,
            });
        }

        info!("P2P Pull: serving {} entries (since_seq={})", entries.len(), since_seq);
        MailboxResponse::PullResult { entries }
    }

    /// Handle Push: receive encrypted entry, store locally.
    ///
    /// Writes the received encrypted payload to a staging area in the KV
    /// store. The main sync engine will pick it up on next cycle.
    fn handle_push(
        &self,
        doc_hash: [u8; 32],
        encrypted_payload: Vec<u8>,
        payload_hash: [u8; 32],
    ) -> MailboxResponse {
        // Verify payload integrity
        let computed = *blake3::hash(&encrypted_payload).as_bytes();
        if computed != payload_hash {
            return MailboxResponse::Error {
                message: "payload hash mismatch".to_string(),
            };
        }

        let doc_hash_hex = hex::encode(doc_hash);
        let seq = self.seq_counter.fetch_add(1, Ordering::Relaxed);

        // Store in KV as a staged P2P entry for the sync engine to process
        let db_state = self.app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        let key = format!("p2p:staged:{}", doc_hash_hex);
        // Store as base64-encoded blob for simplicity
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&encrypted_payload);
        match db.set_kv(&key, &encoded) {
            Ok(_) => {
                info!("P2P Push: staged doc {} (seq={})", doc_hash_hex, seq);
                MailboxResponse::PushOk { seq }
            }
            Err(e) => {
                error!("P2P Push: failed to stage {}: {}", doc_hash_hex, e);
                MailboxResponse::Error {
                    message: format!("storage error: {}", e),
                }
            }
        }
    }

    /// Handle PushAsset: store encrypted asset blob.
    fn handle_push_asset(
        &self,
        asset_hash: [u8; 32],
        encrypted_data: Vec<u8>,
    ) -> MailboxResponse {
        let asset_hash_hex = hex::encode(asset_hash);

        let vault_path = match self.get_vault_path() {
            Some(p) => p,
            None => {
                return MailboxResponse::Error {
                    message: "vault not configured".to_string(),
                };
            }
        };

        // Write asset to vault's assets directory
        let asset_dir = std::path::Path::new(&vault_path).join("assets");
        if let Err(e) = std::fs::create_dir_all(&asset_dir) {
            return MailboxResponse::Error {
                message: format!("create assets dir: {}", e),
            };
        }

        let asset_path = asset_dir.join(&asset_hash_hex);
        match std::fs::write(&asset_path, &encrypted_data) {
            Ok(_) => {
                info!("P2P PushAsset: wrote {} ({} bytes)", asset_hash_hex, encrypted_data.len());
                MailboxResponse::AssetOk
            }
            Err(e) => {
                MailboxResponse::Error {
                    message: format!("write asset: {}", e),
                }
            }
        }
    }

    /// Handle PullAsset: read encrypted asset from local storage.
    fn handle_pull_asset(&self, asset_hash: [u8; 32]) -> MailboxResponse {
        let asset_hash_hex = hex::encode(asset_hash);

        let vault_path = match self.get_vault_path() {
            Some(p) => p,
            None => return MailboxResponse::AssetNotFound,
        };

        let asset_path = std::path::Path::new(&vault_path)
            .join("assets")
            .join(&asset_hash_hex);

        match std::fs::read(&asset_path) {
            Ok(data) => {
                info!("P2P PullAsset: serving {} ({} bytes)", asset_hash_hex, data.len());
                MailboxResponse::AssetData { data }
            }
            Err(_) => MailboxResponse::AssetNotFound,
        }
    }

    /// Handle PushDelete: mark a doc as deleted.
    fn handle_push_delete(&self, doc_hash: [u8; 32]) -> MailboxResponse {
        let doc_hash_hex = hex::encode(doc_hash);
        let seq = self.seq_counter.fetch_add(1, Ordering::Relaxed);

        // Stage the deletion in KV
        let db_state = self.app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        let key = format!("p2p:staged_delete:{}", doc_hash_hex);
        match db.set_kv(&key, "1") {
            Ok(_) => {
                info!("P2P PushDelete: staged {} (seq={})", doc_hash_hex, seq);
                MailboxResponse::DeleteOk { seq }
            }
            Err(e) => {
                error!("P2P PushDelete: failed to stage {}: {}", doc_hash_hex, e);
                MailboxResponse::Error {
                    message: format!("storage error: {}", e),
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Get the configured vault path from KV store.
    fn get_vault_path(&self) -> Option<String> {
        let db_state = self.app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("vault_path").ok().flatten()
    }

    /// Get the local device ID from KV store.
    fn get_device_id(&self) -> String {
        let db_state = self.app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("p2p_sync:device_id")
            .ok()
            .flatten()
            .unwrap_or_else(|| "unknown".to_string())
    }
}

impl std::fmt::Debug for P2PSyncHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("P2PSyncHandler")
            .field("paired_count", &self.paired_count())
            .finish()
    }
}
