use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The single key every secret is stored under on Android.
#[cfg(target_os = "android")]
const ANDROID_SECRETS_KEY: &str = "app_secrets";

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct AppSecrets {
    pub e2ee_password: Option<String>, // KEEP for migration
    #[serde(default)]
    pub e2ee_key: Option<String>, // NEW: base64-encoded 32-byte key
    /// Google Drive's OAuth tokens and sync config, kept as fields so that an
    /// existing secrets blob still deserializes after Drive was removed. Never
    /// read, never written; they fall away the next time secrets are saved.
    #[serde(default, skip_serializing)]
    pub global_sync_config: Option<String>,
    #[serde(default, skip_serializing)]
    pub vault_tokens: HashMap<String, String>,
    #[serde(default)]
    pub app_lock_hash: Option<String>, // Argon2id PHC hash string
    #[serde(default)]
    pub protected_apps: Option<Vec<String>>, // ["finance", "people"]
    #[serde(default)]
    pub protected_notes: Option<Vec<String>>, // ["Notes/diary.md"]
    #[serde(default)]
    pub auto_lock_timeout_secs: Option<u64>, // Default 300
    #[serde(default)]
    pub app_lock_active: Option<bool>, // Tier 1 toggle (independent of PIN)
    /// API keys for Syn's chat providers, keyed by `SynProvider::key_slot()`.
    ///
    /// Here rather than in `SynSettings` because that struct is serialised to
    /// `{vault}/Syn/settings.json` — a file inside the vault, which syncs
    /// between devices, is readable in any editor, and on a vault kept in git
    /// gets committed. A key belongs to one machine's keychain.
    ///
    /// A map rather than one field so that switching provider does not
    /// silently discard the key for the one being left.
    #[serde(default)]
    pub syn_api_keys: HashMap<String, String>,
    /// Whether Syn's family-safe answers are on, on this device.
    ///
    /// The authoritative copy. It used to be only `SynSettings::family_safe`
    /// in `{vault}/Syn/settings.json`, where anybody who can edit a file in the
    /// vault — or any device it syncs to — could switch it off with nothing
    /// asking for the PIN. Here it sits beside `app_lock_hash`, and
    /// `commands::app_lock::set_family_safe` will not turn it off without that
    /// PIN when one is set.
    ///
    /// `None` is "never decided on this device": the vault's old flag is read
    /// once and, if it was on, carried here. See `syn::family_safe::resolve`.
    ///
    /// Not a lock against the machine's owner: anybody who can open this OS
    /// account's keychain (or, on a phone, the app's storage) can change it,
    /// exactly as they can the PIN hash next to it.
    #[serde(default)]
    pub family_safe: Option<bool>,
}

/// Read one value out of the Android keystore-backed store.
///
/// Every step is fallible and none of them may panic. The Java side is reached
/// by name through JNI, so a build that renamed or removed `SecureStore` — R8
/// does exactly that unless a keep rule holds it — fails here rather than at
/// some later point that looks unrelated. Panicking would take the app down on
/// the startup path, since reading the E2EE key is one of the first things the
/// frontend asks for.
///
/// A failed JNI call leaves an exception pending on the thread, which poisons
/// every later call made from it, including Tauri's own. It is cleared before
/// returning.
#[cfg(target_os = "android")]
fn android_secure_store_get(key: &str) -> Result<String, String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|e| format!("the Android JVM is unavailable: {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("could not attach to the Android JVM: {e}"))?;
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let outcome = android_secure_store_get_inner(&mut env, &context, key);
    if outcome.is_err() {
        let _ = env.exception_clear();
    }
    outcome
}

#[cfg(target_os = "android")]
fn android_secure_store_get_inner(
    env: &mut jni::JNIEnv,
    context: &jni::objects::JObject,
    key: &str,
) -> Result<String, String> {
    use jni::objects::JValue;

    let jclass = android_secure_store_class(env, context)?;
    let jkey = env
        .new_string(key)
        .map_err(|e| format!("could not allocate the key string: {e}"))?;

    let value = env
        .call_static_method(
            &jclass,
            "getSecret",
            "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(context), JValue::Object(&jkey)],
        )
        .and_then(|v| v.l())
        .map_err(|e| format!("SecureStore.getSecret is not callable: {e}"))?;

    let value = jni::objects::JString::from(value);
    let value: String = env
        .get_string(&value)
        .map_err(|e| format!("could not read what SecureStore returned: {e}"))?
        .into();
    Ok(value)
}

/// Write one value into the Android keystore-backed store. See
/// [`android_secure_store_get`] for why nothing here is allowed to panic.
#[cfg(target_os = "android")]
fn android_secure_store_put(key: &str, value: &str) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
        .map_err(|e| format!("the Android JVM is unavailable: {e}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("could not attach to the Android JVM: {e}"))?;
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };

    let outcome = android_secure_store_put_inner(&mut env, &context, key, value);
    if outcome.is_err() {
        let _ = env.exception_clear();
    }
    outcome
}

#[cfg(target_os = "android")]
fn android_secure_store_put_inner(
    env: &mut jni::JNIEnv,
    context: &jni::objects::JObject,
    key: &str,
    value: &str,
) -> Result<(), String> {
    use jni::objects::JValue;

    let jclass = android_secure_store_class(env, context)?;
    let jkey = env
        .new_string(key)
        .map_err(|e| format!("could not allocate the key string: {e}"))?;
    let jvalue = env
        .new_string(value)
        .map_err(|e| format!("could not allocate the value string: {e}"))?;

    let stored = env
        .call_static_method(
            &jclass,
            "saveSecret",
            "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
            &[
                JValue::Object(context),
                JValue::Object(&jkey),
                JValue::Object(&jvalue),
            ],
        )
        .and_then(|v| v.z())
        .map_err(|e| format!("SecureStore.saveSecret is not callable: {e}"))?;

    if stored {
        Ok(())
    } else {
        Err("the Android keystore refused the write".to_string())
    }
}

/// Resolve `com.synabit.app.SecureStore` through the app's own class loader.
///
/// The system class loader cannot see application classes from a thread the JVM
/// did not start, which is every thread Rust attaches, so the loader is taken
/// from the activity context instead.
#[cfg(target_os = "android")]
fn android_secure_store_class<'local>(
    env: &mut jni::JNIEnv<'local>,
    context: &jni::objects::JObject,
) -> Result<jni::objects::JClass<'local>, String> {
    use jni::objects::JValue;

    let class_loader = env
        .call_method(context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
        .and_then(|v| v.l())
        .map_err(|e| format!("could not reach the Android class loader: {e}"))?;

    let class_name = env
        .new_string("com.synabit.app.SecureStore")
        .map_err(|e| format!("could not allocate the class name: {e}"))?;

    let class = env
        .call_method(
            &class_loader,
            "loadClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&class_name)],
        )
        .and_then(|v| v.l())
        .map_err(|e| {
            format!(
                "SecureStore is missing from this build — the most likely cause is R8 \
                 removing it, which a keep rule in proguard-rules.pro prevents: {e}"
            )
        })?;

    Ok(jni::objects::JClass::from(class))
}

pub struct SecretManager;

impl SecretManager {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn get_entry() -> Result<keyring::Entry, String> {
        keyring::Entry::new("synabit", "secrets").map_err(|e| format!("Keyring error: {}", e))
    }

    #[cfg(any(target_os = "android", target_os = "ios"))]
    fn get_file_path(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
        use tauri::Manager;
        let mut path = app_handle.path().app_data_dir().unwrap_or_default();
        path.push("synabit_secrets.json");
        path
    }

    /// Every stored secret, or the defaults when there are none.
    ///
    /// For reading only. A store that could not be read comes back as the
    /// defaults, which is the right answer for "is there an API key" and the
    /// wrong one for "what should I write back" — anything that changes a
    /// secret goes through [`Self::update_secrets`], which refuses to write
    /// over a store it could not read.
    pub fn load_secrets(app_handle: Option<&tauri::AppHandle>) -> AppSecrets {
        Self::try_load_secrets(app_handle).unwrap_or_else(|e| {
            log::error!("could not read the stored secrets, treating them as absent: {e}");
            AppSecrets::default()
        })
    }

    /// Every stored secret, telling "nothing stored yet" apart from "could not
    /// read what is stored".
    ///
    /// The difference is the whole point. `Ok(default)` means there is nothing
    /// to lose; `Err` means there is something and it was not reached — a
    /// keychain dialog the user dismissed, a blob a newer build wrote, a
    /// keystore that did not answer. Writing back after the second would
    /// replace the E2EE key and the app-lock PIN with a blob that holds only
    /// the one field being set.
    fn try_load_secrets(app_handle: Option<&tauri::AppHandle>) -> Result<AppSecrets, String> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = app_handle; // unused on desktop
            let entry = Self::get_entry()?;
            match entry.get_password() {
                Ok(content) => serde_json::from_str::<AppSecrets>(&content)
                    .map_err(|e| format!("the keychain holds a secrets blob this build cannot parse: {e}")),
                Err(keyring::Error::NoEntry) => Ok(AppSecrets::default()),
                Err(e) => Err(format!("Keyring error: {e}")),
            }
        }
        #[cfg(target_os = "ios")]
        {
            let Some(handle) = app_handle else {
                return Ok(AppSecrets::default());
            };
            match std::fs::read_to_string(Self::get_file_path(handle)) {
                Ok(content) => serde_json::from_str::<AppSecrets>(&content)
                    .map_err(|e| format!("the secrets file cannot be parsed: {e}")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppSecrets::default()),
                Err(e) => Err(format!("FS error: {e}")),
            }
        }
        #[cfg(target_os = "android")]
        {
            let Some(handle) = app_handle else {
                return Ok(AppSecrets::default());
            };
            match android_secure_store_get(ANDROID_SECRETS_KEY) {
                // Never "treat it as absent" here: the caller may be about to
                // write, and absent is what it would write over.
                Ok(content) if !content.is_empty() => serde_json::from_str::<AppSecrets>(&content)
                    .map_err(|e| format!("the Android keystore holds a secrets blob this build cannot parse: {e}")),
                Ok(_) => {
                    // Nothing stored yet. An install that predates the keystore
                    // left its secrets in a plain file next door; carry those
                    // across once and remove the file.
                    let path = Self::get_file_path(handle);
                    if let Ok(old_content) = std::fs::read_to_string(&path) {
                        if let Ok(secrets) = serde_json::from_str::<AppSecrets>(&old_content) {
                            match android_secure_store_put(ANDROID_SECRETS_KEY, &old_content) {
                                Ok(()) => {
                                    let _ = std::fs::remove_file(&path);
                                }
                                // The file stays where it is, so the next
                                // launch tries the move again.
                                Err(e) => log::error!(
                                    "could not move the stored secrets into the Android \
                                     keystore, leaving them in place: {e}"
                                ),
                            }
                            return Ok(secrets);
                        }
                    }
                    Ok(AppSecrets::default())
                }
                // Loud on purpose. The caller cannot tell "no key yet" from
                // "the key is unreachable", and acting on the first when the
                // second is true means minting a fresh vault key and losing
                // the existing vault.
                Err(e) => Err(format!("could not read the Android keystore: {e}")),
            }
        }
    }

    /// Change the stored secrets: read them, let `change` edit them, write
    /// them back — as one step.
    ///
    /// Every secret lives in one blob, so every setter is a read-modify-write
    /// of the whole of it. Two of those running at once — a Telegram token and
    /// a provider key saved from two settings screens, a connector secret
    /// written while the app lock is reconfigured — each read the blob before
    /// the other wrote, and whichever wrote second silently discarded the
    /// first. [`read_modify_write`] holds one lock across the whole cycle.
    ///
    /// It also refuses to write when the read failed. See
    /// [`Self::try_load_secrets`] for what writing would have destroyed.
    pub fn update_secrets(
        app_handle: Option<&tauri::AppHandle>,
        change: impl FnOnce(&mut AppSecrets),
    ) -> Result<(), String> {
        read_modify_write(
            || Self::try_load_secrets(app_handle),
            |secrets| Self::save_secrets(app_handle, secrets),
            change,
        )
    }

    fn save_secrets(
        app_handle: Option<&tauri::AppHandle>,
        secrets: &AppSecrets,
    ) -> Result<(), String> {
        let content = serde_json::to_string(secrets).map_err(|e| e.to_string())?;

        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = app_handle;
            let entry = Self::get_entry()?;
            entry
                .set_password(&content)
                .map_err(|e| format!("Keyring error: {}", e))
        }
        #[cfg(target_os = "ios")]
        {
            if let Some(handle) = app_handle {
                let path = Self::get_file_path(handle);
                if let Some(p) = path.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                std::fs::write(path, content).map_err(|e| format!("FS error: {}", e))
            } else {
                Err("AppHandle is required on mobile to save secrets".to_string())
            }
        }
        #[cfg(target_os = "android")]
        {
            if app_handle.is_some() {
                android_secure_store_put(ANDROID_SECRETS_KEY, &content)
            } else {
                Err("AppHandle is required on mobile to save secrets".to_string())
            }
        }
    }

    // ──────────────────────────────────────────────
    // E2EE
    // ──────────────────────────────────────────────
    pub fn get_e2ee_password(app_handle: Option<&tauri::AppHandle>) -> Option<String> {
        Self::load_secrets(app_handle).e2ee_password
    }

    pub fn set_e2ee_password(
        app_handle: Option<&tauri::AppHandle>,
        pwd: String,
    ) -> Result<(), String> {
        Self::update_secrets(app_handle, |secrets| secrets.e2ee_password = Some(pwd))
    }

    pub fn clear_e2ee_password(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        Self::update_secrets(app_handle, |secrets| secrets.e2ee_password = None)
    }

    // ──────────────────────────────────────────────
    // E2EE Auto Key (new passwordless system)
    // ──────────────────────────────────────────────
    pub fn get_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> Option<[u8; 32]> {
        let secrets = Self::load_secrets(app_handle);
        secrets.e2ee_key.as_ref().and_then(|b64| {
            use base64::Engine;
            use zeroize::Zeroize;
            let mut bytes = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                bytes.zeroize();
                Some(key)
            } else {
                bytes.zeroize();
                None
            }
        })
    }

    pub fn set_e2ee_key(
        app_handle: Option<&tauri::AppHandle>,
        key: &[u8; 32],
    ) -> Result<(), String> {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(key);
        Self::update_secrets(app_handle, |secrets| secrets.e2ee_key = Some(encoded))
    }

    pub fn clear_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        Self::update_secrets(app_handle, |secrets| secrets.e2ee_key = None)
    }

    pub fn has_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> bool {
        Self::get_e2ee_key(app_handle).is_some()
    }

    // ──────────────────────────────────────────────
    // Syn provider API keys
    // ──────────────────────────────────────────────

    /// The stored key for a provider slot, or `None` if there is not one.
    ///
    /// Blank is the same as absent: a settings field that was opened and left
    /// empty must not become an `Authorization: Bearer ` header.
    pub fn get_syn_api_key(app_handle: Option<&tauri::AppHandle>, slot: &str) -> Option<String> {
        Self::load_secrets(app_handle)
            .syn_api_keys
            .get(slot)
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
    }

    /// Store a key, or remove it when the value is blank.
    ///
    /// Clearing the field in the UI is how a user revokes a key, so an empty
    /// string has to delete rather than store nothing.
    pub fn set_syn_api_key(
        app_handle: Option<&tauri::AppHandle>,
        slot: &str,
        key: &str,
    ) -> Result<(), String> {
        let key = key.trim();
        Self::update_secrets(app_handle, |secrets| {
            if key.is_empty() {
                secrets.syn_api_keys.remove(slot);
            } else {
                secrets.syn_api_keys.insert(slot.to_string(), key.to_string());
            }
        })
    }

    pub fn has_syn_api_key(app_handle: Option<&tauri::AppHandle>, slot: &str) -> bool {
        Self::get_syn_api_key(app_handle, slot).is_some()
    }







    // ──────────────────────────────────────────────
    // App Lock
    // ──────────────────────────────────────────────
    pub fn get_app_lock_hash(app_handle: Option<&tauri::AppHandle>) -> Option<String> {
        Self::load_secrets(app_handle).app_lock_hash
    }

    pub fn set_app_lock_hash(
        app_handle: Option<&tauri::AppHandle>,
        hash: String,
    ) -> Result<(), String> {
        Self::update_secrets(app_handle, |secrets| secrets.app_lock_hash = Some(hash))
    }

    pub fn clear_app_lock(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        Self::update_secrets(app_handle, |secrets| {
            secrets.app_lock_hash = None;
            secrets.protected_apps = None;
            secrets.protected_notes = None;
            secrets.auto_lock_timeout_secs = None;
            secrets.app_lock_active = None;
        })
    }

    /// This device's family-safe flag: `Ok(None)` when it was never decided
    /// here, `Err` when the store could not be read at all — which the caller
    /// must not mistake for "off".
    pub fn try_family_safe(app_handle: Option<&tauri::AppHandle>) -> Result<Option<bool>, String> {
        Self::try_load_secrets(app_handle).map(|secrets| secrets.family_safe)
    }

    /// Record this device's family-safe flag. Callers decide whether they may;
    /// see `commands::app_lock::set_family_safe`.
    pub fn set_family_safe(app_handle: Option<&tauri::AppHandle>, on: bool) -> Result<(), String> {
        Self::update_secrets(app_handle, |secrets| secrets.family_safe = Some(on))
    }

    pub fn get_app_lock_config(
        app_handle: Option<&tauri::AppHandle>,
    ) -> (
        Option<Vec<String>>,
        Option<Vec<String>>,
        Option<u64>,
        Option<bool>,
    ) {
        let secrets = Self::load_secrets(app_handle);
        (
            secrets.protected_apps,
            secrets.protected_notes,
            secrets.auto_lock_timeout_secs,
            secrets.app_lock_active,
        )
    }

    pub fn update_app_lock_config(
        app_handle: Option<&tauri::AppHandle>,
        protected_apps: Option<Vec<String>>,
        protected_notes: Option<Vec<String>>,
        timeout: Option<u64>,
        app_lock_active: Option<bool>,
    ) -> Result<(), String> {
        Self::update_secrets(app_handle, |secrets| {
            if let Some(apps) = protected_apps {
                secrets.protected_apps = Some(apps);
            }
            if let Some(notes) = protected_notes {
                secrets.protected_notes = Some(notes);
            }
            if let Some(t) = timeout {
                secrets.auto_lock_timeout_secs = Some(t);
            }
            if let Some(active) = app_lock_active {
                secrets.app_lock_active = Some(active);
            }
        })
    }
}

/// Secrets that live in a keychain entry of their own, beside the shared blob.
///
/// Safe's Secret Key is the first. It stays out of `AppSecrets` for three
/// reasons: it must not share the blob's read-modify-write with every other
/// setter; a Secret Key per Safe is a key per vault, which is a name, not a
/// field; and later (section 6.7 of `docs/safe-2026-09-28.md`) it is the entry
/// that wants access-control flags the shared blob does not.
impl SecretManager {
    /// `Ok(None)` when nothing is stored under `name`; `Err` when something may
    /// be and could not be read.
    pub fn get_named(app_handle: Option<&tauri::AppHandle>, name: &str) -> Result<Option<String>, String> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = app_handle;
            let entry = keyring::Entry::new("synabit", name).map_err(|e| format!("Keyring error: {e}"))?;
            match entry.get_password() {
                Ok(value) => Ok(Some(value)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("Keyring error: {e}")),
            }
        }
        #[cfg(target_os = "ios")]
        {
            let Some(handle) = app_handle else { return Err("AppHandle is required on mobile".into()) };
            match std::fs::read_to_string(Self::named_file(handle, name)) {
                Ok(value) => Ok(Some(value)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("FS error: {e}")),
            }
        }
        #[cfg(target_os = "android")]
        {
            if app_handle.is_none() {
                return Err("AppHandle is required on mobile".into());
            }
            android_secure_store_get(name).map(|v| (!v.is_empty()).then_some(v))
        }
    }

    pub fn set_named(app_handle: Option<&tauri::AppHandle>, name: &str, value: &str) -> Result<(), String> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = app_handle;
            keyring::Entry::new("synabit", name)
                .and_then(|entry| entry.set_password(value))
                .map_err(|e| format!("Keyring error: {e}"))
        }
        #[cfg(target_os = "ios")]
        {
            let Some(handle) = app_handle else { return Err("AppHandle is required on mobile".into()) };
            let path = Self::named_file(handle, name);
            if let Some(p) = path.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            std::fs::write(path, value).map_err(|e| format!("FS error: {e}"))
        }
        #[cfg(target_os = "android")]
        {
            if app_handle.is_none() {
                return Err("AppHandle is required on mobile".into());
            }
            android_secure_store_put(name, value)
        }
    }

    #[cfg(target_os = "ios")]
    fn named_file(app_handle: &tauri::AppHandle, name: &str) -> std::path::PathBuf {
        use tauri::Manager;
        let mut path = app_handle.path().app_data_dir().unwrap_or_default();
        path.push(format!("{name}.secret"));
        path
    }
}

/// Held for the whole of every read-modify-write of the secrets blob.
///
/// Process-wide rather than per `AppHandle` because the blob is: there is one
/// keychain entry, one keystore slot, one file.
static SECRETS_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Read, change, write — under [`SECRETS_WRITE`], and not at all when the read
/// failed.
///
/// Separate from [`SecretManager::update_secrets`] so the tests can hand it a
/// store in memory: they never touch the real keychain.
fn read_modify_write(
    load: impl FnOnce() -> Result<AppSecrets, String>,
    save: impl FnOnce(&AppSecrets) -> Result<(), String>,
    change: impl FnOnce(&mut AppSecrets),
) -> Result<(), String> {
    // A panic while holding the lock leaves nothing half-written — the blob is
    // written in one call or not at all — so a poisoned lock is still a
    // perfectly good lock.
    let _held = SECRETS_WRITE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut secrets = load()?;
    change(&mut secrets);
    save(&secrets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier, Mutex};

    /// A blob in memory, serialised exactly as the keychain holds it, with a
    /// pause between reading and returning so that two unguarded writers
    /// reliably overlap.
    struct Store(Mutex<String>);

    impl Store {
        fn load(&self) -> Result<AppSecrets, String> {
            let content = self.0.lock().unwrap().clone();
            std::thread::sleep(std::time::Duration::from_millis(5));
            if content.is_empty() {
                return Ok(AppSecrets::default());
            }
            serde_json::from_str(&content).map_err(|e| e.to_string())
        }

        fn save(&self, secrets: &AppSecrets) -> Result<(), String> {
            *self.0.lock().unwrap() = serde_json::to_string(secrets).unwrap();
            Ok(())
        }

        fn read(&self) -> AppSecrets {
            serde_json::from_str(&self.0.lock().unwrap()).unwrap()
        }
    }

    /// Sixteen settings screens saving sixteen different keys at once, and
    /// every one of them is still there afterwards.
    ///
    /// Without the lock, each writer reads the blob during the others' pause
    /// and writes back its own copy with one key added: the survivors are
    /// whichever wrote last.
    #[test]
    fn concurrent_writers_do_not_lose_each_others_secrets() {
        let store = Arc::new(Store(Mutex::new(String::new())));
        let start = Arc::new(Barrier::new(16));

        let writers: Vec<_> = (0..16)
            .map(|i| {
                let store = Arc::clone(&store);
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    read_modify_write(
                        || store.load(),
                        |s| store.save(s),
                        |s| {
                            s.syn_api_keys.insert(format!("slot-{i}"), format!("key-{i}"));
                        },
                    )
                    .unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }

        let keys = store.read().syn_api_keys;
        assert_eq!(keys.len(), 16, "lost writes: only {:?} survived", keys.keys().collect::<Vec<_>>());
    }

    /// A store that could not be read is never written over.
    ///
    /// This is the E2EE key surviving a macOS keychain dialog the user
    /// dismissed: before, the dismissed read came back as "nothing stored",
    /// and saving a provider key then wrote a blob holding only that key.
    #[test]
    fn a_failed_read_writes_nothing() {
        let saved = std::cell::Cell::new(false);
        let changed = std::cell::Cell::new(false);

        let outcome = read_modify_write(
            || Err("the user dismissed the keychain dialog".to_string()),
            |_| {
                saved.set(true);
                Ok(())
            },
            |_| changed.set(true),
        );

        assert!(outcome.is_err());
        assert!(!changed.get(), "the change ran against secrets that were never read");
        assert!(!saved.get(), "a blob was written over a store that could not be read");
    }

    /// And the ordinary case still does what it says.
    #[test]
    fn a_successful_read_is_changed_and_written_back() {
        let store = Store(Mutex::new(
            serde_json::to_string(&AppSecrets {
                e2ee_key: Some("existing".into()),
                ..Default::default()
            })
            .unwrap(),
        ));

        read_modify_write(
            || store.load(),
            |s| store.save(s),
            |s| {
                s.syn_api_keys.insert("anthropic".into(), "sk-ant-test".into());
            },
        )
        .unwrap();

        let after = store.read();
        assert_eq!(after.e2ee_key.as_deref(), Some("existing"), "an unrelated secret was lost");
        assert_eq!(after.syn_api_keys.get("anthropic").map(String::as_str), Some("sk-ant-test"));
    }
}
