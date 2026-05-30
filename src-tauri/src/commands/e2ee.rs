use crate::secrets::SecretManager;
use tauri::Manager;

#[tauri::command]
pub async fn set_e2ee_password(app_handle: tauri::AppHandle, password: String) -> Result<(), String> {
    SecretManager::set_e2ee_password(Some(&app_handle), password)?;
    let db_state = app_handle.state::<crate::db::DbState>();
    if let Ok(db) = db_state.lock() {
        let _ = db.set_kv("force_e2ee_sync", "1");
    }
    Ok(())
}

#[tauri::command]
pub async fn clear_e2ee_password(app_handle: tauri::AppHandle) -> Result<(), String> {
    // DO NOT CLEAR SECRET YET. Set flag for Sync Engine to disable safely.
    let db_state = app_handle.state::<crate::db::DbState>();
    if let Ok(db) = db_state.lock() {
        let _ = db.set_kv("pending_e2ee_disable", "1");
    }
    Ok(())
}

#[tauri::command]
pub async fn change_e2ee_password(app_handle: tauri::AppHandle, old_password: String, new_password: String) -> Result<(), String> {
    // Verify old password
    if let Some(current) = SecretManager::get_e2ee_password(Some(&app_handle)) {
        if current != old_password {
            return Err("Old password is incorrect".to_string());
        }
    } else {
        return Err("E2EE is not enabled".to_string());
    }
    
    // Store new password in a temporary KV flag, Sync Engine will pick it up
    let db_state = app_handle.state::<crate::db::DbState>();
    if let Ok(db) = db_state.lock() {
        let _ = db.set_kv("pending_e2ee_reencrypt", &new_password);
    }
    Ok(())
}

#[tauri::command]
pub async fn is_e2ee_enabled(app_handle: tauri::AppHandle) -> Result<bool, String> {
    let db_state = app_handle.state::<crate::db::DbState>();
    if let Ok(db) = db_state.lock() {
        if db.get_kv("pending_e2ee_disable").unwrap_or_default().is_some() {
            // Technically it's in the process of being disabled, but for UI it's "enabled but turning off"
            // We can return true here, UI should just show it's syncing
        }
    }
    Ok(SecretManager::get_e2ee_password(Some(&app_handle)).is_some())
}
