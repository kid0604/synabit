use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;

/// New ID-based edge for the knowledge graph
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String, // 'wikilink' | 'internal_link' | 'embed' | 'manual'
    pub relation: Option<String>, // 'references' | 'attachment' | 'related' | custom...
    pub created_at: String,
}

impl DbBridge {
    pub fn upsert_node_edge(&self, edge: &NodeEdge) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO node_edges (id, source_id, target_id, edge_type, relation, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(source_id, target_id, edge_type) DO UPDATE SET
                relation = COALESCE(excluded.relation, node_edges.relation),
                id = excluded.id",
                params![
                    edge.id,
                    edge.source_id,
                    edge.target_id,
                    edge.edge_type,
                    edge.relation,
                    edge.created_at
                ],
            )
            .map_err(|e| AppError::General(format!("DB Error upserting node_edge: {}", e)))?;
        Ok(())
    }

    pub fn delete_node_edges_by_source(&self, source_id: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM node_edges WHERE source_id = ?1",
                params![source_id],
            )
            .map_err(|e| AppError::General(format!("DB Error deleting node_edges: {}", e)))?;
        Ok(())
    }

    pub fn delete_node_edge(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM node_edges WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Error deleting node_edge: {}", e)))?;
        Ok(())
    }

    /// Get all edges connected to a node (both incoming and outgoing)
    pub fn get_node_edges_for_node(&self, node_id: &str) -> AppResult<Vec<NodeEdge>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_id, target_id, edge_type, relation, created_at
             FROM node_edges
             WHERE source_id = ?1 OR target_id = ?1
             ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Error querying node_edges: {}", e)))?;

        let rows = stmt
            .query_map(params![node_id], |row| {
                Ok(NodeEdge {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    edge_type: row.get(3)?,
                    relation: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Error mapping node_edges: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    pub fn get_all_node_edges(&self) -> AppResult<Vec<NodeEdge>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_id, target_id, edge_type, relation, created_at FROM node_edges",
            )
            .map_err(|e| AppError::General(format!("DB Error querying all node_edges: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(NodeEdge {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    edge_type: row.get(3)?,
                    relation: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Error mapping all node_edges: {}", e)))?;

        Ok(rows.flatten().collect())
    }
}
