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

#[tauri::command]
fn scan_vault_path(vault_path: String) -> Result<Vec<NoteMetadata>, String> {
    let mut notes = Vec::new();
    let matter = Matter::<YAML>::new();

    for entry in WalkDir::new(&vault_path).into_iter().filter_map(|e| e.ok()) {
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
    
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards");
    let timestamp = since_the_epoch.as_millis();
    
    let filename = format!("Untitled-{}.md", timestamp);
    let path = Path::new(&vault_path).join(&filename);
    
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
    
    let new_path = Path::new(&vault_path).join(&final_name);
    
    if new_path.exists() && new_path != old {
        return Err("A file with this name already exists.".to_string());
    }
    
    fs::rename(old, &new_path).map_err(|e| e.to_string())?;
    Ok(new_path.to_string_lossy().to_string())
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
            rename_note
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
