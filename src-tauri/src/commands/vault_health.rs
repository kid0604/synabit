//! Finding notes that exist twice.
//!
//! Renaming a note used to write it back to the path it had just been moved
//! off — `useNoteRename` sent the save to `note.id`, which nobody had moved on
//! to the new path. The rename produced the new file and the save recreated
//! the old one, so the vault ended up holding one note under two names, both
//! carrying the same `node_id` in their frontmatter.
//!
//! That is fixed. It does not undo the copies already made, and nobody can be
//! expected to find them by eye in a vault of any size.
//!
//! # What this does and does not do
//!
//! It reports. Deleting is left to the person reading the report, through the
//! same trash every other delete goes through, because "these two files are
//! the same note" is a claim about *identity* and the judgement about which
//! copy to keep belongs to whoever wrote them.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use walkdir::WalkDir;

use crate::error::AppResult;

/// One file in a group of files claiming the same identity.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DuplicateFile {
    pub rel_path: String,
    pub title: String,
    /// Unix milliseconds, so the reader can see which copy is the live one.
    pub modified_at: i64,
    pub bytes: u64,
    /// Whether this copy's body matches the newest one in its group.
    ///
    /// Where it does, the older copy holds nothing the newer one does not, and
    /// trashing it loses no writing. Where it does not, the two have been
    /// edited apart since the split and somebody has to read them.
    pub same_body: bool,
}

/// Files that all claim to be the same note.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DuplicateGroup {
    /// The identity they share.
    pub node_id: String,
    /// Newest first, so the first is the copy to keep.
    pub files: Vec<DuplicateFile>,
}

/// One note file as read off the disk.
pub struct ScannedNote {
    pub rel_path: String,
    pub node_id: String,
    pub title: String,
    pub body: String,
    pub modified_at: i64,
    pub bytes: u64,
}

/// Group scanned notes by the identity they carry.
///
/// Split from the walk so the grouping — which is where the judgement lives —
/// can be tested without a vault on disk.
pub fn group_by_identity(notes: Vec<ScannedNote>) -> Vec<DuplicateGroup> {
    let mut by_id: HashMap<String, Vec<ScannedNote>> = HashMap::new();
    for note in notes {
        // A file with no identity in it cannot be shown to be a copy of
        // anything. Two untitled notes are not the same note.
        if note.node_id.is_empty() {
            continue;
        }
        by_id.entry(note.node_id.clone()).or_default().push(note);
    }

    let mut groups: Vec<DuplicateGroup> = by_id
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(node_id, mut files)| {
            // Newest first. Ties broken by path so the order is stable between
            // runs — a report that reshuffles itself is hard to act on.
            files.sort_by(|a, b| {
                b.modified_at
                    .cmp(&a.modified_at)
                    .then_with(|| a.rel_path.cmp(&b.rel_path))
            });
            let newest_body = files[0].body.clone();
            DuplicateGroup {
                node_id,
                files: files
                    .into_iter()
                    .map(|f| DuplicateFile {
                        same_body: f.body == newest_body,
                        rel_path: f.rel_path,
                        title: f.title,
                        modified_at: f.modified_at,
                        bytes: f.bytes,
                    })
                    .collect(),
            }
        })
        .collect();

    // Most files first: the worst splits are the ones worth looking at first.
    groups.sort_by(|a, b| {
        b.files
            .len()
            .cmp(&a.files.len())
            .then_with(|| a.node_id.cmp(&b.node_id))
    });
    groups
}

/// Every note in the vault that shares its identity with another.
#[tauri::command]
pub fn find_duplicate_notes(vault_path: String) -> AppResult<Vec<DuplicateGroup>> {
    let base = Path::new(&vault_path);
    if !base.exists() {
        return Ok(Vec::new());
    }

    let mut scanned = Vec::new();
    for entry in WalkDir::new(base).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel_path = crate::path_utils::to_relative(path, &vault_path);
        // The same places the vault scan declines to look. A note in `.trash/`
        // is not a duplicate of the one it was deleted from.
        if crate::commands::nodes::is_in_unscanned_dir(&rel_path) {
            continue;
        }

        let Some(node) = crate::utils::node_parser::parse_file_to_node(&vault_path, path) else {
            continue;
        };
        let node_id = node
            .properties
            .get("node_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let meta = entry.metadata().ok();
        let modified_at = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        scanned.push(ScannedNote {
            rel_path,
            node_id,
            title: node.title,
            body: node.content,
            modified_at,
            bytes: meta.map(|m| m.len()).unwrap_or(0),
        });
    }

    Ok(group_by_identity(scanned))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(rel_path: &str, node_id: &str, body: &str, modified_at: i64) -> ScannedNote {
        ScannedNote {
            rel_path: rel_path.to_string(),
            node_id: node_id.to_string(),
            title: rel_path.to_string(),
            body: body.to_string(),
            modified_at,
            bytes: body.len() as u64,
        }
    }

    fn write(dir: &Path, rel: &str, node_id: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("---\ntitle: A note\ntype: note\nnode_id: {node_id}\n---\n{body}\n"),
        )
        .unwrap();
    }

    /// The walk itself, over a real directory — the part the pure tests above
    /// cannot reach.
    #[test]
    fn a_real_vault_is_walked_and_the_places_notes_do_not_live_are_skipped() {
        let holder = tempfile::tempdir().expect("tempdir");
        let vault = holder.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        write(&vault, "Notes/new name.md", "id-1", "the body");
        write(&vault, "Notes/old name.md", "id-1", "the body");
        write(&vault, "Notes/unrelated.md", "id-2", "something else");

        // A deleted copy is not a duplicate of the note it was deleted from,
        // and neither is anything under the directories the vault scan skips.
        write(&vault, ".trash/Notes/old name.md", "id-1", "the body");
        write(&vault, "Syn/scratch.md", "id-1", "the body");

        let groups = find_duplicate_notes(vault.to_string_lossy().to_string()).expect("scan");

        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(groups[0].node_id, "id-1");
        assert_eq!(groups[0].files.len(), 2, "{:?}", groups[0].files);
        assert!(groups[0]
            .files
            .iter()
            .all(|f| f.rel_path.starts_with("Notes/")));
        assert!(groups[0].files.iter().all(|f| f.same_body));
    }

    #[test]
    fn a_note_that_exists_once_is_not_reported() {
        let groups = group_by_identity(vec![
            note("Notes/a.md", "id-1", "body", 10),
            note("Notes/b.md", "id-2", "other", 20),
        ]);

        assert!(groups.is_empty(), "{groups:?}");
    }

    /// The shape the rename bug left behind: one note, two files, one identity.
    #[test]
    fn two_files_carrying_one_identity_are_reported_newest_first() {
        let groups = group_by_identity(vec![
            note("Notes/old name.md", "id-1", "the same body", 100),
            note("Notes/new name.md", "id-1", "the same body", 200),
        ]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].node_id, "id-1");
        assert_eq!(
            groups[0]
                .files
                .iter()
                .map(|f| f.rel_path.as_str())
                .collect::<Vec<_>>(),
            vec!["Notes/new name.md", "Notes/old name.md"],
        );
    }

    /// The distinction that decides whether trashing a copy is safe.
    #[test]
    fn a_copy_is_marked_by_whether_it_holds_anything_the_newest_does_not() {
        let groups = group_by_identity(vec![
            note("Notes/new.md", "id-1", "shared body", 200),
            note("Notes/stale.md", "id-1", "shared body", 100),
            note(
                "Notes/diverged.md",
                "id-1",
                "edited apart since the split",
                150,
            ),
        ]);

        let by_path: std::collections::HashMap<_, _> = groups[0]
            .files
            .iter()
            .map(|f| (f.rel_path.as_str(), f.same_body))
            .collect();

        assert!(by_path["Notes/new.md"]);
        assert!(
            by_path["Notes/stale.md"],
            "identical, so nothing is lost by trashing it"
        );
        assert!(
            !by_path["Notes/diverged.md"],
            "this one holds writing the newest copy does not, and needs reading"
        );
    }

    /// Files with no identity in them cannot be shown to be copies of anything.
    #[test]
    fn notes_without_an_identity_are_left_out_rather_than_lumped_together() {
        let groups = group_by_identity(vec![
            note("Notes/a.md", "", "unrelated", 10),
            note("Notes/b.md", "", "also unrelated", 20),
        ]);

        assert!(groups.is_empty(), "{groups:?}");
    }

    #[test]
    fn the_worst_split_is_listed_first() {
        let groups = group_by_identity(vec![
            note("Notes/a1.md", "id-a", "x", 10),
            note("Notes/a2.md", "id-a", "x", 20),
            note("Notes/b1.md", "id-b", "y", 10),
            note("Notes/b2.md", "id-b", "y", 20),
            note("Notes/b3.md", "id-b", "y", 30),
        ]);

        assert_eq!(groups[0].node_id, "id-b");
        assert_eq!(groups[0].files.len(), 3);
        assert_eq!(groups[1].node_id, "id-a");
    }

    /// The report must read the same way twice, or acting on it means
    /// re-reading it each time.
    #[test]
    fn files_modified_at_the_same_moment_keep_a_stable_order() {
        let run = || {
            group_by_identity(vec![
                note("Notes/z.md", "id-1", "x", 100),
                note("Notes/a.md", "id-1", "x", 100),
            ])
        };

        assert_eq!(run(), run());
        assert_eq!(run()[0].files[0].rel_path, "Notes/a.md");
    }
}
