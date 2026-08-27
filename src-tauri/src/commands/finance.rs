//! Writing one transaction without rewriting the month it lives in.
//!
//! # What this replaces
//!
//! Saving a transaction used to mean sending the whole month back: the screen
//! held the array in memory, spliced the new row into it, and wrote the result.
//! That is a read-modify-write against a file other things also write, and
//! `ledger.ts` said as much — "safe here because one process owns the vault; it
//! would not be if that ever stopped being true."
//!
//! P2P sync is exactly that stopping being true. A month refreshed by a sync
//! between the screen's last read and its next save was overwritten by the
//! copy the screen had been holding, and every row that arrived in between went
//! with it.
//!
//! Merging row by row in the CRDT — `sync::core::finance_document` — fixes the
//! two-device case. It does not fix this one: a write that omits a row is
//! indistinguishable from a deletion, so the stale array would still take the
//! new rows out. The fix for that is to stop sending arrays.
//!
//! # What a caller sends instead
//!
//! The rows it changed and the ids it removed. Everything else in the file is
//! read from the file, here, immediately before the write.

use serde_json::{Map, Value};

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::path_utils;
use crate::sync::core::finance_document;

/// The id of a row, if it has a usable one.
fn row_id(row: &Value) -> Option<&str> {
    row.get("id").and_then(Value::as_str).filter(|id| !id.is_empty())
}

/// Apply a set of row changes to the rows already in the file.
///
/// Existing rows keep their position, so an ordinary edit does not reshuffle
/// the file; new rows go on the end in the order they were given. A removal and
/// an upsert naming the same id is a contradiction, and the removal wins —
/// deleting something is the harder action to have asked for by accident.
pub fn apply_row_changes(existing: &[Value], upserts: &[Value], removals: &[String]) -> Vec<Value> {
    let removed: std::collections::HashSet<&str> = removals.iter().map(String::as_str).collect();

    let mut out: Vec<Value> = Vec::with_capacity(existing.len() + upserts.len());
    for row in existing {
        match row_id(row) {
            Some(id) if removed.contains(id) => continue,
            // A row with no id cannot be addressed, so it can only be carried.
            _ => out.push(row.clone()),
        }
    }

    for row in upserts {
        let Some(id) = row_id(row) else { continue };
        if removed.contains(id) {
            continue;
        }
        match out.iter().position(|existing| row_id(existing) == Some(id)) {
            Some(index) => out[index] = row.clone(),
            None => out.push(row.clone()),
        }
    }

    out
}

/// What a file says about the units its amounts are in.
///
/// Absent means schema 1 — whole units — because the marker did not exist when
/// those files were written.
fn schema_of(metadata: &Map<String, Value>) -> u64 {
    metadata
        .get("financeSchema")
        .and_then(Value::as_u64)
        .unwrap_or(1)
}

/// The `metadata` object of a Finance file on disk, or an empty one.
fn metadata_on_disk(abs_path: &std::path::Path) -> Map<String, Value> {
    std::fs::read_to_string(abs_path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|file| file.get("metadata").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

/// Change some rows of a Finance file, leaving the rest of it alone.
///
/// `upserts` are whole rows, each carrying its `id`; `removals` are ids. The
/// file's other `metadata` keys are read from disk and written back untouched,
/// except for whatever `metadata` the caller explicitly sends — which is how
/// the schema marker gets stamped.
///
/// Goes on to `write_node_file` rather than writing bytes itself, so that a
/// row-level save is indistinguishable downstream from any other save: same
/// CRDT containers, same database row, same search index, same events.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn upsert_finance_rows(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
    title: String,
    node_type: String,
    upserts: Vec<Value>,
    removals: Vec<String>,
    metadata: Option<Value>,
) -> AppResult<()> {
    let rows_key = finance_document::rows_key_for(&node_type).ok_or_else(|| {
        AppError::General(format!("'{node_type}' has no rows to change"))
    })?;

    let abs_path = path_utils::resolve_safe_path(&vault_path, &rel_path)?;
    let mut properties = metadata_on_disk(&abs_path);

    // A row in minor units must not join rows that are still in whole units.
    // Merging them would leave one file holding both scales, and stamping the
    // marker on the result would tell every later reader that the old rows are
    // minor units too — every one of them a hundredth of what was spent.
    //
    // The caller's job is to repair the vault first; this only refuses to be
    // the thing that corrupts it.
    if abs_path.exists() {
        let on_disk = schema_of(&properties);
        let incoming = metadata
            .as_ref()
            .and_then(Value::as_object)
            .map(schema_of)
            .unwrap_or(on_disk);

        if incoming > on_disk {
            return Err(AppError::General(format!(
                "'{rel_path}' still stores whole units; open Finance once so it can be updated \
                 before writing to it"
            )));
        }
    }

    let existing: Vec<Value> = properties
        .get(rows_key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let rows = apply_row_changes(&existing, &upserts, &removals);
    properties.insert(rows_key.to_string(), Value::Array(rows));

    // Whatever the caller wants said about the file as a whole, on top of what
    // is already there.
    if let Some(Value::Object(extra)) = metadata {
        for (key, value) in extra {
            properties.insert(key, value);
        }
    }

    super::nodes::write_node_file(
        app_handle,
        state,
        vault_path,
        rel_path,
        title,
        node_type,
        Value::Object(properties),
        Some(String::new()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: &str, amount: i64) -> Value {
        json!({ "id": id, "amount": amount, "type": "expense" })
    }

    fn ids(rows: &[Value]) -> Vec<String> {
        rows.iter()
            .map(|r| row_id(r).unwrap_or("<none>").to_string())
            .collect()
    }

    #[test]
    fn a_new_row_goes_on_the_end() {
        let out = apply_row_changes(&[row("tx-1", 100)], &[row("tx-2", 200)], &[]);
        assert_eq!(ids(&out), vec!["tx-1", "tx-2"]);
    }

    #[test]
    fn an_edited_row_stays_where_it_was() {
        let existing = vec![row("tx-1", 100), row("tx-2", 200), row("tx-3", 300)];
        let out = apply_row_changes(&existing, &[row("tx-2", 250)], &[]);

        assert_eq!(ids(&out), vec!["tx-1", "tx-2", "tx-3"]);
        assert_eq!(out[1]["amount"], 250);
    }

    #[test]
    fn a_removed_row_is_gone_and_nothing_else_moves() {
        let existing = vec![row("tx-1", 100), row("tx-2", 200), row("tx-3", 300)];
        let out = apply_row_changes(&existing, &[], &["tx-2".to_string()]);

        assert_eq!(ids(&out), vec!["tx-1", "tx-3"]);
    }

    /// The point of the whole exercise: a caller that knows about one row
    /// cannot take out the rows it has never heard of.
    #[test]
    fn rows_the_caller_did_not_mention_are_untouched() {
        let arrived_from_sync = vec![row("tx-from-b", 999), row("tx-from-c", 888)];
        let out = apply_row_changes(&arrived_from_sync, &[row("tx-from-a", 100)], &[]);

        assert_eq!(ids(&out), vec!["tx-from-b", "tx-from-c", "tx-from-a"]);
    }

    #[test]
    fn removing_something_that_is_not_there_is_not_an_error() {
        let out = apply_row_changes(&[row("tx-1", 100)], &[], &["tx-9".to_string()]);
        assert_eq!(ids(&out), vec!["tx-1"]);
    }

    #[test]
    fn removing_and_upserting_the_same_row_removes_it() {
        let out = apply_row_changes(&[row("tx-1", 100)], &[row("tx-1", 500)], &["tx-1".to_string()]);
        assert!(out.is_empty(), "{out:?}");
    }

    /// A row with no id predates the app assigning them. It cannot be edited
    /// or deleted through here, but losing it would be worse.
    #[test]
    fn a_row_with_no_id_is_carried_rather_than_dropped() {
        let existing = vec![json!({ "amount": 100 }), row("tx-1", 200)];
        let out = apply_row_changes(&existing, &[row("tx-2", 300)], &["tx-1".to_string()]);

        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["amount"], 100);
        assert_eq!(row_id(&out[1]), Some("tx-2"));
    }

    #[test]
    fn an_upsert_without_an_id_is_ignored_rather_than_appended() {
        let out = apply_row_changes(&[row("tx-1", 100)], &[json!({ "amount": 5 })], &[]);
        assert_eq!(ids(&out), vec!["tx-1"]);
    }

    #[test]
    fn several_changes_at_once_all_land() {
        let existing = vec![row("tx-1", 100), row("tx-2", 200)];
        let out = apply_row_changes(
            &existing,
            &[row("tx-1", 150), row("tx-3", 300)],
            &["tx-2".to_string()],
        );

        assert_eq!(ids(&out), vec!["tx-1", "tx-3"]);
        assert_eq!(out[0]["amount"], 150);
    }

    #[test]
    fn a_file_with_no_marker_is_read_as_whole_units() {
        assert_eq!(schema_of(&Map::new()), 1);
        assert_eq!(
            schema_of(json!({ "financeSchema": 2 }).as_object().unwrap()),
            2
        );
    }

    #[test]
    fn only_the_node_types_that_have_rows_are_accepted() {
        assert_eq!(finance_document::rows_key_for("finance_month"), Some("transactions"));
        assert_eq!(finance_document::rows_key_for("finance_debts"), Some("debts"));
        assert_eq!(finance_document::rows_key_for("finance_config"), None);
    }
}
