use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use walkdir::WalkDir;

use crate::models::event::{EventFrontMatter, EventMetadata};
use crate::error::{AppError, AppResult};
use crate::path_utils;
use crate::db::DbState;

#[tauri::command]
pub fn scan_events(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<Vec<EventMetadata>> {
    let mut events = Vec::new();
    let matter = Matter::<YAML>::new();
    
    let events_dir = Path::new(&vault_path).join("Events");
    if !events_dir.exists() {
        fs::create_dir_all(&events_dir)?;
    }

    let db = state.lock().ok();
    let mut current_disk_files = std::collections::HashSet::new();

    for entry in WalkDir::new(&events_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        let mut title = String::new();
                        let mut event_date = String::new();
                        let mut event_time = String::new();
                        let mut location = String::new();
                        let mut tags = Vec::new();
                        let mut event_content = content.clone();

                        if let Ok(parsed) = matter.parse::<EventFrontMatter>(&content) {
                            if let Some(frontmatter) = parsed.data {
                                title = frontmatter.title;
                                event_date = frontmatter.event_date;
                                event_time = frontmatter.event_time;
                                location = frontmatter.location;
                                tags = frontmatter.tags;
                            }
                            event_content = parsed.content;
                        }
                        
                        let metadata = entry.metadata().map_err(|e| AppError::General(e.to_string()))?;
                        let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                        let created_date: chrono::DateTime<chrono::Local> = created.into();

                        let rel_path = path_utils::to_relative(entry.path(), &vault_path);
                        current_disk_files.insert(rel_path.clone());
                        let event_meta = EventMetadata {
                            id: rel_path.clone(),
                            title,
                            event_date,
                            event_time,
                            location,
                            tags,
                            content: event_content,
                            path: rel_path,
                            created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                        };
                        
                        if let Some(db_bridge) = &db {
                            let _ = db_bridge.upsert_event(&event_meta);
                        }
                        events.push(event_meta);
                    }
                }
            }
        }
    }
    
    if let Some(db_bridge) = &db {
        if let Ok(existing) = db_bridge.get_all_event_timestamps() {
            for id in existing.keys() {
                if !current_disk_files.contains(id) {
                    let _ = db_bridge.delete_event(id);
                }
            }
        }
    }
    
    events.sort_by(|a, b| b.event_date.cmp(&a.event_date));
    Ok(events)
}

#[tauri::command]
pub fn create_event(
    _app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>,
    vault_path: String, metadata: EventFrontMatter, content: String
) -> AppResult<EventMetadata> {
    let events_dir = Path::new(&vault_path).join("Events");
    if !events_dir.exists() {
        fs::create_dir_all(&events_dir)?;
    }
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::General(format!("System time error: {}", e)))?
        .as_millis();
    let filename = format!("event-{}.md", timestamp);
    let path = events_dir.join(&filename);
    
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| AppError::General(e.to_string()))?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, &full_content)?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
    
    let rel_path = path_utils::to_relative(&path, &vault_path);
    let event_meta = EventMetadata {
        id: rel_path.clone(),
        title: metadata.title,
        event_date: metadata.event_date,
        event_time: metadata.event_time,
        location: metadata.location,
        tags: metadata.tags,
        content,
        path: rel_path,
        created_at: date_str,
    };
    
    { let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.upsert_event(&event_meta);
        let mut props_parts: Vec<String> = Vec::new();
        if !event_meta.location.is_empty() { props_parts.push(format!("location:{}", event_meta.location)); }
        if !event_meta.event_time.is_empty() { props_parts.push(format!("time:{}", event_meta.event_time)); }
        let props = props_parts.join(" ");
        db.upsert_search_entry(&event_meta.id, "event", &event_meta.title, &event_meta.tags.join(" "), &event_meta.content, &props, None, &event_meta.event_date, &event_meta.path);
    }
    Ok(event_meta)
}

#[tauri::command]
pub fn update_event(
    _app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>,
    vault_path: String, path: String, metadata: EventFrontMatter, content: String
) -> AppResult<()> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &path)?;
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| AppError::General(e.to_string()))?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&abs_path, full_content)?;
    
    { let db = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(file_meta) = fs::metadata(&abs_path) {
            let created = file_meta.created().unwrap_or(file_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            let created_date: chrono::DateTime<chrono::Local> = created.into();
            
            let event_meta = EventMetadata {
                id: path.clone(),
                title: metadata.title,
                event_date: metadata.event_date,
                event_time: metadata.event_time,
                location: metadata.location,
                tags: metadata.tags,
                content,
                path: path.clone(),
                created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
            };
            let _ = db.upsert_event(&event_meta);
            let mut props_parts: Vec<String> = Vec::new();
            if !event_meta.location.is_empty() { props_parts.push(format!("location:{}", event_meta.location)); }
            if !event_meta.event_time.is_empty() { props_parts.push(format!("time:{}", event_meta.event_time)); }
            let props = props_parts.join(" ");
            db.upsert_search_entry(&event_meta.id, "event", &event_meta.title, &event_meta.tags.join(" "), &event_meta.content, &props, None, &event_meta.event_date, &event_meta.path);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn delete_event(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String, path: String) -> AppResult<()> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &path)?;
    fs::remove_file(&abs_path)?;
    { let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = db.delete_event(&path);
        db.delete_search_entry(&path);
    }
    Ok(())
}
