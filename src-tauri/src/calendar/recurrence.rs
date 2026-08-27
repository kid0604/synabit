use super::rrule::{anchor_on_or_before, ordinal_of, RRule};
use super::tz::{convert_stamp, is_known_zone};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An event, carrying only what a calendar view or a reminder needs.
///
/// Deliberately without `content`: the body of an event is the description
/// shown in the edit form, and only for the one event a user opens. Sending
/// every body with every range query is what made the old `get_nodes('event')`
/// payload scale with the size of the vault rather than with the days on
/// screen.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EventSummary {
    pub id: String,
    /// An identity that outlives the file's path.
    ///
    /// Sync writes `node_id` into the frontmatter and keeps it there across
    /// moves, renames and machines; a file it has not reached falls back to
    /// its path. This is what an exported `UID` carries, so a calendar that
    /// took this event once recognises it again rather than making a copy.
    pub uid: String,
    pub title: String,
    pub is_all_day: bool,
    pub start_at: String,
    pub end_at: String,
    pub location: String,
    pub tags: Vec<String>,
    /// A colour name the user chose; empty means the default.
    #[serde(default)]
    pub colour: String,
    /// The zone the stored wall clock belongs to — `Asia/Tokyo`. Empty means
    /// floating: nine o'clock wherever the reader happens to be, which is
    /// what every event written before this field existed is.
    pub tzid: String,
    /// An RFC 5545 rule — `FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE`. Authoritative
    /// when present; the two fields below are what vaults written before it
    /// stored, and are only read when this is empty.
    pub rrule: String,
    pub recurrence: String,
    pub recurrence_end_at: String,
    pub series_id: String,
    pub exceptions: Vec<String>,
    pub reminders: Vec<String>,
    pub relations: Vec<String>,
    pub created_at: String,
    /// The subscribed calendar this came from, or empty for the user's own.
    ///
    /// An event with one of these is somebody else's: it is a cache of a feed,
    /// the next refresh will overwrite it, and nothing in this app may offer
    /// to edit it. Everything downstream reads this rather than guessing from
    /// where the event was loaded.
    #[serde(default)]
    pub subscription_id: String,
}

/// One day that one event lands on. `event` indexes into
/// [`EventsInRange::events`], so a daily series over a year costs one summary
/// and 365 of these rather than 365 copies of the event.
///
/// `start_at` and `end_at` belong to *this instance*, not to the series. The
/// stored event carries the first occurrence's times; the tenth Monday of a
/// weekly stand-up has to say so itself, or a time axis has nothing to draw
/// with. Every day of a multi-day instance repeats that instance's bounds, so
/// a view can tell "starts today" from "still running from Tuesday".
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct OccurrenceRef {
    pub date: String,
    pub event: usize,
    pub start_at: String,
    pub end_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct EventsInRange {
    pub events: Vec<EventSummary>,
    pub occurrences: Vec<OccurrenceRef>,
}

fn str_prop(props: &Value, key: &str) -> String {
    props
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn str_list(props: &Value, key: &str) -> Vec<String> {
    match props.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        // A single tag written as a bare string in frontmatter.
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

pub fn date_part(s: &str) -> &str {
    s.split('T').next().unwrap_or(s)
}

/// The clock part of a stored timestamp, or `""` for a bare date.
fn time_part(s: &str) -> &str {
    s.split_once('T').map(|(_, t)| t).unwrap_or("")
}

/// `date` and `time` back together, or just `date` when there is no time.
fn stamp(date: NaiveDate, time: &str) -> String {
    let d = date.format("%Y-%m-%d").to_string();
    if time.is_empty() {
        d
    } else {
        format!("{}T{}", d, time)
    }
}

impl EventSummary {
    /// Read an event out of a node's frontmatter, including the shapes older
    /// vaults wrote.
    ///
    /// Before `start_at` existed, an event stored `event_date` alongside
    /// `start_time`/`event_time`/`end_time`. That fallback used to run in the
    /// front end; it has to run here now, because the front end no longer sees
    /// raw properties. Dropping it would make every pre-migration event vanish
    /// from the calendar.
    pub fn from_properties(id: &str, title: &str, created_at: &str, props: &Value) -> Self {
        let mut is_all_day = props.get("is_all_day").and_then(|v| v.as_bool()) == Some(true);
        let mut start_at = str_prop(props, "start_at");
        let mut end_at = str_prop(props, "end_at");

        if start_at.is_empty() {
            let event_date = str_prop(props, "event_date");
            if !event_date.is_empty() {
                let start_time = {
                    let s = str_prop(props, "start_time");
                    if s.is_empty() {
                        str_prop(props, "event_time")
                    } else {
                        s
                    }
                };
                if start_time.is_empty() {
                    start_at = event_date.clone();
                    is_all_day = true;
                } else {
                    start_at = format!("{}T{}:00", event_date, start_time);
                    is_all_day = false;
                }
                let end_time = str_prop(props, "end_time");
                if end_at.is_empty() && !end_time.is_empty() {
                    end_at = format!("{}T{}:00", event_date, end_time);
                }
            }
        }
        if end_at.is_empty() && is_all_day {
            end_at = start_at.clone();
        }

        let relations = {
            let r = str_list(props, "relations");
            if r.is_empty() {
                str_list(props, "related_notes")
            } else {
                r
            }
        };

        // `timezone` was declared years ago and never written; reading it as
        // a fallback costs nothing and covers anything that ever did.
        let tzid = {
            let t = str_prop(props, "tzid");
            if t.is_empty() { str_prop(props, "timezone") } else { t }
        };
        let recurrence = {
            let r = str_prop(props, "recurrence");
            if r.is_empty() {
                "none".to_string()
            } else {
                r
            }
        };
        let rrule = str_prop(props, "rrule");

        let uid = props
            .get("node_id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .to_string();

        Self {
            id: id.to_string(),
            uid,
            title: title.to_string(),
            is_all_day,
            start_at,
            end_at,
            location: str_prop(props, "location"),
            tags: str_list(props, "tags"),
            colour: str_prop(props, "colour"),
            tzid,
            rrule,
            recurrence,
            recurrence_end_at: str_prop(props, "recurrence_end_at"),
            series_id: str_prop(props, "series_id"),
            exceptions: str_list(props, "exceptions"),
            reminders: str_list(props, "reminders"),
            relations,
            created_at: created_at.to_string(),
            subscription_id: String::new(),
        }
    }

    /// The rule this event repeats by, whichever way it was written down.
    pub fn rule(&self) -> Option<RRule> {
        if !self.rrule.trim().is_empty() {
            // A stored rule wins outright. Legacy keys may still be sitting in
            // the file, and reading them as a second opinion is how two
            // sources of truth get created.
            return RRule::parse(&self.rrule);
        }
        RRule::from_legacy(&self.recurrence, &self.recurrence_end_at)
    }

    pub fn occurs_on(&self, target_date_str: &str) -> bool {
        let Ok(target) = NaiveDate::parse_from_str(target_date_str, "%Y-%m-%d") else {
            return false;
        };
        match Series::of(self) {
            Some(series) => series.occurs_on(target, target_date_str),
            None => false,
        }
    }

}

/// An event's timing, parsed.
///
/// Parsing is separated from asking because a year view asks the same event
/// 365 times. Doing the parse inside the question meant millions of string
/// parses for a large vault, which is most of what made the first version of
/// this miss its budget by 3x.
struct Series<'a> {
    start: NaiveDate,
    duration_days: i64,
    rule: Option<RRule>,
    exceptions: &'a [String],
}

impl<'a> Series<'a> {
    fn of(ev: &'a EventSummary) -> Option<Self> {
        let start = NaiveDate::parse_from_str(date_part(&ev.start_at), "%Y-%m-%d").ok()?;
        let end_str = date_part(&ev.end_at);
        let end = if end_str.is_empty() {
            start
        } else {
            NaiveDate::parse_from_str(end_str, "%Y-%m-%d").ok()?
        };
        Some(Self {
            start,
            duration_days: (end - start).num_days().max(0),
            rule: ev.rule(),
            exceptions: &ev.exceptions,
        })
    }

    fn repeats(&self) -> bool {
        self.rule.is_some()
    }

    /// Can this series land anywhere in `[from, to]`?
    ///
    /// A cheap rejection so the day-by-day walk only runs for events that
    /// could possibly match. It cannot filter on `start` alone: a weekly
    /// stand-up that began in 2020 still lands on days in 2026, which is
    /// exactly the event a calendar most needs to find.
    fn could_touch(&self, from: NaiveDate, to: NaiveDate) -> bool {
        if self.start > to {
            return false;
        }
        match &self.rule {
            Some(rule) => rule.until.is_none_or(|until| until >= from),
            None => self.start + chrono::Duration::days(self.duration_days) >= from,
        }
    }

    /// The day the instance covering `target` began, if one does.
    ///
    /// This is the whole of the "when" question: whether an event lands on a
    /// day, and which instance it belongs to, are the same lookup. They used
    /// to be two, and a multi-day series could be drawn on a day whose
    /// instance the other half disagreed about.
    fn anchor_for(&self, target: NaiveDate) -> Option<NaiveDate> {
        let Some(rule) = &self.rule else {
            // A one-off covers a single contiguous block from its start.
            let last = self.start + chrono::Duration::days(self.duration_days);
            return (target >= self.start && target <= last).then_some(self.start);
        };

        if target < self.start {
            return None;
        }
        if let Some(until) = rule.until {
            if target > until {
                return None;
            }
        }

        let anchor = anchor_on_or_before(rule, self.start, target)?;
        if (target - anchor).num_days() > self.duration_days {
            return None;
        }
        if let Some(count) = rule.count {
            if ordinal_of(rule, self.start, anchor) > count {
                return None;
            }
        }
        Some(anchor)
    }

    /// `target_str` is passed alongside `target` only to test the exception
    /// list, which is stored as strings and is almost always empty.
    fn occurs_on(&self, target: NaiveDate, target_str: &str) -> bool {
        if self.exceptions.iter().any(|d| d == target_str) {
            return false;
        }
        self.anchor_for(target).is_some()
    }
}

/// Does a series land on `target_date_str`?
///
/// `contracts/recurrence.json` owns the answer; this is the only
/// implementation of it. Everything that needs to know when an event happens —
/// the grid, the day panel, the reminder loop — comes through here.
pub fn occurs_on_date(
    start_date_str: &str,
    end_date_str: &str,
    recurrence: &str,
    recurrence_end_at: &str,
    exceptions: &[String],
    target_date_str: &str,
) -> bool {
    let ev = EventSummary {
        id: String::new(),
        uid: String::new(),
        title: String::new(),
        is_all_day: false,
        start_at: start_date_str.to_string(),
        end_at: end_date_str.to_string(),
        location: String::new(),
        tags: Vec::new(),
        colour: String::new(),
        tzid: String::new(),
        rrule: String::new(),
        recurrence: recurrence.to_string(),
        recurrence_end_at: recurrence_end_at.to_string(),
        series_id: String::new(),
        exceptions: exceptions.to_vec(),
        reminders: Vec::new(),
        relations: Vec::new(),
        created_at: String::new(),
        subscription_id: String::new(),
    };
    ev.occurs_on(target_date_str)
}

/// Every day in `[from, to]` that each event lands on, as the reader's clock
/// sees them.
///
/// Three things happen here, in this order, and the order is the point:
///
/// 1. A series is expanded **in its own zone**. A weekly stand-up in Tokyo
///    recurs on Tokyo Mondays; expanding it against the reader's calendar
///    would drop or double an occurrence around midnight.
/// 2. Each instance is converted to `viewer_tz`.
/// 3. The days it occupies are worked out **from the converted times**. This
///    is the step that makes it more than an hours problem: an eleven o'clock
///    evening meeting in Tokyo belongs to the previous day in California, and
///    a grid that bucketed it by its Tokyo date would draw it on a day the
///    reader never sees it happen.
///
/// An event with no zone is floating and skips all of it, which is every
/// event written before zones existed.
pub fn expand_range(
    events: Vec<EventSummary>,
    from: &str,
    to: &str,
    viewer_tz: &str,
) -> EventsInRange {
    let mut out = EventsInRange::default();

    let (Some(from_d), Some(to_d)) = (
        NaiveDate::parse_from_str(from, "%Y-%m-%d").ok(),
        NaiveDate::parse_from_str(to, "%Y-%m-%d").ok(),
    ) else {
        return out;
    };
    if to_d < from_d {
        return out;
    }
    let viewer_known = is_known_zone(viewer_tz);

    // Zones span twenty-six hours end to end, so an instance up to two days
    // outside the range in its own zone can still land inside it here.
    const PAD_DAYS: i64 = 2;
    let scan_from = from_d - chrono::Duration::days(PAD_DAYS);
    let scan_to = to_d + chrono::Duration::days(PAD_DAYS);

    let mut calendar: Vec<(NaiveDate, String)> = Vec::new();
    let mut day = scan_from;
    loop {
        calendar.push((day, day.format("%Y-%m-%d").to_string()));
        if day == scan_to {
            break;
        }
        match day.succ_opt() {
            Some(next) => day = next,
            None => break,
        }
    }
    let mut anchors: Vec<NaiveDate> = Vec::new();
    for ev in events {
        let Some(series) = Series::of(&ev) else { continue };
        if !series.could_touch(scan_from, scan_to) {
            continue;
        }

        // One anchor per instance, however many days that instance covers.
        anchors.clear();
        if series.repeats() {
            for (date, _) in &calendar {
                if let Some(anchor) = series.anchor_for(*date) {
                    if anchors.last() != Some(&anchor) {
                        anchors.push(anchor);
                    }
                }
            }
        } else if series.start <= scan_to
            && series.start + chrono::Duration::days(series.duration_days) >= scan_from
        {
            anchors.push(series.start);
        }
        if anchors.is_empty() {
            continue;
        }

        let duration_days = series.duration_days;
        let converts = !ev.is_all_day
            && viewer_known
            && is_known_zone(&ev.tzid)
            && ev.tzid.trim() != viewer_tz.trim();
        drop(series);

        let start_time = time_part(&ev.start_at).to_string();
        let end_time = {
            let t = time_part(&ev.end_at);
            // An event saved without an end reads as zero length; the view
            // gives it a floor rather than the data inventing one.
            if t.is_empty() { start_time.clone() } else { t.to_string() }
        };
        let duration = chrono::Duration::days(duration_days.max(0));

        let mut refs: Vec<OccurrenceRef> = Vec::new();
        for anchor in anchors.drain(..) {
            let mut start_at = stamp(anchor, &start_time);
            let mut end_at = stamp(anchor + duration, &end_time);

            // Which days the instance occupies is read off the times the
            // reader will actually see, not the ones it was stored with — so
            // the dates are only re-read when a conversion moved them. Left
            // alone, they are the anchor, which is already a date.
            let (mut first, mut last) = (anchor, anchor + duration);

            if converts {
                let (Some(moved_start), Some(moved_end)) = (
                    convert_stamp(&start_at, &ev.tzid, viewer_tz),
                    convert_stamp(&end_at, &ev.tzid, viewer_tz),
                ) else {
                    continue;
                };
                let Some(moved_first) =
                    NaiveDate::parse_from_str(date_part(&moved_start), "%Y-%m-%d").ok()
                else {
                    continue;
                };
                last = NaiveDate::parse_from_str(date_part(&moved_end), "%Y-%m-%d")
                    .ok()
                    .filter(|d| *d >= moved_first)
                    .unwrap_or(moved_first);
                first = moved_first;
                start_at = moved_start;
                end_at = moved_end;
            }

            // Indexed rather than searched: a daily series has as many
            // instances as the range has days, and scanning the calendar for
            // each of them turns a year view into three hundred and sixty
            // five scans of a year.

            let lo = (first.max(from_d) - scan_from).num_days();
            let hi = (last.min(to_d) - scan_from).num_days();
            if hi < 0 || lo > hi {
                continue;
            }
            for (_, date_str) in calendar
                .iter()
                .take(hi as usize + 1)
                .skip(lo.max(0) as usize)
            {
                if ev.exceptions.iter().any(|d| d == date_str) {
                    continue;
                }
                refs.push(OccurrenceRef {
                    date: date_str.clone(),
                    event: 0, // filled in once we know the event is kept
                    start_at: start_at.clone(),
                    end_at: end_at.clone(),
                });
            }
        }

        if refs.is_empty() {
            continue;
        }
        let index = out.events.len();
        out.events.push(ev);
        for mut r in refs {
            r.event = index;
            out.occurrences.push(r);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Deserialize)]
    struct CaseEvent {
        start_at: String,
        end_at: String,
        recurrence: String,
        #[serde(default)]
        recurrence_end_at: String,
        #[serde(default)]
        rrule: String,
        #[serde(default)]
        exceptions: Vec<String>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        event: CaseEvent,
        occurs: Vec<String>,
        absent: Vec<String>,
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    /// Embedded at compile time, so moving or deleting the contract breaks the
    /// build rather than quietly leaving the rule untested.
    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../contracts/recurrence.json"
    ));

    fn summary(e: &CaseEvent) -> EventSummary {
        EventSummary::from_properties(
            "Events/case.md",
            "Case",
            "",
            &json!({
                "start_at": e.start_at,
                "end_at": e.end_at,
                "recurrence": e.recurrence,
                "recurrence_end_at": e.recurrence_end_at,
                "rrule": e.rrule,
                "exceptions": e.exceptions,
            }),
        )
    }

    #[test]
    fn matches_the_shared_recurrence_contract() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("contract parses");
        assert!(
            fixture.cases.len() >= 28,
            "contract looks truncated: {} cases",
            fixture.cases.len()
        );

        let mut failures = Vec::new();
        for case in &fixture.cases {
            let ev = summary(&case.event);
            for day in &case.occurs {
                if !ev.occurs_on(day) {
                    failures.push(format!("  {}\n    missing occurrence on {}", case.name, day));
                }
            }
            for day in &case.absent {
                if ev.occurs_on(day) {
                    failures.push(format!(
                        "  {}\n    unexpected occurrence on {}",
                        case.name, day
                    ));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} disagreement(s) with the contract:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// `expand_range` has to agree with the predicate it is built on, over
    /// every case, day by day. This is what lets the front end delete its own
    /// copy of the rule and simply render what it is handed.
    #[test]
    fn expansion_agrees_with_the_predicate_over_the_contract() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("contract parses");
        for case in &fixture.cases {
            let ev = summary(&case.event);
            let mut wanted: Vec<&String> = case.occurs.iter().collect();
            wanted.sort();
            if wanted.is_empty() {
                continue;
            }
            let from = wanted.first().unwrap().as_str();
            let to = wanted.last().unwrap().as_str();

            let got = expand_range(vec![ev], from, to, "");
            for day in &case.occurs {
                assert!(
                    got.occurrences.iter().any(|o| &o.date == day),
                    "{}: expansion dropped {}",
                    case.name,
                    day
                );
            }
            for day in &case.absent {
                if day.as_str() >= from && day.as_str() <= to {
                    assert!(
                        !got.occurrences.iter().any(|o| &o.date == day),
                        "{}: expansion invented {}",
                        case.name,
                        day
                    );
                }
            }
        }
    }

    /// The fallback that used to live in `useCalendarData.ts`. Without it every
    /// event written before `start_at` existed disappears from the calendar.
    #[test]
    fn an_older_vault_still_has_its_events() {
        let timed = EventSummary::from_properties(
            "Events/old.md",
            "Standup",
            "",
            &json!({ "event_date": "2026-03-02", "start_time": "09:00", "end_time": "09:30" }),
        );
        assert_eq!(timed.start_at, "2026-03-02T09:00:00");
        assert_eq!(timed.end_at, "2026-03-02T09:30:00");
        assert!(!timed.is_all_day);
        assert!(timed.occurs_on("2026-03-02"));

        let all_day = EventSummary::from_properties(
            "Events/older.md",
            "Holiday",
            "",
            &json!({ "event_date": "2026-04-30" }),
        );
        assert!(all_day.is_all_day);
        assert_eq!(all_day.end_at, "2026-04-30");

        // `event_time` was the field name before `start_time` was.
        let oldest = EventSummary::from_properties(
            "Events/oldest.md",
            "Call",
            "",
            &json!({ "event_date": "2026-04-30", "event_time": "14:00" }),
        );
        assert_eq!(oldest.start_at, "2026-04-30T14:00:00");
    }

    #[test]
    fn a_single_tag_written_as_a_string_is_still_a_tag() {
        let ev = EventSummary::from_properties(
            "Events/a.md",
            "A",
            "",
            &json!({ "start_at": "2026-03-02", "tags": "meeting" }),
        );
        assert_eq!(ev.tags, vec!["meeting".to_string()]);
    }

    #[test]
    fn related_notes_is_read_when_relations_is_absent() {
        let ev = EventSummary::from_properties(
            "Events/a.md",
            "A",
            "",
            &json!({ "start_at": "2026-03-02", "related_notes": ["[N](synabit://note/N.md)"] }),
        );
        assert_eq!(ev.relations.len(), 1);
    }

    #[test]
    fn an_event_outside_the_range_is_not_sent_at_all() {
        let ev = EventSummary::from_properties(
            "Events/a.md",
            "A",
            "",
            &json!({ "start_at": "2020-01-01", "end_at": "2020-01-01" }),
        );
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "");
        assert!(got.events.is_empty());
        assert!(got.occurrences.is_empty());
    }

    /// A weekly series that began years ago still has to be considered, which
    /// is why the range filter cannot simply be `start_at >= from`.
    #[test]
    fn a_long_running_series_is_still_found() {
        let ev = EventSummary::from_properties(
            "Events/a.md",
            "Standup",
            "",
            &json!({ "start_at": "2020-03-02T09:00", "recurrence": "weekly" }),
        );
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "");
        assert_eq!(got.events.len(), 1);
        assert_eq!(got.occurrences.len(), 5); // Mondays in March 2026
    }

    #[test]
    fn a_daily_series_costs_one_summary_and_many_refs() {
        let ev = EventSummary::from_properties(
            "Events/a.md",
            "Habit",
            "",
            &json!({ "start_at": "2026-01-01", "recurrence": "daily" }),
        );
        let got = expand_range(vec![ev], "2026-01-01", "2026-12-31", "");
        assert_eq!(got.events.len(), 1);
        assert_eq!(got.occurrences.len(), 365);
    }

    #[test]
    fn a_backwards_range_returns_nothing_rather_than_looping() {
        let ev = EventSummary::from_properties(
            "Events/a.md",
            "A",
            "",
            &json!({ "start_at": "2026-03-02", "recurrence": "daily" }),
        );
        let got = expand_range(vec![ev], "2026-03-31", "2026-03-01", "");
        assert!(got.occurrences.is_empty());
    }

    fn on(range: &EventsInRange, date: &str) -> Vec<(String, String)> {
        range
            .occurrences
            .iter()
            .filter(|o| o.date == date)
            .map(|o| (o.start_at.clone(), o.end_at.clone()))
            .collect()
    }

    /// The tenth Monday of a stand-up has to say it is the tenth Monday. The
    /// stored event only knows the first one, and a time axis drawing the
    /// stored value would stack every occurrence of a series on one day.
    #[test]
    fn each_occurrence_carries_its_own_instance_times() {
        let ev = EventSummary::from_properties(
            "Events/standup.md",
            "Standup",
            "",
            &json!({
                "start_at": "2026-03-02T09:00",
                "end_at": "2026-03-02T09:15",
                "recurrence": "weekly",
            }),
        );
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "");
        assert_eq!(
            on(&got, "2026-03-02"),
            vec![("2026-03-02T09:00".into(), "2026-03-02T09:15".into())]
        );
        assert_eq!(
            on(&got, "2026-03-23"),
            vec![("2026-03-23T09:00".into(), "2026-03-23T09:15".into())]
        );
    }

    /// Every day of a multi-day instance repeats that instance's bounds, so a
    /// view can tell "starts today" from "still running since Tuesday".
    #[test]
    fn a_multi_day_instance_repeats_its_bounds_on_every_day_it_covers() {
        let ev = EventSummary::from_properties(
            "Events/trip.md",
            "Trip",
            "",
            &json!({ "start_at": "2026-03-10T18:00", "end_at": "2026-03-13T11:00" }),
        );
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "");
        let expected = vec![("2026-03-10T18:00".to_string(), "2026-03-13T11:00".to_string())];
        for day in ["2026-03-10", "2026-03-11", "2026-03-12", "2026-03-13"] {
            assert_eq!(on(&got, day), expected, "on {}", day);
        }
        assert!(on(&got, "2026-03-14").is_empty());
    }

    /// A monthly series starting on the 31st is anchored to the day it was
    /// clamped to, not to a 31st the month does not have.
    #[test]
    fn a_clamped_monthly_instance_is_anchored_to_the_day_it_landed_on() {
        let ev = EventSummary::from_properties(
            "Events/rent.md",
            "Rent",
            "",
            &json!({ "start_at": "2026-01-31T08:00", "recurrence": "monthly" }),
        );
        let got = expand_range(vec![ev], "2026-01-01", "2026-04-30", "");
        assert_eq!(
            on(&got, "2026-02-28"),
            vec![("2026-02-28T08:00".into(), "2026-02-28T08:00".into())]
        );
        assert_eq!(
            on(&got, "2026-04-30"),
            vec![("2026-04-30T08:00".into(), "2026-04-30T08:00".into())]
        );
    }

    #[test]
    fn a_leap_day_birthday_is_anchored_to_the_28th_in_a_common_year() {
        let ev = EventSummary::from_properties(
            "Events/bday.md",
            "Birthday",
            "",
            &json!({ "start_at": "2028-02-29", "recurrence": "yearly" }),
        );
        let got = expand_range(vec![ev], "2029-01-01", "2029-12-31", "");
        assert_eq!(
            on(&got, "2029-02-28"),
            vec![("2029-02-28".into(), "2029-02-28".into())]
        );
    }

    /// An all-day instance stays a bare date on both ends — attaching a clock
    /// to it would put it on the time axis.
    #[test]
    fn an_all_day_instance_keeps_bare_dates() {
        let ev = EventSummary::from_properties(
            "Events/holiday.md",
            "Holiday",
            "",
            &json!({ "start_at": "2026-04-30", "is_all_day": true }),
        );
        let got = expand_range(vec![ev], "2026-04-01", "2026-04-30", "");
        assert_eq!(on(&got, "2026-04-30"), vec![("2026-04-30".into(), "2026-04-30".into())]);
    }

    fn zoned(id: &str, start: &str, end: &str, tzid: &str, rrule: &str) -> EventSummary {
        EventSummary::from_properties(
            id,
            "Zoned",
            "",
            &json!({ "start_at": start, "end_at": end, "tzid": tzid, "rrule": rrule }),
        )
    }

    /// The whole reason this is more than an hours-and-minutes problem: a
    /// late meeting in Tokyo belongs to the previous day in California, and a
    /// grid that filed it under its Tokyo date would draw it on a day the
    /// reader never sees it happen.
    #[test]
    fn an_evening_in_tokyo_is_filed_under_the_morning_it_lands_on_elsewhere() {
        let ev = zoned("Events/call.md", "2026-03-10T23:00", "2026-03-10T23:30", "Asia/Tokyo", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "America/Los_Angeles");

        assert_eq!(got.occurrences.len(), 1);
        let occ = &got.occurrences[0];
        assert_eq!(occ.date, "2026-03-10");
        assert_eq!(occ.start_at, "2026-03-10T07:00");
        assert_eq!(occ.end_at, "2026-03-10T07:30");
    }

    #[test]
    fn a_late_evening_in_california_is_filed_under_the_next_day_in_tokyo() {
        let ev = zoned("Events/a.md", "2026-03-10T22:00", "2026-03-10T23:00", "America/Los_Angeles", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "Asia/Tokyo");
        assert_eq!(got.occurrences[0].date, "2026-03-11");
        assert_eq!(got.occurrences[0].start_at, "2026-03-11T14:00");
    }

    /// A series recurs on *its own* zone's days. Expanding it against the
    /// reader's calendar instead would drop or double an occurrence wherever
    /// the conversion crosses midnight — here, every single one of them.
    #[test]
    fn a_weekly_series_abroad_keeps_its_own_cadence() {
        let ev = zoned(
            "Events/standup.md",
            "2026-03-02T09:00",
            "2026-03-02T09:15",
            "Asia/Tokyo",
            "FREQ=WEEKLY",
        );
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "America/New_York");

        // Tokyo Mondays are New York Sundays, and there are five of them.
        let days: Vec<&str> = got.occurrences.iter().map(|o| o.date.as_str()).collect();
        assert_eq!(days, ["2026-03-01", "2026-03-08", "2026-03-15", "2026-03-22", "2026-03-29"]);

        // And the reason the stored time is a wall clock rather than an
        // instant: Tokyo never changes its clocks, New York moved on 8 March,
        // so the same nine o'clock stand-up is read an hour later after it.
        // Both are correct; an event stored as an instant could only be one.
        let times: Vec<&str> = got.occurrences.iter().map(|o| o.start_at.as_str()).collect();
        assert_eq!(times, [
            "2026-03-01T19:00", // still EST
            "2026-03-08T20:00", // EDT from here on
            "2026-03-15T20:00",
            "2026-03-22T20:00",
            "2026-03-29T20:00",
        ]);
    }

    /// What the two days of padding are for. Without it the first and last
    /// day of every month would be missing whatever crossed into them.
    #[test]
    fn an_instance_just_outside_the_range_is_found_when_it_lands_inside_it() {
        // 1 April 07:00 in Tokyo is 31 March 18:00 in New York.
        let ev = zoned("Events/a.md", "2026-04-01T07:00", "2026-04-01T08:00", "Asia/Tokyo", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "America/New_York");
        assert_eq!(got.occurrences.len(), 1, "the event crossed into March");
        assert_eq!(got.occurrences[0].date, "2026-03-31");
        assert_eq!(got.occurrences[0].start_at, "2026-03-31T18:00");
    }

    /// And the other edge: something inside the range in its own zone that
    /// lands outside it here must not be sent.
    #[test]
    fn an_instance_that_leaves_the_range_is_not_sent() {
        // 1 March 07:00 in Tokyo is 28 February in New York.
        let ev = zoned("Events/a.md", "2026-03-01T07:00", "2026-03-01T08:00", "Asia/Tokyo", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "America/New_York");
        assert!(got.occurrences.is_empty());
    }

    /// An all-day event is the same day everywhere. Converting it is how a
    /// public holiday ends up on the wrong date for half the world.
    #[test]
    fn an_all_day_event_is_never_moved_by_a_zone() {
        let ev = EventSummary::from_properties(
            "Events/holiday.md",
            "Holiday",
            "",
            &json!({ "start_at": "2026-04-30", "is_all_day": true, "tzid": "Asia/Tokyo" }),
        );
        let got = expand_range(vec![ev], "2026-04-01", "2026-04-30", "America/Los_Angeles");
        assert_eq!(got.occurrences[0].date, "2026-04-30");
        assert_eq!(got.occurrences[0].start_at, "2026-04-30");
    }

    #[test]
    fn an_event_with_no_zone_stays_where_it_was_written() {
        let ev = zoned("Events/a.md", "2026-03-10T09:00", "2026-03-10T10:00", "", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "America/New_York");
        assert_eq!(got.occurrences[0].start_at, "2026-03-10T09:00");
    }

    #[test]
    fn a_reader_with_no_zone_is_shown_the_stored_time() {
        let ev = zoned("Events/a.md", "2026-03-10T09:00", "2026-03-10T10:00", "Asia/Tokyo", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "");
        assert_eq!(got.occurrences[0].start_at, "2026-03-10T09:00");
    }

    #[test]
    fn a_zone_the_reader_is_already_in_is_not_converted() {
        let ev = zoned("Events/a.md", "2026-03-10T09:00", "2026-03-10T10:00", "Asia/Tokyo", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "Asia/Tokyo");
        assert_eq!(got.occurrences[0].start_at, "2026-03-10T09:00");
    }

    /// A multi-day instance keeps its length across the conversion, and
    /// occupies the days the reader sees it on.
    #[test]
    fn a_multi_day_instance_is_rebucketed_onto_the_days_it_lands_on() {
        let ev = zoned("Events/trip.md", "2026-03-10T09:00", "2026-03-12T09:00", "Asia/Tokyo", "");
        let got = expand_range(vec![ev], "2026-03-01", "2026-03-31", "America/New_York");
        let days: Vec<&str> = got.occurrences.iter().map(|o| o.date.as_str()).collect();
        assert_eq!(days, ["2026-03-09", "2026-03-10", "2026-03-11"]);
        for occ in &got.occurrences {
            assert_eq!(occ.start_at, "2026-03-09T20:00");
            assert_eq!(occ.end_at, "2026-03-11T20:00");
        }
    }

    /// The gate for this phase: a year view over a large vault. The old front
    /// end scanned every event once per day — 365 x 5000 filter passes in
    /// JavaScript, recomputed whenever anything changed.
    ///
    /// The budget is only asserted for an optimised build, and deliberately.
    /// An unoptimised build runs this about five times slower, which leaves so
    /// little headroom that a busy machine fails the test for no reason —
    /// which is exactly what it did once before this note was written. Debug
    /// runs still do the work and still print the number, so a regression that
    /// changes the shape of the algorithm shows up either way; run
    /// `cargo test --release` to hold it to the budget.
    #[test]
    fn a_year_of_five_thousand_events_expands_well_inside_the_budget() {
        let mut events = Vec::new();
        for i in 0..5000 {
            let day = 1 + (i % 28);
            let month = 1 + (i % 12);
            let recurrence = match i % 5 {
                0 => "none",
                1 => "weekly",
                2 => "monthly",
                3 => "yearly",
                _ => "none",
            };
            events.push(EventSummary::from_properties(
                &format!("Events/{}.md", i),
                "Event",
                "",
                &json!({
                    "start_at": format!("2026-{:02}-{:02}T09:00", month, day),
                    "end_at": format!("2026-{:02}-{:02}T10:00", month, day),
                    "recurrence": recurrence,
                }),
            ));
        }

        let started = std::time::Instant::now();
        let got = expand_range(events, "2026-01-01", "2026-12-31", "");
        let elapsed = started.elapsed();

        println!(
            "expanded {} occurrences from {} events in {:?}",
            got.occurrences.len(),
            got.events.len(),
            elapsed
        );
        assert_eq!(got.events.len(), 5000);
        assert!(got.occurrences.len() > 30_000);

        if !cfg!(debug_assertions) {
            assert!(
                elapsed.as_millis() < 100,
                "expanding a year of 5000 events took {:?}, over the 100ms gate",
                elapsed
            );
        }
    
    }
}
