use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::models::whiteboard::WhiteboardMetadata;
use crate::path_utils;

/// Scan the Whiteboards/ directory and upsert all .whiteboard.json files into the DB cache.
#[tauri::command]
pub fn scan_whiteboards(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<Vec<WhiteboardMetadata>> {
    let mut boards = Vec::new();
    let wb_dir = Path::new(&vault_path).join("Whiteboards");
    if !wb_dir.exists() {
        fs::create_dir_all(&wb_dir)?;
    }

    let db = state.lock().ok();
    let mut current_disk_files = std::collections::HashSet::new();

    for entry in WalkDir::new(&wb_dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let fname = path.file_name().unwrap_or_default().to_string_lossy();
        if !fname.ends_with(".whiteboard.json") {
            continue;
        }

        if let Ok(raw) = fs::read_to_string(path) {
            // Parse the JSON to extract title and tags for the DB cache
            let title: String;
            let tags: Vec<String>;

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                title = parsed
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled")
                    .to_string();
                tags = parsed
                    .get("tags")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
            } else {
                title = "Untitled".to_string();
                tags = Vec::new();
            }

            let metadata = entry
                .metadata()
                .map_err(|e| AppError::General(e.to_string()))?;
            let created = metadata
                .created()
                .unwrap_or(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            let modified = metadata.modified().unwrap_or(created);

            let created_date: chrono::DateTime<chrono::Local> = created.into();
            let modified_date: chrono::DateTime<chrono::Local> = modified.into();

            let rel_path = path_utils::to_relative(path, &vault_path);
            current_disk_files.insert(rel_path.clone());

            // Build a text summary of nodes for searchability
            let content_summary = build_content_summary(&raw);

            let wb_meta = WhiteboardMetadata {
                id: rel_path.clone(),
                title,
                tags,
                content: content_summary,
                path: rel_path,
                created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                updated_at: modified_date.format("%Y-%m-%d %H:%M:%S").to_string(),
            };

            if let Some(db_bridge) = &db {
                let _ = db_bridge.upsert_whiteboard(&wb_meta);
            }
            boards.push(wb_meta);
        }
    }

    // Purge stale DB entries
    if let Some(db_bridge) = &db {
        if let Ok(existing) = db_bridge.get_all_whiteboard_timestamps() {
            for id in existing.keys() {
                if !current_disk_files.contains(id) {
                    let _ = db_bridge.delete_whiteboard(id);
                }
            }
        }
    }

    boards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(boards)
}

#[tauri::command]
pub fn create_whiteboard(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    title: String,
    tags: Vec<String>,
    content: String,
) -> AppResult<WhiteboardMetadata> {
    let wb_dir = Path::new(&vault_path).join("Whiteboards");
    if !wb_dir.exists() {
        fs::create_dir_all(&wb_dir)?;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::General(format!("System time error: {}", e)))?
        .as_millis();
    let filename = format!("whiteboard-{}.whiteboard.json", timestamp);
    let abs_path = wb_dir.join(&filename);

    fs::write(&abs_path, &content)?;

    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
    let rel_path = path_utils::to_relative(&abs_path, &vault_path);

    let content_summary = build_content_summary(&content);

    let wb_meta = WhiteboardMetadata {
        id: rel_path.clone(),
        title,
        tags,
        content: content_summary,
        path: rel_path,
        created_at: date_str.clone(),
        updated_at: date_str,
    };

    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.upsert_whiteboard(&wb_meta);
        db.upsert_search_entry(
            &wb_meta.id,
            "whiteboard",
            &wb_meta.title,
            &wb_meta.tags.join(" "),
            &wb_meta.content,
            "",
            None,
            &wb_meta.created_at,
            &wb_meta.path,
        );
    }

    Ok(wb_meta)
}

#[tauri::command]
pub fn update_whiteboard(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
    title: String,
    tags: Vec<String>,
    content: String,
) -> AppResult<()> {
    path_utils::enforce_no_traversal(&path)?;
    let abs_path = Path::new(&vault_path).join(&path);
    fs::write(&abs_path, &content)?;

    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(file_meta) = fs::metadata(&abs_path) {
            let created = file_meta
                .created()
                .unwrap_or(file_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            let modified = file_meta.modified().unwrap_or(created);
            let created_date: chrono::DateTime<chrono::Local> = created.into();
            let modified_date: chrono::DateTime<chrono::Local> = modified.into();

            let content_summary = build_content_summary(&content);

            let wb_meta = WhiteboardMetadata {
                id: path.clone(),
                title,
                tags,
                content: content_summary,
                path: path.clone(),
                created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                updated_at: modified_date.format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            let _ = db.upsert_whiteboard(&wb_meta);
            db.upsert_search_entry(
                &wb_meta.id,
                "whiteboard",
                &wb_meta.title,
                &wb_meta.tags.join(" "),
                &wb_meta.content,
                "",
                None,
                &wb_meta.created_at,
                &wb_meta.path,
            );
        }
    }

    Ok(())
}

#[tauri::command]
pub fn delete_whiteboard(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
) -> AppResult<()> {
    path_utils::enforce_no_traversal(&path)?;
    let abs_path = Path::new(&vault_path).join(&path);
    fs::remove_file(&abs_path)?;
    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.delete_whiteboard(&path);
        db.delete_search_entry(&path);
    }
    Ok(())
}

#[tauri::command]
pub fn read_whiteboard(
    _app_handle: tauri::AppHandle,
    vault_path: String,
    path: String,
) -> AppResult<String> {
    path_utils::enforce_no_traversal(&path)?;
    let abs_path = Path::new(&vault_path).join(&path);
    let content = fs::read_to_string(&abs_path)?;
    Ok(content)
}

/// Extract text labels from all nodes for FTS searchability.
fn build_content_summary(raw_json: &str) -> String {
    let mut labels = Vec::new();
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw_json) {
        if let Some(nodes) = parsed.get("nodes").and_then(|v| v.as_array()) {
            for node in nodes {
                if let Some(data) = node.get("data") {
                    if let Some(label) = data.get("label").and_then(|v| v.as_str()) {
                        if !label.is_empty() {
                            labels.push(label.to_string());
                        }
                    }
                }
            }
        }
    }
    labels.join(" ")
}
