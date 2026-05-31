use crate::models::node::NodeMetadata;
use gray_matter::engine::YAML;
use gray_matter::Matter;
use serde_json::Value;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn parse_file_to_node(vault_path: &str, file_path: &Path) -> Option<NodeMetadata> {
    let rel_path = crate::path_utils::to_relative(file_path, vault_path);
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let content = std::fs::read_to_string(file_path).ok()?;
    let metadata = file_path.metadata().ok()?;
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let created = metadata.created().unwrap_or(modified);

    let mut created_at = chrono::DateTime::<chrono::Local>::from(created)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut updated_at = chrono::DateTime::<chrono::Local>::from(modified)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let timestamp = modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut title = file_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
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
            } else if file_path.file_name().unwrap_or_default().to_string_lossy().ends_with(".whiteboard.json") {
                node_type = "whiteboard".to_string();
            } else {
                node_type = ext.to_string();
            }

            if let Some(meta) = json_val.get("metadata") {
                properties = meta.clone();
            } else {
                properties = serde_json::json!({});
            }

            if let Some(c) = json_val.get("content").and_then(|v| v.as_str()) {
                final_content = c.to_string();
            }
        } else {
            node_type = ext.to_string();
            properties = serde_json::json!({});
        }
    } else {
        return None;
    }

    // Override dates from properties if available
    if let Some(c) = properties.get("created_at").and_then(|v| v.as_str()) {
        created_at = c.to_string();
    }
    if let Some(u) = properties.get("updated_at").and_then(|v| v.as_str()) {
        updated_at = u.to_string();
    }

    // Extract blocks if markdown
    let mut blocks = None;
    if ext == "md" {
        blocks = Some(extract_blocks(&final_content));
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
        blocks,
    })
}

pub fn extract_blocks(content: &str) -> Vec<(String, String)> {
    use pulldown_cmark::{Event, Options, Parser, TagEnd};
    use regex::Regex;

    let mut blocks = Vec::new();
    let options = Options::all();
    let parser = Parser::new_ext(content, options).into_offset_iter();

    // Regex to find ` ^block-id` at the end of the block
    let re = Regex::new(r"(?m)\s*\^([a-zA-Z0-9\-]+)\s*$").unwrap();

    for (event, range) in parser {
        match event {
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Item) => {
                let block_text = &content[range.clone()];
                if let Some(captures) = re.captures(block_text) {
                    if let Some(id_match) = captures.get(1) {
                        let block_id = id_match.as_str().to_string();
                        // Extract content without the block ID marker? Or keep it?
                        // Keep full block text for exact rendering.
                        blocks.push((block_id, block_text.to_string()));
                    }
                }
            }
            _ => {}
        }
    }

    blocks
}
