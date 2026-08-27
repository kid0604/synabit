use ammonia::{Builder, UrlRelative};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Attributes a lazy-loading image keeps its real address in, best first.
const LAZY_SRC_ATTRS: &[&str] = &[
    "data-src",
    "data-original",
    "data-lazy-src",
    "data-lazy",
    "data-echo",
];

static IMG_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<img\b[^>]*>").expect("a static pattern"));

static ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)([a-z0-9_:.-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#)
        .expect("a static pattern")
});

/// Move a lazy-loaded image's real address into `src`.
///
/// Half the web ships images as
/// `<img class="lazy" src="data:image/gif;base64,…" data-src="https://…">`:
/// the `src` is a one-pixel placeholder and a script swaps in `data-src` when
/// the image scrolls near. There is no script here, and `data:` is not a
/// scheme the sanitizer allows, so both attributes were dropped and the
/// article was left with `<img alt="…">` — an empty box under a caption
/// describing the picture that should have been in it.
///
/// Runs before sanitizing, never after: whatever this produces still has to
/// get past the sanitizer, which is what makes rewriting tags with a pattern
/// acceptable here. It cannot widen what is allowed, only change which URL is
/// offered.
fn promote_lazy_images(html: &str) -> String {
    if !html.contains("<img") && !html.contains("<IMG") {
        return html.to_string();
    }

    IMG_TAG
        .replace_all(html, |caps: &regex::Captures| {
            let tag = &caps[0];
            let mut attrs: Vec<(String, String)> = Vec::new();
            for attr in ATTR.captures_iter(tag) {
                let name = attr[1].to_ascii_lowercase();
                let value = attr
                    .get(2)
                    .or_else(|| attr.get(3))
                    .or_else(|| attr.get(4))
                    .map(|m| m.as_str())
                    .unwrap_or_default();
                attrs.push((name, value.to_string()));
            }

            let get = |wanted: &str| -> Option<&str> {
                attrs
                    .iter()
                    .find(|(name, _)| name == wanted)
                    .map(|(_, value)| value.as_str())
                    .filter(|value| !value.trim().is_empty())
            };

            let current = get("src").unwrap_or_default();
            // A placeholder is anything the page cannot have meant: a data URI
            // standing in for the real picture, or no source at all.
            let placeholder = current.is_empty() || current.trim_start().starts_with("data:");

            let real = LAZY_SRC_ATTRS
                .iter()
                .find_map(|attr| get(attr))
                .or_else(|| get("data-srcset").and_then(largest_in_srcset))
                .or_else(|| get("srcset").and_then(largest_in_srcset));

            let src = match (placeholder, real) {
                (true, Some(found)) => found,
                _ => current,
            };

            // Rebuilt rather than patched: the sanitizer keeps these three
            // attributes on an image and drops everything else anyway, so
            // carrying the rest through would only be work.
            let mut rebuilt = String::from("<img");
            if !src.is_empty() {
                rebuilt.push_str(&format!(" src=\"{}\"", escape_attr(src)));
            }
            if let Some(alt) = get("alt") {
                rebuilt.push_str(&format!(" alt=\"{}\"", escape_attr(alt)));
            }
            if let Some(title) = get("title") {
                rebuilt.push_str(&format!(" title=\"{}\"", escape_attr(title)));
            }
            rebuilt.push('>');
            rebuilt
        })
        .into_owned()
}

/// The widest candidate in a `srcset`, which is the one worth caching.
fn largest_in_srcset(srcset: &str) -> Option<&str> {
    let mut best: Option<(&str, f64)> = None;

    for candidate in srcset.split(',') {
        let mut parts = candidate.split_whitespace();
        let Some(url) = parts.next() else { continue };
        // `800w`, `2x`, or nothing at all — a bare URL is the 1x candidate.
        let weight = parts
            .next()
            .and_then(|d| d.trim_end_matches(['w', 'x']).parse::<f64>().ok())
            .unwrap_or(1.0);

        if best.is_none_or(|(_, seen)| weight > seen) {
            best = Some((url, weight));
        }
    }

    best.map(|(url, _)| url)
}

/// Make an attribute value safe to sit inside double quotes.
///
/// Only the quote is escaped. The value came out of HTML that was already
/// serialized once — readability hands back `inner_html()`, where `&` is
/// already `&amp;` — so escaping ampersands here would encode them a second
/// time and turn `?w=1020&amp;h=0` into a URL with a literal `&amp;` in it.
fn escape_attr(value: &str) -> String {
    value.replace('"', "&quot;")
}

/// Sanitize HTML content, keeping only safe tags and attributes.
/// Used for article body content.
///
/// `base_url` is the page the HTML came from. Feeds routinely write their
/// links and images as site-relative paths — `<img src="/img/hero.png">` — and
/// nothing downstream can resolve those: the reader renders inside the app's
/// own origin, so a relative path points at the app, not at the publisher.
/// Rewriting them here means every consumer of this function is fixed at once,
/// and it costs nothing when a feed already writes absolute URLs.
///
/// A `base_url` that will not parse leaves relative URLs as they were. That is
/// the old behaviour, which is wrong but no more wrong than before, and it is
/// better than dropping the attribute and losing the link entirely.
pub fn sanitize_html(html: &str, base_url: &str) -> String {
    let mut allowed_tags = HashSet::new();
    for tag in &[
        "p",
        "a",
        "img",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "ul",
        "ol",
        "li",
        "blockquote",
        "pre",
        "code",
        "em",
        "strong",
        "br",
        "hr",
        "figure",
        "figcaption",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
    ] {
        allowed_tags.insert(*tag);
    }

    let mut allowed_attrs = std::collections::HashMap::new();
    let link_attrs: HashSet<&str> = ["href", "title"].iter().copied().collect();
    let img_attrs: HashSet<&str> = ["src", "alt", "title"].iter().copied().collect();
    allowed_attrs.insert("a", link_attrs);
    allowed_attrs.insert("img", img_attrs);

    let html = promote_lazy_images(html);

    let mut builder = Builder::new();
    builder.tags(allowed_tags).tag_attributes(allowed_attrs);

    if let Ok(base) = url::Url::parse(base_url) {
        builder.url_relative(UrlRelative::RewriteWithBase(base));
    }

    builder.clean(&html).to_string()
}

/// Sanitize text to plain text only — strips ALL HTML tags.
/// Used for titles, author names, and other non-HTML fields.
pub fn sanitize_plain(text: &str) -> String {
    Builder::new()
        .tags(HashSet::new())
        .clean(text)
        .to_string()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_html_keeps_safe_tags() {
        let input = "<p>Hello <strong>world</strong></p>";
        let result = sanitize_html(input, "https://example.com/post");
        assert!(result.contains("<p>"));
        assert!(result.contains("<strong>"));
    }

    #[test]
    fn test_sanitize_html_strips_script() {
        let input = "<p>Safe</p><script>alert('xss')</script>";
        let result = sanitize_html(input, "https://example.com/post");
        assert!(!result.contains("<script>"));
        assert!(result.contains("<p>Safe</p>"));
    }

    #[test]
    fn relative_urls_are_resolved_against_the_page_they_came_from() {
        let input = r#"<p><a href="/about">About</a> <img src="img/hero.png" alt=""></p>"#;
        let result = sanitize_html(input, "https://example.com/blog/post");
        assert!(
            result.contains("https://example.com/about"),
            "root-relative link should resolve, got {result}"
        );
        assert!(
            result.contains("https://example.com/blog/img/hero.png"),
            "path-relative image should resolve against the directory, got {result}"
        );
    }

    #[test]
    fn absolute_urls_are_left_alone() {
        let input = r#"<a href="https://other.example/x">x</a>"#;
        let result = sanitize_html(input, "https://example.com/post");
        assert!(result.contains("https://other.example/x"));
    }

    #[test]
    fn an_unusable_base_leaves_the_markup_as_it_was() {
        let input = r#"<a href="/about">About</a>"#;
        let result = sanitize_html(input, "");
        assert!(
            result.contains("/about"),
            "a link we cannot resolve is still better kept than dropped"
        );
    }

    #[test]
    fn a_lazy_loaded_image_keeps_the_picture_rather_than_the_placeholder() {
        // Taken from a real article: the `src` is a one-pixel transparent GIF
        // and the picture is in `data-src`. Both used to be dropped — `data:`
        // is not an allowed scheme and `data-src` is not an allowed attribute
        // — leaving an empty box under a caption describing it.
        let input = r#"<figure><img class="lazy" alt="A boat"
            src="data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw=="
            data-src="https://cdn.example.com/boat.jpg?w=1020"></figure>"#;

        let result = sanitize_html(input, "https://example.com/post");
        assert!(
            result.contains("https://cdn.example.com/boat.jpg?w=1020"),
            "the real picture should survive, got {result}"
        );
        assert!(!result.contains("base64"), "the placeholder should not");
        assert!(result.contains(r#"alt="A boat""#), "and the caption's subject stays");
    }

    #[test]
    fn a_query_string_is_not_escaped_a_second_time() {
        // Readability hands back already-serialized HTML, so `&` arrives as
        // `&amp;`. Escaping it again produced `&amp;amp;`, and a CDN asked for
        // `?w=1020&amp;h=0` answers with nothing.
        let input = r#"<img src="data:image/gif;base64,AA" data-src="https://cdn.example.com/p.jpg?w=1020&amp;h=0&amp;q=100">"#;
        let result = sanitize_html(input, "https://example.com/post");

        assert!(!result.contains("&amp;amp;"), "got {result}");
        assert!(result.contains("p.jpg?w=1020&amp;h=0&amp;q=100"), "got {result}");
    }

    #[test]
    fn a_quote_in_an_attribute_cannot_break_out_of_it() {
        let input = r#"<img data-src='https://cdn.example.com/a.jpg' alt='He said "hello"'>"#;
        let result = sanitize_html(input, "https://example.com/post");
        assert!(result.contains("a.jpg"));
        assert!(result.contains("&quot;hello&quot;"), "got {result}");
    }

    #[test]
    fn an_image_with_a_real_src_is_left_as_it_was() {
        let input = r#"<img src="https://cdn.example.com/real.jpg" data-src="https://cdn.example.com/other.jpg">"#;
        let result = sanitize_html(input, "https://example.com/post");
        assert!(result.contains("real.jpg"));
        assert!(!result.contains("other.jpg"), "a page that means its src means it");
    }

    #[test]
    fn a_srcset_gives_up_its_widest_candidate() {
        let input = r#"<img data-srcset="small.jpg 400w, big.jpg 1600w, medium.jpg 800w">"#;
        let result = sanitize_html(input, "https://example.com/post/");
        assert!(result.contains("big.jpg"), "got {result}");
        assert!(!result.contains("small.jpg"));
    }

    #[test]
    fn a_lazy_url_that_is_relative_still_gets_resolved() {
        // The two rewrites have to compose: promotion happens first, and what
        // it promotes is then subject to the same base as any other URL.
        let input = r#"<img src="data:image/gif;base64,AAAA" data-src="/img/hero.png">"#;
        let result = sanitize_html(input, "https://example.com/blog/post");
        assert!(
            result.contains("https://example.com/img/hero.png"),
            "got {result}"
        );
    }

    #[test]
    fn an_image_with_nothing_to_show_is_still_not_a_hazard() {
        let input = r#"<img class="lazy" onerror="alert(1)" alt="Nothing">"#;
        let result = sanitize_html(input, "https://example.com/post");
        assert!(!result.contains("onerror"));
        assert!(!result.contains("src="));
    }

    #[test]
    fn markup_without_images_is_returned_untouched() {
        let input = "<p>Just words, and a <a href=\"/x\">link</a>.</p>";
        let result = sanitize_html(input, "https://example.com/post");
        assert!(result.contains("https://example.com/x"));
        assert!(result.contains("Just words"));
    }

    #[test]
    fn test_sanitize_plain_strips_all() {
        let input = "<b>Bold</b> and <em>italic</em>";
        let result = sanitize_plain(input);
        assert_eq!(result, "Bold and italic");
    }
}

