use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct AppSecrets {
    pub e2ee_password: Option<String>,          // KEEP for migration
    #[serde(default)]
    pub e2ee_key: Option<String>,               // NEW: base64-encoded 32-byte key
    pub global_sync_config: Option<String>,
    pub vault_tokens: HashMap<String, String>,
    #[serde(default)]
    pub app_lock_hash: Option<String>,           // Argon2id PHC hash string
    #[serde(default)]
    pub protected_apps: Option<Vec<String>>,     // ["finance", "people"]
    #[serde(default)]
    pub protected_notes: Option<Vec<String>>,    // ["Notes/diary.md"]
    #[serde(default)]
    pub auto_lock_timeout_secs: Option<u64>,     // Default 300
    #[serde(default)]
    pub app_lock_active: Option<bool>,           // Tier 1 toggle (independent of PIN)
}

pub struct SecretManager;

impl SecretManager {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn get_entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new("synabit", "secrets").map_err(|e| format!("Keyring error: {}", e))
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    fn get_file_path(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
        use tauri::Manager;
        let mut path = app_handle.path().app_data_dir().unwrap_or_default();
        path.push("synabit_secrets.json");
        path
    }

    pub fn load_secrets(app_handle: Option<&tauri::AppHandle>) -> AppSecrets {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = app_handle; // unused on desktop
            if let Ok(entry) = Self::get_entry() {
                if let Ok(content) = entry.get_password() {
                    if let Ok(secrets) = serde_json::from_str::<AppSecrets>(&content) {
                        return secrets;
                    }
                }
            }
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            if let Some(handle) = app_handle {
                let path = Self::get_file_path(handle);
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(secrets) = serde_json::from_str::<AppSecrets>(&content) {
                        return secrets;
                    }
                }
            }
        }
        AppSecrets::default()
    }

    pub fn save_secrets(
        app_handle: Option<&tauri::AppHandle>,
        secrets: &AppSecrets,
    ) -> Result<(), String> {
        let content = serde_json::to_string(secrets).map_err(|e| e.to_string())?;

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = app_handle;
            let entry = Self::get_entry()?;
            entry
                .set_password(&content)
                .map_err(|e| format!("Keyring error: {}", e))
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            if let Some(handle) = app_handle {
                let path = Self::get_file_path(handle);
                if let Some(p) = path.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                std::fs::write(path, content).map_err(|e| format!("FS error: {}", e))
            } else {
                Err("AppHandle is required on mobile to save secrets".to_string())
            }
        }
    }

    // ──────────────────────────────────────────────
    // E2EE
    // ──────────────────────────────────────────────
    pub fn get_e2ee_password(app_handle: Option<&tauri::AppHandle>) -> Option<String> {
        Self::load_secrets(app_handle).e2ee_password
    }

    pub fn set_e2ee_password(
        app_handle: Option<&tauri::AppHandle>,
        pwd: String,
    ) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_password = Some(pwd);
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn clear_e2ee_password(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_password = None;
        Self::save_secrets(app_handle, &secrets)
    }

    // ──────────────────────────────────────────────
    // E2EE Auto Key (new passwordless system)
    // ──────────────────────────────────────────────
    pub fn get_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> Option<[u8; 32]> {
        let secrets = Self::load_secrets(app_handle);
        secrets.e2ee_key.as_ref().and_then(|b64| {
            use base64::Engine;
            use zeroize::Zeroize;
            let mut bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                bytes.zeroize();
                Some(key)
            } else {
                bytes.zeroize();
                None
            }
        })
    }

    pub fn set_e2ee_key(
        app_handle: Option<&tauri::AppHandle>,
        key: &[u8; 32],
    ) -> Result<(), String> {
        use base64::Engine;
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_key = Some(base64::engine::general_purpose::STANDARD.encode(key));
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn clear_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_key = None;
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn has_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> bool {
        Self::get_e2ee_key(app_handle).is_some()
    }

    // ──────────────────────────────────────────────
    // Vault Sync Config
    // ──────────────────────────────────────────────
    pub fn get_vault_sync_config(app_handle: Option<&tauri::AppHandle>) -> Option<String> {
        Self::load_secrets(app_handle).global_sync_config
    }

    pub fn set_vault_sync_config(
        app_handle: Option<&tauri::AppHandle>,
        config: String,
    ) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.global_sync_config = Some(config);
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn clear_vault_sync_config(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.global_sync_config = None;
        Self::save_secrets(app_handle, &secrets)
    }

    // ──────────────────────────────────────────────
    // Vault Tokens (GDrive OAuth)
    // ──────────────────────────────────────────────
    pub fn get_vault_token(
        app_handle: Option<&tauri::AppHandle>,
        key: &str,
        vault_path: &str,
    ) -> Option<String> {
        let map_key = format!(
            "{}_{}",
            key,
            vault_path.replace("/", "_").replace("\\\\", "_")
        );
        Self::load_secrets(app_handle).vault_tokens.get(&map_key).cloned()
    }

    pub fn set_vault_token(
        app_handle: Option<&tauri::AppHandle>,
        key: &str,
        vault_path: &str,
        token: String,
    ) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        let map_key = format!(
            "{}_{}",
            key,
            vault_path.replace("/", "_").replace("\\\\", "_")
        );
        secrets.vault_tokens.insert(map_key, token);
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn delete_vault_token(
        app_handle: Option<&tauri::AppHandle>,
        key: &str,
        vault_path: &str,
    ) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        let map_key = format!(
            "{}_{}",
            key,
            vault_path.replace("/", "_").replace("\\\\", "_")
        );
        secrets.vault_tokens.remove(&map_key);
        Self::save_secrets(app_handle, &secrets)
    }

    // ──────────────────────────────────────────────
    // App Lock
    // ──────────────────────────────────────────────
    pub fn get_app_lock_hash(app_handle: Option<&tauri::AppHandle>) -> Option<String> {
        Self::load_secrets(app_handle).app_lock_hash
    }

    pub fn set_app_lock_hash(
        app_handle: Option<&tauri::AppHandle>,
        hash: String,
    ) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.app_lock_hash = Some(hash);
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn clear_app_lock(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.app_lock_hash = None;
        secrets.protected_apps = None;
        secrets.protected_notes = None;
        secrets.auto_lock_timeout_secs = None;
        secrets.app_lock_active = None;
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn get_app_lock_config(
        app_handle: Option<&tauri::AppHandle>,
    ) -> (Option<Vec<String>>, Option<Vec<String>>, Option<u64>, Option<bool>) {
        let secrets = Self::load_secrets(app_handle);
        (
            secrets.protected_apps,
            secrets.protected_notes,
            secrets.auto_lock_timeout_secs,
            secrets.app_lock_active,
        )
    }

    pub fn update_app_lock_config(
        app_handle: Option<&tauri::AppHandle>,
        protected_apps: Option<Vec<String>>,
        protected_notes: Option<Vec<String>>,
        timeout: Option<u64>,
        app_lock_active: Option<bool>,
    ) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        if let Some(apps) = protected_apps {
            secrets.protected_apps = Some(apps);
        }
        if let Some(notes) = protected_notes {
            secrets.protected_notes = Some(notes);
        }
        if let Some(t) = timeout {
            secrets.auto_lock_timeout_secs = Some(t);
        }
        if let Some(active) = app_lock_active {
            secrets.app_lock_active = Some(active);
        }
        Self::save_secrets(app_handle, &secrets)
    }
}
