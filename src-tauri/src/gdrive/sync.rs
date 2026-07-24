use std::sync::Arc;

// Provide alias to keep frontend types compatible
pub type SyncResult = crate::sync::core::types::SyncResult;

#[tauri::command]
pub async fn gdrive_sync_full(
    app_handle: tauri::AppHandle,
    vault_path: String,
) -> Result<SyncResult, String> {
    log::info!("Starting Google Drive adapter sync for vault: {}", vault_path);
    
    // Check auth
    if crate::gdrive::auth::get_valid_token(&app_handle).await.is_err() {
        use tauri::Emitter;
        let _ = app_handle.emit("gdrive-auth-required", ());
        return Err("Google Drive authentication required".to_string());
    }

    let e2ee_key: [u8; 32] = crate::secrets::SecretManager::get_e2ee_key(Some(&app_handle))
        .ok_or_else(|| {
            use tauri::Emitter;
            let _ = app_handle.emit("e2ee-setup-required", ());
            "E2EE key not set up. Please set up encryption first.".to_string()
        })?;

    let device_id = app_handle.config().identifier.clone();

    let mut coordinator = crate::sync::coordinator::SyncCoordinator::new();
    let adapter = Arc::new(crate::sync::adapter::gdrive::GoogleDriveAdapter::new(app_handle.clone()));
    
    coordinator.set_adapter(adapter).await.map_err(|e| e.to_string())?;
    
    let ctx = crate::sync::core::types::SyncRunContext {
        run_id: "gdrive_sync".to_string(),
        trigger_reason: "manual".to_string(),
        vault_tag: "gdrive".to_string(),
    };
    
    let result = coordinator.sync(
        &vault_path,
        &device_id,
        &e2ee_key,
        &ctx,
        &app_handle
    ).await.map_err(|e| e.to_string())?;

    Ok(result)
}

#[tauri::command]
pub fn migrate_gdrive_vault(
    _app_handle: tauri::AppHandle,
    _vault_path: String,
) -> Result<(), String> {
    // Migration logic deprecated as we use SyncCoordinator Event Log format now.
    // Old files will remain in Drive but ignored by the new `.sync_log` logic.
    Ok(())
}
