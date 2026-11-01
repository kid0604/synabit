//! Markup in a note's body that the editor will not keep.
//!
//! A write only says the file was written, and a read only gives back what the
//! file says. Neither says what the editor makes of it. On 2026-11-01 Syn wrote
//! `<div class="image-collection">`, which no part of the app knows, said two
//! photos were grouped, and when asked to check, read its own markup back and
//! said it was right. The editor drew two loose photos and dropped the `div` on
//! the next save.
//!
//! So the tools say it, as the arrangement `ambiguity` argues for: a thing the
//! model has to think of checking does not get checked.
//!
//! Only blocks that hold other blocks are looked at, and only the ones that give
//! the editor nothing to match on. A `data-` attribute is taken on trust, since
//! every extension's own block has one and the list of them changes; a warning
//! that fires on a note the app reads well teaches the model to ignore it.

/// The classes the editor reads a plain block by. `details-content` is what
/// toggles were saved with, read as part of their `<details>`.
const KNOWN_CLASSES: &[&str] = &["synabit-gallery", "details-content"];

fn without_code(body: &str) -> String {
    let code = regex::Regex::new(r"(?s)```.*?```|`[^`\n]*`|<!--.*?-->").expect("a valid pattern");
    code.replace_all(body, "").into_owned()
}

/// Each opening tag the editor will drop, as it was written, once each.
pub fn unknown_blocks(body: &str) -> Vec<String> {
    let tag = regex::Regex::new(r"(?i)<(?:div|section|aside|article|figure)\b[^>]*>").expect("a valid pattern");
    let class = regex::Regex::new(r#"(?i)\bclass\s*=\s*["']([^"']*)["']"#).expect("a valid pattern");
    let cleaned = without_code(body);

    let mut found: Vec<String> = Vec::new();
    for hit in tag.find_iter(&cleaned) {
        let written = hit.as_str();
        if written.to_lowercase().contains("data-") {
            continue;
        }
        let known = class
            .captures(written)
            .is_some_and(|c| c[1].split_whitespace().any(|name| KNOWN_CLASSES.contains(&name)));
        if !known && !found.iter().any(|f| f == written) {
            found.push(written.to_string());
        }
    }
    found
}

/// What a tool result says about such a body, if there is anything to say.
pub fn warning(body: &str) -> Option<String> {
    let blocks = unknown_blocks(body);
    if blocks.is_empty() {
        return None;
    }
    Some(format!(
        "The editor does not know {}: it shows what is inside as loose blocks and drops the tag on the next save. Whatever this was meant to do, it does not do. Do not tell the person it is right; say so, or rewrite it in a form the app knows.",
        blocks.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_nothing_in_the_app_reads_is_named() {
        let body = "Ban công\n\n<div class=\"image-collection\">\n\n![](assets/a.jpg)\n\n</div>";
        assert_eq!(unknown_blocks(body), vec!["<div class=\"image-collection\">"]);
        assert!(warning(body).expect("a warning").contains("image-collection"));
    }

    #[test]
    fn what_the_editor_reads_is_left_alone() {
        for body in [
            "Chỉ có chữ, và <span>một span</span>.",
            "<div class=\"synabit-gallery\" data-template=\"classic\">\n  <img src=\"assets/a.jpg\" />\n</div>",
            "<div class=\"synabit-gallery\">\n</div>",
            "<details>\n<summary>x</summary>\n<div class=\"details-content\">y</div>\n</details>",
            "<div data-type=\"whiteboard\"></div>",
            "<div data-pdf-embed=\"true\"></div>",
        ] {
            assert!(warning(body).is_none(), "{body}");
        }
    }

    /// A note about HTML is not HTML.
    #[test]
    fn markup_shown_as_code_is_not_markup() {
        assert!(warning("Dùng `<div class=\"x\">` nhé").is_none());
        assert!(warning("```html\n<div class=\"x\">\n</div>\n```").is_none());
        assert!(warning("<!-- <section> -->").is_none());
    }

    #[test]
    fn a_tag_used_twice_is_named_once() {
        assert_eq!(unknown_blocks("<div>a</div>\n<div>b</div>\n<section>c</section>"), vec!["<div>", "<section>"]);
    }

    /// The classes here are the editor's. If it stops reading one, a note using
    /// it is what this is for, and the warning must not stay quiet about it.
    #[test]
    fn the_known_classes_are_still_the_editors() {
        let gallery = include_str!("../../../src/mini-apps/note/extensions/ImageGallery.ts");
        assert!(gallery.contains("div.synabit-gallery"));
    }
}
