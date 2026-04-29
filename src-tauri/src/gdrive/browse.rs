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


use reqwest::{Client, Url};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{AppError, AppResult};
use crate::db::DbState;
use crate::models::file::FileMetadata;
use super::{CLIENT_ID, CLIENT_SECRET, TOKEN_URI};

const BROWSE_SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

// ──────────────────────────────────────────────
// Keychain helpers (per-vault token isolation)
// ──────────────────────────────────────────────

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn get_keyring_entry(key: &str, vault_path: &str) -> AppResult<keyring::Entry> {
    let safe_account = vault_path.replace("/", "_").replace("\\\\", "_");
    keyring::Entry::new(key, &safe_account)
        .map_err(|e| AppError::General(format!("Keyring error: {}", e)))
}

#[allow(dead_code)]
fn get_token_file_path(app_handle: &tauri::AppHandle, key: &str, vault_path: &str) -> std::path::PathBuf {
    use tauri::Manager;
    let safe_account = vault_path.replace("/", "_").replace("\\\\", "_");
    let mut path = app_handle.path().app_data_dir().unwrap_or_default();
    path.push(format!("{}_{}.json", key, safe_account));
    path
}

fn set_credential(_app_handle: &tauri::AppHandle, key: &str, vault_path: &str, value: &str) -> AppResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let entry = get_keyring_entry(key, vault_path)?;
        entry.set_password(value).map_err(|e| AppError::General(format!("Keyring error: {}", e)))
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let path = get_token_file_path(app_handle, key, vault_path);
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        std::fs::write(path, value).map_err(|e| AppError::General(format!("FS error: {}", e)))
    }
}

fn get_credential(_app_handle: &tauri::AppHandle, key: &str, vault_path: &str) -> AppResult<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let entry = get_keyring_entry(key, vault_path)?;
        entry.get_password().map_err(|e| AppError::General(format!("Keyring error: {}", e)))
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let path = get_token_file_path(app_handle, key, vault_path);
        std::fs::read_to_string(path).map_err(|e| AppError::General(format!("FS error: {}", e)))
    }
}

fn delete_credential(_app_handle: &tauri::AppHandle, key: &str, vault_path: &str) -> AppResult<()> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if let Ok(entry) = get_keyring_entry(key, vault_path) {
            let _ = entry.delete_credential();
        }
        Ok(())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let path = get_token_file_path(app_handle, key, vault_path);
        let _ = std::fs::remove_file(path);
        Ok(())
    }
}

async fn get_valid_access_token(app_handle: &tauri::AppHandle, vault_path: &str) -> AppResult<String> {
    use tauri::Manager;
    let db_state = app_handle.state::<DbState>();

    // Check if token is expired or about to expire (within 60 seconds)
    let needs_refresh = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(Some(expires_str)) = db.get_kv("gdrive_expires_at") {
            if let Ok(expires_at) = expires_str.parse::<i64>() {
                chrono::Utc::now().timestamp() >= expires_at - 60
            } else { false }
        } else { false }
    }; // lock dropped here

    if needs_refresh {
        if let Ok(refresh_token) = get_credential(app_handle, "synabit_gdrive_refresh_token", vault_path) {
            let client = Client::new();
            let resp = client.post(TOKEN_URI)
                .form(&[
                    ("client_id", CLIENT_ID),
                    ("client_secret", CLIENT_SECRET),
                    ("refresh_token", refresh_token.as_str()),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await
                .map_err(|e| AppError::General(format!("Token refresh request failed: {}", e)))?;

            if resp.status().is_success() {
                if let Ok(token_resp) = resp.json::<super::TokenResponse>().await {
                    set_credential(app_handle, "synabit_gdrive_access_token", vault_path, &token_resp.access_token)?;
                    if let Some(new_refresh) = token_resp.refresh_token {
                        set_credential(app_handle, "synabit_gdrive_refresh_token", vault_path, &new_refresh)?;
                    }
                    // Re-lock to update expiration
                    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    let new_expires_at = chrono::Utc::now().timestamp() + token_resp.expires_in.unwrap_or(3600);
                    let _ = db.set_kv("gdrive_expires_at", &new_expires_at.to_string());

                    return Ok(token_resp.access_token);
                }
            }
        }
    }

    get_credential(app_handle, "synabit_gdrive_access_token", vault_path)
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
pub async fn is_gdrive_connected(app_handle: tauri::AppHandle, vault_path: String) -> AppResult<bool> {
    Ok(get_credential(&app_handle, "synabit_gdrive_access_token", &vault_path).is_ok())
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
pub async fn get_gdrive_user_info(app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<String> {
    let cached_email = { state.lock().unwrap_or_else(|e| e.into_inner()).get_kv("gdrive_user_email")? };

    // Check cache first
    if let Some(email) = cached_email {
        return Ok(email);
    }

    let access_token = get_valid_access_token(&app_handle, &vault_path).await?;

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
    let _ = state.lock().unwrap_or_else(|e| e.into_inner()).set_kv("gdrive_user_email", &email);

    Ok(email)
}

/// OAuth2 loopback flow for File Manager (separate from Vault Sync).
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn connect_gdrive(app_handle: tauri::AppHandle, vault_path: String) -> AppResult<String> {
    // 1. Start local server to capture the redirect
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
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

    // 4. Wait for the browser to redirect back to localhost (with 120s timeout)
    let auth_code = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let (mut stream, _) = listener.accept().await.map_err(|e| format!("Accept failed: {}", e))?;
        
        let mut buffer = vec![0; 4096];
        let bytes_read = stream.read(&mut buffer).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);

        let mut extracted_code = String::new();
        if let Some(first_line) = request.lines().next() {
            let parts: Vec<&str> = first_line.split_whitespace().collect();
            if parts.len() > 1 {
                let path = parts[1];
                let url_str = format!("http://localhost{}", path);
                if let Ok(url) = Url::parse(&url_str) {
                    for (key, value) in url.query_pairs() {
                        if key == "code" {
                            extracted_code = value.into_owned();
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
            
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;

        if extracted_code.is_empty() {
            Err("No authorization code received".to_string())
        } else {
            Ok(extracted_code)
        }
    })
    .await
    .map_err(|_| AppError::General("Authentication timed out (120s). Please try again.".to_string()))?
    .map_err(AppError::General)?;

    let code = auth_code;

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
    set_credential(&app_handle, "synabit_gdrive_access_token", &vault_path, &token_data.access_token)?;

    if let Some(refresh_token) = token_data.refresh_token {
        set_credential(&app_handle, "synabit_gdrive_refresh_token", &vault_path, &refresh_token)?;
    }

    // Store non-secrets in DB
    {
        use tauri::Manager;
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let expires_at = chrono::Utc::now().timestamp() + token_data.expires_in;
        db.set_kv("gdrive_expires_at", &expires_at.to_string())?;
    }

    Ok("SUCCESS".to_string())
}


#[tauri::command]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn connect_gdrive(app_handle: tauri::AppHandle, vault_path: String) -> AppResult<String> {
    use tauri_plugin_opener::OpenerExt;
    let redirect_uri = "com.synabit.app:/oauth2callback";
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state=omnidrive",
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(BROWSE_SCOPE)
    );

    app_handle.opener().open_url(auth_url, None::<String>).map_err(|e| AppError::General(format!("Failed to open browser: {}", e)))?;
    Ok("WAITING_DEEP_LINK".to_string())
}

#[tauri::command]
pub async fn connect_gdrive_complete(app_handle: tauri::AppHandle, auth_code: String, vault_path: String) -> AppResult<bool> {
    let redirect_uri = "com.synabit.app:/oauth2callback";
    
    let client = Client::new();
    let token_res = client
        .post(TOKEN_URI)
        .form(&[
            ("client_id", CLIENT_ID),
            ("code", auth_code.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
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

    set_credential(&app_handle, "synabit_gdrive_access_token", &vault_path, &token_data.access_token)?;

    if let Some(refresh_token) = token_data.refresh_token {
        set_credential(&app_handle, "synabit_gdrive_refresh_token", &vault_path, &refresh_token)?;
    }

    {
        use tauri::Manager;
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let expires_at = chrono::Utc::now().timestamp() + token_data.expires_in;
        let _ = db.set_kv("gdrive_expires_at", &expires_at.to_string());
    }

    Ok(true)
}

#[tauri::command]
pub async fn disconnect_gdrive(app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<bool> {

    // Delete tokens from Keychain
    let _ = delete_credential(&app_handle, "synabit_gdrive_access_token", &vault_path);
    let _ = delete_credential(&app_handle, "synabit_gdrive_refresh_token", &vault_path);

    // Delete DB caches
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let _ = db.delete_kv("gdrive_expires_at");
    let _ = db.delete_kv("gdrive_user_email");
    let _ = db.delete_files_by_source_type("gdrive");

    Ok(true)
}

#[tauri::command]
pub async fn get_gdrive_files(app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<Vec<FileMetadata>> {
    let access_token = get_valid_access_token(&app_handle, &vault_path).await?;

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
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    for gfile in file_list.files {
        let mime = gfile.mime_type.unwrap_or_default();
        let ext = if gfile.name.contains('.') {
            gfile.name.split('.').next_back().unwrap_or("").to_string()
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
