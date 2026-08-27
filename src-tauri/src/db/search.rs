use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;
use std::time::Instant;

/// A term, plus a way for it to reach the `đ` shadow column.
///
/// `unicode61` folds tone marks but not `đ`, so `dong` and `đông` tokenize
/// differently and neither finds the other. The shadow column holds the `đ`
/// words with `đ` folded to `d`; matching a term against both places closes
/// the gap in both directions — `dong` reaches the folded `đông`, and `đông`
/// reaches it too because the term is folded on the way in.
///
/// Only when the term could possibly be involved. A term with no `d` and no
/// `đ` in it cannot match anything in that column, and adding the branch
/// anyway would grow every query for nothing.
fn with_d_stroke_branch(term: &str) -> String {
    let folded = crate::search_fold::fold_d_stroke(term);
    let touches_d = term.chars().any(|c| matches!(c, 'd' | 'D' | 'đ' | 'Đ'));
    if !touches_d {
        return format!("\"{}\"", term);
    }
    format!("(\"{}\" OR norm : \"{}\")", term, folded)
}

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

        // The legacy `files` table is deliberately not indexed here.
        //
        // It is still read once, by the schema migration that copies it into
        // `nodes`, and after that its rows describe nothing: the identities in
        // it were replaced by content digests. Indexing them put entries into
        // search that opened nothing when clicked.

        // Boards need no pass of their own: they are `nodes` rows like anything
        // else, and are picked up by the query below.

        // Index nodes (Universal Core)
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE node_type NOT LIKE 'finance_%'"
        ).map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt
            .query_map([], |row| {
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
                // A file node's body is not in the node — it is in `file_text`,
                // where extraction put it.
                //
                // This pass used to index `content`, which for a file node is
                // the empty string. So rebuilding the search index silently
                // erased the full text of every document in the vault, and
                // nothing put it back: extraction is recorded as done, so the
                // words were never read again.
                let content = if node_type == "file" {
                    self.file_text_joined(&id).unwrap_or_default()
                } else {
                    content
                };

                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&properties) {
                    if let Some(p) = json_val.get("path").and_then(|v| v.as_str()) {
                        search_path = p.to_string();
                    }
                    if let Some(tags) = json_val.get("tags").and_then(|v| v.as_array()) {
                        let tags_vec: Vec<String> = tags
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
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
            })
            .map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
            .filter_map(|r| r.ok())
            .count();

        // Index node blocks
        let mut stmt = self
            .conn
            .prepare("SELECT block_id, node_id, content FROM node_blocks")
            .map_err(|e| AppError::General(format!("FTS Reindex Query Error: {}", e)))?;
        let _ = stmt
            .query_map([], |row| {
                let block_id: String = row.get(0)?;
                let node_id: String = row.get(1)?;
                let content: String = row.get(2)?;
                let item_id = format!("{}#{}", node_id, block_id);
                self.index_row(
                    None, &item_id, "block", &block_id, "", &content, "", "", "", &node_id,
                );
                Ok(())
            })
            .map_err(|e| AppError::General(format!("FTS Reindex Map Error: {}", e)))?
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
        // Only the `đ` words, and only from the columns a person searches by
        // name — title, tags, body. Properties are machine keys; folding them
        // would add noise nobody is looking for.
        let norm =
            crate::search_fold::fold_d_stroke_words(&format!("{} {} {}", title, tags, content));

        let inserted = self.conn.execute(
            "INSERT INTO search_index (rowid, item_id, item_type, title, tags, content, properties, status, date, path, norm) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![rowid, item_id, item_type, title, tags, content, properties, status, date, path, norm],
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
            let _ = self
                .conn
                .execute("DELETE FROM search_index WHERE rowid = ?1", params![rid]);
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
            let _ = self
                .conn
                .execute("DELETE FROM search_index WHERE rowid = ?1", params![rid]);
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
                    match_parts.push(with_d_stroke_branch(term));
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
        //
        // Read as a value, not as a substring of the raw JSON. `LIKE
        // '%priority:1%'` is what this used to be, and it matched
        // `priority:10` as readily as `priority:1` — and `status:do` matched
        // `status:done`. A filter that quietly returns more than it was asked
        // for is worse than one that returns nothing.
        //
        // The key goes into the SQL text because `json_extract` needs a
        // literal path, so it is checked first; the value stays a parameter.
        for (key, val) in &parsed.property_filters {
            let Some(path) = crate::search::json_path_for(key) else {
                log::warn!("ignoring property filter on unusable key '{}'", key);
                continue;
            };
            let clause = format!(
                " AND lower(CAST(json_extract(properties, '{}') AS TEXT)) = ?{}",
                path, param_idx
            );
            sql.push_str(&clause);
            count_sql.push_str(&clause);
            param_values.push(val.to_lowercase());
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

    fn upsert_with_props(db: &DbBridge, item_id: &str, title: &str, props: serde_json::Value) {
        db.upsert_search_entry(
            item_id,
            "task",
            title,
            "",
            "body",
            &props.to_string(),
            props.get("status").and_then(|s| s.as_str()),
            "2026-01-01",
            item_id,
        );
    }

    /// Search the way the app searches, so a test failure means a search
    /// somebody ran would have failed.
    fn find(db: &DbBridge, query: &str) -> Vec<String> {
        let parsed = crate::search::parse_query(query);
        db.search_fts(&parsed, 1, 50)
            .expect("search")
            .results
            .into_iter()
            .map(|r| r.id)
            .collect()
    }

    /// A property filter used to be a substring test against the raw JSON, so
    /// `priority:1` also matched `priority:10` and `status:do` matched
    /// `status:done`. A filter that quietly returns more than it was asked for
    /// is worse than one that returns nothing.
    #[test]
    fn a_property_filter_matches_the_value_not_a_piece_of_it() {
        let db = db();
        upsert_with_props(
            &db,
            "Tasks/one.md",
            "one",
            serde_json::json!({ "priority": "1" }),
        );
        upsert_with_props(
            &db,
            "Tasks/ten.md",
            "ten",
            serde_json::json!({ "priority": "10" }),
        );

        assert_eq!(find(&db, "priority:1"), vec!["Tasks/one.md"]);
        assert_eq!(find(&db, "priority:10"), vec!["Tasks/ten.md"]);
    }

    #[test]
    fn a_property_filter_matches_a_number_written_as_a_number() {
        // Frontmatter carries `priority: 2` as a JSON number, not a string.
        let db = db();
        upsert_with_props(
            &db,
            "Tasks/two.md",
            "two",
            serde_json::json!({ "priority": 2 }),
        );

        assert_eq!(find(&db, "priority:2"), vec!["Tasks/two.md"]);
    }

    /// The key reaches the SQL as text, because `json_extract` needs a literal
    /// path. Everything that is not a plain key is refused rather than
    /// escaped, and refusing means the filter is dropped, not that the search
    /// returns somebody else's notes.
    #[test]
    fn a_key_that_is_not_a_key_cannot_reach_the_query() {
        use crate::search::json_path_for;

        assert_eq!(json_path_for("priority"), Some("$.priority".to_string()));
        assert_eq!(json_path_for("due_date"), Some("$.due_date".to_string()));
        for bad in ["", "a'b", "a.b", "a[0]", "a b", "a\"b", &"x".repeat(65)] {
            assert_eq!(json_path_for(bad), None, "{bad:?} must not become a path");
        }
    }

    #[test]
    fn an_unusable_key_drops_its_filter_without_taking_the_search_down() {
        let db = db();
        upsert_with_props(
            &db,
            "Tasks/one.md",
            "one",
            serde_json::json!({ "priority": "1" }),
        );

        // The filter cannot be applied, so it is ignored; the search still runs.
        let hits = find(&db, "a'b:1");
        assert!(hits.is_empty() || hits == vec!["Tasks/one.md"], "{hits:?}");
    }

    /// The complaint this change answers: typing a Vietnamese word without its
    /// tone marks — which is how people type when they are moving — found
    /// nothing at all.
    #[test]
    fn a_word_typed_without_its_tone_marks_still_finds_the_note() {
        let db = db();
        upsert(&db, "Notes/company.md", "công ty cổ phần abc");
        upsert(&db, "Notes/invoice.md", "hoá đơn tháng này");

        assert_eq!(find(&db, "cong"), vec!["Notes/company.md"]);
        assert_eq!(find(&db, "hoa"), vec!["Notes/invoice.md"]);
        assert_eq!(find(&db, "thang"), vec!["Notes/invoice.md"]);
    }

    /// And the other direction still works: someone who does type the marks
    /// must not be punished for it.
    #[test]
    fn a_word_typed_with_its_tone_marks_finds_it_too() {
        let db = db();
        upsert(&db, "Notes/company.md", "công ty cổ phần abc");

        assert_eq!(find(&db, "công"), vec!["Notes/company.md"]);
        assert_eq!(find(&db, "cổ phần"), vec!["Notes/company.md"]);
    }

    /// Folding costs precision, and it is worth being explicit about how much:
    /// `hoa` now reaches `hóa` and `họa` alike. In a language where quick
    /// typing drops the marks, finding too much beats finding nothing.
    #[test]
    fn folding_widens_a_search_rather_than_narrowing_it() {
        let db = db();
        upsert(&db, "Notes/a.md", "hóa đơn");
        upsert(&db, "Notes/b.md", "hoa sen");

        let hits = find(&db, "hoa");
        assert_eq!(hits.len(), 2, "both should match: {hits:?}");
    }

    /// `đ` is a letter in its own right, so unicode61 never folds it and
    /// `dong` could not find `đông`. A shadow column carrying just the `đ`
    /// words, folded, closes that — see `crate::search_fold`.
    #[test]
    fn a_word_typed_with_a_plain_d_finds_one_written_with_a_stroked_d() {
        let db = db();
        upsert(&db, "Notes/east.md", "đông dương");
        upsert(&db, "Notes/order.md", "đơn hàng tháng ba");

        assert_eq!(find(&db, "dong"), vec!["Notes/east.md"]);
        assert_eq!(find(&db, "don"), vec!["Notes/order.md"]);
    }

    /// And typing the stroke still works, which is the half that would break
    /// if the term were only ever matched against the shadow column.
    #[test]
    fn a_word_typed_with_the_stroke_still_finds_it() {
        let db = db();
        upsert(&db, "Notes/east.md", "đông dương");

        assert_eq!(find(&db, "đông"), vec!["Notes/east.md"]);
    }

    /// The shadow column must not quietly rewrite the ranking of searches that
    /// have nothing to do with `đ`.
    ///
    /// Indexing a folded copy of the *whole* note would make every ordinary
    /// term match twice — once in the real columns, once in the copy — which
    /// doubles the term frequency BM25 ranks on. Carrying only the `đ` words
    /// is what avoids it, and this holds the index to that.
    #[test]
    fn only_the_stroked_words_reach_the_shadow_column() {
        let db = db();
        upsert(&db, "Notes/mixed.md", "báo cáo đơn hàng tháng này");

        let norm: String = db
            .conn()
            .query_row(
                "SELECT norm FROM search_index WHERE item_id = 'Notes/mixed.md'",
                [],
                |r| r.get(0),
            )
            .expect("read norm");

        // `dơn`, not `don`: this column folds the one letter the tokenizer
        // cannot, and the tokenizer strips the tone mark on the way in. The
        // search tests above prove the two meet in the middle.
        assert_eq!(norm, "dơn", "only the `đ` word belongs here, got {norm:?}");
    }

    /// A note with no `đ` in it adds nothing to the shadow column at all.
    #[test]
    fn a_note_without_a_stroked_d_carries_no_shadow_text() {
        let db = db();
        upsert(&db, "Notes/company.md", "công ty cổ phần abc");

        let norm: String = db
            .conn()
            .query_row(
                "SELECT norm FROM search_index WHERE item_id = 'Notes/company.md'",
                [],
                |r| r.get(0),
            )
            .expect("read norm");

        assert_eq!(norm, "");
    }

    /// A plain `d` word must not start matching `đ` words it never meant.
    /// `dương` and `đương` are different words, and both are real.
    #[test]
    fn the_shadow_column_widens_recall_without_losing_the_real_match() {
        let db = db();
        upsert(&db, "Notes/plain.md", "dương lịch");
        upsert(&db, "Notes/stroked.md", "đương nhiên");

        let hits = find(&db, "duong");
        assert!(hits.contains(&"Notes/plain.md".to_string()), "{hits:?}");
        assert!(hits.contains(&"Notes/stroked.md".to_string()), "{hits:?}");
    }

    /// Nothing about folding may reach the text the user sees. Search results
    /// quote the note, and quoting it back without its marks would look like
    /// the note itself had been damaged.
    #[test]
    fn results_still_carry_the_text_as_it_was_written() {
        let db = db();
        db.upsert_search_entry(
            "Notes/company.md",
            "note",
            "công ty cổ phần abc",
            "",
            "báo cáo tài chính",
            "{}",
            None,
            "2026-01-01",
            "Notes/company.md",
        );

        let parsed = crate::search::parse_query("cong");
        let hit = db
            .search_fts(&parsed, 1, 50)
            .expect("search")
            .results
            .remove(0);

        assert_eq!(hit.title, "công ty cổ phần abc");
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
