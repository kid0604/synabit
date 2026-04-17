use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use walkdir::WalkDir;

use crate::models::task::{TaskFrontMatter, TaskMetadata};

#[tauri::command]
pub fn scan_tasks(vault_path: String) -> Result<Vec<TaskMetadata>, String> {
    let mut tasks = Vec::new();
    let matter = Matter::<YAML>::new();
    
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        if let Err(e) = fs::create_dir_all(&tasks_dir) {
            return Err(e.to_string());
        }
    }

    let archived_dir = tasks_dir.join("archived");

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
                        
                        let metadata = entry.metadata().map_err(|e| e.to_string())?;
                        let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
                        let modified = metadata.modified().unwrap_or(created);
                        
                        let created_date: chrono::DateTime<chrono::Local> = created.into();
                        let modified_date: chrono::DateTime<chrono::Local> = modified.into();

                        let path_str = entry.path().to_string_lossy().to_string();
                        let rel_path = entry.path().strip_prefix(&vault_path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path_str);
                        tasks.push(TaskMetadata {
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
                        });
                    }
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
) -> Result<TaskMetadata, String> {
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        fs::create_dir_all(&tasks_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("System time error: {}", e))?.as_millis();
    let filename = format!("task-{}.md", timestamp);
    let path = tasks_dir.join(&filename);
    
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, &full_content).map_err(|e| e.to_string())?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
    
    let rel_path = path.strip_prefix(&vault_path).unwrap_or(&path).to_string_lossy().to_string();
    
    Ok(TaskMetadata {
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
    })
}

#[tauri::command]
pub fn update_task(
    vault_path: String, path: String, metadata: TaskFrontMatter, content: String
) -> Result<(), String> {
    let abs_path = Path::new(&vault_path).join(&path);
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&abs_path, full_content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_task(vault_path: String, path: String) -> Result<(), String> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::remove_file(&abs_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_done_tasks(vault_path: String, days: u64) -> Result<u32, String> {
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
                                fs::create_dir_all(&archived_dir).map_err(|e| e.to_string())?;
                            }
                            let file_name = entry.path().file_name().unwrap_or_default();
                            let dest = archived_dir.join(file_name);
                            fs::rename(entry.path(), &dest).map_err(|e| e.to_string())?;
                            archived_count += 1;
                        }
                    }
                }
            }
        }
    }
    
    Ok(archived_count)
}
