use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct AppSecrets {
    pub e2ee_password: Option<String>,
    pub global_sync_config: Option<String>,
    pub vault_tokens: HashMap<String, String>,
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
}
