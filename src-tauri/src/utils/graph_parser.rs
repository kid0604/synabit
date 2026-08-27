use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// Compiling a regex costs far more than running one, and these run constantly:
// `extract_edges` is called once for a node's body and again for every string
// in its properties, so a note with two tags used to compile nine regexes. At
// vault scale that was the single largest cost in the scan — measured at
// 8.2ms per file, against 0.02ms to actually write the node's row.
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)(?:^|\s)#([a-zA-Z0-9_\-/]+)").unwrap());
static WIKI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
/// Attachments a note embeds, written as a vault-relative asset path.
///
/// The editor writes pictures as `![](assets/name.png)` and video and audio as
/// `<video src="assets/name.mp4">` — see
/// `note/editor/composables/useAssetPaths.ts`. Neither shape was ever picked up
/// here, which is why "which notes use this file?" had to be answered by
/// scanning every node's body for the filename as a substring: slow, and wrong
/// often enough to matter, since a file called `note.pdf` matched every note
/// containing the word "note".
static ASSET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:\]\(|src\s*=\s*["'])(assets/[^)"'<>]+)"#).unwrap()
});
static MD_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\[([^\]]*)\]\(synabit://(?:note|node|person|task|quickcap|event|project|file)/([^)]+)\)",
    )
    .unwrap()
});
static RENAME_MD_LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\[([^\]]*)\]\((synabit://(?:note|node|person|task|quickcap|event|project)/)([^)]+)\)",
    )
    .unwrap()
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source_id: String,
    pub target_title_or_path: String,
    pub link_type: String, // 'wikilink', 'internal_link', 'tag', 'person_link'
    /// What the link means, when the link itself says.
    ///
    /// A mention in a note carries no relationship — it is a mention. A person
    /// linked to another person does: friend, mentor, the person who
    /// introduced them. That word is the whole content of the link, and
    /// nowhere else to put it means keeping a second copy of the graph in
    /// somebody's frontmatter.
    #[serde(default)]
    pub relation: Option<String>,
}

/// Extracts all tags and internal links from raw markdown text
pub fn extract_edges(source_id: &str, text: &str) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. Tags (#tag)
    for cap in TAG_RE.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let tag_name = format!("#{}", m.as_str().to_lowercase());
            if seen.insert(tag_name.clone()) {
                edges.push(GraphEdge {
                    source_id: source_id.to_string(),
                    target_title_or_path: tag_name,
                    link_type: "tag".to_string(),
                    relation: None,
                });
            }
        }
    }

    // 2. WikiLinks ([[Link]] or [[Link|Alias]])
    for cap in WIKI_RE.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let inner = m.as_str().trim();
            let target_title = inner
                .split('|')
                .next()
                .unwrap_or(inner)
                .trim()
                .to_lowercase();
            if seen.insert(target_title.clone()) {
                edges.push(GraphEdge {
                    source_id: source_id.to_string(),
                    target_title_or_path: target_title,
                    link_type: "wikilink".to_string(),
                    relation: None,
                });
            }
        }
    }

    // 3. Embedded attachments (![](assets/name.png), <img src="assets/…">)
    for cap in ASSET_RE.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let raw = m.as_str().trim();
            let path = urlencoding::decode(raw)
                .unwrap_or(std::borrow::Cow::Borrowed(raw))
                .to_string()
                .to_lowercase();
            if seen.insert(path.clone()) {
                edges.push(GraphEdge {
                    source_id: source_id.to_string(),
                    target_title_or_path: path,
                    link_type: "attachment".to_string(),
                    relation: Some("attachment".to_string()),
                });
            }
        }
    }

    // 4. Tiptap Internal Links ([Title](synabit://.../path))
    for cap in MD_LINK_RE.captures_iter(text) {
        if let Some(m) = cap.get(2) {
            let encoded_path = m.as_str().trim();
            let path = urlencoding::decode(encoded_path)
                .unwrap_or(std::borrow::Cow::Borrowed(encoded_path))
                .to_string();
            // Path is usually relative or absolute. We just store it as the target_title_or_path
            if seen.insert(path.clone()) {
                edges.push(GraphEdge {
                    source_id: source_id.to_string(),
                    target_title_or_path: path,
                    link_type: "internal_link".to_string(),
                    relation: None,
                });
            }
        }
    }

    edges
}

/// Extracts edges from both content and properties of a Node
pub fn extract_node_edges(node: &crate::models::node::NodeMetadata) -> Vec<GraphEdge> {
    let mut edges = extract_edges(&node.id, &node.content);

    // Also parse properties
    if let serde_json::Value::Object(map) = &node.properties {
        for (key, val) in map {
            if let Some(s) = val.as_str() {
                let prop_edges = extract_edges(&node.id, s);
                for mut e in prop_edges {
                    if e.link_type == "wikilink" {
                        e.link_type = key.clone();
                    }
                    edges.push(e);
                }
            } else if let serde_json::Value::Array(arr) = val {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        let prop_edges = extract_edges(&node.id, s);
                        for mut e in prop_edges {
                            if e.link_type == "wikilink" {
                                e.link_type = key.clone();
                            }
                            edges.push(e);
                        }
                    }
                }
            }
        }
    }

    edges.extend(person_links(node));

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    edges
        .into_iter()
        .filter(|e| seen.insert(format!("{}-{}", e.target_title_or_path, e.link_type)))
        .collect()
}

/// The people this person is linked to, from their own `connections`.
///
/// Person-to-person links used to be kept twice: once as this list, and once
/// as an array of markdown mentions written into the frontmatter beside it
/// purely so that the edge index would notice them. The two drifted — the
/// mentions named a file path, so moving somebody broke them, and each one
/// carried a copy of the name it was written with, so renaming somebody left
/// the old name showing in everybody else's graph.
///
/// One list now, and the edge index reads it directly. The relationship comes
/// with the edge rather than being kept in a parallel copy.
fn person_links(node: &crate::models::node::NodeMetadata) -> Vec<GraphEdge> {
    let source = node.id.clone();

    // A node that is *about* one person says so with a single `person_id`.
    // An interaction is the reason this exists: one recorded coffee belongs
    // to one person, and saying it this way means the link is an edge like
    // any other rather than an entry in an array inside somebody's file.
    let mut edges: Vec<GraphEdge> = node
        .properties
        .get("person_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|target| GraphEdge {
            source_id: source.clone(),
            target_title_or_path: target.to_string(),
            link_type: "about".to_string(),
            relation: None,
        })
        .into_iter()
        .collect();

    edges.extend(person_connection_links(node, &source));
    edges
}

fn person_connection_links(
    node: &crate::models::node::NodeMetadata,
    source: &str,
) -> Vec<GraphEdge> {
    node.properties
        .get("connections")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|connection| {
            let target = connection.get("person_id")?.as_str()?.trim();
            if target.is_empty() {
                return None;
            }
            let relation = connection
                .get("relation_type")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(str::to_string);
            Some(GraphEdge {
                source_id: source.to_string(),
                target_title_or_path: target.to_string(),
                link_type: "person_link".to_string(),
                relation,
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════
//  NEW: Resolved Node Edges (ID-based)
// ═══════════════════════════════════════════════════════════

use crate::db::NodeEdge;

/// Pre-built lookup maps for fast title → identity resolution.
///
/// The keys are the ways a link can name its target — a title, a path, a bare
/// filename. The values are all stable ids, so an edge records *which node* was
/// linked to rather than where that node's file happened to be sitting.
pub struct NodeResolver {
    /// title (lowercase, no .md) → stable id
    title_map: std::collections::HashMap<String, String>,
    /// path (lowercase) → stable id  (for internal_link paths)
    path_map: std::collections::HashMap<String, String>,
    /// node id (a path) → stable id, for links that name the path exactly
    id_map: std::collections::HashMap<String, String>,
    /// filename (lowercase) → stable id  (for file embeds like "image.png")
    filename_map: std::collections::HashMap<String, String>,
    /// Every identity in the vault, for links that already name one.
    stable_ids: std::collections::HashSet<String>,
}

impl NodeResolver {
    /// Build resolver from all nodes — O(N) once, then O(1) per resolve
    pub fn new(all_nodes: &[crate::models::node::NodeMetadata]) -> Self {
        let mut title_map = std::collections::HashMap::new();
        let mut path_map = std::collections::HashMap::new();
        let mut id_map = std::collections::HashMap::new();
        let mut filename_map = std::collections::HashMap::new();
        let mut stable_ids = std::collections::HashSet::new();

        for node in all_nodes {
            // Every lookup answers with the node's stable identity, whatever
            // the caller used to ask for it.
            let id = node.stable_id().to_string();
            stable_ids.insert(id.clone());
            id_map.insert(node.id.clone(), id.clone());

            // Title lookup (lowercase, strip .md)
            let title_lower = node.title.to_lowercase().replace(".md", "");
            title_map.entry(title_lower).or_insert_with(|| id.clone());

            // Path lookup (the node id IS a path for file-based nodes)
            path_map
                .entry(node.id.to_lowercase())
                .or_insert_with(|| id.clone());

            // For file nodes: map filename from properties.path
            if node.node_type == "file" {
                if let Some(p) = node.properties.get("path").and_then(|v| v.as_str()) {
                    let file_path = std::path::Path::new(p);
                    if let Some(fname) = file_path.file_name().and_then(|s| s.to_str()) {
                        let fname_lower = fname.to_lowercase();
                        filename_map
                            .entry(fname_lower.clone())
                            .or_insert_with(|| id.clone());

                        // The form a note actually writes.
                        //
                        // The editor embeds attachments as `assets/<name>`, and
                        // that string is what arrives here to be resolved.
                        // Registering it directly makes the lookup exact rather
                        // than a hopeful fall through to matching on titles,
                        // where a note called `báo-cáo.pdf` would answer for the
                        // picture of the same name.
                        //
                        // No vault root is needed to spot one: an asset is an
                        // asset by virtue of the folder it sits in.
                        if file_path
                            .parent()
                            .and_then(|dir| dir.file_name())
                            .is_some_and(|dir| dir.eq_ignore_ascii_case("assets"))
                        {
                            path_map
                                .entry(format!("assets/{fname_lower}"))
                                .or_insert_with(|| id.clone());
                        }
                    }
                    // Also full path for exact matches
                    path_map
                        .entry(p.to_lowercase())
                        .or_insert_with(|| id.clone());
                }
            }

            // Filename from the ID path (for note/task nodes like "Notes/Meeting.md")
            let id_path = std::path::Path::new(&node.id);
            if let Some(fname) = id_path.file_name().and_then(|s| s.to_str()) {
                let fname_lower = fname.to_lowercase();
                filename_map
                    .entry(fname_lower.clone())
                    .or_insert_with(|| id.clone());
                // Also without .md
                let no_ext = fname_lower.replace(".md", "");
                filename_map.entry(no_ext).or_insert_with(|| id.clone());
            }
        }

        NodeResolver {
            title_map,
            path_map,
            id_map,
            filename_map,
            stable_ids,
        }
    }

    /// Resolve a target string to a stable node identity, or ghost:<target>.
    pub fn resolve(&self, target: &str, _link_type: &str) -> String {
        let lower = target.to_lowercase();
        let no_md = lower.replace(".md", "");

        // 0. Already an identity. A person link written since these became
        //    stable names one directly, and looking it up as a path or a
        //    title would find nothing and call a live person a ghost.
        if self.stable_ids.contains(target) {
            return target.to_string();
        }

        // 1. Direct match on a node's current path
        if let Some(id) = self.id_map.get(target) {
            return id.clone();
        }

        // 2. Title match
        if let Some(id) = self.title_map.get(&no_md) {
            return id.clone();
        }

        // 3. Path match
        if let Some(id) = self.path_map.get(&lower) {
            return id.clone();
        }

        // 4. Filename match (e.g. "Notes/hello.md" → try "hello")
        if let Some(id) = self.filename_map.get(&no_md) {
            return id.clone();
        }
        if let Some(id) = self.filename_map.get(&lower) {
            return id.clone();
        }

        // 5. Try extracting filename from path-like targets
        let target_path = std::path::Path::new(target);
        if let Some(fname) = target_path.file_name().and_then(|s| s.to_str()) {
            let fname_lower = fname.to_lowercase().replace(".md", "");
            if let Some(id) = self.title_map.get(&fname_lower) {
                return id.clone();
            }
            if let Some(id) = self.filename_map.get(&fname_lower) {
                return id.clone();
            }
        }

        // 6. Ghost node
        format!("ghost:{}", lower)
    }
}

/// Extract resolved edges from a node using pre-built resolver.
/// Returns `Vec<NodeEdge>` with target_id as actual node IDs (or ghost:<title>).
/// Tags are SKIPPED (they live in node properties, not edges).
pub fn extract_resolved_node_edges(
    node: &crate::models::node::NodeMetadata,
    resolver: &NodeResolver,
) -> Vec<NodeEdge> {
    let raw_edges = extract_node_edges(node);
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    // Both ends of an edge are stable ids, so the link survives either node's
    // file being moved.
    let source = node.stable_id().to_string();

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for raw in raw_edges {
        // Skip tags — they're stored in node properties
        if raw.link_type == "tag" {
            continue;
        }

        let target_id = resolver.resolve(&raw.target_title_or_path, &raw.link_type);

        // Skip self-links
        if target_id == source {
            continue;
        }

        // Map old link_type → new edge_type
        let edge_type = match raw.link_type.as_str() {
            "wikilink" => "wikilink",
            "internal_link" => "internal_link",
            "person_link" => "person_link",
            "about" => "about",
            // Kept distinct so "which notes show this picture?" is a different
            // question from "which notes mention it".
            "attachment" => "attachment",
            _ => "internal_link", // property-level links (key names like "assignee")
        };

        // A person link says what it means; everything else is a mention, and
        // a mention means only that somebody was named.
        let relation: Option<String> = raw.relation.clone();

        let dedup_key = format!("{}-{}-{}", source, target_id, edge_type);
        if !seen.insert(dedup_key) {
            continue;
        }

        result.push(NodeEdge {
            id: uuid::Uuid::new_v4().to_string(),
            source_id: source.clone(),
            target_id,
            edge_type: edge_type.to_string(),
            relation,
            created_at: now.clone(),
        });
    }

    result
}

/// Replaces WikiLinks targeting `old_name` with `new_name`.
/// Retains existing aliases if present.
pub fn rename_links_in_text(
    text: &str,
    old_title: &str,
    new_title: &str,
    target_id: Option<&str>,
) -> String {
    let old_lower = old_title.to_lowercase();

    let text_with_wiki_links = WIKI_RE
        .replace_all(text, |caps: &regex::Captures| {
            let inner = caps.get(1).unwrap().as_str();
            let mut parts = inner.splitn(2, '|');
            let title = parts.next().unwrap_or("").trim();
            let alias = parts.next().map(|s| s.trim());

            if title.to_lowercase() == old_lower {
                if let Some(a) = alias {
                    format!("[[{}|{}]]", new_title, a)
                } else {
                    format!("[[{}]]", new_title)
                }
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        })
        .to_string();

    // 2. Replace Tiptap internal links
    let text_with_md_links =
        RENAME_MD_LINK_RE.replace_all(&text_with_wiki_links, |caps: &regex::Captures| {
            let label = caps.get(1).unwrap().as_str();
            let prefix = caps.get(2).unwrap().as_str();
            let encoded_path = caps.get(3).unwrap().as_str();
            let decoded_path = urlencoding::decode(encoded_path)
                .unwrap_or(std::borrow::Cow::Borrowed(encoded_path))
                .to_string();

            let file_stem = std::path::Path::new(&decoded_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&decoded_path);

            let is_match = if let Some(id) = target_id {
                decoded_path == id
                    || decoded_path == old_title
                    || file_stem.to_lowercase() == old_lower
            } else {
                file_stem.to_lowercase() == old_lower
            };

            if is_match {
                let new_path = if let Some(id) = target_id {
                    id.to_string()
                } else {
                    decoded_path.replacen(file_stem, new_title, 1)
                };

                let new_label = if label.trim().to_lowercase() == old_lower {
                    new_title
                } else {
                    label
                };

                let safe_path = urlencoding::encode(&new_path)
                    .into_owned()
                    .replace("%2F", "/");
                format!("[{}]({}{})", new_label, prefix, safe_path)
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        });

    text_with_md_links.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_edges ─────────────────────────────

    #[test]
    fn test_extract_tags() {
        let edges = extract_edges("node1", "Hello #world #test");
        let tags: Vec<_> = edges.iter().filter(|e| e.link_type == "tag").collect();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].target_title_or_path, "#world");
        assert_eq!(tags[1].target_title_or_path, "#test");
    }

    #[test]
    fn test_extract_tags_dedup() {
        let edges = extract_edges("node1", "#work hello #work");
        let tags: Vec<_> = edges.iter().filter(|e| e.link_type == "tag").collect();
        assert_eq!(tags.len(), 1); // deduped
    }

    #[test]
    fn test_extract_tags_case_insensitive() {
        let edges = extract_edges("node1", "#Work #WORK #work");
        let tags: Vec<_> = edges.iter().filter(|e| e.link_type == "tag").collect();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].target_title_or_path, "#work");
    }

    #[test]
    fn test_extract_wikilinks() {
        let edges = extract_edges("node1", "See [[Meeting Notes]] and [[Project Plan]]");
        let links: Vec<_> = edges.iter().filter(|e| e.link_type == "wikilink").collect();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target_title_or_path, "meeting notes");
        assert_eq!(links[1].target_title_or_path, "project plan");
    }

    #[test]
    fn test_extract_wikilink_with_alias() {
        let edges = extract_edges("node1", "See [[Real Title|Display Text]]");
        let links: Vec<_> = edges.iter().filter(|e| e.link_type == "wikilink").collect();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_title_or_path, "real title"); // Uses the actual title, not alias
    }

    #[test]
    fn test_extract_internal_links() {
        let edges = extract_edges("node1", "Check [My Note](synabit://note/Notes/hello.md)");
        let links: Vec<_> = edges
            .iter()
            .filter(|e| e.link_type == "internal_link")
            .collect();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_title_or_path, "Notes/hello.md");
    }

    #[test]
    fn test_extract_mixed_content() {
        let text = "# Meeting\n#work #urgent\n\nSee [[Project Alpha]] and [task](synabit://task/Tasks/todo.md)\n";
        let edges = extract_edges("node1", text);
        assert_eq!(edges.len(), 4); // 2 tags + 1 wikilink + 1 internal_link
    }

    #[test]
    fn test_extract_empty_text() {
        let edges = extract_edges("node1", "");
        assert!(edges.is_empty());
    }

    #[test]
    fn test_extract_no_links() {
        let edges = extract_edges("node1", "Just a plain text with no links or tags.");
        assert!(edges.is_empty());
    }

    #[test]
    fn test_source_id_propagation() {
        let edges = extract_edges("my-unique-id", "#test");
        assert_eq!(edges[0].source_id, "my-unique-id");
    }

    // ── rename_links_in_text ──────────────────────

    #[test]
    fn test_rename_wikilink() {
        let text = "See [[Old Title]] for details.";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);
        assert_eq!(result, "See [[New Title]] for details.");
    }

    #[test]
    fn test_rename_wikilink_preserves_alias() {
        let text = "See [[Old Title|Display Name]] here.";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);
        assert_eq!(result, "See [[New Title|Display Name]] here.");
    }

    #[test]
    fn test_rename_case_insensitive() {
        let text = "See [[old title]] here.";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);
        assert_eq!(result, "See [[New Title]] here.");
    }

    #[test]
    fn test_rename_no_false_match() {
        let text = "See [[Different Title]] here.";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);
        assert_eq!(result, "See [[Different Title]] here."); // Unchanged
    }

    #[test]
    fn test_rename_multiple_occurrences() {
        let text = "See [[Old Title]] and also [[Old Title|alias]].";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);
        assert!(result.contains("[[New Title]]"));
        assert!(result.contains("[[New Title|alias]]"));
        assert!(!result.contains("Old Title"));
    }

    #[test]
    fn test_rename_internal_link() {
        let text = "[Old Title](synabit://note/Notes/Old%20Title.md)";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);
        assert!(result.contains("New Title"));
    }

    /// A label the writer chose is not a stale copy of the title, and renaming
    /// the note must not overwrite it.
    ///
    /// This is what makes an alias worth typing: "công ty cũ" reads that way in
    /// the sentence because someone meant it to, and the registered name
    /// changing is no reason for the sentence to change. The mention menu
    /// writes exactly this shape when a `|` is used, so the guarantee is now
    /// load-bearing rather than incidental.
    #[test]
    fn test_rename_internal_link_keeps_a_deliberate_label() {
        let text = "Chuyển từ [công ty cũ](synabit://note/Notes/Old%20Title.md) sang.";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);

        assert!(
            result.contains("[công ty cũ]"),
            "the label was chosen, not derived: {result}"
        );
        assert!(
            result.contains("New%20Title.md"),
            "the destination should still follow the rename: {result}"
        );
        assert!(!result.contains("Old%20Title.md"), "{result}");
    }

    /// The other half of the same rule: a label that *is* the old title was
    /// never a choice, so it follows the rename.
    #[test]
    fn test_rename_internal_link_updates_a_label_that_was_just_the_title() {
        let text = "[Old Title](synabit://note/Notes/Old%20Title.md)";
        let result = rename_links_in_text(text, "Old Title", "New Title", None);
        assert!(result.starts_with("[New Title]"), "{result}");
    }

    // ── NodeResolver + extract_resolved_node_edges ────────

    fn make_node(id: &str, title: &str, node_type: &str) -> crate::models::node::NodeMetadata {
        crate::models::node::NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            content: String::new(),
            properties: serde_json::json!({}),
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    #[test]
    fn test_resolver_title_match() {
        let nodes = vec![
            make_node("Notes/Meeting.md", "Meeting", "note"),
            make_node("Notes/Project.md", "Project Alpha", "note"),
        ];
        let resolver = NodeResolver::new(&nodes);
        assert_eq!(resolver.resolve("meeting", "wikilink"), "Notes/Meeting.md");
        assert_eq!(
            resolver.resolve("project alpha", "wikilink"),
            "Notes/Project.md"
        );
    }

    #[test]
    fn test_resolver_path_match() {
        let nodes = vec![make_node("Notes/hello.md", "Hello", "note")];
        let resolver = NodeResolver::new(&nodes);
        assert_eq!(
            resolver.resolve("Notes/hello.md", "internal_link"),
            "Notes/hello.md"
        );
    }

    #[test]
    fn test_resolver_ghost_fallback() {
        let nodes = vec![make_node("Notes/A.md", "A", "note")];
        let resolver = NodeResolver::new(&nodes);
        assert_eq!(
            resolver.resolve("nonexistent", "wikilink"),
            "ghost:nonexistent"
        );
    }

    #[test]
    fn test_resolved_edges_skip_tags() {
        let mut node = make_node("Notes/A.md", "A", "note");
        node.content = "#tag1 [[B]]".to_string();

        let nodes = vec![node.clone(), make_node("Notes/B.md", "B", "note")];
        let resolver = NodeResolver::new(&nodes);
        let edges = extract_resolved_node_edges(&node, &resolver);

        // Should have 1 edge (wikilink to B), NO tag edge
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target_id, "Notes/B.md");
        assert_eq!(edges[0].edge_type, "wikilink");
    }

    #[test]
    fn test_resolved_edges_skip_self_links() {
        let mut node = make_node("Notes/A.md", "A", "note");
        node.content = "[[A]]".to_string(); // Links to itself

        let nodes = vec![node.clone()];
        let resolver = NodeResolver::new(&nodes);
        let edges = extract_resolved_node_edges(&node, &resolver);

        assert!(edges.is_empty());
    }

    #[test]
    fn test_resolved_edges_ghost_node() {
        let mut node = make_node("Notes/A.md", "A", "note");
        node.content = "[[Deleted Note]]".to_string();

        let nodes = vec![node.clone()];
        let resolver = NodeResolver::new(&nodes);
        let edges = extract_resolved_node_edges(&node, &resolver);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target_id, "ghost:deleted note");
    }

    #[test]
    fn test_resolved_edges_internal_link() {
        let mut node = make_node("Notes/A.md", "A", "note");
        node.content = "[Task](synabit://task/Tasks/todo.md)".to_string();

        let nodes = vec![node.clone(), make_node("Tasks/todo.md", "Todo", "task")];
        let resolver = NodeResolver::new(&nodes);
        let edges = extract_resolved_node_edges(&node, &resolver);

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].target_id, "Tasks/todo.md");
        assert_eq!(edges[0].edge_type, "internal_link");
    }
}

#[cfg(test)]
mod task_link_tests {
    use super::*;
    use crate::models::node::NodeMetadata;

    fn node(id: &str, node_type: &str, title: &str) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            content: String::new(),
            properties: serde_json::json!({}),
            created_at: String::new(),
            updated_at: String::new(),
            timestamp: 0,
            blocks: None,
        }
    }

    /// The Tasks app shows what else in the vault points at a task, which only
    /// works if a link to one resolves to that task's stable identity rather
    /// than to a ghost. These pin each way a link can name a task.
    #[test]
    fn a_synabit_link_to_a_task_resolves_to_it() {
        let task = node("Tasks/abc.md", "task", "Ship the thing");
        let resolver = NodeResolver::new(std::slice::from_ref(&task));
        let edges = extract_edges(
            "Notes/plan.md",
            "agreed to [ship](synabit://task/Tasks/abc.md) this week",
        );
        let resolved: Vec<String> = edges
            .iter()
            .map(|e| resolver.resolve(&e.target_title_or_path, &e.link_type))
            .collect();
        assert_eq!(resolved, vec![task.stable_id().to_string()]);
    }

    #[test]
    fn a_wikilink_by_title_resolves_to_the_task() {
        let task = node("Tasks/abc.md", "task", "Ship the thing");
        let resolver = NodeResolver::new(std::slice::from_ref(&task));
        let edges = extract_edges("Notes/plan.md", "see [[Ship the thing]]");
        assert_eq!(
            resolver.resolve(&edges[0].target_title_or_path, &edges[0].link_type),
            task.stable_id().to_string()
        );
    }

    /// Case is not part of a name here. A note that writes the folder in lower
    /// case still points at the task.
    #[test]
    fn the_case_of_the_path_does_not_matter() {
        let task = node("Tasks/abc.md", "task", "Ship the thing");
        let resolver = NodeResolver::new(std::slice::from_ref(&task));
        assert_eq!(
            resolver.resolve("tasks/abc.md", "internal_link"),
            task.stable_id().to_string()
        );
    }

    /// A link to a task that is gone stays a ghost rather than attaching
    /// itself to whichever task happens to be nearby.
    #[test]
    fn a_link_to_a_task_that_no_longer_exists_stays_a_ghost() {
        let task = node("Tasks/abc.md", "task", "Ship the thing");
        let resolver = NodeResolver::new(std::slice::from_ref(&task));
        assert!(resolver
            .resolve("Tasks/deleted.md", "internal_link")
            .starts_with("ghost:"));
    }
}

#[cfg(test)]
mod person_link_tests {
    use super::*;
    use crate::models::node::NodeMetadata;
    use serde_json::json;

    fn person(id: &str, stable: &str, properties: serde_json::Value) -> NodeMetadata {
        let mut props = properties;
        if let Some(map) = props.as_object_mut() {
            map.insert("node_id".to_string(), json!(stable));
        }
        NodeMetadata {
            id: id.to_string(),
            node_type: "person".to_string(),
            title: id.to_string(),
            content: String::new(),
            properties: props,
            created_at: String::new(),
            updated_at: String::new(),
            timestamp: 0,
            blocks: None,
        }
    }

    #[test]
    fn a_connection_becomes_an_edge_that_carries_the_relationship() {
        // The relationship used to have nowhere to go: the edge index knew
        // two people were linked, and a parallel list in the frontmatter knew
        // what the link meant. One of them had to be the answer.
        let an = person(
            "People/an.md",
            "uuid-an",
            json!({ "connections": [{ "person_id": "uuid-binh", "relation_type": "mentor" }] }),
        );
        let binh = person("People/binh.md", "uuid-binh", json!({}));
        let resolver = NodeResolver::new(&[an.clone(), binh]);

        let edges = extract_resolved_node_edges(&an, &resolver);
        let link = edges
            .iter()
            .find(|e| e.edge_type == "person_link")
            .expect("a person link");
        assert_eq!(link.source_id, "uuid-an");
        assert_eq!(link.target_id, "uuid-binh");
        assert_eq!(link.relation.as_deref(), Some("mentor"));
    }

    #[test]
    fn a_link_written_before_identities_still_resolves() {
        // Older vaults name a path. Refusing those would empty every graph in
        // the app on the day this shipped.
        let an = person(
            "People/an.md",
            "uuid-an",
            json!({ "connections": [{ "person_id": "People/binh.md", "relation_type": "friend" }] }),
        );
        let binh = person("People/binh.md", "uuid-binh", json!({}));
        let resolver = NodeResolver::new(&[an.clone(), binh]);

        let edges = extract_resolved_node_edges(&an, &resolver);
        let link = edges.iter().find(|e| e.edge_type == "person_link").expect("a link");
        assert_eq!(link.target_id, "uuid-binh", "resolved to the identity, not the path");
    }

    #[test]
    fn a_link_survives_the_other_person_being_moved() {
        // The whole reason for storing an identity. The file moves; the link
        // points at the same person.
        let an = person(
            "People/an.md",
            "uuid-an",
            json!({ "connections": [{ "person_id": "uuid-binh", "relation_type": "friend" }] }),
        );
        let moved = person("People/Archive/binh.md", "uuid-binh", json!({}));
        let resolver = NodeResolver::new(&[an.clone(), moved]);

        let edges = extract_resolved_node_edges(&an, &resolver);
        let link = edges.iter().find(|e| e.edge_type == "person_link").expect("a link");
        assert_eq!(link.target_id, "uuid-binh");
    }

    #[test]
    fn a_connection_with_no_relationship_is_still_a_link() {
        let an = person(
            "People/an.md",
            "uuid-an",
            json!({ "connections": [{ "person_id": "uuid-binh" }] }),
        );
        let binh = person("People/binh.md", "uuid-binh", json!({}));
        let resolver = NodeResolver::new(&[an.clone(), binh]);

        let link = extract_resolved_node_edges(&an, &resolver)
            .into_iter()
            .find(|e| e.edge_type == "person_link")
            .expect("a link");
        assert_eq!(link.relation, None);
    }

    #[test]
    fn a_person_with_no_connections_produces_no_person_links() {
        let an = person("People/an.md", "uuid-an", json!({ "tags": ["work"] }));
        let resolver = NodeResolver::new(&[an.clone()]);
        assert!(extract_resolved_node_edges(&an, &resolver)
            .iter()
            .all(|e| e.edge_type != "person_link"));
    }

    #[test]
    fn an_ordinary_mention_carries_no_relationship() {
        // Only a person link says what it means. A note that names somebody
        // means only that they were named.
        let mut note = person("Notes/a.md", "uuid-a", json!({}));
        note.node_type = "note".to_string();
        note.content = "spoke to [Binh](synabit://person/People/binh.md)".to_string();
        let binh = person("People/binh.md", "uuid-binh", json!({}));
        let resolver = NodeResolver::new(&[note.clone(), binh]);

        let edges = extract_resolved_node_edges(&note, &resolver);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge_type, "internal_link");
        assert_eq!(edges[0].relation, None);
    }
}
