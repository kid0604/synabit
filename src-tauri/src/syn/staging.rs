//! Transactions Syn prepared, waiting for the person in Finance.
//!
//! Scenario 25 of `docs/syn-connectors-2026-10-08.md`: a bank statement, read
//! as a table, becomes transactions — but never straight into the ledger.
//! Finance is a kind of data the app trusts to compute with (lesson S4), and a
//! statement is somebody else's file. So Syn *stages*: Rust writes the rows it
//! was pointed at, in the columns the Finance import reads, into
//! `.synabit/staging/`, and Finance offers them through the import preview it
//! already has — which shows how each column was read, which rows it has seen
//! before, and what it could not read, and writes only when the person
//! confirms.
//!
//! Using that preview rather than a second one is the point: one place parses
//! an amount by the currency's minor units (`csv.ts`), one place decides what a
//! duplicate is. A second table of currency digits in Rust is exactly what
//! `finance_storage::Scales` warns against.
//!
//! The model chooses which column is which and may give category rules; every
//! value in the file is the table's own, copied by Rust. A mirror into
//! `staging:transaction` lands here too.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::syn::resultset;

pub const TOOL: &str = "stage_transactions";

/// The header the Finance import reads by name (`csv.ts`, `ALIASES`).
const HEADER: [&str; 6] = ["date", "type", "amount", "category", "account", "note"];

/// Rows one staging may hold. A year of a busy account, with room.
pub const MAX_ROWS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Staged {
    pub id: String,
    /// Syn's words for what this is, shown as Syn's.
    pub why: String,
    /// Where the rows came from: the table and what it was read from.
    pub source: String,
    pub rows: usize,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}

fn dir(vault_path: &str) -> std::path::PathBuf {
    std::path::Path::new(vault_path).join(".synabit").join("staging")
}

fn quote(v: &str) -> String {
    if v.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    }
}

/// A category rule: a row whose note holds `contains` is filed under `category`.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub contains: String,
    pub category: String,
}

/// Stage the rows of `handle` as transactions. `args` names the columns:
/// `date` and `amount` (needed), `note`, `category`, `type`; `account` is an
/// account's name for every row; `rules` give a category by what the note holds.
pub fn stage(
    conn: &rusqlite::Connection,
    vault_path: &str,
    scope: &str,
    conversation_id: Option<&str>,
    args: &Value,
    now: i64,
) -> Result<Value, String> {
    let field = |k: &str| args.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let handle = field("handle").ok_or("stage_transactions needs the table's `handle`.")?;
    let set = resultset::load(conn, scope, handle, now)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("There is no table '{handle}' in this conversation."))?;
    if set.exposure == resultset::Exposure::Aggregates {
        return Err("This table is totals-only for you; its rows cannot be staged from here.".into());
    }
    let names: Vec<String> = set.columns.iter().map(|c| c.name.clone()).collect();
    let column = |k: &str| -> Result<Option<usize>, String> {
        match field(k) {
            None => Ok(None),
            Some(name) => names
                .iter()
                .position(|n| crate::syn::crunch::column_key(n) == crate::syn::crunch::column_key(name))
                .map(Some)
                .ok_or_else(|| format!("`{k}`: '{name}' is not a column of {handle} ({}).", names.join(", "))),
        }
    };
    let (Some(date), Some(amount)) = (column("date")?, column("amount")?) else {
        return Err(format!("Name the `date` and `amount` columns of {handle} ({}).", names.join(", ")));
    };
    let (note, category, kind) = (column("note")?, column("category")?, column("type")?);
    let account = field("account").unwrap_or_default().to_string();
    let rules: Vec<Rule> = match args.get("rules") {
        None | Some(Value::Null) => Vec::new(),
        Some(r) => serde_json::from_value(r.clone()).map_err(|e| format!("`rules` does not read: {e}"))?,
    };
    if set.rows.len() > MAX_ROWS {
        return Err(format!("{} rows is more than one staging holds ({MAX_ROWS}); filter the table with crunch first.", set.rows.len()));
    }

    let cell = |row: &[Value], i: Option<usize>| i.and_then(|i| row.get(i)).map(text).unwrap_or_default();
    let mut ruled = 0usize;
    let mut lines = Vec::new();
    for row in &set.rows {
        let note_text = cell(row, note);
        let mut cat = cell(row, category);
        if cat.is_empty() {
            let lower = note_text.to_lowercase();
            if let Some(r) = rules.iter().find(|r| !r.contains.trim().is_empty() && lower.contains(&r.contains.trim().to_lowercase())) {
                cat = r.category.trim().to_string();
                ruled += 1;
            }
        }
        lines.push([cell(row, Some(date)), cell(row, kind), cell(row, Some(amount)), cat, account.clone(), note_text]);
    }
    let source = format!(
        "{handle}{}",
        set.source.instance.as_deref().map(|i| format!(" · {i}")).unwrap_or_default()
            + set.source.operation.as_deref().map(|o| format!(" · {o}")).unwrap_or_default().as_str()
    );
    let why: String = field("why").unwrap_or_default().chars().take(300).collect();
    let staged = write(vault_path, &lines, &why, &source, conversation_id)?;
    let id = staged.id.clone();
    Ok(json!({
        "staged": staged.rows,
        "id": id,
        "categorised_by_rules": ruled,
        "note": "Nothing is in the ledger yet. Finance shows these to the user in its import preview — how each column was read, which rows it already has, what it could not read — and adds them only when they confirm. Tell them to look there.",
    }))
}

/// File rows — date, type, amount, category, account, note — as one staging.
pub fn write(vault_path: &str, lines: &[[String; 6]], why: &str, source: &str, conversation_id: Option<&str>) -> Result<Staged, String> {
    let mut out = vec![HEADER.join(",")];
    out.extend(lines.iter().map(|f| f.iter().map(|x| quote(x)).collect::<Vec<_>>().join(",")));
    let id = format!("finance-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S-%6f"));
    let staged = Staged {
        id: id.clone(),
        why: why.to_string(),
        source: source.to_string(),
        rows: lines.len(),
        created_at: chrono::Utc::now().to_rfc3339(),
        conversation_id: conversation_id.map(str::to_string),
    };
    let d = dir(vault_path);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{id}.csv")), out.join("\r\n") + "\r\n").map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("{id}.json")), serde_json::to_vec_pretty(&staged).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(staged)
}

/// Every staging waiting in this vault, newest first.
pub fn list(vault_path: &str) -> Vec<Staged> {
    let Ok(entries) = std::fs::read_dir(dir(vault_path)) else { return Vec::new() };
    let mut out: Vec<Staged> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok())
        .filter(|s: &Staged| dir(vault_path).join(format!("{}.csv", s.id)).exists())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

fn safe_id(id: &str) -> Result<&str, String> {
    if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        Ok(id)
    } else {
        Err("not a staging id".into())
    }
}

/// The staged rows, as the CSV the Finance import reads.
pub fn text_of(vault_path: &str, id: &str) -> Result<String, String> {
    std::fs::read_to_string(dir(vault_path).join(format!("{}.csv", safe_id(id)?))).map_err(|e| e.to_string())
}

/// Done with — imported, or set aside by the person.
pub fn discard(vault_path: &str, id: &str) -> Result<(), String> {
    let id = safe_id(id)?;
    for ext in ["csv", "json"] {
        let p = dir(vault_path).join(format!("{id}.{ext}"));
        if p.exists() {
            std::fs::remove_file(p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use resultset::{from_objects, NewSet, Source};

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().expect("open");
        crate::db::local_state::ensure_attached(&c).expect("state");
        c
    }

    #[test]
    fn a_statement_is_staged_in_the_columns_finance_reads() {
        let c = conn();
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        let (columns, rows) = from_objects(&[
            json!({ "Ngày": "01/09/2026", "Mô tả": "HIGHLANDS COFFEE Q1, \"chi nhánh\"", "Số tiền": "-55.000" }),
            json!({ "Ngày": "02/09/2026", "Mô tả": "Luong thang 8", "Số tiền": "25.000.000" }),
        ]);
        let h = resultset::create(
            &c,
            NewSet { scope: "conv".into(), run_id: "run".into(), source: Source { kind: "file".into(), instance: Some("sao-ke.csv".into()), ..Default::default() }, columns, rows, missing: vec![], exposure: resultset::Exposure::Full, ttl_hours: 0 },
            1,
        )
        .expect("filed");
        let out = stage(&c, vault, "conv", Some("c1"), &json!({
            "handle": h, "date": "Ngày", "amount": "Số tiền", "note": "Mô tả", "account": "VCB",
            "rules": [{ "contains": "highlands", "category": "Ăn uống" }], "why": "Sao kê tháng 9",
        }), 2).expect("staged");
        assert_eq!(out["staged"], 2);
        assert_eq!(out["categorised_by_rules"], 1);
        let waiting = list(vault);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0].why, "Sao kê tháng 9");
        let csv = text_of(vault, &waiting[0].id).expect("text");
        let decoded = crate::syn::connector::decode::csv(&csv);
        assert_eq!(decoded[0]["amount"], "-55.000", "the file's own amount; Finance parses it by the currency");
        assert_eq!(decoded[0]["note"], "HIGHLANDS COFFEE Q1, \"chi nhánh\"", "quoted round trip");
        assert_eq!(decoded[0]["category"], "Ăn uống");
        assert_eq!(decoded[1]["category"], "");
        assert_eq!(decoded[1]["account"], "VCB");
        discard(vault, &waiting[0].id).expect("discarded");
        assert!(list(vault).is_empty());
        assert!(text_of(vault, "../../etc/passwd").is_err());
        let bad = stage(&c, vault, "conv", None, &json!({ "handle": h, "date": "Date", "amount": "Số tiền" }), 2).expect_err("no column");
        assert!(bad.contains("'Date' is not a column"), "{bad}");
    }
}
