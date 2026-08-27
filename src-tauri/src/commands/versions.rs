//! Reading a note as it used to be.
//!
//! Every save already goes through a CRDT: `crdt_apply_safe` turns the file's
//! new text into character operations and appends them to a Loro document.
//! That log is not a side effect of sync — it is a complete record of how the
//! file came to look the way it does, kept on disk in `sync_crdt_updates` and
//! folded into `sync_crdt_documents` when it grows. Loro's snapshots carry the
//! history along with the state, so compaction shortens the log without
//! forgetting anything.
//!
//! So the versions were always there. What was missing was any way to ask.
//!
//! # What a version is
//!
//! One Loro *change*: a run of edits by one device, uninterrupted for
//! [`crate::db::crdt::VERSION_MERGE_INTERVAL_MS`]. A sitting, roughly — not a keystroke, and
//! not a whole day. Its timestamp is when that sitting began.
//!
//! # What restoring does
//!
//! It writes the old text back as a new edit, and never rewinds the log. That
//! is not caution for its own sake: another device holds operations built on
//! top of the ones a rewind would remove, and the merge that follows would be
//! between a history and a version of that history with pieces missing. Moving
//! forward to an old state is a thing every peer can agree on. Going back is
//! not.

use std::path::Path;

use serde::Serialize;

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use crate::path_utils;

/// One entry in a note's history.
#[derive(Debug, Clone, Serialize)]
pub struct NodeVersion {
    /// `peer:counter` of the change's last operation, which is also the
    /// version to ask for when reading or restoring it.
    pub id: String,
    /// Unix **milliseconds** when the sitting began — Loro's own unit, passed
    /// through unconverted so it drops straight into a `Date`.
    ///
    /// `None` for a change recorded before the app kept time. Undated is the
    /// honest answer there; a date derived from something else would be a
    /// guess wearing a timestamp.
    pub timestamp: Option<i64>,
    /// Characters in the whole file — frontmatter included — at this version.
    pub size: usize,
    /// Characters this version added, or removed if negative, against the one
    /// before it. What a reader actually scans the list for.
    pub delta: i64,
    /// Whether this is the version currently on disk.
    pub is_current: bool,
    /// Whether this device wrote it. A vault synced across a laptop and a
    /// phone has a history from both, and which is which is worth knowing.
    pub is_local: bool,
}

/// `peer:counter`, the form a version id takes over the wire.
fn version_id(id: loro::ID) -> String {
    format!("{}:{}", id.peer, id.counter)
}

fn parse_version_id(raw: &str) -> AppResult<loro::ID> {
    let (peer, counter) = raw
        .split_once(':')
        .ok_or_else(|| AppError::General(format!("'{raw}' is not a version id")))?;
    Ok(loro::ID {
        peer: peer
            .parse()
            .map_err(|_| AppError::General(format!("'{raw}' has no readable peer")))?,
        counter: counter
            .parse()
            .map_err(|_| AppError::General(format!("'{raw}' has no readable position")))?,
    })
}

/// The vault's id and the document id behind a path, or a clear refusal.
///
/// A note that has never been saved by this build has no CRDT document, and
/// there is nothing wrong with that — it simply has no history yet. Saying so
/// beats an error the caller has to interpret.
///
/// Takes the `DbState` rather than a guard on it. `load_or_register_vault_identity`
/// reaches for that same mutex, and it is not reentrant, so a caller holding
/// the lock across this call hangs the app rather than failing. The identity
/// has to be read first and let go of first — the same ordering
/// `scan_vault_into_db` and `write_node_file` already observe.
fn document_for<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    state: &DbState,
    vault_path: &str,
    rel_path: &str,
) -> AppResult<Option<(String, String)>> {
    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(app_handle, vault_path)?;
    let vault_id = identity.vault_id.to_string();

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let doc_id = db.get_node_id_by_path(&vault_id, rel_path)?;
    Ok(doc_id.map(|doc_id| (vault_id, doc_id)))
}

/// Every version of a note, newest first.
///
/// Ordered by Lamport timestamp rather than by wall clock. The two agree for
/// one device, and where they disagree — a phone whose clock is wrong, edits
/// made offline and synced later — causal order is the one that produces a
/// list where each entry follows from the one above it.
///
/// # Cost
///
/// Each version is measured by checking the document out at it and reading the
/// text, so this is linear in versions *and* in document size. Measured at
/// 200µs per version on a 40k-character note — 60ms for a note with three
/// hundred separate sittings behind it (`version_listing_cost`, below).
///
/// That is comfortable, and it is worth knowing it is spent holding the
/// database lock: this does not just make the panel slow if it grows, it stops
/// every other command until it returns. If a vault ever turns up where this
/// matters, the fix is to stop measuring versions nobody has scrolled to —
/// the sizes are only there to draw one column.
#[tauri::command]
pub fn list_node_versions<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
) -> AppResult<Vec<NodeVersion>> {
    let Some((vault_id, doc_id)) = document_for(&app_handle, &state, &vault_path, &rel_path)?
    else {
        return Ok(Vec::new());
    };

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let doc = db.get_crdt_doc(&vault_id, &doc_id)?;
    let this_peer = db.get_or_create_peer_id()?;

    // (lamport, last op's id, timestamp) for every change in the log.
    let mut marks: Vec<(u32, loro::ID, i64)> = doc.with_oplog(|oplog| {
        let mut marks = Vec::new();
        for changes in oplog.changes().values() {
            for change in changes.iter() {
                let last = loro::ID {
                    peer: change.id().peer,
                    // A change covers a run of counters; the version it leaves
                    // behind is the one after its final operation.
                    counter: change.id().counter + change.ops().atom_len() - 1,
                };
                marks.push((change.lamport(), last, change.timestamp()));
            }
        }
        marks
    });
    marks.sort_by_key(|(lamport, id, _)| (*lamport, id.peer, id.counter));

    let mut versions = Vec::with_capacity(marks.len());
    let mut previous_size = 0i64;
    for (_, id, timestamp) in &marks {
        // Read the document as it stood once this change had landed. The doc
        // is rebuilt from storage on every call to `get_crdt_doc`, so moving it
        // around here cannot disturb what is saved.
        let size = match doc.checkout(&(*id).into()) {
            Ok(()) => crate::sync::core::crdt::node_text(&doc).chars().count(),
            Err(e) => {
                log::warn!(
                    "could not read version {} of {}: {:?}",
                    version_id(*id),
                    rel_path,
                    e
                );
                continue;
            }
        };

        versions.push(NodeVersion {
            id: version_id(*id),
            timestamp: (*timestamp > 0).then_some(*timestamp),
            size,
            delta: size as i64 - previous_size,
            is_current: false,
            is_local: id.peer == this_peer,
        });
        previous_size = size as i64;
    }
    doc.checkout_to_latest();

    if let Some(newest) = versions.last_mut() {
        newest.is_current = true;
    }

    versions.reverse();
    Ok(versions)
}

/// The whole file — frontmatter and body — as it stood at one version.
#[tauri::command]
pub fn read_node_version<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
    version_id: String,
) -> AppResult<String> {
    let Some((vault_id, doc_id)) = document_for(&app_handle, &state, &vault_path, &rel_path)?
    else {
        return Err(AppError::General(format!(
            "'{rel_path}' has no saved history"
        )));
    };

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let doc = db.get_crdt_doc(&vault_id, &doc_id)?;
    let id = parse_version_id(&version_id)?;
    doc.checkout(&id.into())
        .map_err(|e| AppError::General(format!("Version {version_id} cannot be read: {e:?}")))?;
    let text = crate::sync::core::crdt::node_text(&doc);
    doc.checkout_to_latest();
    Ok(text)
}

/// One line of a diff, and which side it came from.
#[derive(Debug, Clone, Serialize)]
pub struct DiffLine {
    /// `"equal"`, `"insert"` or `"delete"`.
    pub kind: &'static str,
    /// The line, without its trailing newline.
    pub text: String,
}

/// A run of changed lines together with the unchanged ones around it.
///
/// Groups rather than one flat list, so the reader can be shown the parts that
/// moved and a fold in place of the parts that did not. A note is mostly
/// unchanged between two versions; printing all of it to show three edited
/// lines is how a diff stops being a diff.
#[derive(Debug, Clone, Serialize)]
pub struct DiffGroup {
    pub lines: Vec<DiffLine>,
    /// Which line of the newer text this group starts at, for a gutter.
    pub start_line: usize,
}

/// What changed between two versions, and by how much.
#[derive(Debug, Clone, Serialize)]
pub struct VersionDiff {
    pub groups: Vec<DiffGroup>,
    pub added: usize,
    pub removed: usize,
    /// True when the two sides are identical, which the groups cannot say —
    /// an unchanged file produces no groups, and so does a failed read.
    pub unchanged: bool,
}

/// How many unchanged lines to keep on each side of a change.
const DIFF_CONTEXT_LINES: usize = 3;

/// What a version changed, against either the one before it or the one on disk.
///
/// Two questions get asked of a history and they want different answers.
/// "What did I do in this sitting?" is answered against the previous version.
/// "What happens if I restore this?" is answered against the current one. The
/// work is identical either way — only which text goes on the left changes —
/// so the caller picks with `against`.
#[tauri::command]
pub fn diff_node_version<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
    version_id: String,
    against: String,
) -> AppResult<VersionDiff> {
    let Some((vault_id, doc_id)) = document_for(&app_handle, &state, &vault_path, &rel_path)?
    else {
        return Err(AppError::General(format!(
            "'{rel_path}' has no saved history"
        )));
    };

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let doc = db.get_crdt_doc(&vault_id, &doc_id)?;
    let id = parse_version_id(&version_id)?;

    let newer = {
        doc.checkout(&id.into()).map_err(|e| {
            AppError::General(format!("Version {version_id} cannot be read: {e:?}"))
        })?;
        crate::sync::core::crdt::node_text(&doc)
    };

    let older = match against.as_str() {
        // The version on disk, which is where a restore would start from.
        "current" => {
            doc.checkout_to_latest();
            crate::sync::core::crdt::node_text(&doc)
        }
        // The state this change was made on top of. Its own dependencies are
        // exactly that, and asking Loro beats guessing at the entry above it in
        // a list that may interleave two devices.
        _ => {
            let deps = doc.with_oplog(|oplog| oplog.get_change_at(id).map(|c| c.deps().clone()));
            match deps {
                // No dependencies means this is where the document began, so
                // there is nothing before it and everything in it is new.
                Some(deps) if !deps.is_empty() => {
                    doc.checkout(&deps).map_err(|e| {
                        AppError::General(format!(
                            "Version {version_id} has no readable parent: {e:?}"
                        ))
                    })?;
                    crate::sync::core::crdt::node_text(&doc)
                }
                _ => String::new(),
            }
        }
    };
    doc.checkout_to_latest();

    // `against: "current"` reads as "what this version would change", so the
    // version being inspected is the new side either way.
    Ok(diff_texts(&older, &newer))
}

/// Line diff between two texts, folded down to the parts that moved.
fn diff_texts(older: &str, newer: &str) -> VersionDiff {
    use similar::{ChangeTag, TextDiff};

    if older == newer {
        return VersionDiff {
            groups: Vec::new(),
            added: 0,
            removed: 0,
            unchanged: true,
        };
    }

    let diff = TextDiff::from_lines(older, newer);
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut groups = Vec::new();

    for ops in diff.grouped_ops(DIFF_CONTEXT_LINES) {
        let start_line = ops.first().map(|op| op.new_range().start + 1).unwrap_or(1);
        let mut lines = Vec::new();
        for op in ops {
            for change in diff.iter_changes(&op) {
                let kind = match change.tag() {
                    ChangeTag::Equal => "equal",
                    ChangeTag::Insert => {
                        added += 1;
                        "insert"
                    }
                    ChangeTag::Delete => {
                        removed += 1;
                        "delete"
                    }
                };
                lines.push(DiffLine {
                    kind,
                    text: change.value().trim_end_matches(['\n', '\r']).to_string(),
                });
            }
        }
        groups.push(DiffGroup { lines, start_line });
    }

    VersionDiff {
        groups,
        added,
        removed,
        unchanged: false,
    }
}

/// Put an old version back, as a new edit on top of the current one.
///
/// Returns the restored text, so the editor can show it without a round trip
/// and without waiting for the file watcher to notice.
#[tauri::command]
pub fn restore_node_version<R: tauri::Runtime>(
    app_handle: tauri::AppHandle<R>,
    state: tauri::State<'_, DbState>,
    vault_path: String,
    rel_path: String,
    version_id: String,
) -> AppResult<String> {
    let abs_path = path_utils::resolve_safe_path(&vault_path, &rel_path)?;
    let text = read_node_version(
        app_handle.clone(),
        state.clone(),
        vault_path.clone(),
        rel_path.clone(),
        version_id,
    )?;

    // Refusing beats writing an empty file over a note. A version whose text is
    // empty is either a document that genuinely started empty — in which case
    // restoring it achieves nothing — or a sign the checkout did not land.
    if text.trim().is_empty() {
        return Err(AppError::General(
            "That version holds no text; nothing was changed.".to_string(),
        ));
    }

    // Identity before the lock, never inside it: see `document_for`.
    let identity =
        crate::sync::core::identity::load_or_register_vault_identity(&app_handle, &vault_path)?;
    let vault_id = identity.vault_id.to_string();

    std::fs::write(&abs_path, &text)?;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // The same path an edit made outside the app takes: the file changed, so
    // the document, the row, the search index and the graph all follow it.
    let ext = abs_path.extension().and_then(|e| e.to_str()).unwrap_or("");
    crate::commands::nodes::bridge_external_edits(
        &db,
        Path::new(&vault_path),
        &abs_path,
        &vault_id,
        &rel_path,
        ext,
    )?;
    crate::commands::nodes::reindex_node_at(&db, &vault_path, &abs_path);

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;

    /// Save each text in turn, the way an edit is saved.
    ///
    /// `merge_interval` is what a test is really choosing here. Under the
    /// production value these saves land milliseconds apart and Loro folds
    /// them into one change — correctly, since they are one sitting — leaving
    /// a single version to inspect. Passing `0` makes every save its own
    /// change, which is what a test about the contents of the log needs.
    fn doc_with_history(
        db: &DbBridge,
        vault: &str,
        doc_id: &str,
        merge_interval: i64,
        texts: &[&str],
    ) {
        for text in texts {
            let doc = db
                .get_crdt_doc_grouped(vault, doc_id, merge_interval)
                .expect("load doc");
            let delta =
                crate::sync::core::crdt::apply_text_update(&doc, text).expect("apply text update");
            db.save_crdt_delta(vault, doc_id, delta)
                .expect("save delta");
        }
    }

    /// Read every version back the way `list_node_versions` does, without the
    /// Tauri plumbing the command needs.
    fn versions_of(db: &DbBridge, vault: &str, doc_id: &str, merge_interval: i64) -> Vec<String> {
        let doc = db
            .get_crdt_doc_grouped(vault, doc_id, merge_interval)
            .expect("load doc");
        let mut marks: Vec<(u32, loro::ID)> = doc.with_oplog(|oplog| {
            let mut marks = Vec::new();
            for changes in oplog.changes().values() {
                for change in changes.iter() {
                    marks.push((
                        change.lamport(),
                        loro::ID {
                            peer: change.id().peer,
                            counter: change.id().counter + change.ops().atom_len() - 1,
                        },
                    ));
                }
            }
            marks
        });
        marks.sort_by_key(|(lamport, id)| (*lamport, id.peer, id.counter));

        let mut out = Vec::new();
        for (_, id) in marks {
            doc.checkout(&id.into()).expect("checkout");
            out.push(doc.get_text("content").to_string());
        }
        doc.checkout_to_latest();
        out
    }

    fn seeded_db() -> DbBridge {
        let db = DbBridge::new_in_memory_full().expect("schema");
        db.conn()
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at)
                 VALUES ('v1', '/tmp/v1', 100, 100)",
                [],
            )
            .expect("seed vault");
        db
    }

    /// The claim the whole feature rests on: the text of an earlier save is
    /// still recoverable from the log, exactly as it was.
    #[test]
    fn each_save_is_recoverable_as_the_text_it_wrote() {
        let db = seeded_db();
        doc_with_history(
            &db,
            "v1",
            "doc-1",
            0,
            &["first draft", "first draft, revised", "rewritten entirely"],
        );

        let seen = versions_of(&db, "v1", "doc-1", 0);

        assert_eq!(
            seen,
            vec![
                "first draft".to_string(),
                "first draft, revised".to_string(),
                "rewritten entirely".to_string(),
            ]
        );
    }

    /// Compaction is what keeps the update log from growing without bound, and
    /// it is also the thing most likely to quietly cost the history. Loro's
    /// snapshots carry the operations as well as the state; this holds it to
    /// that, because everything above stops being true if it ever changes.
    #[test]
    fn compaction_shortens_the_log_without_forgetting_what_was_in_it() {
        let db = seeded_db();
        doc_with_history(&db, "v1", "doc-1", 0, &["one", "one two", "one two three"]);
        let before = versions_of(&db, "v1", "doc-1", 0);

        let mut db = db;
        db.compact_crdt_history("v1", "doc-1").expect("compact");

        let remaining: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sync_crdt_updates WHERE vault_id = 'v1' AND doc_id = 'doc-1'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("count updates");
        assert_eq!(remaining, 0, "compaction should have collapsed the log");

        assert_eq!(
            versions_of(&db, "v1", "doc-1", 0),
            before,
            "the snapshot must carry the history, not just the latest state"
        );
    }

    /// Timestamps are the difference between a history and a list of anonymous
    /// blobs, and Loro records none unless it is told to.
    #[test]
    fn a_change_written_now_carries_the_time_it_was_written() {
        let db = seeded_db();
        let before = chrono::Utc::now().timestamp_millis();
        doc_with_history(&db, "v1", "doc-1", 0, &["something"]);

        let doc = db.get_crdt_doc("v1", "doc-1").expect("load doc");
        let stamps: Vec<i64> = doc.with_oplog(|oplog| {
            oplog
                .changes()
                .values()
                .flat_map(|changes| changes.iter().map(|c| c.timestamp()))
                .collect()
        });

        assert!(!stamps.is_empty(), "the change should be in the log");
        assert!(
            stamps.iter().all(|t| *t >= before),
            "a change should be stamped with roughly now, got {stamps:?}"
        );
    }

    /// What the history looks like to someone who writes for ten minutes: a
    /// handful of entries they can tell apart, not one per autosave.
    ///
    /// Autosave fires 600ms after a keystroke, so an afternoon of writing is
    /// thousands of saves. A list with one row each is not a history anybody
    /// can use, and Loro will fold them for us as long as the interval says to
    /// — which is the entire reason that interval is set rather than left at
    /// its default.
    #[test]
    fn saves_from_one_sitting_collapse_into_a_single_version() {
        let db = seeded_db();
        doc_with_history(
            &db,
            "v1",
            "doc-1",
            crate::db::crdt::VERSION_MERGE_INTERVAL_MS,
            &["a", "ab", "abc", "abcd", "abcde"],
        );

        let seen = versions_of(
            &db,
            "v1",
            "doc-1",
            crate::db::crdt::VERSION_MERGE_INTERVAL_MS,
        );

        assert_eq!(
            seen,
            vec!["abcde".to_string()],
            "five saves seconds apart are one sitting and should read as one version"
        );
    }

    /// The three commands driven the way the app drives them, against a real
    /// vault directory and a real database.
    ///
    /// Worth the setup for two reasons the unit tests above cannot reach.
    /// `load_or_register_vault_identity` takes the same mutex the commands
    /// hold, and a lock taken in the wrong order does not fail a test — it
    /// hangs one, which is the only way that bug ever shows itself. And
    /// restoring is the one operation here that writes to the user's disk.
    #[test]
    fn a_note_can_be_listed_read_and_put_back_the_way_it_was() {
        use tauri::Manager;

        let holder = tempfile::tempdir().expect("tempdir");
        let vault = holder.path().join("vault");
        std::fs::create_dir_all(vault.join("Notes")).expect("vault dir");
        let vault_path = vault.to_string_lossy().to_string();

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        let handle = app.handle().clone();
        handle.manage(crate::db::DbState::new(
            DbBridge::new_in_memory_full().expect("schema"),
        ));
        let state = handle.state::<crate::db::DbState>();

        let rel_path = "Notes/history.md";
        let abs_path = vault.join("Notes/history.md");
        // Both carry the same `node_id`, as a real note does. Without one,
        // `get_or_assign_node_id` mints a fresh identity for each write and the
        // two sittings end up in two different documents — which is the same
        // split that used to turn one daily note into nine files.
        let first = "---\ntitle: History\ntype: note\nnode_id: fixed-id-1\n---\nthe first draft\n";
        let second =
            "---\ntitle: History\ntype: note\nnode_id: fixed-id-1\n---\nrewritten, and worse\n";

        // Two sittings, an hour apart as far as the log is concerned.
        //
        // The commands under test group the history at the production interval,
        // so two saves microseconds apart would arrive as the one version they
        // genuinely are and there would be nothing to restore. The gap has to
        // come from somewhere, and a test cannot wait an hour — hence the
        // explicit commit timestamps. Everything else is the real path: the
        // same text diff, the same delta, the same rows.
        let identity =
            crate::sync::core::identity::load_or_register_vault_identity(&handle, &vault_path)
                .expect("vault identity");
        let vault_id = identity.vault_id.to_string();
        let start = chrono::Utc::now().timestamp_millis() - 2 * 60 * 60 * 1000;
        for (offset, text) in [(0, first), (60 * 60 * 1000, second)] {
            std::fs::write(&abs_path, text).expect("write note");
            let db = state.lock().unwrap_or_else(|e| e.into_inner());
            let doc_id =
                crate::sync::core::identity::get_or_assign_node_id(&vault, &abs_path).expect("id");
            db.upsert_document_path(&vault_id, &doc_id, rel_path)
                .expect("path");
            let doc = db.get_crdt_doc(&vault_id, &doc_id).expect("load doc");
            let before = doc.oplog_vv();
            let handler = doc.get_text("content");
            handler.delete(0, handler.len_unicode()).expect("clear");
            handler.insert(0, text).expect("insert");
            doc.commit_with(loro::CommitOptions::new().timestamp(start + offset));
            db.save_crdt_delta(&vault_id, &doc_id, doc.export_from(&before))
                .expect("save delta");
        }

        let versions = list_node_versions(
            handle.clone(),
            state.clone(),
            vault_path.clone(),
            rel_path.to_string(),
        )
        .expect("list versions");

        assert_eq!(
            versions.len(),
            2,
            "two sittings, two versions: {versions:?}"
        );
        assert!(
            versions[0].is_current,
            "the newest entry is what is on disk"
        );
        assert!(!versions[1].is_current);
        assert!(
            versions.iter().all(|v| v.is_local),
            "this device wrote both: {versions:?}"
        );
        assert!(
            versions.iter().all(|v| v.timestamp.is_some()),
            "changes written now should be dated: {versions:?}"
        );

        let oldest = &versions[1];
        let read_back = read_node_version(
            handle.clone(),
            state.clone(),
            vault_path.clone(),
            rel_path.to_string(),
            oldest.id.clone(),
        )
        .expect("read version");
        assert_eq!(read_back, first);

        let restored = restore_node_version(
            handle.clone(),
            state.clone(),
            vault_path.clone(),
            rel_path.to_string(),
            oldest.id.clone(),
        )
        .expect("restore version");

        assert_eq!(restored, first);
        assert_eq!(
            std::fs::read_to_string(&abs_path).expect("read note"),
            first,
            "the restore has to reach the file, not just the reply"
        );

        // A restore is an edit forward, so the history grows rather than
        // rewinding — the version that was current is still there to go back to.
        let after = list_node_versions(
            handle.clone(),
            state.clone(),
            vault_path,
            rel_path.to_string(),
        )
        .expect("list versions again");
        assert!(
            after.len() > versions.len(),
            "restoring should add a version, not remove one: {after:?}"
        );
    }

    fn kinds(diff: &VersionDiff) -> Vec<(&str, String)> {
        diff.groups
            .iter()
            .flat_map(|g| g.lines.iter())
            .map(|l| (l.kind, l.text.clone()))
            .collect()
    }

    /// The point of showing a diff rather than the whole file: a note is
    /// mostly unchanged, and a reader is looking for the part that is not.
    #[test]
    fn only_the_changed_lines_and_their_surroundings_are_returned() {
        let older = (1..=40)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let newer = older.replace("line 20", "line twenty, rewritten");

        let diff = diff_texts(&older, &newer);

        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 1);
        assert!(!diff.unchanged);
        let shown = kinds(&diff);
        assert!(
            shown.len() <= 2 + DIFF_CONTEXT_LINES * 2 + 2,
            "one edited line in forty should not print forty: {shown:?}"
        );
        assert!(shown.contains(&("delete", "line 20".to_string())));
        assert!(shown.contains(&("insert", "line twenty, rewritten".to_string())));
        assert!(shown.iter().any(|(kind, _)| *kind == "equal"));
    }

    /// Two identical versions produce no groups — and so does a diff that
    /// failed to read anything. `unchanged` is what tells the two apart, so the
    /// panel can say "nothing changed here" instead of showing a blank pane.
    #[test]
    fn an_unchanged_version_says_so_rather_than_returning_an_empty_diff() {
        let diff = diff_texts("same\ntext\n", "same\ntext\n");

        assert!(diff.unchanged);
        assert!(diff.groups.is_empty());
        assert_eq!((diff.added, diff.removed), (0, 0));
    }

    /// A version's parent is asked of the oplog rather than taken to be the row
    /// above it in the list. For the first version there is no parent at all,
    /// and the whole note has to read as added rather than as an error.
    #[test]
    fn the_first_version_of_a_note_is_all_addition() {
        let diff = diff_texts("", "the very first draft\n");

        assert!(!diff.unchanged);
        assert_eq!(diff.added, 1);
        assert_eq!(diff.removed, 0);
        assert_eq!(
            kinds(&diff),
            vec![("insert", "the very first draft".to_string())]
        );
    }

    /// Line endings are stripped so the front end can lay the lines out itself.
    /// Leaving them on put a blank row under every line of the diff.
    #[test]
    fn diff_lines_do_not_carry_their_line_endings() {
        let diff = diff_texts("alpha\r\n", "beta\r\n");

        for (_, text) in kinds(&diff) {
            assert!(
                !text.contains('\n') && !text.contains('\r'),
                "line still carries its ending: {text:?}"
            );
        }
    }

    /// What listing costs on a note with a long history.
    ///
    /// `list_node_versions` walks every version and checks the document out at
    /// each one to measure it, and it does that holding the database lock. If
    /// that walk is slow it does not merely make the panel slow — it stops
    /// every other command in the app until it finishes.
    ///
    /// ```text
    /// cargo test --lib version_listing_cost -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "measurement, not an assertion"]
    fn version_listing_cost() {
        let db = seeded_db();

        // A note grown a paragraph at a time, each its own version.
        let mut text = String::new();
        let mut texts = Vec::new();
        for i in 0..300 {
            text.push_str(&format!(
                "Paragraph {i}. Some prose to give the document a realistic size, \
                 because checkout cost follows the document rather than the count.\n\n"
            ));
            texts.push(text.clone());
        }
        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        doc_with_history(&db, "v1", "doc-1", 0, &refs);

        let doc = db.get_crdt_doc_grouped("v1", "doc-1", 0).expect("load doc");

        let load = std::time::Instant::now();
        let mut marks: Vec<(u32, loro::ID)> = doc.with_oplog(|oplog| {
            let mut marks = Vec::new();
            for changes in oplog.changes().values() {
                for change in changes.iter() {
                    marks.push((
                        change.lamport(),
                        loro::ID {
                            peer: change.id().peer,
                            counter: change.id().counter + change.ops().atom_len() - 1,
                        },
                    ));
                }
            }
            marks
        });
        marks.sort_by_key(|(lamport, id)| (*lamport, id.peer, id.counter));
        let oplog_time = load.elapsed();

        let walk = std::time::Instant::now();
        let mut total = 0usize;
        for (_, id) in &marks {
            doc.checkout(&(*id).into()).expect("checkout");
            total += doc.get_text("content").to_string().chars().count();
        }
        doc.checkout_to_latest();
        let walk_time = walk.elapsed();

        println!("  versions        : {}", marks.len());
        println!("  final size      : {} chars", text.chars().count());
        println!("  oplog scan      : {oplog_time:?}");
        println!("  checkout walk   : {walk_time:?}  ({total} chars read)");
        println!(
            "  per version     : {:?}",
            walk_time / marks.len().max(1) as u32
        );
    }

    #[test]
    fn a_version_id_survives_being_written_out_and_read_back() {
        let id = loro::ID {
            peer: 12_345_678_901_234_567_890,
            counter: 42,
        };
        assert_eq!(parse_version_id(&version_id(id)).unwrap(), id);
    }

    #[test]
    fn nonsense_version_ids_are_refused_rather_than_guessed_at() {
        for raw in ["", "no-colon", "abc:1", "1:xyz"] {
            assert!(
                parse_version_id(raw).is_err(),
                "'{raw}' should not parse as a version id"
            );
        }
    }
}
