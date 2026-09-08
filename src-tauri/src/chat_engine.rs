use crate::calendar::reminders;
use crate::db::DbState;
use crate::models::chat::{ChatContent, ChatMessage, ChatSender};
use chrono::Local;
use serde_json::json;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};
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

pub fn init_engine(app_handle: tauri::AppHandle) {
    let state: tauri::State<'_, ChatEngineState> = app_handle.state();
    let vault_path_state = state.active_vault_path.clone();

    tauri::async_runtime::spawn(async move {
        log::info!("Chat Engine background task started.");
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;

            let vault_path = {
                let lock = vault_path_state.lock().unwrap_or_else(|e| e.into_inner());
                match lock.as_ref() {
                    Some(path) => path.clone(),
                    None => continue,
                }
            };

            let msg_dir = Path::new(&vault_path).join("Messages");
            let _ = std::fs::create_dir_all(&msg_dir);

            let db_state: tauri::State<'_, DbState> = app_handle.state();
            let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());

            let now = Local::now();
            // This machine's zone, by name. Asked once a tick rather than
            // once an event: it is a file read on most platforms.
            let here = iana_time_zone::get_timezone().unwrap_or_default();
            let today_str = now.format("%Y-%m-%d").to_string();

            // Back far enough to catch up on anything missed while the
            // machine was asleep; the delivery record stops it repeating.
            let window_to = now.naive_local();
            let window_from = window_to
                - chrono::Duration::try_days(reminders::CATCH_UP_DAYS)
                    .unwrap_or_else(chrono::Duration::zero);

            let seen_since = (now
                - chrono::Duration::try_days(reminders::CATCH_UP_DAYS + 1)
                    .unwrap_or_else(chrono::Duration::zero))
            .timestamp();
            let mut notified_set = db.delivered_reminders(seen_since).unwrap_or_default();
            let mut delivered: Vec<String> = Vec::new();

            let active_nodes = db.get_active_tasks_and_events().unwrap_or_default();
            // Subscribed calendars are a cache, not files, so they never reach
            // the loop as nodes. Only the ones the user asked to be reminded
            // about: a holidays feed announcing every holiday at midnight is
            // noise, and only they know which kind of calendar this is.
            let subscribed = db.subscribed_events_to_remind().unwrap_or_default();

            let mut new_messages: Vec<ChatMessage> = Vec::new();
            let sender = ChatSender {
                id: "system".to_string(),
                name: "Synabit System".to_string(),
                role: "bot".to_string(),
            };

            // 1. Whatever has come due since the last look.
            //
            // What to announce and when is worked out in
            // `calendar::reminders`, which the phone's scheduler also uses.
            // Deciding it here as well is how the two would come to disagree
            // about when a reminder is.
            for due in reminders::plan_with(&active_nodes, &subscribed, window_from, window_to, &here) {
                let key = due.delivery_key();
                if notified_set.contains(&key) {
                    continue;
                }

                let (title, text, subtype) = match due.target_type {
                    "task" => (
                        if due.overdue {
                            format!("Task Overdue: {}", due.title)
                        } else {
                            format!("Task Due Today: {}", due.title)
                        },
                        "Don't forget to complete your task!".to_string(),
                        "task_due",
                    ),
                    "person" if due.offset == "touch" => (
                        format!("Keep in touch: {}", due.title),
                        if due.overdue {
                            format!("It has been a while since you spoke to {}", due.title)
                        } else {
                            format!("Time to catch up with {}", due.title)
                        },
                        "keep_in_touch",
                    ),
                    "person" => (
                        format!("Birthday Reminder: {}", due.title),
                        match due.offset.as_str() {
                            "0m" => format!("Today is {}'s birthday!", due.title),
                            "1d" => format!("Tomorrow is {}'s birthday!", due.title),
                            other => format!("{}'s birthday is in {}", due.title, other),
                        },
                        "birthday_upcoming",
                    ),
                    _ => (
                        format!("Upcoming Event: {}", due.title),
                        if due.offset == "0m" {
                            format!("Happening now: {}", due.title)
                        } else {
                            format!("Starts in {}", due.offset)
                        },
                        "event_upcoming",
                    ),
                };

                new_messages.push(ChatMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    message_type: "system".to_string(),
                    subtype: subtype.to_string(),
                    timestamp: now.to_rfc3339(),
                    sender: sender.clone(),
                    content: ChatContent {
                        title: title.clone(),
                        text: text.clone(),
                        metadata: json!({
                            "target_id": due.target_id.clone(),
                            "target_type": due.target_type,
                            "trigger_date": due.occurrence_date.clone(),
                            "reminder": due.offset.clone(),
                        }),
                    },
                    read_receipt: false,
                });
                notified_set.insert(key.clone());
                delivered.push(key);

                // What the notification says is worked out in one place, the
                // same one the phone's scheduler calls. A third copy here is
                // how the two platforms came to word the same reminder
                // differently — and how a birthday would have arrived on the
                // desktop titled "Upcoming Event".
                let (heading, body) = crate::calendar::scheduler::headline(&due);
                if let Err(e) = app_handle
                    .notification()
                    .builder()
                    .title(&heading)
                    .body(&body)
                    .show()
                {
                    log::error!("Failed to show notification: {}", e);
                }
            }

            if !delivered.is_empty() {
                if let Err(e) = db.record_reminder_deliveries(&delivered, now.timestamp()) {
                    log::error!("Could not record what was announced: {}", e);
                }
            }

            // 2. Once an hour, notice things nobody asked about.
            //
            // Hourly rather than per tick because it reads every thread, every
            // memory and every run still on disk, and because none of what it
            // finds is news that decays in a minute — a thread stuck for three
            // weeks is still stuck at ten past.
            //
            // Everything the sweep needs from the index is read here, inside
            // the lock that is already held. The runs come off disk afterwards,
            // outside it. See `syn::notice`.
            let for_the_sweep = if now.timestamp() % 3600 < 60
                && crate::syn::settings::load_settings(&vault_path)
                    .map(|s| s.enabled)
                    .unwrap_or(true)
            {
                Some((
                    crate::syn::thread::all(&db).unwrap_or_default(),
                    crate::syn::memory::all(&db).unwrap_or_default(),
                    crate::syn::skill::all(&db).unwrap_or_default(),
                    // A month back, not the reminder loop's one-day catch-up
                    // window: with that window a thread stuck for three weeks
                    // would announce itself again every couple of days. See
                    // `notice::SAID_FOR_DAYS`.
                    db.delivered_reminders(
                        (now
                            - chrono::Duration::try_days(crate::syn::notice::SAID_FOR_DAYS)
                                .unwrap_or_else(chrono::Duration::zero))
                        .timestamp(),
                    )
                    .unwrap_or_default(),
                ))
            } else {
                // Switched off, or not the hour. Either way the sweep does not
                // run at all — noticing is Syn's own initiative, and the switch
                // that turns Syn off has to reach the parts of it that speak
                // without being spoken to first.
                None
            };
            // Only worth doing now and then; a failure here costs a little
            // disk, not a wrong reminder.
            if now.timestamp() % 3600 < 60 {
                let _ = db.prune_reminder_deliveries(now.timestamp());
            }
            drop(db);

            // The sweep itself, with the lock released: `list_runs` parses
            // every run file in the vault, and holding the database while
            // reading two hundred JSON files would stall every query in the app
            // for the duration.
            if let Some((threads, memories, skills, already_said)) = for_the_sweep {
                let runs = crate::syn::run::load_all(&vault_path).unwrap_or_default();
                let found = crate::syn::notice::sweep(
                    &threads,
                    &memories,
                    &runs,
                    &skills,
                    now.with_timezone(&chrono::Utc),
                    &already_said,
                );

                if !found.is_empty() {
                    log::info!("[Syn] Noticed {} thing(s) worth saying", found.len());
                }

                let mut noticed_keys: Vec<String> = Vec::new();
                for notice in found {
                    let mut metadata = json!({});
                    // Only when there is a screen that can open it. A "view
                    // details" resolving to nothing is worse than no button —
                    // see `notice::Notice::target`.
                    if let (Some(id), Some(kind)) = (&notice.target, notice.target_type) {
                        metadata["target_id"] = json!(id);
                        metadata["target_type"] = json!(kind);
                    }

                    new_messages.push(ChatMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        message_type: "system".to_string(),
                        subtype: notice.kind.subtype().to_string(),
                        timestamp: now.to_rfc3339(),
                        // Syn, and not "Synabit System". A reminder is the
                        // calendar doing its job; this is a colleague saying
                        // they noticed something, and the name is the whole
                        // difference between the two.
                        sender: ChatSender {
                            id: "syn".to_string(),
                            name: "Syn".to_string(),
                            role: "bot".to_string(),
                        },
                        content: ChatContent {
                            title: notice.title,
                            text: notice.text,
                            metadata,
                        },
                        read_receipt: false,
                    });
                    noticed_keys.push(notice.key);
                }

                // Deliberately no OS notification, unlike the reminders above.
                // A reminder is time-bound and earns the interruption; a thread
                // that has been dead for three weeks does not become urgent at
                // 09:00. It waits in the list with an unread count, which is
                // the difference between noticing and interrupting — and
                // interrupting is a later nhát, with a contract behind it.
                if !noticed_keys.is_empty() {
                    // Recovering from a poisoned lock the way the rest of this
                    // loop does. Skipping the record instead would mean saying
                    // the same three things again next hour, forever.
                    let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                    if let Err(e) = db.record_reminder_deliveries(&noticed_keys, now.timestamp()) {
                        log::error!("Could not record what was noticed: {}", e);
                    }
                }
            }

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


