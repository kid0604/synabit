//! Hybrid sync orchestrator — sync via server + P2P peers.
//!
//! Orchestrates a full sync cycle that:
//! 1. Always syncs with the central Synabit Sync Server (primary source of truth)
//! 2. Then attempts P2P sync with each online peer (best-effort, non-fatal)
//!
//! This gives us both reliability (server is always available) and speed
//! (P2P sync can deliver changes faster on LAN without round-tripping
//! through the server).

use log::{info, warn};

use crate::error::AppResult;
use crate::sync::engine;
use crate::sync::{SyncResult, SyncRunContext, SyncTransport};

/// Orchestrate sync across the server + online P2P peers.
///
/// # Arguments
///
/// * `app_handle` — Tauri app handle (for DB state + secrets)
/// * `server_transport` — transport to the Synabit Sync Server
/// * `p2p_transports` — transports to online paired devices
/// * `vault_path` — absolute path to the local vault
/// * `device_id` — this device's stable UUID
///
/// # Error handling
///
/// - Server sync failures are fatal (propagated to caller).
/// - P2P sync failures are non-fatal (logged, skipped).
pub async fn hybrid_sync(
    app_handle: &tauri::AppHandle,
    server_transport: Option<&dyn SyncTransport>,
    p2p_transports: &[&dyn SyncTransport],
    vault_path: &str,
    device_id: &str,
    run_context: &SyncRunContext,
) -> AppResult<SyncResult> {
    let mut result = SyncResult::empty();
    let mut any_success = false;
    let mut all_errors = Vec::new();

    info!(
        "sync_run run_id={} trigger={} vault_tag={} scope=hybrid state=start direct_peers={} server_present={}",
        run_context.run_id,
        run_context.trigger_reason,
        run_context.vault_tag,
        p2p_transports.len(),
        server_transport.is_some(),
    );

    // ── 1. P2P sync with each online peer (Fast Path) ────────

    if p2p_transports.is_empty() {
        info!("Hybrid sync: no P2P peers online, skipping P2P phase");
    } else {
        info!(
            "Hybrid sync: attempting {} P2P peer(s)",
            p2p_transports.len()
        );

        for (i, peer_transport) in p2p_transports.iter().enumerate() {
            let peer_name = peer_transport.provider_name();
            info!("Hybrid sync: P2P sync starting via {}", peer_name);
            match engine::p2p_sync_full(
                app_handle,
                *peer_transport,
                vault_path,
                device_id,
                run_context,
            )
            .await
            {
                Ok(peer_result) => {
                    any_success = true;
                    result.pulled += peer_result.pulled;
                    result.pushed += peer_result.pushed;
                    result.deleted += peer_result.deleted;
                    result.tx_bytes += peer_result.tx_bytes;
                    result.rx_bytes += peer_result.rx_bytes;
                    // Merge errors from peer sync
                    result.errors.extend(peer_result.errors);
                    // Merge pulled files list
                    result.pulled_files.extend(peer_result.pulled_files);

                    info!(
                        "Hybrid sync: P2P peer {}/{} ({}) done: +{} pulled, +{} pushed",
                        i + 1,
                        p2p_transports.len(),
                        peer_name,
                        peer_result.pulled,
                        peer_result.pushed,
                    );
                }
                Err(e) => {
                    let err_msg = format!("P2P peer {} failed: {}", peer_name, e);
                    warn!("Hybrid sync: {}", err_msg);
                    all_errors.push(err_msg);
                }
            }
        }
    }

    // ── 2. Server sync (Backup Path) ─────────────────────────

    if let Some(transport) = server_transport {
        info!(
            "Hybrid sync: starting server sync via {}",
            transport.provider_name()
        );

        match engine::p2p_sync_full(
            app_handle,
            transport,
            vault_path,
            device_id,
            run_context,
        )
        .await
        {
            Ok(server_result) => {
                any_success = true;
                result.pulled += server_result.pulled;
                result.pushed += server_result.pushed;
                result.deleted += server_result.deleted;
                result.tx_bytes += server_result.tx_bytes;
                result.rx_bytes += server_result.rx_bytes;
                result.errors.extend(server_result.errors);
                result.pulled_files.extend(server_result.pulled_files);

                info!(
                    "Hybrid sync: server done (+{} pulled, +{} pushed)",
                    server_result.pulled, server_result.pushed
                );
            }
            Err(e) => {
                let err_msg = format!("Server sync failed: {}", e);
                warn!("Hybrid sync: {}", err_msg);
                all_errors.push(err_msg);
            }
        }
    } else {
        info!("Hybrid sync: server transport is None, skipping server phase");
    }

    let tried_any = server_transport.is_some() || !p2p_transports.is_empty();
    
    if tried_any && !any_success {
        return Err(crate::error::AppError::SyncError(format!(
            "All transports failed: {}",
            all_errors.join(", ")
        )));
    }

    if !all_errors.is_empty() {
        result.errors.extend(all_errors);
    }

    info!(
        "sync_run run_id={} trigger={} vault_tag={} scope=hybrid state=complete pulled={} pushed={} deleted={} errors={} tx_bytes={} rx_bytes={}",
        run_context.run_id,
        run_context.trigger_reason,
        run_context.vault_tag,
        result.pulled,
        result.pushed,
        result.deleted,
        result.errors.len(),
        result.tx_bytes,
        result.rx_bytes,
    );

    Ok(result)
}
