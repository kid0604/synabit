//! "Đừng nhắc": what the app is not allowed to bring up on its own.
//!
//! The design is §7.3 and §16 Bước 3 of `docs/timeline-2026-09-17.md`.
//!
//! # The difference from a seal
//!
//! A seal ([`super::seal`]) says *nobody reads this* — not the assistant, not a
//! count, not a list. Quiet says something much narrower and much more common:
//! **the app does not raise it first**. A hushed person is still searched, still
//! opened, still drawn on the graph. What stops is the app tapping the person on
//! the shoulder about them.
//!
//! That is the line §7.3 draws for the dead, and it is the only line that makes
//! sense there. Someone who lost their father does not want the app hiding him;
//! they want it to stop wishing him a happy birthday.
//!
//! # Three ways it goes quiet
//!
//! - **By hand**, a file each: a person, one event, or a stretch of time.
//! - **On its own**, for anyone with a `died_on`. Nobody has to declare it, and
//!   nobody can forget to. §7.3 says *by default*, and a default that waits to
//!   be switched on is not a default.
//! - **By dismissing**, because §16 Bước 3 asks that anything the app offers be
//!   refusable in one action and that the refusal be remembered. A dismissal is
//!   an ordinary hush with an expiry the feature chooses — a year for §7.1, a few
//!   months for §7.2.
//!
//! # Why a file each, not one `quiet.json`
//!
//! The plan said one file. One file is wrong, and it is wrong in the direction
//! that costs the most. Sync settles a JSON document whole, by
//! `metadata.updated_at`: two devices that hush two different people in the same
//! stretch of time produce two whole documents, and one of them wins entirely.
//! The losing hush does not come back — the app simply starts talking about
//! someone the person told it to leave alone. For a feature whose entire job is
//! to honour a refusal, losing a refusal is not a rough edge, it is the failure.
//! A file each cannot collide, which is why [`super::seal`] is built that way
//! too.
//!
//! # Why an event is keyed by its note and its day
//!
//! An event's id is `path#kind#n` — the *n*-th thing derived from that note. It
//! is rebuilt from scratch whenever the note changes, so fixing a typo above a
//! moment can renumber it. A hush is tier 2, a decision of the person, and it
//! has to outlive every rebuild of tier 3. So a hushed event is remembered as
//! the note it came from and the day it happened, both of which survive an edit.
//!
//! Two events on the same day from the same note therefore share a key, and
//! dismissing one dismisses both. That is the wrong answer in the quiet
//! direction rather than the loud one, which is the only direction this module
//! is allowed to be wrong in.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::{json, Value};

use super::seal::{period_bounds, Seals};
use super::store::Event;
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};

/// Where hushes live, one file each.
pub const QUIET_DIR: &str = "Timeline/quiet";

/// What one hush covers.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "about", rename_all = "snake_case")]
pub enum Subject {
    /// A person, by path or identity. Covers everything the app would raise
    /// about them.
    Person { who: String },
    /// One thing that happened: the note it was written in, and the day.
    Moment { node: String, day: String },
    /// Every day from `from` to `to`, inclusive.
    Period { from: String, to: String },
    /// One sentence of one note, by the note and a hash of the sentence.
    ///
    /// A hash rather than the sentence itself, and not to hide anything — the
    /// text is the person's own, in their own vault. It is because a hush file
    /// outlives the note: keep the words here and sealing that note later
    /// would leave a copy of its sentence sitting in `Timeline/quiet`, outside
    /// everything the seal covers. A decision file records a decision, not a
    /// second copy of the diary.
    Line { node: String, line: String },
}

/// How a sentence is named in a hush, so the same sentence is recognised again
/// after the note around it has been edited.
pub fn line_id(text: &str) -> String {
    blake3::hash(text.trim().as_bytes()).to_hex().to_string()
}

/// One decision to stop the app bringing something up.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hush {
    pub id: String,
    #[serde(flatten)]
    pub subject: Subject,
    /// The last day it holds, `YYYY-MM-DD`. `None` means for good.
    pub until: Option<String>,
    /// The day the person said so, kept so a list can be read in order.
    pub made: String,
}

/// Everything the app is not allowed to raise on its own, as of one day.
///
/// Expiry is settled when this is built, so nothing downstream has to carry a
/// date around or remember to check one.
#[derive(Debug, Default)]
pub struct Quiet {
    hushes: Vec<Hush>,
    /// Paths and identities hushed by hand.
    people: HashSet<String>,
    /// `(note, day)` pairs hushed by hand.
    moments: HashSet<(String, String)>,
    /// `(note, line id)` pairs hushed by hand.
    lines: HashSet<(String, String)>,
    periods: Vec<(String, String)>,
    /// Paths and identities of people with a `died_on`. §7.3.
    dead: HashSet<String>,
}

impl Quiet {
    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
            && self.moments.is_empty()
            && self.lines.is_empty()
            && self.periods.is_empty()
            && self.dead.is_empty()
    }

    /// Every hush the person made by hand, oldest first. The dead are not in
    /// here: nobody decided that, so there is nothing to undo.
    pub fn hushes(&self) -> &[Hush] {
        &self.hushes
    }

    /// Whether the app may not raise this person, by path or identity.
    pub fn hushes_person(&self, who: &str) -> bool {
        self.people.contains(who) || self.dead.contains(who)
    }

    /// Whether this person is quiet because they died, rather than because
    /// anyone asked. The two read the same to a reminder and must never read
    /// the same to a person looking at the list.
    pub fn is_dead(&self, who: &str) -> bool {
        self.dead.contains(who)
    }

    /// Whether the app may not raise this day's writing from this note.
    pub fn hushes_moment(&self, node: &str, day: &str) -> bool {
        self.moments.contains(&(node.to_string(), day.to_string()))
    }

    /// Everyone quiet because they died, by every name they are known under.
    pub fn dead_people(&self) -> impl Iterator<Item = &str> {
        self.dead.iter().map(String::as_str)
    }

    /// Whether one sentence of one note has been waved away.
    pub fn hushes_line(&self, node: &str, text: &str) -> bool {
        self.lines.contains(&(node.to_string(), line_id(text)))
    }

    /// Whether a day, `YYYY-MM-DD`, falls in a hushed stretch.
    pub fn hushes_day(&self, day: &str) -> bool {
        self.periods.iter().any(|(from, to)| from.as_str() <= day && day <= to.as_str())
    }
}

/// Something the app is about to raise unasked.
///
/// Built freely by whatever feature wants to offer it; it cannot reach a screen
/// without going through [`allow`] first.
#[derive(Debug, Clone, PartialEq)]
pub struct Nudge {
    /// The day it is about, `YYYY-MM-DD`.
    pub day: String,
    /// The note it quotes, when it quotes one.
    pub node: Option<String>,
    /// Everyone it names, by path or identity.
    pub people: Vec<String>,
}

impl Nudge {
    pub fn on(day: impl Into<String>) -> Nudge {
        Nudge { day: day.into(), node: None, people: Vec::new() }
    }

    pub fn from_note(mut self, node: impl Into<String>) -> Nudge {
        self.node = Some(node.into());
        self
    }

    pub fn naming(mut self, people: impl IntoIterator<Item = String>) -> Nudge {
        self.people = people.into_iter().collect();
        self
    }

    /// The nudge an event would make, naming everyone linked to it.
    pub fn for_event(event: &Event) -> Nudge {
        Nudge {
            day: event.happened_from.clone(),
            node: Some(
                event.container_node.clone().unwrap_or_else(|| event.node_id.clone()),
            ),
            people: event.links.iter().map(|l| l.node_id.clone()).collect(),
        }
    }
}

/// A nudge that has been through both consent gates.
///
/// The inner value is private and there is no other constructor, so a feature
/// cannot hand the screen something that skipped the check — §16 Bước 3 asks
/// that the gate be tested on the road the real feature takes, and the cheapest
/// way to keep that true is to leave no other road.
#[derive(Debug, Clone, PartialEq)]
pub struct Offered(Nudge);

impl Offered {
    pub fn nudge(&self) -> &Nudge {
        &self.0
    }

    pub fn into_nudge(self) -> Nudge {
        self.0
    }
}

/// Everything the app offers unasked passes here, and most of it does not come
/// out the other side.
///
/// Both gates are asked at once because they answer different questions and a
/// caller that remembered one would forget the other: a seal means *withheld
/// entirely*, a hush means *not raised first*, and a reminder is stopped by
/// either.
pub fn allow(nudges: Vec<Nudge>, quiet: &Quiet, seals: &Seals) -> Vec<Offered> {
    nudges
        .into_iter()
        .filter(|nudge| {
            if seals.covers(&nudge.day) || quiet.hushes_day(&nudge.day) {
                return false;
            }
            if let Some(node) = &nudge.node {
                if seals.hides(node) || quiet.hushes_moment(node, &nudge.day) {
                    return false;
                }
            }
            !nudge.people.iter().any(|who| seals.hides_person(who) || quiet.hushes_person(who))
        })
        .map(Offered)
        .collect()
}

// ─── Reading the vault ───────────────────────────────────────────────

/// A person as this module needs them.
pub(crate) struct QuietNode {
    pub id: String,
    pub stable_id: String,
    pub node_type: String,
    pub properties: Value,
}

impl Quiet {
    pub fn read(db: &DbBridge, vault_path: &str, today: &str) -> AppResult<Quiet> {
        let conn = db.conn();
        let err = |e: rusqlite::Error| AppError::General(format!("quiet: {e}"));
        let mut stmt = conn
            .prepare("SELECT id, COALESCE(stable_id, id), node_type, properties FROM nodes WHERE node_type = 'person'")
            .map_err(err)?;
        let people: Vec<QuietNode> = stmt
            .query_map([], |r| {
                Ok(QuietNode {
                    id: r.get(0)?,
                    stable_id: r.get(1)?,
                    node_type: r.get(2)?,
                    properties: serde_json::from_str(
                        &r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    )
                    .unwrap_or(Value::Null),
                })
            })
            .map_err(err)?
            .flatten()
            .collect();
        Ok(compute(read_hushes(vault_path), &people, today))
    }
}

/// Aliases of one node: a hush written against either the path or the identity
/// has to catch the other, because different roads name the same person
/// differently.
fn aliases(node: &QuietNode) -> [&str; 3] {
    let identity = node.properties.get("node_id").and_then(Value::as_str).unwrap_or_default();
    [node.id.as_str(), node.stable_id.as_str(), identity]
}

pub(crate) fn compute(hushes: Vec<Hush>, people: &[QuietNode], today: &str) -> Quiet {
    let live: Vec<Hush> = hushes
        .into_iter()
        .filter(|h| h.until.as_deref().is_none_or(|until| until >= today))
        .collect();

    // A person hushed by one of their names is hushed by all of them.
    let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
    for person in people {
        for name in aliases(person).into_iter().filter(|n| !n.is_empty()) {
            by_name.entry(name).or_default().extend(
                aliases(person).into_iter().filter(|n| !n.is_empty()),
            );
        }
    }

    let mut quiet = Quiet::default();
    for hush in &live {
        match &hush.subject {
            Subject::Person { who } => {
                quiet.people.insert(who.clone());
                for also in by_name.get(who.as_str()).into_iter().flatten() {
                    quiet.people.insert((*also).to_string());
                }
            }
            Subject::Moment { node, day } => {
                quiet.moments.insert((node.clone(), day.clone()));
            }
            Subject::Line { node, line } => {
                quiet.lines.insert((node.clone(), line.clone()));
            }
            Subject::Period { from, to } => {
                if let Some(bounds) = period_bounds(from, to) {
                    quiet.periods.push(bounds);
                }
            }
        }
    }

    // §7.3. Not a hush anyone made, so it is kept apart from the list and
    // cannot be lifted by undoing one.
    for person in people.iter().filter(|p| p.node_type == "person") {
        let died = person.properties.get("died_on").and_then(Value::as_str).unwrap_or("").trim();
        if died.is_empty() || super::when::parse(died).is_none() {
            continue;
        }
        for name in aliases(person).into_iter().filter(|n| !n.is_empty()) {
            quiet.dead.insert(name.to_string());
        }
    }

    quiet.hushes = live;
    quiet.hushes.sort_by(|a, b| a.made.cmp(&b.made).then_with(|| a.id.cmp(&b.id)));
    quiet
}

pub fn read_hushes(vault_path: &str) -> Vec<Hush> {
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join(QUIET_DIR)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
            let text = |key: &str| {
                value.get(key).and_then(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
            };
            let subject = if let Some(who) = text("person") {
                Subject::Person { who }
            } else if let (Some(node), Some(line)) = (text("node"), text("line")) {
                Subject::Line { node, line }
            } else if let (Some(node), Some(day)) = (text("node"), text("day")) {
                Subject::Moment { node, day }
            } else if let (Some(from), Some(to)) = (text("from"), text("to")) {
                Subject::Period { from, to }
            } else {
                return None;
            };
            Some(Hush { id, subject, until: text("until"), made: text("made").unwrap_or_default() })
        })
        .collect()
}

/// Ask the app to stop raising something, as a file of its own.
pub fn write_hush(vault_path: &str, subject: &Subject, until: Option<&str>) -> AppResult<Hush> {
    let mut body = match subject {
        Subject::Person { who } => json!({ "person": who }),
        Subject::Moment { node, day } => json!({ "node": node, "day": day }),
        Subject::Line { node, line } => json!({ "node": node, "line": line }),
        Subject::Period { from, to } => {
            if period_bounds(from, to).is_none() {
                return Err(AppError::General(format!(
                    "'{from}' to '{to}' is not a stretch of time. Write each end as 2019, 2019-02 \
                     or 2019-02-14, with the end not before the start."
                )));
            }
            json!({ "from": from, "to": to })
        }
    };
    let made = crate::timeline::when::iso(chrono::Local::now().date_naive());
    let map = body.as_object_mut().expect("a JSON object was just built");
    map.insert("made".into(), Value::from(made.clone()));
    if let Some(until) = until.map(str::trim).filter(|u| !u.is_empty()) {
        if super::when::parse(until).is_none() {
            return Err(AppError::General(format!("'{until}' is not a day")));
        }
        map.insert("until".into(), Value::from(until));
    }
    // Sync settles two copies of a JSON file by this stamp.
    map.insert("metadata".into(), json!({ "updated_at": chrono::Utc::now().to_rfc3339() }));

    let id = uuid::Uuid::new_v4().to_string();
    let dir = Path::new(vault_path).join(QUIET_DIR);
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_string_pretty(&body)?)
        .map_err(AppError::Io)?;
    Ok(Hush {
        id,
        subject: subject.clone(),
        until: until.map(str::trim).filter(|u| !u.is_empty()).map(str::to_string),
        made,
    })
}

/// Let the app speak about this again.
///
/// The id is a file name this module wrote, which is a uuid. Anything else is
/// refused rather than joined onto a path.
pub fn remove_hush(vault_path: &str, id: &str) -> AppResult<()> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(AppError::General(format!("'{id}' is not a hush")));
    }
    match std::fs::remove_file(Path::new(vault_path).join(QUIET_DIR).join(format!("{id}.json"))) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Io(e)),
    }
}

// ─── The set in force ────────────────────────────────────────────────

type DirSignature = Vec<(String, u64, Option<SystemTime>)>;

struct Cached {
    connection: usize,
    changes: u64,
    vault: String,
    today: String,
    dir: DirSignature,
    quiet: Arc<Quiet>,
}

static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();

fn dir_signature(vault_path: &str) -> DirSignature {
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join(QUIET_DIR)) else {
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

/// The hushes in force, worked out again only when the vault, a hush file or
/// the day has changed since the last time anyone asked.
///
/// The day is part of the key because an expiry passes without anything being
/// written: a hush that ran out at midnight has to stop holding on its own.
pub fn current(db: &DbBridge, vault_path: &str) -> AppResult<Arc<Quiet>> {
    let today = crate::timeline::when::iso(chrono::Local::now().date_naive());
    let connection = db.conn() as *const rusqlite::Connection as usize;
    let changes = db.conn().total_changes();
    let dir = dir_signature(vault_path);
    let cache = CACHE.get_or_init(|| Mutex::new(None));

    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if hit.connection == connection
            && hit.changes == changes
            && hit.vault == vault_path
            && hit.today == today
            && hit.dir == dir
        {
            return Ok(hit.quiet.clone());
        }
    }

    let quiet = Arc::new(Quiet::read(db, vault_path, &today)?);
    *cache.lock().unwrap_or_else(|e| e.into_inner()) = Some(Cached {
        connection,
        changes,
        vault: vault_path.to_string(),
        today: today.clone(),
        dir,
        quiet: quiet.clone(),
    });
    Ok(quiet)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(path: &str, properties: Value) -> QuietNode {
        QuietNode {
            id: path.into(),
            stable_id: path.into(),
            node_type: "person".into(),
            properties,
        }
    }

    fn hush(subject: Subject, until: Option<&str>) -> Hush {
        Hush {
            id: uuid::Uuid::new_v4().to_string(),
            subject,
            until: until.map(str::to_string),
            made: "2026-01-01".into(),
        }
    }

    #[test]
    fn a_person_with_a_death_is_quiet_without_anyone_asking() {
        let quiet = compute(
            Vec::new(),
            &[person("People/ba.md", json!({ "died_on": "2017-11-22" }))],
            "2026-09-18",
        );
        assert!(quiet.hushes_person("People/ba.md"));
        assert!(quiet.is_dead("People/ba.md"));
        assert!(quiet.hushes().is_empty(), "nobody decided it, so there is nothing to undo");
    }

    #[test]
    fn a_living_person_is_not_quiet() {
        let quiet = compute(Vec::new(), &[person("People/lan.md", json!({}))], "2026-09-18");
        assert!(!quiet.hushes_person("People/lan.md"));
        assert!(quiet.is_empty());
    }

    #[test]
    fn a_death_that_is_not_a_date_does_not_hush_anyone() {
        let quiet = compute(
            Vec::new(),
            &[person("People/x.md", json!({ "died_on": "   " })), person("People/y.md", json!({ "died_on": "maybe" }))],
            "2026-09-18",
        );
        assert!(!quiet.hushes_person("People/x.md"));
        assert!(!quiet.hushes_person("People/y.md"));
    }

    #[test]
    fn hushing_a_person_by_path_also_hushes_their_identity() {
        let people = [person("People/minh.md", json!({ "node_id": "uuid-minh" }))];
        let quiet = compute(
            vec![hush(Subject::Person { who: "People/minh.md".into() }, None)],
            &people,
            "2026-09-18",
        );
        assert!(quiet.hushes_person("People/minh.md"));
        assert!(quiet.hushes_person("uuid-minh"), "a link names them by identity");
    }

    #[test]
    fn hushing_a_person_by_identity_also_hushes_their_path() {
        let people = [person("People/minh.md", json!({ "node_id": "uuid-minh" }))];
        let quiet = compute(
            vec![hush(Subject::Person { who: "uuid-minh".into() }, None)],
            &people,
            "2026-09-18",
        );
        assert!(quiet.hushes_person("People/minh.md"));
    }

    #[test]
    fn a_hush_stops_holding_the_day_after_it_runs_out() {
        let subject = Subject::Person { who: "People/lan.md".into() };
        let still = compute(vec![hush(subject.clone(), Some("2026-09-18"))], &[], "2026-09-18");
        assert!(still.hushes_person("People/lan.md"), "the last day it holds is included");
        let over = compute(vec![hush(subject, Some("2026-09-18"))], &[], "2026-09-19");
        assert!(!over.hushes_person("People/lan.md"));
        assert!(over.hushes().is_empty(), "and it is gone from the list too");
    }

    #[test]
    fn a_hushed_stretch_covers_every_day_in_it() {
        let quiet = compute(
            vec![hush(Subject::Period { from: "2019-02".into(), to: "2019-03".into() }, None)],
            &[],
            "2026-09-18",
        );
        assert!(quiet.hushes_day("2019-02-01"));
        assert!(quiet.hushes_day("2019-03-31"), "a month's end is the last of its days");
        assert!(!quiet.hushes_day("2019-04-01"));
    }

    #[test]
    fn a_stretch_written_backwards_is_ignored_rather_than_believed() {
        let quiet = compute(
            vec![hush(Subject::Period { from: "2019-12".into(), to: "2019-01".into() }, None)],
            &[],
            "2026-09-18",
        );
        assert!(!quiet.hushes_day("2019-06-01"));
    }

    /// §16 Bước 3's gate, on the road the real feature takes: nothing else can
    /// build an [`Offered`].
    #[test]
    fn a_sealed_a_hushed_and_a_dead_person_reach_no_reminder() {
        let sealed = crate::timeline::seal::compute(
            Vec::new(),
            &[crate::timeline::seal::SealNode {
                id: "People/ex.md".into(),
                stable_id: "uuid-ex".into(),
                node_type: "person".into(),
                title: "Ex".into(),
                properties: json!({ "sealed": true, "node_id": "uuid-ex" }),
                created_at: String::new(),
            }],
            &[],
        );
        let quiet = compute(
            vec![hush(Subject::Person { who: "People/khanh.md".into() }, None)],
            &[
                person("People/khanh.md", json!({ "node_id": "uuid-khanh" })),
                person("People/ba.md", json!({ "node_id": "uuid-ba", "died_on": "2017-11-22" })),
                person("People/lan.md", json!({ "node_id": "uuid-lan" })),
            ],
            "2026-09-18",
        );

        let offered = allow(
            vec![
                Nudge::on("2020-05-14").naming(["uuid-ex".into()]),
                Nudge::on("2020-05-14").naming(["uuid-khanh".into()]),
                Nudge::on("2020-05-14").naming(["uuid-ba".into()]),
                Nudge::on("2020-05-14").naming(["uuid-lan".into()]),
            ],
            &quiet,
            &sealed,
        );

        let named: Vec<&str> =
            offered.iter().flat_map(|o| o.nudge().people.iter().map(String::as_str)).collect();
        assert_eq!(named, ["uuid-lan"], "only the person nobody refused");
    }

    #[test]
    fn one_hushed_guest_stops_the_whole_reminder() {
        let quiet = compute(
            vec![hush(Subject::Person { who: "People/khanh.md".into() }, None)],
            &[person("People/khanh.md", json!({}))],
            "2026-09-18",
        );
        let offered = allow(
            vec![Nudge::on("2020-05-14")
                .naming(["People/lan.md".into(), "People/khanh.md".into()])],
            &quiet,
            &Seals::default(),
        );
        assert!(offered.is_empty(), "a dinner cannot be half-raised");
    }

    #[test]
    fn dismissing_one_days_writing_does_not_silence_the_note_forever() {
        let quiet = compute(
            vec![hush(
                Subject::Moment { node: "Notes/2020-05-14.md".into(), day: "2020-05-14".into() },
                Some("2021-05-14"),
            )],
            &[],
            "2026-09-18",
        );
        assert!(quiet.is_empty(), "a year later it has run out on its own");
    }

    #[test]
    fn a_dismissed_moment_is_not_offered_again_while_it_holds() {
        let quiet = compute(
            vec![hush(
                Subject::Moment { node: "Notes/2020-05-14.md".into(), day: "2020-05-14".into() },
                Some("2027-05-14"),
            )],
            &[],
            "2026-09-18",
        );
        let offered = allow(
            vec![
                Nudge::on("2020-05-14").from_note("Notes/2020-05-14.md"),
                Nudge::on("2021-05-14").from_note("Notes/2021-05-14.md"),
            ],
            &quiet,
            &Seals::default(),
        );
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].nudge().day, "2021-05-14");
    }

    #[test]
    fn a_sealed_stretch_stops_a_reminder_nobody_hushed() {
        let seals = crate::timeline::seal::compute(
            vec![crate::timeline::seal::SealedPeriod {
                id: "s".into(),
                from: "2019-01-01".into(),
                to: "2019-12-31".into(),
                from_text: "2019".into(),
                to_text: "2019".into(),
            }],
            &[],
            &[],
        );
        let offered = allow(
            vec![Nudge::on("2019-06-01"), Nudge::on("2020-06-01")],
            &Quiet::default(),
            &seals,
        );
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].nudge().day, "2020-06-01");
    }

    #[test]
    fn a_hush_round_trips_through_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();

        let made = write_hush(vault, &Subject::Person { who: "People/khanh.md".into() }, Some("2027-01-01"))
            .unwrap();
        let read = read_hushes(vault);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].subject, Subject::Person { who: "People/khanh.md".into() });
        assert_eq!(read[0].until.as_deref(), Some("2027-01-01"));

        remove_hush(vault, &made.id).unwrap();
        assert!(read_hushes(vault).is_empty());
    }

    #[test]
    fn every_hush_gets_its_own_file_so_two_devices_cannot_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        write_hush(vault, &Subject::Person { who: "People/a.md".into() }, None).unwrap();
        write_hush(vault, &Subject::Person { who: "People/b.md".into() }, None).unwrap();
        assert_eq!(read_hushes(vault).len(), 2);
    }

    #[test]
    fn a_stretch_that_is_not_a_stretch_is_refused_rather_than_written() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        assert!(write_hush(
            vault,
            &Subject::Period { from: "yesterday".into(), to: "today".into() },
            None
        )
        .is_err());
        assert!(read_hushes(vault).is_empty());
    }

    #[test]
    fn a_hush_id_that_is_not_one_is_refused_rather_than_joined_onto_a_path() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        assert!(remove_hush(vault, "../../settings").is_err());
    }
}
