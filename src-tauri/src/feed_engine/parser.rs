use serde::{Deserialize, Serialize};

use super::sanitizer;

/// A single parsed article from a feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedArticle {
    pub guid: String,
    pub title: String,
    pub url: String,
    pub author: String,
    pub content: String,
    pub summary: String,
    pub published_at: String,
    pub thumbnail_url: String,
    pub content_type: String,
    pub word_count: i64,
    pub read_time_minutes: i64,
}

/// Parse raw feed bytes into a list of articles.
/// Supports RSS 2.0, Atom, and JSON Feed via the `feed-rs` crate.
pub fn parse_feed(raw: &[u8]) -> Result<Vec<ParsedArticle>, String> {
    let feed = feed_rs::parser::parse(raw)
        .map_err(|e| format!("Feed parse error: {}", e))?;

    let mut articles = Vec::new();

    for entry in feed.entries {
        // GUID: use entry id, fallback to first link
        let guid = if entry.id.is_empty() {
            entry.links.first().map(|l| l.href.clone()).unwrap_or_default()
        } else {
            entry.id.clone()
        };

        // Title
        let title = entry
            .title
            .as_ref()
            .map(|t| sanitizer::sanitize_plain(&t.content))
            .unwrap_or_default();

        // URL
        let url = entry
            .links
            .first()
            .map(|l| l.href.clone())
            .unwrap_or_default();

        // Author
        let author = entry
            .authors
            .first()
            .map(|a| sanitizer::sanitize_plain(&a.name))
            .or_else(|| {
                feed.authors.first().map(|a| sanitizer::sanitize_plain(&a.name))
            })
            .unwrap_or_default();

        // Content (full HTML body)
        let raw_content = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .unwrap_or_default();

        // Summary / description
        let raw_summary = entry
            .summary
            .as_ref()
            .map(|s| s.content.clone())
            .unwrap_or_default();

        let content = sanitizer::sanitize_html(&raw_content);
        let summary = sanitizer::sanitize_html(&raw_summary);

        // Published date → ISO 8601
        let published_at = entry
            .published
            .or(entry.updated)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        // Thumbnail: look for media content or media thumbnail
        let thumbnail_url = entry
            .media
            .iter()
            .flat_map(|m| m.thumbnails.iter())
            .next()
            .map(|t| t.image.uri.clone())
            .or_else(|| {
                entry
                    .media
                    .iter()
                    .flat_map(|m| m.content.iter())
                    .find(|c| {
                        c.content_type
                            .as_ref()
                            .map(|ct| ct.to_string().starts_with("image"))
                            .unwrap_or(false)
                    })
                    .and_then(|c| c.url.as_ref().map(|u| u.to_string()))
            })
            .unwrap_or_default();

        // Content type
        let content_type = entry
            .content
            .as_ref()
            .map(|c| c.content_type.to_string())
            .unwrap_or_else(|| "text/html".to_string());

        // Word count & read time
        let text_for_count = if !content.is_empty() {
            strip_html_for_counting(&content)
        } else {
            strip_html_for_counting(&summary)
        };
        let word_count = text_for_count.split_whitespace().count() as i64;
        let read_time_minutes = std::cmp::max(1, (word_count as f64 / 200.0).ceil() as i64);

        articles.push(ParsedArticle {
            guid,
            title,
            url,
            author,
            content,
            summary,
            published_at,
            thumbnail_url,
            content_type,
            word_count,
            read_time_minutes,
        });
    }

    Ok(articles)
}

/// Quick HTML tag stripper for word counting — not for display.
fn strip_html_for_counting(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}
