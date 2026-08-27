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

/// Android needs an OAuth client of its own, and there is no falling back to
/// the desktop one.
///
/// Both mobile flows — vault sync in `auth.rs` and the OmniDrive browser in
/// `browse.rs` — redirect to `com.synabit.app:/oauth2callback`, the package
/// name as a custom scheme. Google accepts a custom-scheme redirect only from a
/// client registered as type **Android**. Ask with a Web or Desktop client and
/// the authorization request comes back `invalid_request`: the user is left
/// looking at a browser tab that never returns to the app, and nothing in the
/// log says why.
///
/// This used to fall back to `SYNABIT_GOOGLE_CLIENT_ID` when unset. That builds
/// and ships perfectly happily, then fails on every phone. Build time is the
/// only place the failure is cheap — no test in this repo can reach it, because
/// nothing is actually wrong until Google is asked.
///
/// Registering the client needs the package name and **two** SHA-1
/// fingerprints: the upload key, and the separate key Play App Signing re-signs
/// with. Only the first is on a developer machine, so a locally signed build can
/// work while the one Play distributes does not.
#[cfg(target_os = "android")]
pub(crate) const CLIENT_ID: &str = env!(
    "SYNABIT_ANDROID_CLIENT_ID",
    "Android needs its own Google OAuth client (type: Android). Set \
     SYNABIT_ANDROID_CLIENT_ID at build time — the desktop client ID will not \
     work here, because Google rejects the custom-scheme redirect this flow uses."
);

/// iOS has no generated project yet — `src-tauri/gen/` holds `android` alone —
/// so this arm exists to keep the crate compiling for the target, not because
/// anything ships from it. Whoever runs `tauri ios init` has to give iOS a
/// client of its own for the same reason Android needs one: the redirect is a
/// custom scheme, and a Desktop client will not answer it.
#[cfg(target_os = "ios")]
pub(crate) const CLIENT_ID: &str = env!(
    "SYNABIT_GOOGLE_CLIENT_ID",
    "Set SYNABIT_GOOGLE_CLIENT_ID env var at build time"
);

// `env!` is satisfied by an empty string, and an unconfigured GitHub Actions
// secret expands to exactly that — so "the variable is set" is not the same
// claim as "there is a client ID here", and the requirement above would pass
// while the value stayed useless. Whichever platform this compiled for, an
// empty client ID can only produce an authorization request Google refuses.
const _: () = assert!(
    !CLIENT_ID.is_empty(),
    "the Google OAuth client ID is empty — the variable is set but carries no \
     value. Check the secret exists in whatever environment ran this build."
);

// Desktop OAuth clients: Google still requires client_secret for token exchange/refresh.
// It's considered "not truly secret" for desktop apps, but mandatory for the endpoint.
// PKCE is added as an additional security layer on top.
//
// Desktop-only, and cfg-gated rather than merely unused on mobile. An Android
// client is a public client: it sends no secret, and every reference to this
// constant is already behind the same cfg — the compiler confirms it, having
// reported this as dead code on that target. Left ungated it still had to
// *exist* at build time, so an Android-only CI job was obliged to hold the
// desktop client secret in order to compile code that would never read it.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) const CLIENT_SECRET: &str = env!(
    "SYNABIT_GOOGLE_CLIENT_SECRET",
    "Set SYNABIT_GOOGLE_CLIENT_SECRET env var at build time"
);
pub(crate) const AUTH_URI: &str = "https://accounts.google.com/o/oauth2/auth";
pub(crate) const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
pub(crate) const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
// The ephemeral range the desktop flow scans for a free port to catch its
// loopback redirect on. Mobile is redirected through a custom scheme and never
// binds a socket, so these do not exist there.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) const REDIRECT_PORT_START: u16 = 49152;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) const REDIRECT_PORT_END: u16 = 49200;
pub(crate) const VAULT_FOLDER_NAME: &str = "Synabit Vault";

// ──────────────────────────────────────────────
// Shared Data Structures
// ──────────────────────────────────────────────
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

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

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct DriveFile {
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct DriveFileList {
    pub files: Option<Vec<DriveFile>>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct DriveChange {
    #[serde(rename = "fileId")]
    pub file_id: Option<String>,
    pub file: Option<DriveFile>,
    pub removed: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
pub(crate) struct DriveChangeList {
    pub changes: Option<Vec<DriveChange>>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(rename = "newStartPageToken")]
    pub new_start_page_token: Option<String>,
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

pub fn gdrive_cache_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    config_dir(app_handle).join("gdrive-cache")
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
