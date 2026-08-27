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
        let summary = summarise_whiteboard(&content);
        final_content = summary.text;

        if !properties.is_object() {
            properties = serde_json::json!({});
        }
        if let Some(map) = properties.as_object_mut() {
            map.insert("node_count".to_string(), Value::from(summary.node_count));

            // Notes pinned to a board are links from it, and were invisible:
            // a board's indexed text is the labels its author typed, and a
            // note card carries no label — so dragging a note onto a board
            // put it nowhere in the graph and nowhere in that note's
            // backlinks. Written in the form the link extractor already
            // reads, so the edge, the backlink and the Nexus graph all follow
            // without a second path through the code.
            if !summary.note_links.is_empty() {
                map.insert("linked_notes".to_string(), Value::from(summary.note_links));
            }
            // Tags sit at the top level of a board file rather than inside its
            // metadata, so lift them where every other node type keeps them.
            if !summary.tags.is_empty() && !map.contains_key("tags") {
                map.insert("tags".to_string(), Value::from(summary.tags));
            }

            // A board stamps every save with an RFC 3339 time, because sync
            // compares two copies of a board by that string and the two
            // devices need not share a time zone. Every other date in the
            // index is local time in the app's own format, and the block
            // below copies this one straight over the top of it. Convert it
            // here rather than let one node type put a second date format
            // into lists that sort them as plain strings; a stamp that will
            // not parse is dropped, which leaves the file's mtime standing.
            let stamped = map
                .get("updated_at")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if let Some(stamped) = stamped {
                match chrono::DateTime::parse_from_rfc3339(&stamped) {
                    Ok(parsed) => {
                        let local = parsed
                            .with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string();
                        map.insert("updated_at".to_string(), Value::from(local));
                    }
                    Err(_) => {
                        map.remove("updated_at");
                    }
                }
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

/// What a whiteboard file has to say about itself.
#[derive(Debug, Default, PartialEq)]
pub struct BoardSummary {
    /// The words on the board: the labels its author typed, and the titles of
    /// the notes pinned to it.
    pub text: String,
    /// How many items are on the board.
    pub node_count: usize,
    /// The board's own tags.
    pub tags: Vec<String>,
    /// One link per note card, written as `[Title](synabit://note/<path>)`.
    pub note_links: Vec<String>,
}

/// Reduce a whiteboard file to the parts worth indexing.
///
/// Everything left out — positions, edge handles, styling — describes how to
/// draw the board, not what it says or what it points at.
pub fn summarise_whiteboard(raw_json: &str) -> BoardSummary {
    let Ok(parsed) = serde_json::from_str::<Value>(raw_json) else {
        return BoardSummary::default();
    };

    let board_nodes = parsed.get("nodes").and_then(|v| v.as_array());
    let node_count = board_nodes.map(|n| n.len()).unwrap_or(0);

    let mut words: Vec<String> = Vec::new();
    let mut note_links: Vec<String> = Vec::new();
    let mut seen_notes = std::collections::HashSet::new();

    for node in board_nodes.map(Vec::as_slice).unwrap_or_default() {
        let data = node.get("data");

        if let Some(label) = data.and_then(|d| d.get("label")).and_then(Value::as_str) {
            if !label.is_empty() {
                words.push(label.to_string());
            }
        }

        // A note card names the note it shows by that note's path in the
        // vault, which is exactly the id every other link resolves against.
        let Some(note_id) = data.and_then(|d| d.get("noteId")).and_then(Value::as_str) else {
            continue;
        };
        if note_id.is_empty() {
            continue;
        }

        let title = data
            .and_then(|d| d.get("noteTitle"))
            .and_then(Value::as_str)
            .unwrap_or(note_id);
        if !title.is_empty() {
            words.push(title.to_string());
        }

        if seen_notes.insert(note_id.to_string()) {
            // The link text runs to the first `]` and the target to the first
            // `)`, so a bracket in a note's title or a parenthesis in its path
            // would cut the link short. Encoding the path and dropping
            // brackets from the title keeps both ends intact.
            let safe_title = title.replace(['[', ']'], "");
            let encoded = urlencoding::encode(note_id);
            note_links.push(format!("[{safe_title}](synabit://note/{encoded})"));
        }
    }

    let tags: Vec<String> = parsed
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    BoardSummary {
        text: words.join(" "),
        node_count,
        tags,
        note_links,
    }
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

    /// A board writes its save stamp in RFC 3339 so that sync can compare two
    /// copies of it across time zones. Everything that lists nodes sorts their
    /// dates as plain strings, so the stamp has to reach the index in the same
    /// format the rest of them use.
    #[test]
    fn a_boards_rfc3339_save_stamp_reaches_the_index_in_the_local_format() {
        let (_holder, vault_path) = vault();
        let path = write(
            &vault_path,
            "Whiteboards/stamped.whiteboard.json",
            r#"{"title":"Stamped","metadata":{"updated_at":"2026-08-23T04:05:06Z"},
                "nodes":[],"edges":[]}"#,
        );

        let node = parse_file_to_node(&vault_path, &path).expect("board should parse");

        let expected = chrono::DateTime::parse_from_rfc3339("2026-08-23T04:05:06Z")
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(node.updated_at, expected);
    }

    /// Dragging a note onto a board is a link from the board to that note, and
    /// nothing recorded it: a board's indexed text is the labels its author
    /// typed, and a note card has none. The link has to come out of the file
    /// in the form the extractor reads, or the note has no backlink and the
    /// graph has no edge.
    #[test]
    fn a_note_card_on_a_board_becomes_a_link_the_extractor_can_read() {
        let (_holder, vault_path) = vault();
        let path = write(
            &vault_path,
            "Whiteboards/plan.whiteboard.json",
            r#"{"title":"Plan","nodes":[
                {"id":"n1","type":"note","position":{"x":0,"y":0},
                 "data":{"noteId":"Notes/công ty (cũ).md","noteTitle":"Công ty [cũ]"}},
                {"id":"n2","type":"text","position":{"x":0,"y":0},"data":{"label":"a label"}}
               ],"edges":[]}"#,
        );

        let node = parse_file_to_node(&vault_path, &path).expect("board should parse");

        // The note's title is part of what the board says, so the board is
        // findable by what is pinned to it.
        assert!(node.content.contains("Công ty"), "content: {}", node.content);
        assert!(node.content.contains("a label"), "content: {}", node.content);

        let edges = crate::utils::graph_parser::extract_node_edges(&node);
        let targets: Vec<&str> = edges
            .iter()
            .map(|e| e.target_title_or_path.as_str())
            .collect();
        assert!(
            targets.contains(&"Notes/công ty (cũ).md"),
            "the note card produced no link; got {targets:?}"
        );
    }

    /// The same note pinned twice is one link, and a board with no note cards
    /// carries no `linked_notes` at all rather than an empty list nothing can
    /// tell apart from a board whose links were dropped.
    #[test]
    fn note_links_are_deduplicated_and_absent_when_there_are_none() {
        let (_holder, vault_path) = vault();

        let twice = write(
            &vault_path,
            "Whiteboards/twice.whiteboard.json",
            r#"{"title":"Twice","nodes":[
                {"id":"a","type":"note","position":{"x":0,"y":0},"data":{"noteId":"Notes/one.md"}},
                {"id":"b","type":"note","position":{"x":9,"y":9},"data":{"noteId":"Notes/one.md"}}
               ],"edges":[]}"#,
        );
        let node = parse_file_to_node(&vault_path, &twice).expect("board should parse");
        let links = node.properties.get("linked_notes").expect("links recorded");
        assert_eq!(links.as_array().map(Vec::len), Some(1), "{links:?}");

        let bare = write(
            &vault_path,
            "Whiteboards/bare.whiteboard.json",
            r#"{"title":"Bare","nodes":[],"edges":[]}"#,
        );
        let node = parse_file_to_node(&vault_path, &bare).expect("board should parse");
        assert!(node.properties.get("linked_notes").is_none());
    }

    /// A stamp nobody can read is worse than no stamp: it would sort against
    /// real dates and win or lose arbitrarily. The file's own mtime is the
    /// honest answer, so the unreadable one is dropped before it is copied
    /// over the top of it.
    #[test]
    fn a_board_stamp_that_will_not_parse_leaves_the_file_time_standing() {
        let (_holder, vault_path) = vault();
        let path = write(
            &vault_path,
            "Whiteboards/broken.whiteboard.json",
            r#"{"title":"Broken","metadata":{"updated_at":"last Tuesday"},
                "nodes":[],"edges":[]}"#,
        );

        let node = parse_file_to_node(&vault_path, &path).expect("board should parse");

        assert_ne!(node.updated_at, "last Tuesday");
        // The mtime format, not the file's: %Y-%m-%d %H:%M:%S is 19 characters.
        assert_eq!(node.updated_at.len(), 19);
    }
}
