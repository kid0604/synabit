use std::path::Path;

use crate::db::DbState;
use crate::error::AppResult;

#[tauri::command]
pub fn get_all_tags(state: tauri::State<'_, DbState>) -> AppResult<Vec<(String, i64)>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_all_tags_with_counts()
}

#[tauri::command]
pub fn rename_tag(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    old_tag: String,
    new_tag: String,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = db.get_nodes_by_tag(&old_tag)?;

    for node in nodes {
        let full_path = Path::new(&vault_path).join(&node.id);
        if !full_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if ext == "md" {
            if content.starts_with("---\n") || content.starts_with("---\r\n") {
                let nl_len = if content.starts_with("---\r\n") { 5 } else { 4 };
                if let Some(end_idx) = content[nl_len..].find("---") {
                    let fm_str = &content[nl_len..nl_len + end_idx];
                    let mut fm_val: serde_json::Value =
                        serde_yaml::from_str(fm_str).unwrap_or(serde_json::json!({}));

                    let mut changed = false;
                    if let Some(tags_val) = fm_val.get_mut("tags") {
                        if let Some(tags_arr) = tags_val.as_array_mut() {
                            for t in tags_arr.iter_mut() {
                                if t.as_str() == Some(old_tag.as_str()) {
                                    *t = serde_json::json!(new_tag.clone());
                                    changed = true;
                                }
                            }
                        } else if tags_val.as_str() == Some(old_tag.as_str()) {
                            *tags_val = serde_json::json!(new_tag.clone());
                            changed = true;
                        }
                    }

                    if changed {
                        let new_fm = serde_yaml::to_string(&fm_val).unwrap_or_default();
                        let rest_of_content = &content[nl_len + end_idx + 3..];
                        // ensure we handle any trailing newline after ---
                        let rest_of_content = if rest_of_content.starts_with("\r\n") {
                            &rest_of_content[2..]
                        } else if rest_of_content.starts_with('\n') {
                            &rest_of_content[1..]
                        } else {
                            rest_of_content
                        };

                        let new_content = format!("---\n{}---\n{}", new_fm, rest_of_content);
                        if std::fs::write(&full_path, new_content).is_ok() {
                            if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &full_path) {
                                let _ = db.upsert_node(&parsed_node);
                            }
                        }
                    }
                }
            }
        } else if ext == "json" || ext == "canvas" {
            let mut json_val: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            let mut changed = false;

            if let Some(meta) = json_val.get_mut("metadata") {
                if let Some(tags_val) = meta.get_mut("tags") {
                    if let Some(tags_arr) = tags_val.as_array_mut() {
                        for t in tags_arr.iter_mut() {
                            if t.as_str() == Some(old_tag.as_str()) {
                                *t = serde_json::json!(new_tag.clone());
                                changed = true;
                            }
                        }
                    } else if tags_val.as_str() == Some(old_tag.as_str()) {
                        *tags_val = serde_json::json!(new_tag.clone());
                        changed = true;
                    }
                }
            }

            if changed {
                if std::fs::write(&full_path, serde_json::to_string_pretty(&json_val).unwrap_or_default()).is_ok() {
                    if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &full_path) {
                        let _ = db.upsert_node(&parsed_node);
                    }
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn delete_tag(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    tag: String,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = db.get_nodes_by_tag(&tag)?;

    for node in nodes {
        let full_path = Path::new(&vault_path).join(&node.id);
        if !full_path.exists() {
            continue;
        }

        let content = std::fs::read_to_string(&full_path).unwrap_or_default();
        let ext = full_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if ext == "md" {
            if content.starts_with("---\n") || content.starts_with("---\r\n") {
                let nl_len = if content.starts_with("---\r\n") { 5 } else { 4 };
                if let Some(end_idx) = content[nl_len..].find("---") {
                    let fm_str = &content[nl_len..nl_len + end_idx];
                    let mut fm_val: serde_json::Value =
                        serde_yaml::from_str(fm_str).unwrap_or(serde_json::json!({}));

                    let mut changed = false;
                    if let Some(tags_val) = fm_val.get_mut("tags") {
                        if let Some(tags_arr) = tags_val.as_array_mut() {
                            let original_len = tags_arr.len();
                            tags_arr.retain(|t| t.as_str() != Some(tag.as_str()));
                            if tags_arr.len() != original_len {
                                changed = true;
                            }
                        } else if tags_val.as_str() == Some(tag.as_str()) {
                            *tags_val = serde_json::json!([]);
                            changed = true;
                        }
                    }

                    if changed {
                        let new_fm = serde_yaml::to_string(&fm_val).unwrap_or_default();
                        let rest_of_content = &content[nl_len + end_idx + 3..];
                        let rest_of_content = if rest_of_content.starts_with("\r\n") {
                            &rest_of_content[2..]
                        } else if rest_of_content.starts_with('\n') {
                            &rest_of_content[1..]
                        } else {
                            rest_of_content
                        };

                        let new_content = format!("---\n{}---\n{}", new_fm, rest_of_content);
                        if std::fs::write(&full_path, new_content).is_ok() {
                            if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &full_path) {
                                let _ = db.upsert_node(&parsed_node);
                            }
                        }
                    }
                }
            }
        } else if ext == "json" || ext == "canvas" {
            let mut json_val: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            let mut changed = false;

            if let Some(meta) = json_val.get_mut("metadata") {
                if let Some(tags_val) = meta.get_mut("tags") {
                    if let Some(tags_arr) = tags_val.as_array_mut() {
                        let original_len = tags_arr.len();
                        tags_arr.retain(|t| t.as_str() != Some(tag.as_str()));
                        if tags_arr.len() != original_len {
                            changed = true;
                        }
                    } else if tags_val.as_str() == Some(tag.as_str()) {
                        *tags_val = serde_json::json!([]);
                        changed = true;
                    }
                }
            }

            if changed {
                if std::fs::write(&full_path, serde_json::to_string_pretty(&json_val).unwrap_or_default()).is_ok() {
                    if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &full_path) {
                        let _ = db.upsert_node(&parsed_node);
                    }
                }
            }
        }
    }

    Ok(())
}
