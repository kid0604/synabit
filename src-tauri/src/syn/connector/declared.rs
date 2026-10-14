//! Declared connectors, offered to the model and carried out.
//!
//! The bridge between a declaration this device approved (`manifest`,
//! `instance`) and the run: its operations are tools named like any
//! connector's — `connector__<id>__<operation>` — so the gate, the consent
//! card, the taint, the tool groups, the labels and the audit treat a
//! declared connector and an MCP server alike. What differs is only how a
//! call is carried out: by `http`, from the declaration, here.
//!
//! The list of approved declarations is read when the tools are offered (once
//! a turn) and kept, because the call path is not handed the database. Every
//! call checks the approval again against the file, so a declaration changed
//! between the two is not carried out on the old approval.

use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock, RwLock};

use serde_json::{json, Value};

use super::instance::{self, Instance};
use super::manifest::{self, Effect, Manifest, Operation};
use super::{Called, Table};
use crate::models::syn::{FunctionDefinition, ToolDefinition};
use crate::syn::consent::Capability;
use crate::syn::registry::Reversal;

/// A declaration this device approved, with its approval.
#[derive(Debug, Clone)]
pub struct Declared {
    pub manifest: Manifest,
    pub instance: Instance,
}

/// Approved declarations, by vault, as last read.
static DECLARED: LazyLock<RwLock<HashMap<String, Vec<Declared>>>> = LazyLock::new(Default::default);

/// The app, for the keychain on Android. Set the first time tools are offered.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn know_app(app: Option<tauri::AppHandle>) {
    if let Some(app) = app {
        let _ = APP.set(app);
    }
}

/// Read the vault's declarations again and keep the approved ones.
///
/// A declaration whose `id` is also an MCP server's slug is left out: two
/// connectors answering to one name would let the gate weigh one and the call
/// reach the other.
pub fn refresh(vault_path: &str, db: &crate::db::DbBridge, mcp_slugs: &[String]) -> Vec<Declared> {
    let found = manifest::all(db).unwrap_or_default();
    let approved: Vec<Declared> = found
        .into_iter()
        .filter_map(|f| f.manifest.ok())
        .filter(|m| {
            let clash = mcp_slugs.iter().any(|s| s == &m.id);
            if clash {
                log::warn!("[Syn] The declared connector `{}` has the name of an MCP server and is not offered", m.id);
            }
            !clash
        })
        .filter_map(|m| instance::approved_here(vault_path, &m).map(|instance| Declared { manifest: m, instance }))
        .collect();
    if let Ok(mut all) = DECLARED.write() {
        all.insert(vault_path.to_string(), approved.clone());
    }
    approved
}

/// `run`, for a connector that reaches a machine over ssh. See `ssh::RUN_OP`.
fn run_op(m: &Manifest) -> Operation {
    serde_json::from_value(json!({
        "name": super::ssh::RUN_OP,
        "description": format!(
            "Run one command line on {} and read what it prints. The person sees the line exactly as written and \
             allows it before it runs, every time, so write the line you mean: a shell reads it. No terminal: \
             interactive programs (htop, top without -b, less, vim, watch) do not work — use `top -bn1`, \
             `ps aux --sort=-%cpu | head`. Use it for what the connector's own operations do not cover; \
             do not draft a new operation for a one-off.",
            m.name
        ),
        "effect": "write",
        "params": { "command": { "type": "string", "required": true, "description": "The command line, e.g. `df -h /` or `systemctl --user stop openclaw-gateway`." } },
    }))
    .expect("run is an operation")
}

/// What `d` can be asked to do: what it declares, and `run` where it reaches a
/// machine over ssh and declares no operation of that name.
pub fn operations(d: &Declared) -> Vec<Operation> {
    let mut ops = d.manifest.operations.clone();
    if d.manifest.transport == manifest::Transport::Ssh && !ops.iter().any(|o| o.name == super::ssh::RUN_OP) {
        ops.push(run_op(&d.manifest));
    }
    ops
}

/// Whether `op` is the ssh `run` rather than a declared operation.
fn is_run(d: &Declared, op: &Operation) -> bool {
    d.manifest.transport == manifest::Transport::Ssh && op.name == super::ssh::RUN_OP && op.command.is_empty()
}

/// Whether a yes to `tool` is a yes to one call, never to the tool: `run`,
/// whose every call is a different command. See `gate`.
pub fn asks_per_call(tool: &str) -> bool {
    find_anywhere(tool).is_some_and(|(d, op)| is_run(&d, &op))
}

fn split(name: &str) -> Option<(&str, &str)> {
    name.strip_prefix(super::PREFIX)?.split_once("__")
}

/// The declaration and operation a tool name stands for, in `vault_path`.
pub fn find(vault_path: &str, name: &str) -> Option<(Declared, Operation)> {
    let (id, op) = split(name)?;
    let all = DECLARED.read().ok()?;
    let d = all.get(vault_path)?.iter().find(|d| d.manifest.id == id)?.clone();
    let op = operations(&d).into_iter().find(|o| o.name == op)?;
    Some((d, op))
}

/// The same in whichever vault holds it — for `ToolProvider::capability`,
/// which is not told the vault. `call` checks again in the right one.
///
/// Two vaults may each declare a connector of this id. The one that has this
/// operation is taken, and when more than one does and they disagree, the
/// stricter — a send over a read — as `provider::capability_by_name` does.
pub fn find_anywhere(name: &str) -> Option<(Declared, Operation)> {
    let (id, op_name) = split(name)?;
    let all = DECLARED.read().ok()?;
    let mut found: Vec<(Declared, Operation)> = all
        .values()
        .flatten()
        .filter(|d| d.manifest.id == id)
        .filter_map(|d| operations(d).into_iter().find(|o| o.name == op_name).map(|o| (d.clone(), o)))
        .collect();
    found.sort_by_key(|(d, o)| std::cmp::Reverse(instance::effect_here(&d.instance, o)));
    found.into_iter().next()
}

/// What a call would do, in words Rust vouches for, for the consent card: the
/// command line an ssh operation would send, quoted as it would go. `None`
/// for anything else, and for a call that would not be sent as it stands.
pub fn detail(vault_path: &str, tool: &str, args: &Value) -> Option<String> {
    let (d, op) = find(vault_path, tool)?;
    if d.manifest.transport != manifest::Transport::Ssh {
        return None;
    }
    if is_run(&d, &op) {
        return super::ssh::run_line(args).ok();
    }
    let filled = d.manifest.with_inputs(&d.instance.inputs);
    let op = filled.operations.iter().find(|o| o.name == op.name)?;
    super::ssh::command_line(op, args).ok()
}

/// Whether "always" may be said about a send through `tool`.
///
/// A remembered send is a send nobody looks at again. That is fair where the
/// declaration leaves nothing to point elsewhere — every parameter a switch
/// or one of a fixed list — and not where a parameter is free text: "always
/// send mail" is "always send any mail, to anyone, saying anything", which is
/// what an instruction in a page asks for. So it is offered only for the
/// first kind; an ssh command, whose words run on a machine, never; an MCP
/// tool, whose parameters nothing here describes, never.
pub fn write_may_be_remembered(tool: &str) -> bool {
    let Some((d, op)) = find_anywhere(tool) else { return false };
    d.manifest.transport != manifest::Transport::Ssh
        && op.params.values().all(|p| p.ty == "boolean" || !p.choices.is_empty())
}

/// Whether a tool sends something that cannot be taken back.
pub fn is_irreversible(name: &str) -> bool {
    find_anywhere(name).is_some_and(|(d, op)| instance::effect_here(&d.instance, &op) == Effect::Irreversible)
}

/// What permission an operation needs on this instance: reading from it, or
/// sending to it. `read_attested` counts as reading only where the person
/// vouched for it (`instance::effect_here`). `run` reads when its line only
/// looks (`ssh::reads_only`) and sends otherwise — and without its arguments,
/// sends.
pub fn capability(d: &Declared, op: &Operation, args: &Value) -> Capability {
    if is_run(d, op) {
        return match super::ssh::run_line(args) {
            Ok(line) if super::ssh::reads_only(&line) => Capability::NetRead { domain: d.manifest.name.clone() },
            _ => Capability::NetWrite { domain: d.manifest.name.clone(), tool: op.name.clone() },
        };
    }
    match instance::effect_here(&d.instance, op) {
        Effect::Read | Effect::ReadAttested => Capability::NetRead { domain: d.manifest.name.clone() },
        Effect::Write | Effect::Irreversible => Capability::NetWrite { domain: d.manifest.name.clone(), tool: op.name.clone() },
    }
}

/// The tool the model is offered for an operation.
pub fn definition(d: &Declared, op: &Operation) -> ToolDefinition {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, p) in &op.params {
        let mut schema = json!({ "type": p.ty, "description": p.description });
        if !p.choices.is_empty() {
            schema["enum"] = json!(p.choices);
        }
        properties.insert(name.clone(), schema);
        if p.required {
            required.push(name.clone());
        }
    }
    let language = op.language.as_deref().map(|l| format!(" Queries are written in {l}.")).unwrap_or_default();
    ToolDefinition {
        tool_type: "function".to_string(),
        function: FunctionDefinition {
            name: manifest::tool_name(&d.manifest, op),
            description: format!(
                "From the connector \u{201c}{}\u{201d}: {}{language} The answer comes back as a table with a handle; \
                 use crunch on it for totals and counts.",
                d.manifest.name,
                op.description.trim()
            ),
            parameters: json!({ "type": "object", "properties": properties, "required": required }),
        },
    }
}

/// Secrets tests hand over, by slot. Tests never touch the real keychain.
#[cfg(test)]
pub static TEST_SECRETS: LazyLock<std::sync::Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

#[cfg(test)]
pub(crate) async fn read_secret(slot: String) -> Option<String> {
    TEST_SECRETS.lock().ok()?.get(&slot).cloned()
}

/// One secret from this device's keychain, off the async threads and with
/// the patience `read_secrets` has: a keychain dialog nobody answers is a
/// missing secret, not a run that never ends.
#[cfg(not(test))]
pub(crate) async fn read_secret(slot: String) -> Option<String> {
    let app = APP.get().cloned();
    let read = tokio::task::spawn_blocking(move || crate::secrets::SecretManager::get_syn_api_key(app.as_ref(), &slot));
    tokio::time::timeout(std::time::Duration::from_secs(8), read).await.ok()?.ok()?
}

/// Carry out a declared operation, as the engine does once the gate said go.
pub async fn call(vault_path: &str, found: (Declared, Operation), args: &Value, consented: Option<&Capability>) -> Called {
    let (d, op) = found;
    let fail = |said: String, answered: bool| Called {
        content: json!({ "error": if answered { super::wrap(&d.manifest.name, &op.name, &said) } else { said.clone() } }).to_string(),
        server_answered: answered,
        ok: false,
        reversal: Reversal::Nothing,
        table: None,
    };

    // Approved still, exactly as it was when it was offered?
    match instance::approved_here(vault_path, &d.manifest) {
        Some(now) if now == d.instance => {}
        _ => return fail(format!("{} changed since it was approved on this device; it must be approved again.", d.manifest.name), false),
    }
    let capability = capability(&d, &op, args);
    if consented != Some(&capability) {
        return fail(format!("`{}` changed since permission was weighed; ask for it again.", op.name), false);
    }
    let reversal = crate::syn::registry::reversal_of(&capability);

    let secret = match d.manifest.auth.secret() {
        None => None,
        Some(name) => {
            let slot = crate::commands::connector::secret_slot(&d.manifest.id, name);
            match read_secret(slot).await {
                Some(s) => Some(s),
                None => return fail(format!("{} needs its secret, and this device does not have it. Add it on the approval card.", d.manifest.name), false),
            }
        }
    };

    // What goes into a spreadsheet or a page is made inert there. See `writes::neutralize`.
    let args = &crate::syn::writes::neutralize(op.sink.as_deref(), args);
    match super::http::fetch(&d.manifest, &d.instance, &op, args, secret.as_deref()).await {
        Ok(fetched) => Called {
            content: String::new(),
            server_answered: true,
            ok: true,
            reversal,
            table: Some(Table {
                server: d.manifest.name.clone(),
                tool: op.name.clone(),
                chars: 0,
                at: None,
                columns: fetched.columns,
                rows: fetched.rows,
                missing: fetched.missing,
                exposure: d.instance.exposure,
                asked: Some(fetched.asked),
            }),
        },
        Err(failed) => Called { reversal: if failed.answered { reversal } else { Reversal::Nothing }, ..fail(failed.said, failed.answered) },
    }
}
