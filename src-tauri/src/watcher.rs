use notify_debouncer_mini::{new_debouncer, notify::*};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub struct WatcherState {
    pub debouncer: Mutex<Option<notify_debouncer_mini::Debouncer<RecommendedWatcher>>>,
}

impl Default for WatcherState {
    fn default() -> Self {
        Self {
            debouncer: Mutex::new(None),
        }
    }
}

#[tauri::command]
pub fn start_vault_watcher(
    app_handle: AppHandle,
    state: tauri::State<'_, WatcherState>,
    vault_path: String,
) -> std::result::Result<(), String> {
    let mut db_lock = state.debouncer.lock().unwrap();

    // Drop old debouncer if exists (this halts the previous watching loop)
    *db_lock = None;

    let path = PathBuf::from(&vault_path);
    if !path.exists() {
        return Err("Vault path does not exist".to_string());
    }

    let emit_handle = app_handle.clone();

    // Create a new debouncer with a 2-second timeout
    let mut debouncer = match new_debouncer(
        Duration::from_secs(2),
        move |res: std::result::Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify_debouncer_mini::notify::Error>| {
            match res {
                Ok(events) => {
                    let mut should_emit = false;
                    for event in events {
                        let path_str = event.path.to_string_lossy();
                        // Ignore system hidden files/folders and specific synabit files
                        if path_str.contains(".DS_Store")
                            || path_str.contains(".git")
                            || path_str.contains(".synabit_sync_manifest.json")
                            || path_str.ends_with('~')
                            || path_str.contains(".Trash")
                        {
                            continue;
                        }
                        should_emit = true;
                        break;
                    }

                    if should_emit {
                        let _ = emit_handle.emit("vault-filesystem-changed", ());
                    }
                }
                Err(e) => {
                    eprintln!("Watcher error: {:?}", e);
                }
            }
        },
    ) {
        Ok(d) => d,
        Err(e) => return Err(format!("Failed to initialize watcher: {}", e)),
    };

    // Add path to watcher
    if let Err(e) = debouncer
        .watcher()
        .watch(&path, RecursiveMode::Recursive)
    {
        return Err(format!("Failed to watch path: {}", e));
    }

    *db_lock = Some(debouncer);
    println!("File System Watcher started for: {}", vault_path);

    Ok(())
}
