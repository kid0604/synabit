//! The rooms, as the rest of the app reaches them (`syn::machine`).
//!
//! Stopping is here rather than behind the model on purpose: the tray, quitting
//! and the screen all stop rooms without asking anything that could be talked out
//! of it. `app_shell::gate` keeps these to the main window.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::syn::machine::doors::{self, Door, OpenFailed, Protected, Refused};
use crate::syn::machine::rooms::{Rooms, Status};
use crate::syn::machine::selftest::Report;

fn rooms(app: &AppHandle) -> Result<Arc<Rooms>, String> {
    app.try_state::<Arc<Rooms>>()
        .map(|rooms| rooms.inner().clone())
        .ok_or_else(|| "the rooms could not be set up at startup".to_string())
}

/// At startup: finds the rooms, and ends whatever a crash left running in them.
/// On a thread, since no window should wait for it. Runs no attacks: nothing has
/// asked for a room yet, and the feature is off until somebody turns it on.
pub fn setup(app: &AppHandle) {
    let root = match app.path().app_data_dir() {
        Ok(data) => data.join("machine"),
        Err(error) => {
            log::warn!("machine: no data folder: {error}");
            return;
        }
    };
    let rooms = match Rooms::at(&root) {
        Ok(rooms) => Arc::new(rooms),
        Err(error) => {
            log::warn!("machine: could not set up {}: {error}", root.display());
            return;
        }
    };
    app.manage(rooms.clone());
    std::thread::spawn(move || log_stopped("left by the last run", rooms.stop_all()));
}

fn log_stopped(which: &str, stopped: std::io::Result<usize>) {
    match stopped {
        Ok(0) => {}
        Ok(count) => log::info!("machine: stopped {count} processes {which}"),
        Err(error) => log::warn!("machine: could not stop everything {which}: {error}"),
    }
}

/// From the tray, which must not wait.
pub fn stop_all_from_tray(app: &AppHandle) {
    if let Ok(rooms) = rooms(app) {
        std::thread::spawn(move || log_stopped("from the tray", rooms.stop_all()));
    }
}

/// While the app exits, on the way out by whichever door. It takes as long as the
/// kernel takes, a moment and bounded, and what it misses the next launch clears.
pub fn stop_all_on_exit(app: &AppHandle) {
    if let Ok(rooms) = rooms(app) {
        log_stopped("on the way out", rooms.stop_all());
    }
}

#[tauri::command]
pub async fn syn_machine_status(app: AppHandle) -> Result<Status, String> {
    Ok(rooms(&app)?.status().await)
}

/// Attacks the room now. For when the switch is turned on, and for the screen.
#[tauri::command]
pub async fn syn_machine_check(app: AppHandle) -> Result<Report, String> {
    Ok(rooms(&app)?.check().await)
}

/// How many processes that ended.
#[tauri::command]
pub async fn syn_machine_stop_all(app: AppHandle) -> Result<usize, String> {
    let rooms = rooms(&app)?;
    tokio::task::spawn_blocking(move || rooms.stop_all())
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// A door, as the screen lists it.
#[derive(serde::Serialize)]
pub struct DoorView {
    #[serde(flatten)]
    pub door: Door,
    pub expires: chrono::DateTime<chrono::Utc>,
}

/// What came of putting the folder dialog up.
#[derive(serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Picked {
    Opened { door: Door, examples: Vec<String> },
    Refused { why: Refused },
    Cancelled,
}

/// The folder dialog, and the door it opens. The path is the person's answer to
/// a dialog this process put up, never a string from the page; `title` is only
/// what the dialog says, in the screen's language.
async fn pick(app: &AppHandle, vault_path: &str, title: Option<String>) -> Result<Picked, String> {
    let start = app.path().document_dir().ok();
    let Some(picked) = crate::app_shell::dialogs::ask_folder(app, title, start).await.map_err(|e| e.to_string())? else {
        return Ok(Picked::Cancelled);
    };
    let protected = Protected::of(app, vault_path).ok_or("this Mac's folders could not be found")?;
    let vault = vault_path.to_string();
    let opened = tokio::task::spawn_blocking(move || doors::open(&vault, std::path::Path::new(&picked), &protected, chrono::Utc::now()))
        .await
        .map_err(|e| e.to_string())?;
    match opened {
        Ok(opened) => Ok(Picked::Opened { door: opened.door, examples: opened.examples }),
        Err(OpenFailed::Refused(why)) => Ok(Picked::Refused { why }),
        Err(other) => Err(other.to_string()),
    }
}

/// The folders open to the rooms.
#[tauri::command]
pub async fn syn_doors(vault_path: String) -> Result<Vec<DoorView>, String> {
    Ok(doors::all(&vault_path, chrono::Utc::now())
        .into_iter()
        .map(|door| DoorView { expires: door.expires(), door })
        .collect())
}

/// Opens a folder from the screen, before anything has asked for one.
#[tauri::command]
pub async fn syn_add_door(app: AppHandle, vault_path: String, title: Option<String>) -> Result<Picked, String> {
    pick(&app, &vault_path, title).await
}

/// Answers a card asking for a folder by choosing one. Only a folder chosen
/// answers it: a closed dialog, or a folder that cannot be a door, leaves the
/// card where it is. The call that asked takes the door when the work carries
/// on (`doors::take_pick`).
#[tauri::command]
pub async fn syn_pick_door(app: AppHandle, vault_path: String, run_id: String, title: Option<String>) -> Result<Picked, String> {
    let run = crate::syn::run::get_run(&vault_path, &run_id).map_err(|e| e.to_string())?;
    let asking = run.pending_consent.as_ref().is_some_and(|ask| ask.capability == crate::syn::consent::Capability::Door);
    let (true, Some(conversation)) = (asking, run.conversation_id.clone()) else {
        return Err("nothing is asking for a folder".into());
    };
    let picked = pick(&app, &vault_path, title).await?;
    if let Picked::Opened { door, .. } = &picked {
        doors::picked_for(&conversation, &door.id);
        crate::commands::syn::syn_answer_consent(vault_path, run_id, crate::syn::consent::Answer::Once)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(picked)
}

/// Closes a door. A room reading it reads it no more from its next command.
#[tauri::command]
pub async fn syn_remove_door(vault_path: String, id: String) -> Result<bool, String> {
    doors::remove(&vault_path, &id, chrono::Utc::now()).map_err(|e| e.to_string())
}

/// Whether a door may be used from Telegram and in routines too.
#[tauri::command]
pub async fn syn_door_away(vault_path: String, id: String, away: bool) -> Result<bool, String> {
    doors::set_away(&vault_path, &id, away, chrono::Utc::now()).map_err(|e| e.to_string())
}
