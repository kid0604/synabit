use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use walkdir::WalkDir;

use crate::models::task::{TaskFrontMatter, TaskMetadata};
use crate::error::{AppError, AppResult};
use crate::path_utils;
use crate::db::DbBridge;

#[tauri::command]
pub fn scan_tasks(vault_path: String) -> AppResult<Vec<TaskMetadata>> {
    let mut tasks = Vec::new();
    let matter = Matter::<YAML>::new();
    
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        fs::create_dir_all(&tasks_dir)?;
    }

    let archived_dir = tasks_dir.join("archived");
    let db = DbBridge::new(&vault_path).ok();
    let mut current_disk_files = std::collections::HashSet::new();

    for entry in WalkDir::new(&tasks_dir).into_iter().filter_map(|e| e.ok()) {
        // Skip files inside the archived subdirectory
        if entry.path().starts_with(&archived_dir) {
            continue;
        }
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        let mut title = String::new();
                        let mut status = "todo".to_string();
                        let mut is_transferred = false;
                        let mut transferred_to = String::new();
                        let mut track_progress = false;
                        let mut priority = String::new();
                        let mut start_date = String::new();
                        let mut due_date = String::new();
                        let mut comment = String::new();
                        let mut source_link = String::new();
                        let mut tags = Vec::new();
                        let mut checklist = Vec::new();
                        let mut task_content = content.clone();
                        let mut custom_fields = std::collections::HashMap::new();
                        let mut completed_at = String::new();

                        if let Ok(parsed) = matter.parse::<TaskFrontMatter>(&content) {
                            if let Some(frontmatter) = parsed.data {
                                title = frontmatter.title;
                                status = frontmatter.status;
                                is_transferred = frontmatter.is_transferred;
                                transferred_to = frontmatter.transferred_to;
                                track_progress = frontmatter.track_progress;
                                priority = frontmatter.priority;
                                start_date = frontmatter.start_date;
                                due_date = frontmatter.due_date;
                                comment = frontmatter.comment;
                                source_link = frontmatter.source_link;
                                tags = frontmatter.tags;
                                checklist = frontmatter.checklist;
                                custom_fields = frontmatter.custom_fields;
                                completed_at = frontmatter.completed_at;
                            }
                            task_content = parsed.content;
                        }
                        
                        let metadata = entry.metadata().map_err(|e| AppError::General(e.to_string()))?;
                        let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                        let modified = metadata.modified().unwrap_or(created);
                        
                        let created_date: chrono::DateTime<chrono::Local> = created.into();
                        let modified_date: chrono::DateTime<chrono::Local> = modified.into();

                        let rel_path = path_utils::to_relative(entry.path(), &vault_path);
                        current_disk_files.insert(rel_path.clone());
                        let task_meta = TaskMetadata {
                            id: rel_path.clone(),
                            title,
                            status,
                            is_transferred,
                            transferred_to,
                            track_progress,
                            priority,
                            start_date,
                            due_date,
                            comment,
                            source_link,
                            tags,
                            checklist,
                            content: task_content,
                            path: rel_path,
                            created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                            updated_at: modified_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                            completed_at,
                            custom_fields,
                        };
                        
                        if let Some(db_bridge) = &db {
                            let _ = db_bridge.upsert_task(&task_meta);
                        }
                        tasks.push(task_meta);
                    }
                }
            }
        }
    }
    
    if let Some(db_bridge) = &db {
        if let Ok(existing) = db_bridge.get_all_task_timestamps() {
            for id in existing.keys() {
                if !current_disk_files.contains(id) {
                    let _ = db_bridge.delete_task(id);
                }
            }
        }
    }
    
    // Sort: newest tasks first
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(tasks)
}

#[tauri::command]
pub fn create_task(
    vault_path: String, metadata: TaskFrontMatter, content: String
) -> AppResult<TaskMetadata> {
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        fs::create_dir_all(&tasks_dir)?;
    }
    
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| AppError::General(format!("System time error: {}", e)))?.as_millis();
    let filename = format!("task-{}.md", timestamp);
    let path = tasks_dir.join(&filename);
    
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| AppError::General(e.to_string()))?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, &full_content)?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
    
    let rel_path = path_utils::to_relative(&path, &vault_path);
    
    let task_meta = TaskMetadata {
        id: rel_path.clone(),
        title: metadata.title,
        status: metadata.status,
        is_transferred: metadata.is_transferred,
        transferred_to: metadata.transferred_to,
        track_progress: metadata.track_progress,
        priority: metadata.priority,
        start_date: metadata.start_date,
        due_date: metadata.due_date,
        comment: metadata.comment,
        source_link: metadata.source_link,
        tags: metadata.tags,
        checklist: metadata.checklist,
        custom_fields: metadata.custom_fields,
        completed_at: metadata.completed_at,
        content,
        path: rel_path,
        created_at: date_str.clone(),
        updated_at: date_str,
    };
    
    if let Ok(db) = DbBridge::new(&vault_path) {
        let _ = db.upsert_task(&task_meta);
    }
    Ok(task_meta)
}

#[tauri::command]
pub fn update_task(
    vault_path: String, path: String, metadata: TaskFrontMatter, content: String
) -> AppResult<()> {
    let abs_path = Path::new(&vault_path).join(&path);
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| AppError::General(e.to_string()))?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&abs_path, full_content)?;
    
    if let Ok(db) = DbBridge::new(&vault_path) {
        if let Ok(file_meta) = fs::metadata(&abs_path) {
            let created = file_meta.created().unwrap_or(file_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            let modified = file_meta.modified().unwrap_or(created);
            let created_date: chrono::DateTime<chrono::Local> = created.into();
            let modified_date: chrono::DateTime<chrono::Local> = modified.into();
            
            let task_meta = TaskMetadata {
                id: path.clone(),
                title: metadata.title,
                status: metadata.status,
                is_transferred: metadata.is_transferred,
                transferred_to: metadata.transferred_to,
                track_progress: metadata.track_progress,
                priority: metadata.priority,
                start_date: metadata.start_date,
                due_date: metadata.due_date,
                comment: metadata.comment,
                source_link: metadata.source_link,
                tags: metadata.tags,
                checklist: metadata.checklist,
                content: content,
                path: path.clone(),
                created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                updated_at: modified_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                completed_at: metadata.completed_at,
                custom_fields: metadata.custom_fields,
            };
            let _ = db.upsert_task(&task_meta);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn delete_task(vault_path: String, path: String) -> AppResult<()> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::remove_file(&abs_path)?;
    if let Ok(db) = DbBridge::new(&vault_path) {
        let _ = db.delete_task(&path);
    }
    Ok(())
}

#[tauri::command]
pub fn archive_done_tasks(vault_path: String, days: u64) -> AppResult<u32> {
    use chrono::NaiveDate;
    
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        return Ok(0);
    }
    
    let archived_dir = tasks_dir.join("archived");
    let matter = Matter::<YAML>::new();
    let today = chrono::Local::now().date_naive();
    let mut archived_count: u32 = 0;
    
    let entries: Vec<_> = WalkDir::new(&tasks_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| !e.path().starts_with(&archived_dir))
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();
    
    for entry in entries {
        if let Ok(content) = fs::read_to_string(entry.path()) {
            if let Ok(parsed) = matter.parse::<TaskFrontMatter>(&content) {
                if let Some(ref fm) = parsed.data {
                    if fm.status != "done" || fm.completed_at.is_empty() {
                        continue;
                    }
                    // Parse completed_at date (format: YYYY-MM-DD or YYYY-MM-DD HH:MM:SS)
                    let date_part = fm.completed_at.split_whitespace().next().unwrap_or("");
                    if let Ok(completed_date) = NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                        let elapsed = today.signed_duration_since(completed_date).num_days();
                        if elapsed >= days as i64 {
                            // Move file to archived/
                            if !archived_dir.exists() {
                                fs::create_dir_all(&archived_dir)?;
                            }
                            let file_name = entry.path().file_name().unwrap_or_default();
                            let dest = archived_dir.join(file_name);
                            fs::rename(entry.path(), &dest)?;
                            archived_count += 1;
                        }
                    }
                }
            }
        }
    }
    
    Ok(archived_count)
}
