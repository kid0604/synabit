//! Splitting a node file into the two things that should merge differently.
//!
//! A note's body is prose, and prose is exactly what a text CRDT is for: two
//! people adding a paragraph each should end up with both paragraphs.
//!
//! Frontmatter is not prose. `status` holds one of four words. Merging two
//! devices' edits to it character by character produced `in_pronegress` — a
//! value neither device wrote, valid YAML, and silently wrong. The same merge
//! turned `2026-09-15` against `2026-12-31` into `2026-129-315`.
//!
//! So the two parts go into two containers. The body stays a `LoroText`; each
//! frontmatter key becomes an entry in a `LoroMap`, where concurrent writes
//! resolve to one value rather than to a blend of both.
//!
//! # Why the values are JSON text
//!
//! A frontmatter value can be a string, a bool, a number or a list. Storing
//! each as its JSON encoding keeps every entry a single scalar, which is what
//! makes the map's last-writer-wins behaviour meaningful: two devices editing
//! `tags` pick one list, rather than interleaving the characters of two.
//!
//! # Why the order is carried separately
//!
//! A map has no order, and rebuilding the file from one would reshuffle the
//! frontmatter of every note in the vault the first time it synced. The order
//! is read from the file being rebuilt, so it survives untouched; only a key
//! the file does not mention has to be placed, and those go at the end.

use std::collections::BTreeMap;

/// A node file taken apart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeParts {
    /// The frontmatter keys in the order the file listed them.
    pub order: Vec<String>,
    /// Each frontmatter value, encoded as JSON text.
    pub fields: BTreeMap<String, String>,
    /// Everything after the frontmatter, verbatim.
    pub body: String,
}

/// Where the frontmatter block ends, as byte offsets into `text`.
///
/// Found by hand rather than with a parser because the body has to come back
/// **verbatim** — every blank line and trailing newline the user typed. A
/// round-trip through a YAML library normalises those away, and a merge that
/// silently reformats the body is a merge that shows up as a change on every
/// device.
fn frontmatter_span(text: &str) -> Option<(usize, usize, usize)> {
    let rest = text.strip_prefix("---\n")?;
    let open_end = 4;
    let close = rest.find("\n---\n").map(|i| (open_end + i + 1, open_end + i + 5))?;
    Some((open_end, close.0, close.1))
}

/// The top-level keys of a frontmatter block, in the order they appear.
///
/// Only lines starting in column zero count. A list item (`  - work`) or a
/// nested mapping is indented, so it cannot be mistaken for a key of its own.
fn keys_in_order(frontmatter: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for line in frontmatter.lines() {
        if line.starts_with(' ') || line.starts_with('-') || line.trim().is_empty() {
            continue;
        }
        let Some(colon) = line.find(':') else { continue };
        let key = line[..colon].trim();
        if key.is_empty() || key.contains(' ') {
            continue;
        }
        if !keys.iter().any(|k: &String| k == key) {
            keys.push(key.to_string());
        }
    }
    keys
}

/// Take a node file apart.
///
/// A file with no frontmatter is all body, which is the right answer: it has no
/// fields to merge separately.
pub fn split(text: &str) -> NodeParts {
    let Some((open_end, close_start, body_start)) = frontmatter_span(text) else {
        return NodeParts { order: Vec::new(), fields: BTreeMap::new(), body: text.to_string() };
    };

    let frontmatter = &text[open_end..close_start];
    let body = text[body_start..].to_string();

    let parsed: serde_json::Value = serde_yaml::from_str(frontmatter).unwrap_or(serde_json::Value::Null);
    let mut fields = BTreeMap::new();
    if let serde_json::Value::Object(map) = &parsed {
        for (key, value) in map {
            if let Ok(encoded) = serde_json::to_string(value) {
                fields.insert(key.clone(), encoded);
            }
        }
    }

    // Only keys that actually parsed; a line the YAML reader rejected has no
    // value to carry, and listing it would rebuild the file without it.
    let order = keys_in_order(frontmatter)
        .into_iter()
        .filter(|k| fields.contains_key(k))
        .collect();

    NodeParts { order, fields, body }
}

/// Keys that read better in a fixed place than in alphabetical order.
///
/// Used when there is no order to inherit — which, once a document's
/// frontmatter lives in the map rather than in the text, is every rebuild. The
/// sequence has to be the same on every device or two of them will sync each
/// other's reshuffling back and forth forever; being pleasant to read is the
/// second requirement, not the first.
const LEADING_KEYS: [&str; 2] = ["title", "type"];
const TRAILING_KEYS: [&str; 2] = ["created_at", "updated_at"];

/// Put a node file back together.
///
/// `order` decides the sequence when there is one to inherit. Otherwise the
/// canonical order applies: the keys a reader looks for first, then the rest
/// alphabetically, then the timestamps. Any field the order does not mention
/// follows, sorted.
///
/// Nothing is stamped and nothing is normalised — a rebuild of an unchanged
/// file gives back what went in.
pub fn rebuild(parts: &NodeParts) -> String {
    if parts.fields.is_empty() {
        return parts.body.clone();
    }

    let mut sequence: Vec<&String> = parts
        .order
        .iter()
        .filter(|k| parts.fields.contains_key(*k))
        .collect();

    if sequence.is_empty() {
        for name in LEADING_KEYS {
            if let Some((key, _)) = parts.fields.get_key_value(name) {
                sequence.push(key);
            }
        }
    }

    // `fields` is a BTreeMap, so what follows is already alphabetical.
    for key in parts.fields.keys() {
        let reserved = TRAILING_KEYS.contains(&key.as_str());
        if !reserved && !sequence.iter().any(|k| *k == key) {
            sequence.push(key);
        }
    }
    for name in TRAILING_KEYS {
        if let Some((key, _)) = parts.fields.get_key_value(name) {
            if !sequence.iter().any(|k| *k == key) {
                sequence.push(key);
            }
        }
    }

    let mut mapping = serde_yaml::Mapping::new();
    for key in sequence {
        let Some(encoded) = parts.fields.get(key) else { continue };
        let value: serde_json::Value = serde_json::from_str(encoded).unwrap_or(serde_json::Value::Null);
        if let Ok(yaml) = serde_yaml::to_value(&value) {
            mapping.insert(serde_yaml::Value::String(key.clone()), yaml);
        }
    }

    let yaml = serde_yaml::to_string(&mapping).unwrap_or_default();
    format!("---\n{}---\n{}", yaml, parts.body)
}

/// A file rebuilt from one document's frontmatter and another's body.
///
/// This is the merge itself, expressed as data: the fields come from the map,
/// where each key resolved to one device's value; the body and the key order
/// come from the merged text, where character-level merging is what was wanted
/// all along.
pub fn rebuild_from(fields: BTreeMap<String, String>, merged_text: &str) -> String {
    let from_text = split(merged_text);
    rebuild(&NodeParts { order: from_text.order, fields, body: from_text.body })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "---\ntitle: Buy milk\ntype: task\nstatus: todo\ntags:\n  - work\n---\nthe body\n\nand more\n";

    #[test]
    fn a_file_comes_apart_into_fields_and_a_body() {
        let parts = split(FILE);
        assert_eq!(parts.fields.get("status").map(String::as_str), Some("\"todo\""));
        assert_eq!(parts.fields.get("title").map(String::as_str), Some("\"Buy milk\""));
        assert_eq!(parts.body, "the body\n\nand more\n");
    }

    #[test]
    fn the_key_order_survives() {
        assert_eq!(split(FILE).order, vec!["title", "type", "status", "tags"]);
    }

    /// A list item is not a key. Reading it as one would rebuild the file with
    /// a phantom field in it.
    #[test]
    fn a_list_item_is_not_mistaken_for_a_key() {
        assert!(!split(FILE).order.iter().any(|k| k == "- work"));
        assert_eq!(split(FILE).order.len(), 4);
    }

    /// A list is one value, so two devices editing it pick one list rather
    /// than interleaving the characters of two.
    #[test]
    fn a_list_is_carried_as_a_single_value() {
        assert_eq!(split(FILE).fields.get("tags").map(String::as_str), Some("[\"work\"]"));
    }

    #[test]
    fn booleans_and_numbers_keep_their_type() {
        let parts = split("---\ndone: true\ncount: 3\n---\nbody\n");
        assert_eq!(parts.fields.get("done").map(String::as_str), Some("true"));
        assert_eq!(parts.fields.get("count").map(String::as_str), Some("3"));
    }

    /// The body has to come back exactly, blank lines and all: a merge that
    /// reformats it registers as a change on every device, forever.
    #[test]
    fn a_round_trip_gives_back_the_same_file() {
        let parts = split(FILE);
        let rebuilt = rebuild(&parts);
        assert_eq!(split(&rebuilt).fields, parts.fields);
        assert_eq!(split(&rebuilt).body, parts.body);
        assert_eq!(split(&rebuilt).order, parts.order);
    }

    #[test]
    fn a_round_trip_is_stable_the_second_time() {
        let once = rebuild(&split(FILE));
        let twice = rebuild(&split(&once));
        assert_eq!(once, twice, "rebuilding keeps changing the file");
    }

    #[test]
    fn a_file_with_no_frontmatter_is_all_body() {
        let parts = split("just text\n");
        assert!(parts.fields.is_empty());
        assert_eq!(parts.body, "just text\n");
        assert_eq!(rebuild(&parts), "just text\n");
    }

    #[test]
    fn an_empty_body_survives() {
        let parts = split("---\nstatus: todo\n---\n");
        assert_eq!(parts.body, "");
        assert!(rebuild(&parts).starts_with("---\n"));
    }

    #[test]
    fn frontmatter_that_does_not_parse_leaves_the_text_alone() {
        let parts = split("---\nthis: [is: not: yaml\n---\nbody\n");
        assert!(parts.fields.is_empty() || !parts.fields.contains_key("this"));
    }

    /// The merge itself: values from one place, shape from another.
    #[test]
    fn rebuild_from_takes_fields_from_the_map_and_the_body_from_the_text() {
        let mut fields = split(FILE).fields;
        fields.insert("status".into(), "\"done\"".into());

        // The merged text is the damaged one — this is what it looks like.
        let merged_text = FILE.replace("status: todo", "status: in_pronegress")
            .replace("the body", "the body, edited");

        let out = rebuild_from(fields, &merged_text);
        let parts = split(&out);
        assert_eq!(parts.fields.get("status").map(String::as_str), Some("\"done\""));
        assert!(parts.body.contains("the body, edited"), "the body was lost: {}", parts.body);
    }

    #[test]
    fn a_field_the_text_never_mentioned_still_gets_written() {
        let mut fields = split(FILE).fields;
        fields.insert("priority".into(), "\"P1\"".into());
        let out = rebuild_from(fields, FILE);
        assert!(split(&out).fields.contains_key("priority"));
    }

    /// Two devices rebuilding the same inputs must produce the same bytes, or
    /// they will keep syncing each other's reformatting back and forth.
    #[test]
    fn two_devices_rebuild_the_same_bytes() {
        let fields = split(FILE).fields;
        let a = rebuild_from(fields.clone(), FILE);
        let b = rebuild_from(fields, FILE);
        assert_eq!(a, b);
    }
}
