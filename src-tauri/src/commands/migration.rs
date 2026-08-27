//! The write path a data migration uses, and the flag that stops it running twice.
//!
//! # Why this is not `write_node_file`
//!
//! Every ordinary write runs `crdt_apply_safe`, which lands a row in
//! `sync_crdt_updates` — the queue every other device drains. That is
//! exactly right when a person edited a note. It is exactly wrong when the
//! app is repairing its own storage format, for two reasons.
//!
//! The visible one is volume: a migration that touches five thousand caps
//! queues five thousand deltas in a few seconds, and every paired device
//! replays all of them.
//!
//! The one that actually loses data is subtler. A migration runs on the
//! client, so *each device runs its own copy* — there is no coordinator
//! handing down a single result. And `write_node_file` stamps
//! `updated_at` with `Utc::now()`, so two devices repairing the same file
//! produce two different byte sequences. The CRDT then merges two
//! independent rewrites of one document, and merging two rewrites is how
//! you get a tag listed twice or two sentences interleaved.
//!
//! # The way out
//!
//! A migration that is *deterministic* needs no synchronisation at all.
//! If `f(old bytes) → new bytes` yields the same answer everywhere, each
//! device can repair its own copy in silence and they all converge on
//! identical files without exchanging a byte.
//!
//! That converts the problem into three rules, which this module exists to
//! enforce:
//!
//! 1. Do not go through `write_node_file`. Write the bytes, refresh the
//!    row, tell the sync layer nothing.
//! 2. Keep the transform pure. No clock, no fresh identifiers, no
//!    dependence on the order files happen to be visited. In particular
//!    `created_at` and `updated_at` are carried across untouched — they
//!    are what `parse_file_to_node` reads back, so preserving them is also
//!    what keeps the user's list in the order they left it.
//! 3. Record a flag when it finishes, so it runs once per device.
//!
//! The transform itself lives on the caller's side. This module only
//! guarantees that whatever it produces is written without waking sync.

use serde::{Deserialize, Serialize};

use crate::db::DbState;
use crate::error::{logged, AppResult};
use crate::path_utils;
use crate::utils::node_parser::parse_file_to_node;

/// Keys are namespaced so a migration flag can never collide with the
/// device identity, peer addresses or anything else living in `kv_store`.
const FLAG_PREFIX: &str = "migration:";

/// One file's repaired contents, already computed by the caller.
///
/// `content` is the whole file — frontmatter and body — because a
/// migration that rewrites frontmatter has to be able to say so, and
/// because comparing whole files is what makes re-running free.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilentWrite {
    pub rel_path: String,
    pub content: String,
}

/// What a migration pass did, so the log can say it rather than imply it.
#[derive(Debug, Default, Serialize)]
pub struct MigrationReport {
    /// Files whose bytes differed and were rewritten.
    pub changed: usize,
    /// Files already in the target shape. A second run reports every file
    /// here and writes nothing, which is the definition of idempotent.
    pub unchanged: usize,
    /// Files that could not be written. Their paths are logged; the vault
    /// is intact, those files are simply still in the old shape.
    pub failed: usize,
}

/// Write repaired files without telling the sync layer anything happened.
///
/// Deliberately absent, each for a reason:
///
/// - `crdt_apply_safe` — the whole point; see the module docs.
/// - `get_or_assign_node_id` — minting an identifier is not deterministic,
///   so a cap that has no `node_id` keeps not having one. The next real
///   edit assigns it, under sync, exactly as it would have anyway.
/// - `upsert_document_path` — no path moves here.
///
/// Present, because a repair the user cannot see has not landed: the
/// `nodes` row and the search entry are both refreshed. That second one
/// matters more than it looks — the FTS `tags` column is populated from
/// `properties`, so a migration that fills in tags without reindexing
/// leaves them unsearchable.
#[tauri::command]
pub fn apply_silent_migration(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    writes: Vec<SilentWrite>,
) -> AppResult<MigrationReport> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    Ok(apply_writes(&db, &vault_path, &writes))
}

/// The pass itself, over a plain connection.
///
/// Split from the command so the tests exercise *this* — the code that
/// actually runs — rather than a paraphrase of it standing in for a
/// `tauri::State` they cannot build. A test that mirrors the logic it is
/// checking will happily stay green while someone reintroduces the one
/// call this whole module exists to avoid.
pub(crate) fn apply_writes(
    db: &crate::db::DbBridge,
    vault_path: &str,
    writes: &[SilentWrite],
) -> MigrationReport {
    let mut report = MigrationReport::default();

    for write in writes {
        let abs_path = match path_utils::resolve_safe_path(vault_path, &write.rel_path) {
            Ok(path) => path,
            Err(e) => {
                log::error!("migration: refusing path '{}': {}", write.rel_path, e);
                report.failed += 1;
                continue;
            }
        };

        // A file already in the target shape is left completely alone —
        // not rewritten with identical bytes, not restatted, not touched.
        // This is what makes a second run free and a partial run safe to
        // resume.
        match std::fs::read_to_string(&abs_path) {
            Ok(current) if current == write.content => {
                report.unchanged += 1;
                continue;
            }
            Ok(_) => {}
            Err(e) => {
                log::error!("migration: cannot read '{}': {}", write.rel_path, e);
                report.failed += 1;
                continue;
            }
        }

        if let Err(e) = std::fs::write(&abs_path, &write.content) {
            log::error!("migration: cannot write '{}': {}", write.rel_path, e);
            report.failed += 1;
            continue;
        }

        report.changed += 1;

        let Some(node) = parse_file_to_node(vault_path, &abs_path) else {
            log::error!(
                "migration: wrote '{}' but could not parse it back; its row is now stale",
                write.rel_path
            );
            continue;
        };

        logged("migrate node", &node.id, db.upsert_node(&node));

        let tags = node
            .properties
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let status = node.properties.get("status").and_then(|s| s.as_str());
        let props_str = serde_json::to_string(&node.properties).unwrap_or_default();

        db.upsert_search_entry(
            &node.id,
            &node.node_type,
            &node.title,
            &tags,
            &node.content,
            &props_str,
            status,
            &node.updated_at,
            &node.id,
        );
    }

    log::info!(
        "migration: {} rewritten, {} already current, {} failed",
        report.changed,
        report.unchanged,
        report.failed
    );

    report
}

/// Repair every quickcap in the vault: tags into frontmatter, colour into a
/// name, the leaked `<!--color:…-->` comment out of the body.
///
/// The transform is `utils::quickcap_storage::migrate_cap`, which is a pure
/// function of a file's bytes. Everything that makes running it safe on a
/// synced vault is enforced by `apply_writes` above.
///
/// Caps whose bytes are already correct never reach the writer at all —
/// `migrate_cap` returns `None` for them — so on every launch after the
/// first this walks the list and writes nothing.
#[tauri::command]
pub fn migrate_quickcap_storage(
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<MigrationReport> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let caps = db.get_nodes_by_type("quickcap")?;

    let mut writes = Vec::new();
    for cap in &caps {
        // A node's id is its path relative to the vault root.
        let Ok(abs_path) = path_utils::resolve_safe_path(&vault_path, &cap.id) else {
            log::warn!("migration: skipping unreadable path '{}'", cap.id);
            continue;
        };
        let Ok(current) = std::fs::read_to_string(&abs_path) else {
            continue;
        };
        if let Some(repaired) = crate::utils::quickcap_storage::migrate_cap(&current) {
            writes.push(SilentWrite {
                rel_path: cap.id.clone(),
                content: repaired,
            });
        }
    }

    log::info!(
        "migration: {} of {} caps need repair",
        writes.len(),
        caps.len()
    );
    Ok(apply_writes(&db, &vault_path, &writes))
}

/// Repair the Finance vault: the legacy category list, the missing system
/// categories, the flat budget list, transactions that name their account by
/// name rather than by identifier, and every amount that is still stored in
/// whole units rather than minor ones.
///
/// The transforms are `utils::finance_storage`, which are pure functions of a
/// file's bytes. This walks the config first because the month files need the
/// account list out of it — a list the config migration never touches, so
/// reading it before or after the repair gives the same answer.
///
/// A file already in the target shape never reaches the writer: every
/// transform returns `None` for it. On the second launch this walks the list
/// and writes nothing.
#[tauri::command]
pub fn migrate_finance_storage(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    scales: std::collections::HashMap<String, u32>,
    default_scale: u32,
) -> AppResult<MigrationReport> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // The interface owns this table; see `Scales`. Passing it in is what keeps
    // the two sides from multiplying by different powers of ten. The vault's
    // own currency is read out of the config below, because this runs before
    // Finance has loaded anything.
    let mut scales = crate::utils::finance_storage::Scales {
        table: scales,
        default_scale,
        vault_currency: "USD".to_string(),
    };

    let read = |rel_path: &str| -> Option<String> {
        let abs_path = path_utils::resolve_safe_path(&vault_path, rel_path).ok()?;
        std::fs::read_to_string(abs_path).ok()
    };

    let mut writes = Vec::new();
    let mut accounts = Vec::new();

    for config in db.get_nodes_by_type("finance_config")? {
        let Some(contents) = read(&config.id) else {
            log::warn!("migration: skipping unreadable finance config '{}'", config.id);
            continue;
        };
        scales.vault_currency = crate::utils::finance_storage::currency_in(&contents);
        accounts = crate::utils::finance_storage::accounts_in(&contents);
        if let Some(repaired) = crate::utils::finance_storage::migrate_config(&contents, &scales) {
            writes.push(SilentWrite {
                rel_path: config.id.clone(),
                content: repaired,
            });
        }
    }

    let months = db.get_nodes_by_type("finance_month")?;
    for month in &months {
        let Some(contents) = read(&month.id) else {
            log::warn!("migration: skipping unreadable finance month '{}'", month.id);
            continue;
        };
        if let Some(repaired) = crate::utils::finance_storage::migrate_month(&contents, &accounts, &scales) {
            writes.push(SilentWrite {
                rel_path: month.id.clone(),
                content: repaired,
            });
        }
    }

    for ledger in db.get_nodes_by_type("finance_debts")? {
        let Some(contents) = read(&ledger.id) else {
            log::warn!("migration: skipping unreadable finance debts '{}'", ledger.id);
            continue;
        };
        if let Some(repaired) = crate::utils::finance_storage::migrate_debts(&contents, &scales) {
            writes.push(SilentWrite {
                rel_path: ledger.id.clone(),
                content: repaired,
            });
        }
    }

    log::info!(
        "migration: {} finance files need repair, of {} months plus the config and debts",
        writes.len(),
        months.len()
    );
    Ok(apply_writes(&db, &vault_path, &writes))
}

/// What this device has already migrated, if anything.
#[tauri::command]
pub fn get_migration_flag(
    state: tauri::State<'_, DbState>,
    key: String,
) -> AppResult<Option<String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_kv(&format!("{FLAG_PREFIX}{key}"))
}

/// Record that a migration finished. Called only after a clean pass, so an
/// interrupted run is retried on the next launch rather than skipped.
#[tauri::command]
pub fn set_migration_flag(
    state: tauri::State<'_, DbState>,
    key: String,
    value: String,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.set_kv(&format!("{FLAG_PREFIX}{key}"), &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;
    use std::path::Path;

    /// A cap as it sits on disk, frontmatter and all.
    fn cap(node_id: &str, tags: &str, body: &str) -> String {
        format!(
            "---\nnode_id: {node_id}\ntitle: \"a cap\"\ntype: \"quickcap\"\ntags: {tags}\n\
             created_at: \"2026-01-01 08:00:00\"\nupdated_at: \"2026-02-02 09:30:00\"\n---\n{body}"
        )
    }

    fn deltas(db: &DbBridge) -> i64 {
        db.conn()
            .query_row("SELECT COUNT(*) FROM sync_crdt_updates", [], |r| r.get(0))
            .unwrap()
    }

    fn vault(dir: &tempfile::TempDir) -> String {
        dir.path().to_string_lossy().to_string()
    }

    fn write_cap(vault: &Path, rel: &str, contents: &str) {
        let path = vault.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// The property the whole design rests on. If a migration queues sync
    /// traffic, every paired device replays the repair — and, because each
    /// device runs its own copy, merges it against their own.
    #[test]
    fn writes_nothing_to_the_sync_queue() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        write_cap(
            dir.path(),
            "QuickCaps/a.md",
            &cap("id-a", "[]", "họp #dự-án\n"),
        );

        let before = deltas(&db);
        let report = apply_writes(
            &db,
            &vault(&dir),
            &[SilentWrite {
                rel_path: "QuickCaps/a.md".into(),
                content: cap("id-a", "[\"dự-án\"]", "họp #dự-án\n"),
            }],
        );

        assert_eq!(report.changed, 1);
        assert_eq!(
            deltas(&db),
            before,
            "a migration must not queue sync traffic"
        );
    }

    /// Two devices repair the same vault independently. They exchange
    /// nothing, so the only thing that can make them agree is the transform
    /// being a function of the bytes alone.
    #[test]
    fn two_devices_reach_identical_bytes() {
        let original = cap("id-a", "[]", "họp #dự-án\n");
        let repaired = cap("id-a", "[\"dự-án\"]", "họp #dự-án\n");

        let mut outputs = Vec::new();
        for _ in 0..2 {
            let dir = tempfile::tempdir().unwrap();
            let db = DbBridge::new_in_memory_full().unwrap();
            write_cap(dir.path(), "QuickCaps/a.md", &original);
            apply_writes(
                &db,
                &vault(&dir),
                &[SilentWrite {
                    rel_path: "QuickCaps/a.md".into(),
                    content: repaired.clone(),
                }],
            );
            outputs.push(std::fs::read_to_string(dir.path().join("QuickCaps/a.md")).unwrap());
        }

        assert_eq!(
            outputs[0], outputs[1],
            "two devices must converge byte for byte"
        );
    }

    /// Re-running has to be free, because an interrupted pass is retried on
    /// the next launch and a flag can always be lost.
    #[test]
    fn a_second_pass_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        let repaired = cap("id-a", "[\"dự-án\"]", "họp #dự-án\n");
        write_cap(
            dir.path(),
            "QuickCaps/a.md",
            &cap("id-a", "[]", "họp #dự-án\n"),
        );

        let write = || SilentWrite {
            rel_path: "QuickCaps/a.md".into(),
            content: repaired.clone(),
        };

        let first = apply_writes(&db, &vault(&dir), &[write()]);
        let second = apply_writes(&db, &vault(&dir), &[write()]);

        assert_eq!((first.changed, first.unchanged), (1, 0));
        assert_eq!(
            (second.changed, second.unchanged),
            (0, 1),
            "second pass must be a no-op"
        );
    }

    /// QuickCap sorts by `updated_at`. A repair that restamps it would hand
    /// the user back a list in an order they never chose.
    #[test]
    fn leaves_the_users_ordering_alone() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        write_cap(
            dir.path(),
            "QuickCaps/a.md",
            &cap("id-a", "[]", "họp #dự-án\n"),
        );

        apply_writes(
            &db,
            &vault(&dir),
            &[SilentWrite {
                rel_path: "QuickCaps/a.md".into(),
                content: cap("id-a", "[\"dự-án\"]", "họp #dự-án\n"),
            }],
        );

        let node = db.get_nodes_by_type("quickcap").unwrap().remove(0);
        assert_eq!(node.created_at, "2026-01-01 08:00:00");
        assert_eq!(node.updated_at, "2026-02-02 09:30:00");
    }

    /// The repair has to be visible to the app, or the user sees the old
    /// shape until something else happens to touch the file.
    #[test]
    fn refreshes_the_row_the_ui_reads() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        write_cap(
            dir.path(),
            "QuickCaps/a.md",
            &cap("id-a", "[]", "họp #dự-án\n"),
        );

        apply_writes(
            &db,
            &vault(&dir),
            &[SilentWrite {
                rel_path: "QuickCaps/a.md".into(),
                content: cap("id-a", "[\"dự-án\"]", "họp #dự-án\n"),
            }],
        );

        let node = db.get_nodes_by_type("quickcap").unwrap().remove(0);
        assert_eq!(
            node.properties.get("tags").unwrap(),
            &serde_json::json!(["dự-án"])
        );
    }

    /// `resolve_safe_path` is the guard; this pins that the migration path
    /// actually consults it rather than joining strings itself.
    #[test]
    fn refuses_to_escape_the_vault() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();

        let report = apply_writes(
            &db,
            &vault(&dir),
            &[SilentWrite {
                rel_path: "../outside.md".into(),
                content: "nope".into(),
            }],
        );

        assert_eq!((report.changed, report.failed), (0, 1));
        assert!(!dir.path().parent().unwrap().join("outside.md").exists());
    }
}
