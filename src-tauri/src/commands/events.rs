use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use walkdir::WalkDir;

use crate::models::event::{EventFrontMatter, EventMetadata};

#[tauri::command]
pub fn scan_events(vault_path: String) -> Result<Vec<EventMetadata>, String> {
    let mut events = Vec::new();
    let matter = Matter::<YAML>::new();
    
    let events_dir = Path::new(&vault_path).join("Events");
    if !events_dir.exists() {
        if let Err(e) = fs::create_dir_all(&events_dir) {
            return Err(e.to_string());
        }
    }

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
                        
                        let metadata = entry.metadata().map_err(|e| e.to_string())?;
                        let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                        let created_date: chrono::DateTime<chrono::Local> = created.into();

                        let path_str = entry.path().to_string_lossy().to_string();
                        let rel_path = entry.path().strip_prefix(&vault_path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path_str);
                        events.push(EventMetadata {
                            id: rel_path.clone(),
                            title,
                            event_date,
                            event_time,
                            location,
                            tags,
                            content: event_content,
                            path: rel_path,
                            created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                        });
                    }
                }
            }
        }
    }
    
    events.sort_by(|a, b| b.event_date.cmp(&a.event_date));
    Ok(events)
}

#[tauri::command]
pub fn create_event(
    vault_path: String, metadata: EventFrontMatter, content: String
) -> Result<EventMetadata, String> {
    let events_dir = Path::new(&vault_path).join("Events");
    if !events_dir.exists() {
        fs::create_dir_all(&events_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System time error: {}", e))?
        .as_millis();
    let filename = format!("event-{}.md", timestamp);
    let path = events_dir.join(&filename);
    
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, &full_content).map_err(|e| e.to_string())?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
    
    let rel_path = path.strip_prefix(&vault_path).unwrap_or(&path).to_string_lossy().to_string();
    Ok(EventMetadata {
        id: rel_path.clone(),
        title: metadata.title,
        event_date: metadata.event_date,
        event_time: metadata.event_time,
        location: metadata.location,
        tags: metadata.tags,
        content,
        path: rel_path,
        created_at: date_str,
    })
}

#[tauri::command]
pub fn update_event(
    vault_path: String, path: String, metadata: EventFrontMatter, content: String
) -> Result<(), String> {
    let abs_path = Path::new(&vault_path).join(&path);
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&abs_path, full_content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_event(vault_path: String, path: String) -> Result<(), String> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::remove_file(&abs_path).map_err(|e| e.to_string())
}
