//! The app-lock PIN, and the things only it may do.
//!
//! # What the PIN is, and is not
//!
//! A lock on this app's window. It is checked here, in the backend, for every
//! change that loosens a guard — removing the PIN, replacing it, switching
//! family-safe answers or the whole-app lock off, unprotecting an app or note,
//! lengthening the auto-lock timeout — so a screen that forgot to ask, or a
//! script in the webview, cannot do those things without it.
//!
//! It is not encryption. The vault is a folder of ordinary files, readable in
//! any editor by anybody with the folder; the PIN hash and the flags beside it
//! sit in this OS account's keychain (on a phone, the app's own storage), and
//! somebody with that account can delete them. Everything here assumes a person
//! at the app, not a person with the machine.
//!
//! # A forgotten PIN
//!
//! There is no reset that asks only the app, because anybody at the app could
//! use it — including the child a family-safe PIN is set for. The reset here
//! (`app_lock_reset_begin` / `app_lock_reset_finish`, desktop only) asks for
//! proof that the person can reach this computer's app-data folder through
//! the operating system: a folder with a one-time name, made there by hand.
//! That is the same access that could already delete the PIN from the
//! keychain, so it opens no door that was not open. The operating system's own
//! sign-in (Touch ID, Windows Hello) would be better and needs a plugin this
//! app does not have.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::secrets::SecretManager;

// ── Rate Limiting State (in-memory, resets on app restart) ──

#[derive(Default)]
pub struct AppLockState {
    pub failed_attempts: Mutex<u32>,
    pub locked_until: Mutex<Option<u64>>,
    /// The forgotten-PIN reset in progress, if any. In memory only: a reset
    /// begun before a restart is begun again.
    pub reset: Mutex<Option<ResetChallenge>>,
}

// ── DTOs ──

#[derive(Serialize, Clone)]
pub struct VerifyResult {
    pub success: bool,
    pub remaining_attempts: u8,
    pub locked_until: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct AppLockConfig {
    pub is_enabled: bool,
    pub app_lock_active: bool,
    pub protected_apps: Vec<String>,
    pub protected_notes: Vec<String>,
    pub auto_lock_timeout_secs: u64,
}

#[derive(Deserialize)]
pub struct AppLockConfigUpdate {
    pub protected_apps: Option<Vec<String>>,
    pub protected_notes: Option<Vec<String>>,
    pub auto_lock_timeout_secs: Option<u64>,
    pub app_lock_active: Option<bool>,
}

/// What the lock screen shows somebody who forgot the PIN: make a folder
/// called `name` inside `folder`, then press reset.
#[derive(Serialize, Clone, Debug)]
pub struct ResetChallenge {
    pub folder: String,
    pub name: String,
    #[serde(skip)]
    pub expires_at: u64,
}

const MAX_ATTEMPTS: u32 = 5;
const LOCKOUT_DURATION_SECS: u64 = 30;
const DEFAULT_TIMEOUT_SECS: u64 = 300;
/// Long enough to find a folder in a file manager; short enough that a name
/// read off the screen yesterday is no use today.
const RESET_VALID_SECS: u64 = 30 * 60;

/// Errors the front end matches on, so they are codes rather than prose.
pub const PIN_REQUIRED: &str = "PIN_REQUIRED";
pub const PIN_WRONG: &str = "PIN_WRONG";
pub const PIN_LOCKED_OUT: &str = "PIN_LOCKED_OUT";
pub const PIN_ALREADY_SET: &str = "PIN_ALREADY_SET";
pub const RESET_NOT_STARTED: &str = "RESET_NOT_STARTED";
pub const RESET_EXPIRED: &str = "RESET_EXPIRED";
pub const RESET_NOT_PROVEN: &str = "RESET_NOT_PROVEN";
pub const RESET_NOT_ON_THIS_DEVICE: &str = "RESET_NOT_ON_THIS_DEVICE";

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn valid_pin(pin: &str) -> Result<(), String> {
    if pin.len() != 6 || !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err("PIN must be exactly 6 digits".to_string());
    }
    Ok(())
}

fn hash_pin(pin: &str) -> Result<String, String> {
    valid_pin(pin)?;
    use argon2::{
        password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
        Algorithm, Argon2, Params, Version,
    };
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(19456, 2, 1, None).map_err(|e| e.to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    Ok(argon2
        .hash_password(pin.as_bytes(), &salt)
        .map_err(|e| format!("Hash error: {}", e))?
        .to_string())
}

/// Check `pin` against the stored Argon2id `hash`, counting failures.
///
/// The one place a PIN is compared, so every command that takes one shares the
/// same five-tries-then-wait budget: asking through `remove_app_lock` instead
/// of the lock screen buys no extra guesses.
fn check_pin(state: &AppLockState, hash: &str, pin: &str) -> Result<VerifyResult, String> {
    {
        let locked = state.locked_until.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(until) = *locked {
            if now_unix() < until {
                return Ok(VerifyResult { success: false, remaining_attempts: 0, locked_until: Some(until) });
            }
        }
    }

    use argon2::{
        password_hash::{PasswordHash, PasswordVerifier},
        Argon2,
    };
    let parsed_hash = PasswordHash::new(hash).map_err(|e| format!("Hash parse error: {}", e))?;
    // Parameters come from the PHC string, so a hash made with other costs
    // still verifies.
    let is_valid = Argon2::default().verify_password(pin.as_bytes(), &parsed_hash).is_ok();

    if is_valid {
        *state.failed_attempts.lock().unwrap_or_else(|p| p.into_inner()) = 0;
        *state.locked_until.lock().unwrap_or_else(|p| p.into_inner()) = None;
        return Ok(VerifyResult { success: true, remaining_attempts: MAX_ATTEMPTS as u8, locked_until: None });
    }

    let mut attempts = state.failed_attempts.lock().unwrap_or_else(|p| p.into_inner());
    *attempts += 1;
    let remaining = MAX_ATTEMPTS.saturating_sub(*attempts) as u8;
    let locked_until = if *attempts >= MAX_ATTEMPTS {
        let until = now_unix() + LOCKOUT_DURATION_SECS;
        *state.locked_until.lock().unwrap_or_else(|p| p.into_inner()) = Some(until);
        *attempts = 0;
        Some(until)
    } else {
        None
    };
    Ok(VerifyResult { success: false, remaining_attempts: remaining, locked_until })
}

/// `Ok` when there is no PIN to prove, or `pin` is it.
///
/// No PIN set means nothing to ask for: the household has one secret, and a
/// guard that invented a second would be a PIN nobody remembers setting.
pub(crate) fn require_pin(state: &AppLockState, hash: Option<&str>, pin: Option<&str>) -> Result<(), String> {
    let Some(hash) = hash else { return Ok(()) };
    let Some(pin) = pin.filter(|p| !p.is_empty()) else {
        return Err(PIN_REQUIRED.to_string());
    };
    let result = check_pin(state, hash, pin)?;
    if result.success {
        Ok(())
    } else if result.locked_until.is_some() {
        Err(PIN_LOCKED_OUT.to_string())
    } else {
        Err(PIN_WRONG.to_string())
    }
}

/// Family-safe may always be switched on; off needs the PIN, when there is one.
fn may_set_family_safe(
    currently_on: bool,
    want_on: bool,
    prove: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if currently_on && !want_on {
        prove()
    } else {
        Ok(())
    }
}

/// Whether the folder the reset asked for has been made, in time.
fn reset_proven(challenge: Option<&ResetChallenge>, now: u64) -> Result<PathBuf, String> {
    let challenge = challenge.ok_or_else(|| RESET_NOT_STARTED.to_string())?;
    if now >= challenge.expires_at {
        return Err(RESET_EXPIRED.to_string());
    }
    let marker = Path::new(&challenge.folder).join(&challenge.name);
    if marker.exists() {
        Ok(marker)
    } else {
        Err(RESET_NOT_PROVEN.to_string())
    }
}

fn new_challenge(folder: &Path, now: u64) -> ResetChallenge {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    let code = OsRng.next_u32() % 1_000_000;
    ResetChallenge {
        folder: folder.to_string_lossy().into_owned(),
        name: format!("synabit-reset-{code:06}"),
        expires_at: now + RESET_VALID_SECS,
    }
}

// ── Commands ──

/// Set the first PIN. Refused when one exists: replacing it is
/// `change_app_lock`, which asks for the old one — otherwise "set up" would be
/// a way to swap in a PIN you know and then remove it.
#[tauri::command]
pub fn setup_app_lock(app: tauri::AppHandle, pin: String) -> Result<(), String> {
    if SecretManager::get_app_lock_hash(Some(&app)).is_some() {
        return Err(PIN_ALREADY_SET.to_string());
    }
    store_pin(&app, &pin)
}

fn store_pin(app: &tauri::AppHandle, pin: &str) -> Result<(), String> {
    let hash = hash_pin(pin)?;
    SecretManager::set_app_lock_hash(Some(app), hash)?;

    // Set default timeout if not already set
    let (_, _, timeout, _) = SecretManager::get_app_lock_config(Some(app));
    if timeout.is_none() {
        SecretManager::update_app_lock_config(Some(app), None, None, Some(DEFAULT_TIMEOUT_SECS), None)?;
    }
    Ok(())
}

#[tauri::command]
pub fn verify_app_lock(
    app: tauri::AppHandle,
    pin: String,
    state: tauri::State<'_, AppLockState>,
) -> Result<VerifyResult, String> {
    let hash = SecretManager::get_app_lock_hash(Some(&app)).ok_or("App lock is not set up")?;
    check_pin(&state, &hash, &pin)
}

/// Remove the PIN — only with the PIN.
///
/// It used to take nothing, so the settings screen's PIN prompt was the only
/// thing in the way, and anything that could call a command could remove it.
/// Family-safe answers stay as they are; they are not the PIN's to undo.
#[tauri::command]
pub fn remove_app_lock(
    app: tauri::AppHandle,
    pin: String,
    state: tauri::State<'_, AppLockState>,
) -> Result<(), String> {
    let hash = SecretManager::get_app_lock_hash(Some(&app));
    require_pin(&state, hash.as_deref(), Some(&pin))?;
    SecretManager::clear_app_lock(Some(&app))
}

#[tauri::command]
pub fn change_app_lock(
    app: tauri::AppHandle,
    old_pin: String,
    new_pin: String,
    state: tauri::State<'_, AppLockState>,
) -> Result<(), String> {
    valid_pin(&new_pin)?;
    let hash = SecretManager::get_app_lock_hash(Some(&app)).ok_or("App lock is not set up")?;
    require_pin(&state, Some(&hash), Some(&old_pin)).map_err(|e| {
        if e == PIN_WRONG { "Current PIN is incorrect".to_string() } else { e }
    })?;
    store_pin(&app, &new_pin)
}

#[tauri::command]
pub fn get_app_lock_config(app: tauri::AppHandle) -> Result<AppLockConfig, String> {
    Ok(read_config(&app))
}

fn read_config(app: &tauri::AppHandle) -> AppLockConfig {
    let is_enabled = SecretManager::get_app_lock_hash(Some(app)).is_some();
    let (protected_apps, protected_notes, timeout, app_lock_active) =
        SecretManager::get_app_lock_config(Some(app));

    AppLockConfig {
        is_enabled,
        app_lock_active: app_lock_active.unwrap_or(false),
        protected_apps: protected_apps.unwrap_or_default(),
        protected_notes: protected_notes.unwrap_or_default(),
        auto_lock_timeout_secs: timeout.unwrap_or(DEFAULT_TIMEOUT_SECS),
    }
}

/// Whether an auto-lock timeout of `next` seconds guards less than `current`.
/// `0` is "never", the loosest there is.
fn timeout_loosens(current: u64, next: u64) -> bool {
    current != 0 && (next == 0 || next > current)
}

/// Whether `update` would guard less than `current` does: the whole-app lock
/// switched off, any app or note no longer protected, or a longer (or no)
/// auto-lock timeout. Adding protection, or leaving a field out, never does.
fn loosens(current: &AppLockConfig, update: &AppLockConfigUpdate) -> bool {
    let lock_off = current.app_lock_active && update.app_lock_active == Some(false);
    let drops = |now: &[String], next: &Option<Vec<String>>| {
        next.as_ref().is_some_and(|next| now.iter().any(|id| !next.contains(id)))
    };
    let timeout = update
        .auto_lock_timeout_secs
        .is_some_and(|next| timeout_loosens(current.auto_lock_timeout_secs, next));
    lock_off
        || drops(&current.protected_apps, &update.protected_apps)
        || drops(&current.protected_notes, &update.protected_notes)
        || timeout
}

/// Change what the PIN guards. Tightening is free; loosening (see `loosens`)
/// needs the PIN when one is set, checked here with the lock screen's attempt
/// limit — otherwise switching the whole-app lock off, or unprotecting a note,
/// would be a way round the PIN that `remove_app_lock` already refuses.
#[tauri::command]
pub fn update_app_lock_config(
    app: tauri::AppHandle,
    config: AppLockConfigUpdate,
    pin: Option<String>,
    state: tauri::State<'_, AppLockState>,
) -> Result<(), String> {
    if loosens(&read_config(&app), &config) {
        let hash = SecretManager::get_app_lock_hash(Some(&app));
        require_pin(&state, hash.as_deref(), pin.as_deref())?;
    }
    SecretManager::update_app_lock_config(
        Some(&app),
        config.protected_apps,
        config.protected_notes,
        config.auto_lock_timeout_secs,
        config.app_lock_active,
    )
}

/// The vault settings file's old family-safe flag, for the one-time carry.
fn vault_family_safe(vault_path: Option<&str>) -> bool {
    vault_path
        .filter(|v| !v.is_empty())
        .and_then(|v| crate::syn::settings::load_settings(v).ok())
        .is_some_and(|s| s.family_safe)
}

/// Whether Syn's family-safe answers are on, on this device.
///
/// `vault_path`, when given, lets a vault that had it on carry that across
/// (see `syn::family_safe::resolve`); it is never a way to turn it off.
#[tauri::command]
pub fn get_family_safe(app: tauri::AppHandle, vault_path: Option<String>) -> bool {
    crate::syn::family_safe::is_on(Some(&app), vault_family_safe(vault_path.as_deref()))
}

/// Switch family-safe answers on or off on this device.
///
/// On is always allowed. Off needs the app-lock PIN when one is set, checked
/// here against the stored hash with the same attempt limit as the lock
/// screen — the settings screen asking first is a courtesy, not the lock.
#[tauri::command]
pub fn set_family_safe(
    app: tauri::AppHandle,
    on: bool,
    pin: Option<String>,
    vault_path: Option<String>,
    state: tauri::State<'_, AppLockState>,
) -> Result<(), String> {
    let currently_on = crate::syn::family_safe::is_on(Some(&app), vault_family_safe(vault_path.as_deref()));
    may_set_family_safe(currently_on, on, || {
        let hash = SecretManager::get_app_lock_hash(Some(&app));
        require_pin(&state, hash.as_deref(), pin.as_deref())
    })?;
    crate::syn::family_safe::store(Some(&app), on)
}

/// Start a forgotten-PIN reset: the folder to make, and where.
///
/// Desktop only. On a phone the app's data folder is out of reach without the
/// phone's own "clear storage", which is the reset there.
#[tauri::command]
pub fn app_lock_reset_begin(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppLockState>,
) -> Result<ResetChallenge, String> {
    if cfg!(mobile) {
        return Err(RESET_NOT_ON_THIS_DEVICE.to_string());
    }
    use tauri::Manager;
    let folder = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let challenge = new_challenge(&folder, now_unix());
    *state.reset.lock().unwrap_or_else(|p| p.into_inner()) = Some(challenge.clone());
    Ok(challenge)
}

/// Finish the reset: if the folder was made, the PIN and every guard it held
/// (whole-app lock, protected apps and notes) are cleared. Family-safe answers
/// stay on if they were on — but with no PIN, nothing then stops them being
/// switched off, which the lock screen says before anyone gets here.
#[tauri::command]
pub fn app_lock_reset_finish(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppLockState>,
) -> Result<(), String> {
    if cfg!(mobile) {
        return Err(RESET_NOT_ON_THIS_DEVICE.to_string());
    }
    let mut slot = state.reset.lock().unwrap_or_else(|p| p.into_inner());
    let marker = reset_proven(slot.as_ref(), now_unix())?;
    SecretManager::clear_app_lock(Some(&app))?;
    *slot = None;
    *state.failed_attempts.lock().unwrap_or_else(|p| p.into_inner()) = 0;
    *state.locked_until.lock().unwrap_or_else(|p| p.into_inner()) = None;
    // Tidy, not required: a stale folder cannot be reused, its name is not
    // asked for again.
    let _ = std::fs::remove_dir_all(&marker).or_else(|_| std::fs::remove_file(&marker));
    log::info!("[AppLock] PIN reset after the reset folder was made");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Argon2id PHC string, made cheaply: verification reads the costs
    /// from the string, so the production parameters are not needed to test
    /// the comparison, only to slow down an attacker.
    fn cheap_hash(pin: &str) -> String {
        use argon2::{
            password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
            Algorithm, Argon2, Params, Version,
        };
        let salt = SaltString::generate(&mut OsRng);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(8, 1, 1, None).unwrap())
            .hash_password(pin.as_bytes(), &salt)
            .unwrap()
            .to_string()
    }

    #[test]
    fn with_no_pin_set_there_is_nothing_to_prove() {
        let state = AppLockState::default();
        assert_eq!(require_pin(&state, None, None), Ok(()));
    }

    /// `remove_app_lock` is `require_pin` and then the clear, so this is what
    /// stands between a caller and removing the lock.
    #[test]
    fn removing_the_lock_needs_the_pin() {
        let state = AppLockState::default();
        let hash = cheap_hash("123456");
        assert_eq!(require_pin(&state, Some(&hash), None), Err(PIN_REQUIRED.into()));
        assert_eq!(require_pin(&state, Some(&hash), Some("")), Err(PIN_REQUIRED.into()));
        assert_eq!(require_pin(&state, Some(&hash), Some("654321")), Err(PIN_WRONG.into()));
        assert_eq!(require_pin(&state, Some(&hash), Some("123456")), Ok(()));
    }

    #[test]
    fn remove_app_lock_takes_a_pin_and_checks_it_before_clearing() {
        let source = include_str!("app_lock.rs");
        let body = source.split("pub fn remove_app_lock(").nth(1).unwrap().split("\n}\n").next().unwrap();
        let check = body.find("require_pin(").expect("checks the PIN");
        let clear = body.find("clear_app_lock").expect("clears");
        assert!(check < clear, "the PIN is checked before anything is cleared");
    }

    /// Guessing through another command does not reset the budget.
    #[test]
    fn five_wrong_guesses_lock_out_even_the_right_pin() {
        let state = AppLockState::default();
        let hash = cheap_hash("123456");
        for _ in 0..4 {
            assert_eq!(require_pin(&state, Some(&hash), Some("000000")), Err(PIN_WRONG.into()));
        }
        assert_eq!(require_pin(&state, Some(&hash), Some("000000")), Err(PIN_LOCKED_OUT.into()));
        assert_eq!(require_pin(&state, Some(&hash), Some("123456")), Err(PIN_LOCKED_OUT.into()));
    }

    #[test]
    fn family_safe_goes_on_freely_and_off_only_with_the_pin() {
        let state = AppLockState::default();
        let hash = cheap_hash("123456");
        let prove = |pin: Option<&'static str>| {
            let state = &state;
            let hash = hash.clone();
            move || require_pin(state, Some(&hash), pin)
        };

        // On, from off: nothing asked.
        assert_eq!(may_set_family_safe(false, true, || panic!("asked for a PIN to turn it on")), Ok(()));
        // Staying on, or staying off: nothing asked.
        assert_eq!(may_set_family_safe(true, true, || panic!("asked")), Ok(()));
        assert_eq!(may_set_family_safe(false, false, || panic!("asked")), Ok(()));
        // Off: the PIN, and the right one.
        assert_eq!(may_set_family_safe(true, false, prove(None)), Err(PIN_REQUIRED.into()));
        assert_eq!(may_set_family_safe(true, false, prove(Some("111111"))), Err(PIN_WRONG.into()));
        assert_eq!(may_set_family_safe(true, false, prove(Some("123456"))), Ok(()));
        // Off, with no PIN set at all: nothing to ask for.
        assert_eq!(may_set_family_safe(true, false, || require_pin(&state, None, None)), Ok(()));
    }

    #[test]
    fn a_reset_needs_the_folder_made_in_time() {
        let dir = tempfile::tempdir().unwrap();
        let now = 1_000;
        let challenge = new_challenge(dir.path(), now);
        assert!(challenge.name.starts_with("synabit-reset-"));

        assert_eq!(reset_proven(None, now), Err(RESET_NOT_STARTED.into()));
        assert_eq!(reset_proven(Some(&challenge), now), Err(RESET_NOT_PROVEN.into()));

        std::fs::create_dir(dir.path().join(&challenge.name)).unwrap();
        assert!(reset_proven(Some(&challenge), now + 60).is_ok());
        assert_eq!(reset_proven(Some(&challenge), now + RESET_VALID_SECS), Err(RESET_EXPIRED.into()));
    }

    #[test]
    fn a_folder_with_another_name_proves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let challenge = new_challenge(dir.path(), 0);
        std::fs::create_dir(dir.path().join("synabit-reset-000000x")).unwrap();
        assert_eq!(reset_proven(Some(&challenge), 1), Err(RESET_NOT_PROVEN.into()));
    }

    fn guards(active: bool, apps: &[&str], notes: &[&str], timeout: u64) -> AppLockConfig {
        AppLockConfig {
            is_enabled: true,
            app_lock_active: active,
            protected_apps: apps.iter().map(|s| s.to_string()).collect(),
            protected_notes: notes.iter().map(|s| s.to_string()).collect(),
            auto_lock_timeout_secs: timeout,
        }
    }

    fn change() -> AppLockConfigUpdate {
        AppLockConfigUpdate { protected_apps: None, protected_notes: None, auto_lock_timeout_secs: None, app_lock_active: None }
    }

    fn ids(v: &[&str]) -> Option<Vec<String>> {
        Some(v.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn loosening_is_told_apart_from_tightening() {
        let now = guards(true, &["safe", "journal"], &["n1"], 300);

        // Nothing asked for, or the same again: not loosening.
        assert!(!loosens(&now, &change()));
        assert!(!loosens(&now, &AppLockConfigUpdate { app_lock_active: Some(true), ..change() }));
        assert!(!loosens(&now, &AppLockConfigUpdate { auto_lock_timeout_secs: Some(300), ..change() }));

        // Tightening.
        assert!(!loosens(&now, &AppLockConfigUpdate { protected_apps: ids(&["safe", "journal", "feeds"]), ..change() }));
        assert!(!loosens(&now, &AppLockConfigUpdate { protected_notes: ids(&["n2", "n1"]), ..change() }));
        assert!(!loosens(&now, &AppLockConfigUpdate { auto_lock_timeout_secs: Some(60), ..change() }));
        assert!(!loosens(&guards(false, &[], &[], 300), &AppLockConfigUpdate { app_lock_active: Some(true), ..change() }));
        assert!(!loosens(&guards(true, &[], &[], 0), &AppLockConfigUpdate { auto_lock_timeout_secs: Some(1800), ..change() }));

        // Loosening.
        assert!(loosens(&now, &AppLockConfigUpdate { app_lock_active: Some(false), ..change() }));
        assert!(loosens(&now, &AppLockConfigUpdate { protected_apps: ids(&["safe"]), ..change() }));
        // Swapping one for another still drops one.
        assert!(loosens(&now, &AppLockConfigUpdate { protected_apps: ids(&["safe", "feeds"]), ..change() }));
        assert!(loosens(&now, &AppLockConfigUpdate { protected_notes: ids(&[]), ..change() }));
        assert!(loosens(&now, &AppLockConfigUpdate { auto_lock_timeout_secs: Some(900), ..change() }));
        assert!(loosens(&now, &AppLockConfigUpdate { auto_lock_timeout_secs: Some(0), ..change() }));
        // One loosening field is enough, whatever else tightens.
        assert!(loosens(&now, &AppLockConfigUpdate {
            protected_apps: ids(&["safe", "journal", "feeds"]),
            auto_lock_timeout_secs: Some(0),
            ..change()
        }));
    }

    #[test]
    fn update_app_lock_config_checks_the_pin_before_saving_a_loosening() {
        let source = include_str!("app_lock.rs");
        let body = source.split("pub fn update_app_lock_config(").nth(1).unwrap().split("\n}\n").next().unwrap();
        let classify = body.find("loosens(").expect("classifies the change");
        let check = body.find("require_pin(").expect("checks the PIN");
        let save = body.find("SecretManager::update_app_lock_config").expect("saves");
        assert!(classify < check && check < save, "the PIN is checked before anything is saved");
    }

    /// The command is `loosens` then `require_pin`: loosening needs the right
    /// PIN when one is set, tightening needs nothing.
    #[test]
    fn loosening_needs_the_pin_and_tightening_does_not() {
        let state = AppLockState::default();
        let hash = cheap_hash("123456");
        let now = guards(true, &["safe"], &[], 300);
        let gate = |update: &AppLockConfigUpdate, hash: Option<&str>, pin: Option<&str>| {
            if loosens(&now, update) { require_pin(&state, hash, pin) } else { Ok(()) }
        };
        let off = AppLockConfigUpdate { app_lock_active: Some(false), ..change() };
        let tighter = AppLockConfigUpdate { auto_lock_timeout_secs: Some(60), ..change() };

        assert_eq!(gate(&tighter, Some(&hash), None), Ok(()));
        assert_eq!(gate(&off, Some(&hash), None), Err(PIN_REQUIRED.into()));
        assert_eq!(gate(&off, Some(&hash), Some("111111")), Err(PIN_WRONG.into()));
        assert_eq!(gate(&off, Some(&hash), Some("123456")), Ok(()));
        // No PIN set: nothing to prove.
        assert_eq!(gate(&off, None, None), Ok(()));
    }

    #[test]
    fn pins_are_six_digits() {
        assert!(valid_pin("123456").is_ok());
        assert!(valid_pin("12345").is_err());
        assert!(valid_pin("12345a").is_err());
    }
}
