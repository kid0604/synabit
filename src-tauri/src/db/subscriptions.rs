use crate::calendar::ics::ImportedEvent;
use crate::calendar::recurrence::EventSummary;
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// A calendar somebody else publishes, that this vault reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: String,
    pub url: String,
    pub name: String,
    pub colour: String,
    pub enabled: bool,
    /// Announce these events the way the user's own are announced. */
    pub remind: bool,
    pub etag: String,
    pub last_modified: String,
    pub last_fetched_at: i64,
    pub last_error: String,
    pub event_count: i64,
    pub created_at: i64,
}

/// Colours a subscription can be given, so several of them can be told apart.
///
/// Fixed rather than free: a picker is one more decision to make at the moment
/// someone is trying to paste a URL, and any colour that reads badly on one of
/// the two themes is a bug waiting to be filed.
pub const SUBSCRIPTION_COLOURS: [&str; 6] =
    ["teal", "amber", "rose", "violet", "sky", "lime"];

impl DbBridge {
    pub fn list_subscriptions(&self) -> AppResult<Vec<Subscription>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, url, name, colour, enabled, etag, last_modified,
                        last_fetched_at, last_error, event_count, created_at, remind
                 FROM calendar_subscriptions ORDER BY created_at",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (subscriptions): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Subscription {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    name: row.get(2)?,
                    colour: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? != 0,
                    etag: row.get(5)?,
                    last_modified: row.get(6)?,
                    last_fetched_at: row.get(7)?,
                    last_error: row.get(8)?,
                    event_count: row.get(9)?,
                    created_at: row.get(10)?,
                    remind: row.get::<_, i64>(11)? != 0,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (subscriptions): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    pub fn get_subscription(&self, id: &str) -> AppResult<Option<Subscription>> {
        Ok(self.list_subscriptions()?.into_iter().find(|s| s.id == id))
    }

    /// Add a calendar, giving it whichever colour is least used so far.
    pub fn add_subscription(&self, id: &str, url: &str, name: &str, now: i64) -> AppResult<Subscription> {
        let existing = self.list_subscriptions()?;
        let colour = SUBSCRIPTION_COLOURS
            .iter()
            .min_by_key(|c| existing.iter().filter(|s| &s.colour == *c).count())
            .copied()
            .unwrap_or("teal");

        self.conn
            .execute(
                "INSERT INTO calendar_subscriptions (id, url, name, colour, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, url, name, colour, now],
            )
            .map_err(|e| AppError::General(format!("DB Write Error (subscriptions): {}", e)))?;

        self.get_subscription(id)?
            .ok_or_else(|| AppError::General("The subscription did not save".to_string()))
    }

    pub fn set_subscription_enabled(&self, id: &str, enabled: bool) -> AppResult<()> {
        self.conn
            .execute(
                "UPDATE calendar_subscriptions SET enabled = ?2 WHERE id = ?1",
                params![id, if enabled { 1 } else { 0 }],
            )
            .map_err(|e| AppError::General(format!("DB Write Error (subscriptions): {}", e)))?;
        Ok(())
    }

    pub fn set_subscription_remind(&self, id: &str, remind: bool) -> AppResult<()> {
        self.conn
            .execute(
                "UPDATE calendar_subscriptions SET remind = ?2 WHERE id = ?1",
                params![id, if remind { 1 } else { 0 }],
            )
            .map_err(|e| AppError::General(format!("DB Write Error (subscriptions): {}", e)))?;
        Ok(())
    }

    /// The subscribed events that asked to be announced.
    ///
    /// Separate from `subscribed_event_summaries`, which is what the grid
    /// draws: being on the calendar and being worth interrupting somebody for
    /// are different questions.
    pub fn subscribed_events_to_remind(&self) -> AppResult<Vec<EventSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT e.subscription_id, e.uid, e.payload
                 FROM calendar_subscription_events e
                 JOIN calendar_subscriptions s ON s.id = e.subscription_id
                 WHERE s.enabled = 1 AND s.remind = 1",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (remindable): {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (remindable): {}", e)))?;

        let mut out = Vec::new();
        for (subscription_id, uid, payload) in rows.flatten() {
            let Ok(event) = serde_json::from_str::<ImportedEvent>(&payload) else { continue };
            out.push(summary_of(&subscription_id, &uid, &event));
        }
        Ok(out)
    }

    pub fn rename_subscription(&self, id: &str, name: &str) -> AppResult<()> {
        self.conn
            .execute(
                "UPDATE calendar_subscriptions SET name = ?2 WHERE id = ?1",
                params![id, name],
            )
            .map_err(|e| AppError::General(format!("DB Write Error (subscriptions): {}", e)))?;
        Ok(())
    }

    /// Remove a calendar and everything it put here.
    ///
    /// In one transaction: a subscription without its events is a calendar
    /// that shows nothing, but events without their subscription are events
    /// nothing can explain, colour or ever clean up.
    pub fn remove_subscription(&mut self, id: &str) -> AppResult<()> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB Tx Error (subscriptions): {}", e)))?;
        tx.execute(
            "DELETE FROM calendar_subscription_events WHERE subscription_id = ?1",
            params![id],
        )
        .map_err(|e| AppError::General(format!("DB Delete Error (subscription events): {}", e)))?;
        tx.execute("DELETE FROM calendar_subscriptions WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Error (subscriptions): {}", e)))?;
        tx.commit()
            .map_err(|e| AppError::General(format!("DB Commit Error (subscriptions): {}", e)))
    }

    /// Record that a refresh happened, whatever came of it.
    pub fn note_subscription_fetch(
        &self,
        id: &str,
        etag: &str,
        last_modified: &str,
        error: &str,
        at: i64,
    ) -> AppResult<()> {
        self.conn
            .execute(
                "UPDATE calendar_subscriptions
                 SET etag = ?2, last_modified = ?3, last_error = ?4, last_fetched_at = ?5
                 WHERE id = ?1",
                params![id, etag, last_modified, error, at],
            )
            .map_err(|e| AppError::General(format!("DB Write Error (subscriptions): {}", e)))?;
        Ok(())
    }

    /// Replace everything a calendar holds with what it holds now.
    ///
    /// Wholesale, not merged: a feed says what the calendar contains, not what
    /// changed since last time. Merging would keep every event the other end
    /// has since deleted, forever, with nothing to notice them by.
    pub fn replace_subscription_events(
        &mut self,
        id: &str,
        events: &[ImportedEvent],
    ) -> AppResult<usize> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB Tx Error (subscription events): {}", e)))?;
        tx.execute(
            "DELETE FROM calendar_subscription_events WHERE subscription_id = ?1",
            params![id],
        )
        .map_err(|e| AppError::General(format!("DB Delete Error (subscription events): {}", e)))?;

        let mut written = 0usize;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO calendar_subscription_events (subscription_id, uid, payload)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(subscription_id, uid) DO UPDATE SET payload = excluded.payload",
                )
                .map_err(|e| {
                    AppError::General(format!("DB Prepare Error (subscription events): {}", e))
                })?;
            for (index, event) in events.iter().enumerate() {
                if event.start_at.trim().is_empty() {
                    continue;
                }
                // A feed with no UIDs, or repeated ones, still has to store
                // every event it named rather than collapsing them into one.
                let uid = if event.uid.trim().is_empty() {
                    format!("row-{}", index)
                } else {
                    event.uid.clone()
                };
                let payload = serde_json::to_string(event).map_err(|e| {
                    AppError::General(format!("Could not store an event: {}", e))
                })?;
                stmt.execute(params![id, uid, payload]).map_err(|e| {
                    AppError::General(format!("DB Write Error (subscription events): {}", e))
                })?;
                written += 1;
            }
        }
        tx.execute(
            "UPDATE calendar_subscriptions SET event_count = ?2 WHERE id = ?1",
            params![id, written as i64],
        )
        .map_err(|e| AppError::General(format!("DB Write Error (subscriptions): {}", e)))?;
        tx.commit()
            .map_err(|e| AppError::General(format!("DB Commit Error (subscription events): {}", e)))?;
        Ok(written)
    }

    /// Every subscribed event, in the shape the calendar draws from.
    ///
    /// Only from calendars that are switched on: turning one off is how a
    /// person hides it without losing the URL.
    pub fn subscribed_event_summaries(&self) -> AppResult<Vec<EventSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT e.subscription_id, e.uid, e.payload
                 FROM calendar_subscription_events e
                 JOIN calendar_subscriptions s ON s.id = e.subscription_id
                 WHERE s.enabled = 1",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (subscribed): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let subscription_id: String = row.get(0)?;
                let uid: String = row.get(1)?;
                let payload: String = row.get(2)?;
                Ok((subscription_id, uid, payload))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (subscribed): {}", e)))?;

        let mut out = Vec::new();
        for (subscription_id, uid, payload) in rows.flatten() {
            let Ok(event) = serde_json::from_str::<ImportedEvent>(&payload) else {
                continue;
            };
            out.push(summary_of(&subscription_id, &uid, &event));
        }
        Ok(out)
    }
}

/// A subscribed event, as the calendar's own machinery sees it.
///
/// The id is deliberately not a vault path: nothing must be able to write to
/// it. It reads as `subscription:<id>/<uid>`, which is not a path any node
/// could have.
pub fn summary_of(subscription_id: &str, uid: &str, event: &ImportedEvent) -> EventSummary {
    EventSummary {
        id: format!("subscription:{}/{}", subscription_id, uid),
        uid: uid.to_string(),
        title: event.title.clone(),
        is_all_day: event.is_all_day,
        start_at: event.start_at.clone(),
        end_at: event.end_at.clone(),
        location: event.location.clone(),
        tags: event.tags.clone(),
        // A subscribed event wears its calendar's colour, never one of its own.
        colour: String::new(),
        tzid: event.tzid.clone(),
        rrule: event.rrule.clone(),
        recurrence: String::new(),
        recurrence_end_at: String::new(),
        series_id: String::new(),
        exceptions: event.exceptions.clone(),
        reminders: Vec::new(),
        relations: Vec::new(),
        created_at: String::new(),
        subscription_id: subscription_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    fn event(uid: &str, start: &str) -> ImportedEvent {
        ImportedEvent {
            uid: uid.to_string(),
            title: format!("Event {}", uid),
            start_at: start.to_string(),
            end_at: start.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_calendar_can_be_added_listed_and_removed() {
        let mut db = db();
        db.add_subscription("s1", "https://example.com/a.ics", "Team", 100).unwrap();
        let all = db.list_subscriptions().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://example.com/a.ics");
        assert!(all[0].enabled, "a calendar starts switched on");
        assert!(!all[0].colour.is_empty(), "and with a colour of its own");

        db.remove_subscription("s1").unwrap();
        assert!(db.list_subscriptions().unwrap().is_empty());
    }

    /// Several calendars have to be tellable apart at a glance.
    #[test]
    fn calendars_are_given_different_colours_until_the_palette_runs_out() {
        let db = db();
        let mut colours = Vec::new();
        for i in 0..SUBSCRIPTION_COLOURS.len() {
            let id = format!("s{}", i);
            colours.push(db.add_subscription(&id, "https://e.com/a.ics", "x", 100).unwrap().colour);
        }
        colours.sort();
        colours.dedup();
        assert_eq!(colours.len(), SUBSCRIPTION_COLOURS.len(), "no two share one");
    }

    /// A feed says what a calendar contains now, not what changed. Merging
    /// would keep every event the other end has since deleted, forever.
    #[test]
    fn a_refresh_replaces_what_the_calendar_holds_rather_than_adding_to_it() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();

        db.replace_subscription_events("s1", &[event("a", "2026-03-10"), event("b", "2026-03-11")])
            .unwrap();
        assert_eq!(db.subscribed_event_summaries().unwrap().len(), 2);

        // The other end deleted `b` and added `c`.
        db.replace_subscription_events("s1", &[event("a", "2026-03-10"), event("c", "2026-03-12")])
            .unwrap();
        let uids: Vec<String> = db
            .subscribed_event_summaries()
            .unwrap()
            .into_iter()
            .map(|e| e.uid)
            .collect();
        assert_eq!(uids.len(), 2);
        assert!(uids.contains(&"a".to_string()));
        assert!(uids.contains(&"c".to_string()));
        assert!(!uids.contains(&"b".to_string()), "a deleted event has to go");
    }

    /// Switching a calendar off hides it without losing the URL.
    #[test]
    fn a_calendar_switched_off_draws_nothing_but_is_still_there() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();
        db.replace_subscription_events("s1", &[event("a", "2026-03-10")]).unwrap();

        db.set_subscription_enabled("s1", false).unwrap();
        assert!(db.subscribed_event_summaries().unwrap().is_empty());
        assert_eq!(db.list_subscriptions().unwrap().len(), 1);

        db.set_subscription_enabled("s1", true).unwrap();
        assert_eq!(db.subscribed_event_summaries().unwrap().len(), 1);
    }

    /// Off by default, and deliberately: a holidays feed announcing every
    /// holiday at midnight is noise, and only the person who pasted the URL
    /// knows which kind of calendar it is.
    #[test]
    fn a_subscribed_calendar_says_nothing_until_it_is_asked_to() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Holidays", 100).unwrap();
        db.replace_subscription_events("s1", &[event("a", "2026-03-10")]).unwrap();

        assert!(!db.get_subscription("s1").unwrap().unwrap().remind);
        assert!(db.subscribed_events_to_remind().unwrap().is_empty());
        // But it is still drawn on the calendar.
        assert_eq!(db.subscribed_event_summaries().unwrap().len(), 1);

        db.set_subscription_remind("s1", true).unwrap();
        assert_eq!(db.subscribed_events_to_remind().unwrap().len(), 1);
    }

    /// A calendar switched off draws nothing and must announce nothing
    /// either, however it was set.
    #[test]
    fn a_calendar_switched_off_stays_quiet_even_if_it_was_asked_to_remind() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();
        db.replace_subscription_events("s1", &[event("a", "2026-03-10")]).unwrap();
        db.set_subscription_remind("s1", true).unwrap();
        db.set_subscription_enabled("s1", false).unwrap();

        assert!(db.subscribed_events_to_remind().unwrap().is_empty());
    }

    /// Removing a calendar must take its events with it, or they are events
    /// nothing can explain, colour, or ever clean up.
    #[test]
    fn removing_a_calendar_takes_its_events_with_it() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();
        db.add_subscription("s2", "https://e.com/b.ics", "Other", 100).unwrap();
        db.replace_subscription_events("s1", &[event("a", "2026-03-10")]).unwrap();
        db.replace_subscription_events("s2", &[event("b", "2026-03-11")]).unwrap();

        db.remove_subscription("s1").unwrap();
        let left = db.subscribed_event_summaries().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].uid, "b");
    }

    /// The strongest form of read-only available: the id is not a path, so
    /// nothing that writes nodes could write to it even by mistake.
    #[test]
    fn a_subscribed_event_carries_an_id_no_file_could_have() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();
        db.replace_subscription_events("s1", &[event("a", "2026-03-10")]).unwrap();

        let summary = &db.subscribed_event_summaries().unwrap()[0];
        assert_eq!(summary.id, "subscription:s1/a");
        assert_eq!(summary.subscription_id, "s1");
        assert!(!summary.id.ends_with(".md"), "it is not a file");
    }

    #[test]
    fn a_feed_whose_events_have_no_uid_still_keeps_all_of_them() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();
        let events = vec![event("", "2026-03-10"), event("", "2026-03-11"), event("", "2026-03-12")];
        assert_eq!(db.replace_subscription_events("s1", &events).unwrap(), 3);
        assert_eq!(db.subscribed_event_summaries().unwrap().len(), 3);
    }

    #[test]
    fn an_event_with_no_date_is_not_stored_at_all() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();
        let events = vec![event("a", "2026-03-10"), event("b", "")];
        assert_eq!(db.replace_subscription_events("s1", &events).unwrap(), 1);
    }

    #[test]
    fn a_failed_refresh_is_recorded_where_the_url_is_shown() {
        let db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "Team", 100).unwrap();
        db.note_subscription_fetch("s1", "", "", "404 Not Found", 5_000).unwrap();

        let sub = db.get_subscription("s1").unwrap().unwrap();
        assert_eq!(sub.last_error, "404 Not Found");
        assert_eq!(sub.last_fetched_at, 5_000);
    }

    #[test]
    fn a_calendar_can_be_renamed_without_being_re_read() {
        let mut db = db();
        db.add_subscription("s1", "https://e.com/a.ics", "example.com", 100).unwrap();
        db.replace_subscription_events("s1", &[event("a", "2026-03-10")]).unwrap();
        db.rename_subscription("s1", "Vietnam holidays").unwrap();

        let sub = db.get_subscription("s1").unwrap().unwrap();
        assert_eq!(sub.name, "Vietnam holidays");
        assert_eq!(sub.event_count, 1, "renaming does not disturb the events");
    }
}
