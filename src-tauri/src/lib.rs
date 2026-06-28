pub mod commands;
pub mod db;
pub mod error;
pub mod gdrive;
#[cfg(target_os = "android")]
pub mod jni;
pub mod models;
pub mod path_utils;
pub mod search;
pub mod utils;

pub mod chat_engine;
pub mod watcher;
pub mod crdt_bridge;
pub mod sync;
pub mod p2p;
pub mod secrets;
pub mod feed_engine;
pub mod syn;

use commands::{chat, feeds, files, nexus, nodes, p2p_sync, syn as syn_commands, whiteboards};
use db::DbBridge;

#[tauri::command]
fn open_app_log_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let log_file = log_dir.join("Synabit.log");

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("-R")
        .arg(&log_file)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(&log_file)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    {
        let parent = log_file.parent().unwrap_or(&log_file);
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(watcher::WatcherState::default())
        .setup(|app| {
            use tauri::Manager;
            log::info!("Starting Synabit Backend...");
            let db = DbBridge::init(app.handle()).expect("Failed to initialize database");
            log::info!("Database initialized successfully.");
            app.manage(std::sync::Mutex::new(db));

            // Rebuild FTS5 search index only when needed (schema change or first run)
            {
                let state: tauri::State<'_, db::DbState> = app.state();
                let db = state.lock().unwrap_or_else(|e| e.into_inner());
                let needs_reindex = db
                    .get_kv("fts_needs_reindex")
                    .unwrap_or(None)
                    .map(|v| v == "1")
                    .unwrap_or(false);

                if needs_reindex {
                    let start = std::time::Instant::now();
                    if let Err(e) = db.reindex_search() {
                        log::error!("Failed to build search index: {}", e);
                    } else {
                        let elapsed = start.elapsed().as_millis();
                        log::info!("Search index rebuilt in {}ms.", elapsed);
                        let _ = db.delete_kv("fts_needs_reindex");
                    }
                } else {
                    log::info!("FTS index is up-to-date, skipping rebuild.");
                }
            }

            // Initialize Chat Engine
            app.manage(chat_engine::ChatEngineState::default());
            chat_engine::init_engine(app.handle().clone());

            // App Lock
            app.manage(commands::app_lock::AppLockState::default());

            // P2P Sync
            app.manage(p2p_sync::P2pSyncState::default());
            app.manage(p2p_sync::PairingState::default());
            
            if let Err(e) = crate::p2p::init(app) {
                log::error!("Failed to init P2P module: {}", e);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Nodes (Universal Architecture)
            nodes::scan_all_nodes,
            nodes::scan_specific_nodes,
            nodes::get_all_nodes,
            nodes::get_node,
            nodes::get_nodes,
            nodes::get_linked_nodes,
            nodes::get_node_block,
            nodes::get_node_headings,
            nodes::create_block_reference,
            nodes::update_node_properties,
            nodes::write_node_file,
            nodes::delete_node_file,
            nodes::archive_done_nodes,
            nodes::save_asset,
            nodes::copy_asset_to_vault,
            nodes::rename_node_file,
            nodes::create_node_file,
            nodes::open_daily_note,
            nodes::spawn_node_window,
            nodes::list_pdf_files,
            // Files
            files::add_file_source,
            files::get_file_sources,
            files::remove_file_source,
            files::scan_directory,
            files::query_files,
            files::open_local_file,
            files::update_file_metadata,
            files::reindex_sources,
            files::read_local_file_content,
            files::find_duplicate_files,
            files::export_annotated_pdf,
            files::import_files,
            files::get_file_references,
            files::delete_file,
            // Nexus
            nexus::get_nexus_items,
            nexus::get_nexus_item,
            nexus::get_nexus_graph_data,
            nexus::search_nexus,
            nexus::search_notes,
            nexus::search_tasks,
            nexus::search_files,
            // Tags
            commands::tags::get_all_tags,
            commands::tags::rename_tag,
            commands::tags::delete_tag,
            
            // E2EE
            commands::e2ee::check_e2ee_status,
            commands::e2ee::setup_e2ee,
            commands::e2ee::restore_e2ee_from_phrase,
            commands::e2ee::get_recovery_phrase,



            // Google Drive
            gdrive::auth::gdrive_auth_start,
            gdrive::auth::gdrive_auth_complete,
            gdrive::auth::gdrive_auth_status,
            gdrive::auth::gdrive_disconnect,
            gdrive::sync::gdrive_sync_full,
            gdrive::sync::migrate_gdrive_vault,
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
            // Chat
            chat::get_chat_history,
            chat::mark_chat_read,
            chat::get_unread_notification_count,
            // App Lock
            commands::app_lock::setup_app_lock,
            commands::app_lock::verify_app_lock,
            commands::app_lock::remove_app_lock,
            commands::app_lock::change_app_lock,
            commands::app_lock::get_app_lock_config,
            commands::app_lock::update_app_lock_config,
            // Feeds
            feeds::feed_get_sources,
            feeds::feed_add_source,
            feeds::feed_remove_source,
            feeds::feed_update_source,
            feeds::feed_get_categories,
            feeds::feed_save_categories,
            feeds::feed_get_config,
            feeds::feed_save_config,
            feeds::feed_get_articles,
            feeds::feed_search_articles,
            feeds::feed_get_unread_counts,
            feeds::feed_get_total_unread,
            feeds::feed_mark_read,
            feeds::feed_mark_all_read,
            feeds::feed_toggle_star,
            feeds::feed_toggle_read_later,
            feeds::feed_refresh,
            feeds::feed_discover,
            feeds::feed_fetch_article_content,
            feeds::feed_run_cleanup,
            feeds::feed_import_opml,
            feeds::feed_export_opml,
            feeds::open_url,
            // Syn (Local AI Chat)
            syn_commands::syn_check_status,
            syn_commands::syn_list_models,
            syn_commands::syn_pull_model,
            syn_commands::syn_delete_model,
            syn_commands::syn_send_message,
            syn_commands::syn_stop_generation,
            syn_commands::syn_cancel_pull,
            syn_commands::syn_list_conversations,
            syn_commands::syn_get_conversation,
            syn_commands::syn_create_conversation,
            syn_commands::syn_delete_conversation,
            syn_commands::syn_rename_conversation,
            syn_commands::syn_get_settings,
            syn_commands::syn_save_settings,
            syn_commands::syn_pin_conversation,
            syn_commands::syn_export_conversation,
            // P2P Sync
            p2p_sync::p2p_sync_connect,
            p2p_sync::p2p_sync_full,
            p2p_sync::p2p_sync_disconnect,
            p2p_sync::p2p_sync_status,
            p2p_sync::p2p_sync_metrics,
            p2p_sync::p2p_sync_update_worker_config,
            // P2P Device Pairing
            p2p_sync::p2p_pair_initiate,
            p2p_sync::p2p_pair_cancel,
            p2p_sync::p2p_pair_accept,
            p2p_sync::p2p_list_devices,
            p2p_sync::p2p_remove_device,

            // Key Rotation
            p2p_sync::p2p_current_epoch,
            p2p_sync::p2p_revoke_device,

            // System
            open_app_log_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
