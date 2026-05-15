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
                if r.item_type == "quickcap" || r.item_type == "message" || r.item_type == "notification" { continue; }
                if r.path.starts_with("Messages/") || r.path.contains("/Messages/") || r.path.starts_with("Messages\\") || r.path.contains("\\Messages\\") { continue; }
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

/// FTS5-powered universal search across all item types.
/// Supports advanced query syntax: is:, #tag, "phrase", -exclude, in:title, status:, date:
#[tauri::command]
pub fn search_nexus(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
    page: Option<u32>,
    per_page: Option<u32>,
    case_sensitive: Option<bool>,
) -> AppResult<crate::search::SearchResponse> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.case_sensitive = case_sensitive.unwrap_or(false);
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, page.unwrap_or(1), per_page.unwrap_or(50))
}

/// FTS5-powered search scoped to notes only.
/// Used by the Note mini-app sidebar search.
#[tauri::command]
pub fn search_notes(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
) -> AppResult<crate::search::SearchResponse> {
    // Force type filter to "note" regardless of user input
    let mut parsed = crate::search::parse_query(&query);
    parsed.type_filter = Some("note".to_string());
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, 1, 100)
}


/// FTS5-powered search scoped to tasks only.
/// Used by the Task mini-app search.
#[tauri::command]
pub fn search_tasks(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
) -> AppResult<crate::search::SearchResponse> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.type_filter = Some("task".to_string());
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, 1, 200)
}

/// FTS5-powered search scoped to files only.
/// Used by the File Manager mini-app search.
#[tauri::command]
pub fn search_files(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
) -> AppResult<crate::search::SearchResponse> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.type_filter = Some("file".to_string());
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, 1, 200)
}

#[tauri::command]
pub fn get_nexus_graph_data(_app_handle: tauri::AppHandle, state: tauri::State<'_, DbState>, _vault_path: String) -> AppResult<GraphData> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let items = db.get_all_nexus_items()?;
    let node_edges = db.get_all_node_edges()?;

    let mut nodes = Vec::new();
    let mut links = Vec::new();
    
    let mut node_ids = std::collections::HashSet::new();
    let mut tag_nodes = HashMap::new();
    let mut ghost_nodes = HashMap::new();
    let mut added_links = std::collections::HashSet::new();

    // 1. Build graph nodes from items
    for r in &items {
        if r.item_type == "quickcap" || r.item_type == "message" || r.item_type == "notification" { continue; }
        if r.path.starts_with("Messages/") || r.path.contains("/Messages/") || r.path.starts_with("Messages\\") || r.path.contains("\\Messages\\") { continue; }

        let title = if r.title.is_empty() {
            match r.item_type.as_str() {
                "note" => "Untitled Note".to_string(),
                "task" => "Untitled Task".to_string(),
                _ => r.title.clone(),
            }
        } else { r.title.clone() };

        node_ids.insert(r.id.clone());
        nodes.push(GraphNode {
            id: r.id.clone(),
            item_type: r.item_type.clone(),
            title,
            tags: r.tags.clone(),
        });

        // Tag nodes from properties (not from edges)
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
                added_links.insert(link_key);
                links.push(GraphLink { source: r.id.clone(), target: tag_id });
            }
        }
    }

    // 2. Build links from node_edges (already ID-based — no resolution needed)
    for edge in node_edges {
        // Skip edges where source is not in our graph
        if !node_ids.contains(&edge.source_id) { continue; }

        // Handle ghost targets
        let target_id = if edge.target_id.starts_with("ghost:") {
            let ghost_title = edge.target_id.strip_prefix("ghost:").unwrap_or(&edge.target_id);
            let ghost_id = format!("ghost-{}", ghost_title);
            if !ghost_nodes.contains_key(&ghost_id) {
                ghost_nodes.insert(ghost_id.clone(), GraphNode {
                    id: ghost_id.clone(),
                    item_type: "ghost".to_string(),
                    title: ghost_title.to_string(),
                    tags: vec![],
                });
            }
            ghost_id
        } else if !node_ids.contains(&edge.target_id) {
            continue; // Target node doesn't exist and isn't a ghost — skip
        } else {
            edge.target_id.clone()
        };

        if target_id != edge.source_id {
            let link_key = format!("{}->{}", edge.source_id, target_id);
            if !added_links.contains(&link_key) {
                added_links.insert(link_key);
                links.push(GraphLink { source: edge.source_id, target: target_id });
            }
        }
    }

    for (_, tag_node) in tag_nodes { nodes.push(tag_node); }
    for (_, ghost_node) in ghost_nodes { nodes.push(ghost_node); }

    Ok(GraphData { nodes, links })
}
