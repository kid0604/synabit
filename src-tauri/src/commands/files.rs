use std::path::Path;
use std::process::Command;
use std::collections::HashMap;
use walkdir::WalkDir;
use uuid::Uuid;
use std::time::SystemTime;

use crate::models::file::{FileMetadata, FileSource, FileManagerSettings, DuplicateGroup, DuplicateReport};
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

/// Internal scan logic — creates file nodes in the nodes table
fn do_scan_directory(db: &crate::db::DbBridge, source_path: &str) -> AppResult<()> {
    let path = Path::new(source_path);
    if !path.exists() || !path.is_dir() {
        return Err(AppError::InvalidPath("Source path is invalid or not a directory".to_string()));
    }

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
            let name = entry.file_name().to_string_lossy().to_string();

            // Check if node already exists for this path (preserve tags)
            let existing_tags = find_existing_file_tags(db, &abs_path);

            let file_meta = FileMetadata {
                id: Uuid::new_v4().to_string(),
                path: abs_path, filename: name, extension: ext,
                size: size as i64,
                created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                modified_at: modified_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                tags: existing_tags, source_type: "local".to_string(),
            };

            let node = file_meta.to_node();
            let _ = upsert_file_node(db, &node, &file_meta.path);

            // Update search index
            let props = format!("ext:{} source:local", file_meta.extension);
            db.upsert_search_entry(
                &node.id, "file", &file_meta.filename,
                &file_meta.tags.join(" "), &file_meta.extension,
                &props, None, &file_meta.modified_at, &file_meta.path,
            );
        }
    }
    Ok(())
}

/// Upsert a file node — uses path as conflict key in properties
fn upsert_file_node(db: &crate::db::DbBridge, node: &crate::models::node::NodeMetadata, path: &str) -> AppResult<()> {
    // Check if a node with this path already exists
    if let Ok(nodes) = db.get_nodes_by_type("file") {
        for existing in &nodes {
            if let Some(p) = existing.properties.get("path").and_then(|v| v.as_str()) {
                if p == path {
                    // Update existing node (preserve ID, update metadata)
                    let mut updated = node.clone();
                    updated.id = existing.id.clone();
                    // Preserve existing tags if new node has empty tags
                    if let (Some(new_tags), Some(old_tags)) = (
                        updated.properties.get("tags").and_then(|v| v.as_array()),
                        existing.properties.get("tags").and_then(|v| v.as_array()),
                    ) {
                        if new_tags.is_empty() && !old_tags.is_empty() {
                            if let Some(props) = updated.properties.as_object_mut() {
                                props.insert("tags".to_string(), serde_json::Value::Array(old_tags.clone()));
                            }
                        }
                    }
                    return db.upsert_node(&updated);
                }
            }
        }
    }
    db.upsert_node(node)
}

/// Find existing tags for a file path from nodes table
fn find_existing_file_tags(db: &crate::db::DbBridge, path: &str) -> Vec<String> {
    if let Ok(nodes) = db.get_nodes_by_type("file") {
        for node in &nodes {
            if let Some(p) = node.properties.get("path").and_then(|v| v.as_str()) {
                if p == path {
                    return node.properties.get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                }
            }
        }
    }
    vec![]
}

#[tauri::command]
pub fn scan_directory(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String, source_path: String) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    do_scan_directory(&db, &source_path)
}

#[tauri::command]
pub fn query_files(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<Vec<FileMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = db.get_nodes_by_type("file")?;
    Ok(nodes.iter().filter_map(FileMetadata::from_node).collect())
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
    
    // Find the file node by path
    let nodes = db.get_nodes_by_type("file")?;
    let file_node = nodes.iter().find(|n| {
        n.properties.get("path").and_then(|v| v.as_str()) == Some(&path)
    }).ok_or_else(|| AppError::General("File node not found".to_string()))?;

    let mut updated_node = file_node.clone();
    let mut final_path = path.clone();

    // 1. Update tags in properties
    if let Some(props) = updated_node.properties.as_object_mut() {
        props.insert("tags".to_string(), serde_json::json!(new_tags));
    }

    // 2. Handle rename if needed (skip for assets folder)
    let assets_dir = std::path::Path::new(&vault_path).join("assets").to_string_lossy().to_string();
    if !path.starts_with(&assets_dir) {
        path_utils::enforce_no_traversal(&path)?;
        let path_obj = std::path::Path::new(&path);
        if let Some(parent) = path_obj.parent() {
            if let Some(old_name) = path_obj.file_name() {
                let old_name_str = old_name.to_string_lossy().to_string();
                if old_name_str != new_filename {
                    if !path_utils::is_safe_filename(&new_filename) {
                        return Err(AppError::InvalidPath("Invalid filename".to_string()));
                    }
                    let new_path = parent.join(&new_filename);
                    std::fs::rename(&path, &new_path)
                        .map_err(|e| AppError::General(format!("Failed to rename file on disk: {}", e)))?;
                    
                    final_path = new_path.to_string_lossy().to_string();
                    let mut extension = new_path.extension().unwrap_or_default().to_string_lossy().to_string();
                    if let Ok(Some(kind)) = infer::get_from_path(&new_path) {
                        extension = kind.extension().to_string();
                    }

                    // Update node properties
                    updated_node.title = new_filename;
                    if let Some(props) = updated_node.properties.as_object_mut() {
                        props.insert("path".to_string(), serde_json::json!(final_path));
                        props.insert("extension".to_string(), serde_json::json!(extension));
                    }
                }
            }
        }
    }

    db.upsert_node(&updated_node)?;

    // Sync search index
    if let Some(f) = FileMetadata::from_node(&updated_node) {
        let props = format!("ext:{} source:{}", f.extension, f.source_type);
        db.upsert_search_entry(
            &f.id, "file", &f.filename, &f.tags.join(" "),
            &f.extension, &props, None, &f.modified_at, &f.path,
        );
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
        let _ = do_scan_directory(&db, &source_path);
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

#[tauri::command]
pub fn delete_file(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, file_id: String, file_path: String) -> AppResult<()> {
    // 1. Delete from disk
    let path = std::path::Path::new(&file_path);
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| AppError::General(format!("Failed to delete file from disk: {}", e)))?;
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // 2. Delete from nodes table
    db.delete_node(&file_id)?;

    // 3. Delete from search index
    db.delete_search_entry(&file_id);

    Ok(())
}

#[derive(serde::Serialize)]
pub struct FileReference {
    pub node_id: String,
    pub node_type: String,
    pub title: String,
}

#[tauri::command]
pub fn get_file_references(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String, filename: String) -> AppResult<Vec<FileReference>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let refs = db.find_nodes_referencing_file(&filename)?;
    Ok(refs.into_iter().map(|(node_id, node_type, title)| FileReference { node_id, node_type, title }).collect())
}

#[tauri::command]
pub fn find_duplicate_files(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<DuplicateReport> {
    use std::io::Read;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = db.get_nodes_by_type("file")?;
    let files: Vec<FileMetadata> = nodes.iter().filter_map(FileMetadata::from_node).collect();

    // Step 1: Group by size (files with unique sizes can't be duplicates)
    let mut size_groups: HashMap<i64, Vec<FileMetadata>> = HashMap::new();
    for file in files {
        if file.size > 0 {
            size_groups.entry(file.size).or_default().push(file);
        }
    }

    // Step 2: For groups with 2+ files of same size, compute BLAKE3 hash (first 64KB)
    let mut hash_groups: HashMap<String, Vec<FileMetadata>> = HashMap::new();
    for (_size, group) in size_groups.into_iter().filter(|(_, g)| g.len() > 1) {
        for file in group {
            let path = std::path::Path::new(&file.path);
            if !path.exists() { continue; }
            
            let mut hasher = blake3::Hasher::new();
            if let Ok(mut f) = std::fs::File::open(path) {
                let mut buf = [0u8; 65536];
                if let Ok(n) = f.read(&mut buf) {
                    hasher.update(&buf[..n]);
                }
            } else {
                continue;
            }
            let hash = hasher.finalize().to_hex().to_string();
            hash_groups.entry(hash).or_default().push(file);
        }
    }

    // Step 3: Build duplicate groups from hash matches
    let mut duplicate_groups: Vec<DuplicateGroup> = hash_groups
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(_, files)| {
            let count = files.len();
            let filename = files[0].filename.clone();
            let extension = files[0].extension.clone();
            let size = files[0].size;
            let wasted = size * (count as i64 - 1);
            DuplicateGroup {
                filename, extension, size, count,
                files,
                wasted_bytes: wasted,
            }
        })
        .collect();

    // Sort by wasted space descending
    duplicate_groups.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));

    let total_groups = duplicate_groups.len();
    let total_duplicate_files: usize = duplicate_groups.iter().map(|g| g.count - 1).sum();
    let total_wasted_bytes: i64 = duplicate_groups.iter().map(|g| g.wasted_bytes).sum();

    Ok(DuplicateReport {
        groups: duplicate_groups,
        total_groups,
        total_duplicate_files,
        total_wasted_bytes,
    })
}
