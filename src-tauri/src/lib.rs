mod gdrive;
pub mod watcher;
pub mod models;
pub mod commands;

use commands::{notes, tasks, events, quickcaps, files, nexus};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(watcher::WatcherState::default())
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
            quickcaps::copy_asset_to_vault,
            // Files
            files::get_file_items,
            files::get_settings,
            files::save_settings,
            files::open_local_file,
            files::update_file_metadata,
            files::reindex_sources,
            // Nexus
            nexus::get_nexus_items,
            nexus::search_nexus,
            nexus::get_nexus_stats,
            // Google Drive
            gdrive::gdrive_auth_start,
            gdrive::gdrive_auth_status,
            gdrive::gdrive_disconnect,
            gdrive::gdrive_sync_full,
            gdrive::gdrive_get_cache_path,
            // Watcher
            watcher::start_vault_watcher,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
