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

    pub fn load_secrets(app_handle: Option<&tauri::AppHandle>) -> AppSecrets {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let _ = app_handle; // unused on desktop
            if let Ok(entry) = Self::get_entry() {
                if let Ok(content) = entry.get_password() {
                    if let Ok(secrets) = serde_json::from_str::<AppSecrets>(&content) {
                        return secrets;
                    }
                }
            }
        }
        #[cfg(target_os = "ios")]
        {
            if let Some(handle) = app_handle {
                let path = Self::get_file_path(handle);
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(secrets) = serde_json::from_str::<AppSecrets>(&content) {
                        return secrets;
                    }
                }
            }
        }
        #[cfg(target_os = "android")]
        {
            if let Some(handle) = app_handle {
                match android_secure_store_get(ANDROID_SECRETS_KEY) {
                    Ok(content) if !content.is_empty() => {
                        if let Ok(secrets) = serde_json::from_str::<AppSecrets>(&content) {
                            return secrets;
                        }
                        log::error!(
                            "the Android keystore holds a secrets blob this build cannot parse; \
                             treating it as absent rather than overwriting it"
                        );
                    }
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
                                return secrets;
                            }
                        }
                    }
                    // Loud on purpose. The caller cannot tell "no key yet" from
                    // "the key is unreachable", and acting on the first when the
                    // second is true means minting a fresh vault key and losing
                    // the existing vault.
                    Err(e) => log::error!("could not read the Android keystore: {e}"),
                }
            }
        }
        AppSecrets::default()
    }

    pub fn save_secrets(
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
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_password = Some(pwd);
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn clear_e2ee_password(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_password = None;
        Self::save_secrets(app_handle, &secrets)
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
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_key = Some(base64::engine::general_purpose::STANDARD.encode(key));
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn clear_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.e2ee_key = None;
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn has_e2ee_key(app_handle: Option<&tauri::AppHandle>) -> bool {
        Self::get_e2ee_key(app_handle).is_some()
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
        let mut secrets = Self::load_secrets(app_handle);
        secrets.app_lock_hash = Some(hash);
        Self::save_secrets(app_handle, &secrets)
    }

    pub fn clear_app_lock(app_handle: Option<&tauri::AppHandle>) -> Result<(), String> {
        let mut secrets = Self::load_secrets(app_handle);
        secrets.app_lock_hash = None;
        secrets.protected_apps = None;
        secrets.protected_notes = None;
        secrets.auto_lock_timeout_secs = None;
        secrets.app_lock_active = None;
        Self::save_secrets(app_handle, &secrets)
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
        let mut secrets = Self::load_secrets(app_handle);
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
        Self::save_secrets(app_handle, &secrets)
    }
}
