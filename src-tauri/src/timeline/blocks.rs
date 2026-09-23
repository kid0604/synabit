//! A note as the reader sees it: blocks of text, each with a day.
//!
//! The unit the moment reader works on is a **block**, not a file — a
//! paragraph, one item of a list with what is under it, a table. A file is
//! written over many days; a block is written at one sitting, which is what
//! lets it be put on the day it tells about. See §4 of
//! `docs/timeline-extract-v3-2026-09-22.md`.
//!
//! A block is known by the hash of its words once the markup is gone, so
//! reformatting a paragraph is not writing a new one. A block that is most of
//! an older block of the same note is that block, edited — see [`similar`].

use std::collections::HashMap;
use std::sync::LazyLock;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use regex::Regex;

/// One block of a note.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// What identifies it: its words, markup aside. See [`key`].
    pub hash: String,
    /// As written.
    pub raw: String,
    /// As it reads: what the model is given, and what a quote is checked against.
    pub text: String,
    /// Where it starts in the note's body, in characters.
    pub start: usize,
    /// The heading it sits under, as context. Not a block itself.
    pub heading: Option<String>,
    /// Whether this is the node's own name rather than a piece of its body.
    ///
    /// A task or a calendar entry says what it is in its title — *"Chốt
    /// checklist golive"* — and most have no body at all. So the title is read
    /// as a block. It is not anywhere in the body, so nothing points into the
    /// body for it (see `Evidence::span`).
    pub from_title: bool,
}

/// Anything shorter says nothing that could be a moment.
const SHORTEST: usize = 6;

/// No word of anybody's is longer. See [`split`].
const LONGEST_WORD: usize = 200;

/// A note's body split into blocks.
///
/// Paragraphs, each first-level list item with its children, tables, quotes and
/// HTML (a toggle, most often) are blocks. A heading labels what follows it.
/// Code is skipped: nobody's life is in a code fence.
pub fn split(body: &str) -> Vec<Block> {
    let char_at = char_offsets(body);
    let mut out = Vec::new();
    let mut heading: Option<String> = None;
    let mut depth = 0usize;
    let mut open: Option<(usize, bool)> = None; // (byte start, skip)
    let mut in_list = false;
    let mut item: Option<usize> = None;

    let push = |out: &mut Vec<Block>, range: std::ops::Range<usize>, heading: &Option<String>| {
        let raw = body[range.clone()].trim_end().to_string();
        let text = plain(&raw).0.trim().to_string();
        if text.chars().count() < SHORTEST {
            return;
        }
        // A run of hundreds of characters with no space in it is data — an
        // image pasted as base64, a token, a minified file — not writing. One
        // such block on the real vault was 534,192 characters long.
        if text.split_whitespace().any(|word| word.chars().count() > LONGEST_WORD) {
            return;
        }
        out.push(Block {
            hash: key(&text),
            raw,
            text,
            start: char_at.get(&range.start).copied().unwrap_or(0),
            heading: heading.clone(),
            from_title: false,
        });
    };

    for (event, range) in Parser::new_ext(body, Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    match tag {
                        Tag::List(_) => in_list = true,
                        Tag::Heading { .. } => {
                            heading = Some(plain(&body[range.clone()]).0.trim().to_string()).filter(|h| !h.is_empty());
                            open = Some((range.start, true));
                        }
                        Tag::CodeBlock(_) | Tag::MetadataBlock(_) => open = Some((range.start, true)),
                        _ => open = Some((range.start, false)),
                    }
                } else if depth == 1 && in_list && matches!(tag, Tag::Item) {
                    item = Some(range.start);
                }
                depth += 1;
            }
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if depth == 1 && in_list && matches!(end, TagEnd::Item) {
                    if let Some(start) = item.take() {
                        push(&mut out, start..range.end, &heading);
                    }
                } else if depth == 0 {
                    if in_list {
                        in_list = false;
                    } else if let Some((start, skip)) = open.take() {
                        if !skip {
                            push(&mut out, start..range.end, &heading);
                        }
                    }
                }
            }
            // HTML written between blocks, and text standing on its own.
            Event::Html(_) | Event::Text(_) if depth == 0 => push(&mut out, range, &heading),
            _ => {}
        }
    }
    out
}

fn char_offsets(text: &str) -> HashMap<usize, usize> {
    let mut map: HashMap<usize, usize> = text.char_indices().enumerate().map(|(n, (byte, _))| (byte, n)).collect();
    map.insert(text.len(), text.chars().count());
    map
}

/// What identifies a block: its words, whitespace and case aside.
pub fn key(text: &str) -> String {
    let tidy = text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    blake3::hash(tidy.as_bytes()).to_hex()[..24].to_string()
}

/// How much of two blocks is the same words, from 0 to 1.
///
/// Dice over the words, counted with repeats. A block that is 80% an older
/// block is that block with a typo fixed or a clause added, and reading it
/// again would propose again what was already proposed (§4.1).
pub fn similar(a: &str, b: &str) -> f64 {
    let words = |s: &str| -> HashMap<String, usize> {
        let mut bag = HashMap::new();
        for word in s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
            *bag.entry(word.to_lowercase()).or_insert(0) += 1;
        }
        bag
    };
    let (a, b) = (words(a), words(b));
    let total: usize = a.values().sum::<usize>() + b.values().sum::<usize>();
    if total == 0 {
        return 0.0;
    }
    let shared: usize = a.iter().map(|(word, n)| (*n).min(b.get(word).copied().unwrap_or(0))).sum();
    2.0 * shared as f64 / total as f64
}

/// Close enough to be the same block, edited.
pub const SAME_BLOCK: f64 = 0.8;

/// Everything markdown adds to the words: links (as their label), HTML tags,
/// list and quote markers and heading hashes at the start of a line, emphasis.
static MARKUP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)(?P<wiki>\[\[(?P<target>[^\]|]+)(?:\|(?P<label>[^\]]+))?\]\])|(?P<md>!?\[(?P<text>[^\]]*)\]\([^)]*\))|(?P<tag></?[A-Za-z!][^>]*>)|(?P<marker>^[ \t]*(?:[-*+]|\d+[.)])[ \t]+(?:\[[ xX]\][ \t]+)?|^[ \t]*>[ \t]?|^#{1,6}[ \t]+)|(?P<emphasis>\*\*|__|~~|`|\*)",
    )
    .expect("pattern")
});

/// A piece of markdown as it reads, and for each character of that the
/// character of the markdown it came from.
///
/// A link reads as its label: `[Phan Hương Lê](synabit://…)` is "Phan Hương
/// Lê" to anyone reading it, and to the model, which quotes it that way.
pub fn plain(text: &str) -> (String, Vec<usize>) {
    let char_at = char_offsets(text);
    let mut out = String::new();
    let mut from = Vec::new();
    let keep = |range: std::ops::Range<usize>, out: &mut String, from: &mut Vec<usize>| {
        for (offset, c) in text[range.clone()].char_indices() {
            out.push(c);
            from.push(char_at[&(range.start + offset)]);
        }
    };
    let mut at = 0;
    for found in MARKUP.captures_iter(text) {
        let whole = found.get(0).expect("a match");
        keep(at..whole.start(), &mut out, &mut from);
        if let Some(label) = found.name("label").or_else(|| found.name("target")).or_else(|| found.name("text")) {
            keep(label.range(), &mut out, &mut from);
        }
        at = whole.end();
    }
    keep(at..text.len(), &mut out, &mut from);
    (out, from)
}

/// A node's own name, as a block: what a task or a calendar entry says.
pub fn of_title(title: &str) -> Option<Block> {
    let text = plain(title).0.trim().to_string();
    if text.chars().count() < SHORTEST {
        return None;
    }
    Some(Block { hash: key(&text), raw: title.trim().to_string(), text, start: 0, heading: None, from_title: true })
}

/// The body of a file: what follows its frontmatter.
pub fn body_of(file: &str) -> &str {
    let Some(rest) = file.strip_prefix("---\n").or_else(|| file.strip_prefix("---\r\n")) else {
        return file;
    };
    match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            after.strip_prefix("\r\n").or_else(|| after.strip_prefix('\n')).unwrap_or(after)
        }
        None => file,
    }
}

/// When each block of a note was first there, from its Loro history.
#[derive(Debug, Default, Clone)]
pub struct History {
    /// Block hash → unix milliseconds of the first version holding it.
    pub first: HashMap<String, i64>,
    /// Block hash → its text, for every block the note has ever had.
    pub text: HashMap<String, String>,
    /// When the history begins. A block already there then may be much older.
    pub began: Option<i64>,
}

impl History {
    /// Read from a note's document. Walks every version, so linear in versions
    /// and in size: 0.36 s for the 425 notes of the vault this was measured on.
    pub fn of(doc: &loro::LoroDoc) -> History {
        let mut marks: Vec<(u32, loro::ID, i64)> = doc.with_oplog(|oplog| {
            let mut marks = Vec::new();
            for changes in oplog.changes().values() {
                for change in changes.iter() {
                    let last = loro::ID {
                        peer: change.id().peer,
                        counter: change.id().counter + change.ops().atom_len() - 1,
                    };
                    marks.push((change.lamport(), last, change.timestamp()));
                }
            }
            marks
        });
        marks.sort_by_key(|(lamport, id, _)| (*lamport, id.peer, id.counter));

        let mut history = History::default();
        for (_, id, stamp) in marks {
            if doc.checkout(&id.into()).is_err() {
                continue;
            }
            // A change from before the app kept time has no stamp; it is at
            // least as old as the first one that does.
            if stamp > 0 && history.began.is_none() {
                history.began = Some(stamp);
            }
            let file = crate::sync::core::crdt::node_text(doc);
            for block in split(body_of(&file)) {
                if stamp > 0 {
                    history.first.entry(block.hash.clone()).or_insert(stamp);
                }
                history.text.entry(block.hash).or_insert(block.text);
            }
        }
        doc.checkout_to_latest();
        history
    }

    /// Whether a block was there when the history began, and so may be older
    /// than anything the history can say.
    pub fn from_the_start(&self, hash: &str) -> bool {
        matches!((self.first.get(hash), self.began), (Some(first), Some(began)) if first == &began)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_splits_into_the_blocks_a_reader_sees() {
        let body = "## Công việc\n\nChiều họp UAT v2 với chị Yến.\n\n- Trưa ăn bún chả với Nga, 50k\n  - quán Hàng Mành\n- Tối đưa Cam đi bơi\n\n```\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let blocks = split(body);
        let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts[0], "Chiều họp UAT v2 với chị Yến.");
        assert!(texts[1].starts_with("Trưa ăn bún chả với Nga, 50k") && texts[1].contains("quán Hàng Mành"), "{texts:?}");
        assert_eq!(texts[2], "Tối đưa Cam đi bơi");
        assert!(!texts.iter().any(|t| t.contains("let x")), "code is not a life: {texts:?}");
        assert!(texts.iter().any(|t| t.contains('|') || t.contains('1')), "a table is a block: {texts:?}");
        assert!(blocks.iter().all(|b| b.heading.as_deref() == Some("Công việc")), "{blocks:?}");
        // Where it starts, for marking it in the note.
        let at: String = body.chars().skip(blocks[0].start).take(5).collect();
        assert_eq!(at, "Chiều");
    }

    #[test]
    fn a_block_is_its_words_not_its_markup() {
        let a = split("Gặp **[Phan Hương Lê](synabit://x)** ở quán.");
        let b = split("Gặp Phan Hương Lê   ở quán.");
        assert_eq!(a[0].text, "Gặp Phan Hương Lê ở quán.");
        assert_eq!(a[0].hash, b[0].hash);
    }

    #[test]
    fn a_small_edit_is_the_same_block_and_a_new_thought_is_not() {
        let before = "Chiều họp UAT v2 với chị Yến, chốt lại ngày làm việc trên giao diện.";
        assert!(similar(before, "Chiều họp UAT v2 với chị Yến, chốt lại ngày làm việc trên giao diện mới.") >= SAME_BLOCK);
        assert!(similar(before, "Tối đưa Cam đi bơi ở bể Hoàng Mai.") < SAME_BLOCK);
    }

    #[test]
    fn plain_text_says_where_each_character_came_from() {
        let text = "- [ ] Gọi [[People/me.md|mẹ]] lúc <b>9h</b>";
        let (reads, from) = plain(text);
        assert_eq!(reads, "Gọi mẹ lúc 9h");
        let at = reads.find("mẹ").unwrap();
        let n = reads[..at].chars().count();
        let original: String = text.chars().skip(from[n]).take(2).collect();
        assert_eq!(original, "mẹ");
    }

    #[test]
    fn data_pasted_into_a_note_is_not_a_block() {
        let blob = "A".repeat(5_000);
        let blocks = split(&format!("Hôm nay chụp ảnh.\n\n{blob}\n\nTối ăn phở."));
        let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts, vec!["Hôm nay chụp ảnh.", "Tối ăn phở."]);
    }

    #[test]
    fn the_body_is_what_follows_the_frontmatter() {
        assert_eq!(body_of("---\ntitle: a\n---\nHôm nay"), "Hôm nay");
        assert_eq!(body_of("Không có frontmatter"), "Không có frontmatter");
    }
}
