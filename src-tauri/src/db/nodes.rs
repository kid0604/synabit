use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;

impl DbBridge {
    pub fn upsert_node(&self, node: &crate::models::node::NodeMetadata) -> AppResult<()> {
        let properties_json = serde_json::to_string(&node.properties)?;
        self.conn.execute(
            "INSERT INTO nodes (id, node_type, title, content, properties, created_at, updated_at, timestamp, stable_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                node_type=excluded.node_type,
                title=excluded.title,
                content=excluded.content,
                properties=excluded.properties,
                updated_at=excluded.updated_at,
                timestamp=excluded.timestamp,
                stable_id=excluded.stable_id",
            params![node.id, node.node_type, node.title, node.content, properties_json, node.created_at, node.updated_at, node.timestamp, node.stable_id()],
        ).map_err(|e| AppError::General(format!("DB Upsert Node Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_node(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM nodes WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Node Error: {}", e)))?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> AppResult<Option<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE id = ?1")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let mut rows = stmt
            .query_map(params![id], |row| {
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
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn get_nodes_by_type(
        &self,
        node_type: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE node_type = ?1 ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map(params![node_type], |row| {
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
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    /// How much of a node's body a list screen gets to see.
    const PREVIEW_CHARS: i64 = 150;

    /// Every node of a type, without their bodies.
    ///
    /// `substr` runs inside SQLite, so a five-thousand-note vault moves a
    /// couple of hundred kilobytes here instead of twenty megabytes. SQLite
    /// counts characters rather than bytes on a TEXT column, so the cut cannot
    /// land in the middle of one.
    pub fn get_node_summaries_by_type(
        &self,
        node_type: &str,
    ) -> AppResult<Vec<crate::models::node::NodeSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, node_type, title, substr(content, 1, ?2), properties,
                        created_at, updated_at, timestamp
                 FROM nodes WHERE node_type = ?1 ORDER BY updated_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (summaries): {}", e)))?;

        let rows = stmt
            .query_map(params![node_type, Self::PREVIEW_CHARS], |row| {
                let props_str: String = row.get(4)?;
                Ok(crate::models::node::NodeSummary {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    preview: row.get(3)?,
                    properties: serde_json::from_str(&props_str)
                        .unwrap_or(serde_json::Value::Null),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (summaries): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    /// Find the `file` node describing a given path on disk.
    ///
    /// File nodes are keyed by UUID rather than by path, so this is how the
    /// scanner recognises a file it has already seen. It used to be answered by
    /// walking every file node in the database on each file examined, which
    /// made indexing a folder quadratic in the number of files in it. The
    /// expression index declared alongside the `nodes` table is what lets the
    /// same question be asked directly.
    pub fn get_file_node_by_path(
        &self,
        path: &str,
    ) -> AppResult<Option<crate::models::node::NodeMetadata>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp
                 FROM nodes
                 WHERE node_type = 'file' AND json_extract(properties, '$.path') = ?1",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (file by path): {}", e)))?;

        let mut rows = stmt
            .query_map(params![path], |row| {
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
            .map_err(|e| AppError::General(format!("DB Map Error (file by path): {}", e)))?;

        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn get_active_tasks_and_events(&self) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp 
             FROM nodes 
             WHERE node_type IN ('task', 'event', 'person') 
             AND (
                 (node_type = 'task' AND json_extract(properties, '$.status') NOT IN ('done', 'canceled') AND json_extract(properties, '$.due_date') IS NOT NULL AND json_extract(properties, '$.due_date') != '')
                 OR (node_type = 'event' AND json_extract(properties, '$.start_at') IS NOT NULL AND json_extract(properties, '$.start_at') != '')
                 OR (node_type = 'person' AND json_extract(properties, '$.birthday') IS NOT NULL AND json_extract(properties, '$.birthday') != '')
             )"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_active_tasks_and_events): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
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
            .map_err(|e| {
                AppError::General(format!("DB Map Error (get_active_tasks_and_events): {}", e))
            })?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_linked_nodes(
        &self,
        _target_title: &str,
        target_id: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        if target_id.is_empty() {
            return Ok(Vec::new());
        }

        // Edges are recorded between stable identities rather than paths, so a
        // caller naming a node by its current path has to be translated first.
        // Callers that already hold a stable id — the assistant's tools, an
        // edge's own `target_id` — pass through the COALESCE untouched.
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.node_type, n.title, n.content, n.properties, n.created_at, n.updated_at, n.timestamp
             FROM node_edges e
             JOIN nodes n ON n.stable_id = e.source_id
             WHERE e.target_id = COALESCE(
                 (SELECT stable_id FROM nodes WHERE id = ?1),
                 ?1
             )
             ORDER BY n.updated_at DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_linked_nodes): {}", e)))?;

        let rows = stmt
            .query_map(params![target_id], |row| {
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
            .map_err(|e| AppError::General(format!("DB Map Error (get_linked_nodes): {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_node_title(&self, node_id: &str) -> Option<String> {
        let mut stmt = self
            .conn
            .prepare("SELECT title FROM nodes WHERE id = ?1")
            .ok()?;
        stmt.query_row(params![node_id], |row| row.get::<_, String>(0))
            .ok()
    }

    pub fn get_all_nodes(&self) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
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
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut nodes = Vec::new();
        for n in rows.flatten() {
            nodes.push(n);
        }
        Ok(nodes)
    }

    pub fn get_all_tags_with_counts(&self) -> AppResult<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT json_each.value, COUNT(*) 
             FROM nodes, json_each(nodes.properties, '$.tags') 
             GROUP BY json_each.value 
             ORDER BY COUNT(*) DESC, json_each.value ASC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (get_all_tags): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let tag: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((tag, count))
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for row in rows.flatten() {
            results.push(row);
        }
        Ok(results)
    }

    pub fn get_nodes_by_tag(
        &self,
        target_tag: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp 
             FROM nodes 
             WHERE EXISTS (
                 SELECT 1 FROM json_each(nodes.properties, '$.tags') WHERE value = ?1
             )"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_nodes_by_tag): {}", e)))?;

        let rows = stmt
            .query_map(params![target_tag], |row| {
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
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut nodes = Vec::new();
        for n in rows.flatten() {
            nodes.push(n);
        }
        Ok(nodes)
    }
}

/// Tests for the query layer over the `nodes` table.
///
/// The Universal Node model keeps every type in one table and pushes each
/// type's own fields into a JSON `properties` blob. That makes the schema
/// trivial and the queries subtle: filters like "an unfinished task that has a
/// due date" are `json_extract` expressions, invisible to the type system and
/// unchecked by the compiler. These tests are the only thing standing between
/// a typo in one of those paths and a list that silently comes back empty.
#[cfg(test)]
mod tests {
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use serde_json::json;

    fn node(id: &str, node_type: &str, properties: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: id.to_string(),
            content: String::new(),
            properties,
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    /// `updated_at` is a TEXT column sorted as text, so callers only get a
    /// meaningful order out of it while the format stays fixed-width.
    fn node_updated(id: &str, node_type: &str, updated_at: &str) -> NodeMetadata {
        let mut n = node(id, node_type, json!({}));
        n.updated_at = updated_at.to_string();
        n
    }

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    #[test]
    fn a_node_survives_a_round_trip_with_its_properties_intact() {
        let db = db();
        db.upsert_node(&node("Tasks/a.md", "task", json!({"status": "doing", "n": 3})))
            .unwrap();

        let found = db.get_node("Tasks/a.md").unwrap().expect("node should exist");

        assert_eq!(found.node_type, "task");
        assert_eq!(
            found.properties.get("status").and_then(|v| v.as_str()),
            Some("doing")
        );
        assert_eq!(found.properties.get("n").and_then(|v| v.as_i64()), Some(3));
        assert!(db.get_node("Tasks/missing.md").unwrap().is_none());
    }

    /// The id is the primary key, so writing the same path twice must update
    /// rather than accumulate. The vault scan re-upserts on every edit.
    #[test]
    fn upserting_the_same_id_updates_in_place_instead_of_duplicating() {
        let db = db();
        db.upsert_node(&node("n.md", "note", json!({"v": 1}))).unwrap();
        db.upsert_node(&node("n.md", "task", json!({"v": 2}))).unwrap();

        let all = db.get_all_nodes().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].node_type, "task");
        assert_eq!(all[0].properties.get("v").and_then(|v| v.as_i64()), Some(2));
    }

    #[test]
    fn nodes_are_filtered_by_type_and_returned_newest_first() {
        let db = db();
        db.upsert_node(&node_updated("old.md", "note", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("new.md", "note", "2026-06-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("t.md", "task", "2026-09-01 00:00:00"))
            .unwrap();

        let notes = db.get_nodes_by_type("note").unwrap();

        assert_eq!(
            notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["new.md", "old.md"]
        );
    }

    #[test]
    fn deleting_a_node_removes_it() {
        let db = db();
        db.upsert_node(&node("n.md", "note", json!({}))).unwrap();
        db.delete_node("n.md").unwrap();

        assert!(db.get_node("n.md").unwrap().is_none());
        // Deleting something already gone is not an error — the vault scan
        // relies on being able to fire it blindly.
        db.delete_node("n.md").unwrap();
    }

    #[test]
    fn a_node_is_found_by_any_one_of_its_tags() {
        let db = db();
        db.upsert_node(&node("a.md", "note", json!({"tags": ["work", "urgent"]})))
            .unwrap();
        db.upsert_node(&node("b.md", "note", json!({"tags": ["home"]})))
            .unwrap();
        db.upsert_node(&node("c.md", "note", json!({}))).unwrap();

        assert_eq!(db.get_nodes_by_tag("work").unwrap().len(), 1);
        assert_eq!(db.get_nodes_by_tag("urgent").unwrap()[0].id, "a.md");
        assert_eq!(db.get_nodes_by_tag("home").unwrap()[0].id, "b.md");
        assert!(db.get_nodes_by_tag("nonexistent").unwrap().is_empty());
    }

    /// Tag counts drive the tag sidebar. A node carrying no tags at all must
    /// not break the aggregate — untagged notes are the common case.
    #[test]
    fn tags_are_counted_across_nodes_most_used_first() {
        let db = db();
        db.upsert_node(&node("a.md", "note", json!({"tags": ["work", "urgent"]})))
            .unwrap();
        db.upsert_node(&node("b.md", "task", json!({"tags": ["work"]})))
            .unwrap();
        db.upsert_node(&node("c.md", "note", json!({}))).unwrap();

        let counts = db.get_all_tags_with_counts().unwrap();

        assert_eq!(counts[0], ("work".to_string(), 2));
        assert_eq!(counts[1], ("urgent".to_string(), 1));
        assert_eq!(counts.len(), 2);
    }

    /// The agenda query, and the densest `json_extract` expression in the
    /// codebase. Each exclusion below is a separate clause in that SQL; if any
    /// one of the JSON paths is wrong, the agenda quietly loses a whole class
    /// of item rather than failing.
    #[test]
    fn the_agenda_takes_only_dated_open_items() {
        let db = db();

        db.upsert_node(&node(
            "open.md",
            "task",
            json!({"status": "doing", "due_date": "2026-08-20"}),
        ))
        .unwrap();
        db.upsert_node(&node(
            "done.md",
            "task",
            json!({"status": "done", "due_date": "2026-08-20"}),
        ))
        .unwrap();
        db.upsert_node(&node(
            "canceled.md",
            "task",
            json!({"status": "canceled", "due_date": "2026-08-20"}),
        ))
        .unwrap();
        db.upsert_node(&node("undated.md", "task", json!({"status": "doing"})))
            .unwrap();
        db.upsert_node(&node(
            "blank-date.md",
            "task",
            json!({"status": "doing", "due_date": ""}),
        ))
        .unwrap();
        db.upsert_node(&node("ev.md", "event", json!({"start_at": "2026-08-20T09:00"})))
            .unwrap();
        db.upsert_node(&node("ev-undated.md", "event", json!({}))).unwrap();
        db.upsert_node(&node("p.md", "person", json!({"birthday": "1990-05-05"})))
            .unwrap();
        db.upsert_node(&node("p-nobday.md", "person", json!({}))).unwrap();
        db.upsert_node(&node(
            "note.md",
            "note",
            json!({"due_date": "2026-08-20", "status": "doing"}),
        ))
        .unwrap();

        let mut ids = db
            .get_active_tasks_and_events()
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect::<Vec<_>>();
        ids.sort();

        assert_eq!(ids, vec!["ev.md", "open.md", "p.md"]);
    }

    /// Backlinks resolve through `node_edges` by id alone. The title argument
    /// the caller passes is ignored, which matters: a node is reachable from
    /// its backlinks even after it has been renamed.
    #[test]
    fn backlinks_are_resolved_by_id_and_ignore_the_title_argument() {
        use crate::db::NodeEdge;

        let db = db();
        db.upsert_node(&node("src.md", "note", json!({}))).unwrap();
        db.upsert_node(&node("target.md", "project", json!({}))).unwrap();
        db.upsert_node(&node("unrelated.md", "note", json!({}))).unwrap();
        db.upsert_node_edge(&NodeEdge {
            id: "e1".to_string(),
            source_id: "src.md".to_string(),
            target_id: "target.md".to_string(),
            edge_type: "wikilink".to_string(),
            relation: None,
            created_at: "2026-01-01 00:00:00".to_string(),
        })
        .unwrap();

        let linked = db
            .get_linked_nodes("a title nobody has", "target.md")
            .unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, "src.md");

        // An empty id must not be read as "match everything".
        assert!(db.get_linked_nodes("target", "").unwrap().is_empty());
    }

    /// The point of the summary query: the body does not come along.
    #[test]
    fn a_summary_carries_the_opening_of_the_body_and_not_the_body() {
        let db = db();
        let long_body = "A".repeat(5_000);
        let mut n = node("Notes/a.md", "note", json!({"tags": ["work"], "pinned": true}));
        n.title = "A note".to_string();
        n.content = long_body;
        db.upsert_node(&n).unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].title, "A note");
        assert_eq!(summaries[0].preview.chars().count(), 150);
        assert_eq!(
            summaries[0].properties.get("pinned").and_then(|v| v.as_bool()),
            Some(true),
            "properties drive the list, so they must survive"
        );
    }

    /// The cut happens in SQL, where a naive byte offset would slice a
    /// multi-byte character in half and hand the frontend broken text.
    #[test]
    fn a_preview_never_cuts_a_character_in_half() {
        let db = db();
        let mut n = node("Notes/vi.md", "note", json!({}));
        // Vietnamese, so every character is multi-byte in UTF-8.
        n.content = "Đường đi khó, không khó vì ngăn sông cách núi. ".repeat(20);
        db.upsert_node(&n).unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();
        let preview = &summaries[0].preview;

        assert_eq!(preview.chars().count(), 150);
        assert!(
            n.content.starts_with(preview.as_str()),
            "the preview should be the opening of the body, intact"
        );
    }

    #[test]
    fn a_body_shorter_than_the_preview_comes_back_whole() {
        let db = db();
        let mut n = node("Notes/short.md", "note", json!({}));
        n.content = "Just a line.".to_string();
        db.upsert_node(&n).unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();
        assert_eq!(summaries[0].preview, "Just a line.");
    }

    #[test]
    fn summaries_are_filtered_by_type_and_ordered_like_the_full_query() {
        let db = db();
        db.upsert_node(&node_updated("old.md", "note", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("new.md", "note", "2026-06-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("t.md", "task", "2026-09-01 00:00:00"))
            .unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();
        assert_eq!(
            summaries.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["new.md", "old.md"]
        );
    }

    /// The scanner's "have I seen this file before?" question. It has to match
    /// on the path inside the node's properties, because a file node's id is a
    /// UUID that says nothing about where the file is.
    #[test]
    fn a_file_node_is_found_by_the_path_it_describes() {
        let db = db();
        db.upsert_node(&node(
            "uuid-1",
            "file",
            json!({"path": "/Users/x/Documents/report.pdf"}),
        ))
        .unwrap();
        db.upsert_node(&node("uuid-2", "file", json!({"path": "/Users/x/other.pdf"})))
            .unwrap();

        let found = db
            .get_file_node_by_path("/Users/x/Documents/report.pdf")
            .unwrap()
            .expect("the file node should be found by its path");
        assert_eq!(found.id, "uuid-1");

        assert!(db
            .get_file_node_by_path("/Users/x/never-seen.pdf")
            .unwrap()
            .is_none());
    }

    /// The lookup is restricted to file nodes. Disk-backed nodes also carry a
    /// `path` property in places, and matching one of those would hand the
    /// scanner someone else's node to overwrite.
    #[test]
    fn the_file_lookup_ignores_nodes_of_other_types_sharing_a_path() {
        let db = db();
        db.upsert_node(&node(
            "Notes/a.md",
            "note",
            json!({"path": "/Users/x/report.pdf"}),
        ))
        .unwrap();

        assert!(db
            .get_file_node_by_path("/Users/x/report.pdf")
            .unwrap()
            .is_none());
    }

    /// The index behind the lookup is a partial index on a JSON expression;
    /// if it stops being used the query still returns the right answer, so
    /// only the plan reveals a regression.
    #[test]
    fn the_file_path_lookup_uses_its_index_rather_than_reading_every_node() {
        let db = db();
        let plan: String = db
            .conn()
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM nodes \
                 WHERE node_type = 'file' AND json_extract(properties, '$.path') = ?1",
                ["x"],
                |r| r.get(3),
            )
            .unwrap();

        assert!(
            plan.contains("idx_nodes_file_path"),
            "the file path lookup stopped using its index: {plan}"
        );
    }

    /// A node that carries the identity sync gave it.
    fn node_with_stable_id(id: &str, stable: &str) -> NodeMetadata {
        node(id, "note", json!({ "node_id": stable }))
    }

    fn link(source: &str, target: &str) -> crate::db::NodeEdge {
        crate::db::NodeEdge {
            id: format!("{source}->{target}"),
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type: "wikilink".to_string(),
            relation: None,
            created_at: "2026-01-01 00:00:00".to_string(),
        }
    }

    /// The property the whole change exists for.
    ///
    /// Archiving a task moves its file into `archived/`. Keyed by path, every
    /// backlink to that task pointed at a location nothing was at any more, and
    /// the links vanished from the interface with no error. Keyed by the
    /// identity in the file's own frontmatter, the move is invisible to them.
    #[test]
    fn a_backlink_survives_the_linked_note_moving_to_another_folder() {
        let db = db();
        db.upsert_node(&node_with_stable_id("Notes/writer.md", "uuid-writer"))
            .unwrap();
        db.upsert_node(&node_with_stable_id("Tasks/target.md", "uuid-target"))
            .unwrap();
        db.upsert_node_edge(&link("uuid-writer", "uuid-target"))
            .unwrap();

        let before = db.get_linked_nodes("", "Tasks/target.md").unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].id, "Notes/writer.md");

        // Archive it: the row moves to the new path, the file keeps its id.
        db.delete_node("Tasks/target.md").unwrap();
        db.upsert_node(&node_with_stable_id(
            "Tasks/archived/target.md",
            "uuid-target",
        ))
        .unwrap();

        let after = db.get_linked_nodes("", "Tasks/archived/target.md").unwrap();
        assert_eq!(
            after.len(),
            1,
            "the backlink was lost when the task was archived"
        );
        assert_eq!(after[0].id, "Notes/writer.md");
    }

    /// The same holds for the note doing the linking.
    #[test]
    fn a_backlink_survives_the_linking_note_moving() {
        let db = db();
        db.upsert_node(&node_with_stable_id("Notes/writer.md", "uuid-writer"))
            .unwrap();
        db.upsert_node(&node_with_stable_id("Tasks/target.md", "uuid-target"))
            .unwrap();
        db.upsert_node_edge(&link("uuid-writer", "uuid-target"))
            .unwrap();

        db.delete_node("Notes/writer.md").unwrap();
        db.upsert_node(&node_with_stable_id("Archive/writer.md", "uuid-writer"))
            .unwrap();

        let linked = db.get_linked_nodes("", "Tasks/target.md").unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(
            linked[0].id, "Archive/writer.md",
            "the backlink should now report the note's new home"
        );
    }

    /// A caller may hold either name for a node. Both have to find its links,
    /// because the assistant's tools pass stable ids while the interface passes
    /// paths.
    #[test]
    fn links_are_found_whether_the_caller_names_the_path_or_the_identity() {
        let db = db();
        db.upsert_node(&node_with_stable_id("Notes/writer.md", "uuid-writer"))
            .unwrap();
        db.upsert_node(&node_with_stable_id("Tasks/target.md", "uuid-target"))
            .unwrap();
        db.upsert_node_edge(&link("uuid-writer", "uuid-target"))
            .unwrap();

        assert_eq!(db.get_linked_nodes("", "Tasks/target.md").unwrap().len(), 1);
        assert_eq!(db.get_linked_nodes("", "uuid-target").unwrap().len(), 1);
    }

    /// Files that have never been given an identity still work, keyed by path
    /// exactly as before. A vault mid-upgrade is full of them.
    #[test]
    fn a_node_without_a_stable_identity_still_resolves_by_its_path() {
        let db = db();
        db.upsert_node(&node("Notes/plain.md", "note", json!({})))
            .unwrap();
        db.upsert_node(&node("Notes/other.md", "note", json!({})))
            .unwrap();
        db.upsert_node_edge(&link("Notes/plain.md", "Notes/other.md"))
            .unwrap();

        let linked = db.get_linked_nodes("", "Notes/other.md").unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, "Notes/plain.md");
    }

    /// The backlink join reads a JSON field, which without its index means
    /// reading every node in the vault for every lookup.
    #[test]
    fn the_backlink_join_uses_the_stable_id_index() {
        let db = db();
        // A join plans as several steps; the one that matters is how the nodes
        // side is reached, so read them all rather than only the first.
        let plan: Vec<String> = {
            let mut stmt = db
                .conn()
                .prepare(
                    "EXPLAIN QUERY PLAN SELECT n.id FROM node_edges e \
                     JOIN nodes n ON n.stable_id = e.source_id \
                     WHERE e.target_id = ?1",
                )
                .unwrap();
            let rows = stmt
                .query_map(["x"], |r| r.get::<_, String>(3))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        assert!(
            plan.iter().any(|step| step.contains("idx_nodes_stable_id")),
            "the backlink join stopped using its index:\n  {}",
            plan.join("\n  ")
        );
    }

    #[test]
    fn a_title_can_be_looked_up_without_loading_the_node() {
        let db = db();
        let mut n = node("n.md", "note", json!({}));
        n.title = "Real Title".to_string();
        db.upsert_node(&n).unwrap();

        assert_eq!(db.get_node_title("n.md"), Some("Real Title".to_string()));
        assert_eq!(db.get_node_title("missing.md"), None);
    }
}
