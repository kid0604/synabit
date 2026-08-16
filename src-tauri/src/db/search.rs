use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;
use std::time::Instant;

impl DbBridge {
    /// Rebuild the entire FTS5 search index from all data tables.
    /// Called on app startup or when the user requests a reindex.
    pub fn reindex_search(&self) -> AppResult<()> {
        // Clear existing index, and the map that describes it.
        self.conn
            .execute("DELETE FROM search_index", [])
            .map_err(|e| AppError::General(format!("FTS Clear Error: {}", e)))?;
        self.conn
            .execute("DELETE FROM search_index_rowids", [])
            .map_err(|e| AppError::General(format!("FTS Rowid Map Clear Error: {}", e)))?;

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
            self.index_row(
                None, &id, "file", &filename, &tags.join(" "), &extension, &props, "", &date, &path,
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        // Boards need no pass of their own: they are `nodes` rows like anything
        // else, and are picked up by the query below.

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
            self.index_row(
                None,
                &id,
                &node_type,
                &title,
                &tags_str,
                &content,
                &props_search,
                &status.unwrap_or_default(),
                &date,
                &search_path,
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
            self.index_row(
                None, &item_id, "block", &block_id, "", &content, "", "", "", &node_id,
            );
            Ok(())
        }).map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
        .filter_map(|r| r.ok())
        .count();

        Ok(())
    }

    /// Where an item currently sits in the FTS index, if it is in there.
    ///
    /// This lookup is the whole point of `search_index_rowids`: asking the FTS
    /// table itself would mean reading every row.
    fn fts_rowid_for(&self, item_id: &str) -> Option<i64> {
        self.conn
            .query_row(
                "SELECT fts_rowid FROM search_index_rowids WHERE item_id = ?1",
                params![item_id],
                |r| r.get::<_, i64>(0),
            )
            .ok()
    }

    /// Insert one row into the FTS index and record where it landed.
    ///
    /// Pass `rowid: Some(_)` to place the row back where the item already lived,
    /// or `None` to let FTS5 choose — in which case the choice has to be written
    /// to the map here, since it is the only moment SQLite will report it.
    #[allow(clippy::too_many_arguments)]
    fn index_row(
        &self,
        rowid: Option<i64>,
        item_id: &str,
        item_type: &str,
        title: &str,
        tags: &str,
        content: &str,
        properties: &str,
        status: &str,
        date: &str,
        path: &str,
    ) {
        let inserted = self.conn.execute(
            "INSERT INTO search_index (rowid, item_id, item_type, title, tags, content, properties, status, date, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![rowid, item_id, item_type, title, tags, content, properties, status, date, path],
        );

        if inserted.is_ok() && rowid.is_none() {
            let _ = self.conn.execute(
                "INSERT OR REPLACE INTO search_index_rowids (item_id, fts_rowid) VALUES (?1, ?2)",
                params![item_id, self.conn.last_insert_rowid()],
            );
        }
    }

    /// Insert or update a single entry in the FTS5 search index.
    ///
    /// FTS5 has no `ON CONFLICT`, so an update is a delete followed by an
    /// insert. The delete used to match on `item_id` and so read the entire
    /// index; it now goes straight to the row via the rowid map, and the
    /// reinsert reuses that same rowid so the map stays valid without a second
    /// write.
    ///
    /// Deliberately not wrapped in a transaction. The two statements are no
    /// less atomic than the pair they replace, and every caller here discards
    /// errors — a transaction that failed to open because some caller already
    /// held one would silently stop indexing rather than fail loudly. A torn
    /// write costs one stale entry, which a reindex repairs.
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

        let existing = self.fts_rowid_for(item_id);
        if let Some(rid) = existing {
            let _ = self.conn.execute(
                "DELETE FROM search_index WHERE rowid = ?1",
                params![rid],
            );
        }

        self.index_row(
            existing,
            item_id,
            item_type,
            title,
            tags,
            content,
            properties,
            status.unwrap_or(""),
            date,
            path,
        );
    }

    /// Remove an entry from the FTS5 search index.
    pub fn delete_search_entry(&self, item_id: &str) {
        if let Some(rid) = self.fts_rowid_for(item_id) {
            let _ = self.conn.execute(
                "DELETE FROM search_index WHERE rowid = ?1",
                params![rid],
            );
        }
        let _ = self.conn.execute(
            "DELETE FROM search_index_rowids WHERE item_id = ?1",
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

/// Tests for the FTS5 index's write path.
///
/// FTS5 has no `ON CONFLICT` and no secondary indexes, so "update this entry"
/// has to be spelled out as delete-then-insert, and the delete can only be made
/// fast by knowing the row's rowid. Both halves of that are easy to get wrong in
/// ways nothing else notices: a delete that matches nothing leaves a duplicate,
/// and a duplicate shows up as the same note appearing twice in search rather
/// than as an error.
#[cfg(test)]
mod tests {
    use crate::db::DbBridge;

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    fn count_all(db: &DbBridge) -> i64 {
        db.conn()
            .query_row("SELECT COUNT(*) FROM search_index", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
    }

    fn count_of(db: &DbBridge, item_id: &str) -> i64 {
        db.conn()
            .query_row(
                "SELECT COUNT(*) FROM search_index WHERE item_id = ?1",
                [item_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
    }

    fn upsert(db: &DbBridge, item_id: &str, title: &str) {
        db.upsert_search_entry(
            item_id,
            "note",
            title,
            "",
            "body",
            "{}",
            None,
            "2026-01-01",
            item_id,
        );
    }

    #[test]
    fn upserting_the_same_item_twice_leaves_exactly_one_entry() {
        let db = db();
        upsert(&db, "Notes/a.md", "First");
        upsert(&db, "Notes/a.md", "Second");

        assert_eq!(count_of(&db, "Notes/a.md"), 1);
        let title: String = db
            .conn()
            .query_row(
                "SELECT title FROM search_index WHERE item_id = ?1",
                ["Notes/a.md"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "Second", "the newer entry should have won");
    }

    #[test]
    fn deleting_an_entry_removes_it_and_leaves_its_neighbours_alone() {
        let db = db();
        upsert(&db, "Notes/a.md", "A");
        upsert(&db, "Notes/b.md", "B");

        db.delete_search_entry("Notes/a.md");

        assert_eq!(count_of(&db, "Notes/a.md"), 0);
        assert_eq!(count_of(&db, "Notes/b.md"), 1);
    }

    /// Finance nodes are deliberately excluded from search. Re-stated here
    /// because the check lives at the top of the upsert and is easy to drop.
    #[test]
    fn finance_nodes_never_enter_the_index() {
        let db = db();
        db.upsert_search_entry(
            "Finance/2026-08.json",
            "finance_month",
            "August",
            "",
            "",
            "{}",
            None,
            "2026-01-01",
            "Finance/2026-08.json",
        );

        assert_eq!(count_all(&db), 0);
    }

    /// Blocks are re-indexed every time their note is saved. Each save used to
    /// add another copy rather than replace the previous one, so a note edited
    /// fifty times contributed fifty copies of every block to the index — and
    /// the index is scanned linearly on every write, so the cost compounded.
    #[test]
    fn re_indexing_a_note_does_not_accumulate_duplicate_block_entries() {
        let db = db();
        let blocks = vec![
            ("blk001".to_string(), "first block".to_string()),
            ("blk002".to_string(), "second block".to_string()),
        ];

        db.upsert_node_blocks("Notes/a.md", blocks.clone()).unwrap();
        db.upsert_node_blocks("Notes/a.md", blocks.clone()).unwrap();
        db.upsert_node_blocks("Notes/a.md", blocks).unwrap();

        assert_eq!(count_of(&db, "Notes/a.md#blk001"), 1);
        assert_eq!(count_of(&db, "Notes/a.md#blk002"), 1);
    }

    #[test]
    fn dropping_a_notes_blocks_clears_them_from_the_index() {
        let db = db();
        db.upsert_node_blocks(
            "Notes/a.md",
            vec![("blk001".to_string(), "first".to_string())],
        )
        .unwrap();
        db.upsert_node_blocks(
            "Notes/b.md",
            vec![("blk002".to_string(), "other note".to_string())],
        )
        .unwrap();

        db.delete_node_blocks("Notes/a.md").unwrap();

        assert_eq!(count_of(&db, "Notes/a.md#blk001"), 0);
        assert_eq!(
            count_of(&db, "Notes/b.md#blk002"),
            1,
            "another note's blocks must survive"
        );
    }

    /// A full reindex must start from nothing. If it does not, every rebuild
    /// doubles the index.
    #[test]
    fn a_full_reindex_replaces_the_index_rather_than_adding_to_it() {
        let db = db();
        upsert(&db, "Notes/a.md", "A");

        db.reindex_search().unwrap();
        let after_first = count_all(&db);
        db.reindex_search().unwrap();

        assert_eq!(count_all(&db), after_first, "a second rebuild changed size");
    }
}
