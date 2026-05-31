use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source_id: String,
    pub target_title_or_path: String,
    pub link_type: String, // 'wikilink', 'internal_link', 'tag'
}

/// Extracts all tags and internal links from raw markdown text
pub fn extract_edges(source_id: &str, text: &str) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. Tags (#tag)
    let tag_re = Regex::new(r"(?im)(?:^|\s)#([a-zA-Z0-9_\-/]+)").unwrap();
    for cap in tag_re.captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let tag_name = format!("#{}", m.as_str().to_lowercase());
            if seen.insert(tag_name.clone()) {
                edges.push(GraphEdge {
                    source_id: source_id.to_string(),
                    target_title_or_path: tag_name,
                    link_type: "tag".to_string(),
                });
            }
        }
    }

    // 2. WikiLinks ([[Link]] or [[Link|Alias]])
    let wiki_re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    for cap in wiki_re.captures_iter(text) {
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
                });
            }
        }
    }

    // 3. Tiptap Internal Links ([Title](synabit://.../path))
    let md_link_re =
        Regex::new(r"\[([^\]]*)\]\(synabit://(?:note|node|person|task|quickcap|event|project)/([^)]+)\)")
            .unwrap();
    for cap in md_link_re.captures_iter(text) {
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

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    edges
        .into_iter()
        .filter(|e| seen.insert(format!("{}-{}", e.target_title_or_path, e.link_type)))
        .collect()
}

// ═══════════════════════════════════════════════════════════
//  NEW: Resolved Node Edges (ID-based)
// ═══════════════════════════════════════════════════════════

use crate::db::NodeEdge;

/// Pre-built lookup maps for fast title → ID resolution
pub struct NodeResolver {
    /// title (lowercase, no .md) → node ID
    title_map: std::collections::HashMap<String, String>,
    /// path (lowercase) → node ID  (for internal_link paths)
    path_map: std::collections::HashMap<String, String>,
    /// id → node ID  (direct ID match)
    id_set: std::collections::HashSet<String>,
    /// filename (lowercase) → node ID  (for file embeds like "image.png")
    filename_map: std::collections::HashMap<String, String>,
}

impl NodeResolver {
    /// Build resolver from all nodes — O(N) once, then O(1) per resolve
    pub fn new(all_nodes: &[crate::models::node::NodeMetadata]) -> Self {
        let mut title_map = std::collections::HashMap::new();
        let mut path_map = std::collections::HashMap::new();
        let mut id_set = std::collections::HashSet::new();
        let mut filename_map = std::collections::HashMap::new();

        for node in all_nodes {
            let id = node.id.clone();
            id_set.insert(id.clone());

            // Title lookup (lowercase, strip .md)
            let title_lower = node.title.to_lowercase().replace(".md", "");
            title_map.entry(title_lower).or_insert_with(|| id.clone());

            // Path lookup (the node id IS a path for file-based nodes)
            path_map
                .entry(id.to_lowercase())
                .or_insert_with(|| id.clone());

            // For file nodes: map filename from properties.path
            if node.node_type == "file" {
                if let Some(p) = node.properties.get("path").and_then(|v| v.as_str()) {
                    let file_path = std::path::Path::new(p);
                    if let Some(fname) = file_path.file_name().and_then(|s| s.to_str()) {
                        filename_map
                            .entry(fname.to_lowercase())
                            .or_insert_with(|| id.clone());
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
            id_set,
            filename_map,
        }
    }

    /// Resolve a target string to a node ID, or return ghost:<target>
    pub fn resolve(&self, target: &str, _link_type: &str) -> String {
        let lower = target.to_lowercase();
        let no_md = lower.replace(".md", "");

        // 1. Direct ID match
        if self.id_set.contains(target) {
            return target.to_string();
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

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for raw in raw_edges {
        // Skip tags — they're stored in node properties
        if raw.link_type == "tag" {
            continue;
        }

        let target_id = resolver.resolve(&raw.target_title_or_path, &raw.link_type);

        // Skip self-links
        if target_id == node.id {
            continue;
        }

        // Map old link_type → new edge_type
        let edge_type = match raw.link_type.as_str() {
            "wikilink" => "wikilink",
            "internal_link" => "internal_link",
            _ => "internal_link", // property-level links (key names like "assignee")
        };

        // Determine semantic relation
        let relation: Option<String> = if target_id.starts_with("ghost:") {
            None // Ghost — can't determine
        } else {
            None // Auto-extracted edges default to NULL relation
        };

        let dedup_key = format!("{}-{}-{}", node.id, target_id, edge_type);
        if !seen.insert(dedup_key) {
            continue;
        }

        result.push(NodeEdge {
            id: uuid::Uuid::new_v4().to_string(),
            source_id: node.id.clone(),
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
    let wiki_re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let old_lower = old_title.to_lowercase();

    let text_with_wiki_links = wiki_re
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
    let md_link_re =
        Regex::new(r"\[([^\]]*)\]\((synabit://(?:note|node|person|task|quickcap|event|project)/)([^)]+)\)")
            .unwrap();
    let text_with_md_links =
        md_link_re.replace_all(&text_with_wiki_links, |caps: &regex::Captures| {
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
