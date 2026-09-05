//! A record of everything Syn did that reached past the vault.
//!
//! # Why this is not the run transcript
//!
//! Every run already keeps a transcript, and a capability that asks is written
//! there too. But a transcript answers "what happened in this conversation",
//! and the question somebody has when they are worried is a different one:
//! *what has this thing done on my behalf, ever?* That question cannot be
//! answered by opening runs one at a time, and a person who has to do that will
//! not do it.
//!
//! So: one file, every entry, newest first, readable by a person.
//!
//! # What is not in it
//!
//! Reading and writing the vault. Those never ask, because trash and version
//! history put every one of them back, and a log that recorded them would bury
//! the four or five entries a year that actually matter under thousands that do
//! not. An audit log nobody can scan is a log nobody reads.
//!
//! # Where it lives
//!
//! `{vault}/.synabit/audit.json`, beside the consent ledger and for the same
//! reason: it is a record of what happened *on this device*. Syncing it would
//! merge two machines' histories into one list where neither is true.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::syn::consent::{Capability, Decision};

/// How many entries are kept.
///
/// Generous, because this is the file somebody opens after something went
/// wrong and they are trying to work out when it started. Losing the beginning
/// of that story is the one loss this file cannot afford.
pub const KEEP_ENTRIES: usize = 2_000;

/// What happened to one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Allowed without asking, because the user had already said so.
    Allowed,
    /// The user was asked and the run stopped to wait.
    Asked,
    /// Refused without asking, because the user had said never.
    Refused,
    /// It ran.
    Done,
    /// It was tried and failed.
    Failed,
}

/// One line in the log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub at: String,
    pub run_id: String,
    pub tool: String,
    /// The capability, as `describe` puts it — a sentence rather than a tag, so
    /// the log reads without a key to decode it.
    pub about: String,
    pub outcome: Outcome,
    /// What it would take to undo, when there is anything to undo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reversal: Option<String>,
}

fn path(vault_path: &str) -> AppResult<std::path::PathBuf> {
    let dir = std::path::Path::new(vault_path).join(".synabit");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::General(format!("Failed to create .synabit: {e}")))?;
    Ok(dir.join("audit.json"))
}

/// Everything recorded, newest first. Unreadable means empty.
pub fn read(vault_path: &str) -> Vec<Entry> {
    let Ok(path) = path(vault_path) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        log::warn!("[Syn] Audit log is unreadable: {e}");
        Vec::new()
    })
}

/// Whether this capability is one the log is for.
///
/// The vault arms are not. See the module note: a log that recorded every note
/// edit would bury the entries that matter.
pub fn worth_recording(capability: &Capability) -> bool {
    !matches!(
        capability,
        Capability::VaultRead | Capability::VaultWrite | Capability::VaultStructural
    )
}

/// Write one line.
///
/// Best effort by design, and the caller is expected to ignore the error. An
/// audit log that could refuse an action would be a second, worse consent
/// mechanism — and one that failed closed would mean a full disk stops Syn from
/// answering. Failing to record is bad; failing to work because recording
/// failed is worse.
pub fn record(
    vault_path: &str,
    run_id: &str,
    tool: &str,
    capability: &Capability,
    outcome: Outcome,
) -> AppResult<()> {
    if !worth_recording(capability) {
        return Ok(());
    }

    let reversal = match crate::syn::registry::reversal_of(capability) {
        crate::syn::registry::Reversal::Nothing => None,
        crate::syn::registry::Reversal::Automatic { how }
        | crate::syn::registry::Reversal::Manual { how } => Some(how),
        crate::syn::registry::Reversal::Irreversible => {
            Some("nothing in this app undoes it".to_string())
        }
    };

    let mut entries = read(vault_path);
    entries.insert(
        0,
        Entry {
            at: chrono::Utc::now().to_rfc3339(),
            run_id: run_id.to_string(),
            tool: tool.to_string(),
            about: capability.describe(),
            outcome,
            reversal,
        },
    );
    entries.truncate(KEEP_ENTRIES);

    let path = path(vault_path)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&entries)?)?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::General(format!("Failed to write the audit log: {e}"))
    })
}

/// Record and swallow, for the call sites that must not fail because of it.
pub fn record_best_effort(
    vault_path: &str,
    run_id: &str,
    tool: &str,
    capability: &Capability,
    outcome: Outcome,
) {
    if let Err(e) = record(vault_path, run_id, tool, capability, outcome) {
        log::warn!("[Syn] Could not write to the audit log: {e}");
    }
}

/// What a decision means for the log.
pub fn outcome_of(decision: &Decision) -> Outcome {
    match decision {
        Decision::Allow => Outcome::Allowed,
        Decision::Ask => Outcome::Asked,
        Decision::Refuse => Outcome::Refused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("temp vault");
        let path = dir.path().to_str().expect("utf8").to_string();
        (dir, path)
    }

    fn sending() -> Capability {
        Capability::NetWrite {
            domain: "example.test".into(),
            tool: "send_test".into(),
        }
    }

    /// The vault is not audited, and that is a decision rather than an omission.
    ///
    /// A log with every note edit in it buries the four or five entries a year
    /// that matter under thousands that do not, and a log nobody can scan is a
    /// log nobody reads.
    #[test]
    fn editing_a_note_is_not_an_audit_entry() {
        let (_dir, vault) = vault();
        for capability in [
            Capability::VaultRead,
            Capability::VaultWrite,
            Capability::VaultStructural,
        ] {
            assert!(!worth_recording(&capability));
            record(&vault, "run-1", "create_node", &capability, Outcome::Done).expect("no-op");
        }
        assert!(read(&vault).is_empty());
    }

    /// Everything that reaches outside is recorded, newest first.
    #[test]
    fn what_leaves_the_vault_is_written_down_newest_first() {
        let (_dir, vault) = vault();

        record(&vault, "run-1", "send_test", &sending(), Outcome::Asked).expect("written");
        record(&vault, "run-1", "send_test", &sending(), Outcome::Done).expect("written");

        let entries = read(&vault);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].outcome, Outcome::Done, "newest first");
        assert_eq!(entries[1].outcome, Outcome::Asked);
        assert_eq!(entries[0].run_id, "run-1");
        assert!(
            entries[0].about.contains("example.test"),
            "the line reads without a key to decode it: {:?}",
            entries[0].about
        );
    }

    /// The log says what it would take to undo, when anything would.
    ///
    /// This is the field somebody reads at the worst moment, so it must not
    /// claim more than is true: what has been sent is at somebody else's
    /// server, and the entry says so instead of naming a button.
    #[test]
    fn an_entry_says_what_undoing_it_would_take() {
        let (_dir, vault) = vault();
        record(&vault, "run-1", "send_test", &sending(), Outcome::Done).expect("written");
        let how = read(&vault)[0].reversal.clone().expect("something to say");
        assert!(how.contains("example.test"), "{how}");

        record(&vault, "run-2", "run_code", &Capability::Execute, Outcome::Done).expect("written");
        assert!(read(&vault)[0]
            .reversal
            .as_deref()
            .is_some_and(|how| how.contains("nothing in this app undoes it")));
    }

    /// A refusal is recorded too.
    ///
    /// "Syn tried to do this and was stopped" is exactly what somebody
    /// auditing wants to see, and a log that only recorded successes would
    /// answer the easy half of the question.
    #[test]
    fn being_stopped_is_also_worth_recording() {
        let (_dir, vault) = vault();
        record(&vault, "run-1", "send_test", &sending(), Outcome::Refused).expect("written");
        assert_eq!(read(&vault)[0].outcome, Outcome::Refused);
    }

    /// It does not grow without limit.
    #[test]
    fn the_log_stops_growing() {
        let (_dir, vault) = vault();
        let mut entries: Vec<Entry> = (0..KEEP_ENTRIES + 20)
            .map(|i| Entry {
                at: format!("2026-09-05T00:00:{:02}Z", i % 60),
                run_id: format!("run-{i}"),
                tool: "send_test".into(),
                about: "send".into(),
                outcome: Outcome::Done,
                reversal: None,
            })
            .collect();
        entries.truncate(KEEP_ENTRIES + 20);
        let p = path(&vault).expect("a path");
        std::fs::write(&p, serde_json::to_string(&entries).expect("json")).expect("written");

        record(&vault, "run-new", "send_test", &sending(), Outcome::Done).expect("written");
        assert_eq!(read(&vault).len(), KEEP_ENTRIES);
        assert_eq!(read(&vault)[0].run_id, "run-new", "the newest survives");
    }

    /// It lives where the consent ledger lives, and for the same reason.
    #[test]
    fn the_log_is_somewhere_that_does_not_sync() {
        let (_dir, vault) = vault();
        record(&vault, "run-1", "send_test", &sending(), Outcome::Done).expect("written");

        let p = path(&vault).expect("a path");
        let relative = p.strip_prefix(&vault).expect("inside").to_str().expect("utf8");
        assert!(crate::commands::nodes::is_in_unscanned_dir(relative));
        assert!(
            relative.split(['/', '\\']).any(|part| part.starts_with('.')),
            "two machines' histories must not merge into one list where neither is true"
        );
    }
}
