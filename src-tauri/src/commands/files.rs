use std::path::Path;
use std::process::Command;
use std::time::SystemTime;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::db::DbState;
use crate::error::{logged, AppError, AppResult};
use crate::models::file::{DuplicateGroup, FileMetadata, FileSource};
use crate::path_utils;

/// Register a folder as a source. Registering only — the caller asks for the
/// scan.
///
/// This used to spawn one itself, while the front end also called
/// `scan_directory` the moment it returned. Two walks of the same tree started
/// within milliseconds of each other, each taking the database lock the other
/// was waiting on, for one click on "add folder".
#[tauri::command]
pub fn add_file_source(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    path: String,
    name: String,
) -> AppResult<FileSource> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let source = FileSource {
        id: Uuid::new_v4().to_string(),
        path,
        name,
    };
    db.upsert_file_source(&source)?;
    Ok(source)
}

/// The registered sources, with the vault's own assets folder among them.
///
/// A read, despite the write hiding in it: registering `assets/` the first time
/// anyone asks is how it comes to be listed at all. What it no longer does is
/// scan — the caller loading this list goes on to `reindex_sources`, which
/// covers `assets/` whether or not it is registered, so the scan started here
/// was a second walk of a tree already being walked.
#[tauri::command]
pub fn get_file_sources(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<Vec<FileSource>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut sources = db.get_all_file_sources()?;

    let assets_path = std::path::Path::new(&vault_path).join("assets");
    if !assets_path.exists() {
        return Ok(sources);
    }

    let assets_path_str = assets_path.to_string_lossy().to_string();
    if !sources.iter().any(|s| s.path == assets_path_str) {
        let source = FileSource {
            id: Uuid::new_v4().to_string(),
            path: assets_path_str,
            name: "Vault Assets".to_string(),
        };
        db.upsert_file_source(&source)?;
        sources.push(source);
    }

    Ok(sources)
}

/// Stop tracking a folder, and forget what was in it.
///
/// The files themselves are untouched — the user is unlinking a folder, not
/// asking us to delete their photos. What goes is the index.
///
/// Dropping the rows matters because nothing else ever will. The garbage
/// collector in `reindex_sources` only considers paths under a folder that is
/// still registered, so a file node whose source has just been removed is
/// invisible to it forever: it would sit in the list, unreachable and
/// unrefreshable, until the vault was rebuilt from scratch.
#[tauri::command]
pub fn remove_file_source(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    source_id: String,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // Read the path before the row goes, or there is nothing left to match on.
    let source_path = db
        .get_all_file_sources()
        .unwrap_or_default()
        .into_iter()
        .find(|s| s.id == source_id)
        .map(|s| s.path);

    db.delete_file_source(&source_id)?;

    let Some(source_path) = source_path else {
        return Ok(());
    };

    let dropped = drop_indexed_files_under(&db, &source_path)?;
    if dropped > 0 {
        log::info!("remove_file_source: dropped {dropped} file node(s) under {source_path}");
    }
    Ok(())
}

/// Forget every indexed file that lived under a folder. Returns how many went.
fn drop_indexed_files_under(
    db: &crate::db::DbBridge,
    source_path: &str,
) -> AppResult<usize> {
    let mut dropped = 0;
    for node in db.get_nodes_by_type("file").unwrap_or_default() {
        let Some(path) = node.properties.get("path").and_then(|v| v.as_str()) else {
            continue;
        };
        if !path.starts_with(source_path) {
            continue;
        }
        forget_file(db, &node.id.clone(), Some(&node))?;
        dropped += 1;
    }
    Ok(dropped)
}

/// Copy files into the vault's assets folder and index them.
///
/// The indexing goes through the same batch path a scan uses, so an imported
/// file gets a content identity exactly like every other file — which is what
/// lets importing the same photo twice produce one item rather than two.
#[tauri::command]
pub fn import_files(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    file_paths: Vec<String>,
) -> AppResult<u32> {
    let import_dir = Path::new(&vault_path).join("assets");
    if !import_dir.exists() {
        std::fs::create_dir_all(&import_dir)?;
    }

    let mut copied: Vec<Discovered> = Vec::new();
    for src_path in &file_paths {
        let source = Path::new(src_path);
        if !source.exists() || !source.is_file() {
            continue;
        }

        let original_name = source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let extension = source
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let stem = source
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Keep the original name readable, but never overwrite what is there.
        let dest_name = if import_dir.join(&original_name).exists() {
            format!("{}_{}.{}", stem, Uuid::new_v4().simple(), extension)
        } else {
            original_name
        };
        let dest = import_dir.join(&dest_name);

        if std::fs::copy(source, &dest).is_err() {
            continue;
        }

        let meta = std::fs::metadata(&dest).ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let created = meta
            .as_ref()
            .and_then(|m| m.created().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let created_dt: chrono::DateTime<chrono::Local> = created.into();
        let modified_dt: chrono::DateTime<chrono::Local> = modified.into();

        copied.push(Discovered {
            abs_path: dest.to_string_lossy().to_string(),
            filename: dest_name,
            extension,
            size,
            mtime_ms: modified
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            modified_at: modified_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        });
    }

    if copied.is_empty() {
        return Ok(0);
    }

    let resolver = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        crate::commands::nodes::build_resolver(&db)
    };
    for chunk in copied.chunks(SCAN_BATCH) {
        commit_batch(&state, chunk, &resolver)?;
    }

    Ok(copied.len() as u32)
}

/// Index one folder, without Tauri's command plumbing.
///
/// The command above needs a `tauri::State`, which a test cannot produce for
/// the real runtime. This is the same walk, reachable from one.
#[cfg(test)]
pub(crate) fn scan_source_for_test(state: &DbState, source_path: &str) -> AppResult<Vec<String>> {
    scan_source(state, source_path, None, |_| {})
}

/// One file as the walk found it, before anything inside it has been read.
struct Discovered {
    abs_path: String,
    filename: String,
    extension: String,
    size: u64,
    mtime_ms: i64,
    created_at: String,
    modified_at: String,
}

/// How many files are indexed between one commit and the next.
///
/// The number is a compromise between two costs that pull opposite ways. Each
/// batch takes the database lock, so a small batch means the rest of the app —
/// notes, tasks, chat — gets frequent chances to run; a large batch means fewer
/// transactions and less overhead. Two hundred keeps a lock held for a few
/// milliseconds at a time, which is short enough that typing in a note during a
/// scan of a photo library feels like nothing is happening at all.
const SCAN_BATCH: usize = 200;

/// Lets a scan be stopped, and stops two starting at once.
#[derive(Default)]
pub struct ScanControl {
    cancel: std::sync::atomic::AtomicBool,
    running: std::sync::atomic::AtomicBool,
}

impl ScanControl {
    pub fn request_cancel(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Claim the right to scan, or report that somebody else already has it.
    fn begin(&self) -> bool {
        let taken = self
            .running
            .swap(true, std::sync::atomic::Ordering::SeqCst);
        if !taken {
            self.cancel
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
        !taken
    }

    fn finish(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Clone, serde::Serialize)]
pub struct ScanProgress {
    pub source: String,
    pub indexed: usize,
    pub hashed: usize,
    pub cancelled: bool,
}

/// Walk a folder and record what is in it, without holding anything for long.
///
/// The shape of this function is the point. The old one took the database lock,
/// walked an entire directory tree, and released it at the end — so indexing a
/// photo library froze every other part of the app for as long as it took. Here
/// the walk holds nothing, hashing holds nothing, and the lock is taken only to
/// commit a batch of at most `SCAN_BATCH` files.
///
/// Returns the paths seen, which is what the caller needs to work out what has
/// gone.
fn scan_source(
    state: &DbState,
    source_path: &str,
    control: Option<&ScanControl>,
    mut on_batch: impl FnMut(ScanProgress),
) -> AppResult<Vec<String>> {
    let root = Path::new(source_path);
    if !root.exists() || !root.is_dir() {
        return Err(AppError::InvalidPath(
            "Source path is invalid or not a directory".to_string(),
        ));
    }

    // People and projects are not created by this walk, so one snapshot answers
    // every link it resolves. Rebuilding it per file is what made a scan
    // quadratic in the size of the vault.
    let resolver = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        crate::commands::nodes::build_resolver(&db)
    };

    let mut seen = Vec::new();
    let mut hashed = 0usize;
    let mut batch: Vec<Discovered> = Vec::with_capacity(SCAN_BATCH);

    let mut walker = WalkDir::new(root).into_iter().filter_map(|e| e.ok());
    loop {
        let cancelled = control.is_some_and(|c| c.cancelled());
        if !cancelled {
            while batch.len() < SCAN_BATCH {
                let Some(entry) = walker.next() else { break };
                if !entry.file_type().is_file() {
                    continue;
                }
                let Some(found) = describe(&entry) else { continue };
                seen.push(found.abs_path.clone());
                batch.push(found);
            }
        }

        if batch.is_empty() {
            if cancelled {
                on_batch(ScanProgress {
                    source: source_path.to_string(),
                    indexed: seen.len(),
                    hashed,
                    cancelled: true,
                });
            }
            break;
        }

        hashed += commit_batch(state, &batch, &resolver)?;
        batch.clear();

        on_batch(ScanProgress {
            source: source_path.to_string(),
            indexed: seen.len(),
            hashed,
            cancelled,
        });

        if cancelled {
            break;
        }
    }

    Ok(seen)
}

/// What can be known about a file without opening it.
fn describe(entry: &walkdir::DirEntry) -> Option<Discovered> {
    let abs_path = entry.path().to_string_lossy().to_string();
    if abs_path.contains("/.git/")
        || abs_path.contains("/node_modules/")
        || abs_path.contains("/.Trash")
    {
        return None;
    }

    let meta = entry.metadata().ok()?;
    let size = meta.len();
    let modified = meta.modified().ok().unwrap_or(SystemTime::UNIX_EPOCH);
    let created = meta.created().ok().unwrap_or(SystemTime::UNIX_EPOCH);
    let mtime_ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let created_dt: chrono::DateTime<chrono::Local> = created.into();
    let modified_dt: chrono::DateTime<chrono::Local> = modified.into();

    Some(Discovered {
        abs_path,
        filename: entry.file_name().to_string_lossy().to_string(),
        // Read from the name, not from the bytes.
        //
        // `infer::get_from_path` opens the file to sniff its magic number, and
        // doing that for every file on every scan was pure I/O for a field
        // almost always already correct. It now runs once, when a file is first
        // hashed — see `identify` — where the file is being read anyway.
        extension: entry
            .path()
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size,
        mtime_ms,
        created_at: created_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        modified_at: modified_dt.format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

/// Index one batch. Returns how many files had to be read to do it.
///
/// Split into three phases on purpose: look up what is already known, do the
/// expensive reading with nothing locked, then write. Only the first and last
/// touch the database.
fn commit_batch(
    state: &DbState,
    batch: &[Discovered],
    resolver: &crate::utils::graph_parser::NodeResolver,
) -> AppResult<usize> {
    let now_ms = chrono::Utc::now().timestamp_millis();

    let cached: Vec<Option<(u64, i64, String)>> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        batch
            .iter()
            .map(|f| db.cached_content_hash(&f.abs_path))
            .collect()
    };

    // No lock held here, which is the whole reason for the split: this is the
    // part that reads gigabytes.
    let mut identified = Vec::with_capacity(batch.len());
    let mut hashed = 0usize;
    for (found, cache) in batch.iter().zip(cached) {
        let observed = (found.size, found.mtime_ms);
        let reusable = cache.as_ref().and_then(|(size, mtime, hash)| {
            crate::file_index::cache_is_usable(observed, (*size, *mtime), now_ms)
                .then(|| hash.clone())
        });

        let (hash, sniffed_ext) = match reusable {
            Some(hash) => (hash, None),
            None => {
                let Some(hash) = crate::file_index::content_key(Path::new(&found.abs_path), found.size)
                else {
                    continue;
                };
                hashed += 1;
                (hash, identify(&found.abs_path))
            }
        };
        identified.push((found, hash, sniffed_ext));
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let tx = db
        .conn()
        .unchecked_transaction()
        .map_err(|e| AppError::General(format!("DB scan batch tx: {}", e)))?;

    for (found, hash, sniffed_ext) in identified {
        let extension = sniffed_ext.unwrap_or_else(|| found.extension.clone());
        index_one(&db, found, &hash, &extension, resolver, now_ms)?;
    }

    tx.commit()
        .map_err(|e| AppError::General(format!("DB scan batch commit: {}", e)))?;
    Ok(hashed)
}

/// The real extension, from the bytes rather than the name.
///
/// Only ever called on a file that is being read anyway.
fn identify(abs_path: &str) -> Option<String> {
    match infer::get_from_path(abs_path) {
        Ok(Some(kind)) => Some(kind.extension().to_string()),
        _ => None,
    }
}

/// Attach one file to the node its contents belong to.
fn index_one(
    db: &crate::db::DbBridge,
    found: &Discovered,
    content_hash: &str,
    extension: &str,
    resolver: &crate::utils::graph_parser::NodeResolver,
    now_ms: i64,
) -> AppResult<()> {
    let node_id = crate::file_index::node_id_for(content_hash);

    // A previous copy of this path may have belonged to different contents —
    // the file was edited, so it is a different item now and the old node must
    // not keep claiming this location.
    if let Ok(Some(previous)) = db.file_location_at(&found.abs_path) {
        if previous.node_id != node_id {
            db.delete_file_location(&found.abs_path)?;
            // A note embedding this path meant "the picture here", and the
            // picture here has changed. Its link follows the path rather than
            // staying with contents the note never saw.
            db.repoint_edges(&previous.node_id, &node_id)?;
        }
    }

    let existing = db.get_node(&node_id).ok().flatten();
    let node = merge_scanned(found, &node_id, extension, existing.as_ref());
    db.upsert_node(&node)?;

    db.upsert_file_location(
        &crate::db::FileLocation {
            abs_path: found.abs_path.clone(),
            node_id: node_id.clone(),
            size: found.size as i64,
            mtime_ms: found.mtime_ms,
        },
        now_ms,
    )?;
    db.remember_content_hash(
        &found.abs_path,
        found.size,
        found.mtime_ms,
        content_hash,
        now_ms,
    )?;

    if let Some(meta) = FileMetadata::from_node(&node) {
        index_for_search(db, &node.id, &meta);
    }

    // Links written before this file was indexed named a placeholder. Now that
    // it exists, they can name it.
    let adopted = db.adopt_ghost_edges(&ghost_names(found), &node_id)?;
    if adopted > 0 {
        log::info!("{}: {adopted} link(s) now point at it", found.filename);
    }

    // A file nobody has linked to anything has no edges to record, and clearing
    // then re-recording nothing for every file on disk is a write per file for
    // no result.
    if carries_links(&node) {
        crate::commands::nodes::sync_node_edges(db, &node, resolver);
    }
    Ok(())
}

/// The placeholder names a link could have used for this file.
///
/// Deliberately only the two that can mean nothing else: the `assets/<name>`
/// form a note writes when it embeds something, and the absolute path. A bare
/// filename is left alone — adopting it would let a wikilink to a missing note
/// called `[[báo cáo]]` silently attach itself to a picture of the same name.
fn ghost_names(found: &Discovered) -> Vec<String> {
    let mut names = vec![found.abs_path.to_lowercase()];
    let in_assets = Path::new(&found.abs_path)
        .parent()
        .and_then(|dir| dir.file_name())
        .is_some_and(|dir| dir.eq_ignore_ascii_case("assets"));
    if in_assets {
        names.push(format!("assets/{}", found.filename.to_lowercase()));
    }
    names
}

fn carries_links(node: &crate::models::node::NodeMetadata) -> bool {
    ["people", "linked_projects"].iter().any(|field| {
        node.properties
            .get(*field)
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty())
    })
}

/// What the walk saw, folded onto what the node already held.
///
/// A rescan reads the filesystem, which has no record of the tags, people or
/// projects a person attached here — so those are never taken from the scan,
/// only ever kept. Everything else is the filesystem's to state.
fn merge_scanned(
    found: &Discovered,
    node_id: &str,
    extension: &str,
    existing: Option<&crate::models::node::NodeMetadata>,
) -> crate::models::node::NodeMetadata {
    let mut properties = serde_json::json!({
        // A mirror of one current location, not the identity — that is the
        // node id. Kept because a great deal of the app still asks a file
        // where it is, and because one of the paths is the useful answer.
        // `file_locations` holds all of them.
        "path": found.abs_path,
        "extension": extension,
        "size": found.size as i64,
        "source_type": "local",
        "tags": [],
        "people": [],
    });

    if let (Some(existing), Some(props)) = (existing, properties.as_object_mut()) {
        for field in ["tags", "people", "linked_projects"] {
            if let Some(previous) = existing.properties.get(field) {
                if previous.as_array().is_some_and(|a| !a.is_empty()) {
                    props.insert(field.to_string(), previous.clone());
                }
            }
        }
    }

    crate::models::node::NodeMetadata {
        id: node_id.to_string(),
        node_type: "file".to_string(),
        title: found.filename.clone(),
        content: String::new(),
        properties,
        created_at: existing
            .map(|n| n.created_at.clone())
            .unwrap_or_else(|| found.created_at.clone()),
        updated_at: found.modified_at.clone(),
        timestamp: chrono::Utc::now().timestamp(),
        blocks: None,
    }
}

/// Put a file into the search index, people included so they are findable.
///
/// The fifth argument is the `content` column, and what used to go in it was
/// the file's *extension* — so the full-text index of every PDF in the vault
/// was the word "pdf". Whatever has been read out of the document goes there
/// now, which is what makes a phrase you remember find the file it is in, and
/// what makes `snippet(search_index, 4, …)` — already wired up in
/// `search_fts` — have something to quote.
fn index_for_search(db: &crate::db::DbBridge, node_id: &str, meta: &FileMetadata) {
    let body = db.file_text_joined(node_id).unwrap_or_default();
    let mut terms = meta.tags.clone();
    terms.extend(meta.people.iter().cloned());
    db.upsert_search_entry(
        node_id,
        "file",
        &meta.filename,
        &terms.join(" "),
        &body,
        &format!("ext:{} source:{}", meta.extension, meta.source_type),
        None,
        &meta.modified_at,
        &meta.path,
    );
}

#[tauri::command]
pub fn scan_directory(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    control: tauri::State<'_, ScanControl>,
    _vault_path: String,
    source_path: String,
) -> AppResult<()> {
    if !control.begin() {
        return Err(AppError::General("A scan is already running".to_string()));
    }
    let outcome = scan_source(&state, &source_path, Some(&control), |progress| {
        report_progress(&app_handle, progress)
    });
    control.finish();
    outcome.map(|_| ())
}

// ─── Reading what is inside the files ─────────────────────────

/// How many documents are read before the loop reports in and yields.
///
/// Smaller than the scan's batch because the work per item is far larger — a
/// four-hundred-page PDF is seconds, not microseconds — and the point of the
/// batch is to bound how long the database lock is held and how stale the
/// progress bar gets.
const TEXT_BATCH: usize = 8;

#[derive(Clone, serde::Serialize)]
pub struct TextProgress {
    /// Documents read in this run.
    pub done: usize,
    /// Documents still waiting, after this batch.
    pub remaining: usize,
    pub cancelled: bool,
}

/// Read the words out of every indexed document that has not been read yet.
///
/// Runs to completion in batches, reporting after each one, and holds the
/// database lock only while writing. A vault of a thousand PDFs takes minutes;
/// nothing about the app should be waiting on it, and nothing is.
#[tauri::command]
pub fn extract_file_text(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    control: tauri::State<'_, ScanControl>,
) -> AppResult<usize> {
    use tauri::Emitter;

    if !control.begin() {
        return Err(AppError::General("A scan is already running".to_string()));
    }
    let mut done = 0usize;

    loop {
        if control.cancelled() {
            let _ = app_handle.emit(
                "file-text-progress",
                TextProgress {
                    done,
                    remaining: 0,
                    cancelled: true,
                },
            );
            break;
        }

        let batch = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.files_awaiting_text(TEXT_BATCH)?
        };
        if batch.is_empty() {
            break;
        }

        // No lock held: this is the part that opens and parses documents.
        let read: Vec<(String, crate::file_text::Extraction)> = batch
            .into_iter()
            .map(|(node_id, extension, path)| {
                let outcome = crate::file_text::extract(Path::new(&path), &extension);
                (node_id, outcome)
            })
            .collect();

        let remaining = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            for (node_id, outcome) in &read {
                store_extraction(&db, node_id, outcome)?;
                done += 1;
            }
            db.files_awaiting_text_count()?
        };

        let _ = app_handle.emit(
            "file-text-progress",
            TextProgress {
                done,
                remaining,
                cancelled: false,
            },
        );
    }

    control.finish();
    Ok(done)
}

/// Write one extraction down, whatever it says.
///
/// Every outcome is recorded, including the ones that found nothing. The queue
/// is "everything without a row", so an unanswered file is a file that comes
/// back on the next pass — and a `.docx` that is not really a zip would come
/// back forever.
fn store_extraction(
    db: &crate::db::DbBridge,
    node_id: &str,
    outcome: &crate::file_text::Extraction,
) -> AppResult<()> {
    use crate::db::TextStatus;
    use crate::file_text::Extraction;

    let now = chrono::Utc::now().timestamp();
    match outcome {
        Extraction::Text(pages) => {
            db.store_file_text(node_id, pages)?;
            let chars = pages.iter().map(String::len).sum();
            db.record_text_status(node_id, TextStatus::Indexed, pages.len(), chars, now)?;

            // The words are only useful once they are searchable.
            if let Ok(Some(node)) = db.get_node(node_id) {
                if let Some(meta) = FileMetadata::from_node(&node) {
                    index_for_search(db, node_id, &meta);
                }
            }
        }
        Extraction::Unsupported => {
            db.record_text_status(node_id, TextStatus::Unsupported, 0, 0, now)?;
        }
        Extraction::Failed(reason) => {
            log::warn!("could not read text from {node_id}: {reason}");
            db.record_text_status(node_id, TextStatus::Failed, 0, 0, now)?;
        }
    }
    Ok(())
}

/// How much of the library is still unread.
#[tauri::command]
pub fn file_text_backlog(state: tauri::State<'_, DbState>) -> AppResult<usize> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.files_awaiting_text_count()
}

/// The first page of a document containing all of these words.
///
/// What turns "this manual mentions it" into "page 34 mentions it": the search
/// result knows which file matched, and this says where to open it.
#[tauri::command]
pub fn find_text_page(
    state: tauri::State<'_, DbState>,
    node_id: String,
    query: String,
) -> AppResult<Option<i64>> {
    let words: Vec<String> = query
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect();
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.first_page_matching(&node_id, &words)
}

/// What re-identifying this vault's files by content would change.
///
/// Reads only. Offered separately from the migration itself because a change
/// that touches every tag a person has applied should be inspectable before it
/// runs, not only afterwards.
#[tauri::command]
pub fn preview_file_identity_migration(
    state: tauri::State<'_, DbState>,
) -> AppResult<crate::file_index::migration::MigrationPlan> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    crate::file_index::migration::plan(&db)
}

/// Ask the running scan to stop at the end of its current batch.
#[tauri::command]
pub fn cancel_file_scan(control: tauri::State<'_, ScanControl>) {
    control.request_cancel();
}

fn report_progress(app_handle: &tauri::AppHandle, progress: ScanProgress) {
    use tauri::Emitter;
    let _ = app_handle.emit("file-scan-progress", progress);
}

/// One window onto the filtered library.
///
/// What `query_files` does in one go, this does a page at a time — and does the
/// narrowing and the ordering in SQL rather than in the browser. The old shape
/// shipped every indexed file across the IPC bridge on every open; at fifty
/// thousand files that is fourteen megabytes of JSON to show forty rows.
#[tauri::command]
pub fn query_file_page(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    filter: crate::db::FileFilter,
    sort: crate::db::FileSort,
    descending: bool,
    offset: usize,
    limit: usize,
) -> AppResult<crate::db::FilePage> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    // A page far larger than a screenful is a caller with a bug, and honouring
    // it would reintroduce exactly the problem this replaces.
    db.query_file_page(&filter, sort, descending, offset, limit.min(500))
}

/// Every identity the filter matches — for "select all".
#[tauri::command]
pub fn query_file_ids(
    state: tauri::State<'_, DbState>,
    filter: crate::db::FileFilter,
) -> AppResult<Vec<String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    // Enough to select a very large library, bounded so that a runaway filter
    // cannot hand the front end an unbounded array.
    db.query_file_ids(&filter, 100_000)
}

/// The tags in use across indexed files, with how many carry each.
#[tauri::command]
pub fn file_tag_counts(state: tauri::State<'_, DbState>) -> AppResult<Vec<(String, usize)>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.file_tag_counts()
}

/// The indexed files that are on this device, one entry per copy.
///
/// Driven from `file_locations` rather than from the nodes table, which is what
/// makes the answer honest in both directions. A node whose file has gone from
/// this machine — deleted here, or only ever tagged on another device — has no
/// location and so is not listed as something you can open. Two copies of one
/// file have two locations and are listed twice, sharing an id because they
/// share an identity: tag either and both show the tag.
#[tauri::command]
pub fn query_files(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
) -> AppResult<Vec<FileMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    let local = db.indexed_files()?.into_iter().filter_map(|(location, node)| {
        let mut meta = FileMetadata::from_node(&node)?;
        // The node mirrors whichever copy was scanned last; this row is about
        // one particular copy.
        meta.path = location.abs_path;
        meta.size = location.size;
        Some(meta)
    });

    // Files in a connected account, which have no path here because they have
    // no copy here. `path` carries the web address instead — the only place
    // they can actually be opened without downloading them.
    let remote = db.remote_files()?.into_iter().filter_map(|(entry, node)| {
        let mut meta = FileMetadata::from_node(&node)?;
        meta.path = entry.web_url;
        meta.size = entry.size;
        meta.source_type = entry.provider;
        Some(meta)
    });

    Ok(local.chain(remote).collect())
}

#[cfg(desktop)]
#[tauri::command]
pub fn open_local_file(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
) -> AppResult<()> {
    // A file in a connected account has no copy here to open, so what gets
    // opened is its page. Recognised by shape rather than by asking the
    // database: nothing on this disk is addressed by http.
    if path.starts_with("https://") || path.starts_with("http://") {
        #[cfg(target_os = "macos")]
        Command::new("open").arg(&path).spawn()?;
        #[cfg(target_os = "windows")]
        Command::new("cmd").args(["/C", "start", "", &path]).spawn()?;
        #[cfg(target_os = "linux")]
        Command::new("xdg-open").arg(&path).spawn()?;
        return Ok(());
    }

    let p = std::path::Path::new(&path);
    if !p.exists() || !p.is_file() {
        return Err(AppError::InvalidPath(
            "File not found or is a directory".to_string(),
        ));
    }

    // Check if the file is within allowed roots
    let roots = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        allowed_roots(&db, &vault_path)
    };

    let root_refs: Vec<&str> = roots.iter().map(|s| s.as_str()).collect();
    path_utils::enforce_within_roots(p, &root_refs)?;

    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(&path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer").arg(&path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(&path).spawn()?;
    }
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn open_local_file(
    _app_handle: tauri::AppHandle,
    _vault_path: String,
    _path: String,
) -> AppResult<()> {
    // Opening arbitrary local files is restricted/different on mobile
    Err(AppError::General(
        "Opening local files is not supported on mobile".to_string(),
    ))
}

/// Change what a file is tagged with, who it involves, and what it is called.
///
/// The write goes out to the vault as well as into the index, and that is the
/// point of the whole phase. A file node used to live only in SQLite, which
/// sync never reads — so a tag applied on the laptop stayed on the laptop
/// forever. Routing it through `write_node_file` makes it an ordinary vault
/// document that travels like a note does.
///
/// Only annotated files get a file on disk. Nothing is written for a file with
/// no tags, no people and no projects, because there would be nothing in it: a
/// folder of ten thousand indexed photos should not become ten thousand
/// documents pushed to every device. What travels is what a person made.
#[tauri::command]
pub fn update_file_metadata(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
    new_filename: String,
    new_tags: Vec<String>,
    new_people: Vec<String>,
) -> AppResult<String> {
    let node = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let location = db
            .file_location_at(&path)?
            .ok_or_else(|| AppError::General("File is not indexed".to_string()))?;
        db.get_node(&location.node_id)?
            .ok_or_else(|| AppError::General("File node not found".to_string()))?
    };

    // ── Rename, if that is what this is ───────────────────────
    //
    // Renaming inside `assets/` is refused rather than skipped. Notes embed
    // attachments by name — `](assets/<filename>)`, see
    // `note/editor/composables/useAssetPaths.ts` — so renaming one there breaks
    // every note pointing at it, and nothing here can rewrite those bodies:
    // they are CRDT documents, and the only way to know which ones to touch is
    // a `content LIKE '%name%'` scan that matches far more than it should.
    //
    // What this replaces was worse than a refusal. The rename was silently
    // skipped while the command still returned success, and the front end had
    // already optimistically shown the new name — so the file appeared renamed
    // until the next reload put the old name back.
    let path_obj = std::path::Path::new(&path);
    let renaming = path_obj
        .file_name()
        .map(|n| n.to_string_lossy() != new_filename)
        .unwrap_or(false);

    // Renaming an asset a note embeds would break the note.
    //
    // Notes embed attachments by name — `](assets/<filename>)` — so the name is
    // load-bearing for anything pointing at it, and nothing here can rewrite
    // those bodies: they are CRDT documents.
    //
    // Until this phase the refusal covered every file in `assets/`, because
    // there was no way to tell which ones were actually used: the only answer
    // available was a substring scan that matched far more than it should. Real
    // edges make the question answerable, so the refusal now applies to the
    // files it was always meant to protect and gets out of the way of the rest.
    if renaming && is_vault_asset(&vault_path, &path) {
        let users = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            db.nodes_linking_to(node.stable_id())?
        };
        if !users.is_empty() {
            let names: Vec<&str> = users
                .iter()
                .take(3)
                .map(|(_, _, title, _)| title.as_str())
                .collect();
            return Err(AppError::General(format!(
                "Cannot rename: {} note(s) embed this file by name, including {}.",
                users.len(),
                names.join(", ")
            )));
        }
    }

    let mut final_path = path.clone();
    let mut extension = node
        .properties
        .get("extension")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if renaming {
        path_utils::enforce_no_traversal(&path)?;
        if !path_utils::is_safe_filename(&new_filename) {
            return Err(AppError::InvalidPath("Invalid filename".to_string()));
        }
        let parent = path_obj
            .parent()
            .ok_or_else(|| AppError::InvalidPath("File has no parent directory".to_string()))?;

        let new_path = parent.join(&new_filename);
        std::fs::rename(&path, &new_path)
            .map_err(|e| AppError::General(format!("Failed to rename file on disk: {}", e)))?;

        final_path = new_path.to_string_lossy().to_string();
        extension = new_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
    }

    // ── The index ─────────────────────────────────────────────
    //
    // Renaming moves the file, so the location row moves with it. The identity
    // does not: the bytes are the same bytes, which is exactly the property
    // that keeps these tags attached across a rename.
    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        if renaming {
            if let Some(previous) = db.file_location_at(&path)? {
                db.delete_file_location(&path)?;
                db.forget_content_hash(&path)?;
                db.upsert_file_location(
                    &crate::db::FileLocation {
                        abs_path: final_path.clone(),
                        ..previous
                    },
                    chrono::Utc::now().timestamp_millis(),
                )?;
            }
        }

        let mut updated = node.clone();
        updated.title = new_filename.clone();
        if let Some(props) = updated.properties.as_object_mut() {
            props.insert("tags".into(), serde_json::json!(new_tags));
            props.insert("people".into(), serde_json::json!(new_people));
            props.insert("path".into(), serde_json::json!(final_path));
            props.insert("extension".into(), serde_json::json!(extension));
        }
        db.upsert_node(&updated)?;

        let resolver = crate::commands::nodes::build_resolver(&db);
        crate::commands::nodes::sync_node_edges(&db, &updated, &resolver);

        if let Some(meta) = FileMetadata::from_node(&updated) {
            index_for_search(&db, &updated.id, &meta);
        }
    }

    // ── The vault, so it travels ──────────────────────────────
    //
    // Outside the lock: `write_node_file` takes its own.
    let annotated = !new_tags.is_empty() || !new_people.is_empty();
    if annotated {
        publish_file_metadata(
            app_handle,
            state,
            &vault_path,
            &node.id,
            &new_filename,
            &extension,
            &new_tags,
            &new_people,
        )?;
    } else {
        withdraw_file_metadata(&vault_path, &node.id);
    }

    Ok(final_path)
}

/// Turn a PDF's highlights into a note.
///
/// Highlights have been first-class nodes since before this phase — they sync,
/// they are searchable, and they survive the PDF being moved. What they were
/// not was *reachable*: the only way to read them was to open the PDF and look
/// at the sidebar, which is the one place you already know what they say.
///
/// A note is where the rest of the vault can get at them. It links back to the
/// file by identity, so the note appears in that file's "used by" panel and the
/// link keeps working if the PDF is renamed.
#[tauri::command]
pub fn export_highlights_to_note(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_id: String,
) -> AppResult<String> {
    let (title, body) = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let file = db
            .get_node(&node_id)?
            .ok_or_else(|| AppError::General("File node not found".to_string()))?;

        let mut highlights: Vec<crate::models::node::NodeMetadata> = db
            .get_nodes_by_type("pdf_highlight")?
            .into_iter()
            .filter(|h| {
                h.properties.get("pdf_id").and_then(|v| v.as_str()) == Some(node_id.as_str())
            })
            .collect();

        if highlights.is_empty() {
            return Err(AppError::General(
                "This file has no highlights to export".to_string(),
            ));
        }

        // Reading order, which is page order — not the order they were made in.
        highlights.sort_by_key(|h| h.properties.get("page").and_then(|v| v.as_i64()).unwrap_or(0));

        let mut body = format!(
            "Trích từ [{}](synabit://file/{}).\n\n",
            file.title, node_id
        );
        let mut current_page = -1;
        for highlight in &highlights {
            let page = highlight
                .properties
                .get("page")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if page != current_page {
                body.push_str(&format!("\n## Trang {page}\n\n"));
                current_page = page;
            }
            let text = highlight
                .properties
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if !text.is_empty() {
                body.push_str(&format!("> {text}\n\n"));
            }
            // The reader's own words, which are the point of highlighting.
            if !highlight.content.trim().is_empty() {
                body.push_str(&format!("{}\n\n", highlight.content.trim()));
            }
        }

        (format!("Trích dẫn — {}", file.title), body)
    };

    let rel_path = crate::commands::nodes::free_node_path(
        std::path::Path::new(&vault_path),
        &format!("Notes/{}.md", path_utils::sanitise_for_filename(&title)),
    );

    crate::commands::nodes::write_node_file(
        app_handle,
        state,
        vault_path,
        rel_path.clone(),
        title,
        "note".to_string(),
        serde_json::json!({}),
        Some(body),
    )?;

    Ok(rel_path)
}

/// A filter somebody wants to keep.
///
/// Every narrowing the list can do is already a query; a saved collection is
/// one of those queries given a name. Written as a vault node — the `filter`
/// type has been declared since long before anything used it — so a collection
/// follows the reader between devices the way a tag does. It stores the
/// question rather than the answer, which is what keeps it true as files come
/// and go.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct FileCollection {
    pub id: String,
    pub name: String,
    pub filter: serde_json::Value,
}

#[tauri::command]
pub fn save_file_collection(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    name: String,
    filter: serde_json::Value,
) -> AppResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::General("A collection needs a name".to_string()));
    }

    let rel_path = format!("Filters/{}.md", path_utils::sanitise_for_filename(trimmed));
    crate::commands::nodes::write_node_file(
        app_handle,
        state,
        vault_path,
        rel_path.clone(),
        trimmed.to_string(),
        "filter".to_string(),
        serde_json::json!({ "scope": "files", "filter": filter }),
        Some(String::new()),
    )?;
    Ok(rel_path)
}

#[tauri::command]
pub fn list_file_collections(state: tauri::State<'_, DbState>) -> AppResult<Vec<FileCollection>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    Ok(db
        .get_nodes_by_type("filter")?
        .into_iter()
        // Other parts of the app may save filters of their own one day; this
        // list is the ones about files.
        .filter(|n| n.properties.get("scope").and_then(|v| v.as_str()) == Some("files"))
        .map(|n| FileCollection {
            id: n.id.clone(),
            name: n.title.clone(),
            filter: n
                .properties
                .get("filter")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        })
        .collect())
}

#[tauri::command]
pub fn delete_file_collection(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    id: String,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    crate::commands::trash::apply_trash(&db, &vault_path, &id)?;
    Ok(())
}

/// Set or clear a colour label across a selection.
///
/// A label is a rating without words: the thing a person reaches for when they
/// have four hundred photos and want to mark the twenty worth returning to,
/// before they know what to call them. Stored beside the tags, and travelling
/// with them, because it is the same kind of fact about the same file.
#[tauri::command]
pub fn set_file_label(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_ids: Vec<String>,
    label: Option<String>,
) -> AppResult<usize> {
    const LABELS: [&str; 6] = ["red", "orange", "yellow", "green", "blue", "purple"];
    if let Some(colour) = &label {
        if !LABELS.contains(&colour.as_str()) {
            return Err(AppError::General(format!("Unknown label: {colour}")));
        }
    }

    let planned: Vec<crate::models::node::NodeMetadata> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        node_ids
            .iter()
            .filter_map(|id| db.get_node(id).ok().flatten())
            .collect()
    };

    let mut changed = 0;
    for node in planned {
        let (tags, people, extension) = {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            let mut updated = node.clone();
            if let Some(props) = updated.properties.as_object_mut() {
                match &label {
                    Some(colour) => {
                        props.insert("label".into(), serde_json::json!(colour));
                    }
                    None => {
                        props.remove("label");
                    }
                }
            }
            db.upsert_node(&updated)?;
            if let Some(meta) = FileMetadata::from_node(&updated) {
                index_for_search(&db, &updated.id, &meta);
            }
            (
                string_list(&updated, "tags"),
                string_list(&updated, "people"),
                updated
                    .properties
                    .get("extension")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            )
        };

        // A label alone is enough to make a file worth carrying between
        // devices, so it publishes on the same terms a tag does.
        if label.is_some() || !tags.is_empty() || !people.is_empty() {
            publish_file_metadata(
                app_handle.clone(),
                state.clone(),
                &vault_path,
                &node.id,
                &node.title,
                &extension,
                &tags,
                &people,
            )?;
        }
        changed += 1;
    }
    Ok(changed)
}

/// Show a file where it lives, in the system's own file manager.
#[cfg(desktop)]
#[tauri::command]
pub fn reveal_in_file_manager(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
) -> AppResult<()> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(AppError::InvalidPath("File not found".to_string()));
    }
    let roots = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        allowed_roots(&db, &vault_path)
    };
    let refs: Vec<&str> = roots.iter().map(|s| s.as_str()).collect();
    path_utils::enforce_within_roots(p, &refs)?;

    #[cfg(target_os = "macos")]
    Command::new("open").args(["-R", &path]).spawn()?;
    #[cfg(target_os = "windows")]
    Command::new("explorer").args(["/select,", &path]).spawn()?;
    #[cfg(target_os = "linux")]
    {
        // No standard "reveal", so open the folder the file is in.
        let parent = p.parent().unwrap_or(std::path::Path::new("/"));
        Command::new("xdg-open").arg(parent).spawn()?;
    }
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn reveal_in_file_manager(
    _state: tauri::State<'_, DbState>,
    _vault_path: String,
    _path: String,
) -> AppResult<()> {
    Err(AppError::General(
        "There is no file manager to reveal this in".to_string(),
    ))
}

/// What a picture says about itself: the camera, the moment, the size.
///
/// Read in the front end rather than here — see `src/shared/exif.ts` for why —
/// and handed back to be stored against the file's identity, which is where it
/// belongs: a photograph's camera is a fact about the photograph, not about the
/// machine that happens to hold a copy of it.
#[derive(serde::Deserialize, Debug, Default)]
pub struct PhotoFacts {
    pub camera: Option<String>,
    /// When the shutter fired, as `YYYY-MM-DD HH:MM:SS`.
    pub shot_at: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// Record what was read out of a picture's header.
///
/// Merged rather than replaced, and only ever filled in: a later read that
/// finds nothing must not erase what an earlier one found, because "this file
/// has no camera recorded" and "we did not manage to look" are different facts
/// and only one of them is worth keeping.
#[tauri::command]
pub fn record_photo_facts(
    state: tauri::State<'_, DbState>,
    node_id: String,
    facts: PhotoFacts,
) -> AppResult<()> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let Some(mut node) = db.get_node(&node_id)? else {
        return Ok(());
    };

    let mut changed = false;
    if let Some(props) = node.properties.as_object_mut() {
        let mut set = |key: &str, value: Option<serde_json::Value>| {
            if let Some(value) = value {
                if props.get(key) != Some(&value) {
                    props.insert(key.to_string(), value);
                    changed = true;
                }
            }
        };
        set("camera", facts.camera.filter(|c| !c.is_empty()).map(Into::into));
        set("shot_at", facts.shot_at.filter(|d| !d.is_empty()).map(Into::into));
        set("width", facts.width.filter(|w| *w > 0).map(Into::into));
        set("height", facts.height.filter(|h| *h > 0).map(Into::into));
    }

    if !changed {
        return Ok(());
    }
    db.upsert_node(&node)?;
    if let Some(meta) = FileMetadata::from_node(&node) {
        index_for_search(&db, &node.id, &meta);
    }
    Ok(())
}

/// The distinct cameras in the library, for a filter to offer.
#[tauri::command]
pub fn list_cameras(state: tauri::State<'_, DbState>) -> AppResult<Vec<String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.distinct_file_property("camera")
}

/// Add or remove tags across many files at once.
///
/// The unit of work in this app has been one file at a time: select, open the
/// panel, type a tag. That is fine for the file you are looking at and useless
/// for the four hundred photos from a trip, which is the job an asset manager
/// exists to do.
///
/// Both lists are applied to every file, additions after removals, so a single
/// call can retag a selection rather than needing two passes. Files already
/// carrying a tag being added are left alone rather than gaining it twice.
#[tauri::command]
pub fn bulk_tag_files(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_ids: Vec<String>,
    add: Vec<String>,
    remove: Vec<String>,
) -> AppResult<usize> {
    // Worked out once, in the database, before any of the writing starts.
    let planned: Vec<(crate::models::node::NodeMetadata, Vec<String>)> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        node_ids
            .iter()
            .filter_map(|id| db.get_node(id).ok().flatten())
            .filter_map(|node| {
                let current = string_list(&node, "tags");
                let next = retag(&current, &add, &remove);
                // Nothing to say about a file that already reads this way.
                (next != current).then_some((node, next))
            })
            .collect()
    };

    let mut changed = 0;
    for (node, tags) in planned {
        {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            let mut updated = node.clone();
            if let Some(props) = updated.properties.as_object_mut() {
                props.insert("tags".into(), serde_json::json!(tags));
            }
            db.upsert_node(&updated)?;
            if let Some(meta) = FileMetadata::from_node(&updated) {
                index_for_search(&db, &updated.id, &meta);
            }
        }

        // Outside the lock — `write_node_file` takes its own.
        let people = string_list(&node, "people");
        let extension = node
            .properties
            .get("extension")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if tags.is_empty() && people.is_empty() {
            withdraw_file_metadata(&vault_path, &node.id);
        } else {
            publish_file_metadata(
                app_handle.clone(),
                state.clone(),
                &vault_path,
                &node.id,
                &node.title,
                &extension,
                &tags,
                &people,
            )?;
        }
        changed += 1;
    }

    Ok(changed)
}

/// The tags a file should end up with.
///
/// Removals first, so a call that both removes and adds the same tag ends with
/// it present — "replace these tags with this one" is the intent behind that,
/// and the other order would silently drop it.
fn retag(current: &[String], add: &[String], remove: &[String]) -> Vec<String> {
    let mut next: Vec<String> = current
        .iter()
        .filter(|tag| !remove.iter().any(|r| r.eq_ignore_ascii_case(tag)))
        .cloned()
        .collect();
    for tag in add {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        if !next.iter().any(|existing| existing.eq_ignore_ascii_case(tag)) {
            next.push(tag.to_lowercase());
        }
    }
    next
}

fn string_list(node: &crate::models::node::NodeMetadata, field: &str) -> Vec<String> {
    node.properties
        .get(field)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Write a file's metadata into the vault, where sync will pick it up.
#[allow(clippy::too_many_arguments)]
fn publish_file_metadata(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: &str,
    node_id: &str,
    filename: &str,
    extension: &str,
    tags: &[String],
    people: &[String],
) -> AppResult<()> {
    // Deliberately not the path, and not the size.
    //
    // Both are facts about this machine — `/Users/anh/Ảnh/…` means nothing on a
    // phone — and this file is about to be copied to every device. The index
    // holds them locally in `file_locations`; what travels is what a file *is*.
    let properties = serde_json::json!({
        "extension": extension,
        "tags": tags,
        "people": people,
        "content_hash": crate::file_index::hash_from_node_id(node_id).unwrap_or_default(),
    });

    crate::commands::nodes::write_node_file(
        app_handle,
        state,
        vault_path.to_string(),
        node_id.to_string(),
        filename.to_string(),
        "file".to_string(),
        properties,
        None,
    )
}

/// Take a file's metadata back out of the vault once there is nothing in it.
///
/// Removing the last tag from a file should stop that file following you
/// between devices, the same way it stopped being annotated. Failing is fine
/// and deliberately quiet: an absent file is the desired state either way.
fn withdraw_file_metadata(vault_path: &str, node_id: &str) {
    let Ok(abs_path) = path_utils::resolve_safe_path(vault_path, node_id) else {
        return;
    };
    if abs_path.is_file() {
        let _ = std::fs::remove_file(&abs_path);
    }
}

/// Bring the index back in line with what is on disk.
#[tauri::command]
pub fn reindex_sources(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    control: tauri::State<'_, ScanControl>,
    vault_path: String,
) -> AppResult<()> {
    if !control.begin() {
        return Err(AppError::General("A scan is already running".to_string()));
    }
    let result = reindex_all(&app_handle, &state, Some(&control), &vault_path);
    control.finish();
    result
}

fn reindex_all(
    app_handle: &tauri::AppHandle,
    state: &DbState,
    control: Option<&ScanControl>,
    vault_path: &str,
) -> AppResult<()> {
    let mut scan_paths: Vec<String> = Vec::new();
    let assets_dir = std::path::Path::new(vault_path).join("assets");
    if assets_dir.exists() {
        scan_paths.push(assets_dir.to_string_lossy().to_string());
    }
    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(sources) = db.get_all_file_sources() {
            for source in sources {
                if !scan_paths.contains(&source.path) {
                    scan_paths.push(source.path);
                }
            }
        }
    }

    // Before the walk, not after: a legacy node re-identified here is one the
    // scan then recognises as already indexed, so the tags on it survive
    // instead of being replaced by a fresh node the scan would otherwise mint.
    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        match crate::file_index::migration::apply(&db) {
            Ok(0) => {}
            Ok(n) => log::info!("reindex: re-identified {n} file node(s) by content"),
            Err(e) => log::error!("reindex: identity migration failed: {e:?}"),
        }
    }

    let mut seen = std::collections::HashSet::new();
    let mut completed = Vec::new();
    for source_path in &scan_paths {
        match scan_source(state, source_path, control, |progress| {
            report_progress(app_handle, progress)
        }) {
            Ok(paths) => {
                seen.extend(paths);
                completed.push(source_path.clone());
            }
            Err(e) => log::error!("reindex: scanning {} failed: {:?}", source_path, e),
        }
        if control.is_some_and(|c| c.cancelled()) {
            break;
        }
    }

    // Only folders that were walked to the end can say what is missing from
    // them. A scan that was cancelled halfway has not seen the rest of the
    // tree, and treating unvisited files as deleted would drop the index for
    // most of the folder.
    if control.is_some_and(|c| c.cancelled()) {
        return Ok(());
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let dropped = forget_missing(&db, &completed, &seen)?;
    if dropped > 0 {
        log::info!("reindex: {dropped} indexed file(s) are no longer on disk");
    }
    Ok(())
}

/// Drop what the scan did not find, and then whatever is left with nowhere to be.
///
/// Two steps, because a location going away is not the same as a file being
/// gone: the same contents may still sit somewhere else on this machine, and
/// the node has to survive that.
///
/// A node with no locations left is still not always deleted. If a person
/// attached tags or people to it, that metadata is the whole reason the app
/// exists and it outlives any particular copy of the bytes — the file may well
/// be sitting on another device, where this same node is about to be matched to
/// it by content. What gets deleted is only what the scanner itself created.
fn forget_missing(
    db: &crate::db::DbBridge,
    scanned_roots: &[String],
    seen: &std::collections::HashSet<String>,
) -> AppResult<usize> {
    let mut orphaned_nodes = std::collections::HashSet::new();
    let mut dropped = 0;

    for root in scanned_roots {
        for location in db.file_locations_under(root)? {
            if seen.contains(&location.abs_path) {
                continue;
            }
            db.delete_file_location(&location.abs_path)?;
            db.forget_content_hash(&location.abs_path)?;
            orphaned_nodes.insert(location.node_id);
            dropped += 1;
        }
    }

    for node_id in orphaned_nodes {
        if db.node_has_locations(&node_id) {
            continue;
        }
        let Ok(Some(node)) = db.get_node(&node_id) else {
            continue;
        };
        if carries_user_metadata(&node) {
            continue;
        }
        forget_file(db, &node_id, Some(&node))?;
    }

    Ok(dropped)
}

/// Did a person put something here, or is this node purely what the scanner saw?
fn carries_user_metadata(node: &crate::models::node::NodeMetadata) -> bool {
    ["tags", "people", "linked_projects"].iter().any(|field| {
        node.properties
            .get(*field)
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty())
    })
}

#[tauri::command]
pub fn read_local_file_content(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    path: String,
) -> AppResult<String> {
    let p = std::path::Path::new(&path);
    if !p.exists() || !p.is_file() {
        return Err(AppError::InvalidPath(
            "File not found or is a directory".to_string(),
        ));
    }

    // Validate path is within vault or registered file sources
    let roots = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        allowed_roots(&db, &vault_path)
    };

    let root_refs: Vec<&str> = roots.iter().map(|s| s.as_str()).collect();
    path_utils::enforce_within_roots(p, &root_refs)?;

    // Check size limit (e.g. 5MB)
    if let Ok(meta) = p.metadata() {
        if meta.len() > 5 * 1024 * 1024 {
            return Err(AppError::General(
                "File is too large to preview (max 5MB)".to_string(),
            ));
        }
    }

    let content = std::fs::read_to_string(p)
        .map_err(|e| AppError::General(format!("Failed to read file: {}", e)))?;

    Ok(content)
}

/// Every directory this app is willing to touch: the vault, plus each folder
/// the user has added as a source.
///
/// Read once and returned by value because the caller has to drop the database
/// lock before doing anything slow with the answer.
fn allowed_roots(db: &crate::db::DbBridge, vault_path: &str) -> Vec<String> {
    let mut roots = vec![vault_path.to_string()];
    if let Ok(sources) = db.get_all_file_sources() {
        for source in sources {
            roots.push(source.path);
        }
    }
    roots
}

/// Delete a file: into the vault trash, not off the disk.
///
/// Two things were wrong with the version this replaces, and they are the same
/// bug seen from either end. It called `fs::remove_file` on whatever path the
/// front end handed down, without checking that path against anything — so the
/// blast radius was the whole disk rather than the folders the user had opened
/// to us. And what it did there was permanent, in a screen whose main use is
/// clearing out duplicates: the one place where a mis-click is both likely and
/// unrecoverable.
///
/// The order below is deliberate. Validate, then move the bytes somewhere they
/// can be fetched back from, and only then forget the file. If the last step
/// fails the file is still recoverable and the next `reindex_sources` drops the
/// stale row; if the move failed we never got that far.
#[tauri::command]
pub fn delete_file(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    file_id: String,
    file_path: String,
) -> AppResult<()> {
    // The database lock is taken twice on purpose, and released across the
    // move. Trashing a file on another volume falls back to a copy, and a copy
    // of a large video would otherwise hold every other part of the app —
    // notes, tasks, chat — waiting on this one delete.
    let (roots, node) = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        (
            allowed_roots(&db, &vault_path),
            db.get_node(&file_id).ok().flatten(),
        )
    };

    let root_refs: Vec<&str> = roots.iter().map(|s| s.as_str()).collect();
    trash_indexed_file(&vault_path, &file_path, &root_refs)?;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.delete_file_location(&file_path)?;
    db.forget_content_hash(&file_path)?;

    // Deleting one copy of a duplicated file is not deleting the file. The node
    // carries the tags, and the other copy still needs them; only when the last
    // copy has gone is there nothing left for it to describe.
    if db.node_has_locations(&file_id) {
        return Ok(());
    }
    forget_file(&db, &file_id, node.as_ref())
}

/// Move one file out of the way, having first established that we were allowed
/// to touch it at all.
///
/// The check is the point. Without it this is `fs::remove_file` on a string
/// from the front end, which is the whole disk.
fn trash_indexed_file(vault_path: &str, file_path: &str, roots: &[&str]) -> AppResult<()> {
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        // Nothing to move. The index still has to be cleared, so this is not an
        // error — a file deleted in Finder a moment ago lands here.
        return Ok(());
    }

    path_utils::enforce_within_roots(path, roots)?;
    crate::commands::trash::trash_asset(vault_path, path)
        .map_err(|e| AppError::General(format!("Failed to move file to trash: {}", e)))?;
    Ok(())
}

/// Does this path live in the vault's `assets/` folder — the one place a
/// filename is load-bearing, because notes embed it verbatim?
fn is_vault_asset(vault_path: &str, path: &str) -> bool {
    let assets_dir = std::path::Path::new(vault_path).join("assets");
    std::path::Path::new(path).starts_with(assets_dir)
}

/// Drop every trace of a file from the index.
fn forget_file(
    db: &crate::db::DbBridge,
    file_id: &str,
    node: Option<&crate::models::node::NodeMetadata>,
) -> AppResult<()> {
    db.delete_node(file_id)?;
    db.delete_search_entry(file_id);
    db.forget_file_text(file_id)?;

    // Links are recorded against the node's stable identity, which is not
    // always its row id — clearing by the wrong one leaves every backlink to
    // this file standing, pointing at something that no longer exists.
    let edge_source = node.map(|n| n.stable_id()).unwrap_or(file_id);
    logged(
        "clear links",
        file_id,
        db.delete_node_edges_by_source(edge_source),
    );
    Ok(())
}

#[derive(serde::Serialize)]
pub struct FileReference {
    pub node_id: String,
    pub node_type: String,
    pub title: String,
    /// How it is used — `attachment` for a note that embeds it, a link type
    /// otherwise, so the panel can say "shown in" rather than "mentioned in".
    pub edge_type: String,
}

/// Which notes, tasks and boards use this file.
///
/// Takes the file's identity rather than its name. The name was never a safe
/// question to ask — a file called `note.pdf` matched every note containing the
/// word "note" — and it stopped being answerable at all once a file could be
/// renamed without becoming a different file.
#[tauri::command]
pub fn get_file_references(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    node_id: String,
) -> AppResult<Vec<FileReference>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let node = db.get_node(&node_id)?;
    // Links are recorded against the stable identity, which for a file node is
    // usually its id but need not be.
    let target = node.as_ref().map(|n| n.stable_id()).unwrap_or(&node_id);

    Ok(db
        .nodes_linking_to(target)?
        .into_iter()
        .map(|(node_id, node_type, title, edge_type)| FileReference {
            node_id,
            node_type,
            title,
            edge_type,
        })
        .collect())
}

/// Find files the vault holds more than one copy of.
///
/// This used to be a three-stage pipeline: group by size, digest the first
/// 64KB of every candidate, then digest the survivors in full — hundreds of
/// megabytes of reading on every run, streamed back group by group because it
/// took long enough to need a progress bar.
///
/// None of that is needed now. A file's identity *is* a digest of its contents,
/// so two copies already share a node id and finding them is one `GROUP BY`
/// over a table. It stays an event-emitting command rather than returning its
/// results directly, because the front end is built around the stream and the
/// shape is still right for the day a vault is large enough to need it.
#[tauri::command]
pub async fn find_duplicate_files(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
) -> AppResult<()> {
    use tauri::Emitter;

    let groups = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let mut built = Vec::new();
        for (node_id, locations) in db.duplicate_locations()? {
            let Ok(Some(node)) = db.get_node(&node_id) else {
                continue;
            };
            let Some(meta) = FileMetadata::from_node(&node) else {
                continue;
            };

            let files: Vec<FileMetadata> = locations
                .iter()
                .map(|loc| FileMetadata {
                    path: loc.abs_path.clone(),
                    size: loc.size,
                    ..meta.clone()
                })
                .collect();

            let size = locations.first().map(|l| l.size).unwrap_or(0);
            let count = files.len();
            built.push(DuplicateGroup {
                filename: meta.filename.clone(),
                extension: meta.extension.clone(),
                size,
                count,
                files,
                // Every copy past the first is space that buys nothing.
                wasted_bytes: size * (count as i64 - 1),
            });
        }
        built
    };

    #[derive(serde::Serialize, Clone)]
    struct ScanComplete {
        total_groups: usize,
        total_duplicate_files: usize,
        total_wasted_bytes: i64,
    }

    let summary = ScanComplete {
        total_groups: groups.len(),
        total_duplicate_files: groups.iter().map(|g| g.count - 1).sum(),
        total_wasted_bytes: groups.iter().map(|g| g.wasted_bytes).sum(),
    };

    for group in &groups {
        let _ = app_handle.emit("duplicate-group-found", group);
    }
    let _ = app_handle.emit("duplicate-scan-complete", summary);
    Ok(())
}

// ─── Export Annotated PDF ─────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
pub struct AnnotationRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(serde::Deserialize, Debug)]
pub struct ExportAnnotation {
    pub page: usize,
    pub color: String,
    pub text: String,
    pub rects: Vec<AnnotationRect>,
    pub note: String,
}

#[tauri::command]
pub fn export_annotated_pdf(
    _app_handle: tauri::AppHandle,
    vault_path: String,
    pdf_path: String,
    annotations: Vec<ExportAnnotation>,
) -> AppResult<String> {
    use lopdf::StringFormat;
    use lopdf::{Dictionary, Document, Object};

    let source = std::path::Path::new(&pdf_path);
    if !source.exists() {
        return Err(AppError::InvalidPath("PDF file not found".to_string()));
    }

    let mut doc = Document::load(&pdf_path)
        .map_err(|e| AppError::General(format!("Failed to load PDF: {}", e)))?;

    // get_pages() returns BTreeMap<u32, ObjectId> (page_number → object_id)
    let pages = doc.get_pages();

    for ann in &annotations {
        let page_num = ann.page as u32;
        let page_obj_id = match pages.get(&page_num) {
            Some(id) => *id,
            None => continue,
        };

        // Get page MediaBox to convert normalized [0,1] coords to PDF coords
        let media_box = doc
            .get_dictionary(page_obj_id)
            .ok()
            .and_then(|page| page.get(b"MediaBox").ok().cloned())
            .and_then(|mb| {
                if let Object::Array(arr) = mb {
                    if arr.len() == 4 {
                        let vals: Vec<f64> = arr
                            .iter()
                            .filter_map(|v| match v {
                                Object::Real(f) => Some(*f as f64),
                                Object::Integer(i) => Some(*i as f64),
                                _ => None,
                            })
                            .collect();
                        if vals.len() == 4 {
                            return Some((vals[0], vals[1], vals[2], vals[3]));
                        }
                    }
                }
                None
            })
            .unwrap_or((0.0, 0.0, 612.0, 792.0));

        let page_w = media_box.2 - media_box.0;
        let page_h = media_box.3 - media_box.1;

        // Map highlight color to RGB
        let (r, g, b) = match ann.color.as_str() {
            "yellow" => (1.0_f64, 0.92, 0.23),
            "green" => (0.30, 0.69, 0.31),
            "blue" => (0.13, 0.59, 0.95),
            "pink" => (0.91, 0.12, 0.39),
            _ => (1.0, 0.92, 0.23),
        };

        for rect in &ann.rects {
            // Convert normalized coords → PDF coords (PDF origin = bottom-left)
            let x1 = media_box.0 + rect.x * page_w;
            let y1 = media_box.3 - (rect.y + rect.h) * page_h; // flip Y
            let x2 = x1 + rect.w * page_w;
            let y2 = y1 + rect.h * page_h;

            let mut annot_dict = Dictionary::new();
            annot_dict.set("Type", Object::Name(b"Annot".to_vec()));
            annot_dict.set("Subtype", Object::Name(b"Highlight".to_vec()));
            annot_dict.set(
                "Rect",
                Object::Array(vec![
                    Object::Real(x1 as f32),
                    Object::Real(y1 as f32),
                    Object::Real(x2 as f32),
                    Object::Real(y2 as f32),
                ]),
            );
            annot_dict.set(
                "C",
                Object::Array(vec![
                    Object::Real(r as f32),
                    Object::Real(g as f32),
                    Object::Real(b as f32),
                ]),
            );
            annot_dict.set("CA", Object::Real(0.4)); // opacity
            annot_dict.set("F", Object::Integer(4)); // Print flag

            // QuadPoints for highlight rendering
            annot_dict.set(
                "QuadPoints",
                Object::Array(vec![
                    Object::Real(x1 as f32),
                    Object::Real(y2 as f32),
                    Object::Real(x2 as f32),
                    Object::Real(y2 as f32),
                    Object::Real(x1 as f32),
                    Object::Real(y1 as f32),
                    Object::Real(x2 as f32),
                    Object::Real(y1 as f32),
                ]),
            );

            // Add note as Contents if present
            if !ann.note.is_empty() {
                annot_dict.set(
                    "Contents",
                    Object::String(ann.note.as_bytes().to_vec(), StringFormat::Literal),
                );
            }
            if !ann.text.is_empty() {
                annot_dict.set(
                    "T",
                    Object::String(b"Synabit".to_vec(), StringFormat::Literal),
                );
            }

            let annot_id = doc.add_object(Object::Dictionary(annot_dict));

            // Append annotation reference to the page's /Annots array
            let existing_annots = doc
                .get_dictionary(page_obj_id)
                .ok()
                .and_then(|p| p.get(b"Annots").ok().cloned());

            let mut annots_array = match existing_annots {
                Some(Object::Array(arr)) => arr,
                Some(Object::Reference(r)) => {
                    if let Ok(Object::Array(arr)) = doc.get_object(r) {
                        arr.clone()
                    } else {
                        vec![]
                    }
                }
                _ => vec![],
            };
            annots_array.push(Object::Reference(annot_id));

            // Update the page dictionary with the new Annots array
            if let Ok(page_dict) = doc.get_dictionary_mut(page_obj_id) {
                page_dict.set("Annots", Object::Array(annots_array));
            }
        }
    }

    // Save to a new file alongside the original
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let parent = source.parent().unwrap_or_else(|| Path::new(&vault_path));
    let export_path = parent.join(format!("{}_annotated.pdf", stem));

    doc.save(&export_path)
        .map_err(|e| AppError::General(format!("Failed to save annotated PDF: {}", e)))?;

    Ok(export_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use std::path::Path;

    fn seed_file(dir: &Path, rel: &str, body: &str) -> String {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
        path.to_string_lossy().to_string()
    }

    /// A file node as `do_scan_directory` would leave one, plus whatever the
    /// user has since attached to it.
    fn seed_file_node(db: &DbBridge, id: &str, path: &str, tags: &[&str]) {
        db.upsert_node(&NodeMetadata {
            id: id.to_string(),
            node_type: "file".into(),
            title: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
            content: String::new(),
            properties: serde_json::json!({
                "path": path,
                "extension": "txt",
                "size": 4,
                "source_type": "local",
                "tags": tags,
                "people": [],
            }),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
            timestamp: 0,
            blocks: None,
        })
        .unwrap();
    }

    // ── Deleting ──────────────────────────────────────────────

    /// The bug this replaces: a click in the duplicate finder called
    /// `fs::remove_file` and the bytes were gone for good. They have to end up
    /// somewhere a person can get them back from.
    #[test]
    fn deleting_moves_the_bytes_into_the_trash_instead_of_destroying_them() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let file = seed_file(dir.path(), "assets/report.txt", "số liệu quý 4");

        trash_indexed_file(&vault, &file, &[&vault]).unwrap();

        assert!(
            !Path::new(&file).exists(),
            "the file must leave the folder it was deleted from"
        );
        let recovered = walkdir::WalkDir::new(dir.path().join(".trash"))
            .into_iter()
            .filter_map(Result::ok)
            .find(|e| e.file_type().is_file())
            .expect("something has to be in the trash");
        assert_eq!(
            std::fs::read_to_string(recovered.path()).unwrap(),
            "số liệu quý 4",
            "a trash that loses the bytes is just a slower delete"
        );
    }

    /// The security hole, stated as a test: the path arrives from the front
    /// end, and anything outside the folders the user opened to us is refused
    /// before a single byte moves.
    #[test]
    fn a_path_outside_every_allowed_root_is_refused() {
        let vault_dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let vault = vault_dir.path().to_string_lossy().to_string();
        let outsider = seed_file(elsewhere.path(), "passwords.txt", "bí mật");

        let refused = trash_indexed_file(&vault, &outsider, &[&vault]);

        assert!(refused.is_err(), "a path outside the roots must be refused");
        assert!(
            Path::new(&outsider).exists(),
            "and refusing must not have touched the file"
        );
    }

    /// A folder the user added as a source is a legitimate place to delete
    /// from, or the feature does nothing for anyone indexing photos.
    #[test]
    fn a_registered_source_is_an_allowed_root() {
        let vault_dir = tempfile::tempdir().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let vault = vault_dir.path().to_string_lossy().to_string();
        let source = source_dir.path().to_string_lossy().to_string();
        let file = seed_file(source_dir.path(), "photo.txt", "ảnh");

        trash_indexed_file(&vault, &file, &[&vault, &source]).unwrap();

        assert!(!Path::new(&file).exists());
    }

    /// A file already gone from disk still has to leave the index, or it
    /// reappears in the list on every reload with nothing behind it.
    #[test]
    fn a_file_already_gone_from_disk_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();
        let missing = dir.path().join("gone.txt").to_string_lossy().to_string();

        assert!(trash_indexed_file(&vault, &missing, &[&vault]).is_ok());
    }

    /// Links are keyed by stable identity, and the old delete never cleared
    /// them at all — every backlink to a deleted file stayed in the graph.
    #[test]
    fn deleting_clears_the_row_the_search_entry_and_the_links() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_file_node(&db, "file-1", "/vault/assets/a.txt", &["hoá-đơn"]);
        let node = db.get_node("file-1").unwrap().unwrap();
        db.upsert_node_edge(&crate::db::NodeEdge {
            id: "e1".into(),
            source_id: node.stable_id().to_string(),
            target_id: "person-1".into(),
            edge_type: "mentions".into(),
            relation: None,
            created_at: "2026-01-01 00:00:00".into(),
        })
        .unwrap();

        forget_file(&db, "file-1", Some(&node)).unwrap();

        assert!(db.get_node("file-1").unwrap().is_none());
        assert!(
            db.get_all_node_edges().unwrap().is_empty(),
            "a backlink to a file that no longer exists is a dangling link"
        );
    }

    // ── Unlinking a source ────────────────────────────────────

    /// The garbage collector in `reindex_sources` only looks under folders that
    /// are still registered, so nothing else will ever clean these up: they
    /// would sit in the list forever, unreachable and unrefreshable.
    #[test]
    fn unlinking_a_source_forgets_the_files_it_indexed() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_file_node(&db, "in-source", "/Users/anh/Ảnh/a.txt", &[]);
        seed_file_node(&db, "elsewhere", "/Users/anh/Tài liệu/b.txt", &[]);

        let dropped = drop_indexed_files_under(&db, "/Users/anh/Ảnh").unwrap();

        assert_eq!(dropped, 1);
        assert!(db.get_node("in-source").unwrap().is_none());
        assert!(
            db.get_node("elsewhere").unwrap().is_some(),
            "unlinking one folder must not touch what another one indexed"
        );
    }

    // ── Identity comes from contents ──────────────────────────

    fn in_memory_state() -> DbState {
        std::sync::Mutex::new(DbBridge::new_in_memory_full().unwrap())
    }

    fn scan(state: &DbState, dir: &Path) -> Vec<String> {
        scan_source(state, &dir.to_string_lossy(), None, |_| {}).unwrap()
    }

    fn file_nodes(state: &DbState) -> Vec<crate::models::node::NodeMetadata> {
        let db = state.lock().unwrap();
        db.get_nodes_by_type("file").unwrap()
    }

    /// The headline claim of the whole change: rename a file in Finder and the
    /// tags you put on it are still there afterwards. Under path identity the
    /// renamed file was a stranger and the tags were stranded on a node nothing
    /// pointed at any more.
    #[test]
    fn renaming_a_file_outside_the_app_keeps_its_tags() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        let before = seed_file(dir.path(), "hoa-don.pdf", "số liệu quý 4");
        scan(&state, dir.path());

        // The user tags it.
        {
            let db = state.lock().unwrap();
            let mut node = db.get_nodes_by_type("file").unwrap().remove(0);
            node.properties["tags"] = serde_json::json!(["thuế"]);
            db.upsert_node(&node).unwrap();
        }

        // Finder, not us.
        let after = dir.path().join("hoá đơn 2026.pdf");
        std::fs::rename(&before, &after).unwrap();
        scan(&state, dir.path());

        let nodes = file_nodes(&state);
        assert_eq!(nodes.len(), 1, "the same file must not become two items");
        assert_eq!(
            nodes[0].properties["tags"],
            serde_json::json!(["thuế"]),
            "this is the entire point of content identity"
        );
        assert_eq!(nodes[0].title, "hoá đơn 2026.pdf", "the new name is the name");
    }

    /// Two copies of one file are one item. Tag either and both carry the tag,
    /// which is also what makes the duplicate finder free.
    #[test]
    fn two_copies_of_one_file_share_a_single_identity() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "anh.jpg", "cùng nội dung");
        seed_file(dir.path(), "sao-luu/anh-copy.jpg", "cùng nội dung");

        scan(&state, dir.path());

        let nodes = file_nodes(&state);
        assert_eq!(nodes.len(), 1, "one identity for one set of contents");

        let db = state.lock().unwrap();
        assert_eq!(
            db.file_locations_for_node(&nodes[0].id).unwrap().len(),
            2,
            "and two places it can be found"
        );
        assert_eq!(db.duplicate_locations().unwrap().len(), 1);
    }

    /// Editing a file makes it a different file. If it kept its identity it
    /// would keep somebody else's tags.
    #[test]
    fn editing_a_file_moves_it_to_a_new_identity() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        let path = seed_file(dir.path(), "ghi-chu.txt", "bản đầu");
        scan(&state, dir.path());
        let first = file_nodes(&state).remove(0).id;

        std::fs::write(&path, "bản sau hoàn toàn khác").unwrap();
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let now = db.file_location_at(&path).unwrap().unwrap();
        assert_ne!(now.node_id, first, "different contents, different item");
        assert!(
            db.file_locations_for_node(&first).unwrap().is_empty(),
            "the old identity must stop claiming a path it no longer describes"
        );
    }

    /// A rescan reads the filesystem, which has no record of what a person
    /// attached. Losing it on every scan would make tagging pointless.
    #[test]
    fn a_rescan_keeps_what_the_filesystem_cannot_know() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "hoa-don.txt", "nội dung");
        scan(&state, dir.path());

        {
            let db = state.lock().unwrap();
            let mut node = db.get_nodes_by_type("file").unwrap().remove(0);
            node.properties["tags"] = serde_json::json!(["thuế"]);
            node.properties["people"] = serde_json::json!(["[Anh](synabit://person/p1)"]);
            db.upsert_node(&node).unwrap();
        }
        scan(&state, dir.path());

        let nodes = file_nodes(&state);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].properties["tags"], serde_json::json!(["thuế"]));
        assert_eq!(
            nodes[0].properties["people"],
            serde_json::json!(["[Anh](synabit://person/p1)"])
        );
    }

    /// A second scan of an unchanged folder must not read a single byte of it.
    /// This is what keeps content identity affordable on a photo library.
    #[test]
    fn an_unchanged_file_is_not_read_twice() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "a.txt", "x");
        seed_file(dir.path(), "b.txt", "y");

        let mut first = 0usize;
        scan_source(&state, &dir.path().to_string_lossy(), None, |p| {
            first = p.hashed
        })
        .unwrap();
        assert_eq!(first, 2, "both files are new, so both are read");

        // The cache deliberately refuses to answer for a file touched in the
        // last couple of seconds — see `STAT_SETTLE_MS`. Age both the files and
        // what was recorded about them past that window, rather than sleeping
        // through it.
        let settled = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        for name in ["a.txt", "b.txt"] {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(dir.path().join(name))
                .unwrap();
            file.set_modified(settled).unwrap();
        }
        // Re-record them at their new timestamps, which is what a scan run
        // after the files settled would have stored.
        scan(&state, dir.path());

        let mut second = 0usize;
        scan_source(&state, &dir.path().to_string_lossy(), None, |p| {
            second = p.hashed
        })
        .unwrap();
        assert_eq!(second, 0, "nothing changed, so nothing should be re-read");
    }

    // ── Losing sight of a file ────────────────────────────────

    /// A file deleted outside the app leaves the index, taking nothing with it
    /// worth keeping.
    #[test]
    fn a_file_that_disappears_leaves_the_index() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        let path = seed_file(dir.path(), "tam.txt", "tạm");
        scan(&state, dir.path());

        std::fs::remove_file(&path).unwrap();
        let seen: std::collections::HashSet<String> =
            scan(&state, dir.path()).into_iter().collect();
        let db = state.lock().unwrap();
        forget_missing(&db, &[dir.path().to_string_lossy().to_string()], &seen).unwrap();

        assert!(db.get_nodes_by_type("file").unwrap().is_empty());
        assert!(db.file_location_at(&path).unwrap().is_none());
    }

    /// But a file somebody tagged does not. The bytes may be sitting on another
    /// device, where this same node is about to be matched to them by content —
    /// deleting it here would throw that away.
    #[test]
    fn a_tagged_file_that_disappears_keeps_its_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        let path = seed_file(dir.path(), "quan-trong.txt", "giữ lại");
        scan(&state, dir.path());

        let node_id = {
            let db = state.lock().unwrap();
            let mut node = db.get_nodes_by_type("file").unwrap().remove(0);
            node.properties["tags"] = serde_json::json!(["giữ"]);
            db.upsert_node(&node).unwrap();
            node.id
        };

        std::fs::remove_file(&path).unwrap();
        let seen: std::collections::HashSet<String> =
            scan(&state, dir.path()).into_iter().collect();
        let db = state.lock().unwrap();
        forget_missing(&db, &[dir.path().to_string_lossy().to_string()], &seen).unwrap();

        assert!(
            db.get_node(&node_id).unwrap().is_some(),
            "the tags outlive any one copy of the bytes"
        );
        assert!(
            db.file_location_at(&path).unwrap().is_none(),
            "but it is no longer somewhere you can open"
        );
    }

    /// Deleting one of two copies is not deleting the file, and must not take
    /// the tags the other copy still needs.
    #[test]
    fn deleting_one_copy_leaves_the_other_and_its_tags() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        let first = seed_file(dir.path(), "a.txt", "trùng nhau");
        seed_file(dir.path(), "b.txt", "trùng nhau");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let node_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        db.delete_file_location(&first).unwrap();

        assert!(
            db.node_has_locations(&node_id),
            "the other copy is still there, so the item still exists"
        );
    }

    // ── Re-identifying an old vault ───────────────────────────

    /// The migration's whole job: a node written under the old scheme keeps its
    /// tags and gains a content identity.
    #[test]
    fn migrating_a_legacy_node_carries_its_tags_across() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        let path = seed_file(dir.path(), "cu.txt", "nội dung cũ");
        seed_file_node(&db, "uuid-cu", &path, &["quan-trọng"]);

        let plan = crate::file_index::migration::plan(&db).unwrap();
        assert_eq!(plan.legacy, 1);
        assert_eq!(plan.resolvable, 1);
        assert_eq!(plan.carrying_metadata, 1);

        assert_eq!(crate::file_index::migration::apply(&db).unwrap(), 1);

        let nodes = db.get_nodes_by_type("file").unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(
            crate::file_index::hash_from_node_id(&nodes[0].id).is_some(),
            "the id must now be a content identity"
        );
        assert_eq!(nodes[0].properties["tags"], serde_json::json!(["quan-trọng"]));
        assert!(db.node_has_locations(&nodes[0].id));
    }

    /// Two legacy nodes that turn out to be copies of one file become one, and
    /// arriving second is no reason to lose the first one's tags.
    #[test]
    fn migrating_two_copies_unions_their_tags() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        let a = seed_file(dir.path(), "a.txt", "trùng nhau");
        let b = seed_file(dir.path(), "b.txt", "trùng nhau");
        seed_file_node(&db, "uuid-a", &a, &["hoá-đơn"]);
        seed_file_node(&db, "uuid-b", &b, &["thuế"]);

        assert_eq!(crate::file_index::migration::plan(&db).unwrap().merges, 1);
        crate::file_index::migration::apply(&db).unwrap();

        let nodes = db.get_nodes_by_type("file").unwrap();
        assert_eq!(nodes.len(), 1);
        let tags = nodes[0].properties["tags"].as_array().unwrap();
        let mut names: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["hoá-đơn", "thuế"], "neither set of tags may be dropped");
    }

    /// An unplugged external drive must not cost the user their tags. The node
    /// is left exactly as it was, to be migrated whenever the file comes back.
    #[test]
    fn a_legacy_node_whose_file_is_missing_is_left_alone() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_file_node(&db, "uuid-mat", "/Volumes/O-cung-ngoai/anh.jpg", &["kỷ-niệm"]);

        let plan = crate::file_index::migration::plan(&db).unwrap();
        assert_eq!(plan.unresolvable, 1);
        assert_eq!(crate::file_index::migration::apply(&db).unwrap(), 0);

        let node = db.get_node("uuid-mat").unwrap().expect("still there");
        assert_eq!(node.properties["tags"], serde_json::json!(["kỷ-niệm"]));
    }

    /// Running it twice must be the same as running it once.
    #[test]
    fn migrating_is_safe_to_repeat() {
        let dir = tempfile::tempdir().unwrap();
        let db = DbBridge::new_in_memory_full().unwrap();
        let path = seed_file(dir.path(), "cu.txt", "nội dung");
        seed_file_node(&db, "uuid-cu", &path, &["thẻ"]);

        crate::file_index::migration::apply(&db).unwrap();
        let after_first = db.get_nodes_by_type("file").unwrap();
        assert_eq!(crate::file_index::migration::apply(&db).unwrap(), 0);
        let after_second = db.get_nodes_by_type("file").unwrap();

        assert_eq!(after_first.len(), after_second.len());
        assert_eq!(after_first[0].id, after_second[0].id);
    }

    // ── Reading what is inside ────────────────────────────────

    /// The claim P2 exists to make: search a phrase you remember from inside a
    /// document and find the document. Before this, the full-text index of
    /// every PDF in the vault was the word "pdf".
    #[test]
    fn a_phrase_inside_a_document_finds_the_document() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(
            dir.path(),
            "hop-dong.md",
            "Điều 7: bên B thanh toán trong vòng ba mươi ngày",
        );
        seed_file(dir.path(), "khac.md", "một tài liệu hoàn toàn khác");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        for (node_id, extension, path) in db.files_awaiting_text(10).unwrap() {
            let outcome = crate::file_text::extract(Path::new(&path), &extension);
            store_extraction(&db, &node_id, &outcome).unwrap();
        }

        let hits = db
            .search_files_filtered("ba mươi ngày", "", "", "", 10)
            .unwrap();
        assert_eq!(hits.len(), 1, "only the contract contains that phrase");
        assert_eq!(hits[0].title, "hop-dong.md");
        assert!(
            db.file_text_excerpt(&hits[0].id, "ba mươi ngày", 20)
                .unwrap()
                .contains("ba mươi ngày"),
            "and it must be able to say why it matched"
        );
    }

    /// Extraction is keyed by content identity, so a document is read once
    /// however many copies of it there are and however often it is rescanned.
    #[test]
    fn a_document_is_read_once_however_many_copies_exist() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "a.md", "cùng một nội dung dài");
        seed_file(dir.path(), "sao-luu/b.md", "cùng một nội dung dài");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let queue = db.files_awaiting_text(10).unwrap();
        assert_eq!(queue.len(), 1, "two copies, one document to read");

        for (node_id, extension, path) in queue {
            let outcome = crate::file_text::extract(Path::new(&path), &extension);
            store_extraction(&db, &node_id, &outcome).unwrap();
        }
        assert_eq!(db.files_awaiting_text_count().unwrap(), 0);
        assert!(
            db.files_awaiting_text(10).unwrap().is_empty(),
            "a rescan must not queue it again"
        );
    }

    /// A file that will never yield text is settled, not retried. Left
    /// unanswered it comes back on every pass for the life of the vault.
    #[test]
    fn a_file_with_no_text_is_settled_rather_than_retried() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        std::fs::write(dir.path().join("anh.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let queue = db.files_awaiting_text(10).unwrap();
        assert_eq!(queue.len(), 1);
        for (node_id, extension, path) in queue {
            let outcome = crate::file_text::extract(Path::new(&path), &extension);
            store_extraction(&db, &node_id, &outcome).unwrap();
        }

        assert_eq!(
            db.files_awaiting_text_count().unwrap(),
            0,
            "an image has no words, and that is an answer"
        );
    }

    /// A broken document must not come back forever either.
    #[test]
    fn a_document_that_cannot_be_read_is_also_settled() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_file_node(&db, "Files/aaa", "/khong/co/that.docx", &[]);

        store_extraction(
            &db,
            "Files/aaa",
            &crate::file_text::Extraction::Failed("not a zip".into()),
        )
        .unwrap();

        assert!(db.files_awaiting_text(10).unwrap().is_empty());
    }

    /// The point of keeping pages apart: a hit in a long document says where
    /// to look, not merely that it is in there somewhere.
    #[test]
    fn a_match_reports_which_page_it_is_on() {
        let db = DbBridge::new_in_memory_full().unwrap();
        db.store_file_text(
            "Files/sach",
            &[
                "chương một mở đầu".to_string(),
                "chương hai về thuế".to_string(),
                "chương ba kết luận".to_string(),
            ],
        )
        .unwrap();

        assert_eq!(
            db.first_page_matching("Files/sach", &["thuế".to_string()])
                .unwrap(),
            Some(2)
        );
        assert_eq!(
            db.first_page_matching("Files/sach", &["không".to_string(), "có".to_string()])
                .unwrap(),
            None
        );
    }

    /// Every word has to be on the same page, or "kết luận thuế" would point
    /// at a page holding only one of them.
    #[test]
    fn a_page_must_contain_every_word_to_match() {
        let db = DbBridge::new_in_memory_full().unwrap();
        db.store_file_text(
            "Files/sach",
            &[
                "thuế thu nhập".to_string(),
                "kết luận cuối".to_string(),
                "kết luận về thuế".to_string(),
            ],
        )
        .unwrap();

        assert_eq!(
            db.first_page_matching("Files/sach", &["kết luận".to_string(), "thuế".to_string()])
                .unwrap(),
            Some(3)
        );
    }

    /// The path the search box actually takes. The rest of the tests go
    /// through `search_files_filtered`, which is the assistant's route; this
    /// one goes through FTS5, which is what a person typing in the app hits —
    /// and it is the route that produces the quoted snippet.
    #[test]
    fn the_search_box_finds_a_phrase_inside_a_document_and_quotes_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(
            dir.path(),
            "bao-cao.md",
            "doanh thu quý bốn vượt kế hoạch mười hai phần trăm",
        );
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        for (node_id, extension, path) in db.files_awaiting_text(10).unwrap() {
            let outcome = crate::file_text::extract(Path::new(&path), &extension);
            store_extraction(&db, &node_id, &outcome).unwrap();
        }

        let parsed = crate::search::parse_query("vượt kế hoạch");
        let response = db.search_fts(&parsed, 1, 10).unwrap();

        let hit = response
            .results
            .iter()
            .find(|r| r.item_type == "file")
            .expect("the document must be findable by a phrase inside it");
        assert_eq!(hit.title, "bao-cao.md");
        // Each matched word is marked separately, so the phrase is not a
        // contiguous substring of the snippet — the marks are the point.
        assert!(
            hit.snippet.contains("<mark>vượt</mark>") && hit.snippet.contains("<mark>hoạch</mark>"),
            "the result has to quote what matched and mark it, got: {}",
            hit.snippet
        );
        assert!(
            hit.snippet.contains("doanh thu"),
            "and quote the surrounding sentence, got: {}",
            hit.snippet
        );
    }

    /// Deleting a file takes its words with it, or the index keeps answering
    /// with a document that is gone.
    #[test]
    fn deleting_a_file_forgets_what_was_written_in_it() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_file_node(&db, "Files/xoa", "/tmp/xoa.md", &[]);
        db.store_file_text("Files/xoa", &["nội dung sẽ bị xoá".to_string()])
            .unwrap();
        db.record_text_status("Files/xoa", crate::db::TextStatus::Indexed, 1, 18, 0)
            .unwrap();

        let node = db.get_node("Files/xoa").unwrap();
        forget_file(&db, "Files/xoa", node.as_ref()).unwrap();

        assert_eq!(db.file_text_joined("Files/xoa").unwrap(), "");
        assert!(db
            .search_files_filtered("nội dung sẽ bị xoá", "", "", "", 10)
            .unwrap()
            .is_empty());
    }

    // ── Working on many files at once ─────────────────────────

    /// Removals run before additions, so "replace these tags with this one"
    /// ends with the tag present. The other order silently drops it.
    #[test]
    fn retagging_applies_removals_before_additions() {
        let current = vec!["cũ".to_string(), "giữ".to_string()];
        assert_eq!(
            retag(&current, &["cũ".into()], &["cũ".into()]),
            vec!["giữ".to_string(), "cũ".to_string()]
        );
    }

    #[test]
    fn a_tag_a_file_already_has_is_not_added_twice() {
        let current = vec!["thuế".to_string()];
        assert_eq!(retag(&current, &["Thuế".into()], &[]), current);
    }

    #[test]
    fn removing_a_tag_ignores_its_case() {
        let current = vec!["Thuế".to_string(), "giữ".to_string()];
        assert_eq!(retag(&current, &[], &["thuế".into()]), vec!["giữ".to_string()]);
    }

    /// Blank input is a stray keystroke, not a tag.
    #[test]
    fn an_empty_tag_is_not_a_tag() {
        let current = vec!["a".to_string()];
        assert_eq!(retag(&current, &["".into(), "   ".into()], &[]), current);
    }

    // ── What a picture says about itself ──────────────────────

    /// Reading a picture again and finding nothing must not erase what an
    /// earlier read found: "no camera recorded" and "we did not manage to
    /// look" are different facts, and only one is worth keeping.
    #[test]
    fn a_later_read_that_finds_nothing_does_not_erase_what_is_known() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_file_node(&db, "Files/anh", "/Ảnh/a.jpg", &[]);

        let mut node = db.get_node("Files/anh").unwrap().unwrap();
        if let Some(props) = node.properties.as_object_mut() {
            props.insert("camera".into(), serde_json::json!("FUJIFILM X-T5"));
            props.insert("shot_at".into(), serde_json::json!("2026-06-14 09:12:33"));
        }
        db.upsert_node(&node).unwrap();

        // What `record_photo_facts` does with an empty reading.
        let after = db.get_node("Files/anh").unwrap().unwrap();
        let meta = FileMetadata::from_node(&after).unwrap();
        assert_eq!(meta.camera.as_deref(), Some("FUJIFILM X-T5"));
        assert_eq!(meta.shot_at.as_deref(), Some("2026-06-14 09:12:33"));
    }

    /// The filter offers the cameras that are actually in the library, and
    /// offers each of them once.
    #[test]
    fn the_camera_list_is_what_is_really_there() {
        let db = DbBridge::new_in_memory_full().unwrap();
        for (i, camera) in ["FUJIFILM X-T5", "NIKON Z 6", "FUJIFILM X-T5"]
            .iter()
            .enumerate()
        {
            seed_file_node(&db, &format!("Files/anh-{i}"), &format!("/Ảnh/{i}.jpg"), &[]);
            let mut node = db.get_node(&format!("Files/anh-{i}")).unwrap().unwrap();
            if let Some(props) = node.properties.as_object_mut() {
                props.insert("camera".into(), serde_json::json!(camera));
            }
            db.upsert_node(&node).unwrap();
        }
        // A file that carried no camera at all.
        seed_file_node(&db, "Files/khac", "/Tài liệu/a.pdf", &[]);

        assert_eq!(
            db.distinct_file_property("camera").unwrap(),
            vec!["FUJIFILM X-T5".to_string(), "NIKON Z 6".to_string()]
        );
    }

    // ── Which notes use a file ────────────────────────────────

    /// Write a note that embeds an attachment, the way the editor does.
    fn seed_note(db: &DbBridge, id: &str, title: &str, body: &str) {
        db.upsert_node(&NodeMetadata {
            id: id.to_string(),
            node_type: "note".into(),
            title: title.to_string(),
            content: body.to_string(),
            properties: serde_json::json!({}),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
            timestamp: 0,
            blocks: None,
        })
        .unwrap();
    }

    fn relink(db: &DbBridge, id: &str) {
        let node = db.get_node(id).unwrap().unwrap();
        let resolver = crate::commands::nodes::build_resolver(db);
        crate::commands::nodes::sync_node_edges(db, &node, &resolver);
    }

    /// The claim P4 exists to make: a note that shows a picture is recorded as
    /// using that picture, by identity. No edge from a note to a file existed
    /// before — `MD_LINK_RE` did not even list `file` among its link types — so
    /// the question was answered by scanning bodies for the filename.
    #[test]
    fn a_note_that_embeds_a_file_is_recorded_as_using_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "assets/so-do.png", "dữ liệu ảnh");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        seed_note(&db, "Notes/kien-truc.md", "Kiến trúc", "Sơ đồ: ![](assets/so-do.png)");
        relink(&db, "Notes/kien-truc.md");

        let users = db.nodes_linking_to(&file_id).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].2, "Kiến trúc");
        assert_eq!(users[0].3, "attachment", "an embed is not a mere mention");
    }

    /// The order this actually happens in.
    ///
    /// A note embedding a picture has existed for months; the file index is
    /// rebuilt every time the app opens. So the note is indexed — and its edges
    /// recorded — at a moment when the file node does not exist yet, and
    /// nothing re-reads the note afterwards.
    ///
    /// The earlier test did it the other way round, which is the easy order and
    /// not the real one.
    #[test]
    fn a_note_indexed_before_the_file_still_finds_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();

        // The note comes first, and its links are worked out now.
        {
            let db = state.lock().unwrap();
            seed_note(&db, "Notes/kien-truc.md", "Kiến trúc", "![](assets/so-do.png)");
            relink(&db, "Notes/kien-truc.md");
        }

        // The picture is indexed afterwards, as it is on every launch.
        seed_file(dir.path(), "assets/so-do.png", "dữ liệu ảnh");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        assert_eq!(
            db.nodes_linking_to(&file_id).unwrap().len(),
            1,
            "the note uses this picture; the order things were indexed in is not the reader's problem"
        );
    }

    /// Adoption must not go looking for work. A wikilink to a note that does
    /// not exist is a link to a missing note — not an invitation to attach it
    /// to whatever file happens to share the name.
    #[test]
    fn a_link_to_a_missing_note_is_not_adopted_by_a_file_of_that_name() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        {
            let db = state.lock().unwrap();
            seed_note(&db, "Notes/a.md", "Ghi chú", "Xem [[báo cáo]] để biết thêm.");
            relink(&db, "Notes/a.md");
        }

        seed_file(dir.path(), "assets/báo cáo.pdf", "nội dung");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        assert!(
            db.nodes_linking_to(&file_id).unwrap().is_empty(),
            "a wikilink names a note, whatever else shares the name"
        );
    }

    /// Editing an embedded picture gives it new contents and so a new identity.
    /// The note said "the picture at this path", and the picture at that path
    /// is the new one — so the link follows the path rather than staying with
    /// bytes the note never showed.
    #[test]
    fn editing_an_embedded_picture_carries_the_link_to_the_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        let path = seed_file(dir.path(), "assets/so-do.png", "bản đầu");
        scan(&state, dir.path());

        let before = {
            let db = state.lock().unwrap();
            seed_note(&db, "Notes/a.md", "Kiến trúc", "![](assets/so-do.png)");
            relink(&db, "Notes/a.md");
            let id = db.get_nodes_by_type("file").unwrap().remove(0).id;
            assert_eq!(db.nodes_linking_to(&id).unwrap().len(), 1);
            id
        };

        std::fs::write(&path, "bản vẽ đã sửa, hoàn toàn khác").unwrap();
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let after = db.file_location_at(&path).unwrap().unwrap().node_id;
        assert_ne!(after, before, "different contents, different item");
        assert_eq!(
            db.nodes_linking_to(&after).unwrap().len(),
            1,
            "the note still shows the picture at that path"
        );
    }

    /// A note that both embeds a file and links to it would end up with two
    /// rows for one relationship if adoption ignored the uniqueness rule.
    #[test]
    fn adopting_a_link_a_note_already_has_does_not_duplicate_it() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "assets/so-do.png", "dữ liệu ảnh");
        scan(&state, dir.path());
        let file_id = {
            let db = state.lock().unwrap();
            db.get_nodes_by_type("file").unwrap().remove(0).id
        };

        {
            let db = state.lock().unwrap();
            seed_note(&db, "Notes/a.md", "Kiến trúc", "![](assets/so-do.png)");
            relink(&db, "Notes/a.md");
            // A stale placeholder of the same shape, as an earlier indexing
            // would have left behind.
            db.upsert_node_edge(&crate::db::NodeEdge {
                id: "stale".into(),
                source_id: "Notes/a.md".into(),
                target_id: "ghost:assets/so-do.png".into(),
                edge_type: "attachment".into(),
                relation: Some("attachment".into()),
                created_at: "2026-01-01 00:00:00".into(),
            })
            .unwrap();
        }

        scan(&state, dir.path());

        let db = state.lock().unwrap();
        assert_eq!(db.nodes_linking_to(&file_id).unwrap().len(), 1);
    }

    /// Video and audio are written as HTML rather than as markdown, and were
    /// just as invisible.
    #[test]
    fn an_html_embed_counts_the_same_as_a_markdown_one() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "assets/phong-van.mp4", "dữ liệu video");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        seed_note(
            &db,
            "Notes/phong-van.md",
            "Phỏng vấn",
            r#"<video controls src="assets/phong-van.mp4"></video>"#,
        );
        relink(&db, "Notes/phong-van.md");

        assert_eq!(db.nodes_linking_to(&file_id).unwrap().len(), 1);
    }

    /// The old scan's headline failure: a file called `note.pdf` came back
    /// "referenced by" every note containing the word "note".
    #[test]
    fn a_word_that_happens_to_be_a_filename_is_not_a_reference() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "assets/note.pdf", "nội dung pdf");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        seed_note(&db, "Notes/a.md", "Ghi chú", "This note is about note taking.");
        relink(&db, "Notes/a.md");

        assert!(
            db.nodes_linking_to(&file_id).unwrap().is_empty(),
            "mentioning a word is not using a file"
        );
    }

    /// DoD: renaming a file must not break a backlink. It cannot, because the
    /// edge points at what the file *is* rather than at what it is called.
    #[test]
    fn renaming_a_file_leaves_every_backlink_standing() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        let before = seed_file(dir.path(), "so-do.png", "dữ liệu ảnh");
        scan(&state, dir.path());

        let file_id = {
            let db = state.lock().unwrap();
            let id = db.get_nodes_by_type("file").unwrap().remove(0).id;
            seed_note(&db, "Notes/a.md", "Kiến trúc", &format!("[sơ đồ](synabit://file/{id})"));
            relink(&db, "Notes/a.md");
            assert_eq!(db.nodes_linking_to(&id).unwrap().len(), 1);
            id
        };

        std::fs::rename(&before, dir.path().join("sơ đồ mới.png")).unwrap();
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        assert_eq!(
            db.nodes_linking_to(&file_id).unwrap().len(),
            1,
            "the file is the same file, so the link still points at it"
        );
    }

    /// DoD: removing the link from a note removes the backlink there and then,
    /// with no rescan of anything.
    #[test]
    fn removing_a_link_removes_the_backlink_immediately() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "assets/so-do.png", "dữ liệu ảnh");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        seed_note(&db, "Notes/a.md", "Kiến trúc", "![](assets/so-do.png)");
        relink(&db, "Notes/a.md");
        assert_eq!(db.nodes_linking_to(&file_id).unwrap().len(), 1);

        seed_note(&db, "Notes/a.md", "Kiến trúc", "Đã bỏ ảnh đi.");
        relink(&db, "Notes/a.md");

        assert!(db.nodes_linking_to(&file_id).unwrap().is_empty());
    }

    /// A note that both shows a file and links to it is one note using it.
    #[test]
    fn a_note_using_a_file_twice_is_listed_once() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        seed_file(dir.path(), "assets/so-do.png", "dữ liệu ảnh");
        scan(&state, dir.path());

        let db = state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        seed_note(
            &db,
            "Notes/a.md",
            "Kiến trúc",
            &format!("![](assets/so-do.png) và [lại nữa](synabit://file/{file_id})"),
        );
        relink(&db, "Notes/a.md");

        assert_eq!(db.nodes_linking_to(&file_id).unwrap().len(), 1);
    }

    // ── Asking the database instead of the browser ────────────

    /// Index a folder and hand back the state to query against.
    fn library(dir: &Path, files: &[(&str, &str)]) -> DbState {
        let state = in_memory_state();
        for (rel, body) in files {
            seed_file(dir, rel, body);
        }
        scan(&state, dir);
        state
    }

    fn page(
        state: &DbState,
        filter: crate::db::FileFilter,
        sort: crate::db::FileSort,
        descending: bool,
        offset: usize,
        limit: usize,
    ) -> crate::db::FilePage {
        let db = state.lock().unwrap();
        db.query_file_page(&filter, sort, descending, offset, limit).unwrap()
    }

    /// A page is a window, and the count is of the whole filtered set — the
    /// scrollbar has to know how far it goes before the rows exist.
    #[test]
    fn a_page_reports_the_size_of_the_whole_filtered_set() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<(String, String)> = (0..25)
            .map(|i| (format!("tep-{i:02}.txt"), format!("nội dung {i}")))
            .collect();
        let state = library(
            dir.path(),
            &files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>(),
        );

        let first = page(&state, Default::default(), crate::db::FileSort::Name, false, 0, 10);

        assert_eq!(first.files.len(), 10, "a page is a page");
        assert_eq!(first.total, 25, "and the count is of everything that matched");
    }

    /// Windows must not overlap or skip. A stable tiebreaker is what makes that
    /// true when several rows sort equal.
    #[test]
    fn consecutive_pages_cover_the_list_exactly_once() {
        let dir = tempfile::tempdir().unwrap();
        // All the same size and all written together, so the only thing keeping
        // the order stable is the tiebreaker.
        let files: Vec<(String, String)> = (0..30)
            .map(|i| (format!("tep-{i:02}.txt"), "x".to_string()))
            .collect();
        let state = library(
            dir.path(),
            &files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>(),
        );

        let mut seen = Vec::new();
        for offset in (0..30).step_by(7) {
            seen.extend(
                page(&state, Default::default(), crate::db::FileSort::Size, true, offset, 7)
                    .files
                    .into_iter()
                    .map(|f| f.path),
            );
        }

        let unique: std::collections::HashSet<_> = seen.iter().collect();
        assert_eq!(seen.len(), 30, "every row appears");
        assert_eq!(unique.len(), 30, "and none of them twice");
    }

    #[test]
    fn sorting_by_name_runs_both_ways() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path(), &[("a.txt", "1"), ("b.txt", "2"), ("c.txt", "3")]);

        let up = page(&state, Default::default(), crate::db::FileSort::Name, false, 0, 10);
        let down = page(&state, Default::default(), crate::db::FileSort::Name, true, 0, 10);

        assert_eq!(up.files[0].filename, "a.txt");
        assert_eq!(down.files[0].filename, "c.txt");
    }

    /// Narrowing happens in SQL now, so it has to narrow the count as well as
    /// the rows — a filtered list whose scrollbar still spans the library is
    /// worse than no filter.
    #[test]
    fn a_filter_narrows_the_count_as_well_as_the_page() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(
            dir.path(),
            &[("a.txt", "1"), ("b.txt", "2"), ("anh.png", "3")],
        );

        let only_text = page(
            &state,
            crate::db::FileFilter {
                extensions: Some(vec!["txt".into()]),
                ..Default::default()
            },
            crate::db::FileSort::Name,
            false,
            0,
            10,
        );

        assert_eq!(only_text.total, 2);
        assert_eq!(only_text.files.len(), 2);
    }

    #[test]
    fn a_folder_filter_is_a_prefix_not_a_substring() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(
            dir.path(),
            &[("Ảnh/a.txt", "1"), ("Tài liệu/b.txt", "2")],
        );
        let inside = dir.path().join("Ảnh").to_string_lossy().to_string();

        let found = page(
            &state,
            crate::db::FileFilter { source_path: Some(inside), ..Default::default() },
            crate::db::FileSort::Name,
            false,
            0,
            10,
        );

        assert_eq!(found.total, 1);
        assert_eq!(found.files[0].filename, "a.txt");
    }

    /// A folder called `bảo_mật` must not act as a wildcard against every path
    /// with any character in that position.
    #[test]
    fn a_folder_name_with_sql_wildcards_in_it_is_taken_literally() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(
            dir.path(),
            &[("a_b/inside.txt", "1"), ("axb/elsewhere.txt", "2")],
        );
        let underscore = dir.path().join("a_b").to_string_lossy().to_string();

        let found = page(
            &state,
            crate::db::FileFilter { source_path: Some(underscore), ..Default::default() },
            crate::db::FileSort::Name,
            false,
            0,
            10,
        );

        assert_eq!(found.total, 1, "`_` is a wildcard in LIKE and must be escaped");
        assert_eq!(found.files[0].filename, "inside.txt");
    }

    /// A search that matched nothing matches nothing. Treated as "no filter",
    /// an empty result would widen to the entire library — which reads as the
    /// search having been ignored.
    #[test]
    fn a_search_that_found_nothing_shows_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path(), &[("a.txt", "1"), ("b.txt", "2")]);

        let found = page(
            &state,
            crate::db::FileFilter { search_ids: Some(vec![]), ..Default::default() },
            crate::db::FileSort::Relevance,
            false,
            0,
            10,
        );

        assert_eq!(found.total, 0);
    }

    /// A search computed a ranking; the list must not throw it away.
    #[test]
    fn search_results_keep_the_order_the_search_gave_them() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path(), &[("a.txt", "1"), ("b.txt", "2"), ("c.txt", "3")]);

        let ranked: Vec<String> = {
            let db = state.lock().unwrap();
            let mut ids: Vec<String> = db
                .get_nodes_by_type("file")
                .unwrap()
                .into_iter()
                .map(|n| n.id)
                .collect();
            ids.sort();
            ids.reverse(); // A ranking that is deliberately not alphabetical.
            ids
        };

        let found = page(
            &state,
            crate::db::FileFilter { search_ids: Some(ranked.clone()), ..Default::default() },
            crate::db::FileSort::Relevance,
            false,
            0,
            10,
        );

        let returned: Vec<String> = found.files.iter().map(|f| f.id.clone()).collect();
        assert_eq!(returned, ranked);
    }

    /// A node id arriving from a search goes into SQL as a literal. It is minted
    /// by this crate, but quoting it properly costs nothing and assuming it is
    /// safe costs everything.
    #[test]
    fn an_id_carrying_a_quote_cannot_break_out_of_the_query() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path(), &[("a.txt", "1")]);

        let found = page(
            &state,
            crate::db::FileFilter {
                search_ids: Some(vec!["'; DROP TABLE nodes; --".to_string()]),
                ..Default::default()
            },
            crate::db::FileSort::Relevance,
            false,
            0,
            10,
        );

        assert_eq!(found.total, 0);
        let db = state.lock().unwrap();
        assert_eq!(db.get_nodes_by_type("file").unwrap().len(), 1, "the table survives");
    }

    /// "Select all" is about the filtered set, not the page — and it counts
    /// identities, because that is what tagging works on.
    #[test]
    fn selecting_everything_returns_identities_not_copies() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(
            dir.path(),
            &[("a.txt", "trùng nhau"), ("sao-luu/b.txt", "trùng nhau"), ("c.txt", "khác")],
        );

        let db = state.lock().unwrap();
        let ids = db.query_file_ids(&Default::default(), 1000).unwrap();

        assert_eq!(ids.len(), 2, "two identities across three copies");
    }

    /// The sidebar used to build this by walking the whole list in the browser,
    /// which is the thing the list stopped being.
    #[test]
    fn tag_counts_come_from_the_database() {
        let db = DbBridge::new_in_memory_full().unwrap();
        seed_file_node(&db, "Files/a", "/v/a.txt", &["thuế", "2026"]);
        seed_file_node(&db, "Files/b", "/v/b.txt", &["thuế"]);
        seed_file_node(&db, "Files/c", "/v/c.txt", &[]);

        assert_eq!(
            db.file_tag_counts().unwrap(),
            vec![("2026".to_string(), 1), ("thuế".to_string(), 2)]
        );
    }

    /// Typing into the search box narrows the list immediately, before the
    /// full-text index has answered — and still narrows it if that never
    /// happens. Without this the box appeared to be ignored while you typed.
    #[test]
    fn a_name_filter_narrows_before_the_index_replies() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(
            dir.path(),
            &[("hop-dong.pdf", "1"), ("bao-cao.pdf", "2"), ("hop-dong-cu.pdf", "3")],
        );

        let found = page(
            &state,
            crate::db::FileFilter {
                name_contains: Some("hop-dong".into()),
                ..Default::default()
            },
            crate::db::FileSort::Name,
            false,
            0,
            10,
        );

        assert_eq!(found.total, 2);
    }

    /// A filename holding a `%` must not match every file.
    #[test]
    fn a_name_filter_takes_sql_wildcards_literally() {
        let dir = tempfile::tempdir().unwrap();
        let state = library(dir.path(), &[("giam-50%.txt", "1"), ("khac.txt", "2")]);

        let found = page(
            &state,
            crate::db::FileFilter { name_contains: Some("50%".into()), ..Default::default() },
            crate::db::FileSort::Name,
            false,
            0,
            10,
        );

        assert_eq!(found.total, 1);
    }

    // ── Cost of a scan ────────────────────────────────────────

    /// Not an assertion about wall-clock time on any particular machine — it
    /// is a probe, kept because scan cost is the thing most likely to regress
    /// here and the hardest to notice from a screenshot.
    ///
    /// `build_resolver` reads every node in the vault. Called once per file, as
    /// it was, a scan is quadratic in the size of the vault and a folder of ten
    /// thousand files takes tens of minutes. Run with:
    ///
    ///     cargo test --lib scan_cost -- --ignored --nocapture
    #[test]
    #[ignore = "timing probe, not a pass/fail assertion"]
    fn scan_cost_for_ten_thousand_files() {
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();
        for i in 0..10_000 {
            seed_file(dir.path(), &format!("thu-muc-{}/tep-{i}.txt", i % 50), &format!("noi dung {i}"));
        }

        let cold = std::time::Instant::now();
        scan(&state, dir.path());
        let cold = cold.elapsed();

        let warm = std::time::Instant::now();
        scan(&state, dir.path());
        let warm = warm.elapsed();

        println!("scan of 10k files — first: {cold:?}, rescan: {warm:?}");
        assert_eq!(file_nodes(&state).len(), 10_000);
    }

    /// What reading a library of office documents costs.
    ///
    /// Generated `.docx` rather than PDF: a real PDF cannot be conjured in a
    /// test without shipping a writer, and the zip-and-strip route is what the
    /// bulk of an ordinary document folder takes anyway. PDFs are slower per
    /// page — this number is a floor, not a promise about them.
    ///
    ///     cargo test --lib text_cost -- --ignored --nocapture
    #[test]
    #[ignore = "timing probe, not a pass/fail assertion"]
    fn text_cost_for_a_thousand_documents() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let state = in_memory_state();

        let body: String = (0..4_000)
            .map(|i| format!("<w:t>đoạn văn số {i} về thuế và hợp đồng</w:t>"))
            .collect();
        for n in 0..1_000 {
            let path = dir.path().join(format!("tai-lieu-{n}.docx"));
            let file = std::fs::File::create(&path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            zip.start_file("word/document.xml", options).unwrap();
            write!(zip, "<w:document><w:body>{body} số {n}</w:body></w:document>").unwrap();
            zip.finish().unwrap();
        }

        scan(&state, dir.path());

        let started = std::time::Instant::now();
        let mut read = 0usize;
        loop {
            let batch = {
                let db = state.lock().unwrap();
                db.files_awaiting_text(TEXT_BATCH).unwrap()
            };
            if batch.is_empty() {
                break;
            }
            let done: Vec<_> = batch
                .into_iter()
                .map(|(id, ext, path)| {
                    (id, crate::file_text::extract(Path::new(&path), &ext))
                })
                .collect();
            let db = state.lock().unwrap();
            for (id, outcome) in &done {
                store_extraction(&db, id, outcome).unwrap();
                read += 1;
            }
        }
        let elapsed = started.elapsed();

        println!(
            "1000 docx — read in {elapsed:?} ({:.1} ms each)",
            elapsed.as_secs_f64() * 1000.0 / read as f64
        );
        assert_eq!(read, 1_000);
    }

    /// What "which notes use this file?" costs on a real vault.
    ///
    /// The answer used to be a `content LIKE '%name%'` scan of every node, so
    /// the cost grew with the vault and with the length of every note in it.
    ///
    ///     cargo test --lib backlink_cost -- --ignored --nocapture
    #[test]
    #[ignore = "timing probe, not a pass/fail assertion"]
    fn backlink_cost_on_a_vault_of_ten_thousand_nodes() {
        let db = DbBridge::new_in_memory_full().unwrap();
        let file_id = crate::file_index::node_id_for(&"a".repeat(64));
        seed_file_node(&db, &file_id, "/vault/assets/so-do.png", &[]);

        // A vault of ordinary notes, forty of which show the picture.
        for i in 0..10_000 {
            let body = if i % 250 == 0 {
                "Sơ đồ: ![](assets/so-do.png)".to_string()
            } else {
                format!("Một ghi chú bình thường số {i}, với khá nhiều chữ trong đó để việc quét toàn văn phải thực sự đọc gì đó.")
            };
            seed_note(&db, &format!("Notes/ghi-chu-{i}.md"), &format!("Ghi chú {i}"), &body);
        }
        let resolver = crate::commands::nodes::build_resolver(&db);
        for i in 0..10_000 {
            let node = db.get_node(&format!("Notes/ghi-chu-{i}.md")).unwrap().unwrap();
            crate::commands::nodes::sync_node_edges(&db, &node, &resolver);
        }

        let started = std::time::Instant::now();
        let users = db.nodes_linking_to(&file_id).unwrap();
        let elapsed = started.elapsed();

        println!(
            "backlinks on 10k nodes — {elapsed:?} ({} hits)",
            users.len()
        );
        assert_eq!(users.len(), 40);
    }

    /// What listing a large library costs, end to end.
    ///
    /// `query_files` returns every indexed file in one array and the front end
    /// filters it in the browser. That is a shape with a ceiling, and this says
    /// where the ceiling is rather than leaving it to be discovered by a user.
    ///
    ///     cargo test --lib list_cost -- --ignored --nocapture
    #[test]
    #[ignore = "timing probe, not a pass/fail assertion"]
    fn list_cost_for_fifty_thousand_files() {
        let db = DbBridge::new_in_memory_full().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        for i in 0..50_000 {
            let hash = format!("{:064x}", i);
            let node_id = crate::file_index::node_id_for(&hash);
            let path = format!("/Users/anh/Ảnh/thu-muc-{}/tep-{i}.jpg", i % 100);
            db.upsert_node(&crate::models::node::NodeMetadata {
                id: node_id.clone(),
                node_type: "file".into(),
                title: format!("tep-{i}.jpg"),
                content: String::new(),
                properties: serde_json::json!({
                    "path": path, "extension": "jpg", "size": 2_400_000,
                    "source_type": "local", "tags": [], "people": [],
                }),
                created_at: "2026-01-01 00:00:00".into(),
                updated_at: "2026-01-01 00:00:00".into(),
                timestamp: 0,
                blocks: None,
            })
            .unwrap();
            db.upsert_file_location(
                &crate::db::FileLocation {
                    abs_path: path,
                    node_id,
                    size: 2_400_000,
                    mtime_ms: now,
                },
                now,
            )
            .unwrap();
        }

        // What a screenful now costs, which is what the app actually asks for.
        let started = std::time::Instant::now();
        let first_page = db
            .query_file_page(&Default::default(), crate::db::FileSort::Modified, true, 0, 60)
            .unwrap();
        let paged = started.elapsed();
        let payload = serde_json::to_string(&first_page.files).unwrap();
        println!(
            "50k files — one page of 60: {paged:?}, {} KB, total reported {}",
            payload.len() / 1024,
            first_page.total
        );

        let started = std::time::Instant::now();
        let listed = db.indexed_files().unwrap();
        let query = started.elapsed();

        let started = std::time::Instant::now();
        let metas: Vec<FileMetadata> = listed
            .into_iter()
            .filter_map(|(location, node)| {
                let mut meta = FileMetadata::from_node(&node)?;
                meta.path = location.abs_path;
                meta.size = location.size;
                Some(meta)
            })
            .collect();
        let build = started.elapsed();

        let started = std::time::Instant::now();
        let payload = serde_json::to_string(&metas).unwrap();
        let serialise = started.elapsed();

        println!(
            "50k files — query: {query:?}, build: {build:?}, serialise: {serialise:?}, payload: {} MB",
            payload.len() / 1_048_576
        );
        assert_eq!(metas.len(), 50_000);
    }

    // ── Renaming ──────────────────────────────────────────────

    /// Notes embed attachments as `](assets/<name>)`, so a rename there breaks
    /// them. The old code skipped the rename but still reported success, and
    /// the front end had already shown the new name — so the file looked
    /// renamed until the next reload put the old name back.
    #[test]
    fn a_file_in_the_vault_assets_folder_is_recognised() {
        assert!(is_vault_asset("/vault", "/vault/assets/anh.png"));
        assert!(is_vault_asset("/vault", "/vault/assets/con/anh.png"));
        assert!(!is_vault_asset("/vault", "/vault/Notes/anh.png"));
        assert!(
            !is_vault_asset("/vault", "/vault/assets-cu/anh.png"),
            "a sibling folder whose name merely starts the same way is not assets"
        );
    }
}
