use std::fs;
use super::{
    GDriveTokens, TokenResponse,
    CLIENT_ID, CLIENT_SECRET, TOKEN_URI, AUTH_URI, SCOPE,
    REDIRECT_PORT_START, REDIRECT_PORT_END,
    config_dir, tokens_path,
};

// ──────────────────────────────────────────────
// Token Management
// ──────────────────────────────────────────────

pub(crate) fn load_tokens() -> Option<GDriveTokens> {
    let path = tokens_path();
    if path.exists() {
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    } else {
        None
    }
}

pub(crate) fn save_tokens(tokens: &GDriveTokens) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let content = serde_json::to_string_pretty(tokens).map_err(|e| e.to_string())?;
    fs::write(tokens_path(), content).map_err(|e| e.to_string())
}

pub(crate) async fn get_valid_token() -> Result<String, String> {
    let mut tokens = load_tokens().ok_or("Not authenticated with Google Drive")?;
    let now = chrono::Utc::now().timestamp();

    if now >= tokens.expires_at - 60 {
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
// OAuth2 Commands
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
#[cfg(not(any(target_os = "android", target_os = "ios")))]
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

    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        AUTH_URI,
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(SCOPE),
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
        refresh_token: token_resp.refresh_token.unwrap_or_default(),
        expires_at: now + token_resp.expires_in.unwrap_or(3600),
    };

    save_tokens(&tokens)?;
    Ok("Authentication successful".to_string())
}

#[tauri::command]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn gdrive_auth_start() -> Result<String, String> {
    let redirect_uri = "synabit://oauth2callback";
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        AUTH_URI,
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(SCOPE),
    );

    let _ = opener::open(&auth_url);
    Ok("WAITING_DEEP_LINK".to_string())
}

#[tauri::command]
pub async fn gdrive_auth_complete(auth_code: String) -> Result<String, String> {
    let redirect_uri = "synabit://oauth2callback";
    
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URI)
        .form(&[
            ("code", auth_code.as_str()),
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("redirect_uri", redirect_uri),
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
        refresh_token: token_resp.refresh_token.unwrap_or_default(),
        expires_at: now + token_resp.expires_in.unwrap_or(3600),
    };

    save_tokens(&tokens)?;
    Ok("Authentication successful".to_string())
}
