pub mod commands;
pub mod db;
pub mod error;
mod gdrive;
pub mod models;
pub mod path_utils;
pub mod utils;
pub mod watcher;

use commands::{events, files, nexus, notes, quickcaps, tasks};
use db::DbBridge;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(watcher::WatcherState::default())
        .setup(|app| {
            use tauri::Manager;
            let db = DbBridge::init(app.handle())
                .expect("Failed to initialize database");
            app.manage(std::sync::Mutex::new(db));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            // Tasks
            tasks::scan_tasks,
            tasks::create_task,
            tasks::update_task,
            tasks::delete_task,
            tasks::archive_done_tasks,
            // Events
            events::scan_events,
            events::create_event,
            events::update_event,
            events::delete_event,
            // QuickCaps
            quickcaps::scan_quick_caps,
            quickcaps::create_quick_cap,
            quickcaps::update_quick_cap,
            quickcaps::delete_quick_cap,
            quickcaps::copy_asset_to_vault,
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
            // Google Drive
            gdrive::auth::gdrive_auth_start,
            gdrive::auth::gdrive_auth_complete,
            gdrive::auth::gdrive_auth_status,
            gdrive::auth::gdrive_disconnect,
            gdrive::sync::gdrive_sync_full,
            gdrive::sync::gdrive_get_cache_path,
            // Watcher
            watcher::start_vault_watcher,
            // GDrive File Manager (OmniDrive — independent auth via Keychain)
            gdrive::browse::is_gdrive_connected,
            gdrive::browse::get_gdrive_user_info,
            gdrive::browse::connect_gdrive,
            gdrive::browse::connect_gdrive_complete,
            gdrive::browse::disconnect_gdrive,
            gdrive::browse::get_gdrive_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
