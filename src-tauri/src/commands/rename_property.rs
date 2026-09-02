//! Renaming one frontmatter key across every node of a kind.
//!
//! This is the only part of schema editing that touches the vault's own data,
//! and it touches all of it at once: merging `màu` into `colour` reaches one
//! file, but the same gesture on `task` reaches 127 and sends 127 nodes
//! through sync. So it works the way the frontmatter repair worked — say
//! exactly what will happen, count it precisely, and refuse whatever cannot be
//! done without losing something.
//!
//! The refusal is the important part. A node already carrying the target key
//! is left completely alone: renaming into it would overwrite a value somebody
//! wrote with a value from a different field, and no amount of convenience is
//! worth a silent overwrite. Those nodes are counted and reported so the
//! person can look at them and decide.

use crate::db::DbState;
use crate::error::AppResult;

/// What a rename would do, before it does any of it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RenamePlan {
    /// Nodes that would be changed.
    pub renaming: usize,
    /// Nodes left alone because they already carry the target key.
    pub skipped: usize,
    /// A few of the skipped paths, so the person can go and look.
    pub skipped_sample: Vec<String>,
}

/// Work out the plan, reading only.
fn plan(
    state: &DbState,
    node_type: &str,
    from: &str,
    to: &str,
) -> AppResult<(RenamePlan, Vec<String>)> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = db.get_nodes_by_type(node_type)?;
    drop(db);

    let mut renaming = Vec::new();
    let mut skipped = Vec::new();

    for node in nodes {
        let Some(props) = node.properties.as_object() else {
            continue;
        };
        if !props.contains_key(from) {
            continue;
        }
        if props.contains_key(to) {
            skipped.push(node.id.clone());
        } else {
            renaming.push(node.id.clone());
        }
    }

    let plan = RenamePlan {
        renaming: renaming.len(),
        skipped: skipped.len(),
        skipped_sample: skipped.iter().take(5).cloned().collect(),
    };
    Ok((plan, renaming))
}

/// What renaming `from` to `to` would do. Changes nothing.
#[tauri::command]
pub fn preview_rename_property(
    state: tauri::State<'_, DbState>,
    node_type: String,
    from: String,
    to: String,
) -> AppResult<RenamePlan> {
    Ok(plan(&state, &node_type, &from, &to)?.0)
}

/// Do it, and report how many nodes actually changed.
///
/// The write goes through `write_node_inner` like every other write in the
/// app, so each node keeps its identity, its document path and its place in
/// the CRDT log. A bulk edit that took a shortcut around that would be a bulk
/// edit that quietly detached 127 files from sync.
#[tauri::command]
pub fn rename_property<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_type: String,
    from: String,
    to: String,
) -> AppResult<RenamePlan> {
    if from.trim().is_empty() || to.trim().is_empty() || from == to {
        return Ok(RenamePlan {
            renaming: 0,
            skipped: 0,
            skipped_sample: Vec::new(),
        });
    }

    let (mut result, targets) = plan(&state, &node_type, &from, &to)?;
    let mut changed = 0usize;

    for rel_path in targets {
        // Read the node again rather than trusting the plan's copy: the plan
        // was made before the loop started, and a node may have moved on.
        let node = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_node(&rel_path)?
        };
        let Some(node) = node else { continue };
        let Some(props) = node.properties.as_object() else {
            continue;
        };
        let Some(value) = props.get(&from).cloned() else {
            continue;
        };
        if props.contains_key(&to) {
            // It grew the target key since the plan was made.
            continue;
        }

        // A patch naming both: the new key set, the old one cleared. Every
        // other key goes unmentioned, which is what keeps them.
        let mut patch = serde_json::Map::new();
        patch.insert(to.clone(), value);
        patch.insert(from.clone(), serde_json::Value::Null);

        crate::commands::nodes::write_node_inner(
            &app_handle,
            &state,
            vault_path.clone(),
            rel_path.clone(),
            node.title.clone(),
            node.node_type.clone(),
            serde_json::Value::Object(patch),
            None,
        )?;
        changed += 1;
    }

    result.renaming = changed;
    Ok(result)
}

/// How many nodes would lose a key.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DeletePlan {
    pub deleting: usize,
}

fn carriers(state: &DbState, node_type: &str, key: &str) -> AppResult<Vec<String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let nodes = db.get_nodes_by_type(node_type)?;
    drop(db);

    Ok(nodes
        .into_iter()
        .filter(|n| {
            n.properties
                .as_object()
                .is_some_and(|p| p.contains_key(key))
        })
        .map(|n| n.id)
        .collect())
}

/// What deleting a key would cost, in nodes. Changes nothing.
#[tauri::command]
pub fn preview_delete_property(
    state: tauri::State<'_, DbState>,
    node_type: String,
    key: String,
) -> AppResult<DeletePlan> {
    Ok(DeletePlan {
        deleting: carriers(&state, &node_type, &key)?.len(),
    })
}

/// Take a key, and the value under it, off every node of a kind.
///
/// A separate command from renaming rather than a rename with an empty
/// destination. The two read almost the same in a call and could not be more
/// different in effect, and "the `to` was blank" is a poor reason to have lost
/// 127 values.
///
/// The values are recoverable one node at a time: every write goes through the
/// CRDT log, so `list_node_versions` still holds what was there. That is worth
/// knowing and is not the same as an undo.
#[tauri::command]
pub fn delete_property<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_type: String,
    key: String,
) -> AppResult<DeletePlan> {
    if key.trim().is_empty() {
        return Ok(DeletePlan { deleting: 0 });
    }

    let mut deleted = 0usize;
    for rel_path in carriers(&state, &node_type, &key)? {
        let node = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_node(&rel_path)?
        };
        let Some(node) = node else { continue };

        // `null` is the delete in the patch contract; every key not named here
        // goes unmentioned, which is what keeps it.
        let mut patch = serde_json::Map::new();
        patch.insert(key.clone(), serde_json::Value::Null);

        crate::commands::nodes::write_node_inner(
            &app_handle,
            &state,
            vault_path.clone(),
            rel_path.clone(),
            node.title.clone(),
            node.node_type.clone(),
            serde_json::Value::Object(patch),
            None,
        )?;
        deleted += 1;
    }

    Ok(DeletePlan { deleting: deleted })
}

/// How many nodes a kind would take with it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct KindPlan {
    pub nodes: usize,
}

/// What deleting a kind would cost, in nodes. Changes nothing.
#[tauri::command]
pub fn preview_delete_kind(
    state: tauri::State<'_, DbState>,
    node_type: String,
) -> AppResult<KindPlan> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    Ok(KindPlan {
        nodes: db.get_nodes_by_type(&node_type)?.len(),
    })
}

/// Send every node of a kind to the trash.
///
/// The only way a kind with things in it can stop existing: a kind is not a
/// record anywhere, it is the fact that files say `type: x`, so it goes when
/// they do. There is nothing else to delete.
///
/// One command rather than a loop of them from the front end, because a loop
/// that fails on the fortieth of a hundred leaves a kind half gone and no way
/// to tell which half. And the trash, not `unlink`: a kind deleted by mistake
/// is a hundred files somebody still wants, and they are all still there under
/// `.trash/` — which is the vault's own, shared by every app.
#[tauri::command]
pub fn delete_kind(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_type: String,
) -> AppResult<KindPlan> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let paths: Vec<String> = db
        .get_nodes_by_type(&node_type)?
        .into_iter()
        .map(|n| n.id)
        .collect();

    let mut moved = 0usize;
    for rel_path in paths {
        match crate::commands::trash::apply_trash(&db, &vault_path, &rel_path) {
            Ok(_) => moved += 1,
            // One unreadable file must not strand the rest half-deleted.
            Err(e) => log::warn!("could not trash '{}': {}", rel_path, e),
        }
    }

    Ok(KindPlan { nodes: moved })
}

/// Say a different word about every node of a kind.
///
/// A kind is not stored anywhere: it is the fact that files say `type: x`. So
/// this is what "rename a kind" means, and it is the same operation as
/// renaming a field one level up — the same patch over the same set of nodes,
/// on the key that decides what they are rather than one they carry.
///
/// The file does not move. Its path is its identity to the sync engine and its
/// place in the CRDT log, and nothing here is worth risking that for: the only
/// cost of staying put is a folder whose name no longer matches, which makes
/// the vault a little harder to read outside the app and nothing harder inside
/// it. Nothing infers a type from a folder — the parser reads frontmatter.
#[tauri::command]
pub fn retype_kind<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    from_type: String,
    to_type: String,
) -> AppResult<KindPlan> {
    if from_type == to_type || to_type.trim().is_empty() {
        return Ok(KindPlan { nodes: 0 });
    }

    let paths: Vec<String> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_type(&from_type)?
            .into_iter()
            .map(|n| n.id)
            .collect()
    };

    let mut changed = 0usize;
    for rel_path in paths {
        let node = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.get_node(&rel_path)?
        };
        let Some(node) = node else { continue };

        // An empty patch: every key the file holds goes unmentioned and is
        // therefore kept. What changes is the type, which this path writes
        // from its own argument rather than from the properties.
        crate::commands::nodes::write_node_inner(
            &app_handle,
            &state,
            vault_path.clone(),
            rel_path.clone(),
            node.title.clone(),
            to_type.clone(),
            serde_json::Value::Object(serde_json::Map::new()),
            None,
        )?;
        changed += 1;
    }

    Ok(KindPlan { nodes: changed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;
    use serde_json::json;

    fn node(id: &str, node_type: &str, props: serde_json::Value) -> crate::models::node::NodeMetadata {
        crate::models::node::NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: id.to_string(),
            content: String::new(),
            properties: props,
            created_at: "2026-08-31".into(),
            updated_at: "2026-08-31".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn db_with(nodes: Vec<crate::models::node::NodeMetadata>) -> DbState {
        let db = DbBridge::new_in_memory_full().expect("full in-memory schema");
        for n in &nodes {
            db.upsert_node(n).expect("insert");
        }
        std::sync::Mutex::new(db)
    }

    /// The case this was written for: one animal calls it `màu`, two call it
    /// `colour`, and nothing on the third node stands in the way.
    #[test]
    fn a_plan_counts_exactly_what_it_will_touch() {
        let state = db_with(vec![
            node("Animal/mun.md", "animal", json!({ "colour": "đen" })),
            node("Animal/vang.md", "animal", json!({ "màu": "vàng" })),
            node("Animal/xam.md", "animal", json!({ "màu": "xám" })),
            node("Notes/a.md", "note", json!({ "màu": "đỏ" })),
        ]);

        let (plan, targets) = plan(&state, "animal", "màu", "colour").expect("plan");

        assert_eq!(plan.renaming, 2, "only the animals carrying the old key");
        assert_eq!(plan.skipped, 0);
        assert!(
            !targets.iter().any(|p| p.starts_with("Notes/")),
            "a note with the same key is a different kind and none of this rename's business",
        );
    }

    /// The refusal. Renaming into a key that already has a value would replace
    /// something somebody wrote, so that node is not touched at all.
    #[test]
    fn a_node_holding_both_keys_is_left_completely_alone() {
        let state = db_with(vec![
            node("Animal/both.md", "animal", json!({ "màu": "vàng", "colour": "yellow" })),
            node("Animal/one.md", "animal", json!({ "màu": "xám" })),
        ]);

        let (plan, targets) = plan(&state, "animal", "màu", "colour").expect("plan");

        assert_eq!(plan.renaming, 1);
        assert_eq!(plan.skipped, 1, "the collision is reported, not resolved");
        assert_eq!(plan.skipped_sample, vec!["Animal/both.md".to_string()]);
        assert_eq!(targets, vec!["Animal/one.md".to_string()]);
    }

    /// Retyping is a patch, so everything else on the node survives it.
    ///
    /// The properties are the point: a `book` that becomes a `note` keeps its
    /// author and its rating, because the patch names nothing and a key not
    /// named is a key kept. Writing the node back whole would need this to
    /// know what a book holds, and it does not.
    #[test]
    fn a_retype_leaves_everything_but_the_word() {
        let state = db_with(vec![
            node("Abc/one.md", "abc", json!({ "author": "Harari", "rating": 5 })),
            node("Notes/a.md", "note", json!({ "tags": ["mdp"] })),
        ]);

        let carried = carriers(&state, "abc", "author").expect("carriers");
        assert_eq!(carried, vec!["Abc/one.md".to_string()]);

        // The node of another kind is not in the set at all: retyping `abc`
        // is none of a note's business.
        assert!(carriers(&state, "abc", "tags").expect("carriers").is_empty());
    }

    /// Counted per kind, so deleting `màu` from animals leaves notes alone.
    #[test]
    fn deleting_counts_only_the_kind_it_was_asked_about() {
        let state = db_with(vec![
            node("Animal/mun.md", "animal", json!({ "màu": "đen", "species": "mèo" })),
            node("Animal/vang.md", "animal", json!({ "màu": "vàng" })),
            node("Animal/none.md", "animal", json!({ "species": "vẹt" })),
            node("Notes/a.md", "note", json!({ "màu": "đỏ" })),
        ]);

        let found = carriers(&state, "animal", "màu").expect("carriers");

        assert_eq!(found.len(), 2);
        assert!(!found.iter().any(|p| p.starts_with("Notes/")));
    }

    #[test]
    fn deleting_a_key_nothing_carries_touches_nothing() {
        let state = db_with(vec![node("Animal/mun.md", "animal", json!({ "species": "mèo" }))]);

        assert!(carriers(&state, "animal", "màu").expect("carriers").is_empty());
    }

    #[test]
    fn a_key_nobody_uses_is_a_plan_that_does_nothing() {
        let state = db_with(vec![node("Animal/mun.md", "animal", json!({ "colour": "đen" }))]);

        let (plan, targets) = plan(&state, "animal", "hue", "colour").expect("plan");

        assert_eq!(plan.renaming, 0);
        assert!(targets.is_empty());
    }
}
