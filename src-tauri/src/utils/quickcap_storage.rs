//! The one-time repair that puts a cap's tags and colour where they belong.
//!
//! Two defects, both of which put a styling or display detail into the
//! user's own Markdown and left the real value somewhere the rest of the
//! app never looks:
//!
//! **Tags.** Every cap was created with `tags: []` in frontmatter and never
//! updated. The actual tags lived only as `#word` inside the body, re-read
//! by a regex on every render. Meanwhile the tag manager queries
//! `json_each(properties, '$.tags')` and the FTS index fills its `tags`
//! column from the same place — so a vault with two hundred tagged caps
//! reported zero tags and `#tag` search never matched one.
//!
//! **Colour.** A card's colour was stored as a *Tailwind class string* —
//! `bg-red-50 dark:bg-red-950/30` — sometimes in frontmatter, sometimes as
//! an `<!--color:…-->` comment at the top of the body, and sometimes both,
//! disagreeing. Restyling the app would have broken every cap's colour, and
//! anyone opening the vault in another editor saw the comment.
//!
//! # The rules this has to obey
//!
//! Everything here is a pure function of the file's bytes: no clock, no new
//! identifiers, no dependence on which file is visited first. That is not
//! tidiness, it is the whole reason the repair can run on each device
//! independently without the two of them fighting. `commands::migration`
//! explains why at length.
//!
//! In particular `created_at` and `updated_at` are carried across
//! untouched, which is also what keeps the user's list in the order they
//! left it — QuickCap sorts on `updated_at`.

use serde::Deserialize;
use std::sync::LazyLock;

use crate::utils::tag_grammar::extract_tags;

#[derive(Deserialize)]
struct Colour {
    name: String,
    class: String,
}

#[derive(Deserialize)]
struct ColourTable {
    colours: Vec<Colour>,
}

/// Embedded at compile time so moving the fixture breaks the build rather
/// than silently leaving colours unmigrated.
static COLOURS: LazyLock<Vec<Colour>> = LazyLock::new(|| {
    let raw = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../contracts/quickcap-colours.json"
    ));
    let table: ColourTable = serde_json::from_str(raw).expect("colour fixture parses");
    table.colours
});

/// The semantic name for a stored value.
///
/// Accepts a name already in the new shape (so the repair is idempotent) or
/// a legacy Tailwind class. Anything else returns `None`, and the caller
/// leaves the original value alone rather than guessing.
fn colour_name(stored: &str) -> Option<&'static str> {
    let stored = stored.trim();
    if stored.is_empty() {
        return None;
    }
    COLOURS.iter().find_map(|c| {
        if c.name == stored || c.class == stored {
            // The fixture is 'static for the process; this borrow is of the
            // LazyLock's contents, which outlive every caller.
            let name: &'static str = Box::leak(c.name.clone().into_boxed_str());
            Some(name)
        } else {
            None
        }
    })
}

/// Split `---\n…\n---\n<body>` into its frontmatter lines and its body.
///
/// A cap without well-formed frontmatter is not repaired at all. There is
/// nothing safe to do with a file whose shape we cannot read, and skipping
/// it leaves the user's bytes exactly as they were.
fn split_frontmatter(contents: &str) -> Option<(Vec<String>, &str)> {
    let rest = contents.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let block = &rest[..end];
    let body = &rest[end + 5..];
    Some((block.lines().map(str::to_string).collect(), body))
}

/// How far a frontmatter key extends: its own line, plus any block-sequence
/// or indented continuation lines beneath it.
fn key_extent(lines: &[String], key: &str) -> Option<(usize, usize)> {
    let prefix = format!("{key}:");
    let start = lines.iter().position(|l| l.starts_with(&prefix))?;
    let mut end = start;
    while end + 1 < lines.len() {
        let next = &lines[end + 1];
        if next.starts_with("- ") || next.starts_with(' ') || next.starts_with('\t') {
            end += 1;
        } else {
            break;
        }
    }
    Some((start, end))
}

/// Replace a key's lines, or append it, or remove it — keeping every other
/// line of frontmatter byte-identical.
///
/// Surgical rather than re-serialising the whole block, because
/// re-serialising would reorder keys, requote strings and silently drop any
/// field this code does not know about.
fn set_key(lines: &mut Vec<String>, key: &str, rendered: Option<Vec<String>>) {
    match (key_extent(lines, key), rendered) {
        (Some((start, end)), Some(new_lines)) => {
            lines.splice(start..=end, new_lines);
        }
        (Some((start, end)), None) => {
            lines.drain(start..=end);
        }
        (None, Some(new_lines)) => {
            lines.extend(new_lines);
        }
        (None, None) => {}
    }
}

/// A YAML scalar's value, unquoted.
fn scalar_value(line: &str) -> String {
    let after = line.split_once(':').map(|(_, v)| v).unwrap_or("").trim();
    after
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// Render `tags:` the way serde_yaml would, so a later ordinary write does
/// not churn the file back and forth between two spellings.
fn render_tags(tags: &[String]) -> Vec<String> {
    if tags.is_empty() {
        return vec!["tags: []".to_string()];
    }
    let mut out = vec!["tags:".to_string()];
    for tag in tags {
        let scalar = serde_yaml::to_string(tag)
            .unwrap_or_else(|_| format!("{tag}\n"))
            .trim_end()
            .to_string();
        out.push(format!("- {scalar}"));
    }
    out
}

/// The repaired contents of one cap, or `None` if it is already current.
///
/// Returning `None` rather than identical bytes is what lets the writer skip
/// the file entirely: no rewrite, no changed mtime, nothing.
pub fn migrate_cap(contents: &str) -> Option<String> {
    let (mut front, body) = split_frontmatter(contents)?;

    // The colour comment is a display detail that leaked into the body.
    // Read it before removing it: on a cap that has no frontmatter colour,
    // this is where the user's choice actually is.
    let mut comment_colour = None;
    let mut cleaned_body = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(open) = rest.find("<!--color:") {
        let after = &rest[open + "<!--color:".len()..];
        let Some(close) = after.find("-->") else {
            break;
        };
        if comment_colour.is_none() {
            comment_colour = Some(after[..close].to_string());
        }
        cleaned_body.push_str(&rest[..open]);
        rest = &after[close + 3..];
        rest = rest.strip_prefix('\n').unwrap_or(rest);
    }
    cleaned_body.push_str(rest);

    // Frontmatter wins over the comment, because that is the precedence the
    // card already renders with: `mapNodeToQuickCap` reads properties.color.
    let front_colour = key_extent(&front, "color")
        .map(|(start, _)| scalar_value(&front[start]))
        .filter(|v| !v.is_empty());

    let resolved = front_colour
        .as_deref()
        .or(comment_colour.as_deref())
        .unwrap_or("");

    let colour_line = match colour_name(resolved) {
        Some(name) => Some(vec![format!("color: {name}")]),
        // Unreadable but present: keep the user's bytes rather than guess.
        None if !resolved.trim().is_empty() => Some(vec![format!("color: {}", resolved.trim())]),
        None => None,
    };

    let tags = extract_tags(&cleaned_body);

    set_key(&mut front, "tags", Some(render_tags(&tags)));
    set_key(&mut front, "color", colour_line);

    let repaired = format!("---\n{}\n---\n{}", front.join("\n"), cleaned_body);

    if repaired == contents {
        None
    } else {
        Some(repaired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(front: &str, body: &str) -> String {
        format!("---\nnode_id: n1\ntitle: \"a cap\"\ntype: \"quickcap\"\n{front}created_at: \"2026-01-01 08:00:00\"\nupdated_at: \"2026-02-02 09:30:00\"\n---\n{body}")
    }

    fn front_of(contents: &str) -> String {
        split_frontmatter(contents).unwrap().0.join("\n")
    }

    fn body_of(contents: &str) -> String {
        split_frontmatter(contents).unwrap().1.to_string()
    }

    #[test]
    fn lifts_body_tags_into_frontmatter() {
        let before = cap("tags: []\n", "họp về #dự-án và #ngân-sách\n");
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("tags:\n- dự-án\n- ngân-sách"));
    }

    /// The body is the user's writing. The repair moves metadata out of it
    /// and must not touch a character of the prose.
    #[test]
    fn leaves_the_body_alone() {
        let body = "họp về #dự-án\n\nghi chú thêm ở đoạn hai\n";
        let after = migrate_cap(&cap("tags: []\n", body)).expect("changed");
        assert_eq!(body_of(&after), body);
    }

    #[test]
    fn keeps_created_and_updated_exactly() {
        let after = migrate_cap(&cap("tags: []\n", "#thẻ\n")).expect("changed");
        let front = front_of(&after);
        assert!(front.contains("created_at: \"2026-01-01 08:00:00\""));
        assert!(front.contains("updated_at: \"2026-02-02 09:30:00\""));
    }

    #[test]
    fn keeps_fields_it_knows_nothing_about() {
        let before = cap("tags: []\nsource_link: \"QuickCaps/x.md\"\n", "#thẻ\n");
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("source_link: \"QuickCaps/x.md\""));
        assert!(front_of(&after).contains("node_id: n1"));
    }

    #[test]
    fn turns_a_tailwind_class_into_a_name() {
        let before = cap("tags: []\ncolor: bg-red-50 dark:bg-red-950/30\n", "#thẻ\n");
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("color: red"));
    }

    #[test]
    fn lifts_a_colour_out_of_the_body_comment() {
        let before = cap(
            "tags: []\n",
            "<!--color:bg-blue-50 dark:bg-blue-950/30-->\nnội dung #thẻ\n",
        );
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("color: blue"));
        assert!(!after.contains("<!--color:"), "the comment must be gone");
        assert_eq!(body_of(&after), "nội dung #thẻ\n");
    }

    /// The two stores disagreeing is a real state on disk, produced by the
    /// bug where a colour change wrote one shape and cached the other.
    #[test]
    fn frontmatter_beats_the_body_comment() {
        let before = cap(
            "tags: []\ncolor: bg-green-50 dark:bg-green-950/30\n",
            "<!--color:bg-pink-50 dark:bg-pink-950/30-->\nnội dung\n",
        );
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("color: green"));
    }

    /// A value outside the table is hand-written or from a future palette.
    /// Guessing would lose it; keeping it costs nothing.
    ///
    /// The body carries a tag so the repair has a reason to rewrite the file
    /// at all — an unreadable colour on its own is already the target shape,
    /// which the sibling test below states directly.
    #[test]
    fn keeps_a_colour_it_cannot_read() {
        let before = cap("tags: []\ncolor: bg-teal-50\n", "nội dung #thẻ\n");
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("color: bg-teal-50"));
        assert!(front_of(&after).contains("- thẻ"));
    }

    /// An unreadable colour is not, by itself, something to repair. Left
    /// alone means left alone: no rewrite, no changed mtime, no sync churn.
    #[test]
    fn an_unreadable_colour_alone_is_not_a_reason_to_rewrite() {
        let before = cap("tags: []\ncolor: bg-teal-50\n", "nội dung\n");
        assert!(migrate_cap(&before).is_none());
    }

    #[test]
    fn adds_no_colour_where_there_was_none() {
        let after = migrate_cap(&cap("tags: []\n", "#thẻ\n")).expect("changed");
        assert!(!front_of(&after).contains("color:"));
    }

    /// The property the writer relies on to skip files, and the reason a
    /// half-finished run can simply be started again.
    #[test]
    fn is_idempotent() {
        let before = cap(
            "tags: []\ncolor: bg-red-50 dark:bg-red-950/30\n",
            "<!--color:bg-red-50 dark:bg-red-950/30-->\nhọp #dự-án\n",
        );
        let once = migrate_cap(&before).expect("first pass changes it");
        assert!(
            migrate_cap(&once).is_none(),
            "second pass must find nothing to do"
        );
    }

    #[test]
    fn adds_a_tags_key_that_was_missing() {
        let before = cap("", "#thẻ\n");
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("tags:\n- thẻ"));
    }

    #[test]
    fn writes_an_empty_list_for_a_cap_with_no_tags() {
        let before = cap("", "chỉ là văn bản\n");
        let after = migrate_cap(&before).expect("changed");
        assert!(front_of(&after).contains("tags: []"));
    }

    #[test]
    fn replaces_a_stale_tag_list_rather_than_appending() {
        let before = cap("tags:\n- cũ\n- rất-cũ\n", "#mới\n");
        let after = migrate_cap(&before).expect("changed");
        let front = front_of(&after);
        assert!(front.contains("- mới"));
        assert!(!front.contains("- cũ"));
        assert!(!front.contains("- rất-cũ"));
    }

    /// The grammar is shared with the front end and with the migration's own
    /// fixture; this only pins that the repair actually consults it.
    #[test]
    fn uses_the_shared_grammar_not_a_looser_one() {
        let before = cap(
            "tags: []\n",
            "đổi #ff0000 thành xanh, xem `#define`, ghi #thật\n",
        );
        let after = migrate_cap(&before).expect("changed");
        let front = front_of(&after);
        assert!(front.contains("- thật"));
        assert!(!front.contains("ff0000"));
        assert!(!front.contains("define"));
    }

    #[test]
    fn skips_a_file_with_no_frontmatter() {
        assert!(migrate_cap("chỉ có nội dung, không có frontmatter\n").is_none());
    }

    #[test]
    fn skips_a_file_whose_frontmatter_never_closes() {
        assert!(migrate_cap("---\ntitle: hỏng\nkhông đóng\n").is_none());
    }

    /// Determinism, stated as a test rather than as a comment: the same
    /// bytes in, the same bytes out, every time and on every device.
    #[test]
    fn is_a_function_of_its_input_alone() {
        let before = cap(
            "tags: []\ncolor: bg-pink-50 dark:bg-pink-950/30\n",
            "#a #b\n",
        );
        let first = migrate_cap(&before).unwrap();
        for _ in 0..5 {
            assert_eq!(migrate_cap(&before).unwrap(), first);
        }
    }
}
