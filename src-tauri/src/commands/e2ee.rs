use keyring::Entry;

#[tauri::command]
pub async fn set_e2ee_password(password: String) -> Result<(), String> {
    let entry = Entry::new("synabit_e2ee", "master_password")
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(&password)
        .map_err(|e| format!("Failed to save password: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn clear_e2ee_password() -> Result<(), String> {
    let entry = Entry::new("synabit_e2ee", "master_password")
        .map_err(|e| format!("Keyring error: {}", e))?;
    let _ = entry.delete_credential();
    Ok(())
}

#[tauri::command]
pub async fn is_e2ee_enabled() -> Result<bool, String> {
    let entry = Entry::new("synabit_e2ee", "master_password")
        .map_err(|e| format!("Keyring error: {}", e))?;
    Ok(entry.get_password().is_ok())
}
