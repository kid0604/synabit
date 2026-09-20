//! Phong ấn: what the user has asked never to have brought back.
//!
//! The design is §8.1 of `docs/timeline-2026-09-17.md`.
//!
//! # What can be sealed
//!
//! - **A node**, usually a note: `sealed: true` in its frontmatter.
//! - **A person**: `sealed: true` on them. It covers the person and every
//!   interaction about them.
//! - **A period**: a file `Timeline/seals/<id>.json` holding `from` and `to`.
//!   It covers everything whose own date falls inside it (a daily note, an
//!   event, a finished task, a picture) and anything with no date that was
//!   made inside it. People are never sealed by a period; only what happened
//!   with them then.
//!
//! Every seal is a decision of the person, so every seal lives in the vault and
//! syncs: a flag rides the note's own frontmatter, and each period is a file of
//! its own, so two devices sealing two periods never write the same file.
//!
//! # What a seal does, and what it does not
//!
//! Nothing is deleted, and nothing is hidden from the person: the app's own
//! lists and search still find a sealed note, because they are how the person
//! gets back to it on purpose. What a seal stops is anything *bringing it
//! back*. The assistant does not read it, quote it, count it or name it;
//! reminders do not bring a sealed person back on their birthday; the timeline
//! shows a sealed period as sealed rather than what was in it.
//!
//! # Why one set, computed in one place
//!
//! Content reaches the assistant by about ten roads, and there is no single
//! point they all pass. So every road asks the same question of the same set,
//! computed here once for each change to the vault. No road can have its own
//! idea of what is sealed.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::{json, Value};

use super::derive::{self, NodeView};
use super::store::Event;
use super::when::{self, Precision};
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};

/// Where sealed periods live, one file each.
pub const SEALS_DIR: &str = "Timeline/seals";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SealedPeriod {
    pub id: String,
    /// The first day covered, `YYYY-MM-DD`.
    pub from: String,
    /// The last day covered, `YYYY-MM-DD`.
    pub to: String,
    /// As the person wrote them, so a period entered as `2019-02` is shown
    /// that way and not as `2019-02-01`.
    pub from_text: String,
    pub to_text: String,
}

#[derive(Debug, Default)]
pub struct Seals {
    periods: Vec<SealedPeriod>,
    /// Paths and identities of every node withheld.
    withheld: HashSet<String>,
    /// Paths and identities of sealed people.
    people: HashSet<String>,
}

/// A node as sealing needs it.
pub(crate) struct SealNode {
    pub id: String,
    pub stable_id: String,
    pub node_type: String,
    pub title: String,
    pub properties: Value,
    pub created_at: String,
}

impl Seals {
    pub fn is_empty(&self) -> bool {
        self.periods.is_empty() && self.withheld.is_empty()
    }

    pub fn periods(&self) -> &[SealedPeriod] {
        &self.periods
    }

    /// Whether a node, by path or identity, is withheld.
    ///
    /// A block or a moment inside one, `Notes/x.md#blk001` or
    /// `Files/ab.md#t=12`, is withheld with it: search hands back block ids.
    pub fn hides(&self, name: &str) -> bool {
        self.withheld.contains(name)
            || name
                .split_once('#')
                .is_some_and(|(whole, _)| self.withheld.contains(whole))
    }

    /// Every path and identity withheld.
    pub fn withheld(&self) -> impl Iterator<Item = &str> {
        self.withheld.iter().map(String::as_str)
    }

    /// Whether a day, `YYYY-MM-DD`, is inside a sealed period.
    pub fn covers(&self, day: &str) -> bool {
        self.in_period(day)
    }

    /// Whether any day of a month, `YYYY-MM`, is inside a sealed period.
    pub fn covers_month(&self, month: &str) -> bool {
        self.periods
            .iter()
            .any(|p| p.from.get(..7).is_some_and(|from| from <= month) && p.to.get(..7).is_some_and(|to| month <= to))
    }

    fn in_period(&self, day: &str) -> bool {
        self.periods
            .iter()
            .any(|p| p.from.as_str() <= day && day <= p.to.as_str())
    }

    /// Whether a timeline item must not be shown.
    /// Whether a timeline item must not be shown.
    ///
    /// Anyone it names is enough: a meeting with three people is withheld when
    /// any one of them is sealed, not only when the first is.
    pub fn hides_item(&self, item: &Event) -> bool {
        self.hides(&item.node_id)
            || item.links.iter().any(|link| self.hides_person(&link.node_id))
            || (item.shape.happened_in_time() && self.in_period(&item.happened_from))
    }

    /// Whether a person is sealed, by whichever of their names is to hand.
    ///
    /// A person is reached by path from a list and by identity from an event's
    /// links, and only one of those is in `withheld`. Asking `hides` alone
    /// therefore answers "no" for a sealed person named the other way — so
    /// every road asks this instead, and there is one definition rather than
    /// each caller's own.
    pub fn hides_person(&self, name: &str) -> bool {
        self.people.contains(name) || self.hides(name)
    }

    /// Whether `text` names anything withheld, by path or identity.
    pub fn mentions(&self, text: &str) -> bool {
        self.withheld.iter().any(|name| text.contains(name.as_str()))
    }

    /// A tool call that names a withheld node, answered as though the node did
    /// not exist. Saying "sealed" would tell the assistant it is there.
    pub fn refuse_argument(&self, args: &Value) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let named = args
            .get("node_id")
            .and_then(Value::as_str)
            .into_iter()
            .chain(
                args.get("node_ids")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            );
        for id in named {
            if self.hides(id.trim()) {
                return Some(not_found(id));
            }
        }
        None
    }

    /// A tool's result with everything withheld taken out of it.
    ///
    /// A list keeps its shape and loses its sealed rows, and its counts lose
    /// them too, or "12 results" beside 11 would say one is missing. A single
    /// node that is withheld reads as not found.
    pub fn withhold_results(&self, tool: &str, text: String) -> String {
        if self.is_empty() {
            return text;
        }
        // `read_board` answers in prose that opens with the file it read.
        if tool == "read_board" {
            let rel = text
                .strip_prefix("File: ")
                .and_then(|rest| rest.lines().next())
                .map(str::trim);
            return match rel {
                Some(rel) if self.hides(rel) => not_found(rel),
                _ => text,
            };
        }

        let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
            return text;
        };
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            if self.hides(id) {
                return not_found(id);
            }
        }
        let Some(results) = value.get_mut("results").and_then(Value::as_array_mut) else {
            return text;
        };
        let before = results.len();
        results.retain(|row| {
            !["id", "node_id"].iter().any(|key| {
                row.get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|id| self.hides(id))
            })
        });
        let removed = before - results.len();
        if removed == 0 {
            return text;
        }
        let kept = results.len();
        if let Some(object) = value.as_object_mut() {
            if object.contains_key("_returned") {
                object.insert("_returned".into(), kept.into());
            }
            for key in ["total_matches", "_total"] {
                if let Some(n) = object.get(key).and_then(Value::as_u64) {
                    object.insert(key.into(), n.saturating_sub(removed as u64).into());
                }
            }
        }
        value.to_string()
    }

    /// A node query's result without what is withheld, count included.
    pub fn withhold_query(&self, result: &mut crate::db::QueryResult) {
        let before = result.rows.len();
        result.rows.retain(|row| !self.hides(&row.id));
        result.total = result.total.saturating_sub(before - result.rows.len());
    }

    /// What is on screen, without a sealed node or what was selected in it.
    ///
    /// The person may well be reading a sealed note when they ask something.
    /// Having it open is looking at it on purpose; it is not an invitation for
    /// the assistant to read it too.
    pub fn withhold_focus(&self, mut focus: crate::syn::focus::Focus) -> crate::syn::focus::Focus {
        if focus.node.as_deref().is_some_and(|node| self.hides(node)) {
            focus.node = None;
            focus.node_title = None;
            focus.selection = None;
        }
        focus
    }

    pub fn read(db: &DbBridge, vault_path: &str) -> AppResult<Seals> {
        let periods = read_periods(vault_path);
        let conn = db.conn();
        let err = |e: rusqlite::Error| AppError::General(format!("seals: {e}"));

        let mut stmt = conn
            .prepare("SELECT id, COALESCE(stable_id, id), node_type, title, properties, created_at FROM nodes")
            .map_err(err)?;
        let nodes: Vec<SealNode> = stmt
            .query_map([], |r| {
                Ok(SealNode {
                    id: r.get(0)?,
                    stable_id: r.get(1)?,
                    node_type: r.get(2)?,
                    title: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    properties: serde_json::from_str(&r.get::<_, Option<String>>(4)?.unwrap_or_default())
                        .unwrap_or(Value::Null),
                    created_at: r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                })
            })
            .map_err(err)?
            .flatten()
            .collect();

        let mut stmt = conn
            .prepare(
                "SELECT s.id, t.id FROM node_edges e
                 JOIN nodes s ON s.stable_id = e.source_id
                 JOIN nodes t ON t.stable_id = e.target_id
                 WHERE e.edge_type = 'attachment'",
            )
            .map_err(err)?;
        let attachments: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?
            .flatten()
            .collect();

        Ok(compute(periods, &nodes, &attachments))
    }
}

fn not_found(id: &str) -> String {
    json!({ "error": "Node not found", "node_id": id }).to_string()
}

fn flagged(properties: &Value) -> bool {
    properties.get("sealed").and_then(Value::as_bool) == Some(true)
}

/// Types whose nodes are something the person wrote or kept, and so can be
/// part of a period. The app's own records, money and people are not.
fn is_content(node_type: &str) -> bool {
    !(derive::is_the_apps_own(node_type) || node_type == "person")
}

pub(crate) fn compute(
    periods: Vec<SealedPeriod>,
    nodes: &[SealNode],
    attachments: &[(String, String)],
) -> Seals {
    let date_fields: HashMap<String, Vec<String>> = nodes
        .iter()
        .filter(|n| n.node_type == "schema")
        .filter_map(|n| derive::date_fields_from_schema(&n.title, &n.properties))
        .collect();

    let mut people: HashSet<String> = HashSet::new();
    let mut withheld: HashSet<&str> = HashSet::new();
    for person in nodes.iter().filter(|n| n.node_type == "person" && flagged(&n.properties)) {
        people.insert(person.id.clone());
        people.insert(person.stable_id.clone());
        withheld.insert(&person.id);
    }

    let in_period = |day: &str| periods.iter().any(|p| p.from.as_str() <= day && day <= p.to.as_str());

    for node in nodes {
        if withheld.contains(node.id.as_str()) {
            continue;
        }
        let about_a_sealed_person = node.node_type == "interaction"
            && node
                .properties
                .get("person_id")
                .and_then(Value::as_str)
                .is_some_and(|person| people.contains(person.trim()));
        let hidden = flagged(&node.properties)
            || about_a_sealed_person
            || (!periods.is_empty()
                && is_content(&node.node_type)
                && happened_inside(node, &date_fields, &in_period));
        if hidden {
            withheld.insert(&node.id);
        }
    }

    // A picture that only sealed notes hold was part of what was sealed. One
    // that an open note also holds is not, or sealing a day would take a
    // picture away from every other day it is part of.
    let mut holders: HashMap<&str, (bool, bool)> = HashMap::new();
    for (note, file) in attachments {
        let held = holders.entry(file.as_str()).or_default();
        if withheld.contains(note.as_str()) {
            held.0 = true;
        } else {
            held.1 = true;
        }
    }
    for (file, (by_sealed, by_open)) in holders {
        if by_sealed && !by_open {
            withheld.insert(file);
        }
    }

    let identity: HashMap<&str, &str> = nodes
        .iter()
        .map(|n| (n.id.as_str(), n.stable_id.as_str()))
        .collect();
    let mut names = HashSet::with_capacity(withheld.len() * 2);
    for id in withheld {
        names.insert(id.to_string());
        if let Some(stable) = identity.get(id) {
            names.insert(stable.to_string());
        }
    }

    Seals {
        periods,
        withheld: names,
        people,
    }
}

/// Whether a node's own moment falls inside a sealed period: its date if it
/// has one, the day it was made if it has none.
fn happened_inside(
    node: &SealNode,
    date_fields: &HashMap<String, Vec<String>>,
    in_period: &impl Fn(&str) -> bool,
) -> bool {
    let view = NodeView {
        id: &node.id,
        node_type: &node.node_type,
        title: &node.title,
        properties: &node.properties,
    };
    // A moment kept into a note from the tray is about another day than the
    // note. It is hidden on its own (`hides_item`); it does not hide the note.
    let moments: Vec<String> = derive::derive(&view, date_fields)
        .into_iter()
        .filter(|d| d.shape.happened_in_time() && d.kind != "moment")
        .map(|d| when::iso(d.span.from))
        .collect();
    if moments.is_empty() {
        // A file's `created_at` is the day a scanner found it, which says
        // nothing about when anything happened. The attachment pass decides.
        node.node_type != "file"
            && super::frame::local_day(&node.created_at).is_some_and(|day| in_period(&day))
    } else {
        moments.iter().any(|day| in_period(day))
    }
}

/// A conversation as it is sent again, with the words of any earlier answer
/// that drew on something since sealed left out.
///
/// The turn keeps its place, so turns still alternate for the providers that
/// insist on it; only what it said is withheld.
pub fn history_without_sealed(
    messages: &[crate::models::syn::SynMessage],
    seals: &Seals,
) -> Vec<crate::models::syn::SynMessage> {
    messages
        .iter()
        .map(|message| {
            let drew_on_sealed = !seals.is_empty()
                && message.role == "assistant"
                && (message.sources.as_ref().is_some_and(|sources| sources.iter().any(|s| seals.hides(&s.id)))
                    || seals.mentions(&message.content));
            if !drew_on_sealed {
                return message.clone();
            }
            let mut withheld = message.clone();
            withheld.content = "(An earlier answer, left out: it drew on something since sealed.)".into();
            withheld.sources = None;
            withheld
        })
        .collect()
}

/// The nodes a reminder may be planned for: none that is sealed by flag, by
/// person or by period. See `calendar::reminders`.
///
/// When the seals cannot be read, only the flag on each node is honoured:
/// dropping every reminder over a read error would be worse than that.
pub fn without_sealed(
    db: &DbBridge,
    vault_path: &str,
    nodes: Vec<crate::models::node::NodeMetadata>,
) -> Vec<crate::models::node::NodeMetadata> {
    match current(db, vault_path) {
        Ok(seals) if seals.is_empty() => nodes,
        Ok(seals) => nodes.into_iter().filter(|node| !seals.hides(&node.id)).collect(),
        Err(e) => {
            log::warn!("reminders: seals could not be read, only flags are honoured: {e}");
            nodes
                .into_iter()
                .filter(|node| node.properties.get("sealed").and_then(Value::as_bool) != Some(true))
                .collect()
        }
    }
}

/// Every sealed period in the vault. A file that is not a period is skipped:
/// a seal that cannot be read should not take the others down with it.
pub fn read_periods(vault_path: &str) -> Vec<SealedPeriod> {
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join(SEALS_DIR)) else {
        return Vec::new();
    };
    let mut periods: Vec<SealedPeriod> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
            let from_text = value.get("from")?.as_str()?.trim().to_string();
            let to_text = value.get("to")?.as_str()?.trim().to_string();
            let (from, to) = period_bounds(&from_text, &to_text)?;
            Some(SealedPeriod { id, from, to, from_text, to_text })
        })
        .collect();
    periods.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.id.cmp(&b.id)));
    periods
}

/// A period's first and last day, when both ends are a day, a month or a year
/// and the end does not come first.
pub(crate) fn period_bounds(from: &str, to: &str) -> Option<(String, String)> {
    // `parse_written`, not `parse`: a period in a seal file is a decision that
    // outlives the day it was made, and `yesterday` in one would mean a
    // different pair of days every morning. See `when::parse_written`.
    let point = |text: &str| {
        when::parse_written(text).filter(|span| {
            matches!(span.precision, Precision::Day | Precision::Month | Precision::Year)
        })
    };
    let (start, end) = (point(from)?, point(to)?);
    (start.from <= end.to).then(|| (when::iso(start.from), when::iso(end.to)))
}

/// Seal a period, as a file of its own.
pub fn write_period(vault_path: &str, from: &str, to: &str) -> AppResult<SealedPeriod> {
    let (from, to) = (from.trim(), to.trim());
    let (first, last) = period_bounds(from, to).ok_or_else(|| {
        AppError::General(format!(
            "'{from}' to '{to}' is not a period that can be sealed. Write each end as 2019, 2019-02 \
             or 2019-02-14, with the end not before the start."
        ))
    })?;
    let id = uuid::Uuid::new_v4().to_string();
    let dir = Path::new(vault_path).join(SEALS_DIR);
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    let body = json!({
        "from": from,
        "to": to,
        // Sync settles two copies of a JSON file by this stamp.
        "metadata": { "updated_at": chrono::Utc::now().to_rfc3339() },
    });
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_string_pretty(&body)?)
        .map_err(AppError::Io)?;
    Ok(SealedPeriod {
        id,
        from: first,
        to: last,
        from_text: from.to_string(),
        to_text: to.to_string(),
    })
}

/// Lift a period's seal by removing its file.
///
/// The id is a file name this module wrote, which is a uuid. Anything else is
/// refused rather than joined onto a path.
pub fn remove_period(vault_path: &str, id: &str) -> AppResult<()> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(AppError::General(format!("'{id}' is not a seal")));
    }
    let path = Path::new(vault_path).join(SEALS_DIR).join(format!("{id}.json"));
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Io(e)),
    }
}

type DirSignature = Vec<(String, u64, Option<SystemTime>)>;

struct Cached {
    connection: usize,
    changes: u64,
    vault: String,
    dir: DirSignature,
    seals: Arc<Seals>,
}

static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();

fn dir_signature(vault_path: &str) -> DirSignature {
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join(SEALS_DIR)) else {
        return Vec::new();
    };
    let mut signature: DirSignature = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((entry.file_name().to_string_lossy().to_string(), meta.len(), meta.modified().ok()))
        })
        .collect();
    signature.sort();
    signature
}

/// The seals in force, worked out again only when the vault or a seal file
/// has changed since the last time anyone asked.
///
/// The cache's lock is never held while another lock is taken, so it cannot
/// join a deadlock with the vault cache or the timeline.
pub fn current(db: &DbBridge, vault_path: &str) -> AppResult<Arc<Seals>> {
    let connection = db.conn() as *const rusqlite::Connection as usize;
    let changes = db.conn().total_changes();
    let dir = dir_signature(vault_path);
    let cache = CACHE.get_or_init(|| Mutex::new(None));

    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if hit.connection == connection && hit.changes == changes && hit.vault == vault_path && hit.dir == dir {
            return Ok(hit.seals.clone());
        }
    }

    let seals = Arc::new(Seals::read(db, vault_path)?);
    *cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(Cached {
        connection,
        changes,
        vault: vault_path.to_string(),
        dir,
        seals: seals.clone(),
    });
    Ok(seals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::store::EventLink;

    #[test]
    fn an_event_is_withheld_when_anyone_at_it_is_sealed() {
        let sealed_person = node("People/ex.md", "person", serde_json::json!({ "sealed": true, "node_id": "uuid-ex" }), "");
        let seals = compute(Vec::new(), &[sealed_person], &[]);
        let mut meeting = item("moment", "Notes/day.md", None, "2026-05-01");
        meeting.links = vec![
            EventLink { node_id: "uuid-tuan".into(), role: "with".into(), label: None },
            EventLink { node_id: "uuid-ex".into(), role: "with".into(), label: None },
        ];
        assert!(seals.hides_item(&meeting), "the second person at it is sealed");

        meeting.links = vec![EventLink { node_id: "uuid-tuan".into(), role: "with".into(), label: None }];
        assert!(!seals.hides_item(&meeting));
    }

    #[test]
    fn a_relationship_that_began_inside_a_sealed_period_is_not_brought_back() {
        let seals = compute(vec![period("2019-02", "2019-09")], &[], &[]);
        assert!(seals.hides_item(&item("connection", "People/a.md", Some("People/b.md"), "2019-05-01")));
        assert!(seals.hides_item(&item("important_date", "People/a.md", None, "2019-05-01")));

        // Older than the period, and still going: it did not happen in there.
        assert!(!seals.hides_item(&item("connection", "People/a.md", Some("People/b.md"), "2016-01-01")));
        // A birthday is not of the period it happens to fall in.
        assert!(!seals.hides_item(&item("birthday", "People/a.md", None, "2019-05-01")));
        assert!(!seals.hides_item(&item("experience", "People/a.md", None, "2019-05-01")));
    }

    #[test]
    fn an_earlier_answer_from_something_since_sealed_is_not_sent_again() {
        let seals = compute(Vec::new(), &[node("Notes/diary.md", "note", serde_json::json!({ "sealed": true }), "")], &[]);
        let message = |role: &str, content: &str, source: Option<&str>| -> crate::models::syn::SynMessage {
            serde_json::from_value(serde_json::json!({
                "id": content, "role": role, "content": content, "model": null, "timestamp": "",
                "tokens": null, "duration_ms": null,
                "sources": source.map(|id| vec![serde_json::json!({ "id": id, "title": "t", "node_type": "note" })]),
            }))
            .unwrap()
        };
        let sent = history_without_sealed(
            &[
                message("user", "what did I write?", None),
                message("assistant", "You wrote about the fight.", Some("Notes/diary.md")),
                message("assistant", "From Notes/diary.md: the fight.", None),
                message("assistant", "Buy milk.", Some("Notes/list.md")),
            ],
            &seals,
        );
        assert_eq!(sent.len(), 4, "turns keep their places");
        assert!(sent[1].content.contains("left out") && sent[1].sources.is_none());
        assert!(sent[2].content.contains("left out"));
        assert_eq!(sent[3].content, "Buy milk.");
        assert_eq!(sent[0].content, "what did I write?");
    }

    #[test]
    fn a_block_of_a_sealed_note_is_sealed_with_it() {
        let seals = compute(Vec::new(), &[node("Notes/x.md", "note", serde_json::json!({ "sealed": true }), "")], &[]);
        assert!(seals.hides("Notes/x.md#blk001"));
        assert!(!seals.hides("Notes/y.md#blk001"));
        assert!(!seals.hides("Notes/x.md.bak"));
    }

    #[test]
    fn a_file_is_not_sealed_by_the_day_it_was_indexed() {
        let file = node("Files/ab.md", "file", serde_json::json!({ "path": "/vault/assets/IMG_1234.jpg" }), "2026-03-01T00:00:00.000Z");
        let seals = compute(vec![period("2026-01", "2026-06")], &[file], &[]);
        assert!(!seals.hides("Files/ab.md"));
    }

    #[test]
    fn a_moment_kept_into_a_note_does_not_seal_the_note() {
        let note = node(
            "Notes/2026-09-14.md",
            "note",
            serde_json::json!({ "date": "2026-09-14", "moments": [{ "title": "Tết", "happened": "2019-02-05" }] }),
            "2026-09-14T00:00:00.000Z",
        );
        let seals = compute(vec![period("2019-02", "2019-02")], &[note], &[]);
        assert!(!seals.hides("Notes/2026-09-14.md"));
    }

    #[test]
    fn no_reminder_is_planned_for_anything_sealed() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let db = DbBridge::new_in_memory_full().unwrap();
        let at = |id: &str, node_type: &str, properties: Value, created_at: &str| crate::models::node::NodeMetadata {
            id: id.into(),
            node_type: node_type.into(),
            title: id.into(),
            content: String::new(),
            properties,
            created_at: created_at.into(),
            updated_at: created_at.into(),
            timestamp: 0,
            blocks: None,
        };
        let nodes = vec![
            at("Events/wedding.md", "event", serde_json::json!({ "start_at": "2019-05-11T09:00:00" }), "2019-05-01T00:00:00.000Z"),
            at("Tasks/call.md", "task", serde_json::json!({ "due_date": "2026-09-20", "status": "todo" }), "2026-09-01T00:00:00.000Z"),
            at("Tasks/secret.md", "task", serde_json::json!({ "due_date": "2026-09-21", "status": "todo", "sealed": true }), "2026-09-01T00:00:00.000Z"),
        ];
        for n in &nodes {
            db.upsert_node(n).unwrap();
        }
        write_period(&vault, "2019-02", "2019-09").unwrap();
        let kept: Vec<String> = without_sealed(&db, &vault, nodes).into_iter().map(|n| n.id).collect();
        assert_eq!(kept, vec!["Tasks/call.md".to_string()]);
    }

    fn node(id: &str, node_type: &str, properties: Value, created_at: &str) -> SealNode {
        SealNode {
            id: id.to_string(),
            stable_id: properties
                .get("node_id")
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_string(),
            node_type: node_type.to_string(),
            title: id.to_string(),
            properties,
            created_at: created_at.to_string(),
        }
    }

    fn period(from: &str, to: &str) -> SealedPeriod {
        let (first, last) = period_bounds(from, to).unwrap();
        SealedPeriod {
            id: format!("{from}-{to}"),
            from: first,
            to: last,
            from_text: from.into(),
            to_text: to.into(),
        }
    }

    fn item(kind: &str, node_id: &str, related: Option<&str>, from: &str) -> Event {
        Event {
            id: format!("{node_id}#{kind}"),
            kind: kind.into(),
            node_id: node_id.into(),
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
            happened_from: from.into(),
            happened_to: from.into(),
            precision: "day".into(),
            time_source: "frontmatter".into(),
            source: "derived".into(),
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


    const MADE: &str = "2026-01-01T12:00:00.000Z";

    #[test]
    fn a_note_sealed_by_its_own_flag_is_withheld_under_both_its_names() {
        let seals = compute(
            vec![],
            &[
                node("Notes/a.md", "note", json!({ "sealed": true, "node_id": "uuid-a" }), MADE),
                node("Notes/b.md", "note", json!({ "sealed": false }), MADE),
            ],
            &[],
        );
        assert!(seals.hides("Notes/a.md"));
        assert!(seals.hides("uuid-a"));
        assert!(!seals.hides("Notes/b.md"));
    }

    #[test]
    fn a_sealed_person_takes_their_interactions_but_not_notes_that_merely_mention_them() {
        let seals = compute(
            vec![],
            &[
                node("People/ex.md", "person", json!({ "sealed": true, "node_id": "uuid-ex" }), MADE),
                node("People/Interactions/c.md", "interaction", json!({ "person_id": "uuid-ex", "date": "2024-01-01" }), MADE),
                node("People/Interactions/d.md", "interaction", json!({ "person_id": "uuid-friend", "date": "2024-01-01" }), MADE),
                node("Notes/work.md", "note", json!({}), MADE),
            ],
            &[],
        );
        assert!(seals.hides("People/ex.md"));
        assert!(seals.hides("People/Interactions/c.md"));
        assert!(!seals.hides("People/Interactions/d.md"));
        assert!(!seals.hides("Notes/work.md"));
    }

    #[test]
    fn a_period_takes_what_happened_in_it_and_what_was_made_in_it_with_no_date() {
        let seals = compute(
            vec![period("2019-02", "2019-09")],
            &[
                node("Notes/2019-05-11.md", "note", json!({ "date": "2019-05-11" }), MADE),
                node("Notes/2021.md", "note", json!({ "date": "2021-01-01" }), "2019-05-01T12:00:00.000Z"),
                node("QuickCaps/then.md", "quickcap", json!({}), "2019-03-03T12:00:00.000Z"),
                node("QuickCaps/now.md", "quickcap", json!({}), MADE),
                node("People/linh.md", "person", json!({ "died_on": "2019-05-01" }), "2019-05-01T12:00:00.000Z"),
                node("Schema/animal.md", "schema", json!({}), "2019-05-01T12:00:00.000Z"),
            ],
            &[],
        );
        assert!(seals.hides("Notes/2019-05-11.md"), "dated inside");
        assert!(!seals.hides("Notes/2021.md"), "its own date wins over when it was written");
        assert!(seals.hides("QuickCaps/then.md"), "no date, made inside");
        assert!(!seals.hides("QuickCaps/now.md"));
        assert!(!seals.hides("People/linh.md"), "a person is never sealed by a period");
        assert!(!seals.hides("Schema/animal.md"), "the app's own records are not part of a period");
    }

    #[test]
    fn a_picture_goes_with_the_sealed_note_unless_an_open_note_also_holds_it() {
        let seals = compute(
            vec![],
            &[
                node("Notes/sealed.md", "note", json!({ "sealed": true }), MADE),
                node("Notes/open.md", "note", json!({}), MADE),
                node("Files/only-sealed.md", "file", json!({}), MADE),
                node("Files/shared.md", "file", json!({}), MADE),
            ],
            &[
                ("Notes/sealed.md".into(), "Files/only-sealed.md".into()),
                ("Notes/sealed.md".into(), "Files/shared.md".into()),
                ("Notes/open.md".into(), "Files/shared.md".into()),
            ],
        );
        assert!(seals.hides("Files/only-sealed.md"));
        assert!(!seals.hides("Files/shared.md"));
    }

    #[test]
    fn a_timeline_item_is_withheld_for_its_node_its_person_or_its_period() {
        let seals = compute(
            vec![period("2019-02", "2019-09")],
            &[node("People/ex.md", "person", json!({ "sealed": true, "node_id": "uuid-ex" }), MADE)],
            &[],
        );
        assert!(seals.hides_item(&item("birthday", "People/ex.md", None, "1990-01-01")));
        assert!(seals.hides_item(&item("connection", "People/me.md", Some("uuid-ex"), "2013-10-01")));
        assert!(seals.hides_item(&item("task_done", "Tasks/t.md", None, "2019-05-01")));
        assert!(!seals.hides_item(&item("experience", "People/me.md", None, "2019-05-01")), "a job spans past a period");
        assert!(!seals.hides_item(&item("task_done", "Tasks/t.md", None, "2019-10-01")));
    }

    #[test]
    fn a_tool_result_loses_its_sealed_rows_and_its_counts_agree() {
        let seals = compute(vec![], &[node("Notes/a.md", "note", json!({ "sealed": true }), MADE)], &[]);
        let result = json!({
            "results": [{ "id": "Notes/a.md" }, { "id": "Notes/b.md" }],
            "total_matches": 2,
            "_returned": 2
        })
        .to_string();
        let kept: Value = serde_json::from_str(&seals.withhold_results("query_nodes", result)).unwrap();
        assert_eq!(kept["results"].as_array().unwrap().len(), 1);
        assert_eq!(kept["total_matches"], 1);
        assert_eq!(kept["_returned"], 1);

        let single = json!({ "id": "Notes/a.md", "content": "secret" }).to_string();
        assert!(!seals.withhold_results("get_node", single).contains("secret"));

        let board = "File: Notes/a.md\nsecret".to_string();
        assert!(!seals.withhold_results("read_board", board).contains("secret"));

        assert!(seals.refuse_argument(&json!({ "node_id": "Notes/a.md" })).is_some());
        assert!(seals.refuse_argument(&json!({ "node_ids": ["Notes/b.md", "Notes/a.md"] })).is_some());
        assert!(seals.refuse_argument(&json!({ "node_id": "Notes/b.md" })).is_none());
    }

    /// The instant count is answered straight from the index. A sealed note
    /// counted there is a sealed note announced.
    #[test]
    fn a_count_does_not_include_what_is_sealed() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        for (id, properties) in [("Tasks/a.md", json!({ "sealed": true })), ("Tasks/b.md", json!({}))] {
            db.upsert_node(&crate::models::node::NodeMetadata {
                id: id.into(),
                node_type: "task".into(),
                title: id.into(),
                content: String::new(),
                properties,
                created_at: MADE.into(),
                updated_at: MADE.into(),
                timestamp: 0,
                blocks: None,
            })
            .expect("node");
        }
        let seals = Seals::read(&db, "/nonexistent-vault").expect("seals");
        let mut result = db
            .run_node_query(&crate::query::parse("type:task"))
            .expect("query");
        assert_eq!(result.total, 2, "the fixture counts both");

        seals.withhold_query(&mut result);
        let ids: Vec<&str> = result.rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["Tasks/b.md"]);
        assert_eq!(result.total, 1);
    }

    #[test]
    fn what_is_on_screen_loses_a_sealed_note_and_its_selection() {
        let seals = compute(vec![], &[node("Notes/a.md", "note", json!({ "sealed": true }), MADE)], &[]);
        let focus = crate::syn::focus::Focus {
            app: "note".into(),
            node: Some("Notes/a.md".into()),
            node_title: Some("A".into()),
            selection: Some("secret".into()),
            ..Default::default()
        };
        let kept = seals.withhold_focus(focus);
        assert_eq!((kept.node, kept.node_title, kept.selection), (None, None, None));
        assert_eq!(kept.app, "note", "the app the person is in is not sealed");
    }

    #[test]
    fn a_period_is_written_read_back_and_lifted_as_a_file_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();

        let sealed = write_period(&vault, "2019-02", "2019-09").unwrap();
        assert_eq!((sealed.from.as_str(), sealed.to.as_str()), ("2019-02-01", "2019-09-30"));
        std::fs::write(dir.path().join(SEALS_DIR).join("broken.json"), "not json").unwrap();

        let read = read_periods(&vault);
        assert_eq!(read, vec![sealed.clone()], "an unreadable file is skipped, not fatal");

        assert!(write_period(&vault, "2019-09", "2019-02").is_err(), "an end before the start");
        assert!(write_period(&vault, "~2019", "2020").is_err(), "a guess is not a period");
        assert!(remove_period(&vault, "../../Notes/a").is_err(), "an id is never a path");

        remove_period(&vault, &sealed.id).unwrap();
        assert!(read_periods(&vault).is_empty());
    }
}
