use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use crate::db::{DbBridge, DbState};
use crate::error::{logged, AppResult};
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
fn index_board(db: &DbBridge, vault_path: &str, abs_path: &Path) -> Option<WhiteboardMetadata> {
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

/// Scan the Whiteboards/ directory and index every board found there.
#[tauri::command]
pub fn scan_whiteboards(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<Vec<WhiteboardMetadata>> {
    let mut boards = Vec::new();
    let wb_dir = Path::new(&vault_path).join("Whiteboards");
    if !wb_dir.exists() {
        fs::create_dir_all(&wb_dir)?;
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut current_disk_files = std::collections::HashSet::new();

    for entry in WalkDir::new(&wb_dir)
        .max_depth(1)
        .into_iter()
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

        current_disk_files.insert(path_utils::to_relative(path, &vault_path));

        if let Some(board) = index_board(&db, &vault_path, path) {
            boards.push(board);
        }
    }

    // Purge boards whose file is gone.
    if let Ok(existing) = db.get_all_whiteboard_timestamps() {
        for id in existing.keys() {
            if !current_disk_files.contains(id) {
                forget_board(&db, id);
            }
        }
    }

    boards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(boards)
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
    let abs_path = wb_dir.join(format!("whiteboard-{}.whiteboard.json", timestamp));

    fs::write(&abs_path, &content)?;

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

#[tauri::command]
pub fn update_whiteboard(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
    title: String,
    tags: Vec<String>,
    content: String,
) -> AppResult<()> {
    let _ = (title, tags); // carried inside `content`, which is the file itself
    let abs_path = path_utils::resolve_safe_path(&vault_path, &path)?;
    fs::write(&abs_path, &content)?;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    index_board(&db, &vault_path, &abs_path);

    Ok(())
}

#[tauri::command]
pub fn delete_whiteboard(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
) -> AppResult<()> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &path)?;
    fs::remove_file(&abs_path)?;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    forget_board(&db, &path);

    Ok(())
}

#[tauri::command]
pub fn read_whiteboard(
    _app_handle: tauri::AppHandle,
    vault_path: String,
    path: String,
) -> AppResult<String> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &path)?;
    Ok(fs::read_to_string(&abs_path)?)
}
