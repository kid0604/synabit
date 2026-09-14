//! Reminders, on the phone.
//!
//! The computer already works out what is due and when — `calendar::reminders`,
//! asked every minute by `chat_engine` — and shows it as a notification on the
//! computer. Somebody away from the computer hears nothing, and for "8 giờ tối
//! nhắc tao mua thuốc cho con" that is the whole point missed. So whatever
//! rings the computer is sent to the paired chat as well: the same list, handed
//! over, not a second one worked out here — two lists are how the phone and the
//! computer would come to disagree about when a reminder is.
//!
//! # Kept until sent
//!
//! A reminder goes into an outbox before it is sent and leaves it once Telegram
//! has it. The minute it came due may be a minute the network is down, and the
//! loop that found it will not find it again: it records a reminder as
//! delivered the moment the computer has shown it.
//!
//! # A task can be ticked off from here
//!
//! A task left undone is announced again every day at its hour, and the only
//! way to make that stop used to be the computer. A task's reminder carries a
//! button that marks it done through `update_node` — the write Syn's own edits
//! go through, so `completed_at` is set the way every other writer sets it.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::calendar::reminders::PlannedReminder;
use crate::db::DbBridge;
use crate::error::AppResult;

use super::api::{Api, ApiError};
use super::gate::Pressed;
use super::pairing;
use super::render;
use super::service::{active_vault, current_api, with_db};
use super::words::Words;

/// Reminders waiting to be sent, oldest first by key.
pub const OUTBOX_PREFIX: &str = "telegram:remind:";

/// What a sent task reminder's button is for, by the nonce the button carries.
pub const DONE_PREFIX: &str = "telegram:done:";

/// `off` keeps reminders on the computer. Absent means on: somebody who paired
/// a phone and asked Syn to remind them expects to be reminded on it.
pub const SWITCH_KEY: &str = "telegram:reminders";

/// How a "done" button's data begins, beside a card's `k:`.
pub const DONE_BUTTON: &str = "r:";

/// How late a reminder may arrive before it says so.
///
/// The loop wakes once a minute, so anything later than this was found after
/// the computer woke up or came back online — and "starting now" said of a
/// meeting that started an hour ago is wrong.
const LATE_AFTER_MINUTES: i64 = 10;

/// How long an unsent reminder is kept: the day the loop itself looks back.
const KEEP_UNSENT_SECS: i64 = crate::calendar::reminders::CATCH_UP_DAYS * 24 * 60 * 60;

/// How long a "done" button keeps working. A task still undone by then has
/// been announced six more times, each with a button of its own.
const KEEP_BUTTON_SECS: i64 = 7 * 24 * 60 * 60;

static FLUSHING: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Deserialize)]
struct Outgoing {
    text: String,
    /// The task a "done" button marks, when the reminder is for one.
    #[serde(default)]
    task: Option<String>,
    kept_at: i64,
}

#[derive(Serialize, Deserialize)]
struct Button {
    task: String,
    /// What the reminder said, so the answer can be written under it.
    text: String,
    kept_at: i64,
}

pub fn is_on(db: &DbBridge) -> bool {
    db.get_kv(SWITCH_KEY).ok().flatten().as_deref() != Some("off")
}

pub fn set_on(db: &DbBridge, on: bool) -> AppResult<()> {
    if on {
        db.delete_kv(SWITCH_KEY)
    } else {
        db.set_kv(SWITCH_KEY, "off")
    }
}

fn is_late(trigger_at: NaiveDateTime, now: NaiveDateTime) -> bool {
    now - trigger_at > chrono::Duration::minutes(LATE_AFTER_MINUTES)
}

/// Queue for the phone what just came due, and send whatever is queued.
///
/// Called every minute, with nothing new almost every time — which is also how
/// a reminder that could not be sent is tried again.
pub fn hand_over(app: &AppHandle, due: &[PlannedReminder]) {
    if !due.is_empty() {
        let words = Words::here();
        let now = chrono::Local::now().naive_local();
        let kept_at = chrono::Utc::now().timestamp();
        with_db(app, |db| {
            // Nobody to send to, or not wanted here: not kept either, or a
            // pairing made next week would open with a week of old reminders.
            if pairing::paired(db).is_none() || !is_on(db) {
                return;
            }
            for due in due {
                let out = Outgoing {
                    text: words.reminder(due, is_late(due.trigger_at, now), now),
                    task: (due.target_type == "task").then(|| due.target_id.clone()),
                    kept_at,
                };
                let key = format!("{OUTBOX_PREFIX}{}:{}", due.trigger_at.format("%Y%m%d%H%M"), due.delivery_key());
                match serde_json::to_string(&out) {
                    Ok(json) => {
                        crate::error::logged("keep a reminder for Telegram", &key, db.set_kv(&key, &json));
                    }
                    Err(e) => log::error!("[Telegram] Could not write a reminder down: {e}"),
                }
            }
        });
    }
    let queued = with_db(app, |db| db.get_kv_prefix(OUTBOX_PREFIX).is_ok_and(|rows| !rows.is_empty()));
    if queued {
        flush(app.clone());
    }
}

/// Send what the outbox holds, unless that is already happening.
pub fn flush(app: AppHandle) {
    if FLUSHING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        send_queued(&app).await;
        FLUSHING.store(false, Ordering::SeqCst);
    });
}

async fn send_queued(app: &AppHandle) {
    // Not polling — no token, or a problem. Kept for when it is: `poll` flushes
    // as it starts.
    let Some(api) = current_api(app) else {
        return;
    };
    let now = chrono::Utc::now().timestamp();
    let Some(chat_id) = with_db(app, pairing::paired).map(|p| p.chat_id) else {
        drop_all(app, OUTBOX_PREFIX);
        return;
    };
    let words = Words::here();
    let mut rows = with_db(app, |db| db.get_kv_prefix(OUTBOX_PREFIX)).unwrap_or_default();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, raw) in rows {
        let out = serde_json::from_str::<Outgoing>(&raw).ok().filter(|out| now - out.kept_at <= KEEP_UNSENT_SECS);
        if let Some(out) = out {
            if let Err(e) = send(app, &api, &words, chat_id, &out, now).await {
                log::warn!("[Telegram] Could not send a reminder, trying again in a minute: {e}");
                return;
            }
        }
        crate::error::logged("clear a sent reminder", &key, with_db(app, |db| db.delete_kv(&key)));
    }
    forget_old_buttons(app, now);
}

async fn send(app: &AppHandle, api: &Api, words: &Words, chat_id: i64, out: &Outgoing, now: i64) -> Result<(), ApiError> {
    let html = render::escape(&out.text);
    let Some(task) = &out.task else {
        return api.send(chat_id, &html).await;
    };

    use base64::Engine;
    let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 6]>());
    let key = format!("{DONE_PREFIX}{nonce}");
    // Kept before it is sent, so a button pressed the moment it arrives finds
    // what it is for.
    let button = Button { task: task.clone(), text: out.text.clone(), kept_at: now };
    if let Ok(json) = serde_json::to_string(&button) {
        crate::error::logged("keep a reminder's button", &key, with_db(app, |db| db.set_kv(&key, &json)));
    }
    let rows = vec![vec![(words.done_button().to_string(), format!("{DONE_BUTTON}{nonce}"))]];
    let sent = api.send_buttons(chat_id, &html, &rows).await.map(|_| ());
    if sent.is_err() {
        crate::error::logged("forget an unsent button", &key, with_db(app, |db| db.delete_kv(&key)));
    }
    sent
}

fn drop_all(app: &AppHandle, prefix: &str) {
    with_db(app, |db| {
        for (key, _) in db.get_kv_prefix(prefix).unwrap_or_default() {
            crate::error::logged("forget a Telegram reminder", &key, db.delete_kv(&key));
        }
    });
}

fn forget_old_buttons(app: &AppHandle, now: i64) {
    with_db(app, |db| {
        for (key, raw) in db.get_kv_prefix(DONE_PREFIX).unwrap_or_default() {
            let old = serde_json::from_str::<Button>(&raw).map_or(true, |b| now - b.kept_at > KEEP_BUTTON_SECS);
            if old {
                crate::error::logged("forget an old reminder button", &key, db.delete_kv(&key));
            }
        }
    });
}

/// "Done" pressed under a task's reminder. `false` only when the press could
/// not be dealt with and should be seen again, which never happens here: a
/// failed write is logged and the button stays for another try.
pub async fn answer_done(app: &AppHandle, api: &Api, words: &Words, pressed: &Pressed) -> bool {
    let quiet = |text: Option<&'static str>| async move {
        if let Err(e) = api.answer_callback(&pressed.callback_id, text).await {
            log::info!("[Telegram] Could not answer a button: {e}");
        }
    };

    let key = format!("{DONE_PREFIX}{}", pressed.data.trim_start_matches(DONE_BUTTON));
    let button: Option<Button> =
        with_db(app, |db| db.get_kv(&key).ok().flatten()).and_then(|raw| serde_json::from_str(&raw).ok());
    let (Some(button), Some(vault)) = (button, active_vault(app)) else {
        quiet(Some(words.task_gone())).await;
        return true;
    };

    let said = {
        let db = app.state::<crate::db::DbState>();
        let ctx = crate::syn::tools::ToolContext { db: db.inner(), vault_path: &vault, app, run_id: None };
        crate::syn::tools::execute_tool(
            &ctx,
            "update_node",
            &serde_json::json!({ "node_id": button.task, "properties": { "status": "done" } }),
        )
    };
    let settle = |outcome: &str| format!("{}\n\n→ {}", render::escape(&button.text), render::escape(outcome));

    match said.as_deref().map(written) {
        Ok(true) => {
            quiet(None).await;
            if let Err(e) = api.edit(pressed.chat_id, pressed.message_id, &settle(words.marked_done())).await {
                log::info!("[Telegram] Could not mark a reminder done: {e}");
            }
            crate::error::logged("forget a used reminder button", &key, with_db(app, |db| db.delete_kv(&key)));
        }
        // Moved, renamed or deleted since. The button can do nothing now.
        Ok(false) => {
            quiet(Some(words.task_gone())).await;
            let _ = api.edit(pressed.chat_id, pressed.message_id, &settle(words.task_gone())).await;
            crate::error::logged("forget a stale reminder button", &key, with_db(app, |db| db.delete_kv(&key)));
        }
        Err(e) => {
            log::error!("[Telegram] Could not mark a task done: {e}");
            quiet(None).await;
        }
    }
    true
}

/// Whether `update_node` wrote. It reports a note it could not find in what it
/// returns rather than as an error, because that is what the model reads.
fn written(said: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(said).map_or(true, |value| value.get("error").is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").expect("a time")
    }

    /// The loop wakes every minute; only a reminder found long after its
    /// moment says it is late.
    #[test]
    fn a_reminder_is_late_only_when_the_computer_was_not_there_for_it() {
        assert!(!is_late(at("2026-09-14 20:00"), at("2026-09-14 20:01")));
        assert!(!is_late(at("2026-09-14 20:00"), at("2026-09-14 20:10")));
        assert!(is_late(at("2026-09-14 20:00"), at("2026-09-14 21:30")));
    }

    #[test]
    fn a_task_not_found_is_not_counted_as_done() {
        assert!(written(r#"{"ok":true,"node_id":"Tasks/Mua thuốc.md"}"#));
        assert!(!written(r#"{"error":"Node not found","node_id":"Tasks/Mua thuốc.md"}"#));
    }

    #[test]
    fn reminders_are_on_until_switched_off() {
        let db = DbBridge::new_in_memory_full().expect("db");
        assert!(is_on(&db));
        set_on(&db, false).expect("off");
        assert!(!is_on(&db));
        set_on(&db, true).expect("on");
        assert!(is_on(&db));
    }
}
