//! HTML scraping engine for extracting article cards from web pages.
//!
//! Used as fallback when no RSS/Atom feed is found.

use scraper::{Html, Selector, ElementRef};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A scraped article card from a web page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapedArticle {
    pub url: String,
    pub title: String,
    pub thumbnail_url: String,
    pub summary: String,
    pub published_at: String,
}

/// CSS selectors for finding article containers
const CONTAINER_SELECTORS: &[&str] = &[
    "article",
    "[class*=\"post\"]",
    "[class*=\"article\"]",
    "[class*=\"entry\"]",
    "[class*=\"card\"]",
    "[class*=\"story\"]",
    "[class*=\"news-item\"]",
    "[class*=\"blog-item\"]",
];

/// URL path segments that indicate non-article pages
const BLACKLIST_PATHS: &[&str] = &[
    "/tag", "/category", "/author", "/page/",
    "/login", "/register", "/search", "/contact",
    "/about", "/privacy", "/terms", "/faq",
    "/cart", "/checkout", "/account",
    "#", "javascript:", "mailto:",
];

/// Tags whose descendants should be excluded from article search
const EXCLUDED_TAGS: &[&str] = &["nav", "header", "footer", "aside"];

/// Scrape article cards from a web page's HTML.
pub fn scrape_articles(html: &str, base_url: &str) -> Vec<ScrapedArticle> {
    let document = Html::parse_document(html);
    let domain = extract_domain(base_url);
    let mut articles = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();

    for selector_str in CONTAINER_SELECTORS {
        let selector = match Selector::parse(selector_str) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for container in document.select(&selector) {
            // Skip if inside excluded ancestor (nav, header, footer, aside)
            if is_inside_excluded(&container) {
                continue;
            }

            if let Some(article) = extract_article_card(&container, &domain, base_url) {
                if !article.title.is_empty() && seen_urls.insert(article.url.clone()) {
                    articles.push(article);
                }
            }
        }
    }

    // Deduplicate and limit
    articles.truncate(50);
    articles
}

/// Extract an article card from a container element.
fn extract_article_card(container: &ElementRef, domain: &str, base_url: &str) -> Option<ScrapedArticle> {
    // Find the primary link
    let link_sel = Selector::parse("a[href]").ok()?;
    
    // Prefer links wrapping headings
    let heading_link_sel = Selector::parse("h1 a[href], h2 a[href], h3 a[href], h4 a[href]").ok()?;
    let primary_link = container.select(&heading_link_sel).next()
        .or_else(|| {
            // Or a link that contains a heading
            let a_sel = Selector::parse("a[href]").ok()?;
            container.select(&a_sel).find(|a| {
                let inner = a.inner_html().to_lowercase();
                inner.contains("<h1") || inner.contains("<h2") || inner.contains("<h3") || inner.contains("<h4")
            })
        })
        .or_else(|| container.select(&link_sel).next())?;
    
    let href = primary_link.value().attr("href")?;
    
    // Skip empty, anchor-only, or javascript links
    if href.is_empty() || href == "#" || href.starts_with("javascript:") || href.starts_with("mailto:") {
        return None;
    }
    
    let full_url = resolve_url(href, base_url);
    
    // Filter: same domain only
    if !is_same_domain(&full_url, domain) {
        return None;
    }
    
    // Filter: path must have at least 2 segments (e.g., /category/article-slug)
    let path = extract_path(&full_url);
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    
    // Filter: blacklisted paths
    let path_lower = path.to_lowercase();
    if BLACKLIST_PATHS.iter().any(|bp| path_lower.contains(&bp.to_lowercase())) {
        return None;
    }
    
    // Extract title
    let title = extract_title(container);
    if title.is_empty() || title.len() < 8 {
        return None; // Too short to be an article title
    }
    
    // Extract thumbnail
    let thumbnail_url = extract_thumbnail(container, base_url);
    
    // Extract summary
    let summary = extract_summary(container, &title);
    
    // Extract date
    let published_at = extract_date(container);
    
    // Require title + at least 1 more signal to reduce false positives
    let signals = [
        !thumbnail_url.is_empty(),
        !summary.is_empty(),
        !published_at.is_empty(),
    ];
    if signals.iter().filter(|&&s| s).count() < 1 {
        return None;
    }
    
    Some(ScrapedArticle {
        url: full_url,
        title,
        thumbnail_url,
        summary,
        published_at,
    })
}

/// Extract article title from container, preferring heading elements.
fn extract_title(container: &ElementRef) -> String {
    // Try h1, h2, h3, h4 in order
    for tag in &["h1", "h2", "h3", "h4"] {
        if let Ok(sel) = Selector::parse(tag) {
            if let Some(heading) = container.select(&sel).next() {
                let text = heading.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    return text;
                }
            }
        }
    }
    
    // Fallback: text of the first link with substantial content
    if let Ok(sel) = Selector::parse("a") {
        for link in container.select(&sel) {
            let text = link.text().collect::<String>().trim().to_string();
            if text.len() >= 10 {
                return text;
            }
        }
    }
    
    String::new()
}

/// Extract thumbnail image URL from container.
fn extract_thumbnail(container: &ElementRef, base_url: &str) -> String {
    if let Ok(sel) = Selector::parse("img") {
        for img in container.select(&sel) {
            // Try various image source attributes
            let src = img.value().attr("src")
                .or_else(|| img.value().attr("data-src"))
                .or_else(|| img.value().attr("data-lazy-src"))
                .or_else(|| img.value().attr("data-original"))
                .unwrap_or("");
            
            if src.is_empty() || src.starts_with("data:") {
                continue;
            }
            
            // Skip tiny images (likely icons/avatars) by checking common size attributes
            let width = img.value().attr("width").and_then(|w| w.parse::<u32>().ok()).unwrap_or(999);
            let height = img.value().attr("height").and_then(|h| h.parse::<u32>().ok()).unwrap_or(999);
            if width < 50 || height < 50 {
                continue;
            }
            
            return resolve_url(src, base_url);
        }
    }
    
    String::new()
}

/// Extract summary/excerpt text from container.
fn extract_summary(container: &ElementRef, title: &str) -> String {
    // Try <p> elements
    if let Ok(sel) = Selector::parse("p") {
        for p in container.select(&sel) {
            let text = p.text().collect::<String>().trim().to_string();
            // Skip if it's just the title repeated or too short
            if text.len() >= 20 && text != title {
                // Truncate to 300 chars
                if text.len() > 300 {
                    return format!("{}...", &text[..text.floor_char_boundary(297)]);
                }
                return text;
            }
        }
    }
    
    // Try elements with class containing "excerpt", "summary", "desc"
    for cls in &["excerpt", "summary", "description", "desc", "intro"] {
        let sel_str = format!("[class*=\"{}\"]", cls);
        let text = Selector::parse(&sel_str).ok().and_then(|sel| {
            container.select(&sel).next().map(|el| el.text().collect::<String>().trim().to_string())
        }).unwrap_or_default();
        if text.len() >= 20 && text != title {
            if text.len() > 300 {
                return format!("{}...", &text[..text.floor_char_boundary(297)]);
            }
            return text;
        }
    }
    
    String::new()
}

/// Extract publication date from container.
fn extract_date(container: &ElementRef) -> String {
    // Try <time> element with datetime attribute
    if let Ok(sel) = Selector::parse("time[datetime]") {
        if let Some(time_el) = container.select(&sel).next() {
            if let Some(dt) = time_el.value().attr("datetime") {
                return dt.to_string();
            }
        }
    }
    
    // Try <time> element text content
    if let Ok(sel) = Selector::parse("time") {
        if let Some(time_el) = container.select(&sel).next() {
            let text = time_el.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                return text;
            }
        }
    }
    
    // Try elements with class containing "date", "time", "published"
    for cls in &["date", "time", "published", "posted"] {
        let sel_str = format!("[class*=\"{}\"]", cls);
        let text = Selector::parse(&sel_str).ok().and_then(|sel| {
            container.select(&sel).next().map(|el| el.text().collect::<String>().trim().to_string())
        }).unwrap_or_default();
        if !text.is_empty() && text.len() < 50 {
            return text;
        }
    }
    
    String::new()
}

/// Check if an element is inside an excluded ancestor (nav, header, footer, aside).
fn is_inside_excluded(element: &ElementRef) -> bool {
    // Check the element's tag itself
    let el_html = element.value().name();
    if EXCLUDED_TAGS.contains(&el_html) {
        return true;
    }
    
    // Check class/id for navigation-like patterns
    let classes = element.value().attr("class").unwrap_or("").to_lowercase();
    let id = element.value().attr("id").unwrap_or("").to_lowercase();
    let nav_patterns = ["nav", "menu", "sidebar", "footer", "header", "widget", "breadcrumb"];
    nav_patterns.iter().any(|p| classes.contains(p) || id.contains(p))
}

/// Resolve a potentially relative URL against a base URL.
fn resolve_url(href: &str, base_url: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        let scheme = if base_url.starts_with("https") { "https:" } else { "http:" };
        return format!("{}{}", scheme, href);
    }
    let base = extract_base_url(base_url);
    if href.starts_with('/') {
        format!("{}{}", base, href)
    } else {
        format!("{}/{}", base, href)
    }
}

/// Extract domain from URL (e.g., "example.com" from "https://example.com/path").
fn extract_domain(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let after = &url[pos + 3..];
        after.split('/').next().unwrap_or("").to_lowercase()
    } else {
        url.split('/').next().unwrap_or("").to_lowercase()
    }
}

/// Check if a URL belongs to the same domain.
fn is_same_domain(url: &str, domain: &str) -> bool {
    let url_domain = extract_domain(url);
    url_domain == domain || url_domain.ends_with(&format!(".{}", domain))
}

/// Extract path from URL.
fn extract_path(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let after = &url[pos + 3..];
        if let Some(slash) = after.find('/') {
            after[slash..].to_string()
        } else {
            "/".to_string()
        }
    } else {
        url.to_string()
    }
}

/// Extract scheme + host from a URL.
fn extract_base_url(url: &str) -> String {
    if let Some(pos) = url.find("://") {
        let after = &url[pos + 3..];
        if let Some(slash) = after.find('/') {
            url[..pos + 3 + slash].to_string()
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    }
}
