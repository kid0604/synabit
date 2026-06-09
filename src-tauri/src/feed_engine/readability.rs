//! Simplified readability engine for extracting article content from web pages.
//!
//! Inspired by Mozilla's Readability algorithm. Used for lazy-loading
//! full article content for scrape-type feeds.

use scraper::{Html, Selector, ElementRef};
use crate::feed_engine::sanitizer;

/// Result of content extraction from an article page.
#[derive(Debug, Clone)]
pub struct ReadabilityResult {
    pub title: String,
    pub author: String,
    pub published_at: String,
    pub thumbnail_url: String,
    pub content: String,
    pub word_count: i64,
    pub read_time_minutes: i64,
}

/// Extract article content from a web page's HTML.
pub fn extract_content(html: &str, base_url: &str) -> ReadabilityResult {
    let document = Html::parse_document(html);
    
    // 1. Extract metadata from <head>
    let title = extract_meta_title(&document);
    let author = extract_meta_author(&document);
    let published_at = extract_meta_date(&document);
    let thumbnail_url = extract_meta_image(&document, base_url);
    
    // 2. Find the main content element
    let content = find_main_content(&document);
    
    // 3. Sanitize content
    let sanitized = sanitizer::sanitize_html(&content);
    
    // 4. Calculate stats
    let text_only = Html::parse_fragment(&sanitized)
        .root_element()
        .text()
        .collect::<String>();
    let word_count = text_only.split_whitespace().count() as i64;
    let read_time_minutes = std::cmp::max(1, word_count / 200);
    
    ReadabilityResult {
        title,
        author,
        published_at,
        thumbnail_url,
        content: sanitized,
        word_count,
        read_time_minutes,
    }
}

/// Find the main content element using a series of strategies.
fn find_main_content(document: &Html) -> String {
    // Strategy 1: Try specific content selectors (most reliable)
    let content_selectors = [
        "article .entry-content",
        "article .post-content", 
        "article .article-content",
        "article .article-body",
        ".entry-content",
        ".post-content",
        ".article-content",
        ".article-body",
        ".story-body",
        ".content-body",
        "[itemprop=\"articleBody\"]",
        "[property=\"content:encoded\"]",
        "article",
        "[role=\"main\"] article",
        "[role=\"main\"]",
        "main",
    ];
    
    for sel_str in &content_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            for el in document.select(&sel) {
                let text_len = el.text().collect::<String>().len();
                // Must have substantial text content (at least 200 chars)
                if text_len >= 200 {
                    return clean_content_html(&el);
                }
            }
        }
    }
    
    // Strategy 2: Score-based approach — find the element with highest text density
    if let Ok(sel) = Selector::parse("div, section") {
        let mut best_html = String::new();
        let mut best_score: f64 = 0.0;
        
        for el in document.select(&sel) {
            let classes = el.value().attr("class").unwrap_or("").to_lowercase();
            let id = el.value().attr("id").unwrap_or("").to_lowercase();
            
            // Skip navigation/sidebar/footer elements
            let skip_patterns = ["nav", "menu", "sidebar", "footer", "header", "comment", "widget", "ad-", "advert", "social", "share", "related", "recommend"];
            if skip_patterns.iter().any(|p| classes.contains(p) || id.contains(p)) {
                continue;
            }
            
            let text = el.text().collect::<String>();
            let text_len = text.len() as f64;
            let html_len = el.inner_html().len() as f64;
            
            if text_len < 200.0 || html_len < 1.0 {
                continue;
            }
            
            // Score = text density * bonus for content-like classes
            let mut score = text_len;
            
            // Bonus for content-like class names
            let content_patterns = ["content", "article", "post", "entry", "body", "text", "story"];
            if content_patterns.iter().any(|p| classes.contains(p) || id.contains(p)) {
                score *= 1.5;
            }
            
            // Bonus for having <p> tags (a strong indicator of article content)
            if let Ok(p_sel) = Selector::parse("p") {
                let p_count = el.select(&p_sel).count() as f64;
                score *= 1.0 + (p_count * 0.1);
            }
            
            // Penalty for high link density (navigation-like)
            if let Ok(a_sel) = Selector::parse("a") {
                let link_text_len: f64 = el.select(&a_sel)
                    .map(|a| a.text().collect::<String>().len() as f64)
                    .sum();
                let link_density = if text_len > 0.0 { link_text_len / text_len } else { 1.0 };
                if link_density > 0.5 {
                    score *= 0.1; // Heavy penalty for link-dense elements
                }
            }
            
            if score > best_score {
                best_score = score;
                best_html = clean_content_html(&el);
            }
        }
        
        if !best_html.is_empty() {
            return best_html;
        }
    }
    
    // Strategy 3: Just collect all <p> tags from body
    if let Ok(sel) = Selector::parse("body p") {
        let paragraphs: Vec<String> = document.select(&sel)
            .map(|p| format!("<p>{}</p>", p.inner_html()))
            .filter(|p| p.len() > 40) // Skip tiny paragraphs
            .collect();
        if !paragraphs.is_empty() {
            return paragraphs.join("\n");
        }
    }
    
    String::new()
}

/// Clean content HTML by removing non-content elements.
fn clean_content_html(element: &ElementRef) -> String {
    let mut html = element.inner_html();
    
    // Remove script, style, nav, aside, footer, form, iframe, noscript
    let remove_tags = ["script", "style", "nav", "aside", "footer", "form", "iframe", "noscript", "svg"];
    for tag in &remove_tags {
        // Simple regex-free removal: just strip these tags and content
        // This is approximate — ammonia sanitizer will do the precise job later
        loop {
            let open = format!("<{}", tag);
            let close = format!("</{}>", tag);
            if let Some(start) = html.to_lowercase().find(&open) {
                if let Some(end) = html.to_lowercase()[start..].find(&close) {
                    html = format!("{}{}", &html[..start], &html[start + end + close.len()..]);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    
    // Remove elements with ad/social classes (simple pattern matching)
    // The ammonia sanitizer will clean up any remaining unwanted elements
    
    html
}

/// Extract title from meta tags or <title>.
fn extract_meta_title(document: &Html) -> String {
    // Try og:title first
    if let Some(title) = get_meta_content(document, "og:title") {
        return title;
    }
    // Try twitter:title
    if let Some(title) = get_meta_content(document, "twitter:title") {
        return title;
    }
    // Try <title> tag
    if let Ok(sel) = Selector::parse("title") {
        if let Some(el) = document.select(&sel).next() {
            let text = el.text().collect::<String>().trim().to_string();
            // Often title has " - Site Name" suffix, try to clean it
            if let Some(pos) = text.rfind(" - ") {
                return text[..pos].trim().to_string();
            }
            if let Some(pos) = text.rfind(" | ") {
                return text[..pos].trim().to_string();
            }
            return text;
        }
    }
    // Try h1
    if let Ok(sel) = Selector::parse("h1") {
        if let Some(el) = document.select(&sel).next() {
            return el.text().collect::<String>().trim().to_string();
        }
    }
    String::new()
}

/// Extract author from meta tags.
fn extract_meta_author(document: &Html) -> String {
    if let Some(author) = get_meta_content(document, "author") {
        return author;
    }
    if let Some(author) = get_meta_content(document, "article:author") {
        return author;
    }
    // Try byline elements
    for sel_str in &["[class*=\"author\"]", "[class*=\"byline\"]", "[rel=\"author\"]"] {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = document.select(&sel).next() {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() && text.len() < 100 {
                    return text;
                }
            }
        }
    }
    String::new()
}

/// Extract publication date from meta tags.
fn extract_meta_date(document: &Html) -> String {
    for prop in &["article:published_time", "datePublished", "date", "DC.date.issued"] {
        if let Some(date) = get_meta_content(document, prop) {
            return date;
        }
    }
    // Try <time> element
    if let Ok(sel) = Selector::parse("time[datetime]") {
        if let Some(el) = document.select(&sel).next() {
            if let Some(dt) = el.value().attr("datetime") {
                return dt.to_string();
            }
        }
    }
    // Try JSON-LD
    if let Ok(sel) = Selector::parse("script[type=\"application/ld+json\"]") {
        for el in document.select(&sel) {
            let text = el.text().collect::<String>();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(date) = json.get("datePublished").and_then(|d| d.as_str()) {
                    return date.to_string();
                }
            }
        }
    }
    String::new()
}

/// Extract featured image from meta tags.
fn extract_meta_image(document: &Html, base_url: &str) -> String {
    if let Some(img) = get_meta_content(document, "og:image") {
        return resolve_url_simple(&img, base_url);
    }
    if let Some(img) = get_meta_content(document, "twitter:image") {
        return resolve_url_simple(&img, base_url);
    }
    String::new()
}

/// Helper: get meta tag content by property or name.
fn get_meta_content(document: &Html, key: &str) -> Option<String> {
    // Try property attribute
    let sel_str = format!("meta[property=\"{}\"]", key);
    if let Ok(sel) = Selector::parse(&sel_str) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(content) = el.value().attr("content") {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    // Try name attribute
    let sel_str2 = format!("meta[name=\"{}\"]", key);
    if let Ok(sel) = Selector::parse(&sel_str2) {
        if let Some(el) = document.select(&sel).next() {
            if let Some(content) = el.value().attr("content") {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

/// Simple URL resolver.
fn resolve_url_simple(href: &str, base_url: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        return format!("https:{}", href);
    }
    let base = if let Some(pos) = base_url.find("://") {
        let after = &base_url[pos + 3..];
        if let Some(slash) = after.find('/') {
            &base_url[..pos + 3 + slash]
        } else {
            base_url
        }
    } else {
        base_url
    };
    if href.starts_with('/') {
        format!("{}{}", base, href)
    } else {
        format!("{}/{}", base, href)
    }
}
