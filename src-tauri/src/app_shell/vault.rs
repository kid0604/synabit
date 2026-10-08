//! The vault the app has open.
//!
//! Most commands take `vault_path` from the webview, and for a long time that
//! argument *was* the vault: whatever string arrived was read from, written
//! to, and granted to the asset protocol. A script in the window could name
//! `/` and read any file the app could. So the open vault is decided here, by
//! the few things entitled to decide it, and the gate holds every command's
//! `vault_path` to it (see `gate::judge`).
//!
//! # Who may set it
//!
//! * **Startup**, from the path Rust itself recorded the last time a vault was
//!   opened (`kv.vault_path`, written by `start_vault_watcher`). Before the
//!   window can ask for anything: the front end does not wait for the watcher
//!   before reading notes, and a check that refused the first few hundred
//!   milliseconds of every launch would be a bug, not a lock.
//! * **The folder picker** (`dialogs::pick_vault_folder`), which opens the
//!   dialog itself so the path is the person's choice, not the page's.
//! * **The phone**, where `resolve_mobile_vault_path` decides and there is no
//!   picker at all.
//! * **`start_vault_watcher` with nothing open yet**, only for a folder that is
//!   already a vault (see [`ActiveVault::claim`]). That covers a database
//!   started fresh after damage, where the record is gone but the settings
//!   still name the folder — and it cannot be used to open `/`.
//!
//! # Why a static rather than managed state
//!
//! The roots check in `commands::files::allowed_roots` and Syn's tools need it
//! from places that hold a database but no `AppHandle`. One process has one
//! open vault, which is the shape `safe::session::global` already has.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::error::{AppError, AppResult};

/// A vault, as it was named and as the disk resolves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    /// The string the vault was opened with. Every webview names the vault by
    /// this same string, so comparing it first makes the common case a string
    /// comparison rather than a trip to the filesystem.
    pub given: String,
    /// After `..` and links: what a different spelling is checked against.
    pub canonical: PathBuf,
}

impl Open {
    /// Resolve a folder, refusing anything that is not one.
    pub fn resolve(path: &str) -> AppResult<Self> {
        if path.trim().is_empty() {
            return Err(AppError::InvalidPath("No folder was named".into()));
        }
        let canonical = Path::new(path)
            .canonicalize()
            .map_err(|e| AppError::InvalidPath(format!("'{path}' cannot be opened: {e}")))?;
        if !canonical.is_dir() {
            return Err(AppError::InvalidPath(format!("'{path}' is not a folder")));
        }
        Ok(Self { given: path.to_string(), canonical })
    }

    /// Whether a path a caller sent names this vault.
    ///
    /// An empty string never does. `Path::new("").join(rel)` is `rel`, relative
    /// to the process's working directory — `/` for an app bundle — so an
    /// empty vault would be the whole disk again by another route.
    pub fn is(&self, claimed: &str) -> bool {
        if claimed.trim().is_empty() {
            return false;
        }
        if claimed == self.given || Path::new(claimed) == self.canonical {
            return true;
        }
        Path::new(claimed).canonicalize().is_ok_and(|c| c == self.canonical)
    }
}

/// Whether a folder holds a vault already: it carries the `.synabit` folder
/// every vault is given the first time it is opened.
pub fn looks_like_vault(path: &Path) -> bool {
    path.join(".synabit").is_dir()
}

/// The open vault, if there is one.
#[derive(Default)]
pub struct ActiveVault {
    open: RwLock<Option<Open>>,
}

static GLOBAL: ActiveVault = ActiveVault { open: RwLock::new(None) };

/// The process's open vault.
pub fn global() -> &'static ActiveVault {
    &GLOBAL
}

impl ActiveVault {
    pub fn current(&self) -> Option<Open> {
        self.open.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Open a folder because something entitled to choose chose it. See the
    /// module header for the list.
    pub fn choose(&self, path: &str) -> AppResult<Open> {
        let open = Open::resolve(path)?;
        log::info!("[vault] open: {}", open.canonical.display());
        *self.open.write().unwrap_or_else(|e| e.into_inner()) = Some(open.clone());
        Ok(open)
    }

    /// The folder `start_vault_watcher` was asked to watch, accepted only if it
    /// is the one already open — or, with nothing open, one that is already a
    /// vault. Switching to a different folder goes through the picker.
    pub fn claim(&self, path: &str) -> AppResult<Open> {
        let mut slot = self.open.write().unwrap_or_else(|e| e.into_inner());
        match slot.as_ref() {
            Some(open) if open.is(path) => Ok(open.clone()),
            Some(_) => Err(AppError::InvalidPath(NOT_OPEN.into())),
            None => {
                let open = Open::resolve(path)?;
                if !looks_like_vault(&open.canonical) {
                    return Err(AppError::InvalidPath(NOT_OPEN.into()));
                }
                log::info!("[vault] adopted at start: {}", open.canonical.display());
                *slot = Some(open.clone());
                Ok(open)
            }
        }
    }
}

/// What a call naming some other folder is told.
pub const NOT_OPEN: &str = "That folder is not the vault this app has open. Choose it again from the welcome screen.";

/// Where the front end keeps its settings, read for the vault it last had
/// open when the database has no record of one. A file read rather than the
/// store plugin: loading the store here would make it Rust's, and the front
/// end's own `load` of it would then come back with different options.
pub fn remembered_by_front_end(app_data_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(app_data_dir.join("settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    json.get("vaultPath")?.as_str().filter(|s| !s.trim().is_empty()).map(str::to_string)
}

/// Open at startup what was open last time. The database's record first,
/// since only Rust writes it; the settings file only for a folder that is
/// already a vault, since a page can write that file.
pub fn restore(recorded: Option<String>, remembered: Option<String>) -> Option<Open> {
    if let Some(path) = recorded {
        match global().choose(&path) {
            Ok(open) => return Some(open),
            Err(e) => log::warn!("[vault] the last vault is not here now: {e}"),
        }
    }
    let path = remembered?;
    match global().claim(&path) {
        Ok(open) => Some(open),
        Err(e) => {
            log::warn!("[vault] the folder the settings name was not opened: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn vault() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".synabit")).unwrap();
        dir
    }

    #[test]
    fn a_vault_is_the_same_vault_however_it_is_spelt() {
        let dir = vault();
        std::fs::create_dir_all(dir.path().join("Notes")).unwrap();
        let open = Open::resolve(&dir.path().to_string_lossy()).unwrap();

        assert!(open.is(&dir.path().to_string_lossy()));
        assert!(open.is(&format!("{}/", dir.path().display())));
        assert!(open.is(&dir.path().join("Notes").join("..").to_string_lossy()));
        assert!(!open.is(&dir.path().join("Notes").to_string_lossy()));
        assert!(!open.is("/"));
    }

    /// The empty string is the working directory, which for a bundle is `/`.
    #[test]
    fn no_name_is_not_the_vault() {
        let dir = vault();
        let open = Open::resolve(&dir.path().to_string_lossy()).unwrap();
        assert!(!open.is(""));
        assert!(!open.is("  "));
        assert!(Open::resolve("").is_err());
    }

    #[test]
    fn the_watcher_cannot_move_the_vault() {
        let first = vault();
        let other = vault();
        let active = ActiveVault::default();
        active.choose(&first.path().to_string_lossy()).unwrap();

        assert!(active.claim(&first.path().to_string_lossy()).is_ok());
        assert!(active.claim(&other.path().to_string_lossy()).is_err());
        assert!(active.claim("/").is_err());
        assert_eq!(active.current().unwrap().given, first.path().to_string_lossy());
    }

    /// With nothing open, only a folder that is already a vault is adopted —
    /// so a fresh database does not leave `/` there for the taking.
    #[test]
    fn with_nothing_open_only_a_vault_is_adopted() {
        let active = ActiveVault::default();
        let plain = TempDir::new().unwrap();
        assert!(active.claim(&plain.path().to_string_lossy()).is_err());
        assert!(active.claim("/").is_err());
        assert!(active.current().is_none());

        let real = vault();
        assert!(active.claim(&real.path().to_string_lossy()).is_ok());
        assert!(active.current().is_some());
    }

    #[test]
    fn the_settings_file_is_read_for_the_folder_and_nothing_else() {
        let dir = TempDir::new().unwrap();
        assert_eq!(remembered_by_front_end(dir.path()), None);
        std::fs::write(dir.path().join("settings.json"), r#"{"vaultPath":"/x/v","themeMode":"dark"}"#).unwrap();
        assert_eq!(remembered_by_front_end(dir.path()).as_deref(), Some("/x/v"));
        std::fs::write(dir.path().join("settings.json"), r#"{"vaultPath":""}"#).unwrap();
        assert_eq!(remembered_by_front_end(dir.path()), None);
    }
}
