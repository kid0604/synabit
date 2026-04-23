use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;
use uuid::Uuid;
use std::time::SystemTime;

use crate::models::file::{FileMetadata, FileSource, FileManagerSettings};
use crate::error::{AppError, AppResult};
use crate::db::DbBridge;
use crate::path_utils;

#[tauri::command]
pub fn add_file_source(vault_path: String, path: String, name: String) -> AppResult<FileSource> {
    let db = DbBridge::new(&vault_path)?;
    let id = Uuid::new_v4().to_string();
    let source = FileSource {
        id,
        path: path.clone(),
        name,
    };
    db.upsert_file_source(&source)?;
    
    // Auto trigger scan for the new source
    let vault_path_clone = vault_path.clone();
    let path_clone = path.clone();
    std::thread::spawn(move || {
        let _ = scan_directory(vault_path_clone, path_clone);
    });
    
    Ok(source)
}

#[tauri::command]
pub fn get_file_sources(vault_path: String) -> AppResult<Vec<FileSource>> {
    let db = DbBridge::new(&vault_path)?;
    let mut sources = db.get_all_file_sources()?;
    
    // Auto-add assets folder if it doesn't exist
    let assets_path = std::path::Path::new(&vault_path).join("assets");
    if assets_path.exists() {
        let assets_path_str = assets_path.to_string_lossy().to_string();
        if !sources.iter().any(|s| s.path == assets_path_str) {
            let source = FileSource {
                id: Uuid::new_v4().to_string(),
                path: assets_path_str.clone(),
                name: "Vault Assets".to_string(),
            };
            db.upsert_file_source(&source)?;
            sources.push(source);
            
            // Auto trigger scan for the new source
            let vault_path_clone = vault_path.clone();
            std::thread::spawn(move || {
                let _ = scan_directory(vault_path_clone, assets_path_str);
            });
        }
    }
    
    Ok(sources)
}

#[tauri::command]
pub fn remove_file_source(vault_path: String, source_id: String) -> AppResult<()> {
    let db = DbBridge::new(&vault_path)?;
    // Ideally we should also remove the files associated with this source from DB.
    // For MVP, we just remove the source. The next full scan might clean up or we can just leave it.
    // A proper implementation would query files starting with the source path and delete them.
    db.delete_file_source(&source_id)?;
    Ok(())
}

#[tauri::command]
pub fn scan_directory(vault_path: String, source_path: String) -> AppResult<()> {
    let db = DbBridge::new(&vault_path)?;
    let path = Path::new(&source_path);
    
    if !path.exists() || !path.is_dir() {
        return Err(AppError::InvalidPath("Source path is invalid or not a directory".to_string()));
    }

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let meta = entry.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            
            let modified = meta.as_ref()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
                
            let created = meta.as_ref()
                .and_then(|m| m.created().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);

            let created_dt: chrono::DateTime<chrono::Local> = created.into();
            let modified_dt: chrono::DateTime<chrono::Local> = modified.into();

            let mut ext = entry.path().extension().unwrap_or_default().to_string_lossy().to_string();
            if let Ok(Some(kind)) = infer::get_from_path(entry.path()) {
                ext = kind.extension().to_string();
            }
            
            let name = entry.file_name().to_string_lossy().to_string();
            let abs_path = entry.path().to_string_lossy().to_string();
            
            // Basic filtering to avoid indexing git/node_modules/etc
            if abs_path.contains("/.git/") || abs_path.contains("/node_modules/") || abs_path.contains("/.Trash") {
                continue;
            }

            let file_meta = FileMetadata {
                id: Uuid::new_v4().to_string(), // Or path hash
                path: abs_path,
                filename: name,
                extension: ext,
                size: size as i64,
                created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                modified_at: modified_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                tags: vec![],
                source_type: "local".to_string(),
            };
            
            let _ = db.upsert_file(&file_meta);
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn query_files(vault_path: String) -> AppResult<Vec<FileMetadata>> {
    let db = DbBridge::new(&vault_path)?;
    db.get_all_files()
}

// ─── Legacy/Compatibility endpoints (to not break compilation if used elsewhere) ───

#[tauri::command]
pub fn get_file_items(_vault_path: String) -> AppResult<Vec<crate::models::file::FileItem>> {
    // For now return empty or dummy to avoid compilation errors if something still expects this
    Ok(vec![])
}

#[tauri::command]
pub fn get_settings(_vault_path: String) -> AppResult<FileManagerSettings> {
    Ok(FileManagerSettings::default())
}

#[tauri::command]
pub fn save_settings(_vault_path: String, _settings: FileManagerSettings) -> AppResult<()> {
    Ok(())
}

#[cfg(desktop)]
#[tauri::command]
pub fn open_local_file(_vault_path: String, path: String) -> AppResult<()> {
    path_utils::enforce_no_traversal(&path)?;
    // Note: path is now an absolute path in the new OmniDrive architecture
    let p = path.clone();
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
    let db = DbBridge::new(&vault_path)?;
    
    // 1. Update tags
    db.update_file_tags(&path, new_tags)?;
    
    // 2. Handle rename if needed
    let mut final_path = path.clone();
    
    // Do not rename if it's inside assets folder
    let assets_dir = std::path::Path::new(&vault_path).join("assets").to_string_lossy().to_string();
    if path.starts_with(&assets_dir) {
        return Ok(path);
    }
    
    path_utils::enforce_no_traversal(&path)?;

    let path_obj = std::path::Path::new(&path);
    if let Some(parent) = path_obj.parent() {
        if let Some(old_name) = path_obj.file_name() {
            let old_name_str = old_name.to_string_lossy().to_string();
            if old_name_str != new_filename {
                if !path_utils::is_safe_filename(&new_filename) {
                    return Err(crate::error::AppError::InvalidPath("Invalid filename".to_string()));
                }
                let new_path = parent.join(&new_filename);
                // rename on disk
                if let Err(e) = std::fs::rename(&path, &new_path) {
                    return Err(crate::error::AppError::General(format!("Failed to rename file on disk: {}", e)));
                }
                
                final_path = new_path.to_string_lossy().to_string();
                
                // Extract extension
                let mut extension = new_path.extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                
                if let Ok(Some(kind)) = infer::get_from_path(&new_path) {
                    extension = kind.extension().to_string();
                }
                
                // update db
                db.update_file_path_and_name(&path, &final_path, &new_filename, &extension)?;
            }
        }
    }

    Ok(final_path)
}

#[tauri::command]
pub fn reindex_sources(vault_path: String) -> AppResult<()> {
    let db = DbBridge::new(&vault_path)?;
    
    // Auto scan assets
    let assets_dir = std::path::Path::new(&vault_path).join("assets");
    if assets_dir.exists() {
        let _ = scan_directory(vault_path.clone(), assets_dir.to_string_lossy().to_string());
    }

    // Scan all custom sources
    if let Ok(sources) = db.get_all_file_sources() {
        for source in sources {
            let _ = scan_directory(vault_path.clone(), source.path);
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn read_local_file_content(path: String) -> AppResult<String> {
    path_utils::enforce_no_traversal(&path)?;
    let p = std::path::Path::new(&path);
    if !p.exists() || !p.is_file() {
        return Err(AppError::InvalidPath("File not found or is a directory".to_string()));
    }
    
    // Check size limit (e.g. 5MB)
    if let Ok(meta) = p.metadata() {
        if meta.len() > 5 * 1024 * 1024 {
            return Err(AppError::General("File is too large to preview (max 5MB)".to_string()));
        }
    }
    
    let content = std::fs::read_to_string(p)
        .map_err(|e| AppError::General(format!("Failed to read file: {}", e)))?;
        
    Ok(content)
}
