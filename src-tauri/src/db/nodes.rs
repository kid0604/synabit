use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;

impl DbBridge {
    pub fn upsert_node(&self, node: &crate::models::node::NodeMetadata) -> AppResult<()> {
        let properties_json = serde_json::to_string(&node.properties)?;
        self.conn.execute(
            "INSERT INTO nodes (id, node_type, title, content, properties, created_at, updated_at, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                node_type=excluded.node_type,
                title=excluded.title,
                content=excluded.content,
                properties=excluded.properties,
                updated_at=excluded.updated_at,
                timestamp=excluded.timestamp",
            params![node.id, node.node_type, node.title, node.content, properties_json, node.created_at, node.updated_at, node.timestamp],
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

    pub fn get_active_tasks_and_events(
        &self,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
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
            .map_err(|e| AppError::General(format!("DB Map Error (get_active_tasks_and_events): {}", e)))?;

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

        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.node_type, n.title, n.content, n.properties, n.created_at, n.updated_at, n.timestamp 
             FROM nodes n 
             JOIN node_edges e ON n.id = e.source_id 
             WHERE e.target_id = ?1
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
        let mut stmt = self.conn.prepare(
            "SELECT json_each.value, COUNT(*) 
             FROM nodes, json_each(nodes.properties, '$.tags') 
             GROUP BY json_each.value 
             ORDER BY COUNT(*) DESC, json_each.value ASC"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_all_tags): {}", e)))?;

        let rows = stmt.query_map([], |row| {
            let tag: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((tag, count))
        }).map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for row in rows.flatten() {
            results.push(row);
        }
        Ok(results)
    }

    pub fn get_nodes_by_tag(&self, target_tag: &str) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
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
