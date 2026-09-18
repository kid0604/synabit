//! What the vault already says about when things happened.
//!
//! Every item here comes from a field that holds a date, read as written.
//! There is no model and no guessing: a node whose dates are missing or
//! unreadable gives no items, which is the truth about what it says. The list
//! of sources, and of what is deliberately left out, is §4.8.1 of
//! `docs/timeline-2026-09-17.md`.
//!
//! This is a pure function of one node, plus the vault's schemas. The one
//! source that needs two nodes, a picture dated by the note it sits in, is
//! placed by the store after every node has been read.

use std::collections::HashMap;

use chrono::NaiveDate;
use serde_json::{Map, Value};

use super::when::{self, Precision, Span};
use crate::calendar::recurrence::{date_part, EventSummary};

/// The parts of a node derivation reads.
pub struct NodeView<'a> {
    pub id: &'a str,
    pub node_type: &'a str,
    pub title: &'a str,
    pub properties: &'a Value,
}

/// A node an event names, and how it took part.
///
/// Four roles, from `docs/timeline-2026-09-17.md` §4.2: `with` took part,
/// `where` is the place, `about` is what it concerns, `evidence` is what shows
/// it happened. A photograph does not attend a wedding.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub node: String,
    pub role: &'static str,
    /// Nhãn của vai: "cô dâu chú rể", "hệ thống".
    pub label: Option<String>,
}

impl Link {
    pub fn with(node: impl Into<String>) -> Self {
        Link { node: node.into(), role: "with", label: None }
    }

    /// Where it happened. Either a `Places/` node or the words the person
    /// wrote: §4.2 allows both, and nothing here can tell which apart without
    /// the vault. Resolving the words to a node is the editor's job, later.
    pub fn at(node: impl Into<String>) -> Self {
        Link { node: node.into(), role: "where", label: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    pub kind: &'static str,
    pub span: Span,
    /// Where the date came from: `frontmatter`, `filename`, or `note`.
    pub time_source: &'static str,
    /// Tên của chính sự kiện. Node sinh ra nó có tên riêng (§4.3).
    pub title: Option<String>,
    /// Everything this event names. A meeting has as many as were there.
    pub links: Vec<Link>,
    /// What the person wrote that this code has no meaning for, kept as
    /// written. Rule 1 of §4.3: not understood is not the same as not wanted.
    pub props: Value,
    /// Whether a note wrote this event out rather than being it. A daily note
    /// is the box several events came in.
    pub container: bool,
}

fn item(kind: &'static str, span: Span) -> Derived {
    Derived {
        kind,
        span,
        time_source: "frontmatter",
        title: None,
        links: Vec::new(),
        props: Value::Null,
        container: false,
    }
}

/// The items one node implies.
///
/// `date_fields` maps a user-defined type to its keys of kind `date`, read
/// from the vault's `type: schema` nodes by [`date_fields_from_schema`].
pub fn derive(node: &NodeView, date_fields: &HashMap<String, Vec<String>>) -> Vec<Derived> {
    let mut items = by_type(node, date_fields);
    items.extend(moments(node.properties));
    items
}

/// Moments the person accepted into this node, from the tray (`timeline::extract`).
///
/// Tier 2: written by a person's decision, so read from any type of node.
fn moments(p: &Value) -> Vec<Derived> {
    let Some(Value::Array(list)) = p.get("moments") else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|moment| {
            let span = dated(moment, "happened")?;
            Some(Derived {
                time_source: "user",
                title: text(moment, "title").map(String::from),
                links: cast(moment),
                props: rest_of(moment),
                // The note holding this frontmatter is the box, not the event.
                container: true,
                ..item("moment", span)
            })
        })
        .collect()
}

/// What the app understands about a moment. Everything else is kept in
/// [`Derived::props`] rather than dropped.
const KNOWN_KEYS: &[&str] = &["id", "title", "happened", "people", "where"];

/// Everyone and everywhere the moment names.
///
/// Keeping only the first person is how a wedding became a meeting with one
/// person; see §3 of `docs/timeline-2026-09-17.md`.
fn cast(moment: &Value) -> Vec<Link> {
    let names = |key: &str| -> Vec<String> {
        match moment.get(key) {
            Some(Value::String(one)) => vec![one.to_string()],
            Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).map(String::from).collect(),
            _ => Vec::new(),
        }
    };
    let clean = |list: Vec<String>| {
        list.into_iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>()
    };
    let mut links: Vec<Link> = clean(names("people")).into_iter().map(Link::with).collect();
    links.extend(clean(names("where")).into_iter().map(Link::at));
    links
}

/// The keys this version has no meaning for, as they were written.
fn rest_of(moment: &Value) -> Value {
    let Some(fields) = moment.as_object() else {
        return Value::Null;
    };
    let rest: Map<String, Value> = fields
        .iter()
        .filter(|(key, _)| !KNOWN_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if rest.is_empty() {
        Value::Null
    } else {
        Value::Object(rest)
    }
}

/// A decision on the day it was made, and each day it was looked back on.
/// See `timeline::reflect`.
fn decision(p: &Value) -> Vec<Derived> {
    let mut out: Vec<Derived> = dated(p, "decided_on")
        .map(|span| Derived {
            title: text(p, "expected").map(String::from),
            ..item("decision", span)
        })
        .into_iter()
        .collect();
    if let Some(Value::Array(reviews)) = p.get("reviews") {
        out.extend(reviews.iter().filter_map(|review| {
            dated(review, "on").map(|span| Derived {
                title: text(review, "happened").map(|h| h.chars().take(120).collect()),
                ..item("decision_review", span)
            })
        }));
    }
    out
}

fn by_type(node: &NodeView, date_fields: &HashMap<String, Vec<String>>) -> Vec<Derived> {
    let p = node.properties;
    match node.node_type {
        // Daily notes, and any note that says which day it is about.
        "note" => dated(p, "date").map(|span| item("note", span)).into_iter().collect(),
        "interaction" => dated(p, "date")
            .map(|span| Derived {
                links: text(p, "person_id").map(Link::with).into_iter().collect(),
                title: text(p, "interaction_type").map(String::from),
                ..item("interaction", span)
            })
            .into_iter()
            .collect(),
        "person" => person(p),
        "event" => event(node).into_iter().collect(),
        // Done is something that happened. A due date is a plan.
        "task" => dated(p, "completed_at")
            .map(|span| item("task_done", span))
            .into_iter()
            .collect(),
        "decision" => decision(p),
        "project" => dated(p, "start_date")
            .map(|span| item("project_start", span))
            .into_iter()
            .collect(),
        "file" => media_by_filename(node).into_iter().collect(),
        t if never_dated(t) => Vec::new(),
        t => date_fields
            .get(t)
            .into_iter()
            .flatten()
            .filter_map(|key| {
                dated(p, key).map(|span| Derived {
                    title: Some(key.clone()),
                    ..item("field", span)
                })
            })
            .collect(),
    }
}

/// Types the app owns whose dates are not events in a life: storage, views,
/// the assistant's own records, money (which reaches the timeline only as
/// density, never as items; §4.8.1).
fn never_dated(node_type: &str) -> bool {
    node_type.starts_with("finance_")
        || node_type.starts_with("syn_")
        || node_type.starts_with("pdf_")
        || matches!(
            node_type,
            "quickcap" | "whiteboard" | "filter" | "view" | "schema" | "canvas" | "json" | "place"
        )
}

/// A user-defined type and its date keys, from a `type: schema` node.
///
/// The schema's title is the type it describes; `fields` is a list of
/// `{key, kind}`, and a plain string entry is a text field.
pub fn date_fields_from_schema(title: &str, properties: &Value) -> Option<(String, Vec<String>)> {
    let fields = properties.get("fields")?.as_array()?;
    let keys: Vec<String> = fields
        .iter()
        .filter(|field| field.get("kind").and_then(Value::as_str) == Some("date"))
        .filter_map(|field| text(field, "key").map(String::from))
        .collect();
    let node_type = title.trim();
    (!keys.is_empty() && !node_type.is_empty()).then(|| (node_type.to_string(), keys))
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn dated(value: &Value, key: &str) -> Option<Span> {
    text(value, key).and_then(when::parse)
}

fn person(p: &Value) -> Vec<Derived> {
    let mut out = Vec::new();

    if let Some(Value::Array(jobs)) = p.get("experiences") {
        for job in jobs {
            let Some(start) = dated(job, "start") else {
                continue;
            };
            let ongoing = job.get("current").and_then(Value::as_bool) == Some(true);
            let span = if ongoing {
                Span {
                    from: start.from,
                    to: when::open_end(),
                    precision: Precision::Range,
                }
            } else {
                match dated(job, "end") {
                    Some(end) if end.to >= start.from => Span {
                        from: start.from,
                        to: end.to,
                        precision: Precision::Range,
                    },
                    // An end nobody wrote down is unknown, which is not the
                    // same as still going. The start is all that is known.
                    _ => start,
                }
            };
            let label = [text(job, "role"), text(job, "company")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            out.push(Derived {
                title: (!label.is_empty()).then_some(label),
                ..item("experience", span)
            });
        }
    }

    if let Some(span) = dated(p, "birthday") {
        out.push(item("birthday", span));
    }

    // A relationship with someone, from the day it began. Unlike a job whose
    // end was never written down, a connection without `until` is still
    // going: the People app asks for an end only when there is one.
    if let Some(Value::Array(links)) = p.get("connections") {
        for link in links {
            let (Some(since), Some(other)) = (dated(link, "since"), text(link, "person_id")) else {
                continue;
            };
            let to = match dated(link, "until") {
                Some(until) if until.to >= since.from => until.to,
                // An end before the start is a typo, not a relationship.
                Some(_) => continue,
                None => when::open_end(),
            };
            out.push(Derived {
                links: vec![Link::with(other)],
                title: text(link, "relation_type").map(String::from),
                ..item(
                    "connection",
                    Span {
                        from: since.from,
                        to,
                        precision: Precision::Range,
                    },
                )
            });
        }
    }

    if let Some(span) = dated(p, "died_on") {
        out.push(item("death", span));
    }

    if let Some(Value::Array(dates)) = p.get("important_dates") {
        for entry in dates {
            if let Some(span) = dated(entry, "date") {
                out.push(Derived {
                    title: text(entry, "label").map(String::from),
                    ..item("important_date", span)
                });
            }
        }
    }

    out
}

fn event(node: &NodeView) -> Option<Derived> {
    let event = EventSummary::from_properties(node.id, node.title, "", node.properties);

    // A weekly stand-up is not five hundred things that happened. The
    // occurrences worth keeping are the ones something was written about, and
    // finding those needs the notes linked to them; until then a series
    // contributes nothing rather than one item per week.
    if event.rule().is_some() {
        return None;
    }

    let start = when::parse(date_part(&event.start_at))?;
    let span = match when::parse(date_part(&event.end_at)) {
        Some(end) if end.to > start.from => Span {
            from: start.from,
            to: end.to,
            precision: Precision::Range,
        },
        _ => start,
    };

    Some(Derived {
        title: (!event.location.trim().is_empty()).then(|| event.location.clone()),
        ..item("event", span)
    })
}

const MEDIA_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "m4a", "mp3", "ogg", "oga", "opus",
    "wav", "aac", "mp4", "mov", "m4v", "webm", "mkv",
];

/// Whether a file name is a picture, a recording or a video.
pub fn is_media(name: &str) -> bool {
    name.rsplit_once('.')
        .map(|(_, ext)| MEDIA_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// The file name a `file` node stands for.
pub fn media_name<'a>(node: &NodeView<'a>) -> &'a str {
    text(node.properties, "path")
        .map(|path| path.rsplit(['/', '\\']).next().unwrap_or(path))
        .unwrap_or(node.title)
}

fn media_by_filename(node: &NodeView) -> Option<Derived> {
    let name = media_name(node);
    if !is_media(name) {
        return None;
    }
    // The camera's own date first (§4.8.5), where this device has read it;
    // then a date in the name.
    if let Some(day) = text(node.properties, "shot_at")
        .and_then(|shot| shot.get(..10))
        .and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
    {
        return Some(Derived {
            time_source: "exif",
            ..item("media", Span::day(day))
        });
    }
    let date = date_in_filename(name)?;
    Some(Derived {
        time_source: "filename",
        ..item("media", Span::day(date))
    })
}

/// The day a camera or phone wrote into a file's name.
///
/// `IMG_20240912_101010.jpg`, `PXL_20240912…`, `photo_2024-09-12_22-10-33.jpg`,
/// `Screenshot 2024-09-12 at 10.10.10.png`. What comes before the date must be
/// nothing, or a word of letters: that is what keeps a uuid such as
/// `3f9a1b2c-2024-0912-…` from reading as a day, and the ten-digit upload stamp
/// the editor puts on pasted images (`1777860790-image.png`) is not a date of
/// the picture at all.
pub fn date_in_filename(name: &str) -> Option<NaiveDate> {
    let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
    let bytes = stem.as_bytes();

    for at in 0..bytes.len() {
        let Some((date, end)) = date_at(bytes, at) else {
            continue;
        };
        if bytes.get(end).is_some_and(u8::is_ascii_digit) {
            continue;
        }
        // `at` is an ASCII digit, so it is a character boundary.
        if prefix_is_a_label(&stem[..at]) {
            return Some(date);
        }
    }
    None
}

fn date_at(bytes: &[u8], at: usize) -> Option<(NaiveDate, usize)> {
    let number = |from: usize, len: usize| -> Option<u32> {
        let run = bytes.get(from..from + len)?;
        if !run.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(run).ok()?.parse().ok()
    };

    let year = number(at, 4)?;
    if !(1970..=2099).contains(&year) {
        return None;
    }
    let separator = bytes.get(at + 4).copied().filter(|b| matches!(b, b'-' | b'_' | b'.'));
    let month_at = at + 4 + usize::from(separator.is_some());
    let month = number(month_at, 2)?;
    let day_at = match separator {
        Some(sep) if bytes.get(month_at + 2) == Some(&sep) => month_at + 3,
        Some(_) => return None,
        None => month_at + 2,
    };
    let day = number(day_at, 2)?;
    let date = NaiveDate::from_ymd_opt(year as i32, month, day)?;
    Some((date, day_at + 2))
}

fn prefix_is_a_label(prefix: &str) -> bool {
    let word_and_before = prefix.strip_suffix(['_', '-', ' ']).unwrap_or(prefix);
    if word_and_before.is_empty() {
        return true;
    }
    let word_start = word_and_before
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_ascii_alphabetic())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let word = &word_and_before[word_start..];
    let before = word_and_before[..word_start].chars().last();
    !word.is_empty() && before.is_none_or(|c| !c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moment_keeps_where_it_was_and_what_this_version_cannot_read() {
        let properties = json!({
            "date": "2019-08-02",
            "moments": [{
                "title": "Đi chơi 3 ngày ở Tuần Châu",
                "happened": "2019-08-02/2019-08-04",
                "people": ["uuid-a"],
                "where": "Tuần Châu",
                "severity": "P1",
                "downtime_minutes": 148
            }]
        });
        let derived = derive(
            &NodeView { id: "Notes/2019-08-02.md", node_type: "note", title: "2019-08-02", properties: &properties },
            &HashMap::new(),
        );
        let moment = derived.iter().find(|d| d.kind == "moment").expect("a moment");
        assert_eq!(moment.links, vec![Link::with("uuid-a"), Link::at("Tuần Châu")]);
        assert_eq!(
            moment.props,
            json!({ "severity": "P1", "downtime_minutes": 148 }),
            "a key this version has no meaning for is kept, not dropped"
        );
        assert!(moment.container, "the note is the box the moment came in");
    }

    #[test]
    fn a_place_has_no_dates_of_its_own_to_put_on_the_timeline() {
        let properties = json!({ "founded": "1010-01-01", "date": "2026-01-01" });
        let derived = derive(
            &NodeView { id: "Places/ha-noi.md", node_type: "place", title: "Hà Nội", properties: &properties },
            &HashMap::new(),
        );
        assert!(derived.is_empty(), "a place is an object, not something that happened: {derived:?}");
    }

    #[test]
    fn a_moment_names_everyone_who_was_there() {
        let properties = json!({
            "date": "2016-05-14",
            "moments": [{
                "title": "Đám cưới Tuấn và Thuỳ",
                "happened": "2016-05-14",
                "people": ["uuid-tuan", "uuid-thuy", " ", "uuid-ha"]
            }]
        });
        let derived = derive(
            &NodeView { id: "Notes/2016-05-14.md", node_type: "note", title: "2016-05-14", properties: &properties },
            &HashMap::new(),
        );
        let moment = derived.iter().find(|d| d.kind == "moment").expect("a moment");
        assert_eq!(
            moment.links,
            vec![Link::with("uuid-tuan"), Link::with("uuid-thuy"), Link::with("uuid-ha")],
            "three people, not the first one"
        );
    }
    use serde_json::json;

    fn derive_one(node_type: &str, props: Value) -> Vec<Derived> {
        derive(
            &NodeView {
                id: "x.md",
                node_type,
                title: "t",
                properties: &props,
            },
            &HashMap::new(),
        )
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn a_note_with_a_date_is_on_the_timeline_and_one_without_is_not() {
        let items = derive_one("note", json!({ "date": "2016-05-14" }));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "note");
        assert_eq!(items[0].span, Span::day(day(2016, 5, 14)));

        assert!(derive_one("note", json!({ "title": "Họp" })).is_empty());
    }

    #[test]
    fn an_interaction_names_the_person_it_was_with() {
        let items = derive_one(
            "interaction",
            json!({ "date": "2016-05-03", "person_id": "People/tuan.md", "interaction_type": "coffee" }),
        );
        assert_eq!(items[0].links, vec![Link::with("People/tuan.md")]);
        assert_eq!(items[0].title.as_deref(), Some("coffee"));
    }

    #[test]
    fn a_job_held_today_is_still_going_and_an_unknown_end_is_not_invented() {
        let items = derive_one(
            "person",
            json!({ "experiences": [
                { "company": "Mây", "role": "Founder", "start": "2018-08", "end": "", "current": true },
                { "company": "Công ty đầu", "role": "Dev", "start": "2014-07", "end": "2018-07", "current": false },
                { "company": "Nơi nào đó", "start": "2012", "end": "", "current": false },
                { "company": "Không ngày", "start": "", "end": "", "current": false }
            ]}),
        );
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].span.to, when::open_end());
        assert_eq!(items[0].title.as_deref(), Some("Founder · Mây"));
        assert_eq!((items[1].span.from, items[1].span.to), (day(2014, 7, 1), day(2018, 7, 31)));
        assert_eq!(items[2].span.precision, Precision::Year);
    }

    #[test]
    fn a_birthday_and_important_dates_are_items() {
        let items = derive_one(
            "person",
            json!({ "birthday": "1932-03-02", "important_dates": [{ "label": "Ngày cưới", "date": "2016-05-14" }] }),
        );
        let kinds: Vec<_> = items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["birthday", "important_date"]);
        assert_eq!(items[1].title.as_deref(), Some("Ngày cưới"));
    }

    #[test]
    fn a_one_off_event_spans_its_days() {
        let items = derive_one(
            "event",
            json!({ "start_at": "2016-05-14T09:00:00", "end_at": "2016-05-15T11:00:00", "location": "Hà Nội" }),
        );
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].span.from, items[0].span.to), (day(2016, 5, 14), day(2016, 5, 15)));
        assert_eq!(items[0].title.as_deref(), Some("Hà Nội"));
    }

    /// The gate in the doc: an event written before `start_at` existed.
    #[test]
    fn an_event_in_the_old_shape_is_still_read() {
        let items = derive_one("event", json!({ "event_date": "2016-05-14", "event_time": "09:00" }));
        assert_eq!(items[0].span, Span::day(day(2016, 5, 14)));
    }

    /// The gate in the doc: a series is not one item per week.
    #[test]
    fn a_recurring_event_is_not_multiplied_into_the_timeline() {
        assert!(derive_one("event", json!({ "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY" })).is_empty());
        assert!(derive_one("event", json!({ "start_at": "2026-03-02", "recurrence": "weekly" })).is_empty());
    }

    #[test]
    fn a_task_counts_once_it_is_done_and_its_plans_do_not() {
        assert_eq!(derive_one("task", json!({ "completed_at": "2016-05-30", "due_date": "2016-05-01" })).len(), 1);
        assert!(derive_one("task", json!({ "completed_at": "", "due_date": "2016-05-01", "start_date": "2016-04-01" })).is_empty());
    }

    #[test]
    fn a_project_starts() {
        let items = derive_one("project", json!({ "start_date": "2018-08-01", "due_date": "2019-01-01" }));
        assert_eq!(items[0].kind, "project_start");
    }

    /// The gate in the doc: a type the user made up, with a date field,
    /// reaches the timeline without a line of code for it.
    #[test]
    fn a_users_own_type_is_dated_by_its_schema() {
        let (node_type, keys) = date_fields_from_schema(
            "animal",
            &json!({ "fields": [{ "key": "species", "kind": "text" }, { "key": "vaccinated_at", "kind": "date" }, "colour"] }),
        )
        .unwrap();
        let fields = HashMap::from([(node_type, keys)]);
        let props = json!({ "vaccinated_at": "2026-08-30", "species": "mèo" });
        let items = derive(
            &NodeView { id: "Animal/mun.md", node_type: "animal", title: "Mun", properties: &props },
            &fields,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "field");
        assert_eq!(items[0].title.as_deref(), Some("vaccinated_at"));

        let unknown = json!({ "vaccinated_at": "2026-08-30" });
        let none = derive(
            &NodeView { id: "Plant/x.md", node_type: "plant", title: "x", properties: &unknown },
            &fields,
        );
        assert!(none.is_empty(), "a type without a schema has no known date fields");
    }

    #[test]
    fn a_built_in_types_schema_does_not_turn_its_plans_into_history() {
        let fields = HashMap::from([("task".to_string(), vec!["due_date".to_string()])]);
        let props = json!({ "due_date": "2016-05-01" });
        let items = derive(
            &NodeView { id: "Tasks/a.md", node_type: "task", title: "a", properties: &props },
            &fields,
        );
        assert!(items.is_empty());
    }

    #[test]
    fn money_and_the_apps_own_records_are_not_items() {
        for t in ["finance_month", "quickcap", "syn_memory", "whiteboard", "schema"] {
            assert!(derive_one(t, json!({ "date": "2016-05-14" })).is_empty(), "{t}");
        }
    }

    #[test]
    fn a_camera_file_name_dates_the_picture() {
        for (name, expected) in [
            ("IMG_20240912_101010.jpg", day(2024, 9, 12)),
            ("PXL_20240912_101010123.mp4", day(2024, 9, 12)),
            ("photo_2024-09-12_22-10-33.jpg", day(2024, 9, 12)),
            ("Screenshot 2024-09-12 at 10.10.10.png", day(2024, 9, 12)),
            ("2024-09-12.m4a", day(2024, 9, 12)),
        ] {
            assert_eq!(date_in_filename(name), Some(expected), "{name}");
        }
    }

    #[test]
    fn a_name_that_only_contains_digits_is_not_a_date() {
        for name in [
            "1777860790-image.png",
            "3f9a1b2c-2024-0912-8a7b-1234567890ab.jpg",
            "image.png",
            "20240912101010.jpg",
            "2024-09-12_but_2024-13-40.jpg",
        ] {
            let found = date_in_filename(name);
            if name.starts_with("2024-09-12_but") {
                assert_eq!(found, Some(day(2024, 9, 12)));
            } else {
                assert_eq!(found, None, "{name}");
            }
        }
    }

    #[test]
    fn only_media_files_are_dated_by_their_name() {
        let pic = json!({ "path": "/v/assets/IMG_20240912_101010.jpg" });
        assert_eq!(derive_one("file", pic)[0].time_source, "filename");
        let doc = json!({ "path": "/v/Files/report_2024-09-12.pdf" });
        assert!(derive_one("file", doc).is_empty());
    }

    #[test]
    fn a_relationship_with_a_start_runs_until_it_ends_or_is_still_going() {
        let items = derive_one(
            "person",
            json!({ "connections": [
                { "person_id": "uuid-ha", "relation_type": "partner", "since": "2013-10" },
                { "person_id": "uuid-quang", "relation_type": "colleague", "since": "2014-07", "until": "2018-07" },
                { "person_id": "uuid-tuan", "relation_type": "friend" },
                { "person_id": "uuid-x", "since": "2020", "until": "2019" }
            ]}),
        );
        assert_eq!(items.len(), 2, "no start, or an end before the start, gives nothing");
        assert_eq!(items[0].kind, "connection");
        assert_eq!(items[0].links, vec![Link::with("uuid-ha")]);
        assert_eq!(items[0].title.as_deref(), Some("partner"));
        assert_eq!(items[0].span.to, when::open_end());
        assert_eq!((items[1].span.from, items[1].span.to), (day(2014, 7, 1), day(2018, 7, 31)));
    }

    #[test]
    fn a_death_is_an_item() {
        let items = derive_one("person", json!({ "died_on": "2017-11-22" }));
        assert_eq!(items[0].kind, "death");
        assert_eq!(items[0].span, Span::day(day(2017, 11, 22)));
    }
}
