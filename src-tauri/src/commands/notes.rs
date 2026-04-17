use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use gray_matter::Matter;
use gray_matter::engine::YAML;
use walkdir::WalkDir;

use crate::models::note::{FrontMatter, NoteMetadata};

#[tauri::command]
pub fn scan_vault_path(vault_path: String) -> Result<Vec<NoteMetadata>, String> {
    let mut notes = Vec::new();
    let matter = Matter::<YAML>::new();
    
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        if let Err(e) = fs::create_dir_all(&notes_dir) {
            return Err(e.to_string());
        }
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
    
    for entry in WalkDir::new(&notes_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        let mut title = String::new();
                        let mut tags = Vec::new();
                        let mut pinned = false;
                        let summary;
                        let mut date = String::new();

                        if let Ok(parsed) = matter.parse::<FrontMatter>(&content) {
                            if let Some(frontmatter) = parsed.data {
                                title = frontmatter.title;
                                tags = frontmatter.tags;
                                pinned = frontmatter.pinned;
                            }
                            // Build summary from body content only
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

                        if let Ok(metadata) = entry.metadata() {
                            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                            let date_time: chrono::DateTime<chrono::Local> = modified.into();
                            date = date_time.format("%Y-%m-%d %H:%M:%S").to_string();
                        }

                        let path_str = entry.path().to_string_lossy().to_string();
                        let rel_path = entry.path().strip_prefix(&vault_path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path_str);
                        notes.push(NoteMetadata {
                            id: rel_path.clone(),
                            path: rel_path,
                            title,
                            summary,
                            date,
                            tags,
                            pinned,
                            content,
                        });
                    }
                }
            }
        }
    }
    
    // Sort: pinned first, then by date descending within each group
    notes.sort_by(|a, b| {
        b.pinned.cmp(&a.pinned).then_with(|| b.date.cmp(&a.date))
    });
    Ok(notes)
}

#[tauri::command]
pub fn create_new_note(vault_path: String) -> Result<String, String> {
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        fs::create_dir_all(&notes_dir).map_err(|e| e.to_string())?;
    }
    
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).map_err(|e| format!("System time error: {}", e))?;
    let timestamp = since_the_epoch.as_millis();
    
    let filename = format!("Untitled-{}.md", timestamp);
    let path = notes_dir.join(&filename);
    
    let content = "---\ntitle: Untitled Note\ntags: []\n---\n\n";
    fs::write(&path, content).map_err(|e| e.to_string())?;
    
    let rel_path = path.strip_prefix(&vault_path).unwrap_or(&path).to_string_lossy().to_string();
    Ok(rel_path)
}

#[tauri::command]
pub fn open_daily_note(vault_path: String, format_str: String, tag: String) -> Result<String, String> {
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        fs::create_dir_all(&notes_dir).map_err(|e| e.to_string())?;
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
        fs::write(&path, content).map_err(|e| e.to_string())?;
    }
    
    let rel_path = path.strip_prefix(&vault_path).unwrap_or(&path).to_string_lossy().to_string();
    Ok(rel_path)
}

#[tauri::command]
pub fn read_note(vault_path: String, path: String) -> Result<String, String> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::read_to_string(&abs_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_note(vault_path: String, path: String, content: String) -> Result<(), String> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::write(&abs_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note(vault_path: String, path: String) -> Result<(), String> {
    let abs_path = Path::new(&vault_path).join(&path);
    fs::remove_file(&abs_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_note(vault_path: String, old_path: String, new_name: String) -> Result<String, String> {
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
        return Err("A file with this name already exists.".to_string());
    }
    
    fs::rename(&old, &new_path).map_err(|e| e.to_string())?;
    
    // Return relative path of the new file
    Ok(new_path.strip_prefix(base_dir).unwrap_or(&new_path).to_string_lossy().to_string())
}

#[tauri::command]
pub fn save_asset(vault_path: String, filename: String, bytes: Vec<u8>) -> Result<String, String> {
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    }
    
    // Add timestamp to prevent overwriting
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System time error: {}", e))?
        .as_secs();
    let safe_filename = format!("{}-{}", timestamp, filename);
    let target_path = assets_dir.join(&safe_filename);
    
    fs::write(&target_path, bytes).map_err(|e| e.to_string())?;
    Ok(format!("assets/{}", safe_filename))
}

#[tauri::command]
pub fn spawn_note_window(app_handle: tauri::AppHandle, note_id: String) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let encoded_note_id = urlencoding::encode(&note_id);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System time error: {}", e))?
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
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn get_note_backlinks(vault_path: String, target_id: String) -> Result<Vec<NoteMetadata>, String> {
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
                    
                    if content.starts_with("---\n") {
                        if let Some(end_idx) = content[4..].find("\n---\n") {
                            let frontmatter = &content[4..4+end_idx];
                            for line in frontmatter.lines() {
                                if line.starts_with("title: ") {
                                    title = line[7..].trim().trim_matches('"').to_string();
                                } else if line.starts_with("date: ") {
                                    date = line[6..].trim().trim_matches('"').to_string();
                                } else if line.starts_with("pinned: ") {
                                    pinned = line[8..].trim() == "true";
                                } else if line.starts_with("tags: ") {
                                    let tags_str = line[6..].trim().trim_matches(|c| c == '[' || c == ']');
                                    if !tags_str.is_empty() {
                                        for tag in tags_str.split(',') {
                                            tags.push(tag.trim().trim_matches(|c| c == '"' || c == '\'').to_string());
                                        }
                                    }
                                }
                            }
                            let body = &content[4+end_idx+5..];
                            let summary_text: String = body.chars().take(120).collect();
                            summary = summary_text.replace('\n', " ");
                        }
                    }
                    
                    let path_str = entry.path().to_string_lossy().to_string();
                    let rel_path = entry.path().strip_prefix(&vault_path).map(|p| p.to_string_lossy().to_string()).unwrap_or(path_str);
                    backlinks.push(NoteMetadata {
                        id: rel_path.clone(),
                        path: rel_path,
                        title,
                        date,
                        tags,
                        pinned,
                        summary,
                        content: content.clone(),
                    });
                }
            }
        }
    }
    Ok(backlinks)
}
