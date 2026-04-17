use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub struct WatcherState {
    pub watcher: Mutex<Option<RecommendedWatcher>>,
}

impl Default for WatcherState {
    fn default() -> Self {
        Self {
            watcher: Mutex::new(None),
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
}

#[tauri::command]
pub fn start_vault_watcher(
    app_handle: AppHandle,
    state: tauri::State<'_, WatcherState>,
    vault_path: String,
) -> std::result::Result<(), String> {
    let mut watcher_lock = state.watcher.lock().unwrap();

    // Drop old watcher if exists
    *watcher_lock = None;

    let path = PathBuf::from(&vault_path);
    if !path.exists() {
        return Err("Vault path does not exist".to_string());
    }

    let emit_handle = app_handle.clone();

    // Manual debounce state shared across events
    // We track two separate debounce timers: one for create/delete, one for modify
    let debounce_state = std::sync::Arc::new(Mutex::new(DebounceState::default()));

    let ds = debounce_state.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        match res {
            Ok(event) => {
                // Check if any path in this event is relevant
                let dominated_by_ignored = event.paths.iter().all(|p| {
                    should_ignore(&p.to_string_lossy())
                });
                if dominated_by_ignored {
                    return;
                }

                let is_create_delete = matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Remove(_)
                );

                let mut state = ds.lock().unwrap();

                if is_create_delete {
                    // Create/Delete → debounce 500ms then emit immediate sync event
                    state.last_create_delete = Some(Instant::now());
                    let handle = emit_handle.clone();
                    let ds_inner = ds.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(500));
                        let s = ds_inner.lock().unwrap();
                        // Only fire if no newer create/delete came in during debounce
                        if let Some(last) = s.last_create_delete {
                            if last.elapsed() >= Duration::from_millis(450) {
                                let _ = handle.emit("vault-file-created-deleted", ());
                            }
                        }
                    });
                } else {
                    // Modify → debounce 2s then emit UI-refresh-only event
                    state.last_modify = Some(Instant::now());
                    let handle = emit_handle.clone();
                    let ds_inner = ds.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(2));
                        let s = ds_inner.lock().unwrap();
                        if let Some(last) = s.last_modify {
                            if last.elapsed() >= Duration::from_millis(1900) {
                                let _ = handle.emit("vault-file-modified", ());
                            }
                        }
                    });
                }
            }
            Err(e) => {
                eprintln!("Watcher error: {:?}", e);
            }
        }
    })
    .map_err(|e| format!("Failed to initialize watcher: {}", e))?;

    watcher
        .watch(&path, RecursiveMode::Recursive)
        .map_err(|e| format!("Failed to watch path: {}", e))?;

    *watcher_lock = Some(watcher);
    println!("File System Watcher started for: {}", vault_path);

    Ok(())
}

#[derive(Default)]
struct DebounceState {
    last_create_delete: Option<Instant>,
    last_modify: Option<Instant>,
}
