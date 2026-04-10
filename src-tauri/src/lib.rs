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

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct TaskFrontMatter {
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    start_date: String,
    #[serde(default)]
    due_date: String,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    source_link: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(flatten)]
    custom_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TaskMetadata {
    id: String,
    title: String,
    status: String,
    start_date: String,
    due_date: String,
    comment: String,
    source_link: String,
    tags: Vec<String>,
    content: String,
    path: String,
    created_at: String,
    updated_at: String,
    custom_fields: std::collections::HashMap<String, serde_json::Value>,
}


#[derive(Serialize, Deserialize, Debug)]
pub struct NexusItem {
    id: String,
    item_type: String,
    title: String,
    preview: String,
    tags: Vec<String>,
    date: String,
    path: String,
    #[serde(default)]
    content: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TagStat {
    name: String,
    total_count: usize,
    distribution: std::collections::HashMap<String, usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VaultStats {
    total_items: usize,
    type_distribution: std::collections::HashMap<String, usize>,
    tags: Vec<TagStat>,
}

#[tauri::command]
fn get_nexus_stats(vault_path: String) -> Result<VaultStats, String> {
    let items = get_nexus_items(vault_path)?;
    let mut type_distribution = std::collections::HashMap::new();
    let mut tag_map: std::collections::HashMap<String, TagStat> = std::collections::HashMap::new();
    
    let total_items = items.len();
    
    for item in items {
        *type_distribution.entry(item.item_type.clone()).or_insert(0) += 1;
        
        for mut tag in item.tags {
            if tag.starts_with("#") {
                tag = tag[1..].to_string();
            }
            let t = tag.trim().to_lowercase();
            if t.is_empty() { continue; }
            
            let entry = tag_map.entry(t.clone()).or_insert_with(|| TagStat {
                name: t.clone(),
                total_count: 0,
                distribution: std::collections::HashMap::new(),
            });
            
            entry.total_count += 1;
            *entry.distribution.entry(item.item_type.clone()).or_insert(0) += 1;
        }
    }
    
    let mut tags_vec: Vec<TagStat> = tag_map.into_values().collect();
    tags_vec.sort_by(|a, b| b.total_count.cmp(&a.total_count));
    
    Ok(VaultStats {
        total_items,
        type_distribution,
        tags: tags_vec,
    })
}

#[tauri::command]
fn get_nexus_items(vault_path: String) -> Result<Vec<NexusItem>, String> {
    let mut items = Vec::new();
    
    if let Ok(notes) = scan_vault_path(vault_path.clone()) {
        for n in notes {
            let full_str = format!("{} {}", n.title, n.summary);
            items.push(NexusItem {
                id: n.id,
                item_type: "note".to_string(),
                title: if n.title.is_empty() { "Untitled Note".to_string() } else { n.title },
                preview: n.summary,
                tags: n.tags,
                date: n.date,
                path: n.path,
                content: full_str,
            });
        }
    }
    
    if let Ok(caps) = scan_quick_caps(vault_path.clone()) {
        for c in caps {
            let preview = c.content.trim().chars().take(150).collect();
            let mut extracted_tags: Vec<String> = c.content
                .split_whitespace()
                .filter(|w| w.starts_with('#') && w.len() > 1)
                .map(|w| w[1..].to_string())
                .collect();
            extracted_tags.dedup();
            
            items.push(NexusItem {
                id: c.id.clone(),
                item_type: "quickcap".to_string(),
                title: "⚡ QuickCap".to_string(),
                preview,
                tags: extracted_tags,
                date: c.date,
                path: c.path,
                content: c.content,
            });
        }
    }
    
    if let Ok(tasks) = scan_tasks(vault_path.clone()) {
        for t in tasks {
            let title = if t.title.is_empty() { "Untitled Task".to_string() } else { t.title.clone() };
            let preview = t.content.trim().chars().take(150).collect();
            items.push(NexusItem {
                id: t.id,
                item_type: "task".to_string(),
                title: title.clone(),
                preview,
                tags: t.tags,
                date: t.created_at,
                path: t.path,
                content: format!("{} {}", title, t.content),
            });
        }
    }
    
    items.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(items)
}

#[tauri::command]
fn search_nexus(vault_path: String, query: String) -> Result<Vec<NexusItem>, String> {
    let all_items = get_nexus_items(vault_path)?;
    if query.trim().is_empty() {
        return Ok(all_items.into_iter().take(50).collect()); // return top 50 recent
    }
    
    let q = query.to_lowercase();
    let is_tag_search = q.starts_with("#") && q.len() > 1;
    let tag_search = if is_tag_search { q[1..].trim().to_string() } else { String::new() };

    let filtered = all_items.into_iter().filter(|i| {
        if is_tag_search {
            i.tags.iter().any(|t| t.to_lowercase().contains(&tag_search))
        } else {
            i.title.to_lowercase().contains(&q) || 
            i.content.to_lowercase().contains(&q) ||
            i.tags.iter().any(|t| t.to_lowercase().contains(&q))
        }
    }).take(100).collect(); // cap to top 100 on search
    
    Ok(filtered)
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

#[tauri::command]
fn copy_asset_to_vault(vault_path: String, source_path: String) -> Result<String, String> {
    use std::path::Path;
    let source = Path::new(&source_path);
    if !source.exists() {
        return Err("Source file does not exist".to_string());
    }
    
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis();
    let original_name = source.file_name().unwrap_or_default().to_string_lossy();
    let filename = format!("img-{}-{}", timestamp, original_name);
    let target = assets_dir.join(&filename);
    
    fs::copy(&source, &target).map_err(|e| e.to_string())?;
    
    Ok(format!("assets/{}", filename))
}

#[tauri::command]
fn scan_tasks(vault_path: String) -> Result<Vec<TaskMetadata>, String> {
    use std::path::Path;
    let mut tasks = Vec::new();
    let matter = Matter::<YAML>::new();
    
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        if let Err(e) = fs::create_dir_all(&tasks_dir) {
            return Err(e.to_string());
        }
    }

    for entry in WalkDir::new(&tasks_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        let mut title = String::new();
                        let mut status = "todo".to_string();
                        let mut start_date = String::new();
                        let mut due_date = String::new();
                        let mut comment = String::new();
                        let mut source_link = String::new();
                        let mut tags = Vec::new();
                        let mut task_content = content.clone();
                        let mut custom_fields = std::collections::HashMap::new();

                        if let Ok(parsed) = matter.parse::<TaskFrontMatter>(&content) {
                            if let Some(frontmatter) = parsed.data {
                                title = frontmatter.title;
                                status = frontmatter.status;
                                start_date = frontmatter.start_date;
                                due_date = frontmatter.due_date;
                                comment = frontmatter.comment;
                                source_link = frontmatter.source_link;
                                tags = frontmatter.tags;
                                custom_fields = frontmatter.custom_fields;
                            }
                            task_content = parsed.content;
                        }
                        
                        let metadata = entry.metadata().map_err(|e| e.to_string())?;
                        let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH));
                        let modified = metadata.modified().unwrap_or(created);
                        
                        let created_date: chrono::DateTime<chrono::Local> = created.into();
                        let modified_date: chrono::DateTime<chrono::Local> = modified.into();

                        tasks.push(TaskMetadata {
                            id: entry.path().to_string_lossy().to_string(),
                            title,
                            status,
                            start_date,
                            due_date,
                            comment,
                            source_link,
                            tags,
                            content: task_content,
                            path: entry.path().to_string_lossy().to_string(),
                            created_at: created_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                            updated_at: modified_date.format("%Y-%m-%d %H:%M:%S").to_string(),
                            custom_fields,
                        });
                    }
                }
            }
        }
    }
    
    // Sort logic to have newest tasks first. 
    tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(tasks)
}

#[tauri::command]
fn create_task(
    vault_path: String, metadata: TaskFrontMatter, content: String
) -> Result<TaskMetadata, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::path::Path;
    
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        fs::create_dir_all(&tasks_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time back").as_millis();
    let filename = format!("task-{}.md", timestamp);
    let path = tasks_dir.join(&filename);
    
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, &full_content).map_err(|e| e.to_string())?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
    
    Ok(TaskMetadata {
        id: path.to_string_lossy().to_string(),
        title: metadata.title,
        status: metadata.status,
        start_date: metadata.start_date,
        due_date: metadata.due_date,
        comment: metadata.comment,
        source_link: metadata.source_link,
        tags: metadata.tags,
        custom_fields: metadata.custom_fields,
        content,
        path: path.to_string_lossy().to_string(),
        created_at: date_str.clone(),
        updated_at: date_str,
    })
}

#[tauri::command]
fn update_task(
    path: String, metadata: TaskFrontMatter, content: String
) -> Result<(), String> {
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, full_content).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_task(path: String) -> Result<(), String> {
    fs::remove_file(&path).map_err(|e| e.to_string())
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
            update_quick_cap,
            copy_asset_to_vault,
            scan_tasks,
            create_task,
            update_task,
            delete_task,
            get_nexus_items,
            search_nexus,
            get_nexus_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
