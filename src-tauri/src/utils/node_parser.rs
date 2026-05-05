use crate::models::node::NodeMetadata;
use serde_json::Value;
use gray_matter::Matter;
use gray_matter::engine::YAML;
use std::time::{SystemTime, UNIX_EPOCH};
use std::path::Path;

pub fn parse_file_to_node(vault_path: &str, file_path: &Path) -> Option<NodeMetadata> {
    let rel_path = crate::path_utils::to_relative(file_path, vault_path);
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    
    let content = std::fs::read_to_string(file_path).ok()?;
    let metadata = file_path.metadata().ok()?;
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let created = metadata.created().unwrap_or(modified);
    
    let created_at = chrono::DateTime::<chrono::Local>::from(created).format("%Y-%m-%d %H:%M:%S").to_string();
    let updated_at = chrono::DateTime::<chrono::Local>::from(modified).format("%Y-%m-%d %H:%M:%S").to_string();
    let timestamp = modified.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;

    let mut title = file_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let mut node_type = String::new();
    let mut properties = Value::Null;
    let mut final_content = content.clone();

    if ext == "md" {
        let matter = Matter::<YAML>::new();
        // Parse with generic Value to capture all frontmatter
        if let Ok(parsed) = matter.parse::<serde_json::Value>(&content) {
            if let Some(data) = parsed.data {
                properties = data;
                if let Some(t) = properties.get("title").and_then(|v| v.as_str()) {
                    title = t.to_string();
                }
                if let Some(ty) = properties.get("type").and_then(|v| v.as_str()) {
                    node_type = ty.to_string();
                }
            }
            final_content = parsed.content;
        } else {
            // Failed to parse frontmatter or no frontmatter
            properties = serde_json::json!({});
        }
        
        if node_type.is_empty() {
            node_type = "note".to_string();
        }
    } else if ext == "json" || ext == "canvas" {
        if let Ok(json_val) = serde_json::from_str::<Value>(&content) {
            if let Some(t) = json_val.get("title").and_then(|v| v.as_str()) {
                title = t.to_string();
            }
            if let Some(ty) = json_val.get("type").and_then(|v| v.as_str()) {
                node_type = ty.to_string();
            } else {
                node_type = ext.to_string();
            }
            
            if let Some(meta) = json_val.get("metadata") {
                properties = meta.clone();
            } else {
                properties = serde_json::json!({});
            }
        } else {
            node_type = ext.to_string();
            properties = serde_json::json!({});
        }
    } else {
        return None;
    }

    Some(NodeMetadata {
        id: rel_path,
        node_type,
        title,
        content: final_content,
        properties,
        created_at,
        updated_at,
        timestamp,
    })
}
