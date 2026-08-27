//! Taking a Finance file apart so that two devices can add to it at once.
//!
//! # The problem
//!
//! A month of the ledger is one file holding a list of transactions. Two
//! devices each recording a purchase produce two versions of that one file,
//! and a `.json` file is resolved whole: the later `metadata.updated_at` wins
//! and the other device's purchase is gone. No warning, no conflict copy — the
//! money was simply never spent as far as the vault is concerned.
//!
//! Character-level merging, which is what a note gets, is not the answer
//! either. `scenarios::a_list_inside_one_file_is_merged_by_character_not_by_entry`
//! pins why: two edits to a list of objects converge on a text assembled from
//! both, which is neither version and need not parse.
//!
//! # The shape
//!
//! The same one `node_document` uses for a note, for the same reason. A note
//! splits into prose (a text CRDT) and frontmatter (a map, so `status` resolves
//! to one of its two values rather than to a blend of their letters). A Finance
//! file splits into three maps:
//!
//! - `_fin_head` — `title`, `type`, the body, and which key holds the rows.
//! - `_fin_meta` — every other `metadata` key, JSON-encoded, one per entry.
//! - `_fin_rows` — the transactions or debts, one entry each, keyed by id.
//!
//! Two devices adding different transactions write different keys, so both
//! survive. Two devices editing the same transaction write one key, so one of
//! the two versions wins whole — which is the right answer for a row nobody can
//! half-mean. And two devices where one adds a category while the other adds an
//! account touch different `_fin_meta` keys, so that stops being a conflict too.
//!
//! # Why the values are JSON text
//!
//! Exactly as in `node_document`: a single scalar per entry is what makes a
//! map's last-writer-wins behaviour meaningful. Storing a transaction as a
//! nested Loro structure would let two devices merge *inside* one transaction
//! and produce an amount neither of them entered.
//!
//! # Why the rows come back sorted
//!
//! Two devices have to rebuild the same bytes from the same map, and a map has
//! no order. Sorting by id is the only ordering both can agree on without
//! exchanging anything. Nothing downstream depends on the stored order —
//! `calc.ts` sorts by date for every view — so the cost is one reordering the
//! first time a month is merged.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// The Loro containers this module owns.
pub const HEAD: &str = "_fin_head";
pub const META: &str = "_fin_meta";
pub const ROWS: &str = "_fin_rows";

/// Head keys. Named rather than positional so a reader of a stored document
/// can tell what it is looking at.
pub const KEY_TITLE: &str = "title";
pub const KEY_TYPE: &str = "type";
pub const KEY_BODY: &str = "body";
pub const KEY_ROWS_KEY: &str = "rows_key";

/// A Finance file taken apart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FinanceParts {
    pub title: String,
    pub node_type: String,
    /// The file's `content` field, which Finance leaves empty but does write.
    pub body: String,
    /// Which `metadata` key holds the rows, if this file has rows at all.
    /// `finance_config` does not; it is scalars all the way down.
    pub rows_key: Option<String>,
    /// Every other `metadata` key, as its JSON encoding.
    pub meta: BTreeMap<String, String>,
    /// Each row by its id, as its JSON encoding.
    pub rows: BTreeMap<String, String>,
}

/// Which `metadata` key holds the rows of a given kind of Finance node.
pub fn rows_key_for(node_type: &str) -> Option<&'static str> {
    match node_type {
        "finance_month" => Some("transactions"),
        "finance_debts" => Some("debts"),
        "finance_recurring" => Some("rules"),
        // `finance_config` is accounts, categories, budgets and a currency —
        // all of them whole-value settings rather than a growing list of
        // records, and all of them already separate `_fin_meta` keys.
        _ => None,
    }
}

/// Whether this path is one this module is willing to take apart.
///
/// Decided from the path because both the sending and the receiving side have
/// to reach the same answer, and the receiving side is holding a snapshot
/// rather than a file. A path that passes this and then turns out not to be a
/// Finance node fails in `split`, and the caller falls back to what it did
/// before.
pub fn is_structured(rel_path: &str) -> bool {
    let normalised = rel_path.replace('\\', "/");
    normalised.starts_with("Finance/") && normalised.ends_with(".json")
}

/// Take a Finance file apart, or decline to.
///
/// Returns `None` for anything that is not one of the Finance node types, is
/// not an object, or holds a row without a usable id — a row that cannot be
/// keyed cannot be merged by key, and guessing a key would silently drop
/// whichever rows collided.
pub fn split(file: &str) -> Option<FinanceParts> {
    let parsed: Value = serde_json::from_str(file).ok()?;
    let object = parsed.as_object()?;

    let node_type = object.get(KEY_TYPE)?.as_str()?.to_string();
    if !matches!(
        node_type.as_str(),
        "finance_month" | "finance_debts" | "finance_config" | "finance_recurring"
    ) {
        return None;
    }

    let title = object
        .get(KEY_TITLE)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let body = object
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let metadata = object.get("metadata")?.as_object()?;
    let rows_key = rows_key_for(&node_type).map(str::to_string);

    let mut meta = BTreeMap::new();
    let mut rows = BTreeMap::new();

    for (key, value) in metadata {
        if Some(key.as_str()) == rows_key.as_deref() {
            for row in value.as_array()? {
                let id = row.get("id").and_then(Value::as_str)?;
                if id.is_empty() {
                    return None;
                }
                rows.insert(id.to_string(), serde_json::to_string(row).ok()?);
            }
            continue;
        }
        meta.insert(key.clone(), serde_json::to_string(value).ok()?);
    }

    Some(FinanceParts { title, node_type, body, rows_key, meta, rows })
}

/// Put a Finance file back together.
///
/// The result is laid out exactly the way `write_node_file` lays one out — a
/// pretty-printed object whose keys are ordered, because `serde_json` here has
/// no `preserve_order` feature. That is what keeps a merged file from reading
/// as a change to the next ordinary save.
pub fn rebuild(parts: &FinanceParts) -> String {
    let mut metadata = Map::new();
    for (key, encoded) in &parts.meta {
        let value = serde_json::from_str(encoded).unwrap_or(Value::Null);
        metadata.insert(key.clone(), value);
    }

    if let Some(rows_key) = &parts.rows_key {
        let rows: Vec<Value> = parts
            .rows
            .values()
            .filter_map(|encoded| serde_json::from_str(encoded).ok())
            .collect();
        metadata.insert(rows_key.clone(), Value::Array(rows));
    }

    let file = serde_json::json!({
        "title": parts.title,
        "type": parts.node_type,
        "metadata": Value::Object(metadata),
        "content": parts.body,
    });

    serde_json::to_string_pretty(&file).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn month(rows: &str) -> String {
        format!(
            r#"{{"title":"Month 08/2026","type":"finance_month","content":"",
                 "metadata":{{"financeSchema":2,"updated_at":"2026-08-15T10:00:00Z",
                 "transactions":[{rows}]}}}}"#
        )
    }

    const TX_A: &str = r#"{"id":"tx-a","amount":4500,"type":"expense","note":"lunch"}"#;
    const TX_B: &str = r#"{"id":"tx-b","amount":12000,"type":"expense","note":"petrol"}"#;

    #[test]
    fn a_month_is_split_into_its_rows() {
        let parts = split(&month(&format!("{TX_A},{TX_B}"))).expect("splits");

        assert_eq!(parts.node_type, "finance_month");
        assert_eq!(parts.rows_key.as_deref(), Some("transactions"));
        assert_eq!(parts.rows.keys().collect::<Vec<_>>(), vec!["tx-a", "tx-b"]);
        assert!(parts.meta.contains_key("updated_at"));
        assert!(
            !parts.meta.contains_key("transactions"),
            "the rows must not also sit in the scalar map"
        );
    }

    /// The property everything else rests on: what comes out is what went in.
    #[test]
    fn a_file_survives_a_round_trip() {
        let original = month(&format!("{TX_A},{TX_B}"));
        let once = rebuild(&split(&original).expect("splits"));
        let twice = rebuild(&split(&once).expect("splits again"));

        assert_eq!(once, twice, "rebuilding is not stable");

        let parsed: Value = serde_json::from_str(&once).unwrap();
        assert_eq!(parsed["metadata"]["transactions"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["metadata"]["financeSchema"], 2);
        assert_eq!(parsed["title"], "Month 08/2026");
    }

    /// Two devices must rebuild identical bytes from the same set of rows,
    /// whatever order those rows arrived in.
    #[test]
    fn the_order_rows_arrive_in_does_not_change_the_file() {
        let one = split(&month(&format!("{TX_A},{TX_B}"))).unwrap();
        let other = split(&month(&format!("{TX_B},{TX_A}"))).unwrap();

        assert_eq!(rebuild(&one), rebuild(&other));
    }

    #[test]
    fn a_config_has_no_rows_and_is_all_settings() {
        let file = r#"{"title":"Finance Config","type":"finance_config","content":"",
            "metadata":{"currency":"VND","accounts":[{"id":"acc-1","name":"Cash"}],
            "incomeCategories":["Salary"]}}"#;
        let parts = split(file).expect("splits");

        assert_eq!(parts.rows_key, None);
        assert!(parts.rows.is_empty());
        // Each setting is its own entry, so two devices changing two different
        // settings are not in conflict at all.
        assert!(parts.meta.contains_key("currency"));
        assert!(parts.meta.contains_key("accounts"));
        assert!(parts.meta.contains_key("incomeCategories"));
    }

    #[test]
    fn a_debts_ledger_is_keyed_by_debt() {
        let file = r#"{"title":"Debts Ledger","type":"finance_debts","content":"",
            "metadata":{"debts":[{"id":"d1","totalAmount":500},{"id":"d2","totalAmount":900}]}}"#;
        let parts = split(file).expect("splits");

        assert_eq!(parts.rows_key.as_deref(), Some("debts"));
        assert_eq!(parts.rows.len(), 2);
    }

    #[test]
    fn an_empty_month_is_still_a_month() {
        let parts = split(&month("")).expect("splits");
        assert!(parts.rows.is_empty());
        assert_eq!(parts.rows_key.as_deref(), Some("transactions"));
    }

    // ---- things it declines to touch ---------------------------------------

    /// Guessing a key for a row that has none would drop every row that
    /// happened to collide, which is the failure this module exists to stop.
    #[test]
    fn a_row_without_an_id_is_refused() {
        assert_eq!(split(&month(r#"{"amount":4500}"#)), None);
        assert_eq!(split(&month(r#"{"id":"","amount":4500}"#)), None);
    }

    #[test]
    fn something_that_is_not_a_finance_node_is_refused() {
        assert_eq!(
            split(r#"{"title":"Board","type":"whiteboard","metadata":{},"content":""}"#),
            None
        );
        assert_eq!(split("not json at all"), None);
        assert_eq!(split(r#"{"type":"finance_month"}"#), None, "no metadata");
    }

    #[test]
    fn only_finance_json_paths_are_taken_apart() {
        assert!(is_structured("Finance/2026-08.json"));
        assert!(is_structured("Finance/Config.json"));
        assert!(is_structured("Finance\\Debts.json"), "windows separators");

        assert!(!is_structured("Notes/plan.md"));
        assert!(!is_structured("Whiteboards/plan.whiteboard.json"));
        assert!(!is_structured("Finance/notes.md"));
        assert!(!is_structured("Archive/Finance/2026-08.json"));
    }

    #[test]
    fn the_rows_key_depends_on_the_kind_of_node() {
        assert_eq!(rows_key_for("finance_month"), Some("transactions"));
        assert_eq!(rows_key_for("finance_debts"), Some("debts"));
        assert_eq!(rows_key_for("finance_recurring"), Some("rules"));
        assert_eq!(rows_key_for("finance_config"), None);
        assert_eq!(rows_key_for("note"), None);
    }

    /// Two devices each adding a repeating bill keep both, for the same reason
    /// two transactions do: different keys of the same map.
    #[test]
    fn a_recurring_rule_is_a_row_like_any_other() {
        let file = r#"{"title":"Recurring","type":"finance_recurring","content":"",
            "metadata":{"rules":[{"id":"rule-1","recurrence":"monthly"}]}}"#;
        let parts = split(file).expect("splits");

        assert_eq!(parts.rows_key.as_deref(), Some("rules"));
        assert_eq!(parts.rows.keys().collect::<Vec<_>>(), vec!["rule-1"]);
    }
}
