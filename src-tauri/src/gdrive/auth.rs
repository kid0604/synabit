use super::CLIENT_SECRET;
use super::{
    generate_pkce_pair, GDriveTokens, TokenResponse, AUTH_URI, CLIENT_ID,
    REDIRECT_PORT_END, REDIRECT_PORT_START, SCOPE, TOKEN_URI,
};


// ──────────────────────────────────────────────
// Token Management
// ──────────────────────────────────────────────

use crate::secrets::SecretManager;

pub(crate) fn load_tokens(app_handle: &tauri::AppHandle) -> Option<GDriveTokens> {
    let content = SecretManager::get_vault_sync_config(Some(app_handle))?;
    serde_json::from_str(&content).ok()
}

pub(crate) fn save_tokens(
    app_handle: &tauri::AppHandle,
    tokens: &GDriveTokens,
) -> Result<(), String> {
    let content = serde_json::to_string(tokens).map_err(|e| e.to_string())?;
    SecretManager::set_vault_sync_config(Some(app_handle), content)
}

pub(crate) async fn get_valid_token(app_handle: &tauri::AppHandle) -> Result<String, String> {
    let mut tokens = load_tokens(app_handle).ok_or("Not authenticated with Google Drive")?;
    let now = chrono::Utc::now().timestamp();

    if now >= tokens.expires_at - 60 {
        // Desktop: Google requires client_secret for refresh
        // Mobile: public client, no secret needed
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let form_data = vec![
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("refresh_token", tokens.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        #[cfg(any(target_os = "android", target_os = "ios"))]
        let form_data = vec![
            ("client_id", CLIENT_ID),
            ("refresh_token", tokens.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let client = reqwest::Client::new();
        let resp = client
            .post(TOKEN_URI)
            .form(&form_data)
            .send()
            .await
            .map_err(|e| format!("Token refresh request failed: {}", e))?;

        if !resp.status().is_success() {
            let err_text = resp.text().await.unwrap_or_default();
            if err_text.contains("invalid_grant") {
                let _ = SecretManager::clear_vault_sync_config(Some(app_handle));
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
        save_tokens(app_handle, &tokens)?;
    }

    Ok(tokens.access_token)
}

// ──────────────────────────────────────────────
// OAuth2 Commands
// ──────────────────────────────────────────────

#[tauri::command]
pub fn gdrive_auth_status(app_handle: tauri::AppHandle) -> Result<bool, String> {
    Ok(load_tokens(&app_handle).is_some())
}

#[tauri::command]
pub fn gdrive_disconnect(app_handle: tauri::AppHandle) -> Result<(), String> {
    let _ = SecretManager::clear_vault_sync_config(Some(&app_handle));
    Ok(())
}

#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn gdrive_auth_start(app_handle: tauri::AppHandle) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    log::info!("Starting Google Drive OAuth2 flow (Desktop, PKCE)...");

    // Generate PKCE pair
    let (code_verifier, code_challenge) = generate_pkce_pair();

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

    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256",
        AUTH_URI,
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(SCOPE),
        urlencoding::encode(&code_challenge),
    );

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

        let code = request
            .lines()
            .next()
            .and_then(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let path = parts[1];
                    path.split('?').nth(1).and_then(|qs| {
                        qs.split('&')
                            .find_map(|pair| {
                                let (key, val) = pair.split_once('=')?;
                                if key == "code" { Some(val.to_string()) } else { None }
                            })
                    })
                } else {
                    None
                }
            })
            .ok_or_else(|| "No auth code in callback".to_string())?;

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

    // Exchange code for tokens — Desktop: client_secret + PKCE code_verifier
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let form_data = vec![
        ("code", auth_code.as_str()),
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
        ("code_verifier", code_verifier.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];

    #[cfg(any(target_os = "android", target_os = "ios"))]
    let form_data = vec![
        ("code", auth_code.as_str()),
        ("client_id", CLIENT_ID),
        ("code_verifier", code_verifier.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];

    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URI)
        .form(&form_data)
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
        refresh_token: token_resp.refresh_token.unwrap_or_default(),
        expires_at: now + token_resp.expires_in.unwrap_or(3600),
    };

    save_tokens(&app_handle, &tokens)?;
    log::info!("Google Drive authentication successful (Desktop, PKCE).");
    Ok("Authentication successful".to_string())
}

#[tauri::command]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn gdrive_auth_start(app_handle: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    log::info!("Starting Google Drive OAuth2 flow (Mobile, PKCE)...");

    // Generate PKCE pair and store verifier for the completion step
    let (code_verifier, code_challenge) = generate_pkce_pair();
    {
        use tauri::Manager;
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.set_kv("pkce_code_verifier_vault", &code_verifier);
    }

    let redirect_uri = "com.synabit.app:/oauth2callback";
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256",
        AUTH_URI,
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(SCOPE),
        urlencoding::encode(&code_challenge),
    );

    app_handle
        .opener()
        .open_url(auth_url, None::<String>)
        .map_err(|e| format!("Failed to open browser: {}", e))?;
    Ok("WAITING_DEEP_LINK".to_string())
}

#[tauri::command]
pub async fn gdrive_auth_complete(
    app_handle: tauri::AppHandle,
    auth_code: String,
) -> Result<String, String> {
    // Retrieve the stored PKCE code_verifier
    let code_verifier = {
        use tauri::Manager;
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_kv("pkce_code_verifier_vault")
            .map_err(|e| format!("DB error: {}", e))?
            .ok_or_else(|| "No PKCE verifier found. Please restart authentication.".to_string())?
    };

    let redirect_uri = "com.synabit.app:/oauth2callback";

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let form_data = vec![
        ("code", auth_code.as_str()),
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
        ("code_verifier", code_verifier.as_str()),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];

    #[cfg(any(target_os = "android", target_os = "ios"))]
    let form_data = vec![
        ("code", auth_code.as_str()),
        ("client_id", CLIENT_ID),
        ("code_verifier", code_verifier.as_str()),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];

    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URI)
        .form(&form_data)
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
        refresh_token: token_resp.refresh_token.unwrap_or_default(),
        expires_at: now + token_resp.expires_in.unwrap_or(3600),
    };

    save_tokens(&app_handle, &tokens)?;

    // Clean up stored verifier
    {
        use tauri::Manager;
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.delete_kv("pkce_code_verifier_vault");
    }

    log::info!("Google Drive authentication complete (Mobile, PKCE).");
    Ok("Authentication successful".to_string())
}
