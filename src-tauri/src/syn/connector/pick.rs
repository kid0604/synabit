//! An answer, made into a table the way a declaration says.
//!
//! APIs answer in nested JSON — an issue has `fields.assignee.displayName` and
//! a list of worklogs inside it. A result set is a flat table. `result` in a
//! declaration says where the records are (`each`), which columns to take and
//! what to call them (`pick`), which inner list to spread into one row per item
//! (`explode`), and how to tell that the API cut an inner list short
//! (`truncated_when`). See `docs/syn-connectors-2026-10-08.md` §3.3.
//!
//! Nothing here can fail an answer: a path that finds nothing is an empty
//! cell, and a value of the wrong kind is an empty cell counted in `missing`.
//! What was lost is always said.

use serde_json::Value;

use super::manifest::{Pick, ResultSpec};
use crate::syn::resultset::{self, ColType, Column};

/// Every value a path reaches. `[*]` fans out; `[n]` picks one.
pub fn at(value: &Value, path: &str) -> Vec<Value> {
    let p = path.strip_prefix('$').unwrap_or(path);
    let p = p.strip_prefix('.').unwrap_or(p);
    let mut here = vec![value.clone()];
    if p.is_empty() {
        return here;
    }
    for seg in p.split('.') {
        let (name, index) = match seg.split_once('[') {
            Some((n, rest)) => (n, Some(rest.trim_end_matches(']'))),
            None => (seg, None),
        };
        let mut next = Vec::new();
        for v in here {
            let v = if name.is_empty() { v } else { v.get(name).cloned().unwrap_or(Value::Null) };
            match index {
                None => next.push(v),
                Some("*") => {
                    if let Value::Array(items) = v {
                        next.extend(items);
                    }
                }
                Some(n) => {
                    if let Some(item) = n.parse::<usize>().ok().and_then(|i| v.get(i).cloned()) {
                        next.push(item);
                    }
                }
            }
        }
        here = next;
    }
    here
}

/// One value a path reaches, or null.
pub fn one(value: &Value, path: &str) -> Value {
    at(value, path).into_iter().next().unwrap_or(Value::Null)
}

/// A pick's declared type.
fn declared(pick: &Pick) -> Option<ColType> {
    match pick {
        Pick::Typed { ty: Some(t), .. } => Some(match t.as_str() {
            "number" => ColType::Number,
            "bool" => ColType::Bool,
            "time" => ColType::Time,
            "json" => ColType::Json,
            _ => ColType::Text,
        }),
        _ => None,
    }
}

/// A value made what its column says it is, or `None` when it cannot be.
fn shaped(v: Value, ty: Option<ColType>) -> Option<Value> {
    match (ty, v) {
        (_, Value::Null) => Some(Value::Null),
        (None | Some(ColType::Json), v) => Some(v),
        (Some(ColType::Number), Value::Number(n)) => Some(Value::Number(n)),
        (Some(ColType::Number), Value::String(s)) => crate::pipeline::number_in(&s)
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
        (Some(ColType::Bool), Value::Bool(b)) => Some(Value::Bool(b)),
        (Some(ColType::Bool), Value::String(s)) => match s.to_lowercase().as_str() {
            "true" | "yes" | "1" => Some(Value::Bool(true)),
            "false" | "no" | "0" => Some(Value::Bool(false)),
            _ => None,
        },
        (Some(ColType::Text | ColType::Time), Value::String(s)) => Some(Value::String(s)),
        (Some(ColType::Text | ColType::Time), v @ (Value::Number(_) | Value::Bool(_))) => Some(Value::String(v.to_string())),
        (Some(ColType::Text), v) => Some(Value::String(v.to_string())),
        _ => None,
    }
}

fn column(name: &str, pick: &Pick, cells: &[Value]) -> Column {
    let mut c = Column::new(name, declared(pick).unwrap_or_else(|| resultset::guess_type(cells.iter())));
    if let Pick::Typed { tz, free_text, .. } = pick {
        c.tz = tz.clone();
        c.free_text = *free_text;
    }
    c
}

/// The table in `answer`.
pub fn table(answer: &Value, spec: &ResultSpec) -> (Vec<Column>, Vec<Vec<Value>>, Vec<String>) {
    // An answer in columns: its lists read side by side, one record a position.
    if let Some(z) = &spec.zip {
        let columns = one(answer, z);
        let mut records: Vec<Value> = Vec::new();
        if let Value::Object(lists) = &columns {
            let n = lists.values().filter_map(Value::as_array).map(Vec::len).max().unwrap_or(0);
            for i in 0..n {
                let record: serde_json::Map<String, Value> = lists
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_array().and_then(|a| a.get(i)).cloned().unwrap_or(Value::Null)))
                    .collect();
                records.push(Value::Object(record));
            }
        }
        let flat = ResultSpec { zip: None, each: "$".into(), ..spec.clone() };
        return table(&Value::Array(records), &flat);
    }

    // A path that ends on a list means its items: `$` on an answer that is a
    // list is that list's records, not one record holding them.
    let records: Vec<Value> = at(answer, &spec.each)
        .into_iter()
        .flat_map(|v| match v {
            Value::Array(items) => items,
            other => vec![other],
        })
        .collect();
    let mut missing = Vec::new();

    if spec.pick.is_empty() && spec.explode.is_none() {
        let objects: Vec<Value> = records.into_iter().filter(|r| !r.is_null()).collect();
        let (columns, rows) = resultset::from_objects(&objects);
        return (columns, rows, missing);
    }

    let outer: Vec<(&String, &Pick)> = spec.pick.iter().collect();
    let inner: Vec<(&String, &Pick)> = spec.explode.iter().flat_map(|e| e.pick.iter()).collect();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut wrong = vec![0usize; outer.len() + inner.len()];
    let mut cut_short = 0usize;

    for record in &records {
        if record.is_null() {
            continue;
        }
        let mut base = Vec::with_capacity(outer.len());
        for (i, (_, pick)) in outer.iter().enumerate() {
            let raw = one(record, pick.path());
            base.push(shaped(raw, declared(pick)).unwrap_or_else(|| {
                wrong[i] += 1;
                Value::Null
            }));
        }
        if let Some(t) = &spec.truncated_when {
            let total = one(record, &t.total).as_f64();
            let got = one(record, &t.got).as_f64();
            if let (Some(total), Some(got)) = (total, got) {
                if total > got {
                    cut_short += 1;
                }
            }
        }
        match &spec.explode {
            None => rows.push(base),
            Some(e) => {
                let items: Vec<Value> = at(record, &e.path).into_iter().flat_map(|v| match v {
                    Value::Array(a) => a,
                    Value::Null => Vec::new(),
                    other => vec![other],
                }).collect();
                if items.is_empty() {
                    let mut row = base.clone();
                    row.extend(std::iter::repeat(Value::Null).take(inner.len()));
                    rows.push(row);
                }
                for item in items {
                    let mut row = base.clone();
                    for (j, (_, pick)) in inner.iter().enumerate() {
                        let raw = one(&item, pick.path());
                        row.push(shaped(raw, declared(pick)).unwrap_or_else(|| {
                            wrong[outer.len() + j] += 1;
                            Value::Null
                        }));
                    }
                    rows.push(row);
                }
            }
        }
    }

    let names: Vec<(&String, &Pick)> = outer.iter().chain(inner.iter()).copied().collect();
    let columns: Vec<Column> = names
        .iter()
        .enumerate()
        .map(|(i, (name, pick))| {
            let cells: Vec<Value> = rows.iter().map(|r| r[i].clone()).collect();
            column(name, pick, &cells)
        })
        .collect();
    for ((name, _), n) in names.iter().zip(&wrong) {
        if *n > 0 {
            missing.push(format!("{n} values of {name} were not what the declaration said and were left empty"));
        }
    }
    if cut_short > 0 {
        missing.push(format!(
            "{cut_short} records had more {} than the answer carried — the API cut that list short",
            spec.explode.as_ref().map(|e| e.path.as_str()).unwrap_or("items")
        ));
    }
    (columns, rows, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec(yaml: &str) -> ResultSpec {
        serde_yaml::from_str(yaml).expect("spec")
    }

    #[test]
    fn paths_reach_into_nested_answers() {
        let v = json!({ "issues": [{ "key": "A-1", "fields": { "who": { "name": "Linh" } } }, { "key": "A-2" }] });
        assert_eq!(at(&v, "$.issues[*].key"), vec![json!("A-1"), json!("A-2")]);
        assert_eq!(one(&v, "issues[0].fields.who.name"), json!("Linh"));
        assert_eq!(one(&v, "issues[1].fields.who.name"), Value::Null);
        assert_eq!(at(&v, "$"), vec![v.clone()]);
    }

    #[test]
    fn named_columns_are_taken_and_typed() {
        let v = json!([{ "id": 7, "content": "Gọi Linh", "due": { "date": "2026-10-12" }, "priority": "4" }]);
        let (cols, rows, missing) = table(&v, &spec("each: $\npick:\n  id: id\n  task: { path: content, free_text: true }\n  due: { path: due.date, type: time }\n  priority: { path: priority, type: number }\n"));
        let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["due", "id", "priority", "task"]);
        assert_eq!(cols[3].free_text, true);
        assert_eq!(rows[0][2], json!(4.0), "a number written as text is a number");
        assert!(missing.is_empty());
    }

    /// Jira: one row per worklog, the issue's fields on each, and the issues
    /// whose worklogs the API cut short are counted.
    #[test]
    fn an_inner_list_spreads_into_rows_and_a_cut_list_is_said() {
        let v = json!({ "issues": [
            { "key": "A-1", "fields": { "worklog": { "total": 25, "maxResults": 2, "worklogs": [
                { "author": { "displayName": "Linh" }, "timeSpentSeconds": 3600 },
                { "author": { "displayName": "Minh" }, "timeSpentSeconds": "7200" }
            ] } } },
            { "key": "A-2", "fields": { "worklog": { "total": 0, "maxResults": 20, "worklogs": [] } } }
        ] });
        let s = spec(
            "each: $.issues[*]\npick:\n  key: key\nexplode:\n  path: fields.worklog.worklogs\n  pick:\n    author: author.displayName\n    spent_s: { path: timeSpentSeconds, type: number }\ntruncated_when: { total: fields.worklog.total, got: fields.worklog.maxResults }\n",
        );
        let (cols, rows, missing) = table(&v, &s);
        assert_eq!(cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["key", "author", "spent_s"]);
        assert_eq!(rows.len(), 3, "two worklogs, and an issue with none still has its row");
        assert_eq!(rows[1], vec![json!("A-1"), json!("Minh"), json!(7200.0)]);
        assert_eq!(rows[2], vec![json!("A-2"), Value::Null, Value::Null]);
        assert!(missing.iter().any(|m| m.starts_with("1 records had more")), "{missing:?}");
    }

    #[test]
    fn a_value_of_the_wrong_kind_is_empty_and_counted() {
        let v = json!([{ "n": "lots" }, { "n": 3 }]);
        let (_, rows, missing) = table(&v, &spec("pick:\n  n: { path: n, type: number }\n"));
        assert_eq!(rows[0][0], Value::Null);
        assert!(missing[0].starts_with("1 values of n"), "{missing:?}");
    }

    /// Open-Meteo answers in columns: a list of days, a list of rain, side by side.
    #[test]
    fn an_answer_in_columns_is_read_side_by_side() {
        let v = json!({ "daily": { "time": ["2026-09-01", "2026-09-02"], "precipitation_sum": [0.0, 1.6] } });
        let (cols, rows, _) = table(&v, &spec("zip: $.daily\npick:\n  day: { path: time, type: time }\n  rain_mm: { path: precipitation_sum, type: number }\n"));
        assert_eq!(cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["day", "rain_mm"]);
        assert_eq!(rows, vec![vec![json!("2026-09-01"), json!(0.0)], vec![json!("2026-09-02"), json!(1.6)]]);
    }

    #[test]
    fn no_pick_takes_every_field() {
        let v = json!({ "results": [{ "a": 1, "b": "x" }] });
        let (cols, rows, _) = table(&v, &spec("each: $.results[*]\n"));
        assert_eq!(cols.len(), 2);
        assert_eq!(rows.len(), 1);
    }
}
