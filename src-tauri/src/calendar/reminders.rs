//! What is due to be announced, and exactly when.
//!
//! Split out of the loop that used to do this inline because two very
//! different things need the same answer:
//!
//! * on a desktop, a task waking every minute asks "what has come due since
//!   I last looked?";
//! * on a phone, where the app is usually not running at all, something has
//!   to hand the operating system a list of times *in advance* — and getting
//!   that list means answering the same question about the future.
//!
//! Working both out from one place is what stops the phone and the desktop
//! from disagreeing about when a reminder is.

use crate::calendar::recurrence::EventSummary;
use crate::calendar::tz::convert_wall_clock;
use crate::models::node::NodeMetadata;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde_json::Value;

/// The time of day a task's reminders count back from, when it has none.
const DEFAULT_TASK_HOUR: &str = "09:00:00";

/// The time of day a birthday is announced.
///
/// A birthday has no clock of its own, and midnight is the wrong answer twice
/// over: the phone rings while its owner is asleep, and by the time they look
/// the notification is a day old.
const BIRTHDAY_HOUR: &str = "09:00:00";

/// When a birthday is announced, when the person's file says nothing.
///
/// The day before and the day itself. The day before is the one that lets
/// somebody act on it — buy something, write something — and the day itself
/// is the one they would be upset to have missed.
const BIRTHDAY_OFFSETS: [&str; 2] = ["1d", "0m"];

/// The time of day a "you have not spoken in a while" nudge arrives.
const KEEP_IN_TOUCH_HOUR: &str = "10:00:00";

/// How many days each cadence allows between one contact and the next.
///
/// The same table the People screen uses. Both have to agree, or the dot
/// beside somebody's name turns red on a different day from the one the
/// notification arrives.
pub fn cadence_days_public(frequency: &str) -> Option<i64> {
    cadence_days(frequency)
}

fn cadence_days(frequency: &str) -> Option<i64> {
    match frequency {
        "weekly" => Some(7),
        "biweekly" => Some(14),
        "monthly" => Some(30),
        "quarterly" => Some(90),
        "yearly" => Some(365),
        _ => None,
    }
}

/// How far ahead the phone is asked to hold reminders.
///
/// Long enough that a phone left alone for a week still rings, short enough
/// that the queue stays small and a change to the vault does not mean
/// rewriting hundreds of scheduled items.
pub const SCHEDULE_HORIZON_DAYS: i64 = 7;

/// How far back the desktop loop looks when it wakes.
///
/// It only exists to catch up on what was missed while the machine was
/// asleep. Repeats are prevented by the delivery record, not by this.
pub const CATCH_UP_DAYS: i64 = 1;

/// One reminder, at one moment, on this machine's clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedReminder {
    pub target_id: String,
    /// `"event"`, `"task"`, `"person"` (a birthday) or `"finance_debt"`.
    pub target_type: &'static str,
    pub title: String,
    /// The offset as the user wrote it: `15m`, `2h`, `1d`. `0m` means "now".
    pub offset: String,
    /// The day of the occurrence this belongs to.
    pub occurrence_date: String,
    /// When to announce it, as a local wall clock on this machine.
    pub trigger_at: NaiveDateTime,
    /// When the thing itself happens, same clock.
    pub subject_at: NaiveDateTime,
    /// A task whose due date has already gone by.
    pub overdue: bool,
}

impl PlannedReminder {
    /// What makes this reminder the same reminder tomorrow.
    ///
    /// The occurrence date is in it because a weekly stand-up is a different
    /// reminder every week; the offset is in it because "a day before" and
    /// "fifteen minutes before" are two announcements, not one.
    pub fn delivery_key(&self) -> String {
        format!("{}_{}_{}", self.target_id, self.occurrence_date, self.offset)
    }

    /// A stable handle for the operating system's own scheduler, which counts
    /// in `i32` and needs the same reminder to keep the same number so it can
    /// be replaced instead of duplicated.
    pub fn os_id(&self) -> i32 {
        // FNV-1a, folded into a positive i32. Any stable hash would do; what
        // matters is that it does not move between runs the way `DefaultHasher`
        // is explicitly allowed to.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in self.delivery_key().as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        ((hash % (i32::MAX as u64 - 1)) + 1) as i32
    }
}

/// `15m`, `2h`, `1d` — anything else is no offset at all.
pub fn parse_offset(s: &str) -> Option<Duration> {
    let s = s.trim();
    let (digits, unit) = s.split_at(s.len().checked_sub(1)?);
    let value: i64 = digits.parse().ok()?;
    if value < 0 {
        return None;
    }
    match unit {
        "m" => Duration::try_minutes(value),
        "h" => Duration::try_hours(value),
        "d" => Duration::try_days(value),
        _ => None,
    }
}

fn offsets_of(props: &Value) -> Vec<String> {
    let mut out: Vec<String> = props
        .get("reminders")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .filter(|s| parse_offset(s).is_some())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    if out.is_empty() {
        // No reminder set still means "tell me when it starts".
        out.push("0m".to_string());
    }
    out
}

/// The offsets a node is announced at, defaults included.
///
/// Only the default differs by type — a birthday gets a day's warning where
/// an event gets none — and anything written in the file wins either way.
/// [`furthest_reach`] has to ask the same question as the planners do, or it
/// stops scanning a day before the warning it was supposed to find.
fn offsets_for(node: &NodeMetadata) -> Vec<String> {
    let written = node
        .properties
        .get("reminders")
        .and_then(|v| v.as_array())
        .is_some_and(|arr| !arr.is_empty());

    if node.node_type == "person" && !written {
        return BIRTHDAY_OFFSETS.iter().map(|s| s.to_string()).collect();
    }
    offsets_of(&node.properties)
}

/// The time of day a task's reminders count back from.
///
/// `due_time` is what the Tasks screen writes. `start_time` is what vaults
/// written before it have, and dropping that fallback would move every
/// reminder in them to the default hour without a word.
pub fn task_time_of_day(props: &Value) -> String {
    for key in ["due_time", "start_time"] {
        if let Some(value) = props.get(key).and_then(|v| v.as_str()) {
            if !value.trim().is_empty() {
                return value.to_string();
            }
        }
    }
    DEFAULT_TASK_HOUR.to_string()
}

/// The month and day of a birthday, from either shape a person's file uses.
///
/// `1994-03-02` is what the contact form writes. `03-02` is what somebody
/// types when they know the day but not the year, and three screens in the
/// People app display it — so refusing it here is how a birthday could sit in
/// the vault, visible, and never once be announced.
///
/// The year is deliberately dropped. What is wanted is the anniversary, and
/// the year on file is the year of birth.
pub fn parse_birthday(raw: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = raw.trim().split('-').collect();
    let (month, day) = match parts.as_slice() {
        [_year, month, day] => (month, day),
        [month, day] => (month, day),
        _ => return None,
    };
    let month: u32 = month.trim().parse().ok()?;
    let day: u32 = day.trim().parse().ok()?;

    // A real date in a leap year, so that 29 February is accepted rather than
    // rejected as the impossible day it is in most years.
    NaiveDate::from_ymd_opt(2024, month, day)?;
    Some((month, day))
}

/// Whether `day` is the anniversary of a birthday on `month`/`dom`.
///
/// Somebody born on 29 February has a birthday in one year out of four. The
/// rest of the time it is kept on the 28th, which is where
/// [`crate::calendar::recurrence`] puts a yearly series that overshoots its
/// month — the two have to agree, or the same birthday lands on two different
/// days depending on which part of the app is asked.
fn is_anniversary(day: NaiveDate, month: u32, dom: u32) -> bool {
    if day.month() == month && day.day() == dom {
        return true;
    }
    month == 2
        && dom == 29
        && day.month() == 2
        && day.day() == 28
        && NaiveDate::from_ymd_opt(day.year(), 2, 29).is_none()
}

fn at(date: &str, time: &str) -> Option<NaiveDateTime> {
    let time = time.trim();
    for shape in [
        format!("{}T{}", date, time),
        format!("{}T{}:00", date, time),
        format!("{}T00:00:00", date),
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&shape, "%Y-%m-%dT%H:%M:%S") {
            return Some(dt);
        }
    }
    None
}

/// The furthest any reminder in `nodes` reaches back from its subject.
///
/// The reason this has to be known before anything else: a reminder set for a
/// week before an event is decided by an occurrence a week away, and a plan
/// that only looked at today and tomorrow would not find it. That is exactly
/// what used to happen — a "1 week before" arrived on the day.
fn furthest_reach(nodes: &[NodeMetadata]) -> Duration {
    let mut furthest = Duration::zero();
    for node in nodes {
        for offset in offsets_for(node) {
            if let Some(d) = parse_offset(&offset) {
                if d > furthest {
                    furthest = d;
                }
            }
        }
    }
    furthest
}

/// Every reminder whose moment falls in `[from, to]`.
///
/// Times are this machine's wall clock, converted from whatever zone the
/// event was written in — so a Tokyo meeting's "fifteen minutes before" is
/// fifteen minutes before it happens, not fifteen minutes before a reading of
/// the same numbers here.
pub fn plan(
    nodes: &[NodeMetadata],
    from: NaiveDateTime,
    to: NaiveDateTime,
    here_tz: &str,
) -> Vec<PlannedReminder> {
    plan_with(nodes, &[], from, to, here_tz)
}

/// The same, plus events that are not nodes.
///
/// A subscribed calendar is a cache, not a file, so it cannot arrive as a
/// node — and being invisible to the reminder loop meant subscribing to a
/// team's calendar and never being told about a single meeting in it.
pub fn plan_with(
    nodes: &[NodeMetadata],
    extra_events: &[EventSummary],
    from: NaiveDateTime,
    to: NaiveDateTime,
    here_tz: &str,
) -> Vec<PlannedReminder> {
    let mut out = Vec::new();
    if to < from {
        return out;
    }

    let scan_until = (to + furthest_reach(nodes)).date();

    for node in nodes {
        match node.node_type.as_str() {
            // An event mirroring somebody's birthday is there to be seen in
            // the calendar, not to speak. The person announces it — see
            // `plan_birthday` — and letting the event announce it as well is
            // how one birthday becomes two notifications.
            "event" if node.properties.get("source_person_id").is_some() => {}
            "event" => plan_event(
                &EventSummary::from_properties(&node.id, &node.title, "", &node.properties),
                offsets_of(&node.properties),
                from, to, scan_until, here_tz, &mut out,
            ),
            "task" => plan_task(node, from, to, &mut out),
            "finance_debts" => plan_debts(node, from, to, &mut out),
            "person" => plan_person(node, from, to, scan_until, &mut out),
            _ => {}
        }
    }

    for event in extra_events {
        // A subscribed event carries no reminder of its own, so it is
        // announced when it starts — which is what asking to be reminded
        // about somebody else's calendar can reasonably mean.
        plan_event(event, vec!["0m".to_string()], from, to, scan_until, here_tz, &mut out);
    }

    out.sort_by(|a, b| a.trigger_at.cmp(&b.trigger_at).then(a.target_id.cmp(&b.target_id)));
    out
}

#[allow(clippy::too_many_arguments)]
fn plan_event(
    summary: &EventSummary,
    offsets: Vec<String>,
    from: NaiveDateTime,
    to: NaiveDateTime,
    scan_until: NaiveDate,
    here_tz: &str,
    out: &mut Vec<PlannedReminder>,
) {
    if summary.start_at.is_empty() {
        return;
    }
    let start_time = summary
        .start_at
        .split_once('T')
        .map(|(_, t)| t.to_string())
        .unwrap_or_else(|| "00:00:00".to_string());
    let mut day = from.date();
    while day <= scan_until {
        let date_str = day.format("%Y-%m-%d").to_string();
        if summary.occurs_on(&date_str) {
            if let Some(naive) = at(&date_str, &start_time) {
                // The stored clock belongs to the event's zone, not to this
                // machine's. Reading it as local would move a Tokyo meeting's
                // reminder by the whole offset between them.
                let subject_at = if summary.tzid.trim().is_empty() {
                    naive
                } else {
                    convert_wall_clock(naive, &summary.tzid, here_tz).unwrap_or(naive)
                };

                for offset in &offsets {
                    let Some(reach) = parse_offset(offset) else { continue };
                    let trigger_at = subject_at - reach;
                    if trigger_at >= from && trigger_at <= to {
                        out.push(PlannedReminder {
                            target_id: summary.id.clone(),
                            target_type: "event",
                            title: summary.title.clone(),
                            offset: offset.clone(),
                            occurrence_date: date_str.clone(),
                            trigger_at,
                            subject_at,
                            overdue: false,
                        });
                    }
                }
            }
        }
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
}

/// Birthdays, as reminders like any other.
///
/// This is the only reason a person node reaches the planner at all, and it
/// is why `get_active_tasks_and_events` has always selected them: the query
/// asked for people with a birthday, and then nothing here read the answer.
/// The desktop announced birthdays from a second copy of this logic in
/// `chat_engine`, and the phone — which is handed its reminders in advance
/// and cannot run a loop of its own — announced none at all.
fn plan_person(
    node: &NodeMetadata,
    from: NaiveDateTime,
    to: NaiveDateTime,
    scan_until: NaiveDate,
    out: &mut Vec<PlannedReminder>,
) {
    plan_birthday(node, from, to, scan_until, out);
    plan_keep_in_touch(node, from, to, scan_until, out);
}

/// The nudge that turns an address book into a personal CRM.
///
/// Somebody says they want to speak to a person monthly; a month after the
/// last time they did, this says so. It used to be a panel inside the app,
/// which meant it only ever reached somebody who had already opened the
/// screen to check — the one moment they did not need reminding.
///
/// Due dates repeat: a cadence missed is asked about again one cadence later,
/// not once and never again, and not every day either. A daily nag across two
/// hundred contacts is a notification people turn off.
fn plan_keep_in_touch(
    node: &NodeMetadata,
    from: NaiveDateTime,
    to: NaiveDateTime,
    scan_until: NaiveDate,
    out: &mut Vec<PlannedReminder>,
) {
    let Some(frequency) = node.properties.get("contact_frequency").and_then(|v| v.as_str()) else {
        return;
    };
    let Some(cadence) = cadence_days(frequency.trim()) else {
        return;
    };
    let Some(last) = node.properties.get("last_contacted").and_then(|v| v.as_str()) else {
        // Nothing to count from. Somebody just added is not already overdue.
        return;
    };
    let Ok(last) = NaiveDate::parse_from_str(last.trim(), "%Y-%m-%d") else {
        return;
    };

    let mut day = from.date().max(last);
    while day <= scan_until {
        let since = (day - last).num_days();
        if since > 0 && since % cadence == 0 {
            let date_str = day.format("%Y-%m-%d").to_string();
            if let Some(subject_at) = at(&date_str, KEEP_IN_TOUCH_HOUR) {
                if subject_at >= from && subject_at <= to {
                    out.push(PlannedReminder {
                        target_id: node.id.clone(),
                        target_type: "person",
                        title: node.title.clone(),
                        // Its own offset, so a birthday and a nudge on the
                        // same day stay two separate announcements rather
                        // than one swallowing the other's delivery record.
                        offset: "touch".to_string(),
                        occurrence_date: date_str,
                        trigger_at: subject_at,
                        subject_at,
                        // The first time it comes round is a reminder. Every
                        // one after that is a run of silence.
                        overdue: since > cadence,
                    });
                }
            }
        }
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
}

fn plan_birthday(
    node: &NodeMetadata,
    from: NaiveDateTime,
    to: NaiveDateTime,
    scan_until: NaiveDate,
    out: &mut Vec<PlannedReminder>,
) {
    let Some(raw) = node.properties.get("birthday").and_then(|v| v.as_str()) else {
        return;
    };
    let Some((month, dom)) = parse_birthday(raw) else {
        return;
    };

    let offsets = offsets_for(node);
    let mut day = from.date();
    while day <= scan_until {
        if is_anniversary(day, month, dom) {
            let date_str = day.format("%Y-%m-%d").to_string();
            if let Some(subject_at) = at(&date_str, BIRTHDAY_HOUR) {
                for offset in &offsets {
                    let Some(reach) = parse_offset(offset) else {
                        continue;
                    };
                    let trigger_at = subject_at - reach;
                    if trigger_at >= from && trigger_at <= to {
                        out.push(PlannedReminder {
                            target_id: node.id.clone(),
                            target_type: "person",
                            title: node.title.clone(),
                            offset: offset.clone(),
                            occurrence_date: date_str.clone(),
                            trigger_at,
                            subject_at,
                            // A birthday cannot be late the way a task can.
                            overdue: false,
                        });
                    }
                }
            }
        }
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
}

/// Reminders for money that is due back, or due to be paid.
///
/// Unlike everything else here, one node produces many reminders: the debts
/// ledger is a single file holding a list, so the debt's own id joins the
/// node's to make each reminder addressable on its own.
///
/// A debt marked settled says nothing, and neither does one with no due date —
/// plenty of lending between friends has no date attached, and inventing one to
/// nag about would be the app making up a commitment nobody agreed to.
fn plan_debts(
    node: &NodeMetadata,
    from: NaiveDateTime,
    to: NaiveDateTime,
    out: &mut Vec<PlannedReminder>,
) {
    let Some(debts) = node.properties.get("debts").and_then(|v| v.as_array()) else {
        return;
    };

    for debt in debts {
        if debt.get("status").and_then(|v| v.as_str()) == Some("completed") {
            continue;
        }
        let Some(id) = debt.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(due_date) = debt.get("dueDate").and_then(|v| v.as_str()) else {
            continue;
        };
        let date_part = due_date.split('T').next().unwrap_or("").trim();
        if date_part.is_empty() {
            continue;
        }
        let Some(subject_at) = at(date_part, DEFAULT_TASK_HOUR) else {
            continue;
        };

        let person = debt
            .get("person")
            .and_then(|v| v.as_str())
            .filter(|p| !p.is_empty())
            .unwrap_or("someone");
        let lending = debt.get("type").and_then(|v| v.as_str()) == Some("lend");
        let title = if lending {
            format!("{person} owes you")
        } else {
            format!("You owe {person}")
        };

        // On the day, and the day before. A debt is not an appointment: the
        // useful notice is "this falls due tomorrow", not fifteen minutes.
        for offset in ["1d", "0m"] {
            let Some(reach) = parse_offset(offset) else { continue };
            let trigger_at = subject_at - reach;
            if trigger_at >= from && trigger_at <= to {
                out.push(PlannedReminder {
                    target_id: format!("{}#{}", node.id, id),
                    target_type: "finance_debt",
                    title: title.clone(),
                    offset: offset.to_string(),
                    occurrence_date: date_part.to_string(),
                    trigger_at,
                    subject_at,
                    overdue: false,
                });
            }
        }
    }
}

fn plan_task(
    node: &NodeMetadata,
    from: NaiveDateTime,
    to: NaiveDateTime,
    out: &mut Vec<PlannedReminder>,
) {
    let status = node
        .properties
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if status == "done" || status == "canceled" {
        return;
    }
    let Some(due_date) = node.properties.get("due_date").and_then(|v| v.as_str()) else {
        return;
    };
    if due_date.trim().is_empty() {
        return;
    }
    let Ok(due) = NaiveDate::parse_from_str(due_date.trim(), "%Y-%m-%d") else {
        return;
    };

    let time_of_day = task_time_of_day(&node.properties);
    let offsets = offsets_of(&node.properties);

    let emit = |day: NaiveDate, overdue: bool, out: &mut Vec<PlannedReminder>| {
        let date_str = day.format("%Y-%m-%d").to_string();
        let Some(subject_at) = at(&date_str, &time_of_day) else {
            return;
        };
        // An offset is advance notice, and there is no advance notice for
        // something already late. A task with "a day before" and "two hours
        // before" set used to produce two identical nags a day, both saying
        // it was overdue; now it produces one, at the hour it was due.
        let nag = ["0m".to_string()];
        let offsets: &[String] = if overdue { &nag } else { &offsets };
        for offset in offsets {
            let Some(reach) = parse_offset(offset) else { continue };
            let trigger_at = subject_at - reach;
            if trigger_at >= from && trigger_at <= to {
                out.push(PlannedReminder {
                    target_id: node.id.clone(),
                    target_type: "task",
                    title: node.title.clone(),
                    offset: offset.clone(),
                    occurrence_date: date_str.clone(),
                    trigger_at,
                    subject_at,
                    overdue,
                });
            }
        }
    };

    // The day it is due, so "a day before" means a day before.
    emit(due, false, out);

    // And then once a day for as long as it stays undone.
    //
    // Worked out per day rather than from a single "today", because the phone
    // is handed a whole week at once: taking today's date for all of it
    // queued one nag and then nothing, while a desktop — which recomputes
    // every minute — nagged every day. The two have to agree.
    let mut day = from.date().max(due.succ_opt().unwrap_or(due));
    let last = to.date();
    while day <= last {
        emit(day, true, out);
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(id: &str, node_type: &str, props: Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: id.to_string(),
            content: String::new(),
            properties: props,
            created_at: String::new(),
            updated_at: String::new(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn dt(s: &str) -> NaiveDateTime {
        at(s.split('T').next().unwrap(), s.split('T').nth(1).unwrap()).unwrap()
    }

    fn keys(plan: &[PlannedReminder]) -> Vec<String> {
        plan.iter().map(|r| format!("{} @ {}", r.delivery_key(), r.trigger_at)).collect()
    }

    /// The debts ledger is one file holding a list, so one node has to produce
    /// a reminder per debt — and each has to be addressable on its own or the
    /// second one would be taken for a repeat of the first.
    #[test]
    fn each_debt_in_the_ledger_gets_its_own_reminder() {
        let ledger = node(
            "Finance/Debts.json",
            "finance_debts",
            serde_json::json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "Mai", "dueDate": "2026-08-20", "status": "active" },
                { "id": "d2", "type": "borrow", "person": "Nam", "dueDate": "2026-08-21", "status": "active" },
            ]}),
        );

        let plan = plan(&[ledger], dt("2026-08-01T00:00:00"), dt("2026-08-31T23:59:59"), "UTC");
        let targets: std::collections::BTreeSet<&str> =
            plan.iter().map(|r| r.target_id.as_str()).collect();

        assert_eq!(
            targets.into_iter().collect::<Vec<_>>(),
            vec!["Finance/Debts.json#d1", "Finance/Debts.json#d2"]
        );
    }

    /// Which way round the money goes is the whole content of the reminder.
    #[test]
    fn a_reminder_says_which_direction_the_money_owes() {
        let ledger = node(
            "Finance/Debts.json",
            "finance_debts",
            serde_json::json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "Mai", "dueDate": "2026-08-20", "status": "active" },
                { "id": "d2", "type": "borrow", "person": "Nam", "dueDate": "2026-08-20", "status": "active" },
            ]}),
        );

        let plan = plan(&[ledger], dt("2026-08-01T00:00:00"), dt("2026-08-31T23:59:59"), "UTC");
        let titles: std::collections::BTreeSet<&str> =
            plan.iter().map(|r| r.title.as_str()).collect();

        assert!(titles.contains("Mai owes you"), "{titles:?}");
        assert!(titles.contains("You owe Nam"), "{titles:?}");
    }

    /// A day's notice and a note on the day. A debt is not an appointment, so
    /// fifteen minutes' warning would be no use to anybody.
    #[test]
    fn a_debt_is_announced_the_day_before_and_on_the_day() {
        let ledger = node(
            "Finance/Debts.json",
            "finance_debts",
            serde_json::json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "Mai", "dueDate": "2026-08-20", "status": "active" },
            ]}),
        );

        let plan = plan(&[ledger], dt("2026-08-01T00:00:00"), dt("2026-08-31T23:59:59"), "UTC");
        let offsets: Vec<&str> = plan.iter().map(|r| r.offset.as_str()).collect();

        assert_eq!(offsets.len(), 2, "{offsets:?}");
        assert!(offsets.contains(&"1d"));
        assert!(offsets.contains(&"0m"));
    }

    #[test]
    fn a_settled_debt_says_nothing() {
        let ledger = node(
            "Finance/Debts.json",
            "finance_debts",
            serde_json::json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "Mai", "dueDate": "2026-08-20", "status": "completed" },
            ]}),
        );

        assert!(plan(&[ledger], dt("2026-08-01T00:00:00"), dt("2026-08-31T23:59:59"), "UTC").is_empty());
    }

    /// Plenty of lending between friends has no date on it, and inventing one
    /// to nag about would be the app making up a commitment nobody agreed to.
    #[test]
    fn a_debt_with_no_due_date_says_nothing() {
        let ledger = node(
            "Finance/Debts.json",
            "finance_debts",
            serde_json::json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "Mai", "status": "active" },
                { "id": "d2", "type": "lend", "person": "Nam", "dueDate": "", "status": "active" },
            ]}),
        );

        assert!(plan(&[ledger], dt("2026-08-01T00:00:00"), dt("2026-08-31T23:59:59"), "UTC").is_empty());
    }

    #[test]
    fn an_empty_debts_ledger_says_nothing() {
        let ledger = node("Finance/Debts.json", "finance_debts", serde_json::json!({ "debts": [] }));
        assert!(plan(&[ledger], dt("2026-08-01T00:00:00"), dt("2026-08-31T23:59:59"), "UTC").is_empty());
    }

    #[test]
    fn an_offset_is_a_number_and_a_unit_or_it_is_nothing() {
        assert_eq!(parse_offset("15m"), Duration::try_minutes(15));
        assert_eq!(parse_offset("2h"), Duration::try_hours(2));
        assert_eq!(parse_offset("1d"), Duration::try_days(1));
        assert_eq!(parse_offset(" 30m "), Duration::try_minutes(30));
        assert_eq!(parse_offset("0m"), Duration::try_minutes(0));
        for bad in ["", "m", "soon", "15", "15x", "-5m", "1.5h"] {
            assert!(parse_offset(bad).is_none(), "{:?} is not an offset", bad);
        }
    }

    /// The bug this phase exists for. Only today and tomorrow used to be
    /// looked at, so a reminder set for a week before an event was not found
    /// until the event was almost here — and then arrived at once, six days
    /// late, which is not a week's notice by any reading.
    #[test]
    fn a_reminder_a_week_out_is_found_a_week_out() {
        let nodes = vec![node(
            "Events/launch.md",
            "event",
            json!({ "start_at": "2026-03-20T09:00", "reminders": ["1w-not-valid", "7d", "15m"] }),
        )];
        // Planning the minute the week's notice is due.
        let plan = plan(&nodes, dt("2026-03-13T08:59"), dt("2026-03-13T09:01"), "");
        assert_eq!(
            keys(&plan),
            ["Events/launch.md_2026-03-20_7d @ 2026-03-13 09:00:00"],
        );
    }

    #[test]
    fn the_fifteen_minute_notice_still_arrives_fifteen_minutes_before() {
        let nodes = vec![node(
            "Events/launch.md",
            "event",
            json!({ "start_at": "2026-03-20T09:00", "reminders": ["7d", "15m"] }),
        )];
        let plan = plan(&nodes, dt("2026-03-20T08:44"), dt("2026-03-20T08:46"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].offset, "15m");
        assert_eq!(plan[0].trigger_at, dt("2026-03-20T08:45"));
    }

    #[test]
    fn an_event_with_no_reminder_is_still_announced_when_it_starts() {
        let nodes = vec![node("Events/a.md", "event", json!({ "start_at": "2026-03-20T09:00" }))];
        let plan = plan(&nodes, dt("2026-03-20T08:00"), dt("2026-03-20T10:00"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].offset, "0m");
        assert_eq!(plan[0].trigger_at, dt("2026-03-20T09:00"));
    }

    /// Every Monday is a different reminder, and the delivery key has to say
    /// so or the second week is treated as already sent.
    #[test]
    fn each_occurrence_of_a_series_is_its_own_reminder() {
        let nodes = vec![node(
            "Events/standup.md",
            "event",
            json!({ "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY", "reminders": ["0m"] }),
        )];
        let plan = plan(&nodes, dt("2026-03-01T00:00"), dt("2026-03-31T23:59"), "");
        assert_eq!(
            keys(&plan),
            [
                "Events/standup.md_2026-03-02_0m @ 2026-03-02 09:00:00",
                "Events/standup.md_2026-03-09_0m @ 2026-03-09 09:00:00",
                "Events/standup.md_2026-03-16_0m @ 2026-03-16 09:00:00",
                "Events/standup.md_2026-03-23_0m @ 2026-03-23 09:00:00",
                "Events/standup.md_2026-03-30_0m @ 2026-03-30 09:00:00",
            ],
        );
    }

    #[test]
    fn an_occurrence_the_user_cancelled_is_not_announced() {
        let nodes = vec![node(
            "Events/standup.md",
            "event",
            json!({
                "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY",
                "exceptions": ["2026-03-09"], "reminders": ["0m"]
            }),
        )];
        let plan = plan(&nodes, dt("2026-03-08T00:00"), dt("2026-03-10T23:59"), "");
        assert!(plan.is_empty());
    }

    /// The reminder for a meeting in Tokyo is fifteen minutes before it
    /// happens, not fifteen minutes before the same numbers read here.
    #[test]
    fn a_meeting_abroad_is_announced_relative_to_when_it_actually_starts() {
        let nodes = vec![node(
            "Events/tokyo.md",
            "event",
            json!({ "start_at": "2026-03-20T09:00", "tzid": "Asia/Tokyo", "reminders": ["15m"] }),
        )];
        // 09:00 in Tokyo is 19:00 the evening before in New York.
        let plan = plan(&nodes, dt("2026-03-19T00:00"), dt("2026-03-20T23:59"), "America/New_York");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].subject_at, dt("2026-03-19T20:00"));
        assert_eq!(plan[0].trigger_at, dt("2026-03-19T19:45"));
    }

    #[test]
    fn nothing_outside_the_window_is_planned() {
        let nodes = vec![node("Events/a.md", "event", json!({ "start_at": "2026-03-20T09:00" }))];
        assert!(plan(&nodes, dt("2026-03-20T09:01"), dt("2026-03-20T10:00"), "").is_empty());
        assert!(plan(&nodes, dt("2026-03-19T00:00"), dt("2026-03-19T23:59"), "").is_empty());
        assert!(plan(&nodes, dt("2026-03-20T10:00"), dt("2026-03-20T09:00"), "").is_empty());
    }

    #[test]
    fn a_task_is_announced_at_the_time_of_day_it_is_due() {
        let nodes = vec![node(
            "Tasks/a.md",
            "task",
            json!({ "due_date": "2026-03-20", "due_time": "15:30", "status": "todo" }),
        )];
        let plan = plan(&nodes, dt("2026-03-20T00:00"), dt("2026-03-20T23:59"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].trigger_at, dt("2026-03-20T15:30"));
        assert!(!plan[0].overdue);
    }

    /// The same bug as the week's notice, on the other kind of thing: a task
    /// due Friday with a day's notice was skipped until Friday.
    #[test]
    fn a_task_with_a_days_notice_is_announced_the_day_before() {
        let nodes = vec![node(
            "Tasks/a.md",
            "task",
            json!({ "due_date": "2026-03-20", "due_time": "09:00", "reminders": ["1d"], "status": "todo" }),
        )];
        let plan = plan(&nodes, dt("2026-03-19T00:00"), dt("2026-03-19T23:59"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].trigger_at, dt("2026-03-19T09:00"));
    }

    #[test]
    fn a_task_that_has_slipped_is_nagged_about_today_rather_than_the_day_it_was_due() {
        let nodes = vec![node(
            "Tasks/a.md",
            "task",
            json!({ "due_date": "2026-03-10", "due_time": "09:00", "status": "todo" }),
        )];
        let plan = plan(&nodes, dt("2026-03-20T00:00"), dt("2026-03-20T23:59"), "");
        assert_eq!(plan.len(), 1);
        assert!(plan[0].overdue);
        assert_eq!(plan[0].occurrence_date, "2026-03-20", "today, so it nags once a day");
        assert_eq!(plan[0].trigger_at, dt("2026-03-20T09:00"));
    }

    /// A decision, not an accident. An offset is advance notice, and there is
    /// no advance notice for something already late — so a task with both "a
    /// day before" and "two hours before" set used to produce two identical
    /// nags every day, each saying it was overdue.
    #[test]
    fn an_overdue_task_nags_once_a_day_however_many_offsets_it_has() {
        let nodes = vec![node(
            "Tasks/a.md",
            "task",
            json!({
                "due_date": "2026-03-01", "due_time": "17:00",
                "reminders": ["1d", "2h", "15m"], "status": "todo",
            }),
        )];
        let plan = plan(&nodes, dt("2026-03-05T00:00"), dt("2026-03-06T23:59"), "");
        assert_eq!(
            keys(&plan),
            [
                "Tasks/a.md_2026-03-05_0m @ 2026-03-05 17:00:00",
                "Tasks/a.md_2026-03-06_0m @ 2026-03-06 17:00:00",
            ],
        );
    }

    /// But before it is late, every offset still means what it says.
    #[test]
    fn a_task_still_gets_all_its_advance_notice_before_it_is_due() {
        let nodes = vec![node(
            "Tasks/a.md",
            "task",
            json!({
                "due_date": "2026-03-05", "due_time": "17:00",
                "reminders": ["1d", "2h"], "status": "todo",
            }),
        )];
        let plan = plan(&nodes, dt("2026-03-01T00:00"), dt("2026-03-05T23:59"), "");
        assert_eq!(
            keys(&plan),
            [
                "Tasks/a.md_2026-03-05_1d @ 2026-03-04 17:00:00",
                "Tasks/a.md_2026-03-05_2h @ 2026-03-05 15:00:00",
            ],
        );
    }

    /// The gap this closed: a subscribed calendar is a cache rather than a
    /// file, so it never reached this loop as a node — and subscribing to a
    /// team's calendar meant never being told about a single meeting in it.
    #[test]
    fn an_event_that_is_not_a_file_can_still_be_announced() {
        let subscribed = vec![EventSummary::from_properties(
            "subscription:s1/standup",
            "Team standup",
            "",
            &json!({ "start_at": "2026-03-20T09:00", "end_at": "2026-03-20T09:15" }),
        )];
        let plan = plan_with(&[], &subscribed, dt("2026-03-20T08:00"), dt("2026-03-20T10:00"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].target_id, "subscription:s1/standup");
        assert_eq!(plan[0].title, "Team standup");
        assert_eq!(plan[0].offset, "0m", "somebody else's calendar carries no offsets");
        assert_eq!(plan[0].trigger_at, dt("2026-03-20T09:00"));
    }

    /// And it recurs the way it says it does.
    #[test]
    fn a_subscribed_series_is_announced_every_time_it_happens() {
        let subscribed = vec![EventSummary::from_properties(
            "subscription:s1/weekly",
            "Weekly sync",
            "",
            &json!({ "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY" }),
        )];
        let plan = plan_with(&[], &subscribed, dt("2026-03-01T00:00"), dt("2026-03-31T23:59"), "");
        assert_eq!(plan.len(), 5, "five Mondays in March 2026");
    }

    #[test]
    fn a_finished_task_says_nothing() {
        for status in ["done", "canceled"] {
            let nodes = vec![node(
                "Tasks/a.md",
                "task",
                json!({ "due_date": "2026-03-20", "status": status }),
            )];
            assert!(plan(&nodes, dt("2026-03-20T00:00"), dt("2026-03-20T23:59"), "").is_empty());
        }
    }

    #[test]
    fn a_task_with_no_due_date_says_nothing() {
        let nodes = vec![
            node("Tasks/a.md", "task", json!({ "status": "todo" })),
            node("Tasks/b.md", "task", json!({ "due_date": "", "status": "todo" })),
            node("Tasks/c.md", "task", json!({ "due_date": "whenever", "status": "todo" })),
        ];
        assert!(plan(&nodes, dt("2026-03-20T00:00"), dt("2026-03-20T23:59"), "").is_empty());
    }

    #[test]
    fn a_task_falls_back_to_the_time_older_vaults_recorded() {
        assert_eq!(task_time_of_day(&json!({ "due_time": "15:30" })), "15:30");
        assert_eq!(task_time_of_day(&json!({ "start_time": "07:45:00" })), "07:45:00");
        assert_eq!(task_time_of_day(&json!({ "due_time": "" , "start_time": "07:45:00" })), "07:45:00");
        assert_eq!(task_time_of_day(&json!({})), "09:00:00");
        assert_eq!(task_time_of_day(&json!({ "due_time": 1530 })), "09:00:00");
    }

    /// The phone replaces a scheduled reminder rather than adding a second
    /// one, which only works if the same reminder keeps the same number
    /// between runs of the app.
    #[test]
    fn the_handle_the_operating_system_gets_is_stable_and_distinct() {
        let nodes = vec![node(
            "Events/standup.md",
            "event",
            json!({ "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY", "reminders": ["0m", "15m"] }),
        )];
        let first = plan(&nodes, dt("2026-03-01T00:00"), dt("2026-03-31T23:59"), "");
        let again = plan(&nodes, dt("2026-03-01T00:00"), dt("2026-03-31T23:59"), "");

        let ids: Vec<i32> = first.iter().map(|r| r.os_id()).collect();
        assert_eq!(ids, again.iter().map(|r| r.os_id()).collect::<Vec<_>>());
        assert!(ids.iter().all(|id| *id > 0), "the plugin counts in positive i32");

        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "two reminders must not share a handle");
    }

    /// The invariant this whole phase is built on.
    ///
    /// A phone is handed the week ahead in one go; a desktop wakes every
    /// minute and asks what has just come due. If those two ever disagreed,
    /// the same appointment would ring twice on one device and not at all on
    /// the other — so the union of a week of desktop windows has to be
    /// exactly the week the phone was given.
    #[test]
    fn the_week_a_phone_is_given_is_the_week_a_desktop_would_announce() {
        let nodes = vec![
            node("Events/standup.md", "event", json!({
                "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY;BYDAY=MO,WE,FR",
                "reminders": ["0m", "15m", "1d"],
            })),
            node("Events/tokyo.md", "event", json!({
                "start_at": "2026-03-03T09:00", "tzid": "Asia/Tokyo", "reminders": ["30m"],
            })),
            node("Events/trip.md", "event", json!({
                "start_at": "2026-03-04T08:00", "end_at": "2026-03-06T18:00", "reminders": ["2d"],
            })),
            node("Tasks/report.md", "task", json!({
                "due_date": "2026-03-05", "due_time": "17:00", "reminders": ["1d", "2h"],
                "status": "todo",
            })),
            node("Tasks/late.md", "task", json!({
                "due_date": "2026-02-01", "due_time": "09:00", "status": "in_progress",
            })),
        ];

        let start = dt("2026-03-01T00:00:00");
        let week = start + Duration::try_days(7).unwrap();

        let handed_to_the_phone: Vec<String> =
            plan(&nodes, start, week, "America/New_York")
                .iter()
                .map(|r| r.delivery_key())
                .collect();

        // A desktop, waking every hour across the same week.
        let mut announced: Vec<String> = Vec::new();
        let mut cursor = start;
        while cursor < week {
            let next = cursor + Duration::try_hours(1).unwrap();
            for due in plan(&nodes, cursor, next.min(week), "America/New_York") {
                let key = due.delivery_key();
                if !announced.contains(&key) {
                    announced.push(key);
                }
            }
            cursor = next;
        }

        assert!(!handed_to_the_phone.is_empty(), "the week must contain something");
        let mut a = handed_to_the_phone.clone();
        let mut b = announced.clone();
        a.sort();
        b.sort();
        b.dedup();
        assert_eq!(a, b, "the phone and the desktop must agree about the week");
    }

    #[test]
    fn a_plan_comes_back_in_the_order_it_will_happen() {
        let nodes = vec![
            node("Events/late.md", "event", json!({ "start_at": "2026-03-20T17:00" })),
            node("Events/early.md", "event", json!({ "start_at": "2026-03-20T08:00" })),
            node("Tasks/noon.md", "task", json!({ "due_date": "2026-03-20", "due_time": "12:00", "status": "todo" })),
        ];
        let plan = plan(&nodes, dt("2026-03-20T00:00"), dt("2026-03-20T23:59"), "");
        let order: Vec<&str> = plan.iter().map(|r| r.target_id.as_str()).collect();
        assert_eq!(order, ["Events/early.md", "Tasks/noon.md", "Events/late.md"]);
    }

    // ── Birthdays ───────────────────────────────────────────────

    #[test]
    fn a_birthday_is_read_with_or_without_its_year() {
        // Both shapes reach the vault: the contact form writes the first, and
        // somebody who knows the day but not the year writes the second.
        assert_eq!(parse_birthday("1994-03-02"), Some((3, 2)));
        assert_eq!(parse_birthday("03-02"), Some((3, 2)));
        assert_eq!(parse_birthday(" 1994-3-2 "), Some((3, 2)));
        assert_eq!(parse_birthday("2028-02-29"), Some((2, 29)));

        for bad in ["", "March", "1994", "1994-13-02", "1994-02-30", "1994-03-02-01"] {
            assert!(parse_birthday(bad).is_none(), "{:?} is not a birthday", bad);
        }
    }

    #[test]
    fn a_birthday_without_a_year_is_still_announced() {
        // The regression this guards: the planner took only `YYYY-MM-DD`, so
        // a `MM-DD` birthday sat in the vault, on display in the People app,
        // and was never once announced.
        let nodes = vec![node("People/mai.md", "person", json!({ "birthday": "03-02" }))];
        let plan = plan(&nodes, dt("2026-03-02T00:00"), dt("2026-03-02T23:59"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].target_type, "person");
        assert_eq!(plan[0].offset, "0m");
        assert_eq!(plan[0].trigger_at, dt("2026-03-02T09:00"));
    }

    #[test]
    fn a_birthday_warns_the_day_before_and_again_on_the_day() {
        let nodes = vec![node("People/mai.md", "person", json!({ "birthday": "1994-03-02" }))];
        let plan = plan(&nodes, dt("2026-02-25T00:00"), dt("2026-03-05T23:59"), "");
        assert_eq!(
            keys(&plan),
            [
                "People/mai.md_2026-03-02_1d @ 2026-03-01 09:00:00",
                "People/mai.md_2026-03-02_0m @ 2026-03-02 09:00:00",
            ]
        );
    }

    #[test]
    fn a_birthday_is_announced_once_per_year_it_comes_round() {
        let nodes = vec![node("People/mai.md", "person", json!({ "birthday": "1994-03-02" }))];
        // Two years, so the delivery keys have to differ or the second year
        // would be swallowed as already-announced.
        let plan = plan(&nodes, dt("2026-01-01T00:00"), dt("2027-12-31T23:59"), "");
        let on_the_day: Vec<&str> = plan
            .iter()
            .filter(|r| r.offset == "0m")
            .map(|r| r.occurrence_date.as_str())
            .collect();
        assert_eq!(on_the_day, ["2026-03-02", "2027-03-02"]);
    }

    #[test]
    fn a_leap_day_birthday_falls_back_to_the_28th() {
        // The same rule `calendar::recurrence` applies to a yearly series that
        // overshoots its month. The two have to agree, or one birthday lands
        // on two different days depending on which part of the app is asked.
        let nodes = vec![node("People/leap.md", "person", json!({ "birthday": "2028-02-29" }))];

        let common = plan(&nodes, dt("2027-02-01T00:00"), dt("2027-03-05T23:59"), "");
        let days: Vec<&str> = common
            .iter()
            .filter(|r| r.offset == "0m")
            .map(|r| r.occurrence_date.as_str())
            .collect();
        assert_eq!(days, ["2027-02-28"], "a common year keeps it on the 28th");

        let leap = plan(&nodes, dt("2028-02-01T00:00"), dt("2028-03-05T23:59"), "");
        let days: Vec<&str> = leap
            .iter()
            .filter(|r| r.offset == "0m")
            .map(|r| r.occurrence_date.as_str())
            .collect();
        assert_eq!(days, ["2028-02-29"], "a leap year uses the real day");
    }

    #[test]
    fn a_person_without_a_birthday_plans_nothing() {
        let nodes = vec![
            node("People/nam.md", "person", json!({ "contact_frequency": "monthly" })),
            node("People/bad.md", "person", json!({ "birthday": "sometime in March" })),
            node("People/empty.md", "person", json!({ "birthday": "" })),
        ];
        assert!(plan(&nodes, dt("2026-01-01T00:00"), dt("2026-12-31T23:59"), "").is_empty());
    }

    // ── Keeping in touch ────────────────────────────────────────

    fn tracked(last: &str, frequency: &str) -> NodeMetadata {
        node(
            "People/an.md",
            "person",
            json!({ "last_contacted": last, "contact_frequency": frequency }),
        )
    }

    #[test]
    fn a_nudge_arrives_one_cadence_after_the_last_contact() {
        // The whole point of the feature: it reaches somebody who has *not*
        // opened the app to check, which is the only moment it is useful.
        let nodes = vec![tracked("2026-03-02", "monthly")];
        let plan = plan(&nodes, dt("2026-04-01T00:00"), dt("2026-04-01T23:59"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].target_type, "person");
        assert_eq!(plan[0].offset, "touch");
        assert_eq!(plan[0].trigger_at, dt("2026-04-01T10:00"));
        assert!(!plan[0].overdue, "the first time round is a reminder, not a reproach");
    }

    #[test]
    fn nothing_is_said_before_the_cadence_is_up() {
        let nodes = vec![tracked("2026-03-02", "monthly")];
        assert!(plan(&nodes, dt("2026-03-03T00:00"), dt("2026-03-31T23:59"), "").is_empty());
    }

    #[test]
    fn a_missed_cadence_is_asked_about_again_one_cadence_later() {
        // Not daily. Two hundred contacts nagging every morning is a
        // notification people turn off, and then the feature is worth nothing.
        let nodes = vec![tracked("2026-03-01", "weekly")];
        let plan = plan(&nodes, dt("2026-03-01T00:00"), dt("2026-04-01T23:59"), "");
        let days: Vec<&str> = plan.iter().map(|r| r.occurrence_date.as_str()).collect();
        assert_eq!(days, ["2026-03-08", "2026-03-15", "2026-03-22", "2026-03-29"]);

        // The first is a reminder; the rest are a run of silence.
        assert!(!plan[0].overdue);
        assert!(plan[1..].iter().all(|r| r.overdue));
    }

    #[test]
    fn a_person_with_no_cadence_is_never_nudged() {
        // Somebody whose address you keep and nothing more.
        let nodes = vec![node(
            "People/an.md",
            "person",
            json!({ "last_contacted": "2020-01-01" }),
        )];
        assert!(plan(&nodes, dt("2026-03-01T00:00"), dt("2026-04-01T23:59"), "").is_empty());
    }

    #[test]
    fn somebody_just_added_is_not_already_overdue() {
        // No last contact means nothing to count from. Importing two thousand
        // contacts must not produce two thousand accusations.
        let nodes = vec![node(
            "People/an.md",
            "person",
            json!({ "contact_frequency": "weekly" }),
        )];
        assert!(plan(&nodes, dt("2026-03-01T00:00"), dt("2026-04-01T23:59"), "").is_empty());
    }

    #[test]
    fn a_cadence_nobody_recognises_says_nothing_rather_than_guessing() {
        let nodes = vec![tracked("2026-03-01", "fortnightly")];
        assert!(plan(&nodes, dt("2026-03-01T00:00"), dt("2026-05-01T23:59"), "").is_empty());
    }

    #[test]
    fn a_birthday_and_a_nudge_on_one_day_stay_two_announcements() {
        // They share a target and a date, so only the offset keeps their
        // delivery records apart — without that, one would be swallowed as
        // already-announced and never arrive.
        let nodes = vec![node(
            "People/an.md",
            "person",
            json!({
                "birthday": "1994-04-01",
                "last_contacted": "2026-03-02",
                "contact_frequency": "monthly",
            }),
        )];
        let plan = plan(&nodes, dt("2026-04-01T00:00"), dt("2026-04-01T23:59"), "");
        let keys: Vec<String> = plan.iter().map(|r| r.delivery_key()).collect();
        assert_eq!(
            keys,
            [
                "People/an.md_2026-04-01_0m",
                "People/an.md_2026-04-01_touch",
            ]
        );
    }

    #[test]
    fn a_nudge_is_the_same_nudge_tomorrow() {
        // The delivery record is what stops a reminder repeating every time
        // the loop wakes.
        let nodes = vec![tracked("2026-03-02", "monthly")];
        let monday = plan(&nodes, dt("2026-04-01T09:00"), dt("2026-04-01T23:59"), "");
        let tuesday = plan(&nodes, dt("2026-04-01T00:00"), dt("2026-04-02T23:59"), "");
        assert_eq!(monday[0].delivery_key(), tuesday[0].delivery_key());
    }

    #[test]
    fn a_birthday_on_the_calendar_does_not_announce_itself() {
        // The person is what announces a birthday. The event exists so the
        // day shows up in the month grid; if it spoke too, every birthday
        // would arrive twice.
        let nodes = vec![
            node(
                "Events/birthday-an.md",
                "event",
                json!({
                    "start_at": "2024-04-01",
                    "is_all_day": true,
                    "recurrence": "yearly",
                    "source_person_id": "People/an.md",
                }),
            ),
            node("People/an.md", "person", json!({ "birthday": "1994-04-01" })),
        ];

        let plan = plan(&nodes, dt("2026-04-01T00:00"), dt("2026-04-01T23:59"), "");
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].target_id, "People/an.md");
    }

    #[test]
    fn an_ordinary_event_still_announces_itself() {
        let nodes = vec![node(
            "Events/standup.md",
            "event",
            json!({ "start_at": "2026-04-01T09:00" }),
        )];
        assert_eq!(plan(&nodes, dt("2026-04-01T00:00"), dt("2026-04-01T23:59"), "").len(), 1);
    }

    #[test]
    fn a_person_may_set_their_own_warning() {
        let nodes = vec![node(
            "People/mai.md",
            "person",
            json!({ "birthday": "1994-03-02", "reminders": ["7d"] }),
        )];
        // A week's notice is further than the default, so the scan window has
        // to reach it — this is what `furthest_reach` is for.
        let plan = plan(&nodes, dt("2026-02-20T00:00"), dt("2026-02-25T23:59"), "");
        assert_eq!(keys(&plan), ["People/mai.md_2026-03-02_7d @ 2026-02-23 09:00:00"]);
    }
}
