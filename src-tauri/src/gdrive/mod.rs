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

pub mod api;
pub mod auth;
pub mod browse;
pub mod sync;

// ──────────────────────────────────────────────
// Shared Constants
// ──────────────────────────────────────────────
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) const CLIENT_ID: &str = env!(
    "SYNABIT_GOOGLE_CLIENT_ID",
    "Set SYNABIT_GOOGLE_CLIENT_ID env var at build time"
);

#[cfg(any(target_os = "android", target_os = "ios"))]
pub(crate) const CLIENT_ID: &str = match option_env!("SYNABIT_ANDROID_CLIENT_ID") {
    Some(val) => val,
    None => env!(
        "SYNABIT_GOOGLE_CLIENT_ID",
        "Set SYNABIT_GOOGLE_CLIENT_ID env var at build time"
    ),
};
// Desktop OAuth clients: Google still requires client_secret for token exchange/refresh.
// It's considered "not truly secret" for desktop apps, but mandatory for the endpoint.
// PKCE is added as an additional security layer on top.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) const CLIENT_SECRET: &str = env!(
    "SYNABIT_GOOGLE_CLIENT_SECRET",
    "Set SYNABIT_GOOGLE_CLIENT_SECRET env var at build time"
);
pub(crate) const AUTH_URI: &str = "https://accounts.google.com/o/oauth2/auth";
pub(crate) const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
pub(crate) const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
pub(crate) const REDIRECT_PORT_START: u16 = 49152;
pub(crate) const REDIRECT_PORT_END: u16 = 49200;
pub(crate) const VAULT_FOLDER_NAME: &str = "Synabit Vault";

// ──────────────────────────────────────────────
// Shared Data Structures
// ──────────────────────────────────────────────
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use md5::{Digest as Md5Digest, Md5};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ──────────────────────────────────────────────
// PKCE Helpers (RFC 7636)
// ──────────────────────────────────────────────

/// Generates a PKCE code_verifier (random 43-128 char string) and
/// its corresponding S256 code_challenge.
pub(crate) fn generate_pkce_pair() -> (String, String) {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.random::<u8>()).collect();
    let code_verifier = URL_SAFE_NO_PAD.encode(&bytes);
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
    (code_verifier, code_challenge)
}

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
    #[serde(default)]
    pub local_md5: String,
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
    #[serde(rename = "md5Checksum")]
    pub md5_checksum: Option<String>,
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
    app_handle
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
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

pub(crate) fn file_md5(path: &Path) -> String {
    if let Ok(bytes) = fs::read(path) {
        let mut hasher = Md5::new();
        hasher.update(&bytes);
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect::<String>()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_pair_format() {
        let (verifier, challenge) = generate_pkce_pair();
        // RFC 7636: code_verifier should be 43-128 chars (base64url of 32 bytes = 43 chars)
        assert!(
            verifier.len() >= 43,
            "verifier too short: {}",
            verifier.len()
        );
        assert!(
            verifier.len() <= 128,
            "verifier too long: {}",
            verifier.len()
        );
        // challenge should be base64url(SHA256(verifier)) = 43 chars
        assert_eq!(challenge.len(), 43, "challenge should be 43 chars");
    }

    #[test]
    fn test_pkce_pair_uniqueness() {
        let (v1, _) = generate_pkce_pair();
        let (v2, _) = generate_pkce_pair();
        assert_ne!(v1, v2, "Two PKCE pairs should be unique");
    }

    #[test]
    fn test_pkce_challenge_matches_verifier() {
        let (verifier, challenge) = generate_pkce_pair();
        // Manually compute SHA256(verifier) and compare
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(challenge, expected, "challenge must be SHA256(verifier)");
    }
}
