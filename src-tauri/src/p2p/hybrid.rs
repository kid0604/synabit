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
use crate::sync::{SyncResult, SyncTransport};

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
    server_transport: &dyn SyncTransport,
    p2p_transports: &[&dyn SyncTransport],
    vault_path: &str,
    device_id: &str,
) -> AppResult<SyncResult> {
    // ── 1. Server sync (primary, always) ─────────────────────

    info!(
        "Hybrid sync: starting server sync via {}",
        server_transport.provider_name()
    );

    let mut result = engine::p2p_sync_full(
        app_handle,
        server_transport,
        vault_path,
        device_id,
    )
    .await?;

    info!(
        "Hybrid sync: server done (pulled={}, pushed={}, deleted={})",
        result.pulled, result.pushed, result.deleted
    );

    // ── 2. P2P sync with each online peer (best-effort) ──────

    if p2p_transports.is_empty() {
        info!("Hybrid sync: no P2P peers online, skipping");
        return Ok(result);
    }

    info!(
        "Hybrid sync: attempting {} P2P peer(s)",
        p2p_transports.len()
    );

    for (i, peer_transport) in p2p_transports.iter().enumerate() {
        let peer_name = peer_transport.provider_name();
        match engine::p2p_sync_full(
            app_handle,
            *peer_transport,
            vault_path,
            device_id,
        )
        .await
        {
            Ok(peer_result) => {
                result.pulled += peer_result.pulled;
                result.pushed += peer_result.pushed;
                result.deleted += peer_result.deleted;
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
                warn!(
                    "Hybrid sync: P2P peer {}/{} ({}) failed (non-fatal): {}",
                    i + 1,
                    p2p_transports.len(),
                    peer_name,
                    e
                );
                result.errors.push(format!(
                    "P2P sync with {} failed: {}",
                    peer_name, e
                ));
            }
        }
    }

    info!(
        "Hybrid sync complete: pulled={}, pushed={}, deleted={}, errors={}",
        result.pulled,
        result.pushed,
        result.deleted,
        result.errors.len()
    );

    Ok(result)
}
