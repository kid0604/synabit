//! GDrive → P2P migration support.
//!
//! Provides pre-check validation and post-migration cleanup for users
//! switching from Google Drive sync to the P2P sync engine.


use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------


/// Result summary returned after a migration completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    pub docs_migrated: u32,
    pub assets_migrated: u32,
    pub errors: Vec<String>,
    pub success: bool,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

pub struct SyncMigration;

impl SyncMigration {

    /// Phase 5 Migration: V4 (Path-based doc_id) -> V5 (UUID-based node_id)
    pub fn migrate_v4_to_v5(app_handle: &tauri::AppHandle) -> AppResult<MigrationResult> {
        let db_state = app_handle.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

        let vault_path_str = db.get_kv("vault_path")?.unwrap_or_default();
        if vault_path_str.is_empty() {
            return Err(AppError::General("Vault path not set".to_string()));
        }
        let vault_path = std::path::Path::new(&vault_path_str);

        // Get all current doc_ids from SQLite (these are relative paths in V4)
        let mut stmt = db.conn().prepare("SELECT doc_id FROM crdt_documents")
            .map_err(|e| AppError::General(e.to_string()))?;
        
        let old_doc_ids: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap().filter_map(|r| r.ok()).collect();

        let mut docs_migrated = 0;
        let mut errors = Vec::new();

        for old_doc_id in old_doc_ids {
            let file_path = vault_path.join(&old_doc_id);
            if !file_path.exists() {
                // If file doesn't exist, we skip. It might be a deleted file or a conflict artifact.
                continue;
            }

            match super::core::identity::get_or_assign_node_id(vault_path, &file_path) {
                Ok(new_node_id) => {
                    // Update database
                    // 1. Change doc_id in crdt_documents
                    let _ = db.conn().execute(
                        "UPDATE crdt_documents SET doc_id = ?1 WHERE doc_id = ?2",
                        rusqlite::params![&new_node_id, &old_doc_id]
                    );
                    
                    // 2. Change doc_id in crdt_updates
                    let _ = db.conn().execute(
                        "UPDATE crdt_updates SET doc_id = ?1 WHERE doc_id = ?2",
                        rusqlite::params![&new_node_id, &old_doc_id]
                    );

                    // 3. Upsert into document_paths mapping
                    let _ = db.upsert_document_path(&new_node_id, &old_doc_id);

                    docs_migrated += 1;
                }
                Err(e) => {
                    errors.push(format!("Failed to migrate {}: {}", old_doc_id, e));
                }
            }
        }

        let _ = db.set_kv("v5_identity_migration_completed", "true");

        Ok(MigrationResult {
            docs_migrated,
            assets_migrated: 0,
            errors,
            success: true,
        })
    }
}

