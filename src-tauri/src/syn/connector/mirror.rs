//! Mirrors: a connector's reading operation, kept in the vault.
//!
//! `docs/syn-connectors-2026-10-08.md` §5. A calendar, a contact list, a
//! bank's notifications — read on a schedule by Rust, with no model, and
//! upserted as notes by key. Nothing here is a run: no prompt, no tool, no
//! consent card. What makes it allowed is the person ticking the mirror on the
//! approval card, on this device.
//!
//! # What keeps it honest
//!
//! - **Only `read` operations** (`manifest::problems`): it runs unattended.
//! - **One note per key, wherever it runs.** A note's path is a hash of the
//!   mirror and the row's key, under `<folder>/mirror-<id>/`, so two devices
//!   mirroring the same calendar write the same note and sync merges it —
//!   never two. Which device runs it is the person's choice per device; both
//!   may, and nothing doubles.
//! - **The remote is the source.** A mirrored note edited in the app is
//!   written over on the next run; it is changed through the service. (Two-way
//!   is open question §9.2.)
//! - **What it wrote is somebody else's words.** A note under `mirror-<id>/`
//!   taints a run that reads it, as that connector's answer would (lesson S5;
//!   `taint::mirrored`), and retrieval counts it too.
//! - **Never into what the app computes with.** Transactions go to staging
//!   (`syn::staging`), each key once, for the person to confirm in Finance
//!   (lesson S4).
//! - **Gone is gone only when the answer was whole.** A note whose key the
//!   service no longer returns is moved to the trash — only when the read was
//!   complete, and for a windowed mirror only inside the window.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::instance::Instance;
use super::manifest::{Manifest, Mirror};

/// Notes a mirror writes live under `<folder>/mirror-<connector id>/`.
pub const DIR_PREFIX: &str = "mirror-";

/// How a mirror last went on this device.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Ran {
    pub at: i64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub said: String,
    pub rows: usize,
    pub written: usize,
    pub trashed: usize,
    pub staged: usize,
    /// For a mirror into staging: every key already staged, so a run stages
    /// only what is new.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub staged_keys: BTreeSet<String>,
}

fn state_path(vault_path: &str) -> std::path::PathBuf {
    std::path::Path::new(vault_path).join(".synabit").join("mirror-state.json")
}

pub fn state(vault_path: &str) -> BTreeMap<String, Ran> {
    std::fs::read(state_path(vault_path)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_state(vault_path: &str, all: &BTreeMap<String, Ran>) {
    let p = state_path(vault_path);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&p, serde_json::to_vec_pretty(all).unwrap_or_default()) {
        log::error!("[Syn] Could not keep the mirror state: {e}");
    }
}

pub fn state_key(m: &Manifest, mi: &Mirror) -> String {
    format!("{}/{}", m.id, mi.name)
}

/// Where the note for `key` lives: the same on every device.
pub fn path_for(m: &Manifest, mi: &Mirror, key: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = hex::encode(Sha256::digest(format!("{}\u{1f}{key}", state_key(m, mi)).as_bytes()));
    let folder = crate::syn::tools::folder_for_type(&mi.into);
    format!("{folder}/{DIR_PREFIX}{}/{}.md", m.id, &hash[..16])
}

/// The connector a note was mirrored from, by its path.
pub fn connector_of_path(id: &str) -> Option<String> {
    let at = id.find(&format!("/{DIR_PREFIX}"))?;
    let rest = &id[at + 1 + DIR_PREFIX.len()..];
    let name = rest.split('/').next()?;
    (!name.is_empty() && rest.contains('/')).then(|| name.to_string())
}

/// The window's values, as a mirror's arguments name them.
fn window_values(mi: &Mirror, now: chrono::DateTime<chrono::Utc>) -> Vec<(String, String)> {
    let Some(w) = mi.window else { return Vec::new() };
    let day = now.date_naive();
    let start = day - chrono::Duration::days(w.past_days as i64);
    let end = day + chrono::Duration::days(w.future_days as i64 + 1);
    vec![
        ("window_start".into(), format!("{}T000000Z", start.format("%Y%m%d"))),
        ("window_end".into(), format!("{}T000000Z", end.format("%Y%m%d"))),
        ("window_start_date".into(), start.format("%Y-%m-%d").to_string()),
        ("window_end_date".into(), end.format("%Y-%m-%d").to_string()),
    ]
}

fn fill(template: &str, values: &[(String, String)]) -> String {
    let mut out = template.to_string();
    for (k, v) in values {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    }
}

/// A note's title, body and fields, from one row.
fn note_of(mi: &Mirror, m: &Manifest, key: &str, get: &dyn Fn(&str) -> Option<Value>) -> (String, String, serde_json::Map<String, Value>) {
    let text = |field: &str| mi.fields.get(field).and_then(|col| get(col)).map(|v| cell_text(&v)).unwrap_or_default();
    let title = Some(text("title")).filter(|t| !t.is_empty()).unwrap_or_else(|| key.to_string());
    let content = text("content");
    let mut props = serde_json::Map::new();
    for (field, col) in &mi.fields {
        if field == "title" || field == "content" {
            continue;
        }
        let Some(v) = get(col).filter(|v| !v.is_null() && !cell_text(v).is_empty()) else { continue };
        match (mi.into.as_str(), field.as_str()) {
            ("person", "email" | "phone") => {
                let values: Vec<String> = match &v {
                    Value::Array(a) => a.iter().map(cell_text).collect(),
                    other => vec![cell_text(other)],
                };
                let details = props.entry("details").or_insert_with(|| json!([]));
                for value in values.into_iter().filter(|s| !s.is_empty()) {
                    let (label, ty) = if field == "email" { ("Email", "email") } else { ("Phone", "phone") };
                    if let Value::Array(a) = details {
                        a.push(json!({ "label": label, "value": value, "type": ty }));
                    }
                }
            }
            (_, "tags") => {
                let tags: Vec<String> = match &v {
                    Value::Array(a) => a.iter().map(cell_text).collect(),
                    other => cell_text(other).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
                };
                props.insert("tags".into(), json!(tags));
            }
            ("event", "start_at" | "end_at") => {
                let t = cell_text(&v);
                // A moment in UTC is kept as the calendar keeps one: the clock, and its zone.
                match t.strip_suffix('Z') {
                    Some(clock) => {
                        props.insert(field.clone(), json!(clock));
                        props.entry("tzid").or_insert_with(|| json!("UTC"));
                    }
                    None => {
                        props.insert(field.clone(), json!(t));
                    }
                }
            }
            _ => {
                props.insert(field.clone(), json!(cell_text(&v)));
            }
        }
    }
    if mi.into == "event" && !props.contains_key("is_all_day") {
        let all_day = props.get("start_at").and_then(Value::as_str).is_some_and(|s| s.len() == 10);
        props.insert("is_all_day".into(), json!(all_day));
    }
    props.insert("origin".into(), json!(format!("connector:{}", m.id)));
    props.insert("mirror".into(), json!(format!("{}/{}", m.id, mi.name)));
    props.insert("mirror_key".into(), json!(key));
    (title, content, props)
}

/// Whether the note at a path already says exactly this.
fn same(existing: &crate::models::node::NodeMetadata, title: &str, content: &str, props: &serde_json::Map<String, Value>) -> bool {
    existing.title == title
        && existing.content.trim() == content.trim()
        && props.iter().all(|(k, v)| existing.properties.get(k) == Some(v))
}

/// Run one mirror now: read, then upsert (or stage), then trash what is gone.
pub async fn run_one<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    db: &crate::db::DbState,
    vault_path: &str,
    m: &Manifest,
    inst: &Instance,
    mi: &Mirror,
    secret: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Ran, String> {
    let op = m.operations.iter().find(|o| o.name == mi.from).ok_or("its operation is gone")?;
    let values = window_values(mi, now);
    let args = Value::Object(mi.args.iter().map(|(k, v)| (k.clone(), json!(fill(v, &values)))).collect());
    let mut fetched = super::http::fetch(m, inst, op, &args, secret).await.map_err(|f| f.said)?;
    if let Some(p) = mi.pipeline.as_deref().filter(|p| !p.trim().is_empty()) {
        let (columns, rows) = crate::syn::crunch::apply(&fetched.columns, std::mem::take(&mut fetched.rows), p).map_err(|e| e.to_string())?;
        fetched.columns = columns;
        fetched.rows = rows;
    }
    let col = |name: &str| fetched.columns.iter().position(|c| crate::syn::crunch::column_key(&c.name) == crate::syn::crunch::column_key(name));
    let key_at = col(&mi.key).ok_or_else(|| format!("the answer has no column `{}`", mi.key))?;
    let mut ran = Ran { at: now.timestamp(), ok: true, rows: fetched.rows.len(), ..Default::default() };

    if mi.into == "staging:transaction" {
        let had = state(vault_path).get(&state_key(m, mi)).map(|r| r.staged_keys.clone()).unwrap_or_default();
        ran.staged_keys = had.clone();
        let mut lines = Vec::new();
        for row in &fetched.rows {
            let key = row.get(key_at).map(cell_text).unwrap_or_default();
            if key.is_empty() || had.contains(&key) {
                continue;
            }
            let get = |f: &str| mi.fields.get(f).and_then(|c| col(c)).and_then(|i| row.get(i)).map(cell_text).unwrap_or_default();
            lines.push([get("date"), get("type"), get("amount"), get("category"), String::new(), get("note")]);
            ran.staged_keys.insert(key);
        }
        if !lines.is_empty() {
            crate::syn::staging::write(vault_path, &lines, &format!("{} · {}", m.name, mi.name), &format!("mirror {}", state_key(m, mi)), None)?;
            ran.staged = lines.len();
        }
        return Ok(ran);
    }

    let mut seen = BTreeSet::new();
    for row in &fetched.rows {
        let key = row.get(key_at).map(cell_text).unwrap_or_default();
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        let get = |c: &str| col(c).and_then(|i| row.get(i)).cloned();
        let (title, content, props) = note_of(mi, m, &key, &get);
        let path = path_for(m, mi, &key);
        let existing = db.lock().unwrap_or_else(|e| e.into_inner()).get_node(&path).ok().flatten();
        if existing.as_ref().is_some_and(|n| same(n, &title, &content, &props)) {
            continue;
        }
        crate::commands::nodes::write_node_inner(app, db, vault_path.to_string(), path, title, mi.into.clone(), Value::Object(props), Some(content))
            .map_err(|e| e.to_string())?;
        ran.written += 1;
    }

    // Gone from the service, so gone from here — when the answer was whole.
    if fetched.missing.is_empty() {
        let mirror = format!("{}/{}", m.id, mi.name);
        let window = values.iter().find(|(k, _)| k == "window_start_date").zip(values.iter().find(|(k, _)| k == "window_end_date"));
        let db_guard = db.lock().unwrap_or_else(|e| e.into_inner());
        let mine: Vec<String> = db_guard
            .get_nodes_by_type(&mi.into)
            .unwrap_or_default()
            .into_iter()
            .filter(|n| n.properties.get("mirror").and_then(Value::as_str) == Some(mirror.as_str()))
            .filter(|n| n.properties.get("mirror_key").and_then(Value::as_str).is_some_and(|k| !seen.contains(k)))
            .filter(|n| match window {
                None => true,
                Some(((_, from), (_, to))) => n
                    .properties
                    .get("start_at")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.get(..10).is_some_and(|d| d >= from.as_str() && d < to.as_str())),
            })
            .map(|n| n.id)
            .collect();
        for id in mine {
            match crate::commands::trash::apply_trash(&db_guard, vault_path, &id) {
                Ok(_) => ran.trashed += 1,
                Err(e) => log::warn!("[Syn] A mirrored note no longer there could not be trashed: {e}"),
            }
        }
    }
    Ok(ran)
}

/// Every mirror this device runs that is due, run. Called once a minute.
pub async fn tick<R: tauri::Runtime>(app: &tauri::AppHandle<R>, db: &crate::db::DbState, vault_path: &str) {
    {
        use tauri::Manager;
        super::device::know_data_dir(app.path().app_data_dir().ok());
    }
    let found = {
        let guard = db.lock().unwrap_or_else(|e| e.into_inner());
        super::manifest::all(&guard).unwrap_or_default()
    };
    let now = chrono::Utc::now();
    for m in found.into_iter().filter_map(|f| f.manifest.ok()).filter(|m| !m.mirrors.is_empty()) {
        let Some(inst) = super::instance::approved_here(vault_path, &m) else { continue };
        for mi in m.mirrors.iter().filter(|mi| inst.mirrors.contains(&mi.name)) {
            let key = state_key(&m, mi);
            let last = state(vault_path).get(&key).cloned().unwrap_or_default();
            if now.timestamp() - last.at < mi.every_minutes as i64 * 60 {
                continue;
            }
            let secret = match m.auth.secret() {
                None => None,
                Some(name) => match super::declared::read_secret(crate::commands::connector::secret_slot(&m.id, name)).await {
                    Some(s) => Some(s),
                    None => {
                        record(vault_path, &key, Ran { at: now.timestamp(), ok: false, said: "this device does not have its secret".into(), ..last });
                        continue;
                    }
                },
            };
            let ran = match run_one(app, db, vault_path, &m, &inst, mi, secret.as_deref(), now).await {
                Ok(ran) => ran,
                Err(said) => {
                    log::warn!("[Syn] Mirror {key} failed: {said}");
                    Ran { at: now.timestamp(), ok: false, said, staged_keys: last.staged_keys, ..Default::default() }
                }
            };
            record(vault_path, &key, ran);
        }
    }
}

pub fn record(vault_path: &str, key: &str, ran: Ran) {
    let mut all = state(vault_path);
    all.insert(key.to_string(), ran);
    save_state(vault_path, &all);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mirrored_path_names_its_connector() {
        assert_eq!(connector_of_path("Events/mirror-cal/0123456789abcdef.md").as_deref(), Some("cal"));
        assert_eq!(connector_of_path("Notes/mirror-of-my-thoughts.md"), None, "a note named so is not one");
        assert_eq!(connector_of_path("Events/party.md"), None);
    }
}
