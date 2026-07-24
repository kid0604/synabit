//! SynabitServerAdapter — client-side transport to Synabit Sync Server.
//!
//! Connects to the Sync Server's Mailbox protocol over Iroh QUIC and
//! implements the `SyncAdapter` trait. This is the primary sync transport
//! that replaces Google Drive for always-available push/pull.
//!
//! ## Connection model
//!
//! The client needs the server's **EndpointId** (public key) to establish a
//! mutually authenticated QUIC connection. The server publishes its EndpointId
//! via the `/health` HTTP endpoint, or it can be configured statically.
//!
//! For direct IP connections (no relay), we build an `EndpointAddr` from the
//! server's EndpointId + its socket address.

use async_trait::async_trait;
use iroh::EndpointAddr;
use log::{error, info, warn};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::sync::protocol::{
    write_message, MailboxRequest, MailboxResponse, MAILBOX_ALPN,
};
use crate::sync::core::types::{SyncOperation};
use crate::sync::adapter::{SyncAdapter, PushResult, PullResult, RemoteEntry};

/// A connected session to the Sync Server.
struct MailboxSession {
    send: iroh::endpoint::SendStream,
    recv: tokio::sync::mpsc::Receiver<Result<MailboxResponse, AppError>>,
    conn: iroh::endpoint::Connection,
}

/// Client transport that connects to the Synabit Sync Server.
///
/// ## Usage
///
/// ```rust,ignore
/// let transport = SynabitServerAdapter::new(
///     "1.2.3.4:4433",       // server socket address
///     server_endpoint_id,    // server's public key (EndpointId)
///     &e2ee_key,
///     "device-uuid-here",
/// ).await?;
///
/// transport.authenticate().await?;
/// transport.push_doc(&doc_hash, encrypted_data).await?;
/// let entries = transport.pull_since(0).await?;
/// ```
pub struct SynabitServerAdapter {
    /// Iroh endpoint for QUIC connections
    endpoint: iroh::Endpoint,
    /// Server's EndpointAddr (EndpointId + optional direct address)
    server_addr: EndpointAddr,
    /// BLAKE3(e2ee_key) — vault identifier
    vault_hash: [u8; 32],
    /// blake3::derive_key("synabit-mailbox-v1", &e2ee_key) — auth token
    mailbox_token: [u8; 32],
    /// Stable device identifier
    device_id: String,
    /// Active session (connection + stream), lazily established
    session: Arc<Mutex<Option<MailboxSession>>>,
    /// Tauri AppHandle for emitting events
    app_handle: Option<tauri::AppHandle>,
}

impl std::fmt::Debug for SynabitServerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SynabitServerAdapter")
            .field("server_addr", &self.server_addr)
            .field("device_id", &self.device_id)
            .finish()
    }
}

impl SynabitServerAdapter {
    /// Create a new transport from a server socket address and EndpointId.
    ///
    /// This binds an Iroh endpoint but does NOT connect to the server yet.
    /// Call `authenticate()` (or any push/pull method) to establish the connection.
    pub async fn new(
        server_socket: &str,
        server_id: iroh::EndpointId,
        e2ee_key: &[u8; 32],
        device_id: &str,
        app_handle: Option<tauri::AppHandle>,
    ) -> AppResult<Self> {
        let addr = tokio::net::lookup_host(server_socket)
            .await
            .map_err(|e| AppError::General(format!("failed to resolve server address: {}", e)))?
            .next()
            .ok_or_else(|| AppError::General("could not resolve server address".into()))?;

        // Build EndpointAddr with the server's public key + direct socket address
        let server_addr = EndpointAddr::new(server_id).with_ip_addr(addr);

        // Derive auth credentials from E2EE key
        let vault_hash: [u8; 32] = *blake3::hash(e2ee_key).as_bytes();
        let mailbox_token: [u8; 32] = blake3::derive_key("synabit-mailbox-v1", e2ee_key);

        // Bind a minimal Iroh endpoint (UDP socket for QUIC)
        let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
            .bind()
            .await
            .map_err(|e| AppError::General(format!("failed to bind Iroh endpoint: {}", e)))?;

        info!(
            "SynabitServerAdapter created, target={}, server_id={}",
            addr,
            server_id.fmt_short()
        );

        Ok(Self {
            endpoint,
            server_addr,
            vault_hash,
            mailbox_token,
            device_id: device_id.to_string(),
            session: Arc::new(Mutex::new(None)),
            app_handle,
        })
    }

    /// Ensure we have an active session. If not, connect and authenticate.
    async fn ensure_session(&self) -> AppResult<()> {
        let mut session = self.session.lock().await;
        if session.is_some() {
            return Ok(());
        }

        info!("Connecting to Sync Server...");

        // Connect to the server via Iroh QUIC
        let conn: iroh::endpoint::Connection = self
            .endpoint
            .connect(self.server_addr.clone(), MAILBOX_ALPN)
            .await
            .map_err(|e| AppError::General(format!("connect to sync server failed: {}", e)))?;

        // Open a bidirectional stream for the mailbox protocol
        let (send, mut recv): (iroh::endpoint::SendStream, iroh::endpoint::RecvStream) = conn
            .open_bi()
            .await
            .map_err(|e| AppError::General(format!("open stream failed: {}", e)))?;

        let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(10);
        let app_handle = self.app_handle.clone();

        tokio::spawn(async move {
            loop {
                let resp_res = crate::sync::protocol::read_message::<_, MailboxResponse>(&mut recv).await;
                match resp_res {
                    Ok(Some(MailboxResponse::NotifyNewData { trigger_seq })) => {
                        log::info!("Received server push notification: seq={}", trigger_seq);
                        if let Some(app) = &app_handle {
                            use tauri::Emitter;
                            let _ = app.emit("sync-server-push", ());
                        }
                    }
                    Ok(Some(msg)) => {
                        if resp_tx.send(Ok(msg)).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = resp_tx.send(Err(AppError::SyncError("server closed connection".into()))).await;
                        break;
                    }
                    Err(e) => {
                        let _ = resp_tx.send(Err(AppError::SyncError(format!("recv failed: {}", e)))).await;
                        break;
                    }
                }
            }
        });

        *session = Some(MailboxSession { send, recv: resp_rx, conn });

        // Authenticate on the stream
        drop(session); // Release lock before calling send_auth (which re-acquires)
        self.send_auth().await?;

        Ok(())
    }

    /// Send Auth message and verify response.
    async fn send_auth(&self) -> AppResult<()> {
        let mut session = self.session.lock().await;
        let s = session
            .as_mut()
            .ok_or_else(|| AppError::General("no active session".to_string()))?;

        // 2. Send Auth
        let auth = MailboxRequest::Auth {
            vault_hash: self.vault_hash,
            mailbox_token: self.mailbox_token,
            device_id: self.device_id.clone(),
        };

        write_message(&mut s.send, &auth)
            .await
            .map_err(|e| AppError::General(format!("auth send failed: {}", e)))?;

        let response = Self::wait_for_response(&mut s.recv).await?;

        match response {
            MailboxResponse::AuthOk => {
                info!("Authenticated with Sync Server");
                Ok(())
            }
            MailboxResponse::AuthFailed { reason } => {
                error!("Sync Server auth failed: {}", reason);
                drop(session);
                *self.session.lock().await = None;
                Err(AppError::AuthFailed(format!(
                    "sync server auth failed: {}",
                    reason
                )))
            }
            other => {
                error!("Unexpected auth response: {:?}", other);
                Err(AppError::General("unexpected auth response".to_string()))
            }
        }
    }

    /// Read response from the background duplex channel.
    async fn wait_for_response(recv: &mut tokio::sync::mpsc::Receiver<Result<MailboxResponse, AppError>>) -> AppResult<MailboxResponse> {
        match recv.recv().await {
            Some(Ok(msg)) => Ok(msg),
            Some(Err(e)) => Err(e),
            None => Err(AppError::SyncError("channel closed".into())),
        }
    }

    /// Send a request and read the response. Auto-reconnects once on failure.
    async fn request(&self, req: &MailboxRequest) -> AppResult<MailboxResponse> {
        self.ensure_session().await?;

        let mut session = self.session.lock().await;
        let s = session
            .as_mut()
            .ok_or_else(|| AppError::General("no session after ensure".to_string()))?;

        // Send request
        if let Err(e) = write_message(&mut s.send, req).await {
            warn!("Request send failed, reconnecting: {}", e);
            drop(session);
            *self.session.lock().await = None;

            // Retry once after reconnect
            self.ensure_session().await?;
            let mut session = self.session.lock().await;
            let s = session
                .as_mut()
                .ok_or_else(|| AppError::General("no session after reconnect".to_string()))?;
            write_message(&mut s.send, req)
                .await
                .map_err(|e| AppError::SyncError(format!("retry send failed: {}", e)))?;
            
            let resp = Self::wait_for_response(&mut s.recv).await?;
            return Ok(resp);
        }

        // Read response
        let resp = Self::wait_for_response(&mut s.recv).await?;
        Ok(resp)
    }

    /// Close the connection gracefully.
    pub async fn close(&self) {
        let mut session = self.session.lock().await;
        *session = None;
        self.endpoint.close().await;
        info!("SynabitServerAdapter closed");
    }
}

impl Drop for SynabitServerAdapter {
    fn drop(&mut self) {
        if let Ok(mut session) = self.session.try_lock() {
            *session = None;
        }
    }
}

// ---------------------------------------------------------------------------
// SyncAdapter implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl SyncAdapter for SynabitServerAdapter {
    fn name(&self) -> &str {
        "Synabit Sync Server"
    }

    fn adapter_id(&self) -> String {
        format!("server:{}", self.device_id)
    }

    async fn is_connected(&self) -> bool {
        self.session.lock().await.is_some()
    }

    async fn connect(&self) -> AppResult<()> {
        self.ensure_session().await
    }

    async fn disconnect(&self) -> AppResult<()> {
        let mut session = self.session.lock().await;
        *session = None;
        Ok(())
    }

    async fn push(&self, operations: Vec<SyncOperation>) -> AppResult<PushResult> {
        if operations.is_empty() {
            return Ok(PushResult {
                accepted: vec![],
                tx_bytes: 0,
                new_cursor: String::new(),
            });
        }
        
        let mut tx_bytes = 0;
        let mut items = Vec::new();
        let mut delete_ops = Vec::new();

        for op in operations {
            if op.is_delete {
                delete_ops.push(op);
            } else {
                tx_bytes += op.encrypted_payload.len() as u64;
                items.push(crate::sync::protocol::PushBatchItem {
                    doc_hash: op.doc_hash,
                    encrypted_payload: op.encrypted_payload,
                    payload_hash: op.payload_hash,
                });
            }
        }

        let mut max_seq = 0;

        // Push normal items
        if !items.is_empty() {
            let req = MailboxRequest::PushBatch { items };
            let resp = self.request(&req).await?;

            match resp {
                MailboxResponse::PushBatchOk { max_seq: seq } => {
                    max_seq = max_seq.max(seq);
                }
                MailboxResponse::Error { message } => {
                    return Err(AppError::SyncError(message));
                }
                _ => return Err(AppError::SyncError("Unexpected response to PushBatch".into())),
            }
        }

        // Push delete items
        for op in delete_ops {
            let req = MailboxRequest::PushDelete { doc_hash: op.doc_hash };
            let resp = self.request(&req).await?;
            
            match resp {
                MailboxResponse::DeleteOk { seq } => {
                    max_seq = max_seq.max(seq);
                }
                MailboxResponse::Error { message } => {
                    return Err(AppError::SyncError(message));
                }
                _ => return Err(AppError::SyncError("Unexpected response to PushDelete".into())),
            }
        }

        Ok(PushResult {
            accepted: vec![],
            tx_bytes,
            new_cursor: if max_seq > 0 { max_seq.to_string() } else { String::new() },
        })
    }

    async fn pull(&self, since_cursor: &str) -> AppResult<PullResult> {
        let since_seq: u64 = since_cursor.parse().unwrap_or(0);
        let req = MailboxRequest::Pull { since_seq };
        let resp = self.request(&req).await?;

        match resp {
            MailboxResponse::PullResult { entries } => {
                let mut rx_bytes = 0;
                let mut max_seq = since_seq;
                
                let remote_entries: Vec<RemoteEntry> = entries.into_iter().map(|e| {
                    rx_bytes += e.encrypted_payload.len() as u64;
                    max_seq = max_seq.max(e.seq);
                    RemoteEntry {
                        seq: e.seq,
                        doc_hash: e.doc_hash,
                        source_device: e.source_device,
                        encrypted_payload: e.encrypted_payload,
                        payload_hash: e.payload_hash,
                        timestamp: e.timestamp,
                        is_delete: e.is_delete,
                    }
                }).collect();

                Ok(PullResult {
                    entries: remote_entries,
                    rx_bytes,
                    new_cursor: max_seq.to_string(),
                })
            }
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to Pull".into())),
        }
    }

    async fn ack(&self, cursor: &str) -> AppResult<()> {
        let up_to_seq: u64 = cursor.parse().unwrap_or(0);
        let req = MailboxRequest::Ack { up_to_seq };
        let resp = self.request(&req).await?;
        match resp {
            MailboxResponse::AckOk => Ok(()),
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to Ack".into())),
        }
    }

    async fn push_asset(&self, hash: [u8; 32], data: Vec<u8>) -> AppResult<()> {
        let req = MailboxRequest::PushAsset {
            asset_hash: hash,
            encrypted_data: data,
        };
        let resp = self.request(&req).await?;
        match resp {
            MailboxResponse::AssetOk => Ok(()),
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to PushAsset".into())),
        }
    }

    async fn pull_asset(&self, hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
        let req = MailboxRequest::PullAsset { asset_hash: hash };
        let resp = self.request(&req).await?;
        match resp {
            MailboxResponse::AssetData { data } => Ok(Some(data)),
            MailboxResponse::AssetNotFound => Ok(None),
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to PullAsset".into())),
        }
    }
}
