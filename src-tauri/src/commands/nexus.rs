use std::collections::HashMap;
use crate::models::nexus::NexusItem;
use crate::db::DbState;
use crate::error::AppResult;
use serde::Serialize;

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
}

#[derive(Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}


#[tauri::command]
pub fn get_nexus_items(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<Vec<NexusItem>> {
    let mut items = Vec::new();

    // ─── Query indexed data from SQLite (fast) ─────────────
    { let db = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(rows) = db.get_all_nexus_items() {
            for r in rows {
                let title = if r.title.is_empty() {
                    match r.item_type.as_str() {
                        "note" => "Untitled Note".to_string(),
                        "task" => "Untitled Task".to_string(),
                        _ => r.title,
                    }
                } else { r.title };

                items.push(NexusItem {
                    id: r.id,
                    item_type: r.item_type,
                    title: title.clone(),
                    preview: r.preview,
                    tags: r.tags,
                    date: r.date,
                    path: r.path,
                    content: format!("{} {}", title, r.content),
                    status: r.status,
                });
            }
        }
    }
    
    items.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(items)
}

#[tauri::command]
pub fn get_nexus_item(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String, id: String) -> AppResult<NexusItem> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // Fast path: targeted single-table query by ID prefix
    if let Some(r) = db.get_nexus_item_by_id(&id)? {
        let title = if r.title.is_empty() {
            match r.item_type.as_str() {
                "note" => "Untitled Note".to_string(),
                "task" => "Untitled Task".to_string(),
                _ => r.title,
            }
        } else { r.title };

        return Ok(NexusItem {
            id: r.id,
            item_type: r.item_type,
            title: title.clone(),
            preview: r.preview,
            tags: r.tags,
            date: r.date,
            path: r.path,
            content: format!("{} {}", title, r.content),
            status: r.status,
        });
    }

    // Fallback: full scan (handles edge cases like unexpected ID formats)
    let rows = db.get_all_nexus_items()?;
    for r in rows {
        if r.id == id {
            let title = if r.title.is_empty() {
                match r.item_type.as_str() {
                    "note" => "Untitled Note".to_string(),
                    "task" => "Untitled Task".to_string(),
                    _ => r.title,
                }
            } else { r.title };

            return Ok(NexusItem {
                id: r.id,
                item_type: r.item_type,
                title: title.clone(),
                preview: r.preview,
                tags: r.tags,
                date: r.date,
                path: r.path,
                content: format!("{} {}", title, r.content),
                status: r.status,
            });
        }
    }
    Err(crate::error::AppError::General("Item not found".to_string()))
}

#[tauri::command]
pub fn search_nexus(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String, query: String) -> AppResult<Vec<NexusItem>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut all_items = Vec::new();
    if let Ok(rows) = db.get_all_nexus_items() {
        for r in rows {
            let title = if r.title.is_empty() {
                match r.item_type.as_str() {
                    "note" => "Untitled Note".to_string(),
                    "task" => "Untitled Task".to_string(),
                    _ => r.title,
                }
            } else { r.title };
            all_items.push(NexusItem {
                id: r.id, item_type: r.item_type,
                title: title.clone(), preview: r.preview, tags: r.tags,
                date: r.date, path: r.path,
                content: format!("{} {}", title, r.content),
                status: r.status,
            });
        }
    }
    drop(db); // Release lock before filtering
    all_items.sort_by(|a, b| b.date.cmp(&a.date));
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
pub fn get_nexus_graph_data(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<GraphData> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let items = db.get_all_nexus_items()?;
    let edges = db.get_all_edges()?;

    let mut nodes = Vec::new();
    let mut links = Vec::new();
    
    let mut node_map = HashMap::new();
    let mut title_map = HashMap::new();
    let mut path_map = HashMap::new();
    let mut tag_nodes = HashMap::new();
    let mut ghost_nodes = HashMap::new();
    let mut added_links = std::collections::HashSet::new();

    // 1. Populate nodes and mapping
    for r in &items {
        if r.item_type == "quickcap" { continue; }

        let title = if r.title.is_empty() {
            match r.item_type.as_str() {
                "note" => "Untitled Note".to_string(),
                "task" => "Untitled Task".to_string(),
                _ => r.title.clone(),
            }
        } else { r.title.clone() };

        let node = GraphNode {
            id: r.id.clone(),
            item_type: r.item_type.clone(),
            title: title.clone(),
            tags: r.tags.clone(),
        };

        let title_lower = title.to_lowercase().replace(".md", "");
        title_map.insert(title_lower, r.id.clone());
        path_map.insert(r.path.clone().to_lowercase(), r.id.clone());
        
        let path = std::path::Path::new(&r.path);
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            path_map.insert(file_name.to_lowercase(), r.id.clone());
        }

        node_map.insert(r.id.clone(), node);

        // Add Frontmatter Tags
        for mut tag in r.tags.clone() {
            if tag.starts_with("#") { tag = tag[1..].to_string(); }
            let tag_clean = tag.trim().to_lowercase();
            if tag_clean.is_empty() { continue; }
            
            let tag_id = format!("tag-{}", tag_clean);
            if !tag_nodes.contains_key(&tag_id) {
                tag_nodes.insert(tag_id.clone(), GraphNode {
                    id: tag_id.clone(),
                    item_type: "tag".to_string(),
                    title: format!("#{}", tag_clean),
                    tags: vec![],
                });
            }

            let link_key = format!("{}->{}", r.id, tag_id);
            if !added_links.contains(&link_key) {
                added_links.insert(link_key.clone());
                links.push(GraphLink { source: r.id.clone(), target: tag_id });
            }
        }
    }

    // 2. Build Links from Database Edges
    for edge in edges {
        // Skip edges where source is not in node_map (e.g. quickcaps or deleted items)
        if !node_map.contains_key(&edge.source_id) { continue; }

        let target_id = if edge.link_type == "tag" {
            let tag_name = edge.target_title_or_path.trim().to_lowercase();
            if tag_name.is_empty() { continue; }
            let tag_id = format!("tag-{}", tag_name.replace('#', ""));
            
            if !tag_nodes.contains_key(&tag_id) {
                tag_nodes.insert(tag_id.clone(), GraphNode {
                    id: tag_id.clone(),
                    item_type: "tag".to_string(),
                    title: format!("#{}", tag_name.replace('#', "")),
                    tags: vec![],
                });
            }
            tag_id
        } else {
            // For wikilinks and internal_links
            let link_target = edge.target_title_or_path.to_lowercase();
            let mut resolved_id = title_map.get(&link_target)
                .or_else(|| path_map.get(&link_target))
                .or_else(|| path_map.get(&format!("{}.md", link_target)))
                .or_else(|| path_map.get(&format!("Notes/{}", link_target)))
                .cloned();

            if resolved_id.is_none() {
                // Ghost Node! Target doesn't exist
                let ghost_id = format!("ghost-{}", link_target);
                if !ghost_nodes.contains_key(&ghost_id) {
                    ghost_nodes.insert(ghost_id.clone(), GraphNode {
                        id: ghost_id.clone(),
                        item_type: "ghost".to_string(),
                        title: edge.target_title_or_path.clone(), // Original casing
                        tags: vec![],
                    });
                }
                resolved_id = Some(ghost_id);
            }
            resolved_id.unwrap()
        };

        if target_id != edge.source_id {
            let link_key = format!("{}->{}", edge.source_id, target_id);
            if !added_links.contains(&link_key) {
                added_links.insert(link_key.clone());
                links.push(GraphLink { source: edge.source_id, target: target_id });
            }
        }
    }

    for (_, node) in node_map { nodes.push(node); }
    for (_, tag_node) in tag_nodes { nodes.push(tag_node); }
    for (_, ghost_node) in ghost_nodes { nodes.push(ghost_node); }

    Ok(GraphData { nodes, links })
}
