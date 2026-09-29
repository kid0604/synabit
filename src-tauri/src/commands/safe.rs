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
use crate::safe::session::{Filter, Overview, SafeError, Settings, Unlocked};
use crate::secrets::SecretManager;

/// Emitted whenever the Safe locks — by itself, by a click, by the app
/// quitting — so every open screen and card can follow.
pub const LOCKED_EVENT: &str = "safe://locked";

/// Lock the Safe, if one is open, and do everything a lock means.
pub fn lock_now(app: &tauri::AppHandle) -> bool {
    let session = crate::safe::session::global();
    let vault = session.open_vault();
    let locked = session.lock();
    if locked {
        after_lock(app, vault);
    }
    locked
}

/// What follows a lock, whichever way it came: no approval card still waiting
/// can be answered yes, a secret copied from the Safe comes off the clipboard,
/// every window hears, and connectors drop what they were given.
fn after_lock(app: &tauri::AppHandle, vault: Option<PathBuf>) {
    crate::safe::approvals::abandon_all();
    if let Some(clipboard) = app.try_state::<SafeClipboard>() {
        clipboard.clear_now();
    }
    let _ = app.emit(LOCKED_EVENT, ());
    if let Some(v) = vault {
        crate::syn::connector::after_safe_locked(app.clone(), v.to_string_lossy().into_owned());
    }
}

/// Hold a newly opened Safe; a different vault's Safe open until now is locked
/// the whole way, not just forgotten.
fn install(app: &tauri::AppHandle, unlocked: Unlocked) {
    if let Some(replaced) = crate::safe::session::global().install(unlocked) {
        after_lock(app, Some(replaced));
    }
}

/// Which webviews may use Safe: the main window, Safe's own Quick Access
/// window, and a note opened in a window of its own. Not the capture box —
/// it has no reason to.
fn may_use_safe(label: &str) -> bool {
    label == "main" || label == "safe-quick" || label.starts_with("node_")
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

/// The Secret Key the user typed, or else the one this device keeps. A
/// device whose keychain refused it can still do everything the words allow.
fn secret_key_for(app: &tauri::AppHandle, safe_id: &[u8; 16], typed: &str) -> Result<SecretKey, SafeError> {
    if typed.trim().is_empty() {
        stored_secret_key(app, safe_id)?.ok_or(SafeError::NeedsSecretKey)
    } else {
        secret_key_from_words(typed)
    }
}

fn store_secret_key(app: &tauri::AppHandle, safe_id: &[u8; 16], secret_key: &SecretKey) -> Result<(), SafeError> {
    let hex_key = zeroize::Zeroizing::new(hex::encode(secret_key.as_bytes()));
    SecretManager::set_named(Some(app), &secret_key_entry(safe_id), &hex_key).map_err(SafeError::Keychain)
}

/// A master password is judged here too, not only by the screen's meter: the
/// same bar — zxcvbn's 3 of 4 — whatever sent it.
fn strong_enough(password: &str) -> Result<(), SafeError> {
    if password.chars().count() < keyset::MIN_PASSWORD_CHARS {
        return Err(SafeError::PasswordTooShort);
    }
    if crate::safe::health::strength(password, &[]).score < 3 {
        return Err(SafeError::PasswordTooWeak);
    }
    Ok(())
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
    let session = crate::safe::session::global();
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
        strong_enough(&password)?;
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
        let words_for_guard = zeroize::Zeroizing::new(secret_key.clone());
        // The words are all there is of the Secret Key if the keychain said no:
        // they go back to the screen whatever happens next.
        match Unlocked::open(&vault, created.keyset, created.safe_key) {
            Ok(mut unlocked) => {
                unlocked.words_just_shown();
                unlocked.guard_secret_key(&words_for_guard);
                install(&handle, unlocked)
            }
            Err(e) => log::error!("[Safe] created, but could not open it: {e}"),
        }
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
    previous: Option<bool>,
) -> AppResult<()> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password);
    let typed = zeroize::Zeroizing::new(secret_key.unwrap_or_default());
    let previous = previous.unwrap_or(false);
    let handle = app.clone();
    blocking(move || {
        let current = keyset::read(&vault)?;
        let safe_id = current.header.safe_id;
        let (sk, from_user) = if typed.trim().is_empty() {
            match stored_secret_key(&handle, &safe_id)? {
                Some(sk) => (sk, false),
                None => return Err(SafeError::NeedsSecretKey),
            }
        } else {
            (secret_key_from_words(&typed)?, true)
        };
        let asides = crate::safe::sync::keyset_asides(&vault);
        let (keyset, safe_key) = if previous {
            // The user says they never changed the password: open a keyset
            // set aside with it, and make that one current again everywhere.
            let found = asides.iter().find_map(|path| {
                let older = std::fs::read(path).ok().and_then(|b| crate::safe::format::Keyset::decode(&b).ok())?;
                if older.header.safe_id != safe_id {
                    return None;
                }
                keyset::unlock(&older, &password, &sk).ok().map(|key| (older, key))
            });
            let (older, key) = found.ok_or(SafeError::WrongPassword)?;
            let above = current.header.keyset_revision.max(crate::safe::sync::Seen::load(&vault).keyset);
            let restored = keyset::restore(&vault, &older, &key, &password, &sk, above)?;
            log::warn!("[Safe] the keyset from before another device's change was put back at the user's word");
            (restored, key)
        } else {
            match keyset::unlock(&current, &password, &sk) {
                Ok(key) => (current, key),
                // Another device's keyset is here now. Saying so beats "wrong
                // password" to someone typing the one they have always used.
                Err(keyset::KeysetError::Open(crate::safe::format::OpenError::WrongPassword)) if !asides.is_empty() => {
                    return Err(SafeError::PasswordChangedElsewhere)
                }
                Err(e) => return Err(e.into()),
            }
        };
        if from_user {
            if let Err(e) = store_secret_key(&handle, &safe_id, &sk) {
                log::error!("[Safe] opened, but {e}");
            }
        }
        let mut unlocked = Unlocked::open(&vault, keyset, safe_key)?;
        unlocked.guard_secret_key(&zeroize::Zeroizing::new(words_of(&sk)));
        // A keyset set aside that opens with the same password and Secret Key
        // but holds another Safe Key: two devices rotated at once. Its key
        // goes on the ring, so what that device sealed opens here.
        for path in &asides {
            let older = std::fs::read(path).ok().and_then(|b| crate::safe::format::Keyset::decode(&b).ok());
            if let Some(key) = older.filter(|k| k.header.safe_id == safe_id).and_then(|k| keyset::unlock(&k, &password, &sk).ok()) {
                if let Err(e) = unlocked.remember_older_key(key) {
                    log::warn!("[Safe] could not keep another device's Safe Key: {e}");
                }
            }
        }
        // The keyset that opened is the one in force; those set aside have
        // done their job. Said once, on the screen.
        if !asides.is_empty() {
            if !previous {
                unlocked.note_keyset_changed_elsewhere();
            }
            for path in &asides {
                let _ = std::fs::remove_file(path);
            }
        }
        let settings = unlocked.settings();
        install(&handle, unlocked);
        crate::syn::connector::after_safe_unlocked(handle.clone(), vault.to_string_lossy().into_owned());
        sockets_follow(&handle, &settings);
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn safe_lock(app: tauri::AppHandle, webview: tauri::Webview) -> AppResult<()> {
    gate(&webview)?;
    lock_now(&app);
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
    secret_key: Option<String>,
) -> AppResult<()> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let current = zeroize::Zeroizing::new(current);
    let next = zeroize::Zeroizing::new(next);
    let typed = zeroize::Zeroizing::new(secret_key.unwrap_or_default());
    let handle = app.clone();
    blocking(move || {
        // From the open Safe only — and checked before anything is written, so
        // a refusal never follows a change that already happened.
        crate::safe::session::global().peek(&vault, |_| Ok(()))?;
        strong_enough(&next)?;
        let keyset = keyset::read(&vault)?;
        let sk = secret_key_for(&handle, &keyset.header.safe_id, &typed)?;
        let key = keyset::unlock(&keyset, &current, &sk)?;
        let kdf = keyset.header.kdf;
        let updated = keyset::change_password(&vault, &keyset, &key, &next, &sk, kdf)?;
        // The new keyset is on disk: the new password is the one now, whether
        // or not the Safe locked during the two Argon2 runs. If it did, the
        // next unlock reads the new keyset; nothing is owed the open session.
        let _ = crate::safe::session::global().peek_mut(&vault, |open| {
            open.replace_keyset(updated);
            Ok(())
        });
        Ok(())
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

/// Replace the Safe Key and seal every item again under the new one. Behind
/// the master password: it is the answer to "an old copy of my keyset and an
/// old password may be out there".
#[tauri::command]
pub async fn safe_rotate_key(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    password: String,
    secret_key: Option<String>,
) -> AppResult<usize> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password);
    let typed = zeroize::Zeroizing::new(secret_key.unwrap_or_default());
    let handle = app.clone();
    blocking(move || {
        let keyset = keyset::read(&vault)?;
        let sk = secret_key_for(&handle, &keyset.header.safe_id, &typed)?;
        keyset::unlock(&keyset, &password, &sk)?;
        let words = zeroize::Zeroizing::new(words_of(&sk));
        crate::safe::session::global().with(&vault, |s| {
            let sealed = s.rotate_safe_key(&password, &sk)?;
            s.guard_secret_key(&words);
            log::info!("[Safe] the Safe Key was replaced; {sealed} files sealed again");
            Ok(sealed)
        })
    })
    .await
}

/// A new Secret Key for the same Safe. The new words go back to the screen —
/// the one other time they do after creation — for a new Emergency Kit.
#[tauri::command]
pub async fn safe_change_secret_key(
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
        crate::safe::session::global().peek(&vault, |_| Ok(()))?;
        let current = keyset::read(&vault)?;
        let sk = stored_secret_key(&handle, &current.header.safe_id)?.ok_or(SafeError::NeedsSecretKey)?;
        let key = keyset::unlock(&current, &password, &sk)?;
        let (next, new_sk) = keyset::change_secret_key(&vault, &current, &key, &password)?;
        let stored_on_device = match store_secret_key(&handle, &next.header.safe_id, &new_sk) {
            Ok(()) => true,
            Err(e) => {
                log::error!("[Safe] the Secret Key was changed, but {e}");
                false
            }
        };
        let words = words_of(&new_sk);
        let guard_words = zeroize::Zeroizing::new(words.clone());
        let _ = crate::safe::session::global().peek_mut(&vault, |s| {
            s.replace_keyset(next);
            s.guard_secret_key(&guard_words);
            s.words_just_shown();
            Ok(())
        });
        Ok(Created { secret_key: words, stored_on_device })
    })
    .await
}

/// Write the Emergency Kit to `path`: a page to print, with the Secret Key and
/// a blank for the master password to be written in by hand.
///
/// The Secret Key comes from the keychain here rather than from the screen, so
/// the words do not travel back through the WebView to be written out. Behind
/// the master password, like showing the words: the kit *is* the words —
/// except in the half hour after the words were shown (the Safe created, its
/// Secret Key changed), when the password was typed a moment ago and the
/// words are on the screen already.
#[tauri::command]
pub async fn safe_save_emergency_kit(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    path: String,
    password: Option<String>,
) -> AppResult<()> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password.unwrap_or_default());
    let handle = app.clone();
    blocking(move || {
        let safe_id = crate::safe::session::global().peek(&vault, |s| Ok(s.keyset().header.safe_id))?;
        let keyset = keyset::read(&vault)?;
        let sk = stored_secret_key(&handle, &safe_id)?.ok_or(SafeError::NeedsSecretKey)?;
        if !crate::safe::session::global().peek(&vault, |s| Ok(s.words_shown_recently()))? {
            keyset::unlock(&keyset, &password, &sk)?;
        }
        let html = zeroize::Zeroizing::new(emergency_kit(&words_of(&sk), &safe_id, &chrono::Local::now().format("%Y-%m-%d").to_string()));
        crate::safe::store::write_atomic(std::path::Path::new(&path), html.as_bytes())
            .map_err(|e| SafeError::Failed(format!("could not write the Emergency Kit: {e}")))
    })
    .await
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

fn with<R>(vault_path: &str, f: impl FnOnce(&mut Unlocked) -> Result<R, SafeError>) -> AppResult<R> {
    let vault = vault(vault_path)?;
    crate::safe::session::global().with(&vault, f).map_err(AppError::Safe)
}

/// Read the Safe again after sync brought items from another device, and
/// fold in any version it set aside.
#[tauri::command]
pub async fn safe_refresh(webview: tauri::Webview, vault_path: String) -> AppResult<()> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    // Every item is decrypted again: off the main thread.
    blocking(move || crate::safe::session::global().peek_mut(&vault, |s| s.reload())).await
}

#[tauri::command]
pub fn safe_overview(webview: tauri::Webview, vault_path: String) -> AppResult<Overview> {
    gate(&webview)?;
    with(&vault_path, |s| Ok(s.overview()))
}

#[tauri::command]
pub fn safe_list(
    webview: tauri::Webview,
    vault_path: String,
    filter: Option<Filter>,
    query: Option<String>,
) -> AppResult<Vec<ItemSummary>> {
    gate(&webview)?;
    with(&vault_path, |s| Ok(s.list(&filter.unwrap_or_default(), query.as_deref().unwrap_or(""))))
}

#[tauri::command]
pub fn safe_get(webview: tauri::Webview, vault_path: String, id: String) -> AppResult<ItemView> {
    gate(&webview)?;
    with(&vault_path, |s| s.view(&id))
}

/// The one command that returns a secret value: one field, because the user
/// asked to see it.
#[tauri::command]
pub fn safe_reveal(
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    field: String,
) -> AppResult<String> {
    gate(&webview)?;
    with(&vault_path, |s| s.reveal(&id, &field).map(|v| v.expose().to_string()))
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
    let (value, clear_after) = with(&vault_path, |s| Ok((s.reveal(&id, &field)?, s.settings().clipboard_clear_secs)))?;
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

/// Copy what Quick Access copies: an item's password, its username, or its
/// current one-time code — without the screen knowing which field that is.
#[tauri::command]
pub async fn safe_copy_primary(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    what: String,
) -> AppResult<Copied> {
    use crate::safe::item::FieldKind;
    if what == "totp" {
        return safe_copy_totp(app, webview, vault_path, id);
    }
    gate(&webview)?;
    let kinds: &[FieldKind] = match what.as_str() {
        "username" => &[FieldKind::Username, FieldKind::Email],
        _ => &[FieldKind::Password, FieldKind::Concealed, FieldKind::Pin],
    };
    let field = with(&vault_path, |s| {
        let view = s.view(&id)?;
        view.fields.iter().find(|f| kinds.contains(&f.kind) && !f.empty).map(|f| f.id.clone()).ok_or(SafeError::NotFound)
    })?;
    safe_copy(app, webview, vault_path, id, field)
}

/// The current one-time code of an item. The secret stays here; a code is
/// worth thirty seconds.
#[tauri::command]
pub fn safe_totp(
    webview: tauri::Webview,
    vault_path: String,
    id: String,
) -> AppResult<crate::safe::totp::Code> {
    gate(&webview)?;
    // Not use: the item's screen asks every thirty seconds by itself, and an
    // item left showing its code must not keep the Safe from locking.
    let vault = vault(&vault_path)?;
    crate::safe::session::global().peek(&vault, |s| s.totp(&id, now() as u64)).map_err(AppError::Safe)
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
    let (code, clear_after) = with(&vault_path, |s| {
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
pub fn safe_create_item(webview: tauri::Webview, vault_path: String, item: ItemEdit) -> AppResult<ItemView> {
    gate(&webview)?;
    with(&vault_path, |s| s.create(item, now()))
}

#[tauri::command]
pub fn safe_update_item(
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    item: ItemEdit,
) -> AppResult<ItemView> {
    gate(&webview)?;
    with(&vault_path, |s| s.update(&id, item, now()))
}

#[tauri::command]
pub fn safe_set_favorite(
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    favorite: bool,
) -> AppResult<()> {
    gate(&webview)?;
    with(&vault_path, |s| s.set_favorite(&id, favorite, now()))
}

#[tauri::command]
pub fn safe_set_trashed(
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    trashed: bool,
) -> AppResult<()> {
    gate(&webview)?;
    with(&vault_path, |s| s.set_trashed(&id, trashed, now()))
}

#[tauri::command]
pub fn safe_purge(webview: tauri::Webview, vault_path: String, id: String) -> AppResult<()> {
    gate(&webview)?;
    with(&vault_path, |s| s.purge(&id))
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
    webview: tauri::Webview,
    vault_path: String,
    path: String,
    password: Option<String>,
) -> AppResult<Imported> {
    use crate::safe::exchange::{self, ExchangeError, Format};
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password.unwrap_or_default());
    blocking(move || {
        let file = std::path::Path::new(&path);
        let bytes = zeroize::Zeroizing::new(std::fs::read(file).map_err(|e| SafeError::Failed(format!("could not read the file: {e}")))?);
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let format = exchange::detect(&name, &bytes).ok_or(SafeError::ImportUnknown)?;
        if matches!(format, Format::SafeExport | Format::Kdbx) && password.is_empty() {
            return Err(SafeError::NeedsExportPassword);
        }
        let parsed = exchange::parse(format, &bytes, Some(&password), now()).map_err(|e| match e {
            ExchangeError::Unknown => SafeError::ImportUnknown,
            ExchangeError::EncryptedBitwarden => SafeError::EncryptedBitwarden,
            ExchangeError::WrongExportPassword => SafeError::WrongExportPassword,
            ExchangeError::WrongKdbxPassword => SafeError::WrongKdbxPassword,
            other => SafeError::Failed(other.to_string()),
        })?;
        // In batches, letting go of the Safe between them: a list or a code
        // asked for meanwhile waits for fifty items, not for two thousand.
        let mut items = parsed.items;
        let mut imported = 0;
        while !items.is_empty() {
            let rest = items.split_off(items.len().min(50));
            let batch = std::mem::replace(&mut items, rest);
            imported += crate::safe::session::global().with(&vault, |s| s.import(batch))?;
        }
        Ok(Imported { format, imported, warnings: parsed.warnings, source_was_plaintext: !matches!(format, Format::SafeExport | Format::Kdbx) })
    })
    .await
}

/// Seal every item into a `.safe-export` under a password chosen for it.
#[tauri::command]
pub async fn safe_export(
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
    blocking(move || {
        let items = crate::safe::session::global().with(&vault, |s| s.all_items())?;
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
    secret_key: Option<String>,
) -> AppResult<usize> {
    gate(&webview)?;
    let vault = vault(&vault_path)?;
    let password = zeroize::Zeroizing::new(password);
    let typed = zeroize::Zeroizing::new(secret_key.unwrap_or_default());
    let handle = app.clone();
    blocking(move || {
        let keyset = keyset::read(&vault)?;
        let sk = secret_key_for(&handle, &keyset.header.safe_id, &typed)?;
        keyset::unlock(&keyset, &password, &sk)?;
        let items = crate::safe::session::global().with(&vault, |s| s.all_items())?;
        let csv = crate::safe::exchange::to_csv(&items);
        crate::safe::store::write_atomic(std::path::Path::new(&path), csv.as_bytes()).map_err(|e| SafeError::Failed(e.to_string()))?;
        log::warn!("[Safe] {} items were exported as plaintext", items.len());
        Ok(items.iter().filter(|i| i.trashed_at.is_none()).count())
    })
    .await
}

// ─── Syn ─────────────────────────────────────────────────

/// Set what Syn may do with an item: its level, its handle, and the places it
/// may be sent. The user's decision, made here and nowhere else.
#[tauri::command]
pub fn safe_set_ai(
    webview: tauri::Webview,
    vault_path: String,
    id: String,
    level: crate::safe::item::AiLevel,
    handle: Option<String>,
    destinations: Vec<String>,
) -> AppResult<ItemView> {
    gate(&webview)?;
    with(&vault_path, |s| s.set_ai(&id, level, handle, destinations, now()))
}

#[derive(Serialize)]
pub struct Destination {
    key: String,
    label: String,
}

/// The places an item may be sent: the vault's connectors.
#[tauri::command]
pub fn safe_destinations(webview: tauri::Webview, vault_path: String) -> AppResult<Vec<Destination>> {
    gate(&webview)?;
    Ok(crate::syn::connector::config::load(&vault_path)
        .servers
        .into_iter()
        .map(|s| Destination { key: format!("connector:{}", s.id), label: s.name })
        .collect())
}

/// The card `safe_request` showed, answered: the value goes from the card to
/// the Safe, and Syn learns only that the handle now exists.
#[tauri::command]
pub fn safe_request_submit(
    webview: tauri::Webview,
    vault_path: String,
    request_id: String,
    title: String,
    handle: String,
    value: String,
    destinations: Vec<String>,
) -> AppResult<String> {
    use crate::safe::item::{AiLevel, EditValue, FieldEdit, FieldKind, ItemEdit, ItemKind, SecretString};
    gate(&webview)?;
    let value = SecretString::new(value);
    // Locked, the request stays open: what the user typed can be sent again
    // once they unlock, rather than the card answering "gone".
    with(&vault_path, |_| Ok(()))?;
    let request = crate::safe::requests::take(&vault_path, &request_id).ok_or(AppError::Safe(SafeError::RequestGone))?;
    // Only places the card offered: a destination is a decision the user
    // makes by ticking it, not a string a request can smuggle in.
    let offered: Vec<&str> = request.destinations.iter().map(|d| d.key.as_str()).collect();
    let destinations: Vec<String> = destinations.into_iter().filter(|d| offered.contains(&d.as_str())).collect();
    let level = if destinations.is_empty() { AiLevel::Listed } else { AiLevel::Usable };
    with(&vault_path, |s| {
        let view = s.create(
            ItemEdit {
                kind: ItemKind::ApiKey,
                title,
                fields: vec![FieldEdit {
                    id: None,
                    label: "token".into(),
                    kind: FieldKind::Concealed,
                    value: EditValue::Set { v: value },
                }],
                urls: vec![],
                tags: vec![],
                favorite: false,
                notes: request.why.clone(),
                totp: Default::default(),
                expires_at: None,
            },
            now(),
        )?;
        match s.set_ai(&view.id, level, Some(handle.clone()), destinations, now()) {
            Ok(_) => Ok(handle),
            // The item is saved either way; a clashing name is the user's to fix
            // in Safe, not a reason to lose what they typed.
            Err(e) => {
                log::warn!("[Safe] a requested item was saved without Syn's name: {e}");
                Err(e)
            }
        }
    })
}

// ─── health ──────────────────────────────────────────────

#[derive(Serialize)]
pub struct BreachReport {
    checked: usize,
    breached: usize,
    /// Ranges that could not be fetched; those passwords were not checked.
    failed: usize,
}

/// Check every password against Have I Been Pwned, by k-anonymity: five hex
/// characters of each SHA-1 leave this machine, never a password or a whole
/// hash, and each five only once however many passwords share them. See
/// `safe::health::breach`. Only when the user turned it on.
#[tauri::command]
pub async fn safe_check_breaches(webview: tauri::Webview, vault_path: String) -> AppResult<BreachReport> {
    use crate::safe::health::breach;
    use std::collections::{BTreeMap, HashSet};
    gate(&webview)?;
    let queries = with(&vault_path, |s| {
        if !s.settings().breach_check {
            return Err(SafeError::BreachCheckOff);
        }
        s.breach_queries()
    })?;
    let mut by_prefix: BTreeMap<String, Vec<(crate::safe::store::ItemId, String)>> = BTreeMap::new();
    for (id, prefix, suffix) in queries {
        by_prefix.entry(prefix).or_default().push((id, suffix));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Synabit-Safe")
        .build()
        .map_err(|e| AppError::General(e.to_string()))?;
    let (mut breached, mut failed, mut checked) = (HashSet::new(), 0, 0);
    for (prefix, entries) in &by_prefix {
        let answer = client
            .get(format!("{}{prefix}", breach::RANGE_URL))
            .header("Add-Padding", "true")
            .send()
            .await
            .and_then(|r| r.error_for_status());
        let body = match answer {
            Ok(r) => r.text().await.ok(),
            Err(_) => None,
        };
        let Some(body) = body else {
            failed += entries.len();
            continue;
        };
        for (id, suffix) in entries {
            checked += 1;
            if breach::seen_in(&body, suffix).is_some() {
                breached.insert(*id);
            }
        }
    }
    let found = breached.len();
    with(&vault_path, |s| {
        s.set_breached(breached, now());
        Ok(())
    })?;
    Ok(BreachReport { checked, breached: found, failed })
}

// ─── this device ─────────────────────────────────────────

/// The secrets this device's keychain holds beside the Safe — names only.
/// Behind the open Safe, so that a glance at an unlocked machine does not
/// list them.
#[tauri::command]
pub async fn safe_device_secrets(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
) -> AppResult<Vec<crate::safe::device::DeviceSecret>> {
    gate(&webview)?;
    with(&vault_path, |_| Ok(()))?;
    let handle = app.clone();
    blocking(move || {
        let secrets = SecretManager::load_secrets(Some(&handle));
        let connectors = crate::syn::connector::config::load(&vault_path);
        Ok(crate::safe::device::describe(&secrets, |id| {
            connectors.servers.iter().find(|s| s.id == id).map(|s| s.name.clone())
        }))
    })
    .await
}

/// Forget one of those secrets. Only a provider key or a connector's secret:
/// the sync key, the PIN and Telegram have screens of their own that say
/// what forgetting them does.
#[tauri::command]
pub async fn safe_forget_device_secret(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    slot: String,
) -> AppResult<()> {
    gate(&webview)?;
    with(&vault_path, |_| Ok(()))?;
    let handle = app.clone();
    blocking(move || {
        let secrets = SecretManager::load_secrets(Some(&handle));
        if !crate::safe::device::may_forget(&secrets, &slot) {
            return Err(SafeError::NotFound);
        }
        SecretManager::set_syn_api_key(Some(&handle), &slot, "").map_err(SafeError::Keychain)
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

/// How hard a typed password is to guess: zxcvbn's 0–4 score and bits.
#[tauri::command]
pub fn safe_estimate(webview: tauri::Webview, password: String) -> AppResult<crate::safe::health::Strength> {
    gate(&webview)?;
    let password = zeroize::Zeroizing::new(password);
    Ok(crate::safe::health::strength(&password, &[]))
}

#[tauri::command]
pub fn safe_get_settings(webview: tauri::Webview, vault_path: String) -> AppResult<Settings> {
    gate(&webview)?;
    with(&vault_path, |s| Ok(s.settings()))
}

#[tauri::command]
pub fn safe_set_settings(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    vault_path: String,
    settings: Settings,
) -> AppResult<Settings> {
    gate(&webview)?;
    let saved = with(&vault_path, |s| s.set_settings(settings))?;
    sockets_follow(&app, &saved);
    Ok(saved)
}

/// Start or stop the SSH agent and the command-line socket to match the settings.
fn sockets_follow(app: &tauri::AppHandle, settings: &Settings) {
    #[cfg(all(desktop, unix))]
    {
        if settings.ssh_agent {
            crate::safe::ssh_agent::start(app);
        } else {
            crate::safe::ssh_agent::stop();
        }
        if settings.cli {
            crate::safe::cli_server::start(app);
        } else {
            crate::safe::cli_server::stop(app);
        }
    }
    #[cfg(not(all(desktop, unix)))]
    {
        let _ = (app, settings);
    }
}

#[derive(Serialize)]
pub struct SshKeyView {
    title: String,
    fingerprint: Option<String>,
    /// The line for `authorized_keys`, or why the key cannot be used.
    public: Option<String>,
    problem: Option<String>,
}

#[derive(Serialize)]
pub struct SshStatus {
    /// Whether this platform has the agent at all.
    supported: bool,
    running: bool,
    socket: Option<String>,
    keys: Vec<SshKeyView>,
}

/// What the SSH agent is doing, and which of the Safe's keys it offers.
#[tauri::command]
pub async fn safe_ssh_status(webview: tauri::Webview, vault_path: String) -> AppResult<SshStatus> {
    gate(&webview)?;
    #[cfg(all(desktop, unix))]
    {
        let vault = vault(&vault_path)?;
        // Every item is decrypted to find the keys: off the main thread.
        let pems = blocking(move || crate::safe::session::global().peek(&vault, |s| Ok(s.ssh_keys()))).await?;
        let keys = pems
            .into_iter()
            .map(|(title, pem)| match crate::safe::ssh::parse(pem.expose()) {
                Ok(k) => SshKeyView { title, fingerprint: Some(k.fingerprint()), public: Some(k.authorized_line()), problem: None },
                Err(e) => SshKeyView { title, fingerprint: None, public: None, problem: Some(e.to_string()) },
            })
            .collect();
        let at = crate::safe::ssh_agent::running_at();
        Ok(SshStatus { supported: true, running: at.is_some(), socket: at.map(|p| p.display().to_string()), keys })
    }
    #[cfg(not(all(desktop, unix)))]
    {
        let _ = vault_path;
        Ok(SshStatus { supported: false, running: false, socket: None, keys: Vec::new() })
    }
}

/// Whether `synabit-safe run` can reach the app, and the socket it uses.
#[tauri::command]
pub fn safe_cli_status(app: tauri::AppHandle, webview: tauri::Webview) -> AppResult<serde_json::Value> {
    use tauri::Manager;
    gate(&webview)?;
    let socket = app.path().app_data_dir().ok().map(|d| d.join(crate::safe::cli::SOCKET_NAME).display().to_string());
    #[cfg(all(desktop, unix))]
    let (supported, running) = (true, crate::safe::cli_server::running());
    #[cfg(not(all(desktop, unix)))]
    let (supported, running) = (false, false);
    Ok(serde_json::json!({ "supported": supported, "running": running, "socket": socket }))
}

/// The user's answer on an SSH or command-line approval card.
#[tauri::command]
pub fn safe_ssh_answer(webview: tauri::Webview, id: String, allow: bool) -> AppResult<()> {
    gate(&webview)?;
    crate::safe::approvals::answer(&id, allow);
    Ok(())
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
            let session = crate::safe::session::global();
            let vault = session.open_vault();
            let locked = if crate::safe::session::slept(last, now, TICK) {
                session.lock().then(|| log::info!("[Safe] locked: the machine was asleep"))
            } else {
                session.lock_if_idle(std::time::Instant::now()).then(|| log::info!("[Safe] locked after being left alone"))
            };
            if locked.is_some() {
                after_lock(&app, vault);
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
        assert!(may_use_safe("safe-quick"));
        assert!(!may_use_safe("quick-entry"), "the capture box has no use for Safe");
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
