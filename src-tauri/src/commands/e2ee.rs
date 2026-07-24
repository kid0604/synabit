use crate::secrets::SecretManager;
use serde::Serialize;

#[derive(Serialize)]
pub struct E2eeStatus {
    /// Whether an encryption key is available
    pub key_available: bool,
    /// Whether the user needs to set up E2EE (first time or new device)
    pub needs_setup: bool,
}

#[derive(Serialize)]
pub struct SetupResult {
    pub recovery_phrase: String,
}

/// Check E2EE status — determines what UI to show
#[tauri::command]
pub async fn check_e2ee_status(app_handle: tauri::AppHandle) -> Result<E2eeStatus, String> {
    let has_key = SecretManager::has_e2ee_key(Some(&app_handle));
    
    Ok(E2eeStatus {
        key_available: has_key,
        needs_setup: !has_key,
    })
}

/// First-time setup: generate key, return recovery phrase
#[tauri::command]
pub async fn setup_e2ee(app_handle: tauri::AppHandle) -> Result<SetupResult, String> {
    use tauri::Manager;
    // Don't overwrite existing key
    if SecretManager::has_e2ee_key(Some(&app_handle)) {
        return Err("E2EE key already exists. Use get_recovery_phrase instead.".to_string());
    }

    let key = crate::sync::core::crypto::generate_key();
    let phrase = crate::sync::core::crypto::key_to_mnemonic(&key)?;
    
    SecretManager::set_e2ee_key(Some(&app_handle), &key)?;
    
    // Clear any legacy password that might exist
    let _ = SecretManager::clear_e2ee_password(Some(&app_handle));
    
    // Set flag to encrypt all existing data on next sync
    let db_state = app_handle.state::<crate::db::DbState>();
    if let Ok(db) = db_state.lock() {
        let _ = db.set_kv("e2ee_key_version", "3");
        let _ = db.set_kv("force_e2ee_sync", "1");
        let _ = db.delete_kv("pending_key_migration");
    }
    
    Ok(SetupResult { recovery_phrase: phrase })
}

/// Restore key from recovery phrase (new device)
#[tauri::command]
pub async fn restore_e2ee_from_phrase(
    app_handle: tauri::AppHandle,
    phrase: String,
) -> Result<(), String> {
    use tauri::Manager;
    let key = crate::sync::core::crypto::mnemonic_to_key(&phrase)?;
    
    SecretManager::set_e2ee_key(Some(&app_handle), &key)?;
    
    // Clear any legacy password
    let _ = SecretManager::clear_e2ee_password(Some(&app_handle));
    
    let db_state = app_handle.state::<crate::db::DbState>();
    if let Ok(db) = db_state.lock() {
        let _ = db.set_kv("e2ee_key_version", "3");
        let _ = db.delete_kv("pending_key_migration");
    }
    
    Ok(())
}

/// Get recovery phrase for display (key must already exist)
#[tauri::command]
pub async fn get_recovery_phrase(app_handle: tauri::AppHandle) -> Result<String, String> {
    let key = SecretManager::get_e2ee_key(Some(&app_handle))
        .ok_or("No encryption key found")?;
    crate::sync::core::crypto::key_to_mnemonic(&key)
}
