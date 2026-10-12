//! The settings screen's handle on Syn's connectors. See `syn::connector`.
//!
//! Every command answers with the whole list afterwards, so the screen draws
//! what is true — connected, failed, needs the desktop app — rather than what
//! it expected.

use std::collections::HashMap;

use crate::error::AppError;
use crate::syn::connector::{self, config, ServerView, Tested};

/// Every server this vault names, where each stands, and which of its secrets
/// this device holds. Starts the vault's servers if nothing has yet.
#[tauri::command]
pub async fn syn_connector_list(app: tauri::AppHandle, vault_path: String) -> Result<Vec<ServerView>, AppError> {
    connector::start_if_needed(&vault_path, Some(app.clone()));
    let secrets = connector::read_secrets(Some(&app)).await;
    Ok(connector::views(&vault_path, &secrets))
}

/// Connect again, now, and wait for it.
#[tauri::command]
pub async fn syn_connector_reconnect(app: tauri::AppHandle, vault_path: String) -> Result<Vec<ServerView>, AppError> {
    let secrets = connector::read_secrets(Some(&app)).await;
    connector::refresh(&vault_path, &secrets).await;
    Ok(connector::views(&vault_path, &secrets))
}

/// Add or change a server, keep its secrets in the keychain, agree to it on
/// this computer, and connect.
///
/// `secrets` maps a header or variable name to a value typed just now. A name
/// with nothing typed keeps what the keychain already holds; a name no longer
/// in the server's list has its secret removed.
///
/// Saving here is the person choosing this server on this computer, which is
/// what `config::trust_here` records — see `config` for why that is separate
/// from the file. A server renamed or pointed somewhere else loses the
/// permissions granted to what it was.
#[tauri::command]
pub async fn syn_connector_save(
    app: tauri::AppHandle,
    vault_path: String,
    mut server: config::Server,
    secrets: HashMap<String, String>,
) -> Result<Vec<ServerView>, AppError> {
    server.name = server.name.trim().to_string();
    if server.id.trim().is_empty() {
        server.id = uuid::Uuid::new_v4().to_string();
    }
    let mut current = config::load(&vault_path);
    if let Some(problem) = config::problem(&server, &current.servers) {
        return Err(AppError::General(problem));
    }

    let previous = current.servers.iter().find(|s| s.id == server.id).cloned();
    if let Some(previous) = &previous {
        if previous.name != server.name || previous.fingerprint() != server.fingerprint() {
            config::revoke_grants(&vault_path, &previous.name);
        }
        for (kind, key) in previous.secret_keys() {
            if !server.secret_keys().iter().any(|(k, n)| *k == kind && *n == key) {
                forget_secret(&app, &server.id, kind, &key)?;
            }
        }
    }
    for (kind, key) in server.secret_keys() {
        if let Some(value) = secrets.get(&key).map(|v| v.trim()).filter(|v| !v.is_empty()) {
            crate::secrets::SecretManager::set_syn_api_key(Some(&app), &config::slot(&server.id, kind, &key), value)
                .map_err(AppError::General)?;
            // Written anew: the slot it had before connectors had their name
            // goes, so the two cannot disagree.
            crate::secrets::SecretManager::set_syn_api_key(Some(&app), &config::legacy_slot(&server.id, kind, &key), "")
                .map_err(AppError::General)?;
        }
    }

    match current.servers.iter_mut().find(|s| s.id == server.id) {
        Some(slot) => *slot = server.clone(),
        None => current.servers.push(server.clone()),
    }
    config::save(&vault_path, &current)?;
    config::trust_here(&vault_path, &server)?;

    syn_connector_reconnect(app, vault_path).await
}

/// Remove a server: its line in the file, its secrets on this device, this
/// computer's agreement to it, and every permission granted for it.
#[tauri::command]
pub async fn syn_connector_delete(
    app: tauri::AppHandle,
    vault_path: String,
    server_id: String,
) -> Result<Vec<ServerView>, AppError> {
    let mut current = config::load(&vault_path);
    if let Some(server) = current.servers.iter().find(|s| s.id == server_id).cloned() {
        for (kind, key) in server.secret_keys() {
            forget_secret(&app, &server.id, kind, &key)?;
        }
        config::revoke_grants(&vault_path, &server.name);
        current.servers.retain(|s| s.id != server_id);
        config::save(&vault_path, &current)?;
    }
    config::forget_here(&vault_path, &server_id)?;
    connector::disconnect(&vault_path, Some(&server_id)).await;

    syn_connector_reconnect(app, vault_path).await
}

/// Connect to a server as entered, list what it offers, and let it go.
///
/// Nothing is saved. The secrets are what was typed, over whatever this
/// device already holds for a server being edited.
#[tauri::command]
pub async fn syn_connector_test(
    app: tauri::AppHandle,
    vault_path: String,
    server: config::Server,
    secrets: HashMap<String, String>,
) -> Result<Tested, AppError> {
    let others: Vec<config::Server> = config::load(&vault_path).servers;
    if let Some(problem) = config::problem(&server, &others) {
        return Err(AppError::General(problem));
    }
    let mut known = connector::read_secrets(Some(&app)).await;
    for (kind, key) in server.secret_keys() {
        if let Some(value) = secrets.get(&key).map(|v| v.trim()).filter(|v| !v.is_empty()) {
            known.insert(config::slot(&server.id, kind, &key), value.to_string());
        }
    }
    Ok(connector::test(&server, &known).await)
}

/// Clear one secret, in its slot and in the one it had before connectors had
/// their name.
fn forget_secret(app: &tauri::AppHandle, server_id: &str, kind: &str, key: &str) -> Result<(), AppError> {
    for slot in [config::slot(server_id, kind, key), config::legacy_slot(server_id, kind, key)] {
        crate::secrets::SecretManager::set_syn_api_key(Some(app), &slot, "").map_err(AppError::General)?;
    }
    Ok(())
}


// ═══════════════════════════════════════════════════════════════
//  DECLARED CONNECTORS — see `syn::connector::manifest` and `instance`
// ═══════════════════════════════════════════════════════════════

use crate::syn::connector::instance::{self, Choices, Standing};
use crate::syn::connector::manifest::{self, Manifest, Problem};

/// One declaration as the settings screen shows it.
#[derive(serde::Serialize)]
pub struct DeclaredView {
    pub node_id: String,
    pub title: String,
    pub manifest: Option<Manifest>,
    pub problems: Vec<Problem>,
    pub standing: Standing,
    pub instance: Option<instance::Instance>,
    /// What each operation is treated as on this device, by name.
    pub effects: std::collections::BTreeMap<String, manifest::Effect>,
    /// Whether this device holds the declaration's secret.
    pub secret_here: bool,
    /// How each of its mirrors last went on this device, by mirror name.
    #[serde(default)]
    pub mirrors_ran: std::collections::BTreeMap<String, crate::syn::connector::mirror::Ran>,
}

fn declared_views(app: &tauri::AppHandle, vault_path: &str) -> Result<Vec<DeclaredView>, AppError> {
    use tauri::Manager;
    let state = app.state::<crate::db::DbState>();
    let found = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        manifest::all(&db)?
    };
    let instances = instance::load(vault_path);
    let ran = crate::syn::connector::mirror::state(vault_path);
    Ok(found
        .into_iter()
        .map(|f| match f.manifest {
            Ok(m) => {
                let problems = manifest::problems(&m);
                let standing = instance::standing(vault_path, &m);
                let inst = instances.get(&m.id).cloned();
                let effects = m
                    .operations
                    .iter()
                    .map(|op| (op.name.clone(), inst.as_ref().map(|i| instance::effect_here(i, op)).unwrap_or(op.effect)))
                    .collect();
                let secret_here = m.auth.secret().is_none_or(|name| {
                    crate::secrets::SecretManager::get_syn_api_key(Some(app), &secret_slot(&m.id, name))
                        .is_some_and(|v| !v.is_empty())
                });
                let mirrors_ran = m
                    .mirrors
                    .iter()
                    .filter_map(|mi| {
                        ran.get(&crate::syn::connector::mirror::state_key(&m, mi))
                            .map(|r| (mi.name.clone(), crate::syn::connector::mirror::Ran { staged_keys: Default::default(), ..r.clone() }))
                    })
                    .collect();
                DeclaredView { node_id: f.node_id, title: f.title, manifest: Some(m), problems, standing, instance: inst, effects, secret_here, mirrors_ran }
            }
            Err(p) => DeclaredView {
                node_id: f.node_id,
                title: f.title,
                manifest: None,
                problems: vec![p],
                standing: Standing::Broken,
                instance: None,
                effects: Default::default(),
                secret_here: false,
                mirrors_ran: Default::default(),
            },
        })
        .collect())
}

/// Where a declared connector's secret is kept on this device.
pub fn secret_slot(manifest_id: &str, name: &str) -> String {
    format!("connector:{manifest_id}:secret:{name}")
}

/// Tell the device bridges what this device's approvals let them keep.
fn sync_device(app: &tauri::AppHandle, vault_path: &str) -> Result<(), AppError> {
    use tauri::Manager;
    crate::syn::connector::device::know_data_dir(app.path().app_data_dir().ok());
    let state = app.state::<crate::db::DbState>();
    let all: Vec<Manifest> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        manifest::all(&db)?.into_iter().filter_map(|f| f.manifest.ok()).collect()
    };
    crate::syn::connector::device::sync_packages(&crate::syn::connector::device::packages_approved(vault_path, &all));
    Ok(())
}

/// Whether this device lets Synabit read notifications (Android), and the
/// screen that does. The approval card says what is kept before either.
#[tauri::command]
pub async fn syn_device_notification_access() -> Result<bool, AppError> {
    crate::syn::connector::device::android_call("notificationAccess").map_err(AppError::General)
}

#[tauri::command]
pub async fn syn_device_open_notification_access() -> Result<bool, AppError> {
    crate::syn::connector::device::android_call("openNotificationAccess").map_err(AppError::General)
}

/// Every declared connector, and where each stands on this device.
#[tauri::command]
pub async fn syn_manifest_list(app: tauri::AppHandle, vault_path: String) -> Result<Vec<DeclaredView>, AppError> {
    declared_views(&app, &vault_path)
}

/// What is wrong with a declaration, without saving it.
#[tauri::command]
pub async fn syn_manifest_validate(yaml: String) -> Result<Vec<Problem>, AppError> {
    Ok(match manifest::parse_yaml(&yaml) {
        Ok(m) => manifest::problems(&m),
        Err(p) => vec![p],
    })
}

/// Save a declaration as a note in `SynConnectors/` — new, or over the one with
/// the same `id`. Saving is not approving: a changed declaration stops being
/// approved on every device, this one included, until approved again.
#[tauri::command]
pub async fn syn_manifest_save(
    app: tauri::AppHandle,
    vault_path: String,
    yaml: String,
    about: Option<String>,
) -> Result<Vec<DeclaredView>, AppError> {
    use tauri::Manager;
    let m = manifest::parse_yaml(&yaml).map_err(|p| AppError::General(format!("{}: {}", p.at, p.said)))?;
    let body = format!(
        "{}\n\n{}\n{}\n```\n",
        about.unwrap_or_else(|| format!("Connector \u{201c}{}\u{201d}.", m.name)).trim(),
        manifest::FENCE,
        yaml.trim()
    );
    let state = app.state::<crate::db::DbState>();
    let existing = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        manifest::all(&db)?.into_iter().find(|f| f.manifest.as_ref().is_ok_and(|x| x.id == m.id))
    };
    let ctx = crate::syn::tools::ToolContext { db: &state, vault_path: &vault_path, app: &app, run_id: None, model: None };
    match existing {
        Some(f) => {
            crate::syn::tools::execute_tool(&ctx, "update_node", &serde_json::json!({ "node_id": f.node_id, "content": body }))?;
        }
        None => {
            crate::syn::tools::execute_tool(
                &ctx,
                "create_node",
                &serde_json::json!({ "node_type": manifest::CONNECTOR_TYPE, "title": m.name, "content": body }),
            )?;
        }
    }
    declared_views(&app, &vault_path)
}

/// What pressing "Connect" came to, for the card to say in the person's words.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct Connected {
    pub ok: bool,
    /// `missing`, `format`, `credential_rejected`, `unreachable`, `self_signed`,
    /// `not_found`, `rate_limited`, `server_error`, `shape_changed`,
    /// `permission_needed`, `folder_needed`, `untested` (ok, nothing to try).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The input the card should point at, for `missing` and `format`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// How many rows the try read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<usize>,
    /// For `self_signed`: the certificate the server showed, to trust with one tap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    /// The server's or the system's own words, for the card's details.
    pub said: String,
}

fn not_connected(code: &str, field: Option<&str>, said: impl Into<String>) -> Connected {
    Connected { ok: false, code: Some(code.into()), field: field.map(str::to_string), said: said.into(), ..Default::default() }
}

/// The connect card's one button: keep the secret, approve on this device —
/// with the network worked out from the address, not asked — and try it at
/// once, so the card can say whether it works. See
/// `docs/syn-connectors-simple-2026-10-12.md`.
///
/// Approving is still the person's press, on this device; nothing a model
/// says reaches here. A try that fails leaves the approval in place and the
/// card open, pointing at what to fix.
#[tauri::command]
pub async fn syn_connect(
    app: tauri::AppHandle,
    vault_path: String,
    manifest_id: String,
    inputs: std::collections::BTreeMap<String, String>,
    secrets: std::collections::BTreeMap<String, String>,
    trust_cert: Option<String>,
    folder: Option<String>,
) -> Result<Connected, AppError> {
    use manifest::InputKind;
    let views = declared_views(&app, &vault_path)?;
    let view = views.iter().find(|v| v.manifest.as_ref().is_some_and(|m| m.id == manifest_id));
    let Some(m) = view.and_then(|v| v.manifest.clone()) else {
        return Ok(not_connected("missing", None, format!("No readable declaration with id {manifest_id}")));
    };

    // The secret, checked for its shape, then kept.
    let mut secret: Option<String> = None;
    if let Some(name) = m.auth.secret() {
        let typed = secrets.get(name).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let slot = secret_slot(&m.id, name);
        match typed {
            Some(value) => {
                if let Some(f) = m.input(name).and_then(|i| i.format.as_deref()) {
                    if !regex::Regex::new(f).is_ok_and(|re| re.is_match(&value)) {
                        return Ok(not_connected("format", Some(name), ""));
                    }
                }
                crate::secrets::SecretManager::set_syn_api_key(Some(&app), &slot, &value).map_err(AppError::General)?;
                secret = Some(value);
            }
            None => match crate::secrets::SecretManager::get_syn_api_key(Some(&app), &slot).filter(|v| !v.is_empty()) {
                Some(kept) => secret = Some(kept),
                None => return Ok(not_connected("missing", Some(name), "")),
            },
        }
    }

    // Approved with what is decided rather than asked.
    let before = view.and_then(|v| v.instance.clone());
    let filled = m.with_inputs(&inputs.iter().filter(|(k, _)| m.input(k).is_some_and(|i| i.kind != InputKind::Secret)).map(|(k, v)| (k.clone(), v.trim().to_string())).collect());
    let choices = Choices {
        label: before.as_ref().map(|i| i.label.clone()).unwrap_or_default(),
        zone: before.as_ref().map(|i| i.zone.clone()).unwrap_or_default(),
        exposure: before.as_ref().map(|i| i.exposure).unwrap_or_default(),
        network: instance::network_for(&filled.base_url),
        pinned_cert: trust_cert.or_else(|| before.as_ref().and_then(|i| i.pinned_cert.clone())),
        attested: before.as_ref().map(|i| i.attested.clone()).unwrap_or_default(),
        mirrors: before.as_ref().map(|i| i.mirrors.clone()).unwrap_or_default(),
        folder: folder.or_else(|| before.as_ref().and_then(|i| i.folder.clone())),
        inputs,
    };
    let inst = match instance::approve(&vault_path, &m, choices, &chrono::Utc::now().to_rfc3339()) {
        Ok(i) => i,
        Err(e) => {
            let said = e.to_string();
            let after = |tag: &str| said.find(tag).map(|at| said[at + tag.len()..].trim().to_string());
            let (code, field) = if let Some(f) = after("missing:") {
                ("missing", Some(f))
            } else if let Some(f) = after("format:") {
                ("format", Some(f))
            } else if said.contains("Choose the folder") {
                ("folder_needed", None)
            } else {
                ("server_error", None)
            };
            return Ok(Connected { ok: false, code: Some(code.into()), field, said, ..Default::default() });
        }
    };
    sync_device(&app, &vault_path)?;
    Ok(probe(&m, &inst, secret.as_deref()).await)
}

/// Try a connector once, the way it will be used, and say how it went.
pub async fn probe(m: &Manifest, inst: &instance::Instance, secret: Option<&str>) -> Connected {
    use crate::syn::connector::device;
    if m.transport == manifest::Transport::Device {
        let bridges = device::bridges_of(m);
        if bridges.contains(&"android.notifications") && !device::android_call("notificationAccess").unwrap_or(false) {
            return not_connected("permission_needed", None, "");
        }
    }
    // Something to read with nothing to fill: a `discover` operation first.
    let ready = |o: &&manifest::Operation| o.effect == manifest::Effect::Read && o.params.values().all(|p| !p.required);
    let Some(op) = m.operations.iter().filter(ready).find(|o| o.role.as_deref() == Some("discover")).or_else(|| m.operations.iter().find(ready)) else {
        return Connected { ok: true, code: Some("untested".into()), ..Default::default() };
    };
    match crate::syn::connector::http::fetch(m, inst, op, &serde_json::json!({}), secret).await {
        Ok(f) => Connected { ok: true, rows: Some(f.rows.len()), ..Default::default() },
        Err(failed) => {
            let code = failed.code.unwrap_or("server_error");
            let cert = if code == "self_signed" {
                crate::syn::connector::http::peek_certificate(&m.with_inputs(&inst.inputs).base_url).await.ok()
            } else {
                None
            };
            Connected { ok: false, code: Some(code.into()), cert, said: failed.said, ..Default::default() }
        }
    }
}

/// Approve a declaration on this device, with the person's choices and the
/// secret they typed. The one way a declared connector becomes usable.
#[tauri::command]
pub async fn syn_instance_approve(
    app: tauri::AppHandle,
    vault_path: String,
    manifest_id: String,
    choices: Choices,
    secret: Option<String>,
) -> Result<Vec<DeclaredView>, AppError> {
    let views = declared_views(&app, &vault_path)?;
    let m = views
        .iter()
        .find_map(|v| v.manifest.as_ref().filter(|m| m.id == manifest_id))
        .cloned()
        .ok_or_else(|| AppError::General(format!("No readable declaration with id {manifest_id}")))?;
    if let (Some(name), Some(value)) = (m.auth.secret(), secret.as_deref().map(str::trim).filter(|v| !v.is_empty())) {
        crate::secrets::SecretManager::set_syn_api_key(Some(&app), &secret_slot(&m.id, name), value).map_err(AppError::General)?;
    }
    instance::approve(&vault_path, &m, choices, &chrono::Utc::now().to_rfc3339())?;
    sync_device(&app, &vault_path)?;
    declared_views(&app, &vault_path)
}

/// Stop using a declaration on this device: its approval, its secret, every
/// permission granted for it. The note in the vault stays.
#[tauri::command]
pub async fn syn_instance_remove(app: tauri::AppHandle, vault_path: String, manifest_id: String) -> Result<Vec<DeclaredView>, AppError> {
    let views = declared_views(&app, &vault_path)?;
    if let Some(m) = views.iter().find_map(|v| v.manifest.as_ref().filter(|m| m.id == manifest_id)) {
        if let Some(name) = m.auth.secret() {
            crate::secrets::SecretManager::set_syn_api_key(Some(&app), &secret_slot(&m.id, name), "").map_err(AppError::General)?;
        }
        instance::remove(&vault_path, &m.id, &m.name)?;
    }
    sync_device(&app, &vault_path)?;
    declared_views(&app, &vault_path)
}

/// The certificate a host presents, for the approval card to show and pin —
/// read without trusting it, and with nothing sent after the handshake.
#[tauri::command]
pub async fn syn_instance_peek_cert(url: String) -> Result<String, AppError> {
    crate::syn::connector::http::peek_certificate(&url).await.map_err(AppError::General)
}

/// One declaration the app comes with, as the settings screen offers it.
#[derive(serde::Serialize)]
pub struct BundledView {
    pub id: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub yaml: &'static str,
}

/// The declarations the app comes with. Adding one is `syn_manifest_save`.
#[tauri::command]
pub async fn syn_manifest_bundled() -> Result<Vec<BundledView>, AppError> {
    Ok(crate::syn::connector::bundled::all()
        .into_iter()
        .map(|b| BundledView { id: b.id, name: b.name, about: b.about, yaml: b.yaml })
        .collect())
}
