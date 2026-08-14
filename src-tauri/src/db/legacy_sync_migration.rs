use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::sync::core::identity::VaultIdentity;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegacyMigrationDecision {
    Migrated { migrated_count: usize },
    BootstrapRequired { reason: String },
    AlreadyComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyStateSnapshot {
    pub crdt_documents: Vec<(String, Vec<u8>)>,
    pub crdt_updates: Vec<(i64, String, Vec<u8>, i64)>,
    pub document_paths: Vec<(String, String, i64)>,
    pub kv_store: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultScopedSyncStateSnapshot {
    pub crdt_documents: Vec<(String, String, Vec<u8>, i64)>,
    pub crdt_updates: Vec<(String, String, i64, Vec<u8>, i64)>,
    pub document_paths: Vec<(String, String, String, i64)>,
    pub document_baselines: Vec<(String, String, String, String, i64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyBackupRow {
    pub id: i64,
    pub migration_version: i64,
    pub source_order: i64,
    pub source_table: String,
    pub source_key: String,
    pub raw_payload: Vec<u8>,
    pub backed_up_at: i64,
}

const EMPTY_SNAPSHOT_SENTINEL_TABLE: &str = "__legacy_sync_manifest__";
const EMPTY_SNAPSHOT_SENTINEL_KEY: &str = "empty_snapshot";
const EMPTY_SNAPSHOT_SENTINEL_PAYLOAD: &[u8] = b"EMPTY_SNAPSHOT_V4";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyBackupCandidate {
    pub migration_version: i64,
    pub source_order: i64,
    pub source_table: String,
    pub source_key: String,
    pub raw_payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySyncInventory {
    pub crdt_documents: Vec<(String, Vec<u8>)>,
    pub crdt_updates: Vec<(i64, String, Vec<u8>, i64)>,
    pub document_paths: Vec<(String, String, i64)>,
    pub cursor_keys: Vec<(String, String)>,
    pub baseline_keys: Vec<(String, String)>,
    pub cursor_map: std::collections::BTreeMap<String, String>,
    pub registered_vault_ids: Vec<String>,
    pub existing_provider_pairs: Vec<(String, String)>,
    pub backup_candidates: Vec<LegacyBackupCandidate>,
}

pub fn capture_legacy_sync_inventory(db_bridge: &DbBridge) -> AppResult<LegacySyncInventory> {
    capture_legacy_sync_inventory_conn(db_bridge.conn())
}

pub fn capture_legacy_sync_inventory_conn(
    conn: &rusqlite::Connection,
) -> AppResult<LegacySyncInventory> {
    // 1. crdt_documents(doc_id, snapshot)
    let has_crdt_docs: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='crdt_documents'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(|e| AppError::General(format!("Failed checking crdt_documents table: {}", e)))?;

    let crdt_documents = if has_crdt_docs {
        let mut stmt = conn
            .prepare("SELECT doc_id, snapshot FROM crdt_documents ORDER BY doc_id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    } else {
        Vec::new()
    };

    // 2. crdt_updates(id, doc_id, delta, timestamp)
    let has_crdt_updates: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='crdt_updates'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(|e| AppError::General(format!("Failed checking crdt_updates table: {}", e)))?;

    let crdt_updates = if has_crdt_updates {
        let mut stmt = conn
            .prepare("SELECT id, doc_id, delta, timestamp FROM crdt_updates ORDER BY id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    } else {
        Vec::new()
    };

    // 3. document_paths(doc_id, rel_path, path_updated_at)
    let has_doc_paths: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='document_paths'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(|e| AppError::General(format!("Failed checking document_paths table: {}", e)))?;

    let document_paths = if has_doc_paths {
        let mut stmt = conn
            .prepare(
                "SELECT doc_id, rel_path, path_updated_at FROM document_paths ORDER BY doc_id ASC",
            )
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    } else {
        Vec::new()
    };

    // 4. kv_store (sync_cursor_ and sync_hash_)
    let has_kv_store: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kv_store'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(|e| AppError::General(format!("Failed checking kv_store table: {}", e)))?;

    let (cursor_keys, baseline_keys) = if has_kv_store {
        let mut stmt = conn
            .prepare("SELECT key, value FROM kv_store WHERE key LIKE 'sync_cursor_%' OR key LIKE 'sync_hash_%' ORDER BY key ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::General(e.to_string()))?;
        let rows: Vec<(String, String)> = mapped
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::General(e.to_string()))?;

        let mut cursors = Vec::new();
        let mut baselines = Vec::new();
        for (k, v) in rows {
            if k.starts_with("sync_cursor_") {
                cursors.push((k, v));
            } else if k.starts_with("sync_hash_") {
                baselines.push((k, v));
            }
        }
        (cursors, baselines)
    } else {
        (Vec::new(), Vec::new())
    };

    let cursor_map: std::collections::BTreeMap<String, String> = cursor_keys
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix("sync_cursor_")
                .map(|p| (p.to_string(), v.clone()))
        })
        .collect();

    // 5. registered_vault_ids
    let registered_vault_ids = {
        let mut stmt = conn
            .prepare("SELECT vault_id FROM sync_vaults ORDER BY vault_id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    };

    // 6. existing_provider_pairs
    let existing_provider_pairs = {
        let mut stmt = conn
            .prepare("SELECT vault_id, provider_id FROM sync_provider_state ORDER BY vault_id ASC, provider_id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    };

    // 7. backup_candidates
    let mut backup_candidates = Vec::new();
    let mut order: i64 = 1;

    for (doc_id, snapshot) in &crdt_documents {
        backup_candidates.push(LegacyBackupCandidate {
            migration_version: 4,
            source_order: order,
            source_table: "crdt_documents".to_string(),
            source_key: doc_id.clone(),
            raw_payload: snapshot.clone(),
        });
        order += 1;
    }

    for (id, doc_id, delta, timestamp) in &crdt_updates {
        let payload = serde_json::to_vec(&(id, doc_id.clone(), delta, timestamp))
            .map_err(|e| AppError::General(e.to_string()))?;
        let source_key = format!("{}:{}", doc_id, id);
        backup_candidates.push(LegacyBackupCandidate {
            migration_version: 4,
            source_order: order,
            source_table: "crdt_updates".to_string(),
            source_key,
            raw_payload: payload,
        });
        order += 1;
    }

    for (doc_id, rel_path, path_updated_at) in &document_paths {
        let payload = serde_json::to_vec(&(doc_id.clone(), rel_path.clone(), path_updated_at))
            .map_err(|e| AppError::General(e.to_string()))?;
        backup_candidates.push(LegacyBackupCandidate {
            migration_version: 4,
            source_order: order,
            source_table: "document_paths".to_string(),
            source_key: doc_id.clone(),
            raw_payload: payload,
        });
        order += 1;
    }

    for (key, val) in &cursor_keys {
        backup_candidates.push(LegacyBackupCandidate {
            migration_version: 4,
            source_order: order,
            source_table: "kv_store".to_string(),
            source_key: key.clone(),
            raw_payload: val.as_bytes().to_vec(),
        });
        order += 1;
    }

    for (key, val) in &baseline_keys {
        backup_candidates.push(LegacyBackupCandidate {
            migration_version: 4,
            source_order: order,
            source_table: "kv_store".to_string(),
            source_key: key.clone(),
            raw_payload: val.as_bytes().to_vec(),
        });
        order += 1;
    }

    if backup_candidates.is_empty() {
        backup_candidates.push(LegacyBackupCandidate {
            migration_version: 4,
            source_order: 1,
            source_table: EMPTY_SNAPSHOT_SENTINEL_TABLE.to_string(),
            source_key: EMPTY_SNAPSHOT_SENTINEL_KEY.to_string(),
            raw_payload: EMPTY_SNAPSHOT_SENTINEL_PAYLOAD.to_vec(),
        });
    }

    Ok(LegacySyncInventory {
        crdt_documents,
        crdt_updates,
        document_paths,
        cursor_keys,
        baseline_keys,
        cursor_map,
        registered_vault_ids,
        existing_provider_pairs,
        backup_candidates,
    })
}

pub fn backup_legacy_sync_state(
    tx: &rusqlite::Transaction,
    inventory: &LegacySyncInventory,
    now: i64,
) -> AppResult<usize> {
    let mut stmt = tx
        .prepare(
            "SELECT migration_version, source_order, source_table, source_key, raw_payload FROM sync_legacy_backup_rows WHERE migration_version = 4 ORDER BY source_order ASC",
        )
        .map_err(|e| AppError::General(e.to_string()))?;
    let mapped = stmt
        .query_map([], |r| {
            Ok(LegacyBackupCandidate {
                migration_version: r.get(0)?,
                source_order: r.get(1)?,
                source_table: r.get(2)?,
                source_key: r.get(3)?,
                raw_payload: r.get(4)?,
            })
        })
        .map_err(|e| AppError::General(e.to_string()))?;
    let existing_backups: Vec<LegacyBackupCandidate> = mapped
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::General(e.to_string()))?;
    drop(stmt);

    if existing_backups.is_empty() {
        for candidate in &inventory.backup_candidates {
            tx.execute(
                "INSERT INTO sync_legacy_backup_rows (migration_version, source_order, source_table, source_key, raw_payload, backed_up_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    candidate.migration_version,
                    candidate.source_order,
                    candidate.source_table,
                    candidate.source_key,
                    candidate.raw_payload,
                    now,
                ],
            )
            .map_err(|e| AppError::General(format!("Failed backing up candidate {} {}: {}", candidate.source_table, candidate.source_key, e)))?;
        }
        Ok(inventory.backup_candidates.len())
    } else {
        if existing_backups != inventory.backup_candidates {
            return Err(AppError::General(format!(
                "Backup reconciliation mismatch: existing backup rows ({}) do not match current inventory candidates ({})",
                existing_backups.len(),
                inventory.backup_candidates.len()
            )));
        }
        Ok(existing_backups.len())
    }
}

pub fn reconstruct_legacy_state_from_backup(
    db_bridge: &DbBridge,
) -> AppResult<LegacyStateSnapshot> {
    let backups = snapshot_legacy_backup(db_bridge)?;

    let mut crdt_documents = Vec::new();
    let mut crdt_updates = Vec::new();
    let mut document_paths = Vec::new();
    let mut kv_store = Vec::new();

    let has_sentinel = backups
        .iter()
        .any(|row| row.source_table == EMPTY_SNAPSHOT_SENTINEL_TABLE);

    if has_sentinel {
        if backups.len() != 1 {
            return Err(AppError::General(
                "Empty snapshot manifest cannot coexist with other backup rows".into(),
            ));
        }

        let sentinel = &backups[0];
        if sentinel.migration_version != 4
            || sentinel.source_order != 1
            || sentinel.source_table != EMPTY_SNAPSHOT_SENTINEL_TABLE
            || sentinel.source_key != EMPTY_SNAPSHOT_SENTINEL_KEY
            || sentinel.raw_payload != EMPTY_SNAPSHOT_SENTINEL_PAYLOAD
        {
            return Err(AppError::General(
                "Malformed empty snapshot backup manifest".into(),
            ));
        }

        return Ok(LegacyStateSnapshot {
            crdt_documents,
            crdt_updates,
            document_paths,
            kv_store,
        });
    }

    for row in backups {
        match row.source_table.as_str() {
            "crdt_documents" => {
                crdt_documents.push((row.source_key, row.raw_payload));
            }
            "crdt_updates" => {
                let (id, doc_id, delta, timestamp): (i64, String, Vec<u8>, i64) =
                    serde_json::from_slice(&row.raw_payload)
                        .map_err(|e| AppError::General(e.to_string()))?;
                crdt_updates.push((id, doc_id, delta, timestamp));
            }
            "document_paths" => {
                let (doc_id, rel_path, path_updated_at): (String, String, i64) =
                    serde_json::from_slice(&row.raw_payload)
                        .map_err(|e| AppError::General(e.to_string()))?;
                document_paths.push((doc_id, rel_path, path_updated_at));
            }
            "kv_store" => {
                let val_str = String::from_utf8(row.raw_payload)
                    .map_err(|e| AppError::General(e.to_string()))?;
                kv_store.push((row.source_key, val_str));
            }
            unknown => {
                return Err(AppError::General(format!(
                    "Unknown source table in backup: {}",
                    unknown
                )));
            }
        }
    }

    Ok(LegacyStateSnapshot {
        crdt_documents,
        crdt_updates,
        document_paths,
        kv_store,
    })
}

pub fn migrate_legacy_sync_state_for_vault(
    db_bridge: &mut DbBridge,
    identity: &VaultIdentity,
) -> AppResult<LegacyMigrationDecision> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::General(e.to_string()))?
        .as_millis() as i64;

    let vault_id_str = identity.vault_id.to_string();

    let canonical_str = identity
        .canonical_path
        .canonicalize()
        .map_err(|e| AppError::General(format!("Failed canonicalizing vault path: {}", e)))?
        .to_string_lossy()
        .to_string();

    let registered_canonical: Option<String> = db_bridge
        .conn()
        .query_row(
            "SELECT canonical_root FROM sync_vaults WHERE vault_id = ?1",
            rusqlite::params![vault_id_str],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| {
            AppError::General(format!("Failed reading registered vault mapping: {}", e))
        })?;

    match registered_canonical {
        Some(root) if root == canonical_str => {}
        _ => {
            return Err(AppError::General(
                "Vault identity missing or mismatched".into(),
            ));
        }
    }

    let existing_decision: Option<String> = db_bridge
        .conn()
        .query_row(
            "SELECT decision FROM sync_legacy_migration_state WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| AppError::General(format!("Failed checking legacy migration state: {}", e)))?;

    if let Some(dec) = existing_decision {
        if dec == "Migrated" || dec == "AlreadyComplete" {
            return Ok(LegacyMigrationDecision::AlreadyComplete);
        } else if dec == "BootstrapRequired" {
            return Ok(LegacyMigrationDecision::BootstrapRequired {
                reason: "bootstrap_required: previous migration recorded BootstrapRequired".into(),
            });
        }
    }

    // Step 1: Open an IMMEDIATE transaction for capture + backup commit barrier
    let backup_tx = db_bridge
        .conn_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| AppError::General(format!("Failed starting immediate backup tx: {}", e)))?;

    let inventory = capture_legacy_sync_inventory_conn(&backup_tx)?;
    backup_legacy_sync_state(&backup_tx, &inventory, now)?;

    backup_tx
        .commit()
        .map_err(|e| AppError::General(format!("Failed committing backup tx: {}", e)))?;

    // Step 2: Use captured inventory for apply/decision in a separate transaction
    let vault_count = inventory.registered_vault_ids.len() as i64;
    let is_ambiguous = vault_count > 1 || !inventory.baseline_keys.is_empty();

    if is_ambiguous {
        let apply_tx = db_bridge
            .conn_mut()
            .transaction()
            .map_err(|e| AppError::General(format!("Failed starting apply tx: {}", e)))?;

        let affected_vault_ids = if vault_count > 1 {
            inventory.registered_vault_ids.clone()
        } else {
            vec![vault_id_str.clone()]
        };

        let mut provider_pairs = std::collections::BTreeSet::new();

        for (v_id, p_id) in &inventory.existing_provider_pairs {
            if affected_vault_ids.contains(v_id) {
                provider_pairs.insert((v_id.clone(), p_id.clone()));
            }
        }

        for v_id in &affected_vault_ids {
            for p_id in inventory.cursor_map.keys() {
                provider_pairs.insert((v_id.clone(), p_id.clone()));
            }
        }

        for (v_id, provider_id) in provider_pairs {
            let cursor_val = inventory.cursor_map.get(&provider_id).cloned();
            apply_tx.execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, sync_state, created_at, updated_at)
                 VALUES (?1, ?2, COALESCE(?3, ''), 'bootstrap_required', ?4, ?4)
                 ON CONFLICT(vault_id, provider_id) DO UPDATE SET
                     cursor = CASE WHEN ?3 IS NOT NULL THEN ?3 ELSE sync_provider_state.cursor END,
                     sync_state = 'bootstrap_required',
                     updated_at = excluded.updated_at;",
                rusqlite::params![v_id, provider_id, cursor_val, now],
            )
            .map_err(|e| AppError::General(format!("Failed recording provider bootstrap_required: {}", e)))?;
        }

        apply_tx.execute(
            "INSERT INTO sync_legacy_migration_state (id, migration_version, status, decision, vault_id, completed_at, last_error, migrated_at)
             VALUES (1, 4, 'completed', 'BootstrapRequired', ?1, ?2, NULL, ?2)
             ON CONFLICT(id) DO UPDATE SET
                 migration_version = excluded.migration_version,
                 status = excluded.status,
                 decision = excluded.decision,
                 vault_id = excluded.vault_id,
                 completed_at = excluded.completed_at,
                 migrated_at = excluded.migrated_at;",
            rusqlite::params![vault_id_str, now],
        )
        .map_err(|e| AppError::General(format!("Failed recording bootstrap_required status: {}", e)))?;

        apply_tx.commit().map_err(|e| {
            AppError::General(format!("Failed committing bootstrap_required tx: {}", e))
        })?;

        return Ok(LegacyMigrationDecision::BootstrapRequired {
            reason: if vault_count > 1 {
                "bootstrap_required: multiple registered vaults exist".into()
            } else {
                "bootstrap_required: provider-less baseline keys exist".into()
            },
        });
    }

    // Step 3: Apply legacy state for single-vault deterministic path
    let apply_tx = db_bridge
        .conn_mut()
        .transaction()
        .map_err(|e| AppError::General(format!("Failed starting apply tx: {}", e)))?;

    let mut migrated_count = 0;

    for (doc_id, snapshot) in &inventory.crdt_documents {
        apply_tx
            .execute(
                "INSERT INTO sync_crdt_documents (vault_id, doc_id, snapshot, updated_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![vault_id_str, doc_id, snapshot, now],
            )
            .map_err(|e| AppError::General(format!("Failed migrating crdt_documents: {}", e)))?;
        migrated_count += 1;
    }

    for (id, doc_id, delta, timestamp) in &inventory.crdt_updates {
        apply_tx
            .execute(
                "INSERT INTO sync_crdt_updates (vault_id, doc_id, update_id, delta, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![vault_id_str, doc_id, id, delta, timestamp],
            )
            .map_err(|e| AppError::General(format!("Failed migrating crdt_updates: {}", e)))?;
        migrated_count += 1;
    }

    for (doc_id, rel_path, path_updated_at) in &inventory.document_paths {
        apply_tx
            .execute(
                "INSERT INTO sync_document_paths (vault_id, doc_id, rel_path, updated_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![vault_id_str, doc_id, rel_path, path_updated_at],
            )
            .map_err(|e| AppError::General(format!("Failed migrating document_paths: {}", e)))?;
        migrated_count += 1;
    }

    for (key, val) in &inventory.cursor_keys {
        let provider_id = key.trim_start_matches("sync_cursor_");
        apply_tx
            .execute(
                "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, sync_state, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'ready', ?4, ?4)
                 ON CONFLICT(vault_id, provider_id) DO UPDATE SET
                     cursor = excluded.cursor,
                     updated_at = excluded.updated_at;",
                rusqlite::params![vault_id_str, provider_id, val, now],
            )
            .map_err(|e| AppError::General(format!("Failed migrating provider cursor: {}", e)))?;
        migrated_count += 1;
    }

    apply_tx
        .execute(
            "INSERT INTO sync_legacy_migration_state (id, migration_version, status, decision, vault_id, completed_at, last_error, migrated_at)
             VALUES (1, 4, 'completed', 'Migrated', ?1, ?2, NULL, ?2)
             ON CONFLICT(id) DO UPDATE SET
                 migration_version = excluded.migration_version,
                 status = excluded.status,
                 decision = excluded.decision,
                 vault_id = excluded.vault_id,
                 completed_at = excluded.completed_at,
                 migrated_at = excluded.migrated_at;",
            rusqlite::params![vault_id_str, now],
        )
        .map_err(|e| AppError::General(format!("Failed recording MIGRATED state: {}", e)))?;

    apply_tx
        .commit()
        .map_err(|e| AppError::General(format!("Failed committing apply tx: {}", e)))?;

    Ok(LegacyMigrationDecision::Migrated { migrated_count })
}

pub fn snapshot_legacy_state(db_bridge: &DbBridge) -> AppResult<LegacyStateSnapshot> {
    let conn = db_bridge.conn();

    let crdt_documents = {
        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='crdt_documents'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .map_err(|e| AppError::General(e.to_string()))?;
        if has_table {
            let mut stmt = conn
                .prepare("SELECT doc_id, snapshot FROM crdt_documents ORDER BY doc_id ASC")
                .map_err(|e| AppError::General(e.to_string()))?;
            let mapped = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| AppError::General(e.to_string()))?;
            mapped
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::General(e.to_string()))?
        } else {
            Vec::new()
        }
    };

    let crdt_updates = {
        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='crdt_updates'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .map_err(|e| AppError::General(e.to_string()))?;
        if has_table {
            let mut stmt = conn
                .prepare("SELECT id, doc_id, delta, timestamp FROM crdt_updates ORDER BY id ASC")
                .map_err(|e| AppError::General(e.to_string()))?;
            let mapped = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .map_err(|e| AppError::General(e.to_string()))?;
            mapped
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::General(e.to_string()))?
        } else {
            Vec::new()
        }
    };

    let document_paths = {
        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='document_paths'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .map_err(|e| AppError::General(e.to_string()))?;
        if has_table {
            let mut stmt = conn
                .prepare("SELECT doc_id, rel_path, path_updated_at FROM document_paths ORDER BY doc_id ASC")
                .map_err(|e| AppError::General(e.to_string()))?;
            let mapped = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .map_err(|e| AppError::General(e.to_string()))?;
            mapped
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::General(e.to_string()))?
        } else {
            Vec::new()
        }
    };

    let kv_store = {
        let has_table: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kv_store'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .map_err(|e| AppError::General(e.to_string()))?;
        if has_table {
            let mut stmt = conn
                .prepare("SELECT key, value FROM kv_store WHERE key LIKE 'sync_cursor_%' OR key LIKE 'sync_hash_%' ORDER BY key ASC")
                .map_err(|e| AppError::General(e.to_string()))?;
            let mapped = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| AppError::General(e.to_string()))?;
            mapped
                .collect::<Result<_, _>>()
                .map_err(|e| AppError::General(e.to_string()))?
        } else {
            Vec::new()
        }
    };

    Ok(LegacyStateSnapshot {
        crdt_documents,
        crdt_updates,
        document_paths,
        kv_store,
    })
}

pub fn snapshot_vault_scoped_sync_state(
    db_bridge: &DbBridge,
    vault_id: &str,
) -> AppResult<VaultScopedSyncStateSnapshot> {
    let conn = db_bridge.conn();

    let crdt_documents = {
        let mut stmt = conn
            .prepare("SELECT vault_id, doc_id, snapshot, updated_at FROM sync_crdt_documents WHERE vault_id = ?1 ORDER BY doc_id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map(rusqlite::params![vault_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    };

    let crdt_updates = {
        let mut stmt = conn
            .prepare("SELECT vault_id, doc_id, update_id, delta, timestamp FROM sync_crdt_updates WHERE vault_id = ?1 ORDER BY update_id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map(rusqlite::params![vault_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    };

    let document_paths = {
        let mut stmt = conn
            .prepare("SELECT vault_id, doc_id, rel_path, updated_at FROM sync_document_paths WHERE vault_id = ?1 ORDER BY doc_id ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map(rusqlite::params![vault_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    };

    let document_baselines = {
        let mut stmt = conn
            .prepare("SELECT vault_id, provider_id, rel_path, content_hash, updated_at FROM sync_document_baselines WHERE vault_id = ?1 ORDER BY rel_path ASC")
            .map_err(|e| AppError::General(e.to_string()))?;
        let mapped = stmt
            .query_map(rusqlite::params![vault_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .map_err(|e| AppError::General(e.to_string()))?;
        mapped
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::General(e.to_string()))?
    };

    Ok(VaultScopedSyncStateSnapshot {
        crdt_documents,
        crdt_updates,
        document_paths,
        document_baselines,
    })
}

pub fn snapshot_legacy_backup(db_bridge: &DbBridge) -> AppResult<Vec<LegacyBackupRow>> {
    let conn = db_bridge.conn();
    let mut stmt = conn
        .prepare("SELECT id, migration_version, source_order, source_table, source_key, raw_payload, backed_up_at FROM sync_legacy_backup_rows ORDER BY source_order ASC, id ASC")
        .map_err(|e| AppError::General(e.to_string()))?;

    let mapped = stmt
        .query_map([], |r| {
            Ok(LegacyBackupRow {
                id: r.get(0)?,
                migration_version: r.get(1)?,
                source_order: r.get(2)?,
                source_table: r.get(3)?,
                source_key: r.get(4)?,
                raw_payload: r.get(5)?,
                backed_up_at: r.get(6)?,
            })
        })
        .map_err(|e| AppError::General(e.to_string()))?;

    mapped
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::General(e.to_string()))
}

pub fn snapshot_legacy_migration_state(
    db_bridge: &DbBridge,
) -> AppResult<
    Vec<(
        i64,
        i64,
        String,
        String,
        Option<String>,
        Option<i64>,
        Option<String>,
        i64,
    )>,
> {
    let conn = db_bridge.conn();
    let mut stmt = conn
        .prepare("SELECT id, migration_version, status, decision, vault_id, completed_at, last_error, migrated_at FROM sync_legacy_migration_state ORDER BY id ASC")
        .map_err(|e| AppError::General(e.to_string()))?;

    let mapped = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })
        .map_err(|e| AppError::General(e.to_string()))?;

    mapped
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::General(e.to_string()))
}

pub fn snapshot_provider_states(
    db_bridge: &DbBridge,
) -> AppResult<
    Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<Vec<u8>>,
        Option<String>,
        Option<String>,
        i64,
        i64,
    )>,
> {
    let conn = db_bridge.conn();
    let mut stmt = conn
        .prepare("SELECT vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at FROM sync_provider_state ORDER BY vault_id ASC, provider_id ASC")
        .map_err(|e| AppError::General(e.to_string()))?;

    let mapped = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
                r.get(9)?,
            ))
        })
        .map_err(|e| AppError::General(e.to_string()))?;

    mapped
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::General(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbBridge;
    use crate::sync::core::identity::load_or_register_vault_identity;
    use tauri::Manager;

    fn init_legacy_tables(db: &DbBridge) {
        db.conn()
            .execute_batch(
                "CREATE TABLE crdt_documents (
                    doc_id TEXT PRIMARY KEY,
                    snapshot BLOB NOT NULL
                );
                CREATE TABLE crdt_updates (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    doc_id TEXT NOT NULL,
                    delta BLOB NOT NULL,
                    timestamp INTEGER NOT NULL
                );
                CREATE TABLE document_paths (
                    doc_id TEXT PRIMARY KEY,
                    rel_path TEXT NOT NULL,
                    path_updated_at INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS kv_store (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );",
            )
            .unwrap();
    }

    fn create_test_app_handle(db: DbBridge) -> tauri::AppHandle<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle().clone();
        handle.manage(crate::db::DbState::new(db));
        handle
    }

    #[test]
    fn legacy_single_vault_migration_preserves_all_rows_and_backup() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO crdt_updates VALUES (1, 'doc1', x'01', 100)",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO document_paths VALUES ('doc1', 'file1.md', 100)",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_cursor_gdrive', 'cursor1')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db_guard = db_state.lock().unwrap();

        let snapshot_before = snapshot_legacy_state(&db_guard).unwrap();

        drop(db_guard);
        // insert_sync_vault_mapping load_or_register_vault_identity
        let identity =
            load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap();

        let snapshot_after = snapshot_legacy_state(&db).unwrap();
        assert_eq!(snapshot_before, snapshot_after);

        let target = snapshot_vault_scoped_sync_state(&db, &identity.vault_id.to_string()).unwrap();
        assert_eq!(target.crdt_documents.len(), 1);
        assert_eq!(target.crdt_updates.len(), 1);
        assert_eq!(target.document_paths.len(), 1);

        let backup = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup.len(), 4);
    }

    #[test]
    fn production_legacy_schema_migrates_two_ordered_deltas_losslessly() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();

        db.conn()
            .execute_batch(
                "CREATE TABLE crdt_documents (doc_id TEXT PRIMARY KEY, snapshot BLOB NOT NULL);
                 CREATE TABLE crdt_updates (id INTEGER PRIMARY KEY AUTOINCREMENT, doc_id TEXT NOT NULL, delta BLOB NOT NULL, timestamp INTEGER NOT NULL);
                 CREATE TABLE document_paths (doc_id TEXT PRIMARY KEY, rel_path TEXT NOT NULL, path_updated_at INTEGER NOT NULL);",
            )
            .unwrap();

        db.conn()
            .execute(
                "INSERT INTO crdt_updates (doc_id, delta, timestamp) VALUES ('doc1', x'0102', 100)",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO crdt_updates (doc_id, delta, timestamp) VALUES ('doc1', x'0304', 200)",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO document_paths VALUES ('doc1', 'notes/a.md', 100)",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let _identity =
            load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap();

        let reconstructed = reconstruct_legacy_state_from_backup(&db).unwrap();
        assert_eq!(reconstructed.crdt_updates.len(), 2);
        assert_eq!(reconstructed.crdt_updates[0].0, 1);
        assert_eq!(reconstructed.crdt_updates[1].0, 2);
    }

    #[test]
    fn ambiguous_legacy_state_is_backed_up_and_requires_bootstrap_without_assignment() {
        let temp_dir1 = tempfile::TempDir::new().unwrap();
        let temp_dir2 = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_cursor_gdrive', 'cursor1')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db_guard = db_state.lock().unwrap();

        let m1 = match crate::sync::core::identity::write_vault_metadata_atomically(
            temp_dir1.path(),
            &crate::sync::core::identity::VaultMetadata {
                vault_id: uuid::Uuid::from_u128(1001),
                schema_version: 1,
            },
        )
        .unwrap()
        {
            crate::sync::core::identity::VaultMetadataPublishOutcome::Published(m) => m,
            crate::sync::core::identity::VaultMetadataPublishOutcome::Existing(m) => m,
        };
        let m2 = match crate::sync::core::identity::write_vault_metadata_atomically(
            temp_dir2.path(),
            &crate::sync::core::identity::VaultMetadata {
                vault_id: uuid::Uuid::from_u128(1002),
                schema_version: 1,
            },
        )
        .unwrap()
        {
            crate::sync::core::identity::VaultMetadataPublishOutcome::Published(m) => m,
            crate::sync::core::identity::VaultMetadataPublishOutcome::Existing(m) => m,
        };

        let root1 = temp_dir1
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let root2 = temp_dir2
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        // insert_sync_vault_mapping load_or_register_vault_identity
        db_guard.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![m1.vault_id.to_string(), root1],
        ).unwrap();
        // insert_sync_vault_mapping load_or_register_vault_identity
        db_guard.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![m2.vault_id.to_string(), root2],
        ).unwrap();

        let snapshot_before = snapshot_legacy_state(&db_guard).unwrap();

        let identity1 = VaultIdentity {
            vault_id: m1.vault_id,
            canonical_path: temp_dir1.path().to_path_buf(),
        };

        let decision = migrate_legacy_sync_state_for_vault(&mut db_guard, &identity1).unwrap();
        assert!(matches!(
            decision,
            LegacyMigrationDecision::BootstrapRequired { .. }
        ));

        let snapshot_after = snapshot_legacy_state(&db_guard).unwrap();
        assert_eq!(snapshot_before, snapshot_after);

        let target = snapshot_vault_scoped_sync_state(&db_guard, &m1.vault_id.to_string()).unwrap();
        assert_eq!(target.crdt_documents.len(), 0);

        let providers = snapshot_provider_states(&db_guard).unwrap();
        assert!(!providers.is_empty());
        assert_eq!(providers[0].4, "bootstrap_required");
    }

    #[test]
    fn legacy_baseline_with_underscore_path_never_guesses_provider() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_hash_notes/my_file.md', 'hash123')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let identity =
            load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let decision = migrate_legacy_sync_state_for_vault(&mut db, &identity).unwrap();
        assert!(matches!(
            decision,
            LegacyMigrationDecision::BootstrapRequired { .. }
        ));

        let providers = snapshot_provider_states(&db).unwrap();
        assert!(providers.is_empty());

        let marker = snapshot_legacy_migration_state(&db).unwrap();
        assert_eq!(marker[0].3, "BootstrapRequired");
    }

    #[test]
    fn legacy_cursor_updates_existing_provider_without_overwriting_other_fields() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_cursor_gdrive', 'new_cursor')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let metadata = match crate::sync::core::identity::write_vault_metadata_atomically(
            temp_dir.path(),
            &crate::sync::core::identity::VaultMetadata {
                vault_id: uuid::Uuid::from_u128(123456789),
                schema_version: 1,
            },
        )
        .unwrap()
        {
            crate::sync::core::identity::VaultMetadataPublishOutcome::Published(m) => m,
            crate::sync::core::identity::VaultMetadataPublishOutcome::Existing(m) => m,
        };

        let canonical_str = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        db.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![metadata.vault_id.to_string(), canonical_str],
        ).unwrap();

        db.conn()
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        db.conn().execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at)
             VALUES (?1, 'gdrive', 'old_cursor', 'ack_1', 'ready', x'0102030405060708090a0b0c0d0e0f10', 'remote_v1', NULL, 100, 100)",
            rusqlite::params![metadata.vault_id.to_string()],
        ).unwrap();

        let providers_before = snapshot_provider_states(&db).unwrap();

        let identity = VaultIdentity {
            vault_id: metadata.vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        let decision = migrate_legacy_sync_state_for_vault(&mut db, &identity).unwrap();
        assert!(matches!(decision, LegacyMigrationDecision::Migrated { .. }));

        let providers_after = snapshot_provider_states(&db).unwrap();

        assert_eq!(providers_before.len(), 1);
        assert_eq!(providers_after.len(), 1);
        assert_eq!(providers_after[0].2, Some("new_cursor".to_string()));
    }

    #[test]
    fn providerless_baseline_with_cursor_requires_bootstrap_without_provider_guess() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_hash_notes/a.md', 'hash123')",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_cursor_gdrive', 'cur_123')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        // insert_sync_vault_mapping load_or_register_vault_identity
        let identity =
            load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap();

        let marker = snapshot_legacy_migration_state(&db).unwrap();
        assert_eq!(marker.len(), 1);
        assert_eq!(marker[0].3, "BootstrapRequired");

        let providers = snapshot_provider_states(&db).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].1, "gdrive");
        assert_eq!(providers[0].2, Some("cur_123".to_string()));
        assert_eq!(providers[0].4, "bootstrap_required");

        let target = snapshot_vault_scoped_sync_state(&db, &identity.vault_id.to_string()).unwrap();
        assert_eq!(target.document_baselines.len(), 0);

        let reconstructed = reconstruct_legacy_state_from_backup(&db).unwrap();
        assert_eq!(reconstructed.kv_store.len(), 2);
    }

    #[test]
    fn changed_legacy_source_after_apply_failure_is_rejected_before_retry_apply() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        db.conn()
            .execute_batch(
                "CREATE TRIGGER fail_apply BEFORE INSERT ON sync_crdt_documents BEGIN SELECT RAISE(FAIL, 'apply_failed'); END;"
            )
            .unwrap();

        let app = create_test_app_handle(db);

        // Run 1: load_or_register_vault_identity registers vault in DB then fails on apply
        let res1 = load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap());
        assert!(res1.is_err());

        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let backup_after_fail = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup_after_fail.len(), 1);

        // Read registered vault_id from sync_vaults
        let canonical_root = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id_str: String = db
            .conn()
            .query_row(
                "SELECT vault_id FROM sync_vaults WHERE canonical_root = ?1",
                rusqlite::params![canonical_root],
                |r| r.get(0),
            )
            .unwrap();
        let vault_id = uuid::Uuid::parse_str(&vault_id_str).unwrap();

        let target_after_fail = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        let providers_after_fail = snapshot_provider_states(&db).unwrap();
        let marker_after_fail = snapshot_legacy_migration_state(&db).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        // Mutate legacy raw source
        db.conn()
            .execute(
                "UPDATE crdt_documents SET snapshot = x'9999' WHERE doc_id = 'doc1'",
                [],
            )
            .unwrap();

        // Drop trigger and retry migration
        db.conn().execute_batch("DROP TRIGGER fail_apply;").unwrap();

        let res2 = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res2.is_err());

        let marker = snapshot_legacy_migration_state(&db).unwrap();
        assert!(marker.is_empty() || marker[0].3 != "Migrated");

        let backup_after_retry = snapshot_legacy_backup(&db).unwrap();
        let target_after_retry = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        let providers_after_retry = snapshot_provider_states(&db).unwrap();

        assert_eq!(backup_after_fail, backup_after_retry);
        assert_eq!(target_after_fail, target_after_retry);
        assert_eq!(providers_after_fail, providers_after_retry);
        assert_eq!(marker_after_fail, marker);
    }

    #[test]
    fn deleted_legacy_row_after_apply_failure_is_rejected_before_retry_apply() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();
        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc2', x'040506')", [])
            .unwrap();

        db.conn()
            .execute_batch(
                "CREATE TRIGGER fail_apply BEFORE INSERT ON sync_crdt_documents BEGIN SELECT RAISE(FAIL, 'apply_failed'); END;"
            )
            .unwrap();

        let app = create_test_app_handle(db);

        let res1 = load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap());
        assert!(res1.is_err());

        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_root = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id_str: String = db
            .conn()
            .query_row(
                "SELECT vault_id FROM sync_vaults WHERE canonical_root = ?1",
                rusqlite::params![canonical_root],
                |r| r.get(0),
            )
            .unwrap();
        let vault_id = uuid::Uuid::parse_str(&vault_id_str).unwrap();

        let backup_after_fail = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup_after_fail.len(), 2);
        let target_after_fail = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        let providers_after_fail = snapshot_provider_states(&db).unwrap();
        let marker_after_fail = snapshot_legacy_migration_state(&db).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        // Delete doc2 from legacy source
        db.conn()
            .execute("DELETE FROM crdt_documents WHERE doc_id = 'doc2'", [])
            .unwrap();

        db.conn().execute_batch("DROP TRIGGER fail_apply;").unwrap();

        let res2 = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res2.is_err());

        let backup_after_retry = snapshot_legacy_backup(&db).unwrap();
        let target_after_retry = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        let providers_after_retry = snapshot_provider_states(&db).unwrap();
        let marker_after_retry = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(backup_after_fail, backup_after_retry);
        assert_eq!(target_after_fail, target_after_retry);
        assert_eq!(providers_after_fail, providers_after_retry);
        assert_eq!(marker_after_fail, marker_after_retry);
    }

    #[test]
    fn added_legacy_row_after_apply_failure_is_rejected_before_retry_apply() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        db.conn()
            .execute_batch(
                "CREATE TRIGGER fail_apply BEFORE INSERT ON sync_crdt_documents BEGIN SELECT RAISE(FAIL, 'apply_failed'); END;"
            )
            .unwrap();

        let app = create_test_app_handle(db);

        let res1 = load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap());
        assert!(res1.is_err());

        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_root = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id_str: String = db
            .conn()
            .query_row(
                "SELECT vault_id FROM sync_vaults WHERE canonical_root = ?1",
                rusqlite::params![canonical_root],
                |r| r.get(0),
            )
            .unwrap();
        let vault_id = uuid::Uuid::parse_str(&vault_id_str).unwrap();

        let backup_after_fail = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup_after_fail.len(), 1);
        let target_after_fail = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        let providers_after_fail = snapshot_provider_states(&db).unwrap();
        let marker_after_fail = snapshot_legacy_migration_state(&db).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        // Add doc2 to legacy source
        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc2', x'040506')", [])
            .unwrap();

        db.conn().execute_batch("DROP TRIGGER fail_apply;").unwrap();

        let res2 = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res2.is_err());

        let backup_after_retry = snapshot_legacy_backup(&db).unwrap();
        let target_after_retry = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        let providers_after_retry = snapshot_provider_states(&db).unwrap();
        let marker_after_retry = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(backup_after_fail, backup_after_retry);
        assert_eq!(target_after_fail, target_after_retry);
        assert_eq!(providers_after_fail, providers_after_retry);
        assert_eq!(marker_after_fail, marker_after_retry);
    }

    #[test]
    fn ambiguity_preserves_vault_scoped_provider_pairs_without_cartesian_expansion() {
        let temp_dir1 = tempfile::TempDir::new().unwrap();
        let temp_dir2 = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_hash_notes/a.md', 'hash123')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db_guard = db_state.lock().unwrap();

        let m1 = match crate::sync::core::identity::write_vault_metadata_atomically(
            temp_dir1.path(),
            &crate::sync::core::identity::VaultMetadata {
                vault_id: uuid::Uuid::from_u128(1001),
                schema_version: 1,
            },
        )
        .unwrap()
        {
            crate::sync::core::identity::VaultMetadataPublishOutcome::Published(m) => m,
            crate::sync::core::identity::VaultMetadataPublishOutcome::Existing(m) => m,
        };
        let m2 = match crate::sync::core::identity::write_vault_metadata_atomically(
            temp_dir2.path(),
            &crate::sync::core::identity::VaultMetadata {
                vault_id: uuid::Uuid::from_u128(1002),
                schema_version: 1,
            },
        )
        .unwrap()
        {
            crate::sync::core::identity::VaultMetadataPublishOutcome::Published(m) => m,
            crate::sync::core::identity::VaultMetadataPublishOutcome::Existing(m) => m,
        };

        let root1 = temp_dir1
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let root2 = temp_dir2
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();

        db_guard.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![m1.vault_id.to_string(), root1],
        ).unwrap();
        db_guard.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![m2.vault_id.to_string(), root2],
        ).unwrap();

        // Seed v1/gdrive and v2/server
        db_guard.conn().execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, sync_state, created_at, updated_at) VALUES (?1, 'gdrive', 'ready', 100, 100)",
            rusqlite::params![m1.vault_id.to_string()],
        ).unwrap();
        db_guard.conn().execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, sync_state, created_at, updated_at) VALUES (?1, 'server', 'ready', 100, 100)",
            rusqlite::params![m2.vault_id.to_string()],
        ).unwrap();

        let identity1 = VaultIdentity {
            vault_id: m1.vault_id,
            canonical_path: temp_dir1.path().canonicalize().unwrap(),
        };

        let decision = migrate_legacy_sync_state_for_vault(&mut db_guard, &identity1).unwrap();
        assert!(matches!(
            decision,
            LegacyMigrationDecision::BootstrapRequired { .. }
        ));

        let providers = snapshot_provider_states(&db_guard).unwrap();
        assert_eq!(providers.len(), 2);

        let pairs: std::collections::BTreeSet<(String, String)> = providers
            .iter()
            .map(|p| (p.0.clone(), p.1.clone()))
            .collect();
        assert!(pairs.contains(&(m1.vault_id.to_string(), "gdrive".to_string())));
        assert!(pairs.contains(&(m2.vault_id.to_string(), "server".to_string())));

        for p in &providers {
            assert_eq!(p.4, "bootstrap_required");
        }

        let target1 =
            snapshot_vault_scoped_sync_state(&db_guard, &m1.vault_id.to_string()).unwrap();
        let target2 =
            snapshot_vault_scoped_sync_state(&db_guard, &m2.vault_id.to_string()).unwrap();
        assert_eq!(target1.crdt_documents.len(), 0);
        assert_eq!(target2.crdt_documents.len(), 0);
    }

    #[test]
    fn corrupt_provider_inventory_row_fails_closed() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        let canonical_str = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id = uuid::Uuid::from_u128(123456789);
        db.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![vault_id.to_string(), canonical_str],
        ).unwrap();

        // Seed sync_provider_state with invalid BLOB provider_id that cannot decode as UTF-8 String
        db.conn().execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, sync_state, created_at, updated_at) VALUES (?1, x'ff00ff00', 'ready', 100, 100)",
            rusqlite::params![vault_id.to_string()],
        ).unwrap();

        let target_before = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let backup_before = snapshot_legacy_backup(&db).unwrap();
        let marker_before = snapshot_legacy_migration_state(&db).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        let res = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res.is_err());

        let target_after = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let backup_after = snapshot_legacy_backup(&db).unwrap();
        let marker_after = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(target_before, target_after);
        assert_eq!(backup_before, backup_after);
        assert_eq!(marker_before, marker_after);
    }

    #[test]
    fn legacy_cursor_update_preserves_existing_provider_runtime_state() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_cursor_gdrive', 'new_cursor')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let metadata = match crate::sync::core::identity::write_vault_metadata_atomically(
            temp_dir.path(),
            &crate::sync::core::identity::VaultMetadata {
                vault_id: uuid::Uuid::from_u128(123456789),
                schema_version: 1,
            },
        )
        .unwrap()
        {
            crate::sync::core::identity::VaultMetadataPublishOutcome::Published(m) => m,
            crate::sync::core::identity::VaultMetadataPublishOutcome::Existing(m) => m,
        };

        let canonical_str = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        db.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![metadata.vault_id.to_string(), canonical_str],
        ).unwrap();

        // Seed all 10 provider fields with sync_state = 'disabled'
        db.conn()
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .unwrap();
        db.conn().execute(
            "INSERT INTO sync_provider_state (vault_id, provider_id, cursor, ack_cursor, sync_state, incarnation_id, remote_vault_id, last_error, created_at, updated_at)
             VALUES (?1, 'gdrive', 'old_cursor', 'ack_1', 'disabled', x'0102030405060708090a0b0c0d0e0f10', 'remote_v1', 'some_err', 100, 100)",
            rusqlite::params![metadata.vault_id.to_string()],
        ).unwrap();

        let providers_before = snapshot_provider_states(&db).unwrap();
        assert_eq!(providers_before.len(), 1);
        let before_row = &providers_before[0];

        let identity = VaultIdentity {
            vault_id: metadata.vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        // insert_sync_vault_mapping load_or_register_vault_identity
        let decision = migrate_legacy_sync_state_for_vault(&mut db, &identity).unwrap();
        assert!(matches!(decision, LegacyMigrationDecision::Migrated { .. }));

        let providers_after = snapshot_provider_states(&db).unwrap();
        assert_eq!(providers_after.len(), 1);
        let after_row = &providers_after[0];

        // 1. Vault ID, Provider ID match
        assert_eq!(after_row.0, before_row.0);
        assert_eq!(after_row.1, before_row.1);

        // 2. Cursor updated to new_cursor
        assert_eq!(after_row.2, Some("new_cursor".to_string()));

        // 3. All other 7 preserved fields byte/field equality (including sync_state == 'disabled')
        assert_eq!(after_row.3, before_row.3);
        assert_eq!(after_row.4, "disabled");
        assert_eq!(after_row.5, before_row.5);
        assert_eq!(after_row.6, before_row.6);
        assert_eq!(after_row.7, before_row.7);
        assert_eq!(after_row.8, before_row.8);
    }

    #[test]
    fn unregistered_or_mismatched_identity_cannot_claim_legacy_state() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        let target_before = snapshot_vault_scoped_sync_state(&db, "nonexistent").unwrap();

        let identity = VaultIdentity {
            vault_id: uuid::Uuid::from_u128(999999999),
            canonical_path: temp_dir.path().to_path_buf(),
        };

        let res = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res.is_err());

        let target_after = snapshot_vault_scoped_sync_state(&db, "nonexistent").unwrap();
        assert_eq!(target_before, target_after);
    }

    #[test]
    fn legacy_backup_failure_is_zero_mutation() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_str = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id = uuid::Uuid::from_u128(123456789);
        db.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![vault_id.to_string(), canonical_str],
        ).unwrap();

        db.conn().execute_batch(
            "CREATE TRIGGER fail_backup BEFORE INSERT ON sync_legacy_backup_rows BEGIN SELECT RAISE(FAIL, 'backup_failed'); END;"
        ).unwrap();

        let raw_before = snapshot_legacy_state(&db).unwrap();
        let target_before = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let marker_before = snapshot_legacy_migration_state(&db).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        let res = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res.is_err());

        let raw_after = snapshot_legacy_state(&db).unwrap();
        let target_after = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let marker_after = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(raw_before, raw_after);
        assert_eq!(target_before, target_after);
        assert_eq!(marker_before, marker_after);
    }

    #[test]
    fn legacy_apply_failure_preserves_committed_backup_and_source() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_str = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id = uuid::Uuid::from_u128(123456789);
        db.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![vault_id.to_string(), canonical_str],
        ).unwrap();

        db.conn().execute_batch(
            "CREATE TRIGGER fail_apply BEFORE INSERT ON sync_crdt_documents BEGIN SELECT RAISE(FAIL, 'apply_failed'); END;"
        ).unwrap();

        let raw_before = snapshot_legacy_state(&db).unwrap();
        let target_before = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let marker_before = snapshot_legacy_migration_state(&db).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        let res = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res.is_err());

        let backup = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup.len(), 1);

        let raw_after = snapshot_legacy_state(&db).unwrap();
        let target_after = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let marker_after = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(raw_before, raw_after);
        assert_eq!(target_before, target_after);
        assert_eq!(marker_before, marker_after);
    }

    #[test]
    fn conflicting_scoped_target_aborts_without_marking_complete() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_str = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id = uuid::Uuid::from_u128(123456789);
        db.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![vault_id.to_string(), canonical_str],
        ).unwrap();

        db.conn()
            .execute(
                "INSERT INTO sync_crdt_documents (vault_id, doc_id, snapshot, updated_at) VALUES (?1, 'doc1', x'9999', 50)",
                rusqlite::params![vault_id.to_string()],
            )
            .unwrap();

        let target_before = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let marker_before = snapshot_legacy_migration_state(&db).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        let res = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res.is_err());

        let target_after = snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let marker_after = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(target_before, target_after);
        assert_eq!(marker_before, marker_after);
    }

    #[test]
    fn legacy_migration_reopen_is_idempotent_and_keeps_vault_identity() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute(
                "INSERT INTO crdt_updates VALUES (1, 'doc1', x'010203', 100)",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);

        // Open 1
        let id1 = load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap();

        let target1 = snapshot_vault_scoped_sync_state(&db, &id1.vault_id.to_string()).unwrap();
        let meta1 = std::fs::read(temp_dir.path().join(".synabit").join("vault.json")).unwrap();
        let backup1 = snapshot_legacy_backup(&db).unwrap();
        let marker1 = snapshot_legacy_migration_state(&db).unwrap();

        drop(db);

        // Open 2
        let id2 = load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap();

        let target2 = snapshot_vault_scoped_sync_state(&db, &id2.vault_id.to_string()).unwrap();
        let meta2 = std::fs::read(temp_dir.path().join(".synabit").join("vault.json")).unwrap();
        let backup2 = snapshot_legacy_backup(&db).unwrap();
        let marker2 = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(id1.vault_id, id2.vault_id);
        assert_eq!(target1, target2);
        assert_eq!(meta1, meta2);
        assert_eq!(backup1, backup2);
        assert_eq!(marker1, marker2);
    }

    #[test]
    fn bootstrap_required_reopen_is_idempotent() {
        let temp_dir1 = tempfile::TempDir::new().unwrap();
        let temp_dir2 = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_cursor_gdrive', 'cursor1')",
                [],
            )
            .unwrap();

        let app = create_test_app_handle(db);
        let _id1 =
            load_or_register_vault_identity(&app, temp_dir1.path().to_str().unwrap()).unwrap();
        let id2 =
            load_or_register_vault_identity(&app, temp_dir2.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap();

        let backup1 = snapshot_legacy_backup(&db).unwrap();
        let providers1 = snapshot_provider_states(&db).unwrap();
        let marker1 = snapshot_legacy_migration_state(&db).unwrap();

        drop(db);

        // Reopen identity2
        let _id2_again =
            load_or_register_vault_identity(&app, temp_dir2.path().to_str().unwrap()).unwrap();

        let db_state = app.state::<crate::db::DbState>();
        let db = db_state.lock().unwrap();

        let backup2 = snapshot_legacy_backup(&db).unwrap();
        let providers2 = snapshot_provider_states(&db).unwrap();
        let marker2 = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(backup1, backup2);
        assert_eq!(providers1, providers2);
        assert_eq!(marker1, marker2);
    }

    #[test]
    fn apply_failure_retry_reuses_backup_without_duplicates() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        let app = create_test_app_handle(db);
        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_str = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id = uuid::Uuid::from_u128(123456789);
        db.conn().execute(
            "INSERT INTO sync_vaults (vault_id, canonical_root, metadata_version, created_at, updated_at) VALUES (?1, ?2, 1, 100, 100)",
            rusqlite::params![vault_id.to_string(), canonical_str],
        ).unwrap();

        db.conn().execute_batch(
            "CREATE TRIGGER fail_apply BEFORE INSERT ON sync_crdt_documents BEGIN SELECT RAISE(FAIL, 'apply_failed'); END;"
        ).unwrap();

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().to_path_buf(),
        };

        // First attempt fails
        let res1 = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res1.is_err());

        let target_after_fail =
            snapshot_vault_scoped_sync_state(&db, &vault_id.to_string()).unwrap();
        let backup_after_fail = snapshot_legacy_backup(&db).unwrap();
        let marker_after_fail = snapshot_legacy_migration_state(&db).unwrap();

        // Remove trigger
        db.conn().execute_batch("DROP TRIGGER fail_apply;").unwrap();

        // Second attempt succeeds
        let res2 = migrate_legacy_sync_state_for_vault(&mut db, &identity).unwrap();
        assert!(matches!(res2, LegacyMigrationDecision::Migrated { .. }));

        let backup_after_retry = snapshot_legacy_backup(&db).unwrap();
        let marker_after_retry = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(backup_after_fail, backup_after_retry);
        assert_ne!(marker_after_fail, marker_after_retry);
        assert_eq!(marker_after_retry[0].3, "Migrated");

        let reconstructed = reconstruct_legacy_state_from_backup(&db).unwrap();
        assert_eq!(reconstructed.crdt_documents.len(), 1);
    }

    #[test]
    fn malformed_empty_snapshot_manifest_fails_closed() {
        let db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        // Seed raw SQL row with wrong source_order (99 instead of 1)
        db.conn()
            .execute(
                "INSERT INTO sync_legacy_backup_rows (migration_version, source_order, source_table, source_key, raw_payload, backed_up_at)
                 VALUES (4, 99, '__legacy_sync_manifest__', 'empty_snapshot', x'454d5054595f534e415053484f545f5634', 100)",
                [],
            )
            .unwrap();

        let initial_backups = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(initial_backups.len(), 1);

        let res = reconstruct_legacy_state_from_backup(&db);
        assert!(res.is_err());

        let backups_after = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(initial_backups, backups_after);
    }

    #[test]
    fn empty_snapshot_manifest_cannot_coexist_with_source_rows() {
        let db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        // Seed canonical sentinel AND a real CRDT backup candidate row
        db.conn()
            .execute(
                "INSERT INTO sync_legacy_backup_rows (migration_version, source_order, source_table, source_key, raw_payload, backed_up_at)
                 VALUES (4, 1, '__legacy_sync_manifest__', 'empty_snapshot', x'454d5054595f534e415053484f545f5634', 100)",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO sync_legacy_backup_rows (migration_version, source_order, source_table, source_key, raw_payload, backed_up_at)
                 VALUES (4, 2, 'crdt_documents', 'doc1', x'010203', 100)",
                [],
            )
            .unwrap();

        let initial_backups = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(initial_backups.len(), 2);

        let res = reconstruct_legacy_state_from_backup(&db);
        assert!(res.is_err());

        let backups_after = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(initial_backups, backups_after);
    }

    #[test]
    fn empty_legacy_snapshot_is_durably_recorded_and_rejects_later_source_growth() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        // Inject trigger to fail terminal migration state insert during apply
        db.conn()
            .execute_batch(
                "CREATE TRIGGER fail_marker BEFORE INSERT ON sync_legacy_migration_state BEGIN SELECT RAISE(FAIL, 'marker_failed'); END;"
            )
            .unwrap();

        let app = create_test_app_handle(db);

        let res1 = load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap());
        assert!(res1.is_err());

        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_root = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id_str: String = db
            .conn()
            .query_row(
                "SELECT vault_id FROM sync_vaults WHERE canonical_root = ?1",
                rusqlite::params![canonical_root],
                |r| r.get(0),
            )
            .unwrap();
        let vault_id = uuid::Uuid::parse_str(&vault_id_str).unwrap();

        // 1. Assert durable backup contains valid empty-snapshot sentinel with exact fields
        let backup_after_fail = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup_after_fail.len(), 1);
        assert_eq!(backup_after_fail[0].migration_version, 4);
        assert_eq!(backup_after_fail[0].source_order, 1);
        assert_eq!(
            backup_after_fail[0].source_table,
            "__legacy_sync_manifest__"
        );
        assert_eq!(backup_after_fail[0].source_key, "empty_snapshot");
        assert_eq!(backup_after_fail[0].raw_payload, b"EMPTY_SNAPSHOT_V4");

        // 2. Full-state comparisons for raw source, target, provider, marker
        let raw_after_fail = snapshot_legacy_state(&db).unwrap();
        assert_eq!(
            raw_after_fail,
            LegacyStateSnapshot {
                crdt_documents: vec![],
                crdt_updates: vec![],
                document_paths: vec![],
                kv_store: vec![],
            }
        );

        let target_after_fail = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        assert_eq!(
            target_after_fail,
            VaultScopedSyncStateSnapshot {
                crdt_documents: vec![],
                crdt_updates: vec![],
                document_paths: vec![],
                document_baselines: vec![],
            }
        );

        let providers_after_fail = snapshot_provider_states(&db).unwrap();
        assert_eq!(providers_after_fail, vec![]);

        let marker_after_fail = snapshot_legacy_migration_state(&db).unwrap();
        assert_eq!(marker_after_fail, vec![]);

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().canonicalize().unwrap(),
        };

        // 3. Add a real legacy CRDT row and drop trigger
        db.conn()
            .execute("INSERT INTO crdt_documents VALUES ('doc1', x'010203')", [])
            .unwrap();

        db.conn()
            .execute_batch("DROP TRIGGER fail_marker;")
            .unwrap();

        // 4. Retry production seam
        let res2 = migrate_legacy_sync_state_for_vault(&mut db, &identity);
        assert!(res2.is_err());

        // 5. Assert complete backup equality, target/provider/marker unchanged
        let backup_after_retry = snapshot_legacy_backup(&db).unwrap();
        let raw_after_retry = snapshot_legacy_state(&db).unwrap();
        let target_after_retry = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        let providers_after_retry = snapshot_provider_states(&db).unwrap();
        let marker_after_retry = snapshot_legacy_migration_state(&db).unwrap();

        assert_eq!(
            raw_after_retry,
            LegacyStateSnapshot {
                crdt_documents: vec![("doc1".to_string(), vec![1, 2, 3])],
                crdt_updates: vec![],
                document_paths: vec![],
                kv_store: vec![],
            }
        );
        assert_eq!(backup_after_fail, backup_after_retry);
        assert_eq!(target_after_fail, target_after_retry);
        assert_eq!(providers_after_fail, providers_after_retry);
        assert_eq!(marker_after_fail, marker_after_retry);
    }

    #[test]
    fn ambiguity_marker_failure_preserves_committed_backup_before_retry() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mut db = DbBridge::new_in_memory().unwrap();
        init_legacy_tables(&db);

        // Seed provider-less baseline and explicit cursor
        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_hash_notes/a.md', 'hash123')",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO kv_store VALUES ('sync_cursor_gdrive', 'cur_123')",
                [],
            )
            .unwrap();

        // Inject trigger on terminal migration state insert
        db.conn()
            .execute_batch(
                "CREATE TRIGGER fail_marker BEFORE INSERT ON sync_legacy_migration_state BEGIN SELECT RAISE(FAIL, 'marker_failed'); END;"
            )
            .unwrap();

        let app = create_test_app_handle(db);

        let res1 = load_or_register_vault_identity(&app, temp_dir.path().to_str().unwrap());
        assert!(res1.is_err());

        let db_state = app.state::<crate::db::DbState>();
        let mut db = db_state.lock().unwrap();

        let canonical_root = temp_dir
            .path()
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let vault_id_str: String = db
            .conn()
            .query_row(
                "SELECT vault_id FROM sync_vaults WHERE canonical_root = ?1",
                rusqlite::params![canonical_root],
                |r| r.get(0),
            )
            .unwrap();
        let vault_id = uuid::Uuid::parse_str(&vault_id_str).unwrap();

        // 1. Assert raw source unchanged via full state comparison
        let raw_after_fail = snapshot_legacy_state(&db).unwrap();
        assert_eq!(
            raw_after_fail,
            LegacyStateSnapshot {
                crdt_documents: vec![],
                crdt_updates: vec![],
                document_paths: vec![],
                kv_store: vec![
                    ("sync_cursor_gdrive".to_string(), "cur_123".to_string()),
                    ("sync_hash_notes/a.md".to_string(), "hash123".to_string()),
                ],
            }
        );

        // 2. Assert backup committed and reconstructs byte/field-equal source snapshot
        let backup_after_fail = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup_after_fail.len(), 2);

        let reconstructed = reconstruct_legacy_state_from_backup(&db).unwrap();
        assert_eq!(raw_after_fail, reconstructed);

        // 3. Full state comparisons for target, provider, marker before retry
        let target_after_fail = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        assert_eq!(
            target_after_fail,
            VaultScopedSyncStateSnapshot {
                crdt_documents: vec![],
                crdt_updates: vec![],
                document_paths: vec![],
                document_baselines: vec![],
            }
        );

        let providers_after_fail = snapshot_provider_states(&db).unwrap();
        assert_eq!(providers_after_fail, vec![]);

        let marker_after_fail = snapshot_legacy_migration_state(&db).unwrap();
        assert_eq!(marker_after_fail, vec![]);

        let identity = VaultIdentity {
            vault_id,
            canonical_path: temp_dir.path().canonicalize().unwrap(),
        };

        // 4. Drop trigger and retry with unchanged source
        db.conn()
            .execute_batch("DROP TRIGGER fail_marker;")
            .unwrap();

        let decision = migrate_legacy_sync_state_for_vault(&mut db, &identity).unwrap();
        assert!(matches!(
            decision,
            LegacyMigrationDecision::BootstrapRequired { .. }
        ));

        // 5. Assert backup completely unchanged and raw source equal after retry
        let backup_after_retry = snapshot_legacy_backup(&db).unwrap();
        assert_eq!(backup_after_fail, backup_after_retry);

        let raw_after_retry = snapshot_legacy_state(&db).unwrap();
        assert_eq!(raw_after_retry, raw_after_fail);

        // 6. Assert exact provider-pair semantics, zero scoped target assignment, complete provider/marker tuples
        let target_after_retry = snapshot_vault_scoped_sync_state(&db, &vault_id_str).unwrap();
        assert_eq!(
            target_after_retry,
            VaultScopedSyncStateSnapshot {
                crdt_documents: vec![],
                crdt_updates: vec![],
                document_paths: vec![],
                document_baselines: vec![],
            }
        );

        let providers_after_retry = snapshot_provider_states(&db).unwrap();
        assert_eq!(providers_after_retry.len(), 1);
        let (p_v_id, p_id, cur, ack_cur, st, inc_id, rem_v_id, err, created_at, updated_at) =
            &providers_after_retry[0];
        assert_eq!(p_v_id, &vault_id_str);
        assert_eq!(p_id, "gdrive");
        assert_eq!(cur, &Some("cur_123".to_string()));
        assert_eq!(ack_cur, &None);
        assert_eq!(st, "bootstrap_required");
        assert_eq!(inc_id, &None);
        assert_eq!(rem_v_id, &None);
        assert_eq!(err, &None);
        assert_eq!(*created_at, *updated_at);
        assert!(*created_at > 0);

        let marker_after_retry = snapshot_legacy_migration_state(&db).unwrap();
        assert_eq!(marker_after_retry.len(), 1);
        let (m_id, m_ver, m_status, m_decision, m_v_id, m_completed_at, m_last_err, m_migrated_at) =
            &marker_after_retry[0];
        assert_eq!(*m_id, 1);
        assert_eq!(*m_ver, 4);
        assert_eq!(m_status, "completed");
        assert_eq!(m_decision, "BootstrapRequired");
        assert_eq!(m_v_id, &Some(vault_id_str));
        assert_eq!(*m_completed_at, Some(*m_migrated_at));
        assert_eq!(m_last_err, &None);
        assert!(*m_migrated_at > 0);
    }
}
