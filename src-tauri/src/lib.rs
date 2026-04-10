use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use gray_matter::Matter;
use gray_matter::engine::YAML;
use std::fs;

#[derive(Serialize, Deserialize, Debug, Default)]
struct FrontMatter {
    #[serde(default)]
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    pinned: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NoteMetadata {
    id: String,
    title: String,
    summary: String,
    date: String,
    tags: Vec<String>,
    path: String,
    pinned: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct QuickCapMetadata {
    id: String,
    date: String,
    content: String,
    path: String,
}

#[tauri::command]
fn scan_vault_path(vault_path: String) -> Result<Vec<NoteMetadata>, String> {
    let mut notes = Vec::new();
    let matter = Matter::<YAML>::new();
    
    let notes_dir = std::path::Path::new(&vault_path).join("Notes");
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
                        let mut title = entry.path().file_stem().unwrap_or_default().to_string_lossy().to_string();
                        let mut tags = Vec::new();
                        let mut summary = String::new();
                        let mut pinned = false;

                        if let Ok(parsed) = matter.parse::<FrontMatter>(&content) {
                            if let Some(frontmatter) = parsed.data {
                                tags = frontmatter.tags;
                                if !frontmatter.title.is_empty() {
                                    title = frontmatter.title;
                                }
                                pinned = frontmatter.pinned;
                            }
                            summary = parsed.content.chars().take(150).collect();
                        } else {
                            summary = content.chars().take(150).collect();
                        }
                        
                        let metadata = entry.metadata().map_err(|e| e.to_string())?;
                        let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH));
                        let date: chrono::DateTime<chrono::Local> = created.into();

                        notes.push(NoteMetadata {
                            id: entry.path().to_string_lossy().to_string(),
                            title,
                            summary,
                            date: date.format("%Y-%m-%d").to_string(),
                            tags,
                            path: entry.path().to_string_lossy().to_string(),
                            pinned,
                        });
                    }
                }
            }
        }
    }
    
    // Sort logic to have newest notes first. 
    notes.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(notes)
}

#[tauri::command]
fn create_new_note(vault_path: String) -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::path::Path;
    
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        fs::create_dir_all(&notes_dir).map_err(|e| e.to_string())?;
    }
    
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
    let timestamp = since_the_epoch.as_millis();
    
    let filename = format!("Untitled-{}.md", timestamp);
    let path = notes_dir.join(&filename);
    
    let content = "---\ntitle: Untitled Note\ntags: []\n---\n\n";
    fs::write(&path, content).map_err(|e| e.to_string())?;
    
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
fn read_note(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_note(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_asset(vault_path: String, filename: String, bytes: Vec<u8>) -> Result<String, String> {
    use std::path::Path;
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    }
    
    // Add timestamp to prevent overwriting
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let safe_filename = format!("{}-{}", timestamp, filename);
    let target_path = assets_dir.join(&safe_filename);
    
    fs::write(&target_path, bytes).map_err(|e| e.to_string())?;
    Ok(format!("assets/{}", safe_filename))
}

#[tauri::command]
fn delete_note(path: String) -> Result<(), String> {
    fs::remove_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn rename_note(vault_path: String, old_path: String, new_name: String) -> Result<String, String> {
    use std::path::Path;
    let old = Path::new(&old_path);
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
    
    fs::rename(old, &new_path).map_err(|e| e.to_string())?;
    Ok(new_path.to_string_lossy().to_string())
}

#[tauri::command]
fn scan_quick_caps(vault_path: String) -> Result<Vec<QuickCapMetadata>, String> {
    use std::path::Path;
    let mut caps = Vec::new();
    let qc_dir = Path::new(&vault_path).join("QuickCaps");
    
    if !qc_dir.exists() {
        return Ok(caps);
    }

    for entry in fs::read_dir(&qc_dir).map_err(|e| e.to_string())?.filter_map(|e| e.ok()) {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH));
                    let date: chrono::DateTime<chrono::Local> = created.into();
                    
                    caps.push(QuickCapMetadata {
                        id: entry.path().to_string_lossy().to_string(),
                        date: date.format("%Y-%m-%d %H:%M:%S").to_string(),
                        content,
                        path: entry.path().to_string_lossy().to_string(),
                    });
                }
            }
        }
    }
    
    // Sort logic to have newest quickcaps first. 
    caps.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(caps)
}

#[tauri::command]
fn create_quick_cap(vault_path: String, content: String) -> Result<QuickCapMetadata, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::path::Path;
    
    let qc_dir = Path::new(&vault_path).join("QuickCaps");
    if !qc_dir.exists() {
        fs::create_dir_all(&qc_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time back").as_millis();
    let filename = format!("qc-{}.md", timestamp);
    let path = qc_dir.join(&filename);
    
    fs::write(&path, &content).map_err(|e| e.to_string())?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    
    Ok(QuickCapMetadata {
        id: path.to_string_lossy().to_string(),
        date: date.format("%Y-%m-%d %H:%M:%S").to_string(),
        content,
        path: path.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn update_quick_cap(path: String, content: String) -> Result<(), String> {
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            scan_vault_path, 
            create_new_note, 
            read_note, 
            update_note,
            save_asset,
            delete_note,
            rename_note,
            scan_quick_caps,
            create_quick_cap,
            update_quick_cap
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
