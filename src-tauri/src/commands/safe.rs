//! Safe's commands: a thin layer over `crate::safe`.
//!
//! Section 6.3 of `docs/safe-2026-09-28.md` is the rule these follow: **no
//! command returns a list of values.** `safe_reveal` returns one value of one
//! field because the user pressed the eye on that field; `safe_copy` returns
//! none, because the copy happens here. Everything else carries titles,
//! usernames and hosts at most.
//!
//! Every command also checks who is asking. `syn::browser::may_call` already
//! keeps the browsing webview to one command; this keeps Safe to the app's own
//! windows by name, so that a webview added later is not let in by default.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::safe::clipboard::SafeClipboard;
use crate::safe::crypto::SecretKey;
use crate::safe::generator::{self, Recipe};
use crate::safe::item::{ItemEdit, ItemSummary, ItemView};
use crate::safe::keyset;
use crate::safe::session::{Filter, Overview, SafeError, SafeSession, Settings, Unlocked};
use crate::secrets::SecretManager;

/// Emitted when the Safe locks by itself, so an open screen can follow.
pub const LOCKED_EVENT: &str = "safe://locked";

/// Which webviews may use Safe: the main window, the Quick Entry window, and a
/// note opened in a window of its own.
fn may_use_safe(label: &str) -> bool {
    label == "main" || label == "quick-entry" || label.starts_with("node_")
}

fn gate(webview: &tauri::Webview) -> AppResult<()> {
    if may_use_safe(webview.label()) {
        Ok(())
    } else {
        log::warn!("[Safe] refused a call from webview '{}'", webview.label());
        Err(AppError::Safe(SafeError::Failed("Safe is not available here".into())))
    }
}

fn vault(vault_path: &str) -> AppResult<PathBuf> {
    let path = PathBuf::from(vault_path);
    if vault_path.trim().is_empty() || !path.is_dir() {
        return Err(AppError::InvalidPath(vault_path.to_string()));
    }
    Ok(path)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn secret_key_entry(safe_id: &[u8; 16]) -> String {
    format!("safe-secret-key-{}", hex::encode(safe_id))
}

/// The Secret Key as twelve BIP39 words — the form a person reads, writes
/// down and types back in.
fn words_of(secret_key: &SecretKey) -> String {
    bip39::Mnemonic::from_entropy(secret_key.as_bytes()).expect("16 bytes is a 12-word phrase").to_string()
}

fn secret_key_from_words(words: &str) -> Result<SecretKey, SafeError> {
    let normalised = words.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ");
    let mnemonic = bip39::Mnemonic::parse(&normalised).map_err(|_| SafeError::BadSecretKey)?;
    let mut entropy = mnemonic.to_entropy();
    let bytes: Result<[u8; 16], _> = entropy.as_slice().try_into();
    zeroize::Zeroize::zeroize(&mut entropy);
    bytes.map(SecretKey::from_bytes).map_err(|_| SafeError::BadSecretKey)
}

fn stored_secret_key(app: &tauri::AppHandle, safe_id: &[u8; 16]) -> Result<Option<SecretKey>, SafeError> {
    let Some(hex_key) = SecretManager::get_named(Some(app), &secret_key_entry(safe_id)).map_err(SafeError::Keychain)?
    else {
        return Ok(None);
    };
    let hex_key = zeroize::Zeroizing::new(hex_key);
    let bytes = hex::decode(hex_key.trim()).map_err(|_| SafeError::Keychain("the stored Secret Key is malformed".into()))?;
    let bytes = zeroize::Zeroizing::new(bytes);
    let array: [u8; 16] =
        bytes.as_slice().try_into().map_err(|_| SafeError::Keychain("the stored Secret Key is malformed".into()))?;
    Ok(Some(SecretKey::from_bytes(array)))
}

fn store_secret_key(app: &tauri::AppHandle, safe_id: &[u8; 16], secret_key: &SecretKey) -> Result<(), SafeError> {
    let hex_key = zeroize::Zeroizing::new(hex::encode(secret_key.as_bytes()));
    SecretManager::set_named(Some(app), &secret_key_entry(safe_id), &hex_key).map_err(SafeError::Keychain)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, SafeError> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::General(format!("Safe task failed: {e}")))?
        .map_err(AppError::Safe)
}

// ─── lifecycle ───────────────────────────────────────────

#[derive(Serialize)]
pub struct Status {
    exists: bool,
    unlocked: bool,
    /// Whether this device's keychain holds the Secret Key. When it does not,
    /// the unlock screen asks for it.
    has_secret_key: bool,
}

#[tauri::command]
pub async fn safe_status(app: tauri::AppHandle, webview: tauri::Webview, vault_path: String) -> AppResult<Status> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let session = app.state::<SafeSession>();
    let keyset = match keyset::read(&vault) {
        Ok(k) => k,
        Err(keyset::KeysetError::Missing) => return Ok(Status { exists: false, unlocked: false, has_secret_key: false }),
        Err(e) => return Err(AppError::Safe(e.into())),
    };
    let has_secret_key = stored_secret_key(&app, &keyset.header.safe_id).map(|k| k.is_some()).unwrap_or(false);
    Ok(Status { exists: true, unlocked: session.is_open_for(&vault), has_secret_key })
}

#[derive(Serialize)]
pub struct Created {
    /// The Secret Key, as twelve words. The only time it is sent anywhere
    /// without the master password being typed first.
    secret_key: String,
    /// False when the keychain refused it. The Safe still works — the user
    /// types the words in at unlock — but the screen must say so.
    stored_on_device: bool,
}

/// Create the Safe for this vault and open it.
///
/// Argon2id is calibrated to this machine first, which takes a second or two.
#[tauri::command]
pub async fn safe_create(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    password: String,
) -> AppResult<Created> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password);
    let handle = app.clone();
    blocking(move || {
        let kdf = keyset::calibrate();
        log::info!("[Safe] creating a Safe with Argon2id m={} KiB t={} p={}", kdf.m_kib, kdf.t, kdf.p);
        let created = keyset::create(&vault, &password, kdf)?;
        let stored_on_device = match store_secret_key(&handle, &created.keyset.header.safe_id, &created.secret_key) {
            Ok(()) => true,
            Err(e) => {
                log::error!("[Safe] {e}");
                false
            }
        };
        let secret_key = words_of(&created.secret_key);
        let unlocked = Unlocked::open(&vault, created.keyset, created.safe_key)?;
        handle.state::<SafeSession>().install(unlocked);
        Ok(Created { secret_key, stored_on_device })
    })
    .await
}

/// Open the Safe with the master password, and the Secret Key from the
/// keychain — or from `secret_key` when the keychain has none, in which case a
/// successful unlock stores it there.
#[tauri::command]
pub async fn safe_unlock(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    password: String,
    secret_key: Option<String>,
) -> AppResult<()> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password);
    let typed = zeroize::Zeroizing::new(secret_key.unwrap_or_default());
    let handle = app.clone();
    blocking(move || {
        let keyset = keyset::read(&vault)?;
        let safe_id = keyset.header.safe_id;
        let (sk, from_user) = if typed.trim().is_empty() {
            match stored_secret_key(&handle, &safe_id)? {
                Some(sk) => (sk, false),
                None => return Err(SafeError::NeedsSecretKey),
            }
        } else {
            (secret_key_from_words(&typed)?, true)
        };
        let safe_key = keyset::unlock(&keyset, &password, &sk)?;
        if from_user {
            if let Err(e) = store_secret_key(&handle, &safe_id, &sk) {
                log::error!("[Safe] opened, but {e}");
            }
        }
        let unlocked = Unlocked::open(&vault, keyset, safe_key)?;
        handle.state::<SafeSession>().install(unlocked);
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn safe_lock(app: tauri::AppHandle, webview: tauri::Webview) -> AppResult<()> {
    gate(&webview)?;
    app.state::<SafeSession>().lock();
    Ok(())
}

/// Change the master password. The current one is asked for again even though
/// the Safe is open: somebody at an unlocked screen should not be able to lock
/// its owner out.
#[tauri::command]
pub async fn safe_change_password(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    current: String,
    next: String,
) -> AppResult<()> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let current = zeroize::Zeroizing::new(current);
    let next = zeroize::Zeroizing::new(next);
    let handle = app.clone();
    blocking(move || {
        let keyset = keyset::read(&vault)?;
        let sk = stored_secret_key(&handle, &keyset.header.safe_id)?.ok_or(SafeError::NeedsSecretKey)?;
        let key = keyset::unlock(&keyset, &current, &sk)?;
        let kdf = keyset.header.kdf;
        let updated = keyset::change_password(&vault, &keyset, &key, &next, &sk, kdf)?;
        handle.state::<SafeSession>().with(&vault, |open| {
            open.replace_keyset(updated);
            Ok(())
        })
    })
    .await
}

/// The Secret Key again, as words, for a new Emergency Kit. Behind the master
/// password, like everything that shows it.
#[tauri::command]
pub async fn safe_secret_key(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    password: String,
) -> AppResult<String> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password);
    let handle = app.clone();
    blocking(move || {
        let keyset = keyset::read(&vault)?;
        let sk = stored_secret_key(&handle, &keyset.header.safe_id)?.ok_or(SafeError::NeedsSecretKey)?;
        keyset::unlock(&keyset, &password, &sk)?;
        Ok(words_of(&sk))
    })
    .await
}

/// Write the Emergency Kit to `path`: a page to print, with the Secret Key and
/// a blank for the master password to be written in by hand.
///
/// The Secret Key comes from the keychain here rather than from the screen, so
/// the words do not travel back through the WebView to be written out. Only
/// while the Safe is open.
#[tauri::command]
pub fn safe_save_emergency_kit(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    path: String,
) -> AppResult<()> {
    gate(&webview)?;
    let safe_id = with(&app, &vault_path, |s| Ok(s.keyset().header.safe_id))?;
    let sk = stored_secret_key(&app, &safe_id).map_err(AppError::Safe)?.ok_or(AppError::Safe(SafeError::NeedsSecretKey))?;
    let html = zeroize::Zeroizing::new(emergency_kit(&words_of(&sk), &safe_id, &chrono::Local::now().format("%Y-%m-%d").to_string()));
    crate::safe::store::write_atomic(std::path::Path::new(&path), html.as_bytes())
        .map_err(|e| AppError::Safe(SafeError::Failed(format!("could not write the Emergency Kit: {e}"))))
}

/// The kit itself. Plain HTML with its own styles, so it prints the same from
/// any browser and needs nothing from the app to open.
fn emergency_kit(words: &str, safe_id: &[u8; 16], date: &str) -> String {
    let cells: String = words
        .split(' ')
        .enumerate()
        .map(|(i, w)| format!("<li><span>{}</span>{}</li>", i + 1, w))
        .collect();
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Synabit Safe — Emergency Kit</title>
<style>
body{{font:15px/1.5 system-ui,sans-serif;max-width:640px;margin:40px auto;padding:0 24px;color:#111}}
h1{{font-size:22px;margin:0 0 4px}} .muted{{color:#555}}
ol{{list-style:none;padding:0;display:grid;grid-template-columns:repeat(3,1fr);gap:8px}}
li{{border:1px solid #ccc;border-radius:8px;padding:8px 10px;font:600 16px ui-monospace,monospace}}
li span{{color:#888;font-weight:400;margin-right:8px}}
.box{{border:1px dashed #999;border-radius:8px;height:48px;margin:8px 0 24px}}
code{{font:12px ui-monospace,monospace;color:#555}}
</style></head><body>
<h1>Synabit Safe — Emergency Kit</h1>
<p class="muted">Created {date}. Safe <code>{id}</code></p>
<h2>Secret Key</h2>
<ol>{cells}</ol>
<h2>Master password</h2>
<p class="muted">Write it here by hand, or keep it only in your head. Never type it into a file.</p>
<div class="box"></div>
<h2>How to use this kit</h2>
<p>To open this Safe on a new device, or after this device's keychain was reset, you need both the
twelve words above and your master password. Nobody else can recover them — not Synabit, not anyone.</p>
<p>Print this page, or keep it somewhere offline and safe. Do not keep it in the same place as your vault.</p>
</body></html>
"#,
        id = hex::encode(safe_id),
    )
}

// ─── items ───────────────────────────────────────────────

fn with<R>(app: &tauri::AppHandle, vault_path: &str, f: impl FnOnce(&mut Unlocked) -> Result<R, SafeError>) -> AppResult<R> {
    let vault = vault(vault_path)?;
    app.state::<SafeSession>().with(&vault, f).map_err(AppError::Safe)
}

/// Read the Safe again after sync brought items from another device, and
/// fold in any version it set aside.
#[tauri::command]
pub fn safe_refresh(app: tauri::AppHandle, webview: tauri::Webview, vault_path: String) -> AppResult<()> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.reload())
}

#[tauri::command]
pub fn safe_overview(app: tauri::AppHandle, webview: tauri::Webview, vault_path: String) -> AppResult<Overview> {
    gate(&webview)?;
    with(&app, &vault_path, |s| Ok(s.overview()))
}

#[tauri::command]
pub fn safe_list(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    filter: Option<Filter>,
    query: Option<String>,
) -> AppResult<Vec<ItemSummary>> {
    gate(&webview)?;
    with(&app, &vault_path, |s| Ok(s.list(&filter.unwrap_or_default(), query.as_deref().unwrap_or(""))))
}

#[tauri::command]
pub fn safe_get(app: tauri::AppHandle, webview: tauri::Webview, vault_path: String, id: String) -> AppResult<ItemView> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.view(&id))
}

/// The one command that returns a secret value: one field, because the user
/// asked to see it.
#[tauri::command]
pub fn safe_reveal(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    field: String,
) -> AppResult<String> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.reveal(&id, &field).map(|v| v.expose().to_string()))
}

#[derive(Serialize)]
pub struct Copied {
    /// Seconds until the clipboard is cleared, or 0 for never.
    clear_after_secs: u64,
}

/// Copy one field's value to the clipboard without it passing through the
/// WebView, and take it back off later if nothing else was copied meanwhile.
#[tauri::command]
pub fn safe_copy(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    field: String,
) -> AppResult<Copied> {
    gate(&webview)?;
    let (value, clear_after) = with(&app, &vault_path, |s| Ok((s.reveal(&id, &field)?, s.settings().clipboard_clear_secs)))?;
    let generation = app.state::<SafeClipboard>().copy(&value).map_err(AppError::Safe)?;
    drop(value);
    if clear_after > 0 {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(clear_after)).await;
            handle.state::<SafeClipboard>().clear_if_unchanged(generation);
        });
    }
    Ok(Copied { clear_after_secs: clear_after })
}

/// The current one-time code of an item. The secret stays here; a code is
/// worth thirty seconds.
#[tauri::command]
pub fn safe_totp(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
) -> AppResult<crate::safe::totp::Code> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.totp(&id, now() as u64))
}

/// Copy the current one-time code — or the next one, in the last five
/// seconds of this one, so it is not stale by the time it is pasted.
#[tauri::command]
pub fn safe_copy_totp(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
) -> AppResult<Copied> {
    gate(&webview)?;
    let (code, clear_after) = with(&app, &vault_path, |s| {
        let t = now() as u64;
        let current = s.totp(&id, t)?;
        let code = if current.remaining <= 5 { s.totp(&id, t + u64::from(current.remaining))? } else { current };
        Ok((code, s.settings().clipboard_clear_secs))
    })?;
    let value = crate::safe::item::SecretString::new(code.code);
    let generation = app.state::<SafeClipboard>().copy(&value).map_err(AppError::Safe)?;
    if clear_after > 0 {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(clear_after)).await;
            handle.state::<SafeClipboard>().clear_if_unchanged(generation);
        });
    }
    Ok(Copied { clear_after_secs: clear_after })
}

#[tauri::command]
pub fn safe_create_item(app: tauri::AppHandle, webview: tauri::Webview, vault_path: String, item: ItemEdit) -> AppResult<ItemView> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.create(item, now()))
}

#[tauri::command]
pub fn safe_update_item(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    item: ItemEdit,
) -> AppResult<ItemView> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.update(&id, item, now()))
}

#[tauri::command]
pub fn safe_set_favorite(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    favorite: bool,
) -> AppResult<()> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.set_favorite(&id, favorite, now()))
}

#[tauri::command]
pub fn safe_set_trashed(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    trashed: bool,
) -> AppResult<()> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.set_trashed(&id, trashed, now()))
}

#[tauri::command]
pub fn safe_purge(app: tauri::AppHandle, webview: tauri::Webview, vault_path: String, id: String) -> AppResult<()> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.purge(&id))
}

// ─── moving in and out ───────────────────────────────────

#[derive(Serialize)]
pub struct Imported {
    format: crate::safe::exchange::Format,
    imported: usize,
    /// What was skipped and why. Never a value.
    warnings: Vec<String>,
    /// Whether the source file was plaintext — every format but Safe's own —
    /// so the screen can say to delete it.
    source_was_plaintext: bool,
}

/// Bring in another password manager's export, or a Safe export. The file is
/// read here; the screen names it and gets counts back.
#[tauri::command]
pub async fn safe_import(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    path: String,
    password: Option<String>,
) -> AppResult<Imported> {
    use crate::safe::exchange::{self, ExchangeError, Format};
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password.unwrap_or_default());
    let handle = app.clone();
    blocking(move || {
        let file = std::path::Path::new(&path);
        let bytes = zeroize::Zeroizing::new(std::fs::read(file).map_err(|e| SafeError::Failed(format!("could not read the file: {e}")))?);
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let format = exchange::detect(&name, &bytes).ok_or(SafeError::ImportUnknown)?;
        if format == Format::SafeExport && password.is_empty() {
            return Err(SafeError::NeedsExportPassword);
        }
        let parsed = exchange::parse(format, &bytes, Some(&password), now()).map_err(|e| match e {
            ExchangeError::Unknown => SafeError::ImportUnknown,
            ExchangeError::EncryptedBitwarden => SafeError::EncryptedBitwarden,
            ExchangeError::WrongExportPassword => SafeError::WrongExportPassword,
            other => SafeError::Failed(other.to_string()),
        })?;
        let imported = handle.state::<SafeSession>().with(&vault, |s| s.import(parsed.items))?;
        Ok(Imported { format, imported, warnings: parsed.warnings, source_was_plaintext: format != Format::SafeExport })
    })
    .await
}

/// Seal every item into a `.safe-export` under a password chosen for it.
#[tauri::command]
pub async fn safe_export(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    path: String,
    export_password: String,
) -> AppResult<usize> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let export_password = zeroize::Zeroizing::new(export_password);
    if export_password.chars().count() < keyset::MIN_PASSWORD_CHARS {
        return Err(AppError::Safe(SafeError::PasswordTooShort));
    }
    let handle = app.clone();
    blocking(move || {
        let items = handle.state::<SafeSession>().with(&vault, |s| s.all_items())?;
        let bytes = crate::safe::exchange::seal_export(&items, &export_password, crate::safe::crypto::KdfParams::FLOOR)
            .map_err(|e| SafeError::Failed(e.to_string()))?;
        crate::safe::store::write_atomic(std::path::Path::new(&path), &bytes).map_err(|e| SafeError::Failed(e.to_string()))?;
        Ok(items.len())
    })
    .await
}

/// Every item as a plaintext CSV, for leaving Synabit. Behind the master
/// password even though the Safe is open: this is the one command that puts
/// every secret in a file anyone can read.
#[tauri::command]
pub async fn safe_export_plain(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    path: String,
    password: String,
) -> AppResult<usize> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password);
    let handle = app.clone();
    blocking(move || {
        let keyset = keyset::read(&vault)?;
        let sk = stored_secret_key(&handle, &keyset.header.safe_id)?.ok_or(SafeError::NeedsSecretKey)?;
        keyset::unlock(&keyset, &password, &sk)?;
        let items = handle.state::<SafeSession>().with(&vault, |s| s.all_items())?;
        let csv = crate::safe::exchange::to_csv(&items);
        crate::safe::store::write_atomic(std::path::Path::new(&path), csv.as_bytes()).map_err(|e| SafeError::Failed(e.to_string()))?;
        log::warn!("[Safe] {} items were exported as plaintext", items.len());
        Ok(items.iter().filter(|i| i.trashed_at.is_none()).count())
    })
    .await
}

// ─── tools ───────────────────────────────────────────────

#[derive(Serialize)]
pub struct Generated {
    value: String,
    bits: f64,
}

/// A new password. It goes to the editor, which is the one screen where a
/// value is in the WebView by necessity: the user is typing or looking at it.
#[tauri::command]
pub fn safe_generate(webview: tauri::Webview, recipe: Option<Recipe>) -> AppResult<Generated> {
    gate(&webview)?;
    let recipe = recipe.unwrap_or_default();
    let value = generator::generate(&recipe).map_err(|e| AppError::Safe(e.into()))?;
    Ok(Generated { value, bits: generator::recipe_bits(&recipe) })
}

/// A pessimistic guess at a typed password's strength, in bits.
#[tauri::command]
pub fn safe_estimate(webview: tauri::Webview, password: String) -> AppResult<f64> {
    gate(&webview)?;
    let password = zeroize::Zeroizing::new(password);
    Ok(generator::estimate_bits(&password))
}

#[tauri::command]
pub fn safe_get_settings(app: tauri::AppHandle, webview: tauri::Webview, vault_path: String) -> AppResult<Settings> {
    gate(&webview)?;
    with(&app, &vault_path, |s| Ok(s.settings()))
}

#[tauri::command]
pub fn safe_set_settings(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    settings: Settings,
) -> AppResult<Settings> {
    gate(&webview)?;
    with(&app, &vault_path, |s| s.set_settings(settings))
}

/// Lock whatever Safe has been left alone too long — or sat open while the
/// machine slept — every few seconds, and tell the screen. Started once from
/// `setup`.
pub fn start_auto_lock(app: tauri::AppHandle) {
    const TICK: Duration = Duration::from_secs(10);
    tauri::async_runtime::spawn(async move {
        let mut last = std::time::SystemTime::now();
        loop {
            tokio::time::sleep(TICK).await;
            let now = std::time::SystemTime::now();
            let session = app.state::<SafeSession>();
            let locked = if crate::safe::session::slept(last, now, TICK) {
                session.lock().then(|| log::info!("[Safe] locked: the machine was asleep"))
            } else {
                session.lock_if_idle(std::time::Instant::now()).then(|| log::info!("[Safe] locked after being left alone"))
            };
            if locked.is_some() {
                let _ = app.emit(LOCKED_EVENT, ());
            }
            last = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_emergency_kit_holds_every_word_in_order() {
        let sk = SecretKey::from_bytes([0x5a; 16]);
        let words = words_of(&sk);
        let kit = emergency_kit(&words, &[1; 16], "2026-09-28");
        let mut at = 0;
        for (i, w) in words.split(' ').enumerate() {
            let cell = format!("<li><span>{}</span>{}</li>", i + 1, w);
            let found = kit[at..].find(&cell).unwrap_or_else(|| panic!("word {} missing or out of order", i + 1));
            at += found + cell.len();
        }
        assert!(kit.contains("01010101010101010101010101010101"));
    }

    #[test]
    fn only_the_apps_own_windows_may_use_safe() {
        assert!(may_use_safe("main"));
        assert!(may_use_safe("quick-entry"));
        assert!(may_use_safe("node_1727500000"));
        assert!(!may_use_safe(crate::syn::browser::WINDOW));
        assert!(!may_use_safe("some-future-webview"));
    }

    #[test]
    fn the_secret_key_round_trips_through_words_and_rejects_typos() {
        let sk = SecretKey::from_bytes([0x5a; 16]);
        let words = words_of(&sk);
        assert_eq!(words.split(' ').count(), 12);
        let back = secret_key_from_words(&format!("  {}  ", words.to_uppercase())).unwrap();
        assert_eq!(back.as_bytes(), sk.as_bytes());

        let mut typo: Vec<&str> = words.split(' ').collect();
        typo.swap(0, 1);
        assert!(secret_key_from_words(&typo.join(" ")).is_err(), "the checksum catches a swapped pair");
        assert!(secret_key_from_words("not twelve words").is_err());
    }
}
