//! Highlights and notes on feed articles, kept in the vault.
//!
//! A highlight is something the reader did — the passage they marked and what
//! they wrote beside it — and so it is theirs in a way the article cache is
//! not. It used to live only in the cache, which meant it was in no backup, on
//! none of their other devices, and gone the day the cache was rebuilt.
//!
//! One file, `Feeds/highlights.json`, beside `sources.json`. One rather than a
//! file per device (the way `Feeds/state/` is split) because a highlight is
//! made once and seldom touched after, so two devices rarely change the same
//! one; and the sync layer merges this file highlight by highlight
//! (`sync::core::merge`), so two devices adding at once both keep theirs.
//! Removal is a stamped tombstone for the same reason: a union never forgets,
//! and without one the other device's copy would bring a removed highlight
//! back. The tombstone keeps the ids and drops the words.
//!
//! Keyed by feed and guid, as the read state is: article ids are minted
//! locally and differ on every device, the guid comes from the publisher.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::utils::vault_doc;

/// Vault-relative, for the sync layer's merge rule.
pub const FILE: &str = "Feeds/highlights.json";

/// One highlight, as the file holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    pub id: String,
    pub source_id: String,
    pub guid: String,
    #[serde(default)]
    pub text: String,
    /// Which occurrence of `text` in the article, from zero.
    #[serde(default)]
    pub occurrence: i64,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub created_at: String,
    /// When this copy last changed: what decides which of two copies wins.
    #[serde(default)]
    pub updated_at: String,
    /// When it was taken away. Present, it is a tombstone.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub removed_at: String,
}

impl Stored {
    pub fn is_live(&self) -> bool {
        self.removed_at.is_empty()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    pub highlights: Vec<Stored>,
}

fn path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(FILE)
}

/// Every highlight in the vault, tombstones included.
pub fn read(vault_path: &str) -> AppResult<Book> {
    vault_doc::read(&path(vault_path))
}

/// The live highlights on one article, in reading order.
pub fn on_article(vault_path: &str, source_id: &str, guid: &str) -> AppResult<Vec<Stored>> {
    let mut found: Vec<Stored> = read(vault_path)?
        .highlights
        .into_iter()
        .filter(|h| h.is_live() && h.source_id == source_id && h.guid == guid)
        .collect();
    found.sort_by(|a, b| a.occurrence.cmp(&b.occurrence).then_with(|| a.created_at.cmp(&b.created_at)));
    Ok(found)
}

/// Keep a new highlight.
pub fn add(vault_path: &str, highlight: Stored) -> AppResult<()> {
    let _writing = vault_doc::writing();
    let mut book = read(vault_path)?;
    book.highlights.retain(|h| h.id != highlight.id);
    book.highlights.push(highlight);
    vault_doc::write(&path(vault_path), &book)
}

/// Take a highlight away, leaving a tombstone the other devices will honour.
pub fn remove(vault_path: &str, id: &str, now: &str) -> AppResult<()> {
    let _writing = vault_doc::writing();
    let mut book = read(vault_path)?;
    let Some(gone) = book.highlights.iter_mut().find(|h| h.id == id && h.is_live()) else {
        return Ok(());
    };
    gone.text.clear();
    gone.note.clear();
    gone.removed_at = now.to_string();
    gone.updated_at = now.to_string();
    vault_doc::write(&path(vault_path), &book)
}

/// Move the highlights an older cache kept into the vault, and drop its table.
///
/// Ids already in the file are left as the file has them. Written before the
/// table is dropped, so an interruption costs nothing: the next call finds the
/// table still there and the ids already moved.
pub fn adopt_from_cache(conn: &rusqlite::Connection, vault_path: &str) -> AppResult<usize> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM main.sqlite_master WHERE type = 'table' AND name = 'feed_highlights'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !exists {
        return Ok(0);
    }
    let sql = |e: rusqlite::Error| crate::error::AppError::General(format!("feed_highlights: {e}"));
    let rows: Vec<Stored> = conn
        .prepare("SELECT id, source_id, guid, text, occurrence, note, created_at FROM main.feed_highlights")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let created_at: String = row.get(6)?;
                Ok(Stored {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    guid: row.get(2)?,
                    text: row.get(3)?,
                    occurrence: row.get(4)?,
                    note: row.get(5)?,
                    updated_at: created_at.clone(),
                    created_at,
                    removed_at: String::new(),
                })
            })?
            .collect::<Result<_, _>>()
        })
        .map_err(sql)?;

    let mut moved = 0;
    if !rows.is_empty() {
        let _writing = vault_doc::writing();
        let mut book = read(vault_path)?;
        for row in rows {
            if !book.highlights.iter().any(|h| h.id == row.id) {
                book.highlights.push(row);
                moved += 1;
            }
        }
        if moved > 0 {
            vault_doc::write(&path(vault_path), &book)?;
            log::info!("moved {moved} feed highlight(s) from the cache into {FILE}");
        }
    }
    conn.execute_batch("DROP TABLE main.feed_highlights").map_err(sql)?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlight(id: &str, text: &str, at: &str) -> Stored {
        Stored {
            id: id.into(),
            source_id: "feed-1".into(),
            guid: "post-1".into(),
            text: text.into(),
            occurrence: 0,
            note: String::new(),
            created_at: at.into(),
            updated_at: at.into(),
            removed_at: String::new(),
        }
    }

    fn vault() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault");
        std::fs::create_dir_all(&path).unwrap();
        let path = path.to_string_lossy().to_string();
        (dir, path)
    }

    #[test]
    fn a_highlight_is_kept_in_the_vault_and_read_back() {
        let (_dir, vault) = vault();
        add(&vault, highlight("h1", "đoạn đáng nhớ", "2026-10-01T00:00:00Z")).unwrap();
        let got = on_article(&vault, "feed-1", "post-1").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "đoạn đáng nhớ");
        assert!(Path::new(&vault).join(FILE).exists());
    }

    /// The cache is gone and a new one is empty; the highlight is still there,
    /// because it never depended on the cache.
    #[test]
    fn a_highlight_survives_the_cache_being_replaced() {
        let (_dir, vault) = vault();
        let old = crate::db::DbBridge::new_in_memory_full().unwrap();
        add(&vault, highlight("h1", "giữ lại", "2026-10-01T00:00:00Z")).unwrap();
        drop(old);
        let fresh = crate::db::DbBridge::new_in_memory_full().unwrap();
        assert_eq!(adopt_from_cache(fresh.conn(), &vault).unwrap(), 0);
        assert_eq!(on_article(&vault, "feed-1", "post-1").unwrap()[0].text, "giữ lại");
    }

    #[test]
    fn a_removed_highlight_leaves_a_tombstone_without_its_words() {
        let (_dir, vault) = vault();
        add(&vault, highlight("h1", "riêng tư", "2026-10-01T00:00:00Z")).unwrap();
        remove(&vault, "h1", "2026-10-02T00:00:00Z").unwrap();
        assert!(on_article(&vault, "feed-1", "post-1").unwrap().is_empty());
        let book = read(&vault).unwrap();
        assert_eq!(book.highlights.len(), 1);
        assert_eq!(book.highlights[0].removed_at, "2026-10-02T00:00:00Z");
        assert!(book.highlights[0].text.is_empty());
    }

    /// An unreadable file is somebody's highlights. Adding one must not write
    /// over it with a file holding only the new one.
    #[test]
    fn a_broken_file_is_not_written_over() {
        let (_dir, vault) = vault();
        let file = Path::new(&vault).join(FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{\"highlights\": [ broken").unwrap();
        assert!(add(&vault, highlight("h1", "x", "2026-10-01T00:00:00Z")).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{\"highlights\": [ broken");
    }

    /// A cache from before highlights moved: its rows reach the vault once,
    /// the table goes, and a highlight already in the file is not doubled.
    #[test]
    fn an_old_caches_highlights_move_into_the_vault() {
        let (_dir, vault) = vault();
        add(&vault, highlight("h1", "đã ở vault", "2026-10-01T00:00:00Z")).unwrap();
        let db = crate::db::DbBridge::new_in_memory_full().unwrap();
        db.conn()
            .execute_batch(
                "CREATE TABLE feed_highlights (id TEXT PRIMARY KEY, source_id TEXT NOT NULL, guid TEXT NOT NULL,
                     text TEXT NOT NULL, occurrence INTEGER NOT NULL DEFAULT 0, note TEXT NOT NULL DEFAULT '',
                     created_at TEXT NOT NULL DEFAULT '');
                 INSERT INTO feed_highlights VALUES ('h1', 'feed-1', 'post-1', 'đã ở vault', 0, '', '2026-10-01T00:00:00Z');
                 INSERT INTO feed_highlights VALUES ('h2', 'feed-1', 'post-1', 'chỉ trong cache', 1, 'ghi chú', '2026-09-01T00:00:00Z');",
            )
            .unwrap();

        assert_eq!(adopt_from_cache(db.conn(), &vault).unwrap(), 1);
        let got = on_article(&vault, "feed-1", "post-1").unwrap();
        assert_eq!(got.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["h1", "h2"]);
        assert_eq!(got[1].note, "ghi chú");
        assert_eq!(adopt_from_cache(db.conn(), &vault).unwrap(), 0, "the table is gone");
    }
}
