// ──────────────────────────────────────────────
// File Manager — Google Drive Browse (OmniDrive)
//
// INDEPENDENT from Vault Sync. Has its own:
// - OAuth token (stored in macOS Keychain via `keyring`)
// - OAuth scope (`drive.readonly` — read ALL user files)
// - Connect / Disconnect lifecycle
//
// Vault Sync uses `drive.file` scope and stores tokens
// in a local JSON file. The two never share credentials.
// ──────────────────────────────────────────────

use std::net::TcpListener;
use std::io::{Read, Write};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::db::DbBridge;
use crate::models::file::FileMetadata;
use super::{CLIENT_ID, CLIENT_SECRET, TOKEN_URI};

const BROWSE_SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

// ──────────────────────────────────────────────
// Keychain helpers (per-vault token isolation)
// ──────────────────────────────────────────────

fn get_keyring_entry(key: &str, vault_path: &str) -> AppResult<keyring::Entry> {
    let safe_account = vault_path.replace("/", "_").replace("\\", "_");
    keyring::Entry::new(key, &safe_account)
        .map_err(|e| AppError::General(format!("Keyring error: {}", e)))
}

fn get_access_token(vault_path: &str) -> AppResult<String> {
    let entry = get_keyring_entry("synabit_gdrive_access_token", vault_path)?;
    entry
        .get_password()
        .map_err(|e| AppError::AuthFailed(format!("Google Drive not connected: {:?}", e)))
}

// ──────────────────────────────────────────────
// Token response
// ──────────────────────────────────────────────

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
}

// ──────────────────────────────────────────────
// Commands
// ──────────────────────────────────────────────

#[tauri::command]
pub async fn is_gdrive_connected(vault_path: String) -> AppResult<bool> {
    let entry = get_keyring_entry("synabit_gdrive_access_token", &vault_path)?;
    Ok(entry.get_password().is_ok())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GDriveUser {
    email_address: String,
}

#[derive(Deserialize)]
struct GDriveAboutResponse {
    user: GDriveUser,
}

#[tauri::command]
pub async fn get_gdrive_user_info(vault_path: String) -> AppResult<String> {
    let db = DbBridge::new(&vault_path)?;

    // Check cache first
    if let Some(email) = db.get_kv("gdrive_user_email")? {
        return Ok(email);
    }

    let access_token = get_access_token(&vault_path)?;

    let client = Client::new();
    let res = client
        .get("https://www.googleapis.com/drive/v3/about?fields=user")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| AppError::General(format!("Failed to fetch user info: {}", e)))?;

    if !res.status().is_success() {
        return Err(AppError::General(
            "Failed to fetch user info from API".to_string(),
        ));
    }

    let about: GDriveAboutResponse = res
        .json()
        .await
        .map_err(|e| AppError::General(format!("Failed to parse user info: {}", e)))?;

    let email = about.user.email_address;
    let _ = db.set_kv("gdrive_user_email", &email);

    Ok(email)
}

/// OAuth2 loopback flow for File Manager (separate from Vault Sync).
#[tauri::command]
pub async fn connect_gdrive(vault_path: String) -> AppResult<bool> {
    // 1. Start local server to capture the redirect
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::General(format!("Failed to bind local server: {}", e)))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::General(format!("Failed to get local addr: {}", e)))?
        .port();
    let redirect_uri = format!("http://localhost:{}", port);

    // 2. Construct Authorization URL with drive.readonly scope
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(BROWSE_SCOPE)
    );

    // 3. Open browser
    opener::open(auth_url)
        .map_err(|e| AppError::General(format!("Failed to open browser: {}", e)))?;

    // 4. Wait for the browser to redirect back to localhost
    let mut code = String::new();
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buffer = [0; 2048];
                let bytes_read = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                if let Some(first_line) = request.lines().next() {
                    let parts: Vec<&str> = first_line.split_whitespace().collect();
                    if parts.len() > 1 {
                        let path = parts[1];
                        let url_str = format!("http://localhost{}", path);
                        if let Ok(url) = Url::parse(&url_str) {
                            for (key, value) in url.query_pairs() {
                                if key == "code" {
                                    code = value.into_owned();
                                }
                            }
                        }
                    }
                }

                let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
                    <html><body style=\"font-family:system-ui;display:flex;justify-content:center;\
                    align-items:center;height:100vh;margin:0;background:#1a1a1a;color:#fff\">\
                    <div style=\"text-align:center\"><h1>✅ Connected!</h1>\
                    <p>You can close this window and return to OmniDrive.</p></div>\
                    <script>window.close();</script></body></html>";
                stream.write_all(response.as_bytes()).ok();
                stream.flush().ok();

                break;
            }
            Err(_) => continue,
        }
    }

    if code.is_empty() {
        return Err(AppError::General(
            "No authorization code received".to_string(),
        ));
    }

    // 5. Exchange code for tokens
    let client = Client::new();
    let token_res = client
        .post(TOKEN_URI)
        .form(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("code", code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::General(format!("Token request failed: {}", e)))?;

    if !token_res.status().is_success() {
        let err_text = token_res.text().await.unwrap_or_default();
        return Err(AppError::General(format!(
            "Token exchange error: {}",
            err_text
        )));
    }

    let token_data: OAuthTokenResponse = token_res
        .json()
        .await
        .map_err(|e| AppError::General(format!("Failed to parse tokens: {}", e)))?;

    // 6. Store tokens securely in Keychain
    let access_entry = get_keyring_entry("synabit_gdrive_access_token", &vault_path)?;
    access_entry
        .set_password(&token_data.access_token)
        .map_err(|e| AppError::General(format!("Failed to save access token: {}", e)))?;

    if let Some(refresh_token) = token_data.refresh_token {
        let refresh_entry = get_keyring_entry("synabit_gdrive_refresh_token", &vault_path)?;
        refresh_entry
            .set_password(&refresh_token)
            .map_err(|e| AppError::General(format!("Failed to save refresh token: {}", e)))?;
    }

    // Store non-secrets in DB
    let db = DbBridge::new(&vault_path)?;
    let expires_at = chrono::Utc::now().timestamp() + token_data.expires_in;
    db.set_kv("gdrive_expires_at", &expires_at.to_string())?;

    Ok(true)
}

#[tauri::command]
pub async fn disconnect_gdrive(vault_path: String) -> AppResult<bool> {
    let db = DbBridge::new(&vault_path)?;

    // Delete tokens from Keychain
    let access_entry = get_keyring_entry("synabit_gdrive_access_token", &vault_path)?;
    let _ = access_entry.delete_credential();

    let refresh_entry = get_keyring_entry("synabit_gdrive_refresh_token", &vault_path)?;
    let _ = refresh_entry.delete_credential();

    // Delete DB caches
    let _ = db.delete_kv("gdrive_expires_at");
    let _ = db.delete_kv("gdrive_user_email");
    let _ = db.delete_files_by_source_type("gdrive");

    Ok(true)
}

#[tauri::command]
pub async fn get_gdrive_files(vault_path: String) -> AppResult<Vec<FileMetadata>> {
    let access_token = get_access_token(&vault_path)?;
    let db = DbBridge::new(&vault_path)?;

    let client = Client::new();
    let res = client
        .get("https://www.googleapis.com/drive/v3/files")
        .header("Authorization", format!("Bearer {}", access_token))
        .query(&[
            ("fields", "files(id, name, mimeType, size, modifiedTime)"),
            ("q", "trashed=false"),
            ("pageSize", "1000"),
        ])
        .send()
        .await
        .map_err(|e| AppError::General(format!("Failed to fetch files: {}", e)))?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        return Err(AppError::General(format!("GDrive API error: {}", err_text)));
    }

    #[derive(Deserialize)]
    struct GDriveFileEntry {
        id: String,
        name: String,
        #[serde(rename = "mimeType")]
        mime_type: Option<String>,
        size: Option<String>,
        #[serde(rename = "modifiedTime")]
        modified_time: Option<String>,
    }

    #[derive(Deserialize)]
    struct GDriveListResp {
        files: Vec<GDriveFileEntry>,
    }

    let file_list: GDriveListResp = res
        .json()
        .await
        .map_err(|e| AppError::General(format!("Failed to parse GDrive files: {}", e)))?;

    let mut result_files = Vec::new();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    for gfile in file_list.files {
        let mime = gfile.mime_type.unwrap_or_default();
        let ext = if gfile.name.contains('.') {
            gfile.name.split('.').last().unwrap_or("").to_string()
        } else if mime.contains("folder") {
            "folder".to_string()
        } else if mime.contains("document") {
            "gdoc".to_string()
        } else if mime.contains("spreadsheet") {
            "gsheet".to_string()
        } else {
            "file".to_string()
        };

        let size = gfile
            .size
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let modified = gfile.modified_time.unwrap_or_else(|| now.clone());

        let meta = FileMetadata {
            id: format!("gdrive_{}", gfile.id),
            path: format!("gdrive://{}", gfile.id),
            filename: gfile.name,
            extension: ext,
            size,
            created_at: modified.clone(),
            modified_at: modified,
            tags: vec!["gdrive".to_string()],
            source_type: "gdrive".to_string(),
        };

        if db.upsert_file(&meta).is_ok() {
            result_files.push(meta);
        }
    }

    Ok(result_files)
}
