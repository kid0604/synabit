//! `crunch`: work something out over a table Syn brought back.
//!
//! The model asks for a sum and Rust adds. That is the whole idea, and the
//! reason for it is in the 2026-10-02 eval: two of three models given thirty
//! amounts in plain view added them up wrong. A table larger than the prompt
//! budget is not shown at all (`resultset`), so the only way to a number about
//! it is through here — which means every number about it was computed, and
//! the run says which pipeline computed it.
//!
//! # The language
//!
//! Not a new one: the pipeline half of the query grammar
//! (`docs/query-grammar-2026-09-20.md`, `query.rs`, `pipeline.rs`), the same
//! stages the query bar runs — `where`, `stats … by`, `sort`, `head`, `top` —
//! plus `eval` and the `sum`/`avg`/`min`/`max`/`distinct` tallies that came
//! with this. One grammar, so a stage cannot come to mean two things.
//!
//! Column names are matched in lower case, because the grammar reads its words
//! that way; a table's columns are lowered on the way in, and the summary the
//! model was shown already names them so.
//!
//! # What it cannot do
//!
//! Reach anything. It reads one result set of this conversation and files
//! another; no network, no vault write, no model. `explode` and `ask`, the
//! two stages that would reach out, are refused here by construction —
//! `Around::bare` has nothing to reach with.

use serde_json::{json, Value};

use crate::db::{QueryResult, QueryRow};
use crate::error::{AppError, AppResult};
use crate::query::Stage;
use crate::syn::resultset::{self, ColType, Column, Exposure, NewSet, ResultSet, Source};

pub const TOOL: &str = "crunch";

/// How much of a crunched answer goes back inline before it is filed in turn.
pub const INLINE_BUDGET: usize = 6_000;

/// Which conversation's tables a run may reach.
///
/// The conversation when the run has one — a run that resumed after a
/// question is a new run of the same conversation, and must still find the
/// tables the first one brought back — otherwise the run itself.
pub fn scope_for(vault_path: &str, run_id: Option<&str>) -> String {
    let Some(run_id) = run_id else { return "app".into() };
    match crate::syn::run::get_run(vault_path, run_id) {
        Ok(run) => run.conversation_id.unwrap_or_else(|| run.parent_run_id.unwrap_or(run.id)),
        Err(_) => run_id.to_string(),
    }
}

/// A column as the pipeline names it: lower case, a space as `_`. A bank's
/// "Số tiền" is `số_tiền` — a word the grammar can read — where it used to be
/// a column no stage could name.
///
/// Anything but letters and digits is a break too: "Số tiền (VND)" is
/// `số_tiền_vnd` — a model wrote `số_tiền_(vnd)` six ways and then added the
/// statement up in its head, a million off (2026-10-11 eval).
pub fn column_key(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Whether a column writes its numbers with `.` between thousands —
/// `-1.265.000`, `25.000.000,50` — as Vietnamese and most European banks do.
///
/// Decided for the whole column, because one cell cannot say: `-55.000` is
/// fifty-five thousand đồng or fifty-five, and only its neighbours tell. A
/// column is grouped when any cell is unmistakably (two dots, or dots then a
/// comma), or when every number in it has exactly three digits after each dot.
/// Read cell by cell, a statement summed to a million less than it holds
/// (2026-10-11 eval). The same reading as Finance's import (`csv.ts`).
pub(crate) fn dot_grouped(cells: &[&str]) -> bool {
    static SURE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^[-+]?\d{1,3}(\.\d{3}){2,}(,\d+)?$|^[-+]?\d{1,3}(\.\d{3})+,\d+$").expect("pattern"));
    static SHAPED: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^[-+]?\d{1,3}(\.\d{3})+(,\d+)?$").expect("pattern"));
    let numbers: Vec<&str> = cells.iter().map(|c| c.trim()).filter(|c| c.chars().any(|ch| ch.is_ascii_digit()) && c.contains('.')).collect();
    !numbers.is_empty() && (numbers.iter().any(|c| SURE.is_match(c)) || numbers.iter().all(|c| SHAPED.is_match(c)))
}

/// `-1.265.000` → `-1265000`, `25.000,5` → `25000.5`, in a grouped column.
pub(crate) fn ungrouped(cell: &str) -> String {
    cell.trim().replace('.', "").replace(',', ".")
}

/// A result set as the pipeline reads one: every cell as text, columns keyed.
fn to_query_result(set: &ResultSet) -> QueryResult {
    let columns: Vec<String> = set.columns.iter().map(|c| column_key(&c.name)).collect();
    let text = |v: &Value| match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let grouped: Vec<bool> = (0..set.columns.len())
        .map(|i| {
            let cells: Vec<String> = set.rows.iter().filter_map(|r| r.get(i)).filter(|v| !v.is_null()).map(text).collect();
            dot_grouped(&cells.iter().map(String::as_str).collect::<Vec<_>>())
        })
        .collect();
    let rows = set
        .rows
        .iter()
        .enumerate()
        .map(|(i, row)| QueryRow {
            id: format!("{}#{i}", set.handle),
            node_type: String::new(),
            title: String::new(),
            cells: row
                .iter()
                .enumerate()
                .map(|(c, v)| {
                    let t = text(v);
                    if grouped.get(c).copied().unwrap_or(false) && t.chars().any(|ch| ch.is_ascii_digit()) {
                        ungrouped(&t)
                    } else {
                        t
                    }
                })
                .collect(),
            open: None,
            until: None,
        })
        .collect();
    let total = set.rows.len();
    QueryResult { columns, rows, total, query_time_ms: 0, note: None }
}

/// What the pipeline answered, as a table to file. Rows that are rows of the
/// input (a `where`, a `sort`) keep their id in `_row`, so a claim about one
/// can point at it.
fn from_query_result(out: QueryResult, input: &ResultSet) -> (Vec<Column>, Vec<Vec<Value>>) {
    let carries_rows = out.rows.iter().any(|r| r.id.starts_with(&format!("{}#", input.handle)));
    let mut names = out.columns.clone();
    if carries_rows {
        names.push("_row".into());
    }
    let rows: Vec<Vec<Value>> = out
        .rows
        .iter()
        .map(|r| {
            let mut cells: Vec<Value> = r.cells.iter().map(|c| Value::String(c.clone())).collect();
            cells.resize(out.columns.len(), Value::String(String::new()));
            if carries_rows {
                cells.push(Value::String(r.id.clone()));
            }
            cells
        })
        .collect();
    // A column every filled cell of which is a number comes back as numbers,
    // so the model reads `1500000`, not `"1500000"`.
    let mut columns = Vec::new();
    let mut typed: Vec<Vec<Value>> = rows.clone();
    for (i, name) in names.iter().enumerate() {
        let original = input.columns.iter().find(|c| column_key(&c.name) == *name);
        let filled: Vec<&str> = rows.iter().filter_map(|r| r[i].as_str()).filter(|s| !s.trim().is_empty()).collect();
        let numeric = name != "_row"
            && !filled.is_empty()
            && original.is_none_or(|c| c.ty == ColType::Number)
            && filled.iter().all(|s| crate::pipeline::number_in(s).is_some());
        if numeric {
            for row in &mut typed {
                row[i] = match row[i].as_str().and_then(crate::pipeline::number_in) {
                    Some(n) => serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null),
                    None => Value::Null,
                };
            }
        } else {
            for row in &mut typed {
                if row[i].as_str().is_some_and(|s| s.is_empty()) {
                    row[i] = Value::Null;
                }
            }
        }
        let mut column = Column::new(
            name.clone(),
            if numeric { ColType::Number } else { original.map(|c| c.ty).unwrap_or(ColType::Text) },
        );
        if let Some(o) = original {
            column.free_text = o.free_text;
            column.tz = o.tz.clone();
        }
        columns.push(column);
    }
    (columns, typed)
}

/// Run `pipeline` over the table `handle` of `scope`, and hand the answer back
/// the way any table is: inline when small, filed when not.
pub fn run(
    conn: &rusqlite::Connection,
    scope: &str,
    run_id: &str,
    handle: &str,
    pipeline: &str,
    now: i64,
) -> AppResult<Value> {
    let Some(input) = resultset::load(conn, scope, handle, now)? else {
        return Err(AppError::General(format!(
            "There is no table '{handle}' in this conversation. Use a handle a tool gave you in this conversation, \
             like r1; a table is kept for a day."
        )));
    };

    let text = pipeline.trim().trim_start_matches('|').trim();
    if text.is_empty() {
        return Err(AppError::General("crunch needs a pipeline, like `stats sum(amount) by category`.".into()));
    }
    let pieces = pieces_of(text)?;

    // A bare word in `where` is a column. A model writing `category = Groceries`
    // meant the text, and would get nothing back and conclude there was nothing
    // — so a name that is no column at that point is refused, with the fix.
    if let Some(Piece::Stages(query)) = pieces.first() {
        let mut known: Vec<String> = input.columns.iter().map(|c| column_key(&c.name)).collect();
        for stage in &query.stages {
            match stage {
                Stage::Where(test) => {
                    if let Some(name) = unknown_column(test, &known) {
                        return Err(AppError::General(format!(
                            "crunch: '{name}' is not a column here ({}). To compare with text, quote it: `= \"{name}\"`.",
                            known.join(", ")
                        )));
                    }
                }
                Stage::Eval { name, .. } => known.push(name.clone()),
                // Past these the columns are the stage's own; stop checking.
                _ => break,
            }
        }
    }

    // Under `Aggregates` the model sees only what was gathered. A pipeline
    // without `stats` hands back rows, so it is answered with how many there
    // are and nothing they hold. A table joined in counts as much as the first.
    let gathers = pieces.iter().any(|p| matches!(p, Piece::Stages(q) if q.stages.iter().any(|s| matches!(s, Stage::Stats { .. }))));
    let mut any_aggregates = input.exposure == Exposure::Aggregates;
    let mut extra_missing: Vec<String> = Vec::new();
    let mut out = to_query_result(&input);
    for piece in &pieces {
        out = match piece {
            Piece::Stages(query) => crate::pipeline::run(query, out).map_err(|e| match e {
                AppError::Refused(why) => AppError::General(format!("crunch: {why}")),
                other => other,
            })?,
            Piece::Join { handle: other, left, right } | Piece::Union { handle: other, left, right } => {
                let Some(set) = resultset::load(conn, scope, other, now)? else {
                    return Err(AppError::General(format!("crunch: there is no table '{other}' in this conversation to join.")));
                };
                any_aggregates |= set.exposure == Exposure::Aggregates;
                extra_missing.extend(set.missing.iter().map(|m| format!("{other}: {m}")));
                let theirs = to_query_result(&set);
                if matches!(piece, Piece::Union { .. }) {
                    union(out, handle, theirs, other)
                } else {
                    join(out, theirs, other, left, right)?
                }
            }
        };
    }
    let note = out.note.clone();
    let (columns, rows) = from_query_result(out, &input);

    let mut missing = input.missing.clone();
    missing.extend(extra_missing);
    if let Some(note) = note {
        missing.push(note);
    }
    let exposure = if any_aggregates && !gathers {
        Exposure::Aggregates
    } else {
        Exposure::Full
    };
    let set = NewSet {
        scope: scope.to_string(),
        run_id: run_id.to_string(),
        source: Source {
            kind: "crunch".into(),
            instance: input.source.instance.clone(),
            operation: Some(format!("{handle} | {text}")),
            query: Some(text.to_string()),
        },
        columns,
        rows,
        missing,
        exposure,
        ttl_hours: 0,
    };
    let delivered = resultset::deliver(conn, set, crate::syn::room::scaled(INLINE_BUDGET), now)?;
    let mut answer = delivered.to_value();
    if let Value::Object(o) = &mut answer {
        o.insert("from".into(), json!(handle));
        o.insert("pipeline".into(), json!(text));
    }
    Ok(answer)
}

/// A pipeline cut where it joins another table: the stages between are the
/// query grammar's; `join` and `union` are crunch's own, since only crunch
/// has the conversation's tables to reach.
enum Piece {
    Stages(crate::query::Query),
    /// `join r2 on author` or `join r2 on author = name`: every row here,
    /// with the matching row's columns of r2 beside it (left join).
    Join { handle: String, left: String, right: String },
    /// `union r2`: the rows of both, under the columns of both, with `source`
    /// saying which table each came from.
    Union { handle: String, left: String, right: String },
}

/// Split on `|` outside quotes.
fn stage_texts(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => {
                quote = None;
                cur.push(c);
            }
            (None, '"' | '\'') => {
                quote = Some(c);
                cur.push(c);
            }
            (None, '|') => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn pieces_of(text: &str) -> AppResult<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    let flush = |plain: &mut Vec<String>, pieces: &mut Vec<Piece>| -> AppResult<()> {
        if plain.is_empty() {
            return Ok(());
        }
        let query = crate::query::parse(&format!("| {}", plain.join(" | ")));
        if let Some(why) = query.refused.first() {
            return Err(AppError::General(format!("crunch could not read the pipeline: {why}")));
        }
        plain.clear();
        pieces.push(Piece::Stages(query));
        Ok(())
    };
    for stage in stage_texts(text) {
        let words: Vec<&str> = stage.split_whitespace().collect();
        match words.first().map(|w| w.to_ascii_lowercase()).as_deref() {
            Some("join") => {
                flush(&mut plain, &mut pieces)?;
                let handle = words.get(1).map(|h| h.to_string()).unwrap_or_default();
                let on = stage.splitn(2, |c: char| c.is_whitespace()).nth(1).unwrap_or_default();
                let on = on.trim_start_matches(handle.as_str()).trim();
                let Some(keys) = on.strip_prefix("on ").map(str::trim) else {
                    return Err(AppError::General("crunch: write `join r2 on author`, or `join r2 on author = name`.".into()));
                };
                let (left, right) = match keys.split_once('=') {
                    Some((l, r)) => (column_key(l), column_key(r)),
                    None => (column_key(keys), column_key(keys)),
                };
                if handle.is_empty() || left.is_empty() || right.is_empty() {
                    return Err(AppError::General("crunch: write `join r2 on author`, or `join r2 on author = name`.".into()));
                }
                pieces.push(Piece::Join { handle, left, right });
            }
            Some("union") => {
                flush(&mut plain, &mut pieces)?;
                let Some(handle) = words.get(1).filter(|_| words.len() == 2) else {
                    return Err(AppError::General("crunch: write `union r2`.".into()));
                };
                pieces.push(Piece::Union { handle: handle.to_string(), left: String::new(), right: String::new() });
            }
            _ => plain.push(stage),
        }
    }
    flush(&mut plain, &mut pieces)?;
    if pieces.is_empty() {
        return Err(AppError::General("crunch needs a pipeline, like `stats sum(amount) by category`.".into()));
    }
    Ok(pieces)
}

fn key(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Every row of `ours` with the matching row of `theirs` beside it; a column
/// of theirs whose name is taken is named `<handle>_<name>`.
fn join(ours: QueryResult, theirs: QueryResult, handle: &str, left: &str, right: &str) -> AppResult<QueryResult> {
    let l = ours.columns.iter().position(|c| c == left).ok_or_else(|| {
        AppError::General(format!("crunch: '{left}' is not a column here ({}).", ours.columns.join(", ")))
    })?;
    let r = theirs.columns.iter().position(|c| c == right).ok_or_else(|| {
        AppError::General(format!("crunch: '{right}' is not a column of {handle} ({}).", theirs.columns.join(", ")))
    })?;
    let added: Vec<usize> = (0..theirs.columns.len()).filter(|i| *i != r).collect();
    let mut columns = ours.columns.clone();
    for i in &added {
        let name = &theirs.columns[*i];
        columns.push(if columns.contains(name) { format!("{handle}_{name}") } else { name.clone() });
    }
    let mut by_key: std::collections::HashMap<String, Vec<&QueryRow>> = std::collections::HashMap::new();
    for row in &theirs.rows {
        by_key.entry(key(row.cells.get(r).map(String::as_str).unwrap_or_default())).or_default().push(row);
    }
    let mut rows = Vec::new();
    for row in ours.rows {
        let k = key(row.cells.get(l).map(String::as_str).unwrap_or_default());
        match by_key.get(&k) {
            Some(matches) if !k.is_empty() => {
                for m in matches {
                    let mut joined = row.clone();
                    joined.cells.resize(ours.columns.len(), String::new());
                    joined.cells.extend(added.iter().map(|i| m.cells.get(*i).cloned().unwrap_or_default()));
                    rows.push(joined);
                }
            }
            _ => {
                let mut kept = row;
                kept.cells.resize(columns.len(), String::new());
                rows.push(kept);
            }
        }
    }
    let total = rows.len();
    Ok(QueryResult { columns, rows, total, query_time_ms: 0, note: ours.note })
}

/// The rows of both, under the columns of both, with `source` saying which.
fn union(ours: QueryResult, ours_handle: &str, theirs: QueryResult, theirs_handle: &str) -> QueryResult {
    let mut columns = ours.columns.clone();
    for c in &theirs.columns {
        if !columns.contains(c) {
            columns.push(c.clone());
        }
    }
    let had_source = columns.iter().any(|c| c == "source");
    if !had_source {
        columns.push("source".into());
    }
    let place = |from: &[String], row: QueryRow, origin: &str| -> QueryRow {
        let mut cells = vec![String::new(); columns.len()];
        for (i, c) in from.iter().enumerate() {
            if let Some(at) = columns.iter().position(|x| x == c) {
                cells[at] = row.cells.get(i).cloned().unwrap_or_default();
            }
        }
        if !had_source || cells[columns.len() - 1].is_empty() {
            let at = columns.iter().position(|c| c == "source").unwrap_or(columns.len() - 1);
            if cells[at].is_empty() {
                cells[at] = origin.to_string();
            }
        }
        QueryRow { cells, ..row }
    };
    let mut rows: Vec<QueryRow> = ours.rows.into_iter().map(|r| place(&ours.columns, r, ours_handle)).collect();
    rows.extend(theirs.rows.into_iter().map(|r| place(&theirs.columns, r, theirs_handle)));
    let total = rows.len();
    QueryResult { columns, rows, total, query_time_ms: 0, note: ours.note }
}

/// Run a pipeline over rows that are not filed — a mirror's, before it keeps
/// them. Only the grammar's stages; nothing here reaches another table.
pub fn apply(columns: &[Column], rows: Vec<Vec<Value>>, pipeline: &str) -> AppResult<(Vec<Column>, Vec<Vec<Value>>)> {
    let set = ResultSet {
        handle: "m".into(),
        scope: String::new(),
        source: Source::default(),
        columns: columns.to_vec(),
        rows,
        missing: Vec::new(),
        exposure: Exposure::Full,
        run_id: String::new(),
    };
    let query = crate::query::parse(&format!("| {}", pipeline.trim().trim_start_matches('|')));
    if let Some(why) = query.refused.first() {
        return Err(AppError::General(format!("the mirror's pipeline does not read: {why}")));
    }
    let out = crate::pipeline::run(&query, to_query_result(&set))?;
    Ok(from_query_result(out, &set))
}

/// The first bare word in a test that names no column in `known`.
fn unknown_column(test: &crate::query::Test, known: &[String]) -> Option<String> {
    use crate::query::{Operand, Test};
    let bad = |o: &Operand| match o {
        Operand::Column(name) if !known.iter().any(|k| k == name) => Some(name.clone()),
        _ => None,
    };
    match test {
        Test::Compare(a, _, b) | Test::Equals(a, b, _) => bad(a).or_else(|| bad(b)),
        Test::All(parts) | Test::Any(parts) => parts.iter().find_map(|p| unknown_column(p, known)),
        Test::Nope(inner) => unknown_column(inner, known),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syn::resultset::{from_objects, Delivered};

    fn conn() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().expect("open");
        crate::db::local_state::ensure_attached(&c).expect("state");
        c
    }

    fn file(c: &rusqlite::Connection, items: Vec<Value>, exposure: Exposure) -> String {
        let (columns, rows) = from_objects(&items);
        resultset::create(
            c,
            NewSet { scope: "conv".into(), run_id: "run".into(), source: Source::default(), columns, rows, missing: vec![], exposure, ttl_hours: 0 },
            1,
        )
        .expect("filed")
    }

    fn spending() -> Vec<Value> {
        let mut items = Vec::new();
        for i in 0..400 {
            let category = ["Groceries", "Dining", "Transport", "Shopping"][i % 4];
            items.push(json!({
                "date": format!("2026-{:02}-{:02}", 8 + (i / 4) % 2, i % 28 + 1),
                "category": category,
                "amount": (i as i64 % 37 + 1) * 1_000,
            }));
        }
        items
    }

    fn rows_of(v: &Value) -> Vec<Value> {
        v["rows"].as_array().cloned().unwrap_or_default()
    }

    #[test]
    fn a_sum_by_group_is_added_by_rust() {
        let c = conn();
        let h = file(&c, spending(), Exposure::Full);
        let want: i64 = spending()
            .iter()
            .filter(|e| e["category"] == "Groceries" && e["date"].as_str().is_some_and(|d| d.starts_with("2026-09")))
            .map(|e| e["amount"].as_i64().unwrap_or(0))
            .sum();
        let out = run(&c, "conv", "run", &h, "where date >= 2026-09-01 | stats sum(amount), count by category | sort sum_amount desc", 2).expect("crunched");
        let groceries = rows_of(&out).into_iter().find(|r| r["category"] == "Groceries").expect("groceries");
        assert_eq!(groceries["sum_amount"].as_f64(), Some(want as f64), "{out}");
        assert_eq!(out["from"], json!(h));
    }

    /// Worklogs by account id, joined to who each account is, then totalled by
    /// team — the man-day report's join (scenario 17).
    #[test]
    fn a_join_puts_the_matching_row_beside_each() {
        let c = conn();
        let logs = file(&c, vec![
            json!({ "account": "a1", "hours": 8 }), json!({ "account": "A1 ", "hours": 4 }),
            json!({ "account": "a2", "hours": 6 }), json!({ "account": "a9", "hours": 2 }),
        ], Exposure::Full);
        let people = file(&c, vec![json!({ "id": "a1", "name": "Linh", "team": "Core" }), json!({ "id": "a2", "name": "Minh", "team": "Core" })], Exposure::Full);
        let out = run(&c, "conv", "run", &logs, &format!("join {people} on account = id | stats sum(hours) by team | sort sum_hours desc"), 2).expect("crunched");
        let rows = rows_of(&out);
        let core = rows.iter().find(|r| r["team"] == "Core").expect("core");
        assert_eq!(core["sum_hours"].as_f64(), Some(18.0), "keys match whatever their case and spaces: {out}");
        // a9 has nobody: kept by the join, then said by `stats` as a row with
        // no team rather than dropped in silence.
        assert!(out["missing"].to_string().contains("1 with no team"), "{out}");
        let all = run(&c, "conv", "run", &logs, &format!("join {people} on account = id | stats sum(hours)"), 2).expect("crunched");
        assert_eq!(rows_of(&all)[0]["sum_hours"].as_f64(), Some(20.0), "a left join loses no hours: {all}");
        let named = run(&c, "conv", "run", &logs, &format!("join {people} on account = id | where name = \"Linh\""), 2).expect("crunched");
        assert_eq!(rows_of(&named).len(), 2);
        let bad = run(&c, "conv", "run", &logs, &format!("join {people} on who = id"), 2).expect_err("no such column");
        assert!(bad.to_string().contains("'who' is not a column here"), "{bad}");
    }

    #[test]
    fn a_union_keeps_where_each_row_came_from() {
        let c = conn();
        let aug = file(&c, vec![json!({ "amount": 10, "category": "Ăn" })], Exposure::Full);
        let sep = file(&c, vec![json!({ "amount": 5, "category": "Ăn", "note": "x" }), json!({ "amount": 7, "category": "Đi" })], Exposure::Full);
        let out = run(&c, "conv", "run", &aug, &format!("union {sep} | stats sum(amount), count by source"), 2).expect("crunched");
        let rows = rows_of(&out);
        let of = |h: &str| rows.iter().find(|r| r["source"] == h).and_then(|r| r["sum_amount"].as_f64());
        assert_eq!((of(&aug), of(&sep)), (Some(10.0), Some(12.0)), "{out}");
        // A quoted `|` is not a stage break.
        let quoted = run(&c, "conv", "run", &aug, &format!("union {sep} | where category = \"Ăn | Uống\""), 2).expect("crunched");
        assert_eq!(rows_of(&quoted).len(), 0);
    }

    /// A table the person let the model see only totals of stays that way
    /// when it is joined onto one it may see.
    #[test]
    fn joining_a_totals_only_table_keeps_it_totals_only() {
        let c = conn();
        let open = file(&c, vec![json!({ "id": "a1", "name": "Linh" })], Exposure::Full);
        let closed = file(&c, vec![json!({ "account": "a1", "salary": 30000000 })], Exposure::Aggregates);
        let rows = run(&c, "conv", "run", &open, &format!("join {closed} on id = account"), 2).expect("crunched");
        // No rows and no sample: only what a summary of columns says. (Bounds
        // are in it, by design; a group of one is the known limit of
        // `aggregates`, joined or not.)
        assert!(!rows["rows"].is_array() && rows.get("sample").is_none(), "{rows}");
        let total = run(&c, "conv", "run", &open, &format!("join {closed} on id = account | stats sum(salary)"), 2).expect("crunched");
        assert!(total.to_string().contains("30000000"), "{total}");
    }

    #[test]
    fn a_column_grouped_with_dots_is_read_as_thousands() {
        assert!(dot_grouped(&["-55.000", "-1.265.000", "25.000.000"]));
        assert!(dot_grouped(&["-55.000", "-42.000"]), "every cell shaped so");
        assert!(!dot_grouped(&["3.5", "12.000"]), "a decimal beside it says otherwise");
        assert!(!dot_grouped(&["0.125", "1.5"]));
        assert_eq!(ungrouped("25.000.000,50"), "25000000.50");
    }

    /// A27: the amount out of a bank's notification, by a pattern, and summed.
    #[test]
    fn extract_takes_a_number_out_of_a_notification() {
        let c = conn();
        let h = file(&c, vec![
            json!({ "text": "TK 0071 -55,000VND luc 01-10 HIGHLANDS" }),
            json!({ "text": "TK 0071 +25,000,000VND LUONG" }),
            json!({ "text": "Ma OTP cua ban la ••••" }),
        ], Exposure::Full);
        let out = run(&c, "conv", "run", &h, "eval amount = extract(text, \"([+-][0-9,.]+)VND\") | where amount < 0 | stats sum(amount), count", 2).expect("crunched");
        assert_eq!(rows_of(&out)[0]["sum_amount"].as_f64(), Some(-55_000.0), "{out}");
        let bad = run(&c, "conv", "run", &h, "eval x = extract(text, \"(\")", 2).expect_err("bad pattern");
        assert!(bad.to_string().contains("does not read"), "{bad}");
    }

    /// A mirror's rows worked out before they are kept: the amount out of each
    /// notification, the ones with none dropped.
    #[test]
    fn a_mirror_pipeline_works_out_rows_without_filing_them() {
        let (columns, rows) = from_objects(&[
            json!({ "_id": "a", "text": "TK 0071 -55,000VND luc 01-10 HIGHLANDS" }),
            json!({ "_id": "b", "text": "Ma OTP ••••" }),
        ]);
        let (cols, out) = apply(&columns, rows, "eval amount = extract(text, \"([+-][0-9,.]+)VND\") | where amount < 0").expect("worked");
        let at = cols.iter().position(|c| c.name == "amount").expect("amount");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][at].as_f64(), Some(-55_000.0));
    }

    /// A bank's header — spaces, brackets — is named by its words alone.
    #[test]
    fn a_header_with_brackets_is_a_column_crunch_can_name() {
        assert_eq!(column_key("Số tiền (VND)"), "số_tiền_vnd");
        assert_eq!(column_key("Ngày giao dịch"), "ngày_giao_dịch");
        let c = conn();
        let h = file(&c, vec![json!({ "Số tiền (VND)": "-55.000" }), json!({ "Số tiền (VND)": "-1.265.000" })], Exposure::Full);
        let out = run(&c, "conv", "run", &h, "stats sum(số_tiền_vnd)", 2).expect("crunched");
        assert_eq!(rows_of(&out)[0]["sum_số_tiền_vnd"].as_f64(), Some(-1_320_000.0), "{out}");
    }

    #[test]
    fn no_by_is_a_grand_total() {
        let c = conn();
        let h = file(&c, vec![json!({ "n": 2 }), json!({ "n": "1,500" }), json!({ "n": "not a number" })], Exposure::Full);
        let out = run(&c, "conv", "run", &h, "stats sum(n), avg(n), min(n), max(n), count", 2).expect("crunched");
        let total = &rows_of(&out)[0];
        assert_eq!(total["sum_n"].as_f64(), Some(1502.0));
        assert_eq!(total["avg_n"].as_f64(), Some(751.0));
        assert_eq!((total["min_n"].as_f64(), total["max_n"].as_f64()), (Some(2.0), Some(1500.0)));
        assert_eq!(total["count"].as_f64(), Some(3.0));
        assert!(out["missing"].to_string().contains("1 cells of n were not numbers"), "{out}");
    }

    #[test]
    fn eval_works_hours_out_in_a_zone() {
        let c = conn();
        let h = file(
            &c,
            vec![
                json!({ "at": "2026-10-09T17:30:00Z", "spent": 28800 }),
                json!({ "at": "2026-10-09T02:00:00Z", "spent": 14400 }),
            ],
            Exposure::Full,
        );
        let out = run(&c, "conv", "run", &h, r#"eval h = hour(at, "Asia/Ho_Chi_Minh") | eval md = spent / 28800 | where h >= 22 or h < 6"#, 2).expect("crunched");
        let rows = rows_of(&out);
        assert_eq!(rows.len(), 1, "{out}");
        assert_eq!(rows[0]["h"].as_f64(), Some(0.0), "17:30Z is half past midnight in Hà Nội: {out}");
        assert_eq!(rows[0]["md"].as_f64(), Some(1.0));
        assert_eq!(rows[0]["_row"], json!(format!("{h}#0")), "a row of the input keeps its id");
    }

    #[test]
    fn a_large_answer_is_filed_in_turn() {
        let c = conn();
        let h = file(&c, spending(), Exposure::Full);
        let out = run(&c, "conv", "run", &h, "sort amount desc", 2).expect("crunched");
        assert_eq!(out["rows"], json!(400), "{}", out.to_string().chars().take(300).collect::<String>());
        assert_eq!(out["handle"], json!("r2"));
    }

    #[test]
    fn another_conversations_table_is_not_there() {
        let c = conn();
        let h = file(&c, spending(), Exposure::Full);
        let err = run(&c, "someone-else", "run", &h, "stats count", 2).expect_err("refused");
        assert!(err.to_string().contains("no table"), "{err}");
    }

    #[test]
    fn a_pipeline_it_cannot_read_says_why() {
        let c = conn();
        let h = file(&c, spending(), Exposure::Full);
        for (pipeline, said) in [
            ("stats median(amount)", "median"),
            ("stats sum(nope)", "nope"),
            ("explode sentences", "Synabit folder"),
            ("eval x = frobnicate(amount)", "frobnicate"),
            ("where category = Groceries", "quote it"),
            ("", "needs a pipeline"),
        ] {
            let err = run(&c, "conv", "run", &h, pipeline, 2).expect_err(pipeline).to_string();
            assert!(err.contains(said), "{pipeline}: {err}");
        }
    }

    /// Under `Aggregates`, rows never come back — only what was gathered.
    #[test]
    fn aggregates_answer_gathered_figures_only() {
        let c = conn();
        let items: Vec<Value> = (0..5).map(|i| json!({ "user": format!("secret-user-{i}"), "n": i })).collect();
        let h = file(&c, items, Exposure::Aggregates);
        let rows = run(&c, "conv", "run", &h, "where n > 1", 2).expect("crunched").to_string();
        assert!(!rows.contains("secret-user"), "{rows}");
        let gathered = run(&c, "conv", "run", &h, "stats sum(n)", 2).expect("crunched");
        assert_eq!(rows_of(&gathered)[0]["sum_n"].as_f64(), Some(10.0));
        let _ = Delivered::Inline(Value::Null);
    }
}
