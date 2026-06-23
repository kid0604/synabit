use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;

impl DbBridge {
    /// Search feed articles using FTS5 for RAG context.
    /// Returns: (id, title, summary, published_at).
    /// Handles errors gracefully — returns empty vec on failure.
    pub fn search_feed_articles_for_rag(&self, query: &str, limit: u32) -> Vec<(String, String, String, String)> {
        // Sanitize query for FTS5 MATCH — wrap each term in quotes
        let sanitized: Vec<String> = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect();

        if sanitized.is_empty() {
            return Vec::new();
        }

        let fts_expr = sanitized.join(" OR ");

        let result = self.conn.prepare(
            "SELECT a.id, a.title, a.summary, a.published_at \
             FROM feed_articles a \
             JOIN feed_articles_fts fts ON a.rowid = fts.rowid \
             WHERE feed_articles_fts MATCH ?1 \
             ORDER BY rank \
             LIMIT ?2"
        );

        match result {
            Ok(mut stmt) => {
                let rows = stmt.query_map(params![fts_expr, limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                });
                match rows {
                    Ok(mapped) => mapped.flatten().collect(),
                    Err(e) => {
                        log::warn!("[RAG] Feed article FTS query failed: {}", e);
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                log::warn!("[RAG] Failed to prepare feed article FTS query: {}", e);
                Vec::new()
            }
        }
    }

    /// Search finance nodes for RAG context (these are excluded from the main FTS index).
    /// Returns: (id, title, content, properties).
    /// Uses direct SQL LIKE matching on the nodes table.
    pub fn search_finance_nodes_for_rag(&self, terms: &[String], limit: u32) -> Vec<(String, String, String, String)> {
        if terms.is_empty() {
            return Vec::new();
        }

        // Build WHERE clause: match any term against title or content
        let mut conditions: Vec<String> = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        for (i, term) in terms.iter().enumerate() {
            let idx_title = i * 2 + 1;
            let idx_content = i * 2 + 2;
            conditions.push(format!(
                "(title LIKE ?{} OR content LIKE ?{})",
                idx_title, idx_content
            ));
            let pattern = format!("%{}%", term);
            param_values.push(pattern.clone());
            param_values.push(pattern);
        }

        let where_clause = conditions.join(" OR ");
        let param_idx_limit = param_values.len() + 1;
        let sql = format!(
            "SELECT id, title, content, properties FROM nodes \
             WHERE node_type LIKE 'finance_%' AND ({}) \
             LIMIT ?{}",
            where_clause, param_idx_limit
        );
        param_values.push(limit.to_string());

        let params_refs: Vec<&str> = param_values.iter().map(|s| s.as_str()).collect();

        match self.conn.prepare(&sql) {
            Ok(mut stmt) => {
                let rows = stmt.query_map(
                    rusqlite::params_from_iter(params_refs.iter()),
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                );
                match rows {
                    Ok(mapped) => mapped.flatten().collect(),
                    Err(e) => {
                        log::warn!("[RAG] Finance nodes query failed: {}", e);
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                log::warn!("[RAG] Failed to prepare finance nodes query: {}", e);
                Vec::new()
            }
        }
    }

    /// Get related nodes via node_edges (1-hop graph expansion) for RAG context.
    /// Returns: (id, title, node_type) for connected nodes.
    pub fn get_related_nodes_for_rag(&self, node_id: &str, limit: u32) -> Vec<(String, String, String)> {
        let result = self.conn.prepare(
            "SELECT DISTINCT n.id, n.title, n.node_type \
             FROM node_edges e \
             JOIN nodes n ON (n.id = CASE WHEN e.source_id = ?1 THEN e.target_id ELSE e.source_id END) \
             WHERE e.source_id = ?1 OR e.target_id = ?1 \
             LIMIT ?2"
        );

        match result {
            Ok(mut stmt) => {
                let rows = stmt.query_map(params![node_id, limit], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                });
                match rows {
                    Ok(mapped) => mapped.flatten().collect(),
                    Err(e) => {
                        log::warn!("[RAG] Related nodes query failed: {}", e);
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                log::warn!("[RAG] Failed to prepare related nodes query: {}", e);
                Vec::new()
            }
        }
    }

    /// Search file nodes with SQL-level filtering by query, extension, tag, and person.
    /// Avoids loading all file nodes into memory — filtering happens in the database.
    /// Returns NodeMetadata for matching files, limited to `limit` results.
    pub fn search_files_filtered(
        &self,
        query: &str,
        extension: &str,
        tag: &str,
        person: &str,
        limit: u32,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut conditions = vec!["node_type = 'file'".to_string()];
        let mut param_values: Vec<String> = Vec::new();
        let mut param_idx: usize = 1;

        // Query matches title (filename) via LIKE
        if !query.is_empty() {
            conditions.push(format!("title LIKE ?{}", param_idx));
            param_values.push(format!("%{}%", query));
            param_idx += 1;
        }

        // Extension filter via json_extract on properties
        if !extension.is_empty() {
            conditions.push(format!(
                "LOWER(json_extract(properties, '$.extension')) = LOWER(?{})",
                param_idx
            ));
            param_values.push(extension.to_string());
            param_idx += 1;
        }

        // Tag filter via json_each on properties.tags
        if !tag.is_empty() {
            conditions.push(format!(
                "EXISTS (SELECT 1 FROM json_each(properties, '$.tags') WHERE LOWER(value) LIKE LOWER(?{}))",
                param_idx
            ));
            param_values.push(format!("%{}%", tag));
            param_idx += 1;
        }

        // Person filter via json_each on properties.people
        if !person.is_empty() {
            conditions.push(format!(
                "EXISTS (SELECT 1 FROM json_each(properties, '$.people') WHERE LOWER(value) LIKE LOWER(?{}))",
                param_idx
            ));
            param_values.push(format!("%{}%", person));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp \
             FROM nodes WHERE {} ORDER BY updated_at DESC LIMIT ?{}",
            where_clause, param_idx
        );
        param_values.push(limit.to_string());

        let params_refs: Vec<&str> = param_values.iter().map(|s| s.as_str()).collect();

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| AppError::General(format!("DB Query Error (search_files_filtered): {}", e)))?;

        let rows = stmt
            .query_map(rusqlite::params_from_iter(params_refs.iter()), |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (search_files_filtered): {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }
}
