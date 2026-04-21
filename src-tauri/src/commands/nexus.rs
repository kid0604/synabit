use std::collections::HashMap;

use crate::models::nexus::{NexusItem, TagStat, VaultStats};
use crate::db::DbBridge;
use crate::error::AppResult;

#[tauri::command]
pub fn get_nexus_stats(vault_path: String) -> AppResult<VaultStats> {
    let items = get_nexus_items(vault_path)?;
    let mut type_distribution = HashMap::new();
    let mut tag_map: HashMap<String, TagStat> = HashMap::new();
    
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
                distribution: HashMap::new(),
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
pub fn get_nexus_items(vault_path: String) -> AppResult<Vec<NexusItem>> {
    let mut items = Vec::new();

    // ─── Query indexed data from SQLite (fast) ─────────────
    if let Ok(db) = DbBridge::new(&vault_path) {
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
pub fn search_nexus(vault_path: String, query: String) -> AppResult<Vec<NexusItem>> {
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
