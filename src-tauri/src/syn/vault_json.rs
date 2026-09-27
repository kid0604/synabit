//! Writing Syn's own JSON files in a way the sync layer can live with.
//!
//! Two things the sync layer does to every JSON object in the vault, and that
//! Syn's writers used to undo on every save:
//!
//! * **It stamps `metadata.node_id`** into the file the first time it publishes
//!   it (`sync::core::identity`). Syn's files are serialised from structs that
//!   do not know that field, so each save dropped it, the next sync wrote it
//!   back, and every save became two writes and two pushes — and a save landing
//!   between the sync's read and its write could be lost.
//! * **It resolves a conflict on `metadata.updated_at`**, last writer wins
//!   (`sync::core::apply`). A file without the field compares as `""`, so the
//!   remote copy won every time, whichever was newer.
//!
//! [`write`] fixes both for any object it is handed: it carries the existing
//! file's `metadata` over and stamps `updated_at`. Arrays are written as they
//! are — the sync layer identifies them by path and has nowhere to put either
//! field. Files that need more than last-writer-wins (`routines.json`,
//! `proposals.json`, `declined.json`) are merged item by item in
//! `sync::core::merge`; the stamp does no harm there.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// The stamp: RFC 3339 in UTC with fixed millisecond precision, so two stamps
/// compare correctly as strings — which is how the sync layer compares them.
pub fn now_stamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Serialise `value`, keep the file's existing `metadata`, stamp
/// `metadata.updated_at`, and write through a temp file and a rename.
pub fn write<T: Serialize + ?Sized>(path: &Path, value: &T) -> AppResult<()> {
    let existing = std::fs::read_to_string(path).ok();
    let rendered = render(existing.as_deref(), serde_json::to_value(value)?, &now_stamp())?;
    atomic_write(path, &rendered)
}

/// The pure half of [`write`].
fn render(existing: Option<&str>, mut value: Value, stamp: &str) -> AppResult<String> {
    if let Value::Object(object) = &mut value {
        let mut metadata = existing
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .and_then(|mut old| old.get_mut("metadata").map(Value::take))
            .and_then(|m| match m {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default();
        // Whatever the value itself says about its metadata wins over the file.
        if let Some(Value::Object(own)) = object.remove("metadata") {
            metadata.extend(own);
        }
        metadata.insert("updated_at".to_string(), Value::String(stamp.to_string()));
        object.insert("metadata".to_string(), Value::Object(metadata));
    }
    Ok(serde_json::to_string_pretty(&value)?)
}

fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::General(format!("Failed to write {}: {e}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Thing {
        name: String,
    }

    /// The sync layer's stamp survives a save, so the next sync has nothing to
    /// write back and the file is pushed once, not twice.
    #[test]
    fn a_node_id_the_sync_layer_wrote_survives_a_save() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("Syn").join("thing.json");
        write(&path, &Thing { name: "a".into() }).expect("first");

        // What `identity::get_or_assign_node_id` does to the file.
        let mut stamped: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        stamped["metadata"]["node_id"] = Value::String("n-1".into());
        stamped["metadata"]["other"] = Value::from(7);
        std::fs::write(&path, stamped.to_string()).expect("stamp");

        write(&path, &Thing { name: "b".into() }).expect("second");
        let back: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(back["metadata"]["node_id"], "n-1");
        assert_eq!(back["metadata"]["other"], 7, "unknown metadata is kept too");
        assert_eq!(back["name"], "b");
        assert!(back["metadata"]["updated_at"].as_str().is_some_and(|s| !s.is_empty()));

        // And the struct still reads it.
        let thing: Thing = serde_json::from_value(back).expect("reads");
        assert_eq!(thing, Thing { name: "b".into() });
    }

    /// Same content written twice renders the same apart from the stamp, so a
    /// round trip through the struct is stable.
    #[test]
    fn a_round_trip_is_stable() {
        let first = render(None, serde_json::json!({"name": "a"}), "2026-09-27T00:00:00.000Z").expect("r");
        let reread: Thing = serde_json::from_str(&first).expect("reads");
        let second =
            render(Some(&first), serde_json::to_value(&reread).expect("v"), "2026-09-27T00:00:00.000Z").expect("r");
        assert_eq!(first, second);
    }

    #[test]
    fn stamps_compare_as_strings_in_time_order() {
        let a = now_stamp();
        std::thread::sleep(std::time::Duration::from_millis(3));
        let b = now_stamp();
        assert!(b > a, "{a} then {b}");
        assert!(a.ends_with('Z') && a.len() == "2026-09-27T00:00:00.000Z".len());
    }

    #[test]
    fn arrays_are_written_as_they_are() {
        let out = render(Some(r#"{"metadata":{"node_id":"x"}}"#), serde_json::json!([1, 2]), "t").expect("r");
        assert_eq!(serde_json::from_str::<Value>(&out).expect("json"), serde_json::json!([1, 2]));
    }
}
