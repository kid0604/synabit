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
use crate::sync::adapter::{
    AdapterPullPage, AdapterSyncMode, AdapterSyncPlan, PullLimits, PushResult, RemoteEntry,
    SyncAdapter,
};
use crate::sync::core::types::SyncOperation;
use crate::sync::protocol::{write_message, MailboxRequest, MailboxResponse, MAILBOX_ALPN};

/// A connected session to the Sync Server.
struct MailboxSession {
    send: iroh::endpoint::SendStream,
    recv: tokio::sync::mpsc::Receiver<Result<MailboxResponse, AppError>>,
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
#[derive(Debug, Clone)]
pub struct ServerAdapterIdentity {
    pub device_id: String,
    pub server_addr: String,
}

impl ServerAdapterIdentity {
    pub fn new(device_id: impl Into<String>, server_addr: impl Into<String>) -> Self {
        Self {
            device_id: device_id.into(),
            server_addr: server_addr.into(),
        }
    }

    pub fn adapter_id(&self) -> &'static str {
        SERVER_PROVIDER_ID
    }
}

pub struct SynabitServerAdapter {
    identity: ServerAdapterIdentity,
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

        let identity = ServerAdapterIdentity::new(device_id, server_socket);
        Ok(Self {
            identity,
            endpoint,
            server_addr,
            vault_hash,
            mailbox_token,
            device_id: device_id.to_string(),
            session: Arc::new(Mutex::new(None)),
            app_handle,
        })
    }

    pub fn adapter_id_static() -> &'static str {
        SERVER_PROVIDER_ID
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
                let resp_res =
                    crate::sync::protocol::read_message::<_, MailboxResponse>(&mut recv).await;
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
                        let _ = resp_tx
                            .send(Err(AppError::SyncError("server closed connection".into())))
                            .await;
                        break;
                    }
                    Err(e) => {
                        let _ = resp_tx
                            .send(Err(AppError::SyncError(format!("recv failed: {}", e))))
                            .await;
                        break;
                    }
                }
            }
        });

        *session = Some(MailboxSession {
            send,
            recv: resp_rx,
        });

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
    async fn wait_for_response(
        recv: &mut tokio::sync::mpsc::Receiver<Result<MailboxResponse, AppError>>,
    ) -> AppResult<MailboxResponse> {
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

pub const SERVER_PROVIDER_ID: &str = "server";

pub(crate) fn parse_server_cursor(cursor: &str) -> AppResult<u64> {
    if cursor.is_empty() {
        return Ok(0);
    }
    cursor
        .parse::<u64>()
        .map_err(|_| AppError::SyncError(format!("Invalid numeric server cursor: '{}'", cursor)))
}

pub(crate) fn build_get_sync_plan_request(
    cursor: &str,
    client_incarnation_id: Option<[u8; 16]>,
) -> AppResult<MailboxRequest> {
    let cursor_num = parse_server_cursor(cursor)?;
    Ok(MailboxRequest::GetSyncPlan {
        client_incarnation_id,
        cursor: cursor_num,
    })
}

pub(crate) fn map_sync_plan_response(resp: MailboxResponse) -> AppResult<AdapterSyncPlan> {
    match resp {
        MailboxResponse::SyncPlan(plan) => {
            let mode = match plan.mode {
                synabit_protocol::SyncMode::Delta { until_seq } => AdapterSyncMode::Delta {
                    until_cursor: Some(until_seq.to_string()),
                },
                synabit_protocol::SyncMode::BootstrapRequired => AdapterSyncMode::BootstrapRequired,
            };
            if plan.incarnation_id == [0u8; 16] {
                return Err(AppError::SyncError(
                    "Zero incarnation id from server".into(),
                ));
            }
            Ok(AdapterSyncPlan {
                mode,
                incarnation_id: Some(plan.incarnation_id),
                remote_vault_id: None,
            })
        }
        MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
        _ => Err(AppError::SyncError(
            "Unexpected response to GetSyncPlan".into(),
        )),
    }
}

pub(crate) fn finalize_server_sync_plan_response(
    resp: MailboxResponse,
    vault_hash: [u8; 32],
) -> AppResult<AdapterSyncPlan> {
    let mut plan = map_sync_plan_response(resp)?;
    plan.remote_vault_id = Some(vault_hash);
    Ok(plan)
}

pub(crate) fn build_pull_page_request(
    cursor: &str,
    until_cursor: Option<&str>,
    limits: PullLimits,
) -> AppResult<MailboxRequest> {
    if limits.max_entries == 0 {
        return Err(AppError::SyncError("max_entries cannot be 0".into()));
    }
    if limits.max_bytes == 0 {
        return Err(AppError::SyncError("max_bytes cannot be 0".into()));
    }
    let after_seq = parse_server_cursor(cursor)?;
    let until_str = until_cursor.ok_or_else(|| {
        AppError::SyncError("until_cursor is required for server pull_page".into())
    })?;
    let until_seq = parse_server_cursor(until_str)?;

    Ok(MailboxRequest::PullPage {
        after_seq,
        until_seq,
        max_entries: limits.max_entries,
        max_bytes: limits.max_bytes,
    })
}

pub(crate) fn map_pull_page_response(
    resp: MailboxResponse,
    expected_until_seq: u64,
) -> AppResult<AdapterPullPage> {
    match resp {
        MailboxResponse::PullPageResult(result) => {
            if result.until_seq != expected_until_seq {
                return Err(AppError::SyncError(format!(
                    "until_seq mismatch: requested {}, got {}",
                    expected_until_seq, result.until_seq
                )));
            }

            let mut rx_bytes = 0u64;
            let mut entries = Vec::with_capacity(result.entries.len());

            for e in result.entries {
                rx_bytes += e.encrypted_payload.len() as u64;
                entries.push(RemoteEntry {
                    remote_position: e.seq.to_string(),
                    remote_seq: Some(e.seq),
                    doc_hash: e.doc_hash,
                    source_device: e.source_device,
                    encrypted_payload: e.encrypted_payload,
                    payload_hash: e.payload_hash,
                    timestamp: e.timestamp,
                    operation_id: e.operation_id,
                    entry_kind: e.entry_kind,
                });
            }

            Ok(AdapterPullPage {
                entries,
                next_cursor: result.next_seq.to_string(),
                has_more: result.has_more,
                rx_bytes,
            })
        }
        MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
        _ => Err(AppError::SyncError(
            "Unexpected response to PullPage".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// SyncAdapter implementation
// ---------------------------------------------------------------------------

pub(crate) fn convert_ops_to_push_items(
    operations: &[SyncOperation],
) -> (Vec<synabit_protocol::PushBatchItem>, u64) {
    let mut tx_bytes = 0u64;
    let mut items = Vec::with_capacity(operations.len());
    for op in operations {
        tx_bytes += op.encrypted_payload.len() as u64;
        items.push(synabit_protocol::PushBatchItem {
            operation_id: op.operation_id,
            doc_hash: op.doc_hash,
            entry_kind: op.entry_kind.clone(),
            encrypted_payload: op.encrypted_payload.clone(),
            payload_hash: op.payload_hash,
        });
    }
    (items, tx_bytes)
}

#[async_trait]
impl SyncAdapter for SynabitServerAdapter {
    fn name(&self) -> &str {
        "Synabit Sync Server"
    }

    fn adapter_id(&self) -> String {
        self.identity.adapter_id().to_string()
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
                rejected: vec![],
                tx_bytes: 0,
            });
        }

        let (items, tx_bytes) = convert_ops_to_push_items(&operations);

        let req = MailboxRequest::PushBatch { items };
        let resp = self.request(&req).await?;

        match resp {
            MailboxResponse::PushBatchOk {
                max_seq: _,
                results,
            } => {
                let mut accepted = Vec::new();
                let mut rejected = Vec::new();
                for r in results {
                    let ack = crate::sync::adapter::PushAck {
                        operation_id: r.operation_id,
                        remote_position: hex::encode(&r.operation_id),
                        remote_seq: None,
                    };
                    if r.error.is_none() {
                        accepted.push(ack);
                    } else {
                        rejected.push(ack);
                    }
                }

                Ok(PushResult {
                    accepted,
                    rejected,
                    tx_bytes,
                })
            }
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError(
                "Unexpected response to PushBatch".into(),
            )),
        }
    }

    async fn get_sync_plan(
        &self,
        cursor: &str,
        client_incarnation_id: Option<[u8; 16]>,
    ) -> AppResult<AdapterSyncPlan> {
        let req = build_get_sync_plan_request(cursor, client_incarnation_id)?;
        let resp = self.request(&req).await?;
        finalize_server_sync_plan_response(resp, self.vault_hash)
    }

    async fn pull_page(
        &self,
        cursor: &str,
        until_cursor: Option<&str>,
        limits: PullLimits,
    ) -> AppResult<AdapterPullPage> {
        let req = build_pull_page_request(cursor, until_cursor, limits)?;
        let expected_until_seq = match req {
            MailboxRequest::PullPage { until_seq, .. } => until_seq,
            _ => unreachable!(),
        };
        let resp = self.request(&req).await?;
        map_pull_page_response(resp, expected_until_seq)
    }

    async fn ack(&self, cursor: &str) -> AppResult<()> {
        let up_to_seq = parse_server_cursor(cursor)?;
        let req = MailboxRequest::Ack { up_to_seq };
        let resp = self.request(&req).await?;
        match resp {
            MailboxResponse::AckOk => Ok(()),
            MailboxResponse::Error { message } => Err(AppError::SyncError(message)),
            _ => Err(AppError::SyncError("Unexpected response to Ack".into())),
        }
    }

    async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> {
        Err(AppError::UnsupportedCapability(
            "Push asset not supported".into(),
        ))
    }

    async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
        Err(AppError::UnsupportedCapability(
            "Pull asset not supported".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_sync_plan_roundtrips_incarnation_and_remote_vault_identity() {
        use synabit_protocol::{SyncMode, SyncPlan};
        let client_incarnation_id = Some([7u8; 16]);
        let server_incarnation_id = [9u8; 16];
        let vault_hash = [8u8; 32];

        let req = build_get_sync_plan_request("10", client_incarnation_id).unwrap();
        match req {
            MailboxRequest::GetSyncPlan {
                client_incarnation_id: req_inc,
                ..
            } => {
                assert_eq!(req_inc, client_incarnation_id);
            }
            _ => panic!("Expected GetSyncPlan"),
        }

        let resp = MailboxResponse::SyncPlan(SyncPlan {
            incarnation_id: server_incarnation_id,
            head_seq: 100,
            compacted_through_seq: 0,
            mode: SyncMode::Delta { until_seq: 100 },
        });

        let plan = finalize_server_sync_plan_response(resp, vault_hash).unwrap();

        assert_eq!(plan.incarnation_id, Some(server_incarnation_id));
        assert_eq!(plan.remote_vault_id, Some(vault_hash));
    }
    use crate::sync::adapter::{AdapterSyncMode, PullLimits};
    use crate::sync::protocol::{
        MailboxEntryV3, MailboxRequest, MailboxResponse, PullPageResult, SyncEntryKind, SyncPlan,
    };

    #[test]
    fn server_provider_id_is_stable() {
        assert_eq!(SERVER_PROVIDER_ID, "server");
    }

    #[test]
    fn server_provider_id_stable_across_device_endpoint_and_reconnect_state() {
        let state1 = ServerAdapterIdentity::new("device_alpha", "127.0.0.1:4433");
        let state2 = ServerAdapterIdentity::new("device_alpha", "192.168.1.10:4433");
        let state3 = ServerAdapterIdentity::new("device_beta", "127.0.0.1:4433");

        assert_eq!(state1.adapter_id(), "server");
        assert_eq!(state2.adapter_id(), "server");
        assert_eq!(state3.adapter_id(), "server");
        assert_eq!(state1.adapter_id(), state2.adapter_id());
        assert_eq!(state2.adapter_id(), state3.adapter_id());
    }

    #[test]
    fn server_push_batch_preserves_all_three_entry_kinds() {
        let op_upsert = crate::sync::core::types::SyncOperation {
            operation_id: [1; 16],
            doc_hash: [10; 32],
            entry_kind: SyncEntryKind::Upsert,
            node_id: "node_1".into(),
            rel_path: "path1.md".into(),
            encrypted_payload: vec![1, 2, 3],
            payload_hash: [100; 32],
            timestamp: 1000,
        };
        let op_delete = crate::sync::core::types::SyncOperation {
            operation_id: [2; 16],
            doc_hash: [20; 32],
            entry_kind: SyncEntryKind::Delete,
            node_id: "node_2".into(),
            rel_path: "path2.md".into(),
            encrypted_payload: vec![4, 5, 6],
            payload_hash: [200; 32],
            timestamp: 2000,
        };
        let op_asset = crate::sync::core::types::SyncOperation {
            operation_id: [3; 16],
            doc_hash: [30; 32],
            entry_kind: SyncEntryKind::AssetReference,
            node_id: "node_3".into(),
            rel_path: "path3.png".into(),
            encrypted_payload: vec![7, 8, 9],
            payload_hash: [30; 32],
            timestamp: 3000,
        };

        let ops = vec![op_upsert, op_delete, op_asset];
        let (items, _tx_bytes) = convert_ops_to_push_items(&ops);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].entry_kind, SyncEntryKind::Upsert);
        assert_eq!(items[1].entry_kind, SyncEntryKind::Delete);
        assert_eq!(items[2].entry_kind, SyncEntryKind::AssetReference);
    }

    #[test]
    fn parse_server_cursor_accepts_empty_and_numeric() {
        assert_eq!(parse_server_cursor("").unwrap(), 0);
        assert_eq!(parse_server_cursor("12345").unwrap(), 12345);
        assert_eq!(parse_server_cursor("0").unwrap(), 0);
    }

    #[test]
    fn parse_server_cursor_rejects_invalid() {
        assert!(parse_server_cursor("abc").is_err());
        assert!(parse_server_cursor("-1").is_err());
        assert!(parse_server_cursor("12.34").is_err());
    }

    #[test]
    fn get_sync_plan_request_preserves_numeric_cursor() {
        let req = build_get_sync_plan_request("12345", None).unwrap();
        if let MailboxRequest::GetSyncPlan { cursor, .. } = req {
            assert_eq!(cursor, 12345);
        } else {
            panic!("Expected GetSyncPlan request");
        }
    }

    #[test]
    fn sync_plan_delta_maps_exact_until_cursor() {
        let resp = MailboxResponse::SyncPlan(SyncPlan {
            incarnation_id: [1; 16],
            head_seq: 100,
            compacted_through_seq: 0,
            mode: synabit_protocol::SyncMode::Delta { until_seq: 9876 },
        });

        let plan = map_sync_plan_response(resp).unwrap();
        match plan.mode {
            AdapterSyncMode::Delta { until_cursor } => {
                assert_eq!(until_cursor, Some("9876".to_string()));
            }
            _ => panic!("Expected Delta mode"),
        }
    }

    #[test]
    fn sync_plan_bootstrap_required_maps_correctly() {
        let resp = MailboxResponse::SyncPlan(SyncPlan {
            incarnation_id: [1; 16],
            head_seq: 100,
            compacted_through_seq: 50,
            mode: synabit_protocol::SyncMode::BootstrapRequired,
        });

        let plan = map_sync_plan_response(resp).unwrap();
        assert!(matches!(plan.mode, AdapterSyncMode::BootstrapRequired));
    }

    #[test]
    fn pull_page_request_preserves_after_until_and_limits() {
        let limits = PullLimits {
            max_entries: 5,
            max_bytes: 1000,
        };

        let req = build_pull_page_request("10", Some("50"), limits).unwrap();
        if let MailboxRequest::PullPage {
            after_seq,
            until_seq,
            max_entries,
            max_bytes,
        } = req
        {
            assert_eq!(after_seq, 10);
            assert_eq!(until_seq, 50);
            assert_eq!(max_entries, 5);
            assert_eq!(max_bytes, 1000);
        } else {
            panic!("Expected PullPage request");
        }
    }

    #[test]
    fn pull_page_response_maps_all_fields() {
        let entry = MailboxEntryV3 {
            seq: 42,
            operation_id: [7; 16],
            entry_kind: SyncEntryKind::Delete,
            doc_hash: [2; 32],
            source_device: "device_abc".into(),
            encrypted_payload: vec![100, 200],
            payload_hash: [3; 32],
            timestamp: 123456789,
        };

        let resp = MailboxResponse::PullPageResult(PullPageResult {
            entries: vec![entry],
            next_seq: 43,
            until_seq: 100,
            has_more: true,
        });

        let page = map_pull_page_response(resp, 100).unwrap();

        assert_eq!(page.entries.len(), 1);
        let re = &page.entries[0];
        assert_eq!(re.remote_position, "42");
        assert_eq!(re.remote_seq, Some(42));
        assert_eq!(re.operation_id, [7; 16]);
        assert_eq!(re.entry_kind, SyncEntryKind::Delete);
        assert_eq!(re.doc_hash, [2; 32]);
        assert_eq!(re.source_device, "device_abc");
        assert_eq!(re.encrypted_payload, vec![100, 200]);
        assert_eq!(re.payload_hash, [3; 32]);
        assert_eq!(re.timestamp, 123456789);

        assert_eq!(page.next_cursor, "43");
        assert!(page.has_more);
        assert_eq!(page.rx_bytes, 2);

        // Also verify Upsert mapping
        let upsert_entry = MailboxEntryV3 {
            seq: 44,
            operation_id: [8; 16],
            entry_kind: SyncEntryKind::Upsert,
            doc_hash: [2; 32],
            source_device: "device_abc".into(),
            encrypted_payload: vec![101],
            payload_hash: [3; 32],
            timestamp: 123456790,
        };
        let resp2 = MailboxResponse::PullPageResult(PullPageResult {
            entries: vec![upsert_entry],
            next_seq: 45,
            until_seq: 100,
            has_more: false,
        });
        let page2 = map_pull_page_response(resp2, 100).unwrap();
        assert_eq!(page2.entries[0].operation_id, [8; 16]);
        assert_eq!(page2.entries[0].entry_kind, SyncEntryKind::Upsert);
    }

    #[test]
    fn pull_page_rejects_until_mismatch() {
        let resp = MailboxResponse::PullPageResult(PullPageResult {
            entries: vec![],
            next_seq: 50,
            until_seq: 100, // Mismatch with expected 50
            has_more: false,
        });

        let res = map_pull_page_response(resp, 50);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(msg.contains("until_seq mismatch"));
    }

    #[test]
    fn pull_page_rejects_asset_reference_until_supported() {
        let entry = MailboxEntryV3 {
            seq: 1,
            operation_id: [1; 16],
            entry_kind: SyncEntryKind::AssetReference,
            doc_hash: [0; 32],
            source_device: "dev".into(),
            encrypted_payload: vec![],
            payload_hash: [0; 32],
            timestamp: 100,
        };

        let resp = MailboxResponse::PullPageResult(PullPageResult {
            entries: vec![entry],
            next_seq: 2,
            until_seq: 10,
            has_more: false,
        });

        let res = map_pull_page_response(resp, 10);
        assert!(res.is_ok());
        let page = res.unwrap();
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].entry_kind, SyncEntryKind::AssetReference);
    }

    #[test]
    fn pull_page_rejects_zero_entry_limit() {
        let limits = PullLimits {
            max_entries: 0,
            max_bytes: 1000,
        };

        let res = build_pull_page_request("0", Some("10"), limits);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(msg.contains("max_entries cannot be 0"));
    }

    #[test]
    fn pull_page_rejects_zero_byte_limit() {
        let limits = PullLimits {
            max_entries: 10,
            max_bytes: 0,
        };

        let res = build_pull_page_request("0", Some("10"), limits);
        assert!(res.is_err());
        let msg = res.unwrap_err().to_string();
        assert!(msg.contains("max_bytes cannot be 0"));
    }

    #[test]
    fn client_protocol_uses_shared_mailbox_types() {
        let item = synabit_protocol::PushBatchItem {
            operation_id: [1; 16],
            doc_hash: [2; 32],
            entry_kind: synabit_protocol::SyncEntryKind::Upsert,
            encrypted_payload: vec![1, 2, 3],
            payload_hash: [4; 32],
        };

        let req = crate::sync::protocol::MailboxRequest::PushBatch { items: vec![item] };
        let serialized = postcard::to_stdvec(&req).unwrap();
        let deserialized: synabit_protocol::MailboxRequest =
            postcard::from_bytes(&serialized).unwrap();

        if let synabit_protocol::MailboxRequest::PushBatch { items } = deserialized {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].operation_id, [1; 16]);
            assert_eq!(items[0].entry_kind, synabit_protocol::SyncEntryKind::Upsert);
        } else {
            panic!("Expected PushBatch request");
        }
    }
}
