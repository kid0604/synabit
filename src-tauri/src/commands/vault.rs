//! Where the vault lives on a phone.
//!
//! On desktop the user picks a directory and that is the end of it. Android has
//! no directory picker, so the app chooses, and the choice it made was
//! `app_data_dir()/vault` — internal, app-private storage. Three things follow
//! from that, and all three are bad:
//!
//! * the operating system deletes it when the app is uninstalled;
//! * nothing outside the app can read it, not even over USB;
//! * `adb pull` cannot reach it without root, so neither can support.
//!
//! `document_dir()` on Android resolves to
//! `/storage/emulated/0/Android/data/<package>/files/Documents`, which is a real
//! filesystem path — `std::fs` works on it unchanged — and needs no permission.
//! It is reachable over USB and by `adb pull`.
//!
//! It does *not* survive uninstall either. That problem is the export's to
//! solve, not this module's. What this buys is that the vault stops being
//! invisible.

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

/// The directory the vault is given inside the documents directory.
const VAULT_DIR_NAME: &str = "Synabit";

/// What resolving the mobile vault path had to do to get there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultMigration {
    /// No previous vault existed, or it was empty.
    NothingToMove,
    /// A vault was already at the destination; the old one was left alone.
    DestinationAlreadyExists,
    /// The vault was copied across and the old copy removed.
    Moved { files: usize },
}

/// Move a vault from `legacy` to `target`, or explain why it did not.
///
/// Internal and external storage are different filesystems on Android, so
/// `rename` fails across them with `EXDEV` and the contents have to be copied.
/// That makes this a two-step operation with a window in the middle, which is
/// why the order is: copy everything, check that everything arrived, and only
/// then remove the original. A failure at any point leaves the original intact
/// and the destination removed, so the next launch simply tries again.
///
/// Never overwrites. A vault already at the destination wins, because it is the
/// one the user has been working in.
pub(crate) fn migrate_vault_dir(legacy: &Path, target: &Path) -> AppResult<VaultMigration> {
    if dir_is_missing_or_empty(target).is_none() {
        return Ok(VaultMigration::DestinationAlreadyExists);
    }

    match dir_is_missing_or_empty(legacy) {
        Some(()) => return Ok(VaultMigration::NothingToMove),
        None => {}
    }

    let copied = match copy_dir_recursive(legacy, target) {
        Ok(count) => count,
        Err(e) => {
            // Half a vault at the destination is worse than none: the next run
            // would see it as "already there" and adopt it. Take it back out.
            let _ = std::fs::remove_dir_all(target);
            return Err(e);
        }
    };

    if let Err(e) = verify_copy(legacy, target) {
        let _ = std::fs::remove_dir_all(target);
        return Err(e);
    }

    std::fs::remove_dir_all(legacy).map_err(|e| {
        AppError::General(format!(
            "the vault was copied to '{}' but the old copy at '{}' could not be removed: {e}",
            target.display(),
            legacy.display()
        ))
    })?;

    Ok(VaultMigration::Moved { files: copied })
}

/// `Some(())` when the path holds nothing worth moving.
fn dir_is_missing_or_empty(path: &Path) -> Option<()> {
    match std::fs::read_dir(path) {
        Err(_) => Some(()),
        Ok(mut entries) => {
            if entries.next().is_none() {
                Some(())
            } else {
                None
            }
        }
    }
}

fn copy_dir_recursive(from: &Path, to: &Path) -> AppResult<usize> {
    std::fs::create_dir_all(to)?;
    let mut copied = 0usize;

    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let kind = entry.file_type()?;

        if kind.is_dir() {
            copied += copy_dir_recursive(&source, &destination)?;
        } else if kind.is_file() {
            std::fs::copy(&source, &destination)?;
            copied += 1;
        }
        // Symlinks are skipped deliberately. A vault should not contain them,
        // and following one would copy whatever it points at into the vault.
    }

    Ok(copied)
}

/// Every file under `from` exists under `to` with the same length.
///
/// Not a hash: the point is to catch a copy that stopped part way, which shows
/// up as a missing or short file. Hashing a whole vault to prove a copy that
/// just succeeded would cost more than it tells us.
fn verify_copy(from: &Path, to: &Path) -> AppResult<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        let kind = entry.file_type()?;

        if kind.is_dir() {
            verify_copy(&source, &destination)?;
        } else if kind.is_file() {
            let want = entry.metadata()?.len();
            let got = std::fs::metadata(&destination)
                .map_err(|e| {
                    AppError::General(format!(
                        "'{}' did not survive the move: {e}",
                        source.display()
                    ))
                })?
                .len();
            if want != got {
                return Err(AppError::General(format!(
                    "'{}' arrived incomplete ({got} of {want} bytes)",
                    source.display()
                )));
            }
        }
    }
    Ok(())
}

/// Tell the database the vault is somewhere new.
///
/// This is not optional tidying. `sync_vaults` records one canonical root per
/// vault id and treats a second root for the same id as a copied folder, which
/// it refuses. The vault's identity file travels with it during the move, so
/// without this the very next call to register the vault fails with a mapping
/// collision — and registration is on the path of every scan and every sync,
/// so the app would move the vault successfully and then be unable to use it.
fn rebind_moved_vault(app_handle: &tauri::AppHandle, target: &Path) -> AppResult<()> {
    use tauri::Manager;

    let metadata = crate::sync::core::identity::read_and_parse_vault_metadata(
        &target.join(".synabit").join("vault.json"),
    );

    // A vault that never had an identity file has never been registered either,
    // so there is nothing to rebind and registration will record the new root.
    let metadata = match metadata {
        Ok(m) => m,
        Err(e) => {
            log::info!("the moved vault carries no usable identity yet ({e}); nothing to rebind");
            return Ok(());
        }
    };

    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    let rebound = db.rebind_sync_vault_canonical_root(
        &metadata.vault_id.to_string(),
        &target.to_string_lossy(),
        chrono::Utc::now().timestamp_millis(),
    )?;

    if rebound {
        log::info!(
            "vault {} now points at '{}'",
            metadata.vault_id,
            target.display()
        );
    }
    Ok(())
}

/// Resolve the vault directory for a phone, moving an older one into place.
///
/// Returns the path the frontend should use. Safe to call repeatedly: once the
/// vault is at the destination this only ensures the directory exists.
#[tauri::command]
pub fn resolve_mobile_vault_path(app_handle: tauri::AppHandle) -> AppResult<String> {
    use tauri::Manager;

    let documents = app_handle
        .path()
        .document_dir()
        .map_err(|e| AppError::General(format!("could not locate the documents directory: {e}")))?;
    let target = documents.join(VAULT_DIR_NAME);

    let legacy = app_handle
        .path()
        .app_data_dir()
        .map(|dir| dir.join("vault"))
        .ok();

    if let Some(legacy) = legacy {
        if legacy != target {
            match migrate_vault_dir(&legacy, &target)? {
                VaultMigration::Moved { files } => {
                    log::info!(
                        "moved the vault out of app-private storage to '{}' ({files} files)",
                        target.display()
                    );
                    rebind_moved_vault(&app_handle, &target)?;
                }
                VaultMigration::DestinationAlreadyExists => log::info!(
                    "vault already present at '{}'; the copy in app-private storage was left alone",
                    target.display()
                ),
                VaultMigration::NothingToMove => {}
            }
        }
    }

    std::fs::create_dir_all(&target).map_err(|e| {
        AppError::General(format!(
            "could not create the vault directory '{}': {e}",
            target.display()
        ))
    })?;

    Ok(target.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Backup and restore
// ---------------------------------------------------------------------------

/// Where the archive is staged before it goes anywhere.
///
/// Zip needs to seek while it writes, and reading one needs to seek too. The
/// destination on Android is a file descriptor handed over by whichever app
/// owns the location the user picked, and there is no guarantee it is
/// seekable — a cloud provider may well hand back a pipe. Staging in the app's
/// own cache removes that dependency: the archive is built somewhere known to
/// be a real file, then streamed out with nothing but `Write`.
fn staging_file(app_handle: &tauri::AppHandle, label: &str) -> AppResult<PathBuf> {
    use tauri::Manager;
    let dir = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::General(format!("could not locate the cache directory: {e}")))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{label}-{}.zip", uuid::Uuid::new_v4())))
}

/// Open a destination the user chose.
///
/// `target` is whatever the file dialog returned: an ordinary path on desktop,
/// a `content://` URI on Android. The filesystem plugin resolves both, which is
/// why this needs no Android code of its own.
fn open_chosen(
    app_handle: &tauri::AppHandle,
    target: &str,
    options: tauri_plugin_fs::OpenOptions,
) -> AppResult<std::fs::File> {
    use std::str::FromStr;
    use tauri_plugin_fs::FsExt;

    let path = tauri_plugin_fs::FilePath::from_str(target)
        .map_err(|e| AppError::General(format!("'{target}' is not a usable location: {e}")))?;

    app_handle
        .fs()
        .open(path, options)
        .map_err(|e| AppError::General(format!("could not open '{target}': {e}")))
}

/// Open a user-chosen destination for writing, truncating anything there.
///
/// Shared with the diagnostics export so that both go through the same
/// resolution of a `content://` URI and cannot drift apart in how they treat
/// what a file dialog hands back.
pub(crate) fn open_chosen_for_write(
    app_handle: &tauri::AppHandle,
    target: &str,
) -> AppResult<std::fs::File> {
    open_chosen(
        app_handle,
        target,
        tauri_plugin_fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .to_owned(),
    )
}

/// Pack the vault into a zip at a location the user picked.
#[tauri::command]
pub async fn export_vault_archive(
    app_handle: tauri::AppHandle,
    vault_path: String,
    destination: String,
) -> AppResult<crate::vault_archive::ArchiveSummary> {
    if vault_path.trim().is_empty() {
        return Err(AppError::General("no vault is open".into()));
    }

    tauri::async_runtime::spawn_blocking(move || {
        let staged = staging_file(&app_handle, "export")?;

        let outcome = (|| {
            let mut file = std::fs::File::create(&staged)?;
            let summary = crate::vault_archive::write_vault_zip(Path::new(&vault_path), &mut file)?;
            file.sync_all()?;
            drop(file);

            let mut staged_read = std::fs::File::open(&staged)?;
            let mut destination_file = open_chosen_for_write(&app_handle, &destination)?;
            std::io::copy(&mut staged_read, &mut destination_file)?;
            Ok::<_, AppError>(summary)
        })();

        // The staged copy is a second copy of every note the user has. It does
        // not get to outlive the operation, successful or not.
        let _ = std::fs::remove_file(&staged);
        outcome
    })
    .await
    .map_err(|e| AppError::General(format!("the export did not finish: {e}")))?
}

/// Unpack a zip the user picked into the vault.
#[tauri::command]
pub async fn import_vault_archive(
    app_handle: tauri::AppHandle,
    vault_path: String,
    source: String,
) -> AppResult<crate::vault_archive::RestoreSummary> {
    if vault_path.trim().is_empty() {
        return Err(AppError::General("no vault is open".into()));
    }

    tauri::async_runtime::spawn_blocking(move || {
        let staged = staging_file(&app_handle, "import")?;

        let outcome = (|| {
            let mut chosen = open_chosen(
                &app_handle,
                &source,
                tauri_plugin_fs::OpenOptions::new().read(true).to_owned(),
            )?;
            let mut staged_write = std::fs::File::create(&staged)?;
            std::io::copy(&mut chosen, &mut staged_write)?;
            staged_write.sync_all()?;
            drop(staged_write);

            let staged_read = std::fs::File::open(&staged)?;
            let summary =
                crate::vault_archive::read_vault_zip(staged_read, Path::new(&vault_path))?;

            forget_restored_vault_mapping(&app_handle, &vault_path)?;
            Ok::<_, AppError>(summary)
        })();

        let _ = std::fs::remove_file(&staged);
        outcome
    })
    .await
    .map_err(|e| AppError::General(format!("the restore did not finish: {e}")))?
}

/// Let the restored vault register under the identity it was backed up with.
///
/// The directory was holding a vault created moments ago, with an identity of
/// its own that the archive has just overwritten. Registration would find that
/// stale identity still claiming the directory and refuse — the same mapping
/// collision the move had to deal with, arriving from the other direction.
fn forget_restored_vault_mapping(app_handle: &tauri::AppHandle, vault_path: &str) -> AppResult<()> {
    use tauri::Manager;

    let db_state = app_handle.state::<crate::db::DbState>();
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    if db.forget_sync_vault_at_root(vault_path)? {
        log::info!(
            "discarded the sync state of the vault that was at '{vault_path}' before the restore"
        );
    }
    Ok(())
}

/// The filename to offer in the save dialog.
#[tauri::command]
pub fn suggested_archive_name() -> String {
    crate::vault_archive::suggested_archive_name(chrono::Local::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn a_vault_is_carried_across_with_its_directory_structure() {
        let tmp = TempDir::new().unwrap();
        let legacy = tmp.path().join("legacy");
        let target = tmp.path().join("target");

        write(&legacy, "Notes/plan.md", "# Plan");
        write(&legacy, "Notes/deep/nested.md", "nested");
        write(&legacy, ".synabit/vault.json", "{}");

        let outcome = migrate_vault_dir(&legacy, &target).unwrap();

        assert_eq!(outcome, VaultMigration::Moved { files: 3 });
        assert_eq!(
            std::fs::read_to_string(target.join("Notes/deep/nested.md")).unwrap(),
            "nested"
        );
        assert_eq!(
            std::fs::read_to_string(target.join(".synabit/vault.json")).unwrap(),
            "{}",
            "the vault identity must travel with the vault or it registers as a new one"
        );
        assert!(
            !legacy.exists(),
            "the old copy should be gone once verified"
        );
    }

    /// The destination wins. It is the vault the user has been working in, and
    /// overwriting it with an older copy would lose whatever they wrote there.
    #[test]
    fn an_existing_vault_at_the_destination_is_never_overwritten() {
        let tmp = TempDir::new().unwrap();
        let legacy = tmp.path().join("legacy");
        let target = tmp.path().join("target");

        write(&legacy, "Notes/old.md", "old");
        write(&target, "Notes/current.md", "current");

        let outcome = migrate_vault_dir(&legacy, &target).unwrap();

        assert_eq!(outcome, VaultMigration::DestinationAlreadyExists);
        assert_eq!(
            std::fs::read_to_string(target.join("Notes/current.md")).unwrap(),
            "current"
        );
        assert!(
            legacy.join("Notes/old.md").exists(),
            "the old vault must be left in place rather than deleted"
        );
    }

    #[test]
    fn nothing_to_move_is_not_an_error() {
        let tmp = TempDir::new().unwrap();

        assert_eq!(
            migrate_vault_dir(&tmp.path().join("absent"), &tmp.path().join("target")).unwrap(),
            VaultMigration::NothingToMove
        );

        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(
            migrate_vault_dir(&empty, &tmp.path().join("target2")).unwrap(),
            VaultMigration::NothingToMove
        );
    }

    #[test]
    fn running_the_move_twice_changes_nothing_the_second_time() {
        let tmp = TempDir::new().unwrap();
        let legacy = tmp.path().join("legacy");
        let target = tmp.path().join("target");
        write(&legacy, "Notes/plan.md", "# Plan");

        assert_eq!(
            migrate_vault_dir(&legacy, &target).unwrap(),
            VaultMigration::Moved { files: 1 }
        );
        assert_eq!(
            migrate_vault_dir(&legacy, &target).unwrap(),
            VaultMigration::DestinationAlreadyExists
        );
        assert_eq!(
            std::fs::read_to_string(target.join("Notes/plan.md")).unwrap(),
            "# Plan"
        );
    }

    /// A short file at the destination means the copy stopped part way. The
    /// move must fail loudly and take the partial copy with it, because the
    /// next run would otherwise treat that wreckage as the real vault.
    #[test]
    fn an_incomplete_copy_is_detected_and_the_original_is_kept() {
        let tmp = TempDir::new().unwrap();
        let legacy = tmp.path().join("legacy");
        let target = tmp.path().join("target");
        write(&legacy, "Notes/plan.md", "# Plan");

        // Stand in for a copy that ran out of space: the destination exists but
        // one file is short.
        std::fs::create_dir_all(target.join("Notes")).unwrap();
        std::fs::write(target.join("Notes/plan.md"), "").unwrap();

        let err = verify_copy(&legacy, &target).unwrap_err();

        assert!(
            err.to_string().contains("incomplete"),
            "unhelpful error: {err}"
        );
        assert!(legacy.join("Notes/plan.md").exists());
    }

    /// Following a symlink would copy whatever it points at into the vault.
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_into_the_vault() {
        let tmp = TempDir::new().unwrap();
        let legacy = tmp.path().join("legacy");
        let target = tmp.path().join("target");
        let outside = tmp.path().join("outside.txt");

        write(&legacy, "Notes/plan.md", "# Plan");
        std::fs::write(&outside, "should not be copied").unwrap();
        std::os::unix::fs::symlink(&outside, legacy.join("link.txt")).unwrap();

        let outcome = migrate_vault_dir(&legacy, &target).unwrap();

        assert_eq!(outcome, VaultMigration::Moved { files: 1 });
        assert!(!target.join("link.txt").exists());
    }
}
