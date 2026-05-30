use crate::error::{AppError, AppResult};
use chrono::Local;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[tauri::command]
pub async fn run_crdt_migration(
    app_handle: tauri::AppHandle,
    vault_path: String,
) -> Result<String, String> {
    use tauri::Manager;
    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());

    // 1. Check if migration already done
    if let Ok(Some(val)) = db.get_kv("crdt_migration_done") {
        if val == "true" {
            return Ok("Migration already completed".to_string());
        }
    }

    log::info!("Starting CRDT migration for vault: {}", vault_path);

    // 2. Create Legacy Backup
    let backup_name = format!(
        ".synabit_legacy_backup_{}.zip",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let backup_path = Path::new(&vault_path).join(&backup_name);

    if let Err(e) = create_zip_backup(&vault_path, &backup_path) {
        log::error!("Failed to create backup: {}", e);
        return Err(format!("Backup failed: {}", e));
    }
    log::info!("Created legacy backup at: {:?}", backup_path);

    // 3. Migrate Local .md files to CRDT
    let vault = Path::new(&vault_path);
    let mut migrated_count = 0;

    for entry in WalkDir::new(vault).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" {
                    // Skip hidden files or folders like .synabit_crdt
                    if path.to_string_lossy().contains("/.") || path.to_string_lossy().contains("\\.") {
                        continue;
                    }

                    let rel_path = path
                        .strip_prefix(vault)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .replace('\\', "/");

                    // Check if CRDT exists
                    if let Ok(doc) = db.get_crdt_doc(&rel_path) {
                        let text = doc.get_text("content");
                        if text.to_string().is_empty() {
                            // Empty or new document, inject plain text
                            if let Ok(content) = fs::read_to_string(path) {
                                let _ = text.insert(0, &content);
                                let snapshot = doc.export_snapshot();
                                let _ = db.save_crdt_snapshot(&rel_path, snapshot);
                                migrated_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Mark Migration as Done
    let _ = db.set_kv("crdt_migration_done", "true");
    
    log::info!("Successfully migrated {} files to CRDT.", migrated_count);
    Ok(format!("Migration complete: {} files.", migrated_count))
}

fn create_zip_backup(vault_path: &str, backup_path: &Path) -> AppResult<()> {
    let file = File::create(backup_path)
        .map_err(|e| AppError::General(format!("Cannot create zip file: {}", e)))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let vault = Path::new(vault_path);
    
    for entry in WalkDir::new(vault).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = path
            .strip_prefix(vault)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        // Ignore the backup zip itself and the CRDT folder
        if name.is_empty() || name.starts_with(".synabit_legacy_backup") || name.starts_with(".synabit_crdt") {
            continue;
        }

        if path.is_file() {
            zip.start_file(name, options)
                .map_err(|e| AppError::General(format!("Zip start_file error: {}", e)))?;
            let mut f = File::open(path)
                .map_err(|e| AppError::General(format!("Cannot open file for zip: {}", e)))?;
            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer)
                .map_err(|e| AppError::General(format!("Cannot read file for zip: {}", e)))?;
            zip.write_all(&buffer)
                .map_err(|e| AppError::General(format!("Zip write error: {}", e)))?;
        } else if !name.is_empty() {
            zip.add_directory(name, options)
                .map_err(|e| AppError::General(format!("Zip add_directory error: {}", e)))?;
        }
    }
    
    zip.finish()
        .map_err(|e| AppError::General(format!("Zip finish error: {}", e)))?;
    Ok(())
}
