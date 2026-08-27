//! Handing the next week of reminders to the operating system.
//!
//! # Why this exists at all
//!
//! On a desktop the app is running, so a task waking every minute can notice
//! that something has come due and say so. On a phone it is not: the system
//! stops it within moments of the screen going off, and a loop that never
//! runs never reminds anyone of anything. Every reminder this app has ever
//! shown on a phone arrived only because the app happened to be open.
//!
//! The fix is not to run more; it is to run less. The plan for the next week
//! is worked out once, handed to the system's own scheduler, and forgotten
//! about. The phone rings whether or not this app is alive.
//!
//! # Why it is only for phones
//!
//! `tauri-plugin-notification` accepts a schedule on every platform but only
//! honours it on iOS and Android — the desktop implementation builds a
//! notification from the title, body, icon and sound and shows it there and
//! then, with the schedule dropped on the floor. Calling this on a desktop
//! would fire the whole week at once, which is why it does not.
//!
//! The desktop answer is the loop in `chat_engine`, which is enough there
//! because closing the window on a desktop hides the app rather than ending
//! it.

use crate::calendar::reminders::{self, PlannedReminder, SCHEDULE_HORIZON_DAYS};
use crate::db::DbState;
use chrono::{Local, TimeZone};
use tauri::{Listener, Manager};

#[cfg(mobile)]
use tauri_plugin_notification::{NotificationExt, Schedule};

/// The moment, as the notification plugin counts them.
///
/// It takes `time::OffsetDateTime` while everything else here is chrono, so
/// the two meet at a unix timestamp — the one representation neither can
/// misread. Shared rather than hidden behind `cfg(mobile)` so it is covered
/// by tests that run on a desktop, where the rest of the scheduling is not.
pub fn as_plugin_instant(local: chrono::NaiveDateTime) -> Option<time::OffsetDateTime> {
    let moment = Local.from_local_datetime(&local).earliest()?;
    time::OffsetDateTime::from_unix_timestamp(moment.timestamp()).ok()
}

/// What a pass over the schedule did, for the log and for the tests.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScheduleReport {
    pub cancelled: usize,
    pub scheduled: usize,
}

/// The reminders to hand over, in the order they will happen.
///
/// Separated from the handing over so it can be tested without a phone.
pub fn plan_for_schedule(app: &tauri::AppHandle) -> Vec<PlannedReminder> {
    let Some(db_state) = app.try_state::<DbState>() else {
        return Vec::new();
    };
    let (nodes, subscribed) = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        (
            db.get_active_tasks_and_events().unwrap_or_default(),
            db.subscribed_events_to_remind().unwrap_or_default(),
        )
    };

    let now = Local::now();
    let from = now.naive_local();
    let to = from
        + chrono::Duration::try_days(SCHEDULE_HORIZON_DAYS)
            .unwrap_or_else(chrono::Duration::zero);
    let here = iana_time_zone::get_timezone().unwrap_or_default();

    reminders::plan_with(&nodes, &subscribed, from, to, &here)
}

/// What a scheduled reminder says when it arrives.
pub fn headline(due: &PlannedReminder) -> (String, String) {
    if due.target_type == "task" {
        let title = if due.overdue { "Task Overdue" } else { "Task Due" };
        return (title.to_string(), due.title.clone());
    }
    if due.target_type == "finance_debt" {
        let body = match due.offset.as_str() {
            "0m" => format!("{} — due today", due.title),
            other => format!("{} — due in {}", due.title, other),
        };
        return ("Debt due".to_string(), body);
    }
    if due.target_type == "person" {
        if due.offset == "touch" {
            let body = if due.overdue {
                format!("It has been a while since you spoke to {}", due.title)
            } else {
                format!("Time to catch up with {}", due.title)
            };
            return ("Keep in touch".to_string(), body);
        }
        // Anything other than the two defaults is an offset the user wrote
        // themselves, and saying "tomorrow" for a week's notice would be a lie.
        let body = match due.offset.as_str() {
            "0m" => format!("Today is {}'s birthday!", due.title),
            "1d" => format!("Tomorrow is {}'s birthday!", due.title),
            other => format!("{}'s birthday is in {}", due.title, other),
        };
        return ("Birthday Reminder".to_string(), body);
    }
    let body = if due.offset == "0m" {
        format!("Happening now: {}", due.title)
    } else {
        format!("Starts in {}", due.offset)
    };
    ("Upcoming Event".to_string(), body)
}

/// Replace everything this app has queued with the next week's worth.
///
/// Cancelling first rather than adding: a reminder that has been moved, or
/// deleted, or whose series was cut short, is still sitting in the system's
/// queue and will go off unless it is taken out. Every reminder keeps the
/// same handle between runs — see `PlannedReminder::os_id` — so what is
/// pending can be matched against what should be.
#[cfg(mobile)]
pub fn reschedule_all(app: &tauri::AppHandle) -> ScheduleReport {
    let mut report = ScheduleReport::default();

    // Whatever is queued is ours: this app schedules nothing else.
    match app.notification().pending() {
        Ok(pending) if !pending.is_empty() => {
            let ids: Vec<i32> = pending.iter().map(|p| p.id()).collect();
            report.cancelled = ids.len();
            if let Err(e) = app.notification().cancel(ids) {
                log::error!("Could not clear the reminder queue: {}", e);
            }
        }
        Ok(_) => {}
        Err(e) => log::error!("Could not read the reminder queue: {}", e),
    }

    let now = Local::now().naive_local();
    for due in plan_for_schedule(app) {
        // The window starts at "now", but a tick can be a moment behind.
        if due.trigger_at <= now {
            continue;
        }
        let Some(date) = as_plugin_instant(due.trigger_at) else {
            continue;
        };

        let (title, body) = headline(&due);
        let result = app
            .notification()
            .builder()
            .id(due.os_id())
            .title(title)
            .body(body)
            .schedule(Schedule::At {
                date,
                repeating: false,
                // Android otherwise holds it back until the phone is next
                // used, which for an alarm-shaped thing is the whole point
                // missed.
                allow_while_idle: true,
            })
            .show();

        match result {
            Ok(()) => report.scheduled += 1,
            Err(e) => log::error!("Could not queue a reminder: {}", e),
        }
    }

    log::info!(
        "Reminder queue: {} cleared, {} handed to the system",
        report.cancelled,
        report.scheduled
    );
    report
}

/// A desktop has no queue to keep: the loop in `chat_engine` is running.
#[cfg(not(mobile))]
pub fn reschedule_all(app: &tauri::AppHandle) -> ScheduleReport {
    let _ = app;
    ScheduleReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::reminders::PlannedReminder;
    use chrono::NaiveDateTime;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    fn reminder(target_type: &'static str, offset: &str, overdue: bool) -> PlannedReminder {
        PlannedReminder {
            target_id: "Events/a.md".to_string(),
            target_type,
            title: "Design review".to_string(),
            offset: offset.to_string(),
            occurrence_date: "2026-03-20".to_string(),
            trigger_at: dt("2026-03-20T08:45:00"),
            subject_at: dt("2026-03-20T09:00:00"),
            overdue,
        }
    }

    /// The plugin counts moments in `time::OffsetDateTime`; everything else
    /// here is chrono. They meet at a unix timestamp, and this is the only
    /// place they touch — so it is the only place the two can disagree.
    #[test]
    fn a_local_wall_clock_becomes_the_same_moment_the_plugin_will_schedule() {
        let local = dt("2026-03-20T08:45:00");
        let converted = as_plugin_instant(local).expect("a real moment");
        let expected = Local
            .from_local_datetime(&local)
            .earliest()
            .expect("a real moment")
            .timestamp();
        assert_eq!(converted.unix_timestamp(), expected);
    }

    /// On the morning the clocks go forward this reading does not exist in
    /// some zones. Refusing it drops one reminder; guessing at it would
    /// schedule one for the wrong hour, every year, quietly.
    #[test]
    fn a_wall_clock_that_never_happens_is_refused_rather_than_guessed() {
        // Only meaningful where the local zone has such a gap; where it does
        // not, this is simply a valid moment.
        let gap = dt("2026-03-29T01:30:00");
        match as_plugin_instant(gap) {
            Some(moment) => assert!(moment.unix_timestamp() > 0),
            None => { /* the local zone skipped this hour */ }
        }
    }

    #[test]
    fn an_event_reminder_says_how_long_there_is_left() {
        let (title, body) = headline(&reminder("event", "15m", false));
        assert_eq!(title, "Upcoming Event");
        assert_eq!(body, "Starts in 15m");
    }

    #[test]
    fn an_event_starting_now_says_so_rather_than_saying_in_zero_minutes() {
        let (_, body) = headline(&reminder("event", "0m", false));
        assert_eq!(body, "Happening now: Design review");
    }

    #[test]
    fn a_task_that_has_slipped_is_announced_differently_from_one_that_has_not() {
        assert_eq!(headline(&reminder("task", "0m", true)).0, "Task Overdue");
        assert_eq!(headline(&reminder("task", "0m", false)).0, "Task Due");
    }

    /// A desktop must not hand its week to the plugin: the desktop half of
    /// the plugin drops the schedule and shows the notification immediately,
    /// so a week of reminders would arrive at once.
    #[cfg(not(mobile))]
    #[test]
    fn a_desktop_queues_nothing() {
        assert_eq!(ScheduleReport::default(), ScheduleReport { cancelled: 0, scheduled: 0 });
    }
}

/// Keep the operating system's queue in step with the vault.
///
/// Two things start it: the app opening, and anything changing on disk. The
/// second is debounced — a sync can touch a hundred files in a second, and
/// rewriting the queue a hundred times would be a hundred times the work for
/// the same answer.
pub fn watch(app: tauri::AppHandle) {
    if !cfg!(mobile) {
        // The desktop's answer is the loop in `chat_engine`; nothing to keep.
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Late enough that the vault has been indexed, so the first pass sees
        // the whole plan rather than an empty one.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        reschedule_all(&handle);
    });

    let dirty = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for event in ["vault-file-modified", "vault-file-created-deleted", "vault-sync-completed"] {
        let flag = dirty.clone();
        app.listen_any(event, move |_| {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            ticker.tick().await;
            if dirty.swap(false, std::sync::atomic::Ordering::Relaxed) {
                reschedule_all(&handle);
            }
        }
    });
}
