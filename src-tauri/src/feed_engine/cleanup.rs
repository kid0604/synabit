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
/// - Rebuilds FTS5 index after cleanup
pub fn run_cleanup(
    conn: &rusqlite::Connection,
    max_age_days: i64,
    max_per_feed: i64,
) -> Result<CleanupResult, String> {
    let mut total_deleted_articles: usize = 0;
    let mut deleted_logs: usize = 0;

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

    deleted_logs = conn
        .execute(
            "DELETE FROM feed_fetch_log WHERE fetched_at < ?1",
            params![log_cutoff_str],
        )
        .map_err(|e| format!("Cleanup log error: {}", e))?;

    // 4. Rebuild FTS5 index after cleanup
    let _ = conn.execute("DELETE FROM feed_articles_fts", []);
    let _ = conn.execute(
        "INSERT INTO feed_articles_fts (rowid, title, author, content, summary)
         SELECT rowid, title, author, content, summary FROM feed_articles",
        [],
    );

    Ok(CleanupResult {
        deleted_articles: total_deleted_articles,
        deleted_logs,
    })
}
