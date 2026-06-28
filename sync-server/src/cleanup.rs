//! Garbage collection / retention policy.
//!
//! Runs as a background task that periodically:
//! 1. For each vault, deletes entries that all devices have ACKed.
//! 2. Deletes entries older than `max_entry_age_secs` regardless of ACK state
//!    (prevents unbounded growth if a device disappears).
//! 3. Removes the corresponding blob files from disk.

use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::mailbox::MailboxHandler;

/// Spawn the background cleanup task. Returns a `JoinHandle` that can be
/// used to monitor the task (it runs until the cancellation token fires).
pub fn spawn_cleanup_task(
    handler: Arc<MailboxHandler>,
    cancel: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    let interval_secs = handler.config().cleanup_interval_secs;
    let max_age = handler.config().max_entry_age_secs;

    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(interval_secs));
        // The first tick fires immediately — skip it so we don't GC on startup.
        interval.tick().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("cleanup task shutting down");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = run_cleanup(handler.db(), max_age) {
                        error!(error = %e, "cleanup cycle failed");
                    }
                }
            }
        }
    })
}

/// Execute one cleanup cycle.
fn run_cleanup(db: &crate::db::Database, max_age_secs: u64) -> anyhow::Result<()> {
    debug!("starting cleanup cycle");

    // Phase 1: Per-vault ACK-based GC.
    let vaults = db.list_vault_hashes()?;
    let mut total_ack_deleted = 0usize;

    for vault_hash in &vaults {
        let min_seq = db.min_cursor(vault_hash)?;
        if min_seq > 0 {
            let paths = db.gc_acked_entries(vault_hash, min_seq)?;
            for path in &paths {
                remove_blob(path);
            }
            total_ack_deleted += paths.len();
        }
    }

    // Phase 2: Age-based GC (hard TTL) for mailbox entries.
    let old_paths = db.gc_old_entries(max_age_secs)?;
    for path in &old_paths {
        remove_blob(path);
    }

    // Phase 3: Age-based GC for assets.
    let old_asset_paths = db.gc_old_assets(max_age_secs)?;
    for path in &old_asset_paths {
        remove_blob(path);
    }

    if total_ack_deleted > 0 || !old_paths.is_empty() || !old_asset_paths.is_empty() {
        info!(
            ack_deleted = total_ack_deleted,
            age_deleted = old_paths.len(),
            assets_deleted = old_asset_paths.len(),
            "cleanup cycle complete"
        );
    } else {
        debug!("cleanup cycle: nothing to delete");
    }

    Ok(())
}

/// Best-effort deletion of a blob file. Tombstones (marker paths) and
/// already-deleted files are silently ignored.
fn remove_blob(path: &str) {
    if path == "(tombstone)" {
        return;
    }
    match std::fs::remove_file(path) {
        Ok(()) => debug!(path = path, "blob removed"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Already gone — fine.
        }
        Err(e) => {
            warn!(path = path, error = %e, "failed to remove blob file");
        }
    }
}
