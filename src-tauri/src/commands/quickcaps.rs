use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::quickcap::QuickCapMetadata;
use crate::error::{AppError, AppResult};
use crate::path_utils;
use crate::db::DbBridge;

#[tauri::command]
pub fn scan_quick_caps(vault_path: String) -> AppResult<Vec<QuickCapMetadata>> {
    let mut caps = Vec::new();
    let qc_dir = Path::new(&vault_path).join("QuickCaps");
    
    if !qc_dir.exists() {
        return Ok(caps);
    }

    let db = DbBridge::new(&vault_path).ok();
    let mut current_disk_files = std::collections::HashSet::new();

    for entry in fs::read_dir(&qc_dir)?.filter_map(|e| e.ok()) {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                    let date: chrono::DateTime<chrono::Local> = created.into();
                    
                    let rel_path = path_utils::to_relative(&entry.path(), &vault_path);
                    current_disk_files.insert(rel_path.clone());
                    let qc_meta = QuickCapMetadata {
                        id: rel_path.clone(),
                        date: date.format("%Y-%m-%d %H:%M:%S").to_string(),
                        content,
                        path: rel_path,
                    };
                    
                    if let Some(db_bridge) = &db {
                        let _ = db_bridge.upsert_quickcap(&qc_meta);
                    }
                    caps.push(qc_meta);
                }
            }
        }
    }
    
    if let Some(db_bridge) = &db {
        if let Ok(existing) = db_bridge.get_all_quickcap_timestamps() {
            for id in existing.keys() {
                if !current_disk_files.contains(id) {
                    let _ = db_bridge.delete_quickcap(id);
                }
            }
        }
    }
    
    // Sort: newest quickcaps first
    caps.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(caps)
}

#[tauri::command]
pub fn create_quick_cap(vault_path: String, content: String) -> AppResult<QuickCapMetadata> {
    let qc_dir = Path::new(&vault_path).join("QuickCaps");
    if !qc_dir.exists() {
        fs::create_dir_all(&qc_dir)?;
    }
    
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| AppError::General(format!("System time error: {}", e)))?.as_millis();
    let filename = format!("qc-{}.md", timestamp);
    let path = qc_dir.join(&filename);
    
    fs::write(&path, &content)?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    
    let rel_path = path_utils::to_relative(&path, &vault_path);
    let qc_meta = QuickCapMetadata {
        id: rel_path.clone(),
        date: date.format("%Y-%m-%d %H:%M:%S").to_string(),
        content,
        path: rel_path,
    };
    
    if let Ok(db) = DbBridge::new(&vault_path) {
        let _ = db.upsert_quickcap(&qc_meta);
    }
    Ok(qc_meta)
}

#[tauri::command]
pub fn delete_quick_cap(vault_path: String, path: String) -> AppResult<()> {
    path_utils::enforce_no_traversal(&path)?;
    let abs_path = Path::new(&vault_path).join(&path);
    fs::remove_file(&abs_path)?;
    if let Ok(db) = DbBridge::new(&vault_path) {
        let _ = db.delete_quickcap(&path);
    }
    Ok(())
}

#[tauri::command]
pub fn update_quick_cap(vault_path: String, path: String, content: String) -> AppResult<()> {
    path_utils::enforce_no_traversal(&path)?;
    let abs_path = Path::new(&vault_path).join(&path);
    fs::write(&abs_path, content.clone())?;
    
    if let Ok(db) = DbBridge::new(&vault_path) {
        if let Ok(file_meta) = fs::metadata(&abs_path) {
            let created = file_meta.created().unwrap_or(file_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            let created_date: chrono::DateTime<chrono::Local> = created.into();
            let qc_meta = QuickCapMetadata {
                id: path.clone(),
                date: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                content,
                path: path.clone(),
            };
            let _ = db.upsert_quickcap(&qc_meta);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn copy_asset_to_vault(vault_path: String, source_path: String) -> AppResult<String> {
    let source = Path::new(&source_path);
    if !source.exists() {
        return Err(AppError::InvalidPath("Source file does not exist".to_string()));
    }
    
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        fs::create_dir_all(&assets_dir)?;
    }
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::General(format!("System time error: {}", e)))?
        .as_millis();
    let original_name = source.file_name().unwrap_or_default().to_string_lossy();
    let filename = format!("img-{}-{}", timestamp, original_name);
    let target = assets_dir.join(&filename);
    
    fs::copy(&source, &target)?;
    
    Ok(format!("assets/{}", filename))
}
