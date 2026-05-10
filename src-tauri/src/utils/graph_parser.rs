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
    let md_link_re = Regex::new(r"\[([^\]]*)\]\(synabit://(?:note|node|person|task|quickcap|event)/([^)]+)\)").unwrap();
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
    edges.into_iter().filter(|e| {
        seen.insert(format!("{}-{}", e.target_title_or_path, e.link_type))
    }).collect()
}

/// Replaces WikiLinks targeting `old_name` with `new_name`.
/// Retains existing aliases if present.
pub fn rename_links_in_text(text: &str, old_title: &str, new_title: &str, target_id: Option<&str>) -> String {
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
    let md_link_re = Regex::new(r"\[([^\]]*)\]\((synabit://(?:note|node|person|task|quickcap|event)/)([^)]+)\)").unwrap();
    let text_with_md_links = md_link_re
        .replace_all(&text_with_wiki_links, |caps: &regex::Captures| {
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
                decoded_path == id || decoded_path == old_title || file_stem.to_lowercase() == old_lower
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
                
                let safe_path = urlencoding::encode(&new_path).into_owned().replace("%2F", "/");
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
        let links: Vec<_> = edges.iter().filter(|e| e.link_type == "internal_link").collect();
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
}
