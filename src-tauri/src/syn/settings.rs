//! Settings persistence for Syn (Local AI Chat).
//!
//! Loads and saves user-configurable settings from `{vault}/Syn/settings.json`.
//! Falls back to sensible defaults if the file doesn't exist or is corrupted.

use crate::error::{AppError, AppResult};
use crate::models::syn::SynSettings;
use std::path::Path;

/// Load settings from `{vault}/Syn/settings.json`.
/// Returns defaults if the file doesn't exist or contains invalid JSON.
pub fn load_settings(vault_path: &str) -> AppResult<SynSettings> {
    let settings_path = Path::new(vault_path).join("Syn").join("settings.json");
    if !settings_path.exists() {
        return Ok(SynSettings::default());
    }
    let content = std::fs::read_to_string(&settings_path)?;
    let settings: SynSettings = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[Syn] Settings file contains invalid JSON, using defaults: {}", e);
            SynSettings::default()
        }
    };
    Ok(settings)
}

/// Save settings to `{vault}/Syn/settings.json`.
/// Creates the `Syn/` directory if it doesn't exist.
pub fn save_settings(vault_path: &str, settings: &SynSettings) -> AppResult<()> {
    let syn_dir = Path::new(vault_path).join("Syn");
    std::fs::create_dir_all(&syn_dir).map_err(|e| {
        AppError::General(format!("Failed to create Syn directory: {}", e))
    })?;
    let settings_path = syn_dir.join("settings.json");
    let json = serde_json::to_string_pretty(settings)?;
    let tmp_path = settings_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &settings_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        AppError::General(format!("Failed to rename temp settings file: {}", e))
    })?;
    Ok(())
}
