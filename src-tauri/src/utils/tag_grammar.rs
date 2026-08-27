//! QuickCap's tag grammar, on the Rust side.
//!
//! There are two implementations of this grammar and there have to be. The
//! app shows a cap's tags while the user is still typing, which happens in
//! TypeScript (`src/mini-apps/quickcap/parsing.ts`); the migration reads
//! them off bytes on disk, which happens here. Two implementations that
//! drift apart produce the worst possible outcome — a vault where the tags
//! displayed are not the tags stored.
//!
//! So neither side owns the grammar. `contracts/tag-grammar.json` does, and
//! both test suites run every case in it. A disagreement is a failing test
//! rather than a support ticket.
//!
//! # Why this is hand-written rather than a regex
//!
//! The TypeScript version leans on lookahead to say "a tag ends at
//! whitespace or sentence punctuation, and the punctuation is not part of
//! the tag". Rust's `regex` crate has no lookahead by design, and the
//! rewrites that work around it — consuming the delimiter, then losing the
//! next tag because its leading space was eaten — are exactly the kind of
//! subtle mismatch the fixture exists to prevent.
//!
//! A scanner is longer but says what it means. It also makes the two
//! implementations genuinely independent, which is what gives their
//! agreement any evidential weight at all.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// The legacy `<!--color:…-->` marker, which is not prose and not a tag.
/// `.` excludes newlines in Rust's regex by default, matching the
/// TypeScript pattern this mirrors.
static COLOR_COMMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!--color:.*?-->\n?").unwrap());

/// What may follow a tag without being part of it.
///
/// Sentence punctuation is here so `họp về #dự-án.` still carries a tag.
fn is_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']')
}

/// What may appear after a tag's opening letter.
fn is_tag_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// 3, 4, 6 or 8 hex digits: the shapes CSS accepts for a colour.
///
/// Needed because "must start with a letter" does not catch `#fff` or
/// `#ff0000` — `f` is a letter. The cost is that `#cafe` and `#facade`
/// read as colours; see the fixture, where that is pinned as a decision.
fn is_hex_colour(value: &str) -> bool {
    matches!(value.chars().count(), 3 | 4 | 6 | 8) && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// A `#` only opens a tag at the start of the text or after whitespace.
/// This is what keeps `C#` and `A#1` from being read as tags.
fn opens_token(chars: &[char], hash: usize) -> bool {
    hash == 0 || chars[hash - 1].is_whitespace()
}

/// Trim separators that ran off the end, so `#dự-án-` yields `dự-án`
/// rather than yielding nothing.
fn clean(raw: &str) -> &str {
    raw.trim().trim_end_matches(['-', '_'])
}

/// Blank a range in place, preserving length so nothing after it shifts.
fn blank(chars: &mut [char], from: usize, to_inclusive: usize) {
    for c in chars.iter_mut().take(to_inclusive + 1).skip(from) {
        *c = ' ';
    }
}

/// Replace paired ``` or ~~~ fences, and everything between them, with spaces.
fn mask_paired_fences(chars: &mut [char], marker: char) {
    let is_fence = |chars: &[char], i: usize| {
        i + 2 < chars.len() && chars[i..i + 3].iter().all(|&c| c == marker)
    };

    let mut i = 0;
    while i < chars.len() {
        if is_fence(chars, i) {
            let mut j = i + 3;
            let mut closed = None;
            while j < chars.len() {
                if is_fence(chars, j) {
                    closed = Some(j);
                    break;
                }
                j += 1;
            }
            match closed {
                Some(j) => {
                    blank(chars, i, j + 2);
                    i = j + 3;
                    continue;
                }
                // Unterminated: leave it to the inline pass and the
                // trailing-fence pass, exactly as the TypeScript order does.
                None => break,
            }
        }
        i += 1;
    }
}

/// Replace `inline code` spans with spaces. A span never crosses a newline.
fn mask_inline_code(chars: &mut [char]) {
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '`' {
            let mut j = i + 1;
            let mut closed = None;
            while j < chars.len() && chars[j] != '\n' {
                if chars[j] == '`' {
                    closed = Some(j);
                    break;
                }
                j += 1;
            }
            if let Some(j) = closed {
                blank(chars, i, j);
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
}

/// A fence the user opened and has not closed yet — the normal state of a
/// note being typed. Everything from it to the end is code.
fn mask_trailing_fence(chars: &mut [char]) {
    let len = chars.len();
    for i in 0..len {
        if i + 2 < len && chars[i..i + 3].iter().all(|&c| c == '`') {
            blank(chars, i, len - 1);
            return;
        }
    }
}

fn mask_code(chars: &mut [char]) {
    mask_paired_fences(chars, '`');
    mask_paired_fences(chars, '~');
    mask_inline_code(chars);
    mask_trailing_fence(chars);
}

/// Read `#nhiều chữ#` starting at `hash`. Returns the tag and the index of
/// its closing hash.
///
/// The scan stops dead at the first `#` it finds: a wrapped tag cannot
/// contain one, so if that `#` is not followed by a delimiter there is no
/// longer match to try.
fn read_wrapped(chars: &[char], hash: usize) -> Option<(String, usize)> {
    if !chars.get(hash + 1)?.is_alphabetic() {
        return None;
    }
    let mut j = hash + 2;
    while j < chars.len() && chars[j] != '#' && chars[j] != '\n' {
        j += 1;
    }
    if chars.get(j) != Some(&'#') {
        return None;
    }
    match chars.get(j + 1) {
        None => {}
        Some(&c) if is_delimiter(c) => {}
        Some(_) => return None,
    }
    let raw: String = chars[hash + 1..j].iter().collect();
    let tag = clean(&raw);
    if tag.is_empty() || is_hex_colour(tag) {
        return None;
    }
    Some((tag.to_string(), j))
}

/// Read `#word` starting at `hash`. Returns the tag and the index of its
/// last character.
fn read_simple(chars: &[char], hash: usize) -> Option<(String, usize)> {
    if !chars.get(hash + 1)?.is_alphabetic() {
        return None;
    }
    let mut end = hash + 1;
    while end + 1 < chars.len() && is_tag_char(chars[end + 1]) {
        end += 1;
    }
    match chars.get(end + 1) {
        None => {}
        Some(&c) if is_delimiter(c) => {}
        Some(_) => return None,
    }
    let raw: String = chars[hash + 1..=end].iter().collect();
    let tag = clean(&raw);
    if tag.is_empty() || is_hex_colour(tag) {
        return None;
    }
    Some((tag.to_string(), end))
}

/// Every distinct tag in a cap, in the order it first appears.
///
/// Wrapped tags are read first and blanked out, so the closing hash of
/// `#nhiều chữ#` can never be picked up as the start of a second tag. That
/// also fixes the output order — wrapped tags precede plain ones, which is
/// what the TypeScript side does and what the fixture records.
pub fn extract_tags(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }

    let stripped = COLOR_COMMENT.replace_all(content, "");
    let mut chars: Vec<char> = stripped.chars().collect();
    mask_code(&mut chars);

    let mut tags: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let push = |tag: String, tags: &mut Vec<String>, seen: &mut HashSet<String>| {
        if seen.insert(tag.clone()) {
            tags.push(tag);
        }
    };

    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#' && opens_token(&chars, i) {
            if let Some((tag, end)) = read_wrapped(&chars, i) {
                push(tag, &mut tags, &mut seen);
                blank(&mut chars, i, end);
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#' && opens_token(&chars, i) {
            if let Some((tag, end)) = read_simple(&chars, i) {
                push(tag, &mut tags, &mut seen);
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        input: String,
        tags: Vec<String>,
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    /// Embedded at compile time, so moving or deleting the fixture breaks
    /// the build rather than quietly leaving the grammar untested.
    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../contracts/tag-grammar.json"
    ));

    /// The whole point of the fixture: this assertion and the one in
    /// `parsing.spec.ts` read the same cases, so the two implementations
    /// cannot drift without one of them going red.
    #[test]
    fn matches_the_shared_grammar_fixture() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("fixture parses");
        assert!(
            fixture.cases.len() >= 30,
            "fixture looks truncated: {} cases",
            fixture.cases.len()
        );

        let mut failures = Vec::new();
        for case in &fixture.cases {
            let got = extract_tags(&case.input);
            if got != case.tags {
                failures.push(format!(
                    "  {}\n    input:    {:?}\n    expected: {:?}\n    got:      {:?}",
                    case.name, case.input, case.tags, got
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "{} of {} grammar cases disagree with the fixture:\n{}",
            failures.len(),
            fixture.cases.len(),
            failures.join("\n")
        );
    }
}
