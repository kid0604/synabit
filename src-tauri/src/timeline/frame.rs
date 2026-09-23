//! The timeline as Nexus needs it to travel back.
//!
//! Scrubbing the graph to a month asks, for every node, whether it was part of
//! the user's life yet, and for every relationship with dates, whether it held.
//! The answers are computed here once, so the graph only looks them up while
//! it is dragged. See `docs/timeline-2026-09-17.md` §10.
//!
//! # When a node arrives
//!
//! The earliest thing that places it in the user's life: an interaction with a
//! person, a daily note, a finished task, a relationship's start, a picture's
//! day. Also the day the node was made, which is an upper bound: a note written
//! today about 2016 is placed in 2016 by its `date`, and one with no date at
//! all has at least existed since it was created.
//!
//! Some dates are about a node without being about the user. Someone's
//! birthday is when they were born, not when they were met; a job held before
//! the user knew them is theirs; an anniversary recurs; a death is an ending.
//! None of those move a node's arrival.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Local, NaiveDate};
use serde::Serialize;

use super::derive::Shape;
use super::store::Event;
use super::when;
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct TimeFrame {
    /// Node path → the first day the vault places it in the user's life.
    pub first_seen: HashMap<String, String>,
    /// Person path → the day they died.
    pub died_on: HashMap<String, String>,
    /// Relationships with a start, by path. `until` is absent while it lasts.
    pub links: Vec<TimedLink>,
    /// How many things happened each month, oldest first.
    pub density: Vec<MonthCount>,
    /// The first month anything happened, or none for a vault with no dates.
    pub earliest: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TimedLink {
    pub source: String,
    pub target: String,
    pub since: String,
    pub until: Option<String>,
    /// How many events had both of them in it. Zero for a relationship that
    /// was declared rather than met: `connections[]` is still read, but as one
    /// more statement rather than the only source (§9).
    pub met: u32,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MonthCount {
    pub month: String,
    pub count: u32,
    /// The month's events added up by size, not counted one apiece: a month
    /// holding a wedding is not the same as one holding four errands, even
    /// where the tally matches. See [`super::magnitude`].
    pub weight: f64,
}

/// A node as the frame needs it.
pub struct FrameNode {
    pub id: String,
    pub stable_id: String,
    pub created_at: String,
}

/// How many times two people have to turn up at the same thing before the
/// graph calls it a relationship. Three, the same threshold reflection uses
/// before it will call anything a pattern.
const MET_ENOUGH: u32 = 3;

pub fn read_nodes(cache: &DbBridge) -> AppResult<Vec<FrameNode>> {
    let mut stmt = cache
        .conn()
        .prepare("SELECT id, COALESCE(stable_id, id), created_at FROM nodes")
        .map_err(|e| AppError::General(format!("timeline frame: {e}")))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FrameNode {
                id: r.get(0)?,
                stable_id: r.get(1)?,
                created_at: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
            })
        })
        .map_err(|e| AppError::General(format!("timeline frame: {e}")))?;
    Ok(rows.flatten().filter(|n| !super::is_timeline_path(&n.id)).collect())
}

pub fn build(items: &[Event], nodes: &[FrameNode], today: NaiveDate) -> TimeFrame {
    let today = when::iso(today);
    let open_end = when::iso(when::open_end());

    // Items point at other nodes by whatever name the vault used: a person's
    // `node_id` from an interaction, or a path from an older one.
    let mut path_of: HashMap<&str, &str> = HashMap::with_capacity(nodes.len() * 2);
    for node in nodes {
        path_of.insert(node.stable_id.as_str(), node.id.as_str());
        path_of.insert(node.id.as_str(), node.id.as_str());
    }
    let resolve = |name: &str| path_of.get(name).map(|path| path.to_string());

    let mut first_seen: HashMap<String, String> = HashMap::new();
    for node in nodes {
        if let Some(day) = local_day(&node.created_at).filter(|day| *day <= today) {
            earliest(&mut first_seen, &node.id, &day);
        }
    }

    // Two people at one event met. Counting that is how a relationship is
    // read from what happened rather than from what was declared about it.
    let mut together: HashMap<(String, String), (String, u32)> = HashMap::new();
    let mut died_on = HashMap::new();
    let mut links = Vec::new();
    let mut linked: HashSet<(String, String, String)> = HashSet::new();
    // Pairs that said so themselves. A relationship somebody wrote down, with
    // an end, must not be joined by a second edge that never ends.
    let mut declared: HashSet<(String, String)> = HashSet::new();
    let mut months: BTreeMap<String, (u32, f64)> = BTreeMap::new();

    for item in items {
        if item.happened_from > today {
            continue;
        }
        // Everyone the event says was there, under whatever name the vault
        // used for them.
        let there: Vec<String> = item
            .links
            .iter()
            .filter(|link| link.role == "with")
            .filter_map(|link| resolve(&link.node_id))
            .collect();

        if item.shape.fills_the_strip() {
            if let Some(month) = item.happened_from.get(..7) {
                let month = months.entry(month.to_string()).or_default();
                month.0 += 1;
                month.1 += item.magnitude;
            }
        }

        for (at, one) in there.iter().enumerate() {
            for other in there.iter().skip(at + 1) {
                if one == other {
                    continue;
                }
                let pair = if one <= other {
                    (one.clone(), other.clone())
                } else {
                    (other.clone(), one.clone())
                };
                let met = together
                    .entry(pair)
                    .or_insert_with(|| (item.happened_from.clone(), 0));
                if item.happened_from < met.0 {
                    met.0 = item.happened_from.clone();
                }
                met.1 += 1;
            }
        }

        if item.shape == Shape::Ending {
            died_on.insert(item.node_id.clone(), item.happened_from.clone());
        }

        // Người kia của một quan hệ, từ chính link của sự kiện.
        let other = there.first().cloned();
        if item.shape.is_an_arrival() {
            earliest(&mut first_seen, &item.node_id, &item.happened_from);
            if let Some(other) = &other {
                earliest(&mut first_seen, other, &item.happened_from);
            }
            // Everyone else who was there, too. `related_id` holds one name;
            // a wedding with three guests used to place only the first.
            for who in &there {
                earliest(&mut first_seen, who, &item.happened_from);
            }
        }

        if item.shape == Shape::Bond {
            let Some(other) = other else { continue };
            // A relationship entered on both people is recorded twice; it is
            // still one relationship.
            let pair = if item.node_id <= other {
                (item.node_id.clone(), other.clone(), item.happened_from.clone())
            } else {
                (other.clone(), item.node_id.clone(), item.happened_from.clone())
            };
            declared.insert((pair.0.clone(), pair.1.clone()));
            if linked.insert(pair) {
                links.push(TimedLink {
                    source: item.node_id.clone(),
                    target: other,
                    since: item.happened_from.clone(),
                    until: (item.happened_to != open_end).then(|| item.happened_to.clone()),
                    met: 0,
                });
            }
        }
    }

    // A pair seen together often enough to call it a relationship. Fewer than
    // this is a coincidence of attendance, not a life shared.
    for ((source, target), (since, met)) in together {
        if met < MET_ENOUGH {
            continue;
        }
        if declared.contains(&(source.clone(), target.clone())) {
            // They already told us about this one, ending and all.
            continue;
        }
        // No end: people met are not people parted, and saying when something
        // stopped needs evidence that it stopped.
        links.push(TimedLink { source, target, since, until: None, met });
    }

    TimeFrame {
        first_seen,
        died_on,
        links,
        earliest: months.keys().next().cloned(),
        density: months
            .into_iter()
            .map(|(month, (count, weight))| MonthCount { month, count, weight })
            .collect(),
    }
}

fn earliest(map: &mut HashMap<String, String>, id: &str, day: &str) {
    match map.get_mut(id) {
        Some(known) if day < known.as_str() => *known = day.to_string(),
        Some(_) => {}
        None => {
            map.insert(id.to_string(), day.to_string());
        }
    }
}

/// The day a stored instant fell on, here.
pub(crate) fn local_day(stamp: &str) -> Option<String> {
    if let Ok(moment) = DateTime::parse_from_rfc3339(stamp) {
        return Some(moment.with_timezone(&Local).format("%Y-%m-%d").to_string());
    }
    NaiveDate::parse_from_str(stamp.get(..10)?, "%Y-%m-%d")
        .ok()
        .map(when::iso)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::store::EventLink;

    fn item(kind: &str, node_id: &str, related: Option<&str>, from: &str, to: &str) -> Event {
        Event {
            id: format!("{node_id}#{kind}#{from}"),
            kind: kind.to_string(),
            node_id: node_id.to_string(),
            node_type: String::new(),
            title: String::new(),
            node_title: String::new(),
            // Người kia nằm ở link, không còn ở một cột (§4.9).
            links: related
                .map(|node| vec![EventLink { node_id: node.to_string(), role: "with".into(), label: None }])
                .unwrap_or_default(),
            magnitude: 0.0,
            container_node: None,
            props: serde_json::Value::Null,
            happened_from: from.to_string(),
            happened_to: to.to_string(),
            precision: "day".to_string(),
            time_source: "frontmatter".to_string(),
            source: "derived".to_string(),
            shape: shape_of(kind),
        }
    }

    /// The shape derivation gives each of these kinds, so a test that names a
    /// kind gets the behaviour the real event would have.
    fn shape_of(kind: &str) -> crate::timeline::derive::Shape {
        use crate::timeline::derive::Shape;
        match kind {
            "experience" => Shape::Spell,
            "connection" => Shape::Bond,
            "birthday" => Shape::Marker,
            "important_date" => Shape::Noted,
            "death" => Shape::Ending,
            "task_done" => Shape::Chore,
            _ => Shape::Occasion,
        }
    }


    fn node(id: &str, stable: &str, created: &str) -> FrameNode {
        FrameNode {
            id: id.to_string(),
            stable_id: stable.to_string(),
            created_at: created.to_string(),
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 15).unwrap()
    }

    #[test]
    fn a_person_arrives_with_the_first_thing_that_happened_with_them() {
        let nodes = [
            node("People/tuan.md", "uuid-tuan", "2026-01-01T12:00:00.000Z"),
            node("People/Interactions/c.md", "c", "2026-01-01T12:00:00.000Z"),
        ];
        let items = [
            item("interaction", "People/Interactions/c.md", Some("uuid-tuan"), "2016-05-03", "2016-05-03"),
            item("birthday", "People/tuan.md", None, "1990-01-01", "1990-01-01"),
            item("experience", "People/tuan.md", None, "2010-01-01", "9999-12-31"),
        ];
        let frame = build(&items, &nodes, today());

        assert_eq!(frame.first_seen["People/tuan.md"], "2016-05-03", "not born, not hired: met");
        assert_eq!(frame.first_seen["People/Interactions/c.md"], "2016-05-03");
    }

    #[test]
    fn a_node_with_no_dates_has_been_there_since_it_was_made() {
        let nodes = [node("Notes/idea.md", "idea", "2026-03-04T12:00:00.000Z")];
        let frame = build(&[], &nodes, today());
        assert_eq!(frame.first_seen["Notes/idea.md"], "2026-03-04");
        assert_eq!(frame.earliest, None, "a node being made is not something that happened");
    }

    #[test]
    fn nothing_counts_before_it_has_happened() {
        let nodes = [node("Events/later.md", "later", "2026-09-01T12:00:00.000Z")];
        let items = [item("event", "Events/later.md", None, "2099-01-01", "2099-01-01")];
        let frame = build(&items, &nodes, today());
        assert_eq!(frame.first_seen["Events/later.md"], "2026-09-01");
        assert!(frame.density.is_empty());
    }

    #[test]
    fn a_relationship_entered_on_both_people_is_one_link() {
        let nodes = [
            node("People/me.md", "uuid-me", "2026-01-01T12:00:00.000Z"),
            node("People/quang.md", "uuid-quang", "2026-01-01T12:00:00.000Z"),
            node("People/ha.md", "uuid-ha", "2026-01-01T12:00:00.000Z"),
        ];
        let items = [
            item("connection", "People/me.md", Some("uuid-quang"), "2014-07-01", "2018-07-31"),
            item("connection", "People/quang.md", Some("uuid-me"), "2014-07-01", "2018-07-31"),
            item("connection", "People/me.md", Some("uuid-ha"), "2013-10-19", "9999-12-31"),
        ];
        let frame = build(&items, &nodes, today());

        assert_eq!(frame.links.len(), 2);
        let work = frame.links.iter().find(|l| l.since == "2014-07-01").unwrap();
        assert_eq!(work.until.as_deref(), Some("2018-07-31"));
        let love = frame.links.iter().find(|l| l.since == "2013-10-19").unwrap();
        assert_eq!(love.until, None, "still going");
        assert_eq!(frame.first_seen["People/ha.md"], "2013-10-19");
    }

    #[test]
    fn the_strip_starts_with_the_users_life_not_with_a_grandmothers_birth() {
        let nodes = [node("People/ba.md", "ba", "2026-01-01T12:00:00.000Z")];
        let items = [
            item("birthday", "People/ba.md", None, "1932-03-02", "1932-03-02"),
            item("experience", "People/ba.md", None, "1950-01-01", "1980-12-31"),
            item("death", "People/ba.md", None, "2017-11-22", "2017-11-22"),
            item("note", "Notes/d.md", None, "2015-08-16", "2015-08-16"),
        ];
        let frame = build(&items, &nodes, today());

        assert_eq!(frame.earliest.as_deref(), Some("2015-08"));
        assert_eq!(
            frame.density,
            vec![
                MonthCount { month: "2015-08".into(), count: 1, weight: 0.0 },
                MonthCount { month: "2017-11".into(), count: 1, weight: 0.0 },
            ]
        );
        assert_eq!(frame.died_on["People/ba.md"], "2017-11-22");
        assert_eq!(frame.first_seen["People/ba.md"], "2026-01-01", "a death is not an arrival");
    }
}
