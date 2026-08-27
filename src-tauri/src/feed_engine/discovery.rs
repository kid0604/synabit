use futures::StreamExt;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A discovered feed from a web page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredFeed {
    pub url: String,
    pub title: String,
    pub feed_type: String,
}

/// Common feed paths to probe if no <link> tags are found.
const COMMON_FEED_PATHS: &[&str] = &[
    "/feed",
    "/rss",
    "/atom.xml",
    "/feed.xml",
    "/rss.xml",
    "/index.xml",
    "/feed/rss",
    "/feed/atom",
];

/// Discover feeds from a URL by:
/// 1. Parsing HTML <link> alternate tags
/// 2. Probing common feed paths
pub async fn discover_feeds(url: &str) -> Result<Vec<DiscoveredFeed>, String> {
    super::fetcher::guard_url(url)?;

    // Sites whose feed address is derivable from the page address get it
    // straight away. YouTube and Reddit both publish feeds and neither
    // advertises them, so this path used to end in the scraper producing a
    // worse version of a feed that was there all along.
    if let Some(adapted) = super::adapters::adapt(url) {
        if !adapted.needs_channel_lookup {
            return Ok(vec![DiscoveredFeed {
                url: adapted.url,
                title: String::new(),
                feed_type: adapted.feed_type,
            }]);
        }

        // A handle or vanity URL carries no channel id, so the page itself has
        // to say. If it will not, fall through to ordinary discovery rather
        // than failing — the reader gets the scraper instead of nothing.
        if let Ok(html) = super::fetcher::fetch_page(url).await {
            if let Some(channel_id) = super::adapters::youtube_channel_id(&html) {
                return Ok(vec![DiscoveredFeed {
                    url: super::adapters::youtube_feed_for_channel(&channel_id),
                    title: String::new(),
                    feed_type: adapted.feed_type,
                }]);
            }
        }
    }

    let client = super::fetcher::build_client(Duration::from_secs(15))?;

    let mut feeds = Vec::new();

    // First, try to parse the URL itself as a feed
    // (user might have directly provided a feed URL)
    if let Ok(response) = client.get(url).send().await {
        if response.status().is_success() {
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_lowercase();

            let body = read_capped_text(response).await;

            // If it looks like a feed, return it directly
            if is_feed_content_type(&content_type) || looks_like_feed(&body) {
                let feed_type = detect_feed_type(&body);
                feeds.push(DiscoveredFeed {
                    url: url.to_string(),
                    title: String::new(),
                    feed_type,
                });
                return Ok(feeds);
            }

            // Parse HTML for <link rel="alternate"> tags
            feeds.extend(parse_link_tags(&body, url));
        }
    }

    // If no feeds found via link tags, probe common paths
    if feeds.is_empty() {
        let base_url = extract_base_url(url);
        for path in COMMON_FEED_PATHS {
            let probe_url = format!("{}{}", base_url, path);
            if let Ok(resp) = client.get(&probe_url).send().await {
                if resp.status().is_success() {
                    let ct = resp
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("")
                        .to_lowercase();
                    let body = read_capped_text(resp).await;
                    if is_feed_content_type(&ct) || looks_like_feed(&body) {
                        let feed_type = detect_feed_type(&body);
                        feeds.push(DiscoveredFeed {
                            url: probe_url,
                            title: String::new(),
                            feed_type,
                        });
                        // One working feed is enough; the remaining guesses
                        // are seven more requests at a site that has already
                        // answered.
                        break;
                    }
                }
            }
        }
    }

    Ok(feeds)
}

/// Read a probe response as text, giving up on anything absurdly large.
///
/// Discovery pulls down pages chosen by whoever typed the URL, so it does not
/// get to trust their size. A page it cannot read is simply not a feed.
async fn read_capped_text(response: reqwest::Response) -> String {
    const MAX_PROBE_SIZE: usize = 4 * 1024 * 1024;

    if response
        .content_length()
        .is_some_and(|len| len as usize > MAX_PROBE_SIZE)
    {
        return String::new();
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { return String::new() };
        if body.len() + chunk.len() > MAX_PROBE_SIZE {
            return String::new();
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8_lossy(&body).into_owned()
}

/// Find the feeds a page advertises in its `<link rel="alternate">` tags.
///
/// This used to be two regexes, each matching one fixed order of the three
/// attributes — `rel, type, href` and `href, type, rel`. HTML does not promise
/// an order, so a page writing `type, rel, href` advertised its feed and this
/// found nothing. The crate that parses HTML properly was already a dependency
/// and already used two modules over.
fn parse_link_tags(html: &str, base_url: &str) -> Vec<DiscoveredFeed> {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse("link[rel~=alternate][href]") else {
        return Vec::new();
    };

    let mut feeds: Vec<DiscoveredFeed> = Vec::new();

    for element in document.select(&selector) {
        let Some(href) = element.value().attr("href") else {
            continue;
        };
        if href.trim().is_empty() {
            continue;
        }

        let Some(feed_type) = element
            .value()
            .attr("type")
            .map(str::trim)
            .and_then(feed_type_for_mime)
        else {
            continue;
        };

        let full_url = resolve_url(href, base_url);
        if feeds.iter().any(|f| f.url == full_url) {
            continue;
        }

        feeds.push(DiscoveredFeed {
            url: full_url,
            // The title comes off the same element rather than a second search
            // of the document for whichever tag happened to share this href.
            title: element
                .value()
                .attr("title")
                .unwrap_or_default()
                .trim()
                .to_string(),
            feed_type: feed_type.to_string(),
        });
    }

    feeds
}

/// The feed kind a `<link type=…>` names, if it names one at all.
fn feed_type_for_mime(mime: &str) -> Option<&'static str> {
    // A type may carry parameters: `application/rss+xml; charset=utf-8`.
    let base = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "application/rss+xml" => Some("rss"),
        "application/atom+xml" => Some("atom"),
        "application/feed+json" | "application/json" => Some("json"),
        _ => None,
    }
}

/// Check if a content-type header indicates a feed format.
fn is_feed_content_type(ct: &str) -> bool {
    ct.contains("application/rss")
        || ct.contains("application/atom")
        || ct.contains("application/xml")
        || ct.contains("text/xml")
        || ct.contains("application/feed+json")
}

/// Check if raw body text looks like a feed (quick heuristic).
fn looks_like_feed(body: &str) -> bool {
    let trimmed = body.trim_start();
    trimmed.starts_with("<?xml")
        || trimmed.starts_with("<rss")
        || trimmed.starts_with("<feed")
        || trimmed.starts_with("{\"version\":\"https://jsonfeed.org")
}

/// Detect feed type from content.
fn detect_feed_type(body: &str) -> String {
    let trimmed = body.trim_start();
    if trimmed.contains("<rss") {
        "rss".to_string()
    } else if trimmed.contains("<feed") {
        "atom".to_string()
    } else if trimmed.starts_with('{') {
        "json".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(href: &str, base_url: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        let scheme = if base_url.starts_with("https") {
            "https:"
        } else {
            "http:"
        };
        return format!("{}{}", scheme, href);
    }
    let base = extract_base_url(base_url);
    if href.starts_with('/') {
        format!("{}{}", base, href)
    } else {
        format!("{}/{}", base, href)
    }
}

/// Extract scheme + host from a URL (e.g., "https://example.com").
fn extract_base_url(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let after_scheme = &url[pos + 3..];
        if let Some(slash_pos) = after_scheme.find('/') {
            url[..pos + 3 + slash_pos].to_string()
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feed_link_is_found_whatever_order_its_attributes_are_in() {
        // Each of these advertises the same feed. The two regexes this
        // replaced matched the first and the last; the middle one was invisible.
        for html in [
            r#"<link rel="alternate" type="application/rss+xml" href="/feed.xml">"#,
            r#"<link type="application/rss+xml" rel="alternate" href="/feed.xml">"#,
            r#"<link href="/feed.xml" type="application/rss+xml" rel="alternate">"#,
            r#"<link href='/feed.xml' rel='alternate' type='application/rss+xml'>"#,
        ] {
            let feeds = parse_link_tags(html, "https://example.com/blog");
            assert_eq!(feeds.len(), 1, "should have found the feed in: {html}");
            assert_eq!(feeds[0].url, "https://example.com/feed.xml");
            assert_eq!(feeds[0].feed_type, "rss");
        }
    }

    #[test]
    fn the_title_comes_from_the_link_that_owns_it() {
        let html = r#"
            <link rel="alternate" type="application/rss+xml" title="Posts" href="/posts.xml">
            <link rel="alternate" type="application/rss+xml" title="Comments" href="/comments.xml">
        "#;
        let feeds = parse_link_tags(html, "https://example.com/");
        assert_eq!(feeds.len(), 2);
        let posts = feeds.iter().find(|f| f.url.ends_with("posts.xml")).unwrap();
        assert_eq!(posts.title, "Posts");
        let comments = feeds.iter().find(|f| f.url.ends_with("comments.xml")).unwrap();
        assert_eq!(comments.title, "Comments");
    }

    #[test]
    fn a_type_with_a_charset_is_still_that_type() {
        let html = r#"<link rel="alternate" type="application/atom+xml; charset=utf-8" href="/a.xml">"#;
        let feeds = parse_link_tags(html, "https://example.com/");
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].feed_type, "atom");
    }

    #[test]
    fn alternates_that_are_not_feeds_are_left_alone() {
        let html = r#"
            <link rel="alternate" hreflang="fr" href="/fr/">
            <link rel="stylesheet" type="text/css" href="/style.css">
            <link rel="alternate" type="application/rss+xml" href="/feed.xml">
        "#;
        let feeds = parse_link_tags(html, "https://example.com/");
        assert_eq!(feeds.len(), 1, "a translation is not a feed, nor is a stylesheet");
        assert!(feeds[0].url.ends_with("/feed.xml"));
    }

    #[test]
    fn the_same_feed_listed_twice_is_offered_once() {
        let html = r#"
            <link rel="alternate" type="application/rss+xml" href="/feed.xml">
            <link rel="alternate" type="application/rss+xml" href="https://example.com/feed.xml">
        "#;
        let feeds = parse_link_tags(html, "https://example.com/");
        assert_eq!(feeds.len(), 1);
    }
}
