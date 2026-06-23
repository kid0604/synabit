use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;

/// Lightweight row struct for Nexus unified queries
#[derive(Debug)]
pub struct NexusRow {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub preview: String,
    pub tags: Vec<String>,
    pub date: String,
    pub path: String,
    pub content: String,
    pub status: Option<String>,
}

impl DbBridge {
    pub fn get_all_nexus_items(&self) -> AppResult<Vec<NexusRow>> {
        let mut items = Vec::new();

        // Note: Files are now stored in the `nodes` table (Universal Architecture)
        // and are queried below along with other node types.

        // Whiteboards
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, tags, content, path, created_at, updated_at FROM whiteboards",
            )
            .map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                let tags_str: String = row.get(2)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                let content_json: String = row.get(3)?;
                let node_count = content_json.matches("\"id\"").count();
                let preview = format!("Whiteboard • {} nodes", node_count);
                // Extract text labels from nodes for searchable content
                let node_texts: String =
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content_json) {
                        parsed
                            .get("nodes")
                            .and_then(|n| n.as_array())
                            .map(|nodes| {
                                nodes
                                    .iter()
                                    .filter_map(|n| {
                                        n.get("data")
                                            .and_then(|d| d.get("label").and_then(|l| l.as_str()))
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" • ")
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                Ok(NexusRow {
                    id: row.get(0)?,
                    item_type: "whiteboard".to_string(),
                    title: row.get(1)?,
                    preview,
                    tags,
                    date: row.get(6)?,
                    path: row.get(4)?,
                    content: node_texts,
                    status: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() {
            items.push(r);
        }

        // Nodes (Universal Architecture)
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE node_type NOT LIKE 'finance_%'"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(4)?;
                let mut tags = Vec::new();
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&props_str) {
                    if let Some(t) = json_val.get("tags").and_then(|v| v.as_array()) {
                        tags = t
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                }
                let content: String = row.get(3)?;
                let preview: String = content.chars().take(150).collect();
                let node_type: String = row.get(1)?;
                Ok(NexusRow {
                    id: row.get(0)?,
                    item_type: node_type,
                    title: row.get(2)?,
                    preview,
                    tags,
                    date: row.get(5)?,
                    path: row.get(0)?,
                    content,
                    status: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Nexus Map Error: {}", e)))?;
        for r in rows.flatten() {
            items.push(r);
        }

        Ok(items)
    }

    /// Fast single-item lookup: determines the correct table from the ID prefix
    /// and runs a targeted `WHERE id = ?` query instead of scanning all tables.
    pub fn get_nexus_item_by_id(&self, id: &str) -> AppResult<Option<NexusRow>> {
        // Whiteboards
        if id.starts_with("Whiteboards/") || id.starts_with("whiteboard-") {
            let mut stmt = self.conn
                .prepare("SELECT id, title, tags, content, path, created_at, updated_at FROM whiteboards WHERE id = ?1")
                .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
            let mut rows = stmt
                .query_map(params![id], |row| {
                    let tags_str: String = row.get(2)?;
                    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
                    let content_json: String = row.get(3)?;
                    let node_count = content_json.matches("\"id\"").count();
                    let preview = format!("Whiteboard • {} nodes", node_count);
                    let node_texts: String = if let Ok(parsed) =
                        serde_json::from_str::<serde_json::Value>(&content_json)
                    {
                        parsed
                            .get("nodes")
                            .and_then(|n| n.as_array())
                            .map(|nodes| {
                                nodes
                                    .iter()
                                    .filter_map(|n| {
                                        n.get("data")
                                            .and_then(|d| d.get("label").and_then(|l| l.as_str()))
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" • ")
                            })
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };
                    Ok(NexusRow {
                        id: row.get(0)?,
                        item_type: "whiteboard".to_string(),
                        title: row.get(1)?,
                        preview,
                        tags,
                        date: row.get(6)?,
                        path: row.get(4)?,
                        content: node_texts,
                        status: None,
                    })
                })
                .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
            return Ok(rows.next().and_then(|r| r.ok()));
        }
        // Universal Nodes table
        {
            let mut stmt = self.conn
                .prepare("SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE id = ?1 AND node_type NOT LIKE 'finance_%'")
                .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
            let mut rows = stmt
                .query_map(params![id], |row| {
                    let props_str: String = row.get(4)?;
                    let mut tags = Vec::new();
                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&props_str) {
                        if let Some(t) = json_val.get("tags").and_then(|v| v.as_array()) {
                            tags = t
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    let content: String = row.get(3)?;
                    let preview: String = content.chars().take(150).collect();
                    let node_type: String = row.get(1)?;
                    Ok(NexusRow {
                        id: row.get(0)?,
                        item_type: node_type,
                        title: row.get(2)?,
                        preview,
                        tags,
                        date: row.get(5)?,
                        path: row.get(0)?,
                        content,
                        status: None,
                    })
                })
                .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;
            if let Some(Ok(row)) = rows.next() {
                return Ok(Some(row));
            }
        }

        Ok(None)
    }
}
