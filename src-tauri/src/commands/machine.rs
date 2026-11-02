//! The rooms, as the rest of the app reaches them (`syn::machine`).
//!
//! Stopping is here rather than behind the model on purpose: the tray, quitting
//! and the screen all stop rooms without asking anything that could be talked out
//! of it. `app_shell::gate` keeps these to the main window.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

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
