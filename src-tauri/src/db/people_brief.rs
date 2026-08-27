//! Assembling one person's brief out of everything the vault holds.
//!
//! The reading half of [`crate::people::brief`], which holds the rules. Split
//! so the rules can be tested without a database and the queries can be read
//! without the arithmetic in the way.

use super::DbBridge;
use crate::error::AppResult;
use crate::people::brief::{
    contact_status, reciprocity, relationships_of, LastInteraction, OpenTask, PersonBrief,
    UpcomingMeeting,
};
use chrono::NaiveDate;
use rusqlite::params;
use serde_json::Value;

/// How far ahead "coming up" reaches. A meeting in three months is not
/// something to prepare for today.
const HORIZON_DAYS: i64 = 30;

impl DbBridge {
    /// Everything worth knowing about one person, right now.
    ///
    /// One query set rather than six screens each working it out: the card
    /// shown before a meeting, the assistant, and the balance of what has
    /// passed between you all read this.
    pub fn person_brief(&self, person_id: &str, today: NaiveDate) -> AppResult<Option<PersonBrief>> {
        let Some(person) = self.get_node(person_id)? else {
            return Ok(None);
        };
        if person.node_type != "person" {
            return Ok(None);
        }
        let props = &person.properties;

        // The later of what was logged and what the vault noticed — the same
        // answer the reminder engine works from.
        let derived = self.last_contact_by_person().unwrap_or_default();
        let stored = props
            .get("last_contacted")
            .and_then(Value::as_str)
            .unwrap_or("");
        let last_contact = derived
            .get(&person.id)
            .map(String::as_str)
            .filter(|seen| *seen > stored)
            .unwrap_or(stored);
        let last_contact = (!last_contact.is_empty()).then(|| last_contact.to_string());

        let days_since_contact = last_contact.as_deref().and_then(|date| {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .ok()
                .map(|then| (today - then).num_days())
        });

        let cadence = props
            .get("contact_frequency")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_string);
        let cadence_len = cadence
            .as_deref()
            .and_then(crate::people::brief::cadence_days);

        let birthday = props
            .get("birthday")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_string);
        let days_until_birthday = birthday
            .as_deref()
            .and_then(|raw| days_until_anniversary(raw, today));

        let interactions = self.nodes_about_person(&person.id, "interaction")?;
        let last_interaction = interactions.first().map(|node| LastInteraction {
            id: node.id.clone(),
            date: node
                .properties
                .get("date")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kind: node
                .properties
                .get("interaction_type")
                .and_then(Value::as_str)
                .unwrap_or("other")
                .to_string(),
            note: node.content.trim().to_string(),
        });

        Ok(Some(PersonBrief {
            person_id: person.id.clone(),
            title: person.title.clone(),
            relationships: relationships_of(props),
            cadence,
            last_contact,
            days_since_contact,
            status: contact_status(days_since_contact, cadence_len).to_string(),
            birthday,
            days_until_birthday,
            next_meeting: self.next_meeting_with(&person.id, today)?,
            open_tasks: self.open_tasks_about(&person.id, today)?,
            last_interaction,
            interaction_count: interactions.len(),
            reciprocity: reciprocity(
                props,
                &self.transactions_for_person(&person.id)?,
                &self.debts_for_person(&person.id, &person.title)?,
            ),
        }))
    }

    /// The next time you are due to see them, if the calendar says.
    ///
    /// The thing an ordinary contact app cannot answer, because it does not
    /// have your calendar. Recurring series are expanded rather than read off
    /// their anchor: a weekly one-to-one started in January is not a meeting
    /// in January, it is a meeting on Thursday.
    fn next_meeting_with(
        &self,
        person_id: &str,
        today: NaiveDate,
    ) -> AppResult<Option<UpcomingMeeting>> {
        // A birthday entry mirrors a date already in the brief. Showing it as
        // an appointment would say you were meeting them for it.
        let events: Vec<_> = self
            .events_linked_to(person_id)?
            .into_iter()
            .filter(|e| !e.tags.iter().any(|t| t == "birthday"))
            .collect();
        if events.is_empty() {
            return Ok(None);
        }

        let horizon = today + chrono::Duration::days(HORIZON_DAYS);
        let expanded = crate::calendar::recurrence::expand_range(
            events,
            &today.format("%Y-%m-%d").to_string(),
            &horizon.format("%Y-%m-%d").to_string(),
            "",
        );

        let mut soonest: Option<UpcomingMeeting> = None;
        for occurrence in &expanded.occurrences {
            let Ok(day) = NaiveDate::parse_from_str(&occurrence.date, "%Y-%m-%d") else {
                continue;
            };
            let days_away = (day - today).num_days();
            if days_away < 0 {
                continue;
            }
            if soonest.as_ref().is_some_and(|s| s.days_away <= days_away) {
                continue;
            }
            let Some(event) = expanded.events.get(occurrence.event) else {
                continue;
            };
            soonest = Some(UpcomingMeeting {
                id: event.id.clone(),
                title: event.title.clone(),
                start_at: occurrence.start_at.clone(),
                days_away,
            });
        }
        Ok(soonest)
    }

    /// Tasks about this person that are not finished, soonest due first.
    fn open_tasks_about(&self, person_id: &str, today: NaiveDate) -> AppResult<Vec<OpenTask>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT n.id, n.title, json_extract(n.properties, '$.due_date')
                 FROM node_edges e
                 JOIN nodes n ON n.stable_id = e.source_id
                 WHERE n.node_type = 'task'
                   AND COALESCE(json_extract(n.properties, '$.status'), '') NOT IN ('done', 'canceled')
                   AND e.target_id = COALESCE(
                       (SELECT stable_id FROM nodes WHERE id = ?1),
                       ?1
                   )
                 ORDER BY json_extract(n.properties, '$.due_date') IS NULL,
                          json_extract(n.properties, '$.due_date')",
            )
            .map_err(|e| {
                crate::error::AppError::General(format!("DB Query Error (open tasks): {}", e))
            })?;

        let rows = stmt
            .query_map(params![person_id], |row| {
                let due: Option<String> = row.get(2)?;
                Ok(OpenTask {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    overdue: due
                        .as_deref()
                        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                        .is_some_and(|d| d < today),
                    due_date: due,
                })
            })
            .map_err(|e| {
                crate::error::AppError::General(format!("DB Map Error (open tasks): {}", e))
            })?;

        Ok(rows.flatten().collect())
    }

    /// Transactions tagged to this person, out of every month on record.
    ///
    /// Narrowed in SQL. The People screen used to read every month the vault
    /// had, with all their transactions, to find the handful belonging to one
    /// contact.
    fn transactions_for_person(&self, person_id: &str) -> AppResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT value FROM nodes, json_each(nodes.properties, '$.transactions')
                 WHERE nodes.node_type = 'finance_month'
                   AND json_extract(value, '$.personId') = ?1",
            )
            .map_err(|e| {
                crate::error::AppError::General(format!("DB Query Error (transactions): {}", e))
            })?;

        let rows = stmt
            .query_map(params![person_id], |row| row.get::<_, String>(0))
            .map_err(|e| {
                crate::error::AppError::General(format!("DB Map Error (transactions): {}", e))
            })?;

        Ok(rows
            .flatten()
            .filter_map(|raw| serde_json::from_str(&raw).ok())
            .collect())
    }

    /// Debts recorded against this person.
    ///
    /// Matched on the identity where there is one, and on the name only where
    /// the debt predates people being linked at all — two people with the same
    /// name would otherwise share each other's money.
    fn debts_for_person(&self, person_id: &str, title: &str) -> AppResult<Vec<Value>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT value FROM nodes, json_each(nodes.properties, '$.debts')
                 WHERE nodes.node_type = 'finance_debts'
                   AND (
                       json_extract(value, '$.personId') = ?1
                       OR (
                           COALESCE(json_extract(value, '$.personId'), '') = ''
                           AND lower(COALESCE(json_extract(value, '$.person'), '')) = lower(?2)
                       )
                   )",
            )
            .map_err(|e| {
                crate::error::AppError::General(format!("DB Query Error (debts): {}", e))
            })?;

        let rows = stmt
            .query_map(params![person_id, title], |row| row.get::<_, String>(0))
            .map_err(|e| crate::error::AppError::General(format!("DB Map Error (debts): {}", e)))?;

        Ok(rows
            .flatten()
            .filter_map(|raw| serde_json::from_str(&raw).ok())
            .collect())
    }
}

/// Days until a `YYYY-MM-DD` or `MM-DD` comes round again; 0 means today.
fn days_until_anniversary(raw: &str, today: NaiveDate) -> Option<i64> {
    let (month, dom) = crate::calendar::reminders::parse_birthday(raw)?;
    for year in [today.year_of(), today.year_of() + 1] {
        let day = NaiveDate::from_ymd_opt(year, month, dom)
            // 29 February in a common year is kept on the 28th, which is where
            // the reminder engine and the calendar both put it.
            .or_else(|| NaiveDate::from_ymd_opt(year, month, dom - 1))?;
        if day >= today {
            return Some((day - today).num_days());
        }
    }
    None
}

/// `chrono::Datelike` under another name, so the import does not leak into
/// every caller of this module.
trait YearOf {
    fn year_of(&self) -> i32;
}
impl YearOf for NaiveDate {
    fn year_of(&self) -> i32 {
        use chrono::Datelike;
        self.year()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::edges::NodeEdge;
    use crate::models::node::NodeMetadata;
    use serde_json::json;

    const TODAY: &str = "2026-08-25";

    fn today() -> NaiveDate {
        NaiveDate::parse_from_str(TODAY, "%Y-%m-%d").unwrap()
    }

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("schema")
    }

    fn node(id: &str, node_type: &str, stable: &str, properties: Value) -> NodeMetadata {
        let mut props = properties;
        if let Some(map) = props.as_object_mut() {
            map.insert("node_id".into(), json!(stable));
        }
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: id.to_string(),
            content: String::new(),
            properties: props,
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn about(source: &str, person: &str) -> NodeEdge {
        NodeEdge {
            id: format!("{source}->{person}"),
            source_id: source.to_string(),
            target_id: person.to_string(),
            edge_type: "about".to_string(),
            relation: None,
            created_at: "2026-01-01 00:00:00".into(),
        }
    }

    fn link(source: &str, person: &str) -> NodeEdge {
        NodeEdge {
            edge_type: "internal_link".to_string(),
            ..about(source, person)
        }
    }

    /// A vault with one person in it, ready to have things added around them.
    fn with_person(properties: Value) -> DbBridge {
        let db = db();
        let mut an = node("People/an.md", "person", "uuid-an", properties);
        an.title = "An Nguyễn".into();
        db.upsert_node(&an).unwrap();
        db
    }

    fn brief(db: &DbBridge) -> PersonBrief {
        db.person_brief("People/an.md", today())
            .unwrap()
            .expect("a brief")
    }

    #[test]
    fn a_person_who_is_not_there_has_no_brief() {
        assert!(db().person_brief("People/nobody.md", today()).unwrap().is_none());
    }

    #[test]
    fn something_that_is_not_a_person_has_no_brief() {
        let db = db();
        db.upsert_node(&node("Notes/a.md", "note", "uuid-a", json!({}))).unwrap();
        assert!(db.person_brief("Notes/a.md", today()).unwrap().is_none());
    }

    #[test]
    fn the_brief_says_where_the_relationship_stands() {
        let db = with_person(json!({
            "contact_frequency": "monthly",
            "last_contacted": "2026-07-01",
            "relationship_type": ["Friend", "Colleague"],
        }));
        let got = brief(&db);

        assert_eq!(got.title, "An Nguyễn");
        assert_eq!(got.relationships, ["Friend", "Colleague"]);
        assert_eq!(got.days_since_contact, Some(55));
        assert_eq!(got.status, "overdue");
    }

    #[test]
    fn a_note_about_somebody_counts_as_contact_in_the_brief_too() {
        // The brief has to agree with the reminder engine about what "last
        // contact" means, or the card and the notification tell two stories.
        let db = with_person(json!({ "contact_frequency": "monthly", "last_contacted": "2026-02-01" }));
        let mut note = node("Notes/coffee.md", "note", "uuid-coffee", json!({}));
        note.updated_at = "2026-08-20 09:00:00".into();
        db.upsert_node(&note).unwrap();
        db.upsert_node_edge(&link("uuid-coffee", "uuid-an")).unwrap();

        let got = brief(&db);
        assert_eq!(got.last_contact.as_deref(), Some("2026-08-20"));
        assert_eq!(got.status, "thriving");
    }

    #[test]
    fn the_next_meeting_is_the_next_one_not_the_first_one() {
        let db = with_person(json!({}));
        for (id, stable, start) in [
            ("Events/past.md", "uuid-past", "2026-01-05T09:00"),
            ("Events/soon.md", "uuid-soon", "2026-08-27T09:00"),
            ("Events/later.md", "uuid-later", "2026-09-30T09:00"),
        ] {
            db.upsert_node(&node(id, "event", stable, json!({ "start_at": start })))
                .unwrap();
            db.upsert_node_edge(&link(stable, "uuid-an")).unwrap();
        }

        let meeting = brief(&db).next_meeting.expect("a meeting");
        assert_eq!(meeting.id, "Events/soon.md");
        assert_eq!(meeting.days_away, 2);
    }

    #[test]
    fn a_weekly_one_to_one_is_a_meeting_this_week_not_in_january() {
        // Read off its anchor, a series started in January is a meeting in
        // January — which is never, because January has gone.
        let db = with_person(json!({}));
        db.upsert_node(&node(
            "Events/one-to-one.md",
            "event",
            "uuid-1to1",
            json!({ "start_at": "2026-01-05T09:00", "recurrence": "weekly" }),
        ))
        .unwrap();
        db.upsert_node_edge(&link("uuid-1to1", "uuid-an")).unwrap();

        let meeting = brief(&db).next_meeting.expect("a meeting");
        assert!(meeting.days_away <= 7, "{:?}", meeting);
    }

    #[test]
    fn a_birthday_on_the_calendar_is_not_an_appointment() {
        // It mirrors a date already in the brief. Showing it as a meeting
        // would say you were seeing them for it.
        let db = with_person(json!({ "birthday": "1994-08-27" }));
        db.upsert_node(&node(
            "Events/birthday-an.md",
            "event",
            "uuid-bday",
            json!({
                "start_at": "2024-08-27",
                "is_all_day": true,
                "recurrence": "yearly",
                "tags": ["birthday", "people"],
                "source_person_id": "People/an.md",
            }),
        ))
        .unwrap();
        db.upsert_node_edge(&link("uuid-bday", "uuid-an")).unwrap();

        let got = brief(&db);
        assert!(got.next_meeting.is_none(), "{:?}", got.next_meeting);
        assert_eq!(got.days_until_birthday, Some(2));
    }

    #[test]
    fn open_tasks_about_them_are_listed_and_finished_ones_are_not() {
        let db = with_person(json!({}));
        for (id, stable, status, due) in [
            ("Tasks/send-cv.md", "uuid-cv", "todo", "2026-08-20"),
            ("Tasks/book.md", "uuid-book", "doing", "2026-09-01"),
            ("Tasks/done.md", "uuid-done", "done", "2026-08-01"),
        ] {
            db.upsert_node(&node(id, "task", stable, json!({ "status": status, "due_date": due })))
                .unwrap();
            db.upsert_node_edge(&link(stable, "uuid-an")).unwrap();
        }

        let tasks = brief(&db).open_tasks;
        assert_eq!(
            tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["Tasks/send-cv.md", "Tasks/book.md"]
        );
        assert!(tasks[0].overdue, "its due date has gone by");
        assert!(!tasks[1].overdue);
    }

    #[test]
    fn the_last_thing_you_did_together_is_the_most_recent_one() {
        let db = with_person(json!({}));
        for (id, stable, date, note) in [
            ("People/Interactions/old.md", "uuid-old", "2026-02-01", "the old one"),
            ("People/Interactions/new.md", "uuid-new", "2026-08-20", "the recent one"),
        ] {
            let mut interaction = node(
                id,
                "interaction",
                stable,
                json!({ "date": date, "interaction_type": "coffee", "person_id": "uuid-an" }),
            );
            interaction.content = note.to_string();
            db.upsert_node(&interaction).unwrap();
            db.upsert_node_edge(&about(stable, "uuid-an")).unwrap();
        }

        let got = brief(&db);
        assert_eq!(got.interaction_count, 2);
        let last = got.last_interaction.expect("an interaction");
        assert_eq!(last.date, "2026-08-20");
        assert_eq!(last.note, "the recent one");
        assert_eq!(last.kind, "coffee");
    }

    // ── Money and gifts ─────────────────────────────────────

    #[test]
    fn money_tagged_to_them_is_found_without_reading_every_month() {
        let db = with_person(json!({}));
        db.upsert_node(&node(
            "Finance/2026-08.md",
            "finance_month",
            "uuid-aug",
            json!({ "transactions": [
                { "id": "t1", "type": "expense", "amount": 250000.0, "personId": "People/an.md" },
                { "id": "t2", "type": "income", "amount": 80000.0, "personId": "People/an.md" },
                { "id": "t3", "type": "expense", "amount": 999000.0, "personId": "People/binh.md" },
                { "id": "t4", "type": "expense", "amount": 5000.0 },
            ]}),
        ))
        .unwrap();

        let got = brief(&db).reciprocity;
        assert_eq!(got.money_out, 250_000.0, "somebody else's spending got in");
        assert_eq!(got.money_in, 80_000.0);
    }

    #[test]
    fn a_debt_is_matched_by_identity_first_and_by_name_only_as_a_fallback() {
        // Two people with the same name must not share each other's money.
        let db = with_person(json!({}));
        let mut other = node("People/other.md", "person", "uuid-other", json!({}));
        other.title = "An Nguyễn".into();
        db.upsert_node(&other).unwrap();

        db.upsert_node(&node(
            "Finance/debts.md",
            "finance_debts",
            "uuid-debts",
            json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "An Nguyễn", "personId": "People/an.md",
                  "totalAmount": 5000000.0, "paidAmount": 2000000.0 },
                { "id": "d2", "type": "borrow", "person": "An Nguyễn", "personId": "People/other.md",
                  "totalAmount": 900000.0, "paidAmount": 0.0 },
            ]}),
        ))
        .unwrap();

        assert_eq!(brief(&db).reciprocity.outstanding, 3_000_000.0);
    }

    #[test]
    fn a_debt_from_before_people_were_linked_is_still_found_by_name() {
        let db = with_person(json!({}));
        db.upsert_node(&node(
            "Finance/debts.md",
            "finance_debts",
            "uuid-debts",
            json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "an nguyễn",
                  "totalAmount": 1000000.0, "paidAmount": 0.0 },
            ]}),
        ))
        .unwrap();

        assert_eq!(brief(&db).reciprocity.outstanding, 1_000_000.0);
    }

    #[test]
    fn somebody_with_nothing_recorded_has_no_balance_rather_than_an_even_one() {
        let got = brief(&with_person(json!({}))).reciprocity;
        assert!(!got.has_history);
    }
}
