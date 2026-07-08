use rusqlite::params;
use crate::error::{AppError, AppResult};
use super::DbBridge;

impl DbBridge {
    /// Get or create a stable device peer ID for CRDT operations.
    /// This ensures all CRDT operations from this device use the same peer ID,
    /// preventing version vector bloat.
    pub fn get_or_create_peer_id(&self) -> AppResult<u64> {
        if let Ok(Some(id_str)) = self.get_kv("device_peer_id") {
            if let Ok(id) = id_str.parse::<u64>() {
                return Ok(id);
            }
        }
        // Generate a new peer ID from UUID
        let id = uuid::Uuid::new_v4().as_u128() as u64;
        self.set_kv("device_peer_id", &id.to_string())?;
        Ok(id)
    }

    pub fn get_crdt_doc(&self, doc_id: &str) -> AppResult<loro::LoroDoc> {
        let mut stmt = self.conn.prepare("SELECT snapshot FROM crdt_documents WHERE doc_id = ?1")
            .map_err(|e| AppError::General(format!("DB Error prepare get_crdt_doc: {}", e)))?;
        let mut rows = stmt.query(params![doc_id])
            .map_err(|e| AppError::General(format!("DB Error querying crdt_documents: {}", e)))?;
        
        let doc = loro::LoroDoc::new();

        // Set stable peer ID to prevent version vector bloat
        if let Ok(peer_id) = self.get_or_create_peer_id() {
            doc.set_peer_id(peer_id).ok();
        }
        
        if let Some(row) = rows.next().unwrap_or(None) {
            let snapshot: Vec<u8> = row.get(0).unwrap_or_default();
            if !snapshot.is_empty() {
                doc.import(&snapshot).map_err(|e| AppError::General(format!("Failed to import Loro snapshot: {:?}", e)))?;
            }
        }
        
        // Load pending deltas
        let mut delta_stmt = self.conn.prepare("SELECT delta FROM crdt_updates WHERE doc_id = ?1 ORDER BY id ASC")
            .map_err(|e| AppError::General(format!("DB Error prepare crdt_updates: {}", e)))?;
        let delta_rows = delta_stmt.query_map(params![doc_id], |row| {
            let delta: Vec<u8> = row.get(0)?;
            Ok(delta)
        }).map_err(|e| AppError::General(format!("DB Error querying crdt_updates: {}", e)))?;
        
        for delta in delta_rows.flatten() {
            if !delta.is_empty() {
                doc.import(&delta).map_err(|e| AppError::General(format!("Failed to import Loro delta: {:?}", e)))?;
            }
        }
        
        Ok(doc)
    }

    pub fn save_crdt_delta(&self, doc_id: &str, delta: Vec<u8>) -> AppResult<()> {
        if delta.is_empty() {
            return Ok(());
        }
        let timestamp = chrono::Utc::now().timestamp_millis();
        self.conn.execute(
            "INSERT INTO crdt_updates (doc_id, delta, timestamp) VALUES (?1, ?2, ?3)",
            params![doc_id, delta, timestamp]
        ).map_err(|e| AppError::General(format!("DB Error saving crdt_delta: {}", e)))?;
        Ok(())
    }

    pub fn save_crdt_snapshot(&self, doc_id: &str, snapshot: Vec<u8>) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO crdt_documents (doc_id, snapshot) VALUES (?1, ?2)
             ON CONFLICT(doc_id) DO UPDATE SET snapshot=excluded.snapshot",
            params![doc_id, snapshot]
        ).map_err(|e| AppError::General(format!("DB Error saving crdt_snapshot: {}", e)))?;
        Ok(())
    }

    pub fn compact_crdt_history(&self, doc_id: &str) -> AppResult<()> {
        let doc = self.get_crdt_doc(doc_id)?;
        let snapshot = doc.export_snapshot();
        self.save_crdt_snapshot(doc_id, snapshot)?;
        
        self.conn.execute(
            "DELETE FROM crdt_updates WHERE doc_id = ?1",
            params![doc_id]
        ).map_err(|e| AppError::General(format!("DB Error compacting crdt_updates: {}", e)))?;
        Ok(())
    }

    pub fn compact_all_crdt(&self) -> AppResult<()> {
        // Only compact documents that have accumulated significant deltas
        let mut stmt = self.conn.prepare(
            "SELECT doc_id, COUNT(*) as cnt FROM crdt_updates GROUP BY doc_id HAVING cnt > 20"
        ).map_err(|e| AppError::General(format!("DB Error getting docs for compaction: {}", e)))?;
        let rows = stmt.query_map([], |row| {
            let doc_id: String = row.get(0)?;
            Ok(doc_id)
        }).map_err(|e| AppError::General(format!("DB Map error in compaction: {}", e)))?;
        
        for doc_id in rows.flatten() {
            let _ = self.compact_crdt_history(&doc_id);
        }
        Ok(())
    }

    /// Delete all CRDT data for a document (snapshot + deltas).
    /// Called when a node file is deleted.
    pub fn delete_crdt_doc(&self, doc_id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM crdt_documents WHERE doc_id = ?1", params![doc_id])
            .map_err(|e| AppError::General(format!("DB Error deleting crdt_documents: {}", e)))?;
        self.conn
            .execute("DELETE FROM crdt_updates WHERE doc_id = ?1", params![doc_id])
            .map_err(|e| AppError::General(format!("DB Error deleting crdt_updates: {}", e)))?;
        Ok(())
    }

    /// Identity Mapping: Get `node_id` by `rel_path`
    pub fn get_node_id_by_path(&self, rel_path: &str) -> AppResult<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT doc_id FROM document_paths WHERE rel_path = ?1")
            .map_err(|e| AppError::General(format!("DB Error prepare get_node_id_by_path: {}", e)))?;
        let mut rows = stmt.query(params![rel_path])
            .map_err(|e| AppError::General(format!("DB Error querying document_paths: {}", e)))?;
        
        if let Some(row) = rows.next().unwrap_or(None) {
            let doc_id: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            Ok(Some(doc_id))
        } else {
            Ok(None)
        }
    }

    /// Identity Mapping: Get `rel_path` by `node_id`
    pub fn get_path_by_node_id(&self, doc_id: &str) -> AppResult<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT rel_path FROM document_paths WHERE doc_id = ?1")
            .map_err(|e| AppError::General(format!("DB Error prepare get_path_by_node_id: {}", e)))?;
        let mut rows = stmt.query(params![doc_id])
            .map_err(|e| AppError::General(format!("DB Error querying document_paths: {}", e)))?;
        
        if let Some(row) = rows.next().unwrap_or(None) {
            let rel_path: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            Ok(Some(rel_path))
        } else {
            Ok(None)
        }
    }

    /// Identity Mapping: Get all known document paths
    pub fn get_all_document_paths(&self) -> AppResult<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT doc_id, rel_path FROM document_paths")
            .map_err(|e| AppError::General(format!("DB Error prepare get_all_document_paths: {}", e)))?;
        
        let rows = stmt.query_map([], |row| {
            let doc_id: String = row.get(0)?;
            let rel_path: String = row.get(1)?;
            Ok((doc_id, rel_path))
        }).map_err(|e| AppError::General(format!("DB Error querying document_paths: {}", e)))?;

        let mut paths = Vec::new();
        for row in rows {
            if let Ok(path) = row {
                paths.push(path);
            }
        }
        Ok(paths)
    }

    /// Identity Mapping: Upsert a document path mapping
    pub fn upsert_document_path(&self, doc_id: &str, rel_path: &str) -> AppResult<()> {
        let timestamp = chrono::Utc::now().timestamp_millis();
        self.conn.execute(
            "INSERT INTO document_paths (doc_id, rel_path, path_updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(doc_id) DO UPDATE SET rel_path=excluded.rel_path, path_updated_at=excluded.path_updated_at",
            params![doc_id, rel_path, timestamp]
        ).map_err(|e| AppError::General(format!("DB Error upserting document_path: {}", e)))?;
        Ok(())
    }

    /// Identity Mapping: Delete a mapping
    pub fn delete_document_path(&self, doc_id: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM document_paths WHERE doc_id = ?1", params![doc_id])
            .map_err(|e| AppError::General(format!("DB Error deleting document_path: {}", e)))?;
        Ok(())
    }
}
