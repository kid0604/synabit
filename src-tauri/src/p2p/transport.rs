//! SynabitServerTransport — client-side transport to Synabit Sync Server.
//!
//! Connects to the Sync Server's Mailbox protocol over Iroh QUIC and
//! implements the `SyncTransport` trait. This is the primary sync transport
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
use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::sync::protocol::{
    read_message, write_message, MailboxRequest, MailboxResponse, MAILBOX_ALPN,
};
use crate::sync::{RemoteSyncEntry, SyncTransport};

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
/// let transport = SynabitServerTransport::new(
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
pub struct SynabitServerTransport {
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

impl std::fmt::Debug for SynabitServerTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SynabitServerTransport")
            .field("server_addr", &self.server_addr)
            .field("device_id", &self.device_id)
            .finish()
    }
}

impl SynabitServerTransport {
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
            "SynabitServerTransport created, target={}, server_id={}",
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
        info!("SynabitServerTransport closed");
    }
}

impl Drop for SynabitServerTransport {
    fn drop(&mut self) {
        if let Ok(mut session) = self.session.try_lock() {
            *session = None;
        }
    }
}

// ---------------------------------------------------------------------------
// SyncTransport implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl SyncTransport for SynabitServerTransport {
    fn provider_name(&self) -> &str {
        "Synabit Sync Server"
    }

    async fn authenticate(&self) -> AppResult<()> {
        self.ensure_session().await
    }

    async fn disconnect(&self) -> AppResult<()> {
        self.close().await;
        Ok(())
    }

    async fn push_doc(
        &self,
        doc_hash: &[u8; 32],
        encrypted_payload: Vec<u8>,
    ) -> AppResult<u64> {
        let payload_hash: [u8; 32] = *blake3::hash(&encrypted_payload).as_bytes();

        let resp = self
            .request(&MailboxRequest::Push {
                doc_hash: *doc_hash,
                encrypted_payload,
                payload_hash,
            })
            .await?;

        match resp {
            MailboxResponse::PushOk { seq } => {
                debug!("Pushed doc, assigned seq={}", seq);
                Ok(seq)
            }
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("push failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected push response: {:?}",
                other
            ))),
        }
    }

    async fn pull_since(&self, since_seq: u64) -> AppResult<Vec<RemoteSyncEntry>> {
        let resp = self
            .request(&MailboxRequest::Pull { since_seq })
            .await?;

        match resp {
            MailboxResponse::PullResult { entries } => {
                debug!("Pulled {} entries since seq={}", entries.len(), since_seq);
                Ok(entries
                    .into_iter()
                    .map(|e| RemoteSyncEntry {
                        seq: e.seq,
                        doc_hash: e.doc_hash,
                        source_device: e.source_device,
                        encrypted_payload: e.encrypted_payload,
                        payload_hash: e.payload_hash,
                        timestamp: e.timestamp,
                        is_delete: e.is_delete,
                    })
                    .collect())
            }
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("pull failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected pull response: {:?}",
                other
            ))),
        }
    }

    async fn ack(&self, up_to_seq: u64) -> AppResult<()> {
        let resp = self
            .request(&MailboxRequest::Ack { up_to_seq })
            .await?;

        match resp {
            MailboxResponse::AckOk => Ok(()),
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("ack failed: {}", message)))
            }
            _ => Ok(()), // Non-critical
        }
    }

    async fn push_asset(
        &self,
        asset_hash: &[u8; 32],
        encrypted_data: Vec<u8>,
    ) -> AppResult<()> {
        let resp = self
            .request(&MailboxRequest::PushAsset {
                asset_hash: *asset_hash,
                encrypted_data,
            })
            .await?;

        match resp {
            MailboxResponse::AssetOk => Ok(()),
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("push asset failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected push asset response: {:?}",
                other
            ))),
        }
    }

    async fn pull_asset(&self, asset_hash: &[u8; 32]) -> AppResult<Option<Vec<u8>>> {
        let resp = self
            .request(&MailboxRequest::PullAsset {
                asset_hash: *asset_hash,
            })
            .await?;

        match resp {
            MailboxResponse::AssetData { data } => Ok(Some(data)),
            MailboxResponse::AssetNotFound => Ok(None),
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("pull asset failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected pull asset response: {:?}",
                other
            ))),
        }
    }

    async fn push_delete(&self, doc_hash: &[u8; 32]) -> AppResult<u64> {
        let resp = self
            .request(&MailboxRequest::PushDelete {
                doc_hash: *doc_hash,
            })
            .await?;

        match resp {
            MailboxResponse::DeleteOk { seq } => Ok(seq),
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("push delete failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected delete response: {:?}",
                other
            ))),
        }
    }

    async fn ping(&self) -> AppResult<()> {
        let resp = self.request(&MailboxRequest::Ping).await?;
        match resp {
            MailboxResponse::Pong => Ok(()),
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("ping failed: {}", message)))
            }
            _ => Err(AppError::SyncError("Unexpected response to Ping".into())),
        }
    }

    async fn is_available(&self) -> bool {
        let session = self.session.lock().await;
        match session.as_ref() {
            Some(s) => s.conn.close_reason().is_none(),
            None => false,
        }
    }
}
