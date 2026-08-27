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

use reqwest::Client;
use serde::Deserialize;

// The loopback listener the desktop OAuth flow catches its redirect on, and the
// URL parsing that reads the code back out of the request line. Mobile is
// redirected through a custom scheme and binds no socket.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use reqwest::Url;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tokio::net::TcpListener;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use super::CLIENT_SECRET;
use super::{generate_pkce_pair, CLIENT_ID, TOKEN_URI};
use crate::db::DbState;
use crate::error::{AppError, AppResult};

const BROWSE_SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";

// ──────────────────────────────────────────────
// Keychain helpers (per-vault token isolation)
// ──────────────────────────────────────────────

use crate::secrets::SecretManager;

fn set_credential(
    app_handle: &tauri::AppHandle,
    key: &str,
    vault_path: &str,
    value: &str,
) -> AppResult<()> {
    SecretManager::set_vault_token(Some(app_handle), key, vault_path, value.to_string())
        .map_err(AppError::General)
}

fn get_credential(app_handle: &tauri::AppHandle, key: &str, vault_path: &str) -> AppResult<String> {
    SecretManager::get_vault_token(Some(app_handle), key, vault_path)
        .ok_or_else(|| AppError::AuthFailed("No token found".to_string()))
}

fn delete_credential(app_handle: &tauri::AppHandle, key: &str, vault_path: &str) -> AppResult<()> {
    let _ = SecretManager::delete_vault_token(Some(app_handle), key, vault_path);
    Ok(())
}

async fn get_valid_access_token(
    app_handle: &tauri::AppHandle,
    vault_path: &str,
) -> AppResult<String> {
    use tauri::Manager;
    let db_state = app_handle.state::<DbState>();

    // Check if token is expired or about to expire (within 60 seconds)
    let needs_refresh = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(Some(expires_str)) = db.get_kv("gdrive_expires_at") {
            if let Ok(expires_at) = expires_str.parse::<i64>() {
                chrono::Utc::now().timestamp() >= expires_at - 60
            } else {
                false
            }
        } else {
            false
        }
    }; // lock dropped here

    if needs_refresh {
        if let Ok(refresh_token) =
            get_credential(app_handle, "synabit_gdrive_refresh_token", vault_path)
        {
            let client = Client::new();
            // Google requires client_secret for refresh on desktop apps
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let form_data = vec![
                ("client_id", CLIENT_ID),
                ("client_secret", CLIENT_SECRET),
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ];

            #[cfg(any(target_os = "android", target_os = "ios"))]
            let form_data = vec![
                ("client_id", CLIENT_ID),
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ];

            let resp = client
                .post(TOKEN_URI)
                .form(&form_data)
                .send()
                .await
                .map_err(|e| AppError::General(format!("Token refresh request failed: {}", e)))?;

            if resp.status().is_success() {
                if let Ok(token_resp) = resp.json::<super::TokenResponse>().await {
                    set_credential(
                        app_handle,
                        "synabit_gdrive_access_token",
                        vault_path,
                        &token_resp.access_token,
                    )?;
                    if let Some(new_refresh) = token_resp.refresh_token {
                        set_credential(
                            app_handle,
                            "synabit_gdrive_refresh_token",
                            vault_path,
                            &new_refresh,
                        )?;
                    }
                    // Re-lock to update expiration
                    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    let new_expires_at =
                        chrono::Utc::now().timestamp() + token_resp.expires_in.unwrap_or(3600);
                    crate::error::logged("store token expiry", "gdrive_expires_at", db.set_kv("gdrive_expires_at", &new_expires_at.to_string()));

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
pub async fn is_gdrive_connected(
    app_handle: tauri::AppHandle,
    vault_path: String,
) -> AppResult<bool> {
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
pub async fn get_gdrive_user_info(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<String> {
    let cached_email = {
        state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_kv("gdrive_user_email")?
    };

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
    let _ = state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set_kv("gdrive_user_email", &email);

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

    // 2. Generate PKCE pair
    let (code_verifier, code_challenge) = generate_pkce_pair();

    // 3. Construct Authorization URL with drive.readonly scope + PKCE
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256",
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(BROWSE_SCOPE),
        urlencoding::encode(&code_challenge)
    );

    // 4. Open browser
    opener::open(auth_url)
        .map_err(|e| AppError::General(format!("Failed to open browser: {}", e)))?;

    // 5. Wait for the browser to redirect back to localhost (with 120s timeout)
    let auth_code = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| format!("Accept failed: {}", e))?;

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
    .map_err(|_| {
        AppError::General("Authentication timed out (120s). Please try again.".to_string())
    })?
    .map_err(AppError::General)?;

    let code = auth_code;

    if code.is_empty() {
        return Err(AppError::General(
            "No authorization code received".to_string(),
        ));
    }

    // 6. Exchange code for tokens — Desktop: client_secret + PKCE code_verifier
    let client = Client::new();
    let token_res = client
        .post(TOKEN_URI)
        .form(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("code_verifier", code_verifier.as_str()),
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

    // 7. Store tokens securely in Keychain
    set_credential(
        &app_handle,
        "synabit_gdrive_access_token",
        &vault_path,
        &token_data.access_token,
    )?;

    if let Some(refresh_token) = token_data.refresh_token {
        set_credential(
            &app_handle,
            "synabit_gdrive_refresh_token",
            &vault_path,
            &refresh_token,
        )?;
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
// `vault_path` is unread here — the mobile flow only opens a browser and waits
// for the deep link, and the vault is not known until `connect_gdrive_complete`
// takes it. It keeps its name rather than gaining an underscore because the
// name is the IPC contract: Tauri maps the caller's `vaultPath` onto it, and
// renaming the binding renames the argument the front end has to send.
#[allow(unused_variables)]
pub async fn connect_gdrive(app_handle: tauri::AppHandle, vault_path: String) -> AppResult<String> {
    use tauri_plugin_opener::OpenerExt;

    // Generate PKCE pair and store verifier for completion step
    let (code_verifier, code_challenge) = generate_pkce_pair();
    {
        use tauri::Manager;
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        crate::error::logged("store PKCE verifier", "pkce_code_verifier_omnidrive", db.set_kv("pkce_code_verifier_omnidrive", &code_verifier));
    }

    let redirect_uri = "com.synabit.app:/oauth2callback";
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state=omnidrive&code_challenge={}&code_challenge_method=S256",
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(BROWSE_SCOPE),
        urlencoding::encode(&code_challenge)
    );

    app_handle
        .opener()
        .open_url(auth_url, None::<String>)
        .map_err(|e| AppError::General(format!("Failed to open browser: {}", e)))?;
    Ok("WAITING_DEEP_LINK".to_string())
}

#[tauri::command]
pub async fn connect_gdrive_complete(
    app_handle: tauri::AppHandle,
    auth_code: String,
    vault_path: String,
) -> AppResult<bool> {
    // Retrieve the stored PKCE code_verifier
    let code_verifier = {
        use tauri::Manager;
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("pkce_code_verifier_omnidrive")
            .map_err(|e| AppError::General(format!("DB error: {}", e)))?
            .ok_or_else(|| {
                AppError::General(
                    "No PKCE verifier found. Please restart authentication.".to_string(),
                )
            })?
    };

    let redirect_uri = "com.synabit.app:/oauth2callback";

    let client = Client::new();
    let token_res = client
        .post(TOKEN_URI)
        .form(&[
            ("client_id", CLIENT_ID),
            ("code_verifier", code_verifier.as_str()),
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

    set_credential(
        &app_handle,
        "synabit_gdrive_access_token",
        &vault_path,
        &token_data.access_token,
    )?;

    if let Some(refresh_token) = token_data.refresh_token {
        set_credential(
            &app_handle,
            "synabit_gdrive_refresh_token",
            &vault_path,
            &refresh_token,
        )?;
    }

    {
        use tauri::Manager;
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let expires_at = chrono::Utc::now().timestamp() + token_data.expires_in;
        crate::error::logged("store token expiry", "gdrive_expires_at", db.set_kv("gdrive_expires_at", &expires_at.to_string()));
        // Clean up stored verifier
        crate::error::logged("clear PKCE verifier", "pkce_code_verifier_omnidrive", db.delete_kv("pkce_code_verifier_omnidrive"));
    }

    Ok(true)
}

#[tauri::command]
pub async fn disconnect_gdrive(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<bool> {
    // Delete tokens from Keychain
    let _ = delete_credential(&app_handle, "synabit_gdrive_access_token", &vault_path);
    let _ = delete_credential(&app_handle, "synabit_gdrive_refresh_token", &vault_path);

    // Delete DB caches
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    crate::error::logged("clear token expiry", "gdrive_expires_at", db.delete_kv("gdrive_expires_at"));
    crate::error::logged("clear account email", "gdrive_user_email", db.delete_kv("gdrive_user_email"));
    crate::error::logged(
        "drop cached Drive files",
        "gdrive",
        db.forget_provider(crate::file_providers::gdrive::PROVIDER).map(|_| ()),
    );

    Ok(true)
}

/// Bring the local picture of a Drive account up to date.
///
/// Metadata only, and every page of it.
///
/// What this replaces asked for a single page of a thousand files and stopped:
/// a Drive with more than that reported a fraction of itself and said nothing
/// about the rest. It also wrote what it found into the legacy `files` table
/// while the screen read from `nodes`, so even the thousand it did fetch never
/// appeared. The paging now lives in `file_providers`, where a test drives it.
#[tauri::command]
pub async fn get_gdrive_files(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<usize> {
    use crate::file_providers::{self, gdrive::PROVIDER};

    let access_token = get_valid_access_token(&app_handle, &vault_path).await?;
    let account = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("gdrive_user_email").ok().flatten().unwrap_or_default()
    };

    // Fetched first, written second. The paging loop is synchronous — it is
    // pure logic, and keeping it that way is what makes it testable — so each
    // page is awaited here and handed to it complete.
    let client = Client::new();
    let mut pages: Vec<file_providers::RemotePage> = Vec::new();
    let mut token: Option<String> = None;
    loop {
        let page = file_providers::gdrive::fetch_page(&client, &access_token, token.clone()).await?;
        let next = page.next.clone();
        pages.push(page);
        match next {
            Some(next) if Some(&next) != token.as_ref() && pages.len() < 1_000 => {
                token = Some(next)
            }
            _ => break,
        }
    }

    let mut cursor = 0usize;
    let files = file_providers::collect_pages(|_| {
        let page = pages.get(cursor).cloned().unwrap_or_default();
        cursor += 1;
        Ok(page)
    })?;

    let now = chrono::Utc::now().timestamp_millis();
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut listed = Vec::with_capacity(files.len());

    for file in &files {
        let node_id = file_providers::node_id_for(PROVIDER, &file.remote_id);

        // Whatever a person attached to this file stays attached: a listing
        // knows what Drive holds, not what the reader thought about it.
        let existing = db.get_node(&node_id).ok().flatten();
        let mut properties = serde_json::json!({
            "extension": file.extension,
            "size": file.size,
            "source_type": PROVIDER,
            "web_url": file.web_url,
            "tags": [],
            "people": [],
        });
        if let (Some(existing), Some(props)) = (&existing, properties.as_object_mut()) {
            for field in ["tags", "people", "linked_projects"] {
                if let Some(previous) = existing.properties.get(field) {
                    if previous.as_array().is_some_and(|a| !a.is_empty()) {
                        props.insert(field.to_string(), previous.clone());
                    }
                }
            }
        }

        db.upsert_node(&crate::models::node::NodeMetadata {
            id: node_id.clone(),
            node_type: "file".to_string(),
            title: file.name.clone(),
            content: String::new(),
            properties,
            created_at: existing
                .as_ref()
                .map(|n| n.created_at.clone())
                .unwrap_or_else(|| file.modified_at.clone()),
            updated_at: file.modified_at.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            blocks: None,
        })?;

        db.upsert_remote_file(
            &crate::db::RemoteEntry {
                node_id: node_id.clone(),
                provider: PROVIDER.to_string(),
                remote_id: file.remote_id.clone(),
                account: account.clone(),
                size: file.size,
                modified_at: file.modified_at.clone(),
                web_url: file.web_url.clone(),
            },
            now,
        )?;

        listed.push(node_id);
    }

    // Anything the account no longer lists has gone from it.
    let dropped = db.prune_remote_files(PROVIDER, &listed)?;
    log::info!(
        "gdrive: {} file(s) listed across {} page(s), {dropped} no longer there",
        files.len(),
        pages.len()
    );

    Ok(files.len())
}
