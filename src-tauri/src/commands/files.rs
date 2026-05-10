use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;
use uuid::Uuid;
use std::time::SystemTime;

use crate::models::file::{FileMetadata, FileSource, FileManagerSettings};
use crate::error::{AppError, AppResult};
use crate::db::DbState;
use crate::path_utils;

#[tauri::command]
pub fn add_file_source(app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String, path: String, name: String) -> AppResult<FileSource> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let id = Uuid::new_v4().to_string();
    let source = FileSource {
        id,
        path: path.clone(),
        name,
    };
    db.upsert_file_source(&source)?;
    
    // Auto trigger scan for the new source
    let _vault_path_clone = vault_path.clone();
    let path_clone = path.clone();
    drop(db); // Release lock before spawning thread
    std::thread::spawn(move || {
        use tauri::Manager;
        let db_state = app_handle.state::<DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = do_scan_directory(&db, &path_clone);
    });
    
    Ok(source)
}

#[tauri::command]
pub fn get_file_sources(app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<Vec<FileSource>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
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
            drop(db); // Release lock before spawning
            let _vault_path_clone = vault_path.clone();
            std::thread::spawn(move || {
                use tauri::Manager;
                let db_state = app_handle.state::<DbState>();
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                let _ = do_scan_directory(&db, &assets_path_str);
            });
        }
    }
    
    Ok(sources)
}

#[tauri::command]
pub fn remove_file_source(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String, source_id: String) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    // Ideally we should also remove the files associated with this source from DB.
    // For MVP, we just remove the source. The next full scan might clean up or we can just leave it.
    // A proper implementation would query files starting with the source path and delete them.
    db.delete_file_source(&source_id)?;
    Ok(())
}

/// Internal scan logic that takes a &DbBridge directly (no State needed)
fn do_scan_directory(db: &crate::db::DbBridge, source_path: &str) -> AppResult<()> {
    let path = Path::new(source_path);
    if !path.exists() || !path.is_dir() {
        return Err(AppError::InvalidPath("Source path is invalid or not a directory".to_string()));
    }

    for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let meta = entry.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta.as_ref().and_then(|m| m.modified().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
            let created = meta.as_ref().and_then(|m| m.created().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
            let created_dt: chrono::DateTime<chrono::Local> = created.into();
            let modified_dt: chrono::DateTime<chrono::Local> = modified.into();
            let mut ext = entry.path().extension().unwrap_or_default().to_string_lossy().to_string();
            if let Ok(Some(kind)) = infer::get_from_path(entry.path()) {
                ext = kind.extension().to_string();
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let abs_path = entry.path().to_string_lossy().to_string();
            if abs_path.contains("/.git/") || abs_path.contains("/node_modules/") || abs_path.contains("/.Trash") {
                continue;
            }
            let file_meta = FileMetadata {
                id: Uuid::new_v4().to_string(),
                path: abs_path, filename: name, extension: ext,
                size: size as i64,
                created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                modified_at: modified_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                tags: vec![], source_type: "local".to_string(),
            };
            let _ = db.upsert_file(&file_meta);
            let props = format!("ext:{} source:local", file_meta.extension);
            db.upsert_search_entry(
                &file_meta.id, "file", &file_meta.filename,
                &file_meta.tags.join(" "), &file_meta.extension,
                &props, None, &file_meta.modified_at, &file_meta.path,
            );
        }
    }
    Ok(())
}

#[tauri::command]
pub fn scan_directory(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String, source_path: String) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    do_scan_directory(&db, &source_path)
}

#[tauri::command]
pub fn query_files(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<Vec<FileMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
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
pub fn open_local_file(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String, path: String) -> AppResult<()> {
    let p = std::path::Path::new(&path);
    if !p.exists() || !p.is_file() {
        return Err(AppError::InvalidPath("File not found or is a directory".to_string()));
    }
    
    // Check if the file is within allowed roots
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut allowed_roots = vec![vault_path.clone()];
    if let Ok(sources) = db.get_all_file_sources() {
        for source in sources {
            allowed_roots.push(source.path);
        }
    }
    
    let root_refs: Vec<&str> = allowed_roots.iter().map(|s| s.as_str()).collect();
    path_utils::enforce_within_roots(p, &root_refs)?;

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(&path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&path).spawn()?;
    }
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn open_local_file(_app_handle: tauri::AppHandle, _vault_path: String, _path: String) -> AppResult<()> {
    // Opening arbitrary local files is restricted/different on mobile
    Err(AppError::General("Opening local files is not supported on mobile".to_string()))
}

#[tauri::command]
pub fn update_file_metadata(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String, path: String, new_filename: String, new_tags: Vec<String>) -> AppResult<String> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    
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

    // Sync search index after any changes
    if let Ok(files) = db.get_all_files() {
        if let Some(f) = files.iter().find(|f| f.path == final_path) {
            let props = format!("ext:{} source:{}", f.extension, f.source_type);
            db.upsert_search_entry(
                &f.id, "file", &f.filename, &f.tags.join(" "),
                &f.extension, &props, None, &f.modified_at, &f.path,
            );
        }
    }

    Ok(final_path)
}

#[tauri::command]
pub fn reindex_sources(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    
    let assets_dir = std::path::Path::new(&vault_path).join("assets");
    let mut scan_paths: Vec<String> = Vec::new();
    if assets_dir.exists() {
        scan_paths.push(assets_dir.to_string_lossy().to_string());
    }
    if let Ok(sources) = db.get_all_file_sources() {
        for source in sources {
            scan_paths.push(source.path);
        }
    }

    for source_path in scan_paths {
        let path = Path::new(&source_path);
        if !path.exists() || !path.is_dir() { continue; }
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let abs_path = entry.path().to_string_lossy().to_string();
                if abs_path.contains("/.git/") || abs_path.contains("/node_modules/") || abs_path.contains("/.Trash") {
                    continue;
                }
                let meta = entry.metadata().ok();
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = meta.as_ref().and_then(|m| m.modified().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
                let created = meta.as_ref().and_then(|m| m.created().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
                let created_dt: chrono::DateTime<chrono::Local> = created.into();
                let modified_dt: chrono::DateTime<chrono::Local> = modified.into();
                let mut ext = entry.path().extension().unwrap_or_default().to_string_lossy().to_string();
                if let Ok(Some(kind)) = infer::get_from_path(entry.path()) {
                    ext = kind.extension().to_string();
                }
                let file_meta = FileMetadata {
                    id: Uuid::new_v4().to_string(),
                    path: abs_path,
                    filename: entry.file_name().to_string_lossy().to_string(),
                    extension: ext, size: size as i64,
                    created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                    modified_at: modified_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                    tags: vec![], source_type: "local".to_string(),
                };
                let _ = db.upsert_file(&file_meta);
                let props = format!("ext:{} source:local", file_meta.extension);
                db.upsert_search_entry(
                    &file_meta.id, "file", &file_meta.filename,
                    &file_meta.tags.join(" "), &file_meta.extension,
                    &props, None, &file_meta.modified_at, &file_meta.path,
                );
            }
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn read_local_file_content(state: tauri::State<'_, DbState>, vault_path: String, path: String) -> AppResult<String> {
    let p = std::path::Path::new(&path);
    if !p.exists() || !p.is_file() {
        return Err(AppError::InvalidPath("File not found or is a directory".to_string()));
    }

    // Validate path is within vault or registered file sources
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut allowed_roots = vec![vault_path.clone()];
    if let Ok(sources) = db.get_all_file_sources() {
        for source in sources {
            allowed_roots.push(source.path);
        }
    }
    drop(db);

    let root_refs: Vec<&str> = allowed_roots.iter().map(|s| s.as_str()).collect();
    path_utils::enforce_within_roots(p, &root_refs)?;
    
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
