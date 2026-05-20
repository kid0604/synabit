use crate::models::chat::ChatMessage;
use std::path::Path;

#[tauri::command]
pub fn get_chat_history(vault_path: String) -> Result<Vec<ChatMessage>, String> {
    let mut all_msgs = Vec::new();
    let msg_dir = Path::new(&vault_path).join("Messages");

    if let Ok(entries) = std::fs::read_dir(&msg_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.ends_with(".json") && file_name.len() == 15 {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(mut msgs) = serde_json::from_str::<Vec<ChatMessage>>(&content) {
                        all_msgs.append(&mut msgs);
                    }
                }
            }
        }
    }

    // Sort chronologically (oldest to newest)
    all_msgs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // Deduplicate by target_id + subtype + reminder, keeping only the newest
    use std::collections::HashMap;
    let mut dedup_map = HashMap::new();
    for msg in all_msgs {
        if let Some(target_id) = msg.content.metadata.get("target_id").and_then(|v| v.as_str()) {
            let reminder = msg.content.metadata.get("reminder").and_then(|v| v.as_str()).unwrap_or("0m");
            let key = format!("{}_{}_{}", target_id, msg.subtype, reminder);
            dedup_map.insert(key, msg); // Inserts replace existing, keeping the latest because it's sorted
        } else {
            // If no target_id, just keep it (e.g. random AI messages or other types)
            let key = msg.id.clone();
            dedup_map.insert(key, msg);
        }
    }

    let mut deduped_msgs: Vec<ChatMessage> = dedup_map.into_values().collect();
    // Sort again chronologically for UI display
    deduped_msgs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    Ok(deduped_msgs)
}

#[tauri::command]
pub fn mark_chat_read(vault_path: String) -> Result<(), String> {
    let msg_dir = Path::new(&vault_path).join("Messages");

    if let Ok(entries) = std::fs::read_dir(&msg_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.ends_with(".json") && file_name.len() == 15 {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(mut msgs) = serde_json::from_str::<Vec<ChatMessage>>(&content) {
                        let mut changed = false;
                        for msg in msgs.iter_mut() {
                            if !msg.read_receipt {
                                msg.read_receipt = true;
                                changed = true;
                            }
                        }
                        if changed {
                            if let Ok(json_str) = serde_json::to_string_pretty(&msgs) {
                                let _ = std::fs::write(entry.path(), json_str);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
