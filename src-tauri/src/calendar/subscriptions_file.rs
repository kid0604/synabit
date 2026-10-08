//! Which calendars the user subscribes to, kept in the vault.
//!
//! The URL, the name they gave it, its colour and its two switches are a
//! decision somebody made, and are kept in `Calendar/subscriptions.json`. What
//! the calendar *contains* is still a cache (`calendar_subscription_events`,
//! replaced whole on every refresh) for the reasons in `commands::calendar_subs`:
//! it is somebody else's data, and writing it into the vault would sync it,
//! make it editable, and leave orphans behind.
//!
//! The `calendar_subscriptions` table is the index of this file plus what this
//! device last heard from each server (ETag, last error, event count). The
//! file is the truth: [`reconcile`] brings the table in line with it, adding
//! what is new and removing — with its events — what is gone.
//!
//! Merged subscription by subscription by the sync layer
//! (`sync::core::merge`), with a stamped tombstone for a removal so the other
//! device's copy does not bring it back.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::subscriptions::{Subscription, SUBSCRIPTION_COLOURS};
use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::utils::vault_doc;

/// Vault-relative, for the sync layer's merge rule.
pub const FILE: &str = "Calendar/subscriptions.json";

/// Set in the cache once its subscriptions have been written to the vault.
///
/// In the cache on purpose: it must be lost exactly when the table it guards
/// is lost. A fresh cache has nothing to move, and an old one that still has
/// rows and no flag has not moved them yet.
const MOVED_FLAG: &str = "calendar_subscriptions_in_vault";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub colour: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub remind: bool,
    /// Seconds since the epoch, as the table has always kept it; it orders
    /// the list.
    #[serde(default)]
    pub created_at: i64,
    /// When this entry last changed: what decides which of two copies wins.
    #[serde(default)]
    pub updated_at: String,
    /// When it was removed. Present, it is a tombstone.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub removed_at: String,
}

fn yes() -> bool {
    true
}

impl Entry {
    pub fn is_live(&self) -> bool {
        self.removed_at.is_empty()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    pub subscriptions: Vec<Entry>,
}

fn path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(FILE)
}

pub fn read(vault_path: &str) -> AppResult<Book> {
    vault_doc::read(&path(vault_path))
}

fn now_stamp() -> String {
    crate::syn::vault_json::now_stamp()
}

/// Add a calendar, giving it whichever colour is least used so far.
pub fn add(vault_path: &str, id: &str, url: &str, name: &str, created_at: i64) -> AppResult<Entry> {
    let _writing = vault_doc::writing();
    let mut book = read(vault_path)?;
    let live: Vec<&Entry> = book.subscriptions.iter().filter(|e| e.is_live()).collect();
    let colour = SUBSCRIPTION_COLOURS
        .iter()
        .min_by_key(|c| live.iter().filter(|e| &e.colour == *c).count())
        .copied()
        .unwrap_or("teal");
    let entry = Entry {
        id: id.to_string(),
        url: url.to_string(),
        name: name.to_string(),
        colour: colour.to_string(),
        enabled: true,
        remind: false,
        created_at,
        updated_at: now_stamp(),
        removed_at: String::new(),
    };
    book.subscriptions.retain(|e| e.id != id);
    book.subscriptions.push(entry.clone());
    vault_doc::write(&path(vault_path), &book)?;
    Ok(entry)
}

/// Change one calendar in place. A calendar that is not there, or was
/// removed, is an error rather than silently brought back.
pub fn update(vault_path: &str, id: &str, change: impl FnOnce(&mut Entry)) -> AppResult<()> {
    let _writing = vault_doc::writing();
    let mut book = read(vault_path)?;
    let entry = book
        .subscriptions
        .iter_mut()
        .find(|e| e.id == id && e.is_live())
        .ok_or_else(|| AppError::General("No such calendar".to_string()))?;
    change(entry);
    entry.updated_at = now_stamp();
    vault_doc::write(&path(vault_path), &book)
}

/// Remove a calendar, leaving a tombstone the other devices will honour.
pub fn remove(vault_path: &str, id: &str) -> AppResult<()> {
    let _writing = vault_doc::writing();
    let mut book = read(vault_path)?;
    let Some(entry) = book.subscriptions.iter_mut().find(|e| e.id == id && e.is_live()) else {
        return Ok(());
    };
    let now = now_stamp();
    entry.removed_at = now.clone();
    entry.updated_at = now;
    vault_doc::write(&path(vault_path), &book)
}

fn entry_of(sub: &Subscription) -> Entry {
    Entry {
        id: sub.id.clone(),
        url: sub.url.clone(),
        name: sub.name.clone(),
        colour: sub.colour.clone(),
        enabled: sub.enabled,
        remind: sub.remind,
        created_at: sub.created_at,
        updated_at: chrono::DateTime::from_timestamp(sub.created_at, 0)
            .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            .unwrap_or_default(),
        removed_at: String::new(),
    }
}

/// Bring the table in line with the vault's file.
///
/// The first time, on a cache that kept subscriptions before they moved, the
/// table is written to the file instead — once, flag-gated, and only if the
/// file is not already there. A file that cannot be read leaves the table
/// alone: it is the only other copy.
pub fn reconcile(db: &mut DbBridge, vault_path: &str) -> AppResult<()> {
    let file = path(vault_path);
    if db.get_kv(MOVED_FLAG)?.as_deref() != Some("1") {
        let existing = db.list_subscriptions()?;
        if !file.exists() && !existing.is_empty() {
            let _writing = vault_doc::writing();
            let book = Book { subscriptions: existing.iter().map(entry_of).collect() };
            vault_doc::write(&file, &book)?;
            log::info!("moved {} calendar subscription(s) from the cache into {FILE}", existing.len());
        }
        db.set_kv(MOVED_FLAG, "1")?;
    }

    let book = read(vault_path)?;
    let live: Vec<&Entry> = book.subscriptions.iter().filter(|e| e.is_live()).collect();
    for entry in &live {
        db.upsert_subscription_config(entry)?;
    }
    for row in db.list_subscriptions()? {
        if !live.iter().any(|e| e.id == row.id) {
            db.remove_subscription(&row.id)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::ics;

    fn temp_vault() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault");
        std::fs::create_dir_all(&path).unwrap();
        let path = path.to_string_lossy().to_string();
        (dir, path)
    }

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().unwrap()
    }

    const ONE_EVENT: &str = "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:A\r\nDTSTART;VALUE=DATE:20261001\r\nEND:VEVENT\r\n";

    /// The cache is replaced by an empty one; the subscription comes back
    /// from the vault, waiting to be fetched again.
    #[test]
    fn a_subscription_survives_the_cache_being_replaced() {
        let (_dir, vault) = temp_vault();
        let mut old = db();
        let entry = add(&vault, "s1", "https://example.com/holidays.ics", "Ngày lễ", 100).unwrap();
        update(&vault, "s1", |e| e.remind = true).unwrap();
        reconcile(&mut old, &vault).unwrap();
        old.replace_subscription_events("s1", &ics::import(ONE_EVENT)).unwrap();
        drop(old);

        let mut fresh = db();
        reconcile(&mut fresh, &vault).unwrap();
        let subs = fresh.list_subscriptions().unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].url, "https://example.com/holidays.ics");
        assert_eq!(subs[0].name, "Ngày lễ");
        assert_eq!(subs[0].colour, entry.colour);
        assert!(subs[0].remind);
        assert_eq!(subs[0].last_fetched_at, 0, "so the front end knows to fetch it");
    }

    /// A cache from before subscriptions moved: written to the vault once,
    /// with what made them what they are.
    #[test]
    fn an_old_caches_subscriptions_move_into_the_vault() {
        let (_dir, vault) = temp_vault();
        let mut db = db();
        db.add_subscription("s1", "https://example.com/team.ics", "Team", 100).unwrap();
        db.set_subscription_enabled("s1", false).unwrap();
        db.note_subscription_fetch("s1", "etag-1", "", "", 500).unwrap();

        reconcile(&mut db, &vault).unwrap();
        let book = read(&vault).unwrap();
        assert_eq!(book.subscriptions.len(), 1);
        assert_eq!(book.subscriptions[0].url, "https://example.com/team.ics");
        assert!(!book.subscriptions[0].enabled);
        // The fetch state stays in the cache, where it belongs.
        assert_eq!(db.get_subscription("s1").unwrap().unwrap().etag, "etag-1");

        // A second vault opened later does not get them too.
        let (_other_dir, other) = temp_vault();
        reconcile(&mut db, &other).unwrap();
        assert!(!Path::new(&other).join(FILE).exists());
        assert!(db.list_subscriptions().unwrap().is_empty());
    }

    #[test]
    fn a_removed_subscription_takes_its_events_and_leaves_a_tombstone() {
        let (_dir, vault) = temp_vault();
        let mut db = db();
        add(&vault, "s1", "https://example.com/a.ics", "A", 100).unwrap();
        reconcile(&mut db, &vault).unwrap();
        db.replace_subscription_events("s1", &ics::import(ONE_EVENT)).unwrap();

        remove(&vault, "s1").unwrap();
        reconcile(&mut db, &vault).unwrap();
        assert!(db.list_subscriptions().unwrap().is_empty());
        assert!(db.subscribed_event_summaries().unwrap().is_empty());
        assert!(!read(&vault).unwrap().subscriptions[0].removed_at.is_empty());
        assert!(update(&vault, "s1", |e| e.name = "back".into()).is_err());
    }

    /// A rename on another device arrives as a new file; the table follows it
    /// without losing what this device knows about the server.
    #[test]
    fn a_change_in_the_file_reaches_the_table_and_keeps_the_fetch_state() {
        let (_dir, vault) = temp_vault();
        let mut db = db();
        add(&vault, "s1", "https://example.com/a.ics", "A", 100).unwrap();
        reconcile(&mut db, &vault).unwrap();
        db.note_subscription_fetch("s1", "etag-1", "", "", 500).unwrap();

        update(&vault, "s1", |e| e.name = "Đổi tên".into()).unwrap();
        reconcile(&mut db, &vault).unwrap();
        let sub = db.get_subscription("s1").unwrap().unwrap();
        assert_eq!(sub.name, "Đổi tên");
        assert_eq!(sub.etag, "etag-1");

        // A new address is a new calendar as far as the server is concerned.
        update(&vault, "s1", |e| e.url = "https://example.com/b.ics".into()).unwrap();
        reconcile(&mut db, &vault).unwrap();
        assert_eq!(db.get_subscription("s1").unwrap().unwrap().etag, "");
    }

    #[test]
    fn an_unreadable_file_leaves_the_table_alone() {
        let (_dir, vault) = temp_vault();
        let mut db = db();
        add(&vault, "s1", "https://example.com/a.ics", "A", 100).unwrap();
        reconcile(&mut db, &vault).unwrap();
        std::fs::write(Path::new(&vault).join(FILE), "{ nope").unwrap();
        assert!(reconcile(&mut db, &vault).is_err());
        assert_eq!(db.list_subscriptions().unwrap().len(), 1);
    }
}
