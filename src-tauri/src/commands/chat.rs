use std::path::Path;
use crate::models::chat::ChatMessage;

#[tauri::command]
pub fn get_chat_history(vault_path: String) -> Result<Vec<ChatMessage>, String> {
    let mut all_msgs = Vec::new();
    let msg_dir = Path::new(&vault_path).join("Messages");
    
    if let Ok(entries) = std::fs::read_dir(&msg_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |ext| ext == "json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(mut msgs) = serde_json::from_str::<Vec<ChatMessage>>(&content) {
                        all_msgs.append(&mut msgs);
                    }
                }
            }
        }
    }
    
    // Sort chronologically
    all_msgs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    
    Ok(all_msgs)
}
