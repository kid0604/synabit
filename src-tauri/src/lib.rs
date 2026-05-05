pub mod commands;
pub mod db;
pub mod error;
mod gdrive;
pub mod models;
pub mod path_utils;
pub mod search;
pub mod utils;
pub mod watcher;

use commands::{files, nexus, nodes, notes, whiteboards};
use db::DbBridge;

#[tauri::command]
fn open_app_log_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let log_file = log_dir.join("Synabit.log");

    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg("-R").arg(&log_file).spawn().map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg("/select,").arg(&log_file).spawn().map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    {
        let parent = log_file.parent().unwrap_or(&log_file);
        std::process::Command::new("xdg-open").arg(parent).spawn().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_log::Builder::new()
            .level(log::LevelFilter::Info)
            .build())
        .manage(watcher::WatcherState::default())
        .setup(|app| {
            use tauri::Manager;
            log::info!("Starting Synabit Backend...");
            let db = DbBridge::init(app.handle())
                .expect("Failed to initialize database");
            log::info!("Database initialized successfully.");
            app.manage(std::sync::Mutex::new(db));

            // Build FTS5 search index on startup
            {
                let state: tauri::State<'_, db::DbState> = app.state();
                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                if let Err(e) = db.reindex_search() {
                    log::error!("Failed to build search index: {}", e);
                } else {
                    log::info!("Search index built successfully.");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Nodes (Universal Architecture)
            nodes::scan_all_nodes,
            nodes::get_all_nodes,
            nodes::get_nodes,
            nodes::get_linked_nodes,
            nodes::write_node_file,
            nodes::delete_node_file,
            nodes::migrate_events_to_nodes,
            nodes::migrate_tasks_to_nodes,
            nodes::archive_done_nodes,
            nodes::migrate_quickcaps_to_nodes,
            nodes::copy_asset_to_vault,
            // Notes
            notes::scan_vault_path,
            notes::create_new_note,
            notes::read_note,
            notes::update_note,
            notes::delete_note,
            notes::rename_note,
            notes::save_asset,
            notes::spawn_note_window,
            notes::open_daily_note,
            notes::get_note_backlinks,


            // Files
            files::add_file_source,
            files::get_file_sources,
            files::remove_file_source,
            files::scan_directory,
            files::query_files,
            files::get_file_items,
            files::get_settings,
            files::save_settings,
            files::open_local_file,
            files::update_file_metadata,
            files::reindex_sources,
            files::read_local_file_content,
            // Nexus
            nexus::get_nexus_items,
            nexus::get_nexus_item,
            nexus::get_nexus_graph_data,
            nexus::search_nexus,
            nexus::search_notes,
            nexus::search_quickcaps,
            nexus::search_tasks,
            nexus::search_files,
            // Google Drive
            gdrive::auth::gdrive_auth_start,
            gdrive::auth::gdrive_auth_complete,
            gdrive::auth::gdrive_auth_status,
            gdrive::auth::gdrive_disconnect,
            gdrive::sync::gdrive_sync_full,
            gdrive::sync::gdrive_get_cache_path,
            // Watcher
            watcher::start_vault_watcher,
            // Whiteboards
            whiteboards::scan_whiteboards,
            whiteboards::create_whiteboard,
            whiteboards::update_whiteboard,
            whiteboards::delete_whiteboard,
            whiteboards::read_whiteboard,
            // GDrive File Manager (OmniDrive — independent auth via Keychain)
            gdrive::browse::is_gdrive_connected,
            gdrive::browse::get_gdrive_user_info,
            gdrive::browse::connect_gdrive,
            gdrive::browse::connect_gdrive_complete,
            gdrive::browse::disconnect_gdrive,
            gdrive::browse::get_gdrive_files,
            // System
            open_app_log_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
