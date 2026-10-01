use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Result of a cleanup operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    pub deleted_articles: usize,
    pub deleted_logs: usize,
}

/// Run cleanup to remove old articles and fetch logs.
///
/// - Deletes read articles older than `max_age_days` (keeps starred & read_later)
/// - Deletes excess articles per feed beyond `max_per_feed` (keeps starred & read_later)
/// - Deletes old fetch logs (> 7 days)
///
/// The search index looks after itself: `feed_articles` carries triggers, so
/// deleting an article here removes its index entry in the same statement.
/// This used to end by emptying and refilling the whole index, which was both
/// the only thing keeping the index honest and a full reindex every six hours.
pub fn run_cleanup(
    conn: &rusqlite::Connection,
    max_age_days: i64,
    max_per_feed: i64,
) -> Result<CleanupResult, String> {
    let mut total_deleted_articles: usize = 0;

    // 1. Delete read articles older than max_age_days (keep starred & read_later)
    let cutoff = chrono::Utc::now() - chrono::Duration::days(max_age_days);
    let cutoff_str = cutoff.to_rfc3339();

    let count = conn
        .execute(
            "DELETE FROM feed_articles
             WHERE is_read = 1
               AND is_starred = 0
               AND is_read_later = 0
               AND published_at < ?1
               AND published_at != ''",
            params![cutoff_str],
        )
        .map_err(|e| format!("Cleanup age error: {}", e))?;
    total_deleted_articles += count;

    // 2. Delete excess articles per feed beyond max_per_feed (keep starred & read_later)
    let mut source_stmt = conn
        .prepare("SELECT DISTINCT feed_source_id FROM feed_articles")
        .map_err(|e| format!("Cleanup source query error: {}", e))?;
    let source_ids: Vec<String> = source_stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("Cleanup source map error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    for source_id in &source_ids {
        // Count total non-protected articles for this source
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM feed_articles
                 WHERE feed_source_id = ?1 AND is_starred = 0 AND is_read_later = 0",
                params![source_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if total > max_per_feed {
            // Delete oldest excess articles
            let excess = (total - max_per_feed) as usize;
            let count = conn
                .execute(
                    "DELETE FROM feed_articles WHERE id IN (
                        SELECT id FROM feed_articles
                        WHERE feed_source_id = ?1 AND is_starred = 0 AND is_read_later = 0
                        ORDER BY published_at ASC
                        LIMIT ?2
                    )",
                    params![source_id, excess as i64],
                )
                .map_err(|e| format!("Cleanup excess error: {}", e))?;
            total_deleted_articles += count;
        }
    }

    // 3. Delete old fetch logs (> 7 days)
    let log_cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let log_cutoff_str = log_cutoff.to_rfc3339();

    let deleted_logs: usize = conn
        .execute(
            "DELETE FROM feed_fetch_log WHERE fetched_at < ?1",
            params![log_cutoff_str],
        )
        .map_err(|e| format!("Cleanup log error: {}", e))?;

    Ok(CleanupResult {
        deleted_articles: total_deleted_articles,
        deleted_logs,
    })
}

/// Rewrite publish dates stored before `dates::normalize_published` existed.
///
/// Scraped articles used to keep the page's own words as their date ("1h
/// trước", or an author's name). Each is read again against the moment it was
/// fetched — so "21' trước" means 21 minutes before that, not before now — and
/// anything that still is not a date is cleared. Cheap to run on every listing:
/// once the old rows are fixed the scan finds none.
pub fn repair_published_dates(conn: &rusqlite::Connection) -> Result<usize, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, published_at, fetched_at FROM feed_articles
             WHERE published_at != '' AND published_at NOT LIKE '____-__-__T%'",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    let mut fixed = 0;
    for (id, published, fetched) in rows {
        let fetched_at = chrono::DateTime::parse_from_rfc3339(&fetched)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        let normalized = super::dates::normalize_published(&published, fetched_at);
        conn.execute(
            "UPDATE feed_articles SET published_at = ?1 WHERE id = ?2",
            params![normalized, id],
        )
        .map_err(|e| e.to_string())?;
        fixed += 1;
    }
    Ok(fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(conn: &rusqlite::Connection, id: &str, published: &str, fetched: &str) {
        conn.execute(
            "INSERT INTO feed_articles
                (id, feed_source_id, guid, title, url, author, content, summary,
                 published_at, fetched_at, thumbnail_url, word_count, read_time_minutes,
                 content_type, is_read, is_starred, is_read_later)
             VALUES (?1, 's1', ?1, 'Title', '', '', '', '', ?2, ?3, '', 0, 1, 'text/html', 0, 0, 0)",
            params![id, published, fetched],
        )
        .unwrap();
    }

    fn published(conn: &rusqlite::Connection, id: &str) -> String {
        conn.query_row("SELECT published_at FROM feed_articles WHERE id = ?1", [id], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn old_scraped_rows_get_real_dates_or_none() {
        let db = crate::db::DbBridge::new_in_memory_full().expect("schema");
        let conn = db.conn();
        insert(conn, "rel", "21' trước", "2026-09-30T10:00:00+00:00");
        insert(conn, "author", "Đỗ Duy Thọ", "2026-09-30T10:00:00+00:00");
        insert(conn, "ok", "2026-09-01T10:00:00+00:00", "2026-09-30T10:00:00+00:00");

        assert_eq!(repair_published_dates(conn).unwrap(), 2);
        // Relative to when it was fetched, not to now.
        assert_eq!(published(conn, "rel"), "2026-09-30T09:39:00+00:00");
        assert_eq!(published(conn, "author"), "");
        assert_eq!(published(conn, "ok"), "2026-09-01T10:00:00+00:00");
        // And there is nothing left to do the second time.
        assert_eq!(repair_published_dates(conn).unwrap(), 0);
    }
}
