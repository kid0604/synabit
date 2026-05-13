use std::collections::HashSet;
use std::path::Path;
use walkdir::WalkDir;

use crate::db::DbState;
use crate::error::AppResult;
use crate::path_utils;
use crate::utils::node_parser::parse_file_to_node;
use crate::utils::graph_parser::{NodeResolver, extract_resolved_node_edges};
use crate::models::node::NodeMetadata;
/// Helper: extract and sync node_edges for a node.
fn sync_node_edges(db: &crate::db::DbBridge, node: &NodeMetadata, resolver: &NodeResolver) {
    let _ = db.delete_node_edges_by_source(&node.id);
    let edges = extract_resolved_node_edges(node, resolver);
    for edge in edges {
        let _ = db.upsert_node_edge(&edge);
    }
}

/// Helper: delete node_edges for a source
fn delete_node_edges_for(db: &crate::db::DbBridge, source_id: &str) {
    let _ = db.delete_node_edges_by_source(source_id);
}

/// Build a NodeResolver from all nodes in the DB
fn build_resolver(db: &crate::db::DbBridge) -> NodeResolver {
    let all_nodes = db.get_all_nodes().unwrap_or_default();
    NodeResolver::new(&all_nodes)
}
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

    if let Some(blocks) = node.blocks.clone() {
        let _ = db.upsert_node_blocks(&node.id, blocks);
    }
}


#[tauri::command]
pub fn scan_all_nodes(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, vault_path: String) -> AppResult<()> {
    let base_dir = Path::new(&vault_path);
    if !base_dir.exists() {
        return Ok(());
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    
    // We will get all node timestamps to avoid re-parsing unchanged files.
    let existing_nodes = db.get_all_nodes()?;
    let mut existing_timestamps = std::collections::HashMap::new();
    for n in &existing_nodes {
        existing_timestamps.insert(n.id.clone(), n.timestamp);
    }
    
    // Build resolver once for all nodes (O(N) setup, O(1) per resolve)
    let resolver = NodeResolver::new(&existing_nodes);
    
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
                            sync_node_edges(&db, &node, &resolver);
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
            delete_node_edges_for(&db, id);
            let _ = db.delete_node_blocks(id);
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
    let resolver = build_resolver(&db);

    for rel_path in paths {
        // Validate path stays within vault
        let abs_path = match path_utils::resolve_safe_path(&vault_path, &rel_path) {
            Ok(p) => p,
            Err(_) => continue, // Skip invalid paths silently
        };
        
        if abs_path.exists() && abs_path.is_file() {
            if let Some(node) = parse_file_to_node(&vault_path, &abs_path) {
                let _ = db.upsert_node(&node);
                sync_node_edges(&db, &node, &resolver);
                sync_node_to_search(&db, &node);
            }
        } else {
            // File was deleted
            let _ = db.delete_node(&rel_path);
            delete_node_edges_for(&db, &rel_path);
            let _ = db.delete_node_blocks(&rel_path);
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
pub fn get_node_block(state: tauri::State<'_, DbState>, node_id: String, block_id: String) -> AppResult<Option<String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    
    // Get node content from DB
    let nodes = db.get_all_nodes()?;
    let node = match nodes.into_iter().find(|n| n.id == node_id) {
        Some(n) => n,
        None => return Ok(None),
    };
    
    // Scan content for the line containing ^block_id marker
    let marker = format!(" ^{}", block_id);
    let re = block_id_regex();
    
    for line in node.content.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with(&marker) {
            // Return line content WITHOUT the ^id marker
            let clean = re.replace(trimmed, "").to_string();
            // Also strip frontmatter-style prefixes like "# ", "## ", etc.
            return Ok(Some(clean.trim().to_string()));
        }
    }
    
    Ok(None) // Block marker was deleted from source
}

/// Returned by get_node_headings for each parseable block in a note.
#[derive(serde::Serialize)]
pub struct BlockPreview {
    pub block_id: String,
    pub content_preview: String,
    pub raw_content: String,        // Full original line text for file matching
    pub block_type: String,         // "h1", "h2", "h3", "paragraph"
    pub has_persistent_id: bool,    // true if ^id already exists in file
}

/// Generate a 6-char lowercase alphanumeric block ID
fn generate_block_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..6).map(|_| {
        let idx = rng.random_range(0..36u32);
        if idx < 10 { (b'0' + idx as u8) as char } else { (b'a' + (idx - 10) as u8) as char }
    }).collect()
}

/// Helper: find safe char boundary at or before byte index (UTF-8 safe)
fn safe_split(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() { return s; }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Regex pattern for ^block-id markers at end of line
fn block_id_regex() -> regex::Regex {
    regex::Regex::new(r" \^([a-z0-9]{6})$").unwrap()
}

/// Parse note content into block previews, detecting existing ^id markers.
fn parse_blocks_from_content(content: &str) -> Vec<BlockPreview> {
    // Strip frontmatter
    let body = if content.starts_with("---") {
        if let Some(end_pos) = content[3..].find("---") {
            let skip = 3 + end_pos + 3;
            if skip <= content.len() {
                content[skip..].trim()
            } else {
                content
            }
        } else {
            content
        }
    } else {
        content
    };

    let re = block_id_regex();
    let mut blocks = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for existing ^id marker
        let (clean_text, existing_id) = if let Some(caps) = re.captures(trimmed) {
            let id = caps[1].to_string();
            let text_end = caps.get(0).unwrap().start();
            (trimmed[..text_end].to_string(), Some(id))
        } else {
            (trimmed.to_string(), None)
        };

        // Determine block type
        let (block_type, preview) = if let Some(rest) = clean_text.strip_prefix("### ") {
            ("h3".to_string(), rest.trim().to_string())
        } else if let Some(rest) = clean_text.strip_prefix("## ") {
            ("h2".to_string(), rest.trim().to_string())
        } else if let Some(rest) = clean_text.strip_prefix("# ") {
            ("h1".to_string(), rest.trim().to_string())
        } else if !clean_text.starts_with("- ")
            && !clean_text.starts_with("* ")
            && !clean_text.starts_with("> ")
            && !clean_text.starts_with("```")
            && !clean_text.starts_with("|")
        {
            let preview = if clean_text.len() > 120 {
                format!("{}…", safe_split(&clean_text, 120))
            } else {
                clean_text.clone()
            };
            ("paragraph".to_string(), preview)
        } else {
            continue;
        };

        // Use existing ^id if present, otherwise use a content hash as temporary display ID
        let has_persistent_id = existing_id.is_some();
        let block_id = existing_id.unwrap_or_else(|| {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(clean_text.trim().as_bytes());
            let result = hasher.finalize();
            format!("{:02x}{:02x}{:02x}{:02x}", result[0], result[1], result[2], result[3])
        });

        blocks.push(BlockPreview {
            block_id,
            content_preview: preview,
            raw_content: clean_text,
            block_type,
            has_persistent_id,
        });
    }

    blocks
}

#[tauri::command]
pub fn get_node_headings(state: tauri::State<'_, DbState>, node_id: String) -> AppResult<Vec<BlockPreview>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // Get node content from DB
    let nodes = db.get_all_nodes()?;
    let node = nodes.into_iter().find(|n| n.id == node_id);
    let node = match node {
        Some(n) => n,
        None => return Ok(vec![]),
    };

    Ok(parse_blocks_from_content(&node.content))
}

#[tauri::command]
pub fn create_block_reference(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_id: String,
    content_snippet: String,
) -> AppResult<String> {
    let block_id = generate_block_id();
    let marker = format!(" ^{}", block_id);

    // Read the source file
    let abs_path = path_utils::resolve_safe_path(&vault_path, &node_id)?;
    let file_content = std::fs::read_to_string(&abs_path)
        .map_err(|e| crate::error::AppError::General(format!("Failed to read file: {}", e)))?;

    // Find the line matching content_snippet and append ^id
    let snippet_trimmed = content_snippet.trim();
    let mut found = false;
    let mut updated_lines: Vec<String> = Vec::new();

    for line in file_content.lines() {
        if !found && line.trim().contains(snippet_trimmed) {
            // Check if this line already has a ^id — don't add another
            let re = block_id_regex();
            if let Some(caps) = re.captures(line.trim()) {
                // Already has ^id, return existing one
                let existing_id = caps[1].to_string();
                return Ok(existing_id);
            }
            updated_lines.push(format!("{}{}", line, marker));
            found = true;
        } else {
            updated_lines.push(line.to_string());
        }
    }

    if !found {
        return Err(crate::error::AppError::General(
            "Content snippet not found in source file".to_string(),
        ));
    }

    // Write back to disk
    let new_content = updated_lines.join("\n");
    std::fs::write(&abs_path, &new_content)
        .map_err(|e| crate::error::AppError::General(format!("Failed to write file: {}", e)))?;

    // Update DB with new file content
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(node) = parse_file_to_node(&vault_path, &abs_path) {
        let _ = db.upsert_node(&node);
    }

    Ok(block_id)
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

        let resolver = build_resolver(&db);
        sync_node_edges(&db, &node, &resolver);
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

                    let resolver = build_resolver(&db);
                    sync_node_edges(&db, &parsed_node, &resolver);
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
    delete_node_edges_for(&db, &rel_path);
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
            let resolver = build_resolver(&db);
            sync_node_edges(&db, &parsed_node, &resolver);
            
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
                                    let resolver = build_resolver(&db);
                                    sync_node_edges(&db, &node, &resolver);
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
                                    let resolver = build_resolver(&db);
                                    sync_node_edges(&db, &node, &resolver);
                                    
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
                                    let resolver = build_resolver(&db);
                                    sync_node_edges(&db, &node, &resolver);
                                    
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
                                delete_node_edges_for(&db, &node.id);
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

/// List all PDF files in the vault's assets/ directory.
#[tauri::command]
pub fn list_pdf_files(vault_path: String) -> AppResult<Vec<serde_json::Value>> {
    let assets_dir = Path::new(&vault_path).join("assets");
    let mut pdfs = Vec::new();

    if !assets_dir.exists() {
        return Ok(pdfs);
    }

    for entry in WalkDir::new(&assets_dir).max_depth(2).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("pdf") {
            let rel_path = path_utils::to_relative(path, &vault_path);
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let name = filename.strip_suffix(".pdf")
                .or_else(|| filename.strip_suffix(".PDF"))
                .unwrap_or(&filename)
                .to_string();

            pdfs.push(serde_json::json!({
                "name": name,
                "path": rel_path
            }));
        }
    }

    pdfs.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.cmp(nb)
    });

    Ok(pdfs)
}

#[tauri::command]
pub fn migrate_files_to_nodes(state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<u32> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // Check if already migrated
    if let Ok(Some(v)) = db.get_kv("files_migrated_to_nodes") {
        if v == "true" { return Ok(0); }
    }

    // Read all files from the legacy files table
    let files = db.get_all_files().unwrap_or_default();
    let mut count: u32 = 0;

    for file in &files {
        let node = file.to_node();
        if db.upsert_node(&node).is_ok() {
            count += 1;
        }
    }

    // Mark migration as done
    let _ = db.set_kv("files_migrated_to_nodes", "true");

    Ok(count)
}

/// Migrate legacy graph_edges to new node_edges table.
/// Resolves target_title_or_path → target_id using NodeResolver.
/// Skips tags (already in properties). Unresolved targets become ghost nodes.
#[tauri::command]
pub fn migrate_graph_edges(state: tauri::State<'_, DbState>) -> AppResult<u32> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // Check if already migrated
    if let Ok(Some(v)) = db.get_kv("edges_migrated_to_node_edges") {
        if v == "true" {
            return Ok(0);
        }
    }

    let old_edges = db.get_all_edges()?;
    let all_nodes = db.get_all_nodes()?;
    let resolver = NodeResolver::new(&all_nodes);
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut count: u32 = 0;
    let mut seen = std::collections::HashSet::new();

    for edge in old_edges {
        // Skip tags — they live in node properties
        if edge.link_type == "tag" { continue; }

        let target_id = resolver.resolve(&edge.target_title_or_path, &edge.link_type);

        // Skip self-links
        if target_id == edge.source_id { continue; }

        let edge_type = match edge.link_type.as_str() {
            "wikilink" => "wikilink",
            "internal_link" => "internal_link",
            _ => "internal_link",
        };

        let dedup_key = format!("{}-{}-{}", edge.source_id, target_id, edge_type);
        if !seen.insert(dedup_key) { continue; }

        let new_edge = crate::db::NodeEdge {
            id: uuid::Uuid::new_v4().to_string(),
            source_id: edge.source_id,
            target_id,
            edge_type: edge_type.to_string(),
            relation: None,
            created_at: now.clone(),
        };

        if db.upsert_node_edge(&new_edge).is_ok() {
            count += 1;
        }
    }

    let _ = db.set_kv("edges_migrated_to_node_edges", "true");
    Ok(count)
}
