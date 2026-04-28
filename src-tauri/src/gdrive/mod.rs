// Google Drive integration — modular layout
//
// ┌─────────────┐
// │  mod.rs     │ ← re-exports + shared types/constants
// │  auth.rs    │ ← Vault Sync OAuth2 (tokens in JSON file, scope: drive.file)
// │  api.rs     │ ← Drive API helpers (list, upload, download, delete, folders)
// │  sync.rs    │ ← Full sync engine (3-way merge logic)
// │  browse.rs  │ ← File Manager GDrive browse (tokens in Keychain, scope: drive.readonly)
// └─────────────┘
//
// auth.rs + api.rs + sync.rs = Vault Sync (backup vault to Drive)
// browse.rs = OmniDrive File Manager (browse ALL user Drive files)
// Each has its own OAuth token. Users can enable either, both, or neither.

pub mod auth;
pub mod api;
pub mod sync;
pub mod browse;

// ──────────────────────────────────────────────
// Shared Constants
// ──────────────────────────────────────────────
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) const CLIENT_ID: &str = env!("SYNABIT_GOOGLE_CLIENT_ID", "Set SYNABIT_GOOGLE_CLIENT_ID env var at build time");

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) const CLIENT_ID: &str = match option_env!("SYNABIT_ANDROID_CLIENT_ID") {
    Some(val) => val,
    None => env!("SYNABIT_GOOGLE_CLIENT_ID", "Set SYNABIT_GOOGLE_CLIENT_ID env var at build time"),
};
pub(crate) const CLIENT_SECRET: &str = env!("SYNABIT_GOOGLE_CLIENT_SECRET", "Set SYNABIT_GOOGLE_CLIENT_SECRET env var at build time");
pub(crate) const AUTH_URI: &str = "https://accounts.google.com/o/oauth2/auth";
pub(crate) const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
pub(crate) const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
pub(crate) const REDIRECT_PORT_START: u16 = 49152;
pub(crate) const REDIRECT_PORT_END: u16 = 49200;
pub(crate) const VAULT_FOLDER_NAME: &str = "Synabit Vault";

// ──────────────────────────────────────────────
// Shared Data Structures
// ──────────────────────────────────────────────
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GDriveTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SyncManifest {
    pub files: HashMap<String, SyncFileEntry>,
    pub vault_folder_id: String,
    pub folder_ids: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncFileEntry {
    pub drive_file_id: String,
    pub local_sha256: String,
    pub drive_modified_time: String,
    pub local_modified_time: String,
}

#[derive(Deserialize, Debug)]
pub(crate) struct DriveFile {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    #[serde(rename = "modifiedTime")]
    pub modified_time: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct DriveFileList {
    pub files: Option<Vec<DriveFile>>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
}

// ──────────────────────────────────────────────
// Shared Path Helpers
// ──────────────────────────────────────────────

pub(crate) fn config_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app_handle.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub(crate) fn tokens_path(app_handle: &tauri::AppHandle) -> PathBuf {
    config_dir(app_handle).join("gdrive_tokens.json")
}

pub(crate) fn manifest_path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(".synabit_sync_manifest.json")
}

pub fn gdrive_cache_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    config_dir(app_handle).join("gdrive-cache")
}

pub(crate) fn file_sha256(path: &Path) -> String {
    if let Ok(bytes) = fs::read(path) {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        format!("{:x}", hasher.finalize())
    } else {
        String::new()
    }
}

pub(crate) fn load_manifest(vault_path: &str) -> SyncManifest {
    let path = manifest_path(vault_path);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(manifest) = serde_json::from_str(&content) {
                return manifest;
            }
        }
    }
    SyncManifest::default()
}

pub(crate) fn save_manifest(vault_path: &str, manifest: &SyncManifest) -> Result<(), String> {
    let content = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(manifest_path(vault_path), content).map_err(|e| e.to_string())
}
