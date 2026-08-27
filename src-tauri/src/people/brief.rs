//! Everything the vault knows about one person, in one answer.
//!
//! Three things wanted the same facts and would otherwise have worked them out
//! three times, in three places, and disagreed: the card shown before you meet
//! somebody, the assistant when asked about them, and the balance of what has
//! passed between you.
//!
//! # Why this can exist here and not in a contact app
//!
//! Because the notes, the tasks, the calendar and the household accounts are
//! in the same vault. A personal CRM that only holds contacts can tell you
//! when you last emailed. This can tell you that you lent them money in
//! February, that a task about them is still open, and that you are seeing
//! them on Thursday — because it is not a contact app, it is the rest of your
//! life with a contact list in it.

use serde::Serialize;
use serde_json::Value;

/// Days each cadence allows between one contact and the next.
///
/// The same table the People screen and the reminder engine use. Kept in
/// step by [`crate::people::brief::tests::the_cadence_table_matches_the_one_that_reminds`].
pub fn cadence_days(frequency: &str) -> Option<i64> {
    crate::calendar::reminders::cadence_days_public(frequency)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OpenTask {
    pub id: String,
    pub title: String,
    pub due_date: Option<String>,
    /// Its due date has gone by.
    pub overdue: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UpcomingMeeting {
    pub id: String,
    pub title: String,
    pub start_at: String,
    pub days_away: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LastInteraction {
    pub id: String,
    pub date: String,
    pub kind: String,
    pub note: String,
}

/// What has passed between the two of you, in both directions.
///
/// Not a score and not a judgement — a count and a total, with enough context
/// to read them. Somebody who always pays for coffee is not in your debt; the
/// number is only worth showing because nobody remembers it accurately.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Reciprocity {
    pub gifts_given: usize,
    pub gifts_received: usize,
    /// Money that went out to them, by whatever route.
    pub money_out: f64,
    pub money_in: f64,
    /// Still owed, positive when they owe you.
    pub outstanding: f64,
    /// Whether anything at all was recorded. Zeroes mean two different things.
    pub has_history: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PersonBrief {
    pub person_id: String,
    pub title: String,
    pub relationships: Vec<String>,
    pub cadence: Option<String>,
    pub last_contact: Option<String>,
    pub days_since_contact: Option<i64>,
    /// `thriving`, `on_track`, `due_soon`, `overdue` or `unknown`.
    pub status: String,
    pub birthday: Option<String>,
    pub days_until_birthday: Option<i64>,
    pub next_meeting: Option<UpcomingMeeting>,
    pub open_tasks: Vec<OpenTask>,
    pub last_interaction: Option<LastInteraction>,
    pub interaction_count: usize,
    pub reciprocity: Reciprocity,
}

/// The relationships on a person, whichever shape they are stored in.
///
/// A list since relationships stopped being one comma-separated string; a
/// vault written before that still holds the string, and a brief that refused
/// to read it would show nothing for everybody in it.
pub fn relationships_of(properties: &Value) -> Vec<String> {
    match properties.get("relationship_type") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::String(raw)) => raw
            .split(',')
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Where a relationship stands, from the cadence and the last contact.
///
/// The same boundaries the coloured dot uses. Two answers to one question is
/// how a contact comes to read "Overdue" in one place and "Due Soon" in
/// another.
pub fn contact_status(days_since: Option<i64>, cadence: Option<i64>) -> &'static str {
    let (Some(days), Some(cadence)) = (days_since, cadence) else {
        return "unknown";
    };
    if cadence <= 0 {
        return "unknown";
    }
    let ratio = days as f64 / cadence as f64;
    if ratio <= 0.5 {
        "thriving"
    } else if ratio <= 0.85 {
        "on_track"
    } else if ratio <= 1.2 {
        "due_soon"
    } else {
        "overdue"
    }
}

/// Gifts and money, from the person's own record and the household accounts.
///
/// `transactions` and `debts` arrive already narrowed to this person: reading
/// every month the vault has ever recorded, for one contact, is what the
/// People screen used to do on the way in.
pub fn reciprocity(properties: &Value, transactions: &[Value], debts: &[Value]) -> Reciprocity {
    let mut out = Reciprocity::default();

    for gift in properties
        .get("gifts")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        out.has_history = true;
        // `direction` is what the gift form writes: given, or received.
        match gift.get("direction").and_then(Value::as_str) {
            Some("received") => out.gifts_received += 1,
            _ => out.gifts_given += 1,
        }
    }

    for transaction in transactions {
        let Some(amount) = transaction.get("amount").and_then(Value::as_f64) else {
            continue;
        };
        if amount == 0.0 {
            continue;
        }
        out.has_history = true;
        match transaction.get("type").and_then(Value::as_str) {
            Some("income") => out.money_in += amount,
            // A transfer tagged to somebody is money that went their way; an
            // expense tagged to them is money spent on them. Both are out.
            _ => out.money_out += amount,
        }
    }

    for debt in debts {
        let total = debt.get("totalAmount").and_then(Value::as_f64).unwrap_or(0.0);
        let paid = debt.get("paidAmount").and_then(Value::as_f64).unwrap_or(0.0);
        let remaining = (total - paid).max(0.0);
        if total == 0.0 {
            continue;
        }
        out.has_history = true;
        match debt.get("type").and_then(Value::as_str) {
            // They borrowed from you: they still owe it.
            Some("lend") => out.outstanding += remaining,
            Some("borrow") => out.outstanding -= remaining,
            _ => {}
        }
    }

    out
}

#[cfg(test)]
mod tests;
