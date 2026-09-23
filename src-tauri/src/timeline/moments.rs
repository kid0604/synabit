//! A moment is a file of its own.
//!
//! `Moments/<uuid>.md`, `type: moment`, the moment in its frontmatter and
//! whatever the person cares to write about it in the body. The design is §15.3
//! of `docs/timeline-extract-v3-2026-09-22.md`.
//!
//! # Why not in the note it came from
//!
//! Moments used to be kept as a `moments[]` list in the frontmatter of the note
//! they were read from, or of the day's note when written by hand. That tied a
//! moment's life to a note's: deleting the note deleted every moment kept in
//! it, without a word, and changing one meant writing into somebody else's
//! file. A file of its own outlives its source, and is changed by changing it.
//!
//! # Why a uuid, not a title
//!
//! A node's id is its path. Named after its title or its day, a moment would
//! change identity every time either was corrected — which is what reviewing a
//! proposal is for — and every link to it would have to be rewritten. Titles
//! repeat, too: lunch with the same friend is most weeks' lunch. So the name is
//! a uuid and never changes, and there is no folder per year for the same
//! reason: the year can be corrected.
//!
//! # Not a node in the graph
//!
//! The graph and the Nodes tab are what the person wrote. A moment is what was
//! read out of that, and drawing it beside its source would draw every note
//! twice. So `moment` is one of the kinds left out unless asked for by name —
//! see `db::internal` — and the reader never reads one back (`extract::never`).

use serde_json::{Map, Value};

use crate::db::DbState;
use crate::error::{AppError, AppResult};

/// Where moments live, at the top of the vault.
pub const FOLDER: &str = "Moments";

/// The type a moment's frontmatter declares.
pub const TYPE: &str = "moment";

/// The file a moment with this id is.
pub fn path_of(id: &str) -> String {
    format!("{FOLDER}/{id}.md")
}

/// A uuid that comes out the same wherever it is worked out.
///
/// Two devices moving the same note's moments, or keeping the same proposal,
/// must arrive at the same file rather than at two copies of one moment.
pub fn settled_id(parts: &[&str]) -> String {
    let hash = blake3::hash(parts.join("\0").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    uuid::Builder::from_random_bytes(bytes).into_uuid().to_string()
}

/// A moment's frontmatter, from an entry in the shape `moments[]` used.
///
/// That shape is also what a kept proposal and the compose box produce, so
/// there is one way in. `id` is the file's name rather than a key in it; every
/// other key is kept as written, including ones this version has no meaning
/// for.
///
/// `written_in` is the note the moment was read from, `block` the block of it
/// (`timeline::blocks::key`), so a later edit of those very words can be told
/// from an edit anywhere else (§15). A moment written by hand has neither.
pub fn frontmatter(
    entry: &Map<String, Value>,
    written_in: Option<&str>,
    quote: Option<&str>,
    block: Option<&str>,
) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("type".into(), Value::String(TYPE.into()));
    for (key, value) in entry {
        if key != "id" && key != "type" {
            out.insert(key.clone(), value.clone());
        }
    }
    let origin = if entry.contains_key("extract") { "extract" } else { "manual" };
    out.entry("origin").or_insert_with(|| Value::String(origin.into()));
    if let Some(node) = written_in.filter(|node| !node.is_empty()) {
        let mut source = Map::new();
        source.insert("node".into(), Value::String(node.into()));
        if let Some(quote) = quote.map(str::trim).filter(|q| !q.is_empty()) {
            source.insert("quote".into(), Value::String(quote.into()));
        }
        if let Some(block) = block.map(str::trim).filter(|b| !b.is_empty()) {
            source.insert("block".into(), Value::String(block.into()));
        }
        out.insert("source".into(), Value::Object(source));
    }
    out
}

/// Write one moment to its file, and return the file.
///
/// A file already there is added to, not replaced: another device may have
/// moved the same moment first, and the person may have written in it since.
pub fn write<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    id: &str,
    frontmatter: Map<String, Value>,
) -> AppResult<String> {
    let path = path_of(id);
    let title = frontmatter
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .ok_or_else(|| AppError::General("A moment with no title is not a moment".into()))?
        .to_string();
    crate::commands::nodes::write_node_inner(
        app_handle,
        state,
        vault_path.to_string(),
        path.clone(),
        title,
        TYPE.to_string(),
        Value::Object(frontmatter),
        None,
    )?;
    Ok(path)
}

/// A moment as it is kept, for holding against what its source says now.
#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    /// Its file: `Moments/<uuid>.md`.
    pub path: String,
    pub title: String,
    /// The note it was read from, and the words it rests on.
    pub source_node: Option<String>,
    pub quote: Option<String>,
    /// The block of that note, as it was when this was read.
    pub block: Option<String>,
    /// Fields the person wrote themselves. Never proposed over (§15.2).
    pub hand: Vec<String>,
    /// Everything in its frontmatter.
    pub fields: Map<String, Value>,
}

/// A kept moment from its path and frontmatter. For tests and for [`kept`].
pub fn kept_from(path: String, fields: Map<String, Value>) -> Option<Kept> {
    Kept::of(path, fields)
}

impl Kept {
    fn of(path: String, fields: Map<String, Value>) -> Option<Kept> {
        let text = |key: &str| fields.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from);
        let source = fields.get("source");
        Some(Kept {
            title: text("title")?,
            source_node: source.and_then(|s| s.get("node")).and_then(Value::as_str).map(String::from),
            quote: source.and_then(|s| s.get("quote")).and_then(Value::as_str).map(String::from),
            block: source.and_then(|s| s.get("block")).and_then(Value::as_str).map(String::from),
            hand: fields
                .get("hand")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(Value::as_str).map(String::from).collect())
                .unwrap_or_default(),
            path,
            fields,
        })
    }
}

/// Every moment kept in the vault.
pub fn kept(db: &crate::db::DbBridge) -> AppResult<Vec<Kept>> {
    let mut stmt = db
        .conn()
        .prepare("SELECT id, properties FROM nodes WHERE node_type = ?1 ORDER BY id")
        .map_err(|e| AppError::General(format!("moments: {e}")))?;
    let rows: Vec<(String, String)> = stmt
        .query_map([TYPE], |r| Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))
        .map_err(|e| AppError::General(format!("moments: {e}")))?
        .flatten()
        .collect();
    Ok(rows
        .into_iter()
        .filter_map(|(path, properties)| match serde_json::from_str::<Value>(&properties) {
            Ok(Value::Object(fields)) => Kept::of(path, fields),
            _ => None,
        })
        .collect())
}

/// Change a kept moment, leaving every field the person wrote themselves.
///
/// The person's decisions are theirs; what the reader offers is the fields it
/// still owns. A title somebody rewrote is never proposed over again (§15.2),
/// and `source` is refreshed so the moment points at the words as they are now.
pub fn apply<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    moment: &Kept,
    changes: Map<String, Value>,
    source: Option<(&str, &str, &str)>,
) -> AppResult<()> {
    let mut fields = Map::new();
    for (key, value) in changes {
        if moment.hand.contains(&key) || key == "id" || key == "type" || key == "hand" || key == "source" {
            continue;
        }
        fields.insert(key, value);
    }
    if let Some((node, block, quote)) = source {
        fields.insert(
            "source".into(),
            serde_json::json!({ "node": node, "block": block, "quote": quote }),
        );
    }
    write_fields(app_handle, state, vault_path, moment, fields)
}

/// Change a kept moment because the person said so.
///
/// Unlike [`apply`], nothing is held back: these *are* their words. Every
/// field they touch joins `hand`, so no later reading proposes over it — the
/// same promise a field edited in the review carries (§15.2).
pub fn edit<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    moment: &Kept,
    changes: Map<String, Value>,
) -> AppResult<()> {
    let mut fields = Map::new();
    let mut theirs = moment.hand.clone();
    for (key, value) in changes {
        if matches!(key.as_str(), "id" | "type" | "hand" | "source" | "extract" | "origin") {
            continue;
        }
        // Left as it was is not a decision; changed is.
        if moment.fields.get(&key) != Some(&value) && !theirs.contains(&key) {
            theirs.push(key.clone());
        }
        fields.insert(key, value);
    }
    if fields.is_empty() {
        return Ok(());
    }
    theirs.sort();
    theirs.dedup();
    fields.insert("hand".into(), serde_json::json!(theirs));
    write_fields(app_handle, state, vault_path, moment, fields)
}

fn write_fields<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    moment: &Kept,
    fields: Map<String, Value>,
) -> AppResult<()> {
    if fields.is_empty() {
        return Ok(());
    }
    let title = fields
        .get("title")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| moment.title.clone());
    crate::commands::nodes::write_node_inner(
        app_handle,
        state,
        vault_path.to_string(),
        moment.path.clone(),
        title,
        TYPE.to_string(),
        Value::Object(fields),
        None,
    )
}

/// One kept moment, by its file.
pub fn one(db: &crate::db::DbBridge, path: &str) -> AppResult<Option<Kept>> {
    Ok(kept(db)?.into_iter().find(|moment| moment.path == path))
}

/// What moving moments out of notes did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Moved {
    pub notes: usize,
    pub moments: usize,
    /// Notes left as they were, and why. Their moments stay where they are and
    /// are still read from there.
    pub failed: Vec<String>,
}

/// Move every `moments[]` entry still kept in a note into a file of its own,
/// and take the list out of the note.
///
/// Runs after every vault scan, so a note that arrives by sync from a device
/// on an older version is moved the next time this one opens. Costs one query
/// when there is nothing to move.
///
/// A note is only emptied once each of its moments is safely in its own file.
/// If any one fails, the note keeps its list — both are read, and a moment
/// that is in both places is the same file on the next attempt, not a copy.
pub fn move_out_of_notes<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
) -> AppResult<Moved> {
    let holding: Vec<(String, String, String)> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = db
            .conn()
            .prepare(
                "SELECT id, title, node_type FROM nodes
                 WHERE json_type(properties, '$.moments') = 'array'
                   AND json_array_length(properties, '$.moments') > 0
                 ORDER BY id",
            )
            .map_err(|e| AppError::General(format!("moments to move: {e}")))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| AppError::General(format!("moments to move: {e}")))?;
        rows.flatten().collect()
    };

    let mut moved = Moved::default();
    for (note, title, node_type) in holding {
        match move_one(app_handle, state, vault_path, &note, &title, &node_type) {
            Ok(0) => {}
            Ok(count) => {
                moved.notes += 1;
                moved.moments += count;
            }
            Err(e) => moved.failed.push(format!("{note}: {e}")),
        }
    }
    if moved.moments > 0 || !moved.failed.is_empty() {
        log::info!(
            "moments: moved {} out of {} note(s) into {FOLDER}/; {} note(s) left as they were",
            moved.moments,
            moved.notes,
            moved.failed.len()
        );
        for failure in &moved.failed {
            log::warn!("moments: not moved: {failure}");
        }
    }
    Ok(moved)
}

fn move_one<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    note: &str,
    title: &str,
    node_type: &str,
) -> AppResult<usize> {
    // From the file, not the cache: what is on disk is what gets rewritten.
    let abs = crate::path_utils::resolve_safe_path(vault_path, note).map_err(|e| AppError::General(e.to_string()))?;
    let on_disk = crate::commands::nodes::existing_properties(&abs, "md");
    let Some(Value::Array(entries)) = on_disk.get("moments") else {
        return Ok(0);
    };

    let mut count = 0;
    for (n, entry) in entries.iter().enumerate() {
        let Value::Object(entry) = entry else {
            continue;
        };
        let written = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string();
        if written("title").is_empty() {
            // Nothing to call it by: not a moment, and not ours to invent a
            // name for. It stays in the note, which is not emptied below.
            return Err(AppError::General(format!("moment {n} has no title")));
        }
        let id = match written("id") {
            id if uuid::Uuid::parse_str(&id).is_ok() => id,
            _ => settled_id(&["moved", note, &n.to_string(), &written("title"), &written("happened")]),
        };
        // A moment written into the day's note by hand was not *read* from it;
        // one kept from a proposal was.
        let written_in = entry.contains_key("extract").then_some(note);
        write(app_handle, state, vault_path, &id, frontmatter(entry, written_in, None, None))?;
        count += 1;
    }

    let title = on_disk
        .get("title")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| title.to_string());
    let node_type = on_disk
        .get("type")
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| node_type.to_string());
    crate::commands::nodes::write_node_inner(
        app_handle,
        state,
        vault_path.to_string(),
        note.to_string(),
        title,
        node_type,
        serde_json::json!({ "moments": null }),
        None,
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;
    use tauri::Manager;

    use super::*;
    use crate::db::DbBridge;

    fn app() -> tauri::App<tauri::test::MockRuntime> {
        let cache: DbState = Mutex::new(DbBridge::new_in_memory_full().unwrap());
        tauri::test::mock_builder()
            .manage(cache)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("a mock app")
    }

    fn vault() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(dir.path()).unwrap().to_string_lossy().to_string();
        (dir, path)
    }

    fn read(vault_path: &str, rel: &str) -> Map<String, Value> {
        let abs = crate::path_utils::resolve_safe_path(vault_path, rel).unwrap();
        crate::commands::nodes::existing_properties(&abs, "md")
    }

    fn moment_files(vault_path: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(std::path::Path::new(vault_path).join(FOLDER))
            .map(|dir| dir.flatten().map(|f| f.file_name().to_string_lossy().to_string()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    #[test]
    fn the_same_parts_are_the_same_id_and_a_real_uuid() {
        let a = settled_id(&["moved", "Notes/a.md", "0"]);
        assert_eq!(a, settled_id(&["moved", "Notes/a.md", "0"]));
        assert_ne!(a, settled_id(&["moved", "Notes/a.md", "1"]));
        assert!(uuid::Uuid::parse_str(&a).is_ok(), "{a}");
    }

    #[test]
    fn a_moment_keeps_every_key_and_says_where_it_came_from() {
        let entry: Map<String, Value> = serde_json::from_value(json!({
            "id": "6f3c9a2e-0000-4000-8000-000000000000",
            "title": "Họp UAT v2", "happened": "2026-07-21", "people": ["People/yen.md", "Đức"],
            "extract": "x123", "mood": "mệt"
        }))
        .unwrap();
        let written = frontmatter(&entry, Some("Notes/uat.md"), Some("Chiều họp UAT v2"), Some("b3f1"));
        assert_eq!(written["type"], "moment");
        assert_eq!(written["origin"], "extract");
        assert_eq!(written["source"], json!({ "node": "Notes/uat.md", "quote": "Chiều họp UAT v2", "block": "b3f1" }));
        assert_eq!(written["mood"], "mệt", "a key nobody here understands is still somebody's");
        assert!(!written.contains_key("id"), "the id is the file's name");

        let by_hand: Map<String, Value> = serde_json::from_value(json!({ "title": "Ăn tối", "happened": "2026-07-21" })).unwrap();
        let written = frontmatter(&by_hand, None, None, None);
        assert_eq!(written["origin"], "manual");
        assert!(!written.contains_key("source"));
    }

    /// The migration, end to end: moments leave the note for files of their
    /// own, the note keeps everything else, and running it again moves nothing.
    #[test]
    fn moments_move_out_of_the_note_and_nothing_else_does() {
        let app = app();
        let (_dir, vault_path) = vault();
        let managed = app.state::<DbState>();
        let state = managed.inner();

        let kept_id = "6f3c9a2e-1111-4111-8111-111111111111";
        crate::commands::nodes::write_node_inner(
            app.handle(),
            state,
            vault_path.clone(),
            "Notes/2026-07-21.md".into(),
            "2026-07-21".into(),
            "note".into(),
            json!({
                "date": "2026-07-21",
                "tags": ["daily"],
                "moments": [
                    { "id": kept_id, "title": "Họp UAT v2", "happened": "2026-07-21", "people": ["Đức"], "extract": "x1" },
                    { "title": "Ăn tối với Nga", "happened": "2026-07-21", "where": "Kim Mã" }
                ]
            }),
            Some("Chiều họp UAT v2 với Đức.".into()),
        )
        .unwrap();

        let moved = move_out_of_notes(app.handle(), state, &vault_path).unwrap();
        assert_eq!((moved.notes, moved.moments, moved.failed.len()), (1, 2, 0), "{moved:?}");

        let files = moment_files(&vault_path);
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(files.contains(&format!("{kept_id}.md")), "a moment with an id keeps it: {files:?}");

        let kept = read(&vault_path, &path_of(kept_id));
        assert_eq!(kept["type"], "moment");
        assert_eq!(kept["title"], "Họp UAT v2");
        assert_eq!(kept["people"], json!(["Đức"]));
        assert_eq!(kept["source"], json!({ "node": "Notes/2026-07-21.md" }), "read from that note");

        let other = files.iter().find(|f| !f.starts_with(kept_id)).unwrap();
        let by_hand = read(&vault_path, &format!("{FOLDER}/{other}"));
        assert_eq!(by_hand["where"], "Kim Mã");
        assert_eq!(by_hand["origin"], "manual");
        assert!(!by_hand.contains_key("source"), "written by hand, not read from the day");

        let note = read(&vault_path, "Notes/2026-07-21.md");
        assert!(!note.contains_key("moments"), "{note:?}");
        assert_eq!(note["tags"], json!(["daily"]), "the note's own keys stay");
        let body = std::fs::read_to_string(std::path::Path::new(&vault_path).join("Notes/2026-07-21.md")).unwrap();
        assert!(body.contains("Chiều họp UAT v2 với Đức."), "and its text: {body}");

        let again = move_out_of_notes(app.handle(), state, &vault_path).unwrap();
        assert_eq!(again, Moved::default(), "nothing left to move");
        assert_eq!(moment_files(&vault_path).len(), 2);
    }

    /// A note whose list cannot all be moved keeps all of it.
    #[test]
    fn a_note_is_only_emptied_when_every_moment_made_it() {
        let app = app();
        let (_dir, vault_path) = vault();
        let managed = app.state::<DbState>();
        let state = managed.inner();

        crate::commands::nodes::write_node_inner(
            app.handle(),
            state,
            vault_path.clone(),
            "Notes/a.md".into(),
            "a".into(),
            "note".into(),
            json!({ "moments": [{ "title": "Có tên", "happened": "2026-07-21" }, { "happened": "2026-07-22" }] }),
            None,
        )
        .unwrap();

        let moved = move_out_of_notes(app.handle(), state, &vault_path).unwrap();
        assert_eq!(moved.failed.len(), 1, "{moved:?}");
        let note = read(&vault_path, "Notes/a.md");
        assert_eq!(note["moments"].as_array().map(Vec::len), Some(2), "{note:?}");

        // What did make it is the same file next time, not a second one.
        let first = moment_files(&vault_path);
        move_out_of_notes(app.handle(), state, &vault_path).unwrap();
        assert_eq!(moment_files(&vault_path), first);
    }
}
