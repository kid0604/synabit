use crate::db::DbState;
use crate::models::chat::{ChatContent, ChatMessage, ChatSender};
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Duration as ChronoDuration};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Manager, Emitter};
use tauri_plugin_notification::NotificationExt;

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

fn parse_duration(s: &str) -> ChronoDuration {
    let s = s.trim();
    if s.ends_with('m') {
        let val: i64 = s[..s.len()-1].parse().unwrap_or(0);
        ChronoDuration::try_minutes(val).unwrap_or_else(ChronoDuration::zero)
    } else if s.ends_with('h') {
        let val: i64 = s[..s.len()-1].parse().unwrap_or(0);
        ChronoDuration::try_hours(val).unwrap_or_else(ChronoDuration::zero)
    } else if s.ends_with('d') {
        let val: i64 = s[..s.len()-1].parse().unwrap_or(0);
        ChronoDuration::try_days(val).unwrap_or_else(ChronoDuration::zero)
    } else {
        ChronoDuration::zero()
    }
}

fn occurs_on_date(
    start_date_str: &str,
    recurrence: &str,
    recurrence_end_at: &str,
    exceptions: &[String],
    target_date_str: &str,
) -> bool {
    if target_date_str < start_date_str { return false; }
    if !recurrence_end_at.is_empty() && target_date_str > recurrence_end_at { return false; }
    if exceptions.contains(&target_date_str.to_string()) { return false; }
    
    match recurrence {
        "daily" => true,
        "weekly" => {
            if let (Ok(s), Ok(t)) = (NaiveDate::parse_from_str(start_date_str, "%Y-%m-%d"), NaiveDate::parse_from_str(target_date_str, "%Y-%m-%d")) {
                s.weekday() == t.weekday()
            } else { false }
        },
        "monthly" => {
            start_date_str.split('-').nth(2) == target_date_str.split('-').nth(2)
        },
        "yearly" => {
            let s_mmdd = start_date_str.splitn(2, '-').nth(1);
            let t_mmdd = target_date_str.splitn(2, '-').nth(1);
            s_mmdd.is_some() && s_mmdd == t_mmdd
        },
        _ => start_date_str == target_date_str,
    }
}

pub fn init_engine(app_handle: tauri::AppHandle) {
    let state: tauri::State<'_, ChatEngineState> = app_handle.state();
    let vault_path_state = state.active_vault_path.clone();

    tauri::async_runtime::spawn(async move {
        log::info!("Chat Engine background task started.");
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            let vault_path = {
                let lock = vault_path_state.lock().unwrap();
                match lock.as_ref() {
                    Some(path) => path.clone(),
                    None => continue,
                }
            };
            
            let msg_dir = Path::new(&vault_path).join("Messages");
            let _ = std::fs::create_dir_all(&msg_dir);
            let mut notified_set = HashSet::new();
            
            if let Ok(entries) = std::fs::read_dir(&msg_dir) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.ends_with(".json") && file_name.len() == 15 {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if let Ok(msgs) = serde_json::from_str::<Vec<ChatMessage>>(&content) {
                                for msg in msgs {
                                    let target_id = msg.content.metadata.get("target_id").and_then(|v| v.as_str()).unwrap_or("");
                                    let trigger_date = msg.content.metadata.get("trigger_date").and_then(|v| v.as_str()).unwrap_or("");
                                    let reminder = msg.content.metadata.get("reminder").and_then(|v| v.as_str()).unwrap_or("0m");
                                    if !target_id.is_empty() && !trigger_date.is_empty() {
                                        notified_set.insert(format!("{}_{}_{}", target_id, trigger_date, reminder));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let db_state: tauri::State<'_, DbState> = app_handle.state();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

            let now = Local::now();
            let today_str = now.format("%Y-%m-%d").to_string();
            let tomorrow_str = (now + chrono::Duration::try_days(1).unwrap()).format("%Y-%m-%d").to_string();
            
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
                    if status == "done" || status == "canceled" { continue; }
                    
                    if let Some(due_date) = task.properties.get("due_date").and_then(|v: &Value| v.as_str()) {
                        if due_date.is_empty() { continue; }
                        let is_overdue = due_date < today_str.as_str();
                        let target_date = if is_overdue || due_date == today_str.as_str() { &today_str } else { continue; };
                        
                        let mut reminders = vec![];
                        if let Some(rems) = task.properties.get("reminders").and_then(|v| v.as_array()) {
                            for r in rems {
                                if let Some(r_str) = r.as_str() { reminders.push(r_str.to_string()); }
                            }
                        }
                        if reminders.is_empty() { reminders.push("0m".to_string()); }
                        
                        let start_time = task.properties.get("start_time").and_then(|v| v.as_str()).unwrap_or("09:00:00");
                        let dt_str = format!("{}T{}", target_date, start_time);
                        let event_dt = NaiveDateTime::parse_from_str(&dt_str, "%Y-%m-%dT%H:%M:%S").unwrap_or_else(|_| {
                            NaiveDateTime::parse_from_str(&format!("{}:00", dt_str), "%Y-%m-%dT%H:%M:%S").unwrap_or_default()
                        });
                        
                        if let Some(event_local) = Local.from_local_datetime(&event_dt).single() {
                            for rem in reminders {
                                let dur = parse_duration(&rem);
                                let trigger_time = event_local - dur;
                                
                                if now >= trigger_time {
                                    let dedup_key = format!("{}_{}_{}", task.id, target_date, rem);
                                    if !notified_set.contains(&dedup_key) {
                                        new_messages.push(ChatMessage {
                                            id: uuid::Uuid::new_v4().to_string(),
                                            message_type: "system".to_string(),
                                            subtype: "task_due".to_string(),
                                            timestamp: now.to_rfc3339(),
                                            sender: sender.clone(),
                                            content: ChatContent {
                                                title: if is_overdue { format!("Task Overdue: {}", task.title) } else { format!("Task Due Today: {}", task.title) },
                                                text: "Don't forget to complete your task!".to_string(),
                                                metadata: json!({
                                                    "target_id": task.id.clone(),
                                                    "trigger_date": target_date.to_string(),
                                                    "reminder": rem.clone()
                                                }),
                                            },
                                            read_receipt: false,
                                        });
                                        notified_set.insert(dedup_key);
                                        
                                        // Push Notification
                                        if let Err(e) = app_handle.notification().builder()
                                            .title(if is_overdue { "Task Overdue" } else { "Task Due" })
                                            .body(&task.title)
                                            .show() {
                                                log::error!("Failed to show notification: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 2. Process Events
            if let Ok(events) = db.get_nodes_by_type("event") {
                for event in events {
                    let start_at = event.properties.get("start_at").and_then(|v: &Value| v.as_str()).unwrap_or("");
                    if start_at.is_empty() { continue; }
                    
                    let start_date = start_at.split('T').next().unwrap_or(start_at);
                    let start_time = if start_at.contains('T') { start_at.split('T').nth(1).unwrap_or("00:00:00") } else { "00:00:00" };
                    
                    let recurrence = event.properties.get("recurrence").and_then(|v| v.as_str()).unwrap_or("none");
                    let recurrence_end_at = event.properties.get("recurrence_end_at").and_then(|v| v.as_str()).unwrap_or("");
                    let exceptions: Vec<String> = event.properties.get("exceptions").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                    
                    for target_date in [&today_str, &tomorrow_str] {
                        if occurs_on_date(start_date, recurrence, recurrence_end_at, &exceptions, target_date) {
                            let mut reminders = vec![];
                            if let Some(rems) = event.properties.get("reminders").and_then(|v| v.as_array()) {
                                for r in rems {
                                    if let Some(r_str) = r.as_str() { reminders.push(r_str.to_string()); }
                                }
                            }
                            if reminders.is_empty() { reminders.push("0m".to_string()); }
                            
                            let dt_str = format!("{}T{}", target_date, start_time);
                            let event_dt = NaiveDateTime::parse_from_str(&dt_str, "%Y-%m-%dT%H:%M:%S").unwrap_or_else(|_| {
                                NaiveDateTime::parse_from_str(&format!("{}:00", dt_str), "%Y-%m-%dT%H:%M:%S").unwrap_or_else(|_| {
                                    NaiveDateTime::parse_from_str(&format!("{}T00:00:00", target_date), "%Y-%m-%dT%H:%M:%S").unwrap()
                                })
                            });
                            
                            if let Some(event_local) = Local.from_local_datetime(&event_dt).single() {
                                for rem in reminders {
                                    let dur = parse_duration(&rem);
                                    let trigger_time = event_local - dur;
                                    
                                    if now >= trigger_time {
                                        let dedup_key = format!("{}_{}_{}", event.id, target_date, rem);
                                        if !notified_set.contains(&dedup_key) {
                                            new_messages.push(ChatMessage {
                                                id: uuid::Uuid::new_v4().to_string(),
                                                message_type: "system".to_string(),
                                                subtype: "event_upcoming".to_string(),
                                                timestamp: now.to_rfc3339(),
                                                sender: sender.clone(),
                                                content: ChatContent {
                                                    title: format!("Upcoming Event: {}", event.title),
                                                    text: if rem == "0m" { format!("Happening now: {}", event.title) } else { format!("Starts in {}", rem) },
                                                    metadata: json!({
                                                        "target_id": event.id.clone(),
                                                        "trigger_date": target_date.to_string(),
                                                        "reminder": rem.clone()
                                                    }),
                                                },
                                                read_receipt: false,
                                            });
                                            notified_set.insert(dedup_key);
                                            
                                            if let Err(e) = app_handle.notification().builder()
                                                .title("Upcoming Event")
                                                .body(&format!("{} ({})", event.title, start_time))
                                                .show() {
                                                    log::error!("Failed to show notification: {}", e);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // 3. Process Birthdays (Skipped rewrite for brevity, just keeping it functional)
            if let Ok(people) = db.get_nodes_by_type("person") {
                let current_mmdd = today_str[5..].to_string();
                let tomorrow_mmdd = tomorrow_str[5..].to_string();
                for person in people {
                    if let Some(birthday) = person.properties.get("birthday").and_then(|v: &Value| v.as_str()) {
                        if birthday.is_empty() { continue; }
                        let parts: Vec<&str> = birthday.split('-').collect();
                        if parts.len() == 3 {
                            let mmdd = format!("{}-{}", parts[1], parts[2]);
                            if mmdd == current_mmdd || mmdd == tomorrow_mmdd {
                                let is_today = mmdd == current_mmdd;
                                let target_date = if is_today { &today_str } else { &tomorrow_str };
                                let dedup_key = format!("{}_{}_0m", person.id, target_date);
                                if !notified_set.contains(&dedup_key) {
                                    new_messages.push(ChatMessage {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        message_type: "system".to_string(),
                                        subtype: "birthday_upcoming".to_string(),
                                        timestamp: now.to_rfc3339(),
                                        sender: sender.clone(),
                                        content: ChatContent {
                                            title: format!("Birthday Reminder: {}", person.title),
                                            text: if is_today { format!("Today is {}'s birthday!", person.title) } else { format!("Tomorrow is {}'s birthday!", person.title) },
                                            metadata: json!({
                                                "target_id": person.id.clone(),
                                                "trigger_date": target_date.to_string(),
                                                "reminder": "0m"
                                            }),
                                        },
                                        read_receipt: false,
                                    });
                                    notified_set.insert(dedup_key);
                                    
                                    if let Err(e) = app_handle.notification().builder()
                                        .title("Birthday Reminder")
                                        .body(&if is_today { format!("Today is {}'s birthday!", person.title) } else { format!("Tomorrow is {}'s birthday!", person.title) })
                                        .show() {
                                            log::error!("Failed to show notification: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            drop(db);

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
                        let _ = app_handle.emit("new-chat-message", ());
                    }
                }
            }
        }
    });
}
