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

/// Import individual files: copy them into vault/assets/imported/ and index as file nodes
#[tauri::command]
pub fn import_files(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    file_paths: Vec<String>,
) -> AppResult<u32> {
    let import_dir = Path::new(&vault_path).join("assets");
    if !import_dir.exists() {
        std::fs::create_dir_all(&import_dir)?;
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut count: u32 = 0;

    for src_path in &file_paths {
        let source = Path::new(src_path);
        if !source.exists() || !source.is_file() { continue; }

        let original_name = source.file_name().unwrap_or_default().to_string_lossy().to_string();
        let extension = source.extension().unwrap_or_default().to_string_lossy().to_string();

        // Create a unique filename to avoid collisions, but keep the original name readable
        let stem = source.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let dest_name = if import_dir.join(&original_name).exists() {
            format!("{}_{}.{}", stem, Uuid::new_v4().simple(), extension)
        } else {
            original_name.clone()
        };
        let dest = import_dir.join(&dest_name);

        if std::fs::copy(source, &dest).is_err() { continue; }

        let abs_path = dest.to_string_lossy().to_string();
        let meta = std::fs::metadata(&dest).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta.as_ref().and_then(|m| m.modified().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
        let created = meta.as_ref().and_then(|m| m.created().ok()).unwrap_or(SystemTime::UNIX_EPOCH);
        let created_dt: chrono::DateTime<chrono::Local> = created.into();
        let modified_dt: chrono::DateTime<chrono::Local> = modified.into();

        let mut ext = extension.clone();
        if let Ok(Some(kind)) = infer::get_from_path(&dest) {
            ext = kind.extension().to_string();
        }

        let file_meta = FileMetadata {
            id: Uuid::new_v4().to_string(),
            path: abs_path.clone(),
            filename: dest_name,
            extension: ext.clone(),
            size: size as i64,
            created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            modified_at: modified_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            tags: vec![],
            source_type: "imported".to_string(),
        };

        let node = file_meta.to_node();
        let _ = upsert_file_node(&db, &node, &abs_path);

        // Update search index
        let props = format!("ext:{} source:imported", ext);
        db.upsert_search_entry(
            &node.id, "file", &file_meta.filename,
            "", &file_meta.extension,
            &props, None, &file_meta.modified_at, &abs_path,
        );

        count += 1;
    }

    Ok(count)
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

/// Find existing tags for a file path from nodes table, with fallback to legacy files table
fn find_existing_file_tags(db: &crate::db::DbBridge, path: &str) -> Vec<String> {
    // First check the nodes table (current architecture)
    if let Ok(nodes) = db.get_nodes_by_type("file") {
        for node in &nodes {
            if let Some(p) = node.properties.get("path").and_then(|v| v.as_str()) {
                if p == path {
                    let tags = node.properties.get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect::<Vec<_>>())
                        .unwrap_or_default();
                    if !tags.is_empty() {
                        return tags;
                    }
                }
            }
        }
    }
    // Fallback: check legacy files table for tags
    if let Some(tags) = db.get_legacy_file_tags(path) {
        return tags;
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
    } else {
        log::warn!("reindex_sources: assets_dir NOT found at {}", assets_dir.display());
    }
    if let Ok(sources) = db.get_all_file_sources() {

        for source in sources {
            // Avoid scanning the same path twice
            if !scan_paths.contains(&source.path) {
                scan_paths.push(source.path);
            }
        }
    }

    log::info!("reindex_sources: scanning {} paths", scan_paths.len());
    for source_path in scan_paths {
        match do_scan_directory(&db, &source_path) {
            Ok(()) => log::info!("reindex_sources: scanned {} OK", source_path),
            Err(e) => log::error!("reindex_sources: scan {} FAILED: {:?}", source_path, e),
        }
    }
    
    // Check result
    if let Ok(nodes) = db.get_nodes_by_type("file") {
        log::info!("reindex_sources: {} file nodes in DB after scan", nodes.len());
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

/// Compute BLAKE3 hash of first 64KB of a file (partial hash for fast pre-filtering)
fn blake3_partial_hash(path: &std::path::Path) -> Option<(String, usize)> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 65536];
    let n = f.read(&mut buf).ok()?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&buf[..n]);
    Some((hasher.finalize().to_hex().to_string(), n))
}

/// Compute BLAKE3 hash of the entire file (full verification)
fn blake3_full_hash(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 65536];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => { hasher.update(&buf[..n]); },
            Err(_) => return None,
        }
    }
    Some(hasher.finalize().to_hex().to_string())
}

const PARTIAL_HASH_SIZE: usize = 65536; // 64KB

#[tauri::command]
pub async fn find_duplicate_files(app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<()> {
    use tauri::Emitter;

    // Read file list from DB (fast, no heavy I/O)
    let files: Vec<FileMetadata> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let nodes = db.get_nodes_by_type("file")?;
        nodes.iter().filter_map(FileMetadata::from_node).collect()
    };

    // Clone app_handle for use inside blocking thread
    let handle = app_handle.clone();

    // Offload heavy I/O (hashing) to a blocking thread pool
    tauri::async_runtime::spawn_blocking(move || {
        log::info!("Duplicate scan: {} files loaded from DB", files.len());

        // Step 1: Group by size (files with unique sizes can't be duplicates)
        let mut size_groups: HashMap<i64, Vec<FileMetadata>> = HashMap::new();
        for file in files {
            if file.size > 0 {
                size_groups.entry(file.size).or_default().push(file);
            }
        }

        let candidate_size_groups: usize = size_groups.values().filter(|g| g.len() > 1).count();
        log::info!("Duplicate scan: {} size groups with 2+ files", candidate_size_groups);

        // Step 2: Partial hash (first 64KB) for groups with 2+ files of same size
        let mut partial_groups: HashMap<String, (Vec<FileMetadata>, bool)> = HashMap::new();
        for (_size, group) in size_groups.into_iter().filter(|(_, g)| g.len() > 1) {
            for file in group {
                let path = std::path::Path::new(&file.path);
                if !path.exists() {
                    log::warn!("Duplicate scan: file not found on disk: {}", file.path);
                    continue;
                }

                if let Some((hash, bytes_read)) = blake3_partial_hash(path) {
                    let is_complete = bytes_read < PARTIAL_HASH_SIZE || file.size <= PARTIAL_HASH_SIZE as i64;
                    let entry = partial_groups.entry(hash).or_insert_with(|| (Vec::new(), is_complete));
                    entry.0.push(file);
                    if !is_complete {
                        entry.1 = false;
                    }
                }
            }
        }

        let candidate_hash_groups: usize = partial_groups.values().filter(|(g, _)| g.len() > 1).count();
        log::info!("Duplicate scan: {} hash groups with 2+ files", candidate_hash_groups);

        // Step 3: Full hash verification + stream each verified group immediately
        let mut total_groups: usize = 0;
        let mut total_duplicate_files: usize = 0;
        let mut total_wasted_bytes: i64 = 0;

        for (_partial_hash, (candidates, already_complete)) in partial_groups.into_iter() {
            if candidates.len() < 2 { continue; }

            let verified: Vec<Vec<FileMetadata>> = if already_complete {
                vec![candidates]
            } else {
                let mut full_hash_map: HashMap<String, Vec<FileMetadata>> = HashMap::new();
                for file in candidates {
                    let path = std::path::Path::new(&file.path);
                    if let Some(full_hash) = blake3_full_hash(path) {
                        full_hash_map.entry(full_hash).or_default().push(file);
                    }
                }
                full_hash_map.into_values().filter(|g| g.len() > 1).collect()
            };

            // Emit each verified group immediately
            for files in verified {
                let count = files.len();
                let filename = files[0].filename.clone();
                let extension = files[0].extension.clone();
                let size = files[0].size;
                let wasted = size * (count as i64 - 1);
                let group = DuplicateGroup {
                    filename, extension, size, count,
                    files,
                    wasted_bytes: wasted,
                };

                total_groups += 1;
                total_duplicate_files += group.count - 1;
                total_wasted_bytes += group.wasted_bytes;

                let _ = handle.emit("duplicate-group-found", &group);
            }
        }

        // Step 4: Signal scan completion with summary stats
        #[derive(serde::Serialize, Clone)]
        struct ScanComplete {
            total_groups: usize,
            total_duplicate_files: usize,
            total_wasted_bytes: i64,
        }

        let _ = handle.emit("duplicate-scan-complete", ScanComplete {
            total_groups,
            total_duplicate_files,
            total_wasted_bytes,
        });
    });

    Ok(())
}

// ─── Export Annotated PDF ─────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
pub struct AnnotationRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(serde::Deserialize, Debug)]
pub struct ExportAnnotation {
    pub page: usize,
    pub color: String,
    pub text: String,
    pub rects: Vec<AnnotationRect>,
    pub note: String,
}

#[tauri::command]
pub fn export_annotated_pdf(
    _app_handle: tauri::AppHandle,
    vault_path: String,
    pdf_path: String,
    annotations: Vec<ExportAnnotation>,
) -> AppResult<String> {
    use lopdf::{Document, Object, Dictionary};
    use lopdf::StringFormat;

    let source = std::path::Path::new(&pdf_path);
    if !source.exists() {
        return Err(AppError::InvalidPath("PDF file not found".to_string()));
    }

    let mut doc = Document::load(&pdf_path)
        .map_err(|e| AppError::General(format!("Failed to load PDF: {}", e)))?;

    // get_pages() returns BTreeMap<u32, ObjectId> (page_number → object_id)
    let pages = doc.get_pages();

    for ann in &annotations {
        let page_num = ann.page as u32;
        let page_obj_id = match pages.get(&page_num) {
            Some(id) => *id,
            None => continue,
        };

        // Get page MediaBox to convert normalized [0,1] coords to PDF coords
        let media_box = doc.get_dictionary(page_obj_id)
            .ok()
            .and_then(|page| page.get(b"MediaBox").ok().cloned())
            .and_then(|mb| {
                if let Object::Array(arr) = mb {
                    if arr.len() == 4 {
                        let vals: Vec<f64> = arr.iter().filter_map(|v| match v {
                            Object::Real(f) => Some(*f as f64),
                            Object::Integer(i) => Some(*i as f64),
                            _ => None,
                        }).collect();
                        if vals.len() == 4 {
                            return Some((vals[0], vals[1], vals[2], vals[3]));
                        }
                    }
                }
                None
            })
            .unwrap_or((0.0, 0.0, 612.0, 792.0));

        let page_w = media_box.2 - media_box.0;
        let page_h = media_box.3 - media_box.1;

        // Map highlight color to RGB
        let (r, g, b) = match ann.color.as_str() {
            "yellow" => (1.0_f64, 0.92, 0.23),
            "green"  => (0.30, 0.69, 0.31),
            "blue"   => (0.13, 0.59, 0.95),
            "pink"   => (0.91, 0.12, 0.39),
            _        => (1.0, 0.92, 0.23),
        };

        for rect in &ann.rects {
            // Convert normalized coords → PDF coords (PDF origin = bottom-left)
            let x1 = media_box.0 + rect.x * page_w;
            let y1 = media_box.3 - (rect.y + rect.h) * page_h; // flip Y
            let x2 = x1 + rect.w * page_w;
            let y2 = y1 + rect.h * page_h;

            let mut annot_dict = Dictionary::new();
            annot_dict.set("Type", Object::Name(b"Annot".to_vec()));
            annot_dict.set("Subtype", Object::Name(b"Highlight".to_vec()));
            annot_dict.set("Rect", Object::Array(vec![
                Object::Real(x1 as f32), Object::Real(y1 as f32),
                Object::Real(x2 as f32), Object::Real(y2 as f32),
            ]));
            annot_dict.set("C", Object::Array(vec![
                Object::Real(r as f32), Object::Real(g as f32), Object::Real(b as f32),
            ]));
            annot_dict.set("CA", Object::Real(0.4)); // opacity
            annot_dict.set("F", Object::Integer(4)); // Print flag

            // QuadPoints for highlight rendering
            annot_dict.set("QuadPoints", Object::Array(vec![
                Object::Real(x1 as f32), Object::Real(y2 as f32),
                Object::Real(x2 as f32), Object::Real(y2 as f32),
                Object::Real(x1 as f32), Object::Real(y1 as f32),
                Object::Real(x2 as f32), Object::Real(y1 as f32),
            ]));

            // Add note as Contents if present
            if !ann.note.is_empty() {
                annot_dict.set("Contents", Object::String(ann.note.as_bytes().to_vec(), StringFormat::Literal));
            }
            if !ann.text.is_empty() {
                annot_dict.set("T", Object::String(b"Synabit".to_vec(), StringFormat::Literal));
            }

            let annot_id = doc.add_object(Object::Dictionary(annot_dict));

            // Append annotation reference to the page's /Annots array
            let existing_annots = doc.get_dictionary(page_obj_id)
                .ok()
                .and_then(|p| p.get(b"Annots").ok().cloned());

            let mut annots_array = match existing_annots {
                Some(Object::Array(arr)) => arr,
                Some(Object::Reference(r)) => {
                    if let Ok(Object::Array(arr)) = doc.get_object(r) {
                        arr.clone()
                    } else {
                        vec![]
                    }
                },
                _ => vec![],
            };
            annots_array.push(Object::Reference(annot_id));

            // Update the page dictionary with the new Annots array
            if let Ok(page_dict) = doc.get_dictionary_mut(page_obj_id) {
                page_dict.set("Annots", Object::Array(annots_array));
            }
        }
    }

    // Save to a new file alongside the original
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let parent = source.parent().unwrap_or_else(|| Path::new(&vault_path));
    let export_path = parent.join(format!("{}_annotated.pdf", stem));
    
    doc.save(&export_path)
        .map_err(|e| AppError::General(format!("Failed to save annotated PDF: {}", e)))?;

    Ok(export_path.to_string_lossy().to_string())
}

