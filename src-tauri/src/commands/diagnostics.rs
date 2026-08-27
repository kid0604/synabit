//! Getting a log off a user's device.
//!
//! Synabit collects no telemetry, which is a promise worth keeping and also
//! means nothing reports a crash, a failed sync or a quarantined document. The
//! only signal that ever escapes the device is the one a user chooses to send.
//!
//! So the log has to be easy to hand over, and honest about what is in it. It
//! records the relative paths of files involved in sync conflicts and failures,
//! which is to say the names of the user's notes. That is stated in the file
//! itself and in the button that produces it, because somebody deciding whether
//! to attach this to a bug report needs to know before they decide, not after.

use crate::error::{AppError, AppResult};
use std::io::Write;
use std::path::PathBuf;

/// Where the rolling log lives.
fn log_file_path(app_handle: &tauri::AppHandle) -> AppResult<PathBuf> {
    use tauri::Manager;
    let dir = app_handle
        .path()
        .app_log_dir()
        .map_err(|e| AppError::General(format!("could not locate the log directory: {e}")))?;
    Ok(dir.join("Synabit.log"))
}

/// Whether there is anything to send, and how large it is.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticsInfo {
    pub available: bool,
    pub bytes: u64,
}

#[tauri::command]
pub fn diagnostics_info(app_handle: tauri::AppHandle) -> AppResult<DiagnosticsInfo> {
    let path = log_file_path(&app_handle)?;
    Ok(match std::fs::metadata(&path) {
        Ok(meta) => DiagnosticsInfo {
            available: meta.len() > 0,
            bytes: meta.len(),
        },
        Err(_) => DiagnosticsInfo {
            available: false,
            bytes: 0,
        },
    })
}

/// The filename to offer in the save dialog.
#[tauri::command]
pub fn suggested_diagnostics_name() -> String {
    format!(
        "synabit-log-{}.txt",
        chrono::Local::now().format("%Y-%m-%d-%H%M")
    )
}

/// Copy the log to a location the user picked, with a header describing it.
///
/// The header exists for whoever receives the file: it says which build
/// produced it and what it contains, so a report does not arrive as an
/// unlabelled wall of text.
#[tauri::command]
pub async fn export_diagnostics(
    app_handle: tauri::AppHandle,
    destination: String,
) -> AppResult<u64> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = log_file_path(&app_handle)?;
        let mut log = std::fs::File::open(&source).map_err(|e| {
            AppError::General(format!(
                "there is no log to send yet ('{}'): {e}",
                source.display()
            ))
        })?;

        let header = format!(
            "Synabit diagnostics\n\
             version: {}\n\
             platform: {} {}\n\
             generated: {}\n\
             \n\
             This file is the application log. It contains the relative paths of\n\
             files involved in syncing — that is, the names of notes, though not\n\
             their contents. Read it before sharing it.\n\
             {}\n\n",
            app_handle.package_info().version,
            std::env::consts::OS,
            std::env::consts::ARCH,
            chrono::Local::now().to_rfc3339(),
            "-".repeat(60),
        );

        let mut destination_file =
            crate::commands::vault::open_chosen_for_write(&app_handle, &destination)?;
        destination_file.write_all(header.as_bytes())?;
        let copied = std::io::copy(&mut log, &mut destination_file)?;
        destination_file.flush()?;

        Ok::<_, AppError>(copied)
    })
    .await
    .map_err(|e| AppError::General(format!("the export did not finish: {e}")))?
}
