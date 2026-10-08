//! Renaming and removing a tag across the vault.
//!
//! Both are bulk edits of one frontmatter key, so they take the road
//! `rename_property` takes: a patch through `write_node_inner`, which writes
//! the file atomically, keeps its identity and its body, and records the change
//! in the CRDT log so it syncs. They used to edit the file themselves — finding
//! the frontmatter's end with the first `---` anywhere in it, writing in place,
//! dropping every error and never telling sync — and a tag rename was the one
//! edit in the app another device never heard about.

use serde_json::Value;

use crate::db::DbState;
use crate::error::{AppError, AppResult};

#[tauri::command]
pub fn get_all_tags(state: tauri::State<'_, DbState>) -> AppResult<Vec<(String, i64)>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_all_tags_with_counts()
}

#[tauri::command(async)]
pub fn rename_tag<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    old_tag: String,
    new_tag: String,
) -> AppResult<()> {
    let new_tag = new_tag.trim().to_string();
    if new_tag.is_empty() || new_tag == old_tag {
        return Ok(());
    }
    retag(&app_handle, &state, &vault_path, &old_tag, |tags| {
        renamed(tags, &old_tag, &new_tag)
    })
    .map(|_| ())
}

#[tauri::command(async)]
pub fn delete_tag<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    tag: String,
) -> AppResult<()> {
    retag(&app_handle, &state, &vault_path, &tag, |tags| removed(tags, &tag)).map(|_| ())
}

/// Apply `change` to the `tags` of every note carrying `tag`, returning how
/// many were written.
///
/// The index says which notes to look at; the file says what their tags are.
/// The two can disagree — the index is a cache, and a sync may have landed
/// since — and the file is the one being written, so it is the one read. A
/// note whose file no longer carries the tag is left alone.
///
/// Every note is attempted. One that cannot be written does not stop the
/// rest, and does not disappear either: the call fails naming it, so a rename
/// that only partly happened says so instead of reporting success.
fn retag<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    tag: &str,
    change: impl Fn(&Value) -> Option<Value>,
) -> AppResult<usize> {
    let nodes = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_tag(tag)?
    };

    let mut written = 0;
    let mut failed = Vec::new();
    for node in nodes {
        // A `file` node describes a file the vault does not own; its id is a
        // UUID, and writing to it would create a note of that name.
        if !crate::commands::nodes::is_disk_backed_id(&node.id) {
            continue;
        }
        let Ok(abs_path) = crate::path_utils::resolve_safe_path(vault_path, &node.id) else {
            continue;
        };
        if !abs_path.is_file() {
            continue;
        }
        let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let on_disk = crate::commands::nodes::existing_properties(&abs_path, ext);
        let Some(tags) = on_disk.get("tags").and_then(&change) else {
            continue;
        };

        let mut patch = serde_json::Map::new();
        patch.insert("tags".to_string(), tags);
        match crate::commands::nodes::write_node_inner(
            app_handle,
            state,
            vault_path.to_string(),
            node.id.clone(),
            node.title.clone(),
            node.node_type.clone(),
            Value::Object(patch),
            None,
        ) {
            Ok(()) => written += 1,
            Err(e) => failed.push(format!("{}: {e}", node.id)),
        }
    }

    if failed.is_empty() {
        Ok(written)
    } else {
        Err(AppError::General(format!(
            "{} note(s) could not be changed ({} were): {}",
            failed.len(),
            written,
            failed.join("; ")
        )))
    }
}

/// `tags` with `from` replaced by `to`, or `None` if it does not carry `from`.
///
/// A note that already has `to` keeps one copy of it, in the first place
/// either stood.
fn renamed(tags: &Value, from: &str, to: &str) -> Option<Value> {
    match tags {
        Value::String(s) if s == from => Some(Value::String(to.to_string())),
        Value::Array(items) if items.iter().any(|t| t.as_str() == Some(from)) => {
            let mut out: Vec<Value> = Vec::with_capacity(items.len());
            for t in items {
                let t = if t.as_str() == Some(from) {
                    Value::String(to.to_string())
                } else {
                    t.clone()
                };
                if t.as_str() != Some(to) || !out.iter().any(|o| o.as_str() == Some(to)) {
                    out.push(t);
                }
            }
            Some(Value::Array(out))
        }
        _ => None,
    }
}

/// `tags` without `tag`, or `None` if it does not carry it.
fn removed(tags: &Value, tag: &str) -> Option<Value> {
    match tags {
        Value::String(s) if s == tag => Some(Value::Array(Vec::new())),
        Value::Array(items) if items.iter().any(|t| t.as_str() == Some(tag)) => Some(Value::Array(
            items
                .iter()
                .filter(|t| t.as_str() != Some(tag))
                .cloned()
                .collect(),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;
    use serde_json::json;
    use tauri::Manager;

    fn app() -> tauri::AppHandle<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle().clone();
        handle.manage(DbState::new(DbBridge::new_in_memory_full().unwrap()));
        handle
    }

    /// A note written the way the app writes one, so it is indexed, has an
    /// identity and has a CRDT document.
    fn write(
        app: &tauri::AppHandle<tauri::test::MockRuntime>,
        vault: &str,
        rel: &str,
        props: Value,
        body: &str,
    ) {
        crate::commands::nodes::write_node_inner(
            app,
            &app.state::<DbState>(),
            vault.to_string(),
            rel.to_string(),
            "Họp nhóm".to_string(),
            "note".to_string(),
            props,
            Some(body.to_string()),
        )
        .unwrap();
    }

    /// The frontmatter keys in the order the file has them.
    fn keys(text: &str) -> Vec<String> {
        let fm = text.strip_prefix("---\n").unwrap().split("\n---\n").next().unwrap();
        fm.lines()
            .filter(|l| !l.starts_with(' ') && !l.starts_with('-') && l.contains(':'))
            .map(|l| l.split(':').next().unwrap().to_string())
            .collect()
    }

    #[test]
    fn renaming_a_tag_keeps_the_rest_of_the_note_and_reaches_the_crdt() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let app = app();
        let body = "Nội dung\n\n---\n\nsau đường kẻ";
        let props =
            json!({"tags": ["việc", "nhà"], "status": "todo", "summary": "a --- b", "zeta": 1});
        // Twice: the app's writer puts keys in its own order (title, type,
        // then by name) and the first save of a new note still has `node_id`
        // where identity stamping put it. From the second save on, a save
        // moves nothing — and a tag edit is a save.
        write(&app, &vault, "Notes/hop.md", props.clone(), body);
        write(&app, &vault, "Notes/hop.md", props, body);
        let before = std::fs::read_to_string(dir.path().join("Notes/hop.md")).unwrap();

        let state = app.state::<DbState>();
        let n = retag(&app, &state, &vault, "việc", |t| renamed(t, "việc", "công-việc")).unwrap();
        assert_eq!(n, 1);

        let after = std::fs::read_to_string(dir.path().join("Notes/hop.md")).unwrap();
        assert_eq!(keys(&after), keys(&before), "no key moved");
        assert!(after.contains("a --- b"), "a value holding a fence is not the fence");
        assert!(after.ends_with(body), "the body, rule and all, is untouched");

        // Before the lock below: resolving the identity takes it too.
        let vault_id = crate::sync::core::identity::load_or_register_vault_identity(&app, &vault)
            .unwrap()
            .vault_id
            .to_string();

        let db = state.lock().unwrap();
        let node = db.get_node("Notes/hop.md").unwrap().unwrap();
        assert_eq!(node.properties["tags"], json!(["công-việc", "nhà"]));
        assert_eq!(node.properties["node_id"], {
            let first = gray_matter::Matter::<gray_matter::engine::YAML>::new()
                .parse::<Value>(&before)
                .unwrap()
                .data
                .unwrap();
            first["node_id"].clone()
        }, "the note keeps its identity");

        // What sync will send is what is on disk.
        let doc_id = db.get_node_id_by_path(&vault_id, "Notes/hop.md").unwrap().unwrap();
        let doc = db.get_crdt_doc(&vault_id, &doc_id).unwrap();
        assert_eq!(crate::sync::core::crdt::node_text(&doc), after);
    }

    #[test]
    fn deleting_a_tag_leaves_other_notes_and_other_tags_alone() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let app = app();
        write(&app, &vault, "Notes/a.md", json!({"tags": ["x", "y"]}), "a");
        write(&app, &vault, "Notes/b.md", json!({"tags": ["y"]}), "b");
        let untouched = std::fs::read_to_string(dir.path().join("Notes/b.md")).unwrap();

        let state = app.state::<DbState>();
        assert_eq!(retag(&app, &state, &vault, "x", |t| removed(t, "x")).unwrap(), 1);

        let db = state.lock().unwrap();
        assert_eq!(db.get_node("Notes/a.md").unwrap().unwrap().properties["tags"], json!(["y"]));
        assert_eq!(std::fs::read_to_string(dir.path().join("Notes/b.md")).unwrap(), untouched);
    }

    /// A note that cannot be written is reported, and the others still change.
    #[cfg(unix)]
    #[test]
    fn a_note_that_cannot_be_written_is_an_error_not_a_silence() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().canonicalize().unwrap().to_string_lossy().to_string();
        let app = app();
        write(&app, &vault, "Locked/a.md", json!({"tags": ["x"]}), "a");
        write(&app, &vault, "Notes/b.md", json!({"tags": ["x"]}), "b");
        let locked = dir.path().join("Locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();

        let state = app.state::<DbState>();
        let result = retag(&app, &state, &vault, "x", |t| renamed(t, "x", "z"));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let err = result.expect_err("the locked note is reported").to_string();
        assert!(err.contains("Locked/a.md"), "{err}");
        let db = state.lock().unwrap();
        assert_eq!(db.get_node("Notes/b.md").unwrap().unwrap().properties["tags"], json!(["z"]));
    }

    #[test]
    fn a_rename_into_a_tag_already_there_leaves_one() {
        assert_eq!(renamed(&json!(["a", "b", "a"]), "a", "b"), Some(json!(["b"])));
        assert_eq!(renamed(&json!(["b", "a"]), "a", "b"), Some(json!(["b"])));
        assert_eq!(renamed(&json!("a"), "a", "b"), Some(json!("b")));
        assert_eq!(renamed(&json!(["c"]), "a", "b"), None);
        assert_eq!(removed(&json!("a"), "a"), Some(json!([])));
        assert_eq!(removed(&json!(["c"]), "a"), None);
    }
}
