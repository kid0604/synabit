// File system watcher — Desktop only.
// The `notify` crate relies on OS-specific APIs (FSEvents, inotify, ReadDirectoryChangesW)
// that are not available on iOS/Android. On mobile, vault changes are detected
// by re-scanning on app resume instead.

#[cfg(desktop)]
mod desktop {
    use crate::error::{AppError, AppResult};
    use crate::path_utils;
    use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter};

    pub struct WatcherState {
        pub watcher: Mutex<Option<RecommendedWatcher>>,
        /// Shared debounce state — kept alive so we can signal shutdown to the poll thread.
        debounce: Mutex<Option<Arc<Mutex<DebounceState>>>>,
    }

    impl Default for WatcherState {
        fn default() -> Self {
            Self {
                watcher: Mutex::new(None),
                debounce: Mutex::new(None),
            }
        }
    }

    /// Returns true if this path should be ignored by the watcher.
    fn should_ignore(path_str: &str) -> bool {
        path_str.contains(".DS_Store")
            || path_str.contains(".git")
            || path_str.contains(".synabit_sync_manifest.json")
            || path_str.ends_with('~')
            || path_str.contains(".Trash")
            || path_str.contains(".db") // Prevent looping on db writes
    }

    #[tauri::command]
    pub fn start_vault_watcher(
        app_handle: AppHandle,
        state: tauri::State<'_, WatcherState>,
        vault_path: String,
    ) -> AppResult<()> {
        use tauri::Manager;
        let mut watcher_lock = state.watcher.lock().unwrap_or_else(|e| e.into_inner());
        let mut debounce_lock = state.debounce.lock().unwrap_or_else(|e| e.into_inner());

        // Signal the old poll thread to shut down, then drop watcher
        if let Some(old_ds) = debounce_lock.take() {
            if let Ok(mut s) = old_ds.lock() {
                s.shutdown = true;
            }
        }
        *watcher_lock = None;

        let path = PathBuf::from(&vault_path);
        if !path.exists() {
            return Err(AppError::InvalidPath(
                "Vault path does not exist".to_string(),
            ));
        }

        // Update ChatEngineState
        let chat_state: tauri::State<'_, crate::chat_engine::ChatEngineState> = app_handle.state();
        let mut active_vault = chat_state.active_vault_path.lock().unwrap_or_else(|e| e.into_inner());
        *active_vault = Some(vault_path.clone());

        let emit_handle = app_handle.clone();

        // Shared debounce state — ONE instance for the lifetime of this watcher
        let debounce_state = Arc::new(Mutex::new(DebounceState::default()));

        // Spawn ONE polling thread that handles ALL debouncing.
        // It checks every 100ms whether enough quiet-time has elapsed
        // since the last event, and only then emits the Tauri event.
        let poll_ds = debounce_state.clone();
        let poll_handle = emit_handle.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(100));

                let mut s = poll_ds.lock().unwrap_or_else(|e| e.into_inner());

                if s.shutdown {
                    break;
                }

                // Create/Delete debounce — 500ms of quiet time
                if let Some(last) = s.last_create_delete {
                    if !s.fired_create_delete && last.elapsed() >= Duration::from_millis(500) {
                        let payload: Vec<String> = s.created_deleted_paths.drain().collect();
                        let _ = poll_handle.emit("vault-file-created-deleted", payload);
                        s.fired_create_delete = true;
                    }
                }

                // Modify debounce — 2s of quiet time
                if let Some(last) = s.last_modify {
                    if !s.fired_modify && last.elapsed() >= Duration::from_secs(2) {
                        let payload: Vec<String> = s.modified_paths.drain().collect();
                        let _ = poll_handle.emit("vault-file-modified", payload);
                        s.fired_modify = true;
                    }
                }
            }
        });

        let ds = debounce_state.clone();
        let watch_vault_path = vault_path.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    let dominated_by_ignored = event
                        .paths
                        .iter()
                        .all(|p| should_ignore(&p.to_string_lossy()));
                    if dominated_by_ignored {
                        return;
                    }

                    let is_create_delete =
                        matches!(event.kind, EventKind::Create(_) | EventKind::Remove(_));

                    let mut state = ds.lock().unwrap_or_else(|e| e.into_inner());

                    for p in event.paths {
                        if !should_ignore(&p.to_string_lossy()) {
                            let rel_path = path_utils::to_relative(&p, &watch_vault_path);
                            if is_create_delete {
                                state.created_deleted_paths.insert(rel_path);
                            } else {
                                state.modified_paths.insert(rel_path);
                            }
                        }
                    }

                    if is_create_delete {
                        state.last_create_delete = Some(Instant::now());
                        state.fired_create_delete = false;
                    } else {
                        state.last_modify = Some(Instant::now());
                        state.fired_modify = false;
                    }
                }
                Err(e) => {
                    log::error!("Watcher error: {:?}", e);
                }
            })
            .map_err(|e| AppError::General(format!("Failed to initialize watcher: {}", e)))?;

        watcher
            .watch(&path, RecursiveMode::Recursive)
            .map_err(|e| AppError::General(format!("Failed to watch path: {}", e)))?;

        *watcher_lock = Some(watcher);
        *debounce_lock = Some(debounce_state);
        log::info!("File System Watcher started for: {}", vault_path);

        Ok(())
    }

    #[derive(Default)]
    struct DebounceState {
        last_create_delete: Option<Instant>,
        last_modify: Option<Instant>,
        fired_create_delete: bool,
        fired_modify: bool,
        shutdown: bool,
        modified_paths: HashSet<String>,
        created_deleted_paths: HashSet<String>,
    }
}

// Re-export desktop items so existing imports in lib.rs keep working
#[cfg(desktop)]
pub use desktop::*;

// Mobile stub — no-op watcher
#[cfg(not(desktop))]
pub mod mobile_stub {
    use crate::error::AppResult;
    use std::sync::Mutex;

    pub struct WatcherState {
        pub watcher: Mutex<Option<()>>,
    }

    impl Default for WatcherState {
        fn default() -> Self {
            Self {
                watcher: Mutex::new(None),
            }
        }
    }

    #[tauri::command]
    pub fn start_vault_watcher(
        app_handle: tauri::AppHandle,
        _state: tauri::State<'_, WatcherState>,
        vault_path: String,
    ) -> AppResult<()> {
        use tauri::Manager;
        // Update ChatEngineState
        let chat_state: tauri::State<'_, crate::chat_engine::ChatEngineState> = app_handle.state();
        let mut active_vault = chat_state.active_vault_path.lock().unwrap_or_else(|e| e.into_inner());
        *active_vault = Some(vault_path.clone());

        // On mobile, file watching is a no-op.
        // The frontend re-scans on app resume instead.
        Ok(())
    }
}

#[cfg(not(desktop))]
pub use mobile_stub::*;
