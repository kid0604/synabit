//! What the person says is big, whatever the arithmetic says.
//!
//! §4.5, third thing to say plainly: **big is not important**. A ten-word line
//! — *"bố gọi, giọng khác mọi khi"* — can be the heaviest thing in a year, and
//! every measurable signal puts it near the bottom: one day long, nobody
//! linked, no photographs, ten words. [`super::magnitude`] is not wrong about
//! it. It is measuring size, and size is not what that line has.
//!
//! So there has to be a way to say so by hand, and the zoom has to obey it.
//! A pin lifts one thing above any threshold: at the widest view, where twenty
//! things are shown out of hundreds, a pinned line is one of the twenty.
//!
//! # Why the pin is not stored on the event
//!
//! An event lives in the index, which is tier 3 and rebuilt from the vault
//! whenever derivation changes (§4.7). A pin is a decision, which is tier 2 and
//! must outlive every rebuild and reach every device. So it is a file in the
//! vault, keyed by the note it came from and the day it happened — the pair
//! that survives an edit, for the same reason [`super::quiet::Subject::Moment`]
//! is keyed that way.

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use super::store::Event;
use crate::error::{AppError, AppResult};

/// Where pins live, one file each, so two devices never write the same file.
pub const PINNED_DIR: &str = "Timeline/pinned";

/// One thing the person lifted by hand.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Pin {
    pub id: String,
    /// The note it was written in.
    pub node: String,
    /// The day it happened, `YYYY-MM-DD`.
    pub day: String,
}

/// Everything pinned, as a set that can answer about an event.
#[derive(Debug, Default)]
pub struct Pinned {
    held: HashSet<(String, String)>,
    pins: Vec<Pin>,
}

impl Pinned {
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    pub fn pins(&self) -> &[Pin] {
        &self.pins
    }

    /// Whether this event is one the person lifted.
    pub fn holds(&self, event: &Event) -> bool {
        let node = event.container_node.as_deref().unwrap_or(&event.node_id);
        self.held.contains(&(node.to_string(), event.happened_from.clone()))
    }
}

/// Keep the biggest `room` things, and everything pinned whatever its size.
///
/// Returns what survived, in the order it was lived, and how many were left
/// out. §4.5's third rule is the whole reason this is not a plain `sort` on
/// magnitude: a pinned line sits above every unpinned one however small it
/// measures, and the room is widened rather than a pin dropped when there are
/// more pins than places.
pub fn keep(mut items: Vec<Event>, room: usize, pinned: &Pinned) -> (Vec<Event>, usize) {
    let before = items.len();
    let held = items.iter().filter(|item| pinned.holds(item)).count();
    items.sort_by(|a, b| {
        pinned
            .holds(b)
            .cmp(&pinned.holds(a))
            .then_with(|| b.magnitude.total_cmp(&a.magnitude))
            .then_with(|| a.happened_from.cmp(&b.happened_from))
            .then_with(|| a.id.cmp(&b.id))
    });
    items.truncate(room.max(held));
    items.sort_by(|a, b| {
        a.happened_from
            .cmp(&b.happened_from)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.id.cmp(&b.id))
    });
    let left_out = before - items.len();
    (items, left_out)
}

pub fn read(vault_path: &str) -> Pinned {
    let Ok(entries) = std::fs::read_dir(Path::new(vault_path).join(PINNED_DIR)) else {
        return Pinned::default();
    };
    let mut pinned = Pinned::default();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        let Ok(value) = std::fs::read_to_string(&path).ok().as_deref().map(serde_json::from_str::<Value>).transpose()
        else {
            continue;
        };
        let Some(value) = value else { continue };
        let text = |key: &str| {
            value.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
        };
        let (Some(node), Some(day)) = (text("node"), text("day")) else { continue };
        pinned.held.insert((node.to_string(), day.to_string()));
        pinned.pins.push(Pin { id, node: node.to_string(), day: day.to_string() });
    }
    pinned.pins.sort_by(|a, b| a.day.cmp(&b.day).then_with(|| a.id.cmp(&b.id)));
    pinned
}

/// Lift something by hand.
pub fn write(vault_path: &str, node: &str, day: &str) -> AppResult<Pin> {
    let (node, day) = (node.trim(), day.trim());
    if node.is_empty() || super::when::parse(day).is_none() {
        return Err(AppError::General(format!("'{node}' on '{day}' is not something to pin")));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let dir = Path::new(vault_path).join(PINNED_DIR);
    std::fs::create_dir_all(&dir).map_err(AppError::Io)?;
    let body = json!({
        "node": node,
        "day": day,
        // Sync settles two copies of a JSON file by this stamp.
        "metadata": { "updated_at": chrono::Utc::now().to_rfc3339() },
    });
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_string_pretty(&body)?)
        .map_err(AppError::Io)?;
    Ok(Pin { id, node: node.to_string(), day: day.to_string() })
}

/// Let it fall back to its measured size.
///
/// The id is a file name this module wrote, which is a uuid. Anything else is
/// refused rather than joined onto a path.
pub fn remove(vault_path: &str, id: &str) -> AppResult<()> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(AppError::General(format!("'{id}' is not a pin")));
    }
    match std::fs::remove_file(Path::new(vault_path).join(PINNED_DIR).join(format!("{id}.json"))) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(AppError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(node: &str, day: &str, container: Option<&str>) -> Event {
        Event {
            id: format!("{node}#moment"),
            kind: "moment".into(),
            node_id: node.into(),
            node_type: "note".into(),
            title: "Bố gọi, giọng khác mọi khi".into(),
            node_title: String::new(),
            links: Vec::new(),
            happened_from: day.into(),
            happened_to: day.into(),
            precision: "day".into(),
            time_source: "frontmatter".into(),
            source: "user".into(),
            magnitude: 1.4,
            container_node: container.map(String::from),
            props: Value::Null,
            shape: crate::timeline::derive::Shape::Occasion,
        }
    }

    #[test]
    fn a_pin_round_trips_through_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        assert!(read(vault).is_empty());

        let pin = write(vault, "Notes/2026-03-02.md", "2026-03-02").unwrap();
        let pinned = read(vault);
        assert!(pinned.holds(&event("Notes/2026-03-02.md", "2026-03-02", None)));
        assert_eq!(pinned.pins().len(), 1);

        remove(vault, &pin.id).unwrap();
        assert!(read(vault).is_empty());
    }

    #[test]
    fn a_moment_is_pinned_through_the_note_that_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        write(vault, "Notes/2026-03-02.md", "2026-03-02").unwrap();
        let pinned = read(vault);
        assert!(
            pinned.holds(&event("Notes/2026-03-02.md#moment#0", "2026-03-02", Some("Notes/2026-03-02.md"))),
            "the container is what the pin names"
        );
    }

    #[test]
    fn a_different_day_of_the_same_note_is_not_pinned() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        write(vault, "Notes/a.md", "2026-03-02").unwrap();
        let pinned = read(vault);
        assert!(!pinned.holds(&event("Notes/a.md", "2026-03-03", None)));
    }

    #[test]
    fn each_pin_gets_its_own_file_so_two_devices_cannot_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        write(vault, "Notes/a.md", "2026-03-02").unwrap();
        write(vault, "Notes/b.md", "2026-04-02").unwrap();
        assert_eq!(read(vault).pins().len(), 2);
    }

    #[test]
    fn something_that_is_not_a_day_is_refused_rather_than_written() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        assert!(write(vault, "Notes/a.md", "hôm qua").is_err());
        assert!(write(vault, "  ", "2026-03-02").is_err());
        assert!(read(vault).is_empty());
    }

    #[test]
    fn a_pin_id_that_is_not_one_is_refused_rather_than_joined_onto_a_path() {
        let dir = tempfile::tempdir().unwrap();
        assert!(remove(dir.path().to_str().unwrap(), "../../settings").is_err());
    }

    fn sized(id: &str, day: &str, magnitude: f64) -> Event {
        let mut e = event(id, day, None);
        e.magnitude = magnitude;
        e
    }

    #[test]
    fn the_widest_view_keeps_the_biggest_and_says_how_many_it_dropped() {
        let items: Vec<Event> =
            (1..=50).map(|n| sized(&format!("Notes/{n}.md"), "2026-01-01", n as f64)).collect();
        let (kept, left_out) = keep(items, 20, &Pinned::default());
        assert_eq!(kept.len(), 20);
        assert_eq!(left_out, 30);
        assert!(kept.iter().all(|e| e.magnitude >= 31.0), "the biggest twenty");
    }

    #[test]
    fn a_pinned_line_beats_everything_larger_than_it() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        write(vault, "Notes/quiet.md", "2026-01-01").unwrap();
        let pinned = read(vault);

        let mut items: Vec<Event> =
            (1..=50).map(|n| sized(&format!("Notes/{n}.md"), "2026-02-01", n as f64)).collect();
        // The ten-word line: smallest thing here by every measurable signal.
        items.push(sized("Notes/quiet.md", "2026-01-01", 1.4));

        let (kept, _) = keep(items, 20, &pinned);
        assert_eq!(kept.len(), 20);
        assert!(
            kept.iter().any(|e| e.node_id == "Notes/quiet.md"),
            "§4.5 rule 3: big is not important"
        );
    }

    #[test]
    fn more_pins_than_places_widens_the_view_rather_than_dropping_a_pin() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_str().unwrap();
        for n in 1..=25 {
            write(vault, &format!("Notes/{n}.md"), "2026-01-01").unwrap();
        }
        let pinned = read(vault);
        let items: Vec<Event> =
            (1..=25).map(|n| sized(&format!("Notes/{n}.md"), "2026-01-01", 1.0)).collect();

        let (kept, left_out) = keep(items, 20, &pinned);
        assert_eq!(kept.len(), 25, "a pin the person made is never the thing that gives way");
        assert_eq!(left_out, 0);
    }

    /// Open question 1, measured — what a "whole life" view actually shows.
    ///
    ///   SYN_PROBE_CACHE=... cargo test --lib -- --ignored the_twenty_biggest --nocapture
    #[test]
    #[ignore = "needs a copy of a real vault cache; run by hand"]
    fn the_twenty_biggest() {
        use std::sync::Mutex;

        let path = std::env::var("SYN_PROBE_CACHE").expect("a copy of a vault_cache.db");
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = crate::db::DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = crate::timeline::store::TimelineStore::open_in_memory().unwrap();
        crate::timeline::store::catch_up(&cache, &mut timeline).expect("built");

        let today = chrono::Local::now().date_naive();
        let all = timeline.all_items(today).unwrap();
        let (kept, left_out) = keep(all.clone(), 20, &Pinned::default());

        let span_of = |e: &Event| {
            let day = |t: &str| chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").ok();
            match (day(&e.happened_from), day(&e.happened_to)) {
                (Some(a), Some(b)) => (b - a).num_days(),
                _ => 0,
            }
        };
        let long = kept.iter().filter(|e| span_of(e) > 31).count();

        eprintln!("\n═══ the whole life, twenty of {} ═══", all.len());
        eprintln!("  left out:            {left_out}");
        eprintln!("  spans over a month:  {long} of {}", kept.len());
        let mut by_size = kept.clone();
        by_size.sort_by(|a, b| b.magnitude.total_cmp(&a.magnitude));
        for e in &by_size {
            eprintln!(
                "  {:>5.2}  {:>5}d  {:<14} {}",
                e.magnitude,
                span_of(e),
                e.kind,
                e.title.chars().take(52).collect::<String>()
            );
        }
    }

    /// §16 Bước 9's gate: write the whole index out, once before a refactor
    /// and once after, and diff the two files.
    ///
    ///   SYN_PROBE_CACHE=... SYN_SNAPSHOT_OUT=/tmp/before.txt \\
    ///     cargo test --lib -- --ignored write_the_whole_index_out --nocapture
    #[test]
    #[ignore = "needs a copy of a real vault cache; run by hand"]
    fn write_the_whole_index_out() {
        use std::sync::Mutex;

        let path = std::env::var("SYN_PROBE_CACHE").expect("a copy of a vault_cache.db");
        let out = std::env::var("SYN_SNAPSHOT_OUT").expect("where to write it");
        let conn = rusqlite::Connection::open(&path).expect("the cache");
        let db = crate::db::DbBridge::init_with_conn(conn).expect("its schema");
        let cache = Mutex::new(db);
        let mut timeline = crate::timeline::store::TimelineStore::open_in_memory().unwrap();
        crate::timeline::store::catch_up(&cache, &mut timeline).expect("built");

        let mut lines = timeline.snapshot_lines().unwrap();

        // The frame is derived at read time, so it belongs in the picture too.
        let far = chrono::NaiveDate::from_ymd_opt(9999, 12, 31).unwrap();
        let items = timeline.all_items(far).unwrap();
        let nodes: Vec<crate::timeline::frame::FrameNode> = {
            let db = cache.lock().unwrap();
            let mut stmt = db
                .conn()
                .prepare("SELECT id, COALESCE(stable_id, id), COALESCE(created_at, '') FROM nodes")
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok(crate::timeline::frame::FrameNode {
                        id: r.get(0)?,
                        stable_id: r.get(1)?,
                        created_at: r.get(2)?,
                    })
                })
                .unwrap();
            rows.flatten().collect()
        };
        let frame = crate::timeline::frame::build(&items, &nodes, far);
        let mut seen: Vec<(&String, &String)> = frame.first_seen.iter().collect();
        seen.sort();
        for (id, day) in seen {
            lines.push(format!("first_seen\t{id}\t{day}"));
        }
        let mut died: Vec<(&String, &String)> = frame.died_on.iter().collect();
        died.sort();
        for (id, day) in died {
            lines.push(format!("died\t{id}\t{day}"));
        }

        std::fs::write(&out, lines.join("\n")).unwrap();
        eprintln!("\n{} lines -> {out}", lines.len());
    }
}
