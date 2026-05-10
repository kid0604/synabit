use std::collections::HashSet;
use std::path::Path;
use walkdir::WalkDir;

use crate::db::DbState;
use crate::error::AppResult;
use crate::path_utils;
use crate::utils::node_parser::parse_file_to_node;
use crate::utils::graph_parser::extract_node_edges;
use crate::models::node::NodeMetadata;

fn sync_node_to_search(db: &crate::db::DbBridge, node: &NodeMetadata) {
    let mut tags_str = String::new();
    let mut status = None;
    let mut props_search = serde_json::to_string(&node.properties).unwrap_or_default();
    
    if let Some(tags) = node.properties.get("tags").and_then(|v| v.as_array()) {
        let tags_vec: Vec<String> = tags.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
        tags_str = tags_vec.join(" ");
    }
    if let Some(s) = node.properties.get("status").and_then(|v| v.as_str()) {
        status = Some(s.to_string());
    }
    if let Some(p) = node.properties.get("priority").and_then(|v| v.as_str()) {
        props_search = format!("{} priority:{}", props_search, p);
    }
    
    db.upsert_search_entry(
        &node.id,
        &node.node_type,
        &node.title,
        &tags_str,
        &node.content,
        &props_search,
        status.as_deref(),
        &node.updated_at,
        &node.id
    );
}


#[tauri::command]
pub fn scan_all_nodes(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<()> {
    let base_dir = Path::new(&vault_path);
    if !base_dir.exists() {
        return Ok(());
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    
    // We will get all node timestamps to avoid re-parsing unchanged files.
    // However, for simplicity and since nodes table is new, let's just get all current nodes
    // and their timestamps.
    let existing_nodes = db.get_all_nodes()?;
    let mut existing_timestamps = std::collections::HashMap::new();
    for n in existing_nodes {
        existing_timestamps.insert(n.id, n.timestamp);
    }
    
    let mut current_disk_files = HashSet::new();

    for entry in WalkDir::new(base_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        
        // Skip hidden folders like .git, .Trash, and the assets folder
        if path.components().any(|c| {
            let name = c.as_os_str().to_string_lossy();
            name.starts_with('.') && name != "." || name == "assets" || name == "Files"
        }) {
            continue;
        }

        if entry.file_type().is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "md" || ext == "json" || ext == "canvas" {
                let rel_path = path_utils::to_relative(path, &vault_path);
                current_disk_files.insert(rel_path.clone());

                if let Ok(metadata) = entry.metadata() {
                    let modified = metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    let timestamp = modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;

                    let needs_update = match existing_timestamps.get(&rel_path) {
                        Some(&ts) => timestamp > ts,
                        None => true,
                    };

                    if needs_update {
                        if let Some(node) = parse_file_to_node(&vault_path, path) {
                            let _ = db.upsert_node(&node);
                            // Extract graph edges from node content and properties
                            let edges = extract_node_edges(&node);
                            let _ = db.update_edges(&node.id, edges);
                            
                            // Synchronize FTS5 search index
                            sync_node_to_search(&db, &node);
                        }
                    }
                }
            }
        }
    }
    
    // Cleanup deleted files from DB
    for id in existing_timestamps.keys() {
        if !current_disk_files.contains(id) {
            let _ = db.delete_node(id);
            // Delete edges
            let _ = db.delete_edges(id);
            // Synchronize FTS5 search index
            db.delete_search_entry(id);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn scan_specific_nodes(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String, paths: Vec<String>) -> AppResult<()> {
    let base_dir = Path::new(&vault_path);
    if !base_dir.exists() {
        return Ok(());
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    for rel_path in paths {
        // Validate path stays within vault
        let abs_path = match path_utils::resolve_safe_path(&vault_path, &rel_path) {
            Ok(p) => p,
            Err(_) => continue, // Skip invalid paths silently
        };
        
        if abs_path.exists() && abs_path.is_file() {
            if let Some(node) = parse_file_to_node(&vault_path, &abs_path) {
                let _ = db.upsert_node(&node);
                
                let edges = extract_node_edges(&node);
                let _ = db.update_edges(&node.id, edges);
                
                sync_node_to_search(&db, &node);
            }
        } else {
            // File was deleted
            let _ = db.delete_node(&rel_path);
            let _ = db.delete_edges(&rel_path);
            db.delete_search_entry(&rel_path);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn get_all_nodes(state: tauri::State<'_, DbState>) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_all_nodes()
}

#[tauri::command]
pub fn get_nodes(state: tauri::State<'_, DbState>, node_type: String) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_nodes_by_type(&node_type)
}

#[tauri::command]
pub fn get_linked_nodes(state: tauri::State<'_, DbState>, target_title: String, target_id: Option<String>) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let id_str = target_id.unwrap_or_default();
    db.get_linked_nodes(&target_title, &id_str)
}

#[tauri::command]
pub fn write_node_file(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
    title: String,
    node_type: String,
    properties: serde_json::Value,
    content: String,
) -> AppResult<()> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &rel_path)?;
    
    // Ensure directory exists
    if let Some(parent) = abs_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    // Query old title before writing to disk
    let old_title: Option<String> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_node_title(&rel_path)
    };
    
    // Construct the file content
    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let file_content = if ext == "json" || ext == "canvas" {
        // Output as pure JSON
        let json_obj = serde_json::json!({
            "title": title.clone(),
            "type": node_type.clone(),
            "metadata": properties.clone(),
            "content": content.clone()
        });
        serde_json::to_string_pretty(&json_obj).unwrap_or_default()
    } else {
        // Output as Markdown with YAML frontmatter
        let mut props_map = serde_yaml::Mapping::new();
        props_map.insert(serde_yaml::Value::String("title".to_string()), serde_yaml::Value::String(title.clone()));
        props_map.insert(serde_yaml::Value::String("type".to_string()), serde_yaml::Value::String(node_type.clone()));
        
        // Merge user properties
        if let serde_json::Value::Object(map) = &properties {
            for (k, v) in map {
                if k == "title" || k == "type" { continue; } // Skip standard fields
                if let Ok(yaml_val) = serde_yaml::to_value(v) {
                    props_map.insert(serde_yaml::Value::String(k.clone()), yaml_val);
                }
            }
        }
        
        let frontmatter = serde_yaml::to_string(&props_map).unwrap_or_default();
        // serde_yaml output usually ends with newline and might start with ---, but usually just standard YAML format
        // we manually add --- blocks to ensure Markdown compatibility
        let yaml_str = frontmatter.trim_start_matches("---\n");
        format!("---\n{}---\n{}", yaml_str, content)
    };
    
    // Write to disk
    std::fs::write(&abs_path, file_content)?;
    
    // Update DB immediately
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(node) = parse_file_to_node(&vault_path, &abs_path) {
        let _ = db.upsert_node(&node);
        
        // Sync FTS5 Search Index
        let tags = node.properties.get("tags")
            .and_then(|t| t.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
            .unwrap_or_default();
        let status = node.properties.get("status").and_then(|s| s.as_str());
        let props_str = serde_json::to_string(&node.properties).unwrap_or_default();
        
        db.upsert_search_entry(
            &node.id,
            &node.node_type,
            &node.title,
            &tags,
            &node.content,
            &props_str,
            status,
            &node.updated_at,
            &node.id,
        );

        let edges = extract_node_edges(&node);
        let _ = db.update_edges(&node.id, edges);
        if let Some(old) = old_title {
            if old != node.title {
                drop(db); // release lock before updating other files
                let _ = update_node_mentions(&state, vault_path, old, node.title, node.id);
            }
        }
    }
    
    Ok(())
}

fn update_node_mentions(
    state: &tauri::State<'_, DbState>,
    vault_path: String,
    old_title: String,
    new_title: String,
    node_id: String
) -> AppResult<()> {
    let linked_nodes = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_linked_nodes(&old_title, &node_id).unwrap_or_default()
    };

    let vault_dir = Path::new(&vault_path);

    for node in linked_nodes {
        let file_path = vault_dir.join(&node.id);
        if !file_path.exists() { continue; }

        if let Ok(content) = std::fs::read_to_string(&file_path) {
            let updated = crate::utils::graph_parser::rename_links_in_text(&content, &old_title, &new_title, Some(&node_id));
            if updated != content && std::fs::write(&file_path, updated).is_ok() {
                // Update DB synchronously for the linked file to avoid watcher race conditions
                if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &file_path) {
                    let db = state.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = db.upsert_node(&parsed_node);
                    
                    // Sync FTS5 Search Index
                    let tags = parsed_node.properties.get("tags")
                        .and_then(|t| t.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
                        .unwrap_or_default();
                    let status = parsed_node.properties.get("status").and_then(|s| s.as_str());
                    let props_str = serde_json::to_string(&parsed_node.properties).unwrap_or_default();
                    
                    db.upsert_search_entry(
                        &parsed_node.id,
                        &parsed_node.node_type,
                        &parsed_node.title,
                        &tags,
                        &parsed_node.content,
                        &props_str,
                        status,
                        &parsed_node.updated_at,
                        &parsed_node.id,
                    );

                    let edges = crate::utils::graph_parser::extract_node_edges(&parsed_node);
                    let _ = db.update_edges(&parsed_node.id, edges);
                }
            }
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn delete_node_file(state: tauri::State<'_, DbState>, vault_path: String, rel_path: String) -> AppResult<()> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &rel_path)?;
    
    if abs_path.exists() {
        std::fs::remove_file(abs_path)?;
    }
    
    // Update DB immediately
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let _ = db.delete_node(&rel_path);
    let _ = db.delete_edges(&rel_path);
    db.delete_search_entry(&rel_path);
    
    Ok(())
}

#[tauri::command]
pub fn rename_node_file(state: tauri::State<'_, DbState>, vault_path: String, old_rel_path: String, new_name: String) -> AppResult<String> {
    let old_abs = path_utils::resolve_safe_path(&vault_path, &old_rel_path)?;
    
    if !old_abs.exists() {
        return Err(crate::error::AppError::InvalidPath("File not found.".to_string()));
    }
    
    // Parse the current node
    let node = if let Some(n) = crate::utils::node_parser::parse_file_to_node(&vault_path, &old_abs) {
        n
    } else {
        return Err(crate::error::AppError::InvalidPath("Failed to parse node.".to_string()));
    };
    
    let old_title = node.title.clone();
    
    // Update the title property and rewrite the file
    let mut props_map = serde_yaml::Mapping::new();
    props_map.insert(serde_yaml::Value::String("title".to_string()), serde_yaml::Value::String(new_name.clone()));
    props_map.insert(serde_yaml::Value::String("type".to_string()), serde_yaml::Value::String(node.node_type.clone()));
    
    if let serde_json::Value::Object(map) = &node.properties {
        for (k, v) in map {
            if k == "title" || k == "type" { continue; }
            if let Ok(yaml_val) = serde_yaml::to_value(v) {
                props_map.insert(serde_yaml::Value::String(k.clone()), yaml_val);
            }
        }
    }
    
    let frontmatter = serde_yaml::to_string(&props_map).unwrap_or_default();
    let yaml_str = frontmatter.trim_start_matches("---\n");
    let file_content = format!("---\n{}---\n{}", yaml_str, node.content);
    
    std::fs::write(&old_abs, file_content)?;
    
    // Update DB and Mentions
    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        
        if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &old_abs) {
            let _ = db.upsert_node(&parsed_node);
            let tags = parsed_node.properties.get("tags")
                .and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
                .unwrap_or_default();
            let status = parsed_node.properties.get("status").and_then(|s| s.as_str());
            let props_str = serde_json::to_string(&parsed_node.properties).unwrap_or_default();
            
            db.upsert_search_entry(
                &parsed_node.id,
                &parsed_node.node_type,
                &parsed_node.title,
                &tags,
                &parsed_node.content,
                &props_str,
                status,
                &parsed_node.updated_at,
                &parsed_node.id,
            );
            let edges = crate::utils::graph_parser::extract_node_edges(&parsed_node);
            let _ = db.update_edges(&parsed_node.id, edges);
            
            if old_title != new_name {
                drop(db); // release lock
                let _ = update_node_mentions(&state, vault_path, old_title, new_name, parsed_node.id);
            }
        }
    }
    
    Ok(old_rel_path)
}

#[tauri::command]
pub fn create_node_file(state: tauri::State<'_, DbState>, vault_path: String, directory: String, node_type: String, date_format: Option<String>) -> AppResult<String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dir_path = path_utils::resolve_safe_path(&vault_path, &directory)?;
    if !dir_path.exists() {
        std::fs::create_dir_all(&dir_path)?;
    }
    
    let title = if let Some(fmt_str) = date_format {
        let chrono_format = fmt_str
            .replace("YYYY", "%Y")
            .replace("YY", "%y")
            .replace("MM", "%m")
            .replace("M", "%-m")
            .replace("DD", "%d")
            .replace("D", "%-d");
        chrono::Local::now().format(&chrono_format).to_string()
    } else {
        let start = SystemTime::now();
        let timestamp = start.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
        format!("Untitled {}", timestamp)
    };
    
    let filename = format!("{}.md", uuid::Uuid::new_v4());
    
    let path = dir_path.join(&filename);
    
    if !path.exists() {
        let content = format!("---\ntitle: \"{}\"\ntype: \"{}\"\n---\n\n", title, node_type);
        std::fs::write(&path, content)?;
        
        // Sync DB immediately
        if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &path) {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            let _ = db.upsert_node(&parsed_node);
            
            let tags = parsed_node.properties.get("tags")
                .and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
                .unwrap_or_default();
            let status = parsed_node.properties.get("status").and_then(|s| s.as_str());
            let props_str = serde_json::to_string(&parsed_node.properties).unwrap_or_default();
            
            db.upsert_search_entry(
                &parsed_node.id,
                &parsed_node.node_type,
                &parsed_node.title,
                &tags,
                &parsed_node.content,
                &props_str,
                status,
                &parsed_node.updated_at,
                &parsed_node.id,
            );
        }
    }
    
    let rel_path = path_utils::to_relative(&path, &vault_path);
    Ok(rel_path)
}

#[tauri::command]
pub fn open_daily_note(state: tauri::State<'_, DbState>, vault_path: String, format_str: String, tag: String) -> AppResult<String> {
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        std::fs::create_dir_all(&notes_dir)?;
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
    
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let notes = db.get_nodes_by_type("note").unwrap_or_default();
    
    if let Some(existing) = notes.iter().find(|n| n.title == date_str) {
        return Ok(existing.id.clone());
    }

    let filename = format!("{}.md", uuid::Uuid::new_v4());
    let path = notes_dir.join(&filename);

    let title = date_str.clone();
    let content = if tag.trim().is_empty() {
        format!("---\ntitle: \"{}\"\ntype: \"note\"\n---\n\n", title)
    } else {
        format!("---\ntitle: \"{}\"\ntype: \"note\"\ntags:\n  - {}\n---\n\n", title, tag.trim())
    };
    std::fs::write(&path, content)?;
        
    // Sync DB immediately to avoid race condition with frontend scanVault
    if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &path) {
        let _ = db.upsert_node(&parsed_node);
            
            let tags = parsed_node.properties.get("tags")
                .and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
                .unwrap_or_default();
            let status = parsed_node.properties.get("status").and_then(|s| s.as_str());
            let props_str = serde_json::to_string(&parsed_node.properties).unwrap_or_default();
            
            db.upsert_search_entry(
                &parsed_node.id,
                &parsed_node.node_type,
                &parsed_node.title,
                &tags,
                &parsed_node.content,
                &props_str,
                status,
                &parsed_node.updated_at,
                &parsed_node.id,
            );
        }

    let rel_path = path_utils::to_relative(&path, &vault_path);
    Ok(rel_path)
}

#[tauri::command]
pub fn migrate_events_to_nodes(state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<()> {
    use gray_matter::Matter;
    use gray_matter::engine::YAML;
    
    let events_dir = Path::new(&vault_path).join("Events");
    if !events_dir.exists() {
        return Ok(());
    }

    let matter = Matter::<YAML>::new();

    for entry in WalkDir::new(&events_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(parsed) = matter.parse::<serde_yaml::Value>(&content) {
                            let mut frontmatter_map = serde_yaml::Mapping::new();
                            
                            if let Some(serde_yaml::Value::Mapping(map)) = parsed.data {
                                frontmatter_map = map.clone();
                                
                                // if it's already type: event, skip it!
                                if let Some(serde_yaml::Value::String(s)) = frontmatter_map.get(serde_yaml::Value::String("type".to_string())) {
                                    if s == "event" {
                                        continue;
                                    }
                                }
                            }
                            
                            // Inject type
                            frontmatter_map.insert(serde_yaml::Value::String("type".to_string()), serde_yaml::Value::String("event".to_string()));
                            
                            let new_yaml = serde_yaml::to_string(&frontmatter_map).unwrap_or_default();
                            let yaml_str = new_yaml.trim_start_matches("---\n");
                            let file_content = format!("---\n{}---\n{}", yaml_str, parsed.content);
                            
                            if std::fs::write(entry.path(), file_content).is_ok() {
                                if let Some(node) = parse_file_to_node(&vault_path, entry.path()) {
                                    let db = state.lock().unwrap_or_else(|e| e.into_inner());
                                    let _ = db.upsert_node(&node);
                                    let edges = extract_node_edges(&node);
                                    let _ = db.update_edges(&node.id, edges);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn migrate_notes_to_nodes(state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<()> {
    use gray_matter::Matter;
    use gray_matter::engine::YAML;
    
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        return Ok(());
    }

    let matter = Matter::<YAML>::new();

    for entry in WalkDir::new(&notes_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(parsed) = matter.parse::<serde_yaml::Value>(&content) {
                            let mut frontmatter_map = serde_yaml::Mapping::new();
                            
                            if let Some(serde_yaml::Value::Mapping(map)) = parsed.data {
                                frontmatter_map = map.clone();
                                
                                // if it's already type: note, skip it!
                                if let Some(serde_yaml::Value::String(s)) = frontmatter_map.get(serde_yaml::Value::String("type".to_string())) {
                                    if s == "note" {
                                        continue;
                                    }
                                }
                            }
                            
                            // Inject type
                            frontmatter_map.insert(serde_yaml::Value::String("type".to_string()), serde_yaml::Value::String("note".to_string()));
                            
                            let new_yaml = serde_yaml::to_string(&frontmatter_map).unwrap_or_default();
                            let yaml_str = new_yaml.trim_start_matches("---\n");
                            let file_content = format!("---\n{}---\n{}", yaml_str, parsed.content);
                            
                            if std::fs::write(entry.path(), file_content).is_ok() {
                                if let Some(node) = parse_file_to_node(&vault_path, entry.path()) {
                                    let db = state.lock().unwrap_or_else(|e| e.into_inner());
                                    let _ = db.upsert_node(&node);
                                    let edges = extract_node_edges(&node);
                                    let _ = db.update_edges(&node.id, edges);
                                    
                                    // Update search index manually
                                    let tags = node.properties.get("tags")
                                        .and_then(|t| t.as_array())
                                        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
                                        .unwrap_or_default();
                                    let status = node.properties.get("status").and_then(|s| s.as_str());
                                    let props_str = serde_json::to_string(&node.properties).unwrap_or_default();
                                    
                                    db.upsert_search_entry(
                                        &node.id,
                                        &node.node_type,
                                        &node.title,
                                        &tags,
                                        &node.content,
                                        &props_str,
                                        status,
                                        &node.created_at,
                                        &node.id
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn migrate_tasks_to_nodes(state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<()> {
    let tasks_dir = Path::new(&vault_path).join("Tasks");
    if !tasks_dir.exists() {
        return Ok(());
    }
    
    let archived_dir = tasks_dir.join("archived");
    let matter = gray_matter::Matter::<gray_matter::engine::YAML>::new();
    
    for entry in walkdir::WalkDir::new(&tasks_dir).into_iter().filter_map(|e| e.ok()) {
        // Skip archived dir
        if entry.path().starts_with(&archived_dir) {
            continue;
        }
        
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Ok(parsed) = matter.parse::<serde_yaml::Value>(&content) {
                            let mut frontmatter_map = serde_yaml::Mapping::new();
                            
                            if let Some(serde_yaml::Value::Mapping(map)) = parsed.data {
                                frontmatter_map = map.clone();
                                
                                // if it's already type: task, skip it
                                if let Some(serde_yaml::Value::String(s)) = frontmatter_map.get(serde_yaml::Value::String("type".to_string())) {
                                    if s == "task" {
                                        continue;
                                    }
                                }
                            }
                            
                            // Inject type
                            frontmatter_map.insert(serde_yaml::Value::String("type".to_string()), serde_yaml::Value::String("task".to_string()));
                            
                            let new_yaml = serde_yaml::to_string(&frontmatter_map).unwrap_or_default();
                            let yaml_str = new_yaml.trim_start_matches("---\n");
                            let file_content = format!("---\n{}---\n{}", yaml_str, parsed.content);
                            
                            if std::fs::write(entry.path(), file_content).is_ok() {
                                if let Some(node) = parse_file_to_node(&vault_path, entry.path()) {
                                    let db = state.lock().unwrap_or_else(|e| e.into_inner());
                                    let _ = db.upsert_node(&node);
                                    let edges = extract_node_edges(&node);
                                    let _ = db.update_edges(&node.id, edges);
                                    
                                    // Update search index manually
                                    let tags = node.properties.get("tags")
                                        .and_then(|t| t.as_array())
                                        .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
                                        .unwrap_or_default();
                                    let status = node.properties.get("status").and_then(|s| s.as_str());
                                    let props_str = serde_json::to_string(&node.properties).unwrap_or_default();
                                    
                                    db.upsert_search_entry(
                                        &node.id,
                                        &node.node_type,
                                        &node.title,
                                        &tags,
                                        &node.content,
                                        &props_str,
                                        status,
                                        &node.created_at,
                                        &node.id
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn archive_done_nodes(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String, node_type: String, days: u64) -> AppResult<u32> {
    use chrono::NaiveDate;
    
    // Map node_type to its default directory name
    let dir_name = match node_type.as_str() {
        "task" => "Tasks",
        "event" => "Events",
        "note" => "Notes",
        _ => return Ok(0),
    };
    
    let base_dir = Path::new(&vault_path).join(dir_name);
    if !base_dir.exists() {
        return Ok(0);
    }
    
    let archived_dir = base_dir.join("archived");
    let today = chrono::Local::now().date_naive();
    let mut archived_count: u32 = 0;
    
    // We only process items in DB for that type
    let nodes = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_type(&node_type)?
    };
    
    for node in nodes {
        // Node must be "done"
        if let Some(status) = node.properties.get("status").and_then(|s| s.as_str()) {
            if status != "done" {
                continue;
            }
            
            // Node must have completed_at
            if let Some(completed_at) = node.properties.get("completed_at").and_then(|c| c.as_str()) {
                if completed_at.is_empty() {
                    continue;
                }
                
                let date_part = completed_at.split_whitespace().next().unwrap_or("");
                if let Ok(completed_date) = NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                    let elapsed = today.signed_duration_since(completed_date).num_days();
                    if elapsed >= days as i64 {
                        let abs_path = Path::new(&vault_path).join(&node.id);
                        if abs_path.exists() {
                            if !archived_dir.exists() {
                                let _ = std::fs::create_dir_all(&archived_dir);
                            }
                            let file_name = abs_path.file_name().unwrap_or_default();
                            let dest = archived_dir.join(file_name);
                            if std::fs::rename(&abs_path, &dest).is_ok() {
                                archived_count += 1;
                                
                                // Remove old path from DB and index
                                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                                let _ = db.delete_node(&node.id);
                                let _ = db.delete_edges(&node.id);
                                db.delete_search_entry(&node.id);
                                
                                // The new file will be picked up by the next scan_all_nodes if it's not excluded
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(archived_count)
}

#[tauri::command]
pub fn migrate_quickcaps_to_nodes(state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<()> {
    use gray_matter::Matter;
    use gray_matter::engine::YAML;
    
    let qc_dir = Path::new(&vault_path).join("QuickCaps");
    if !qc_dir.exists() {
        return Ok(());
    }

    let mut matter = Matter::<YAML>::new();
    matter.delimiter = "---".to_string();

    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    for entry in std::fs::read_dir(&qc_dir)?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.starts_with("---") && content.contains("type: quickcap") {
                    continue;
                }

                let mut body = content.clone();
                let mut color = String::new();

                if let Some(start) = body.find("<!--color:") {
                    if let Some(end) = body[start..].find("-->") {
                        color = body[start + 10..start + end].to_string();
                        body.replace_range(start..start + end + 3, "");
                    }
                }
                
                body = body.trim().to_string();

                let mut props = serde_yaml::Mapping::new();
                props.insert(serde_yaml::Value::String("type".to_string()), serde_yaml::Value::String("quickcap".to_string()));
                if !color.is_empty() {
                    props.insert(serde_yaml::Value::String("color".to_string()), serde_yaml::Value::String(color));
                }

                let frontmatter = serde_yaml::to_string(&props).unwrap_or_default();
                let yaml_str = frontmatter.trim_start_matches("---\n");
                let new_content = format!("---\n{}---\n{}", yaml_str, body);

                if std::fs::write(&path, new_content).is_ok() {
                    if let Some(node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &path) {
                        let _ = db.upsert_node(&node);
                        let tags = node.properties.get("tags")
                            .and_then(|t| t.as_array())
                            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<&str>>().join(" "))
                            .unwrap_or_default();
                        let props_str = serde_json::to_string(&node.properties).unwrap_or_default();
                        
                        db.upsert_search_entry(
                            &node.id,
                            &node.node_type,
                            &node.title,
                            &tags,
                            &node.content,
                            &props_str,
                            None,
                            &node.updated_at,
                            &node.id,
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn save_asset(vault_path: String, filename: String, bytes: Vec<u8>) -> AppResult<String> {
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        std::fs::create_dir_all(&assets_dir)?;
    }
    
    let extension = Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let safe_filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let target_path = assets_dir.join(&safe_filename);
    
    std::fs::write(&target_path, bytes)?;
    Ok(format!("assets/{}", safe_filename))
}

#[tauri::command]
pub fn copy_asset_to_vault(vault_path: String, source_path: String) -> AppResult<String> {
    let source = Path::new(&source_path);
    if !source.exists() || !source.is_file() {
        return Err(crate::error::AppError::InvalidPath("Source file does not exist or is not a regular file".to_string()));
    }
    // Validate the output stays within vault
    path_utils::resolve_safe_path(&vault_path, "assets")?;
    
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        std::fs::create_dir_all(&assets_dir)?;
    }
    
    let extension = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let target = assets_dir.join(&filename);
    
    std::fs::copy(source, target)?;
    
    Ok(format!("assets/{}", filename))
}

#[cfg(desktop)]
#[tauri::command]
pub fn spawn_node_window(app_handle: tauri::AppHandle, node_id: String) -> AppResult<()> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    use std::time::{SystemTime, UNIX_EPOCH};
    let encoded_node_id = urlencoding::encode(&node_id);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| crate::error::AppError::General(format!("System time error: {}", e)))?
        .as_micros();
    let window_label = format!("node_{}", timestamp);
    
    let url = WebviewUrl::App(format!("index.html?floatingNote={}", encoded_node_id).into());

    let _ = WebviewWindowBuilder::new(&app_handle, window_label, url)
        .title("Node View")
        .inner_size(600.0, 700.0)
        .minimizable(true)
        .maximizable(true)
        .closable(true)
        .build()
        .map_err(|e| crate::error::AppError::General(e.to_string()))?;

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn spawn_node_window(_app_handle: tauri::AppHandle, _node_id: String) -> AppResult<()> {
    Err(crate::error::AppError::General("Multiple windows are not supported on mobile".to_string()))
}
