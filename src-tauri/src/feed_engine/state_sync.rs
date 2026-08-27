//! Read state that travels between one person's own devices.
//!
//! Which articles have been read, starred or saved for later lives in SQLite,
//! which is local — so reading fifty things on the desktop left the phone
//! showing fifty unread. Putting that state in a shared file instead would run
//! straight into the thing that makes `sources.json` dangerous: the sync layer
//! merges text character by character, and a document two devices rewrite
//! constantly does not survive that.
//!
//! So each device writes only its own file, `Feeds/state/<deviceId>.json`, and
//! reads everybody's. A file with one writer has nothing to merge, and the
//! union of the files is the answer. Where two devices disagree about the same
//! article the later decision wins.
//!
//! The map is keyed by `(sourceId, guid)` rather than by article id, because
//! article ids are UUIDs minted locally at insert time and differ on every
//! device; the source id comes from `sources.json`, which is shared, and the
//! guid comes from the feed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// One article's flags, as one device last set them.
///
/// The field names are short because there is one of these per article and the
/// whole map is a synced document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleState {
    #[serde(rename = "r")]
    pub is_read: bool,
    #[serde(rename = "s")]
    pub is_starred: bool,
    #[serde(rename = "l")]
    pub is_read_later: bool,
    /// When this device decided it, RFC 3339. The tiebreaker.
    #[serde(rename = "t")]
    pub updated_at: String,
}

/// A whole device's decisions. `BTreeMap` so the file is ordered: an ordered
/// document changes in one place when one entry changes, which is what keeps
/// the character-level sync diff small.
pub type StateMap = BTreeMap<String, ArticleState>;

fn state_dir(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join("Feeds").join("state")
}

/// `sourceId|guid`. Source ids are UUIDs, so the first bar is the separator.
fn state_key(source_id: &str, guid: &str) -> String {
    format!("{}|{}", source_id, guid)
}

fn split_key(key: &str) -> Option<(&str, &str)> {
    key.split_once('|')
}

/// Write this device's decisions to its own file.
///
/// Returns whether anything was written. The file is a projection of the
/// article cache, so an article that cleanup has removed drops out of it
/// naturally and the file cannot outgrow the cache.
pub fn publish(
    conn: &rusqlite::Connection,
    vault_path: &str,
    device_id: &str,
) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(
            "SELECT feed_source_id, guid, is_read, is_starred, is_read_later, state_updated_at
             FROM feed_articles
             WHERE state_updated_at != ''",
        )
        .map_err(|e| format!("State query error: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                state_key(&row.get::<_, String>(0)?, &row.get::<_, String>(1)?),
                ArticleState {
                    is_read: row.get::<_, i64>(2)? != 0,
                    is_starred: row.get::<_, i64>(3)? != 0,
                    is_read_later: row.get::<_, i64>(4)? != 0,
                    updated_at: row.get(5)?,
                },
            ))
        })
        .map_err(|e| format!("State map error: {}", e))?;

    let map: StateMap = rows.filter_map(|r| r.ok()).collect();

    let dir = state_dir(vault_path);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create Feeds/state: {}", e))?;
    let path = dir.join(format!("{}.json", device_id));

    let json = serde_json::to_string_pretty(&map)
        .map_err(|e| format!("Failed to serialize read state: {}", e))?;

    // Writing an identical file would still be a vault change, and every vault
    // change is something for the sync layer to carry.
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == json {
            return Ok(false);
        }
    }

    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write read state: {}", e))?;
    Ok(true)
}

/// The union of every other device's file, later decision winning.
fn union_of_others(vault_path: &str, device_id: &str) -> (StateMap, String) {
    let dir = state_dir(vault_path);
    let own = format!("{}.json", device_id);

    let mut files: Vec<(String, String)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == own || !name.ends_with(".json") {
                continue;
            }
            if let Ok(body) = std::fs::read_to_string(entry.path()) {
                files.push((name, body));
            }
        }
    }
    // Sorted so the fingerprint does not depend on directory order.
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    let mut union: StateMap = StateMap::new();

    for (name, body) in &files {
        hasher.update(name.as_bytes());
        hasher.update(body.as_bytes());

        // One unreadable file — half-synced, or hand-edited — must not stop
        // the others from being applied.
        let Ok(map) = serde_json::from_str::<StateMap>(body) else {
            continue;
        };
        for (key, state) in map {
            match union.get(&key) {
                Some(existing) if existing.updated_at >= state.updated_at => {}
                _ => {
                    union.insert(key, state);
                }
            }
        }
    }

    (union, hex::encode(hasher.finalize()))
}

/// Apply what the other devices decided, where they decided it more recently.
///
/// Returns how many articles changed. `force` re-applies even when no file has
/// changed, which is what a refresh needs: articles that arrived just now were
/// not in the database the last time this ran.
pub fn apply(
    conn: &rusqlite::Connection,
    vault_path: &str,
    device_id: &str,
    force: bool,
    last_seen: Option<&str>,
) -> Result<(usize, String), String> {
    let (union, fingerprint) = union_of_others(vault_path, device_id);

    if !force && last_seen == Some(fingerprint.as_str()) {
        return Ok((0, fingerprint));
    }

    let mut stmt = conn
        .prepare(
            // The last clause makes the returned count mean something. Without
            // it SQLite reports every row the predicate matched, so re-applying
            // an already-applied file looked like news and sent the app off to
            // reload a list that had not changed.
            "UPDATE feed_articles
                SET is_read = ?1, is_starred = ?2, is_read_later = ?3
              WHERE feed_source_id = ?4
                AND guid = ?5
                AND (state_updated_at = '' OR state_updated_at < ?6)
                AND (is_read != ?1 OR is_starred != ?2 OR is_read_later != ?3)",
        )
        .map_err(|e| format!("State apply prepare error: {}", e))?;

    let mut changed = 0;
    for (key, state) in &union {
        let Some((source_id, guid)) = split_key(key) else {
            continue;
        };
        // `state_updated_at` is deliberately left alone. It marks what *this*
        // device decided, and it is what keeps this device's published file to
        // its own decisions instead of a copy of everybody else's.
        changed += stmt
            .execute(params![
                state.is_read as i64,
                state.is_starred as i64,
                state.is_read_later as i64,
                source_id,
                guid,
                state.updated_at,
            ])
            .map_err(|e| format!("State apply error: {}", e))?;
    }

    Ok((changed, fingerprint))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_survives_a_guid_that_is_a_url() {
        let key = state_key("6f0d-uuid", "https://example.com/post?a=1|b");
        assert_eq!(
            split_key(&key),
            Some(("6f0d-uuid", "https://example.com/post?a=1|b")),
            "only the first bar separates; guids may contain their own"
        );
    }

    #[test]
    fn the_union_prefers_the_later_decision() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_string_lossy().to_string();
        let state = state_dir(&vault);
        std::fs::create_dir_all(&state).unwrap();

        std::fs::write(
            state.join("device-a.json"),
            r#"{"s1|g1":{"r":true,"s":false,"l":false,"t":"2026-08-01T00:00:00+00:00"}}"#,
        )
        .unwrap();
        std::fs::write(
            state.join("device-b.json"),
            r#"{"s1|g1":{"r":false,"s":true,"l":false,"t":"2026-08-02T00:00:00+00:00"}}"#,
        )
        .unwrap();

        let (union, _) = union_of_others(&vault, "device-c");
        let entry = union.get("s1|g1").expect("the article");
        assert!(!entry.is_read, "the later file said unread");
        assert!(entry.is_starred);
    }

    #[test]
    fn a_devices_own_file_is_not_read_back_in() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_string_lossy().to_string();
        let state = state_dir(&vault);
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("device-a.json"),
            r#"{"s1|g1":{"r":true,"s":false,"l":false,"t":"2026-08-01T00:00:00+00:00"}}"#,
        )
        .unwrap();

        let (union, _) = union_of_others(&vault, "device-a");
        assert!(union.is_empty(), "this device is the authority on its own file");
    }

    #[test]
    fn one_unreadable_file_does_not_lose_the_others() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_string_lossy().to_string();
        let state = state_dir(&vault);
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("device-a.json"), "{ this is not json").unwrap();
        std::fs::write(
            state.join("device-b.json"),
            r#"{"s1|g1":{"r":true,"s":false,"l":false,"t":"2026-08-02T00:00:00+00:00"}}"#,
        )
        .unwrap();

        let (union, _) = union_of_others(&vault, "device-c");
        assert_eq!(union.len(), 1, "a half-synced file is skipped, not fatal");
    }

    // ── Against a real article cache ─────────────────────────────────

    fn db_with_article(state_updated_at: &str) -> crate::db::DbBridge {
        let db = crate::db::DbBridge::new_in_memory_full().expect("schema");
        db.conn()
            .execute(
                "INSERT INTO feed_articles
                    (id, feed_source_id, guid, title, url, author, content, summary,
                     published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                     content_type, is_read, is_starred, is_read_later, state_updated_at)
                 VALUES ('a1', 's1', 'g1', 'Title', '', '', '', '', '', '', '',
                         0, 1, 'text/html', 0, 0, 0, ?1)",
                rusqlite::params![state_updated_at],
            )
            .expect("insert");
        db
    }

    fn write_remote(vault: &str, device: &str, body: &str) {
        let dir = state_dir(vault);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{device}.json")), body).unwrap();
    }

    fn flags(conn: &rusqlite::Connection) -> (i64, i64, i64, String) {
        conn.query_row(
            "SELECT is_read, is_starred, is_read_later, state_updated_at
             FROM feed_articles WHERE id = 'a1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .expect("row")
    }

    #[test]
    fn another_devices_decision_arrives_without_being_claimed_as_ours() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let db = db_with_article("");
        write_remote(
            &vault,
            "device-a",
            r#"{"s1|g1":{"r":true,"s":true,"l":false,"t":"2026-08-02T00:00:00+00:00"}}"#,
        );

        let (changed, _) = apply(db.conn(), &vault, "me", false, None).expect("apply");
        assert_eq!(changed, 1);

        let (read, starred, _, updated) = flags(db.conn());
        assert_eq!((read, starred), (1, 1));
        assert!(
            updated.is_empty(),
            "state_updated_at marks what this device decided; relaying is not deciding"
        );
    }

    #[test]
    fn a_newer_local_decision_is_not_undone_by_an_older_remote_one() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let db = db_with_article("2026-08-05T00:00:00+00:00");
        db.conn()
            .execute("UPDATE feed_articles SET is_read = 0 WHERE id = 'a1'", [])
            .unwrap();
        write_remote(
            &vault,
            "device-a",
            r#"{"s1|g1":{"r":true,"s":false,"l":false,"t":"2026-08-02T00:00:00+00:00"}}"#,
        );

        let (changed, _) = apply(db.conn(), &vault, "me", false, None).expect("apply");
        assert_eq!(changed, 0, "we marked it unread more recently than they read it");
        assert_eq!(flags(db.conn()).0, 0);
    }

    #[test]
    fn a_newer_remote_decision_wins_over_an_older_local_one() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let db = db_with_article("2026-08-01T00:00:00+00:00");
        write_remote(
            &vault,
            "device-a",
            r#"{"s1|g1":{"r":true,"s":false,"l":false,"t":"2026-08-09T00:00:00+00:00"}}"#,
        );

        apply(db.conn(), &vault, "me", false, None).expect("apply");
        assert_eq!(flags(db.conn()).0, 1);
    }

    #[test]
    fn an_unchanged_set_of_files_is_not_applied_twice() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let db = db_with_article("");
        write_remote(
            &vault,
            "device-a",
            r#"{"s1|g1":{"r":true,"s":false,"l":false,"t":"2026-08-02T00:00:00+00:00"}}"#,
        );

        let (_, fingerprint) = apply(db.conn(), &vault, "me", false, None).expect("first");
        let (changed, _) =
            apply(db.conn(), &vault, "me", false, Some(&fingerprint)).expect("second");
        assert_eq!(changed, 0);

        let (forced, _) = apply(db.conn(), &vault, "me", true, Some(&fingerprint)).expect("forced");
        assert_eq!(forced, 0, "already applied, so forcing changes nothing either");
    }

    #[test]
    fn publishing_writes_only_what_this_device_decided() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let db = db_with_article("");
        db.conn()
            .execute(
                "INSERT INTO feed_articles
                    (id, feed_source_id, guid, title, url, author, content, summary,
                     published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                     content_type, is_read, is_starred, is_read_later, state_updated_at)
                 VALUES ('a2', 's1', 'g2', 'Mine', '', '', '', '', '', '', '',
                         0, 1, 'text/html', 1, 0, 0, '2026-08-09T00:00:00+00:00')",
                [],
            )
            .unwrap();

        assert!(publish(db.conn(), &vault, "me").expect("publish"));

        let body = std::fs::read_to_string(state_dir(&vault).join("me.json")).unwrap();
        let map: StateMap = serde_json::from_str(&body).unwrap();
        assert_eq!(map.len(), 1, "the untouched article is nobody's decision");
        assert!(map.contains_key("s1|g2"));

        assert!(
            !publish(db.conn(), &vault, "me").expect("republish"),
            "an identical file is not worth handing to the sync layer"
        );
    }

    #[test]
    fn the_fingerprint_changes_only_when_the_files_do() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_string_lossy().to_string();
        let state = state_dir(&vault);
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join("device-a.json"),
            r#"{"s1|g1":{"r":true,"s":false,"l":false,"t":"2026-08-01T00:00:00+00:00"}}"#,
        )
        .unwrap();

        let (_, first) = union_of_others(&vault, "me");
        let (_, again) = union_of_others(&vault, "me");
        assert_eq!(first, again);

        std::fs::write(
            state.join("device-a.json"),
            r#"{"s1|g1":{"r":false,"s":false,"l":false,"t":"2026-08-03T00:00:00+00:00"}}"#,
        )
        .unwrap();
        let (_, changed) = union_of_others(&vault, "me");
        assert_ne!(first, changed);
    }
}
