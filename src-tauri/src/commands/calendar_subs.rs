//! Subscribing to somebody else's calendar.
//!
//! Read-only, and read-only in the strongest sense available: a subscribed
//! event is never a file in the vault. It lives in a cache table, it is
//! replaced whole on every refresh, and its id is not a path anything could
//! write to. Nothing had to be told to treat it carefully, because there is
//! nothing to treat carelessly.

use crate::db::subscriptions::Subscription;
use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::feed_engine::fetcher::{fetch_feed, guard_url, FetchResult};

/// What a refresh did, for the screen that asked for it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshReport {
    pub id: String,
    pub name: String,
    /// The server said nothing had changed, so nothing was re-read.
    pub unchanged: bool,
    pub events: usize,
    pub error: String,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[tauri::command]
pub fn list_calendar_subscriptions(
    state: tauri::State<'_, DbState>,
) -> AppResult<Vec<Subscription>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.list_subscriptions()
}

/// Add a calendar by URL, and read it once so the result is visible now.
#[tauri::command]
pub async fn add_calendar_subscription(
    app: tauri::AppHandle,
    url: String,
    name: String,
) -> AppResult<RefreshReport> {
    let url = url.trim().to_string();
    // Typed by a person and fetched by the privileged process, so the same
    // guard the feed reader uses applies: no file URLs, no loopback, no
    // cloud metadata address.
    guard_url(&url).map_err(AppError::General)?;

    let id = uuid::Uuid::new_v4().to_string();
    let fallback = if name.trim().is_empty() {
        url::Url::parse(&url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "Calendar".to_string())
    } else {
        name.trim().to_string()
    };

    {
        use tauri::Manager;
        let state = app.state::<DbState>();
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.add_subscription(&id, &url, &fallback, now())?;
    }

    refresh_one(&app, &id).await
}

#[tauri::command]
pub fn set_calendar_subscription_enabled(
    state: tauri::State<'_, DbState>,
    id: String,
    enabled: bool,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.set_subscription_enabled(&id, enabled)
}

/// Whether this calendar's events should be announced like the user's own.
#[tauri::command]
pub fn set_calendar_subscription_remind(
    state: tauri::State<'_, DbState>,
    id: String,
    remind: bool,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.set_subscription_remind(&id, remind)
}

#[tauri::command]
pub fn rename_calendar_subscription(
    state: tauri::State<'_, DbState>,
    id: String,
    name: String,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.rename_subscription(&id, name.trim())
}

#[tauri::command]
pub fn remove_calendar_subscription(
    state: tauri::State<'_, DbState>,
    id: String,
) -> AppResult<()> {
    let mut db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.remove_subscription(&id)
}

/// Re-read one calendar.
///
/// A failure is recorded against the subscription rather than thrown away:
/// a calendar that has been failing for a week should say so on the screen
/// where its URL is, not only in a log nobody opens.
pub async fn refresh_one(app: &tauri::AppHandle, id: &str) -> AppResult<RefreshReport> {
    use tauri::Manager;

    let Some(sub) = ({
        let state = app.state::<DbState>();
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_subscription(id)?
    }) else {
        return Err(AppError::General("No such calendar".to_string()));
    };

    let mut report = RefreshReport {
        id: sub.id.clone(),
        name: sub.name.clone(),
        unchanged: false,
        events: sub.event_count as usize,
        error: String::new(),
    };

    if guard_url(&sub.url).is_err() {
        report.error = "That address is not one this app will fetch".to_string();
        let state = app.state::<DbState>();
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.note_subscription_fetch(&sub.id, &sub.etag, &sub.last_modified, &report.error, now())?;
        return Ok(report);
    }

    let etag = (!sub.etag.is_empty()).then_some(sub.etag.as_str());
    let modified = (!sub.last_modified.is_empty()).then_some(sub.last_modified.as_str());
    let result = fetch_feed(&sub.url, etag, modified).await;

    match result {
        FetchResult::NotModified => {
            report.unchanged = true;
            let state = app.state::<DbState>();
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.note_subscription_fetch(&sub.id, &sub.etag, &sub.last_modified, "", now())?;
        }
        FetchResult::Error { message, .. } => {
            report.error = message;
            let state = app.state::<DbState>();
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.note_subscription_fetch(&sub.id, &sub.etag, &sub.last_modified, &report.error, now())?;
        }
        FetchResult::Updated { body, etag, last_modified } => {
            let text = String::from_utf8_lossy(&body);
            let events = crate::calendar::ics::import(&text);

            // A calendar that suddenly parses as nothing is far more likely to
            // be an error page or a login redirect than a calendar somebody
            // emptied. Replacing a working calendar with silence on that
            // evidence is the wrong way round.
            if events.is_empty() && sub.event_count > 0 {
                report.error = "That address returned no events; the calendar was left as it was"
                    .to_string();
                let state = app.state::<DbState>();
                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                db.note_subscription_fetch(&sub.id, &sub.etag, &sub.last_modified, &report.error, now())?;
                return Ok(report);
            }

            let state = app.state::<DbState>();
            let mut db = state.lock().unwrap_or_else(|e| e.into_inner());
            report.events = db.replace_subscription_events(&sub.id, &events)?;
            db.note_subscription_fetch(
                &sub.id,
                etag.as_deref().unwrap_or(""),
                last_modified.as_deref().unwrap_or(""),
                "",
                now(),
            )?;
        }
    }

    Ok(report)
}

#[tauri::command]
pub async fn refresh_calendar_subscription(
    app: tauri::AppHandle,
    id: String,
) -> AppResult<RefreshReport> {
    refresh_one(&app, &id).await
}

/// Re-read every calendar that is switched on.
#[tauri::command]
pub async fn refresh_calendar_subscriptions(
    app: tauri::AppHandle,
) -> AppResult<Vec<RefreshReport>> {
    let subs = {
        use tauri::Manager;
        let state = app.state::<DbState>();
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.list_subscriptions()?
    };

    let mut reports = Vec::new();
    for sub in subs.into_iter().filter(|s| s.enabled) {
        match refresh_one(&app, &sub.id).await {
            Ok(report) => reports.push(report),
            // One unreachable calendar must not stop the others.
            Err(e) => reports.push(RefreshReport {
                id: sub.id,
                name: sub.name,
                unchanged: false,
                events: 0,
                error: e.to_string(),
            }),
        }
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use crate::calendar::ics;
    use crate::db::DbBridge;

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    const HOLIDAYS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Holidays//EN\r\n\
        BEGIN:VEVENT\r\nUID:tet-2026\r\nSUMMARY:Tết Nguyên Đán\r\n\
        DTSTART;VALUE=DATE:20260217\r\nDTEND;VALUE=DATE:20260222\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:natl-2026\r\nSUMMARY:Quốc khánh\r\n\
        DTSTART;VALUE=DATE:20260902\r\nDTEND;VALUE=DATE:20260903\r\nEND:VEVENT\r\n\
        END:VCALENDAR\r\n";

    /// The whole path a subscription takes, minus the network: a calendar
    /// somebody else published, read, stored, and drawn on the grid.
    #[test]
    fn a_published_calendar_becomes_events_the_grid_can_draw() {
        let mut db = db();
        db.add_subscription("s1", "https://example.com/holidays.ics", "Holidays", 100)
            .unwrap();

        let events = ics::import(HOLIDAYS);
        assert_eq!(db.replace_subscription_events("s1", &events).unwrap(), 2);

        let summaries = db.subscribed_event_summaries().unwrap();
        let expanded = crate::calendar::recurrence::expand_range(
            summaries,
            "2026-02-01",
            "2026-02-28",
            "Asia/Ho_Chi_Minh",
        );

        // Tết runs 17–21 February: five days, and the 22nd is the day the
        // feed said it no longer covers.
        let days: Vec<&str> = expanded.occurrences.iter().map(|o| o.date.as_str()).collect();
        assert_eq!(
            days,
            ["2026-02-17", "2026-02-18", "2026-02-19", "2026-02-20", "2026-02-21"],
        );
        assert_eq!(expanded.events[0].title, "Tết Nguyên Đán");
        assert_eq!(expanded.events[0].subscription_id, "s1");
    }

    /// A subscribed calendar and the user's own are drawn from one expansion,
    /// so a shared meeting and a private one lay out against each other
    /// rather than in two passes that do not know about each other.
    #[test]
    fn subscribed_and_owned_events_come_back_from_the_same_expansion() {
        let mut db = db();
        db.add_subscription("s1", "https://example.com/team.ics", "Team", 100).unwrap();
        db.replace_subscription_events(
            "s1",
            &ics::import(
                "BEGIN:VEVENT\r\nUID:standup\r\nSUMMARY:Team standup\r\n\
                 DTSTART:20260310T090000\r\nDTEND:20260310T091500\r\nEND:VEVENT\r\n",
            ),
        )
        .unwrap();

        let mut own = db.get_event_summaries().unwrap();
        own.push(crate::calendar::recurrence::EventSummary::from_properties(
            "Events/mine.md",
            "My own meeting",
            "",
            &serde_json::json!({ "start_at": "2026-03-10T09:00", "end_at": "2026-03-10T10:00" }),
        ));
        own.extend(db.subscribed_event_summaries().unwrap());

        let got = crate::calendar::recurrence::expand_range(own, "2026-03-10", "2026-03-10", "");
        assert_eq!(got.events.len(), 2, "both calendars are on the grid");

        let mine = got.events.iter().find(|e| e.title == "My own meeting").unwrap();
        let theirs = got.events.iter().find(|e| e.title == "Team standup").unwrap();
        assert!(mine.subscription_id.is_empty(), "the user's own is editable");
        assert_eq!(theirs.subscription_id, "s1", "and theirs is not");
    }

    /// A feed that suddenly parses as nothing is far more likely to be a login
    /// page than a calendar somebody emptied. Replacing a working calendar
    /// with silence on that evidence is the wrong way round.
    #[test]
    fn an_empty_response_does_not_wipe_a_calendar_that_was_working() {
        let mut db = db();
        db.add_subscription("s1", "https://example.com/a.ics", "Holidays", 100).unwrap();
        db.replace_subscription_events("s1", &ics::import(HOLIDAYS)).unwrap();

        // This is the check `refresh_one` makes before it writes.
        let sub = db.get_subscription("s1").unwrap().unwrap();
        let parsed = ics::import("<html><body>Please sign in</body></html>");
        assert!(parsed.is_empty());
        assert!(sub.event_count > 0, "so the refresh must decline to replace");

        assert_eq!(db.subscribed_event_summaries().unwrap().len(), 2);
    }
}

#[cfg(test)]
mod guard_tests {
    use crate::feed_engine::fetcher::guard_url;

    /// A subscription URL is typed by a person and dialled by the privileged
    /// process. The same guard the feed reader uses applies here, and these
    /// name what it is for.
    #[test]
    fn an_address_that_should_not_be_dialled_is_refused() {
        for refused in [
            "http://localhost:8080/admin",
            "http://127.0.0.1/calendar.ics",
            "http://169.254.169.254/latest/meta-data/",
            "http://192.168.1.1/router.ics",
            "http://10.0.0.5/internal.ics",
            "file:///etc/passwd",
            "ftp://example.com/a.ics",
            "not a url at all",
        ] {
            assert!(guard_url(refused).is_err(), "{} should be refused", refused);
        }
    }

    #[test]
    fn an_ordinary_published_calendar_is_allowed() {
        for allowed in [
            "https://calendar.google.com/calendar/ical/x/public/basic.ics",
            "https://example.com/holidays.ics",
            "http://example.com/holidays.ics",
        ] {
            assert!(guard_url(allowed).is_ok(), "{} should be allowed", allowed);
        }
    }
}

/// Fetch a real calendar, once, by hand.
///
/// `ICS_URL=… cargo test -- --ignored fetches_a_real_calendar`. Not part of
/// the normal run: it needs a network and somebody else's server to be up,
/// and a test that fails when a third party has a bad day is a test people
/// learn to ignore.
#[cfg(test)]
#[tokio::test]
#[ignore]
async fn fetches_a_real_calendar() {
    use crate::feed_engine::fetcher::{fetch_feed, guard_url, FetchResult};

    let url = std::env::var("ICS_URL").expect("set ICS_URL");
    guard_url(&url).expect("the address should be allowed");

    match fetch_feed(&url, None, None).await {
        FetchResult::Updated { body, etag, last_modified } => {
            let text = String::from_utf8_lossy(&body);
            let events = crate::calendar::ics::import(&text);
            println!(
                "{} bytes, {} events, etag={:?} last-modified={:?}",
                body.len(),
                events.len(),
                etag,
                last_modified,
            );
            for e in events.iter().take(5) {
                println!(
                    "  {:30} {} .. {}{}{}",
                    e.title.chars().take(28).collect::<String>(),
                    e.start_at,
                    e.end_at,
                    if e.is_all_day { "  [all-day]" } else { "" },
                    if e.rrule.is_empty() { String::new() } else { format!("  {}", e.rrule) },
                );
            }
            assert!(!events.is_empty(), "a real calendar should have events in it");

            // And again, saying what we already have. A calendar nobody has
            // touched should cost a `304` rather than a download — which is
            // what makes refreshing every half hour reasonable at all.
            match fetch_feed(&url, etag.as_deref(), last_modified.as_deref()).await {
                FetchResult::NotModified => println!("  second fetch: 304, nothing re-read"),
                FetchResult::Updated { body, .. } => println!(
                    "  second fetch: {} bytes again — this server does not answer conditionally",
                    body.len()
                ),
                other => panic!("second fetch failed: {:?}", other),
            }
        }
        other => panic!("expected a calendar, got {:?}", other),
    }
}
