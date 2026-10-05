use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::db::{DbBridge, DbState};
use crate::error::{logged, AppError, AppResult};
use crate::models::node::NodeMetadata;
use crate::models::whiteboard::WhiteboardMetadata;
use crate::path_utils;
use crate::utils::node_parser::parse_file_to_node;

/// Describe an indexed board the way the frontend asks for it.
///
/// A board is an ordinary node; this is a view of one, not a second copy. The
/// id and path are the same string — the vault-relative path of the file — and
/// were the same string back when they were separate columns too.
fn board_from_node(node: &NodeMetadata) -> WhiteboardMetadata {
    let tags = node
        .properties
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    WhiteboardMetadata {
        id: node.id.clone(),
        path: node.id.clone(),
        title: node.title.clone(),
        tags,
        content: node.content.clone(),
        created_at: node.created_at.clone(),
        updated_at: node.updated_at.clone(),
    }
}

/// Read a board file and record it as a node, index included.
///
/// Parsing goes through the same `parse_file_to_node` every other file in the
/// vault goes through, so a board indexed here and the same board indexed by a
/// vault scan cannot end up describing themselves differently.
pub(crate) fn index_board(db: &DbBridge, vault_path: &str, abs_path: &Path) -> Option<WhiteboardMetadata> {
    let node = parse_file_to_node(vault_path, abs_path)?;
    let board = board_from_node(&node);

    logged("index whiteboard", &node.id, db.upsert_node(&node));
    db.upsert_search_entry(
        &node.id,
        "whiteboard",
        &node.title,
        &board.tags.join(" "),
        &node.content,
        "",
        None,
        &node.updated_at,
        &node.id,
    );

    let resolver = crate::commands::nodes::build_resolver(db);
    crate::commands::nodes::sync_node_edges(db, &node, &resolver);

    Some(board)
}

/// Forget a board entirely: the row, its links, and its search entry.
fn forget_board(db: &DbBridge, rel_path: &str) {
    logged("drop whiteboard", rel_path, db.delete_node(rel_path));
    logged(
        "clear links",
        rel_path,
        db.delete_node_edges_by_source(rel_path),
    );
    db.delete_search_entry(rel_path);
}

/// List every board, indexing the ones that changed since they were last seen.
///
/// Boards live under `Whiteboards/`, in subfolders too. A board the user moved
/// elsewhere in the vault is still a board — the vault scan indexes it by its
/// suffix — and stays listed for as long as its file exists.
///
/// This runs on every change the vault watcher reports, the app's own saves
/// included, so a board whose file has not changed since it was indexed is
/// listed from its row instead of being read, parsed and re-linked again:
/// re-indexing rebuilds the link resolver from every node, and doing that per
/// board, per save, held the database for the whole scan.
#[tauri::command]
pub fn scan_whiteboards(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<Vec<WhiteboardMetadata>> {
    let wb_dir = Path::new(&vault_path).join("Whiteboards");
    if !wb_dir.exists() {
        fs::create_dir_all(&wb_dir)?;
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let known = db.get_all_whiteboard_timestamps().unwrap_or_default();
    let mut boards = Vec::new();
    let mut listed = std::collections::HashSet::new();

    for entry in WalkDir::new(&wb_dir)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let is_board = path
            .file_name()
            .map(|n| n.to_string_lossy().ends_with(".whiteboard.json"))
            .unwrap_or(false);
        if !is_board {
            continue;
        }

        let rel = path_utils::to_relative(path, &vault_path);
        let modified = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        let unchanged = modified.is_some() && known.get(&rel).copied() == modified;
        let board = match unchanged {
            true => db.get_node(&rel).ok().flatten().map(|node| board_from_node(&node)),
            false => None,
        }
        .or_else(|| index_board(&db, &vault_path, path));

        listed.insert(rel);
        if let Some(board) = board {
            boards.push(board);
        }
    }

    // Boards indexed from elsewhere in the vault: listed while their file is
    // there, forgotten once it is not.
    for id in known.keys() {
        if listed.contains(id) {
            continue;
        }
        let exists = path_utils::resolve_safe_path(&vault_path, id)
            .map(|p| p.is_file())
            .unwrap_or(false);
        if !exists {
            forget_board(&db, id);
        } else if let Ok(Some(node)) = db.get_node(id) {
            boards.push(board_from_node(&node));
        }
    }

    boards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(boards)
}

/// A file name for a new board that no other board has, reserved.
///
/// Boards are named by the millisecond they were made in, and two made in the
/// same one — the app and Syn, a project resource and "keep as board" — used
/// to get the same name, and the second replaced the first. The name is taken
/// by creating the file only if it is not there, so two makers at once cannot
/// both have it.
pub(crate) fn claim_board_file(dir: &Path, stamp: u128) -> AppResult<std::path::PathBuf> {
    for n in 0..1000u32 {
        let name = if n == 0 { format!("whiteboard-{stamp}.whiteboard.json") } else { format!("whiteboard-{stamp}-{n}.whiteboard.json") };
        let path = dir.join(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::General("No free name for a new board.".into()))
}

#[tauri::command]
pub fn create_whiteboard(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    title: String,
    tags: Vec<String>,
    content: String,
) -> AppResult<WhiteboardMetadata> {
    let wb_dir = Path::new(&vault_path).join("Whiteboards");
    if !wb_dir.exists() {
        fs::create_dir_all(&wb_dir)?;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| crate::error::AppError::General(format!("System time error: {}", e)))?
        .as_millis();
    let abs_path = claim_board_file(&wb_dir, timestamp)?;

    path_utils::write_atomic(&abs_path, content.as_bytes())?;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    index_board(&db, &vault_path, &abs_path).ok_or_else(|| {
        crate::error::AppError::General(
            "The new whiteboard was written but could not be read back".to_string(),
        )
    })
    .map(|mut board| {
        // The caller's title and tags are already inside the file it handed us;
        // these only matter if the file did not carry them.
        if board.title.is_empty() {
            board.title = title;
        }
        if board.tags.is_empty() {
            board.tags = tags;
        }
        board
    })
}

// ─── One writer at a time ──────────────────────────────────────

/// A board has several writers — the app, the board shown in a note, the pane
/// beside a conversation, Syn, sync — and each reads the file, changes it and
/// writes it back. Two of those overlapping wrote the second over the first,
/// and the first one's work was gone. So every writer in Rust holds this lock
/// from its read to its write, and a writer in the webview, which cannot hold
/// a lock across the bridge, says which version it read (`expected`) and is
/// refused if the file is no longer that one.
static BOARD_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, std::sync::Arc<std::sync::Mutex<()>>>>,
> = std::sync::LazyLock::new(Default::default);

/// The lock for one board file. The same file reached by two spellings of its
/// path gets the same lock.
pub fn board_lock(path: &Path) -> std::sync::Arc<std::sync::Mutex<()>> {
    let key = match (path.parent().and_then(|dir| fs::canonicalize(dir).ok()), path.file_name()) {
        (Some(dir), Some(name)) => dir.join(name),
        _ => path.to_path_buf(),
    };
    let mut locks = BOARD_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks.entry(key).or_default().clone()
}

/// What a writer quotes back to say which version of a board it read.
pub fn board_version(bytes: &[u8]) -> String {
    crate::sync::utils::sha256_hex(bytes)
}

/// A board's file, inside the vault. These commands read and write boards and
/// nothing else: a note or a settings file is not theirs to touch.
fn board_path(vault_path: &str, path: &str) -> AppResult<std::path::PathBuf> {
    if !path.ends_with(".whiteboard.json") {
        return Err(AppError::InvalidPath(format!("not a board: {path}")));
    }
    path_utils::resolve_safe_path(vault_path, path)
}

/// Write a board only if it is still the version `expected` names, keeping
/// the version it replaces in `history` when one is due (see `keep_version`).
fn write_board_if(
    abs_path: &Path,
    content: &[u8],
    expected: Option<&str>,
    history: Option<&Path>,
    keep_now: bool,
) -> AppResult<()> {
    let lock = board_lock(abs_path);
    let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
    let before = match fs::read(abs_path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    if let Some(want) = expected {
        match &before {
            None => return Err(AppError::Stale("the board is no longer there".into())),
            Some(bytes) if board_version(bytes) != want => {
                return Err(AppError::Stale("the board was written by someone else".into()))
            }
            _ => {}
        }
    }
    if let (Some(dir), Some(bytes)) = (history, &before) {
        // History is a convenience: failing to keep a version is not a reason
        // to refuse the save.
        if let Err(e) = keep_version(dir, bytes, chrono::Utc::now().timestamp_millis(), keep_now) {
            log::warn!("Could not keep a version of {}: {e}", abs_path.display());
        }
    }
    path_utils::write_atomic(abs_path, content)?;
    Ok(())
}

// ─── Versions ──────────────────────────────────────────────────

/// A board's earlier versions, on this device.
///
/// A note's history lives in its CRDT log; a board's does not survive sync,
/// which replaces the board's document with the merged copy. So a board keeps
/// copies of itself instead: before the first save of each sitting — a save
/// more than [`SITTING_MS`] after the last version kept — the file as it was
/// goes into the app's own data folder. Not into the vault: these are this
/// device's safety net, not something to sync to every other one.
const SITTING_MS: i64 = 10 * 60 * 1000;
/// How many versions of one board are kept; the oldest go first.
const KEEP_VERSIONS: usize = 60;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BoardVersion {
    /// When it was kept, in Unix milliseconds — also how it is asked for.
    pub at: i64,
    pub size: u64,
}

/// Keep the board as it is now before something other than the open board —
/// Syn, a sync — writes over it. The app's own saves keep their versions in
/// `write_board_if`; these writers went straight to the file, so what Syn
/// removed, or a merge dropped, could not be got back from the history.
///
/// `now` keeps it whatever the time since the last version (Syn's edits: each
/// is something to be able to undo); otherwise once a sitting, as saves do.
/// Called under the board's lock. Failing to keep a version never stops the
/// write: the history is a safety net, not a gate.
pub(crate) fn keep_before_write<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    vault_path: &str,
    abs_path: &Path,
    now: bool,
) {
    use tauri::Manager;
    if let Ok(data) = app.path().app_data_dir() {
        keep_before_write_in(&data, vault_path, abs_path, now, chrono::Utc::now().timestamp_millis());
    }
}

/// `keep_before_write`, given where the app keeps its data.
fn keep_before_write_in(app_data: &Path, vault_path: &str, abs_path: &Path, now: bool, now_ms: i64) {
    let Ok(before) = fs::read(abs_path) else { return };
    let rel = path_utils::to_relative(abs_path, vault_path);
    let dir = history_dir(app_data, vault_path, &rel);
    if let Err(e) = keep_version(&dir, &before, now_ms, now) {
        log::warn!("Could not keep a version of {}: {e}", abs_path.display());
    }
}

fn history_dir(app_data: &Path, vault_path: &str, rel_path: &str) -> std::path::PathBuf {
    let short = |s: &str| board_version(s.as_bytes())[..16].to_string();
    app_data.join("board-history").join(short(vault_path)).join(short(rel_path))
}

/// The versions kept in `dir`, newest first.
fn versions_in(dir: &Path) -> Vec<BoardVersion> {
    let mut out: Vec<BoardVersion> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let at = name.strip_suffix(".json")?.parse::<i64>().ok()?;
            Some(BoardVersion { at, size: entry.metadata().map(|m| m.len()).unwrap_or(0) })
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}

/// Keep `bytes` as a version, when a sitting has begun since the last one —
/// or now, whatever the time, when `now` says so: the write that puts an
/// earlier version back keeps the board it replaces, which may be minutes of
/// work since the last version kept.
fn keep_version(dir: &Path, bytes: &[u8], now_ms: i64, now: bool) -> std::io::Result<()> {
    let kept = versions_in(dir);
    if let Some(newest) = kept.first() {
        if !now && now_ms - newest.at < SITTING_MS {
            return Ok(());
        }
        // The same board as the newest version: nothing new to keep.
        if fs::read(dir.join(format!("{}.json", newest.at))).is_ok_and(|was| was == bytes) {
            return Ok(());
        }
    }
    path_utils::write_atomic(&dir.join(format!("{now_ms}.json")), bytes)?;
    for old in kept.iter().skip(KEEP_VERSIONS - 1) {
        let _ = fs::remove_file(dir.join(format!("{}.json", old.at)));
    }
    Ok(())
}

#[tauri::command]
pub fn list_board_versions(
    app_handle: tauri::AppHandle,
    vault_path: String,
    path: String,
) -> AppResult<Vec<BoardVersion>> {
    use tauri::Manager;
    board_path(&vault_path, &path)?;
    let data = app_handle.path().app_data_dir().map_err(|e| AppError::General(e.to_string()))?;
    Ok(versions_in(&history_dir(&data, &vault_path, &path)))
}

#[tauri::command]
pub fn read_board_version(
    app_handle: tauri::AppHandle,
    vault_path: String,
    path: String,
    at: i64,
) -> AppResult<String> {
    use tauri::Manager;
    board_path(&vault_path, &path)?;
    let data = app_handle.path().app_data_dir().map_err(|e| AppError::General(e.to_string()))?;
    Ok(fs::read_to_string(history_dir(&data, &vault_path, &path).join(format!("{at}.json")))?)
}

// A command's arguments are what the webview sends, named; bundling them
// into a struct would only rename them on the wire.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn update_whiteboard(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
    title: String,
    tags: Vec<String>,
    content: String,
    expected: Option<String>,
    keep: Option<bool>,
) -> AppResult<()> {
    let _ = (title, tags); // carried inside `content`, which is the file itself
    let abs_path = board_path(&vault_path, &path)?;
    let history = {
        use tauri::Manager;
        app_handle.path().app_data_dir().ok().map(|data| history_dir(&data, &vault_path, &path))
    };
    write_board_if(&abs_path, content.as_bytes(), expected.as_deref(), history.as_deref(), keep.unwrap_or(false))?;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    index_board(&db, &vault_path, &abs_path);

    Ok(())
}

/// Delete a board the way every other node is deleted: by moving it aside.
///
/// This used to call `fs::remove_file`, which is the one delete in the app
/// that could not be taken back — a board is hours of work behind a single
/// hover-revealed icon, and notes, captures and tasks all go to `.trash/`
/// instead. Returns where the file went, so the caller can say so.
#[tauri::command]
pub fn delete_whiteboard(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
) -> AppResult<String> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    // The same three deletes `forget_board` does — row, links, search entry —
    // and the move itself, in the order that has been tested.
    crate::commands::trash::apply_trash(&db, &vault_path, &path)
}

#[tauri::command]
pub fn read_whiteboard(
    _app_handle: tauri::AppHandle,
    vault_path: String,
    path: String,
) -> AppResult<String> {
    let abs_path = board_path(&vault_path, &path)?;
    Ok(fs::read_to_string(&abs_path)?)
}

// ─── Libraries ────────────────────────────────────────────────

/// Where a vault keeps its shape libraries: beside its boards, as files the
/// user can see, copy and sync — `Whiteboards/Libraries/<name>.boardlib.json`.
const LIBRARY_DIR: &str = "Whiteboards/Libraries";
const LIBRARY_SUFFIX: &str = ".boardlib.json";

fn library_path(vault_path: &str, path: &str) -> AppResult<std::path::PathBuf> {
    let inside = path
        .strip_prefix(LIBRARY_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|name| name.ends_with(LIBRARY_SUFFIX) && !name.contains('/') && !name.contains('\\'));
    if inside.is_none() {
        return Err(AppError::InvalidPath(format!("not a library: {path}")));
    }
    path_utils::resolve_safe_path(vault_path, path)
}

#[derive(serde::Serialize)]
pub struct LibraryFile {
    pub path: String,
    pub content: String,
}

/// Every shape library in the vault, as the files hold them.
#[tauri::command]
pub fn list_board_libraries(vault_path: String) -> AppResult<Vec<LibraryFile>> {
    let dir = Path::new(&vault_path).join(LIBRARY_DIR);
    let Ok(entries) = fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(LIBRARY_SUFFIX) || !entry.path().is_file() {
            continue;
        }
        match fs::read_to_string(entry.path()) {
            Ok(content) => out.push(LibraryFile { path: format!("{LIBRARY_DIR}/{name}"), content }),
            Err(e) => log::warn!("Could not read the library {name}: {e}"),
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Write a shape library, whole. Only files in the libraries folder.
#[tauri::command]
pub fn write_board_library(vault_path: String, path: String, content: String) -> AppResult<()> {
    let abs = library_path(&vault_path, &path)?;
    if let Some(parent) = abs.parent() {
        fs::create_dir_all(parent)?;
    }
    path_utils::write_atomic(&abs, content.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What another writer is about to replace goes into the history the
    /// History panel lists: the panel asks by the board's vault path.
    #[test]
    fn a_write_from_elsewhere_keeps_the_board_it_replaces() {
        let vault_dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let vault = vault_dir.path().to_string_lossy().to_string();
        fs::create_dir(vault_dir.path().join("Whiteboards")).unwrap();
        let file = vault_dir.path().join("Whiteboards").join("b.whiteboard.json");
        fs::write(&file, b"before Syn").unwrap();

        keep_before_write_in(data.path(), &vault, &file, true, 5_000);
        // Syn again a minute later: kept again, each edit can be taken back.
        fs::write(&file, b"after one edit").unwrap();
        keep_before_write_in(data.path(), &vault, &file, true, 65_000);

        let dir = history_dir(data.path(), &vault, "Whiteboards/b.whiteboard.json");
        assert_eq!(versions_in(&dir).iter().map(|v| v.at).collect::<Vec<_>>(), vec![65_000, 5_000]);
        assert_eq!(fs::read(dir.join("5000.json")).unwrap(), b"before Syn");

        // A sync merge keeps once a sitting, like a save.
        keep_before_write_in(data.path(), &vault, &file, false, 70_000);
        assert_eq!(versions_in(&dir).len(), 2);
    }

    #[test]
    fn two_boards_made_in_one_millisecond_get_two_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = claim_board_file(dir.path(), 42).unwrap();
        let b = claim_board_file(dir.path(), 42).unwrap();
        assert_ne!(a, b);
        assert!(a.exists() && b.exists());
    }

    #[test]
    fn libraries_are_read_and_written_only_in_their_folder() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        assert!(list_board_libraries(vault.clone()).unwrap().is_empty());
        write_board_library(vault.clone(), "Whiteboards/Libraries/Mine.boardlib.json".into(), "{}".into()).unwrap();
        let found = list_board_libraries(vault.clone()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "Whiteboards/Libraries/Mine.boardlib.json");
        for wrong in [
            "Notes/a.boardlib.json",
            "Whiteboards/Libraries/a.md",
            "Whiteboards/Libraries/../../x.boardlib.json",
            "Whiteboards/Libraries/sub/a.boardlib.json",
            "Whiteboards/LibrariesX/a.boardlib.json",
        ] {
            assert!(write_board_library(vault.clone(), wrong.into(), "{}".into()).is_err(), "{wrong}");
        }
    }

    /// A write quoting the version it read goes through; one quoting an older
    /// version is refused and leaves the file as it is.
    #[test]
    fn a_write_based_on_an_old_read_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.whiteboard.json");
        fs::write(&path, b"one").unwrap();
        let read = board_version(b"one");

        write_board_if(&path, b"two", Some(&read), None, false).expect("still the version read");
        let err = write_board_if(&path, b"three", Some(&read), None, false).expect_err("written since");
        assert!(matches!(err, AppError::Stale(_)));
        assert_eq!(fs::read(&path).unwrap(), b"two");

        // No version quoted: written, as before.
        write_board_if(&path, b"four", None, None, false).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"four");
    }

    /// A version is kept at the start of each sitting, not at every save, and
    /// only so many are kept.
    #[test]
    fn versions_are_kept_once_a_sitting() {
        let dir = tempfile::tempdir().unwrap();
        let h = dir.path().join("h");
        keep_version(&h, b"v1", 1_000, false).unwrap();
        keep_version(&h, b"v2", 1_000 + 60_000, false).unwrap(); // same sitting
        keep_version(&h, b"v3", 1_000 + SITTING_MS + 1, false).unwrap(); // next one
        let kept = versions_in(&h);
        assert_eq!(kept.iter().map(|v| v.at).collect::<Vec<_>>(), vec![1_000 + SITTING_MS + 1, 1_000]);
        assert_eq!(fs::read(h.join("1000.json")).unwrap(), b"v1");

        for i in 0..(KEEP_VERSIONS as i64 + 5) {
            keep_version(&h, format!("x{i}").as_bytes(), 10_000_000 + i * (SITTING_MS + 1), false).unwrap();
        }
        assert_eq!(versions_in(&h).len(), KEEP_VERSIONS);
    }

    /// Restoring keeps the board it replaces, however soon after the last version.
    #[test]
    fn a_restore_keeps_what_it_replaces_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let h = dir.path().join("h");
        keep_version(&h, b"morning", 1_000, false).unwrap();
        keep_version(&h, b"nine minutes of work", 1_000 + 9 * 60_000, true).unwrap();
        assert_eq!(versions_in(&h).len(), 2);
    }

    #[test]
    fn only_boards_are_read_or_written() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        assert!(matches!(board_path(&vault, "Notes/a.md"), Err(AppError::InvalidPath(_))));
    }

    #[test]
    fn one_file_has_one_lock_however_its_path_is_spelled() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("W")).unwrap();
        let plain = dir.path().join("W").join("b.whiteboard.json");
        let roundabout = dir.path().join("W").join("..").join("W").join("b.whiteboard.json");
        assert!(std::sync::Arc::ptr_eq(&board_lock(&plain), &board_lock(&roundabout)));
    }
}
