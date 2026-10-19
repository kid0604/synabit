//! What a sync conflict left behind, described in the note's own terms.
//!
//! Sync reports a conflict as two paths: where the contested file was, and
//! where ours was moved to. That is enough for sync and close to useless for a
//! person. A QuickCap is named after a UUID, so the notice read as a column of
//! hex; and one note that lost its place repeatedly arrived as eight entries,
//! each a longer string of `(conflict …)` than the last. Nothing in it said
//! whether the copies held anything, which is the one question anyone looking
//! at it has.
//!
//! So the notice asks this module instead. Copies are grouped under the file
//! they were split from, named by the note's title, and each is compared with
//! that file: identical, differing only in its properties, or differing in its
//! text. Only the first kind is offered for deletion, and the deletion checks
//! again at the moment it happens, because the dialog may have been open
//! across a sync.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::db::DbState;
use crate::error::AppResult;
use crate::sync::core::node_document;

/// One conflict as sync reported it.
#[derive(Debug, Clone, Deserialize)]
pub struct KeptCopyReport {
    pub kept_as: String,
}

/// How a kept copy compares with the file it was split from.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Difference {
    /// Same text, same properties. Trashing it loses nothing.
    None,
    /// Same text, but a property differs — a tag, a status, a type. Not
    /// offered for deletion: the difference is small and still somebody's.
    Properties,
    /// The text itself differs.
    Text,
    /// The file it was split from is gone, so this copy is the only one.
    OnlyCopy,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeptCopy {
    pub rel_path: String,
    pub difference: Difference,
}

/// Every copy split from one file.
#[derive(Debug, Clone, Serialize)]
pub struct KeptGroup {
    /// The path with every `(conflict …)` taken off.
    pub original: String,
    pub title: String,
    /// The node type, so the dialog can say "QuickCap" and open the right app.
    pub node_type: String,
    pub copies: Vec<KeptCopy>,
}

/// Properties that are expected to differ between two copies of the same
/// writing: each copy has an identity and timestamps of its own.
const OWN_TO_EACH_COPY: [&str; 3] = ["node_id", "created_at", "updated_at"];

/// The path a kept copy was split from.
///
/// Every `(conflict …)` comes off, not just the last: a note that lost its
/// place several times carries one per loss, and all of them were split from
/// the same file.
pub fn original_of(kept_as: &str) -> String {
    static SUFFIX: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r" \(conflict [0-9a-f]+\)").expect("valid pattern"));
    let (dir, name) = match kept_as.rfind('/') {
        Some(i) => (&kept_as[..=i], &kept_as[i + 1..]),
        None => ("", kept_as),
    };
    format!("{dir}{}", SUFFIX.replace_all(name, ""))
}

/// The frontmatter as text, less the lines every copy has of its own.
///
/// The fallback for a frontmatter the YAML reader rejects, where there are no
/// parsed fields to compare. Compared as written, so a reordering counts as a
/// difference — the cautious answer when the alternative is offering a delete.
fn raw_frontmatter(text: &str) -> String {
    let Some(rest) = text.strip_prefix("---\n") else { return String::new() };
    let Some((front, _)) = rest.split_once("\n---\n") else { return String::new() };
    front
        .lines()
        .filter(|line| !OWN_TO_EACH_COPY.iter().any(|k| line.starts_with(&format!("{k}:"))))
        .collect::<Vec<_>>()
        .join("\n")
}

fn shared_fields(fields: &BTreeMap<String, String>) -> BTreeMap<&str, &str> {
    fields
        .iter()
        .filter(|(k, _)| !OWN_TO_EACH_COPY.contains(&k.as_str()))
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

/// Compare two note files.
pub fn compare_notes(original: &str, copy: &str) -> Difference {
    let a = node_document::split(original);
    let b = node_document::split(copy);
    if a.body.trim() != b.body.trim() {
        return Difference::Text;
    }
    // Parsed fields where both parsed; the text as written where either did
    // not, so a frontmatter the reader rejects is never taken for an empty one.
    let same = if !a.fields.is_empty() && !b.fields.is_empty() {
        shared_fields(&a.fields) == shared_fields(&b.fields)
    } else {
        raw_frontmatter(original) == raw_frontmatter(copy)
    };
    if same { Difference::None } else { Difference::Properties }
}

fn is_note(rel_path: &str) -> bool {
    rel_path.ends_with(".md")
}

fn compare_files(vault_path: &str, original: &str, copy: &str) -> AppResult<Difference> {
    let original_path = crate::path_utils::resolve_safe_path(vault_path, original)?;
    let copy_path = crate::path_utils::resolve_safe_path(vault_path, copy)?;
    if !original_path.is_file() {
        return Ok(Difference::OnlyCopy);
    }
    if is_note(copy) {
        let a = std::fs::read_to_string(&original_path)?;
        let b = std::fs::read_to_string(&copy_path)?;
        Ok(compare_notes(&a, &b))
    } else if std::fs::read(&original_path)? == std::fs::read(&copy_path)? {
        Ok(Difference::None)
    } else {
        Ok(Difference::Text)
    }
}

/// A title a person would recognise.
///
/// The frontmatter's when it has one; otherwise the first line of the body,
/// which for a QuickCap is the capture itself. The file name comes last,
/// because for a QuickCap it is a UUID.
pub fn title_of(rel_path: &str, text: Option<&str>) -> String {
    if let Some(text) = text.filter(|_| is_note(rel_path)) {
        let parts = node_document::split(text);
        let from_fields = parts
            .fields
            .get("title")
            .and_then(|v| serde_json::from_str::<String>(v).ok())
            .filter(|t| !t.trim().is_empty());
        if let Some(title) = from_fields {
            return title;
        }
        if let Some(line) = parts
            .body
            .lines()
            .map(|l| l.trim_start_matches('#').trim())
            .find(|l| !l.is_empty())
        {
            return line.chars().take(120).collect();
        }
    }
    let name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    if is_note(name) { name.trim_end_matches(".md").to_string() } else { name.to_string() }
}

fn type_of(rel_path: &str, text: Option<&str>) -> Option<String> {
    if !is_note(rel_path) {
        return Some("file".to_string());
    }
    let text = text?;
    node_document::split(text)
        .fields
        .get("type")
        .and_then(|v| serde_json::from_str::<String>(v).ok())
        .or_else(|| raw_field(text, "type"))
}

/// One top-level value read off its line, for a frontmatter the YAML reader
/// rejects. A title holding `: ` is enough to make it do that, and the copy
/// that still knows it is a QuickCap is exactly the one most likely to have
/// such a title.
fn raw_field(text: &str, key: &str) -> Option<String> {
    let rest = text.strip_prefix("---\n")?;
    let (front, _) = rest.split_once("\n---\n")?;
    let prefix = format!("{key}:");
    front
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|v| !v.is_empty())
}

/// The conflicts still worth showing, grouped and compared.
///
/// A copy no longer on disk has been dealt with — trashed here, renamed, or
/// deleted on another device — and is left out; the dialog drops the notice
/// for it. A group with no copies left is left out with it.
#[tauri::command(async)]
pub fn describe_kept_copies(vault_path: String, conflicts: Vec<KeptCopyReport>) -> AppResult<Vec<KeptGroup>> {
    let mut groups: Vec<KeptGroup> = Vec::new();

    for report in conflicts {
        let copy_path = crate::path_utils::resolve_safe_path(&vault_path, &report.kept_as)?;
        if !copy_path.is_file() {
            continue;
        }
        let original = original_of(&report.kept_as);
        if groups.iter().any(|g| g.copies.iter().any(|c| c.rel_path == report.kept_as)) {
            continue;
        }
        let difference = compare_files(&vault_path, &original, &report.kept_as)?;
        let copy = KeptCopy { rel_path: report.kept_as.clone(), difference };

        if let Some(group) = groups.iter_mut().find(|g| g.original == original) {
            group.copies.push(copy);
            // A bare copy says nothing about what it is; a later one may.
            if group.node_type == "note" {
                let text = std::fs::read_to_string(&copy_path).ok();
                if let Some(found) = type_of(&report.kept_as, text.as_deref()) {
                    group.node_type = found;
                }
            }
            continue;
        }

        // Named from the original when it is there, from the copy when not.
        let read = |rel: &str| -> Option<String> {
            let path = crate::path_utils::resolve_safe_path(&vault_path, rel).ok()?;
            if is_note(rel) { std::fs::read_to_string(path).ok() } else { None }
        };
        let original_text = read(&original);
        let copy_text = read(&report.kept_as);
        let (named_from, text) = match &original_text {
            Some(t) => (original.as_str(), Some(t.as_str())),
            None => (report.kept_as.as_str(), copy_text.as_deref()),
        };
        let node_type = type_of(&original, original_text.as_deref())
            .or_else(|| type_of(&report.kept_as, copy_text.as_deref()))
            .unwrap_or_else(|| "note".to_string());

        groups.push(KeptGroup {
            title: title_of(named_from, text),
            original,
            node_type,
            copies: vec![copy],
        });
    }

    Ok(groups)
}

/// Move to the trash every copy that is still identical to its original.
///
/// Checked again here rather than trusted from the dialog: a sync may have run
/// while it was open, and a copy that has since been edited is not the copy
/// the person agreed to throw away. Returns the copies that are gone now —
/// those trashed, and those already missing — so the notice can forget them.
#[tauri::command]
pub fn trash_identical_kept_copies(
    state: tauri::State<'_, DbState>,
    vault_path: String,
    copies: Vec<String>,
) -> AppResult<Vec<String>> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let mut gone = Vec::new();
    for copy in copies {
        let copy_path = crate::path_utils::resolve_safe_path(&vault_path, &copy)?;
        if !copy_path.is_file() {
            gone.push(copy);
            continue;
        }
        let original = original_of(&copy);
        if compare_files(&vault_path, &original, &copy)? != Difference::None {
            log::info!("kept copy {copy} no longer matches {original}; left in place");
            continue;
        }
        crate::commands::trash::apply_trash(&db, &vault_path, &copy)?;
        gone.push(copy);
    }
    Ok(gone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_conflict_suffix_comes_off_and_the_folder_and_extension_stay() {
        assert_eq!(
            original_of("QuickCaps/d438 (conflict 63a42537) (conflict 63a42537).md"),
            "QuickCaps/d438.md"
        );
        assert_eq!(original_of("plan (conflict abcdef01).md"), "plan.md");
        assert_eq!(
            original_of("Whiteboards/plan (conflict abcdef12).whiteboard.json"),
            "Whiteboards/plan.whiteboard.json"
        );
        assert_eq!(original_of("assets/LICENSE (conflict aaaabbbb)"), "assets/LICENSE");
    }

    #[test]
    fn a_folder_that_happens_to_say_conflict_is_left_alone() {
        assert_eq!(
            original_of("Old (conflict abcdef01)/note (conflict 12345678).md"),
            "Old (conflict abcdef01)/note.md"
        );
    }

    const NOTE: &str = "---\nnode_id: a\ntitle: Plan\ntype: note\ntags: [x]\ncreated_at: \"1\"\n---\nThe body\n";

    #[test]
    fn copies_that_differ_only_in_identity_and_timestamps_are_identical() {
        let copy = "---\nnode_id: b\ntitle: Plan\ntype: note\ntags: [x]\ncreated_at: \"2\"\n---\nThe body\n";
        assert_eq!(compare_notes(NOTE, copy), Difference::None);
    }

    #[test]
    fn a_different_tag_is_a_difference_in_properties() {
        let copy = "---\nnode_id: b\ntitle: Plan\ntype: note\ntags: [y]\n---\nThe body\n";
        assert_eq!(compare_notes(NOTE, copy), Difference::Properties);
    }

    #[test]
    fn different_writing_is_a_difference_in_text() {
        let copy = "---\nnode_id: a\ntitle: Plan\ntype: note\ntags: [x]\n---\nThe body, edited\n";
        assert_eq!(compare_notes(NOTE, copy), Difference::Text);
    }

    /// The case that started this: a full QuickCap set aside, and a bare file
    /// with only an identity left in its place. The text matches, but trashing
    /// the copy would throw away the one file that knows it is a QuickCap.
    #[test]
    fn a_copy_carrying_properties_its_original_lost_is_not_identical() {
        let real = "---\nnode_id: 63a4\ntitle: Ý tưởng: chụp ảnh\ntype: quickcap\ntags: []\n---\nÝ tưởng: chụp ảnh\n";
        let bare = "---\nnode_id: eef6\n---\n\nÝ tưởng: chụp ảnh\n";
        assert_eq!(compare_notes(bare, real), Difference::Properties);
    }

    #[test]
    fn a_quickcap_is_named_by_its_text_not_its_uuid() {
        let bare = "---\nnode_id: eef6\n---\n\nÝ tưởng: chụp ảnh lavabo\n";
        assert_eq!(title_of("QuickCaps/d4389688.md", Some(bare)), "Ý tưởng: chụp ảnh lavabo");
        assert_eq!(title_of("Notes/plan.md", Some(NOTE)), "Plan");
        assert_eq!(title_of("Notes/empty.md", Some("---\nnode_id: a\n---\n")), "empty");
        assert_eq!(title_of("assets/report.pdf", None), "report.pdf");
    }

    #[test]
    fn copies_are_grouped_under_their_original_and_missing_ones_are_dropped() {
        let holder = tempfile::tempdir().expect("tempdir");
        let vault = holder.path();
        std::fs::create_dir_all(vault.join("Notes")).unwrap();
        let copy = NOTE.replace("node_id: a", "node_id: b");
        std::fs::write(vault.join("Notes/plan.md"), NOTE).unwrap();
        std::fs::write(vault.join("Notes/plan (conflict 11111111).md"), &copy).unwrap();
        std::fs::write(
            vault.join("Notes/plan (conflict 11111111) (conflict 11111111).md"),
            NOTE.replace("The body", "Other words"),
        )
        .unwrap();

        let report = |p: &str| KeptCopyReport { kept_as: p.to_string() };
        let groups = describe_kept_copies(
            vault.to_string_lossy().to_string(),
            vec![
                report("Notes/plan (conflict 11111111).md"),
                report("Notes/plan (conflict 11111111) (conflict 11111111).md"),
                report("Notes/gone (conflict 22222222).md"),
            ],
        )
        .expect("describe");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].original, "Notes/plan.md");
        assert_eq!(groups[0].title, "Plan");
        assert_eq!(groups[0].node_type, "note");
        let differences: Vec<_> = groups[0].copies.iter().map(|c| c.difference).collect();
        assert_eq!(differences, vec![Difference::None, Difference::Text]);
    }

    #[test]
    fn a_copy_whose_original_is_gone_is_the_only_copy() {
        let holder = tempfile::tempdir().expect("tempdir");
        let vault = holder.path();
        std::fs::write(vault.join("plan (conflict 11111111).md"), NOTE).unwrap();
        let groups = describe_kept_copies(
            vault.to_string_lossy().to_string(),
            vec![KeptCopyReport { kept_as: "plan (conflict 11111111).md".into() }],
        )
        .expect("describe");
        assert_eq!(groups[0].copies[0].difference, Difference::OnlyCopy);
        assert_eq!(groups[0].title, "Plan");
    }

    /// The vault that prompted this: the original and the first copy are bare,
    /// and only a later copy still says it is a QuickCap.
    #[test]
    fn the_type_comes_from_whichever_copy_still_knows_it() {
        let holder = tempfile::tempdir().expect("tempdir");
        let vault = holder.path();
        std::fs::create_dir_all(vault.join("QuickCaps")).unwrap();
        let bare = |id: &str| format!("---\nnode_id: {id}\n---\n\nÝ tưởng: chụp ảnh lavabo\n");
        let real = "---\nnode_id: 63a4\ntitle: Ý tưởng: chụp ảnh lavabo\ntype: quickcap\ntags: []\n---\nÝ tưởng: chụp ảnh lavabo\n";
        std::fs::write(vault.join("QuickCaps/d438.md"), bare("eef6")).unwrap();
        std::fs::write(vault.join("QuickCaps/d438 (conflict 63a42537).md"), bare("8292")).unwrap();
        std::fs::write(vault.join("QuickCaps/d438 (conflict 63a42537) (conflict 63a42537).md"), real).unwrap();

        let groups = describe_kept_copies(
            vault.to_string_lossy().to_string(),
            vec![
                KeptCopyReport { kept_as: "QuickCaps/d438 (conflict 63a42537).md".into() },
                KeptCopyReport { kept_as: "QuickCaps/d438 (conflict 63a42537) (conflict 63a42537).md".into() },
            ],
        )
        .expect("describe");

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Ý tưởng: chụp ảnh lavabo");
        assert_eq!(groups[0].node_type, "quickcap");
        let differences: Vec<_> = groups[0].copies.iter().map(|c| c.difference).collect();
        // The bare copy can go; the one that still knows it is a QuickCap cannot.
        assert_eq!(differences, vec![Difference::None, Difference::Properties]);
    }
}
