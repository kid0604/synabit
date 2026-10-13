//! Code Syn writes and Rust runs — with nothing it can reach.
//!
//! # Why code at all
//!
//! The agents that do the most are the ones that write code: a filter nobody
//! anticipated, a date worked out, two lists matched up, a table reshaped. Syn
//! had `crunch`, a pipeline language — totals, groups, joins — and nothing past
//! it, so anything else was the model doing arithmetic in its head, which is
//! what result sets were built to stop.
//!
//! # Why this cannot leak, or bring the app down
//!
//! A shell is how an agent like this loses everything: code a page talked the
//! model into writing, run with the user's files and network. So the language
//! here has **no edge to anything**: no files, no network, no clock worth the
//! name, no `eval`, no modules. What goes in is what the call names — tables
//! this conversation already has, and data passed in the call — and what comes
//! out is a value. Reading from outside cannot make it reach further, which is
//! why it is allowed after reading, in helpers and in routines.
//!
//! What it could still do is spin or swell. Rhai is chosen over Starlark for
//! exactly that (2026-10-12): Rhai refuses an operation that would exceed its
//! limits *before* it allocates — a two-gigabyte string, a billion-element
//! array, a loop with no end, recursion with no floor — where Starlark checked
//! its heap after the fact, peaked at 7.7 GB and once aborted the process. The
//! limits below are those checks.
//!
//! A table the person allowed only totals of (`Exposure::Aggregates`) is not
//! given to a script: a script can hand back one row's value, which is the
//! thing that exposure exists to keep from the model. `crunch` totals it.

use serde_json::{Map, Value};

/// The tool.
pub const TOOL: &str = "compute";

/// Operations one script may take — loop turns, calls, expressions. A few
/// tens of milliseconds of work; a loop with no end stops here.
pub const MAX_OPERATIONS: u64 = 5_000_000;
/// The longest string it may make.
pub const MAX_STRING: usize = 4 * 1024 * 1024;
/// The most items in one array, and keys in one map.
pub const MAX_ARRAY: usize = 200_000;
pub const MAX_MAP: usize = 50_000;
/// How deep calls may go.
pub const MAX_CALLS: usize = 32;

/// What a script left, and what it printed on the way.
#[derive(Debug, Clone, PartialEq)]
pub struct Ran {
    pub value: Value,
    pub printed: Vec<String>,
}

fn engine(printed: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> rhai::Engine {
    let mut e = rhai::Engine::new();
    e.set_max_operations(MAX_OPERATIONS);
    e.set_max_string_size(MAX_STRING);
    e.set_max_array_size(MAX_ARRAY);
    e.set_max_map_size(MAX_MAP);
    e.set_max_call_levels(MAX_CALLS);
    e.set_max_expr_depths(64, 32);
    e.set_max_variables(2_000);
    e.set_max_functions(200);
    // A typo is an error, not a silent unit.
    e.set_strict_variables(true);
    // Code that writes code is code nobody read.
    e.disable_symbol("eval");
    let out = printed.clone();
    e.on_print(move |s| {
        if let Ok(mut p) = out.lock() {
            if p.len() < 200 {
                p.push(s.chars().take(1_000).collect());
            }
        }
    });
    let out = printed;
    e.on_debug(move |s, _, _| {
        if let Ok(mut p) = out.lock() {
            if p.len() < 200 {
                p.push(s.chars().take(1_000).collect());
            }
        }
    });
    // A number however it was written: 1265000, "1,265,000", "1.265.000 đ",
    // "-55.000". `()` when it is not one, so a script can skip it.
    e.register_fn("num", |v: rhai::Dynamic| -> rhai::Dynamic {
        let as_json: Value = rhai::serde::from_dynamic(&v).unwrap_or(Value::Null);
        let n = match &as_json {
            Value::String(s) => loose_number(s),
            other => crate::syn::resultset::number_of(other),
        };
        n.map_or(rhai::Dynamic::UNIT, rhai::Dynamic::from_float)
    });
    e
}

/// A number as people write one, one value at a time: "1265000", "-55.000",
/// "1.265.000 đ", "1,265,000", "12,5", "3.14", "$1,200.50".
///
/// With both separators, the last is the decimal point. With one kind used
/// more than once, or once with exactly three digits after it, it groups
/// thousands — the way a Vietnamese statement writes 55.000. Otherwise it is
/// the decimal point. `crunch` decides by the whole column instead
/// (`crunch::dot_grouped`); a script reads one value, so this decides by that.
pub fn loose_number(text: &str) -> Option<f64> {
    let negative = text.trim_start().starts_with('-') || (text.contains('(') && text.contains(')'));
    let kept: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    if !kept.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let dots = kept.matches('.').count();
    let commas = kept.matches(',').count();
    let groups = |sep: char| {
        let parts: Vec<&str> = kept.split(sep).collect();
        parts.len() > 2 || parts.get(1).is_some_and(|p| p.len() == 3)
    };
    let plain: String = match (dots, commas) {
        (0, 0) => kept,
        (_, 0) if groups('.') => kept.replace('.', ""),
        (0, _) if groups(',') => kept.replace(',', ""),
        (_, 0) => kept,
        (0, _) => kept.replace(',', "."),
        _ => {
            let last_dot = kept.rfind('.').unwrap_or(0);
            let last_comma = kept.rfind(',').unwrap_or(0);
            if last_dot > last_comma {
                kept.replace(',', "")
            } else {
                kept.replace('.', "").replace(',', ".")
            }
        }
    };
    plain.parse::<f64>().ok().map(|n| if negative { -n } else { n })
}

/// Run a script. `tables` by handle, each a list of rows as maps; `data` as
/// given. The script's last expression is what it returns.
pub fn run(code: &str, tables: Map<String, Value>, data: Value) -> Result<Ran, String> {
    if code.trim().is_empty() {
        return Err("compute needs `code`.".into());
    }
    let printed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let e = engine(printed.clone());
    let mut scope = rhai::Scope::new();
    scope.push_constant(
        "tables",
        rhai::serde::to_dynamic(Value::Object(tables)).map_err(|e| format!("The tables could not be handed in: {e}"))?,
    );
    scope.push_constant("data", rhai::serde::to_dynamic(data).map_err(|e| format!("`data` could not be handed in: {e}"))?);
    let out = e
        .eval_with_scope::<rhai::Dynamic>(&mut scope, code)
        .map_err(|err| format!("The script stopped: {err}. Fix it and call compute again."))?;
    let value: Value = if out.is_unit() {
        Value::Null
    } else {
        rhai::serde::from_dynamic(&out).map_err(|e| format!("What the script returned is not data: {e}"))?
    };
    let printed = printed.lock().map(|p| p.clone()).unwrap_or_default();
    Ok(Ran { value, printed })
}

/// Whether a value is a table: a non-empty list of records.
pub fn is_table(v: &Value) -> bool {
    matches!(v, Value::Array(rows) if !rows.is_empty() && rows.iter().all(Value::is_object))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn go(code: &str) -> Result<Value, String> {
        run(code, Map::new(), Value::Null).map(|r| r.value)
    }

    #[test]
    fn it_computes_over_a_table_it_was_given() {
        let mut tables = Map::new();
        tables.insert(
            "r1".into(),
            json!([{ "amount": "-55.000", "category": "Ăn uống" }, { "amount": "-1.265.000", "category": "Nhà" }, { "amount": "x" }]),
        );
        let out = run(
            r#"let total = 0.0; for row in tables.r1 { let n = num(row.amount); if n != () { total += n; } } total"#,
            tables,
            Value::Null,
        )
        .expect("ran");
        assert_eq!(out.value, json!(-1320000.0));
    }

    #[test]
    fn numbers_are_read_as_people_write_them() {
        for (text, n) in [
            ("1265000", 1265000.0), ("-55.000", -55000.0), ("1.265.000 đ", 1265000.0), ("1,265,000", 1265000.0),
            ("12,5", 12.5), ("3.14", 3.14), ("$1,200.50", 1200.5), ("1.200,50 €", 1200.5), ("(300)", -300.0),
        ] {
            assert_eq!(loose_number(text), Some(n), "{text}");
        }
        assert_eq!(loose_number("x"), None);
    }

    #[test]
    fn data_goes_in_and_records_come_out() {
        let out = run(
            r#"let out = []; for d in data.days { out.push(#{ day: d, half: d / 2 }); } out"#,
            Map::new(),
            json!({ "days": [2, 4] }),
        )
        .expect("ran");
        assert!(is_table(&out.value));
        assert_eq!(out.value, json!([{ "day": 2, "half": 1 }, { "day": 4, "half": 2 }]));
    }

    /// Every way to spin or swell stops at a limit, before it allocates.
    #[test]
    fn nothing_it_does_can_spin_or_swell() {
        for (what, code) in [
            ("a loop with no end", "let s = 0; while true { s += 1; } s"),
            ("a very long loop", "let s = 0; for i in 0..10000000000 { s += i; } s"),
            ("a two-gigabyte string", r#"let x = ""; x.pad(2000000000, 'a'); x.len()"#),
            ("a string doubled forty times", r#"let s = "ab"; for i in 0..40 { s = s + s; } s.len()"#),
            ("a billion-element array", "let a = []; a.pad(2000000000, 1); a.len()"),
            ("recursion with no floor", "fn f(x) { f(x + 1) } f(0)"),
        ] {
            let t = std::time::Instant::now();
            assert!(go(code).is_err(), "{what} ran to the end");
            assert!(t.elapsed() < std::time::Duration::from_secs(5), "{what} took {:?}", t.elapsed());
        }
    }

    /// It reaches nothing: no eval, no modules, no names it was not given.
    #[test]
    fn it_reaches_nothing() {
        assert!(go(r#"eval("40 + 2")"#).is_err(), "eval");
        assert!(go(r#"import "fs" as fs; 1"#).is_err(), "modules");
        assert!(go("vault_path").is_err(), "an undeclared name is an error");
    }

    #[test]
    fn what_it_prints_comes_back_and_an_error_says_how_to_fix_it() {
        let out = run(r#"print("half way"); 1 + 1"#, Map::new(), Value::Null).expect("ran");
        assert_eq!(out.printed, vec!["half way".to_string()]);
        assert_eq!(out.value, json!(2));
        assert!(go("let x = ;").unwrap_err().contains("call compute again"));
    }
}
