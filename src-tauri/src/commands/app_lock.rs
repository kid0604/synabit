use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::secrets::SecretManager;

// ── Rate Limiting State (in-memory, resets on app restart) ──

#[derive(Default)]
pub struct AppLockState {
    pub failed_attempts: Mutex<u32>,
    pub locked_until: Mutex<Option<u64>>,
}

// ── DTOs ──

#[derive(Serialize, Clone)]
pub struct VerifyResult {
    pub success: bool,
    pub remaining_attempts: u8,
    pub locked_until: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct AppLockConfig {
    pub is_enabled: bool,
    pub app_lock_active: bool,
    pub protected_apps: Vec<String>,
    pub protected_notes: Vec<String>,
    pub auto_lock_timeout_secs: u64,
}

#[derive(Deserialize)]
pub struct AppLockConfigUpdate {
    pub protected_apps: Option<Vec<String>>,
    pub protected_notes: Option<Vec<String>>,
    pub auto_lock_timeout_secs: Option<u64>,
    pub app_lock_active: Option<bool>,
}

const MAX_ATTEMPTS: u32 = 5;
const LOCKOUT_DURATION_SECS: u64 = 30;
const DEFAULT_TIMEOUT_SECS: u64 = 300;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Commands ──

#[tauri::command]
pub fn setup_app_lock(app: tauri::AppHandle, pin: String) -> Result<(), String> {
    if pin.len() != 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err("PIN must be exactly 6 digits".to_string());
    }

    // Hash with Argon2id
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Algorithm, Argon2, Params, Version,
    };

    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(19456, 2, 1, None).map_err(|e| e.to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = argon2
        .hash_password(pin.as_bytes(), &salt)
        .map_err(|e| format!("Hash error: {}", e))?
        .to_string();

    SecretManager::set_app_lock_hash(Some(&app), hash)?;

    // Set default timeout if not already set
    let (_, _, timeout, _) = SecretManager::get_app_lock_config(Some(&app));
    if timeout.is_none() {
        SecretManager::update_app_lock_config(
            Some(&app),
            None,
            None,
            Some(DEFAULT_TIMEOUT_SECS),
            None,
        )?;
    }

    Ok(())
}

#[tauri::command]
pub fn verify_app_lock(
    app: tauri::AppHandle,
    pin: String,
    state: tauri::State<'_, AppLockState>,
) -> Result<VerifyResult, String> {
    // Check lockout
    {
        let locked = state.locked_until.lock().unwrap();
        if let Some(until) = *locked {
            if now_unix() < until {
                return Ok(VerifyResult {
                    success: false,
                    remaining_attempts: 0,
                    locked_until: Some(until),
                });
            }
        }
    }

    let hash =
        SecretManager::get_app_lock_hash(Some(&app)).ok_or("App lock is not set up")?;

    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };

    let parsed_hash =
        PasswordHash::new(&hash).map_err(|e| format!("Hash parse error: {}", e))?;
    let is_valid = Argon2::default()
        .verify_password(pin.as_bytes(), &parsed_hash)
        .is_ok();

    if is_valid {
        // Reset on success
        *state.failed_attempts.lock().unwrap() = 0;
        *state.locked_until.lock().unwrap() = None;
        Ok(VerifyResult {
            success: true,
            remaining_attempts: MAX_ATTEMPTS as u8,
            locked_until: None,
        })
    } else {
        let mut attempts = state.failed_attempts.lock().unwrap();
        *attempts += 1;
        let remaining = MAX_ATTEMPTS.saturating_sub(*attempts) as u8;

        let locked_until = if *attempts >= MAX_ATTEMPTS {
            let until = now_unix() + LOCKOUT_DURATION_SECS;
            *state.locked_until.lock().unwrap() = Some(until);
            *attempts = 0; // Reset counter after lockout
            Some(until)
        } else {
            None
        };

        Ok(VerifyResult {
            success: false,
            remaining_attempts: remaining,
            locked_until,
        })
    }
}

#[tauri::command]
pub fn remove_app_lock(app: tauri::AppHandle) -> Result<(), String> {
    SecretManager::clear_app_lock(Some(&app))
}

#[tauri::command]
pub fn change_app_lock(
    app: tauri::AppHandle,
    old_pin: String,
    new_pin: String,
    state: tauri::State<'_, AppLockState>,
) -> Result<(), String> {
    // Verify old PIN first
    let result = verify_app_lock(app.clone(), old_pin, state)?;
    if !result.success {
        return Err("Current PIN is incorrect".to_string());
    }
    // Set new PIN
    setup_app_lock(app, new_pin)
}

#[tauri::command]
pub fn get_app_lock_config(app: tauri::AppHandle) -> Result<AppLockConfig, String> {
    let is_enabled = SecretManager::get_app_lock_hash(Some(&app)).is_some();
    let (protected_apps, protected_notes, timeout, app_lock_active) =
        SecretManager::get_app_lock_config(Some(&app));

    Ok(AppLockConfig {
        is_enabled,
        app_lock_active: app_lock_active.unwrap_or(false),
        protected_apps: protected_apps.unwrap_or_default(),
        protected_notes: protected_notes.unwrap_or_default(),
        auto_lock_timeout_secs: timeout.unwrap_or(DEFAULT_TIMEOUT_SECS),
    })
}

#[tauri::command]
pub fn update_app_lock_config(
    app: tauri::AppHandle,
    config: AppLockConfigUpdate,
) -> Result<(), String> {
    SecretManager::update_app_lock_config(
        Some(&app),
        config.protected_apps,
        config.protected_notes,
        config.auto_lock_timeout_secs,
        config.app_lock_active,
    )
}
