//! GDriveTransport — adapter exposing the file-based Google Drive sync
//! behind the `SyncTransport` trait interface.
//!
//! ## Why Option C?
//!
//! Google Drive sync is fundamentally **file-based**: it lists Drive files,
//! compares SHA-256 hashes + modification times, and uploads/downloads
//! encrypted blobs via the Google Drive REST API. It tracks state through
//! a local JSON manifest with `drive_file_id`, `local_sha256`, mtime, etc.
//!
//! The `SyncTransport` trait is **sequence-based**: the server assigns
//! monotonic `seq` numbers, and clients use a cursor to pull new entries.
//!
//! These models are too different to create a clean adapter without
//! significant risk of breaking the existing sync. Instead, `GDriveTransport`
//! is a **facade** that wraps the battle-tested `gdrive_sync_full()` behind
//! the trait interface. The individual trait methods (`push_doc`, `pull_since`)
//! return `Unsupported` — all sync work happens through the `sync_full()`
//! convenience method which calls the existing monolith.
//!
//! This enables:
//! - Unified `SyncTransport` type-checking across all backends
//! - Future incremental migration to sequence-based GDrive sync
//! - Shared utilities (`file_sha256`, `collect_local_files`) via `sync::utils`

use async_trait::async_trait;
use log::info;

use crate::error::{AppError, AppResult};
use crate::sync::{RemoteSyncEntry, SyncTransport};

/// Google Drive sync transport.
///
/// This is a **facade** adapter: individual trait methods are not supported
/// because GDrive sync requires the full 3-way manifest-based flow.
/// Use `sync_full()` to run a complete GDrive sync cycle.
pub struct GDriveTransport {
    /// Tauri AppHandle for accessing DB state, secrets, etc.
    app_handle: tauri::AppHandle,
}

impl GDriveTransport {
    /// Create a new GDrive transport.
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self { app_handle }
    }

    /// Run a full GDrive sync cycle (pull → push → delete).
    ///
    /// This delegates to the existing `gdrive_sync_full()` which handles
    /// the complete file-based sync flow including manifest tracking,
    /// concurrent uploads/downloads, CRDT merging, and conflict resolution.
    pub async fn sync_full(&self, vault_path: &str) -> Result<super::sync::SyncResult, String> {
        info!("GDriveTransport: starting full sync for {}", vault_path);
        super::sync::gdrive_sync_full(self.app_handle.clone(), vault_path.to_string()).await
    }
}

impl std::fmt::Debug for GDriveTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GDriveTransport")
            .field("provider", &"Google Drive")
            .finish()
    }
}

// ---------------------------------------------------------------------------
// SyncTransport trait implementation (facade — individual ops not supported)
// ---------------------------------------------------------------------------

#[async_trait]
impl SyncTransport for GDriveTransport {
    fn provider_name(&self) -> &str {
        "Google Drive"
    }

    async fn authenticate(&self) -> AppResult<()> {
        // Authentication is handled internally by gdrive_sync_full via
        // get_valid_token(). Calling this is a no-op — the real auth
        // happens at sync time.
        let _token = super::auth::get_valid_token(&self.app_handle)
            .await
            .map_err(AppError::AuthFailed)?;
        Ok(())
    }

    async fn disconnect(&self) -> AppResult<()> {
        // GDrive is stateless REST, no connection to close
        Ok(())
    }

    async fn push_doc(
        &self,
        _doc_hash: &[u8; 32],
        _encrypted_payload: Vec<u8>,
    ) -> AppResult<u64> {
        // GDrive uses file-based sync, not sequence-based.
        // Use sync_full() instead.
        Err(AppError::SyncError(
            "GDriveTransport: push_doc not supported — use sync_full()".to_string(),
        ))
    }

    async fn pull_since(&self, _since_seq: u64) -> AppResult<Vec<RemoteSyncEntry>> {
        // GDrive uses file-based sync, not sequence-based.
        // Use sync_full() instead.
        Err(AppError::SyncError(
            "GDriveTransport: pull_since not supported — use sync_full()".to_string(),
        ))
    }

    async fn ack(&self, _up_to_seq: u64) -> AppResult<()> {
        // No-op for GDrive: manifest persistence handles acknowledgement.
        Ok(())
    }

    async fn push_asset(
        &self,
        _asset_hash: &[u8; 32],
        _encrypted_data: Vec<u8>,
    ) -> AppResult<()> {
        // Asset sync is handled within sync_full().
        Err(AppError::SyncError(
            "GDriveTransport: push_asset not supported — use sync_full()".to_string(),
        ))
    }

    async fn pull_asset(&self, _asset_hash: &[u8; 32]) -> AppResult<Option<Vec<u8>>> {
        // Asset sync is handled within sync_full().
        Err(AppError::SyncError(
            "GDriveTransport: pull_asset not supported — use sync_full()".to_string(),
        ))
    }

    async fn push_delete(&self, _doc_hash: &[u8; 32]) -> AppResult<u64> {
        // Delete sync is handled within sync_full().
        Err(AppError::SyncError(
            "GDriveTransport: push_delete not supported — use sync_full()".to_string(),
        ))
    }

    async fn ping(&self) -> AppResult<()> { Ok(()) }

    async fn is_available(&self) -> bool {
        // GDrive is available if we have a valid OAuth token.
        super::auth::get_valid_token(&self.app_handle)
            .await
            .is_ok()
    }
}
