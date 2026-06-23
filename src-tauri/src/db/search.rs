use rusqlite::params;
use std::time::Instant;
use crate::error::{AppError, AppResult};
use super::DbBridge;

impl DbBridge {
    /// Rebuild the entire FTS5 search index from all data tables.
    /// Called on app startup or when the user requests a reindex.
    pub fn reindex_search(&self) -> AppResult<()> {
        // Clear existing index
        self.conn
            .execute("DELETE FROM search_index", [])
            .map_err(|e| AppError::General(format!("FTS Clear Error: {}", e)))?;

        // Index files (with properties)
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, filename, tags, extension, modified_at, path, source_type FROM files",
            )
            .map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let filename: String = row.get(1)?;
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let extension: String = row.get(3)?;
            let date: String = row.get(4)?;
            let path: String = row.get(5)?;
            let source_type: String = row.get::<_, String>(6).unwrap_or_default();
            let props = format!("ext:{} source:{}", extension, source_type);
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'file', ?2, ?3, ?4, ?5, NULL, ?6, ?7)",
                params![id, filename, tags.join(" "), extension, props, date, path],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        // Index whiteboards
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, tags, path, updated_at FROM whiteboards")
            .map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let path: String = row.get(3)?;
            let date: String = row.get(4)?;
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'whiteboard', ?2, ?3, ?2, '', NULL, ?4, ?5)",
                params![id, title, tags.join(" "), date, path],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        // Index nodes (Universal Core)
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE node_type NOT LIKE 'finance_%'"
        ).map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let node_type: String = row.get(1)?;
            let title: String = row.get(2)?;
            let content: String = row.get(3)?;
            let properties: String = row.get(4)?;
            let date: String = row.get(5)?;
            // Attempt to extract tags, status, and priority from properties if present
            let mut tags_str = String::new();
            let mut status = None;
            let mut props_search = properties.clone();
            let mut search_path = id.clone();
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&properties) {
                if let Some(p) = json_val.get("path").and_then(|v| v.as_str()) {
                    search_path = p.to_string();
                }
                if let Some(tags) = json_val.get("tags").and_then(|v| v.as_array()) {
                    let tags_vec: Vec<String> = tags.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    tags_str = tags_vec.join(" ");
                }
                if let Some(s) = json_val.get("status").and_then(|v| v.as_str()) {
                    status = Some(s.to_string());
                }
                // Extract priority to append to properties text for BM25 search
                if let Some(p) = json_val.get("priority").and_then(|v| v.as_str()) {
                    props_search = format!("{} priority:{}", properties, p);
                }
            }
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![id, node_type, title, tags_str, content, props_search, status.unwrap_or("".to_string()), date, search_path],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        // Index node blocks
        let mut stmt = self
            .conn
            .prepare("SELECT block_id, node_id, content FROM node_blocks")
            .map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt.query_map([], |row| {
            let block_id: String = row.get(0)?;
            let node_id: String = row.get(1)?;
            let content: String = row.get(2)?;
            let item_id = format!("{}#{}", node_id, block_id);
            let _ = self.conn.execute(
                "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, 'block', ?2, '', ?3, '', '', '', ?4)",
                params![item_id, block_id, content, node_id],
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        Ok(())
    }

    /// Insert or update a single entry in the FTS5 search index.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_search_entry(
        &self,
        item_id: &str,
        item_type: &str,
        title: &str,
        tags: &str,
        content: &str,
        properties: &str,
        status: Option<&str>,
        date: &str,
        path: &str,
    ) {
        if item_type.starts_with("finance_") {
            return;
        }
        // FTS5 doesn't support ON CONFLICT, so delete + insert
        let _ = self.conn.execute(
            "DELETE FROM search_index WHERE item_id = ?1",
            params![item_id],
        );
        let _ = self.conn.execute(
            "INSERT INTO search_index (item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![item_id, item_type, title, tags, content, properties, status.unwrap_or(""), date, path],
        );
    }

    /// Remove an entry from the FTS5 search index.
    pub fn delete_search_entry(&self, item_id: &str) {
        let _ = self.conn.execute(
            "DELETE FROM search_index WHERE item_id = ?1",
            params![item_id],
        );
    }

    /// Perform a full-text search using FTS5 with BM25 ranking.
    pub fn search_fts(
        &self,
        parsed: &crate::search::ParsedQuery,
        page: u32,
        per_page: u32,
    ) -> AppResult<crate::search::SearchResponse> {
        let start = Instant::now();
        let offset = (page.saturating_sub(1)) * per_page;

        let has_fts_terms = !parsed.fts_terms.is_empty();
        let has_exclude = !parsed.exclude_terms.is_empty();

        // All parameter values collected here; SQL uses numbered placeholders ?N
        let mut param_values: Vec<String> = Vec::new();
        // Tracks the next available placeholder index (1-based for SQLite)
        let mut param_idx: usize = 1;

        // Build the SQL query dynamically
        let mut sql;
        let mut count_sql;

        if has_fts_terms || has_exclude {
            // Build FTS5 MATCH expression
            let mut match_parts: Vec<String> = Vec::new();
            for term in &parsed.fts_terms {
                if term.starts_with('"') && term.ends_with('"') {
                    // Phrase query — pass directly
                    match_parts.push(term.clone());
                } else if parsed.title_only {
                    // Restrict to title column
                    match_parts.push(format!("title : \"{}\"", term));
                } else {
                    // Search across title (boosted), tags, content with column weighting
                    // FTS5: {col1 col2} : term
                    match_parts.push(format!("\"{}\"", term));
                }
            }
            for term in &parsed.exclude_terms {
                match_parts.push(format!("NOT \"{}\"", term));
            }

            let fts_expr = match_parts.join(" AND ");
            param_values.push(fts_expr);
            param_idx += 1; // ?1 is used for the MATCH expression

            // Main query with BM25 ranking
            // bm25 weights: item_id=0, item_type=0, title=10, tags=5, content=1, properties=3
            sql = "SELECT item_id, item_type, title, snippet(search_index, 4, '<mark>', '</mark>', '…', 48) as snip, tags, date, path, bm25(search_index, 0.0, 0.0, 10.0, 5.0, 1.0, 3.0) as rank, status FROM search_index WHERE search_index MATCH ?1".to_string();
            count_sql = "SELECT COUNT(*) FROM search_index WHERE search_index MATCH ?1".to_string();
        } else {
            // No FTS terms — browse mode (filter only)
            sql = "SELECT item_id, item_type, title, substr(content, 1, 200) as snip, tags, date, path, 0.0 as rank, status FROM search_index WHERE 1=1".to_string();
            count_sql = "SELECT COUNT(*) FROM search_index WHERE 1=1".to_string();
        }

        // Apply filters — all use parameterized placeholders
        if let Some(type_val) = &parsed.type_filter {
            sql.push_str(&format!(" AND item_type = ?{}", param_idx));
            count_sql.push_str(&format!(" AND item_type = ?{}", param_idx));
            param_values.push(type_val.clone());
            param_idx += 1;
        }

        for tag in &parsed.tag_filters {
            sql.push_str(&format!(" AND tags LIKE ?{}", param_idx));
            count_sql.push_str(&format!(" AND tags LIKE ?{}", param_idx));
            param_values.push(format!("%{}%", tag));
            param_idx += 1;
        }

        if let Some(status_val) = &parsed.status_filter {
            sql.push_str(&format!(" AND status = ?{}", param_idx));
            count_sql.push_str(&format!(" AND status = ?{}", param_idx));
            param_values.push(status_val.clone());
            param_idx += 1;
        }

        // Apply generic property filters
        for (key, val) in &parsed.property_filters {
            sql.push_str(&format!(" AND properties LIKE ?{}", param_idx));
            count_sql.push_str(&format!(" AND properties LIKE ?{}", param_idx));
            param_values.push(format!("%{}:{}%", key, val));
            param_idx += 1;
        }

        // Ordering
        if has_fts_terms {
            sql.push_str(" ORDER BY rank"); // BM25 returns negative values, lower = better
        } else {
            sql.push_str(" ORDER BY date DESC");
        }

        // LIMIT and OFFSET as parameters
        sql.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_idx, param_idx + 1));
        param_values.push(per_page.to_string());
        param_values.push(offset.to_string());

        // Execute count query (uses only the filter params, not LIMIT/OFFSET)
        let count_params: Vec<&str> = param_values
            .iter()
            .take(param_values.len() - 2) // exclude LIMIT and OFFSET
            .map(|s| s.as_str())
            .collect();
        let total_count: u32 = self
            .conn
            .query_row(
                &count_sql,
                rusqlite::params_from_iter(count_params.iter()),
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Execute search query (all params including LIMIT/OFFSET)
        let all_params: Vec<&str> = param_values.iter().map(|s| s.as_str()).collect();
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| AppError::General(format!("FTS Search Prepare Error: {}", e)))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(all_params.iter()), |row| {
                let tags_str: String = row.get(4)?;
                let tags: Vec<String> = tags_str
                    .split_whitespace()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                let rank: f64 = row.get(7)?;
                Ok(crate::search::SearchResult {
                    id: row.get(0)?,
                    item_type: row.get(1)?,
                    title: row.get(2)?,
                    snippet: row.get(3)?,
                    tags,
                    date: row.get(5)?,
                    path: row.get(6)?,
                    score: -rank, // BM25 returns negative; negate for display
                    status: row.get(8)?,
                })
            })
            .map_err(|e| AppError::General(format!("FTS Search Map Error: {}", e)))?;

        let mut results = Vec::new();
        for row in rows.flatten() {
            results.push(row);
        }

        let elapsed = start.elapsed().as_millis() as u64;

        // Case-sensitive post-filter: FTS5 is case-insensitive, so we filter results here
        if parsed.case_sensitive && !parsed.fts_terms.is_empty() {
            let original_terms: Vec<&str> = parsed
                .fts_terms
                .iter()
                .map(|t| t.trim_matches('"'))
                .filter(|t| !t.is_empty())
                .collect();

            results.retain(|r| {
                let haystack = format!(
                    "{} {} {}",
                    r.title,
                    r.snippet.replace("<mark>", "").replace("</mark>", ""),
                    r.tags.join(" ")
                );
                original_terms.iter().all(|term| haystack.contains(term))
            });
            let filtered_count = results.len() as u32;
            return Ok(crate::search::SearchResponse {
                results,
                total_count: filtered_count,
                query_time_ms: elapsed,
            });
        }

        Ok(crate::search::SearchResponse {
            results,
            total_count,
            query_time_ms: elapsed,
        })
    }
}
