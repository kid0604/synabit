use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

use crate::models::file::{FileItem, FileManagerSettings};
use crate::error::{AppError, AppResult};
use crate::path_utils;

fn get_file_meta(vault_path: &str) -> HashMap<String, Vec<String>> {
    let meta_path = Path::new(vault_path).join(".synabit_fm_meta.json");
    if let Ok(content) = fs::read_to_string(meta_path) {
        if let Ok(meta) = serde_json::from_str(&content) {
            return meta;
        }
    }
    HashMap::new()
}

fn save_file_meta(vault_path: &str, meta: &HashMap<String, Vec<String>>) -> AppResult<()> {
    let meta_path = Path::new(vault_path).join(".synabit_fm_meta.json");
    let content = serde_json::to_string(meta)?;
    fs::write(meta_path, content)?;
    Ok(())
}

#[tauri::command]
pub fn get_file_items(vault_path: String) -> AppResult<Vec<FileItem>> {
    let mut items = Vec::new();
    let file_meta = get_file_meta(&vault_path);
    let folders_to_scan = ["assets", "files"];
    
    for folder_name in folders_to_scan.iter() {
        let dir_path = Path::new(&vault_path).join(folder_name);
        if dir_path.exists() {
            for entry in WalkDir::new(&dir_path).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let size = entry.metadata().map(|m| m.len() as f64 / 1024.0 / 1024.0).unwrap_or(0.0);
                    let ext = entry.path().extension().unwrap_or_default().to_string_lossy().to_string();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let rel_path = path_utils::to_relative(entry.path(), &vault_path);
                    let date = "Unknown".to_string();
                    let tags = file_meta.get(&rel_path).cloned().unwrap_or_default();
                    
                    items.push(FileItem {
                        id: rel_path.clone(),
                        name: name.clone(),
                        extension: ext,
                        size_mb: size,
                        source_folder: folder_name.to_string(),
                        date_modified: date,
                        path: rel_path.clone(),
                        tags
                    });
                }
            }
        }
    }
    if let Ok(settings) = get_settings(vault_path.clone()) {
        for source in settings.tracked_sources {
            if Path::new(&source).exists() {
                for entry in WalkDir::new(&source).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_file() {
                        let size = entry.metadata().map(|m| m.len() as f64 / 1024.0 / 1024.0).unwrap_or(0.0);
                        let ext = entry.path().extension().unwrap_or_default().to_string_lossy().to_string();
                        let name = entry.file_name().to_string_lossy().to_string();
                        let rel_path = path_utils::to_relative(entry.path(), &vault_path);
                        let tags = file_meta.get(&rel_path).cloned().unwrap_or_default();
                        
                        items.push(FileItem {
                            id: rel_path.clone(),
                            name: name.clone(),
                            extension: ext,
                            size_mb: size,
                            source_folder: source.clone(),
                            date_modified: "Unknown".to_string(),
                            path: rel_path,
                            tags
                        });
                    }
                }
            }
        }
    }
    Ok(items)
}

#[tauri::command]
pub fn get_settings(vault_path: String) -> AppResult<FileManagerSettings> {
    let settings_path = Path::new(&vault_path).join(".synabit_fm_settings.json");
    if let Ok(content) = fs::read_to_string(settings_path) {
        if let Ok(settings) = serde_json::from_str(&content) {
            return Ok(settings);
        }
    }
    Ok(FileManagerSettings::default())
}

#[tauri::command]
pub fn save_settings(vault_path: String, settings: FileManagerSettings) -> AppResult<()> {
    let settings_path = Path::new(&vault_path).join(".synabit_fm_settings.json");
    let content = serde_json::to_string(&settings)?;
    fs::write(settings_path, content)?;
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub fn open_local_file(vault_path: String, path: String) -> AppResult<()> {
    let abs_path = Path::new(&vault_path).join(&path);
    let p = abs_path.to_string_lossy().to_string();
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&p).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(&p).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&p).spawn()?;
    }
    Ok(())
}

#[tauri::command]
pub fn update_file_metadata(vault_path: String, path: String, new_filename: String, new_tags: Vec<String>) -> AppResult<String> {
    let mut meta = get_file_meta(&vault_path);
    let original_path = Path::new(&vault_path).join(&path);
    
    let current_filename = original_path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut final_path_str = path.clone();
    
    if current_filename != new_filename {
        if let Some(parent) = original_path.parent() {
            if parent.ends_with("assets") {
                return Err(AppError::InvalidPath("Cannot rename files inside the 'assets' directory. You can only edit tags.".to_string()));
            } else {
                let new_path = parent.join(&new_filename);
                if new_path.exists() {
                     return Err(AppError::InvalidPath(format!("File '{}' already exists.", new_filename)));
                }
                match fs::rename(&original_path, &new_path) {
                    Ok(_) => {
                        final_path_str = path_utils::to_relative(&new_path, &vault_path);
                        meta.remove(&path);
                    },
                    Err(e) => return Err(AppError::Io(e))
                }
            }
        }
    }
    
    meta.insert(final_path_str.clone(), new_tags);
    save_file_meta(&vault_path, &meta)?;
    
    Ok(final_path_str)
}

#[tauri::command]
pub fn reindex_sources(_vault_path: String) -> AppResult<()> {
    // Basic placeholder
    Ok(())
}
