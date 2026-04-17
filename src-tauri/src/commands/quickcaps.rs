use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::quickcap::QuickCapMetadata;

#[tauri::command]
pub fn scan_quick_caps(vault_path: String) -> Result<Vec<QuickCapMetadata>, String> {
    let mut caps = Vec::new();
    let qc_dir = Path::new(&vault_path).join("QuickCaps");
    
    if !qc_dir.exists() {
        return Ok(caps);
    }

    for entry in fs::read_dir(&qc_dir).map_err(|e| e.to_string())?.filter_map(|e| e.ok()) {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                    let date: chrono::DateTime<chrono::Local> = created.into();
                    
                    let path_str = entry.path().to_string_lossy().to_string();
                    let rel_path = entry.path().strip_prefix(&vault_path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path_str);
                    caps.push(QuickCapMetadata {
                        id: rel_path.clone(),
                        date: date.format("%Y-%m-%d %H:%M:%S").to_string(),
                        content,
                        path: rel_path,
                    });
                }
            }
        }
    }
    
    // Sort: newest quickcaps first
    caps.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(caps)
}

#[tauri::command]
pub fn create_quick_cap(vault_path: String, content: String) -> Result<QuickCapMetadata, String> {
    let qc_dir = Path::new(&vault_path).join("QuickCaps");
    if !qc_dir.exists() {
        fs::create_dir_all(&qc_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("System time error: {}", e))?.as_millis();
    let filename = format!("qc-{}.md", timestamp);
    let path = qc_dir.join(&filename);
    
    fs::write(&path, &content).map_err(|e| e.to_string())?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    
    let rel_path = path.strip_prefix(&vault_path).unwrap_or(&path).to_string_lossy().to_string();
    Ok(QuickCapMetadata {
        id: rel_path.clone(),
        date: date.format("%Y-%m-%d %H:%M:%S").to_string(),
        content,
        path: rel_path,
    })
}

#[tauri::command]
pub fn update_quick_cap(vault_path: String, path: String, content: String) -> Result<(), String> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::write(&abs_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_asset_to_vault(vault_path: String, source_path: String) -> Result<String, String> {
    let source = Path::new(&source_path);
    if !source.exists() {
        return Err("Source file does not exist".to_string());
    }
    
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System time error: {}", e))?
        .as_millis();
    let original_name = source.file_name().unwrap_or_default().to_string_lossy();
    let filename = format!("img-{}-{}", timestamp, original_name);
    let target = assets_dir.join(&filename);
    
    fs::copy(&source, &target).map_err(|e| e.to_string())?;
    
    Ok(format!("assets/{}", filename))
}
