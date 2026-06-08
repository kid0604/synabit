use ammonia::Builder;
use std::collections::HashSet;

/// Sanitize HTML content, keeping only safe tags and attributes.
/// Used for article body content.
pub fn sanitize_html(html: &str) -> String {
    let mut allowed_tags = HashSet::new();
    for tag in &[
        "p", "a", "img", "h1", "h2", "h3", "h4", "h5", "h6",
        "ul", "ol", "li", "blockquote", "pre", "code", "em", "strong",
        "br", "hr", "figure", "figcaption",
        "table", "thead", "tbody", "tr", "th", "td",
    ] {
        allowed_tags.insert(*tag);
    }

    let mut allowed_attrs = std::collections::HashMap::new();
    let link_attrs: HashSet<&str> = ["href", "title"].iter().copied().collect();
    let img_attrs: HashSet<&str> = ["src", "alt", "title"].iter().copied().collect();
    allowed_attrs.insert("a", link_attrs);
    allowed_attrs.insert("img", img_attrs);

    Builder::new()
        .tags(allowed_tags)
        .tag_attributes(allowed_attrs)
        .clean(html)
        .to_string()
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
        let result = sanitize_html(input);
        assert!(result.contains("<p>"));
        assert!(result.contains("<strong>"));
    }

    #[test]
    fn test_sanitize_html_strips_script() {
        let input = "<p>Safe</p><script>alert('xss')</script>";
        let result = sanitize_html(input);
        assert!(!result.contains("<script>"));
        assert!(result.contains("<p>Safe</p>"));
    }

    #[test]
    fn test_sanitize_plain_strips_all() {
        let input = "<b>Bold</b> and <em>italic</em>";
        let result = sanitize_plain(input);
        assert_eq!(result, "Bold and italic");
    }
}
