//! Tauri IPC commands for the Feeds mini-app.
//!
//! Handles feed source CRUD (vault JSON files), article caching (SQLite),
//! feed refresh, discovery, OPML import/export, and maintenance.

use std::collections::HashMap;
use std::path::Path;

use futures::StreamExt;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbState;
use crate::feed_engine::{
    cleanup, discovery, fetcher, image_cache, opml as feed_opml, parser, readability, scrape,
    state_sync,
};

// ═══════════════════════════════════════════════════════════════
//  DATA TYPES
// ═══════════════════════════════════════════════════════════════

/// A subscription as `sources.json` stores it: what the reader decided, and
/// nothing this particular machine merely observed.
///
/// The ETag, the last fetch time and the last error used to live here too, and
/// every refresh rewrote the whole file to update them. That file is synced,
/// and the sync layer merges text character by character — so two devices
/// refreshing at the same time produced two pretty-printed documents differing
/// in dozens of timestamps, and the merge of those two is not reliably JSON.
/// When it was not, `sources.json` failed to parse and every subscription
/// vanished from the app at once.
///
/// Now the file changes only when a person changes something.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredFeedSource {
    pub id: String,
    pub url: String,
    pub site_url: String,
    pub feed_type: String,
    pub title: String,
    pub description: String,
    pub icon_url: String,
    pub category_id: String,
    pub update_interval: i64,
    pub is_paused: bool,
    pub added_at: String,
    /// Fetch and extract the article page instead of trusting what the feed
    /// put in `<content>`. For the many feeds that publish only a teaser.
    #[serde(default)]
    pub full_text_fetch: bool,
    /// A CSS selector naming the article cards on a scraped page, for a site
    /// the built-in guesses do not fit.
    #[serde(default)]
    pub scrape_container: String,

    // Read from vaults written by earlier versions and never written back —
    // `skip_serializing` is what retires them. `migrate_legacy_state` copies
    // them into the database once so an upgrade does not throw away every
    // ETag and re-download every feed in full.
    #[serde(default, skip_serializing)]
    pub last_fetched_at: Option<String>,
    #[serde(default, skip_serializing)]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing)]
    pub etag: Option<String>,
    #[serde(default, skip_serializing)]
    pub last_modified_header: Option<String>,
}

/// This device's view of a feed: what it knows about fetching it.
#[derive(Debug, Clone, Default)]
pub struct SourceState {
    pub etag: String,
    pub last_modified: String,
    pub last_fetched_at: String,
    pub last_error: String,
}

/// What the front end is handed: the subscription joined to this device's view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedSource {
    pub id: String,
    pub url: String,
    pub site_url: String,
    pub feed_type: String,
    pub title: String,
    pub description: String,
    pub icon_url: String,
    pub category_id: String,
    pub update_interval: i64,
    pub is_paused: bool,
    pub added_at: String,
    #[serde(default)]
    pub full_text_fetch: bool,
    #[serde(default)]
    pub scrape_container: String,
    pub last_fetched_at: String,
    pub last_error: Option<String>,
}

impl StoredFeedSource {
    fn with_state(self, state: SourceState) -> FeedSource {
        FeedSource {
            id: self.id,
            url: self.url,
            site_url: self.site_url,
            feed_type: self.feed_type,
            title: self.title,
            description: self.description,
            icon_url: self.icon_url,
            category_id: self.category_id,
            update_interval: self.update_interval,
            is_paused: self.is_paused,
            added_at: self.added_at,
            full_text_fetch: self.full_text_fetch,
            scrape_container: self.scrape_container,
            last_fetched_at: state.last_fetched_at,
            last_error: if state.last_error.is_empty() {
                None
            } else {
                Some(state.last_error)
            },
        }
    }

    /// True if this record still carries fields that belong in the database.
    fn has_legacy_state(&self) -> bool {
        self.last_fetched_at.is_some()
            || self.last_error.is_some()
            || self.etag.is_some()
            || self.last_modified_header.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedCategory {
    pub id: String,
    pub name: String,
    pub color: String,
    pub sort_order: i64,
    pub is_collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedConfig {
    /// Which list layout the app opens with: magazine, cards or titles.
    #[serde(default = "default_view")]
    pub default_view: String,
    /// `"newest"` or `"oldest"`.
    #[serde(default = "default_sort")]
    pub sort_order: String,
    /// Mark an article read once it has scrolled past the top of the list.
    #[serde(default)]
    pub mark_read_on_scroll: bool,
    #[serde(default = "default_cleanup_days")]
    pub auto_cleanup_days: i64,
    #[serde(default = "default_max_articles")]
    pub max_articles_per_feed: i64,
    /// How often a newly added feed is checked, in minutes.
    #[serde(default = "default_update_interval")]
    pub global_update_interval: i64,
    #[serde(default = "default_font_size")]
    pub reading_font_size: i64,
    #[serde(default = "default_max_width")]
    pub reading_max_width: i64,
}

// `show_read_articles` used to live here too, and nothing ever read it: it is
// what the Unread view is for. `mark_read_on_scroll` is back because there is
// now something behind it.

fn default_view() -> String {
    "magazine".to_string()
}
fn default_sort() -> String {
    "newest".to_string()
}
fn default_cleanup_days() -> i64 {
    30
}
fn default_max_articles() -> i64 {
    500
}
fn default_update_interval() -> i64 {
    30
}
fn default_font_size() -> i64 {
    16
}
fn default_max_width() -> i64 {
    720
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self {
            default_view: default_view(),
            sort_order: default_sort(),
            mark_read_on_scroll: false,
            auto_cleanup_days: default_cleanup_days(),
            max_articles_per_feed: default_max_articles(),
            global_update_interval: default_update_interval(),
            reading_font_size: default_font_size(),
            reading_max_width: default_max_width(),
        }
    }
}

/// Known list layouts. A config written before these were the only three
/// choices can name something else — "all" was the shipped default — and the
/// front end has no branch for it, so it would render nothing.
const VIEW_MODES: &[&str] = &["magazine", "cards", "titles"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedArticle {
    pub id: String,
    pub feed_source_id: String,
    pub guid: String,
    pub title: String,
    pub url: String,
    pub author: String,
    pub content: String,
    pub summary: String,
    pub published_at: String,
    pub fetched_at: String,
    pub thumbnail_url: String,
    pub word_count: i64,
    pub read_time_minutes: i64,
    pub content_type: String,
    pub is_read: bool,
    pub is_starred: bool,
    pub is_read_later: bool,
    /// Tags a rule attached when the article arrived.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Split the comma-wrapped tag string the database stores.
fn split_tags(stored: &str) -> Vec<String> {
    stored
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Join tags back into the comma-wrapped form, so `LIKE '%,rust,%'` is exact.
fn join_tags(tags: &[String]) -> String {
    if tags.is_empty() {
        return String::new();
    }
    format!(",{},", tags.join(","))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleFilter {
    /// Which feeds to draw from; `None` means all of them.
    ///
    /// A category selection arrives here already resolved to its members.
    /// Which feed belongs to which category is answered by `sources.json` in
    /// the vault, and this is a database query — the two used to be wired
    /// together by a comment saying the front end would handle it, which
    /// neither side did, so picking a category showed every article there was.
    pub source_ids: Option<Vec<String>>,
    pub view: String,
    /// Start of "today" as an instant, sent by the front end because only it
    /// knows the reader's timezone.
    pub today_start: Option<String>,
    /// `"oldest"` reads a backlog in the order it was written; anything else
    /// means newest first.
    pub sort: Option<String>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Unread counts behind each of the sidebar's saved views.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewCounts {
    pub today: usize,
    pub unread: usize,
    pub starred: usize,
    pub read_later: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub total_fetched: usize,
    pub total_new: usize,
    pub errors: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════
//  HELPERS — Vault JSON File Operations
// ═══════════════════════════════════════════════════════════════

/// Ensure the Feeds directory exists in the vault.
fn ensure_feeds_dir(vault_path: &str) -> Result<std::path::PathBuf, String> {
    let feeds_dir = Path::new(vault_path).join("Feeds");
    std::fs::create_dir_all(&feeds_dir)
        .map_err(|e| format!("Failed to create Feeds directory: {}", e))?;
    Ok(feeds_dir)
}

/// Read and deserialize a JSON file, returning a default if it doesn't exist.
fn read_json_file<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Serialize and write a JSON file.
fn write_json_file<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(())
}

/// Start of "today" for a reader who did not say where they are.
///
/// The front end normally sends its own local midnight; this is the answer for
/// callers that cannot, and it is UTC midnight.
fn today_cutoff(today_start: Option<&String>) -> String {
    match today_start {
        Some(instant) if !instant.trim().is_empty() => instant.clone(),
        _ => chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_utc()
            .to_rfc3339(),
    }
}

/// `?n, ?n+1, …` for an `IN (…)` list, and the values to bind to them.
fn in_clause(ids: &[String], first_idx: usize) -> String {
    (0..ids.len())
        .map(|i| format!("?{}", first_idx + i))
        .collect::<Vec<_>>()
        .join(", ")
}

/// How much of an article body a list row carries: enough for the two-line
/// preview a card shows when its feed sent no summary, and no more.
const LIST_CONTENT_PREVIEW: usize = 400;

/// Every column of one article, in the order `row_to_article` expects.
const ARTICLE_COLUMNS: &str = "id, feed_source_id, guid, title, url, author, content, summary,
     published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
     content_type, is_read, is_starred, is_read_later, tags";

/// Map a rusqlite row to a CachedArticle.
fn row_to_article(row: &rusqlite::Row) -> rusqlite::Result<CachedArticle> {
    Ok(CachedArticle {
        id: row.get(0)?,
        feed_source_id: row.get(1)?,
        guid: row.get(2)?,
        title: row.get(3)?,
        url: row.get(4)?,
        author: row.get(5)?,
        content: row.get(6)?,
        summary: row.get(7)?,
        published_at: row.get(8)?,
        fetched_at: row.get(9)?,
        thumbnail_url: row.get(10)?,
        word_count: row.get(11)?,
        read_time_minutes: row.get(12)?,
        content_type: row.get(13)?,
        is_read: row.get::<_, i64>(14)? != 0,
        is_starred: row.get::<_, i64>(15)? != 0,
        is_read_later: row.get::<_, i64>(16)? != 0,
        tags: split_tags(&row.get::<_, String>(17)?),
    })
}

// ═══════════════════════════════════════════════════════════════
//  FEED SOURCE CRUD (vault JSON files)
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn feed_get_sources(
    vault_path: String,
    db: tauri::State<'_, DbState>,
) -> Result<Vec<FeedSource>, String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("sources.json");
    let mut stored: Vec<StoredFeedSource> = read_json_file(&path)?;

    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    // A vault written by an earlier version still carries fetch state in the
    // file. Take it into the database once, then write the file back without
    // it — leaving it there means it keeps being rewritten on every refresh,
    // which is the thing this change exists to stop.
    if migrate_legacy_state(conn, &mut stored) {
        write_json_file(&path, &stored)?;
    }

    let mut states = load_source_states(conn);
    Ok(stored
        .into_iter()
        .map(|source| {
            let state = states.remove(&source.id).unwrap_or_default();
            source.with_state(state)
        })
        .collect())
}

/// Read `sources.json` without touching the database — for the paths that
/// only need the subscriptions themselves.
fn read_stored_sources(vault_path: &str) -> Result<(std::path::PathBuf, Vec<StoredFeedSource>), String> {
    let feeds_dir = ensure_feeds_dir(vault_path)?;
    let path = feeds_dir.join("sources.json");
    let sources: Vec<StoredFeedSource> = read_json_file(&path)?;
    Ok((path, sources))
}

#[tauri::command]
pub async fn feed_add_source(
    vault_path: String,
    url: String,
    category_id: Option<String>,
) -> Result<FeedSource, String> {
    // Step 1: Try RSS/Atom discovery
    let discovered = discovery::discover_feeds(&url).await?;

    if !discovered.is_empty() {
        // RSS/Atom found — existing flow
        let feed_url = discovered[0].url.clone();
        let feed_type = discovered[0].feed_type.clone();

        let fetch_result = fetcher::fetch_feed(&feed_url, None, None).await;
        let (title, description, site_url) = match &fetch_result {
            fetcher::FetchResult::Updated { body, .. } => {
                match feed_rs::parser::parse(body.as_slice()) {
                    Ok(feed) => {
                        let t = feed.title.map(|t| t.content).unwrap_or_default();
                        let d = feed.description.map(|d| d.content).unwrap_or_default();
                        let s = feed
                            .links
                            .first()
                            .map(|l| l.href.clone())
                            .unwrap_or_default();
                        (t, d, s)
                    }
                    Err(_) => (String::new(), String::new(), String::new()),
                }
            }
            _ => (String::new(), String::new(), String::new()),
        };

        let now = chrono::Utc::now().to_rfc3339();
        let source = StoredFeedSource {
            id: uuid::Uuid::new_v4().to_string(),
            url: feed_url,
            site_url,
            feed_type,
            title,
            description,
            icon_url: String::new(),
            category_id: category_id.unwrap_or_default(),
            update_interval: feed_get_config(vault_path.clone())
                .map(|c| c.global_update_interval)
                .unwrap_or_else(|_| default_update_interval()),
            is_paused: false,
            added_at: now,
            full_text_fetch: false,
            scrape_container: String::new(),
            last_fetched_at: None,
            last_error: None,
            etag: None,
            last_modified_header: None,
        };

        let (path, mut sources) = read_stored_sources(&vault_path)?;
        if sources.iter().any(|s| s.url == source.url) {
            return Err("A feed with this URL already exists".to_string());
        }
        sources.push(source.clone());
        write_json_file(&path, &sources)?;
        return Ok(source.with_state(SourceState::default()));
    }

    // Step 2: No RSS found — try scrape mode
    let html = fetcher::fetch_page(&url).await?;

    let scraped = scrape::scrape_articles(&html, &url, None);
    if scraped.is_empty() {
        return Err("No RSS feed or articles found at this URL".to_string());
    }

    // Extract site title from HTML
    let doc = scraper::Html::parse_document(&html);
    let title = if let Ok(sel) = scraper::Selector::parse("title") {
        doc.select(&sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| url.clone())
    } else {
        url.clone()
    };
    let description = if let Ok(sel) = scraper::Selector::parse("meta[name=\"description\"]") {
        doc.select(&sel)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let now = chrono::Utc::now().to_rfc3339();
    let source = StoredFeedSource {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.clone(),
        site_url: url,
        feed_type: "scrape".to_string(),
        title,
        description,
        icon_url: String::new(),
        category_id: category_id.unwrap_or_default(),
        // Scraping fetches a whole homepage rather than a feed document, so it
        // stays deliberately slower than whatever the global interval is.
        update_interval: feed_get_config(vault_path.clone())
            .map(|c| (c.global_update_interval * 2).max(60))
            .unwrap_or(60),
        is_paused: false,
        added_at: now,
        // A scraped card carries no body, so its page is always fetched.
        full_text_fetch: true,
        scrape_container: String::new(),
        last_fetched_at: None,
        last_error: None,
        etag: None,
        last_modified_header: None,
    };

    let (path, mut sources) = read_stored_sources(&vault_path)?;
    if sources.iter().any(|s| s.url == source.url) {
        return Err("A feed with this URL already exists".to_string());
    }
    sources.push(source.clone());
    write_json_file(&path, &sources)?;
    Ok(source.with_state(SourceState::default()))
}

#[tauri::command]
pub fn feed_remove_source(
    vault_path: String,
    source_id: String,
    db: tauri::State<'_, DbState>,
) -> Result<(), String> {
    // Remove from sources.json
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("sources.json");
    let mut sources: Vec<FeedSource> = read_json_file(&path)?;
    sources.retain(|s| s.id != source_id);
    write_json_file(&path, &sources)?;

    // Delete cached articles from DB
    let db = db.lock().map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            "DELETE FROM feed_articles WHERE feed_source_id = ?1",
            params![source_id],
        )
        .map_err(|e| format!("Failed to delete cached articles: {}", e))?;

    // Delete fetch logs
    db.conn()
        .execute(
            "DELETE FROM feed_fetch_log WHERE feed_source_id = ?1",
            params![source_id],
        )
        .map_err(|e| format!("Failed to delete fetch logs: {}", e))?;

    clear_fetch_state(db.conn(), &source_id);

    Ok(())
}

#[tauri::command]
pub fn feed_update_source(vault_path: String, source: FeedSource) -> Result<(), String> {
    let (path, mut sources) = read_stored_sources(&vault_path)?;

    let Some(existing) = sources.iter_mut().find(|s| s.id == source.id) else {
        return Err("Feed source not found".to_string());
    };

    // Field by field rather than wholesale, so that the fetch state the front
    // end was handed for display cannot travel back into the vault.
    existing.url = source.url;
    existing.site_url = source.site_url;
    existing.feed_type = source.feed_type;
    existing.title = source.title;
    existing.description = source.description;
    existing.icon_url = source.icon_url;
    existing.category_id = source.category_id;
    existing.update_interval = source.update_interval;
    existing.is_paused = source.is_paused;
    existing.full_text_fetch = source.full_text_fetch;
    existing.scrape_container = source.scrape_container;

    write_json_file(&path, &sources)
}

// ═══════════════════════════════════════════════════════════════
//  CATEGORY CRUD (vault JSON files)
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn feed_get_categories(vault_path: String) -> Result<Vec<FeedCategory>, String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("categories.json");
    read_json_file(&path)
}

#[tauri::command]
pub fn feed_save_categories(
    vault_path: String,
    categories: Vec<FeedCategory>,
) -> Result<(), String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("categories.json");
    write_json_file(&path, &categories)
}

// ═══════════════════════════════════════════════════════════════
//  CONFIG (vault JSON files)
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn feed_get_config(vault_path: String) -> Result<FeedConfig, String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("config.json");
    if !path.exists() {
        return Ok(FeedConfig::default());
    }
    let mut config: FeedConfig = read_json_file(&path)?;
    if !VIEW_MODES.contains(&config.default_view.as_str()) {
        config.default_view = default_view();
    }
    if !matches!(config.sort_order.as_str(), "newest" | "oldest") {
        config.sort_order = default_sort();
    }
    Ok(config)
}

#[tauri::command]
pub fn feed_save_config(vault_path: String, config: FeedConfig) -> Result<(), String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("config.json");
    write_json_file(&path, &config)
}

// ═══════════════════════════════════════════════════════════════
//  ARTICLE CACHE (SQLite)
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn feed_get_articles(
    db: tauri::State<'_, DbState>,
    filter: ArticleFilter,
) -> Result<Vec<CachedArticle>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    query_articles(db.conn(), &filter)
}

/// The query behind `feed_get_articles`, separated from the Tauri state so a
/// test can hand it a connection.
pub(crate) fn query_articles(
    conn: &rusqlite::Connection,
    filter: &ArticleFilter,
) -> Result<Vec<CachedArticle>, String> {
    // A selection that resolves to no feeds is not the same as no selection at
    // all: an empty category holds no articles, it does not hold every one.
    if filter.source_ids.as_ref().is_some_and(|ids| ids.is_empty()) {
        return Ok(Vec::new());
    }

    let mut conditions: Vec<String> = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(ref ids) = filter.source_ids {
        conditions.push(format!(
            "a.feed_source_id IN ({})",
            in_clause(ids, param_idx)
        ));
        for id in ids {
            param_values.push(Box::new(id.clone()));
        }
        param_idx += ids.len();
    }

    match filter.view.as_str() {
        "today" => {
            // `datetime()` on both sides, not a string comparison. A feed may
            // publish "2026-08-23T01:00:00+07:00", which sorts after
            // "2026-08-23" as text while being the previous day everywhere
            // west of Hanoi — so the old prefix match called it today twice
            // over, once for the wrong timezone and once for the wrong date.
            conditions.push(format!(
                "a.published_at != '' AND datetime(a.published_at) >= datetime(?{})",
                param_idx
            ));
            param_values.push(Box::new(today_cutoff(filter.today_start.as_ref())));
            param_idx += 1;
        }
        "unread" => conditions.push("a.is_read = 0".to_string()),
        "starred" => conditions.push("a.is_starred = 1".to_string()),
        "read-later" => conditions.push("a.is_read_later = 1".to_string()),
        _ => {} // "all" — no filter
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let limit = filter.limit.unwrap_or(100);
    let offset = filter.offset.unwrap_or(0);

    // `content` is the article's whole body, and a list of fifty of them was
    // shipping megabytes of HTML across the IPC boundary to render a two-line
    // preview. The card falls back to the body when a feed sends no summary,
    // so a slice of it still goes — just not all of it. The reader asks for
    // the real row with `feed_get_article`.
    let sql = format!(
        "SELECT id, feed_source_id, guid, title, url, author,
                substr(content, 1, {}) AS content, summary,
                published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                content_type, is_read, is_starred, is_read_later, tags
         FROM feed_articles a
         {}
         ORDER BY published_at {}
         LIMIT ?{} OFFSET ?{}",
        LIST_CONTENT_PREVIEW,
        where_clause,
        if filter.sort.as_deref() == Some("oldest") {
            "ASC"
        } else {
            "DESC"
        },
        param_idx,
        param_idx + 1
    );

    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Query error: {}", e))?;
    let articles = stmt
        .query_map(params_ref.as_slice(), row_to_article)
        .map_err(|e| format!("Query map error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(articles)
}

#[tauri::command]
pub fn feed_search_articles(
    db: tauri::State<'_, DbState>,
    query: String,
    source_ids: Option<Vec<String>>,
    limit: Option<i64>,
) -> Result<Vec<CachedArticle>, String> {
    // What a person types is not FTS5 syntax. A stray quote, a bare hyphen or
    // the word AND used to reach `MATCH` untouched and come back as a syntax
    // error, and the only thing the front end could do with that was swallow
    // it — the results simply stopped changing as you typed. Reuse the parser
    // the main search already uses, so both searches agree on what a query
    // means and both learn the same lessons about escaping.
    let parsed = crate::search::parse_query(&query);
    let match_expr = crate::search::build_fts_match(&parsed);

    // `#rust` is a tag filter in the main search, so it is one here too — which
    // is what makes a tag a rule attaches worth attaching.
    if match_expr.is_none() && parsed.tag_filters.is_empty() {
        return Ok(Vec::new());
    }

    if source_ids.as_ref().is_some_and(|ids| ids.is_empty()) {
        return Ok(Vec::new());
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut conditions: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if let Some(expr) = &match_expr {
        conditions.push(format!("feed_articles_fts MATCH ?{}", param_idx));
        param_values.push(Box::new(expr.clone()));
        param_idx += 1;
    }

    for tag in &parsed.tag_filters {
        conditions.push(format!("a.tags LIKE ?{}", param_idx));
        // Comma-wrapped on both sides, so `#rust` does not match `rustlang`.
        param_values.push(Box::new(format!("%,{},%", tag)));
        param_idx += 1;
    }

    // Searching stays inside whatever the sidebar has selected. Typing into
    // the box while a feed is open should not silently widen the search to
    // every feed there is.
    if let Some(ref ids) = source_ids {
        conditions.push(format!("a.feed_source_id IN ({})", in_clause(ids, param_idx)));
        for id in ids {
            param_values.push(Box::new(id.clone()));
        }
        param_idx += ids.len();
    }

    // Without a text query there is no relevance to rank by, and no index to
    // join against either.
    let (join, order) = if match_expr.is_some() {
        (
            "JOIN feed_articles_fts fts ON a.rowid = fts.rowid",
            "ORDER BY rank",
        )
    } else {
        ("", "ORDER BY a.published_at DESC")
    };

    let sql = format!(
        "SELECT a.id, a.feed_source_id, a.guid, a.title, a.url, a.author,
                substr(a.content, 1, {}) AS content, a.summary, a.published_at, a.fetched_at,
                a.thumbnail_url, a.word_count, a.read_time_minutes,
                a.content_type, a.is_read, a.is_starred, a.is_read_later, a.tags
         FROM feed_articles a
         {}
         WHERE {}
         {}
         LIMIT ?{}",
        LIST_CONTENT_PREVIEW,
        join,
        conditions.join(" AND "),
        order,
        param_idx
    );
    param_values.push(Box::new(limit.unwrap_or(50)));

    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Search query error: {}", e))?;
    let articles = stmt
        .query_map(params_ref.as_slice(), row_to_article)
        .map_err(|e| format!("Search query map error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(articles)
}

/// The number beside each saved view in the sidebar.
///
/// One query rather than one per view. Today and Unread count what is waiting
/// to be read, because that is what those views are for. Starred and read-later
/// count everything they hold: an article is usually starred after it has been
/// read, so an unread count there would sit at zero and say nothing. "Today"
/// used to show the unread total of every feed regardless of date, and the
/// other two were hard-coded zeroes.
#[tauri::command]
pub fn feed_get_view_counts(
    db: tauri::State<'_, DbState>,
    today_start: Option<String>,
) -> Result<ViewCounts, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    view_counts_in(db.conn(), today_start.as_ref())
}

pub(crate) fn view_counts_in(
    conn: &rusqlite::Connection,
    today_start: Option<&String>,
) -> Result<ViewCounts, String> {
    let counts = conn
        .query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN is_read = 0 AND published_at != ''
                                       AND datetime(published_at) >= datetime(?1)
                                  THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_read = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_starred = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_read_later = 1 THEN 1 ELSE 0 END), 0)
             FROM feed_articles",
            params![today_cutoff(today_start)],
            |row| {
                Ok(ViewCounts {
                    today: row.get::<_, i64>(0)? as usize,
                    unread: row.get::<_, i64>(1)? as usize,
                    starred: row.get::<_, i64>(2)? as usize,
                    read_later: row.get::<_, i64>(3)? as usize,
                })
            },
        )
        .map_err(|e| format!("Query error: {}", e))?;

    Ok(counts)
}

/// One article, whole.
///
/// The list deliberately carries only a slice of each body, so opening an
/// article has to come back for the rest of it.
#[tauri::command]
pub fn feed_get_article(
    db: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<CachedArticle, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.conn()
        .query_row(
            &format!("SELECT {} FROM feed_articles WHERE id = ?1", ARTICLE_COLUMNS),
            params![article_id],
            row_to_article,
        )
        .map_err(|e| format!("Article not found: {}", e))
}

#[tauri::command]
pub fn feed_get_unread_counts(
    db: tauri::State<'_, DbState>,
) -> Result<HashMap<String, usize>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let mut stmt = conn
        .prepare(
            "SELECT feed_source_id, COUNT(*) FROM feed_articles
             WHERE is_read = 0 GROUP BY feed_source_id",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let rows = stmt
        .query_map([], |row| {
            let source_id: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((source_id, count as usize))
        })
        .map_err(|e| format!("Query map error: {}", e))?;

    let mut counts = HashMap::new();
    for r in rows.flatten() {
        counts.insert(r.0, r.1);
    }
    Ok(counts)
}

#[tauri::command]
pub fn feed_get_total_unread(db: tauri::State<'_, DbState>) -> Result<usize, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM feed_articles WHERE is_read = 0",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Query error: {}", e))?;

    Ok(count as usize)
}

#[tauri::command]
pub fn feed_mark_read(
    db: tauri::State<'_, DbState>,
    article_id: String,
    read: bool,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            "UPDATE feed_articles SET is_read = ?1, state_updated_at = ?3 WHERE id = ?2",
            params![read as i64, article_id, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("Update error: {}", e))?;
    Ok(())
}

/// Mark everything in scope as read, and say what changed so it can be undone.
///
/// `source_ids` is `None` only when the reader really did ask for every feed.
/// It used to be that selecting a category left the scope empty, and an empty
/// scope ran `UPDATE feed_articles SET is_read = 1` across the whole table —
/// one click on one category threw away the unread state of every feed, with
/// nothing to undo it.
#[tauri::command]
pub fn feed_mark_all_read(
    db: tauri::State<'_, DbState>,
    source_ids: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    mark_all_read_in(db.conn(), source_ids.as_deref())
}

/// The work behind `feed_mark_all_read`, separated from the Tauri state so a
/// test can prove that a scope really is a scope.
pub(crate) fn mark_all_read_in(
    conn: &rusqlite::Connection,
    source_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    if source_ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(Vec::new());
    }

    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let scope = match source_ids {
        Some(ids) => {
            let clause = format!(" AND feed_source_id IN ({})", in_clause(ids, 1));
            for id in ids {
                param_values.push(Box::new(id.clone()));
            }
            clause
        }
        None => String::new(),
    };
    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    // Collected before the update, because afterwards there is no way to tell
    // which rows this call changed and which were already read.
    let select_sql = format!("SELECT id FROM feed_articles WHERE is_read = 0{}", scope);
    let changed: Vec<String> = {
        let mut stmt = conn
            .prepare(&select_sql)
            .map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt
            .query_map(params_ref.as_slice(), |row| row.get::<_, String>(0))
            .map_err(|e| format!("Query map error: {}", e))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    if changed.is_empty() {
        return Ok(changed);
    }

    // The ids were collected first, so the same predicate can be reused here
    // without a thousand-parameter `IN` list.
    let update_sql = format!(
        "UPDATE feed_articles SET is_read = 1, state_updated_at = '{}' WHERE is_read = 0{}",
        chrono::Utc::now().to_rfc3339(),
        scope
    );
    conn.execute(&update_sql, params_ref.as_slice())
        .map_err(|e| format!("Update error: {}", e))?;

    Ok(changed)
}

/// Set the read flag on a known list of articles — the undo half of
/// `feed_mark_all_read`.
#[tauri::command]
pub fn feed_mark_read_bulk(
    db: tauri::State<'_, DbState>,
    article_ids: Vec<String>,
    read: bool,
) -> Result<(), String> {
    if article_ids.is_empty() {
        return Ok(());
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    // Marking a busy account read can touch thousands of rows, and SQLite has
    // a ceiling on how many parameters one statement may bind.
    for chunk in article_ids.chunks(500) {
        let sql = format!(
            "UPDATE feed_articles SET is_read = ?1, state_updated_at = '{}' WHERE id IN ({})",
            chrono::Utc::now().to_rfc3339(),
            in_clause(chunk, 2)
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(read as i64)];
        for id in chunk {
            param_values.push(Box::new(id.clone()));
        }
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_ref.as_slice())
            .map_err(|e| format!("Update error: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub fn feed_toggle_star(db: tauri::State<'_, DbState>, article_id: String) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let current: i64 = conn
        .query_row(
            "SELECT is_starred FROM feed_articles WHERE id = ?1",
            params![article_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let new_value = if current == 0 { 1i64 } else { 0i64 };
    conn.execute(
        "UPDATE feed_articles SET is_starred = ?1, state_updated_at = ?3 WHERE id = ?2",
        params![new_value, article_id, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|e| format!("Update error: {}", e))?;

    Ok(new_value == 1)
}

#[tauri::command]
pub fn feed_toggle_read_later(
    db: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let current: i64 = conn
        .query_row(
            "SELECT is_read_later FROM feed_articles WHERE id = ?1",
            params![article_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let new_value = if current == 0 { 1i64 } else { 0i64 };
    conn.execute(
        "UPDATE feed_articles SET is_read_later = ?1, state_updated_at = ?3 WHERE id = ?2",
        params![new_value, article_id, chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|e| format!("Update error: {}", e))?;

    Ok(new_value == 1)
}

// ═══════════════════════════════════════════════════════════════
//  FETCH & REFRESH
// ═══════════════════════════════════════════════════════════════

/// One feed's worth of work, pulled out of `sources.json` before any network
/// happens so the fetches can run without holding a borrow on the list.
#[derive(Clone)]
struct RefreshTarget {
    id: String,
    url: String,
    feed_type: String,
    etag: Option<String>,
    last_modified: Option<String>,
    scrape_container: String,
}

/// What came back from the network for one feed. No database work has happened
/// yet — that is deliberate, because the fetches run concurrently and SQLite
/// writes do not.
enum FetchOutcome {
    NotModified,
    Feed {
        articles: Vec<parser::ParsedArticle>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
    Scraped(Vec<scrape::ScrapedArticle>),
    Failed {
        message: String,
        /// What the server asked for, if it said. Honoured over our own guess.
        retry_after: Option<i64>,
    },
}

/// How many feeds to fetch at once.
///
/// The refresh used to walk the list one at a time with a thirty-second
/// timeout each, so a hundred feeds with a few dead ones among them took
/// minutes. Six is polite to the network and to the machine, and turns that
/// same list into something that finishes while the reader is still looking.
const REFRESH_CONCURRENCY: usize = 6;

#[tauri::command]
pub async fn feed_refresh(
    db: tauri::State<'_, DbState>,
    vault_path: String,
    source_id: Option<String>,
    manual: Option<bool>,
    due_only: Option<bool>,
) -> Result<RefreshResult, String> {
    refresh_sources(
        &db,
        &vault_path,
        source_id,
        manual.unwrap_or(false),
        due_only.unwrap_or(false),
    )
    .await
}

/// The refresh itself, reachable from the background scheduler as well as from
/// the command.
///
/// `due_only` asks for the feeds whose own interval has elapsed, which is what
/// a timer wants; without it every feed is fetched, which is what a person
/// pressing refresh wants. Deciding that here rather than in the front end is
/// what lets the same code serve a background sweep on the desktop and a
/// foreground one on Android, where there is no background to run in.
pub(crate) async fn refresh_sources(
    db: &DbState,
    vault_path: &str,
    source_id: Option<String>,
    manual: bool,
    due_only: bool,
) -> Result<RefreshResult, String> {
    let (_, stored) = read_stored_sources(vault_path)?;
    // Read once for the whole sweep rather than once per feed.
    let rules = feed_get_rules(vault_path.to_string()).unwrap_or_default();

    // A person pressing refresh gets the fetch they asked for; a background
    // sweep respects the backoff, because the whole point of the backoff is
    // that the sweep should stop hammering a feed that keeps refusing.
    //
    // This has to be told, not inferred from whether one feed was named: the
    // automatic sweep names each feed in turn, so reading `source_id` as "a
    // person asked" would have exempted the only caller the backoff exists
    // for.
    let (backing_off, states) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let conn = db.conn();
        let backing_off = if manual {
            std::collections::HashSet::new()
        } else {
            sources_in_backoff(conn)
        };
        (backing_off, load_source_states(conn))
    };

    let now = chrono::Utc::now();
    let targets: Vec<RefreshTarget> = stored
        .iter()
        .filter(|s| !s.is_paused && source_id.as_ref().map(|sid| s.id == *sid).unwrap_or(true))
        .filter(|s| !backing_off.contains(&s.id))
        .filter(|s| !due_only || is_due(states.get(&s.id), s.update_interval, now))
        .map(|s| {
            let state = states.get(&s.id);
            RefreshTarget {
                id: s.id.clone(),
                url: s.url.clone(),
                feed_type: s.feed_type.clone(),
                etag: state
                    .map(|st| st.etag.clone())
                    .filter(|e| !e.is_empty()),
                last_modified: state
                    .map(|st| st.last_modified.clone())
                    .filter(|m| !m.is_empty()),
                scrape_container: s.scrape_container.clone(),
            }
        })
        .collect();

    // Phase one: the network, concurrently.
    let fetched: Vec<(RefreshTarget, FetchOutcome)> = futures::stream::iter(
        targets.into_iter().map(|target| async move {
            let outcome = if target.feed_type == "scrape" {
                match scrape_fetch(&target.url, &target.scrape_container).await {
                    Ok(articles) => FetchOutcome::Scraped(articles),
                    Err(e) => FetchOutcome::Failed {
                        message: format!("Scrape error for {}: {}", target.url, e),
                        retry_after: None,
                    },
                }
            } else {
                match fetcher::fetch_feed(
                    &target.url,
                    target.etag.as_deref(),
                    target.last_modified.as_deref(),
                )
                .await
                {
                    fetcher::FetchResult::NotModified => FetchOutcome::NotModified,
                    fetcher::FetchResult::Updated {
                        body,
                        etag,
                        last_modified,
                    } => match parser::parse_feed(&body) {
                        Ok(articles) => FetchOutcome::Feed {
                            articles,
                            etag,
                            last_modified,
                        },
                        Err(e) => FetchOutcome::Failed {
                            message: format!("Parse error for {}: {}", target.url, e),
                            retry_after: None,
                        },
                    },
                    fetcher::FetchResult::Error {
                        message,
                        retry_after,
                    } => FetchOutcome::Failed {
                        message: format!("Fetch error for {}: {}", target.url, message),
                        retry_after,
                    },
                }
            };
            (target, outcome)
        }),
    )
    .buffer_unordered(REFRESH_CONCURRENCY)
    .collect()
    .await;

    // Phase two: the database, once, holding the lock for one pass rather than
    // taking and dropping it several times per feed.
    let mut result = RefreshResult {
        total_fetched: 0,
        total_new: 0,
        errors: Vec::new(),
    };

    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    for (target, outcome) in fetched {
        match outcome {
            FetchOutcome::NotModified => {
                record_fetch_success(conn, &target.id, None, None);
            }
            FetchOutcome::Feed {
                articles,
                etag,
                last_modified,
            } => {
                let found = articles.len();
                result.total_fetched += found;
                let new_count = insert_articles(conn, &target.id, &articles, &rules)?;
                result.total_new += new_count;
                log_fetch(conn, &target.id, "ok", found, new_count, None)?;
                record_fetch_success(
                    conn,
                    &target.id,
                    etag.as_deref(),
                    last_modified.as_deref(),
                );
            }
            FetchOutcome::Scraped(articles) => {
                let found = articles.len();
                result.total_fetched += found;
                let new_count = insert_scraped_articles(conn, &target.id, &articles, &rules)?;
                result.total_new += new_count;
                log_fetch(conn, &target.id, "ok", found, new_count, None)?;
                record_fetch_success(conn, &target.id, None, None);
            }
            FetchOutcome::Failed {
                message,
                retry_after,
            } => {
                result.errors.push(message.clone());
                log_fetch(conn, &target.id, "error", 0, 0, Some(&message))?;
                record_fetch_failure(conn, &target.id, &message, retry_after);
            }
        }
    }

    // `sources.json` is not written here at all any more. Nothing this
    // function learns belongs to the vault.
    Ok(result)
}

/// Whether a feed's own interval has elapsed since this device last fetched it.
///
/// A feed never fetched here is due immediately; so is one whose recorded time
/// cannot be parsed, because refusing to guess would strand it forever.
fn is_due(
    state: Option<&SourceState>,
    update_interval_minutes: i64,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    let Some(state) = state else { return true };
    if state.last_fetched_at.is_empty() {
        return true;
    }
    let Ok(last) = chrono::DateTime::parse_from_rfc3339(&state.last_fetched_at) else {
        return true;
    };
    let interval = chrono::Duration::minutes(update_interval_minutes.max(1));
    now.signed_duration_since(last.with_timezone(&chrono::Utc)) >= interval
}

/// Fetch a homepage and lift the article cards off it. Network only.
async fn scrape_fetch(
    url: &str,
    container_override: &str,
) -> Result<Vec<scrape::ScrapedArticle>, String> {
    let html = fetcher::fetch_page(url).await?;
    Ok(scrape::scrape_articles(&html, url, Some(container_override)))
}

/// Insert scraped cards, which carry a summary and a link but no body.
fn insert_scraped_articles(
    conn: &rusqlite::Connection,
    source_id: &str,
    articles: &[scrape::ScrapedArticle],
    rules: &[FeedRule],
) -> Result<usize, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut new_count = 0;

    let mut insert_stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO feed_articles
             (id, feed_source_id, guid, title, url, author, content, summary,
              published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
              content_type, is_read, is_starred, is_read_later, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, 0, ?12, ?13, ?14, 0, ?15)",
        )
        .map_err(|e| format!("Prepare error: {}", e))?;

    for article in articles {
        let verdict = judge(
            rules,
            source_id,
            &LoweredFields {
                title: article.title.to_lowercase(),
                summary: article.summary.to_lowercase(),
                author: String::new(),
            },
        );
        if verdict.mute {
            continue;
        }

        let inserted = insert_stmt
            .execute(params![
                uuid::Uuid::new_v4().to_string(),
                source_id,
                article.url, // the link is the only stable identity a card has
                article.title,
                article.url,
                "",
                "", // no body until the article's own page is fetched
                article.summary,
                if article.published_at.is_empty() {
                    &now
                } else {
                    &article.published_at
                },
                now,
                article.thumbnail_url,
                "scrape",
                verdict.mark_read as i64,
                verdict.star as i64,
                join_tags(&verdict.tags),
            ])
            .map_err(|e| format!("Insert error: {}", e))?;

        if inserted > 0 {
            new_count += 1;
        }
    }

    Ok(new_count)
}


/// Insert parsed articles into the database, returning the count of newly inserted articles.
fn insert_articles(
    conn: &rusqlite::Connection,
    source_id: &str,
    articles: &[parser::ParsedArticle],
    rules: &[FeedRule],
) -> Result<usize, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut new_count = 0;

    let mut insert_stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO feed_articles
             (id, feed_source_id, guid, title, url, author, content, summary,
              published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
              content_type, is_read, is_starred, is_read_later, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0, ?17)",
        )
        .map_err(|e| format!("Prepare error: {}", e))?;

    for article in articles {
        let verdict = judge(
            rules,
            source_id,
            &LoweredFields {
                title: article.title.to_lowercase(),
                summary: article.summary.to_lowercase(),
                author: article.author.to_lowercase(),
            },
        );
        // A muted article is never stored, so it costs nothing to keep and
        // shows up in no count. Unmuting the rule brings it back on the next
        // refresh that carries it.
        if verdict.mute {
            continue;
        }

        let inserted = insert_stmt
            .execute(params![
                uuid::Uuid::new_v4().to_string(),
                source_id,
                article.guid,
                article.title,
                article.url,
                article.author,
                article.content,
                article.summary,
                article.published_at,
                now,
                article.thumbnail_url,
                article.word_count,
                article.read_time_minutes,
                article.content_type,
                verdict.mark_read as i64,
                verdict.star as i64,
                join_tags(&verdict.tags),
            ])
            .map_err(|e| format!("Insert error: {}", e))?;

        if inserted > 0 {
            new_count += 1;
        }
    }

    Ok(new_count)
}

/// How long to wait after `error_count` consecutive failures, in minutes.
///
/// A feed that has moved, or gone, or is rate-limiting us does not become
/// reachable by being asked again on the same schedule. Doubling from five
/// minutes reaches the six-hour ceiling after seven failures, which is roughly
/// a day of a feed being broken before we settle into asking four times a day.
fn backoff_delay_minutes(error_count: i64) -> i64 {
    const BASE_MINUTES: i64 = 5;
    const MAX_MINUTES: i64 = 6 * 60;
    let doublings = (error_count - 1).clamp(0, 16) as u32;
    (BASE_MINUTES.saturating_mul(1i64 << doublings)).min(MAX_MINUTES)
}

/// Record that this device could not reach a feed, and push back its next try.
///
/// `retry_after` is what the server asked for. Where it and our own backoff
/// disagree the longer wait wins: a server saying "thirty seconds" does not
/// mean a feed that has failed nine times deserves another try in thirty
/// seconds, and a server saying "two hours" is not a request to argue with.
fn record_fetch_failure(
    conn: &rusqlite::Connection,
    source_id: &str,
    message: &str,
    retry_after: Option<i64>,
) {
    let previous: i64 = conn
        .query_row(
            "SELECT error_count FROM feed_source_state WHERE source_id = ?1",
            params![source_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let error_count = previous + 1;
    let our_wait = chrono::Duration::minutes(backoff_delay_minutes(error_count));
    let asked = retry_after.map(chrono::Duration::seconds).unwrap_or_default();
    let next_retry_at = (chrono::Utc::now() + our_wait.max(asked)).to_rfc3339();

    let _ = conn.execute(
        "INSERT INTO feed_source_state (source_id, last_error, error_count, next_retry_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(source_id) DO UPDATE SET
             last_error = ?2, error_count = ?3, next_retry_at = ?4",
        params![source_id, message, error_count, next_retry_at],
    );
}

/// Note a fetch that worked: refresh the caching headers, forget the failures.
///
/// `None` for a header means "keep whatever is stored", which is what a 304
/// needs — the server said nothing changed, including the ETag it validated
/// against, and overwriting it with nothing would force a full download next
/// time.
fn record_fetch_success(
    conn: &rusqlite::Connection,
    source_id: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let _ = conn.execute(
        "INSERT INTO feed_source_state
             (source_id, etag, last_modified, last_fetched_at, last_error, error_count, next_retry_at)
         VALUES (?1, COALESCE(?2, ''), COALESCE(?3, ''), ?4, '', 0, '')
         ON CONFLICT(source_id) DO UPDATE SET
             etag = COALESCE(?2, etag),
             last_modified = COALESCE(?3, last_modified),
             last_fetched_at = ?4,
             last_error = '',
             error_count = 0,
             next_retry_at = ''",
        params![source_id, etag, last_modified, now],
    );
}

/// Forget everything this device knew about fetching a feed.
fn clear_fetch_state(conn: &rusqlite::Connection, source_id: &str) {
    let _ = conn.execute(
        "DELETE FROM feed_source_state WHERE source_id = ?1",
        params![source_id],
    );
}

/// This device's fetch state for every feed it has an opinion about.
fn load_source_states(conn: &rusqlite::Connection) -> HashMap<String, SourceState> {
    let mut stmt = match conn.prepare(
        "SELECT source_id, etag, last_modified, last_fetched_at, last_error FROM feed_source_state",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return HashMap::new(),
    };
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            SourceState {
                etag: row.get(1)?,
                last_modified: row.get(2)?,
                last_fetched_at: row.get(3)?,
                last_error: row.get(4)?,
            },
        ))
    });
    match rows {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => HashMap::new(),
    }
}

/// Move fetch state out of a `sources.json` written by an earlier version.
///
/// Runs once per feed: if the database already knows about a source, whatever
/// the file says is older news and is discarded. Returns whether the file
/// should now be rewritten without those fields.
fn migrate_legacy_state(conn: &rusqlite::Connection, sources: &mut [StoredFeedSource]) -> bool {
    let mut migrated = false;

    for source in sources.iter_mut() {
        if !source.has_legacy_state() {
            continue;
        }
        migrated = true;

        let known: bool = conn
            .query_row(
                "SELECT 1 FROM feed_source_state WHERE source_id = ?1",
                params![source.id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !known {
            let _ = conn.execute(
                "INSERT INTO feed_source_state
                     (source_id, etag, last_modified, last_fetched_at, last_error)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    source.id,
                    source.etag.clone().unwrap_or_default(),
                    source.last_modified_header.clone().unwrap_or_default(),
                    source.last_fetched_at.clone().unwrap_or_default(),
                    source.last_error.clone().unwrap_or_default(),
                ],
            );
        }

        source.etag = None;
        source.last_modified_header = None;
        source.last_fetched_at = None;
        source.last_error = None;
    }

    migrated
}

/// Feeds whose next attempt is not due yet.
fn sources_in_backoff(conn: &rusqlite::Connection) -> std::collections::HashSet<String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut stmt = match conn
        .prepare("SELECT source_id FROM feed_source_state WHERE datetime(next_retry_at) > datetime(?1)")
    {
        Ok(stmt) => stmt,
        Err(_) => return std::collections::HashSet::new(),
    };
    let rows = match stmt.query_map(params![now], |row| row.get::<_, String>(0)) {
        Ok(rows) => rows,
        Err(_) => return std::collections::HashSet::new(),
    };
    rows.filter_map(|r| r.ok()).collect()
}

/// Log a fetch operation.
fn log_fetch(
    conn: &rusqlite::Connection,
    source_id: &str,
    status: &str,
    found: usize,
    new: usize,
    error: Option<&str>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO feed_fetch_log (id, feed_source_id, fetched_at, status, articles_found, articles_new, error_message)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            uuid::Uuid::new_v4().to_string(),
            source_id,
            chrono::Utc::now().to_rfc3339(),
            status,
            found as i64,
            new as i64,
            error,
        ],
    )
    .map_err(|e| format!("Log insert error: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn feed_discover(url: String) -> Result<Vec<discovery::DiscoveredFeed>, String> {
    discovery::discover_feeds(&url).await
}

/// Fetch an article's own page and extract the body from it.
///
/// Called automatically when an article arrived without one — a scraped card,
/// or a feed whose `<content>` was empty — and on request for feeds that
/// publish only a teaser, which is most of them.
///
/// `force` is what separates the two: without it, an article that already has
/// a body is returned untouched, which is what makes the automatic call cheap
/// to repeat.
#[tauri::command]
pub async fn feed_fetch_article_content(
    db: tauri::State<'_, DbState>,
    article_id: String,
    force: Option<bool>,
) -> Result<CachedArticle, String> {
    let force = force.unwrap_or(false);

    // 1. Get article from DB
    let (url, current_content) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let conn = db.conn();
        conn.query_row(
            "SELECT url, content FROM feed_articles WHERE id = ?1",
            params![article_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|e| format!("Article not found: {}", e))?
    };

    // 2. If already has content, just return the article
    if !current_content.is_empty() && !force {
        let db = db.lock().map_err(|e| e.to_string())?;
        let conn = db.conn();
        let article = conn
            .query_row(
                &format!("SELECT {} FROM feed_articles WHERE id = ?1", ARTICLE_COLUMNS),
                params![article_id],
                row_to_article,
            )
            .map_err(|e| format!("Query error: {}", e))?;
        return Ok(article);
    }

    // 3. Fetch the article page
    let html = fetcher::fetch_page(&url).await?;

    // 4. Extract content using readability
    let extracted = readability::extract_content(&html, &url);

    // 5. Update article in DB — but only if the extraction actually found
    //    more than the feed already gave us. Readability is a guess, and on a
    //    page it cannot read it returns very little; writing that over a
    //    perfectly good feed body would destroy the article to no purpose,
    //    and there is no copy to restore it from.
    let worth_keeping = extracted.content.len() > current_content.len();
    if worth_keeping {
        let db = db.lock().map_err(|e| e.to_string())?;
        let conn = db.conn();
        conn.execute(
            "UPDATE feed_articles SET content = ?1, author = CASE WHEN author = '' THEN ?2 ELSE author END,
             word_count = ?3, read_time_minutes = ?4,
             thumbnail_url = CASE WHEN thumbnail_url = '' THEN ?5 ELSE thumbnail_url END,
             content_type = 'full-text'
             WHERE id = ?6",
            params![
                extracted.content,
                extracted.author,
                extracted.word_count,
                extracted.read_time_minutes,
                extracted.thumbnail_url,
                article_id,
            ],
        )
        .map_err(|e| format!("Update error: {}", e))?;
    } else {
        // Mark the attempt so the reader does not try again on every open.
        let db = db.lock().map_err(|e| e.to_string())?;
        db.conn()
            .execute(
                "UPDATE feed_articles SET content_type = 'full-text' WHERE id = ?1",
                params![article_id],
            )
            .map_err(|e| format!("Update error: {}", e))?;
    }

    // 6. Return updated article
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();
    conn.query_row(
        &format!("SELECT {} FROM feed_articles WHERE id = ?1", ARTICLE_COLUMNS),
        params![article_id],
        row_to_article,
    )
    .map_err(|e| format!("Query error: {}", e))
}

// ═══════════════════════════════════════════════════════════════
//  HIGHLIGHTS
// ═══════════════════════════════════════════════════════════════

/// A passage the reader marked, and optionally a thought about it.
///
/// A highlight is located by its exact text plus which occurrence of that text
/// it is. Storing a character offset would be more precise and would survive
/// nothing: the same article is re-extracted when its full text is fetched,
/// and any offset taken before that points into the wrong sentence after. The
/// words themselves are the one part that does not move.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub id: String,
    pub source_id: String,
    pub guid: String,
    pub text: String,
    /// Which occurrence of `text` within the article this one is, counting
    /// from zero — an article quoting the same phrase twice has two places it
    /// could mean.
    pub occurrence: i64,
    pub note: String,
    pub created_at: String,
}

fn row_to_highlight(row: &rusqlite::Row) -> rusqlite::Result<Highlight> {
    Ok(Highlight {
        id: row.get(0)?,
        source_id: row.get(1)?,
        guid: row.get(2)?,
        text: row.get(3)?,
        occurrence: row.get(4)?,
        note: row.get(5)?,
        created_at: row.get(6)?,
    })
}

/// Which feed and guid an article id belongs to.
fn article_identity(
    conn: &rusqlite::Connection,
    article_id: &str,
) -> Result<(String, String), String> {
    conn.query_row(
        "SELECT feed_source_id, guid FROM feed_articles WHERE id = ?1",
        params![article_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|e| format!("Article not found: {}", e))
}

#[tauri::command]
pub fn feed_get_highlights(
    db: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<Vec<Highlight>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();
    let (source_id, guid) = article_identity(conn, &article_id)?;

    let mut stmt = conn
        .prepare(
            "SELECT id, source_id, guid, text, occurrence, note, created_at
             FROM feed_highlights
             WHERE source_id = ?1 AND guid = ?2
             ORDER BY occurrence ASC, created_at ASC",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let highlights = stmt
        .query_map(params![source_id, guid], row_to_highlight)
        .map_err(|e| format!("Query map error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(highlights)
}

#[tauri::command]
pub fn feed_add_highlight(
    db: tauri::State<'_, DbState>,
    article_id: String,
    text: String,
    occurrence: i64,
    note: Option<String>,
) -> Result<Highlight, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Nothing was selected".to_string());
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();
    let (source_id, guid) = article_identity(conn, &article_id)?;

    let highlight = Highlight {
        id: uuid::Uuid::new_v4().to_string(),
        source_id,
        guid,
        text: trimmed.to_string(),
        occurrence: occurrence.max(0),
        note: note.unwrap_or_default(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    conn.execute(
        "INSERT INTO feed_highlights
            (id, source_id, guid, text, occurrence, note, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            highlight.id,
            highlight.source_id,
            highlight.guid,
            highlight.text,
            highlight.occurrence,
            highlight.note,
            highlight.created_at,
        ],
    )
    .map_err(|e| format!("Insert error: {}", e))?;

    Ok(highlight)
}

#[tauri::command]
pub fn feed_remove_highlight(
    db: tauri::State<'_, DbState>,
    highlight_id: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.conn()
        .execute(
            "DELETE FROM feed_highlights WHERE id = ?1",
            params![highlight_id],
        )
        .map_err(|e| format!("Delete error: {}", e))?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  RULES
// ═══════════════════════════════════════════════════════════════

/// A standing instruction about arriving articles.
///
/// Rules are what turn a subscription you mostly want into one you want: a
/// feed that is ninety per cent useful and ten per cent press releases is
/// otherwise a choice between reading the press releases and unsubscribing.
///
/// They live in `sources.json`'s neighbour `rules.json`, in the vault, because
/// they are something a person wrote and should follow them between devices.
/// Unlike fetch state they change only when edited, so a synced file is safe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Which feeds it applies to. Empty means all of them.
    #[serde(default)]
    pub source_ids: Vec<String>,
    /// `title`, `summary`, `author`, or `any`.
    #[serde(default = "default_field")]
    pub field: String,
    /// Matched case-insensitively, as a substring.
    pub contains: String,

    #[serde(default)]
    pub mark_read: bool,
    #[serde(default)]
    pub star: bool,
    /// Drop the article rather than storing it.
    #[serde(default)]
    pub mute: bool,
    /// A tag to attach; empty attaches none.
    #[serde(default)]
    pub tag: String,
}

fn default_true() -> bool {
    true
}
fn default_field() -> String {
    "any".to_string()
}

/// What the rules decided about one arriving article.
#[derive(Debug, Default, Clone)]
struct RuleVerdict {
    mute: bool,
    mark_read: bool,
    star: bool,
    tags: Vec<String>,
}

impl FeedRule {
    fn applies_to(&self, source_id: &str) -> bool {
        self.enabled && (self.source_ids.is_empty() || self.source_ids.iter().any(|s| s == source_id))
    }

    /// Whether the article's text contains this rule's term.
    ///
    /// The haystack is lower-cased once per field rather than per rule, so a
    /// long rule list over a big refresh does not re-fold the same strings.
    fn matches(&self, fields: &LoweredFields) -> bool {
        let needle = self.contains.trim().to_lowercase();
        if needle.is_empty() {
            return false;
        }
        match self.field.as_str() {
            "title" => fields.title.contains(&needle),
            "summary" => fields.summary.contains(&needle),
            "author" => fields.author.contains(&needle),
            _ => {
                fields.title.contains(&needle)
                    || fields.summary.contains(&needle)
                    || fields.author.contains(&needle)
            }
        }
    }
}

struct LoweredFields {
    title: String,
    summary: String,
    author: String,
}

/// Run every applicable rule over one article and combine the results.
///
/// Rules add rather than override: two rules that both want to star it star it
/// once, and one rule muting it means muted whatever the others said. There is
/// no ordering to learn, which is the point — a rule list is not a program.
fn judge(rules: &[FeedRule], source_id: &str, fields: &LoweredFields) -> RuleVerdict {
    let mut verdict = RuleVerdict::default();

    for rule in rules.iter().filter(|r| r.applies_to(source_id)) {
        if !rule.matches(fields) {
            continue;
        }
        verdict.mute |= rule.mute;
        verdict.mark_read |= rule.mark_read;
        verdict.star |= rule.star;

        let tag = rule.tag.trim();
        if !tag.is_empty() && !verdict.tags.iter().any(|t| t == tag) {
            verdict.tags.push(tag.to_string());
        }
    }

    verdict
}

#[tauri::command]
pub fn feed_get_rules(vault_path: String) -> Result<Vec<FeedRule>, String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    read_json_file(&feeds_dir.join("rules.json"))
}

#[tauri::command]
pub fn feed_save_rules(vault_path: String, rules: Vec<FeedRule>) -> Result<(), String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    write_json_file(&feeds_dir.join("rules.json"), &rules)
}

/// Run the rules over articles already in the cache.
///
/// A rule written today is usually a reaction to something read yesterday, so
/// it has to be able to reach backwards. Muting is deliberately not applied
/// here: deleting things the reader already has, on the strength of a rule
/// they are still writing, is not a mistake worth making silently.
#[tauri::command]
pub fn feed_apply_rules(
    db: tauri::State<'_, DbState>,
    vault_path: String,
) -> Result<usize, String> {
    let rules = feed_get_rules(vault_path)?;
    if rules.is_empty() {
        return Ok(0);
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let existing: Vec<(String, String, String, String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, feed_source_id, title, summary, author, tags FROM feed_articles")
            .map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(|e| format!("Query map error: {}", e))?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut changed = 0;
    for (id, source_id, title, summary, author, stored_tags) in existing {
        let fields = LoweredFields {
            title: title.to_lowercase(),
            summary: summary.to_lowercase(),
            author: author.to_lowercase(),
        };
        let verdict = judge(&rules, &source_id, &fields);
        if !verdict.mark_read && !verdict.star && verdict.tags.is_empty() {
            continue;
        }

        let mut tags = split_tags(&stored_tags);
        for tag in verdict.tags {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }

        changed += conn
            .execute(
                "UPDATE feed_articles
                    SET is_read = CASE WHEN ?2 = 1 THEN 1 ELSE is_read END,
                        is_starred = CASE WHEN ?3 = 1 THEN 1 ELSE is_starred END,
                        tags = ?4
                  WHERE id = ?1
                    AND (tags != ?4
                         OR (?2 = 1 AND is_read = 0)
                         OR (?3 = 1 AND is_starred = 0))",
                params![id, verdict.mark_read as i64, verdict.star as i64, join_tags(&tags)],
            )
            .map_err(|e| format!("Update error: {}", e))?;
    }

    Ok(changed)
}

// ═══════════════════════════════════════════════════════════════
//  ARTICLE IMAGES
// ═══════════════════════════════════════════════════════════════

fn image_cache_dir(app_handle: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    Ok(app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data directory: {}", e))?
        .join(image_cache::CACHE_DIR_NAME))
}

/// Fetch the images an article uses and say where they landed.
///
/// The reader asks for a batch and swaps the local paths into the markup
/// before it renders, so the webview never dials the publisher: no tracking
/// pixel fires, no address is handed over, and the article reads the same
/// offline as on.
///
/// URLs that could not be fetched are left out of the map rather than reported
/// as errors. A missing picture is not a reason to fail to open an article.
#[tauri::command]
pub async fn feed_cache_images(
    app_handle: tauri::AppHandle,
    urls: Vec<String>,
) -> Result<HashMap<String, String>, String> {
    const MAX_IMAGES_PER_ARTICLE: usize = 60;
    const IMAGE_CONCURRENCY: usize = 4;

    let cache_dir = image_cache_dir(&app_handle)?;

    // Same URL twice in one article is one fetch.
    let mut wanted: Vec<String> = Vec::new();
    for url in urls.into_iter().take(MAX_IMAGES_PER_ARTICLE) {
        if !wanted.contains(&url) {
            wanted.push(url);
        }
    }

    let results: Vec<(String, Option<std::path::PathBuf>)> = futures::stream::iter(
        wanted.into_iter().map(|url| {
            let cache_dir = cache_dir.clone();
            async move {
                let path = image_cache::cache_image(&cache_dir, &url).await;
                (url, path)
            }
        }),
    )
    .buffer_unordered(IMAGE_CONCURRENCY)
    .collect()
    .await;

    Ok(results
        .into_iter()
        .filter_map(|(url, path)| Some((url, path?.to_string_lossy().to_string())))
        .collect())
}

// ═══════════════════════════════════════════════════════════════
//  BACKGROUND REFRESH
// ═══════════════════════════════════════════════════════════════

/// Holds the running scheduler so a second vault does not start a second one.
#[derive(Default)]
pub struct FeedSchedulerState {
    shutdown: std::sync::Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
}

/// How often the scheduler wakes to see whether anything is due. Each feed's
/// own interval decides whether it is actually fetched.
///
/// Not compiled on Android: there is no background loop there to tick. See
/// `feed_start_scheduler`, which refreshes in the foreground instead because a
/// timer the OS kills is worse than no timer at all.
#[cfg(not(target_os = "android"))]
const SCHEDULER_TICK_SECONDS: u64 = 5 * 60;

/// Start refreshing feeds on a timer, whether or not the Feeds app is open.
///
/// The refresh loop used to live in the Feeds mini-app's `onMounted`, so feeds
/// updated only while a person was looking at them — open the app after three
/// days away and you waited for it. On Android there is no equivalent: the OS
/// stops the process, and a timer that the system kills is worse than none, so
/// that build keeps refreshing in the foreground.
#[tauri::command]
pub fn feed_start_scheduler(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, FeedSchedulerState>,
    vault_path: String,
) -> Result<(), String> {
    feed_stop_scheduler(state.clone())?;

    #[cfg(not(target_os = "android"))]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use tauri::{Emitter, Manager};

        let shutdown = std::sync::Arc::new(AtomicBool::new(false));
        {
            let mut slot = state.shutdown.lock().map_err(|e| e.to_string())?;
            *slot = Some(shutdown.clone());
        }

        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(SCHEDULER_TICK_SECONDS)).await;
                if shutdown.load(Ordering::Relaxed) {
                    return;
                }

                let db = app_handle.state::<DbState>();
                match refresh_sources(&db, &vault_path, None, false, true).await {
                    Ok(result) if result.total_new > 0 || !result.errors.is_empty() => {
                        let _ = app_handle.emit("feeds:refreshed", &result);
                    }
                    Ok(_) => {}
                    Err(e) => log::warn!("scheduled feed refresh failed: {}", e),
                }
            }
        });
    }

    #[cfg(target_os = "android")]
    {
        let _ = (app_handle, vault_path);
    }

    Ok(())
}

/// Signal a running scheduler to stop at its next tick.
///
/// Not a command: nothing outside asks for this. It exists so that starting
/// the scheduler twice — a second vault, a reload — does not leave two of them
/// racing each other.
fn feed_stop_scheduler(state: tauri::State<'_, FeedSchedulerState>) -> Result<(), String> {
    let mut slot = state.shutdown.lock().map_err(|e| e.to_string())?;
    if let Some(flag) = slot.take() {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  READ STATE, ACROSS DEVICES
// ═══════════════════════════════════════════════════════════════

/// Publish what this device decided, then take in what the others decided.
///
/// Cheap to call: publishing skips the write when nothing changed, and
/// applying skips the work when no other device's file has moved. `force`
/// overrides the second of those, which a refresh needs — articles that
/// arrived seconds ago were not in the database last time this ran, so the
/// state waiting for them had nothing to attach to.
#[tauri::command]
pub fn feed_state_sync(
    db: tauri::State<'_, DbState>,
    vault_path: String,
    force: Option<bool>,
) -> Result<usize, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let device_id = ensure_feed_device_id(&db)?;
    let conn = db.conn();

    state_sync::publish(conn, &vault_path, &device_id)?;

    let last_seen = db.get_kv(STATE_FINGERPRINT_KEY).ok().flatten();
    let (changed, fingerprint) = state_sync::apply(
        conn,
        &vault_path,
        &device_id,
        force.unwrap_or(false),
        last_seen.as_deref(),
    )?;

    let _ = db.set_kv(STATE_FINGERPRINT_KEY, &fingerprint);
    Ok(changed)
}

/// What the other devices' state files looked like the last time they were
/// applied, so an unchanged set can be skipped.
const STATE_FINGERPRINT_KEY: &str = "feeds_state_fingerprint";

/// The stable per-device id, shared with the sync layer so a device is one
/// device everywhere.
fn ensure_feed_device_id(db: &crate::db::DbBridge) -> Result<String, String> {
    match db.get_kv("device_id") {
        Ok(Some(id)) if !id.is_empty() => Ok(id),
        _ => {
            let id = uuid::Uuid::new_v4().to_string();
            db.set_kv("device_id", &id)
                .map_err(|e| format!("failed to persist device_id: {}", e))?;
            Ok(id)
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  MAINTENANCE
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn feed_run_cleanup(
    app_handle: tauri::AppHandle,
    db: tauri::State<'_, DbState>,
    max_age_days: i64,
    max_per_feed: i64,
) -> Result<cleanup::CleanupResult, String> {
    // Cached images follow the articles they belong to. Anything pruned here
    // is fetched again the next time an article that uses it is opened, so the
    // cutoff can match the articles' own without risking anything.
    if let Ok(cache_dir) = image_cache_dir(&app_handle) {
        image_cache::prune(&cache_dir, max_age_days);
    }

    // Highlights are not cleaned up with the articles they point at. A
    // highlight is something the reader wrote; the article is a cached copy of
    // somebody else's page, and cleanup deleting the first because it deleted
    // the second would be the app throwing away the only part that was theirs.

    let db = db.lock().map_err(|e| e.to_string())?;
    cleanup::run_cleanup(db.conn(), max_age_days, max_per_feed)
}

/// Colours handed to categories an OPML file brings with it, in the order the
/// Add Feed dialog offers them.
const CATEGORY_COLORS: &[&str] = &[
    "#f97316", "#ef4444", "#8b5cf6", "#3b82f6", "#10b981", "#f59e0b", "#ec4899", "#6366f1",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpmlImportResult {
    pub added: usize,
    pub skipped: usize,
    pub categories_created: usize,
}

/// Import subscriptions from an OPML file into the vault.
///
/// This used to parse the file and hand the result back for the front end to
/// do something with, which it never did — the feeds were dropped on the floor
/// and the dialog reported success. Writing them is the whole job, so it
/// happens here, where `sources.json` and `categories.json` are already owned.
///
/// No feed is fetched on the way in. An OPML file already carries the feed URL
/// and its title, and running discovery over a two-hundred-feed export would
/// hold the dialog open for minutes to learn what the file just said. The
/// first refresh fills in the rest.
#[tauri::command]
pub fn feed_import_opml(
    vault_path: String,
    opml_content: String,
) -> Result<OpmlImportResult, String> {
    let imported = feed_opml::import_opml(&opml_content)?;

    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let sources_path = feeds_dir.join("sources.json");
    let categories_path = feeds_dir.join("categories.json");

    let mut sources: Vec<StoredFeedSource> = read_json_file(&sources_path)?;
    let mut categories: Vec<FeedCategory> = read_json_file(&categories_path)?;
    let interval = feed_get_config(vault_path.clone())
        .map(|c| c.global_update_interval)
        .unwrap_or_else(|_| default_update_interval());

    let mut result = OpmlImportResult {
        added: 0,
        skipped: 0,
        categories_created: 0,
    };

    for feed in imported {
        if feed.url.trim().is_empty() {
            continue;
        }

        // Importing the same file twice should be a no-op, not a second copy
        // of every subscription.
        if sources.iter().any(|s| s.url == feed.url) {
            result.skipped += 1;
            continue;
        }

        let category_id = if feed.category.trim().is_empty() {
            String::new()
        } else {
            let existing = categories
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(feed.category.trim()))
                .map(|c| c.id.clone());
            match existing {
                Some(id) => id,
                None => {
                    let category = FeedCategory {
                        id: format!("cat-opml-{}", uuid::Uuid::new_v4()),
                        name: feed.category.trim().to_string(),
                        color: CATEGORY_COLORS[categories.len() % CATEGORY_COLORS.len()]
                            .to_string(),
                        sort_order: categories.len() as i64,
                        is_collapsed: false,
                    };
                    let id = category.id.clone();
                    categories.push(category);
                    result.categories_created += 1;
                    id
                }
            }
        };

        let feed_type = match feed.feed_type.to_ascii_lowercase().as_str() {
            "atom" => "atom",
            "json" => "json",
            _ => "rss",
        };

        let title = if feed.title.trim().is_empty() {
            feed.url.clone()
        } else {
            feed.title.trim().to_string()
        };

        sources.push(StoredFeedSource {
            id: uuid::Uuid::new_v4().to_string(),
            url: feed.url,
            site_url: feed.site_url,
            feed_type: feed_type.to_string(),
            title,
            description: String::new(),
            icon_url: String::new(),
            category_id,
            update_interval: interval,
            is_paused: false,
            added_at: chrono::Utc::now().to_rfc3339(),
            full_text_fetch: false,
            scrape_container: String::new(),
            last_fetched_at: None,
            last_error: None,
            etag: None,
            last_modified_header: None,
        });
        result.added += 1;
    }

    if result.categories_created > 0 {
        write_json_file(&categories_path, &categories)?;
    }
    if result.added > 0 {
        write_json_file(&sources_path, &sources)?;
    }

    Ok(result)
}

#[tauri::command]
pub fn feed_export_opml(vault_path: String) -> Result<String, String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let sources: Vec<StoredFeedSource> = read_json_file(&feeds_dir.join("sources.json"))?;
    let categories: Vec<FeedCategory> = read_json_file(&feeds_dir.join("categories.json"))?;

    // Build category name lookup
    let cat_map: HashMap<String, String> = categories
        .iter()
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect();

    // Convert to ExportSource
    let export_sources: Vec<feed_opml::ExportSource> = sources
        .iter()
        .map(|s| feed_opml::ExportSource {
            url: s.url.clone(),
            site_url: s.site_url.clone(),
            title: s.title.clone(),
            description: s.description.clone(),
            category_name: cat_map.get(&s.category_id).cloned().unwrap_or_default(),
        })
        .collect();

    Ok(feed_opml::export_opml(&export_sources))
}

// ═══════════════════════════════════════════════════════════════
//  OPEN URL (system default browser)
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    opener::open(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;

    fn seeded_db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full schema")
    }

    fn insert(conn: &rusqlite::Connection, id: &str, source: &str, title: &str, read: bool) {
        conn.execute(
            "INSERT INTO feed_articles
                (id, feed_source_id, guid, title, url, author, content, summary,
                 published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                 content_type, is_read, is_starred, is_read_later)
             VALUES (?1, ?2, ?3, ?4, '', '', 'body text', '', '2026-08-20T00:00:00Z',
                     '2026-08-20T00:00:00Z', '', 0, 1, 'text/html', ?5, 0, 0)",
            params![id, source, id, title, read as i64],
        )
        .expect("insert article");
    }

    fn search_titles(conn: &rusqlite::Connection, query: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT a.title FROM feed_articles a
                 JOIN feed_articles_fts fts ON a.rowid = fts.rowid
                 WHERE feed_articles_fts MATCH ?1",
            )
            .expect("prepare search");
        let rows = stmt
            .query_map(params![query], |row| row.get::<_, String>(0))
            .expect("search");
        rows.filter_map(|r| r.ok()).collect()
    }

    // ── The index is the database's job ──────────────────────────────

    #[test]
    fn deleting_an_article_takes_its_index_entry_with_it() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Kingfisher", false);

        assert_eq!(search_titles(conn, "Kingfisher"), vec!["Kingfisher"]);

        conn.execute("DELETE FROM feed_articles WHERE id = 'a1'", [])
            .expect("delete");

        assert!(
            search_titles(conn, "Kingfisher").is_empty(),
            "a deleted article must not still be findable"
        );
    }

    #[test]
    fn a_reused_rowid_does_not_inherit_the_old_articles_words() {
        // The failure this guards against is not a missing result but a wrong
        // one: SQLite hands out deleted rowids again, so a stale index entry
        // starts naming whichever article lands on the rowid next.
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Kingfisher", false);
        conn.execute("DELETE FROM feed_articles WHERE id = 'a1'", [])
            .expect("delete");
        insert(conn, "a2", "feed-1", "Woodpecker", false);

        assert!(
            search_titles(conn, "Kingfisher").is_empty(),
            "the new article must not answer to the deleted one's title"
        );
        assert_eq!(search_titles(conn, "Woodpecker"), vec!["Woodpecker"]);
    }

    #[test]
    fn rewriting_an_articles_body_reindexes_it() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Placeholder", false);

        conn.execute(
            "UPDATE feed_articles SET content = 'estuary tides' WHERE id = 'a1'",
            [],
        )
        .expect("update");

        assert_eq!(search_titles(conn, "estuary"), vec!["Placeholder"]);
    }

    #[test]
    fn marking_an_article_read_leaves_the_index_alone() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Kingfisher", false);

        conn.execute("UPDATE feed_articles SET is_read = 1 WHERE id = 'a1'", [])
            .expect("mark read");

        assert_eq!(search_titles(conn, "Kingfisher"), vec!["Kingfisher"]);
    }

    // ── Scope means scope ────────────────────────────────────────────

    #[test]
    fn marking_a_scope_read_leaves_every_other_feed_alone() {
        // One click on one category used to run `UPDATE feed_articles SET
        // is_read = 1` across the whole table.
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "In scope", false);
        insert(conn, "a2", "feed-2", "Out of scope", false);

        let changed = mark_all_read_in(conn, Some(&["feed-1".to_string()])).expect("mark");
        assert_eq!(changed, vec!["a1".to_string()]);

        let unread: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM feed_articles WHERE is_read = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unread, 1, "the other feed must still be unread");
    }

    #[test]
    fn an_empty_scope_marks_nothing() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Untouched", false);

        let changed = mark_all_read_in(conn, Some(&[])).expect("mark");
        assert!(changed.is_empty());

        let unread: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM feed_articles WHERE is_read = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(unread, 1, "an empty category holds no articles, not all of them");
    }

    #[test]
    fn no_scope_still_means_everything() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "One", false);
        insert(conn, "a2", "feed-2", "Two", false);

        let mut changed = mark_all_read_in(conn, None).expect("mark");
        changed.sort();
        assert_eq!(changed, vec!["a1".to_string(), "a2".to_string()]);
    }

    #[test]
    fn mark_all_read_reports_only_what_it_changed() {
        // Undo has to put back the articles this call touched, and only those.
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Already read", true);
        insert(conn, "a2", "feed-1", "Freshly read", false);

        let changed = mark_all_read_in(conn, None).expect("mark");
        assert_eq!(changed, vec!["a2".to_string()]);
    }

    // ── Filtering by category ────────────────────────────────────────

    fn filter_for(source_ids: Option<Vec<String>>) -> ArticleFilter {
        ArticleFilter {
            source_ids,
            view: "all".to_string(),
            today_start: None,
            sort: None,
            search: None,
            limit: Some(50),
            offset: Some(0),
        }
    }

    #[test]
    fn a_category_shows_only_its_own_feeds() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "In category", false);
        insert(conn, "a2", "feed-2", "In category", false);
        insert(conn, "a3", "feed-3", "Elsewhere", false);

        let found = query_articles(
            conn,
            &filter_for(Some(vec!["feed-1".to_string(), "feed-2".to_string()])),
        )
        .expect("query");

        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|a| a.title == "In category"));
    }

    #[test]
    fn an_empty_category_shows_nothing_rather_than_everything() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Somewhere else", false);

        let found = query_articles(conn, &filter_for(Some(Vec::new()))).expect("query");
        assert!(found.is_empty());
    }

    #[test]
    fn no_selection_still_shows_every_feed() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "One", false);
        insert(conn, "a2", "feed-2", "Two", false);

        let found = query_articles(conn, &filter_for(None)).expect("query");
        assert_eq!(found.len(), 2);
    }

    // ── The numbers beside the views ─────────────────────────────────

    #[test]
    fn view_counts_answer_for_their_own_view() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Unread", false);
        insert(conn, "a2", "feed-1", "Read", true);
        insert(conn, "a3", "feed-2", "Also unread", false);
        conn.execute(
            "UPDATE feed_articles SET is_starred = 1 WHERE id IN ('a1', 'a2')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE feed_articles SET is_read_later = 1 WHERE id = 'a2'",
            [],
        )
        .unwrap();

        let counts = view_counts_in(conn, None).expect("counts");

        assert_eq!(counts.unread, 2, "Unread counts what is waiting");
        assert_eq!(
            counts.starred, 2,
            "starring usually follows reading, so this counts everything held"
        );
        assert_eq!(counts.read_later, 1);
    }

    #[test]
    fn today_counts_only_what_arrived_today_and_is_unread() {
        let db = seeded_db();
        let conn = db.conn();
        // `insert` dates everything 2026-08-20, which is not today.
        insert(conn, "old", "feed-1", "Yesterday's news", false);
        let today = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE feed_articles SET published_at = ?1 WHERE id = 'old'",
            params![today],
        )
        .unwrap();
        insert(conn, "older", "feed-1", "Genuinely old", false);

        let counts = view_counts_in(conn, None).expect("counts");
        assert_eq!(counts.today, 1);
        assert_eq!(counts.unread, 2, "Unread is not limited by date");
    }

    #[test]
    fn today_follows_the_readers_midnight_not_the_servers() {
        // 08:00 in Hanoi on the 23rd is 01:00 UTC on the 23rd. A reader in
        // Hanoi whose day began at 17:00 UTC on the 22nd should see it; the
        // old prefix comparison against the UTC date agreed here only by
        // accident, and disagreed for the eight hours either side.
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Morning in Hanoi", false);
        conn.execute(
            "UPDATE feed_articles SET published_at = '2026-08-23T08:00:00+07:00' WHERE id = 'a1'",
            [],
        )
        .unwrap();

        let hanoi_midnight = "2026-08-22T17:00:00+00:00".to_string();
        let counts = view_counts_in(conn, Some(&hanoi_midnight)).expect("counts");
        assert_eq!(counts.today, 1, "it is already today in Hanoi");

        // Someone in Auckland is a day ahead; their midnight is later still.
        let auckland_midnight = "2026-08-23T12:00:00+00:00".to_string();
        let counts = view_counts_in(conn, Some(&auckland_midnight)).expect("counts");
        assert_eq!(counts.today, 0, "for them the article is yesterday's");
    }

    #[test]
    fn an_offset_timestamp_is_compared_as_an_instant() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Late yesterday", false);
        // Sorts after "2026-08-23" as text, but is 18:00 on the 22nd in UTC.
        conn.execute(
            "UPDATE feed_articles SET published_at = '2026-08-23T01:00:00+07:00' WHERE id = 'a1'",
            [],
        )
        .unwrap();

        let utc_midnight = "2026-08-23T00:00:00+00:00".to_string();
        let counts = view_counts_in(conn, Some(&utc_midnight)).expect("counts");
        assert_eq!(
            counts.today, 0,
            "text comparison said today; the instant says yesterday"
        );
    }

    #[test]
    fn oldest_first_reverses_the_list_rather_than_filtering_it() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Older", false);
        insert(conn, "a2", "feed-1", "Newer", false);
        conn.execute(
            "UPDATE feed_articles SET published_at = '2026-08-01T00:00:00Z' WHERE id = 'a1'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE feed_articles SET published_at = '2026-08-09T00:00:00Z' WHERE id = 'a2'",
            [],
        )
        .unwrap();

        let mut filter = filter_for(None);
        let newest = query_articles(conn, &filter).expect("query");
        assert_eq!(newest[0].title, "Newer");

        filter.sort = Some("oldest".to_string());
        let oldest = query_articles(conn, &filter).expect("query");
        assert_eq!(oldest[0].title, "Older");
        assert_eq!(oldest.len(), newest.len(), "sorting is not filtering");
    }

    // ── A selector for a site the guesses do not fit ─────────────────

    const AWKWARD_PAGE: &str = r#"
        <html><body>
          <div class="feed-row">
            <a href="/2026/08/first-piece"><h4>The first piece of writing</h4></a>
            <p>Something about the first piece, at length.</p>
          </div>
          <div class="feed-row">
            <a href="/2026/08/second-piece"><h4>The second piece of writing</h4></a>
            <p>Something about the second piece, at length.</p>
          </div>
        </body></html>
    "#;

    #[test]
    fn a_supplied_selector_finds_what_the_guesses_miss() {
        let guessed = scrape::scrape_articles(AWKWARD_PAGE, "https://example.com/", None);
        let told = scrape::scrape_articles(AWKWARD_PAGE, "https://example.com/", Some(".feed-row"));

        assert!(
            told.len() > guessed.len(),
            "the selector should beat the guesses on a page written for neither"
        );
        assert_eq!(told.len(), 2);
        assert!(told
            .iter()
            .any(|a| a.title == "The first piece of writing"));
    }

    #[test]
    fn a_selector_that_has_gone_stale_falls_back_rather_than_emptying_the_feed() {
        let page = r#"
            <html><body>
              <article>
                <h2><a href="/2026/08/a-real-post">A post that really exists</a></h2>
                <p>Some words about it, enough of them to look like a summary.</p>
              </article>
            </body></html>
        "#;
        let found = scrape::scrape_articles(page, "https://example.com/", Some(".no-longer-there"));
        assert!(
            !found.is_empty(),
            "a site redesign should cost accuracy, not the whole feed"
        );
    }

    #[test]
    fn nonsense_in_the_selector_box_is_not_fatal() {
        let found = scrape::scrape_articles(AWKWARD_PAGE, "https://example.com/", Some("!!! >>>"));
        // Falls through to the guesses; the point is that it does not panic.
        assert!(found.len() <= 2);
    }

    // ── Standing instructions about arriving articles ────────────────

    fn rule(contains: &str) -> FeedRule {
        FeedRule {
            id: "r1".into(),
            name: "Test".into(),
            enabled: true,
            source_ids: Vec::new(),
            field: "any".into(),
            contains: contains.into(),
            mark_read: false,
            star: false,
            mute: false,
            tag: String::new(),
        }
    }

    fn fields(title: &str, summary: &str, author: &str) -> LoweredFields {
        LoweredFields {
            title: title.to_lowercase(),
            summary: summary.to_lowercase(),
            author: author.to_lowercase(),
        }
    }

    #[test]
    fn matching_ignores_case_on_both_sides() {
        let r = rule("Rust");
        assert!(r.matches(&fields("Learning RUST today", "", "")));
        assert!(r.matches(&fields("", "", "The rust council")));
        assert!(!r.matches(&fields("Learning Go today", "", "")));
    }

    #[test]
    fn a_rule_can_be_narrowed_to_one_field() {
        let mut r = rule("sponsored");
        r.field = "title".into();
        assert!(r.matches(&fields("Sponsored: buy this", "", "")));
        assert!(
            !r.matches(&fields("A real post", "this post is sponsored", "")),
            "the body mentioning it is not the title being it"
        );
    }

    #[test]
    fn a_rule_with_nothing_to_match_matches_nothing() {
        // Otherwise a half-written rule silently applies to every article.
        let r = rule("   ");
        assert!(!r.matches(&fields("anything at all", "", "")));
    }

    #[test]
    fn a_disabled_rule_is_not_consulted() {
        let mut r = rule("rust");
        r.star = true;
        r.enabled = false;
        assert!(!judge(&[r], "feed-1", &fields("rust news", "", "")).star);
    }

    #[test]
    fn a_rule_can_be_scoped_to_particular_feeds() {
        let mut r = rule("rust");
        r.star = true;
        r.source_ids = vec!["feed-1".into()];

        assert!(judge(std::slice::from_ref(&r), "feed-1", &fields("rust news", "", "")).star);
        assert!(!judge(&[r], "feed-2", &fields("rust news", "", "")).star);
    }

    #[test]
    fn rules_add_up_rather_than_overriding_each_other() {
        let mut starrer = rule("rust");
        starrer.star = true;
        starrer.tag = "lang".into();
        let mut reader = rule("news");
        reader.id = "r2".into();
        reader.mark_read = true;
        reader.tag = "lang".into(); // same tag from two rules

        let verdict = judge(&[starrer, reader], "feed-1", &fields("rust news", "", ""));
        assert!(verdict.star && verdict.mark_read);
        assert_eq!(verdict.tags, vec!["lang".to_string()], "attached once");
    }

    #[test]
    fn one_rule_muting_is_enough_to_mute() {
        let mut keeper = rule("rust");
        keeper.star = true;
        let mut muter = rule("press release");
        muter.id = "r2".into();
        muter.mute = true;

        let verdict = judge(
            &[keeper, muter],
            "feed-1",
            &fields("rust press release", "", ""),
        );
        assert!(verdict.mute, "however many other rules liked it");
    }

    #[test]
    fn a_muted_article_is_never_stored() {
        let db = seeded_db();
        let conn = db.conn();

        let mut muter = rule("press release");
        muter.mute = true;

        let articles = vec![
            parser::ParsedArticle {
                guid: "g1".into(),
                title: "A press release".into(),
                url: String::new(),
                author: String::new(),
                content: String::new(),
                summary: String::new(),
                published_at: "2026-08-20T00:00:00Z".into(),
                thumbnail_url: String::new(),
                content_type: "text/html".into(),
                word_count: 0,
                read_time_minutes: 1,
            },
            parser::ParsedArticle {
                guid: "g2".into(),
                title: "Something worth reading".into(),
                url: String::new(),
                author: String::new(),
                content: String::new(),
                summary: String::new(),
                published_at: "2026-08-20T00:00:00Z".into(),
                thumbnail_url: String::new(),
                content_type: "text/html".into(),
                word_count: 0,
                read_time_minutes: 1,
            },
        ];

        let inserted = insert_articles(conn, "feed-1", &articles, &[muter]).expect("insert");
        assert_eq!(inserted, 1, "one arrived, one was muted");

        let titles: Vec<String> = query_articles(conn, &filter_for(None))
            .expect("query")
            .into_iter()
            .map(|a| a.title)
            .collect();
        assert_eq!(titles, vec!["Something worth reading".to_string()]);
    }

    #[test]
    fn tags_are_stored_so_that_a_prefix_is_not_a_match() {
        // `,rust,` rather than `rust`, so `#rust` cannot find `rustlang`.
        assert_eq!(join_tags(&["rust".into(), "work".into()]), ",rust,work,");
        assert_eq!(join_tags(&[]), "");
        assert_eq!(split_tags(",rust,work,"), vec!["rust".to_string(), "work".to_string()]);
        assert!(split_tags("").is_empty());
    }

    #[test]
    fn a_rule_written_today_can_reach_yesterdays_articles() {
        let vault = temp_vault();
        let path = vault.path().to_string_lossy().to_string();
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Rust in production", false);
        insert(conn, "a2", "feed-1", "Something else", false);

        let mut r = rule("rust");
        r.star = true;
        r.tag = "lang".into();
        feed_save_rules(path.clone(), vec![r]).expect("save");

        // `feed_apply_rules` needs Tauri state; this is the work it does.
        let rules = feed_get_rules(path).expect("read back");
        assert_eq!(rules.len(), 1);

        let verdict = judge(&rules, "feed-1", &fields("Rust in production", "", ""));
        assert!(verdict.star);
        assert_eq!(verdict.tags, vec!["lang".to_string()]);
    }

    // ── Per-device state stays out of the vault ──────────────────────

    #[test]
    fn a_subscription_written_to_the_vault_carries_no_fetch_state() {
        // This is the whole point of the split: if these fields can reach the
        // file, every refresh rewrites it, and two devices refreshing at once
        // hand the character-level merge a document it cannot safely combine.
        let source = StoredFeedSource {
            id: "s1".into(),
            url: "https://example.com/feed".into(),
            site_url: String::new(),
            feed_type: "rss".into(),
            title: "Example".into(),
            description: String::new(),
            icon_url: String::new(),
            category_id: String::new(),
            update_interval: 30,
            is_paused: false,
            added_at: "2026-08-23T00:00:00+00:00".into(),
            full_text_fetch: false,
            scrape_container: String::new(),
            last_fetched_at: Some("2026-08-23T09:00:00+00:00".into()),
            last_error: Some("boom".into()),
            etag: Some("\"abc\"".into()),
            last_modified_header: Some("Sat, 23 Aug 2026 09:00:00 GMT".into()),
        };

        let json = serde_json::to_string(&source).expect("serialize");
        for field in ["lastFetchedAt", "lastError", "etag", "lastModifiedHeader"] {
            assert!(!json.contains(field), "{field} must not reach sources.json: {json}");
        }
        assert!(json.contains("fullTextFetch"), "the reader's own choices still travel");
    }

    #[test]
    fn an_older_vault_hands_its_fetch_state_to_the_database_once() {
        let db = seeded_db();
        let conn = db.conn();

        let mut sources = vec![StoredFeedSource {
            id: "s1".into(),
            url: "https://example.com/feed".into(),
            site_url: String::new(),
            feed_type: "rss".into(),
            title: "Example".into(),
            description: String::new(),
            icon_url: String::new(),
            category_id: String::new(),
            update_interval: 30,
            is_paused: false,
            added_at: "2026-08-23T00:00:00+00:00".into(),
            full_text_fetch: false,
            scrape_container: String::new(),
            last_fetched_at: Some("2026-08-23T09:00:00+00:00".into()),
            last_error: None,
            etag: Some("\"abc\"".into()),
            last_modified_header: None,
        }];

        assert!(migrate_legacy_state(conn, &mut sources), "there was state to move");

        let states = load_source_states(conn);
        let state = states.get("s1").expect("state row");
        assert_eq!(state.etag, "\"abc\"", "the ETag survives, so no full re-download");
        assert_eq!(state.last_fetched_at, "2026-08-23T09:00:00+00:00");

        assert!(!sources[0].has_legacy_state(), "and the file is cleaned");
        assert!(
            !migrate_legacy_state(conn, &mut sources),
            "a second pass finds nothing to do"
        );
    }

    #[test]
    fn migration_does_not_overwrite_what_this_device_already_knows() {
        let db = seeded_db();
        let conn = db.conn();
        record_fetch_success(conn, "s1", Some("fresh"), None);

        let mut sources = vec![StoredFeedSource {
            id: "s1".into(),
            url: "https://example.com/feed".into(),
            site_url: String::new(),
            feed_type: "rss".into(),
            title: "Example".into(),
            description: String::new(),
            icon_url: String::new(),
            category_id: String::new(),
            update_interval: 30,
            is_paused: false,
            added_at: String::new(),
            full_text_fetch: false,
            scrape_container: String::new(),
            last_fetched_at: None,
            last_error: None,
            etag: Some("stale".into()),
            last_modified_header: None,
        }];
        migrate_legacy_state(conn, &mut sources);

        let states = load_source_states(conn);
        assert_eq!(states.get("s1").expect("state").etag, "fresh");
    }

    #[test]
    fn a_not_modified_response_keeps_the_etag_it_validated_against() {
        let db = seeded_db();
        let conn = db.conn();
        record_fetch_success(conn, "s1", Some("\"v1\""), Some("Mon, 01 Jan 2026 00:00:00 GMT"));
        record_fetch_success(conn, "s1", None, None);

        let states = load_source_states(conn);
        let state = states.get("s1").expect("state");
        assert_eq!(state.etag, "\"v1\"");
        assert_eq!(state.last_modified, "Mon, 01 Jan 2026 00:00:00 GMT");
        assert!(!state.last_fetched_at.is_empty());
    }

    // ── Whose turn it is ─────────────────────────────────────────────

    #[test]
    fn a_feed_is_due_when_its_own_interval_has_passed() {
        let now = chrono::Utc::now();
        let recent = SourceState {
            last_fetched_at: (now - chrono::Duration::minutes(10)).to_rfc3339(),
            ..Default::default()
        };
        assert!(!is_due(Some(&recent), 30, now), "ten minutes into a thirty-minute wait");
        assert!(is_due(Some(&recent), 5, now), "but a five-minute feed is ready");
    }

    #[test]
    fn a_feed_nobody_has_fetched_is_due_now() {
        let now = chrono::Utc::now();
        assert!(is_due(None, 60, now), "never fetched here");
        assert!(
            is_due(Some(&SourceState::default()), 60, now),
            "a row with no fetch time is the same thing"
        );
        let unparseable = SourceState {
            last_fetched_at: "sometime last tuesday".into(),
            ..Default::default()
        };
        assert!(
            is_due(Some(&unparseable), 60, now),
            "refusing to guess would strand the feed forever"
        );
    }

    // ── Backing off a feed that keeps refusing ───────────────────────

    #[test]
    fn backoff_doubles_and_then_stops_doubling() {
        assert_eq!(backoff_delay_minutes(1), 5);
        assert_eq!(backoff_delay_minutes(2), 10);
        assert_eq!(backoff_delay_minutes(3), 20);
        assert_eq!(backoff_delay_minutes(7), 320);
        assert_eq!(backoff_delay_minutes(8), 360, "six hours is the ceiling");
        assert_eq!(backoff_delay_minutes(500), 360, "and it stays the ceiling");
    }

    #[test]
    fn a_failing_feed_is_left_alone_until_its_next_try_is_due() {
        let db = seeded_db();
        let conn = db.conn();

        record_fetch_failure(conn, "feed-1", "unreachable", None);
        assert!(sources_in_backoff(conn).contains("feed-1"));

        let count: i64 = conn
            .query_row(
                "SELECT error_count FROM feed_source_state WHERE source_id = 'feed-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        record_fetch_failure(conn, "feed-1", "unreachable", None);
        let count: i64 = conn
            .query_row(
                "SELECT error_count FROM feed_source_state WHERE source_id = 'feed-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "failures accumulate rather than resetting");
    }

    #[test]
    fn a_feed_that_answers_is_forgiven_completely() {
        let db = seeded_db();
        let conn = db.conn();
        record_fetch_failure(conn, "feed-1", "unreachable", None);
        record_fetch_failure(conn, "feed-1", "unreachable", None);

        clear_fetch_state(conn, "feed-1");

        assert!(sources_in_backoff(conn).is_empty());
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM feed_source_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "one good fetch starts the count over");
    }

    #[test]
    fn a_retry_that_has_come_due_is_not_backed_off() {
        let db = seeded_db();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO feed_source_state (source_id, error_count, next_retry_at)
             VALUES ('feed-1', 3, '2020-01-01T00:00:00+00:00')",
            [],
        )
        .unwrap();

        assert!(
            sources_in_backoff(conn).is_empty(),
            "a due retry is due, however many failures preceded it"
        );
    }

    // ── OPML actually imports ────────────────────────────────────────

    fn temp_vault() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp vault")
    }

    /// `feed_get_sources` joins in per-device state and so needs Tauri state;
    /// these tests are about what reaches the vault file.
    fn stored_for_test(vault_path: &str) -> Vec<StoredFeedSource> {
        read_stored_sources(vault_path).expect("sources").1
    }

    const SAMPLE_OPML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="1.0">
  <head><title>Subscriptions</title></head>
  <body>
    <outline text="Tech" title="Tech">
      <outline type="rss" text="Example" title="Example"
               xmlUrl="https://example.com/feed.xml" htmlUrl="https://example.com/" />
    </outline>
    <outline type="rss" text="Loose" title="Loose"
             xmlUrl="https://loose.example/rss" htmlUrl="https://loose.example/" />
  </body>
</opml>"#;

    #[test]
    fn importing_opml_writes_the_feeds_to_the_vault() {
        // The import used to parse the file, hand the result back, and let the
        // front end drop it — reporting success while subscribing to nothing.
        let vault = temp_vault();
        let path = vault.path().to_string_lossy().to_string();

        let result = feed_import_opml(path.clone(), SAMPLE_OPML.to_string()).expect("import");
        assert_eq!(result.added, 2);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.categories_created, 1);

        let sources = stored_for_test(&path);
        assert_eq!(sources.len(), 2);

        let categorised = sources
            .iter()
            .find(|s| s.url == "https://example.com/feed.xml")
            .expect("the categorised feed");
        assert_eq!(categorised.title, "Example");
        assert_eq!(categorised.site_url, "https://example.com/");
        assert!(!categorised.category_id.is_empty());

        let loose = sources
            .iter()
            .find(|s| s.url == "https://loose.example/rss")
            .expect("the uncategorised feed");
        assert!(loose.category_id.is_empty(), "no folder means no category");

        let categories = feed_get_categories(path).expect("categories");
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].name, "Tech");
    }

    #[test]
    fn importing_the_same_file_twice_subscribes_once() {
        let vault = temp_vault();
        let path = vault.path().to_string_lossy().to_string();

        feed_import_opml(path.clone(), SAMPLE_OPML.to_string()).expect("first import");
        let second = feed_import_opml(path.clone(), SAMPLE_OPML.to_string()).expect("second");

        assert_eq!(second.added, 0);
        assert_eq!(second.skipped, 2);
        assert_eq!(second.categories_created, 0);
        assert_eq!(stored_for_test(&path).len(), 2);
    }

    #[test]
    fn what_is_exported_can_be_imported_again() {
        let first = temp_vault();
        let first_path = first.path().to_string_lossy().to_string();
        feed_import_opml(first_path.clone(), SAMPLE_OPML.to_string()).expect("import");

        let exported = feed_export_opml(first_path).expect("export");

        let second = temp_vault();
        let second_path = second.path().to_string_lossy().to_string();
        let result = feed_import_opml(second_path.clone(), exported).expect("re-import");

        assert_eq!(result.added, 2);
        let sources = stored_for_test(&second_path);
        let mut urls: Vec<&str> = sources.iter().map(|s| s.url.as_str()).collect();
        urls.sort();
        assert_eq!(
            urls,
            vec!["https://example.com/feed.xml", "https://loose.example/rss"]
        );
    }

    #[test]
    fn importing_junk_fails_loudly() {
        let vault = temp_vault();
        let path = vault.path().to_string_lossy().to_string();

        // The path of a file, which is what the dialog used to pass in place
        // of the document itself.
        let result = feed_import_opml(path.clone(), "/Users/me/feeds.opml".to_string());

        assert!(result.is_err(), "a path is not an OPML document");
        assert!(stored_for_test(&path).is_empty());
    }

    #[test]
    fn the_unread_view_filters_by_read_state() {
        let db = seeded_db();
        let conn = db.conn();
        insert(conn, "a1", "feed-1", "Unread", false);
        insert(conn, "a2", "feed-1", "Read", true);

        let mut filter = filter_for(None);
        filter.view = "unread".to_string();
        let found = query_articles(conn, &filter).expect("query");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Unread");
    }
}
