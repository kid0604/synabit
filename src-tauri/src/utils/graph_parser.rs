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

    // 3. Tiptap Internal Links ([Title](synabit://note/path))
    let md_link_re = Regex::new(r"\[([^\]]*)\]\(synabit://note/([^)]+)\)").unwrap();
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

/// Replaces WikiLinks targeting `old_name` with `new_name`.
/// Retains existing aliases if present.
pub fn rename_links_in_text(text: &str, old_name: &str, new_name: &str) -> String {
    let wiki_re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let old_lower = old_name.to_lowercase();

    let text_with_wiki_links = wiki_re
        .replace_all(text, |caps: &regex::Captures| {
            let inner = caps.get(1).unwrap().as_str();
            let mut parts = inner.splitn(2, '|');
            let title = parts.next().unwrap_or("").trim();
            let alias = parts.next().map(|s| s.trim());

            if title.to_lowercase() == old_lower {
                if let Some(a) = alias {
                    format!("[[{}|{}]]", new_name, a)
                } else {
                    format!("[[{}]]", new_name)
                }
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        })
        .to_string();

    // 2. Replace Tiptap internal links: [Anything](synabit://note/OldName)
    let md_link_re = Regex::new(r"\[([^\]]*)\]\(synabit://note/([^)]+)\)").unwrap();
    let text_with_md_links = md_link_re
        .replace_all(&text_with_wiki_links, |caps: &regex::Captures| {
            let label = caps.get(1).unwrap().as_str();
            let encoded_path = caps.get(2).unwrap().as_str();
            let decoded_path = urlencoding::decode(encoded_path)
                .unwrap_or(std::borrow::Cow::Borrowed(encoded_path))
                .to_string();

            let file_stem = std::path::Path::new(&decoded_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&decoded_path);

            if file_stem.to_lowercase() == old_lower {
                // Re-encode the new name
                let new_path = decoded_path.replacen(file_stem, new_name, 1);
                let new_label = if label.trim().to_lowercase() == old_lower {
                    new_name
                } else {
                    label
                };
                // Replace spaces to be safe
                format!(
                    "[{}](synabit://note/{})",
                    new_label,
                    urlencoding::encode(&new_path)
                )
            } else {
                caps.get(0).unwrap().as_str().to_string()
            }
        })
        .to_string();

    text_with_md_links
}
