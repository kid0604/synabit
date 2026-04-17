use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::future::Future;

// ──────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────
const CLIENT_ID: &str = "REDACTED_GOOGLE_CLIENT_ID";
const CLIENT_SECRET: &str = "REDACTED_GOOGLE_CLIENT_SECRET";
const AUTH_URI: &str = "https://accounts.google.com/o/oauth2/auth";
const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const REDIRECT_PORT_START: u16 = 49152;
const REDIRECT_PORT_END: u16 = 49200;
const VAULT_FOLDER_NAME: &str = "Synabit Vault";

// ──────────────────────────────────────────────
// Data Structures
// ──────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GDriveTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SyncManifest {
    /// Map from relative path (e.g. "Notes/my-note.md") to file metadata
    pub files: HashMap<String, SyncFileEntry>,
    /// The Google Drive folder ID for the vault root
    pub vault_folder_id: String,
    /// Folder ID cache: relative dir path -> Drive folder ID
    pub folder_ids: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncFileEntry {
    pub drive_file_id: String,
    pub local_sha256: String,
    pub drive_modified_time: String,
    pub local_modified_time: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncResult {
    pub pulled: u32,
    pub pushed: u32,
    pub deleted: u32,
    pub errors: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct DriveFile {
    id: Option<String>,
    name: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
}

#[derive(Deserialize, Debug)]
struct DriveFileList {
    files: Option<Vec<DriveFile>>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

// ──────────────────────────────────────────────
// Path Helpers
// ──────────────────────────────────────────────

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".synabit")
}

fn tokens_path() -> PathBuf {
    config_dir().join("gdrive_tokens.json")
}

fn manifest_path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(".synabit_sync_manifest.json")
}

pub fn gdrive_cache_dir() -> PathBuf {
    config_dir().join("gdrive-cache")
}

fn file_sha256(path: &Path) -> String {
    if let Ok(bytes) = fs::read(path) {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        format!("{:x}", hasher.finalize())
    } else {
        String::new()
    }
}

// ──────────────────────────────────────────────
// Token Management
// ──────────────────────────────────────────────

fn load_tokens() -> Option<GDriveTokens> {
    let path = tokens_path();
    if path.exists() {
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

fn save_tokens(tokens: &GDriveTokens) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let content = serde_json::to_string_pretty(tokens).map_err(|e| e.to_string())?;
    fs::write(tokens_path(), content).map_err(|e| e.to_string())
}

fn load_manifest(vault_path: &str) -> SyncManifest {
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

fn save_manifest(vault_path: &str, manifest: &SyncManifest) -> Result<(), String> {
    let content = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(manifest_path(vault_path), content).map_err(|e| e.to_string())
}

async fn get_valid_token() -> Result<String, String> {
    let mut tokens = load_tokens().ok_or("Not authenticated with Google Drive")?;
    let now = chrono::Utc::now().timestamp();

    if now >= tokens.expires_at - 60 {
        // Token expired or about to expire, refresh it
        let client = reqwest::Client::new();
        let resp = client
            .post(TOKEN_URI)
            .form(&[
                ("client_id", CLIENT_ID),
                ("client_secret", CLIENT_SECRET),
                ("refresh_token", tokens.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(|e| format!("Token refresh request failed: {}", e))?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            // If refresh fails with invalid_grant, tokens are revoked
            if err_text.contains("invalid_grant") {
                let _ = fs::remove_file(tokens_path());
                return Err("Google Drive session expired. Please reconnect.".to_string());
            }
            return Err(format!("Token refresh failed: {}", err_text));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;

        tokens.access_token = token_resp.access_token;
        tokens.expires_at = now + token_resp.expires_in.unwrap_or(3600);
        if let Some(new_refresh) = token_resp.refresh_token {
            tokens.refresh_token = new_refresh;
        }
        save_tokens(&tokens)?;
    }

    Ok(tokens.access_token)
}

// ──────────────────────────────────────────────
// OAuth2 Flow
// ──────────────────────────────────────────────

#[tauri::command]
pub fn gdrive_auth_status() -> Result<bool, String> {
    Ok(load_tokens().is_some())
}

#[tauri::command]
pub fn gdrive_disconnect() -> Result<(), String> {
    let path = tokens_path();
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn gdrive_auth_start() -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Find an available port
    let mut port = 0u16;
    let mut listener_opt = None;
    for p in REDIRECT_PORT_START..=REDIRECT_PORT_END {
        match TcpListener::bind(format!("127.0.0.1:{}", p)).await {
            Ok(l) => {
                port = p;
                listener_opt = Some(l);
                break;
            }
            Err(_) => continue,
        }
    }

    let listener = listener_opt.ok_or("Could not find available port for OAuth callback")?;
    let redirect_uri = format!("http://127.0.0.1:{}", port);

    // Build authorization URL
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        AUTH_URI,
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(SCOPE),
    );

    // Open browser
    let _ = opener::open(&auth_url);

    // Wait for callback (with 120s timeout)
    let auth_code = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("Accept failed: {}", e))?;

        let mut buf = vec![0u8; 4096];
        let n = stream
            .read(&mut buf)
            .await
            .map_err(|e| format!("Read failed: {}", e))?;
        let request = String::from_utf8_lossy(&buf[..n]).to_string();

        // Extract code from "GET /?code=xxx&scope=... HTTP/1.1"
        let code = request
            .lines()
            .next()
            .and_then(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let path = parts[1];
                    // Parse query string manually to avoid url crate dependency
                    path.split('?').nth(1).and_then(|qs| {
                        qs.split('&')
                            .find_map(|pair| {
                                let mut kv = pair.splitn(2, '=');
                                let key = kv.next()?;
                                let val = kv.next()?;
                                if key == "code" { Some(val.to_string()) } else { None }
                            })
                    })
                } else {
                    None
                }
            })
            .ok_or_else(|| "No auth code in callback".to_string())?;

        // Send success response to browser
        let html = r#"<html><body style="font-family:system-ui;display:flex;justify-content:center;align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff"><div style="text-align:center"><h1>✅ Connected!</h1><p>You can close this tab and return to Synabit.</p></div></body></html>"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(),
            html
        );
        let _ = stream.write_all(response.as_bytes()).await;

        Ok::<String, String>(code)
    })
    .await
    .map_err(|_| "Authentication timed out (120s). Please try again.".to_string())??;

    // Exchange code for tokens
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URI)
        .form(&[
            ("code", auth_code.as_str()),
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| format!("Token exchange request failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", err));
    }

    let token_resp: TokenResponse = resp
        .json()
        .await
        .map_err(|e| format!("Token parse failed: {}", e))?;

    let now = chrono::Utc::now().timestamp();
    let tokens = GDriveTokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp
            .refresh_token
            .unwrap_or_default(),
        expires_at: now + token_resp.expires_in.unwrap_or(3600),
    };

    save_tokens(&tokens)?;

    Ok("Authentication successful".to_string())
}

// ──────────────────────────────────────────────
// Google Drive API Helpers
// ──────────────────────────────────────────────

async fn drive_list_files(
    token: &str,
    folder_id: &str,
) -> Result<Vec<DriveFile>, String> {
    let client = reqwest::Client::new();
    let mut all_files = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut url = format!(
            "https://www.googleapis.com/drive/v3/files?q='{}'+in+parents+and+trashed=false&fields=files(id,name,mimeType,modifiedTime),nextPageToken&pageSize=1000",
            folder_id
        );
        if let Some(ref pt) = page_token {
            url.push_str(&format!("&pageToken={}", pt));
        }

        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("Drive list failed: {}", e))?;

        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            return Err(format!("Drive list error: {}", err));
        }

        let list: DriveFileList = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(files) = list.files {
            all_files.extend(files);
        }
        match list.next_page_token {
            Some(pt) => page_token = Some(pt),
            None => break,
        }
    }

    Ok(all_files)
}

async fn drive_download_file(token: &str, file_id: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?alt=media",
        file_id
    );
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Download error: {}", err));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Read bytes failed: {}", e))
}

async fn drive_upload_file(
    token: &str,
    folder_id: &str,
    name: &str,
    content: &[u8],
) -> Result<(String, String), String> {
    let client = reqwest::Client::new();

    let metadata = serde_json::json!({
        "name": name,
        "parents": [folder_id]
    });

    let form = reqwest::multipart::Form::new()
        .part(
            "metadata",
            reqwest::multipart::Part::text(metadata.to_string())
                .mime_str("application/json")
                .unwrap(),
        )
        .part(
            "file",
            reqwest::multipart::Part::bytes(content.to_vec())
                .file_name(name.to_string())
                .mime_str("application/octet-stream")
                .unwrap(),
        );

    let resp = client
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,modifiedTime")
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Upload error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let id = result["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No file ID returned".to_string())?;
    
    let modified_time = result["modifiedTime"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        
    Ok((id, modified_time))
}

async fn drive_update_file(
    token: &str,
    file_id: &str,
    content: &[u8],
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/upload/drive/v3/files/{}?uploadType=media&fields=id,modifiedTime",
        file_id
    );

    let resp = client
        .patch(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/octet-stream")
        .body(content.to_vec())
        .send()
        .await
        .map_err(|e| format!("Update failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Update error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let modified_time = result["modifiedTime"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    Ok(modified_time)
}

async fn drive_delete_file(token: &str, file_id: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}",
        file_id
    );

    let resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Delete failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Delete error: {}", err));
    }

    Ok(())
}

async fn drive_create_folder(
    token: &str,
    parent_id: &str,
    name: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();

    let metadata = serde_json::json!({
        "name": name,
        "mimeType": "application/vnd.google-apps.folder",
        "parents": [parent_id]
    });

    let resp = client
        .post("https://www.googleapis.com/drive/v3/files?fields=id")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(metadata.to_string())
        .send()
        .await
        .map_err(|e| format!("Create folder failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Create folder error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    result["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No folder ID returned".to_string())
}

/// Find the "Synabit Vault" root folder on Drive, or create it.
async fn find_or_create_vault_folder(token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();

    // Search for existing folder
    let query = format!(
        "name='{}' and mimeType='application/vnd.google-apps.folder' and trashed=false",
        VAULT_FOLDER_NAME
    );
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&pageSize=1",
        urlencoding::encode(&query)
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Search folder failed: {}", e))?;

    if resp.status().is_success() {
        let list: DriveFileList = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(files) = list.files {
            if let Some(existing) = files.first() {
                if let Some(ref id) = existing.id {
                    return Ok(id.clone());
                }
            }
        }
    }

    // Not found: create it at root
    let metadata = serde_json::json!({
        "name": VAULT_FOLDER_NAME,
        "mimeType": "application/vnd.google-apps.folder"
    });

    let resp = client
        .post("https://www.googleapis.com/drive/v3/files?fields=id")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(metadata.to_string())
        .send()
        .await
        .map_err(|e| format!("Create vault folder failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Create vault folder error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    result["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No vault folder ID returned".to_string())
}

/// Ensure a nested folder path exists on Drive (e.g. "Notes" or "Tasks/archived").
/// Returns the folder ID of the deepest folder.
async fn ensure_drive_folder_path(
    token: &str,
    manifest: &mut SyncManifest,
    relative_dir: &str,
) -> Result<String, String> {
    if relative_dir.is_empty() || relative_dir == "." {
        return Ok(manifest.vault_folder_id.clone());
    }

    // Check cache
    if let Some(id) = manifest.folder_ids.get(relative_dir) {
        return Ok(id.clone());
    }

    let parts: Vec<&str> = relative_dir.split('/').filter(|s| !s.is_empty()).collect();
    let mut parent_id = manifest.vault_folder_id.clone();
    let mut current_path = String::new();

    for part in parts {
        if !current_path.is_empty() {
            current_path.push('/');
        }
        current_path.push_str(part);

        if let Some(id) = manifest.folder_ids.get(&current_path) {
            parent_id = id.clone();
            continue;
        }

        // Search for the folder under parent
        let existing = drive_list_files(token, &parent_id).await?;
        let folder = existing.iter().find(|f| {
            f.name.as_deref() == Some(part)
                && f.mime_type.as_deref() == Some("application/vnd.google-apps.folder")
        });

        let folder_id = if let Some(f) = folder {
            f.id.clone().unwrap_or_default()
        } else {
            drive_create_folder(token, &parent_id, part).await?
        };

        manifest
            .folder_ids
            .insert(current_path.clone(), folder_id.clone());
        parent_id = folder_id;
    }

    Ok(parent_id)
}

// ──────────────────────────────────────────────
// Sync Engine
// ──────────────────────────────────────────────

/// Recursively collect all files from Drive folder tree.
/// Returns Vec of (relative_path, DriveFile).
/// Uses Box::pin to handle recursive async.
fn collect_drive_files<'a>(
    token: &'a str,
    folder_id: &'a str,
    prefix: &'a str,
) -> Pin<Box<dyn Future<Output = Result<Vec<(String, DriveFile)>, String>> + Send + 'a>> {
    Box::pin(async move {
        let mut result = Vec::new();
        let files = drive_list_files(token, folder_id).await?;

        for f in files {
            let name = f.name.clone().unwrap_or_default();
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };

            if f.mime_type.as_deref() == Some("application/vnd.google-apps.folder") {
                let sub_id = f.id.clone().unwrap_or_default();
                let sub_files = collect_drive_files(token, &sub_id, &relative).await?;
                result.extend(sub_files);
            } else {
                result.push((relative, f));
            }
        }

        Ok(result)
    })
}

/// Collect all local files relative to vault_path.
fn collect_local_files(vault_path: &str) -> Vec<String> {
    let base = Path::new(vault_path);
    let mut files = Vec::new();

    for entry in walkdir::WalkDir::new(base)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            // Skip hidden/meta files
            if name.starts_with('.') || name == ".synabit_sync_manifest.json" {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(base) {
                let rel_str = rel.to_string_lossy().to_string();
                // Normalize to forward slashes
                files.push(rel_str.replace('\\', "/"));
            }
        }
    }

    files
}

#[tauri::command]
pub async fn gdrive_sync_full(vault_path: String) -> Result<SyncResult, String> {
    let token = get_valid_token().await?;
    let mut manifest = load_manifest(&vault_path);
    let mut result = SyncResult {
        pulled: 0,
        pushed: 0,
        deleted: 0,
        errors: Vec::new(),
    };

    // 1. Ensure vault root folder exists on Drive
    if manifest.vault_folder_id.is_empty() {
        manifest.vault_folder_id = find_or_create_vault_folder(&token).await?;
    }

    let vault = Path::new(&vault_path);
    if !vault.exists() {
        fs::create_dir_all(vault).map_err(|e| e.to_string())?;
    }

    // 2. Collect remote files
    let drive_files = collect_drive_files(&token, &manifest.vault_folder_id, "").await?;

    // Build a map of relative_path -> DriveFile for quick lookup
    let mut drive_map: HashMap<String, DriveFile> = HashMap::new();
    for (rel, f) in &drive_files {
        drive_map.insert(rel.clone(), DriveFile {
            id: f.id.clone(),
            name: f.name.clone(),
            mime_type: f.mime_type.clone(),
            modified_time: f.modified_time.clone(),
        });
    }

    // 3. Collect local files
    let local_files = collect_local_files(&vault_path);

    // 4. PULL: files on Drive but not locally, or newer on Drive
    for (rel_path, df) in &drive_files {
        let local_path = vault.join(rel_path);
        let drive_id = df.id.clone().unwrap_or_default();
        let drive_mtime = df.modified_time.clone().unwrap_or_default();

        let should_pull = if !local_path.exists() {
            // If it's in the manifest, it means we synced it before and user deleted it locally. Do not pull (let DELETE step handle it).
            // If it's NOT in the manifest, it's a new remote file. Do pull.
            !manifest.files.contains_key(rel_path)
        } else if let Some(entry) = manifest.files.get(rel_path) {
            let current_hash = file_sha256(&local_path);
            if current_hash == entry.local_sha256 {
                // Local hasn't changed. Pull if Google's time is different (covers newer edits OR older times due to clock drift)
                drive_mtime != entry.drive_modified_time
            } else {
                // Local HAS changed (conflict). Only pull and overwrite if Google is strictly newer.
                drive_mtime > entry.drive_modified_time
            }
        } else {
            // File exists locally but not in manifest (first sync): compare hash
            false
        };

        if should_pull {
            // Ensure parent directory exists locally
            if let Some(parent) = local_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            match drive_download_file(&token, &drive_id).await {
                Ok(content) => {
                    if let Err(e) = fs::write(&local_path, &content) {
                        result.errors.push(format!("Write {}: {}", rel_path, e));
                        continue;
                    }
                    let hash = file_sha256(&local_path);
                    manifest.files.insert(
                        rel_path.clone(),
                        SyncFileEntry {
                            drive_file_id: drive_id,
                            local_sha256: hash,
                            drive_modified_time: drive_mtime,
                            local_modified_time: chrono::Utc::now().to_rfc3339(),
                        },
                    );
                    result.pulled += 1;
                }
                Err(e) => {
                    result.errors.push(format!("Download {}: {}", rel_path, e));
                }
            }
        }
    }

    // 4.5 Handle files deleted remotely but still present locally and in manifest
    // This MUST run before PUSH phase.
    let remotely_deleted_keys: Vec<String> = manifest
        .files
        .keys()
        .filter(|k| vault.join(k).exists() && !drive_map.contains_key(k.as_str()))
        .cloned()
        .collect();

    for key in &remotely_deleted_keys {
        let local_path = vault.join(key);
        let current_hash = file_sha256(&local_path);
        let entry_hash = manifest.files.get(key).map(|e| e.local_sha256.clone()).unwrap_or_default();

        if current_hash == entry_hash {
            // Logic 1: Not modified locally. Safe to delete.
            let _ = fs::remove_file(&local_path);
            manifest.files.remove(key);
            result.deleted += 1;
        } else {
            // Logic 2: Modified locally! We must recover it.
            // By stripping it from the manifest, the PUSH phase below will treat it 
            // as a brand new local file and will securely upload it.
            manifest.files.remove(key);
        }
    }

    // 5. PUSH: files local but not on Drive, or modified locally since last sync
    for rel_path in &local_files {
        let local_path = vault.join(rel_path);
        let current_hash = file_sha256(&local_path);

        if let Some(entry) = manifest.files.get(rel_path) {
            // Already tracked: check if local has changed
            if current_hash != entry.local_sha256 {
                // Local file changed, push update
                match fs::read(&local_path) {
                    Ok(content) => {
                        match drive_update_file(&token, &entry.drive_file_id, &content).await {
                            Ok(new_gdrive_time) => {
                                let mut updated = entry.clone();
                                updated.local_sha256 = current_hash;
                                updated.local_modified_time = chrono::Utc::now().to_rfc3339();
                                updated.drive_modified_time = new_gdrive_time;
                                manifest.files.insert(rel_path.clone(), updated);
                                result.pushed += 1;
                            }
                            Err(e) => {
                                result.errors.push(format!("Update {}: {}", rel_path, e));
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("Read {}: {}", rel_path, e));
                    }
                }
            }
            // else: unchanged, skip
        } else if !drive_map.contains_key(rel_path) {
            // New local file, not on Drive: upload
            let rel_dir = Path::new(rel_path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
                .replace('\\', "/");

            let filename = Path::new(rel_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            match ensure_drive_folder_path(&token, &mut manifest, &rel_dir).await {
                Ok(parent_folder_id) => {
                    match fs::read(&local_path) {
                        Ok(content) => {
                            match drive_upload_file(&token, &parent_folder_id, &filename, &content)
                                .await
                            {
                                Ok((file_id, new_gdrive_time)) => {
                                    manifest.files.insert(
                                        rel_path.clone(),
                                        SyncFileEntry {
                                            drive_file_id: file_id,
                                            local_sha256: current_hash,
                                            drive_modified_time: new_gdrive_time,
                                            local_modified_time: chrono::Utc::now().to_rfc3339(),
                                        },
                                    );
                                    result.pushed += 1;
                                }
                                Err(e) => {
                                    result.errors.push(format!("Upload {}: {}", rel_path, e));
                                }
                            }
                        }
                        Err(e) => {
                            result.errors.push(format!("Read {}: {}", rel_path, e));
                        }
                    }
                }
                Err(e) => {
                    result.errors.push(format!("Ensure folder {}: {}", rel_dir, e));
                }
            }
        }
    }

    // 6. DELETE from manifest entries that no longer exist locally or on Drive
    let stale_keys: Vec<String> = manifest
        .files
        .keys()
        .filter(|k| {
            let local_exists = vault.join(k).exists();
            let drive_exists = drive_map.contains_key(k.as_str());
            !local_exists && !drive_exists
        })
        .cloned()
        .collect();

    for key in &stale_keys {
        manifest.files.remove(key);
        result.deleted += 1;
    }

    // Handle files deleted locally but still on Drive: delete from Drive
    let locally_deleted: Vec<(String, String)> = manifest
        .files
        .iter()
        .filter(|(k, _v)| !vault.join(k).exists() && drive_map.contains_key(k.as_str()))
        .map(|(k, v)| (k.clone(), v.drive_file_id.clone()))
        .collect();

    for (key, drive_id) in &locally_deleted {
        match drive_delete_file(&token, drive_id).await {
            Ok(_) => {
                manifest.files.remove(key);
                result.deleted += 1;
            }
            Err(e) => {
                result.errors.push(format!("Delete remote {}: {}", key, e));
            }
        }
    }

    // 7. Save manifest
    save_manifest(&vault_path, &manifest)?;

    Ok(result)
}

#[tauri::command]
pub fn gdrive_get_cache_path() -> Result<String, String> {
    let cache = gdrive_cache_dir();
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    Ok(cache.to_string_lossy().to_string())
}
