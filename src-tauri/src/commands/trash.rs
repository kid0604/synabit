//! Deleting a node without destroying it.
//!
//! `delete_node_file` removes the file. That is the right primitive and the
//! wrong default: a capture inbox is where half-formed thoughts live, and a
//! misplaced click on a small icon is the single fastest way to lose one.
//! Losing one is also the fastest way to lose the user — nobody returns to
//! a box that ate an idea.
//!
//! So a delete moves the file into `.trash/` instead. Two facts make that
//! cheap rather than clever:
//!
//! - the vault scanner already skips every directory whose name begins with
//!   a dot, so a file in `.trash/` disappears from the index by itself;
//! - a rename within one filesystem is atomic, so there is no window where
//!   the note exists in neither place.
//!
//! # What this deliberately does not do
//!
//! There is no `restore` here, and that is a design decision rather than an
//! omission. Sync detects a deletion by noticing that a tracked path no
//! longer holds a file (`sync::core::change::detect_deletions`), so the
//! moment this runs, a tombstone is on its way to every other device.
//! Restoring afterwards would be a race against that tombstone, and the
//! tombstone would sometimes win.
//!
//! The undo the user actually gets is upstream of here: the front end holds
//! the deletion for a few seconds and touches nothing at all until the
//! window closes. Undo is cancelling a timer, so there is nothing to race.
//! If the app quits inside that window the deletion simply never happens,
//! which is the safe direction to fail in.

use std::path::{Path, PathBuf};

use crate::db::DbState;
use crate::error::{logged, AppResult};
use crate::path_utils;

/// Where deleted files go. Leading dot, so the scanner ignores it.
const TRASH_DIR: &str = ".trash";

/// A free path inside the trash for something being deleted.
///
/// The original relative path is preserved underneath, so what is in there
/// stays legible to a person opening the vault in a file manager. A name
/// already taken gains a counter rather than overwriting: a trash that
/// destroys the thing it is holding would be worse than no trash.
fn free_trash_path(vault: &Path, rel_path: &str) -> PathBuf {
    let base = vault.join(TRASH_DIR).join(rel_path);
    if !base.exists() {
        return base;
    }

    let stem = base
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = base.extension().map(|e| e.to_string_lossy().to_string());
    let parent = base.parent().map(Path::to_path_buf).unwrap_or_default();

    for n in 1..1000 {
        let name = match &ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    base
}

/// Move a node into the vault's trash and drop it from the index.
///
/// Returns the trash-relative path, so a caller can tell the user where the
/// thing went rather than just asserting it is safe.
#[tauri::command]
pub fn trash_node_file(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
) -> AppResult<String> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    apply_trash(&db, &vault_path, &rel_path)
}

/// The move itself, over a plain connection.
///
/// Split from the command so the tests drive *this* — the code that runs —
/// rather than checking the path helper and the database deletes separately
/// and hoping the command joins them up correctly.
pub(crate) fn apply_trash(
    db: &crate::db::DbBridge,
    vault_path: &str,
    rel_path: &str,
) -> AppResult<String> {
    let abs_path = path_utils::resolve_safe_path(vault_path, rel_path)?;
    let vault = Path::new(vault_path);

    if abs_path.exists() {
        let target = free_trash_path(vault, rel_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Same filesystem, so this is a rename rather than a copy: atomic,
        // and instant regardless of how large an attachment the cap carries.
        std::fs::rename(&abs_path, &target)?;
    }

    logged("trash node", rel_path, db.delete_node(rel_path));
    crate::commands::nodes::delete_node_edges_for(db, rel_path);
    db.delete_search_entry(rel_path);

    // `DELETE ... WHERE id = ?` reports success when it matches nothing, and a
    // file that reappears seconds after being deleted looks identical whether
    // the row was never dropped or something put it back. Say which, once, at
    // the only point where both facts are still in hand.
    let row_survived = db.get_node(rel_path).ok().flatten().is_some();
    let file_survived = abs_path.exists();
    if row_survived || file_survived {
        log::warn!(
            "trash '{}': row still present = {}, file still at its old path = {}",
            rel_path,
            row_survived,
            file_survived
        );
    } else {
        log::info!("trash '{}': moved and de-indexed", rel_path);
    }

    Ok(format!("{TRASH_DIR}/{rel_path}"))
}

/// Where the Files app puts what it deletes, under the same trash.
///
/// A subdirectory rather than the trash root because the two are holding
/// different kinds of thing. Everything else in `.trash/` is a node file, and
/// `list_trash` parses every file it finds as one to say what it was; asked to
/// describe a JPEG it would produce an entry with no title and no type. Keeping
/// assets in their own subtree lets that listing skip them while `purge_trash`,
/// which only reads modification times, still ages them out for free.
const ASSET_TRASH_SUBDIR: &str = "files";

/// Move a file the Files app is deleting into the vault trash.
///
/// Separate from `apply_trash` because the input is a different shape: that one
/// takes a vault-relative node path and de-indexes the row behind it, while a
/// file the user added as a source may sit anywhere on the disk and has no node
/// file to speak of. Only the bytes move here; the caller owns the index.
///
/// The absolute path is preserved underneath `.trash/files/` with its root
/// marker stripped, for the same reason node paths are preserved: somebody
/// opening the vault in a file manager should be able to see where a thing came
/// from without consulting the app.
pub(crate) fn trash_asset(vault_path: &str, abs_path: &Path) -> AppResult<String> {
    let vault = Path::new(vault_path);
    let rel = format!("{ASSET_TRASH_SUBDIR}/{}", trail_of(abs_path));
    let target = free_trash_path(vault, &rel);

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // A rename is atomic and instant, and it is what happens whenever the file
    // is already inside the vault. It is also the one thing that cannot work
    // when a source folder lives on another volume — an external drive, a
    // network mount — because a rename cannot cross a filesystem boundary.
    // Copying then removing is slower and briefly leaves two copies, which is
    // the safe direction to fail in: an interrupted copy loses nothing.
    if std::fs::rename(abs_path, &target).is_err() {
        std::fs::copy(abs_path, &target)?;
        std::fs::remove_file(abs_path)?;
    }

    // Where it landed, not where it was headed. `free_trash_path` steps aside
    // for a name already taken, so on the second delete of the same file these
    // two differ — and reporting the intended path would point the caller at
    // the copy trashed earlier.
    Ok(target
        .strip_prefix(vault)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| format!("{TRASH_DIR}/{rel}")))
}

/// An absolute path rewritten as something that can hang below a directory.
///
/// `Path::join` throws away everything to its left when the argument is
/// absolute, so `/Users/a/photo.jpg` joined onto the trash *is* the original
/// path and the move would be a no-op onto itself. Dropping the root prefix —
/// the leading separator on Unix, the `C:\` on Windows — makes it a trail
/// instead, which is what keeps the original location readable.
fn trail_of(abs_path: &Path) -> String {
    let trail: Vec<String> = abs_path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            // A drive letter arrives as `C:`, and a colon is not a legal
            // filename character on the platform it comes from.
            std::path::Component::Prefix(p) => {
                let raw = p.as_os_str().to_string_lossy().replace([':', '\\'], "");
                (!raw.is_empty()).then_some(raw)
            }
            _ => None,
        })
        .collect();

    if trail.is_empty() {
        "unnamed".to_string()
    } else {
        trail.join("/")
    }
}

/// Delete anything that has been sitting in the trash longer than
/// `max_age_days`, and report how many files went.
///
/// A trash nobody empties is just a slow disk leak, and on a phone the vault
/// lives in app storage where space is not the user's to spare.
#[tauri::command]
pub fn purge_trash(vault_path: String, max_age_days: u64) -> AppResult<usize> {
    let trash = Path::new(&vault_path).join(TRASH_DIR);
    if !trash.exists() {
        return Ok(0);
    }

    let max_age = std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = std::time::SystemTime::now();
    let mut removed = 0;

    for entry in walkdir::WalkDir::new(&trash)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };

        if age > max_age && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }

    if removed > 0 {
        log::info!("trash: purged {removed} file(s) older than {max_age_days} days");
    }
    Ok(removed)
}

/// One thing sitting in the trash.
#[derive(serde::Serialize, Debug, Clone)]
pub struct TrashEntry {
    /// Path within the vault, `.trash/...`, which identifies it to the other commands.
    pub trash_path: String,
    /// Where it came from, so the list can say what it was rather than where it is.
    pub original_path: String,
    pub title: String,
    pub node_type: String,
    /// When it was moved here, as milliseconds since the epoch.
    pub deleted_at: i64,
    pub size: u64,
}

/// What is currently in the trash, newest first.
#[tauri::command]
pub fn list_trash(vault_path: String) -> AppResult<Vec<TrashEntry>> {
    let trash = Path::new(&vault_path).join(TRASH_DIR);
    if !trash.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(&trash)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let Ok(rel) = entry.path().strip_prefix(&trash) else {
            continue;
        };
        let original_path = rel.to_string_lossy().replace('\\', "/");

        // Assets deleted by the Files app live here too, and nothing below
        // parses them into anything a person would recognise. See
        // `ASSET_TRASH_SUBDIR`.
        if original_path.starts_with(&format!("{ASSET_TRASH_SUBDIR}/")) {
            continue;
        }
        let meta = entry.metadata().ok();
        let deleted_at = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // Parsed rather than guessed from the filename: tasks and notes are
        // named with a UUID, so the filename says nothing a person can read.
        let parsed = crate::utils::node_parser::parse_file_to_node(
            &trash.to_string_lossy(),
            entry.path(),
        );

        entries.push(TrashEntry {
            trash_path: format!("{TRASH_DIR}/{original_path}"),
            original_path,
            title: parsed
                .as_ref()
                .map(|n| n.title.clone())
                .unwrap_or_else(|| {
                    entry.path().file_name().unwrap_or_default().to_string_lossy().to_string()
                }),
            node_type: parsed.map(|n| n.node_type).unwrap_or_default(),
            deleted_at,
            size: meta.map(|m| m.len()).unwrap_or(0),
        });
    }

    entries.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    Ok(entries)
}

/// Where a trashed file would go back to, guaranteed not to overwrite anything.
fn restore_target(vault: &Path, trash_path: &str) -> AppResult<String> {
    let original = trash_path
        .strip_prefix(&format!("{TRASH_DIR}/"))
        .ok_or_else(|| crate::error::AppError::InvalidPath("not a trash path".into()))?;
    Ok(crate::commands::nodes::free_node_path(vault, original))
}

/// Put something back, as a new node rather than the one that was deleted.
///
/// The identity is deliberately not restored with it. Sync spots a deletion by
/// noticing a tracked path no longer holds a file, so a tombstone for the old
/// document left this device the moment it was trashed; bringing the same
/// `node_id` back would be a race against that tombstone, and the tombstone
/// would sometimes win — the file would reappear here and vanish again when
/// the next device synced.
///
/// Dropping `node_id` sidesteps the race entirely. What comes back is a new
/// document holding the old content: the tombstone stays true, because the
/// thing it describes really is gone, and the restored file is an ordinary
/// creation that every device accepts. The cost is the old version history,
/// which is a smaller thing to lose than the file.
#[tauri::command]
pub fn restore_from_trash(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    trash_path: String,
) -> AppResult<String> {
    let source = path_utils::resolve_safe_path(&vault_path, &trash_path)?;
    if !source.is_file() {
        return Err(crate::error::AppError::InvalidPath(
            "nothing at that path in the trash".into(),
        ));
    }

    let vault = Path::new(&vault_path);
    let target_rel = restore_target(vault, &trash_path)?;
    let target = path_utils::resolve_safe_path(&vault_path, &target_rel)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&source, &target)?;

    let parsed = crate::utils::node_parser::parse_file_to_node(&vault_path, &target);
    let title = parsed
        .as_ref()
        .map(|n| n.title.clone())
        .unwrap_or_else(|| "Untitled".to_string());
    let node_type = parsed
        .as_ref()
        .map(|n| n.node_type.clone())
        .unwrap_or_else(|| "note".to_string());

    // Through the ordinary write, so the index, the search entry, the edges
    // and the CRDT document are all set up the way any other file's are.
    // `node_id: null` removes the old identity; a fresh one is assigned on the
    // way through. The body is left alone — `None` means "keep what is there".
    crate::commands::nodes::write_node_file(
        app_handle,
        state,
        vault_path,
        target_rel.clone(),
        title,
        node_type,
        serde_json::json!({ "node_id": null }),
        None,
    )?;

    log::info!("trash: restored '{}' as '{}'", trash_path, target_rel);
    Ok(target_rel)
}

/// Remove one thing from the trash for good.
#[tauri::command]
pub fn delete_trash_entry(vault_path: String, trash_path: String) -> AppResult<()> {
    let target = path_utils::resolve_safe_path(&vault_path, &trash_path)?;
    if !target.is_file() {
        return Ok(());
    }
    std::fs::remove_file(&target)?;
    log::info!("trash: permanently removed '{}'", trash_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;

    fn seed_file(vault: &Path, rel: &str, body: &str) {
        let path = vault.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn seed_row(db: &DbBridge, rel: &str) {
        db.upsert_node(&NodeMetadata {
            id: rel.to_string(),
            node_type: "quickcap".into(),
            title: "a cap".into(),
            content: "nội dung".into(),
            properties: serde_json::json!({}),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
            timestamp: 0,
            blocks: None,
        })
        .unwrap();
    }

    /// The whole flow, through the code the command actually runs: the file
    /// leaves its path, the bytes survive, and the index forgets it.
    #[test]
    fn moves_the_file_and_drops_the_row() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        seed_file(dir.path(), "QuickCaps/a.md", "ý tưởng quan trọng");
        seed_row(&db, "QuickCaps/a.md");

        let landed = apply_trash(&db, &vault, "QuickCaps/a.md").unwrap();

        assert_eq!(landed, ".trash/QuickCaps/a.md");
        assert!(
            !dir.path().join("QuickCaps/a.md").exists(),
            "the file must leave the path sync watches"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".trash/QuickCaps/a.md")).unwrap(),
            "ý tưởng quan trọng",
            "the bytes have to survive, or this is just a delete"
        );
        assert!(
            db.get_nodes_by_type("quickcap").unwrap().is_empty(),
            "a trashed cap must not come back on the next read"
        );
    }

    /// A cap whose file is already gone still has to leave the index, or it
    /// reappears on every reload with nothing behind it.
    #[test]
    fn a_missing_file_still_clears_the_index() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_row(&db, "QuickCaps/gone.md");

        apply_trash(&db, &dir.path().to_string_lossy(), "QuickCaps/gone.md").unwrap();

        assert!(db.get_nodes_by_type("quickcap").unwrap().is_empty());
    }

    /// The original path is kept underneath, so the trash stays readable to
    /// a person rather than being a heap of unrelated filenames.
    #[test]
    fn keeps_the_original_path_inside_the_trash() {
        let dir = tempfile::tempdir().unwrap();
        let target = free_trash_path(dir.path(), "QuickCaps/a.md");
        assert_eq!(target, dir.path().join(".trash/QuickCaps/a.md"));
    }

    /// An absolute path has to become a trail before it can hang below the
    /// trash. `Path::join` discards everything to its left when the argument is
    /// absolute, so without this the "move" resolves to the file's own path and
    /// the delete silently does nothing.
    #[test]
    fn an_absolute_path_becomes_a_trail_rather_than_replacing_the_trash() {
        assert_eq!(trail_of(Path::new("/Users/anh/Ảnh/a.jpg")), "Users/anh/Ảnh/a.jpg");
        assert_eq!(trail_of(Path::new("/a.jpg")), "a.jpg");
        assert_eq!(trail_of(Path::new("/")), "unnamed");
    }

    /// The bytes of a file outside the vault have to survive the move, and the
    /// original location has to stay readable to a person opening `.trash/`.
    #[test]
    fn an_asset_keeps_its_bytes_and_its_original_location() {
        let vault_dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let file = source_dir.path().join("ảnh.txt");
        std::fs::write(&file, "nội dung ảnh").unwrap();

        let landed = trash_asset(&vault_dir.path().to_string_lossy(), &file).unwrap();

        assert!(landed.starts_with(".trash/files/"), "got {landed}");
        assert!(landed.ends_with("ảnh.txt"), "got {landed}");
        assert!(!file.exists(), "the file must leave where it was");
        assert_eq!(
            std::fs::read_to_string(vault_dir.path().join(&landed)).unwrap(),
            "nội dung ảnh"
        );
    }

    /// Trashing the same name twice must not have the second copy destroy the
    /// first — the same rule the node trash follows.
    #[test]
    fn a_second_asset_of_the_same_name_does_not_overwrite_the_first() {
        let vault_dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let file = source_dir.path().join("a.txt");

        std::fs::write(&file, "bản đầu").unwrap();
        let first = trash_asset(&vault_dir.path().to_string_lossy(), &file).unwrap();
        std::fs::write(&file, "bản sau").unwrap();
        let second = trash_asset(&vault_dir.path().to_string_lossy(), &file).unwrap();

        assert_ne!(first, second);
        assert_eq!(
            std::fs::read_to_string(vault_dir.path().join(&first)).unwrap(),
            "bản đầu",
            "the copy already in the trash must survive"
        );
    }

    /// `list_trash` parses everything it finds as a node to say what it was.
    /// A JPEG has no frontmatter, so left in the listing it would show up as an
    /// entry with no type — and restoring it would run it through the node
    /// writer. Assets stay out of that listing.
    #[test]
    fn trashed_assets_do_not_appear_in_the_node_trash_listing() {
        let vault_dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let vault = vault_dir.path().to_string_lossy().to_string();
        let db = DbBridge::new_in_memory_full().unwrap();

        seed_file(vault_dir.path(), "QuickCaps/a.md", "một ghi chú");
        seed_row(&db, "QuickCaps/a.md");
        apply_trash(&db, &vault, "QuickCaps/a.md").unwrap();

        let asset = source_dir.path().join("ảnh.jpg");
        std::fs::write(&asset, "bytes").unwrap();
        trash_asset(&vault, &asset).unwrap();

        let listed = list_trash(vault).unwrap();
        assert_eq!(listed.len(), 1, "only the node belongs in this listing");
        assert_eq!(listed[0].original_path, "QuickCaps/a.md");
    }

    /// Deleting two caps that once had the same name must not have the
    /// second quietly destroy the first.
    #[test]
    fn never_overwrites_something_already_in_the_trash() {
        let dir = tempfile::tempdir().unwrap();
        seed_file(dir.path(), ".trash/QuickCaps/a.md", "bản đầu");

        let target = free_trash_path(dir.path(), "QuickCaps/a.md");

        assert_eq!(target, dir.path().join(".trash/QuickCaps/a (1).md"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".trash/QuickCaps/a.md")).unwrap(),
            "bản đầu",
            "the file already in the trash must survive"
        );
    }

    #[test]
    fn keeps_counting_past_the_first_collision() {
        let dir = tempfile::tempdir().unwrap();
        seed_file(dir.path(), ".trash/QuickCaps/a.md", "một");
        seed_file(dir.path(), ".trash/QuickCaps/a (1).md", "hai");

        assert_eq!(
            free_trash_path(dir.path(), "QuickCaps/a.md"),
            dir.path().join(".trash/QuickCaps/a (2).md")
        );
    }

    /// The index has to forget it immediately, or the cap keeps appearing in
    /// lists and in search until the next full scan.
    #[test]
    fn drops_the_row_and_the_search_entry() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_row(&db, "QuickCaps/a.md");
        db.upsert_search_entry(
            "QuickCaps/a.md",
            "quickcap",
            "a cap",
            "",
            "nội dung",
            "{}",
            None,
            "2026-01-01",
            "QuickCaps/a.md",
        );

        db.delete_node("QuickCaps/a.md").unwrap();
        db.delete_search_entry("QuickCaps/a.md");

        assert!(db.get_nodes_by_type("quickcap").unwrap().is_empty());
        let hits: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM search_index WHERE item_id = ?1",
                ["QuickCaps/a.md"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 0);
    }

    /// Both directions of the age test, without backdating a file: a
    /// just-deleted cap survives a 30-day purge, and the same cap does not
    /// survive one with no grace period at all.
    #[test]
    fn purge_spares_the_recent_and_takes_the_expired() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        seed_file(dir.path(), ".trash/QuickCaps/fresh.md", "mới");

        assert_eq!(purge_trash(vault.clone(), 30).unwrap(), 0);
        assert!(dir.path().join(".trash/QuickCaps/fresh.md").exists());

        assert_eq!(purge_trash(vault, 0).unwrap(), 1);
        assert!(!dir.path().join(".trash/QuickCaps/fresh.md").exists());
    }

    #[test]
    fn purge_is_fine_with_no_trash_at_all() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            purge_trash(dir.path().to_string_lossy().to_string(), 30).unwrap(),
            0
        );
    }
}


#[cfg(test)]
mod trash_listing_tests {
    use super::*;
    use std::io::Write;

    fn vault_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        for (rel, contents) in files {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
            let mut f = std::fs::File::create(&path).expect("create");
            f.write_all(contents.as_bytes()).expect("write");
        }
        dir
    }

    #[test]
    fn an_empty_vault_has_an_empty_trash() {
        let dir = tempfile::tempdir().expect("temp dir");
        let listed = list_trash(dir.path().to_string_lossy().to_string()).expect("list");
        assert!(listed.is_empty());
    }

    /// A task's filename is a UUID, so the title has to come out of the file
    /// or the list is a column of hex nobody can act on.
    #[test]
    fn an_entry_reports_the_title_from_inside_the_file() {
        let dir = vault_with(&[(
            ".trash/Tasks/abc.md",
            "---\ntitle: Buy milk\ntype: task\n---\nbody\n",
        )]);
        let listed = list_trash(dir.path().to_string_lossy().to_string()).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Buy milk");
        assert_eq!(listed[0].node_type, "task");
    }

    #[test]
    fn an_entry_reports_where_it_came_from_and_where_it_is() {
        let dir = vault_with(&[(".trash/Tasks/abc.md", "---\ntitle: T\ntype: task\n---\n")]);
        let listed = list_trash(dir.path().to_string_lossy().to_string()).expect("list");
        assert_eq!(listed[0].original_path, "Tasks/abc.md");
        assert_eq!(listed[0].trash_path, ".trash/Tasks/abc.md");
    }

    #[test]
    fn a_file_that_does_not_parse_still_appears_named_after_itself() {
        let dir = vault_with(&[(".trash/Notes/broken.md", "\u{0}not really markdown")]);
        let listed = list_trash(dir.path().to_string_lossy().to_string()).expect("list");
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].title.is_empty());
    }

    #[test]
    fn everything_in_the_trash_is_listed_however_deep() {
        let dir = vault_with(&[
            (".trash/Tasks/a.md", "---\ntitle: A\ntype: task\n---\n"),
            (".trash/Notes/work/q3/b.md", "---\ntitle: B\ntype: note\n---\n"),
        ]);
        let listed = list_trash(dir.path().to_string_lossy().to_string()).expect("list");
        assert_eq!(listed.len(), 2);
    }

    #[test]
    fn a_restore_target_is_the_original_path_when_it_is_free() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = restore_target(dir.path(), ".trash/Tasks/abc.md").expect("target");
        assert_eq!(target, "Tasks/abc.md");
    }

    /// Restoring must never overwrite a file that has taken the old name since.
    #[test]
    fn a_restore_target_steps_aside_for_a_file_already_there() {
        let dir = vault_with(&[("Tasks/abc.md", "something else")]);
        let target = restore_target(dir.path(), ".trash/Tasks/abc.md").expect("target");
        assert_ne!(target, "Tasks/abc.md");
        assert!(target.starts_with("Tasks/"));
    }

    #[test]
    fn a_path_outside_the_trash_is_not_restorable() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(restore_target(dir.path(), "Tasks/abc.md").is_err());
    }

    #[test]
    fn removing_something_that_is_not_there_is_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let result = delete_trash_entry(
            dir.path().to_string_lossy().to_string(),
            ".trash/Tasks/gone.md".to_string(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn removing_an_entry_takes_the_file_with_it() {
        let dir = vault_with(&[(".trash/Tasks/a.md", "---\ntitle: A\ntype: task\n---\n")]);
        let vault = dir.path().to_string_lossy().to_string();
        delete_trash_entry(vault.clone(), ".trash/Tasks/a.md".to_string()).expect("delete");
        assert!(list_trash(vault).expect("list").is_empty());
    }
}
