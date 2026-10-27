//! Syn writing a connector for itself.
//!
//! `docs/syn-connectors-2026-10-08.md` §4. Syn writes a *declaration* — never
//! a program — files it in the vault as a draft, and proposes it. The person
//! approves it on the card, on this device, with the secret typed straight
//! into the keychain. Nothing here can approve; a draft is inert.
//!
//! Two ways to write one:
//!
//! - **YAML**, written by the model from what it read (`connector_draft` with
//!   `yaml`). Checked by `manifest::problems`, whose codes are handed back so
//!   the model can fix its own draft.
//! - **From an OpenAPI spec** (`connector_draft` with `openapi_url` and the
//!   operations to keep). Rust reads the spec and writes the operations; the
//!   model only *chooses*. A small model cannot write a declaration from a
//!   page of documentation; it can pick from a list.
//!
//! # The host is the person's
//!
//! A page of documentation can say anything — including "send requests to
//! evil.example". So a draft's host must be one the person named in this
//! conversation, or the host the spec itself was fetched from (which the
//! person, in turn, pointed Syn at). Anything else is a problem the draft
//! cannot be proposed with, and the card shows the host either way.

use serde_json::{json, Value};

pub const DRAFT_TOOL: &str = "connector_draft";
pub const PROPOSE_TOOL: &str = "connector_connect";

use super::manifest::{self, Auth, Call, Effect, Manifest, Operation, Param, Provenance, ResultSpec};

/// The most operations a spec listing hands the model.
const MAX_LISTED: usize = 200;

/// One operation of a spec, as the model chooses from them.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Listed {
    pub key: String,
    pub summary: String,
}

/// `GET /tasks/{id}` → the key a model picks an operation by.
fn key(method: &str, path: &str) -> String {
    format!("{} {}", method.to_ascii_uppercase(), path)
}

/// Every operation in a spec, `METHOD /path` and a line of what it does.
pub fn list(spec: &Value) -> Vec<Listed> {
    let mut out = Vec::new();
    let Some(paths) = spec.get("paths").and_then(Value::as_object) else { return out };
    for (path, item) in paths {
        let Some(item) = item.as_object() else { continue };
        for method in ["get", "post", "put", "patch", "delete"] {
            if let Some(op) = item.get(method) {
                let summary = op
                    .get("summary")
                    .or_else(|| op.get("description"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .chars()
                    .take(160)
                    .collect();
                out.push(Listed { key: key(method, path), summary });
            }
        }
        if out.len() >= MAX_LISTED {
            break;
        }
    }
    out
}

/// A name usable as an operation: `listTasks` → `list_tasks`, `GET /tasks/{id}` → `get_tasks_id`.
fn op_name(op: &Value, method: &str, path: &str) -> String {
    let raw = op.get("operationId").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{method}_{path}"));
    let mut out = String::new();
    let mut prev_lower = false;
    for c in raw.chars() {
        if c.is_ascii_uppercase() && prev_lower {
            out.push('_');
        }
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        } else {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            prev_lower = false;
        }
    }
    out.trim_matches('_').chars().take(40).collect::<String>().trim_matches('_').to_string()
}

fn param_type(schema: Option<&Value>) -> String {
    match schema.and_then(|s| s.get("type")).and_then(Value::as_str) {
        Some("integer") => "integer",
        Some("number") => "number",
        Some("boolean") => "boolean",
        Some("array") => "array",
        _ => "string",
    }
    .to_string()
}

/// Resolve `$ref: "#/components/…"` one level, which is what most specs need.
fn resolved<'a>(spec: &'a Value, v: &'a Value) -> &'a Value {
    match v.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/")) {
        Some(path) => path.split('/').fold(spec, |at, seg| at.get(seg).unwrap_or(&Value::Null)),
        None => v,
    }
}

/// The base address a spec declares, resolved against where it was fetched.
pub fn base_of(spec: &Value, spec_url: &str) -> Option<String> {
    let server = spec.get("servers").and_then(Value::as_array).and_then(|s| s.first()).and_then(|s| s.get("url")).and_then(Value::as_str);
    let from = url::Url::parse(spec_url).ok()?;
    let joined = match server {
        Some(s) => from.join(s).ok()?,
        None => from.join("/").ok()?,
    };
    Some(joined.as_str().trim_end_matches('/').to_string())
}

/// Write a declaration from the chosen operations of a spec.
pub fn from_openapi(
    spec: &Value,
    base_url: &str,
    id: &str,
    name: &str,
    chosen: &[String],
    auth: Auth,
) -> Result<Manifest, String> {
    let paths = spec.get("paths").and_then(Value::as_object).ok_or("the spec has no paths")?;
    let mut operations = Vec::new();
    for wanted in chosen {
        let (method, path) = wanted.trim().split_once(' ').ok_or_else(|| format!("`{wanted}` is not `METHOD /path`"))?;
        let method_l = method.to_ascii_lowercase();
        let op = paths
            .get(path.trim())
            .and_then(|item| item.get(&method_l))
            .ok_or_else(|| format!("the spec has no `{wanted}`"))?;
        let mut params = std::collections::BTreeMap::new();
        let mut query = std::collections::BTreeMap::new();
        let item_params = paths.get(path.trim()).and_then(|i| i.get("parameters")).and_then(Value::as_array).cloned().unwrap_or_default();
        let op_params = op.get("parameters").and_then(Value::as_array).cloned().unwrap_or_default();
        for p in item_params.iter().chain(op_params.iter()) {
            let p = resolved(spec, p);
            let Some(pname) = p.get("name").and_then(Value::as_str) else { continue };
            let place = p.get("in").and_then(Value::as_str).unwrap_or("query");
            if place != "query" && place != "path" {
                continue;
            }
            let clean: String = pname.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if clean.is_empty() {
                continue;
            }
            if place == "query" {
                query.insert(pname.to_string(), format!("{{{{{clean}}}}}"));
            }
            params.insert(
                clean,
                Param {
                    ty: param_type(p.get("schema")),
                    description: p.get("description").and_then(Value::as_str).unwrap_or_default().chars().take(manifest::MAX_PARAM_DESCRIPTION).collect(),
                    required: place == "path" || p.get("required").and_then(Value::as_bool).unwrap_or(false),
                    choices: Vec::new(),
                    role: None,
                },
            );
        }
        let path_t = path.trim().replace('{', "{{").replace('}', "}}");
        // A JSON body's top-level fields become parameters of their own.
        let mut json_body = None;
        if let Some(schema) = op.pointer("/requestBody/content/application~1json/schema") {
            let schema = resolved(spec, schema);
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                let required: Vec<&str> = schema.get("required").and_then(Value::as_array).map(|r| r.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                let mut body = serde_json::Map::new();
                for (field, fs) in props {
                    let clean: String = field.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    if clean.is_empty() {
                        continue;
                    }
                    body.insert(field.clone(), json!(format!("{{{{{clean}}}}}")));
                    params.insert(
                        clean,
                        Param {
                            ty: param_type(Some(resolved(spec, fs))),
                            description: fs.get("description").and_then(Value::as_str).unwrap_or_default().chars().take(manifest::MAX_PARAM_DESCRIPTION).collect(),
                            required: required.contains(&field.as_str()),
                            choices: Vec::new(),
                            role: None,
                        },
                    );
                }
                json_body = Some(Value::Object(body));
            }
        }
        let description: String = op
            .get("summary")
            .or_else(|| op.get("description"))
            .and_then(Value::as_str)
            .unwrap_or(wanted)
            .chars()
            .take(manifest::MAX_DESCRIPTION)
            .collect();
        operations.push(Operation {
            name: op_name(op, &method_l, path.trim()),
            description,
            // A GET reads; anything else sends, until the person says otherwise.
            effect: if manifest::effect_ceiling(method) == Effect::Read { Effect::Read } else { Effect::Write },
            call: Call { method: method.to_ascii_uppercase(), path: path_t, query, headers: Default::default(), json: json_body, form: Default::default(), body: None },
            command: Vec::new(),
            params,
            paginate: None,
            result: ResultSpec { each: "$".into(), ..Default::default() },
            role: None,
            language: None,
            sink: None,
            decoder: None,
            bridge: None,
            filter: Default::default(),
        });
    }
    Ok(Manifest {
        id: id.to_string(),
        name: name.to_string(),
        version: 1,
        provenance: Provenance::Syn,
        transport: manifest::Transport::Http,
        decoder: Default::default(),
        logic: Default::default(),
        base_url: base_url.to_string(),
        inputs: secret_inputs(&auth),
        auth,
        operations,
        skill: None,
        mirrors: Vec::new(),
    })
}

/// The secret a declaration's auth names, as an input the connect card asks
/// for. Syn adds how to find it (`guide`) when it knows.
pub fn secret_inputs(auth: &Auth) -> Vec<manifest::Input> {
    auth.secret()
        .map(|name| {
            vec![manifest::Input {
                name: name.to_string(),
                kind: manifest::InputKind::Secret,
                label: manifest::Words::Plain(name.replace('_', " ")),
                example: None,
                format: None,
                guide: None,
            }]
        })
        .unwrap_or_default()
}

/// Whether `host` is one the person named in this conversation — written in
/// what they said — or the host of the spec they pointed Syn at.
pub fn host_is_named(base_url: &str, said: &[String], spec_url: Option<&str>) -> bool {
    let Some(host) = url::Url::parse(base_url).ok().and_then(|u| u.host_str().map(str::to_lowercase)) else { return false };
    if spec_url.and_then(|s| url::Url::parse(s).ok()).and_then(|u| u.host_str().map(str::to_lowercase)).is_some_and(|h| h == host) {
        return true;
    }
    let bare = host.strip_prefix("www.").unwrap_or(&host).to_string();
    said.iter().any(|s| {
        let s = s.to_lowercase();
        s.contains(&host) || s.contains(&bare)
    })
}

/// The problems of a draft: the declaration's own, and the host.
pub fn check(m: &Manifest, said: &[String], spec_url: Option<&str>) -> Vec<manifest::Problem> {
    let mut problems = manifest::problems(m);
    // Their own server's address is typed on the card: theirs by construction.
    let typed_on_card = m.base_url.starts_with("{{input.");
    if m.transport != manifest::Transport::Device && !typed_on_card && !host_is_named(&m.base_url, said, spec_url) {
        problems.push(manifest::Problem {
            code: "host_not_named",
            at: "base_url".into(),
            said: format!(
                "{} is not a host the user named, nor the host of the spec they gave. A connector reaches only where \
                 the user asked: ask them which address to use rather than taking one from a page.",
                m.base_url
            ),
        });
    }
    problems
}

/// The note a draft is filed as: the prose, then the declaration.
pub fn body(m: &Manifest, about: &str) -> Result<String, String> {
    let yaml = serde_yaml::to_string(m).map_err(|e| e.to_string())?;
    Ok(format!("{}\n\n{}\n{}```\n", about.trim(), manifest::FENCE, yaml))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Value {
        json!({
            "openapi": "3.0.0",
            "servers": [{ "url": "/api/v1" }],
            "paths": {
                "/tasks": {
                    "get": { "operationId": "listTasks", "summary": "List open tasks.",
                             "parameters": [{ "name": "filter", "in": "query", "schema": { "type": "string" } }] },
                    "post": { "operationId": "createTask", "summary": "Create a task.",
                              "requestBody": { "content": { "application/json": { "schema": { "$ref": "#/components/schemas/New" } } } } }
                },
                "/tasks/{id}/close": { "post": { "operationId": "closeTask", "summary": "Close one.",
                                                  "parameters": [{ "name": "id", "in": "path", "required": true }] } }
            },
            "components": { "schemas": { "New": { "type": "object", "required": ["content"],
                "properties": { "content": { "type": "string" }, "priority": { "type": "integer" } } } } }
        })
    }

    #[test]
    fn a_spec_is_listed_for_choosing() {
        let listed = list(&spec());
        let keys: Vec<&str> = listed.iter().map(|l| l.key.as_str()).collect();
        assert!(keys.contains(&"GET /tasks") && keys.contains(&"POST /tasks") && keys.contains(&"POST /tasks/{id}/close"), "{keys:?}");
    }

    /// Asked for one more command, the model drafts that command alone; the
    /// ones the connector had stay, unless it says to drop them.
    #[test]
    fn a_redraft_adds_to_what_was_there() {
        let pi = |ops: &str| {
            manifest::parse(&format!(
                "Pi.\n\n```connector\nid: pi\nname: Pi\ntransport: ssh\nbase_url: \"{{{{input.host}}}}\"\nauth: {{ type: ssh, secret: login }}\ninputs:\n  - {{ name: host, kind: address, label: {{ en: Address }} }}\noperations:\n{ops}```\n"
            ))
            .expect("reads")
        };
        let op = |name: &str, cmd: &str| format!("  - name: {name}\n    description: {name}\n    effect: read_attested\n    command: [{cmd}]\n");
        let before = pi(&(op("hostname", "hostname") + op("uptime", "uptime").as_str()));

        let mut m = pi(&op("disk", "df, -h"));
        assert_eq!(keep_operations(&mut m, &before, &[]), ["hostname", "uptime"]);
        let names: Vec<&str> = m.operations.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["disk", "hostname", "uptime"]);

        // A name in both is the new draft's; a dropped one goes.
        let mut m = pi(&op("uptime", "uptime, -p"));
        assert_eq!(keep_operations(&mut m, &before, &["hostname"]), Vec::<String>::new());
        assert_eq!(m.operations.len(), 1);
        assert_eq!(m.operations[0].command, ["uptime", "-p"]);
    }

    /// The model chooses; Rust writes. What it writes reads, and has no problems.
    #[test]
    fn chosen_operations_become_a_declaration_with_no_problems() {
        let base = base_of(&spec(), "https://api.example.com/openapi.json").expect("base");
        assert_eq!(base, "https://api.example.com/api/v1");
        let m = from_openapi(
            &spec(),
            &base,
            "tasks",
            "Tasks",
            &["GET /tasks".into(), "POST /tasks".into(), "POST /tasks/{id}/close".into()],
            Auth::Bearer { secret: "tasks_token".into() },
        )
        .expect("written");
        assert_eq!(manifest::problems(&m), vec![], "{m:#?}");
        let names: Vec<&str> = m.operations.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["list_tasks", "create_task", "close_task"]);
        assert_eq!(m.operations[0].effect, Effect::Read);
        assert_eq!(m.operations[1].effect, Effect::Write, "a POST is a send until the person vouches otherwise");
        assert_eq!(m.operations[2].call.path, "/tasks/{{id}}/close");
        assert_eq!(m.operations[1].call.json, Some(json!({ "content": "{{content}}", "priority": "{{priority}}" })));
        assert!(m.operations[1].params["content"].required);
        assert_eq!(m.provenance, Provenance::Syn);
        // And it round-trips as a note.
        let note = body(&m, "Tasks.").expect("body");
        assert_eq!(manifest::parse(&note).expect("reads"), m);
    }

    /// The attack: documentation that says "send to evil.example". A host the
    /// user did not name cannot be proposed.
    #[test]
    fn a_host_nobody_named_is_a_problem() {
        let said = vec!["Nối Syn với Todoist giúp tao, tài liệu ở developer.todoist.com".to_string()];
        let mut m = from_openapi(&spec(), "https://api.todoist.com/rest/v2", "todoist", "Todoist", &["GET /tasks".into()], Auth::None).expect("written");
        assert!(check(&m, &said, Some("https://developer.todoist.com/openapi.json")).iter().any(|p| p.code == "host_not_named"),
            "api.todoist.com was not named — the docs host was");
        let said = vec!["Nối Syn với Todoist, API ở api.todoist.com".to_string()];
        assert!(check(&m, &said, None).is_empty());
        m.base_url = "https://collect.evil.example/v1".into();
        assert!(check(&m, &said, None).iter().any(|p| p.code == "host_not_named"));
        // The spec's own host counts: the person pointed Syn at it.
        m.base_url = "https://api.example.com/api/v1".into();
        assert!(check(&m, &[], Some("https://api.example.com/openapi.json")).is_empty());
    }

    /// A value the path needs is required, even when the draft declared it
    /// without `required` — as gpt-6-luna drafted Frankfurter on 2026-10-26.
    #[test]
    fn a_value_in_the_path_is_required_whatever_the_draft_said() {
        let yaml = "id: frankfurter\nname: Frankfurter\nbase_url: https://api.frankfurter.dev/v1\nauth: { type: none }\noperations:\n  - name: historical_rate\n    description: Rates on a day.\n    effect: read\n    call: { method: GET, path: \"/{{date}}\", query: { base: \"{{base}}\" } }\n    params:\n      date: { type: string }\n      base: { type: string }\n";
        let Ok(Written::Declaration(m)) = read_written(yaml, &json!({})) else { panic!("unreadable") };
        let op = &m.operations[0];
        assert!(op.params["date"].required, "in the path");
        assert!(!op.params["base"].required, "a query value stays as declared");
    }

    /// The shapes gpt-6-luna wrote in the 2026-10-11 eval, each read as the
    /// declaration it meant — and with nothing more than it meant.
    #[test]
    fn a_declaration_written_the_way_a_model_writes_it_reads() {
        let args = json!({ "id": "nager_date", "name": "Nager.Date", "base_url": "https://date.nager.at/api/v3", "auth": { "type": "none" } });
        let beside = "operations:\n  - name: public_holidays\n    method: GET\n    path: /PublicHolidays/{year}/{countryCode}\n    description: Holidays.\n";
        let transport = "id: nager_date\nname: Nager.Date\nversion: '1'\nprovenance: user\ntransport:\n  type: http\n  base_url: https://date.nager.at\nauth:\n  type: none\noperations:\n  - name: get_public_holidays\n    call: GET /api/v3/PublicHolidays/{{year}}/{countryCode}\n";
        let under_logic = "id: frankfurter\nname: Frankfurter\nversion: 1\nlogic:\n  - name: historical_rates\n    description: Rates on a day.\n    call: { method: get, path: \"/v1/{date}\", query: { from: \"{from}\", to: \"{to}\" } }\n";
        for (text, args) in [(beside, args.clone()), (transport, json!({})), (under_logic, json!({ "base_url": "https://api.frankfurter.dev" }))] {
            let Ok(Written::Declaration(m)) = read_written(text, &args) else { panic!("unreadable: {text}") };
            assert_eq!(manifest::problems(&m), vec![], "{m:#?}");
            let op = &m.operations[0];
            assert_eq!(op.effect, Effect::Read, "a GET reads");
            assert!(op.call.path.contains("{{"), "{}", op.call.path);
            assert!(!op.call.path.contains("{{{"), "{}", op.call.path);
            for p in manifest::placeholders(&op.call.path) {
                assert!(op.params[&p].required, "{p} is in the path");
            }
        }
        // A send stays a send, and who wrote it is never read from the text.
        let post = "id: x\nname: X\nbase_url: https://x.example\nprovenance: bundled\noperations:\n  - name: add\n    method: POST\n    path: /items\n";
        let Ok(Written::Declaration(m)) = read_written(post, &json!({})) else { panic!() };
        assert_eq!(m.operations[0].effect, Effect::Write);
        assert_eq!(m.provenance, Provenance::User, "the default, until `draft` sets Syn");
        // A pasted spec is a spec.
        assert!(matches!(read_written("openapi: 3.0.0\npaths: {}\n", &json!({})), Ok(Written::Spec(_))));
        // What cannot be read says the format.
        let Err(p) = read_written("operations: 7\n", &args) else { panic!() };
        assert!(p.said.contains("call: { method: GET"), "{}", p.said);
    }

    #[test]
    fn a_choice_the_spec_does_not_have_is_said() {
        let err = from_openapi(&spec(), "https://a.example", "x", "X", &["DELETE /tasks".into()], Auth::None).expect_err("refused");
        assert!(err.contains("no `DELETE /tasks`"), "{err}");
    }
}

/// Fetch a spec, JSON or YAML. A host the person named may be on their own
/// network (an internal API); any other is reached only as a public address.
/// No redirect is followed — an answer that points elsewhere is an answer.
pub async fn fetch_spec(spec_url: &str, said: &[String]) -> Result<Value, String> {
    let named = host_is_named(spec_url, said, None);
    if !named {
        crate::feed_engine::fetcher::guard_url(spec_url)?;
    }
    let mut b = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(concat!("Synabit/", env!("CARGO_PKG_VERSION")));
    if !named {
        b = b.dns_resolver(std::sync::Arc::new(crate::feed_engine::fetcher::PublicOnly));
    }
    let client = b.build().map_err(|e| e.to_string())?;
    let resp = client.get(spec_url).send().await.map_err(|e| format!("could not fetch the spec: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("the spec answered {}", resp.status()));
    }
    let bytes = crate::feed_engine::fetcher::read_capped(resp, 4 * 1024 * 1024).await?;
    let text = String::from_utf8_lossy(&bytes);
    serde_json::from_str::<Value>(&text)
        .or_else(|_| serde_yaml::from_str::<Value>(&text))
        .map_err(|_| "the spec is neither JSON nor YAML".to_string())
}

/// What `connector_draft` did: the answer the model gets, and whether a spec
/// was read — the spec's words are the connector's own, and taint the run as
/// that connector's answers do. See `taint::Origin`.
pub struct Drafted {
    pub content: Value,
    pub read_spec: bool,
}

fn said_problems(problems: &[manifest::Problem]) -> Value {
    json!({
        "saved": false,
        "problems": problems.iter().map(|p| json!({ "code": p.code, "at": p.at, "said": p.said })).collect::<Vec<_>>(),
        "note": "Not saved. Fix these and call connector_draft again; a host problem is a question for the user.",
    })
}

/// Write a draft from `connector_draft`'s arguments and file it. Nothing is
/// approved here: a filed draft is a note, and approving is the person's,
/// on the card. A draft over an approved declaration changes it, which stops
/// it being approved until the person approves it again (`instance`).
pub async fn draft<R: tauri::Runtime>(ctx: &crate::syn::tools::ToolContext<'_, R>, args: &Value, said: &[String]) -> Drafted {
    let field = |k: &str| args.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let said_only = |content: Value| Drafted { content, read_spec: false };
    let (Some(id), Some(name)) = (field("id"), field("name")) else {
        return said_only(json!({ "error": "connector_draft needs `id` and `name`." }));
    };
    let about = field("about").map(str::to_string).unwrap_or_else(|| format!("Connector \u{201c}{name}\u{201d}, written by Syn."));

    // One the person wrote is theirs to change, on the Connectors screen.
    let existing = {
        let db = ctx.db.lock().unwrap_or_else(|e| e.into_inner());
        manifest::all(&db).unwrap_or_default().into_iter().find(|f| f.manifest.as_ref().is_ok_and(|m| m.id == id))
    };
    if existing.as_ref().and_then(|f| f.manifest.as_ref().ok()).is_some_and(|m| m.provenance != Provenance::Syn) {
        return said_only(json!({ "error": format!("`{id}` is a connector the user wrote or one Synabit ships; choose another id, or ask them to change theirs.") }));
    }

    let mut read_spec = false;
    let spec_url = field("openapi_url");
    let chosen: Vec<String> = args
        .get("operations")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let auth_of = |args: &Value| -> Result<Auth, String> {
        match args.get("auth").filter(|a| !a.is_null()) {
            None => Ok(Auth::None),
            Some(a) => serde_json::from_value::<Auth>(a.clone()).map_err(|e| format!("`auth` does not read: {e}")),
        }
    };
    let mut m = if let Some(yaml) = field("yaml") {
        match read_written(yaml, args) {
            Ok(Written::Declaration(m)) => m,
            // A spec pasted in: imported as one, from the address given.
            Ok(Written::Spec(spec)) => {
                let Some(base) = field("base_url") else {
                    return said_only(json!({ "error": "That is an OpenAPI spec. Give `base_url` (the API's address the user named) with it, and `operations` chosen from it." }));
                };
                if chosen.is_empty() {
                    return said_only(json!({ "operations": list(&spec), "note": "Call again with the same yaml and base_url, and `operations` chosen from these." }));
                }
                let auth = match auth_of(args) {
                    Ok(a) => a,
                    Err(e) => return said_only(json!({ "error": e })),
                };
                match from_openapi(&spec, base, id, name, &chosen, auth) {
                    Ok(m) => m,
                    Err(e) => return said_only(json!({ "error": e })),
                }
            }
            Err(p) => return said_only(said_problems(&[p])),
        }
    } else if let Some(spec_url) = spec_url {
        let spec = match fetch_spec(spec_url, said).await {
            Ok(s) => s,
            Err(e) => return said_only(json!({ "error": e })),
        };
        read_spec = true;
        if chosen.is_empty() {
            let listed = list(&spec);
            return Drafted {
                content: json!({
                    "operations": listed,
                    "base_url": base_of(&spec, spec_url),
                    "note": "Call connector_draft again with the same id, name and openapi_url, and `operations`: the ones the user's request needs, as listed. Fewer is better; each send is approved one by one anyway.",
                }),
                read_spec,
            };
        }
        let Some(base) = field("base_url").map(str::to_string).or_else(|| base_of(&spec, spec_url)) else {
            return Drafted { content: json!({ "error": "The spec names no server; give `base_url`." }), read_spec };
        };
        let auth = match auth_of(args) {
            Ok(a) => a,
            Err(e) => return Drafted { content: json!({ "error": e }), read_spec },
        };
        match from_openapi(&spec, &base, id, name, &chosen, auth) {
            Ok(m) => m,
            Err(e) => return Drafted { content: json!({ "error": e }), read_spec },
        }
    } else {
        return said_only(json!({ "error": "Give `openapi_url` (then choose operations) or `yaml`." }));
    };

    // Who wrote it is not the draft's to say.
    m.provenance = Provenance::Syn;
    // A secret the auth names is something to ask for, and how to find it.
    if let Some(name) = m.auth.secret().map(str::to_string) {
        if m.input(&name).is_none() {
            m.inputs.extend(secret_inputs(&m.auth));
        }
        if let Some(guide) = args.get("guide").filter(|g| g.is_object()) {
            if let (Ok(g), Some(input)) = (serde_json::from_value::<manifest::Guide>(guide.clone()), m.inputs.iter_mut().find(|i| i.name == name)) {
                if input.guide.is_none() {
                    input.guide = Some(g);
                }
            }
        }
    }
    m.id = id.to_string();
    // A machine is one connector. Asked to stop a process on the Pi it already
    // reached, the model drafted `raspberry_pi_stop` beside `raspberry_pi_ssh`,
    // and the person got an empty card asking for the Pi's address and
    // password all over again (2026-10-13). A second ssh connector is drafted
    // only when the model says it is another machine.
    if existing.is_none() && m.transport == manifest::Transport::Ssh && args.get("new_machine").and_then(Value::as_bool) != Some(true) {
        let machines: Vec<Value> = {
            let db = ctx.db.lock().unwrap_or_else(|e| e.into_inner());
            manifest::all(&db).unwrap_or_default()
        }
        .into_iter()
        .filter_map(|f| f.manifest.ok())
        .filter(|o| o.transport == manifest::Transport::Ssh && o.provenance == Provenance::Syn)
        .map(|o| json!({ "id": o.id, "name": o.name, "operations": o.operations.iter().map(|op| op.name.clone()).collect::<Vec<_>>() }))
        .collect();
        if !machines.is_empty() {
            return Drafted {
                content: json!({
                    "error": "A machine reached over ssh is one connector, and these already exist. To add a command to one of them, call connector_draft again with its id and name and only the new operation — what it has is kept. Only if the user means a different machine, call again with `new_machine: true`.",
                    "ssh_connectors": machines,
                }),
                read_spec,
            };
        }
    }
    // A draft over one Syn already wrote adds to it. Asked for disk usage, the
    // model wrote a declaration with `df -h` alone, and the Pi's hostname and
    // uptime were gone; asked about a service next, `df -h` went too
    // (2026-10-13). What it means to take away it names in `drop`.
    let dropped: Vec<&str> = args
        .get("drop")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let kept = match existing.as_ref().and_then(|f| f.manifest.as_ref().ok()) {
        Some(before) => keep_operations(&mut m, before, &dropped),
        None => Vec::new(),
    };
    let problems = check(&m, said, spec_url);
    if !problems.is_empty() {
        return Drafted { content: said_problems(&problems), read_spec };
    }
    let note = match body(&m, &about) {
        Ok(b) => b,
        Err(e) => return Drafted { content: json!({ "error": e }), read_spec },
    };
    let filed = match existing {
        Some(f) => crate::syn::tools::execute_tool(ctx, "update_node", &json!({ "node_id": f.node_id, "content": note })),
        None => crate::syn::tools::execute_tool(
            ctx,
            "create_node",
            &json!({ "node_type": manifest::CONNECTOR_TYPE, "title": m.name, "content": note }),
        ),
    };
    if let Err(e) = filed {
        return Drafted { content: json!({ "error": format!("The draft could not be filed: {e}") }), read_spec };
    }
    Drafted {
        content: json!({
            "saved": true,
            "id": m.id,
            "host": url::Url::parse(&m.base_url).ok().and_then(|u| u.host_str().map(str::to_string)),
            "operations": m.operations.iter().map(|o| json!({
                "tool": manifest::tool_name(&m, o),
                "does": format!("{} {}", o.call.method, o.call.path),
                "sends": o.effect >= Effect::Write,
            })).collect::<Vec<_>>(),
            "kept_from_before": kept,
            "note": "Drafted, not usable yet. Call connector_connect with this id so the user can approve it.",
        }),
        read_spec,
    }
}

/// Carry into `m` the operations of `before` it does not name, except those in
/// `dropped`; a name in both is the new draft's. Returns what was carried.
fn keep_operations(m: &mut Manifest, before: &Manifest, dropped: &[&str]) -> Vec<String> {
    let mut kept = Vec::new();
    for op in &before.operations {
        if dropped.contains(&op.name.as_str()) || m.operations.iter().any(|o| o.name == op.name) {
            continue;
        }
        kept.push(op.name.clone());
        m.operations.push(op.clone());
    }
    kept
}

pub const FIND_TOOL: &str = "connector_find";
pub const TRY_TOOL: &str = "connector_try";

/// Rows of a try the model is shown.
const TRY_SAMPLE: usize = 5;

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(str::to_string)
        .collect()
}

/// How many of `need`'s words are in `about`.
fn score(need: &[String], about: &str) -> usize {
    let about = about.to_lowercase();
    need.iter().filter(|w| about.contains(w.as_str())).count()
}

/// What Syn can already reach, before it writes anything: declarations in the
/// vault (and where each stands here), the ones Synabit ships, and connected
/// MCP servers. A connector written again beside one that exists is two
/// approvals for one service, and two names the gate weighs.
pub fn find(db: &crate::db::DbBridge, vault_path: &str, need: &str) -> Value {
    let need = words(need);
    let mut found: Vec<(usize, Value)> = Vec::new();
    let mut in_vault = std::collections::BTreeSet::new();
    for f in manifest::all(db).unwrap_or_default() {
        let Ok(m) = f.manifest else { continue };
        in_vault.insert(m.id.clone());
        let standing = super::instance::standing(vault_path, &m);
        let ops: Vec<String> = m.operations.iter().map(|o| format!("{}: {}", o.name, o.description)).collect();
        let how = match standing {
            super::instance::Standing::Approved => format!("Approved: its tools are connector__{}__<operation>; bring them with find_tools \"connector:{}\".", m.id, m.id),
            super::instance::Standing::Broken => "Has problems: fix it with connector_draft (same id).".to_string(),
            _ => format!("Not approved on this device: connector_connect {{\"id\":\"{}\"}}.", m.id),
        };
        let s = score(&need, &format!("{} {} {} {}", m.id, m.name, m.base_url, ops.join(" ")));
        found.push((s, json!({ "kind": "declared", "id": m.id, "name": m.name, "host": url::Url::parse(&m.base_url).ok().and_then(|u| u.host_str().map(str::to_string)), "operations": ops, "how": how })));
    }
    for server in super::provider::catalog_of(vault_path) {
        let Some(first) = server.tools.first() else { continue };
        let slug = super::provider::server_slug_of(&first.name).unwrap_or_default().to_string();
        let tools: Vec<String> = server.tools.iter().map(|t| t.name.clone()).collect();
        let s = score(&need, &format!("{} {} {}", first.server_name, slug, server.tools.iter().map(|t| t.description.as_str()).collect::<Vec<_>>().join(" ")));
        found.push((s, json!({ "kind": "mcp", "id": slug, "name": first.server_name, "tools": tools,
            "how": format!("Connected MCP server: bring its tools with find_tools \"connector:{slug}\".") })));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    let matched: Vec<Value> = found.iter().filter(|(s, _)| *s > 0).map(|(_, v)| v.clone()).collect();
    if matched.is_empty() {
        json!({
            "matched": [],
            "others": found.into_iter().map(|(_, v)| json!({ "kind": v["kind"], "id": v["id"], "name": v["name"] })).collect::<Vec<_>>(),
            "note": "Nothing here does that. Draft one with connector_draft: from its OpenAPI spec if it has one, else as yaml from its documentation.",
        })
    } else {
        json!({ "matched": matched, "note": "Use one of these rather than drafting a new one, unless none does what the user asked." })
    }
}

/// Whether a host is this machine or a private network: a draft that reaches
/// one is tried as the person's own network, which the host check already
/// required they named.
fn is_local(base_url: &str) -> bool {
    let Some(host) = url::Url::parse(base_url).ok().and_then(|u| u.host_str().map(str::to_lowercase)) else { return false };
    if host == "localhost" || host.ends_with(".local") || host.ends_with(".localhost") {
        return true;
    }
    match host.trim_matches(|c| c == '[' || c == ']').parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => false,
    }
}

/// Call one *reading* operation of a draft, once, before it is proposed — so
/// a wrong path or a wrong `each` shows now and not after the person approved
/// it. Nothing is kept: the rows are a sample for the model to check the
/// declaration against. A draft that needs a secret cannot be tried, because
/// the secret is typed on the card; it is proposed and tried after.
pub async fn try_once(m: &Manifest, op_name: &str, args: &Value, said: &[String]) -> Value {
    let Some(op) = m.operations.iter().find(|o| o.name == op_name) else {
        let names: Vec<&str> = m.operations.iter().map(|o| o.name.as_str()).collect();
        return json!({ "error": format!("`{}` has no operation `{op_name}`; it has {names:?}.", m.id) });
    };
    if op.effect != Effect::Read {
        return json!({ "error": "Only an operation declared `read` can be tried. A send goes through propose_writes once the connector is approved." });
    }
    let problems = check(m, said, None);
    if !problems.is_empty() {
        return said_problems(&problems);
    }
    if m.auth.secret().is_some() {
        return json!({ "error": "It needs a secret, which the user types on the approval card. Propose it with connector_connect; try it after." });
    }
    let instance = super::instance::Instance {
        manifest_id: m.id.clone(),
        label: String::new(),
        zone: Default::default(),
        exposure: Default::default(),
        network: if is_local(&m.base_url) { super::instance::Network::Internal } else { super::instance::Network::Public },
        pinned_cert: None,
        attested: Default::default(),
        mirrors: Default::default(),
        folder: None,
        inputs: Default::default(),
        operations: Default::default(),
        approved: "a try".into(),
        approved_at: String::new(),
    };
    match super::http::fetch(m, &instance, op, args, None).await {
        Ok(fetched) => json!({
            "ok": true,
            "asked": fetched.asked,
            "rows": fetched.rows.len(),
            "columns": fetched.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "sample": fetched.rows.iter().take(TRY_SAMPLE).collect::<Vec<_>>(),
            "missing": fetched.missing,
            "note": "A try, not kept. If the columns or rows are not what the user needs, fix the draft (result.each, pick) with connector_draft and try again; otherwise connector_connect.",
        }),
        Err(failed) => json!({
            "ok": false,
            "error": failed.said,
            "note": "Fix the draft with connector_draft (path, params, base_url) and try again.",
        }),
    }
}

/// The format, shown with a declaration that does not read. A model that has
/// never seen one guesses; the 2026-10-11 eval had it write OpenAPI, a
/// `transport: { type: http }` block, `method` beside `call`, until it ran out
/// of rounds.
pub const EXAMPLE: &str = "id: rates\nname: Rates\nbase_url: https://api.example.com/v1\nauth: { type: none }   # or { type: bearer, secret: rates_token }\noperations:\n  - name: on_date\n    description: Rates on one day.\n    effect: read          # read | write | irreversible\n    call: { method: GET, path: \"/{{date}}\", query: { from: \"{{from}}\" } }\n    params:\n      date: { required: true, description: \"YYYY-MM-DD\" }\n      from: { description: \"Currency code\" }\n    result: { each: \"$\" }  # or \"$.items[*]\"; the rows\n";

/// The same for a machine reached over SSH: the address and the login typed
/// on the card, one command per thing to do.
pub const SSH_EXAMPLE: &str = "id: pi\nname: Raspberry Pi\ntransport: ssh\nbase_url: \"{{input.host}}\"\nauth: { type: ssh, secret: login }\ninputs:\n  - { name: host, kind: address, label: { en: Address, vi: Địa chỉ }, example: \"ssh://pi@raspberrypi.local\" }\n  - { name: login, kind: secret, label: { en: Password or private key, vi: Mật khẩu hoặc khoá riêng } }\noperations:\n  - name: restart_service\n    description: Restart one service.\n    effect: write         # read_attested for one that only looks\n    command: [sudo, systemctl, restart, \"{{unit}}\"]\n    params:\n      unit: { required: true, description: \"The service, e.g. nginx\" }\n";

fn placeholders_in(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.extend(manifest::placeholders(s)),
        Value::Array(a) => a.iter().for_each(|x| placeholders_in(x, out)),
        Value::Object(o) => o.values().for_each(|x| placeholders_in(x, out)),
        _ => {}
    }
}

/// `{x}` → `{{x}}`, leaving `{{x}}` as it is.
fn doubled(s: &str) -> String {
    let re = regex::Regex::new(r"\{\{?([A-Za-z_][A-Za-z0-9_]*)\}\}?").expect("pattern");
    re.replace_all(s, "{{$1}}").into_owned()
}

fn doubled_in(v: &mut Value) {
    match v {
        Value::String(s) => *s = doubled(s),
        Value::Array(a) => a.iter_mut().for_each(doubled_in),
        Value::Object(o) => o.values_mut().for_each(doubled_in),
        _ => {}
    }
}

/// A declaration as a model writes it, brought to the format where the
/// difference is only how it is written: `method`/`path` beside `call`, `call:
/// "GET /x"`, `{x}` for `{{x}}`, a missing effect (by method), missing params
/// for the placeholders, `transport: { type: http, base_url }`, the top-level
/// fields given as arguments. Nothing here widens what it may do — the card
/// shows the result, and `manifest::problems` and the host check still apply.
pub fn normalize(mut m: Value, args: &Value) -> Value {
    let Some(top) = m.as_object_mut() else { return m };
    for key in ["id", "name", "base_url"] {
        if !top.contains_key(key) {
            if let Some(v) = args.get(key) {
                top.insert(key.into(), v.clone());
            }
        }
    }
    if let Some(t) = top.get("transport").cloned().filter(Value::is_object) {
        if let Some(b) = t.get("base_url").filter(|_| !top.contains_key("base_url")) {
            top.insert("base_url".into(), b.clone());
        }
        top.insert("transport".into(), t.get("type").cloned().unwrap_or(json!("http")));
    }
    if let Some(d) = top.get("decoder").cloned().filter(Value::is_object) {
        top.insert("decoder".into(), d.get("type").cloned().unwrap_or(json!("json")));
    }
    // Operations written under `logic`, which names how a connector works.
    if !top.contains_key("operations") {
        if let Some(l) = top.get("logic").cloned().filter(|l| !l.is_string()) {
            let ops = l.get("operations").cloned().unwrap_or(l.clone());
            if let Some(b) = l.get("base_url").filter(|_| !top.contains_key("base_url")) {
                top.insert("base_url".into(), b.clone());
            }
            top.insert("operations".into(), ops);
            top.remove("logic");
        }
    }
    if !top.contains_key("auth") {
        top.insert("auth".into(), args.get("auth").cloned().unwrap_or(json!({ "type": "none" })));
    }
    if let Some(v) = top.get("version").and_then(Value::as_str).and_then(|s| s.parse::<u64>().ok()) {
        top.insert("version".into(), json!(v));
    }
    // Who wrote it is set after, never read from here.
    top.remove("provenance");

    let ssh = top.get("transport").and_then(Value::as_str) == Some("ssh");
    if let Some(ops) = top.get_mut("operations").and_then(Value::as_array_mut) {
        for op in ops.iter_mut() {
            let Some(o) = op.as_object_mut() else { continue };
            // An ssh operation is a command: written as one string, it is
            // split into its words; nothing is guessed about a call.
            if ssh || o.contains_key("command") {
                normalize_command(o);
                continue;
            }
            if let Some(Value::String(c)) = o.get("call").cloned() {
                let (method, path) = c.trim().split_once(' ').unwrap_or(("GET", c.trim()));
                o.insert("call".into(), json!({ "method": method, "path": path.trim() }));
            }
            let mut call = o.get("call").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            for key in ["method", "path", "query", "headers", "json", "form"] {
                if let Some(v) = o.remove(key) {
                    call.as_object_mut().expect("object").entry(key).or_insert(v);
                }
            }
            if call.get("method").is_none() {
                call["method"] = json!("GET");
            }
            if let Some(m) = call.get("method").and_then(Value::as_str) {
                call["method"] = json!(m.to_ascii_uppercase());
            }
            doubled_in(&mut call);
            let method = call["method"].as_str().unwrap_or("GET").to_string();
            let mut wanted = Vec::new();
            placeholders_in(&call, &mut wanted);
            let in_path: Vec<String> = call.get("path").and_then(Value::as_str).map(manifest::placeholders).unwrap_or_default();
            o.insert("call".into(), call);
            if !o.contains_key("description") {
                let d = o.get("summary").cloned().or_else(|| o.get("name").cloned()).unwrap_or(json!(""));
                o.insert("description".into(), d);
            }
            o.remove("summary");
            if !o.contains_key("effect") {
                o.insert("effect".into(), json!(if method == "GET" { "read" } else { "write" }));
            }
            let params = o.entry("params").or_insert_with(|| json!({}));
            if let Some(p) = params.as_object_mut() {
                for v in p.values_mut() {
                    if let Value::String(t) = v {
                        *v = json!({ "type": t });
                    }
                }
                for name in wanted {
                    p.entry(name.clone()).or_insert_with(|| json!({ "required": in_path.contains(&name) }));
                }
                // A value in the path is needed whatever the draft said of it:
                // `/v1/{{date}}` without a date is no request at all. The
                // 2026-10-26 eval: `date` declared without `required`, the
                // card's try called the operation with nothing, and "`date` is
                // needed" read as the connector not working.
                for name in &in_path {
                    if let Some(Value::Object(param)) = p.get_mut(name) {
                        param.insert("required".into(), json!(true));
                    }
                }
            }
        }
    }
    m
}

/// An ssh operation as a model writes it: `command: "df -h {x}"` as words,
/// `{x}` as `{{x}}`, a missing effect as a send, missing params for the
/// placeholders. A command written as one string is split at spaces only —
/// quoting is not read, so `"a b"` stays two words and the card shows them.
fn normalize_command(o: &mut serde_json::Map<String, Value>) {
    if let Some(Value::String(line)) = o.get("command").cloned() {
        o.insert("command".into(), json!(line.split_whitespace().collect::<Vec<_>>()));
    }
    let mut command = o.get("command").cloned().unwrap_or_else(|| json!([]));
    doubled_in(&mut command);
    let mut wanted = Vec::new();
    placeholders_in(&command, &mut wanted);
    o.insert("command".into(), command);
    if !o.contains_key("description") {
        let d = o.get("summary").cloned().or_else(|| o.get("name").cloned()).unwrap_or(json!(""));
        o.insert("description".into(), d);
    }
    o.remove("summary");
    o.entry("effect").or_insert(json!("write"));
    let params = o.entry("params").or_insert_with(|| json!({}));
    if let Some(p) = params.as_object_mut() {
        for v in p.values_mut() {
            if let Value::String(t) = v {
                *v = json!({ "type": t });
            }
        }
        for name in wanted.into_iter().filter(|n| !n.starts_with("input.")) {
            p.entry(name).or_insert_with(|| json!({ "required": true }));
        }
    }
}

/// What `yaml` in `connector_draft` holds, read: a declaration, made
/// readable where it is only written differently, or — for a model that
/// pasted the service's OpenAPI instead — that spec, imported as a spec is.
pub enum Written {
    Declaration(Manifest),
    Spec(Value),
}

pub fn read_written(text: &str, args: &Value) -> Result<Written, manifest::Problem> {
    let raw: Value = serde_yaml::from_str(text).map_err(|e| manifest::Problem {
        code: "not_readable",
        at: String::new(),
        said: format!("not YAML: {e}. The format:\n{EXAMPLE}\nFor a machine reached over SSH:\n{SSH_EXAMPLE}"),
    })?;
    if raw.get("openapi").is_some() || raw.get("swagger").is_some() || raw.get("paths").is_some() {
        return Ok(Written::Spec(raw));
    }
    let shaped = serde_yaml::to_string(&normalize(raw, args)).map_err(|e| manifest::Problem { code: "not_readable", at: String::new(), said: e.to_string() })?;
    manifest::parse_yaml(&shaped).map(Written::Declaration).map_err(|mut p| {
        if p.code == "not_readable" {
            p.said = format!("{}. The format:\n{EXAMPLE}\nFor a machine reached over SSH:\n{SSH_EXAMPLE}", p.said);
        }
        p
    })
}
