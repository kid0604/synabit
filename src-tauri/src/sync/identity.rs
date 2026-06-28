use std::path::Path;
use crate::error::{AppError, AppResult};
use serde_json::Value;

/// Extracts `node_id` from a file, or generates a new one and injects it.
pub fn get_or_assign_node_id(vault_path: &Path, file_path: &Path) -> AppResult<String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| AppError::General(format!("Failed to read file for identity: {}", e)))?;
    
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    
    if ext == "md" {
        // Simple regex to extract node_id from frontmatter
        let re = regex::Regex::new(r"(?m)^node_id:\s*([a-zA-Z0-9\-]+)\s*$").unwrap();
        if let Some(caps) = re.captures(&content) {
            if let Some(id_match) = caps.get(1) {
                return Ok(id_match.as_str().to_string());
            }
        }
        
        // Try fallback to synabit_id just in case
        let re_legacy = regex::Regex::new(r"(?m)^synabit_id:\s*([a-zA-Z0-9\-]+)\s*$").unwrap();
        if let Some(caps) = re_legacy.captures(&content) {
            if let Some(id_match) = caps.get(1) {
                return Ok(id_match.as_str().to_string());
            }
        }
        
        // No ID found, we need to inject it.
        let new_id = uuid::Uuid::new_v4().to_string();
        let new_content = inject_markdown_id(&content, &new_id);
        std::fs::write(file_path, new_content)
            .map_err(|e| AppError::General(format!("Failed to write injected node_id to markdown: {}", e)))?;
        
        Ok(new_id)
        
    } else if ext == "json" || ext == "canvas" {
        let mut json_val: Value = serde_json::from_str(&content)
            .map_err(|e| AppError::General(format!("Failed to parse JSON for identity: {}", e)))?;
            
        if let Some(meta) = json_val.get_mut("metadata") {
            if let Some(node_id) = meta.get("node_id").and_then(|v| v.as_str()) {
                return Ok(node_id.to_string());
            }
            if let Some(meta_obj) = meta.as_object_mut() {
                let new_id = uuid::Uuid::new_v4().to_string();
                meta_obj.insert("node_id".to_string(), Value::String(new_id.clone()));
                std::fs::write(file_path, serde_json::to_string_pretty(&json_val).unwrap())
                    .map_err(|e| AppError::General(format!("Failed to write injected node_id to json: {}", e)))?;
                return Ok(new_id);
            }
        } else {
            // No metadata object, create it
            if let Some(root_obj) = json_val.as_object_mut() {
                let new_id = uuid::Uuid::new_v4().to_string();
                let mut meta_obj = serde_json::Map::new();
                meta_obj.insert("node_id".to_string(), Value::String(new_id.clone()));
                root_obj.insert("metadata".to_string(), Value::Object(meta_obj));
                std::fs::write(file_path, serde_json::to_string_pretty(&json_val).unwrap())
                    .map_err(|e| AppError::General(format!("Failed to write injected node_id to json: {}", e)))?;
                return Ok(new_id);
            }
        }
        
        // Fallback if not a json object
        let rel_path = crate::path_utils::to_relative(file_path, vault_path.to_string_lossy().as_ref());
        Ok(rel_path)
    } else {
        // Assets or unknown files: use relative path as ID for now
        let rel_path = crate::path_utils::to_relative(file_path, vault_path.to_string_lossy().as_ref());
        Ok(rel_path)
    }
}

/// Helper to inject `node_id` into Markdown frontmatter
fn inject_markdown_id(content: &str, node_id: &str) -> String {
    if content.starts_with("---\n") || content.starts_with("---\r\n") {
        // Has frontmatter, inject after the first line
        let first_nl = content.find('\n').unwrap() + 1;
        let mut new_content = content.to_string();
        new_content.insert_str(first_nl, &format!("node_id: {}\n", node_id));
        new_content
    } else {
        // No frontmatter, prepend it
        format!("---\nnode_id: {}\n---\n\n{}", node_id, content)
    }
}
