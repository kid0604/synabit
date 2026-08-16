use std::collections::HashSet;
use std::path::Path;
use walkdir::WalkDir;

use crate::db::DbState;
use crate::error::{logged, AppResult};
use crate::models::node::NodeMetadata;
use crate::path_utils;
use crate::utils::graph_parser::{extract_resolved_node_edges, NodeResolver};
use crate::utils::node_parser::parse_file_to_node;

/// What a vault scan did, and what it could not do.
///
/// Returned rather than logged alone so the interface can say "1,482 indexed,
/// 3 skipped" instead of leaving the user to guess why a note never turns up in
/// search. Existing callers ignore the value, which is why adding it is safe.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ScanReport {
    /// Files parsed and written to the index on this run.
    pub indexed: usize,
    /// Nodes dropped because the file behind them is gone.
    pub removed: usize,
    /// Files the scan could not fully index. The vault is intact; the index is
    /// incomplete for these, and the log records what went wrong with each.
    pub failed: usize,
}

/// Helper: extract and sync node_edges for a node.
///
/// Returns whether the graph now matches the node.
pub(crate) fn sync_node_edges(
    db: &crate::db::DbBridge,
    node: &NodeMetadata,
    resolver: &NodeResolver,
) -> bool {
    let mut ok = logged(
        "clear old links",
        &node.id,
        db.delete_node_edges_by_source(&node.id),
    );
    for edge in extract_resolved_node_edges(node, resolver) {
        ok &= logged("record link", &node.id, db.upsert_node_edge(&edge));
    }
    ok
}

/// Helper: delete node_edges for a source
fn delete_node_edges_for(db: &crate::db::DbBridge, source_id: &str) -> bool {
    logged(
        "clear links",
        source_id,
        db.delete_node_edges_by_source(source_id),
    )
}

/// Build a NodeResolver from all nodes in the DB
pub(crate) fn build_resolver(db: &crate::db::DbBridge) -> NodeResolver {
    let all_nodes = db.get_all_nodes().unwrap_or_default();
    NodeResolver::new(&all_nodes)
}
/// Returns whether the search index now reflects the node.
fn sync_node_to_search(db: &crate::db::DbBridge, node: &NodeMetadata) -> bool {
    let mut tags_str = String::new();
    let mut status = None;
    let mut props_search = serde_json::to_string(&node.properties).unwrap_or_default();

    if let Some(tags) = node.properties.get("tags").and_then(|v| v.as_array()) {
        let tags_vec: Vec<String> = tags
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        tags_str = tags_vec.join(" ");
    }
    if let Some(s) = node.properties.get("status").and_then(|v| v.as_str()) {
        status = Some(s.to_string());
    }
    if let Some(p) = node.properties.get("priority").and_then(|v| v.as_str()) {
        props_search = format!("{} priority:{}", props_search, p);
    }

    db.upsert_search_entry(
        &node.id,
        &node.node_type,
        &node.title,
        &tags_str,
        &node.content,
        &props_search,
        status.as_deref(),
        &node.updated_at,
        &node.id,
    );

    match node.blocks.clone() {
        Some(blocks) => logged(
            "index blocks",
            &node.id,
            db.upsert_node_blocks(&node.id, blocks),
        ),
        None => true,
    }
}

#[tauri::command]
pub fn scan_all_nodes(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
) -> AppResult<ScanReport> {
    scan_vault_into_db(&app_handle, state.inner(), &vault_path)
}

/// The vault scan itself, with Tauri's command plumbing peeled off.
///
/// Kept generic over the runtime and handed the database state rather than a
/// concrete handle so it can be driven from tests and benchmarks against a mock
/// runtime. The command wrapper above cannot be: `tauri::AppHandle` names the
/// real runtime, and nothing in a test can produce one.
///
/// It takes the `DbState` rather than an already-locked guard on purpose.
/// `load_or_register_vault_identity` locks that same mutex internally, and the
/// mutex is not reentrant, so acquiring the lock before calling it deadlocks.
/// The identity call has to come first and finish first.
pub(crate) fn scan_vault_into_db<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    db_state: &DbState,
    vault_path: &str,
) -> AppResult<ScanReport> {
    let mut report = ScanReport::default();

    let base_dir = Path::new(vault_path);
    if !base_dir.exists() {
        return Ok(report);
    }

    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(app_handle, vault_path)?;
    let vault_id = identity.vault_id.to_string();

    // Read what the database already knows, then let go of it. Everything
    // between here and the writes below is filesystem work — walking, reading,
    // parsing — and used to be done with the lock held, which on a large vault
    // meant seconds during which nothing else could read the database.
    let (existing_nodes, existing_timestamps, resolver) = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        let existing_nodes = db.get_all_nodes()?;
        let mut existing_timestamps = std::collections::HashMap::new();
        for n in &existing_nodes {
            existing_timestamps.insert(n.id.clone(), n.timestamp);
        }
        // Built once for the whole scan: O(N) here, O(1) per link resolved.
        let resolver = NodeResolver::new(&existing_nodes);
        (existing_nodes, existing_timestamps, resolver)
    };

    let mut current_disk_files = HashSet::new();
    let mut batch: Vec<(NodeMetadata, std::path::PathBuf)> = Vec::with_capacity(SCAN_BATCH);

    for entry in WalkDir::new(base_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        if entry.file_type().is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "md" || ext == "json" || ext == "canvas" {
                let rel_path = path_utils::to_relative(path, vault_path);

                // Skip hidden folders like .git and .Trash, plus the directories
                // the vault manages by other means. Judged on the vault-relative
                // path: the absolute one carries the vault's own ancestors, and a
                // vault living under any dotted directory would otherwise skip
                // every file it contains.
                if is_in_unscanned_dir(&rel_path) {
                    continue;
                }

                current_disk_files.insert(rel_path.clone());

                if let Ok(metadata) = entry.metadata() {
                    let modified = metadata
                        .modified()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    let timestamp = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;

                    let needs_update = match existing_timestamps.get(&rel_path) {
                        Some(&ts) => timestamp > ts,
                        None => true,
                    };

                    if needs_update {
                        if let Some(node) = parse_file_to_node(vault_path, path) {
                            batch.push((node, path.to_path_buf()));
                            if batch.len() >= SCAN_BATCH {
                                index_batch(
                                    db_state, &mut batch, base_dir, &vault_id, &resolver,
                                    &mut report,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    index_batch(
        db_state,
        &mut batch,
        base_dir,
        &vault_id,
        &resolver,
        &mut report,
    );

    {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        report.removed =
            remove_orphaned_nodes(&db, base_dir, &existing_nodes, &current_disk_files);
    }

    if report.failed > 0 {
        log::warn!(
            "vault scan finished with {} file(s) only partly indexed",
            report.failed
        );
    }

    Ok(report)
}

/// How many files the scan indexes per acquisition of the database lock.
///
/// The scan used to hold the lock from the first file to the last, so a vault
/// that took four seconds to index was four seconds in which nothing else could
/// read the database — which during startup is the entire interface. Batching
/// trades a few extra lock acquisitions for a gap other callers can get through.
/// At roughly half a millisecond of database work per file, a hundred files is
/// on the order of fifty milliseconds of held lock.
const SCAN_BATCH: usize = 100;

/// Write one batch of parsed nodes, holding the lock only for that batch.
///
/// Parsing already happened, outside the lock. What is left is the part that
/// genuinely needs the database: the CRDT bridge, the row, its links and its
/// search entry.
fn index_batch(
    db_state: &DbState,
    batch: &mut Vec<(NodeMetadata, std::path::PathBuf)>,
    base_dir: &Path,
    vault_id: &str,
    resolver: &NodeResolver,
    report: &mut ScanReport,
) {
    if batch.is_empty() {
        return;
    }

    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    for (node, abs_path) in batch.drain(..) {
        let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // A failure here is reported and stepped over. One corrupt CRDT
        // document or one file that cannot be given an identity used to abort
        // the scan, leaving every file after it in the walk unindexed and the
        // user with a vault that had silently stopped updating.
        let mut ok = logged(
            "bridge external edits",
            &node.id,
            bridge_external_edits(&db, base_dir, &abs_path, vault_id, &node.id, ext),
        );

        ok &= logged("write node", &node.id, db.upsert_node(&node));
        ok &= sync_node_edges(&db, &node, resolver);
        ok &= sync_node_to_search(&db, &node);

        if ok {
            report.indexed += 1;
        } else {
            report.failed += 1;
        }
    }
}

/// Mirror a file's current bytes into the CRDT layer that sync reads from.
///
/// Lifted out of the scan loop so a failure on one file can be reported and
/// stepped over rather than ending the walk.
fn bridge_external_edits(
    db: &crate::db::DbBridge,
    base_dir: &Path,
    path: &Path,
    vault_id: &str,
    rel_path: &str,
    ext: &str,
) -> AppResult<()> {
    let Ok(file_content) = std::fs::read_to_string(path) else {
        // Not every file in a vault is text. Nothing to mirror, nothing wrong.
        return Ok(());
    };

    let node_id = crate::sync::core::identity::get_or_assign_node_id(base_dir, path)?;
    db.upsert_document_path(vault_id, &node_id, rel_path)?;

    // JSON is replaced wholesale; merging its characters between two devices
    // produces something that is valid text and invalid JSON.
    if ext == "json" || ext == "canvas" {
        sync_crdt_snapshot_replace(db, vault_id, &node_id, &file_content)
    } else {
        crdt_apply_safe(db, vault_id, &node_id, &file_content)
    }
}

/// Whether a node's id names a file the vault scan is responsible for.
///
/// Node ids come in two shapes. A node parsed from disk is keyed by its
/// vault-relative path; a node the database manages on its own — a `file` node,
/// say — is keyed by a bare UUID. Only the first kind can be judged absent by
/// looking at the disk, so only the first kind may be cleaned up that way.
///
/// This used to be a list of node types, which quietly decided the question for
/// types nobody thought to add to it: `project` and `person` are ordinary
/// Markdown files, but deleting one outside the app left its row, its edges and
/// its search entry behind for good. Asking about the id instead answers the
/// question that was actually being asked, and answers it for types that do not
/// exist yet.
fn is_disk_backed_id(id: &str) -> bool {
    matches!(
        Path::new(id).extension().and_then(|e| e.to_str()),
        Some("md") | Some("json") | Some("canvas")
    )
}

/// Vault-relative segments the scan does not walk into.
///
/// The cleanup pass deletes any disk-backed node the walk did not encounter, so
/// it has to agree with the walk about where the walk went — hence one
/// predicate, used by both. A node under one of these directories is missing
/// from the scan for a reason that has nothing to do with whether its file is
/// still there, and deleting it would be reading "not looked at" as "not there".
fn is_in_unscanned_dir(rel_id: &str) -> bool {
    rel_id.split(['/', '\\']).any(|name| {
        (name.starts_with('.') && name != ".")
            || name == "assets"
            || name == "Files"
            || name == "Syn"
    })
}

/// Drop nodes whose backing file is gone, along with everything keyed to them.
///
/// Split out from `scan_all_nodes` so it can be tested against a real database:
/// the command itself needs an `AppHandle`, and the delete cascade — row, edges,
/// blocks, search entry — is the part worth proving.
fn remove_orphaned_nodes(
    db: &crate::db::DbBridge,
    base_dir: &Path,
    existing_nodes: &[NodeMetadata],
    current_disk_files: &HashSet<String>,
) -> usize {
    // Having walked past a file is good evidence it exists, and checking the
    // set is far cheaper than asking the filesystem about every node. But its
    // absence is not evidence of anything: the walk is not instantaneous, and
    // the scan no longer holds the database lock throughout, so a file created
    // while it was running is missing from the set for reasons that have
    // nothing to do with whether it is there.
    //
    // So the set decides what to keep, and the disk decides what to delete.
    let is_gone = |id: &str| !current_disk_files.contains(id) && !base_dir.join(id).exists();

    let mut removed = 0;
    for n in existing_nodes {
        // Syn/ entries should never have been indexed at all; purge them
        // whatever their shape, and before the unscanned-directory guard below
        // would otherwise protect them.
        let is_syn = n.id.starts_with("Syn/") || n.id.starts_with("Syn\\");

        let is_orphan =
            is_disk_backed_id(&n.id) && !is_in_unscanned_dir(&n.id) && is_gone(&n.id);

        if is_syn || is_orphan {
            logged("drop node", &n.id, db.delete_node(&n.id));
            delete_node_edges_for(db, &n.id);
            logged("drop blocks", &n.id, db.delete_node_blocks(&n.id));
            db.delete_search_entry(&n.id);
            removed += 1;
        }
    }
    removed
}

#[tauri::command]
pub fn scan_specific_nodes(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    paths: Vec<String>,
) -> AppResult<ScanReport> {
    let mut report = ScanReport::default();

    let base_dir = Path::new(&vault_path);
    if !base_dir.exists() {
        return Ok(report);
    }

    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(&app_handle, &vault_path)?;
    let vault_id = identity.vault_id.to_string();

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let resolver = build_resolver(&db);

    for rel_path in paths {
        // Validate path stays within vault
        let abs_path = match path_utils::resolve_safe_path(&vault_path, &rel_path) {
            Ok(p) => p,
            Err(_) => continue, // Skip invalid paths silently
        };

        if abs_path.exists() && abs_path.is_file() {
            if let Some(node) = parse_file_to_node(&vault_path, &abs_path) {
                let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");

                // As in the full scan: one file's failure is reported, not
                // allowed to abandon the rest of the batch.
                let mut ok = logged(
                    "bridge external edits",
                    &rel_path,
                    bridge_external_edits(
                        &db,
                        Path::new(&vault_path),
                        &abs_path,
                        &vault_id,
                        &rel_path,
                        ext,
                    ),
                );

                ok &= logged("write node", &node.id, db.upsert_node(&node));
                ok &= sync_node_edges(&db, &node, &resolver);
                ok &= sync_node_to_search(&db, &node);

                if ok {
                    report.indexed += 1;
                } else {
                    report.failed += 1;
                }
            }
        } else {
            // File was deleted
            logged("drop node", &rel_path, db.delete_node(&rel_path));
            delete_node_edges_for(&db, &rel_path);
            logged("drop blocks", &rel_path, db.delete_node_blocks(&rel_path));
            db.delete_search_entry(&rel_path);
            report.removed += 1;
        }
    }

    Ok(report)
}

#[tauri::command]
pub fn get_all_nodes(
    state: tauri::State<'_, DbState>,
) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_all_nodes()
}

#[tauri::command]
pub fn get_node(
    state: tauri::State<'_, DbState>,
    id: String,
) -> AppResult<Option<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_node(&id)
}

#[tauri::command]
pub fn get_nodes(
    state: tauri::State<'_, DbState>,
    node_type: String,
) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_nodes_by_type(&node_type)
}

/// Every node of a type, without their bodies — what a list screen needs.
///
/// Use `get_node` for the one node the user actually opens. Reaching for
/// `get_nodes` to populate a list sends the whole vault's text across for the
/// sake of a few lines of preview.
#[tauri::command]
pub fn get_node_summaries(
    state: tauri::State<'_, DbState>,
    node_type: String,
) -> AppResult<Vec<crate::models::node::NodeSummary>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_node_summaries_by_type(&node_type)
}

#[tauri::command]
pub fn get_linked_nodes(
    state: tauri::State<'_, DbState>,
    target_title: String,
    target_id: Option<String>,
) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let id_str = target_id.unwrap_or_default();
    db.get_linked_nodes(&target_title, &id_str)
}

#[tauri::command]
pub fn get_node_block(
    state: tauri::State<'_, DbState>,
    node_id: String,
    block_id: String,
) -> AppResult<Option<String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // Read the one node asked for. This used to load every node in the vault
    // and search the result in memory, which cost the whole vault's content to
    // answer a question about a single line of one note.
    let node = match db.get_node(&node_id)? {
        Some(n) => n,
        None => return Ok(None),
    };

    // Scan content for the line containing ^block_id marker
    let marker = format!(" ^{}", block_id);
    let re = block_id_regex();

    for line in node.content.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with(&marker) {
            // Return line content WITHOUT the ^id marker
            let clean = re.replace(trimmed, "").to_string();
            // Also strip frontmatter-style prefixes like "# ", "## ", etc.
            return Ok(Some(clean.trim().to_string()));
        }
    }

    Ok(None) // Block marker was deleted from source
}

/// Returned by get_node_headings for each parseable block in a note.
#[derive(serde::Serialize)]
pub struct BlockPreview {
    pub block_id: String,
    pub content_preview: String,
    pub raw_content: String,     // Full original line text for file matching
    pub block_type: String,      // "h1", "h2", "h3", "paragraph"
    pub has_persistent_id: bool, // true if ^id already exists in file
}

/// Generate a 6-char lowercase alphanumeric block ID
fn generate_block_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    (0..6)
        .map(|_| {
            let idx = rng.random_range(0..36u32);
            if idx < 10 {
                (b'0' + idx as u8) as char
            } else {
                (b'a' + (idx - 10) as u8) as char
            }
        })
        .collect()
}

/// Helper: find safe char boundary at or before byte index (UTF-8 safe)
fn safe_split(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Regex pattern for ^block-id markers at end of line.
///
/// Compiled once. `parse_blocks_from_content` calls this per line of a note,
/// which made rebuilding it per call the dominant cost of reading headings.
static BLOCK_ID_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r" \^([a-z0-9]{6})$").unwrap());

fn block_id_regex() -> &'static regex::Regex {
    &BLOCK_ID_RE
}

/// Parse note content into block previews, detecting existing ^id markers.
fn parse_blocks_from_content(content: &str) -> Vec<BlockPreview> {
    // Strip frontmatter
    let body = if content.starts_with("---") {
        if let Some(end_pos) = content[3..].find("---") {
            let skip = 3 + end_pos + 3;
            if skip <= content.len() {
                content[skip..].trim()
            } else {
                content
            }
        } else {
            content
        }
    } else {
        content
    };

    let re = block_id_regex();
    let mut blocks = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for existing ^id marker
        let (clean_text, existing_id) = if let Some(caps) = re.captures(trimmed) {
            let id = caps[1].to_string();
            let text_end = caps.get(0).unwrap().start();
            (trimmed[..text_end].to_string(), Some(id))
        } else {
            (trimmed.to_string(), None)
        };

        // Determine block type
        let (block_type, preview) = if let Some(rest) = clean_text.strip_prefix("### ") {
            ("h3".to_string(), rest.trim().to_string())
        } else if let Some(rest) = clean_text.strip_prefix("## ") {
            ("h2".to_string(), rest.trim().to_string())
        } else if let Some(rest) = clean_text.strip_prefix("# ") {
            ("h1".to_string(), rest.trim().to_string())
        } else if !clean_text.starts_with("- ")
            && !clean_text.starts_with("* ")
            && !clean_text.starts_with("> ")
            && !clean_text.starts_with("```")
            && !clean_text.starts_with("|")
        {
            let preview = if clean_text.len() > 120 {
                format!("{}…", safe_split(&clean_text, 120))
            } else {
                clean_text.clone()
            };
            ("paragraph".to_string(), preview)
        } else {
            continue;
        };

        // Use existing ^id if present, otherwise use a content hash as temporary display ID
        let has_persistent_id = existing_id.is_some();
        let block_id = existing_id.unwrap_or_else(|| {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(clean_text.trim().as_bytes());
            let result = hasher.finalize();
            format!(
                "{:02x}{:02x}{:02x}{:02x}",
                result[0], result[1], result[2], result[3]
            )
        });

        blocks.push(BlockPreview {
            block_id,
            content_preview: preview,
            raw_content: clean_text,
            block_type,
            has_persistent_id,
        });
    }

    blocks
}

#[tauri::command]
pub fn get_node_headings(
    state: tauri::State<'_, DbState>,
    node_id: String,
) -> AppResult<Vec<BlockPreview>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    let node = match db.get_node(&node_id)? {
        Some(n) => n,
        None => return Ok(vec![]),
    };

    Ok(parse_blocks_from_content(&node.content))
}

#[tauri::command]
pub fn create_block_reference(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_id: String,
    content_snippet: String,
) -> AppResult<String> {
    let block_id = generate_block_id();
    let marker = format!(" ^{}", block_id);

    // Read the source file
    let abs_path = path_utils::resolve_safe_path(&vault_path, &node_id)?;
    let file_content = std::fs::read_to_string(&abs_path)
        .map_err(|e| crate::error::AppError::General(format!("Failed to read file: {}", e)))?;

    // Find the line matching content_snippet and append ^id
    let snippet_trimmed = content_snippet.trim();
    let mut found = false;
    let mut updated_lines: Vec<String> = Vec::new();

    for line in file_content.lines() {
        if !found && line.trim().contains(snippet_trimmed) {
            // Check if this line already has a ^id — don't add another
            let re = block_id_regex();
            if let Some(caps) = re.captures(line.trim()) {
                // Already has ^id, return existing one
                let existing_id = caps[1].to_string();
                return Ok(existing_id);
            }
            updated_lines.push(format!("{}{}", line, marker));
            found = true;
        } else {
            updated_lines.push(line.to_string());
        }
    }

    if !found {
        return Err(crate::error::AppError::General(
            "Content snippet not found in source file".to_string(),
        ));
    }

    // Write back to disk
    let new_content = updated_lines.join("\n");
    std::fs::write(&abs_path, &new_content)
        .map_err(|e| crate::error::AppError::General(format!("Failed to write file: {}", e)))?;

    // Update DB with new file content
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(node) = parse_file_to_node(&vault_path, &abs_path) {
        logged("write node", &node.id, db.upsert_node(&node));
    }

    Ok(block_id)
}

#[tauri::command]
pub fn update_file_node_properties(
    state: tauri::State<'_, DbState>,
    id: String,
    properties: serde_json::Value,
) -> AppResult<()> {
    // This writes to the database and nowhere else, which is right for exactly
    // one kind of node: a `file` node, which describes a file the vault does
    // not own and has no document of its own on disk.
    //
    // For every other type the file *is* the node, and a properties change kept
    // only in the database would not reach disk and would not sync. Worse, it
    // would not be corrected either: the vault scan skips files whose mtime has
    // not moved, so the wrong value would sit there indefinitely and then
    // disappear the moment anything touched the file. Refusing is the only
    // honest answer this command can give.
    if is_disk_backed_id(&id) {
        return Err(crate::error::AppError::General(format!(
            "'{id}' is backed by a file on disk; use write_node_file so the change is written there too"
        )));
    }

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut node = db
        .get_node(&id)?
        .ok_or_else(|| crate::error::AppError::General("Node not found".to_string()))?;

    node.properties = properties.clone();
    db.upsert_node(&node)?;

    let resolver = build_resolver(&db);
    sync_node_edges(&db, &node, &resolver);

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

    Ok(())
}

#[tauri::command]
pub fn write_node_file(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
    title: String,
    node_type: String,
    properties: serde_json::Value,
    content: String,
) -> AppResult<()> {
    // The app writing a type it does not itself handle means a caller invented
    // one, which is a bug in the caller rather than something in the user's
    // vault. Still only a warning: the write is honest, and refusing it would
    // lose whatever the caller was trying to save.
    if !crate::models::node::NodeType::from(node_type.as_str()).is_known() {
        log::warn!(
            "writing '{}' with type '{}', which no part of the app queries",
            rel_path,
            node_type
        );
    }

    // Validate budget and spent strictly
    if let serde_json::Value::Object(map) = &properties {
        for key in &["budget", "spent"] {
            if let Some(val) = map.get(*key) {
                if let Some(s) = val.as_str() {
                    if !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit() || c == '.') {
                        return Err(crate::error::AppError::General(format!(
                            "Invalid number format for {}",
                            key
                        )));
                    }
                } else if !val.is_number() && !val.is_null() {
                    return Err(crate::error::AppError::General(format!(
                        "Invalid number format for {}",
                        key
                    )));
                }
            }
        }
    }

    let abs_path = path_utils::resolve_safe_path(&vault_path, &rel_path)?;

    // Ensure directory exists
    if let Some(parent) = abs_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Query old title before writing to disk
    let old_title: Option<String> = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_node_title(&rel_path)
    };

    // Construct the file content
    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let file_content = if ext == "json" || ext == "canvas" {
        let mut mut_props = properties.clone();
        let now = chrono::Utc::now().to_rfc3339();
        if let serde_json::Value::Object(ref mut map) = mut_props {
            if !map.contains_key("created_at") {
                map.insert(
                    "created_at".to_string(),
                    serde_json::Value::String(now.clone()),
                );
            }
            map.insert("updated_at".to_string(), serde_json::Value::String(now));
        }
        // Output as pure JSON
        let json_obj = serde_json::json!({
            "title": title.clone(),
            "type": node_type.clone(),
            "metadata": mut_props,
            "content": content.clone()
        });
        serde_json::to_string_pretty(&json_obj).unwrap_or_default()
    } else {
        let now = chrono::Utc::now().to_rfc3339();
        // Output as Markdown with YAML frontmatter
        let mut props_map = serde_yaml::Mapping::new();
        props_map.insert(
            serde_yaml::Value::String("title".to_string()),
            serde_yaml::Value::String(title.clone()),
        );
        props_map.insert(
            serde_yaml::Value::String("type".to_string()),
            serde_yaml::Value::String(node_type.clone()),
        );

        let mut has_created_at = false;

        // Merge user properties
        if let serde_json::Value::Object(map) = &properties {
            for (k, v) in map {
                if k == "title" || k == "type" || k == "updated_at" {
                    continue;
                } // Skip standard fields
                if k == "created_at" {
                    has_created_at = true;
                }
                if let Ok(yaml_val) = serde_yaml::to_value(v) {
                    props_map.insert(serde_yaml::Value::String(k.clone()), yaml_val);
                }
            }
        }

        if !has_created_at {
            props_map.insert(
                serde_yaml::Value::String("created_at".to_string()),
                serde_yaml::Value::String(now.clone()),
            );
        }
        props_map.insert(
            serde_yaml::Value::String("updated_at".to_string()),
            serde_yaml::Value::String(now),
        );

        let frontmatter = serde_yaml::to_string(&props_map).unwrap_or_default();
        // serde_yaml output usually ends with newline and might start with ---, but usually just standard YAML format
        // we manually add --- blocks to ensure Markdown compatibility
        let yaml_str = frontmatter.trim_start_matches("---\n");
        format!("---\n{}---\n{}", yaml_str, content)
    };

    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(&app_handle, &vault_path)?;
    let vault_id = identity.vault_id.to_string();

    // Before saving CRDT, we must determine the node_id
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // We write to disk first so that get_or_assign_node_id can work
    std::fs::write(&abs_path, &file_content)?;

    let vault_path_obj = std::path::Path::new(&vault_path);
    let node_id = crate::sync::core::identity::get_or_assign_node_id(vault_path_obj, &abs_path)?;

    db.upsert_document_path(&vault_id, &node_id, &rel_path)?;

    // Now that get_or_assign_node_id has potentially injected a UUID, read the injected content
    let final_file_content = std::fs::read_to_string(&abs_path).unwrap_or(file_content);

    // --- Phase 1: CRDT Bridge ---
    if ext == "json" || ext == "canvas" {
        // JSON files: snapshot replacement (last-write-wins)
        sync_crdt_snapshot_replace(&db, &vault_id, &node_id, &final_file_content)?;
    } else {
        // Markdown files: character-level CRDT merge with panic recovery
        crdt_apply_safe(&db, &vault_id, &node_id, &final_file_content)?;
    }
    // ----------------------------

    // Update DB immediately
    if let Some(node) = parse_file_to_node(&vault_path, &abs_path) {
        logged("write node", &node.id, db.upsert_node(&node));

        // Sync FTS5 Search Index
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

        let resolver = build_resolver(&db);
        sync_node_edges(&db, &node, &resolver);
        if let Some(old) = old_title {
            if old != node.title {
                drop(db); // release lock before updating other files
                let _ = update_node_mentions(&state, vault_path, old, node.title, node.id);
            }
        }
    }

    Ok(())
}

fn update_node_mentions(
    state: &tauri::State<'_, DbState>,
    vault_path: String,
    old_title: String,
    new_title: String,
    node_id: String,
) -> AppResult<()> {
    let linked_nodes = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_linked_nodes(&old_title, &node_id)
            .unwrap_or_default()
    };

    let vault_dir = Path::new(&vault_path);

    for node in linked_nodes {
        let file_path = vault_dir.join(&node.id);
        if !file_path.exists() {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(&file_path) {
            let updated = crate::utils::graph_parser::rename_links_in_text(
                &content,
                &old_title,
                &new_title,
                Some(&node_id),
            );
            if updated != content && std::fs::write(&file_path, updated).is_ok() {
                // Update DB synchronously for the linked file to avoid watcher race conditions
                if let Some(parsed_node) =
                    crate::utils::node_parser::parse_file_to_node(&vault_path, &file_path)
                {
                    let db = state.lock().unwrap_or_else(|e| e.into_inner());
                    logged("write node", &parsed_node.id, db.upsert_node(&parsed_node));

                    // Sync FTS5 Search Index
                    let tags = parsed_node
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
                    let status = parsed_node
                        .properties
                        .get("status")
                        .and_then(|s| s.as_str());
                    let props_str =
                        serde_json::to_string(&parsed_node.properties).unwrap_or_default();

                    db.upsert_search_entry(
                        &parsed_node.id,
                        &parsed_node.node_type,
                        &parsed_node.title,
                        &tags,
                        &parsed_node.content,
                        &props_str,
                        status,
                        &parsed_node.updated_at,
                        &parsed_node.id,
                    );

                    let resolver = build_resolver(&db);
                    sync_node_edges(&db, &parsed_node, &resolver);
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn delete_node_file(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
) -> AppResult<()> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &rel_path)?;

    if abs_path.exists() {
        std::fs::remove_file(abs_path)?;
    }

    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(&app_handle, &vault_path)?;
    let _vault_id = identity.vault_id.to_string();

    // Update DB immediately
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    logged("drop node", &rel_path, db.delete_node(&rel_path));
    delete_node_edges_for(&db, &rel_path);
    db.delete_search_entry(&rel_path);

    Ok(())
}

#[tauri::command]
pub fn rename_node_file(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    old_rel_path: String,
    new_name: String,
) -> AppResult<String> {
    let old_abs = path_utils::resolve_safe_path(&vault_path, &old_rel_path)?;

    if !old_abs.exists() {
        return Err(crate::error::AppError::InvalidPath(
            "File not found.".to_string(),
        ));
    }

    // Parse the current node
    let node = if let Some(n) = crate::utils::node_parser::parse_file_to_node(&vault_path, &old_abs)
    {
        n
    } else {
        return Err(crate::error::AppError::InvalidPath(
            "Failed to parse node.".to_string(),
        ));
    };

    let old_title = node.title.clone();

    // Update the title property and rewrite the file
    let mut props_map = serde_yaml::Mapping::new();
    props_map.insert(
        serde_yaml::Value::String("title".to_string()),
        serde_yaml::Value::String(new_name.clone()),
    );
    props_map.insert(
        serde_yaml::Value::String("type".to_string()),
        serde_yaml::Value::String(node.node_type.clone()),
    );

    let now = chrono::Utc::now().to_rfc3339();
    let mut has_created_at = false;

    if let serde_json::Value::Object(map) = &node.properties {
        for (k, v) in map {
            if k == "title" || k == "type" || k == "updated_at" {
                continue;
            }
            if k == "created_at" {
                has_created_at = true;
            }
            if let Ok(yaml_val) = serde_yaml::to_value(v) {
                props_map.insert(serde_yaml::Value::String(k.clone()), yaml_val);
            }
        }
    }

    if !has_created_at {
        props_map.insert(
            serde_yaml::Value::String("created_at".to_string()),
            serde_yaml::Value::String(now.clone()),
        );
    }
    props_map.insert(
        serde_yaml::Value::String("updated_at".to_string()),
        serde_yaml::Value::String(now),
    );

    let frontmatter = serde_yaml::to_string(&props_map).unwrap_or_default();
    let yaml_str = frontmatter.trim_start_matches("---\n");
    let file_content = format!("---\n{}---\n{}", yaml_str, node.content);

    std::fs::write(&old_abs, file_content)?;

    // Update DB and Mentions
    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(parsed_node) =
            crate::utils::node_parser::parse_file_to_node(&vault_path, &old_abs)
        {
            logged("write node", &parsed_node.id, db.upsert_node(&parsed_node));
            let tags = parsed_node
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
            let status = parsed_node
                .properties
                .get("status")
                .and_then(|s| s.as_str());
            let props_str = serde_json::to_string(&parsed_node.properties).unwrap_or_default();

            db.upsert_search_entry(
                &parsed_node.id,
                &parsed_node.node_type,
                &parsed_node.title,
                &tags,
                &parsed_node.content,
                &props_str,
                status,
                &parsed_node.updated_at,
                &parsed_node.id,
            );
            let resolver = build_resolver(&db);
            sync_node_edges(&db, &parsed_node, &resolver);

            if old_title != new_name {
                drop(db); // release lock
                let _ =
                    update_node_mentions(&state, vault_path, old_title, new_name, parsed_node.id);
            }
        }
    }

    Ok(old_rel_path)
}

/// Frontmatter for a newly created note, carrying its sync identity from the
/// first byte it ever has.
///
/// A note used to be created without one, so its identity was decided later by
/// whichever code path reached it first. When that turned out to be sync, sync
/// had to mint an id and write it into the file — a read-modify-write against a
/// file the editor may be saving at the same moment, with no coordination
/// between them. Whoever wrote second won, and what the loser wrote was either
/// the user's edit or the identity the vault had just agreed on.
///
/// Assigning it here does not make that write safe; it makes it unnecessary,
/// which is the only version of safe available without a lock spanning both.
fn new_note_frontmatter(title: &str, node_type: &str, tag: Option<&str>) -> String {
    let created_at = chrono::Utc::now().to_rfc3339();
    let mut out = format!(
        "---\nnode_id: {}\ntitle: \"{}\"\ntype: \"{}\"\ncreated_at: \"{}\"\nupdated_at: \"{}\"\n",
        uuid::Uuid::new_v4(),
        title,
        node_type,
        created_at,
        created_at
    );
    if let Some(tag) = tag.map(str::trim).filter(|t| !t.is_empty()) {
        out.push_str(&format!("tags:\n  - {}\n", tag));
    }
    out.push_str("---\n\n");
    out
}

#[tauri::command]
pub fn create_node_file(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    directory: String,
    node_type: String,
    date_format: Option<String>,
) -> AppResult<String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dir_path = path_utils::resolve_safe_path(&vault_path, &directory)?;
    if !dir_path.exists() {
        std::fs::create_dir_all(&dir_path)?;
    }

    let title = if let Some(fmt_str) = date_format {
        let chrono_format = fmt_str
            .replace("YYYY", "%Y")
            .replace("YY", "%y")
            .replace("MM", "%m")
            .replace("M", "%-m")
            .replace("DD", "%d")
            .replace("D", "%-d");
        chrono::Local::now().format(&chrono_format).to_string()
    } else {
        let start = SystemTime::now();
        let timestamp = start
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("Untitled {}", timestamp)
    };

    let filename = format!("{}.md", uuid::Uuid::new_v4());

    let path = dir_path.join(&filename);

    if !path.exists() {
        let content = new_note_frontmatter(&title, &node_type, None);
        std::fs::write(&path, content)?;

        // Sync DB immediately
        if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &path)
        {
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            logged("write node", &parsed_node.id, db.upsert_node(&parsed_node));

            let tags = parsed_node
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
            let status = parsed_node
                .properties
                .get("status")
                .and_then(|s| s.as_str());
            let props_str = serde_json::to_string(&parsed_node.properties).unwrap_or_default();

            db.upsert_search_entry(
                &parsed_node.id,
                &parsed_node.node_type,
                &parsed_node.title,
                &tags,
                &parsed_node.content,
                &props_str,
                status,
                &parsed_node.updated_at,
                &parsed_node.id,
            );
        }
    }

    let rel_path = path_utils::to_relative(&path, &vault_path);
    Ok(rel_path)
}

#[tauri::command]
pub fn open_daily_note(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    format_str: String,
    tag: String,
) -> AppResult<String> {
    let notes_dir = Path::new(&vault_path).join("Notes");
    if !notes_dir.exists() {
        std::fs::create_dir_all(&notes_dir)?;
    }

    // Convert common YYYY-MM-DD pattern to chrono's format
    let chrono_format = format_str
        .replace("YYYY", "%Y")
        .replace("YY", "%y")
        .replace("MM", "%m")
        .replace("M", "%-m")
        .replace("DD", "%d")
        .replace("D", "%-d");

    let today = chrono::Local::now();
    let date_str = today.format(&chrono_format).to_string();

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let notes = db.get_nodes_by_type("note").unwrap_or_default();

    if let Some(existing) = notes.iter().find(|n| n.title == date_str) {
        return Ok(existing.id.clone());
    }

    let filename = format!("{}.md", uuid::Uuid::new_v4());
    let path = notes_dir.join(&filename);

    let title = date_str.clone();
    let content = new_note_frontmatter(&title, "note", Some(&tag));
    std::fs::write(&path, content)?;

    // Sync DB immediately to avoid race condition with frontend scanVault
    if let Some(parsed_node) = crate::utils::node_parser::parse_file_to_node(&vault_path, &path) {
        logged("write node", &parsed_node.id, db.upsert_node(&parsed_node));

        let tags = parsed_node
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
        let status = parsed_node
            .properties
            .get("status")
            .and_then(|s| s.as_str());
        let props_str = serde_json::to_string(&parsed_node.properties).unwrap_or_default();

        db.upsert_search_entry(
            &parsed_node.id,
            &parsed_node.node_type,
            &parsed_node.title,
            &tags,
            &parsed_node.content,
            &props_str,
            status,
            &parsed_node.updated_at,
            &parsed_node.id,
        );
    }

    let rel_path = path_utils::to_relative(&path, &vault_path);
    Ok(rel_path)
}

#[tauri::command]
pub fn archive_done_nodes(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_type: String,
    days: u64,
) -> AppResult<u32> {
    use chrono::NaiveDate;

    // Map node_type to its default directory name
    let dir_name = match node_type.as_str() {
        "task" => "Tasks",
        "event" => "Events",
        "note" => "Notes",
        _ => return Ok(0),
    };

    let base_dir = Path::new(&vault_path).join(dir_name);
    if !base_dir.exists() {
        return Ok(0);
    }

    let archived_dir = base_dir.join("archived");
    let today = chrono::Local::now().date_naive();
    let mut archived_count: u32 = 0;

    // We only process items in DB for that type
    let nodes = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_nodes_by_type(&node_type)?
    };

    for node in nodes {
        // Node must be "done"
        if let Some(status) = node.properties.get("status").and_then(|s| s.as_str()) {
            if status != "done" {
                continue;
            }

            // Node must have completed_at
            if let Some(completed_at) = node.properties.get("completed_at").and_then(|c| c.as_str())
            {
                if completed_at.is_empty() {
                    continue;
                }

                let date_part = completed_at.split_whitespace().next().unwrap_or("");
                if let Ok(completed_date) = NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                    let elapsed = today.signed_duration_since(completed_date).num_days();
                    if elapsed >= days as i64 {
                        let abs_path = Path::new(&vault_path).join(&node.id);
                        if abs_path.exists() {
                            if !archived_dir.exists() {
                                let _ = std::fs::create_dir_all(&archived_dir);
                            }
                            let file_name = abs_path.file_name().unwrap_or_default();
                            let dest = archived_dir.join(file_name);
                            if std::fs::rename(&abs_path, &dest).is_ok() {
                                archived_count += 1;

                                // The row and the search entry are keyed by the
                                // old path, so they go; the next scan re-adds
                                // the file at its new one.
                                //
                                // Its links do not go with them. Edges are keyed
                                // by the node's stable identity, which the file
                                // carries into `archived/` — deleting them here
                                // is what used to make archiving a task quietly
                                // erase every backlink to it.
                                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                                logged("drop node", &node.id, db.delete_node(&node.id));
                                db.delete_search_entry(&node.id);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(archived_count)
}

#[tauri::command]
pub fn save_asset(vault_path: String, filename: String, bytes: Vec<u8>) -> AppResult<String> {
    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        std::fs::create_dir_all(&assets_dir)?;
    }

    let extension = Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let safe_filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let target_path = assets_dir.join(&safe_filename);

    std::fs::write(&target_path, bytes)?;
    Ok(format!("assets/{}", safe_filename))
}

#[tauri::command]
pub fn copy_asset_to_vault(vault_path: String, source_path: String) -> AppResult<String> {
    let source = Path::new(&source_path);
    if !source.exists() || !source.is_file() {
        return Err(crate::error::AppError::InvalidPath(
            "Source file does not exist or is not a regular file".to_string(),
        ));
    }
    // Validate the output stays within vault
    path_utils::resolve_safe_path(&vault_path, "assets")?;

    let assets_dir = Path::new(&vault_path).join("assets");
    if !assets_dir.exists() {
        std::fs::create_dir_all(&assets_dir)?;
    }

    let extension = source.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let target = assets_dir.join(&filename);

    std::fs::copy(source, target)?;

    Ok(format!("assets/{}", filename))
}

#[cfg(desktop)]
#[tauri::command]
pub fn spawn_node_window(app_handle: tauri::AppHandle, node_id: String) -> AppResult<()> {
    use std::time::{SystemTime, UNIX_EPOCH};
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let encoded_node_id = urlencoding::encode(&node_id);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| crate::error::AppError::General(format!("System time error: {}", e)))?
        .as_micros();
    let window_label = format!("node_{}", timestamp);

    let url = WebviewUrl::App(format!("index.html?floatingNote={}", encoded_node_id).into());

    let _ = WebviewWindowBuilder::new(&app_handle, window_label, url)
        .title("Node View")
        .inner_size(600.0, 700.0)
        .minimizable(true)
        .maximizable(true)
        .closable(true)
        .build()
        .map_err(|e| crate::error::AppError::General(e.to_string()))?;

    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn spawn_node_window(_app_handle: tauri::AppHandle, _node_id: String) -> AppResult<()> {
    Err(crate::error::AppError::General(
        "Multiple windows are not supported on mobile".to_string(),
    ))
}

/// List all PDF files in the vault's assets/ directory.
#[tauri::command]
pub fn list_pdf_files(vault_path: String) -> AppResult<Vec<serde_json::Value>> {
    let assets_dir = Path::new(&vault_path).join("assets");
    let mut pdfs = Vec::new();

    if !assets_dir.exists() {
        return Ok(pdfs);
    }

    for entry in WalkDir::new(&assets_dir)
        .max_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("pdf") {
            let rel_path = path_utils::to_relative(path, &vault_path);
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let name = filename
                .strip_suffix(".pdf")
                .or_else(|| filename.strip_suffix(".PDF"))
                .unwrap_or(&filename)
                .to_string();

            pdfs.push(serde_json::json!({
                "name": name,
                "path": rel_path
            }));
        }
    }

    pdfs.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.cmp(nb)
    });

    Ok(pdfs)
}

/// Apply CRDT update for JSON files using snapshot replacement (last-write-wins).
/// JSON structured data is unsuitable for character-level CRDT merge — merging
/// individual characters of `{"amount":123}` between two devices produces garbage.
/// Instead, we replace the entire CRDT document with a fresh snapshot each time.
/// Public so sync module can use it as a fallback when CRDT merge panics.
pub fn sync_crdt_snapshot_replace(
    db: &crate::db::DbBridge,
    vault_id: &str,
    node_id: &str,
    content: &str,
) -> AppResult<()> {
    let doc = loro::LoroDoc::new();
    let peer_id = db.get_or_create_peer_id()?;
    doc.set_peer_id(peer_id)
        .map_err(|e| crate::error::AppError::General(format!("set_peer_id error: {:?}", e)))?;
    let text = doc.get_text("content");
    text.insert(0, content)
        .map_err(|e| crate::error::AppError::General(format!("CRDT insert error: {:?}", e)))?;
    doc.commit();
    let snapshot = doc.export_snapshot();
    db.replace_crdt_snapshot(vault_id, node_id, &snapshot)?;
    Ok(())
}

/// Apply CRDT update for Markdown files using character-level diff merge.
/// Wrapped in catch_unwind to prevent Loro panics from crashing the entire app.
/// If the CRDT document is corrupted, it falls back to snapshot replacement.
pub(crate) fn crdt_apply_safe(
    db: &crate::db::DbBridge,
    vault_id: &str,
    node_id: &str,
    content: &str,
) -> AppResult<()> {
    let doc = db.get_crdt_doc(vault_id, node_id)?;
    let doc_ref = &doc;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::sync::core::crdt::apply_text_update(doc_ref, content)
    }));

    match result {
        Ok(Ok(delta)) => {
            if !delta.is_empty() {
                db.save_crdt_delta(vault_id, node_id, delta)?;
            }
            Ok(())
        }
        Ok(Err(e)) => Err(crate::error::AppError::General(format!(
            "CRDT update error for {}: {}",
            node_id, e
        ))),
        Err(_panic) => Err(crate::error::AppError::General(format!(
            "CRDT panic caught for {}",
            node_id
        ))),
    }
}

/// A stopwatch on the vault scan, kept out of the normal test run.
///
/// The scan's cost is dominated by things that do not show up in a unit test:
/// one implicit transaction — and therefore one WAL flush — per statement, and
/// a global database lock held for the whole walk. Both are only visible at
/// vault sizes nobody creates by hand, so this builds one.
///
/// ```text
/// cargo test --lib scan_benchmark -- --ignored --nocapture
/// SYNABIT_BENCH_FILES=20000 cargo test --lib scan_benchmark -- --ignored --nocapture
/// ```
#[cfg(test)]
mod scan_benchmark {
    use super::*;
    use crate::db::DbBridge;
    use tauri::Manager;

    fn app_with_db(db: DbBridge) -> tauri::AppHandle<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle().clone();
        handle.manage(crate::db::DbState::new(db));
        handle
    }

    /// Build a vault of `count` notes spread over subdirectories.
    ///
    /// Each note carries a `node_id` already, which is the steady state after
    /// the identity-at-creation change: the scan then reads identities instead
    /// of minting and writing them back. Measuring the other case would measure
    /// a one-off import, not the startup cost every launch pays.
    /// Extra prose per note, in KB, from `SYNABIT_BENCH_BODY_KB`.
    ///
    /// Defaults to none, which models a vault of short captures. Whether
    /// `content` is worth withholding from list queries depends entirely on
    /// this number, so it has to be adjustable rather than assumed.
    fn body_padding() -> String {
        let kb: usize = std::env::var("SYNABIT_BENCH_BODY_KB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if kb == 0 {
            return String::new();
        }
        let paragraph = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, \
                         sed do eiusmod tempor incididunt ut labore. ";
        let mut out = String::with_capacity(kb * 1024);
        while out.len() < kb * 1024 {
            out.push_str(paragraph);
        }
        format!("\n\n{out}")
    }

    fn generate_vault(dir: &std::path::Path, count: usize) {
        let padding = body_padding();
        for i in 0..count {
            let sub = dir.join(format!("Notes/{:03}", i % 100));
            std::fs::create_dir_all(&sub).unwrap();
            let body = format!(
                "---\nnode_id: {}\ntitle: \"Note {i}\"\ntype: \"note\"\ntags:\n  - bench\n  - t{}\n---\n\n\
                 Body of note {i}. It links to [[Note {}]] and mentions #bench.\n\n\
                 A second paragraph with a block marker. ^blk{:04}\n{}",
                uuid::Uuid::new_v4(),
                i % 7,
                (i + 1) % count.max(1),
                i % 10000,
                padding,
            );
            std::fs::write(sub.join(format!("note-{i}.md")), body).unwrap();
        }
    }

    #[test]
    #[ignore = "benchmark: run explicitly with --ignored --nocapture"]
    fn scanning_a_large_vault() {
        let count: usize = std::env::var("SYNABIT_BENCH_FILES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5_000);

        let holder = tempfile::tempdir().unwrap();
        let vault = holder.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        let built = std::time::Instant::now();
        generate_vault(&vault, count);
        let build_time = built.elapsed();

        let db = DbBridge::new_in_memory_full().unwrap();
        let handle = app_with_db(db);
        let vault_path = vault.to_string_lossy().to_string();

        let state = handle.state::<crate::db::DbState>();

        let cold = std::time::Instant::now();
        let cold_report =
            scan_vault_into_db(&handle, state.inner(), &vault_path).expect("cold scan");
        let cold_time = cold.elapsed();

        let warm = std::time::Instant::now();
        let warm_report =
            scan_vault_into_db(&handle, state.inner(), &vault_path).expect("warm scan");
        let warm_time = warm.elapsed();

        assert_eq!(cold_report.indexed, count, "the cold scan skipped files");
        assert_eq!(cold_report.failed, 0, "the cold scan reported failures");
        assert_eq!(
            warm_report.indexed, 0,
            "the warm scan reindexed files nothing had touched"
        );

        let indexed: i64 = {
            let db = state.lock().unwrap();
            db.conn()
                .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get::<_, i64>(0))
                .unwrap()
        };

        println!("\n─── vault scan ─────────────────────────────");
        println!("  files generated : {count} (in {build_time:?})");
        println!("  nodes indexed   : {indexed}");
        println!("  cold scan       : {cold_time:?}");
        println!("  warm scan       : {warm_time:?}  (nothing changed on disk)");
        println!(
            "  per file (cold) : {:?}",
            cold_time / count.max(1) as u32
        );
        println!("────────────────────────────────────────────\n");

        assert_eq!(indexed, count as i64, "every generated note should index");
    }

    /// What a list screen pays to open.
    ///
    /// Every mini-app that shows a list calls `get_nodes(type)`, which returns
    /// each node in full — `content` included — and hands the lot to the
    /// frontend as JSON. Whether that is worth avoiding depends on numbers
    /// nobody has taken: the query itself, and the serialisation that follows
    /// it, which is the part IPC actually charges for.
    #[test]
    #[ignore = "benchmark: run explicitly with --ignored --nocapture"]
    fn what_a_list_screen_pays_to_open() {
        let count: usize = std::env::var("SYNABIT_BENCH_FILES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5_000);

        let holder = tempfile::tempdir().unwrap();
        let vault = holder.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        generate_vault(&vault, count);
        let vault_path = vault.to_string_lossy().to_string();

        let db = DbBridge::new_in_memory_full().unwrap();
        for entry in walkdir::WalkDir::new(&vault)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Some(node) = parse_file_to_node(&vault_path, entry.path()) {
                db.upsert_node(&node).unwrap();
            }
        }

        let t = std::time::Instant::now();
        let nodes = db.get_nodes_by_type("note").unwrap();
        let full_query = t.elapsed();
        let t = std::time::Instant::now();
        let full_json = serde_json::to_string(&nodes).unwrap();
        let full_serialise = t.elapsed();

        let t = std::time::Instant::now();
        let summaries = db.get_node_summaries_by_type("note").unwrap();
        let summary_query = t.elapsed();
        let t = std::time::Instant::now();
        let summary_json = serde_json::to_string(&summaries).unwrap();
        let summary_serialise = t.elapsed();

        assert_eq!(summaries.len(), nodes.len(), "both queries see every note");

        println!("\n─── opening a list of {count} notes ───");
        println!("  whole notes (get_nodes)");
        println!("    query      : {full_query:?}");
        println!("    serialise  : {full_serialise:?}");
        println!("    payload    : {} KB", full_json.len() / 1024);
        println!("  summaries (get_node_summaries)");
        println!("    query      : {summary_query:?}");
        println!("    serialise  : {summary_serialise:?}");
        println!("    payload    : {} KB", summary_json.len() / 1024);
        println!("──────────────────────────────────────\n");
    }

    /// The property batching exists for: the rest of the app can reach the
    /// database while a scan is running.
    ///
    /// Not a benchmark — a scan that holds the lock from first file to last
    /// fails this outright, however fast it is. Startup runs a scan and builds
    /// the interface at the same time, and the interface reads the database to
    /// draw anything at all.
    #[test]
    fn other_callers_can_reach_the_database_while_a_scan_is_running() {
        let holder = tempfile::tempdir().unwrap();
        let vault = holder.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        // Several batches' worth, so there are gaps to be caught between them.
        generate_vault(&vault, SCAN_BATCH * 3);

        let db = DbBridge::new_in_memory_full().unwrap();
        let handle = app_with_db(db);
        let vault_path = vault.to_string_lossy().to_string();

        let scanning = handle.clone();
        let scan = std::thread::spawn(move || {
            let state = scanning.state::<crate::db::DbState>();
            scan_vault_into_db(&scanning, state.inner(), &vault_path).expect("scan")
        });

        let state = handle.state::<crate::db::DbState>();
        let mut got_in = 0;
        while !scan.is_finished() {
            if let Ok(db) = state.inner().try_lock() {
                // Do what a real caller would: read something.
                let _ = db.get_all_nodes();
                got_in += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        let report = scan.join().expect("the scan thread should not panic");
        assert_eq!(report.indexed, SCAN_BATCH * 3, "the scan missed files");
        assert_eq!(report.failed, 0);
        // Measured at 29 windows for this vault while developing the change,
        // so the bar is set low enough to survive a fast machine and still
        // rule out having caught only the moment after the last batch.
        assert!(
            got_in > 1,
            "the database was reachable {got_in} time(s) during the scan: the lock is being held throughout"
        );
    }

    /// Attribute the scan's cost to its stages.
    ///
    /// The whole-scan number above says the scan is slow but not why, and the
    /// obvious suspect — one WAL flush per statement — cannot be the answer,
    /// because the measurement above runs against an in-memory database that
    /// never touches a disk. This runs each stage over the same files so the
    /// cost lands on whichever one is actually carrying it.
    ///
    /// One quirk in the query plans it prints: FTS5 reports every access as
    /// `SCAN ... VIRTUAL TABLE INDEX 0:`, whether or not it is really scanning.
    /// What distinguishes them is the index string after the colon — an `=`
    /// there means the rowid constraint was taken and the lookup is direct. The
    /// timings above are the real evidence either way: a cost per file that
    /// stays flat as the vault grows is one that is not reading the whole index.
    #[test]
    #[ignore = "benchmark: run explicitly with --ignored --nocapture"]
    fn where_the_scan_spends_its_time() {
        let count: usize = std::env::var("SYNABIT_BENCH_FILES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);

        let holder = tempfile::tempdir().unwrap();
        let vault = holder.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        generate_vault(&vault, count);
        let vault_path = vault.to_string_lossy().to_string();

        let files: Vec<std::path::PathBuf> = walkdir::WalkDir::new(&vault)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        // ── parse ───────────────────────────────────────────
        let t = std::time::Instant::now();
        let nodes: Vec<NodeMetadata> = files
            .iter()
            .filter_map(|p| parse_file_to_node(&vault_path, p))
            .collect();
        let parse = t.elapsed();

        let db = DbBridge::new_in_memory_full().unwrap();
        // CRDT rows are foreign-keyed to a registered vault.
        db.insert_sync_vault_mapping(&crate::db::sync_vault::SyncVaultRecord {
            vault_id: "bench-vault".into(),
            canonical_root: vault_path.clone(),
            metadata_version: 1,
            created_at: 100,
            updated_at: 100,
        })
        .unwrap();
        let resolver = NodeResolver::new(&nodes);

        // ── edges ───────────────────────────────────────────
        let t = std::time::Instant::now();
        for n in &nodes {
            let _ = crate::utils::graph_parser::extract_resolved_node_edges(n, &resolver);
        }
        let edges_parse = t.elapsed();

        // ── row upsert ──────────────────────────────────────
        let t = std::time::Instant::now();
        for n in &nodes {
            db.upsert_node(n).unwrap();
        }
        let upsert = t.elapsed();

        // ── search index ────────────────────────────────────
        let t = std::time::Instant::now();
        for n in &nodes {
            sync_node_to_search(&db, n);
        }
        let search = t.elapsed();

        // ── CRDT bridge ─────────────────────────────────────
        let t = std::time::Instant::now();
        for (i, n) in nodes.iter().enumerate() {
            crdt_apply_safe(&db, "bench-vault", &format!("doc-{i}"), &n.content).unwrap();
        }
        let crdt = t.elapsed();

        // Ask SQLite how it satisfies the delete half of `upsert_search_entry`,
        // rather than inferring it from the timings. Both forms are shown: the
        // one keyed on `item_id` is what the code used to issue, and it reads
        // the whole index every time — which is what made a vault scan
        // quadratic. The rowid form is what it issues now.
        let explain = |sql: &str| -> Vec<String> {
            let mut stmt = db.conn().prepare(sql).unwrap();
            let rows = stmt
                .query_map(["x"], |r| r.get::<_, String>(3))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            rows
        };
        let plan_by_item_id = explain("EXPLAIN QUERY PLAN DELETE FROM search_index WHERE item_id = ?1");
        let plan_by_rowid = explain("EXPLAIN QUERY PLAN DELETE FROM search_index WHERE rowid = ?1");
        let plan_lookup =
            explain("EXPLAIN QUERY PLAN SELECT fts_rowid FROM search_index_rowids WHERE item_id = ?1");

        let per = |d: std::time::Duration| d / count.max(1) as u32;
        println!("\n─── where the scan spends its time ({count} files) ───");
        println!("  parse file      : {parse:?}\t({:?}/file)", per(parse));
        println!("  extract edges   : {edges_parse:?}\t({:?}/file)", per(edges_parse));
        println!("  upsert node row : {upsert:?}\t({:?}/file)", per(upsert));
        println!("  search index    : {search:?}\t({:?}/file)", per(search));
        println!("  CRDT bridge     : {crdt:?}\t({:?}/file)", per(crdt));
        println!("  search delete, keyed on item_id (the old way):");
        for step in &plan_by_item_id {
            println!("      {step}");
        }
        println!("  search delete, keyed on rowid (the way it works now):");
        for step in &plan_by_rowid {
            println!("      {step}");
        }
        println!("  rowid lookup that makes it possible:");
        for step in &plan_lookup {
            println!("      {step}");
        }
        println!("──────────────────────────────────────────────────\n");
    }
}

/// Tests for the pass that decides a node's file is gone.
///
/// This is the only code in the app that deletes user data without the user
/// asking, so both of its mistakes are expensive: deleting a node whose file is
/// still there loses work, and keeping one whose file is gone leaves a ghost in
/// every list, search result and backlink for the life of the vault.
#[cfg(test)]
mod orphan_cleanup_tests {
    use super::*;
    use crate::db::{DbBridge, NodeEdge};

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    /// Seed a node together with everything else keyed to its id, so a test can
    /// tell a real cleanup from one that merely drops the row.
    fn seed(db: &DbBridge, id: &str, node_type: &str) -> NodeMetadata {
        let node = NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: id.to_string(),
            content: String::new(),
            properties: serde_json::json!({}),
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            timestamp: 0,
            blocks: None,
        };
        db.upsert_node(&node).unwrap();
        db.upsert_node_edge(&NodeEdge {
            id: format!("edge-{id}"),
            source_id: id.to_string(),
            target_id: "Notes/other.md".to_string(),
            edge_type: "wikilink".to_string(),
            relation: None,
            created_at: "2026-01-01 00:00:00".to_string(),
        })
        .unwrap();
        db.upsert_node_blocks(id, vec![("blk001".to_string(), "a block".to_string())])
            .unwrap();
        db.upsert_search_entry(id, node_type, id, "", "", "{}", None, "2026-01-01", id);
        node
    }

    fn search_entries(db: &DbBridge, item_id: &str) -> i64 {
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM search_index WHERE item_id = ?1",
                [item_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    /// A vault directory with nothing in it, so the disk check finds nothing.
    ///
    /// Held for the duration of the call it is passed to — long enough, since
    /// the cleanup only ever reads it.
    fn empty_vault() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn on_disk(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    /// The defect this change exists for. `project` is an ordinary Markdown
    /// file, but it was missing from the old list of cleanable types, so a
    /// project deleted outside the app stayed in the database for good.
    #[test]
    fn a_project_deleted_from_disk_is_erased_along_with_its_edges_blocks_and_search_entry() {
        let db = db();
        let node = seed(&db, "Projects/gone.md", "project");

        remove_orphaned_nodes(&db, empty_vault().path(), &[node], &on_disk(&[]));

        assert!(db.get_node("Projects/gone.md").unwrap().is_none());
        assert!(db
            .get_node_edges_for_node("Projects/gone.md")
            .unwrap()
            .is_empty());
        assert!(db.get_node_block("Projects/gone.md", "blk001").unwrap().is_none());
        assert_eq!(search_entries(&db, "Projects/gone.md"), 0);
    }

    /// Same defect, second type. Worth its own case: people are the only nodes
    /// a user is likely to delete by hand in Finder.
    #[test]
    fn a_person_deleted_from_disk_is_erased_too() {
        let db = db();
        let node = seed(&db, "People/someone.md", "person");

        remove_orphaned_nodes(&db, empty_vault().path(), &[node], &on_disk(&[]));

        assert!(db.get_node("People/someone.md").unwrap().is_none());
    }

    /// The protection the old type list was written to provide, kept intact.
    /// A `file` node is keyed by a bare UUID and has no file in the vault at
    /// all, so a scan that did not see it has learned nothing about it.
    #[test]
    fn a_database_managed_file_node_is_never_deleted_by_a_disk_scan() {
        let db = db();
        let node = seed(&db, "550e8400-e29b-41d4-a716-446655440000", "file");

        remove_orphaned_nodes(&db, empty_vault().path(), &[node], &on_disk(&[]));

        assert!(db
            .get_node("550e8400-e29b-41d4-a716-446655440000")
            .unwrap()
            .is_some());
    }

    /// The hazard the new rule introduces, and the guard against it: judging by
    /// file extension alone would condemn every node the walk deliberately
    /// skipped. If a future node type is parked under `assets/`, this fails
    /// rather than deleting the user's data.
    #[test]
    fn a_node_under_a_directory_the_scan_skips_is_left_alone() {
        let db = db();
        let node = seed(&db, "assets/embedded.json", "json");

        remove_orphaned_nodes(&db, empty_vault().path(), &[node], &on_disk(&[]));

        assert!(db.get_node("assets/embedded.json").unwrap().is_some());
    }

    /// `Syn/` is skipped by the walk like `assets/`, but unlike `assets/` its
    /// entries are legacy mistakes to be purged, so it has to beat the guard
    /// above rather than benefit from it.
    #[test]
    fn a_syn_entry_is_purged_even_though_syn_is_never_scanned() {
        let db = db();
        let node = seed(&db, "Syn/chat.md", "note");

        remove_orphaned_nodes(&db, empty_vault().path(), &[node], &on_disk(&[]));

        assert!(db.get_node("Syn/chat.md").unwrap().is_none());
    }

    #[test]
    fn a_node_whose_file_is_still_on_disk_survives() {
        let db = db();
        let node = seed(&db, "Notes/kept.md", "note");

        remove_orphaned_nodes(&db, empty_vault().path(), &[node], &on_disk(&["Notes/kept.md"]));

        assert!(db.get_node("Notes/kept.md").unwrap().is_some());
        assert_eq!(search_entries(&db, "Notes/kept.md"), 1);
    }

    /// Every disk-backed type the app writes today, swept in one pass, so
    /// adding a type to the app without revisiting this rule is caught here.
    #[test]
    fn every_disk_backed_type_is_cleaned_and_only_the_uuid_keyed_one_remains() {
        let db = db();
        let nodes: Vec<NodeMetadata> = [
            ("Notes/n.md", "note"),
            ("Tasks/t.md", "task"),
            ("Projects/p.md", "project"),
            ("Events/e.md", "event"),
            ("People/x.md", "person"),
            ("QuickCaps/q.md", "quickcap"),
            ("Finance/2026-08.json", "finance_month"),
            ("PDFAnnotations/a.json", "pdf_highlight"),
            ("Whiteboards/b.whiteboard.json", "whiteboard"),
            ("Boards/c.canvas", "canvas"),
            ("f47ac10b-58cc-4372-a567-0e02b2c3d479", "file"),
        ]
        .iter()
        .map(|(id, ty)| seed(&db, id, ty))
        .collect();

        remove_orphaned_nodes(&db, empty_vault().path(), &nodes, &on_disk(&[]));

        let survivors: Vec<String> = db
            .get_all_nodes()
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(survivors, vec!["f47ac10b-58cc-4372-a567-0e02b2c3d479"]);
    }

    /// The race the disk check closes.
    ///
    /// The scan releases the database lock between batches now, so a note
    /// created while it is running — by the editor, by sync, by the watcher —
    /// gets indexed by another path but is missing from the set of files this
    /// walk happened to pass. Treating that absence as proof the file is gone
    /// would delete a note the user had just written.
    #[test]
    fn a_node_created_after_the_walk_passed_its_folder_is_not_mistaken_for_an_orphan() {
        let db = db();
        let vault = empty_vault();
        std::fs::create_dir_all(vault.path().join("Notes")).unwrap();
        std::fs::write(vault.path().join("Notes/written-midway.md"), "---\n---\n").unwrap();

        let node = seed(&db, "Notes/written-midway.md", "note");

        // The walk never saw it: the set is empty.
        remove_orphaned_nodes(&db, vault.path(), &[node], &on_disk(&[]));

        assert!(
            db.get_node("Notes/written-midway.md").unwrap().is_some(),
            "a note whose file is on disk was deleted for not having been walked past"
        );
    }

    /// And the other direction still holds: a file that is genuinely gone is
    /// still removed, which is the entire point of the pass.
    #[test]
    fn a_node_missing_from_both_the_walk_and_the_disk_is_still_removed() {
        let db = db();
        let vault = empty_vault();
        let node = seed(&db, "Notes/deleted.md", "note");

        remove_orphaned_nodes(&db, vault.path(), &[node], &on_disk(&[]));

        assert!(db.get_node("Notes/deleted.md").unwrap().is_none());
    }

    #[test]
    fn the_number_of_nodes_dropped_is_reported() {
        let db = db();
        let gone_a = seed(&db, "Projects/a.md", "project");
        let gone_b = seed(&db, "Notes/b.md", "note");
        let kept = seed(&db, "Notes/c.md", "note");
        let managed = seed(&db, "550e8400-e29b-41d4-a716-446655440000", "file");

        let removed = remove_orphaned_nodes(
            &db,
            empty_vault().path(),
            &[gone_a, gone_b, kept, managed],
            &on_disk(&["Notes/c.md"]),
        );

        assert_eq!(removed, 2);
    }

    /// A failing index write has to be counted rather than swallowed, and it
    /// must not stop the writes that follow it. Forced here by removing the
    /// table underneath — the shape of a real failure (a corrupt or locked
    /// database) without needing one.
    #[test]
    fn an_index_write_that_fails_is_reported_and_does_not_stop_the_rest() {
        let db = db();
        db.conn().execute("DROP TABLE nodes", []).unwrap();

        let ok = logged(
            "write node",
            "Notes/a.md",
            db.upsert_node(&NodeMetadata {
                id: "Notes/a.md".to_string(),
                node_type: "note".to_string(),
                title: "A".to_string(),
                content: String::new(),
                properties: serde_json::json!({}),
                created_at: "2026-01-01 00:00:00".to_string(),
                updated_at: "2026-01-01 00:00:00".to_string(),
                timestamp: 0,
                blocks: None,
            }),
        );

        assert!(!ok, "a failed write must report itself as failed");

        // The search index lives in its own table and is still writable, which
        // is the point: one broken write does not abandon the others.
        db.upsert_search_entry("Notes/a.md", "note", "A", "", "", "{}", None, "", "Notes/a.md");
        let indexed: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM search_index WHERE item_id = 'Notes/a.md'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1);
    }

    /// The guard on `update_file_node_properties` reuses this predicate, so the
    /// two stay in step: whatever the scan considers a document, that command
    /// refuses to write behind.
    #[test]
    fn the_ids_the_property_update_refuses_are_exactly_the_disk_backed_ones() {
        for backed in ["Notes/a.md", "Projects/p.md", "Finance/Config.json", "b.canvas"] {
            assert!(
                is_disk_backed_id(backed),
                "{backed} should be refused a database-only property write"
            );
        }
        for managed in ["f47ac10b-58cc-4372-a567-0e02b2c3d479", "no-extension"] {
            assert!(
                !is_disk_backed_id(managed),
                "{managed} has no file, so a database-only write is all there is"
            );
        }
    }

    #[test]
    fn only_ids_naming_a_parseable_file_count_as_disk_backed() {
        assert!(is_disk_backed_id("Notes/a.md"));
        assert!(is_disk_backed_id("Finance/Config.json"));
        assert!(is_disk_backed_id("Boards/b.canvas"));

        assert!(!is_disk_backed_id("f47ac10b-58cc-4372-a567-0e02b2c3d479"));
        assert!(!is_disk_backed_id("Notes/no-extension"));
        assert!(!is_disk_backed_id("photo.png"));
        assert!(!is_disk_backed_id(""));
    }

    #[test]
    fn the_unscanned_directories_are_recognised_at_any_depth_and_on_both_separators() {
        assert!(is_in_unscanned_dir("assets/a.json"));
        assert!(is_in_unscanned_dir("Notes/assets/a.json"));
        assert!(is_in_unscanned_dir("Files/a.md"));
        assert!(is_in_unscanned_dir("Syn/a.md"));
        assert!(is_in_unscanned_dir(".trash/a.md"));
        assert!(is_in_unscanned_dir("Notes\\assets\\a.json"));

        assert!(!is_in_unscanned_dir("Notes/a.md"));
        // A name that merely starts with a skipped one is a different directory.
        assert!(!is_in_unscanned_dir("assets-archive/a.md"));
    }
}

#[cfg(test)]
mod new_note_identity_tests {

    /// A fixture directory unique to this run.
    ///
    /// A fixed name under the system temp directory is shared by every process
    /// on the machine, so two `cargo test` runs at once — an IDE beside a
    /// terminal, or CI beside a local one — delete each other's fixtures
    /// mid-test. The failure lands on whichever test lost the race, which reads
    /// as flakiness with no pattern to it. It contaminated a measurement in this
    /// very repository.
    ///
    /// The named subdirectory is not decoration: `tempdir()` hands back a
    /// dot-prefixed path, and vault walking filters dotfiles — including the
    /// root of the walk itself.
    fn unique_dir(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let holder = tempfile::tempdir().expect("tempdir");
        let path = holder.path().join(name);
        std::fs::create_dir_all(&path).expect("create fixture dir");
        (holder, path)
    }
    use super::new_note_frontmatter;

    /// The property the whole change exists for: sync finds an identity already
    /// in the file, so it never reaches for the pen.
    ///
    /// `get_or_assign_node_id_with_hint` writes the file when it has to mint an
    /// id. That write races the editor saving the same file, and this asserts
    /// the race is not entered rather than that it is survived.
    #[test]
    fn a_freshly_created_note_is_never_rewritten_to_give_it_an_identity() {
        let (_holder, dir) = unique_dir("synabit_new_note_identity");
        let path = dir.join("note.md");

        let created = new_note_frontmatter("My note", "note", None);
        std::fs::write(&path, &created).unwrap();

        let resolved =
            crate::sync::core::identity::get_or_assign_node_id_with_hint(&dir, &path, None)
                .expect("identity should resolve from the frontmatter");

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            created,
            "the file was rewritten to inject an id it already had"
        );
        assert!(
            created.contains(&format!("node_id: {}", resolved)),
            "the id sync resolved is not the one written at creation"
        );

    }

    #[test]
    fn two_notes_created_in_the_same_moment_do_not_share_an_identity() {
        // Copied files sharing a node_id has already been a bug once. Minting at
        // creation must not reintroduce it by deriving the id from the clock.
        let a = new_note_frontmatter("One", "note", None);
        let b = new_note_frontmatter("Two", "note", None);
        let id_of = |s: &str| {
            s.lines()
                .find_map(|l| l.strip_prefix("node_id: "))
                .map(str::to_string)
                .expect("frontmatter should carry a node_id")
        };
        assert_ne!(id_of(&a), id_of(&b), "two new notes were given one identity");
    }

    #[test]
    fn a_tag_is_carried_into_the_frontmatter_and_an_empty_one_is_not() {
        let tagged = new_note_frontmatter("Daily", "note", Some("journal"));
        assert!(tagged.contains("tags:\n  - journal\n"), "the tag was dropped");

        for empty in [Some(""), Some("   "), None] {
            let plain = new_note_frontmatter("Daily", "note", empty);
            assert!(
                !plain.contains("tags:"),
                "an empty tag should not produce a tags block: {empty:?}"
            );
        }
    }
}
