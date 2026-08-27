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
    // Cleared by the same identity the new edges are recorded under.
    //
    // This used to clear by `node.id` — the file's path — while
    // `extract_resolved_node_edges` records the source as the node's stable
    // id. For any node that has one, which is every node written since
    // identities landed, the two never matched: nothing was ever cleared, and
    // a link removed from a note stayed in the graph for good. Backlinks only
    // ever grew.
    let mut ok = logged(
        "clear old links",
        &node.id,
        db.delete_node_edges_by_source(node.stable_id()),
    );
    for edge in extract_resolved_node_edges(node, resolver) {
        ok &= logged("record link", &node.id, db.upsert_node_edge(&edge));
    }
    ok
}

/// Helper: delete node_edges for a source
pub(crate) fn delete_node_edges_for(db: &crate::db::DbBridge, rel_path: &str) -> bool {
    logged(
        "clear links",
        rel_path,
        db.delete_node_edges_for_path(rel_path),
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

    // Links are derived from what a node says, and what the app understands a
    // node to be saying has changed. Rebuild them once, from the rows already
    // in hand.
    rebuild_links_if_stale(db_state, &existing_nodes, &resolver);

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
                                    db_state,
                                    &mut batch,
                                    base_dir,
                                    &vault_id,
                                    &resolver,
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
        report.removed = remove_orphaned_nodes(&db, base_dir, &existing_nodes, &current_disk_files);
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
/// What this app currently understands a link to be.
///
/// Bumped when the extractor learns a shape it did not know before, which makes
/// every link recorded under an older number incomplete.
///
/// - **2** — attachments. `![](assets/x.png)` and `<video src="assets/x.mp4">`
///   were invisible to the extractor, so no note had ever been recorded as
///   using a file. "Used by" was empty for every file in every vault.
const LINK_SCHEMA_VERSION: i64 = 2;

/// Recompute every node's links, once, after the extractor learns something.
///
/// Cheap enough to be unremarkable and cheap for a specific reason: links come
/// from a node's own text and properties, both already in the database. Nothing
/// here touches a file.
///
/// It has to be its own pass rather than riding on the scan below, because that
/// scan skips any file whose modification time has not moved — which is every
/// note in an existing vault. The vault would have to be edited note by note
/// before its links caught up.
fn rebuild_links_if_stale(db_state: &DbState, nodes: &[NodeMetadata], resolver: &NodeResolver) {
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

    let recorded: i64 = db
        .get_kv("link_schema_version")
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    if recorded >= LINK_SCHEMA_VERSION {
        return;
    }

    log::info!(
        "rebuilding links for {} node(s): schema {recorded} → {LINK_SCHEMA_VERSION}",
        nodes.len()
    );
    for node in nodes {
        sync_node_edges(&db, node, resolver);
    }

    logged(
        "record link schema",
        "link_schema_version",
        db.set_kv("link_schema_version", &LINK_SCHEMA_VERSION.to_string()),
    );
}

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
pub(crate) fn bridge_external_edits(
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

/// Bring the database in line with a file that has just changed on disk.
///
/// The row, the search index and the graph edges all describe the file, and a
/// caller that writes one without the others leaves a note that is on disk and
/// unfindable, or findable under the text it used to hold.
pub(crate) fn reindex_node_at(db: &crate::db::DbBridge, vault_path: &str, abs_path: &Path) {
    let Some(node) = parse_file_to_node(vault_path, abs_path) else {
        return;
    };
    logged("write node", &node.id, db.upsert_node(&node));
    sync_node_to_search(db, &node);
    let resolver = build_resolver(db);
    sync_node_edges(db, &node, &resolver);
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
pub(crate) fn is_in_unscanned_dir(rel_id: &str) -> bool {
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

        // Deleted things, likewise. A row under `.trash/` is always a mistake:
        // the trash is where a file goes to stop being part of the vault, so
        // nothing there should be in the index. The unscanned-directory guard
        // below deliberately protects such rows from the is-it-gone test — it
        // has no business judging files the walk never visited — which is what
        // left these behind once something else had wrongly created them.
        let is_trashed = n.id.starts_with(".trash/") || n.id.starts_with(".trash\\");

        let is_orphan = is_disk_backed_id(&n.id) && !is_in_unscanned_dir(&n.id) && is_gone(&n.id);

        if is_syn || is_trashed || is_orphan {
            delete_node_edges_for(db, &n.id);
            logged("drop node", &n.id, db.delete_node(&n.id));
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
        // The same rule the full scan applies, and for the same reason. This
        // pass takes its paths straight from the file watcher, which reports
        // everything under the vault — including the directories the vault
        // manages by other means.
        //
        // Without it, moving a note into `.trash/` produced a create event,
        // this pass indexed the trashed file as a live node, and the deleted
        // note reappeared seconds later at `.trash/…`. Deleting it again
        // nested another layer, which is how a path ends up reading
        // `.trash/.trash/.trash/…`.
        if is_in_unscanned_dir(&rel_path) {
            continue;
        }

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
            delete_node_edges_for(&db, &rel_path);
            logged("drop node", &rel_path, db.delete_node(&rel_path));
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

/// How many nodes of a type there are, without reading any of them.
///
/// The QuickCap tab shows this as a badge, and a badge that had to load every
/// cap to draw a number would be the most expensive thing on screen — it is
/// painted on every launch, whether or not the user ever opens that tab.
#[tauri::command]
pub fn count_nodes(state: tauri::State<'_, DbState>, node_type: String) -> AppResult<i64> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.count_nodes_by_type(&node_type)
}

/// How many caps are still waiting, ignoring the ones put away on purpose.
#[tauri::command]
pub fn count_inbox_caps(state: tauri::State<'_, DbState>) -> AppResult<i64> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.count_inbox_caps()
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

/// The events that land on the days between `from` and `to`, already expanded.
///
/// The front end used to fetch every event in the vault, bodies included, and
/// then re-derive the recurrence rule in JavaScript once per day cell — 365
/// passes over the whole list to draw a year. It now asks for the days it is
/// showing and renders the answer.
#[tauri::command]
pub fn get_events_in_range(
    state: tauri::State<'_, DbState>,
    from: String,
    to: String,
    viewer_tz: Option<String>,
) -> AppResult<crate::calendar::recurrence::EventsInRange> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut events = db.get_event_summaries()?;
    // Calendars the user subscribes to are drawn on the same grid, out of the
    // same expansion, so a Tokyo meeting from a shared calendar is converted
    // and laid out exactly like one of their own. What they are not is
    // editable — see `EventSummary::subscription_id`.
    events.extend(db.subscribed_event_summaries()?);

    // The reader's zone comes from the front end, which is the only place
    // that knows it by name. Without one, everything stays floating — the
    // behaviour of every version before zones existed.
    Ok(crate::calendar::recurrence::expand_range(
        events,
        &from,
        &to,
        viewer_tz.as_deref().unwrap_or(""),
    ))
}

/// Read a wall clock in one zone off a clock in another.
///
/// The time grid works in the reader's zone, so dropping an event that lives
/// in another one has to be turned back into that zone's wall clock before it
/// is stored. Doing that here rather than in the front end keeps one
/// implementation of a conversion that has two genuinely awkward hours a year.
#[tauri::command]
pub fn convert_event_time(
    stamps: Vec<String>,
    from_tz: String,
    to_tz: String,
) -> AppResult<Vec<String>> {
    Ok(stamps
        .into_iter()
        .map(|s| {
            crate::calendar::tz::convert_stamp(&s, &from_tz, &to_tz).unwrap_or(s)
        })
        .collect())
}

/// Write every event in the vault to an `.ics` file.
///
/// The whole calendar, not a date range: an export is for taking the calendar
/// somewhere else, and a range would silently leave part of it behind.
#[tauri::command]
pub fn export_calendar_ics(
    state: tauri::State<'_, DbState>,
    destination: String,
) -> AppResult<usize> {
    let events = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_event_summaries()?
    };
    let written = events.iter().filter(|e| !e.start_at.trim().is_empty()).count();

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let text = crate::calendar::ics::export(&events, &stamp);

    let path = std::path::Path::new(&destination);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| crate::error::AppError::General(format!("Could not prepare the folder: {}", e)))?;
    }
    std::fs::write(path, text)
        .map_err(|e| crate::error::AppError::General(format!("Could not write the calendar: {}", e)))?;
    Ok(written)
}

/// Read an `.ics` file into the shapes this app stores.
///
/// Reading only. Nothing is written to the vault here: the caller writes each
/// event through the same path everything else uses, so an imported event is
/// indexed, synced and linked exactly like one typed in by hand.
#[tauri::command]
pub fn read_calendar_ics(
    source: String,
) -> AppResult<Vec<crate::calendar::ics::ImportedEvent>> {
    let text = std::fs::read_to_string(&source)
        .map_err(|e| crate::error::AppError::General(format!("Could not read that file: {}", e)))?;
    Ok(crate::calendar::ics::import(&text))
}

/// Which of these `UID`s the vault already has, and where.
///
/// So an import updates an event it has seen before instead of leaving a
/// second copy beside it. Matched on the identity that survives a rename, not
/// on the title.
#[tauri::command]
pub fn match_event_uids(
    state: tauri::State<'_, DbState>,
    uids: Vec<String>,
) -> AppResult<std::collections::HashMap<String, String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let wanted: std::collections::HashSet<String> = uids.into_iter().collect();
    let mut found = std::collections::HashMap::new();
    for ev in db.get_event_summaries()? {
        if wanted.contains(&ev.uid) {
            found.insert(ev.uid.clone(), ev.id.clone());
        }
    }
    Ok(found)
}

/// The tasks due on the days between `from` and `to`.
///
/// Paired with `get_events_in_range`: both halves of what a calendar draws
/// now come back scoped to the days on screen.
#[tauri::command]
pub fn get_tasks_in_range(
    state: tauri::State<'_, DbState>,
    from: String,
    to: String,
) -> AppResult<Vec<crate::models::node::NodeSummary>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_tasks_in_range(&from, &to)
}

/// The occurrences that answer a question, rather than the ones on a page.
///
/// Three ways to ask, and they narrow rather than replace each other:
///
/// * nothing — what is coming up;
/// * `query` — events whose words match, wherever they fall;
/// * `person_id` — every meeting that named someone, which is the question an
///   ordinary calendar cannot answer at all and the reason events in this app
///   are nodes in a graph.
///
/// Subscribed calendars are searched too when nothing narrows to a person:
/// a shared calendar is still a place a meeting can be.
#[tauri::command]
pub fn search_event_occurrences(
    state: tauri::State<'_, DbState>,
    query: Option<String>,
    person_id: Option<String>,
    from: String,
    to: String,
    viewer_tz: Option<String>,
) -> AppResult<crate::calendar::recurrence::EventsInRange> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let person = person_id.unwrap_or_default();
    let text = query.unwrap_or_default();

    let mut events = if !person.trim().is_empty() {
        db.events_linked_to(person.trim())?
    } else {
        let mut all = db.get_event_summaries()?;
        all.extend(db.subscribed_event_summaries()?);
        all
    };

    if !text.trim().is_empty() {
        let mut parsed = crate::search::parse_query(&text);
        parsed.type_filter = Some("event".to_string());
        let found = db.search_fts(&parsed, 1, 500)?;
        let matched: std::collections::HashSet<String> =
            found.results.into_iter().map(|r| r.id).collect();

        // A subscribed event is not in the text index — it is not a node —
        // so it is matched on what is in front of the reader instead.
        let needle = text.trim().to_lowercase();
        events.retain(|e| {
            matched.contains(&e.id)
                || (!e.subscription_id.is_empty()
                    && (e.title.to_lowercase().contains(&needle)
                        || e.location.to_lowercase().contains(&needle)))
        });
    }

    Ok(crate::calendar::recurrence::expand_range(
        events,
        &from,
        &to,
        viewer_tz.as_deref().unwrap_or(""),
    ))
}

/// A recurring event together with every node split off from it.
///
/// Editing "all events in the series" has to find parts that may fall outside
/// the range on screen.
#[tauri::command]
pub fn get_event_series(
    state: tauri::State<'_, DbState>,
    root_id: String,
) -> AppResult<Vec<crate::calendar::recurrence::EventSummary>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.get_event_series(&root_id)
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

/// What happened when a block marker was placed.
pub(crate) enum BlockMarker {
    /// The line already carried one; this is it.
    AlreadyThere(String),
    /// The file with the marker added.
    Inserted(String),
    /// No line in the body matched.
    NotFound,
}

/// Where a paragraph's `^id` marker goes, and what the file looks like after.
///
/// Split out of `create_block_reference` because this edits prose somebody
/// wrote, and three things about how it used to do that were wrong in ways
/// only a test would show.
///
/// **It searched the frontmatter.** The snippet comes from a paragraph or a
/// heading in the editor, and a heading very often repeats the note's title —
/// which appears, earlier in the file, as `title: …`. That line *contains* the
/// snippet, so the marker was appended inside the YAML and the title silently
/// became `My Note ^abc123`.
///
/// **It took the first line containing the snippet**, rather than the line
/// that is the snippet. A short paragraph could be claimed by a longer line
/// above it that merely mentioned the same words. The `contains` test is still
/// needed — the editor hands over `My Heading` for a source line reading
/// `## My Heading`, and inline formatting differs too — so an exact match
/// after stripping markdown's leading marks is tried first, and `contains` is
/// only the fallback.
///
/// **It rebuilt the file with `lines()` and `join("\n")`,** which drops the
/// trailing newline and rewrites every CRLF as LF. Adding one marker showed up
/// as every line of the file having changed, to git and to sync alike.
pub(crate) fn place_block_marker(
    file_content: &str,
    content_snippet: &str,
    block_id: &str,
) -> BlockMarker {
    let snippet = content_snippet.trim();
    if snippet.is_empty() {
        return BlockMarker::NotFound;
    }

    // `split_inclusive` keeps each line's terminator, so what is not changed is
    // written back byte for byte.
    let lines: Vec<&str> = file_content.split_inclusive('\n').collect();
    let body_starts_at = frontmatter_end(&lines);

    let wanted = normalise_for_match(snippet);
    let target = (body_starts_at..lines.len())
        .find(|i| normalise_for_match(lines[*i]) == wanted)
        .or_else(|| (body_starts_at..lines.len()).find(|i| lines[*i].trim().contains(snippet)));

    let Some(index) = target else {
        return BlockMarker::NotFound;
    };

    let line = lines[index];
    let (text, terminator) = split_terminator(line);
    if let Some(caps) = block_id_regex().captures(text.trim()) {
        return BlockMarker::AlreadyThere(caps[1].to_string());
    }

    let mut out = String::with_capacity(file_content.len() + block_id.len() + 2);
    for (i, chunk) in lines.iter().enumerate() {
        if i == index {
            out.push_str(text);
            out.push_str(&format!(" ^{}", block_id));
            out.push_str(terminator);
        } else {
            out.push_str(chunk);
        }
    }
    BlockMarker::Inserted(out)
}

/// The index of the first line after any YAML frontmatter.
fn frontmatter_end(lines: &[&str]) -> usize {
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return 0;
    }
    lines
        .iter()
        .skip(1)
        .position(|l| l.trim_end() == "---")
        .map(|p| p + 2)
        .unwrap_or(0)
}

/// A line without its newline, and the newline it had.
fn split_terminator(line: &str) -> (&str, &str) {
    if let Some(rest) = line.strip_suffix("\r\n") {
        (rest, "\r\n")
    } else if let Some(rest) = line.strip_suffix('\n') {
        (rest, "\n")
    } else {
        (line, "")
    }
}

/// A line reduced to the words in it, for comparing against the editor's text.
///
/// What the editor hands over is the *rendered* text of a block: `Heading` for
/// a line reading `## Heading`, and `một đoạn văn` for one reading
/// `một **đoạn** văn`. Neither is equal to its source, and the second is not
/// even contained in it — the asterisks sit in the middle of the words — so a
/// paragraph with a bold word in it could not be given a block marker at all.
///
/// Both sides are put through this, so both are compared as prose.
fn normalise_for_match(line: &str) -> String {
    let without_block_marks = strip_leading_marks(line.trim());

    let mut out = String::with_capacity(without_block_marks.len());
    let mut chars = without_block_marks.chars().peekable();
    // Set by `]`, so the `(` that may follow is recognised as a link's URL
    // rather than an ordinary bracket. Reading `out` to decide that cannot
    // work: `]` is dropped before ever reaching it.
    let mut just_closed_a_label = false;

    while let Some(c) = chars.next() {
        let closed_a_label = std::mem::take(&mut just_closed_a_label);
        match c {
            // Inline emphasis, code, strikethrough and highlight marks carry
            // no words of their own.
            '*' | '_' | '`' | '~' | '=' => {}
            '[' => {}
            ']' => just_closed_a_label = true,
            // `[text](url)` reads as `text`, so the URL half is skipped.
            '(' if closed_a_label => {
                let mut depth = 1usize;
                for c in chars.by_ref() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => out.push(c),
        }
    }

    // Whitespace is not part of the comparison either.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Drop markdown's leading block marks, so `## Heading` reads as `Heading`.
fn strip_leading_marks(line: &str) -> &str {
    let mut rest = line;
    loop {
        let trimmed = rest
            .trim_start_matches(['#', '>', '-', '*', '+'])
            .trim_start();
        let after_ordered = match trimmed.find(". ") {
            Some(dot) if trimmed[..dot].chars().all(|c| c.is_ascii_digit()) && dot > 0 => {
                trimmed[dot + 2..].trim_start()
            }
            _ => trimmed,
        };
        if after_ordered == rest {
            return rest;
        }
        rest = after_ordered;
    }
}

#[tauri::command]
pub fn create_block_reference(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_id: String,
    content_snippet: String,
) -> AppResult<String> {
    let block_id = generate_block_id();

    // Read the source file
    let abs_path = path_utils::resolve_safe_path(&vault_path, &node_id)?;
    let file_content = std::fs::read_to_string(&abs_path)
        .map_err(|e| crate::error::AppError::General(format!("Failed to read file: {}", e)))?;

    let new_content = match place_block_marker(&file_content, &content_snippet, &block_id) {
        BlockMarker::AlreadyThere(existing) => return Ok(existing),
        BlockMarker::NotFound => {
            return Err(crate::error::AppError::General(
                "Content snippet not found in source file".to_string(),
            ))
        }
        BlockMarker::Inserted(content) => content,
    };

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

/// The frontmatter a file on disk already carries.
///
/// Read from the file rather than from the `nodes` row, because the row is a
/// cache: the vault scan skips files whose mtime has not moved, so a key a
/// person added by hand may never have reached the database. The file is the
/// only place that is always right.
///
/// Deliberately not `parse_file_to_node`, which is the other way to get at
/// this. That function *derives* properties for some types — a whiteboard
/// comes back with a `node_count` and the labels lifted out of its diagram —
/// and merging derived values back in would write the app's own bookkeeping
/// into the user's file, a little more of it on every save.
pub(crate) fn existing_properties(
    abs_path: &Path,
    ext: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let Ok(content) = std::fs::read_to_string(abs_path) else {
        return serde_json::Map::new();
    };

    let found = if ext == "json" || ext == "canvas" {
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("metadata").cloned())
    } else {
        gray_matter::Matter::<gray_matter::engine::YAML>::new()
            .parse::<serde_json::Value>(&content)
            .ok()
            .and_then(|parsed| parsed.data)
    };

    match found {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}

/// The body a file already has, with its frontmatter stripped off.
///
/// The twin of `existing_properties`, and there for the same reason. A caller
/// that is only changing a property — the checkbox on a task, a card dragged
/// between columns — holds whatever body it read when the list was loaded,
/// which may be minutes old and, after a sync or an edit in another window, no
/// longer what is on disk. Writing that back reverts the note to the version
/// the list happened to have. Passing no content at all says "I have nothing
/// to say about the body", and the body stays as it is.
pub(crate) fn existing_body(abs_path: &Path, ext: &str) -> String {
    let Ok(content) = std::fs::read_to_string(abs_path) else {
        return String::new();
    };

    if ext == "json" || ext == "canvas" {
        serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(str::to_string))
            .unwrap_or_default()
    } else {
        gray_matter::Matter::<gray_matter::engine::YAML>::new()
            .parse::<serde_json::Value>(&content)
            .map(|parsed| parsed.content)
            .unwrap_or(content)
    }
}

/// What a file's frontmatter should end up holding.
///
/// A write is a patch, never a replacement: keys the caller names are set,
/// keys it does not name are left where they are, and a key it maps to `null`
/// is removed.
///
/// It reads as the cautious choice and is really the only correct one. No
/// caller in this app knows the whole frontmatter of the file it is writing.
/// The note editor sends four keys and has no idea what else is in there — an
/// `aliases` the user typed by hand, a key some later version of this app will
/// write, a key another tool put there. Rebuilding the frontmatter from those
/// four deleted the rest, silently, on the first autosave after the note was
/// opened. For a product whose whole case is that the files stay yours,
/// quietly eating what you put in them is the worst bug available.
///
/// The two screens that look like exceptions are not. The task editor lists
/// every frontmatter key as an editable row, so its payload really is the
/// whole file — but the project editor filters `created_at` out of its rows
/// and never sends it back, which is exactly how a project's creation date
/// used to be reset to now on every save. A category with one member that
/// does not fit is not a category, so there is no "this caller owns the file"
/// mode to get wrong. A screen that lets someone delete a field says so by
/// sending that key as `null`.
pub(crate) fn resolve_properties(
    mut existing: serde_json::Map<String, serde_json::Value>,
    incoming: &serde_json::Value,
) -> serde_json::Value {
    let serde_json::Value::Object(incoming) = incoming else {
        // Nothing usable was sent. Keeping what is on disk beats replacing it
        // with a scalar that the frontmatter writer would then drop entirely.
        return serde_json::Value::Object(existing);
    };

    for (key, value) in incoming {
        if value.is_null() {
            existing.remove(key);
        } else {
            existing.insert(key.clone(), value.clone());
        }
    }

    serde_json::Value::Object(existing)
}

/// A free vault path for a node that is being created rather than edited.
///
/// The create tools name a file after the node's title, and a title is not
/// unique: a second "Standup" is an ordinary thing to want. Writing straight
/// to the name replaced the first one — body, frontmatter and all — with
/// nothing to undo it, because a create path has no old version to fall back
/// on the way an edit does.
///
/// Counter suffixes rather than a timestamp or a uuid, and the same
/// `stem (n).ext` shape [`free_trash_path`](super::trash) uses, so the two
/// places in the app that dodge a taken name produce names that look alike.
///
/// [`resolve_properties`] handles the rest: on the vanishing chance that the
/// chosen path is taken between this check and the write, the write patches
/// that file's frontmatter instead of rebuilding it from the caller's keys.
pub(crate) fn free_node_path(vault: &Path, rel_path: &str) -> String {
    if !vault.join(rel_path).exists() {
        return rel_path.to_string();
    }

    // Split on the vault separator first: a directory with a dot in it must
    // not be mistaken for the extension of a file that has none.
    let (dir, name) = match rel_path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, rel_path),
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
        _ => (name, None),
    };
    let join = |name: String| match dir {
        Some(dir) => format!("{dir}/{name}"),
        None => name,
    };
    let numbered = |n: &str| match ext {
        Some(ext) => join(format!("{stem} ({n}).{ext}")),
        None => join(format!("{stem} ({n})")),
    };

    for n in 1..1000 {
        let candidate = numbered(&n.to_string());
        if !vault.join(&candidate).exists() {
            return candidate;
        }
    }

    // A thousand notes of one name is not a real vault, but the fallback still
    // has to be a free path: handing back the original here would reintroduce
    // exactly the overwrite this function exists to prevent. A uuid is ugly
    // and certainly unused, which is the right trade at this end of the range.
    numbered(&uuid::Uuid::new_v4().simple().to_string())
}

/// A markdown node file: YAML frontmatter, then the body.
///
/// Pulled out of `write_node_file` so a test can drive the bytes that actually
/// reach the disk. `title` and `type` come from the arguments rather than from
/// `properties`, and `updated_at` is stamped here, so those three are skipped
/// wherever they appear in the map — a merged-in copy read back off the file
/// would otherwise overwrite the values the caller just supplied.
pub(crate) fn markdown_with_frontmatter(
    title: &str,
    node_type: &str,
    properties: &serde_json::Value,
    content: &str,
) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    let mut props_map = serde_yaml::Mapping::new();
    props_map.insert(
        serde_yaml::Value::String("title".to_string()),
        serde_yaml::Value::String(title.to_string()),
    );
    props_map.insert(
        serde_yaml::Value::String("type".to_string()),
        serde_yaml::Value::String(node_type.to_string()),
    );

    let mut has_created_at = false;

    if let serde_json::Value::Object(map) = properties {
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

    // Only when the file does not already have one. A creation date that moves
    // is not a creation date.
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
    // serde_yaml may or may not lead with a document marker; the fences are
    // added here so the result is markdown-compatible either way.
    let yaml_str = frontmatter.trim_start_matches("---\n");
    format!("---\n{}---\n{}", yaml_str, content)
}

#[cfg(test)]
mod date_pattern_tests {
    use super::date_string_from_pattern;

    fn at(s: &str) -> chrono::DateTime<chrono::Local> {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(2026, 8, 5, 12, 0, 0)
            .single()
            .unwrap_or_else(|| panic!("fixture date {s}"))
    }

    #[test]
    fn the_patterns_people_actually_write_come_out_right() {
        let now = at("2026-08-05");
        assert_eq!(
            date_string_from_pattern("YYYY-MM-DD", now).unwrap(),
            "2026-08-05"
        );
        assert_eq!(
            date_string_from_pattern("DD/MM/YYYY", now).unwrap(),
            "05/08/2026"
        );
        assert_eq!(date_string_from_pattern("D/M/YY", now).unwrap(), "5/8/26");
    }

    /// The crash this exists to stop.
    ///
    /// The daily-note format is a plain text field. An unrecognised `%` escape
    /// makes chrono's `to_string()` panic, and the front end's check — does it
    /// mention a year, a month and a day — waves this straight through.
    #[test]
    fn a_pattern_chrono_cannot_read_is_refused_rather_than_run() {
        let now = at("2026-08-05");
        for pattern in ["YYYY-MM-DD (100%)", "YYYY-MM-DD %", "%Q YYYY"] {
            assert_eq!(
                date_string_from_pattern(pattern, now),
                None,
                "{pattern:?} should be refused, not rendered"
            );
        }
    }

    /// Text around the date is ordinary and must survive.
    #[test]
    fn literal_text_in_a_pattern_is_kept() {
        let now = at("2026-08-05");
        assert_eq!(
            date_string_from_pattern("Nhật ký YYYY-MM-DD", now).unwrap(),
            "Nhật ký 2026-08-05"
        );
    }
}

#[cfg(test)]
mod archive_tests {
    use super::*;

    fn day(s: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn task(props: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: "Tasks/one.md".to_string(),
            node_type: "task".to_string(),
            title: "One".to_string(),
            content: String::new(),
            properties: props,
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    #[test]
    fn a_task_finished_long_enough_ago_is_ready() {
        let node = task(serde_json::json!({ "status": "done", "completed_at": "2026-01-01" }));
        assert!(is_ready_to_archive(&node, day("2026-02-01"), 30));
    }

    #[test]
    fn a_task_finished_recently_is_left_where_it_is() {
        let node = task(serde_json::json!({ "status": "done", "completed_at": "2026-01-20" }));
        assert!(!is_ready_to_archive(&node, day("2026-02-01"), 30));
    }

    #[test]
    fn a_task_that_is_not_finished_is_never_moved() {
        for status in ["todo", "in-progress", ""] {
            let node = task(serde_json::json!({ "status": status, "completed_at": "2020-01-01" }));
            assert!(
                !is_ready_to_archive(&node, day("2026-02-01"), 30),
                "status {status:?} should not be archived"
            );
        }
    }

    /// Marked done at some point, with no record of when. This runs on a timer
    /// with nobody watching, so a guess here moves somebody's file for reasons
    /// they will never be able to reconstruct.
    #[test]
    fn a_task_with_no_completion_date_is_left_alone() {
        for props in [
            serde_json::json!({ "status": "done" }),
            serde_json::json!({ "status": "done", "completed_at": "" }),
            serde_json::json!({ "status": "done", "completed_at": "sometime last year" }),
        ] {
            let node = task(props.clone());
            assert!(
                !is_ready_to_archive(&node, day("2026-02-01"), 30),
                "{props} should not be archived"
            );
        }
    }

    /// The stored value carries a time as well on some paths, and it is only
    /// the date that decides.
    #[test]
    fn a_completion_stamp_with_a_time_on_it_still_reads() {
        let node =
            task(serde_json::json!({ "status": "done", "completed_at": "2026-01-01 09:30:00" }));
        assert!(is_ready_to_archive(&node, day("2026-02-01"), 30));
    }

    #[test]
    fn the_day_the_window_closes_counts_as_inside_it() {
        let node = task(serde_json::json!({ "status": "done", "completed_at": "2026-01-01" }));
        assert!(is_ready_to_archive(&node, day("2026-01-31"), 30));
        assert!(!is_ready_to_archive(&node, day("2026-01-30"), 30));
    }

    /// Archiving keeps only the file's name and drops the folders above it, so
    /// two notes called the same thing in different folders arrive at one
    /// destination. `fs::rename` replaces what is there without a word.
    #[test]
    fn two_notes_sharing_a_name_do_not_land_on_top_of_each_other() {
        let holder = tempfile::tempdir().expect("tempdir");
        let vault = holder.path().join("vault");
        std::fs::create_dir_all(vault.join("Notes/archived")).unwrap();

        let first = free_node_path(&vault, "Notes/archived/Họp.md");
        std::fs::write(vault.join(&first), "the first note").unwrap();

        let second = free_node_path(&vault, "Notes/archived/Họp.md");

        assert_ne!(first, second, "the second must not be given the same path");
        std::fs::write(vault.join(&second), "the second note").unwrap();
        assert_eq!(
            std::fs::read_to_string(vault.join(&first)).unwrap(),
            "the first note",
            "the first note must still be there"
        );
    }
}

#[cfg(test)]
mod block_marker_tests {
    use super::{place_block_marker, BlockMarker};

    fn inserted(file: &str, snippet: &str) -> String {
        match place_block_marker(file, snippet, "abc123") {
            BlockMarker::Inserted(out) => out,
            BlockMarker::AlreadyThere(id) => panic!("expected an insert, found existing id {id}"),
            BlockMarker::NotFound => panic!("expected an insert, found no matching line"),
        }
    }

    /// The corruption this was extracted to prove.
    ///
    /// A heading usually repeats the note's title, and the title also sits in
    /// the frontmatter as `title: …` — earlier in the file, and *containing*
    /// the snippet. Copying a block link from that heading appended the marker
    /// to the YAML, and the note's title silently gained ` ^abc123`.
    #[test]
    fn the_marker_never_lands_in_the_frontmatter() {
        let file = "---\ntitle: Họp tuần\ntype: note\n---\n# Họp tuần\n\nnội dung\n";

        let out = inserted(file, "Họp tuần");

        assert!(
            out.contains("title: Họp tuần\n"),
            "the frontmatter title must be untouched:\n{out}"
        );
        assert!(out.contains("# Họp tuần ^abc123"), "{out}");
    }

    /// The line that *is* the snippet wins over a line that merely mentions it.
    #[test]
    fn an_exact_line_is_preferred_to_one_that_only_contains_the_words() {
        let file = "---\ntype: note\n---\nSee abc for details\n\nabc\n";

        let out = inserted(file, "abc");

        assert!(out.contains("See abc for details\n"), "{out}");
        assert!(out.contains("\nabc ^abc123"), "{out}");
    }

    /// The editor hands over the text of a heading, not its markdown. The
    /// leading marks have to be looked past for the exact match to work.
    #[test]
    fn markdown_block_marks_are_looked_past_when_matching() {
        for (line, snippet) in [
            ("## Ghi chú", "Ghi chú"),
            ("- một mục", "một mục"),
            ("> trích dẫn", "trích dẫn"),
            ("3. mục thứ ba", "mục thứ ba"),
        ] {
            let file = format!("---\ntype: note\n---\n{line}\n");
            let out = inserted(&file, snippet);
            assert!(
                out.contains(&format!("{line} ^abc123")),
                "{line:?} + {snippet:?} gave:\n{out}"
            );
        }
    }

    /// The editor hands over rendered text, so a line with a bold word in it
    /// is not equal to — nor even contains — what comes back. Such a paragraph
    /// could not be given a block marker at all.
    #[test]
    fn a_line_with_inline_formatting_is_matched_by_the_words_in_it() {
        for (line, snippet) in [
            ("một **đoạn** văn", "một đoạn văn"),
            ("một `đoạn` văn", "một đoạn văn"),
            ("một ~~đoạn~~ văn", "một đoạn văn"),
            (
                "xem [tài liệu](https://example.com) nhé",
                "xem tài liệu nhé",
            ),
        ] {
            let file = format!("---\ntype: note\n---\n{line}\n");
            let out = inserted(&file, snippet);
            assert!(
                out.contains(&format!("{line} ^abc123")),
                "{line:?} + {snippet:?} gave:\n{out}"
            );
        }
    }

    /// Adding one marker used to rewrite every line of the file: `lines()`
    /// drops the trailing newline and turns every CRLF into an LF, so git and
    /// sync both saw the whole note change.
    #[test]
    fn nothing_but_the_marked_line_is_rewritten() {
        let file = "---\r\ntype: note\r\n---\r\nđoạn một\r\nđoạn hai\r\n";

        let out = inserted(file, "đoạn một");

        assert_eq!(
            out, "---\r\ntype: note\r\n---\r\nđoạn một ^abc123\r\nđoạn hai\r\n",
            "line endings and the trailing newline must survive"
        );
    }

    #[test]
    fn a_line_that_already_has_a_marker_hands_back_the_one_it_has() {
        // Six characters: the shape `generate_block_id` produces.
        let file = "---\ntype: note\n---\nđoạn một ^exist1\n";

        match place_block_marker(file, "đoạn một", "abc123") {
            BlockMarker::AlreadyThere(id) => assert_eq!(id, "exist1"),
            _ => panic!("should have found the marker already there"),
        }
    }

    #[test]
    fn a_snippet_that_matches_nothing_changes_nothing() {
        let file = "---\ntype: note\n---\nđoạn một\n";

        assert!(matches!(
            place_block_marker(file, "không có ở đâu cả", "abc123"),
            BlockMarker::NotFound
        ));
        assert!(matches!(
            place_block_marker(file, "   ", "abc123"),
            BlockMarker::NotFound
        ));
    }

    /// A file with no frontmatter is all body.
    #[test]
    fn a_note_without_frontmatter_is_searched_from_its_first_line() {
        let out = inserted("đoạn một\nđoạn hai\n", "đoạn một");
        assert_eq!(out, "đoạn một ^abc123\nđoạn hai\n");
    }
}

#[cfg(test)]
mod frontmatter_merge_tests {
    use super::{existing_properties, markdown_with_frontmatter, resolve_properties};
    use serde_json::json;

    fn map(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        match value {
            serde_json::Value::Object(m) => m,
            _ => panic!("test fixture must be an object"),
        }
    }

    fn write(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write fixture");
        path
    }

    /// The bug this whole change exists for.
    ///
    /// The note editor sends four keys. Everything else in the file — an
    /// `aliases` typed by hand, the note's own identity — used to be gone the
    /// moment the first autosave fired.
    #[test]
    fn keys_the_caller_never_mentions_survive_the_write() {
        let existing = map(json!({
            "node_id": "abc-123",
            "created_at": "2026-01-01T00:00:00Z",
            "aliases": ["Old Name", "Older Name"],
            "cssclass": "wide",
            "tags": ["work"],
        }));

        let merged = resolve_properties(
            existing,
            &json!({ "tags": ["work", "urgent"], "pinned": true }),
        );

        assert_eq!(merged["aliases"], json!(["Old Name", "Older Name"]));
        assert_eq!(merged["cssclass"], json!("wide"));
        assert_eq!(merged["node_id"], json!("abc-123"));
        assert_eq!(merged["created_at"], json!("2026-01-01T00:00:00Z"));
        // And the keys it did mention are the caller's to set.
        assert_eq!(merged["tags"], json!(["work", "urgent"]));
        assert_eq!(merged["pinned"], json!(true));
    }

    /// The other half of the contract. Without this a screen that lets someone
    /// delete a field would appear to work and then hand the field back.
    #[test]
    fn a_key_sent_as_null_is_removed_rather_than_written_as_null() {
        let existing = map(json!({ "wip_limit": "5", "budget": "100", "tags": [] }));

        let merged = resolve_properties(existing, &json!({ "wip_limit": null }));

        assert!(
            merged.get("wip_limit").is_none(),
            "an explicit null means the key goes, not that it is set to null: {merged}"
        );
        assert_eq!(merged["budget"], json!("100"));
    }

    /// A new file has nothing to preserve, so a patch is the whole frontmatter.
    #[test]
    fn a_file_that_does_not_exist_yet_yields_exactly_what_the_caller_sent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("Notes/never-written.md");

        let merged = resolve_properties(
            existing_properties(&missing, "md"),
            &json!({ "tags": ["new"] }),
        );

        assert_eq!(merged, json!({ "tags": ["new"] }));
    }

    #[test]
    fn frontmatter_is_read_back_off_a_markdown_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(
            dir.path(),
            "note.md",
            "---\ntitle: Meeting\ntype: note\naliases:\n  - Standup\n---\nbody text\n",
        );

        let found = existing_properties(&path, "md");

        assert_eq!(found["aliases"], json!(["Standup"]));
        assert_eq!(found["title"], json!("Meeting"));
    }

    /// A `.json` node keeps its properties under `metadata`, not at the top
    /// level. Reading the wrong one would merge `title`, `type` and the whole
    /// body into the frontmatter on every save.
    #[test]
    fn a_json_node_is_read_from_its_metadata_and_not_its_top_level() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(
            dir.path(),
            "cap.json",
            r#"{"title":"Cap","type":"quickcap","metadata":{"source":"widget"},"content":"hi"}"#,
        );

        let found = existing_properties(&path, "json");

        assert_eq!(found["source"], json!("widget"));
        assert!(
            found.get("title").is_none(),
            "top-level keys are not properties: {found:?}"
        );
        assert!(
            found.get("content").is_none(),
            "the body is not a property: {found:?}"
        );
    }

    /// The whole path, from the file on disk to the bytes written back over it.
    ///
    /// The unit tests above prove the merge; this proves it is actually wired
    /// into what gets saved, which is the part a user would notice.
    #[test]
    fn a_note_save_that_names_four_keys_rewrites_the_file_without_losing_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(
            dir.path(),
            "note.md",
            "---\n\
             title: Standup\n\
             type: note\n\
             node_id: abc-123\n\
             created_at: '2026-01-01T00:00:00Z'\n\
             aliases:\n\
             \x20 - Daily\n\
             cssclass: wide\n\
             tags:\n\
             \x20 - work\n\
             ---\n\
             the old body\n",
        );

        // What the note editor sends: four keys, and nothing about the rest.
        let payload = json!({
            "pinned": true,
            "full_width": false,
            "tags": ["work", "urgent"],
            "linked_projects": [],
        });

        let merged = resolve_properties(existing_properties(&path, "md"), &payload);
        let written = markdown_with_frontmatter("Standup", "note", &merged, "the new body\n");

        // Read the result back the way the app reads any file, rather than
        // matching on YAML text that serde is free to lay out how it likes.
        std::fs::write(&path, &written).expect("rewrite fixture");
        let after = existing_properties(&path, "md");

        assert_eq!(after["aliases"], json!(["Daily"]), "in: {written}");
        assert_eq!(after["cssclass"], json!("wide"), "in: {written}");
        assert_eq!(after["node_id"], json!("abc-123"), "in: {written}");
        assert_eq!(
            after["created_at"],
            json!("2026-01-01T00:00:00Z"),
            "a note's creation date is not renewable: {written}"
        );
        assert_eq!(after["tags"], json!(["work", "urgent"]));
        assert_eq!(after["pinned"], json!(true));
        assert!(written.ends_with("the new body\n"), "in: {written}");
    }

    /// A file with no frontmatter at all, or one whose frontmatter is broken,
    /// must not take the write down with it — the user's prose is still worth
    /// saving.
    #[test]
    fn a_file_without_readable_frontmatter_contributes_nothing_and_does_not_fail() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bare = write(dir.path(), "bare.md", "just prose, no frontmatter\n");
        let broken = write(dir.path(), "broken.json", "{ this is not json");

        assert!(existing_properties(&bare, "md").is_empty());
        assert!(existing_properties(&broken, "json").is_empty());

        let merged = resolve_properties(existing_properties(&bare, "md"), &json!({ "tags": [] }));
        assert_eq!(merged, json!({ "tags": [] }));
    }
}

#[cfg(test)]
mod free_node_path_tests {
    use super::{
        existing_properties, free_node_path, markdown_with_frontmatter, resolve_properties,
    };
    use serde_json::json;

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("Notes")).expect("create Notes");
        dir
    }

    fn touch(dir: &tempfile::TempDir, rel: &str, body: &str) {
        std::fs::write(dir.path().join(rel), body).expect("write fixture");
    }

    #[test]
    fn a_name_nobody_has_taken_is_used_exactly_as_asked() {
        let dir = vault();

        assert_eq!(
            free_node_path(dir.path(), "Notes/Standup.md"),
            "Notes/Standup.md"
        );
    }

    #[test]
    fn a_taken_name_gains_a_counter_and_keeps_its_extension() {
        let dir = vault();
        touch(&dir, "Notes/Standup.md", "the one that was already there");

        assert_eq!(
            free_node_path(dir.path(), "Notes/Standup.md"),
            "Notes/Standup (1).md"
        );
    }

    #[test]
    fn the_counter_climbs_until_it_finds_a_gap() {
        let dir = vault();
        touch(&dir, "Notes/Standup.md", "first");
        touch(&dir, "Notes/Standup (1).md", "second");
        touch(&dir, "Notes/Standup (2).md", "third");

        assert_eq!(
            free_node_path(dir.path(), "Notes/Standup.md"),
            "Notes/Standup (3).md"
        );
    }

    /// A dot in a folder name is not an extension. Splitting on the last dot
    /// in the whole path would turn `My.Vault/plan` into `My (1).Vault/plan`,
    /// which names a directory that does not exist.
    #[test]
    fn a_dot_in_a_folder_name_is_not_mistaken_for_an_extension() {
        let dir = vault();
        std::fs::create_dir_all(dir.path().join("My.Notes")).expect("create dir");
        touch(&dir, "My.Notes/plan", "no extension on this one");

        assert_eq!(
            free_node_path(dir.path(), "My.Notes/plan"),
            "My.Notes/plan (1)"
        );
    }

    /// The regression this exists for.
    ///
    /// `create_note` names the file after the title. Ask Syn for a second
    /// "Standup" and the first one — the body someone typed, the frontmatter
    /// they hand-edited — was overwritten in place, with no old version to
    /// recover from because a create has none.
    #[test]
    fn a_second_note_of_the_same_name_leaves_the_first_one_untouched() {
        let dir = vault();
        let original = "---\n\
             title: Standup\n\
             type: note\n\
             node_id: abc-123\n\
             aliases:\n\
             \x20 - Daily\n\
             ---\n\
             notes from the standup nobody wants to lose\n";
        touch(&dir, "Notes/Standup.md", original);

        // What `write_tool_node` does, in the same order.
        let rel_path = free_node_path(dir.path(), "Notes/Standup.md");
        let full_path = dir.path().join(&rel_path);
        let properties = resolve_properties(
            existing_properties(&full_path, "md"),
            &json!({ "tags": ["syn"] }),
        );
        let file_content =
            markdown_with_frontmatter("Standup", "note", &properties, "what Syn wrote\n");
        std::fs::write(&full_path, &file_content).expect("write node");

        assert_eq!(rel_path, "Notes/Standup (1).md");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("Notes/Standup.md")).expect("read original"),
            original,
            "the note that was already there is not the tool's to rewrite"
        );

        let written = existing_properties(&full_path, "md");
        assert_eq!(written["tags"], json!(["syn"]));
        assert!(
            file_content.ends_with("what Syn wrote\n"),
            "in: {file_content}"
        );
    }

    /// The second half of the guarantee. `free_node_path` checks and the write
    /// follows, so a file that appears in between is not impossible — and if
    /// one does, the frontmatter it carries is patched rather than rebuilt
    /// from the two keys a create tool happens to send.
    #[test]
    fn frontmatter_at_the_chosen_path_is_merged_rather_than_replaced() {
        let dir = vault();
        touch(
            &dir,
            "Notes/Standup.md",
            "---\ntitle: Standup\ntype: note\ncssclass: wide\n---\nbody\n",
        );

        let full_path = dir.path().join("Notes/Standup.md");
        let properties = resolve_properties(
            existing_properties(&full_path, "md"),
            &json!({ "tags": ["syn"] }),
        );

        assert_eq!(properties["cssclass"], json!("wide"));
        assert_eq!(properties["tags"], json!(["syn"]));
    }
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
    // The new body, or `None` to leave the one on disk alone — which is what a
    // property-only write should send. See `existing_body`.
    content: Option<String>,
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

    // Fold the caller's properties into whatever the file already says. Read
    // here, before the write below that would otherwise erase the rest of it.
    let properties = resolve_properties(existing_properties(&abs_path, ext), &properties);
    // Read before the write below, for the same reason the properties are.
    let content = content.unwrap_or_else(|| existing_body(&abs_path, ext));
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
        markdown_with_frontmatter(&title, &node_type, &properties, &content)
    };

    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(&app_handle, &vault_path)?;
    let vault_id = identity.vault_id.to_string();

    // Before saving CRDT, we must determine the node_id
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // The identity this path already has, read before the write that may erase
    // it from the file.
    //
    // The frontmatter is rebuilt from the `properties` the caller sent, so a
    // caller that does not send `node_id` back — the note editor sends four
    // fields and nothing else — writes a file with no identity in it. Left to
    // itself, `get_or_assign_node_id` then mints a fresh one, and one note
    // becomes two documents: the copy already published under the old id comes
    // back from sync claiming a path that now belongs to the new id, the
    // coordinator sets the local file aside as `(conflict …)`, and every
    // autosave repeats it. Nine copies of one daily note is what that looks
    // like from the outside.
    //
    // Passing the known id as the hint means the file gets its own identity
    // back rather than a new one, so nothing splits.
    let known_id = db.get_node_id_by_path(&vault_id, &rel_path).ok().flatten();

    // We write to disk first so that get_or_assign_node_id can work
    std::fs::write(&abs_path, &file_content)?;

    let vault_path_obj = std::path::Path::new(&vault_path);
    let node_id = crate::sync::core::identity::get_or_assign_node_id_with_hint(
        vault_path_obj,
        &abs_path,
        known_id.as_deref(),
    )?;

    db.upsert_document_path(&vault_id, &node_id, &rel_path)?;

    // Now that get_or_assign_node_id has potentially injected a UUID, read the injected content
    let final_file_content = std::fs::read_to_string(&abs_path).unwrap_or(file_content);

    // --- Phase 1: CRDT Bridge ---
    if crate::sync::core::finance_document::is_structured(&rel_path) {
        // A Finance file is a list of records two devices add to at once, so
        // it keeps a real history rather than being replaced whole. Falls back
        // to replacement if this particular file cannot be taken apart.
        finance_apply_safe(&db, &vault_id, &node_id, &final_file_content)?;
    } else if ext == "json" || ext == "canvas" {
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
    delete_node_edges_for(&db, &rel_path);
    logged("drop node", &rel_path, db.delete_node(&rel_path));
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

    let untitled = || {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("Untitled {}", timestamp)
    };

    let title = match date_format {
        Some(fmt) => date_string_from_pattern(&fmt, chrono::Local::now()).unwrap_or_else(untitled),
        None => untitled(),
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

/// Today's date under a user-written pattern, or `None` if the pattern is not
/// one chrono can render.
///
/// Two commands took a date format straight from settings, translated the
/// friendly tokens and handed the result to chrono. An unrecognised `%`
/// escape makes `format(..).to_string()` **panic**, and the setting is a plain
/// text field: `YYYY-MM-DD (100%)` passes the front end's check — it contains
/// all three tokens — and then brings the command down.
///
/// So the translated pattern is inspected before it is used, and a pattern
/// chrono cannot read is reported rather than run.
pub(crate) fn date_string_from_pattern(
    pattern: &str,
    now: chrono::DateTime<chrono::Local>,
) -> Option<String> {
    let chrono_format = pattern
        .replace("YYYY", "%Y")
        .replace("YY", "%y")
        .replace("MM", "%m")
        .replace("M", "%-m")
        .replace("DD", "%d")
        .replace("D", "%-d");

    let readable = !chrono::format::StrftimeItems::new(&chrono_format)
        .into_iter()
        .any(|item| matches!(item, chrono::format::Item::Error));
    if !readable {
        log::warn!("date format '{}' is not one chrono can read", pattern);
        return None;
    }

    Some(now.format(&chrono_format).to_string())
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

    let today = chrono::Local::now();
    // A pattern chrono cannot read falls back rather than bringing the command
    // down; a daily note under yesterday's convention beats no daily note.
    let date_str = date_string_from_pattern(&format_str, today)
        .unwrap_or_else(|| today.format("%Y-%m-%d").to_string());

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

/// Whether a node has been finished for long enough to be filed away.
///
/// Pulled out of `archive_done_nodes` because it decides, unattended and on a
/// timer, that a file should be moved. Every clause is a way for that to
/// happen to something it should not: a node with no `completed_at`, one whose
/// date does not parse, one finished today.
pub(crate) fn is_ready_to_archive(
    node: &NodeMetadata,
    today: chrono::NaiveDate,
    days: u64,
) -> bool {
    let done = node
        .properties
        .get("status")
        .and_then(|s| s.as_str())
        .is_some_and(|s| s == "done");
    if !done {
        return false;
    }

    let Some(completed_at) = node
        .properties
        .get("completed_at")
        .and_then(|c| c.as_str())
        .filter(|c| !c.is_empty())
    else {
        // Marked done, but with no record of when. Moving it on a guess is the
        // one thing not to do with somebody's file.
        return false;
    };

    let date_part = completed_at.split_whitespace().next().unwrap_or("");
    let Ok(completed) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") else {
        return false;
    };

    today.signed_duration_since(completed).num_days() >= days as i64
}

/// Whether a node is already sitting in the archive.
///
/// Archiving moves a file into `archived/` and drops its database row, but the
/// next vault scan indexes it again at its new path — still a task, still
/// `done`, still with the same old `completed_at`. So the run after that found
/// it ready to archive a second time and renamed it on top of itself, which
/// `free_node_path` resolved by appending ` (1)`.
///
/// Left alone that repeats on every run. Measured on a real vault: all 93
/// archived tasks had accumulated a chain of ` (1)` suffixes, the longest
/// having reached the filesystem's 255-character ceiling — at which point the
/// rename simply fails and the file stops being archivable at all.
pub(crate) fn is_already_archived(node_id: &str, dir_name: &str) -> bool {
    let normalised = node_id.replace('\\', "/");
    normalised.starts_with(&format!("{dir_name}/archived/"))
}

#[tauri::command]
pub fn archive_done_nodes(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    node_type: String,
    days: u64,
) -> AppResult<u32> {
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

    let vault = Path::new(&vault_path);

    for node in nodes {
        // Already filed away. Without this the archive keeps re-archiving its
        // own contents; see `is_already_archived`.
        if is_already_archived(&node.id, dir_name) {
            continue;
        }
        if !is_ready_to_archive(&node, today, days) {
            continue;
        }

        let abs_path = vault.join(&node.id);
        if !abs_path.exists() {
            continue;
        }
        if !archived_dir.exists() {
            let _ = std::fs::create_dir_all(&archived_dir);
        }

        // A free name, not simply the one it had.
        //
        // Archiving keeps only the file's name, dropping the folders above it,
        // so `Notes/a/Họp.md` and `Notes/b/Họp.md` both arrive as
        // `Notes/archived/Họp.md`. `fs::rename` replaces its destination
        // without a word, so the second note archived silently destroyed the
        // first — and this runs on a timer, with nobody having pressed
        // anything.
        let file_name = abs_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel_dest = free_node_path(vault, &format!("{dir_name}/archived/{file_name}"));
        let dest = vault.join(&rel_dest);

        if std::fs::rename(&abs_path, &dest).is_ok() {
            archived_count += 1;

            // The row and the search entry are keyed by the old path, so they
            // go; the next scan re-adds the file at its new one.
            //
            // Its links do not go with them. Edges are keyed by the node's
            // stable identity, which the file carries into `archived/` —
            // deleting them here is what used to make archiving a task quietly
            // erase every backlink to it.
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            logged("drop node", &node.id, db.delete_node(&node.id));
            db.delete_search_entry(&node.id);
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

    // Named after its contents, so pasting the same screenshot into five
    // caps stores — and syncs — one file rather than five. See
    // `utils::asset_naming`.
    let safe_filename = crate::utils::asset_naming::content_name(&bytes, &filename);
    let target_path = assets_dir.join(&safe_filename);

    if target_path.exists() {
        return Ok(format!("assets/{}", safe_filename));
    }

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

    // Hashed in chunks rather than read whole: the picker hands over
    // whatever the user chose, which may be a photo straight off a camera.
    let filename = crate::utils::asset_naming::content_name_of_file(source)?;
    let target = assets_dir.join(&filename);

    if target.exists() {
        return Ok(format!("assets/{}", filename));
    }

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

/// Apply a CRDT update for a Finance file, keeping its per-row containers.
///
/// Same shape as `crdt_apply_safe` — a Loro panic must not take the app down —
/// with one extra fallback: a Finance file this build cannot take apart (a row
/// with no id, a shape from the future) is stored the old way rather than not
/// stored at all.
pub(crate) fn finance_apply_safe(
    db: &crate::db::DbBridge,
    vault_id: &str,
    node_id: &str,
    content: &str,
) -> AppResult<()> {
    if crate::sync::core::finance_document::split(content).is_none() {
        log::warn!("finance: {} could not be taken apart; storing it whole", node_id);
        return sync_crdt_snapshot_replace(db, vault_id, node_id, content);
    }

    let doc = db.get_crdt_doc(vault_id, node_id)?;
    let doc_ref = &doc;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::sync::core::crdt::apply_finance_update(doc_ref, content)
    }));

    match result {
        Ok(Ok(delta)) => {
            if !delta.is_empty() {
                db.save_crdt_delta(vault_id, node_id, delta)?;
            }
            Ok(())
        }
        Ok(Err(e)) => {
            log::warn!("finance: CRDT update failed for {node_id} ({e}); storing it whole");
            sync_crdt_snapshot_replace(db, vault_id, node_id, content)
        }
        Err(_panic) => {
            log::warn!("finance: CRDT panicked for {node_id}; storing it whole");
            sync_crdt_snapshot_replace(db, vault_id, node_id, content)
        }
    }
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
        crate::sync::core::crdt::apply_node_update(doc_ref, content)
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

    /// The bug a user reported: "Used by" said "not used by any node" for every
    /// file in the vault, including files plainly embedded in notes.
    ///
    /// Three things had to line up to produce it, and this test reproduces all
    /// three. The extractor had never known what an embedded attachment looked
    /// like, so no such link had ever been recorded. The vault scan skips a file
    /// whose modification time has not moved — which is every note that already
    /// exists — so teaching the extractor changed nothing on its own. And a note
    /// is indexed before the Files app has indexed anything, so even a fresh
    /// pass resolves the picture to a placeholder.
    #[test]
    fn an_existing_vault_learns_which_notes_use_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().to_string_lossy().to_string();

        // A note written long ago, sitting on disk, already indexed — with the
        // links an older version of this app worked out for it.
        std::fs::create_dir_all(dir.path().join("Notes")).unwrap();
        std::fs::write(
            dir.path().join("Notes/kien-truc.md"),
            "---\ntitle: Kiến trúc\ntype: note\n---\n\nSơ đồ: ![](assets/so-do.png)\n",
        )
        .unwrap();

        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&NodeMetadata {
            id: "Notes/kien-truc.md".into(),
            node_type: "note".into(),
            title: "Kiến trúc".into(),
            content: "Sơ đồ: ![](assets/so-do.png)".into(),
            properties: serde_json::json!({}),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "2026-01-01 00:00:00".into(),
            // Far in the future, so the scan is certain to skip the file —
            // exactly as it does for a note nobody has touched.
            timestamp: i64::MAX,
            blocks: None,
        })
        .unwrap();

        // The picture, indexed by the Files app the way it would be on launch.
        std::fs::create_dir_all(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets/so-do.png"), "dữ liệu ảnh").unwrap();

        let handle = app_with_db(db);
        let db_state = handle.state::<crate::db::DbState>();

        // Opening the vault: no links to this picture exist yet, and the note
        // will not be re-read.
        scan_vault_into_db(&handle, db_state.inner(), &vault).unwrap();

        // Then the Files app indexes the folder.
        let assets = dir.path().join("assets").to_string_lossy().to_string();
        crate::commands::files::scan_source_for_test(db_state.inner(), &assets).unwrap();

        let db = db_state.lock().unwrap();
        let file_id = db.get_nodes_by_type("file").unwrap().remove(0).id;
        let users = db.nodes_linking_to(&file_id).unwrap();

        assert_eq!(users.len(), 1, "the note embeds this picture");
        assert_eq!(users[0].2, "Kiến trúc");
        assert_eq!(users[0].3, "attachment");
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
        println!("  per file (cold) : {:?}", cold_time / count.max(1) as u32);
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
        let plan_by_item_id =
            explain("EXPLAIN QUERY PLAN DELETE FROM search_index WHERE item_id = ?1");
        let plan_by_rowid = explain("EXPLAIN QUERY PLAN DELETE FROM search_index WHERE rowid = ?1");
        let plan_lookup = explain(
            "EXPLAIN QUERY PLAN SELECT fts_rowid FROM search_index_rowids WHERE item_id = ?1",
        );

        let per = |d: std::time::Duration| d / count.max(1) as u32;
        println!("\n─── where the scan spends its time ({count} files) ───");
        println!("  parse file      : {parse:?}\t({:?}/file)", per(parse));
        println!(
            "  extract edges   : {edges_parse:?}\t({:?}/file)",
            per(edges_parse)
        );
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

    /// The regression that made a deleted note come back.
    ///
    /// Trashing a node moves its file to `.trash/`, which the watcher reports
    /// as a creation. `scan_specific_nodes` takes its paths straight from the
    /// watcher, so without the unscanned-directory rule it indexed the
    /// trashed file as a live node — and the note the user had just deleted
    /// reappeared a few seconds later.
    #[test]
    fn a_trashed_path_is_never_indexed() {
        for path in [
            ".trash/QuickCaps/a.md",
            ".trash/.trash/QuickCaps/a.md",
            ".git/config.md",
            "assets/note.md",
            "Files/thing.md",
            "Syn/chat.md",
        ] {
            assert!(
                is_in_unscanned_dir(path),
                "'{path}' must never reach the index"
            );
        }

        assert!(!is_in_unscanned_dir("QuickCaps/a.md"));
        assert!(!is_in_unscanned_dir("Notes/nested/deep.md"));
    }

    /// Cleaning up after the bug above: rows it already created have to go,
    /// even though their files are still on disk. The is-it-gone test cannot
    /// remove them, because it deliberately declines to judge files the walk
    /// never visited.
    #[test]
    fn rows_under_trash_are_purged_even_though_the_file_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let db = db();

        let trashed = seed(&db, ".trash/QuickCaps/a.md", "quickcap");
        let live = seed(&db, "QuickCaps/b.md", "quickcap");

        // Both files exist; only the trashed one is a mistake.
        for id in [&trashed.id, &live.id] {
            let path = dir.path().join(id);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "---\ntype: quickcap\n---\n").unwrap();
        }

        let on_disk: HashSet<String> = [live.id.clone()].into_iter().collect();
        let removed =
            remove_orphaned_nodes(&db, dir.path(), &[trashed.clone(), live.clone()], &on_disk);

        assert_eq!(removed, 1);
        assert!(
            db.get_node(&trashed.id).unwrap().is_none(),
            "the trashed row must go"
        );
        assert!(
            db.get_node(&live.id).unwrap().is_some(),
            "the live row must stay"
        );
        assert_eq!(search_entries(&db, &trashed.id), 0);
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
        assert!(db
            .get_node_block("Projects/gone.md", "blk001")
            .unwrap()
            .is_none());
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

        remove_orphaned_nodes(
            &db,
            empty_vault().path(),
            &[node],
            &on_disk(&["Notes/kept.md"]),
        );

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
        db.upsert_search_entry(
            "Notes/a.md",
            "note",
            "A",
            "",
            "",
            "{}",
            None,
            "",
            "Notes/a.md",
        );
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
        for backed in [
            "Notes/a.md",
            "Projects/p.md",
            "Finance/Config.json",
            "b.canvas",
        ] {
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
        assert_ne!(
            id_of(&a),
            id_of(&b),
            "two new notes were given one identity"
        );
    }

    #[test]
    fn a_tag_is_carried_into_the_frontmatter_and_an_empty_one_is_not() {
        let tagged = new_note_frontmatter("Daily", "note", Some("journal"));
        assert!(
            tagged.contains("tags:\n  - journal\n"),
            "the tag was dropped"
        );

        for empty in [Some(""), Some("   "), None] {
            let plain = new_note_frontmatter("Daily", "note", empty);
            assert!(
                !plain.contains("tags:"),
                "an empty tag should not produce a tags block: {empty:?}"
            );
        }
    }
}

#[cfg(test)]
mod archive_readiness_tests {
    use super::{is_already_archived, is_ready_to_archive};
    use crate::models::node::NodeMetadata;

    /// `archive_done_nodes` runs unattended, on a timer, and its decision moves
    /// a file out from under whatever has it open. Every clause below is a way
    /// for that to happen to something it should not, so each gets its own
    /// test rather than sharing one.
    fn node(properties: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: "Tasks/a.md".to_string(),
            node_type: "task".to_string(),
            title: "a task".to_string(),
            content: String::new(),
            properties,
            created_at: String::new(),
            updated_at: String::new(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn today() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 8, 23).unwrap()
    }

    #[test]
    fn a_task_finished_long_enough_ago_is_filed_away() {
        let n = node(serde_json::json!({ "status": "done", "completed_at": "2026-08-13" }));
        assert!(is_ready_to_archive(&n, today(), 7));
    }

    #[test]
    fn the_boundary_day_counts_as_ready() {
        let n = node(serde_json::json!({ "status": "done", "completed_at": "2026-08-16" }));
        assert!(is_ready_to_archive(&n, today(), 7));
    }

    #[test]
    fn a_task_finished_one_day_too_recently_stays_put() {
        let n = node(serde_json::json!({ "status": "done", "completed_at": "2026-08-17" }));
        assert!(!is_ready_to_archive(&n, today(), 7));
    }

    #[test]
    fn a_task_finished_today_stays_put() {
        let n = node(serde_json::json!({ "status": "done", "completed_at": "2026-08-23" }));
        assert!(!is_ready_to_archive(&n, today(), 7));
    }

    #[test]
    fn unfinished_work_is_never_filed_away_however_old() {
        for status in ["todo", "in_progress", "backlog"] {
            let n = node(serde_json::json!({ "status": status, "completed_at": "2020-01-01" }));
            assert!(!is_ready_to_archive(&n, today(), 7), "status {status}");
        }
    }

    /// Marked done, but with no record of when. Moving it on a guess is the one
    /// thing not to do with somebody's file.
    #[test]
    fn a_task_with_no_completion_date_stays_put() {
        let n = node(serde_json::json!({ "status": "done" }));
        assert!(!is_ready_to_archive(&n, today(), 7));
        let blank = node(serde_json::json!({ "status": "done", "completed_at": "" }));
        assert!(!is_ready_to_archive(&blank, today(), 7));
    }

    #[test]
    fn a_completion_date_that_does_not_parse_stays_put() {
        for value in ["yesterday", "23/08/2026", "2026-13-45", "null"] {
            let n = node(serde_json::json!({ "status": "done", "completed_at": value }));
            assert!(!is_ready_to_archive(&n, today(), 7), "completed_at {value}");
        }
    }

    /// The board writes a bare date; older files carry a full timestamp. Both
    /// have to read the same, or archiving would skip every task written by
    /// the other one.
    #[test]
    fn a_completion_timestamp_reads_the_same_as_a_bare_date() {
        let n = node(serde_json::json!({ "status": "done", "completed_at": "2026-08-13 09:31:00" }));
        assert!(is_ready_to_archive(&n, today(), 7));
    }

    /// A date in the future is not a countdown that has elapsed. It happens
    /// when a device with a wrong clock syncs in.
    #[test]
    fn a_completion_date_in_the_future_stays_put() {
        let n = node(serde_json::json!({ "status": "done", "completed_at": "2026-09-30" }));
        assert!(!is_ready_to_archive(&n, today(), 7));
    }

    #[test]
    fn a_zero_day_setting_files_away_everything_already_finished() {
        let n = node(serde_json::json!({ "status": "done", "completed_at": "2026-08-23" }));
        assert!(is_ready_to_archive(&n, today(), 0));
    }

    /// The defect that ate 93 filenames.
    ///
    /// An archived file is indexed again at its new path, still done and still
    /// carrying its old completion date, so every later run found it ready and
    /// renamed it on top of itself — ` (1)`, then ` (1) (1)`, until the name
    /// hit the filesystem's limit.
    #[test]
    fn a_file_already_in_the_archive_is_left_alone() {
        assert!(is_already_archived("Tasks/archived/abc.md", "Tasks"));
        assert!(is_already_archived("Notes/archived/deep/abc.md", "Notes"));
    }

    #[test]
    fn a_file_still_in_the_open_folder_is_not() {
        assert!(!is_already_archived("Tasks/abc.md", "Tasks"));
        assert!(!is_already_archived("Tasks/subfolder/abc.md", "Tasks"));
    }

    /// A Windows vault hands back backslashes; the same file has to read the
    /// same way or the guard would not hold there.
    #[test]
    fn a_windows_path_reads_the_same() {
        assert!(is_already_archived("Tasks\\archived\\abc.md", "Tasks"));
    }

    /// A folder that merely starts with the same letters is a different folder.
    #[test]
    fn a_similarly_named_folder_is_not_the_archive() {
        assert!(!is_already_archived("Tasks/archived-notes/abc.md", "Tasks"));
        assert!(!is_already_archived("TasksArchive/archived/abc.md", "Tasks"));
    }

    /// Each type has its own archive, and one must not shield another.
    #[test]
    fn the_guard_is_scoped_to_the_type_being_archived() {
        assert!(!is_already_archived("Notes/archived/abc.md", "Tasks"));
    }

    #[test]
    fn a_node_with_no_status_at_all_stays_put() {
        let n = node(serde_json::json!({ "completed_at": "2020-01-01" }));
        assert!(!is_ready_to_archive(&n, today(), 7));
    }
}

#[cfg(test)]
mod existing_body_tests {
    use super::existing_body;
    use std::io::Write;

    fn write_temp(name: &str, contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(contents.as_bytes()).expect("write");
        (dir, path)
    }

    /// The whole point: a property-only write must not be able to revert the
    /// body to whatever the caller happened to be holding.
    ///
    /// The surrounding whitespace goes, which is what `parse_file_to_node`
    /// does to the same file — so a body read here and a body read by the
    /// indexer are the same string, and a property-only write is a no-op on
    /// the text rather than a whitespace edit that syncs.
    #[test]
    fn a_markdown_body_comes_back_without_its_frontmatter() {
        let (_d, p) = write_temp(
            "a.md",
            "---\ntitle: A task\nstatus: todo\n---\nthe body\n\nand more of it\n",
        );
        assert_eq!(existing_body(&p, "md"), "the body\n\nand more of it");
    }

    #[test]
    fn a_file_with_no_frontmatter_is_all_body() {
        let (_d, p) = write_temp("a.md", "just text\n");
        assert_eq!(existing_body(&p, "md"), "just text");
    }

    #[test]
    fn an_empty_body_reads_as_empty_not_as_the_frontmatter() {
        let (_d, p) = write_temp("a.md", "---\ntitle: A task\n---\n");
        assert_eq!(existing_body(&p, "md").trim(), "");
    }

    #[test]
    fn a_json_node_yields_its_content_field() {
        let (_d, p) = write_temp("a.canvas", r#"{"title":"Board","content":"{\"nodes\":[]}"}"#);
        assert_eq!(existing_body(&p, "canvas"), "{\"nodes\":[]}");
    }

    /// A path with nothing at it is the create case, where there is no body to
    /// keep. Returning empty is what lets a create and an update share a code
    /// path.
    #[test]
    fn a_missing_file_has_no_body_rather_than_failing() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert_eq!(existing_body(&dir.path().join("nope.md"), "md"), "");
    }

    /// Frontmatter that does not parse must not take the body down with it.
    #[test]
    fn a_file_whose_frontmatter_is_broken_still_yields_something() {
        let (_d, p) = write_temp("a.md", "---\nthis: [is: not: yaml\n---\nthe body\n");
        assert!(existing_body(&p, "md").contains("the body"));
    }
}
