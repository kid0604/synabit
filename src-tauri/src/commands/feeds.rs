//! Tauri IPC commands for the Feeds mini-app.
//!
//! Handles feed source CRUD (vault JSON files), article caching (SQLite),
//! feed refresh, discovery, OPML import/export, and maintenance.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbState;
use crate::feed_engine::{cleanup, discovery, fetcher, opml as feed_opml, parser, scrape, readability};

// ═══════════════════════════════════════════════════════════════
//  DATA TYPES
// ═══════════════════════════════════════════════════════════════

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
    pub last_fetched_at: String,
    pub last_error: Option<String>,
    pub etag: Option<String>,
    pub last_modified_header: Option<String>,
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
    pub default_view: String,
    pub show_read_articles: bool,
    pub mark_read_on_scroll: bool,
    pub auto_cleanup_days: i64,
    pub max_articles_per_feed: i64,
    pub global_update_interval: i64,
    pub reading_font_size: i64,
    pub reading_max_width: i64,
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self {
            default_view: "all".to_string(),
            show_read_articles: true,
            mark_read_on_scroll: false,
            auto_cleanup_days: 30,
            max_articles_per_feed: 500,
            global_update_interval: 30,
            reading_font_size: 16,
            reading_max_width: 720,
        }
    }
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleFilter {
    pub source_id: Option<String>,
    pub category_id: Option<String>,
    pub view: String,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
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
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Serialize and write a JSON file.
fn write_json_file<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize JSON: {}", e))?;
    std::fs::write(path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(())
}

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
    })
}

// ═══════════════════════════════════════════════════════════════
//  FEED SOURCE CRUD (vault JSON files)
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn feed_get_sources(vault_path: String) -> Result<Vec<FeedSource>, String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("sources.json");
    read_json_file(&path)
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
                        let s = feed.links.first().map(|l| l.href.clone()).unwrap_or_default();
                        (t, d, s)
                    }
                    Err(_) => (String::new(), String::new(), String::new()),
                }
            }
            _ => (String::new(), String::new(), String::new()),
        };

        let now = chrono::Utc::now().to_rfc3339();
        let source = FeedSource {
            id: uuid::Uuid::new_v4().to_string(),
            url: feed_url,
            site_url,
            feed_type,
            title,
            description,
            icon_url: String::new(),
            category_id: category_id.unwrap_or_default(),
            update_interval: 30,
            is_paused: false,
            added_at: now,
            last_fetched_at: String::new(),
            last_error: None,
            etag: None,
            last_modified_header: None,
        };

        let feeds_dir = ensure_feeds_dir(&vault_path)?;
        let path = feeds_dir.join("sources.json");
        let mut sources: Vec<FeedSource> = read_json_file(&path)?;
        if sources.iter().any(|s| s.url == source.url) {
            return Err("A feed with this URL already exists".to_string());
        }
        sources.push(source.clone());
        write_json_file(&path, &sources)?;
        return Ok(source);
    }

    // Step 2: No RSS found — try scrape mode
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Synabit/1.0 Feed Reader")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client.get(&url).send().await
        .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;
    let html = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let scraped = scrape::scrape_articles(&html, &url);
    if scraped.is_empty() {
        return Err("No RSS feed or articles found at this URL".to_string());
    }

    // Extract site title from HTML
    let doc = scraper::Html::parse_document(&html);
    let title = if let Ok(sel) = scraper::Selector::parse("title") {
        doc.select(&sel).next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| url.clone())
    } else {
        url.clone()
    };
    let description = if let Ok(sel) = scraper::Selector::parse("meta[name=\"description\"]") {
        doc.select(&sel).next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let now = chrono::Utc::now().to_rfc3339();
    let source = FeedSource {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.clone(),
        site_url: url,
        feed_type: "scrape".to_string(),
        title,
        description,
        icon_url: String::new(),
        category_id: category_id.unwrap_or_default(),
        update_interval: 60, // Less frequent for scrape
        is_paused: false,
        added_at: now,
        last_fetched_at: String::new(),
        last_error: None,
        etag: None,
        last_modified_header: None,
    };

    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("sources.json");
    let mut sources: Vec<FeedSource> = read_json_file(&path)?;
    if sources.iter().any(|s| s.url == source.url) {
        return Err("A feed with this URL already exists".to_string());
    }
    sources.push(source.clone());
    write_json_file(&path, &sources)?;
    Ok(source)
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

    Ok(())
}

#[tauri::command]
pub fn feed_update_source(vault_path: String, source: FeedSource) -> Result<(), String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let path = feeds_dir.join("sources.json");
    let mut sources: Vec<FeedSource> = read_json_file(&path)?;

    if let Some(existing) = sources.iter_mut().find(|s| s.id == source.id) {
        *existing = source;
    } else {
        return Err("Feed source not found".to_string());
    }

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
    read_json_file(&path)
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
    source_id: Option<String>,
    filter: ArticleFilter,
) -> Result<Vec<CachedArticle>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let mut conditions = Vec::new();
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    // Filter by source_id (from argument or filter)
    let effective_source_id = source_id.or(filter.source_id);
    if let Some(ref sid) = effective_source_id {
        conditions.push(format!("a.feed_source_id = ?{}", param_idx));
        param_values.push(Box::new(sid.clone()));
        param_idx += 1;
    }

    // View filters
    match filter.view.as_str() {
        "today" => {
            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
            conditions.push(format!("a.published_at >= ?{}", param_idx));
            param_values.push(Box::new(today));
            param_idx += 1;
        }
        "unread" => {
            conditions.push("a.is_read = 0".to_string());
        }
        "starred" => {
            conditions.push("a.is_starred = 1".to_string());
        }
        "read-later" => {
            conditions.push("a.is_read_later = 1".to_string());
        }
        _ => {} // "all" — no filter
    }

    // Category filter: get all source IDs in the category
    if let Some(ref cat_id) = filter.category_id {
        // We need to read sources.json to find which sources belong to this category.
        // Since we don't have vault_path here, we use a subquery approach:
        // The caller should pass source_ids instead, or we handle it differently.
        // For now, we'll accept it as a placeholder — in practice, the frontend
        // filters by source_id directly.
        let _ = cat_id; // Category filtering is handled by the frontend
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let limit = filter.limit.unwrap_or(100);
    let offset = filter.offset.unwrap_or(0);

    let sql = format!(
        "SELECT id, feed_source_id, guid, title, url, author, content, summary,
                published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                content_type, is_read, is_starred, is_read_later
         FROM feed_articles a
         {}
         ORDER BY published_at DESC
         LIMIT ?{} OFFSET ?{}",
        where_clause, param_idx, param_idx + 1
    );

    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Query error: {}", e))?;
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
) -> Result<Vec<CachedArticle>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    // Use FTS5 to search articles
    let sql = "SELECT a.id, a.feed_source_id, a.guid, a.title, a.url, a.author,
                      a.content, a.summary, a.published_at, a.fetched_at,
                      a.thumbnail_url, a.word_count, a.read_time_minutes,
                      a.content_type, a.is_read, a.is_starred, a.is_read_later
               FROM feed_articles a
               JOIN feed_articles_fts fts ON a.rowid = fts.rowid
               WHERE feed_articles_fts MATCH ?1
               ORDER BY rank
               LIMIT 50";

    let mut stmt = conn.prepare(sql).map_err(|e| format!("FTS query error: {}", e))?;
    let articles = stmt
        .query_map(params![query], row_to_article)
        .map_err(|e| format!("FTS query map error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(articles)
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
            "UPDATE feed_articles SET is_read = ?1 WHERE id = ?2",
            params![read as i64, article_id],
        )
        .map_err(|e| format!("Update error: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn feed_mark_all_read(
    db: tauri::State<'_, DbState>,
    source_id: Option<String>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    if let Some(sid) = source_id {
        db.conn()
            .execute(
                "UPDATE feed_articles SET is_read = 1 WHERE feed_source_id = ?1",
                params![sid],
            )
            .map_err(|e| format!("Update error: {}", e))?;
    } else {
        db.conn()
            .execute("UPDATE feed_articles SET is_read = 1", [])
            .map_err(|e| format!("Update error: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub fn feed_toggle_star(
    db: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<bool, String> {
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
        "UPDATE feed_articles SET is_starred = ?1 WHERE id = ?2",
        params![new_value, article_id],
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
        "UPDATE feed_articles SET is_read_later = ?1 WHERE id = ?2",
        params![new_value, article_id],
    )
    .map_err(|e| format!("Update error: {}", e))?;

    Ok(new_value == 1)
}

// ═══════════════════════════════════════════════════════════════
//  FETCH & REFRESH
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub async fn feed_refresh(
    db: tauri::State<'_, DbState>,
    vault_path: String,
    source_id: Option<String>,
) -> Result<RefreshResult, String> {
    // Read sources from vault
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let sources_path = feeds_dir.join("sources.json");
    let mut all_sources: Vec<FeedSource> = read_json_file(&sources_path)?;

    // Filter to specific source if requested — collect owned data to avoid borrow conflicts
    let source_ids_to_refresh: Vec<(String, String, Option<String>, Option<String>, bool, String)> =
        all_sources
            .iter()
            .filter(|s| {
                !s.is_paused
                    && source_id
                        .as_ref()
                        .map(|sid| s.id == *sid)
                        .unwrap_or(true)
            })
            .map(|s| {
                (
                    s.id.clone(),
                    s.url.clone(),
                    s.etag.clone(),
                    s.last_modified_header.clone(),
                    s.is_paused,
                    s.feed_type.clone(),
                )
            })
            .collect();

    let mut result = RefreshResult {
        total_fetched: 0,
        total_new: 0,
        errors: Vec::new(),
    };

    for (sid, url, etag, last_mod, _, feed_type) in &source_ids_to_refresh {
        if feed_type == "scrape" {
            // Scrape mode: fetch homepage and extract article cards
            match scrape_refresh(&db, sid, url).await {
                Ok((fetched, new_count)) => {
                    result.total_fetched += fetched;
                    result.total_new += new_count;
                    if let Some(s) = all_sources.iter_mut().find(|s2| s2.id == *sid) {
                        s.last_fetched_at = chrono::Utc::now().to_rfc3339();
                        s.last_error = None;
                    }
                }
                Err(e) => {
                    let msg = format!("Scrape error for {}: {}", url, e);
                    result.errors.push(msg);
                    if let Some(s) = all_sources.iter_mut().find(|s2| s2.id == *sid) {
                        s.last_error = Some(e);
                    }
                }
            }
            continue;
        }

        // Existing RSS/Atom refresh code below...
        let fetch = fetcher::fetch_feed(
            url,
            etag.as_deref(),
            last_mod.as_deref(),
        )
        .await;

        match fetch {
            fetcher::FetchResult::NotModified => {
                // Nothing new, update last_fetched_at
                if let Some(s) = all_sources.iter_mut().find(|s2| s2.id == *sid) {
                    s.last_fetched_at = chrono::Utc::now().to_rfc3339();
                    s.last_error = None;
                }
            }
            fetcher::FetchResult::Updated {
                body,
                etag: new_etag,
                last_modified: new_last_mod,
            } => {
                match parser::parse_feed(&body) {
                    Ok(articles) => {
                        let articles_found = articles.len();
                        result.total_fetched += articles_found;

                        // Insert articles into DB
                        let new_count = {
                            let db = db.lock().map_err(|e| e.to_string())?;
                            insert_articles(db.conn(), sid, &articles)?
                        };
                        result.total_new += new_count;

                        // Log the fetch
                        {
                            let db = db.lock().map_err(|e| e.to_string())?;
                            log_fetch(
                                db.conn(),
                                sid,
                                "ok",
                                articles_found,
                                new_count,
                                None,
                            )?;
                        }

                        // Update source metadata
                        if let Some(s) = all_sources.iter_mut().find(|s2| s2.id == *sid) {
                            s.last_fetched_at = chrono::Utc::now().to_rfc3339();
                            s.last_error = None;
                            s.etag = new_etag;
                            s.last_modified_header = new_last_mod;
                        }
                    }
                    Err(parse_err) => {
                        let msg = format!("Parse error for {}: {}", url, parse_err);
                        result.errors.push(msg.clone());

                        if let Some(s) = all_sources.iter_mut().find(|s2| s2.id == *sid) {
                            s.last_error = Some(parse_err.clone());
                        }

                        let db = db.lock().map_err(|e| e.to_string())?;
                        log_fetch(db.conn(), sid, "error", 0, 0, Some(&parse_err))?;
                    }
                }
            }
            fetcher::FetchResult::Error { message } => {
                let msg = format!("Fetch error for {}: {}", url, message);
                result.errors.push(msg.clone());

                if let Some(s) = all_sources.iter_mut().find(|s2| s2.id == *sid) {
                    s.last_error = Some(message.clone());
                }

                let db = db.lock().map_err(|e| e.to_string())?;
                log_fetch(db.conn(), sid, "error", 0, 0, Some(&message))?;
            }
        }
    }

    // Write updated sources back to vault
    write_json_file(&sources_path, &all_sources)?;

    Ok(result)
}

/// Refresh a scrape-type feed source by fetching the homepage and extracting article cards.
async fn scrape_refresh(
    db: &tauri::State<'_, DbState>,
    source_id: &str,
    url: &str,
) -> Result<(usize, usize), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Synabit/1.0 Feed Reader")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client.get(url).send().await
        .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;
    let html = response.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let scraped = scrape::scrape_articles(&html, url);
    let fetched = scraped.len();
    let now = chrono::Utc::now().to_rfc3339();
    let mut new_count = 0;

    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();

    let mut insert_stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO feed_articles
             (id, feed_source_id, guid, title, url, author, content, summary,
              published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
              content_type, is_read, is_starred, is_read_later)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, 0, ?12, 0, 0, 0)",
        )
        .map_err(|e| format!("Prepare error: {}", e))?;

    let mut fts_stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO feed_articles_fts (rowid, title, author, content, summary)
             SELECT rowid, title, author, content, summary FROM feed_articles WHERE id = ?1",
        )
        .map_err(|e| format!("FTS prepare error: {}", e))?;

    for article in &scraped {
        let article_id = uuid::Uuid::new_v4().to_string();
        // Use URL as guid for deduplication
        let inserted = insert_stmt
            .execute(params![
                article_id,
                source_id,
                article.url,        // guid = url for scrape articles
                article.title,
                article.url,
                "",                  // author (extracted later on read)
                "",                  // content (lazy-loaded on read)
                article.summary,
                if article.published_at.is_empty() { &now } else { &article.published_at },
                now,
                article.thumbnail_url,
                "scrape",            // content_type
            ])
            .map_err(|e| format!("Insert error: {}", e))?;

        if inserted > 0 {
            new_count += 1;
            let _ = fts_stmt.execute(params![article_id]);
        }
    }

    Ok((fetched, new_count))
}

/// Insert parsed articles into the database, returning the count of newly inserted articles.
fn insert_articles(
    conn: &rusqlite::Connection,
    source_id: &str,
    articles: &[parser::ParsedArticle],
) -> Result<usize, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut new_count = 0;

    let mut insert_stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO feed_articles
             (id, feed_source_id, guid, title, url, author, content, summary,
              published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
              content_type, is_read, is_starred, is_read_later)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 0, 0, 0)",
        )
        .map_err(|e| format!("Prepare error: {}", e))?;

    let mut fts_stmt = conn
        .prepare(
            "INSERT OR IGNORE INTO feed_articles_fts (rowid, title, author, content, summary)
             SELECT rowid, title, author, content, summary FROM feed_articles WHERE id = ?1",
        )
        .map_err(|e| format!("FTS prepare error: {}", e))?;

    for article in articles {
        let article_id = uuid::Uuid::new_v4().to_string();
        let inserted = insert_stmt
            .execute(params![
                article_id,
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
            ])
            .map_err(|e| format!("Insert error: {}", e))?;

        if inserted > 0 {
            new_count += 1;
            // Index in FTS
            let _ = fts_stmt.execute(params![article_id]);
        }
    }

    Ok(new_count)
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

/// Lazy-load full article content for scrape-type articles.
/// Called when user clicks to read an article that has no content yet.
#[tauri::command]
pub async fn feed_fetch_article_content(
    db: tauri::State<'_, DbState>,
    article_id: String,
) -> Result<CachedArticle, String> {
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
    if !current_content.is_empty() {
        let db = db.lock().map_err(|e| e.to_string())?;
        let conn = db.conn();
        let article = conn.query_row(
            "SELECT id, feed_source_id, guid, title, url, author, content, summary,
                    published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                    content_type, is_read, is_starred, is_read_later
             FROM feed_articles WHERE id = ?1",
            params![article_id],
            row_to_article,
        ).map_err(|e| format!("Query error: {}", e))?;
        return Ok(article);
    }

    // 3. Fetch the article page
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("Synabit/1.0 Feed Reader")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client.get(&url).send().await
        .map_err(|e| format!("Failed to fetch article: {}", e))?;
    let html = response.text().await
        .map_err(|e| format!("Failed to read article: {}", e))?;

    // 4. Extract content using readability
    let extracted = readability::extract_content(&html, &url);

    // 5. Update article in DB
    {
        let db = db.lock().map_err(|e| e.to_string())?;
        let conn = db.conn();
        conn.execute(
            "UPDATE feed_articles SET content = ?1, author = CASE WHEN author = '' THEN ?2 ELSE author END,
             word_count = ?3, read_time_minutes = ?4,
             thumbnail_url = CASE WHEN thumbnail_url = '' THEN ?5 ELSE thumbnail_url END
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

        // Update FTS
        let _ = conn.execute(
            "DELETE FROM feed_articles_fts WHERE rowid = (SELECT rowid FROM feed_articles WHERE id = ?1)",
            params![article_id],
        );
        let _ = conn.execute(
            "INSERT INTO feed_articles_fts (rowid, title, author, content, summary)
             SELECT rowid, title, author, content, summary FROM feed_articles WHERE id = ?1",
            params![article_id],
        );
    }

    // 6. Return updated article
    let db = db.lock().map_err(|e| e.to_string())?;
    let conn = db.conn();
    conn.query_row(
        "SELECT id, feed_source_id, guid, title, url, author, content, summary,
                published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                content_type, is_read, is_starred, is_read_later
         FROM feed_articles WHERE id = ?1",
        params![article_id],
        row_to_article,
    ).map_err(|e| format!("Query error: {}", e))
}

// ═══════════════════════════════════════════════════════════════
//  MAINTENANCE
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
pub fn feed_run_cleanup(
    db: tauri::State<'_, DbState>,
    max_age_days: i64,
    max_per_feed: i64,
) -> Result<cleanup::CleanupResult, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    cleanup::run_cleanup(db.conn(), max_age_days, max_per_feed)
}

#[tauri::command]
pub fn feed_import_opml(
    _vault_path: String,
    opml_content: String,
) -> Result<Vec<feed_opml::ImportedFeed>, String> {
    let imported = feed_opml::import_opml(&opml_content)?;

    // Optionally auto-add the imported feeds to sources.json
    // For now, just return them so the frontend can review and add selectively
    Ok(imported)
}

#[tauri::command]
pub fn feed_export_opml(vault_path: String) -> Result<String, String> {
    let feeds_dir = ensure_feeds_dir(&vault_path)?;
    let sources: Vec<FeedSource> = read_json_file(&feeds_dir.join("sources.json"))?;
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
            category_name: cat_map
                .get(&s.category_id)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();

    Ok(feed_opml::export_opml(&export_sources))
}
