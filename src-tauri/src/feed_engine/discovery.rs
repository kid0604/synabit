use regex::Regex;
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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Synabit/1.0 Feed Reader")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

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

            let body = response.text().await.unwrap_or_default();

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
                    let body = resp.text().await.unwrap_or_default();
                    if is_feed_content_type(&ct) || looks_like_feed(&body) {
                        let feed_type = detect_feed_type(&body);
                        feeds.push(DiscoveredFeed {
                            url: probe_url,
                            title: String::new(),
                            feed_type,
                        });
                    }
                }
            }
        }
    }

    Ok(feeds)
}

/// Parse HTML for <link rel="alternate" type="application/rss+xml|application/atom+xml"> tags.
fn parse_link_tags(html: &str, base_url: &str) -> Vec<DiscoveredFeed> {
    let mut feeds = Vec::new();

    // Match <link ... rel="alternate" ... type="application/rss+xml" ... href="..." ... />
    let re = Regex::new(
        r#"<link\s[^>]*?(?:rel\s*=\s*["']alternate["'])[^>]*?(?:type\s*=\s*["'](application/(?:rss\+xml|atom\+xml|feed\+json))["'])[^>]*?(?:href\s*=\s*["']([^"']+)["'])[^>]*/?\s*>"#
    ).unwrap();

    for cap in re.captures_iter(html) {
        let feed_type_str = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let href = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        if href.is_empty() {
            continue;
        }

        let full_url = resolve_url(href, base_url);
        let feed_type = match feed_type_str {
            "application/rss+xml" => "rss",
            "application/atom+xml" => "atom",
            "application/feed+json" => "json",
            _ => "unknown",
        };

        feeds.push(DiscoveredFeed {
            url: full_url,
            title: extract_title_attr(html, href),
            feed_type: feed_type.to_string(),
        });
    }

    // Also try with attributes in different order (type before rel, href before type, etc.)
    let re2 = Regex::new(
        r#"<link\s[^>]*?(?:href\s*=\s*["']([^"']+)["'])[^>]*?(?:type\s*=\s*["'](application/(?:rss\+xml|atom\+xml|feed\+json))["'])[^>]*?(?:rel\s*=\s*["']alternate["'])[^>]*/?\s*>"#
    ).unwrap();

    for cap in re2.captures_iter(html) {
        let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let feed_type_str = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        if href.is_empty() {
            continue;
        }

        let full_url = resolve_url(href, base_url);
        // Check if we already have this URL
        if feeds.iter().any(|f| f.url == full_url) {
            continue;
        }

        let feed_type = match feed_type_str {
            "application/rss+xml" => "rss",
            "application/atom+xml" => "atom",
            "application/feed+json" => "json",
            _ => "unknown",
        };

        feeds.push(DiscoveredFeed {
            url: full_url,
            title: extract_title_attr(html, href),
            feed_type: feed_type.to_string(),
        });
    }

    feeds
}

/// Extract the title attribute from a link tag containing a specific href.
fn extract_title_attr(html: &str, href: &str) -> String {
    let escaped_href = regex::escape(href);
    let pattern = format!(
        r#"<link\s[^>]*href\s*=\s*["']{}["'][^>]*title\s*=\s*["']([^"']+)["'][^>]*/?\s*>"#,
        escaped_href
    );
    if let Ok(re) = Regex::new(&pattern) {
        if let Some(cap) = re.captures(html) {
            return cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        }
    }
    String::new()
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
