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

    fn should_ignore(path_str: &str) -> bool {
        path_str.contains(".DS_Store")
            || path_str.contains(".git")
            || path_str.contains(".synabit_sync_manifest.json")
            || path_str.ends_with('~')
            || path_str.contains(".Trash")
            || path_str.ends_with(".tmp") // Prevent looping on atomic_write temp files
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
        let mut active_vault = chat_state
            .active_vault_path
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *active_vault = Some(vault_path.clone());

        // Save to KV store for background P2P Sync
        {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            crate::error::logged(
                "record vault path",
                "vault_path",
                db.set_kv("vault_path", &vault_path),
            );
        }

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

    /// Watching the folders the Files app was pointed at.
    ///
    /// Separate from the vault watcher above, and deliberately so. That one
    /// emits `vault-file-*`, which everything downstream resolves relative to
    /// the vault root; a source folder is somewhere else entirely, so putting
    /// its events on the same channel would have them interpreted as vault
    /// paths and resolved to the wrong files.
    ///
    /// This one says only that a folder changed. What to do about it — rescan
    /// that folder, re-identify what moved — belongs to the Files app, which is
    /// the only thing that knows what the folder is for.
    #[derive(Default)]
    pub struct SourceWatcherState {
        watchers: Mutex<Vec<RecommendedWatcher>>,
    }

    /// Watch every registered source folder, replacing whatever was watched
    /// before.
    ///
    /// Called when the list of sources changes, which is rare, so rebuilding
    /// the whole set is simpler than reconciling it and costs nothing.
    #[tauri::command]
    pub fn watch_file_sources(
        app_handle: AppHandle,
        state: tauri::State<'_, SourceWatcherState>,
        paths: Vec<String>,
    ) -> AppResult<usize> {
        let mut held = state.watchers.lock().unwrap_or_else(|e| e.into_inner());
        held.clear();

        // One debounce shared by every folder: a copy into one of them lands as
        // a burst of events, and re-scanning once after the burst is what the
        // reader wants rather than once per file.
        let quiet = Arc::new(Mutex::new(SourceDebounce::default()));
        let poll = quiet.clone();
        let poll_handle = app_handle.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(200));
            let mut state = poll.lock().unwrap_or_else(|e| e.into_inner());
            if state.shutdown {
                break;
            }
            let Some(last) = state.last else { continue };
            if last.elapsed() < Duration::from_secs(2) {
                continue;
            }
            let changed: Vec<String> = state.folders.drain().collect();
            state.last = None;
            drop(state);
            if !changed.is_empty() {
                let _ = poll_handle.emit("file-source-changed", changed);
            }
        });

        for path in &paths {
            let root = PathBuf::from(path);
            if !root.is_dir() {
                continue;
            }
            let folder = path.clone();
            let debounce = quiet.clone();
            let mut watcher = match notify::recommended_watcher(
                move |res: Result<Event, notify::Error>| {
                    let Ok(event) = res else { return };
                    if event
                        .paths
                        .iter()
                        .all(|p| should_ignore(&p.to_string_lossy()))
                    {
                        return;
                    }
                    if !matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
                    ) {
                        return;
                    }
                    let mut state = debounce.lock().unwrap_or_else(|e| e.into_inner());
                    state.folders.insert(folder.clone());
                    state.last = Some(Instant::now());
                },
            ) {
                Ok(w) => w,
                Err(e) => {
                    log::warn!("cannot watch {path}: {e}");
                    continue;
                }
            };

            if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
                log::warn!("cannot watch {path}: {e}");
                continue;
            }
            held.push(watcher);
        }

        log::info!("watching {} source folder(s)", held.len());
        Ok(held.len())
    }

    #[derive(Default)]
    struct SourceDebounce {
        last: Option<Instant>,
        folders: HashSet<String>,
        shutdown: bool,
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

    /// No-op on mobile, for the reason at the top of this file: `notify` has no
    /// Android or iOS backend. Source folders are re-scanned on resume instead.
    #[derive(Default)]
    pub struct SourceWatcherState;

    #[tauri::command]
    pub fn watch_file_sources(
        _state: tauri::State<'_, SourceWatcherState>,
        _paths: Vec<String>,
    ) -> AppResult<usize> {
        Ok(0)
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
        let mut active_vault = chat_state
            .active_vault_path
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *active_vault = Some(vault_path.clone());

        // Save to KV store for background P2P Sync
        {
            let db_state = app_handle.state::<crate::db::DbState>();
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            crate::error::logged(
                "record vault path",
                "vault_path",
                db.set_kv("vault_path", &vault_path),
            );
        }

        // On mobile, file watching is a no-op: `notify` has no Android or iOS
        // backend. The frontend re-scans when the app returns to the
        // foreground instead — see `rescanOnResume` in App.vue, which is
        // registered on `visibilitychange` and is what actually keeps the
        // index in step with the vault on a phone.
        Ok(())
    }
}

#[cfg(not(desktop))]
pub use mobile_stub::*;
