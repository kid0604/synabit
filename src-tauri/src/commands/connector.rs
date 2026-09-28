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
