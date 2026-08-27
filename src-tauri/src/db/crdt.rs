use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;

/// Milliseconds of quiet before consecutive local edits become separate
/// versions.
///
/// Milliseconds because that is what Loro compares against, and the crate is
/// not consistent about saying so: `Change::timestamp` is documented as
/// seconds, while `get_sys_timestamp` — the function that fills it in — returns
/// `as_millis()`. The unit the code uses is the one that counts, and reading
/// the field doc instead cost an interval a thousand times shorter than
/// intended, which would have made a version out of nearly every autosave.
///
/// Read by `commands::versions` too, which describes the list this produces.
pub const VERSION_MERGE_INTERVAL_MS: i64 = 5 * 60 * 1000;

impl DbBridge {
    /// Get or create a stable device peer ID for CRDT operations.
    pub fn get_or_create_peer_id(&self) -> AppResult<u64> {
        match self.get_kv("device_peer_id")? {
            Some(id_str) => match id_str.parse::<u64>() {
                Ok(id) => Ok(id),
                Err(e) => Err(AppError::General(format!(
                    "Failed to parse stored device_peer_id '{}': {}",
                    id_str, e
                ))),
            },
            None => {
                let id = uuid::Uuid::new_v4().as_u128() as u64;
                self.set_kv("device_peer_id", &id.to_string())?;
                Ok(id)
            }
        }
    }

    /// The document behind a path, rebuilt from its snapshot and update log.
    pub fn get_crdt_doc(&self, vault_id: &str, doc_id: &str) -> AppResult<loro::LoroDoc> {
        self.get_crdt_doc_grouped(vault_id, doc_id, VERSION_MERGE_INTERVAL_MS)
    }

    /// The same document, with the grouping of its history spelled out.
    ///
    /// Loro folds consecutive local changes into one when they are less than
    /// `merge_interval_ms` apart, and it does that **as the log is replayed**
    /// — so how many versions a note appears to have is decided here, on read,
    /// not when the edits were made. The operations underneath are untouched
    /// either way; only their grouping moves.
    ///
    /// One exception, and it is worth knowing: `compact_crdt_history` exports
    /// a snapshot of a document loaded this way, which writes the grouping
    /// into the snapshot. Whatever was folded before a compaction stays
    /// folded.
    pub fn get_crdt_doc_grouped(
        &self,
        vault_id: &str,
        doc_id: &str,
        merge_interval_ms: i64,
    ) -> AppResult<loro::LoroDoc> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        let doc = loro::LoroDoc::new();
        let peer_id = self.get_or_create_peer_id()?;
        doc.set_peer_id(peer_id)
            .map_err(|e| AppError::General(format!("Failed to set Loro peer_id: {:?}", e)))?;

        // Loro does not record when a change happened unless asked, and the
        // whole operation log was being kept without a single timestamp in it.
        // The history was there and unreadable: every version of every note
        // claimed to have been written at the start of 1970.
        //
        // This only fixes it going forward. Changes already in the log have no
        // time in them and never will, so `list_node_versions` reports those as
        // undated rather than inventing one.
        doc.set_record_timestamp(true);

        // How far apart two local edits must be before they count as separate
        // versions. This has to be set before the import below, which is where
        // the folding happens.
        //
        // Loro's default is a little under seventeen minutes, long enough that
        // a morning's writing would arrive as one or two entries. Five minutes
        // is the span a person recognises as a sitting, and is where comparable
        // history features land.
        doc.set_change_merge_interval(merge_interval_ms);

        let mut stmt = self
            .conn
            .prepare("SELECT snapshot FROM sync_crdt_documents WHERE vault_id = ?1 AND doc_id = ?2")
            .map_err(|e| AppError::General(format!("DB Error prepare get_crdt_doc: {}", e)))?;

        let mut rows = stmt.query(params![vault_id, doc_id]).map_err(|e| {
            AppError::General(format!("DB Error querying sync_crdt_documents: {}", e))
        })?;

        if let Some(row) = rows.next().map_err(|e| AppError::General(e.to_string()))? {
            let snapshot: Vec<u8> = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            if snapshot.is_empty() {
                return Err(AppError::General("Corrupt snapshot: empty bytes".into()));
            }
            doc.import(&snapshot).map_err(|e| {
                AppError::General(format!("Failed to import Loro snapshot: {:?}", e))
            })?;
        }

        let mut delta_stmt = self
            .conn
            .prepare("SELECT delta FROM sync_crdt_updates WHERE vault_id = ?1 AND doc_id = ?2 ORDER BY update_id ASC")
            .map_err(|e| AppError::General(format!("DB Error prepare sync_crdt_updates: {}", e)))?;

        let mut delta_rows = delta_stmt.query(params![vault_id, doc_id]).map_err(|e| {
            AppError::General(format!("DB Error querying sync_crdt_updates: {}", e))
        })?;

        while let Some(row) = delta_rows
            .next()
            .map_err(|e| AppError::General(e.to_string()))?
        {
            let delta: Vec<u8> = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            if delta.is_empty() {
                return Err(AppError::General("Corrupt delta: empty bytes".into()));
            }
            doc.import(&delta)
                .map_err(|e| AppError::General(format!("Failed to import Loro delta: {:?}", e)))?;
        }

        Ok(doc)
    }

    pub fn save_crdt_delta(&self, vault_id: &str, doc_id: &str, delta: Vec<u8>) -> AppResult<()> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        if delta.is_empty() {
            return Ok(());
        }
        let timestamp = chrono::Utc::now().timestamp_millis();
        self.conn
            .execute(
                "INSERT INTO sync_crdt_updates (vault_id, doc_id, update_id, delta, timestamp) VALUES (?1, ?2, (SELECT COALESCE(MAX(update_id), 0) + 1 FROM sync_crdt_updates WHERE vault_id = ?1), ?3, ?4)",
                params![vault_id, doc_id, delta, timestamp],
            )
            .map_err(|e| AppError::General(format!("DB Error saving crdt_delta: {}", e)))?;
        Ok(())
    }

    pub fn save_crdt_snapshot(
        &self,
        vault_id: &str,
        doc_id: &str,
        snapshot: Vec<u8>,
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        let updated_at = chrono::Utc::now().timestamp_millis();
        self.conn
            .execute(
                "INSERT INTO sync_crdt_documents (vault_id, doc_id, snapshot, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(vault_id, doc_id) DO UPDATE SET snapshot=excluded.snapshot, updated_at=excluded.updated_at",
                params![vault_id, doc_id, snapshot, updated_at],
            )
            .map_err(|e| AppError::General(format!("DB Error saving crdt_snapshot: {}", e)))?;
        Ok(())
    }

    pub fn replace_crdt_snapshot(
        &self,
        vault_id: &str,
        doc_id: &str,
        snapshot: &[u8],
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        if snapshot.is_empty() {
            return Err(AppError::General("Corrupt snapshot: empty bytes".into()));
        }

        // Validate snapshot bytes with fresh LoroDoc before opening transaction or mutating tables
        let check_doc = loro::LoroDoc::new();
        check_doc
            .import(snapshot)
            .map_err(|e| AppError::General(format!("Invalid Loro snapshot bytes: {:?}", e)))?;

        let updated_at = chrono::Utc::now().timestamp_millis();
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| AppError::General(format!("DB Error starting transaction: {}", e)))?;

        tx.execute(
            "INSERT INTO sync_crdt_documents (vault_id, doc_id, snapshot, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(vault_id, doc_id) DO UPDATE SET snapshot=excluded.snapshot, updated_at=excluded.updated_at",
            params![vault_id, doc_id, snapshot, updated_at],
        )
        .map_err(|e| AppError::General(format!("DB Error saving snapshot in replace_crdt_snapshot: {}", e)))?;

        tx.execute(
            "DELETE FROM sync_crdt_updates WHERE vault_id = ?1 AND doc_id = ?2",
            params![vault_id, doc_id],
        )
        .map_err(|e| {
            AppError::General(format!(
                "DB Error deleting updates in replace_crdt_snapshot: {}",
                e
            ))
        })?;

        tx.commit().map_err(|e| {
            AppError::General(format!("DB Error committing replace_crdt_snapshot: {}", e))
        })?;
        Ok(())
    }

    /// Collapse a document's update log into a single snapshot.
    ///
    /// Loaded with no grouping at all, which is deliberate. A snapshot records
    /// the changes as they stand in the document it was exported from, so
    /// whatever this fold together is folded for good — and a note that had
    /// been compacted would show one version for the whole month while an
    /// identical note that had not showed one per sitting. Keeping the finest
    /// grouping on disk leaves how many versions to *show* a question the
    /// reader's own load answers, every time, the same way.
    pub fn compact_crdt_history(&mut self, vault_id: &str, doc_id: &str) -> AppResult<()> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        let doc = self.get_crdt_doc_grouped(vault_id, doc_id, 0)?;
        let snapshot = doc.export_snapshot();
        self.replace_crdt_snapshot(vault_id, doc_id, &snapshot)?;
        Ok(())
    }

    pub fn compact_all_crdt(&mut self, vault_id: &str) -> AppResult<()> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        // Compact on accumulated size as well as row count. Counting rows alone
        // missed the case that actually matters: a document holding one very
        // large update never reached the threshold and was never collapsed, so
        // it sat in the update log forever. Vaults were found carrying half a
        // megabyte per document this way.
        const COMPACT_AFTER_UPDATES: i64 = 20;
        const COMPACT_AFTER_BYTES: i64 = 256 * 1024;

        let doc_ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT doc_id FROM sync_crdt_updates WHERE vault_id = ?1
                     GROUP BY doc_id
                     HAVING COUNT(*) > ?2 OR SUM(LENGTH(delta)) > ?3",
                )
                .map_err(|e| {
                    AppError::General(format!("DB Error getting docs for compaction: {}", e))
                })?;
            let mut rows = stmt
                .query(params![
                    vault_id,
                    COMPACT_AFTER_UPDATES,
                    COMPACT_AFTER_BYTES
                ])
                .map_err(|e| AppError::General(e.to_string()))?;
            let mut list = Vec::new();
            while let Some(row) = rows.next().map_err(|e| AppError::General(e.to_string()))? {
                let id: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
                list.push(id);
            }
            list
        };

        // One document that cannot be rebuilt must not stop the others being
        // compacted — a single unreadable snapshot used to leave every other
        // document's history growing unchecked.
        for doc_id in doc_ids {
            if let Err(e) = self.compact_crdt_history(vault_id, &doc_id) {
                log::warn!("could not compact CRDT history for {}: {}", doc_id, e);
            }
        }
        Ok(())
    }

    pub fn export_snapshots(&self, vault_id: &str) -> AppResult<Vec<(String, Vec<u8>)>> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        let doc_ids: Vec<String> = {
            let mut stmt = self
                .conn
                .prepare(
                    "SELECT doc_id FROM sync_crdt_documents WHERE vault_id = ?1 UNION SELECT doc_id FROM sync_crdt_updates WHERE vault_id = ?1 ORDER BY doc_id ASC",
                )
                .map_err(|e| AppError::General(e.to_string()))?;
            let mut rows = stmt
                .query(params![vault_id])
                .map_err(|e| AppError::General(e.to_string()))?;
            let mut list = Vec::new();
            while let Some(row) = rows.next().map_err(|e| AppError::General(e.to_string()))? {
                let id: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
                list.push(id);
            }
            list
        };

        let mut res = Vec::new();
        for doc_id in doc_ids {
            let doc = self.get_crdt_doc(vault_id, &doc_id)?;
            let snapshot = doc.export_snapshot();
            res.push((doc_id, snapshot));
        }
        Ok(res)
    }

    pub fn delete_crdt_doc(&self, vault_id: &str, doc_id: &str) -> AppResult<()> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| AppError::General(format!("DB Error starting transaction: {}", e)))?;
        tx.execute(
            "DELETE FROM sync_crdt_documents WHERE vault_id = ?1 AND doc_id = ?2",
            params![vault_id, doc_id],
        )
        .map_err(|e| AppError::General(format!("DB Error deleting sync_crdt_documents: {}", e)))?;
        tx.execute(
            "DELETE FROM sync_crdt_updates WHERE vault_id = ?1 AND doc_id = ?2",
            params![vault_id, doc_id],
        )
        .map_err(|e| AppError::General(format!("DB Error deleting sync_crdt_updates: {}", e)))?;
        tx.commit().map_err(|e| {
            AppError::General(format!("DB Error committing delete_crdt_doc: {}", e))
        })?;
        Ok(())
    }

    pub fn get_node_id_by_path(&self, vault_id: &str, rel_path: &str) -> AppResult<Option<String>> {
        if vault_id.trim().is_empty() || rel_path.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and rel_path cannot be empty".into(),
            ));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT doc_id FROM sync_document_paths WHERE vault_id = ?1 AND rel_path = ?2")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mut rows = stmt
            .query(params![vault_id, rel_path])
            .map_err(|e| AppError::General(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| AppError::General(e.to_string()))? {
            let doc_id: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            Ok(Some(doc_id))
        } else {
            Ok(None)
        }
    }

    pub fn get_path_by_node_id(&self, vault_id: &str, doc_id: &str) -> AppResult<Option<String>> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT rel_path FROM sync_document_paths WHERE vault_id = ?1 AND doc_id = ?2")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mut rows = stmt
            .query(params![vault_id, doc_id])
            .map_err(|e| AppError::General(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| AppError::General(e.to_string()))? {
            let rel_path: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            Ok(Some(rel_path))
        } else {
            Ok(None)
        }
    }

    pub fn get_document_paths(&self, vault_id: &str) -> AppResult<Vec<(String, String)>> {
        if vault_id.trim().is_empty() {
            return Err(AppError::General("vault_id cannot be empty".into()));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT doc_id, rel_path FROM sync_document_paths WHERE vault_id = ?1 ORDER BY doc_id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mut rows = stmt
            .query(params![vault_id])
            .map_err(|e| AppError::General(e.to_string()))?;
        let mut list = Vec::new();
        while let Some(row) = rows.next().map_err(|e| AppError::General(e.to_string()))? {
            let doc_id: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            let rel_path: String = row.get(1).map_err(|e| AppError::General(e.to_string()))?;
            list.push((doc_id, rel_path));
        }
        Ok(list)
    }

    pub fn upsert_document_path(
        &self,
        vault_id: &str,
        doc_id: &str,
        rel_path: &str,
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() || rel_path.trim().is_empty() {
            return Err(AppError::General(
                "vault_id, doc_id and rel_path cannot be empty".into(),
            ));
        }
        let updated_at = chrono::Utc::now().timestamp_millis();
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| AppError::General(format!("DB tx error: {}", e)))?;

        // A path belongs to exactly one document. If some other document was
        // recorded at this path, it no longer is: the file there now identifies
        // itself as this one. That happens whenever a file's identity is
        // rewritten outside the app — an external editor stripping frontmatter,
        // a checkout, a restore from backup — and used to abort the whole sync
        // with a UNIQUE constraint error instead of simply re-pointing.
        tx.execute(
            "DELETE FROM sync_document_paths
             WHERE vault_id = ?1 AND rel_path = ?2 AND doc_id <> ?3",
            params![vault_id, rel_path, doc_id],
        )
        .map_err(|e| AppError::General(format!("DB Error clearing stale path claim: {}", e)))?;

        tx.execute(
            "INSERT INTO sync_document_paths (vault_id, doc_id, rel_path, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(vault_id, doc_id) DO UPDATE SET rel_path=excluded.rel_path, updated_at=excluded.updated_at",
            params![vault_id, doc_id, rel_path, updated_at],
        )
        .map_err(|e| AppError::General(format!("DB Error upserting sync_document_path: {}", e)))?;

        tx.commit()
            .map_err(|e| AppError::General(format!("DB commit error: {}", e)))?;
        Ok(())
    }

    pub fn delete_document_path(&self, vault_id: &str, doc_id: &str) -> AppResult<()> {
        if vault_id.trim().is_empty() || doc_id.trim().is_empty() {
            return Err(AppError::General(
                "vault_id and doc_id cannot be empty".into(),
            ));
        }
        self.conn
            .execute(
                "DELETE FROM sync_document_paths WHERE vault_id = ?1 AND doc_id = ?2",
                params![vault_id, doc_id],
            )
            .map_err(|e| {
                AppError::General(format!("DB Error deleting sync_document_path: {}", e))
            })?;
        Ok(())
    }

    pub fn get_document_baseline(
        &self,
        vault_id: &str,
        provider_id: &str,
        rel_path: &str,
    ) -> AppResult<Option<String>> {
        if vault_id.trim().is_empty() || provider_id.trim().is_empty() || rel_path.trim().is_empty()
        {
            return Err(AppError::General(
                "vault_id, provider_id and rel_path cannot be empty".into(),
            ));
        }
        let mut stmt = self
            .conn
            .prepare("SELECT content_hash FROM sync_document_baselines WHERE vault_id = ?1 AND provider_id = ?2 AND rel_path = ?3")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mut rows = stmt
            .query(params![vault_id, provider_id, rel_path])
            .map_err(|e| AppError::General(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| AppError::General(e.to_string()))? {
            let hash: String = row.get(0).map_err(|e| AppError::General(e.to_string()))?;
            Ok(Some(hash))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_document_baseline(
        &self,
        vault_id: &str,
        provider_id: &str,
        rel_path: &str,
        content_hash: &str,
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() || provider_id.trim().is_empty() || rel_path.trim().is_empty()
        {
            return Err(AppError::General(
                "vault_id, provider_id and rel_path cannot be empty".into(),
            ));
        }
        self.ensure_sync_provider_state(vault_id, provider_id)?;
        let updated_at = chrono::Utc::now().timestamp_millis();
        self.conn
            .execute(
                "INSERT INTO sync_document_baselines (vault_id, provider_id, rel_path, content_hash, updated_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(vault_id, provider_id, rel_path) DO UPDATE SET content_hash=excluded.content_hash, updated_at=excluded.updated_at",
                params![vault_id, provider_id, rel_path, content_hash, updated_at],
            )
            .map_err(|e| AppError::General(format!("DB Error upserting sync_document_baseline: {}", e)))?;
        Ok(())
    }

    pub fn delete_document_baseline(
        &self,
        vault_id: &str,
        provider_id: &str,
        rel_path: &str,
    ) -> AppResult<()> {
        if vault_id.trim().is_empty() || provider_id.trim().is_empty() || rel_path.trim().is_empty()
        {
            return Err(AppError::General(
                "vault_id, provider_id and rel_path cannot be empty".into(),
            ));
        }
        self.conn
            .execute(
                "DELETE FROM sync_document_baselines WHERE vault_id = ?1 AND provider_id = ?2 AND rel_path = ?3",
                params![vault_id, provider_id, rel_path],
            )
            .map_err(|e| AppError::General(format!("DB Error deleting sync_document_baseline: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_vault_row(db: &DbBridge, vault_id: &str, canonical_root: &str) {
        db.conn
            .execute(
                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES (?1, ?2, 100, 100)",
                params![vault_id, canonical_root],
            )
            .unwrap();
    }

    #[test]
    fn delete_crdt_doc_rolls_back_on_second_statement_failure() {
        let db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        seed_vault_row(&db, vault_a, "/tmp/v_a");

        let doc = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc, "hello").unwrap();
        let snap = doc.export_snapshot();

        let doc2 = loro::LoroDoc::new();
        doc2.import(&snap).unwrap();
        let delta = crate::sync::core::crdt::apply_text_update(&doc2, " world").unwrap();

        db.save_crdt_snapshot(vault_a, "doc1", snap).unwrap();
        db.save_crdt_delta(vault_a, "doc1", delta).unwrap();

        let before_doc: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_updates: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        db.conn
            .execute_batch(
                "CREATE TRIGGER fail_second_delete BEFORE DELETE ON sync_crdt_updates BEGIN SELECT RAISE(FAIL, 'delete_failed'); END;",
            )
            .unwrap();

        let res = db.delete_crdt_doc(vault_a, "doc1");
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("delete_failed"));

        let after_doc: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_updates: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_doc, after_doc);
        assert_eq!(before_updates, after_updates);
    }

    #[test]
    fn vault_scoped_crdt_path_and_baseline_crud_isolated() {
        let db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        let vault_b = "vault_b";
        seed_vault_row(&db, vault_a, "/tmp/v_a");
        seed_vault_row(&db, vault_b, "/tmp/v_b");

        let same_doc = "same_doc";
        let same_path = "same_path";

        let doc1 = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc1, "content_a").unwrap();
        let snapshot_a = doc1.export_snapshot();

        let doc2 = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc2, "content_b").unwrap();
        let snapshot_b = doc2.export_snapshot();

        db.save_crdt_snapshot(vault_a, same_doc, snapshot_a)
            .unwrap();
        db.save_crdt_snapshot(vault_b, same_doc, snapshot_b)
            .unwrap();

        let doc_a_mod = loro::LoroDoc::new();
        doc_a_mod
            .import(
                &db.get_crdt_doc(vault_a, same_doc)
                    .unwrap()
                    .export_snapshot(),
            )
            .unwrap();
        let delta_a =
            crate::sync::core::crdt::apply_text_update(&doc_a_mod, "content_a append_a").unwrap();

        let doc_b_mod = loro::LoroDoc::new();
        doc_b_mod
            .import(
                &db.get_crdt_doc(vault_b, same_doc)
                    .unwrap()
                    .export_snapshot(),
            )
            .unwrap();
        let delta_b =
            crate::sync::core::crdt::apply_text_update(&doc_b_mod, "content_b append_b").unwrap();

        db.save_crdt_delta(vault_a, same_doc, delta_a).unwrap();
        db.save_crdt_delta(vault_b, same_doc, delta_b).unwrap();

        db.upsert_document_path(vault_a, same_doc, same_path)
            .unwrap();
        db.upsert_document_path(vault_b, same_doc, same_path)
            .unwrap();

        let baseline_a = "hash_a_baseline";
        let baseline_b = "hash_b_baseline";
        db.upsert_document_baseline(vault_a, "gdrive", same_path, baseline_a)
            .unwrap();
        db.upsert_document_baseline(vault_b, "gdrive", same_path, baseline_b)
            .unwrap();

        let doc_a_read = db.get_crdt_doc(vault_a, same_doc).unwrap();
        let doc_b_read = db.get_crdt_doc(vault_b, same_doc).unwrap();

        assert_eq!(
            doc_a_read.get_text("content").to_string(),
            "content_a append_a"
        );
        assert_eq!(
            doc_b_read.get_text("content").to_string(),
            "content_b append_b"
        );

        assert_eq!(
            db.get_path_by_node_id(vault_a, same_doc).unwrap(),
            Some(same_path.to_string())
        );
        assert_eq!(
            db.get_path_by_node_id(vault_b, same_doc).unwrap(),
            Some(same_path.to_string())
        );

        assert_eq!(
            db.get_document_baseline(vault_a, "gdrive", same_path)
                .unwrap(),
            Some(baseline_a.to_string())
        );
        assert_eq!(
            db.get_document_baseline(vault_b, "gdrive", same_path)
                .unwrap(),
            Some(baseline_b.to_string())
        );
    }

    #[test]
    fn cross_vault_crdt_path_and_baseline_mutations_preserve_other_vault() {
        let db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        let vault_b = "vault_b";
        seed_vault_row(&db, vault_a, "/tmp/v_a");
        seed_vault_row(&db, vault_b, "/tmp/v_b");

        let doc_id = "doc1";
        let rel_path = "notes/a.md";

        let doc1 = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc1, "vault_a_text").unwrap();
        let snap_a = doc1.export_snapshot();
        let delta_a =
            crate::sync::core::crdt::apply_text_update(&doc1, "vault_a_text_mod").unwrap();

        let doc2 = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc2, "vault_b_text").unwrap();
        let snap_b = doc2.export_snapshot();
        let delta_b =
            crate::sync::core::crdt::apply_text_update(&doc2, "vault_b_text_mod").unwrap();

        db.save_crdt_snapshot(vault_a, doc_id, snap_a).unwrap();
        db.save_crdt_snapshot(vault_b, doc_id, snap_b).unwrap();

        db.save_crdt_delta(vault_a, doc_id, delta_a).unwrap();
        db.save_crdt_delta(vault_b, doc_id, delta_b).unwrap();

        db.upsert_document_path(vault_a, doc_id, rel_path).unwrap();
        db.upsert_document_path(vault_b, doc_id, rel_path).unwrap();

        db.upsert_document_baseline(vault_a, "gdrive", rel_path, "hash_a")
            .unwrap();
        db.upsert_document_baseline(vault_b, "gdrive", rel_path, "hash_b")
            .unwrap();

        let before_doc_a: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_updates_a: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_paths_a: Vec<(String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_baselines_a: Vec<(String, String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, provider_id, rel_path, content_hash, updated_at FROM sync_document_baselines WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_doc_b: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_updates_b: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_paths_b: Vec<(String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_baselines_b: Vec<(String, String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, provider_id, rel_path, content_hash, updated_at FROM sync_document_baselines WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        db.delete_crdt_doc(vault_a, doc_id).unwrap();
        db.delete_document_path(vault_a, doc_id).unwrap();
        db.delete_document_baseline(vault_a, "gdrive", rel_path)
            .unwrap();

        let after_doc_a: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_updates_a: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_paths_a: Vec<(String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_baselines_a: Vec<(String, String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, provider_id, rel_path, content_hash, updated_at FROM sync_document_baselines WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_doc_b: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_updates_b: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_paths_b: Vec<(String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_baselines_b: Vec<(String, String, String, String, i64)> = db
            .conn
            .prepare("SELECT vault_id, provider_id, rel_path, content_hash, updated_at FROM sync_document_baselines WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(!before_doc_a.is_empty());
        assert!(!before_updates_a.is_empty());
        assert!(!before_paths_a.is_empty());
        assert!(!before_baselines_a.is_empty());

        assert_eq!(after_doc_a, vec![]);
        assert_eq!(after_updates_a, vec![]);
        assert_eq!(after_paths_a, vec![]);
        assert_eq!(after_baselines_a, vec![]);

        assert_eq!(before_doc_b, after_doc_b);
        assert_eq!(before_updates_b, after_updates_b);
        assert_eq!(before_paths_b, after_paths_b);
        assert_eq!(before_baselines_b, after_baselines_b);

        assert_eq!(
            db.get_crdt_doc(vault_a, doc_id)
                .unwrap()
                .get_text("content")
                .to_string(),
            ""
        );
        assert_eq!(db.get_path_by_node_id(vault_a, doc_id).unwrap(), None);
        assert_eq!(
            db.get_document_baseline(vault_a, "gdrive", rel_path)
                .unwrap(),
            None
        );

        assert_eq!(
            db.get_crdt_doc(vault_b, doc_id)
                .unwrap()
                .get_text("content")
                .to_string(),
            "vault_b_text_mod"
        );
        assert_eq!(
            db.get_path_by_node_id(vault_b, doc_id).unwrap(),
            Some(rel_path.to_string())
        );
        assert_eq!(
            db.get_document_baseline(vault_b, "gdrive", rel_path)
                .unwrap(),
            Some("hash_b".to_string())
        );
    }

    #[test]
    fn vault_scoped_export_contains_only_requested_documents() {
        let db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        let vault_b = "vault_b";
        seed_vault_row(&db, vault_a, "/tmp/v_a");
        seed_vault_row(&db, vault_b, "/tmp/v_b");

        let doc1 = loro::LoroDoc::new();
        doc1.get_text("content").insert(0, "doc1_text").unwrap();
        let snap1 = doc1.export_snapshot();

        let doc2 = loro::LoroDoc::new();
        doc2.get_text("content").insert(0, "doc2_text").unwrap();
        let snap2 = doc2.export_snapshot();

        db.save_crdt_snapshot(vault_a, "doc1", snap1.clone())
            .unwrap();
        db.save_crdt_snapshot(vault_b, "doc2", snap2.clone())
            .unwrap();

        let exported_a = db.export_snapshots(vault_a).unwrap();
        let exported_b = db.export_snapshots(vault_b).unwrap();

        assert_eq!(exported_a, vec![("doc1".to_string(), snap1)]);
        assert_eq!(exported_b, vec![("doc2".to_string(), snap2)]);
    }

    #[test]
    fn scoped_crdt_compaction_rolls_back_on_failure() {
        let mut db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        let vault_b = "vault_b";
        seed_vault_row(&db, vault_a, "/tmp/v_a");
        seed_vault_row(&db, vault_b, "/tmp/v_b");

        let doc_id = "doc1";

        let doc_a = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc_a, "compaction_initial").unwrap();
        let snap_a = doc_a.export_snapshot();

        let doc_a_mod = loro::LoroDoc::new();
        doc_a_mod.import(&snap_a).unwrap();
        let delta_a = crate::sync::core::crdt::apply_text_update(&doc_a_mod, " update_a").unwrap();

        db.save_crdt_snapshot(vault_a, doc_id, snap_a).unwrap();
        db.save_crdt_delta(vault_a, doc_id, delta_a).unwrap();

        let doc_b = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc_b, "b_initial").unwrap();
        let snap_b = doc_b.export_snapshot();
        let doc_b_mod = loro::LoroDoc::new();
        doc_b_mod.import(&snap_b).unwrap();
        let delta_b =
            crate::sync::core::crdt::apply_text_update(&doc_b_mod, "b_initial update_b").unwrap();
        db.save_crdt_snapshot(vault_b, doc_id, snap_b).unwrap();
        db.save_crdt_delta(vault_b, doc_id, delta_b).unwrap();

        let before_doc_a: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_updates_a: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_doc_b: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_b: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        db.conn
            .execute_batch(
                "CREATE TRIGGER fail_delete BEFORE DELETE ON sync_crdt_updates BEGIN SELECT RAISE(FAIL, 'compaction_failed'); END;",
            )
            .unwrap();

        let res = db.compact_crdt_history(vault_a, doc_id);
        let err_msg = res.unwrap_err().to_string();
        assert!(err_msg.contains("compaction_failed"));

        let after_doc_a: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_updates_a: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_doc_b: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_b: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_doc_a, after_doc_a);
        assert_eq!(before_updates_a, after_updates_a);
        assert_eq!(before_doc_b, after_doc_b);
        assert_eq!(before_b, after_b);
    }

    #[test]
    fn corrupt_scoped_crdt_and_path_rows_fail_closed() {
        let db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        seed_vault_row(&db, vault_a, "/tmp/v_a");

        db.conn
            .execute(
                "INSERT INTO sync_crdt_updates (vault_id, doc_id, update_id, delta, timestamp) VALUES (?1, 'doc1', 1, CAST(x'FF' AS BLOB), 100)",
                params![vault_a],
            )
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO sync_document_paths (vault_id, doc_id, rel_path, updated_at) VALUES (?1, 'doc2', CAST(x'FF' AS BLOB), 100)",
                params![vault_a],
            )
            .unwrap();

        let before_updates: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_paths: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let doc_res = db.get_crdt_doc(vault_a, "doc1");
        assert!(doc_res.is_err());

        let path_res = db.get_document_paths(vault_a);
        assert!(path_res.is_err());

        let after_updates: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_paths: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_updates, after_updates);
        assert_eq!(before_paths, after_paths);
    }

    #[test]
    fn peer_id_read_and_parse_errors_fail_closed_without_replacement() {
        let db = DbBridge::new_in_memory().unwrap();
        db.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS kv_store (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                [],
            )
            .unwrap();

        // 1) Stored TEXT that cannot be parsed as u64
        db.set_kv("device_peer_id", "not_a_number").unwrap();
        let before_kv_1: Vec<(String, String)> = db
            .conn
            .prepare("SELECT key, value FROM kv_store ORDER BY key")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let res1 = db.get_or_create_peer_id();
        assert!(res1.is_err());

        let after_kv_1: Vec<(String, String)> = db
            .conn
            .prepare("SELECT key, value FROM kv_store ORDER BY key")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_kv_1, after_kv_1);

        // 2) Stored SQLite BLOB that cannot be decoded as String
        db.conn.execute("DELETE FROM kv_store", []).unwrap();
        db.conn
            .execute(
                "INSERT INTO kv_store (key, value) VALUES ('device_peer_id', CAST(x'FF' AS BLOB))",
                [],
            )
            .unwrap();

        let before_kv_2: Vec<(String, Vec<u8>)> = db
            .conn
            .prepare("SELECT key, value FROM kv_store ORDER BY key")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let res2 = db.get_or_create_peer_id();
        assert!(res2.is_err());

        let after_kv_2: Vec<(String, Vec<u8>)> = db
            .conn
            .prepare("SELECT key, value FROM kv_store ORDER BY key")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_kv_2, after_kv_2);
    }

    #[test]
    fn replace_crdt_snapshot_rejects_invalid_and_rolls_back_on_delete_failure() {
        let db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        seed_vault_row(&db, vault_a, "/tmp/v_a");

        let doc_id = "doc1";
        let doc1 = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc1, "initial").unwrap();
        let snap_valid = doc1.export_snapshot();

        db.save_crdt_snapshot(vault_a, doc_id, snap_valid.clone())
            .unwrap();
        db.save_crdt_delta(vault_a, doc_id, vec![1, 2, 3]).unwrap();

        let before_doc: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_updates: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        // Subcase A: Invalid non-empty snapshot
        let res_invalid = db.replace_crdt_snapshot(vault_a, doc_id, &[1, 2, 3, 4, 5]);
        assert!(res_invalid.is_err());

        let after_doc_a: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_updates_a: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_doc, after_doc_a);
        assert_eq!(before_updates, after_updates_a);

        // Subcase B: Trigger failure on delete
        db.conn
            .execute_batch(
                "CREATE TRIGGER replace_failed BEFORE DELETE ON sync_crdt_updates BEGIN SELECT RAISE(FAIL, 'replace_failed'); END;",
            )
            .unwrap();

        let doc2 = loro::LoroDoc::new();
        crate::sync::core::crdt::apply_text_update(&doc2, "replacement").unwrap();
        let snap_new = doc2.export_snapshot();

        let res_trigger = db.replace_crdt_snapshot(vault_a, doc_id, &snap_new);
        let err_msg = res_trigger.unwrap_err().to_string();
        assert!(err_msg.contains("replace_failed"));

        let after_doc_b: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_updates_b: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_doc, after_doc_b);
        assert_eq!(before_updates, after_updates_b);
    }

    #[test]
    fn crdt_apply_safe_preserves_corrupt_durable_state() {
        let db = DbBridge::new_in_memory().unwrap();
        let vault_a = "vault_a";
        seed_vault_row(&db, vault_a, "/tmp/v_a");

        db.conn
            .execute(
                "INSERT INTO sync_crdt_updates (vault_id, doc_id, update_id, delta, timestamp) VALUES (?1, 'doc1', 1, CAST(x'FF' AS BLOB), 100)",
                params![vault_a],
            )
            .unwrap();

        let before_doc: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let before_updates: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let res = crate::commands::nodes::crdt_apply_safe(&db, vault_a, "doc1", "new_content");
        assert!(res.is_err());

        let after_doc: Vec<(String, String, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        let after_updates: Vec<(String, String, i64, Vec<u8>, i64)> = db
            .conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1")
            .unwrap()
            .query_map(params![vault_a], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert_eq!(before_doc, after_doc);
        assert_eq!(before_updates, after_updates);
    }
}

/// One row of the change-detection stat cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatCacheEntry {
    pub file_size: u64,
    pub mtime_ms: i64,
    pub content_hash: String,
}

impl DbBridge {
    /// Load every baseline for a provider in one query.
    ///
    /// Change detection used to take the database lock once per file; a vault of
    /// a few thousand notes meant a few thousand lock acquisitions per sync.
    pub fn load_document_baselines(
        &self,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<std::collections::HashMap<String, String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT rel_path, content_hash FROM sync_document_baselines
                 WHERE vault_id = ?1 AND provider_id = ?2",
            )
            .map_err(|e| AppError::General(format!("DB prepare baselines: {}", e)))?;

        let rows = stmt
            .query_map(params![vault_id, provider_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::General(format!("DB query baselines: {}", e)))?;

        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (path, hash) = row.map_err(|e| AppError::General(e.to_string()))?;
            out.insert(path, hash);
        }
        Ok(out)
    }

    /// Load the whole stat cache for a provider in one query.
    pub fn load_stat_cache(
        &self,
        vault_id: &str,
        provider_id: &str,
    ) -> AppResult<std::collections::HashMap<String, StatCacheEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT rel_path, file_size, mtime_ms, content_hash FROM sync_stat_cache
                 WHERE vault_id = ?1 AND provider_id = ?2",
            )
            .map_err(|e| AppError::General(format!("DB prepare stat cache: {}", e)))?;

        let rows = stmt
            .query_map(params![vault_id, provider_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    StatCacheEntry {
                        file_size: row.get::<_, i64>(1)?.max(0) as u64,
                        mtime_ms: row.get::<_, i64>(2)?,
                        content_hash: row.get::<_, String>(3)?,
                    },
                ))
            })
            .map_err(|e| AppError::General(format!("DB query stat cache: {}", e)))?;

        let mut out = std::collections::HashMap::new();
        for row in rows {
            let (path, entry) = row.map_err(|e| AppError::General(e.to_string()))?;
            out.insert(path, entry);
        }
        Ok(out)
    }

    /// Replace the stat cache for a provider with exactly what was observed on
    /// disk, so entries for files that no longer exist do not accumulate.
    pub fn replace_stat_cache(
        &mut self,
        vault_id: &str,
        provider_id: &str,
        entries: &[(String, StatCacheEntry)],
        now: i64,
    ) -> AppResult<()> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB tx stat cache: {}", e)))?;

        tx.execute(
            "DELETE FROM sync_stat_cache WHERE vault_id = ?1 AND provider_id = ?2",
            params![vault_id, provider_id],
        )
        .map_err(|e| AppError::General(format!("DB clear stat cache: {}", e)))?;

        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO sync_stat_cache
                     (vault_id, provider_id, rel_path, file_size, mtime_ms, content_hash, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|e| AppError::General(format!("DB prepare stat insert: {}", e)))?;

            for (rel_path, entry) in entries {
                stmt.execute(params![
                    vault_id,
                    provider_id,
                    rel_path,
                    entry.file_size as i64,
                    entry.mtime_ms,
                    entry.content_hash,
                    now
                ])
                .map_err(|e| AppError::General(format!("DB insert stat entry: {}", e)))?;
            }
        }

        tx.commit()
            .map_err(|e| AppError::General(format!("DB commit stat cache: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod compaction_tests {
    use super::*;
    use crate::db::DbBridge;

    fn db_with_doc() -> DbBridge {
        let mut db = DbBridge::new_in_memory_full().unwrap();
        db.insert_sync_vault_mapping(&crate::db::sync_vault::SyncVaultRecord {
            vault_id: "v1".into(),
            canonical_root: "/v1".into(),
            metadata_version: 1,
            created_at: 100,
            updated_at: 100,
        })
        .unwrap();
        db
    }

    /// A document holding one very large update never reached a row-count
    /// threshold, so it was never collapsed and sat in the update log forever.
    #[test]
    fn one_oversized_update_is_compacted_even_though_it_is_a_single_row() {
        let mut db = db_with_doc();

        let doc = loro::LoroDoc::new();
        doc.get_text("content")
            .insert(0, &"x".repeat(400_000))
            .unwrap();
        doc.commit();
        db.save_crdt_delta("v1", "doc", doc.export_snapshot())
            .unwrap();

        let before: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sync_crdt_updates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, 1, "precondition: exactly one, oversized, update");

        db.compact_all_crdt("v1").unwrap();

        let after: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sync_crdt_updates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 0, "the oversized update should have been collapsed");

        let snapshots: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sync_crdt_documents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(snapshots, 1, "and folded into a snapshot");
    }

    /// A small amount of history is left alone: compacting on every save would
    /// rewrite the whole document for one keystroke.
    #[test]
    fn a_little_history_is_left_to_accumulate() {
        let mut db = db_with_doc();
        let doc = loro::LoroDoc::new();
        doc.get_text("content").insert(0, "small").unwrap();
        doc.commit();
        db.save_crdt_delta("v1", "doc", doc.export_snapshot())
            .unwrap();

        db.compact_all_crdt("v1").unwrap();

        let after: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM sync_crdt_updates", [], |r| r.get(0))
            .unwrap();
        assert_eq!(after, 1, "a single small update is not worth collapsing");
    }
}
