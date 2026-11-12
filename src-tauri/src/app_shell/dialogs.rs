//! File dialogs, opened from Rust.
//!
//! A command that writes to "the file the person chose" used to take that
//! file as a string from the webview. Nothing tied the string to a dialog: a
//! script could pass `~/.zshrc` to the diagnostics export and the app would
//! truncate it and write a log over it — `tauri-plugin-fs`'s Rust API does
//! not consult the scope the JavaScript one does.
//!
//! Two shapes, by how much a flow had to move:
//!
//! * **The dialog inside the command.** The vault backup, its restore and the
//!   diagnostics export ask here and never take a path at all (`ask_save`,
//!   `ask_open`). Cancelling returns `None`, which the screen already treated
//!   as "nothing happened".
//! * **A path picked here, handed back once.** `pick_save_path` and
//!   `pick_folder` open the dialog, remember the answer in [`Chosen`], and
//!   return it; the gate lets the named argument of the command that uses it
//!   through only if it is a path remembered here, and forgets it as it does
//!   (`gate::CHOSEN_ARGS`). For the commands owned elsewhere — the calendar,
//!   contacts and table exports, Safe's — so their bodies did not have to move.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder, FilePath};

use crate::error::AppResult;

/// One line in a dialog's "Format" menu.
#[derive(Debug, Clone, Deserialize)]
pub struct Filter {
    pub name: String,
    pub extensions: Vec<String>,
}

impl Filter {
    pub fn new(name: &str, extensions: &[&str]) -> Self {
        Self { name: name.into(), extensions: extensions.iter().map(|e| e.to_string()).collect() }
    }
}

/// Paths the person picked in a dialog this process opened, each good for
/// one use.
///
/// One use, so a path picked for the calendar export cannot be written to a
/// second time by something else. A time limit too, so a dialog answered and
/// then abandoned — the export failed, the screen closed — does not leave a
/// path open for the rest of the session.
pub struct Chosen {
    /// Each pick, when it was made, and the one command it may be spent on,
    /// if it was opened for one.
    picked: Mutex<Vec<(String, Instant, Option<String>)>>,
}

/// Long enough for somebody to pick a file and then type the export's
/// password; not so long it outlives the screen they were on.
const GOOD_FOR: Duration = Duration::from_secs(15 * 60);

impl Chosen {
    pub const fn new() -> Self {
        Self { picked: Mutex::new(Vec::new()) }
    }

    pub fn remember(&self, path: &str) {
        self.remember_for(path, None);
    }

    /// A pick that only `command` may spend: a folder chosen to save results
    /// into must not become a folder Files reads from.
    pub fn remember_for(&self, path: &str, command: Option<&str>) {
        let mut picked = self.picked.lock().unwrap_or_else(|e| e.into_inner());
        picked.retain(|(_, at, _)| at.elapsed() < GOOD_FOR);
        picked.push((path.to_string(), Instant::now(), command.map(str::to_string)));
    }

    /// Whether this exact path was picked and is still good, forgetting it if
    /// so. Compared as the string the dialog returned: the webview hands back
    /// what it was given, and anything else was not picked.
    pub fn take(&self, path: &str) -> bool {
        self.take_for(path, None)
    }

    /// [`Self::take`], by `command`: a pick made for another command does not count.
    pub fn take_for(&self, path: &str, command: Option<&str>) -> bool {
        let mut picked = self.picked.lock().unwrap_or_else(|e| e.into_inner());
        picked.retain(|(_, at, _)| at.elapsed() < GOOD_FOR);
        let fits = |owner: &Option<String>| owner.as_deref().is_none_or(|o| Some(o) == command);
        match picked.iter().position(|(p, _, owner)| p == path && fits(owner)) {
            Some(i) => {
                picked.remove(i);
                true
            }
            None => false,
        }
    }
}

impl Default for Chosen {
    fn default() -> Self {
        Self::new()
    }
}

static CHOSEN: Chosen = Chosen::new();

pub fn chosen() -> &'static Chosen {
    &CHOSEN
}

/// A dialog in front of the app's window, as the JavaScript one would be.
fn dialog<R: tauri::Runtime>(app: &tauri::AppHandle<R>, filters: &[Filter]) -> FileDialogBuilder<R> {
    #[allow(unused_mut)]
    let mut builder = app.dialog().file();
    #[cfg(desktop)]
    if let Some(window) = super::app_window(app) {
        builder = builder.set_parent(&window);
    }
    for filter in filters {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        builder = builder.add_filter(&filter.name, &extensions);
    }
    builder
}

/// Wait for a dialog without holding a thread: the plugin calls back from a
/// thread of its own, and a closed dialog (or one that never opened) is `None`.
async fn answer(open: impl FnOnce(Box<dyn FnOnce(Option<FilePath>) + Send>)) -> Option<FilePath> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    open(Box::new(move |picked| {
        let _ = tx.send(picked);
    }));
    rx.await.ok().flatten()
}

/// Ask where to save a file.
pub async fn ask_save<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    file_name: &str,
    filters: &[Filter],
) -> Option<FilePath> {
    let builder = dialog(app, filters).set_file_name(file_name);
    answer(|done| builder.save_file(done)).await
}

/// Ask for one file to read.
pub async fn ask_open<R: tauri::Runtime>(app: &tauri::AppHandle<R>, filters: &[Filter]) -> Option<FilePath> {
    let builder = dialog(app, filters);
    answer(|done| builder.pick_file(done)).await
}

/// Ask for a folder, as the string the commands below hand back.
#[cfg(desktop)]
pub(crate) async fn ask_folder<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    title: Option<String>,
    start: Option<std::path::PathBuf>,
) -> AppResult<Option<String>> {
    let mut builder = dialog(app, &[]);
    if let Some(title) = title {
        builder = builder.set_title(title);
    }
    if let Some(start) = start {
        builder = builder.set_directory(start);
    }
    Ok(answer(|done| builder.pick_folder(done)).await.map(|picked| picked.to_string()))
}

/// A phone has no folder picker; the plugin does not offer one there.
#[cfg(not(desktop))]
pub(crate) async fn ask_folder<R: tauri::Runtime>(
    _app: &tauri::AppHandle<R>,
    _title: Option<String>,
    _start: Option<std::path::PathBuf>,
) -> AppResult<Option<String>> {
    Err(crate::error::AppError::UnsupportedCapability("There is no folder picker on this device".into()))
}

/// Where to save an export a command will write. The answer is good for one
/// call naming it (see `gate::CHOSEN_ARGS`); `None` when the dialog was closed.
#[tauri::command]
pub async fn pick_save_path(
    app: tauri::AppHandle,
    default_name: String,
    filters: Vec<Filter>,
) -> AppResult<Option<String>> {
    let Some(picked) = ask_save(&app, &default_name, &filters).await else {
        return Ok(None);
    };
    let picked = picked.to_string();
    chosen().remember(&picked);
    Ok(Some(picked))
}

/// A folder, for the command named by `purpose` (by default `add_file_source`)
/// and no other: good for one call of it.
#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle, title: Option<String>, purpose: Option<String>) -> AppResult<Option<String>> {
    // Only the commands that take a folder may be named.
    const FOLDER_COMMANDS: &[&str] = &["add_file_source", "utility_save_all"];
    let purpose = purpose.unwrap_or_else(|| "add_file_source".to_string());
    if !FOLDER_COMMANDS.contains(&purpose.as_str()) {
        return Err(crate::error::AppError::General(format!("no folder is picked for {purpose}")));
    }
    let picked = ask_folder(&app, title, None).await?;
    if let Some(picked) = &picked {
        chosen().remember_for(picked, Some(&purpose));
    }
    Ok(picked)
}

/// The folder to use as the vault, chosen in a dialog and opened on the spot.
///
/// The one way a desktop vault is opened while the app runs: the folder is
/// the person's answer to a dialog this process put up, so a page cannot name
/// one. See `vault` for the others; a phone's is `resolve_mobile_vault_path`.
#[tauri::command]
pub async fn pick_vault_folder(app: tauri::AppHandle, title: Option<String>) -> AppResult<Option<String>> {
    use tauri::Manager;
    let start = app.path().document_dir().ok();
    let picked = ask_folder(&app, title, start).await?;
    if let Some(picked) = &picked {
        super::vault::global().choose(picked)?;
    }
    Ok(picked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picked_path_is_good_once() {
        let chosen = Chosen::new();
        chosen.remember("/Users/a/export.ics");
        assert!(!chosen.take("/Users/a/.zshrc"));
        assert!(chosen.take("/Users/a/export.ics"));
        assert!(!chosen.take("/Users/a/export.ics"), "a second write to the same pick");
    }

    #[test]
    fn a_pick_for_one_command_is_not_another_commands() {
        let chosen = Chosen::new();
        chosen.remember_for("/Users/a", Some("utility_save_all"));
        assert!(!chosen.take_for("/Users/a", Some("add_file_source")));
        assert!(chosen.take_for("/Users/a", Some("utility_save_all")));
        chosen.remember("/Users/b");
        assert!(chosen.take_for("/Users/b", Some("export_contacts")), "a pick for nothing in particular serves any");
    }

    #[test]
    fn only_the_exact_answer_counts() {
        let chosen = Chosen::new();
        chosen.remember("/Users/a/export.ics");
        assert!(!chosen.take("/Users/a/./export.ics"));
        assert!(!chosen.take("/Users/a/export.ics/../.zshrc"));
        assert!(!chosen.take(""));
    }
}
