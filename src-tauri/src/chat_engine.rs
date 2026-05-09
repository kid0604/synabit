use crate::db::DbState;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use chrono::Local;
use tauri::Manager;
use crate::models::chat::{ChatMessage, ChatSender, ChatContent};

/// Shared state to track the currently active vault path.
/// Updated by the file watcher whenever the user opens a vault.
pub struct ChatEngineState {
    pub active_vault_path: Arc<Mutex<Option<String>>>,
}

impl Default for ChatEngineState {
    fn default() -> Self {
        Self {
            active_vault_path: Arc::new(Mutex::new(None)),
        }
    }
}

/// Initializes the chat engine background task.
/// This thread runs continuously and evaluates scheduling rules.
pub fn init_engine(app_handle: tauri::AppHandle) {
    let state: tauri::State<'_, ChatEngineState> = app_handle.state();
    let vault_path_state = state.active_vault_path.clone();

    tauri::async_runtime::spawn(async move {
        log::info!("Chat Engine background task started.");
        
        let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute
        
        loop {
            interval.tick().await;
            
            let vault_path = {
                let lock = vault_path_state.lock().unwrap();
                match lock.as_ref() {
                    Some(path) => path.clone(),
                    None => continue, // No vault selected yet
                }
            };
            
            // Build deduplication set from existing JSON logs in Messages directory
            let mut notified_set = HashSet::new();
            let msg_dir = Path::new(&vault_path).join("Messages");
            let _ = std::fs::create_dir_all(&msg_dir);
            
            if let Ok(entries) = std::fs::read_dir(&msg_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().map_or(false, |ext| ext == "json") {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if let Ok(msgs) = serde_json::from_str::<Vec<ChatMessage>>(&content) {
                                for msg in msgs {
                                    let target_id = msg.content.metadata.get("target_id").and_then(|v| v.as_str()).unwrap_or("");
                                    let trigger_date = msg.content.metadata.get("trigger_date").and_then(|v| v.as_str()).unwrap_or("");
                                    if !target_id.is_empty() && !trigger_date.is_empty() {
                                        notified_set.insert(format!("{}_{}", target_id, trigger_date));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Acquire DB Lock
            let db_state: tauri::State<'_, DbState> = app_handle.state();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

            let today_str = Local::now().format("%Y-%m-%d").to_string();
            let tomorrow_str = (Local::now() + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
            let current_mmdd = Local::now().format("%m-%d").to_string();
            let tomorrow_mmdd = (Local::now() + chrono::Duration::days(1)).format("%m-%d").to_string();
            
            let mut new_messages: Vec<ChatMessage> = Vec::new();

            let sender = ChatSender {
                id: "system".to_string(),
                name: "Synabit System".to_string(),
                role: "bot".to_string(),
            };

            // 1. Process Tasks
            if let Ok(tasks) = db.get_nodes_by_type("task") {
                for task in tasks {
                    let status = task.properties.get("status").and_then(|v: &Value| v.as_str()).unwrap_or("");
                    if status == "done" || status == "canceled" {
                        continue;
                    }
                    
                    if let Some(due_date) = task.properties.get("due_date").and_then(|v: &Value| v.as_str()) {
                        if due_date.is_empty() { continue; }
                        
                        // Overdue or Due Today
                        if due_date <= today_str.as_str() {
                            let is_overdue = due_date < today_str.as_str();
                            let dedup_key = format!("{}_{}", task.id, today_str);
                            if !notified_set.contains(&dedup_key) {
                                new_messages.push(ChatMessage {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    message_type: "system".to_string(),
                                    subtype: "task_due".to_string(),
                                    timestamp: Local::now().to_rfc3339(),
                                    sender: sender.clone(),
                                    content: ChatContent {
                                        title: if is_overdue { format!("Task Overdue: {}", task.title) } else { format!("Task Due Today: {}", task.title) },
                                        text: "Don't forget to complete your task!".to_string(),
                                        metadata: json!({
                                            "target_id": task.id.clone(),
                                            "trigger_date": today_str.clone()
                                        }),
                                    },
                                    read_receipt: false,
                                });
                                notified_set.insert(dedup_key);
                            }
                        }
                    }
                }
            }

            // 2. Process Events
            if let Ok(events) = db.get_nodes_by_type("event") {
                for event in events {
                    if let Some(start_at) = event.properties.get("start_at").and_then(|v: &Value| v.as_str()) {
                        if start_at.is_empty() { continue; }
                        let date_part = start_at.split('T').next().unwrap_or(start_at);
                        let date_part = date_part.split(' ').next().unwrap_or(date_part);
                        
                        if date_part == today_str || date_part == tomorrow_str {
                            let dedup_key = format!("{}_{}", event.id, date_part);
                            if !notified_set.contains(&dedup_key) {
                                new_messages.push(ChatMessage {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    message_type: "system".to_string(),
                                    subtype: "event_upcoming".to_string(),
                                    timestamp: Local::now().to_rfc3339(),
                                    sender: sender.clone(),
                                    content: ChatContent {
                                        title: format!("Event Upcoming: {}", event.title),
                                        text: format!("You have an event on {}", date_part),
                                        metadata: json!({
                                            "target_id": event.id.clone(),
                                            "trigger_date": date_part.to_string()
                                        }),
                                    },
                                    read_receipt: false,
                                });
                                notified_set.insert(dedup_key);
                            }
                        }
                    }
                }
            }

            // 3. Process Birthdays
            if let Ok(people) = db.get_nodes_by_type("person") {
                for person in people {
                    if let Some(birthday) = person.properties.get("birthday").and_then(|v: &Value| v.as_str()) {
                        if birthday.is_empty() { continue; }
                        // Handle YYYY-MM-DD
                        let parts: Vec<&str> = birthday.split('-').collect();
                        if parts.len() == 3 {
                            let mmdd = format!("{}-{}", parts[1], parts[2]);
                            if mmdd == current_mmdd || mmdd == tomorrow_mmdd {
                                let dedup_key = format!("{}_{}", person.id, today_str);
                                if !notified_set.contains(&dedup_key) {
                                    let is_today = mmdd == current_mmdd;
                                    new_messages.push(ChatMessage {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        message_type: "system".to_string(),
                                        subtype: "birthday_upcoming".to_string(),
                                        timestamp: Local::now().to_rfc3339(),
                                        sender: sender.clone(),
                                        content: ChatContent {
                                            title: format!("Birthday Reminder: {}", person.title),
                                            text: if is_today { format!("Today is {}'s birthday!", person.title) } else { format!("Tomorrow is {}'s birthday!", person.title) },
                                            metadata: json!({
                                                "target_id": person.id.clone(),
                                                "trigger_date": today_str.clone()
                                            }),
                                        },
                                        read_receipt: false,
                                    });
                                    notified_set.insert(dedup_key);
                                }
                            }
                        }
                    }
                }
            }
            
            // Drop DB lock before writing to disk
            drop(db);

            // Write new messages to disk
            if !new_messages.is_empty() {
                let daily_file_path = msg_dir.join(format!("{}.json", today_str));
                
                let mut existing_messages: Vec<ChatMessage> = Vec::new();
                if daily_file_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&daily_file_path) {
                        if let Ok(msgs) = serde_json::from_str::<Vec<ChatMessage>>(&content) {
                            existing_messages = msgs;
                        }
                    }
                }
                
                existing_messages.extend(new_messages);
                
                if let Ok(json_str) = serde_json::to_string_pretty(&existing_messages) {
                    if let Err(e) = std::fs::write(&daily_file_path, json_str) {
                        log::error!("Failed to write daily chat log: {}", e);
                    } else {
                        log::info!("Updated daily chat log: {}", daily_file_path.display());
                    }
                }
            }
        }
    });
}
