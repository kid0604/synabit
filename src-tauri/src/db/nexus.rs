use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;

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

/// How an item reads in the feed before you open it.
///
/// Most nodes preview as the opening of their text. A whiteboard has no opening
/// line — its content is the loose collection of labels scattered across it —
/// so it says how big it is instead.
fn nexus_preview(node_type: &str, content: &str, properties: &serde_json::Value) -> String {
    if node_type == "whiteboard" {
        let count = properties
            .get("node_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        return format!("Whiteboard • {} nodes", count);
    }
    content.chars().take(150).collect()
}

/// The tags a node carries, however its properties happen to be shaped.
fn tags_of(properties: &serde_json::Value) -> Vec<String> {
    properties
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl DbBridge {
    pub fn get_all_nexus_items(&self) -> AppResult<Vec<NexusRow>> {
        let mut items = Vec::new();

        // Everything the vault holds — files, notes, boards alike — lives in the
        // `nodes` table and is read here in one pass. Whiteboards used to be
        // fetched separately from a table of their own, which listed every board
        // twice: once from there and once from `nodes`, where the vault scan had
        // also indexed the same file under the same id.

        // Nodes (Universal Architecture)
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE node_type NOT LIKE 'finance_%'"
        ).map_err(|e| AppError::General(format!("DB Nexus Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                let content: String = row.get(3)?;
                let node_type: String = row.get(1)?;
                Ok(NexusRow {
                    id: row.get(0)?,
                    preview: nexus_preview(&node_type, &content, &properties),
                    tags: tags_of(&properties),
                    item_type: node_type,
                    title: row.get(2)?,
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
        // One table, one lookup. There is no longer a separate place a
        // whiteboard could be hiding, so there is no longer a prefix check on
        // the id to decide which place to look in.
        {
            let mut stmt = self.conn
                .prepare("SELECT id, node_type, title, content, properties, updated_at FROM nodes WHERE id = ?1 AND node_type NOT LIKE 'finance_%'")
                .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
            let mut rows = stmt
                .query_map(params![id], |row| {
                    let props_str: String = row.get(4)?;
                    let properties: serde_json::Value =
                        serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                    let content: String = row.get(3)?;
                    let node_type: String = row.get(1)?;
                    Ok(NexusRow {
                        id: row.get(0)?,
                        preview: nexus_preview(&node_type, &content, &properties),
                        tags: tags_of(&properties),
                        item_type: node_type,
                        title: row.get(2)?,
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

/// Tests for the unified feed Nexus reads from.
#[cfg(test)]
mod tests {
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    /// A whiteboard file produces a row in `nodes` — the vault scan indexes it
    /// like any other file — and used to produce a second row in a table of its
    /// own, written by the whiteboard commands. Both were keyed by the same
    /// relative path, so Nexus showed the same board twice, and the two copies
    /// did not even agree about what a board's content is.
    ///
    /// The board is written twice here on purpose. There is one table to write
    /// it to now, so the second write lands on the first, which is exactly the
    /// property that was missing.
    #[test]
    fn a_whiteboard_appears_in_the_nexus_feed_exactly_once() {
        let db = db();
        let id = "Whiteboards/plan.whiteboard.json";

        let board = NodeMetadata {
            id: id.to_string(),
            node_type: "whiteboard".to_string(),
            title: "Plan".to_string(),
            content: "Kickoff Design Ship".to_string(),
            properties: serde_json::json!({"node_count": 3}),
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            timestamp: 0,
            blocks: None,
        };
        db.upsert_node(&board).unwrap();
        db.upsert_node(&board).unwrap();

        let items = db.get_all_nexus_items().unwrap();
        let matching: Vec<_> = items.iter().filter(|i| i.id == id).collect();

        assert_eq!(
            matching.len(),
            1,
            "the same whiteboard was listed {} times",
            matching.len()
        );
        assert_eq!(matching[0].item_type, "whiteboard");
    }

    /// What the user reads in the feed. The preview must describe the board,
    /// not expose the file that stores it.
    #[test]
    fn a_whiteboards_preview_describes_the_board_rather_than_its_json() {
        let db = db();
        let id = "Whiteboards/plan.whiteboard.json";
        db.upsert_node(&NodeMetadata {
            id: id.to_string(),
            node_type: "whiteboard".to_string(),
            title: "Plan".to_string(),
            content: "Kickoff Design Ship".to_string(),
            properties: serde_json::json!({"node_count": 3}),
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            timestamp: 0,
            blocks: None,
        })
        .unwrap();

        let items = db.get_all_nexus_items().unwrap();
        let board = items.iter().find(|i| i.id == id).expect("board is listed");

        assert!(
            !board.preview.contains('{') && !board.preview.contains("\"nodes\""),
            "the preview is showing raw JSON: {}",
            board.preview
        );
        assert!(
            board.preview.contains('3'),
            "the preview should say how big the board is: {}",
            board.preview
        );
        assert!(
            board.content.contains("Kickoff"),
            "the board's text must stay searchable"
        );
    }
}
