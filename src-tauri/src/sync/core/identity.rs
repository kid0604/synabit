use crate::db::DbState;
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::Manager;

pub const VAULT_METADATA_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultMetadata {
    pub schema_version: u32,
    pub vault_id: uuid::Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultMetadataPublishOutcome {
    Published(VaultMetadata),
    Existing(VaultMetadata),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultIdentity {
    pub vault_id: uuid::Uuid,
    pub canonical_path: PathBuf,
}

pub fn write_vault_metadata_atomically(
    vault_path: &Path,
    metadata: &VaultMetadata,
) -> AppResult<VaultMetadataPublishOutcome> {
    let synabit_dir = vault_path.join(".synabit");
    if !synabit_dir.exists() {
        std::fs::create_dir_all(&synabit_dir).map_err(|e| {
            AppError::General(format!("Failed to create .synabit directory: {}", e))
        })?;
    }

    let temp_name = format!("vault.json.tmp.{}", uuid::Uuid::new_v4());
    let temp_path = synabit_dir.join(temp_name);
    let vault_json_path = synabit_dir.join("vault.json");

    let json_bytes = serde_json::to_string_pretty(metadata)
        .map_err(|e| AppError::General(format!("Failed to serialize VaultMetadata: {}", e)))?;

    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| AppError::General(format!("Failed to create temp vault.json file: {}", e)))?;

    temp_file
        .write_all(json_bytes.as_bytes())
        .map_err(|e| AppError::General(format!("Failed to write vault metadata bytes: {}", e)))?;

    temp_file.sync_all().map_err(|e| {
        AppError::General(format!("Failed to sync temp vault metadata file: {}", e))
    })?;

    let outcome = match std::fs::hard_link(&temp_path, &vault_json_path) {
        Ok(()) => {
            std::fs::remove_file(&temp_path).map_err(|e| {
                AppError::General(format!("Failed to remove temp vault.json file: {}", e))
            })?;
            VaultMetadataPublishOutcome::Published(metadata.clone())
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if temp_path.exists() {
                std::fs::remove_file(&temp_path).map_err(|e| {
                    AppError::General(format!("Failed to remove temp vault.json file: {}", e))
                })?;
            }
            let existing = read_and_parse_vault_metadata(&vault_json_path)?;
            VaultMetadataPublishOutcome::Existing(existing)
        }
        Err(e) => {
            if temp_path.exists() {
                std::fs::remove_file(&temp_path).map_err(|cleanup_err| {
                    AppError::General(format!("Failed cleanup: {}", cleanup_err))
                })?;
            }
            return Err(AppError::General(format!(
                "Failed to publish vault metadata via hard_link: {}",
                e
            )));
        }
    };

    let parent_dir_file = File::open(&synabit_dir).map_err(|e| {
        AppError::General(format!("Failed to open .synabit directory for sync: {}", e))
    })?;
    parent_dir_file
        .sync_all()
        .map_err(|e| AppError::General(format!("Failed to sync .synabit directory: {}", e)))?;

    Ok(outcome)
}

fn read_and_parse_vault_metadata(vault_json_path: &Path) -> AppResult<VaultMetadata> {
    let content = std::fs::read_to_string(vault_json_path)
        .map_err(|e| AppError::General(format!("Failed to read vault metadata file: {}", e)))?;

    let parsed: VaultMetadata = serde_json::from_str(&content).map_err(|e| {
        AppError::SyncError(format!(
            "Corrupt vault metadata in '{}': {}",
            vault_json_path.display(),
            e
        ))
    })?;

    if parsed.schema_version != VAULT_METADATA_SCHEMA_VERSION {
        return Err(AppError::SyncError(format!(
            "Unsupported vault metadata schema_version {} (expected {}) in '{}'",
            parsed.schema_version,
            VAULT_METADATA_SCHEMA_VERSION,
            vault_json_path.display()
        )));
    }

    Ok(parsed)
}

pub fn load_or_register_vault_identity<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    raw_vault_path: &str,
) -> AppResult<VaultIdentity> {
    if raw_vault_path.trim().is_empty() {
        return Err(AppError::General("vault_path must not be empty".into()));
    }

    let raw_path = Path::new(raw_vault_path);
    let canonical_path = std::fs::canonicalize(raw_path).map_err(|e| {
        AppError::General(format!(
            "Failed to canonicalize vault path '{}': {}",
            raw_vault_path, e
        ))
    })?;

    let synabit_dir = canonical_path.join(".synabit");
    let vault_json_path = synabit_dir.join("vault.json");

    let metadata = if vault_json_path.exists() {
        read_and_parse_vault_metadata(&vault_json_path)?
    } else {
        let new_metadata = VaultMetadata {
            schema_version: VAULT_METADATA_SCHEMA_VERSION,
            vault_id: uuid::Uuid::new_v4(),
        };
        match write_vault_metadata_atomically(&canonical_path, &new_metadata)? {
            VaultMetadataPublishOutcome::Published(published) => published,
            VaultMetadataPublishOutcome::Existing(existing) => existing,
        }
    };

    let canonical_str = canonical_path.to_string_lossy().to_string();
    let vault_id_str = metadata.vault_id.to_string();
    let now = chrono::Utc::now().timestamp_millis();

    let db_state = app_handle.state::<DbState>();
    let mut db = db_state.lock().unwrap_or_else(|e| e.into_inner());

    let record = crate::db::sync_vault::SyncVaultRecord {
        vault_id: vault_id_str,
        canonical_root: canonical_str,
        metadata_version: metadata.schema_version,
        created_at: now,
        updated_at: now,
    };
    db.insert_sync_vault_mapping(&record)?;

    let identity = VaultIdentity {
        vault_id: metadata.vault_id,
        canonical_path,
    };

    crate::db::legacy_sync_migration::migrate_legacy_sync_state_for_vault(&mut db, &identity)?;

    Ok(identity)
}

pub fn get_or_assign_node_id(vault_path: &Path, file_path: &Path) -> AppResult<String> {
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext == "md" {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| AppError::General(format!("Failed to read file for identity: {}", e)))?;

        // Simple regex to extract node_id from frontmatter
        let re = regex::Regex::new(r"(?m)^node_id:\s*([a-zA-Z0-9\-]+)\s*$").unwrap();
        if let Some(caps) = re.captures(&content) {
            if let Some(id_match) = caps.get(1) {
                return Ok(id_match.as_str().to_string());
            }
        }

        // Try fallback to synabit_id just in case
        let re_legacy = regex::Regex::new(r"(?m)^synabit_id:\s*([a-zA-Z0-9\-]+)\s*$").unwrap();
        if let Some(caps) = re_legacy.captures(&content) {
            if let Some(id_match) = caps.get(1) {
                return Ok(id_match.as_str().to_string());
            }
        }

        // No ID found, we need to inject it.
        let new_id = uuid::Uuid::new_v4().to_string();
        let new_content = inject_markdown_id(&content, &new_id);
        std::fs::write(file_path, new_content).map_err(|e| {
            AppError::General(format!(
                "Failed to write injected node_id to markdown: {}",
                e
            ))
        })?;

        Ok(new_id)
    } else if ext == "json" || ext == "canvas" {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| AppError::General(format!("Failed to read file for identity: {}", e)))?;
        let mut json_val: Value = serde_json::from_str(&content)
            .map_err(|e| AppError::General(format!("Failed to parse JSON for identity: {}", e)))?;

        let root_obj = match json_val.as_object_mut() {
            Some(obj) => obj,
            None => {
                return Err(AppError::General(format!(
                    "JSON root is not an object in {}",
                    file_path.display()
                )))
            }
        };

        if let Some(meta) = root_obj.get_mut("metadata") {
            if let Some(node_id) = meta.get("node_id").and_then(|v| v.as_str()) {
                return Ok(node_id.to_string());
            }
            if let Some(meta_obj) = meta.as_object_mut() {
                let new_id = uuid::Uuid::new_v4().to_string();
                meta_obj.insert("node_id".to_string(), Value::String(new_id.clone()));
                std::fs::write(file_path, serde_json::to_string_pretty(&json_val).unwrap())
                    .map_err(|e| {
                        AppError::General(format!(
                            "Failed to write injected node_id to json: {}",
                            e
                        ))
                    })?;
                return Ok(new_id);
            } else {
                return Err(AppError::General(format!(
                    "JSON metadata field is not an object in {}",
                    file_path.display()
                )));
            }
        } else {
            // No metadata object, create it
            let new_id = uuid::Uuid::new_v4().to_string();
            let mut meta_obj = serde_json::Map::new();
            meta_obj.insert("node_id".to_string(), Value::String(new_id.clone()));
            root_obj.insert("metadata".to_string(), Value::Object(meta_obj));
            std::fs::write(file_path, serde_json::to_string_pretty(&json_val).unwrap()).map_err(
                |e| AppError::General(format!("Failed to write injected node_id to json: {}", e)),
            )?;
            return Ok(new_id);
        }
    } else {
        // Assets or unknown files: use relative path as ID for now
        let rel_path =
            crate::path_utils::to_relative(file_path, vault_path.to_string_lossy().as_ref());
        Ok(rel_path)
    }
}

/// Helper to inject `node_id` into Markdown frontmatter
fn inject_markdown_id(content: &str, node_id: &str) -> String {
    if content.starts_with("---\n") || content.starts_with("---\r\n") {
        // Has frontmatter, inject after the first line
        let first_nl = content.find('\n').unwrap() + 1;
        let mut new_content = content.to_string();
        new_content.insert_str(first_nl, &format!("node_id: {}\n", node_id));
        new_content
    } else {
        // No frontmatter, prepend it
        format!("---\nnode_id: {}\n---\n\n{}", node_id, content)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db::DbBridge;
    use tempfile::TempDir;

    fn create_test_app_handle() -> tauri::AppHandle<tauri::test::MockRuntime> {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let handle = app.handle().clone();
        handle.manage(DbState::new(DbBridge::new_in_memory().unwrap()));
        handle
    }

    fn snapshot_sync_vault_rows(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
    ) -> Vec<crate::db::sync_vault::SyncVaultRecord> {
        let db_state = app_handle.state::<DbState>();
        let db_guard = db_state.lock().unwrap();
        let mut stmt = db_guard
            .conn()
            .prepare("SELECT vault_id, canonical_root, metadata_version, created_at, updated_at FROM sync_vaults ORDER BY vault_id ASC")
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok(crate::db::sync_vault::SyncVaultRecord {
                    vault_id: row.get(0)?,
                    canonical_root: row.get(1)?,
                    metadata_version: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    #[test]
    fn vault_metadata_atomic_create_and_reopen_is_stable() {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path();

        let app_handle = create_test_app_handle();

        let id1 =
            load_or_register_vault_identity(&app_handle, vault_path.to_str().unwrap()).unwrap();

        let synabit_dir = id1.canonical_path.join(".synabit");
        let vault_json = synabit_dir.join("vault.json");
        assert!(vault_json.exists());

        // Assert no temp/lock artifacts remain
        let entries: Vec<_> = std::fs::read_dir(&synabit_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["vault.json"]);

        // Reopen vault
        let id2 =
            load_or_register_vault_identity(&app_handle, vault_path.to_str().unwrap()).unwrap();
        assert_eq!(id1.vault_id, id2.vault_id);
        assert_eq!(id1.canonical_path, id2.canonical_path);
    }

    #[test]
    fn corrupt_vault_metadata_is_actionable_and_not_replaced() {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path();
        let synabit_dir = vault_path.join(".synabit");
        std::fs::create_dir_all(&synabit_dir).unwrap();

        let vault_json = synabit_dir.join("vault.json");
        let corrupt_content = "{corrupt_json_content_without_closing_brace";
        std::fs::write(&vault_json, corrupt_content).unwrap();

        let app_handle = create_test_app_handle();

        let res = load_or_register_vault_identity(&app_handle, vault_path.to_str().unwrap());
        assert!(res.is_err());

        // Raw file bytes must remain untouched / corrupted, not replaced with new metadata
        let after_content = std::fs::read_to_string(&vault_json).unwrap();
        assert_eq!(after_content, corrupt_content);
    }

    #[test]
    fn unsupported_vault_metadata_version_is_rejected_and_not_replaced() {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path();
        let synabit_dir = vault_path.join(".synabit");
        std::fs::create_dir_all(&synabit_dir).unwrap();

        let vault_json = synabit_dir.join("vault.json");
        let unsupported_content =
            r#"{"schemaVersion": 99, "vaultId": "00000000-0000-0000-0000-000000000000"}"#;
        std::fs::write(&vault_json, unsupported_content).unwrap();

        let app_handle = create_test_app_handle();

        let res = load_or_register_vault_identity(&app_handle, vault_path.to_str().unwrap());
        assert!(res.is_err());

        let after_content = std::fs::read_to_string(&vault_json).unwrap();
        assert_eq!(after_content, unsupported_content);
    }

    #[test]
    fn canonical_alias_registers_one_vault_mapping() {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path();
        let child_dir = vault_path.join("child");
        std::fs::create_dir_all(&child_dir).unwrap();

        let alias_path = child_dir.join("..");

        let app_handle = create_test_app_handle();

        let id1 =
            load_or_register_vault_identity(&app_handle, vault_path.to_str().unwrap()).unwrap();

        // Call again with syntactically distinct parent-path alias (containing join(".."))
        let id2 =
            load_or_register_vault_identity(&app_handle, alias_path.to_str().unwrap()).unwrap();

        assert_eq!(id1.vault_id, id2.vault_id);
        assert_eq!(id1.canonical_path, id2.canonical_path);

        let db_state = app_handle.state::<DbState>();
        let db_guard = db_state.lock().unwrap();
        let mapping = db_guard
            .get_sync_vault_by_canonical_root(&id1.canonical_path.to_string_lossy())
            .unwrap();
        assert!(mapping.is_some());
        assert_eq!(mapping.unwrap().vault_id, id1.vault_id.to_string());
    }

    #[test]
    fn same_vault_id_for_two_roots_is_rejected_without_db_mutation() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let app_handle = create_test_app_handle();

        // Register vault 1
        let _id1 = load_or_register_vault_identity(&app_handle, temp_dir1.path().to_str().unwrap())
            .unwrap();

        // Copy vault.json from vault 1 to vault 2
        let synabit_dir2 = temp_dir2.path().join(".synabit");
        std::fs::create_dir_all(&synabit_dir2).unwrap();
        let v1_json = temp_dir1.path().join(".synabit").join("vault.json");
        std::fs::copy(&v1_json, synabit_dir2.join("vault.json")).unwrap();

        // Snapshot full DB mapping rows before
        let rows_before = snapshot_sync_vault_rows(&app_handle);
        assert_eq!(rows_before.len(), 1);

        // Try registering vault 2 (same vault_id, different root)
        let res = load_or_register_vault_identity(&app_handle, temp_dir2.path().to_str().unwrap());
        assert!(res.is_err());

        // Snapshot full DB mapping rows after — must be zero mutation
        let rows_after = snapshot_sync_vault_rows(&app_handle);
        assert_eq!(rows_before, rows_after);
    }

    #[test]
    fn concurrent_identity_creation_converges_on_one_published_id() {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path().to_path_buf();
        let app_handle = create_test_app_handle();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let app1 = app_handle.clone();
        let path1 = vault_path.clone();
        let barrier1 = barrier.clone();
        let handle1 = std::thread::spawn(move || {
            barrier1.wait();
            load_or_register_vault_identity(&app1, path1.to_str().unwrap())
        });

        let app2 = app_handle.clone();
        let path2 = vault_path.clone();
        let barrier2 = barrier.clone();
        let handle2 = std::thread::spawn(move || {
            barrier2.wait();
            load_or_register_vault_identity(&app2, path2.to_str().unwrap())
        });

        let res1 = handle1.join().unwrap().unwrap();
        let res2 = handle2.join().unwrap().unwrap();

        // Both competing threads must return the exact same VaultIdentity
        assert_eq!(res1.vault_id, res2.vault_id);
        assert_eq!(res1.canonical_path, res2.canonical_path);

        // Inspect the published metadata file on disk
        let vault_json_path = res1.canonical_path.join(".synabit").join("vault.json");
        let disk_content = std::fs::read_to_string(&vault_json_path).unwrap();
        let disk_metadata: VaultMetadata = serde_json::from_str(&disk_content).unwrap();
        assert_eq!(disk_metadata.vault_id, res1.vault_id);

        // Inspect the registered DB mapping
        let db_state = app_handle.state::<DbState>();
        let db_guard = db_state.lock().unwrap();
        let mapping = db_guard
            .get_sync_vault_by_canonical_root(&res1.canonical_path.to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(mapping.vault_id, res1.vault_id.to_string());
    }

    #[test]
    fn json_node_identity_never_falls_back_to_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let vault_path = temp_dir.path();

        // 1) Primitive JSON root
        let json_prim_path = vault_path.join("primitive.json");
        let before_prim_content = "\"just a string\"";
        std::fs::write(&json_prim_path, before_prim_content).unwrap();

        let res1 = get_or_assign_node_id(vault_path, &json_prim_path);
        assert!(res1.is_err());
        let after_prim_content = std::fs::read_to_string(&json_prim_path).unwrap();
        assert_eq!(before_prim_content, after_prim_content);
        let err1_str = res1.unwrap_err().to_string();
        assert_ne!(err1_str, "primitive.json");

        // 2) Object with scalar metadata
        let json_scalar_meta_path = vault_path.join("scalar_meta.json");
        let before_scalar_content = r#"{"metadata": "invalid_scalar"}"#;
        std::fs::write(&json_scalar_meta_path, before_scalar_content).unwrap();

        let res2 = get_or_assign_node_id(vault_path, &json_scalar_meta_path);
        assert!(res2.is_err());
        let after_scalar_content = std::fs::read_to_string(&json_scalar_meta_path).unwrap();
        assert_eq!(before_scalar_content, after_scalar_content);
        let err2_str = res2.unwrap_err().to_string();
        assert_ne!(err2_str, "scalar_meta.json");
    }
}
