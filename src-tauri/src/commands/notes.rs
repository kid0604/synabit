use std::fs;
use std::path::Path;
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use walkdir::WalkDir;

use crate::models::note::{FrontMatter, NoteMetadata};
use crate::error::{AppError, AppResult};
use crate::db::DbBridge;
use crate::path_utils;

#[tauri::command]
pub fn scan_vault_path(vault_path: String) -> AppResult<Vec<NoteMetadata>> {
    let matter = Matter::<YAML>::new();
    
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        fs::create_dir_all(&notes_dir)?;
    }
    
    // Auto-migrate existing loose .md files from root vault to Notes folder
    if let Ok(entries) = fs::read_dir(&vault_path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(file_name) = path.file_name() {
                            let target = notes_dir.join(file_name);
                            let _ = fs::rename(&path, &target);
                        }
                    }
                }
            }
        }
    }
    
    let db = DbBridge::new(&vault_path)?;
    let existing_timestamps = db.get_all_note_timestamps()?;
    let mut current_disk_files = HashSet::new();

    for entry in WalkDir::new(&notes_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    let rel_path = path_utils::to_relative(entry.path(), &vault_path);
                    current_disk_files.insert(rel_path.clone());

                    if let Ok(metadata) = entry.metadata() {
                        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        let timestamp = modified.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;

                        // Check if file needs parsing
                        let needs_update = match existing_timestamps.get(&rel_path) {
                            Some(&ts) => timestamp > ts,
                            None => true,
                        };

                        if needs_update {
                            if let Ok(content) = fs::read_to_string(entry.path()) {
                                let mut title = String::new();
                                let mut tags = Vec::new();
                                let mut pinned = false;
                                let summary;
                                let date_time: chrono::DateTime<chrono::Local> = modified.into();
                                let date = date_time.format("%Y-%m-%d %H:%M:%S").to_string();

                                if let Ok(parsed) = matter.parse::<FrontMatter>(&content) {
                                    if let Some(frontmatter) = parsed.data {
                                        title = frontmatter.title;
                                        tags = frontmatter.tags;
                                        pinned = frontmatter.pinned;
                                    }
                                    let body_text: String = parsed.content.chars().take(120).collect();
                                    summary = body_text.replace('\n', " ");
                                } else {
                                    summary = content.chars().take(120).collect::<String>().replace('\n', " ");
                                }

                                if title.is_empty() {
                                    title = entry.path()
                                        .file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                }

                                let note = NoteMetadata {
                                    id: rel_path.clone(),
                                    path: rel_path.clone(),
                                    title,
                                    summary,
                                    date,
                                    timestamp, // We temporarily use timestamp from metadata as u64
                                    tags,
                                    pinned,
                                    content,
                                    is_task: false,
                                    is_event: false,
                                    has_reminder: false,
                                    is_done: false,
                                    raw_frontmatter: String::new(),
                                };
                                let _ = db.upsert_note(&note);
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Cleanup deleted files from DB
    for id in existing_timestamps.keys() {
        if !current_disk_files.contains(id) {
            let _ = db.delete_note(id);
        }
    }

    let mut notes = db.get_all_notes()?;
    // Sort: pinned first, then by date descending
    notes.sort_by(|a, b| {
        b.pinned.cmp(&a.pinned).then_with(|| b.date.cmp(&a.date))
    });
    
    Ok(notes)
}

#[tauri::command]
pub fn create_new_note(vault_path: String) -> AppResult<String> {
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        fs::create_dir_all(&notes_dir)?;
    }
    
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).map_err(|e| AppError::General(format!("System time error: {}", e)))?;
    let timestamp = since_the_epoch.as_millis();
    
    let filename = format!("Untitled-{}.md", timestamp);
    let path = notes_dir.join(&filename);
    
    let content = "---\ntitle: Untitled Note\ntags: []\n---\n\n";
    fs::write(&path, content)?;
    
    let rel_path = path_utils::to_relative(&path, &vault_path);
    Ok(rel_path)
}

#[tauri::command]
pub fn open_daily_note(vault_path: String, format_str: String, tag: String) -> AppResult<String> {
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        fs::create_dir_all(&notes_dir)?;
    }
    
    // Convert common YYYY-MM-DD pattern to chrono's format
    let chrono_format = format_str
        .replace("YYYY", "%Y")
        .replace("YY", "%y")
        .replace("MM", "%m")
        .replace("M", "%-m")
        .replace("DD", "%d")
        .replace("D", "%-d");
        
    let today = chrono::Local::now();
    let date_str = today.format(&chrono_format).to_string();
    let filename = format!("{}.md", date_str);
    let path = notes_dir.join(&filename);
    
    if !path.exists() {
        let title = date_str.clone();
        let content = if tag.trim().is_empty() {
            format!("---\ntitle: \"{}\"\n---\n\n", title)
        } else {
            format!("---\ntitle: \"{}\"\ntags:\n  - {}\n---\n\n", title, tag.trim())
        };
        fs::write(&path, content)?;
    }
    
    let rel_path = path_utils::to_relative(&path, &vault_path);
    Ok(rel_path)
}

#[tauri::command]
pub fn read_note(vault_path: String, path: String) -> AppResult<String> {
    let abs_path = Path::new(&vault_path).join(&path);
    let content = fs::read_to_string(&abs_path)?;
    Ok(content)
}

#[tauri::command]
pub fn update_note(vault_path: String, path: String, content: String) -> AppResult<()> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::write(&abs_path, content)?;
    Ok(())
}

#[tauri::command]
pub fn delete_note(vault_path: String, path: String) -> AppResult<()> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::remove_file(&abs_path)?;
    Ok(())
}

#[tauri::command]
pub fn rename_note(vault_path: String, old_path: String, new_name: String) -> AppResult<String> {
    let base_dir = Path::new(&vault_path);
    let old = base_dir.join(&old_path);
    
    // Secure the new name: ensuring no path traversal
    let safe_name = new_name.replace("/", "").replace("\\", "");
    let mut final_name = safe_name;
    if !final_name.to_lowercase().ends_with(".md") {
        final_name = format!("{}.md", final_name);
    }
    
    // Rename in the same directory as the original file
    let parent_dir = old.parent().unwrap_or_else(|| Path::new(&vault_path));
    let new_path = parent_dir.join(&final_name);
    
    if new_path.exists() && new_path != old {
        return Err(AppError::InvalidPath("A file with this name already exists.".to_string()));
    }
    
    fs::rename(&old, &new_path)?;
    
    // Return relative path of the new file
    Ok(path_utils::to_relative(&new_path, &vault_path))
}

#[tauri::command]
pub fn save_asset(vault_path: String, filename: String, bytes: Vec<u8>) -> AppResult<String> {
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        fs::create_dir_all(&assets_dir)?;
    }
    
    // Add timestamp to prevent overwriting
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::General(format!("System time error: {}", e)))?
        .as_secs();
    let safe_filename = format!("{}-{}", timestamp, filename);
    let target_path = assets_dir.join(&safe_filename);
    
    fs::write(&target_path, bytes)?;
    Ok(format!("assets/{}", safe_filename))
}

#[tauri::command]
pub fn spawn_note_window(app_handle: tauri::AppHandle, note_id: String) -> AppResult<()> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let encoded_note_id = urlencoding::encode(&note_id);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::General(format!("System time error: {}", e)))?
        .as_micros();
    let window_label = format!("note_{}", timestamp);
    
    let url = WebviewUrl::App(format!("index.html?floatingNote={}", encoded_note_id).into());

    let _ = WebviewWindowBuilder::new(&app_handle, window_label, url)
        .title("Note View")
        .inner_size(600.0, 700.0)
        .minimizable(true)
        .maximizable(true)
        .closable(true)
        .build()
        .map_err(|e| AppError::General(e.to_string()))?;

    Ok(())
}

#[tauri::command]
pub async fn get_note_backlinks(vault_path: String, target_id: String) -> AppResult<Vec<NoteMetadata>> {
    let notes_dir = Path::new(&vault_path).join("Notes");
    let mut backlinks = Vec::new();
    
    if !notes_dir.exists() {
        return Ok(backlinks);
    }

    for entry in WalkDir::new(&notes_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "md") {
            if entry.path().file_name().unwrap_or_default() == target_id.as_str() {
                continue; 
            }
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if content.contains("synabit://note/") && content.contains(&target_id) {
                    let mut title = String::from("Untitled");
                    let mut date = String::new();
                    let mut tags = Vec::new();
                    let mut pinned = false;
                    let mut summary = String::new();
                    
                    if let Some(stripped) = content.strip_prefix("---\n") {
                        if let Some(end_idx) = stripped.find("\n---\n") {
                            let frontmatter = &stripped[..end_idx];
                            for line in frontmatter.lines() {
                                if let Some(stripped_line) = line.strip_prefix("title: ") {
                                    title = stripped_line.trim().trim_matches('"').to_string();
                                } else if let Some(stripped_line) = line.strip_prefix("date: ") {
                                    date = stripped_line.trim().trim_matches('"').to_string();
                                } else if let Some(stripped_line) = line.strip_prefix("pinned: ") {
                                    pinned = stripped_line.trim() == "true";
                                } else if let Some(stripped_line) = line.strip_prefix("tags: ") {
                                    let tags_str = stripped_line.trim().trim_matches(|c| c == '[' || c == ']');
                                    if !tags_str.is_empty() {
                                        for tag in tags_str.split(',') {
                                            tags.push(tag.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
                                        }
                                    }
                                }
                            }
                            let body = &stripped[end_idx+5..];
                            let summary_text: String = body.chars().take(120).collect();
                            summary = summary_text.replace('\n', " ");
                        }
                    }
                    
                    let rel_path = path_utils::to_relative(entry.path(), &vault_path);
                    backlinks.push(NoteMetadata {
                        id: rel_path.clone(),
                        path: rel_path,
                        title,
                        date,
                        timestamp: 0,
                        tags,
                        pinned,
                        summary,
                        content: content.clone(),
                        is_task: false,
                        is_event: false,
                        has_reminder: false,
                        is_done: false,
                        raw_frontmatter: String::new(),
                    });
                }
            }
        }
    }
    Ok(backlinks)
}
