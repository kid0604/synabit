use crate::models::node::NodeMetadata;
use gray_matter::engine::YAML;
use gray_matter::Matter;
use serde_json::Value;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn parse_file_to_node(vault_path: &str, file_path: &Path) -> Option<NodeMetadata> {
    let rel_path = crate::path_utils::to_relative(file_path, vault_path);
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let content = std::fs::read_to_string(file_path).ok()?;
    let metadata = file_path.metadata().ok()?;
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let created = metadata.created().unwrap_or(modified);

    let mut created_at = chrono::DateTime::<chrono::Local>::from(created)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut updated_at = chrono::DateTime::<chrono::Local>::from(modified)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let timestamp = modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut title = file_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut node_type = String::new();
    let mut properties = Value::Null;
    let mut final_content = content.clone();

    if ext == "md" {
        let matter = Matter::<YAML>::new();
        // Parse with generic Value to capture all frontmatter
        if let Ok(parsed) = matter.parse::<serde_json::Value>(&content) {
            if let Some(data) = parsed.data {
                properties = data;
                if let Some(t) = properties.get("title").and_then(|v| v.as_str()) {
                    title = t.to_string();
                }
                if let Some(ty) = properties.get("type").and_then(|v| v.as_str()) {
                    node_type = ty.to_string();
                }
            }
            final_content = parsed.content;
        } else {
            // Failed to parse frontmatter or no frontmatter
            properties = serde_json::json!({});
        }

        if node_type.is_empty() {
            node_type = "note".to_string();
        }
    } else if ext == "json" || ext == "canvas" {
        if let Ok(json_val) = serde_json::from_str::<Value>(&content) {
            if let Some(t) = json_val.get("title").and_then(|v| v.as_str()) {
                title = t.to_string();
            }
            if let Some(ty) = json_val.get("type").and_then(|v| v.as_str()) {
                node_type = ty.to_string();
            } else if file_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .ends_with(".whiteboard.json")
            {
                node_type = "whiteboard".to_string();
            } else {
                node_type = ext.to_string();
            }

            if let Some(meta) = json_val.get("metadata") {
                properties = meta.clone();
            } else {
                properties = serde_json::json!({});
            }

            if let Some(c) = json_val.get("content").and_then(|v| v.as_str()) {
                final_content = c.to_string();
            }
        } else {
            node_type = ext.to_string();
            properties = serde_json::json!({});
        }
    } else {
        return None;
    }

    // A type nobody handles is almost always a typo in frontmatter, and the
    // symptom — a note that simply stops appearing in its mini-app — points
    // nowhere near the cause. Say so here, where the file is still in hand.
    // The value is left exactly as written: see `NodeType::Other`.
    if !crate::models::node::NodeType::from(node_type.as_str()).is_known() {
        log::warn!(
            "'{}' declares type '{}', which no part of the app queries; it will not appear in any list",
            rel_path,
            node_type
        );
    }

    // A whiteboard's file is a diagram, not prose. Storing the raw JSON as the
    // node's content put braces and coordinates into the search index and into
    // every preview; storing the labels its author typed puts the board's actual
    // words there instead. The board itself is always read back from disk, so
    // nothing needs `content` to round-trip the file.
    if node_type == "whiteboard" {
        let (summary, node_count, board_tags) = summarise_whiteboard(&content);
        final_content = summary;

        if !properties.is_object() {
            properties = serde_json::json!({});
        }
        if let Some(map) = properties.as_object_mut() {
            map.insert("node_count".to_string(), Value::from(node_count));
            // Tags sit at the top level of a board file rather than inside its
            // metadata, so lift them where every other node type keeps them.
            if !board_tags.is_empty() && !map.contains_key("tags") {
                map.insert("tags".to_string(), Value::from(board_tags));
            }
        }
    }

    // Override dates from properties if available
    if let Some(c) = properties.get("created_at").and_then(|v| v.as_str()) {
        created_at = c.to_string();
    }
    if let Some(u) = properties.get("updated_at").and_then(|v| v.as_str()) {
        updated_at = u.to_string();
    }

    // Extract blocks if markdown
    let mut blocks = None;
    if ext == "md" {
        blocks = Some(extract_blocks(&final_content));
    }

    Some(NodeMetadata {
        id: rel_path,
        node_type,
        title,
        content: final_content,
        properties,
        created_at,
        updated_at,
        timestamp,
        blocks,
    })
}

/// Reduce a whiteboard file to the parts worth indexing.
///
/// Returns the text its author typed into the board, how many items are on it,
/// and the board's tags. Everything else in the file — positions, edge handles,
/// styling — describes how to draw the board, not what it says.
pub fn summarise_whiteboard(raw_json: &str) -> (String, usize, Vec<String>) {
    let Ok(parsed) = serde_json::from_str::<Value>(raw_json) else {
        return (String::new(), 0, Vec::new());
    };

    let board_nodes = parsed.get("nodes").and_then(|v| v.as_array());
    let node_count = board_nodes.map(|n| n.len()).unwrap_or(0);

    let labels: Vec<String> = board_nodes
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|n| n.get("data")?.get("label")?.as_str())
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let tags: Vec<String> = parsed
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    (labels.join(" "), node_count, tags)
}

/// Matches a ` ^block-id` marker at the end of a block.
///
/// Compiled once: `extract_blocks` runs for every Markdown file in the vault,
/// and building the regex costs more than matching with it.
static BLOCK_MARKER_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(?m)\s*\^([a-zA-Z0-9\-]+)\s*$").unwrap());

pub fn extract_blocks(content: &str) -> Vec<(String, String)> {
    use pulldown_cmark::{Event, Options, Parser, TagEnd};

    let mut blocks = Vec::new();
    let options = Options::all();
    let parser = Parser::new_ext(content, options).into_offset_iter();

    let re = &*BLOCK_MARKER_RE;

    for (event, range) in parser {
        match event {
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Item) => {
                let block_text = &content[range.clone()];
                if let Some(captures) = re.captures(block_text) {
                    if let Some(id_match) = captures.get(1) {
                        let block_id = id_match.as_str().to_string();
                        // Extract content without the block ID marker? Or keep it?
                        // Keep full block text for exact rendering.
                        blocks.push((block_id, block_text.to_string()));
                    }
                }
            }
            _ => {}
        }
    }

    blocks
}

/// Tests for the point where a file on disk becomes a node.
///
/// Everything downstream — which mini-app shows the item, which query finds it,
/// whether the vault scan will ever clean it up — keys off `node_type`, and
/// `node_type` is decided here and nowhere else. These lock down the rules that
/// decision follows, because they are implicit in a chain of `if let` branches
/// rather than stated anywhere.
#[cfg(test)]
mod tests {
    use super::*;

    /// A vault directory unique to this run.
    ///
    /// Mirrors the helper in `commands::nodes`: a fixed name under the system
    /// temp directory is shared by every process on the machine, so two test
    /// runs at once delete each other's fixtures.
    fn vault() -> (tempfile::TempDir, String) {
        let holder = tempfile::tempdir().expect("tempdir");
        let path = holder.path().join("vault");
        std::fs::create_dir_all(&path).expect("create vault dir");
        let as_string = path.to_string_lossy().to_string();
        (holder, as_string)
    }

    fn write(vault_path: &str, rel: &str, content: &str) -> std::path::PathBuf {
        let path = std::path::Path::new(vault_path).join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(&path, content).expect("write fixture");
        path
    }

    /// The default that most of the vault relies on: a plain Markdown file with
    /// no frontmatter is still a note, not an untyped orphan.
    #[test]
    fn markdown_without_frontmatter_becomes_a_note_titled_after_its_filename() {
        let (_holder, vault_path) = vault();
        let path = write(&vault_path, "Notes/hello.md", "Just some text.\n");

        let node = parse_file_to_node(&vault_path, &path).expect("markdown should parse");

        assert_eq!(node.node_type, "note");
        assert_eq!(node.title, "hello");
        // Reading an absent property must stay safe — callers do this everywhere
        // without checking the shape of `properties` first.
        assert!(node.properties.get("tags").is_none());
    }

    /// `type:` in frontmatter is the only thing that makes a file a task, a
    /// project, or a person. Nothing validates it against a known list.
    #[test]
    fn frontmatter_decides_the_type_and_the_title() {
        let (_holder, vault_path) = vault();
        let path = write(
            &vault_path,
            "Tasks/t.md",
            "---\ntitle: Ship the thing\ntype: task\nstatus: doing\n---\nbody text\n",
        );

        let node = parse_file_to_node(&vault_path, &path).expect("markdown should parse");

        assert_eq!(node.node_type, "task");
        assert_eq!(node.title, "Ship the thing");
        assert_eq!(
            node.properties.get("status").and_then(|v| v.as_str()),
            Some("doing")
        );
        assert_eq!(
            node.content.trim(),
            "body text",
            "frontmatter must be stripped from content"
        );
    }

    /// An unknown `type:` is accepted verbatim. Worth stating plainly: there is
    /// no registry, so a typo in frontmatter silently creates a new node type
    /// that no mini-app will ever query.
    #[test]
    fn an_unrecognised_type_is_taken_at_face_value() {
        let (_holder, vault_path) = vault();
        let path = write(&vault_path, "x.md", "---\ntype: taks\n---\n");

        let node = parse_file_to_node(&vault_path, &path).expect("markdown should parse");

        assert_eq!(node.node_type, "taks");
    }

    /// Whiteboards carry their type in the filename rather than the payload, so
    /// a `.whiteboard.json` with no `type` field must still resolve. The
    /// frontend patches this case by hand today; the rule lives here.
    #[test]
    fn a_whiteboard_json_is_recognised_by_its_filename_when_the_payload_is_silent() {
        let (_holder, vault_path) = vault();
        let path = write(
            &vault_path,
            "Whiteboards/board.whiteboard.json",
            r#"{"title":"Board","content":"{}"}"#,
        );

        let node = parse_file_to_node(&vault_path, &path).expect("json should parse");

        assert_eq!(node.node_type, "whiteboard");
        assert_eq!(node.title, "Board");
    }

    /// The JSON shape the app writes: type, metadata and content are separate
    /// fields, and `metadata` — not the whole document — becomes `properties`.
    #[test]
    fn a_typed_json_node_takes_its_properties_from_the_metadata_field() {
        let (_holder, vault_path) = vault();
        let path = write(
            &vault_path,
            "Finance/Config.json",
            r#"{"title":"Config","type":"finance_config","metadata":{"currency":"VND"},"content":"note"}"#,
        );

        let node = parse_file_to_node(&vault_path, &path).expect("json should parse");

        assert_eq!(node.node_type, "finance_config");
        assert_eq!(
            node.properties.get("currency").and_then(|v| v.as_str()),
            Some("VND")
        );
        assert_eq!(node.content, "note");
        assert!(
            node.properties.get("title").is_none(),
            "top-level fields must not leak into properties"
        );
    }

    /// A half-written or hand-mangled JSON file must not take down the scan. It
    /// degrades to a node typed after its extension, which is also what makes
    /// such files visible enough to notice and fix.
    #[test]
    fn malformed_json_degrades_to_a_node_typed_after_its_extension() {
        let (_holder, vault_path) = vault();
        let path = write(&vault_path, "broken.json", "{not valid json");

        let node = parse_file_to_node(&vault_path, &path).expect("broken json should still parse");

        assert_eq!(node.node_type, "json");
        assert_eq!(node.title, "broken");
    }

    /// Timestamps in the file win over the filesystem's. Copying a vault or
    /// restoring from backup rewrites mtime for every file at once; without
    /// this, every note would claim to have been edited at restore time.
    #[test]
    fn timestamps_in_the_frontmatter_override_the_filesystem() {
        let (_holder, vault_path) = vault();
        let path = write(
            &vault_path,
            "n.md",
            "---\ncreated_at: \"2020-01-01 00:00:00\"\nupdated_at: \"2021-02-03 04:05:06\"\n---\n",
        );

        let node = parse_file_to_node(&vault_path, &path).expect("markdown should parse");

        assert_eq!(node.created_at, "2020-01-01 00:00:00");
        assert_eq!(node.updated_at, "2021-02-03 04:05:06");
    }

    /// The node id is the vault-relative path, always forward-slashed. Every
    /// edge, search entry and cleanup check is keyed on this string, so the
    /// separator is not cosmetic — it has to match across platforms.
    #[test]
    fn the_id_is_the_vault_relative_path_with_forward_slashes() {
        let (_holder, vault_path) = vault();
        let path = write(&vault_path, "Projects/sub/deep.md", "---\ntype: project\n---\n");

        let node = parse_file_to_node(&vault_path, &path).expect("markdown should parse");

        assert_eq!(node.id, "Projects/sub/deep.md");
    }

    /// Anything that is not Markdown or JSON is not a node at all. This is what
    /// keeps images and PDFs in the vault out of the nodes table.
    #[test]
    fn a_file_that_is_neither_markdown_nor_json_is_not_a_node() {
        let (_holder, vault_path) = vault();
        let path = write(&vault_path, "photo.png", "not really a png");

        assert!(parse_file_to_node(&vault_path, &path).is_none());
    }

    /// Block markers are harvested for Markdown so block references resolve,
    /// and left alone for JSON, where a `^id` would just be payload text.
    #[test]
    fn blocks_are_harvested_from_markdown_and_not_from_json() {
        let (_holder, vault_path) = vault();

        let md = write(&vault_path, "b.md", "A first line.\n\nSecond line ^abc123\n");
        let node = parse_file_to_node(&vault_path, &md).expect("markdown should parse");
        let blocks = node.blocks.expect("markdown should carry a block list");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "abc123");

        let json = write(&vault_path, "b.json", r#"{"type":"note","content":"x ^abc123"}"#);
        let node = parse_file_to_node(&vault_path, &json).expect("json should parse");
        assert!(node.blocks.is_none());
    }
}
