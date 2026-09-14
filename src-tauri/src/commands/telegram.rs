//! The settings screen's handle on the Telegram bot. See `syn::telegram`.
//!
//! Every command answers with the status afterwards, so the screen draws what
//! is true rather than what it expected.

use crate::error::AppError;
use crate::syn::telegram::{self, Status};

/// Where the pairing link points, and until when it works.
#[derive(serde::Serialize)]
pub struct PairingLink {
    pub link: String,
    pub expires_at: String,
}

fn with_db<T>(app: &tauri::AppHandle, f: impl FnOnce(&crate::db::DbBridge) -> T) -> T {
    use tauri::Manager;
    let state = app.state::<crate::db::DbState>();
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    f(&db)
}

#[tauri::command]
pub async fn telegram_status(app: tauri::AppHandle) -> Result<Status, AppError> {
    Ok(telegram::status(&app).await)
}

/// Check a token with Telegram, then keep it and start the bot.
///
/// Checked before it is stored, so a mistyped token is an error on the screen
/// where it was typed rather than a bot that silently never starts.
#[tauri::command]
pub async fn telegram_set_token(app: tauri::AppHandle, token: String) -> Result<Status, AppError> {
    if !cfg!(desktop) {
        return Err(AppError::General("The Telegram bot runs in the desktop app".to_string()));
    }
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AppError::General("A bot token is needed".to_string()));
    }

    let api = telegram::api::Api::new(&token).map_err(|e| AppError::General(e.to_string()))?;
    let me = api.get_me().await.map_err(|e| match e {
        telegram::api::ApiError::Unauthorized => {
            AppError::General("Telegram did not accept this token".to_string())
        }
        other => AppError::General(other.to_string()),
    })?;
    let username = me.username.unwrap_or_else(|| me.id.to_string());

    // A different bot is a different set of chats. Whoever was paired with the
    // old one, and whatever it was holding, means nothing to this one.
    let previous = with_db(&app, |db| db.get_kv(telegram::USERNAME_KEY).ok().flatten());
    if previous.as_deref().is_some_and(|p| p != username) {
        telegram::forget(&app);
    }

    crate::secrets::SecretManager::set_syn_api_key(Some(&app), telegram::TOKEN_SLOT, &token)
        .map_err(AppError::General)?;
    with_db(&app, |db| db.set_kv(telegram::USERNAME_KEY, &username))?;

    telegram::restart(&app);
    Ok(telegram::status(&app).await)
}

/// Remove the token. The bot stops, and this computer forgets it.
#[tauri::command]
pub async fn telegram_clear_token(app: tauri::AppHandle) -> Result<Status, AppError> {
    crate::secrets::SecretManager::set_syn_api_key(Some(&app), telegram::TOKEN_SLOT, "")
        .map_err(AppError::General)?;
    telegram::forget(&app);
    telegram::restart(&app);
    Ok(telegram::status(&app).await)
}

/// A link that pairs whichever Telegram account opens it.
#[tauri::command]
pub async fn telegram_start_pairing(app: tauri::AppHandle) -> Result<PairingLink, AppError> {
    let Some(username) = with_db(&app, |db| db.get_kv(telegram::USERNAME_KEY).ok().flatten()) else {
        return Err(AppError::General("Connect a bot before pairing".to_string()));
    };
    let pending = with_db(&app, |db| telegram::pairing::begin(db, chrono::Utc::now()))?;
    Ok(PairingLink {
        link: telegram::pairing::link(&username, &pending.code),
        expires_at: pending.expires_at,
    })
}

#[tauri::command]
pub async fn telegram_unpair(app: tauri::AppHandle) -> Result<Status, AppError> {
    with_db(&app, telegram::pairing::unpair)?;
    Ok(telegram::status(&app).await)
}

/// Whether what rings this computer is sent to the paired phone as well.
#[tauri::command]
pub async fn telegram_set_reminders(app: tauri::AppHandle, on: bool) -> Result<Status, AppError> {
    with_db(&app, |db| telegram::remind::set_on(db, on))?;
    Ok(telegram::status(&app).await)
}

/// Try again after a problem that needed a person — another computer was
/// polling, or the token was refused.
#[tauri::command]
pub async fn telegram_retry(app: tauri::AppHandle) -> Result<Status, AppError> {
    telegram::restart(&app);
    Ok(telegram::status(&app).await)
}
