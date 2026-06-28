//! DirectP2PTransport — device-to-device sync transport.
//!
//! Connects to a paired device's Iroh endpoint using the same mailbox protocol
//! as `SynabitServerTransport`, but targets a peer (not a server). The peer
//! runs a `P2PSyncHandler` that speaks the mailbox protocol.
//!
//! ## Key differences from SynabitServerTransport
//!
//! - Uses an **existing** `iroh::Endpoint` (from `PersistentEndpoint`) instead
//!   of creating its own ephemeral endpoint.
//! - Connects by `EndpointId` only — Iroh's relay + DNS discovery (N0 preset)
//!   resolves the peer's address. No socket address needed.
//! - Uses the P2P sync ALPN (`synabit/p2p-sync/1`) instead of the server
//!   mailbox ALPN.

use async_trait::async_trait;
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::sync::protocol::{
    read_message, write_message, MailboxRequest, MailboxResponse,
};
use crate::sync::{RemoteSyncEntry, SyncTransport};

use super::handler::P2P_SYNC_ALPN;

/// A connected session to a paired device.
struct P2PSession {
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
}

/// Transport for direct device-to-device P2P sync.
///
/// Connects to a paired device's `PersistentEndpoint` and exchanges
/// mailbox protocol messages. The peer runs a `P2PSyncHandler` on the
/// other end.
///
/// ## Usage
///
/// ```rust,ignore
/// let transport = DirectP2PTransport::new(
///     endpoint.endpoint_cloned(),  // from PersistentEndpoint
///     peer_endpoint_id,            // peer's public key
///     &e2ee_key,
///     "device-uuid-here",
/// );
///
/// transport.authenticate().await?;
/// let entries = transport.pull_since(0).await?;
/// ```
pub struct DirectP2PTransport {
    /// Shared Iroh endpoint (from PersistentEndpoint)
    endpoint: iroh::Endpoint,
    /// Peer's EndpointId (public key)
    peer_id: iroh::EndpointId,
    /// BLAKE3(e2ee_key) — vault identifier
    vault_hash: [u8; 32],
    /// blake3::derive_key("synabit-mailbox-v1", &e2ee_key) — auth token
    mailbox_token: [u8; 32],
    /// Stable device identifier
    device_id: String,
    /// Local LAN address if discovered via mDNS
    lan_address: Option<std::net::SocketAddr>,
    /// Active session (connection + stream), lazily established
    session: Arc<Mutex<Option<P2PSession>>>,
}

impl std::fmt::Debug for DirectP2PTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirectP2PTransport")
            .field("peer_id", &format!("{}", self.peer_id.fmt_short()))
            .field("device_id", &self.device_id)
            .field("lan_address", &self.lan_address)
            .finish()
    }
}

impl DirectP2PTransport {
    /// Create a new direct P2P transport.
    ///
    /// # Arguments
    ///
    /// * `endpoint` — Iroh endpoint from `PersistentEndpoint` (cheap clone)
    /// * `peer_id` — the peer device's public key
    /// * `e2ee_key` — the vault's E2EE key (for deriving auth credentials)
    /// * `device_id` — this device's stable UUID
    /// * `lan_address` — optional direct IP address discovered via mDNS
    pub fn new(
        endpoint: iroh::Endpoint,
        peer_id: iroh::EndpointId,
        e2ee_key: &[u8; 32],
        device_id: &str,
        lan_address: Option<std::net::SocketAddr>,
    ) -> Self {
        let vault_hash: [u8; 32] = *blake3::hash(e2ee_key).as_bytes();
        let mailbox_token: [u8; 32] = blake3::derive_key("synabit-mailbox-v1", e2ee_key);

        info!(
            "DirectP2PTransport created, peer={}",
            peer_id.fmt_short()
        );

        Self {
            endpoint,
            peer_id,
            vault_hash,
            mailbox_token,
            device_id: device_id.to_string(),
            lan_address,
            session: Arc::new(Mutex::new(None)),
        }
    }

    /// Ensure we have an active session. If not, connect and authenticate.
    async fn ensure_session(&self) -> AppResult<()> {
        let mut session = self.session.lock().await;
        if session.is_some() {
            return Ok(());
        }

        info!("Connecting to peer {}...", self.peer_id.fmt_short());

        // Connect to the peer via Iroh relay/discovery
        let mut peer_addr = iroh::EndpointAddr::new(self.peer_id);
        if let Some(addr) = self.lan_address {
            peer_addr = peer_addr.with_ip_addr(addr);
        }
        
        let conn: iroh::endpoint::Connection = self
            .endpoint
            .connect(peer_addr, P2P_SYNC_ALPN)
            .await
            .map_err(|e| {
                AppError::General(format!(
                    "connect to peer {} failed: {}",
                    self.peer_id.fmt_short(),
                    e
                ))
            })?;

        // Open a bidirectional stream for the mailbox protocol
        let (send, recv): (iroh::endpoint::SendStream, iroh::endpoint::RecvStream) = conn
            .open_bi()
            .await
            .map_err(|e| AppError::General(format!("open stream failed: {}", e)))?;

        *session = Some(P2PSession { send, recv });

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

        let auth = MailboxRequest::Auth {
            vault_hash: self.vault_hash,
            mailbox_token: self.mailbox_token,
            device_id: self.device_id.clone(),
        };

        write_message(&mut s.send, &auth)
            .await
            .map_err(|e| AppError::General(format!("auth send failed: {}", e)))?;

        let response: MailboxResponse = read_message(&mut s.recv)
            .await
            .map_err(|e| AppError::General(format!("auth recv failed: {}", e)))?
            .ok_or_else(|| {
                AppError::General("peer closed connection during auth".to_string())
            })?;

        match response {
            MailboxResponse::AuthOk => {
                info!("Authenticated with peer {}", self.peer_id.fmt_short());
                Ok(())
            }
            MailboxResponse::AuthFailed { reason } => {
                error!(
                    "Peer {} auth failed: {}",
                    self.peer_id.fmt_short(),
                    reason
                );
                drop(session);
                *self.session.lock().await = None;
                Err(AppError::AuthFailed(format!(
                    "peer auth failed: {}",
                    reason
                )))
            }
            other => {
                error!("Unexpected auth response from peer: {:?}", other);
                Err(AppError::General("unexpected auth response".to_string()))
            }
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
            warn!("P2P request send failed, reconnecting: {}", e);
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
            let resp: MailboxResponse = read_message(&mut s.recv)
                .await
                .map_err(|e| AppError::SyncError(format!("retry recv failed: {}", e)))?
                .ok_or_else(|| {
                    AppError::SyncError("peer closed after retry".to_string())
                })?;
            return Ok(resp);
        }

        // Read response
        let resp: MailboxResponse = read_message(&mut s.recv)
            .await
            .map_err(|e| AppError::SyncError(format!("recv failed: {}", e)))?
            .ok_or_else(|| AppError::SyncError("peer closed connection".to_string()))?;

        Ok(resp)
    }

    /// Close the connection gracefully.
    pub async fn close(&self) {
        let mut session = self.session.lock().await;
        *session = None;
        info!(
            "DirectP2PTransport to peer {} closed",
            self.peer_id.fmt_short()
        );
    }
}

// ---------------------------------------------------------------------------
// SyncTransport implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl SyncTransport for DirectP2PTransport {
    fn provider_name(&self) -> &str {
        "Direct P2P"
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
                debug!("P2P pushed doc, assigned seq={}", seq);
                Ok(seq)
            }
            MailboxResponse::Error { message } => {
                Err(AppError::SyncError(format!("p2p push failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected p2p push response: {:?}",
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
                debug!(
                    "P2P pulled {} entries since seq={}",
                    entries.len(),
                    since_seq
                );
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
                Err(AppError::SyncError(format!("p2p pull failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected p2p pull response: {:?}",
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
                Err(AppError::SyncError(format!("p2p ack failed: {}", message)))
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
                Err(AppError::SyncError(format!("p2p push asset failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected p2p push asset response: {:?}",
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
                Err(AppError::SyncError(format!("p2p pull asset failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected p2p pull asset response: {:?}",
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
                Err(AppError::SyncError(format!("p2p push delete failed: {}", message)))
            }
            other => Err(AppError::SyncError(format!(
                "unexpected p2p delete response: {:?}",
                other
            ))),
        }
    }

    async fn is_available(&self) -> bool {
        let session = self.session.lock().await;
        session.is_some()
    }
}
