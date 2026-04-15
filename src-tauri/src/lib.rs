use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use gray_matter::Matter;
use gray_matter::engine::YAML;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileItem {
    pub id: String,
    pub name: String,
    pub extension: String,
    pub size_mb: f64,
    pub source_folder: String,
    pub date_modified: String,
    pub absolute_path: String,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct FileManagerSettings {
    pub tracked_sources: Vec<String>,
}

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
pub struct ChecklistItem {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct TaskFrontMatter {
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    is_transferred: bool,
    #[serde(default)]
    transferred_to: String,
    #[serde(default)]
    track_progress: bool,
    #[serde(default)]
    priority: String,
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
    #[serde(default)]
    checklist: Vec<ChecklistItem>,
    #[serde(default)]
    completed_at: String,
    #[serde(flatten)]
    custom_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TaskMetadata {
    id: String,
    title: String,
    status: String,
    is_transferred: bool,
    transferred_to: String,
    track_progress: bool,
    priority: String,
    start_date: String,
    due_date: String,
    comment: String,
    source_link: String,
    tags: Vec<String>,
    checklist: Vec<ChecklistItem>,
    content: String,
    path: String,
    created_at: String,
    updated_at: String,
    completed_at: String,
    custom_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct EventFrontMatter {
    #[serde(default)]
    title: String,
    #[serde(default)]
    event_date: String,
    #[serde(default)]
    event_time: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventMetadata {
    id: String,
    title: String,
    event_date: String,
    event_time: String,
    location: String,
    tags: Vec<String>,
    content: String,
    path: String,
    created_at: String,
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
    
    if let Ok(files) = get_file_items(vault_path.clone()) {
        for f in files {
            items.push(NexusItem {
                id: f.absolute_path.clone(),
                item_type: "file".to_string(),
                title: f.name.clone(),
                preview: format!("{} • {}MB", f.extension, (f.size_mb * 100.0).round() / 100.0),
                tags: f.tags.clone(),
                date: f.date_modified,
                path: f.absolute_path,
                content: f.name,
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

    let mut type_filter = None;
    let mut clean_query = query.trim().to_lowercase();
    
    // Simple filter syntax parsing
    let prefixes = ["is:task", "is:note", "is:quickcap", "is:file"];
    for p in prefixes.iter() {
        if clean_query.contains(*p) {
            type_filter = Some(p[3..].to_string());
            clean_query = clean_query.replace(*p, "").trim().to_string();
        }
    }

    let terms: Vec<String> = clean_query.split_whitespace().map(|s| s.to_string()).collect();
    let mut scored_items: Vec<(NexusItem, i32)> = Vec::new();

    for item in all_items {
        if let Some(ref t) = type_filter {
            if item.item_type != *t { continue; }
        }

        let mut score = 0;
        let title_lower = item.title.to_lowercase();
        let content_lower = item.content.to_lowercase();
        
        let mut matches_all_terms = true;
        for term in &terms {
            let mut term_matched = false;
            let is_tag_term = term.starts_with("#");
            let tag_name = if is_tag_term { term[1..].to_string() } else { String::new() };

            if is_tag_term {
                if item.tags.iter().any(|t| t.to_lowercase() == tag_name) {
                    score += 50;
                    term_matched = true;
                } else if item.tags.iter().any(|t| t.to_lowercase().contains(&tag_name)) {
                    score += 10;
                    term_matched = true;
                }
            } else {
                if title_lower.contains(term) {
                    score += 30;
                    term_matched = true;
                } else if item.tags.iter().any(|t| t.to_lowercase().contains(term)) {
                    score += 10;
                    term_matched = true;
                } else if content_lower.contains(term) {
                    score += 5;
                    term_matched = true;
                }
            }

            if !term_matched && !clean_query.is_empty() {
                matches_all_terms = false;
                break; // must contain all terms (AND logic)
            }
        }

        if matches_all_terms && (score > 0 || clean_query.is_empty()) {
            // Bonus for exact phrase match
            if clean_query.len() > 3 && content_lower.contains(&clean_query) {
                score += 20;
            }
            if clean_query.len() > 3 && title_lower.contains(&clean_query) {
                score += 50;
            }
            scored_items.push((item, score));
        }
    }

    // Sort by score first, then by date descending
    scored_items.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| b.0.date.cmp(&a.0.date))
    });

    let result = scored_items.into_iter().map(|(item, _)| item).take(100).collect();
    Ok(result)
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
fn spawn_note_window(app_handle: tauri::AppHandle, note_id: String) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let encoded_note_id = urlencoding::encode(&note_id);
    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros();
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
                                custom_fields = frontmatter.custom_fields;
                                completed_at = frontmatter.completed_at;
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
                            is_transferred,
                            transferred_to,
                            track_progress,
                            priority,
                            start_date,
                            due_date,
                            comment,
                            source_link,
                            tags,
                            checklist: std::vec::Vec::new(),
                            content: task_content,
                            path: entry.path().to_string_lossy().to_string(),
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

#[tauri::command]
fn archive_done_tasks(vault_path: String, days: u64) -> Result<u32, String> {
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


#[tauri::command]
async fn get_note_backlinks(vault_path: String, target_id: String) -> Result<Vec<NoteMetadata>, String> {
    use std::path::Path;
    use walkdir::WalkDir;

    let notes_dir = Path::new(&vault_path).join("notes");
    let mut backlinks = Vec::new();
    
    if !notes_dir.exists() {
        let notes_dir_cap = Path::new(&vault_path).join("Notes");
        if !notes_dir_cap.exists() {
            return Ok(backlinks);
        }
    }
    let active_dir = if notes_dir.exists() { notes_dir } else { Path::new(&vault_path).join("Notes") };

    for entry in WalkDir::new(&active_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "md") {
            if entry.path().file_name().unwrap_or_default() == target_id.as_str() {
                continue; 
            }
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
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
                    
                    backlinks.push(NoteMetadata {
                        id: entry.path().file_name().unwrap().to_string_lossy().to_string(),
                        path: entry.path().to_string_lossy().to_string(),
                        title,
                        date,
                        tags,
                        pinned,
                        summary
                    });
                }
            }
        }
    }
    Ok(backlinks)
}

#[tauri::command]
fn get_settings(vault_path: String) -> Result<FileManagerSettings, String> {
    let settings_path = Path::new(&vault_path).join(".synabit_fm_settings.json");
    if let Ok(content) = fs::read_to_string(settings_path) {
        if let Ok(settings) = serde_json::from_str(&content) {
            return Ok(settings);
        }
    }
    Ok(FileManagerSettings::default())
}

#[tauri::command]
fn save_settings(vault_path: String, settings: FileManagerSettings) -> Result<(), String> {
    let settings_path = Path::new(&vault_path).join(".synabit_fm_settings.json");
    let content = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    fs::write(settings_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_local_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

use std::collections::HashMap;

fn get_file_meta(vault_path: &str) -> HashMap<String, Vec<String>> {
    let meta_path = Path::new(vault_path).join(".synabit_fm_meta.json");
    if let Ok(content) = fs::read_to_string(meta_path) {
        if let Ok(meta) = serde_json::from_str(&content) {
            return meta;
        }
    }
    HashMap::new()
}

fn save_file_meta(vault_path: &str, meta: &HashMap<String, Vec<String>>) -> Result<(), String> {
    let meta_path = Path::new(vault_path).join(".synabit_fm_meta.json");
    let content = serde_json::to_string(meta).unwrap_or_default();
    fs::write(meta_path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_file_metadata(vault_path: String, absolute_path: String, new_filename: String, new_tags: Vec<String>) -> Result<String, String> {
    let mut meta = get_file_meta(&vault_path);
    let original_path = Path::new(&absolute_path);
    
    let current_filename = original_path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let mut final_path_str = absolute_path.clone();
    
    if current_filename != new_filename {
        if let Some(parent) = original_path.parent() {
            if parent.ends_with("assets") {
                return Err("Cannot rename files inside the 'assets' directory. You can only edit tags.".to_string());
            } else {
                let new_path = parent.join(&new_filename);
                if new_path.exists() {
                     return Err(format!("File '{}' already exists.", new_filename));
                }
                match fs::rename(&original_path, &new_path) {
                    Ok(_) => {
                        final_path_str = new_path.to_string_lossy().to_string();
                        meta.remove(&absolute_path);
                    },
                    Err(e) => return Err(e.to_string())
                }
            }
        }
    }
    
    meta.insert(final_path_str.clone(), new_tags);
    save_file_meta(&vault_path, &meta)?;
    
    Ok(final_path_str)
}

#[tauri::command]
fn reindex_sources(_vault_path: String) -> Result<(), String> {
    // Basic placeholder
    Ok(())
}

#[tauri::command]
fn get_file_items(vault_path: String) -> Result<Vec<FileItem>, String> {
    use walkdir::WalkDir;
    let mut items = Vec::new();
    let file_meta = get_file_meta(&vault_path);
    let folders_to_scan = ["assets", "files"];
    
    for folder_name in folders_to_scan.iter() {
        let dir_path = Path::new(&vault_path).join(folder_name);
        if dir_path.exists() {
            for entry in WalkDir::new(&dir_path).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let size = entry.metadata().map(|m| m.len() as f64 / 1024.0 / 1024.0).unwrap_or(0.0);
                    let ext = entry.path().extension().unwrap_or_default().to_string_lossy().to_string();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let absolute = entry.path().to_string_lossy().to_string();
                    let date = "Unknown".to_string();
                    let tags = file_meta.get(&absolute).cloned().unwrap_or_default();
                    
                    items.push(FileItem {
                        id: absolute.clone(),
                        name: name.clone(),
                        extension: ext,
                        size_mb: size,
                        source_folder: folder_name.to_string(),
                        date_modified: date,
                        absolute_path: absolute.clone(),
                        tags
                    });
                }
            }
        }
    }
    if let Ok(settings) = get_settings(vault_path.clone()) {
        for source in settings.tracked_sources {
            if Path::new(&source).exists() {
                for entry in WalkDir::new(&source).into_iter().filter_map(|e| e.ok()) {
                    if entry.file_type().is_file() {
                        let size = entry.metadata().map(|m| m.len() as f64 / 1024.0 / 1024.0).unwrap_or(0.0);
                        let ext = entry.path().extension().unwrap_or_default().to_string_lossy().to_string();
                        let name = entry.file_name().to_string_lossy().to_string();
                        let absolute = entry.path().to_string_lossy().to_string();
                        let tags = file_meta.get(&absolute).cloned().unwrap_or_default();
                        
                        items.push(FileItem {
                            id: absolute.clone(),
                            name: name.clone(),
                            extension: ext,
                            size_mb: size,
                            source_folder: source.clone(),
                            date_modified: "Unknown".to_string(),
                            absolute_path: absolute.clone(),
                            tags
                        });
                    }
                }
            }
        }
    }
    Ok(items)
}

#[tauri::command]
fn scan_events(vault_path: String) -> Result<Vec<EventMetadata>, String> {
    use std::path::Path;
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
                        let created = metadata.created().unwrap_or(metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH));
                        let created_date: chrono::DateTime<chrono::Local> = created.into();

                        events.push(EventMetadata {
                            id: entry.path().to_string_lossy().to_string(),
                            title,
                            event_date,
                            event_time,
                            location,
                            tags,
                            content: event_content,
                            path: entry.path().to_string_lossy().to_string(),
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
fn create_event(
    vault_path: String, metadata: EventFrontMatter, content: String
) -> Result<EventMetadata, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::path::Path;
    
    let events_dir = Path::new(&vault_path).join("Events");
    if !events_dir.exists() {
        fs::create_dir_all(&events_dir).map_err(|e| e.to_string())?;
    }
    
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time back").as_millis();
    let filename = format!("event-{}.md", timestamp);
    let path = events_dir.join(&filename);
    
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, &full_content).map_err(|e| e.to_string())?;
    
    let date: chrono::DateTime<chrono::Local> = SystemTime::now().into();
    let date_str = date.format("%Y-%m-%d %H:%M:%S").to_string();
    
    Ok(EventMetadata {
        id: path.to_string_lossy().to_string(),
        title: metadata.title,
        event_date: metadata.event_date,
        event_time: metadata.event_time,
        location: metadata.location,
        tags: metadata.tags,
        content,
        path: path.to_string_lossy().to_string(),
        created_at: date_str,
    })
}

#[tauri::command]
fn update_event(
    path: String, metadata: EventFrontMatter, content: String
) -> Result<(), String> {
    let yaml_string = serde_yaml::to_string(&metadata).map_err(|e| e.to_string())?;
    let full_content = format!("---\n{}\n---\n\n{}", yaml_string.trim(), content);
        
    fs::write(&path, full_content).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_event(path: String) -> Result<(), String> {
    std::fs::remove_file(&path).map_err(|e| e.to_string())
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
            spawn_note_window,
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
            get_nexus_stats,
            get_note_backlinks,
            get_file_items,
            get_settings,
            save_settings,
            open_local_file,
            update_file_metadata,
            reindex_sources,
            scan_events,
            create_event,
            update_event,
            delete_event,
            archive_done_tasks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
