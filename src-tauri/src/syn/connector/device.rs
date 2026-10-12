//! `device`: data on this device, through a fixed list of bridges.
//!
//! `docs/syn-connectors-2026-10-08.md` §3.1.1. The most personal data often
//! sits behind no web API at all — a statement in Downloads, a bank's
//! notification, last night's sleep. Each bridge is the app's own code with a
//! fixed schema; a declaration only *chooses* one, filters it by fields the
//! system wrote, and picks columns. Syn never writes a bridge.
//!
//! What a bridge returns is somebody else's words — a bank's CSV, a message
//! someone sent — and is treated as a connector's answer is: it taints, and
//! its text columns are free text.
//!
//! # `folder.watch` (desktop)
//!
//! Files directly in one folder the person chose on the approval card — not
//! below it, not through a link out of it — newest first, each decoded by the
//! operation's decoder, one row per record with `_file`, `_row` and `_id`
//! (`file#row`, a key for a mirror). The model never names the folder; it may
//! narrow by `since_days` and `file`.

use serde_json::{json, Value};

use super::http::{Failed, Fetched};
use super::instance::Instance;
use super::manifest::{Decoder, Manifest, Operation};

/// Every bridge, where it runs, what it takes and what it can be filtered by.
struct Bridge {
    name: &'static str,
    /// `desktop`, `android`, `mobile`.
    runs_on: &'static str,
    params: &'static [&'static str],
    filters: &'static [&'static str],
}

const BRIDGES: &[Bridge] = &[
    Bridge { name: "folder.watch", runs_on: "desktop", params: &["since_days", "file"], filters: &["extensions"] },
    Bridge { name: "android.notifications", runs_on: "android", params: &["since_days"], filters: &["package"] },
];

/// Where the Android side and this one hand data over: Kotlin's `filesDir`.
static DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Set from Tauri's `app_data_dir`. On Android that is the app's data root
/// (where `timeline.db` lives), and Kotlin's `context.filesDir` is `files/`
/// inside it — seen on the emulator, 2026-10-11 — so the handoff is there.
pub fn know_data_dir(dir: Option<std::path::PathBuf>) {
    if let Some(d) = dir {
        let d = if cfg!(target_os = "android") { d.join("files") } else { d };
        let _ = DATA_DIR.set(d);
    }
}

/// Written by `SynNotificationListener`, one JSON line per notification kept.
pub const NOTIFICATION_STORE: &str = "syn-notifications.jsonl";
/// Read by it: the packages whose notifications are kept.
pub const NOTIFICATION_PACKAGES: &str = "syn-notification-packages.json";

/// The packages every approved `android.notifications` connector on this
/// device filters for — the only notifications the listener keeps.
pub fn packages_approved(vault_path: &str, declared: &[Manifest]) -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    for m in declared {
        if super::instance::approved_here(vault_path, m).is_none() {
            continue;
        }
        for op in m.operations.iter().filter(|o| o.bridge.as_deref() == Some("android.notifications")) {
            for p in op.filter.get("package").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                out.insert(p.to_string());
            }
        }
    }
    out.into_iter().collect()
}

/// Tell the listener which packages to keep. Called whenever an approval changes.
pub fn sync_packages(packages: &[String]) {
    let Some(dir) = DATA_DIR.get() else { return };
    if let Err(e) = std::fs::create_dir_all(dir).and_then(|_| std::fs::write(dir.join(NOTIFICATION_PACKAGES), serde_json::to_vec(packages).unwrap_or_default())) {
        log::error!("[Syn] Could not tell the notification listener what to keep: {e}");
    }
}

fn bridge(name: &str) -> Option<&'static Bridge> {
    BRIDGES.iter().find(|b| b.name == name)
}

pub fn known(name: &str) -> bool {
    bridge(name).is_some()
}

/// The bridges a declaration reads.
pub fn bridges_of(m: &Manifest) -> Vec<&str> {
    let mut out: Vec<&str> = m.operations.iter().filter_map(|o| o.bridge.as_deref()).collect();
    out.sort();
    out.dedup();
    out
}

pub fn names() -> Vec<&'static str> {
    BRIDGES.iter().map(|b| b.name).collect()
}

pub fn takes(name: &str, param: &str) -> bool {
    bridge(name).is_some_and(|b| b.params.contains(&param))
}

pub fn filters(name: &str, key: &str) -> bool {
    bridge(name).is_some_and(|b| b.filters.contains(&key))
}

/// Whether a bridge can run on this build.
pub fn here(name: &str) -> bool {
    match bridge(name).map(|b| b.runs_on) {
        Some("desktop") => cfg!(desktop),
        Some("android") => cfg!(target_os = "android"),
        Some("mobile") => cfg!(mobile),
        _ => false,
    }
}

/// Whether a declaration's bridges need a folder chosen on the card.
pub fn needs_folder(m: &Manifest) -> bool {
    m.operations.iter().any(|o| o.bridge.as_deref() == Some("folder.watch"))
}

/// The most files one read takes, and the most bytes of each.
const MAX_FILES: usize = 50;
const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;

fn failed(said: impl Into<String>) -> Failed {
    Failed { said: said.into(), answered: false, code: Some("unreachable") }
}

/// Carry out a device operation.
pub async fn run(m: &Manifest, inst: &Instance, op: &Operation, args: &Value) -> Result<Fetched, Failed> {
    let name = op.bridge.as_deref().unwrap_or_default();
    if !here(name) {
        return Err(failed(format!("`{name}` does not run on this device.")));
    }
    let records = match name {
        "folder.watch" => folder_watch(m, inst, op, args)?,
        "android.notifications" => {
            let dir = DATA_DIR.get().ok_or_else(|| failed("this device's data folder is not known yet"))?;
            notifications(&dir.join(NOTIFICATION_STORE), op, args)
        }
        _ => return Err(failed(format!("there is no bridge `{name}`"))),
    };
    let (columns, rows, missing) = super::pick::table(&records.0, &op.result);
    let mut missing = missing;
    missing.extend(records.1);
    Ok(Fetched { columns, rows, missing, asked: records.2 })
}

/// The notifications kept for the packages `op` filters for, newest first.
///
/// The package is the system's field; the title and text are whoever sent the
/// notification, and are free text. On Android 15 and later a notification
/// the system judges to hold a one-time code reaches a listener redacted.
pub fn notifications(store: &std::path::Path, op: &Operation, args: &Value) -> (Value, Vec<String>, String) {
    let packages: Vec<&str> = op.filter.get("package").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
    let since_days = args.get("since_days").and_then(Value::as_u64).unwrap_or(31);
    let cutoff = chrono::Utc::now().timestamp_millis() - (since_days as i64) * 86_400_000;
    let text = std::fs::read_to_string(store).unwrap_or_default();
    let mut seen = std::collections::BTreeSet::new();
    let mut records: Vec<Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|n| n.get("package").and_then(Value::as_str).is_some_and(|p| packages.contains(&p)))
        .filter(|n| n.get("posted_at").and_then(Value::as_i64).is_some_and(|t| t >= cutoff))
        .filter(|n| n.get("id").and_then(Value::as_str).is_some_and(|id| seen.insert(id.to_string())))
        .map(|n| {
            let at = n.get("posted_at").and_then(Value::as_i64).and_then(chrono::DateTime::from_timestamp_millis).map(|t| t.to_rfc3339());
            json!({ "_id": n["id"], "package": n["package"], "title": n["title"], "text": n["text"], "posted_at": at })
        })
        .collect();
    records.sort_by_key(|r| std::cmp::Reverse(r["posted_at"].as_str().unwrap_or_default().to_string()));
    let asked = format!("android.notifications {} (last {since_days} days)", packages.join(","));
    (Value::Array(records), Vec::new(), asked)
}

/// Whether Synabit has notification access, and opening the screen that gives
/// it. Through `SynDeviceBridge` (Kotlin), by JNI, as `secrets` reaches
/// `SecureStore`.
#[cfg(target_os = "android")]
pub fn android_call(method: &str) -> Result<bool, String> {
    use jni::objects::JValue;
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| format!("the Android JVM is unavailable: {e}"))?;
    let mut env = vm.attach_current_thread().map_err(|e| format!("could not attach to the Android JVM: {e}"))?;
    let context = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    let outcome = (|| -> Result<bool, String> {
        let loader = env
            .call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
            .and_then(|v| v.l())
            .map_err(|e| format!("could not reach the class loader: {e}"))?;
        let name = env.new_string("com.synabit.app.SynDeviceBridge").map_err(|e| e.to_string())?;
        let class = env
            .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])
            .and_then(|v| v.l())
            .map_err(|e| format!("SynDeviceBridge is missing from this build (a keep rule holds it): {e}"))?;
        let class = jni::objects::JClass::from(class);
        env.call_static_method(&class, method, "(Landroid/content/Context;)Z", &[JValue::Object(&context)])
            .and_then(|v| v.z())
            .map_err(|e| format!("SynDeviceBridge.{method} is not callable: {e}"))
    })();
    if outcome.is_err() {
        let _ = env.exception_clear();
    }
    outcome
}

#[cfg(not(target_os = "android"))]
pub fn android_call(_method: &str) -> Result<bool, String> {
    Ok(false)
}

/// Records from the files of the chosen folder, the files skipped, and what was asked.
fn folder_watch(m: &Manifest, inst: &Instance, op: &Operation, args: &Value) -> Result<(Value, Vec<String>, String), Failed> {
    let folder = inst.folder.as_deref().ok_or_else(|| failed(format!("{} has no folder chosen on this device; choose one on its approval card.", m.name)))?;
    let dir = std::path::Path::new(folder);
    let extensions: Vec<String> = op
        .filter
        .get("extensions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(|e| e.trim_start_matches('.').to_ascii_lowercase()).collect())
        .unwrap_or_else(|| vec!["csv".into()]);
    let since_days = args.get("since_days").and_then(Value::as_u64).unwrap_or(31);
    let only = args.get("file").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(since_days * 86_400);

    let entries = std::fs::read_dir(dir).map_err(|e| failed(format!("the folder could not be read: {e}")))?;
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf, String)> = Vec::new();
    for e in entries.flatten() {
        // A link could lead out of the folder the person chose.
        let Ok(meta) = std::fs::symlink_metadata(e.path()) else { continue };
        if !meta.file_type().is_file() {
            continue;
        }
        let file_name = e.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') || only.is_some_and(|o| o != file_name) {
            continue;
        }
        let ext = e.path().extension().and_then(|x| x.to_str()).unwrap_or_default().to_ascii_lowercase();
        if !extensions.contains(&ext) {
            continue;
        }
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        if only.is_none() && modified < cutoff {
            continue;
        }
        files.push((modified, e.path(), file_name));
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    let mut missing = Vec::new();
    if files.len() > MAX_FILES {
        missing.push(format!("{} files; the newest {MAX_FILES} were read", files.len()));
        files.truncate(MAX_FILES);
    }

    let decoder = op.decoder.unwrap_or(match m.decoder {
        Decoder::Json => Decoder::Csv,
        other => other,
    });
    let mut records = Vec::new();
    for (modified, path, file_name) in &files {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if size > MAX_FILE_BYTES {
            missing.push(format!("{file_name} is {} MB, more than one read takes", size / 1024 / 1024));
            continue;
        }
        let bytes = std::fs::read(path).map_err(|e| failed(format!("{file_name} could not be read: {e}")))?;
        let decoded = match super::decode::decode(decoder, &bytes) {
            Ok(v) => v,
            Err(e) => {
                missing.push(format!("{file_name}: {e}"));
                continue;
            }
        };
        let modified: chrono::DateTime<chrono::Utc> = (*modified).into();
        let items = match decoded {
            Value::Array(a) => a,
            one => vec![one],
        };
        for (i, item) in items.into_iter().enumerate() {
            let mut o = match item {
                Value::Object(o) => o,
                other => {
                    let mut o = serde_json::Map::new();
                    o.insert("value".into(), other);
                    o
                }
            };
            o.insert("_file".into(), json!(file_name));
            o.insert("_row".into(), json!(i + 1));
            o.insert("_id".into(), json!(format!("{file_name}#{}", i + 1)));
            o.insert("_modified".into(), json!(modified.to_rfc3339()));
            records.push(Value::Object(o));
        }
    }
    let asked = format!("folder.watch {} ({} file(s), last {since_days} days)", extensions.join(","), files.len());
    Ok((Value::Array(records), missing, asked))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the chosen packages, newest first, each once, within the window —
    /// a balance change from the bank, not the chat beside it.
    #[test]
    fn only_the_chosen_apps_notifications_are_read() {
        let dir = tempfile::tempdir().expect("temp");
        let store = dir.path().join(NOTIFICATION_STORE);
        let now = chrono::Utc::now().timestamp_millis();
        let line = |id: &str, pkg: &str, text: &str, ago_days: i64| {
            json!({ "id": id, "package": pkg, "title": "", "text": text, "posted_at": now - ago_days * 86_400_000 }).to_string()
        };
        std::fs::write(
            &store,
            [
                line("a@1", "com.VCB", "TK 0071 -55,000VND luc 01-10 HIGHLANDS", 1),
                line("b@2", "com.zing.zalo", "Mai đi nhậu không?", 1),
                line("a@1", "com.VCB", "TK 0071 -55,000VND luc 01-10 HIGHLANDS", 1),
                line("c@3", "com.VCB", "TK 0071 +25,000,000VND LUONG", 2),
                line("d@4", "com.VCB", "old", 90),
                "not json".to_string(),
            ]
            .join("\n"),
        )
        .expect("store");
        let m = crate::syn::connector::manifest::parse_yaml(
            "id: vcb\nname: VCB\ntransport: device\noperations:\n  - name: balance\n    description: Balance changes.\n    effect: read\n    bridge: android.notifications\n    filter: { package: [com.VCB] }\n",
        )
        .expect("reads");
        assert_eq!(crate::syn::connector::manifest::problems(&m), vec![]);
        let (records, _, asked) = notifications(&store, &m.operations[0], &json!({}));
        let texts: Vec<&str> = records.as_array().expect("list").iter().filter_map(|r| r["text"].as_str()).collect();
        assert_eq!(texts, ["TK 0071 -55,000VND luc 01-10 HIGHLANDS", "TK 0071 +25,000,000VND LUONG"], "newest first, each once");
        assert!(!texts.iter().any(|t| t.contains("nhậu") || *t == "old"), "{texts:?}");
        assert!(asked.contains("com.VCB"));
    }
}
