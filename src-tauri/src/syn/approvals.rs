//! What this device agreed to, in one file.
//!
//! # Why one file
//!
//! Five kinds of agreement are kept per device, never synced — a permission
//! given on the laptop is not one given on the phone, and a file that synced
//! would let one device grant for another:
//!
//! | Section | What it holds | Kept by |
//! | --- | --- | --- |
//! | `consent` | Once / Always / Never, by capability | `syn::consent` |
//! | `servers` | MCP servers trusted, by fingerprint | `connector::config` |
//! | `read_only` | which of a server's tools were believed to only read | `connector::config` |
//! | `instances` | declared connectors approved, with their choices | `connector::instance` |
//! | `routines` | routines approved to run here | `syn::routine` |
//! | `skills` | skills allowed into the prompt here, as they were | `syn::skill` |
//! | `provider` | which AI provider, at which address, this device sends to | `syn::settings` |
//!
//! They were five files with five readers and five writers. One file is one
//! place to see what this device has agreed to, one place to forget it, and one
//! writer that every section goes through — so the next kind of agreement (a
//! job, a skill with a connection) is a section, not a sixth file.
//!
//! Each section keeps its own shape; the modules above still decide what an
//! agreement means. This decides only where it is written.
//!
//! # The rules it keeps
//!
//! - **Unreadable is empty.** A file that does not parse is read as having
//!   agreed to nothing, so everything is asked again. Read as "everything
//!   agreed", a corrupt file would be a silent grant.
//! - **Not synced.** It lives in `.synabit/`, which sync skips.
//! - **One writer at a time.** A write reads the file, changes its section and
//!   writes the whole file back, atomically, under one lock — two sections
//!   written at once must not lose each other.
//! - **Moved once.** A section not in the file yet is read from its old file,
//!   written here, and the old file put in `.synabit/migrated/`.

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};

/// The file.
pub const FILE: &str = "approvals.json";

/// One kind of agreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Consent,
    Servers,
    ReadOnly,
    Instances,
    Routines,
    Skills,
    Provider,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Section::Consent => "consent",
            Section::Servers => "servers",
            Section::ReadOnly => "read_only",
            Section::Instances => "instances",
            Section::Routines => "routines",
            Section::Skills => "skills",
            Section::Provider => "provider",
        }
    }

    /// Where it was kept before, newest name first.
    fn old_files(self) -> &'static [&'static str] {
        match self {
            Section::Consent => &["consent.json"],
            Section::Servers => &["connectors-trusted.json", "mcp-trusted.json"],
            Section::ReadOnly => &["connectors-read-only.json", "mcp-read-only.json"],
            Section::Instances => &["connector-instances.json"],
            Section::Routines => &["routines-approved.json"],
            Section::Skills | Section::Provider => &[],
        }
    }
}

static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn dir(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(".synabit")
}

fn read_file(vault_path: &str) -> Map<String, Value> {
    std::fs::read_to_string(dir(vault_path).join(FILE))
        .ok()
        .and_then(|c| match serde_json::from_str::<Value>(&c) {
            Ok(Value::Object(m)) => Some(m),
            Ok(_) | Err(_) => {
                log::warn!("[Syn] {FILE} is unreadable; this device is treated as having agreed to nothing");
                None
            }
        })
        .unwrap_or_default()
}

fn write_file(vault_path: &str, all: &Map<String, Value>) -> AppResult<()> {
    let d = dir(vault_path);
    std::fs::create_dir_all(&d).map_err(|e| AppError::General(format!("Failed to create .synabit: {e}")))?;
    let bytes = serde_json::to_string_pretty(all)?;
    crate::path_utils::write_atomic(&d.join(FILE), bytes.as_bytes())
        .map_err(|e| AppError::General(format!("Failed to write {FILE}: {e}")))
}

/// A section's old file, if one is there: its contents, and where it is.
fn old(vault_path: &str, section: Section) -> Option<(Value, PathBuf)> {
    section.old_files().iter().find_map(|name| {
        let p = dir(vault_path).join(name);
        let text = std::fs::read_to_string(&p).ok()?;
        // Unreadable is empty here too: moved as nothing, and asked again.
        Some((serde_json::from_str(&text).unwrap_or(Value::Null), p))
    })
}

/// Put the old files of a section away, so they are not read again.
fn retire(vault_path: &str, section: Section) {
    let to = dir(vault_path).join("migrated");
    for name in section.old_files() {
        let from = dir(vault_path).join(name);
        if from.exists() {
            let _ = std::fs::create_dir_all(&to);
            if let Err(e) = std::fs::rename(&from, to.join(name)) {
                log::warn!("[Syn] Could not put away {}: {e}", from.display());
            }
        }
    }
}

/// The section as it is — moved from its old file the first time — or its
/// default when there is none, or it does not read as `T`.
pub fn read<T: DeserializeOwned + Default>(vault_path: &str, section: Section) -> T {
    let _turn = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = read_file(vault_path);
    let value = match all.get(section.key()) {
        Some(v) => v.clone(),
        None => match old(vault_path, section) {
            Some((v, _)) => {
                all.insert(section.key().into(), v.clone());
                match write_file(vault_path, &all) {
                    Ok(()) => retire(vault_path, section),
                    Err(e) => log::warn!("[Syn] Could not move {} into {FILE}: {e}", section.key()),
                }
                v
            }
            None => return T::default(),
        },
    };
    serde_json::from_value(value).unwrap_or_else(|e| {
        log::warn!("[Syn] The {} section of {FILE} is unreadable; asking again: {e}", section.key());
        T::default()
    })
}

/// Whether this device has ever written the section — here, or in its old file.
pub fn has(vault_path: &str, section: Section) -> bool {
    let _turn = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    read_file(vault_path).contains_key(section.key()) || old(vault_path, section).is_some()
}

/// Write a section, leaving the others as they are.
pub fn write<T: Serialize>(vault_path: &str, section: Section, value: &T) -> AppResult<()> {
    let _turn = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = read_file(vault_path);
    // A section never read on this device still has an old file; writing over
    // it here must not leave that to be moved in later on top of this.
    if !all.contains_key(section.key()) {
        retire(vault_path, section);
    }
    all.insert(section.key().into(), serde_json::to_value(value)?);
    write_file(vault_path, &all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn vault() -> (tempfile::TempDir, String) {
        let d = tempfile::tempdir().expect("temp");
        let p = d.path().to_str().expect("utf8").to_string();
        (d, p)
    }

    #[test]
    fn sections_are_kept_apart_in_one_file() {
        let (d, v) = vault();
        let a: BTreeMap<String, String> = [("jira".to_string(), "f1".to_string())].into();
        let b: BTreeMap<String, bool> = [("r1".to_string(), true)].into();
        write(&v, Section::Servers, &a).unwrap();
        write(&v, Section::Routines, &b).unwrap();
        assert_eq!(read::<BTreeMap<String, String>>(&v, Section::Servers), a);
        assert_eq!(read::<BTreeMap<String, bool>>(&v, Section::Routines), b);
        assert!(d.path().join(".synabit").join(FILE).exists());
        assert!(read::<BTreeMap<String, String>>(&v, Section::Instances).is_empty());
    }

    /// The old file is read once, moved in, and put away — its newest name first.
    #[test]
    fn an_old_file_is_moved_in_once() {
        let (d, v) = vault();
        let s = d.path().join(".synabit");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("mcp-trusted.json"), r#"{"old":"f0"}"#).unwrap();
        std::fs::write(s.join("connectors-trusted.json"), r#"{"jira":"f1"}"#).unwrap();
        let got: BTreeMap<String, String> = read(&v, Section::Servers);
        assert_eq!(got.get("jira").map(String::as_str), Some("f1"));
        assert!(!s.join("connectors-trusted.json").exists() && !s.join("mcp-trusted.json").exists());
        assert!(s.join("migrated/connectors-trusted.json").exists());
        // And read from here after.
        assert_eq!(read::<BTreeMap<String, String>>(&v, Section::Servers), got);
    }

    /// Unreadable is empty, never everything.
    #[test]
    fn an_unreadable_file_has_agreed_to_nothing() {
        let (d, v) = vault();
        let s = d.path().join(".synabit");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join(FILE), "{ not json").unwrap();
        assert!(read::<BTreeMap<String, String>>(&v, Section::Consent).is_empty());
        std::fs::write(s.join(FILE), r#"{"consent": 7}"#).unwrap();
        assert!(read::<BTreeMap<String, String>>(&v, Section::Consent).is_empty());
    }

    /// Writing a section never read still puts its old file away, so the old
    /// file cannot come back over what was written.
    #[test]
    fn writing_first_puts_the_old_file_away() {
        let (d, v) = vault();
        let s = d.path().join(".synabit");
        std::fs::create_dir_all(&s).unwrap();
        std::fs::write(s.join("routines-approved.json"), r#"{"old":true}"#).unwrap();
        let fresh: BTreeMap<String, bool> = [("new".to_string(), true)].into();
        write(&v, Section::Routines, &fresh).unwrap();
        assert_eq!(read::<BTreeMap<String, bool>>(&v, Section::Routines), fresh);
    }

    /// Two sections written from two threads at once both survive.
    #[test]
    fn writers_do_not_lose_each_other() {
        let (_d, v) = vault();
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let v = v.clone();
                std::thread::spawn(move || {
                    let section = if i % 2 == 0 { Section::Servers } else { Section::Routines };
                    let mut m: BTreeMap<String, String> = read(&v, section);
                    m.insert(format!("k{i}"), "x".into());
                    write(&v, section, &m).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert!(!read::<BTreeMap<String, String>>(&v, Section::Servers).is_empty());
        assert!(!read::<BTreeMap<String, String>>(&v, Section::Routines).is_empty());
    }
}
