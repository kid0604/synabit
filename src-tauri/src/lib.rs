pub mod calendar;
pub mod commands;
pub mod db;
pub mod error;
pub mod gdrive;
pub mod models;
pub mod people;
pub mod path_utils;
pub mod search;
pub mod search_fold;
pub mod utils;

pub mod chat_engine;
pub mod feed_engine;
pub mod file_index;
pub mod file_text;
pub mod file_providers;
pub mod secrets;
pub mod syn;
pub mod sync;
pub mod vault_archive;
pub mod watcher;

pub mod hwid;
pub mod license;
pub mod signing;

use commands::{
    calendar_subs, capture, chat, feeds, files, finance, license_cmds, migration, nexus, nodes, people as people_commands,
    syn as syn_commands,
    sync as sync_cmds, thumbnails, trash, vault_health, versions, whiteboards,
};
use db::DbBridge;

/// The system-wide shortcut that opens the compose box.
///
/// This is the desktop half of what a share sheet and a launcher shortcut do
/// on a phone: the point of a capture inbox is the distance between having a
/// thought and having it written down, and on a desktop that distance is
/// measured in whether you had to leave what you were doing.
///
/// It fires the same `quickcap/compose` deep link the Android launcher
/// shortcut does, so there is one path to "let me write something" rather
/// than one per surface.
#[cfg(desktop)]
fn register_capture_hotkey(app: &tauri::AppHandle) {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

    // Cmd/Ctrl+Shift+Space. Chosen to sit clear of the obvious neighbours —
    // Spotlight owns Cmd+Space, and input-source switching owns Ctrl+Space.
    let shortcut = Shortcut::new(Some(Modifiers::SHIFT | Modifiers::SUPER), Code::Space);

    let handler_app = app.clone();
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(move |_app, _shortcut, event| {
            // Key-down only. Without this the window is raised twice per press.
            if event.state() != ShortcutState::Pressed {
                return;
            }
            surface_quick_entry(&handler_app);
        })
        .build();

    if let Err(e) = app.plugin(plugin) {
        log::error!("global shortcut plugin failed to start: {e}");
        return;
    }

    // Another app may already own the combination. That is a normal thing for
    // a shortcut to lose, and no reason to fail startup — every other way into
    // QuickCap still works.
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    match app.global_shortcut().register(shortcut) {
        Ok(()) => log::info!("capture hotkey registered: Cmd/Ctrl+Shift+Space"),
        Err(e) => log::warn!("capture hotkey unavailable, most likely taken by another app: {e}"),
    }
}

/// The window the hotkey opens: a box over the user's work, not the app.
///
/// Built hidden at startup and shown on demand, because a window created at
/// the moment the hotkey fires would have to load a webview first — and a
/// capture box that takes a second to appear is one people stop reaching for.
/// Kept alive afterwards for the same reason.
#[cfg(desktop)]
fn build_quick_entry_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    WebviewWindowBuilder::new(
        app,
        "quick-entry",
        WebviewUrl::App("index.html#/quick-entry".into()),
    )
    .title("Synabit — quick capture")
    .inner_size(620.0, 148.0)
    // No title bar: this is a panel, and a close button on it would only
    // offer a worse version of Escape.
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .center()
    .build()?;

    Ok(())
}

/// Put the capture box in front of whatever the user is doing.
///
/// Centred on every appearance rather than once: the window outlives the
/// screen arrangement, and a panel that opens on a monitor that is no longer
/// there is indistinguishable from a hotkey that does not work.
#[cfg(desktop)]
fn surface_quick_entry(app: &tauri::AppHandle) {
    use tauri::Manager;
    #[cfg(target_os = "macos")]
    {
        let _ = app.show();
    }
    let Some(window) = app.get_webview_window("quick-entry") else {
        // The panel failed to build at startup; the main window is a worse
        // answer than nothing at all.
        surface_main_window(app);
        return;
    };
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Bring the main window to the front, wherever it was.
///
/// Shared by the hotkey and the tray, because "make Synabit visible" has more
/// than one wrong way to do it: a window minimised to the dock stays invisible
/// if it is only asked to show, and one that is merely behind another needs
/// focus rather than showing at all.
#[cfg(desktop)]
fn surface_main_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    use tauri::Manager;
    // Closing hides the whole application on macOS, so bringing one window
    // forward starts by bringing the application back.
    #[cfg(target_os = "macos")]
    {
        let _ = app.show();
    }
    let window = app.get_webview_window("main")?;
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    Some(window)
}

/// The tray menu's items, kept so their text can be translated once the front
/// end knows which language the user reads.
///
/// The menu is drawn by the operating system, so it is the one piece of the
/// app's own wording that its i18n cannot reach — the same gap the Android
/// text-selection label had, and worth closing for the same reason.
#[cfg(desktop)]
pub struct TrayLabels {
    pub capture: tauri::menu::MenuItem<tauri::Wry>,
    pub open: tauri::menu::MenuItem<tauri::Wry>,
    pub quit: tauri::menu::MenuItem<tauri::Wry>,
}

/// Translate the tray menu. Called by the front end at startup and whenever
/// the language changes.
#[cfg(desktop)]
#[tauri::command]
fn set_tray_labels(
    state: tauri::State<'_, TrayLabels>,
    capture: String,
    open: String,
    quit: String,
) -> Result<(), String> {
    state.capture.set_text(capture).map_err(|e| e.to_string())?;
    state.open.set_text(open).map_err(|e| e.to_string())?;
    state.quit.set_text(quit).map_err(|e| e.to_string())?;
    Ok(())
}

/// QuickCap's lightning bolt, drawn rather than loaded.
///
/// A menu-bar icon has to be a template: macOS reads only its alpha channel
/// and paints the result to match the bar. That rules out the app icon, an
/// opaque square which becomes a solid block, and it rules out reusing any of
/// `icons/` for the same reason.
///
/// Rasterising the shape here keeps it to one function with the outline
/// visible in it, instead of five PNGs nobody can edit — the same call as the
/// Android shortcut, which is a vector for the same reason. The outline is
/// lucide's `zap`, so the tray matches the icon QuickCap uses inside the app.
#[cfg(desktop)]
fn bolt_icon(size: u32) -> tauri::image::Image<'static> {
    // lucide `zap`, in its own 24×24 box.
    const OUTLINE: [(f32, f32); 6] = [
        (13.0, 2.0),
        (3.0, 14.0),
        (12.0, 14.0),
        (11.0, 22.0),
        (21.0, 10.0),
        (12.0, 10.0),
    ];
    const BOX: f32 = 24.0;
    /// Samples per axis. Edges of a bolt are all diagonal, and without this
    /// they come out as visible stairs at menu-bar size.
    const SAMPLES: u32 = 4;

    let inside = |x: f32, y: f32| {
        let mut hit = false;
        let mut j = OUTLINE.len() - 1;
        for i in 0..OUTLINE.len() {
            let (xi, yi) = OUTLINE[i];
            let (xj, yj) = OUTLINE[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                hit = !hit;
            }
            j = i;
        }
        hit
    };

    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let scale = BOX / size as f32;

    for py in 0..size {
        for px in 0..size {
            let mut covered = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) * scale;
                    if inside(x, y) {
                        covered += 1;
                    }
                }
            }
            let alpha = (covered * 255 / (SAMPLES * SAMPLES)) as u8;
            // Black where it is opaque. Under a template the colour is
            // discarded, and off a template a dark glyph is still the right
            // answer for a light menu bar.
            rgba.extend_from_slice(&[0, 0, 0, alpha]);
        }
    }

    tauri::image::Image::new_owned(rgba, size, size)
}

/// The menu-bar icon, and the reason closing the window no longer quits.
///
/// A global hotkey only exists while the process does. Without somewhere for
/// the app to live after its window is closed, the shortcut works exactly
/// during the period when the user already has the compose box in front of
/// them — and stops working the moment it would start being useful.
#[cfg(desktop)]
fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::TrayIconBuilder;
    use tauri::Manager;

    // English to begin with; `set_tray_labels` replaces these as soon as the
    // front end has loaded the user's language.
    let capture = MenuItem::with_id(app, "capture", "New quick cap", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Synabit", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Synabit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&capture, &open, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    app.manage(TrayLabels {
        capture: capture.clone(),
        open: open.clone(),
        quit: quit.clone(),
    });

    TrayIconBuilder::with_id("main")
        .icon(bolt_icon(44))
        // macOS builds a menu-bar icon out of the alpha channel when it is a
        // template, which is what lets one image read correctly against both a
        // light and a dark menu bar. It is also why the app icon cannot be
        // used here: it is an opaque square, so as a template it renders as a
        // filled white block — which is exactly what appeared.
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Synabit")
        .menu(&menu)
        // The menu is the whole point of the icon on Windows and Linux, where
        // a left click would otherwise do nothing discoverable.
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "capture" => surface_quick_entry(app),
            "open" => {
                surface_main_window(app);
            }
            // The only way out, now that closing the window hides it. Without
            // this the app would be genuinely impossible to quit.
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    log::info!("tray icon created");
    Ok(())
}

/// Put Synabit out of the way without stopping it.
///
/// The app has to outlive its window for the global hotkey to mean anything,
/// otherwise the shortcut works only while the compose box is already on
/// screen. The tray's Quit is the way out.
///
/// macOS hides the *application*, not the window, and the difference decides
/// whether the Dock icon works: with only the window hidden, the app keeps an
/// icon that does nothing, because the one thing that would restore it —
/// `applicationShouldHandleReopen` — never arrives. Hiding the application
/// makes the Dock click AppKit's business, and AppKit has always known how to
/// unhide an app.
#[cfg(desktop)]
#[tauri::command]
fn hide_to_background(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.hide();
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }
}

#[tauri::command]
fn open_app_log_folder(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let log_file = log_dir.join("Synabit.log");

    // Android has no file manager to hand a path to. Every arm below is a
    // desktop `cfg`, so this used to fall through all of them and return
    // `Ok(())` having done nothing at all — the caller was told the folder had
    // been opened. Saying so is more useful than a silent success.
    #[cfg(target_os = "android")]
    {
        let _ = log_file;
        return Err(format!(
            "Opening the log folder is a desktop feature. The log is at {}",
            log_dir.display()
        ));
    }

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

    // Unreachable on Android, where the arm above returns. Reachable on every
    // other target, where each arm falls through to it.
    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn init_rustls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_deep_link::init())
        // Desktop only: a phone has no login items, and the capture surfaces
        // there survive a reboot on their own.
        //
        // Registering the plugin does not switch anything on. Nothing starts
        // with the machine until the user asks for it in Settings, which is
        // the only defensible default — an app that adds itself to login
        // items unasked is one people uninstall.
        .plugin({
            #[cfg(desktop)]
            {
                tauri_plugin_autostart::init(
                    tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                    None,
                )
            }
            #[cfg(not(desktop))]
            {
                tauri_plugin_os::init()
            }
        })
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(10_000_000) // 10 MB
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .filter(|metadata| {
                    // Filter out noisy iroh transport logs that flood the file
                    let target = metadata.target();
                    !target.starts_with("iroh::socket::transports")
                        && !target.starts_with("iroh::socket::remote_map")
                        && !target.starts_with("tracing::span")
                })
                .build(),
        )
        .manage(watcher::WatcherState::default())
        .manage(commands::files::ScanControl::default())
        .manage(watcher::SourceWatcherState::default())
        .manage(feeds::FeedSchedulerState::default())
        .on_window_event(|window, event| {
            // Closing hides. The app has to outlive its window for the global
            // hotkey to mean anything — otherwise the shortcut stops working
            // at exactly the moment it starts being useful, which is when the
            // user is doing something else.
            //
            // The tray's Quit is the way out, and it is the only one: without
            // a tray this would make the app impossible to close, so the two
            // changes belong together.
            // A fallback only. The front end registers its own
            // `onCloseRequested`, and once it does, Tauri hands the decision to
            // JavaScript and stops delivering this event here — which is why
            // the close kept going through despite `prevent_close`. The real
            // handling lives in `App.vue`; this covers the window closing
            // before the front end has loaded.
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();

                    // macOS hides the *application*, not the window, and the
                    // difference is the whole bug: with the window hidden the
                    // app keeps a Dock icon that does nothing, because the
                    // only thing that would restore it is
                    // `applicationShouldHandleReopen` — which does not arrive.
                    //
                    // Hiding the app instead makes clicking the Dock icon
                    // AppKit's business, and AppKit has always known how to
                    // unhide an application. Nothing has to be listened for.
                    #[cfg(target_os = "macos")]
                    {
                        use tauri::Manager;
                        let _ = window.app_handle().hide();
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        let _ = window.hide();
                    }
                }
            }
            #[cfg(not(desktop))]
            {
                let _ = (window, event);
            }
        })
        .setup(|app| {
            use tauri::Manager;
            log::info!("Starting Synabit Backend...");
            let db = DbBridge::init(app.handle()).expect("Failed to initialize database");
            log::info!("Database initialized successfully.");
            app.manage(std::sync::Mutex::new(db));

            #[cfg(desktop)]
            {
                if let Err(e) = build_quick_entry_window(app.handle()) {
                    log::error!("quick capture window unavailable: {e}");
                }
                if let Err(e) = setup_tray(app.handle()) {
                    log::error!("tray icon unavailable: {e}");
                }
                register_capture_hotkey(app.handle());
            }

            // Every way into QuickCap from outside the app arrives here, as a
            // `synabit://quickcap/new?text=…` URL. Queueing is all this does:
            // the vault may be locked, unchosen, or not loaded yet, and a
            // capture that fails because of any of those teaches the user the
            // fast path cannot be trusted. See `commands::capture`.
            {
                use tauri::Emitter;
                use tauri_plugin_deep_link::DeepLinkExt;
                let capture_handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        let Some(input) = commands::capture::capture_from_url(url.as_str()) else {
                            continue;
                        };
                        let state: tauri::State<'_, db::DbState> = capture_handle.state();
                        let db = state.lock().unwrap_or_else(|e| e.into_inner());
                        match commands::capture::enqueue(&db, &input) {
                            Ok(id) => {
                                log::info!(
                                    "capture queued from {}: {}",
                                    input.source.as_deref().unwrap_or("a deep link"),
                                    id
                                );
                                // Tell the window, in case one is already open
                                // with a vault: the capture should land now
                                // rather than waiting for the next launch.
                                drop(db);
                                let _ = capture_handle.emit("capture-queued", ());
                            }
                            Err(e) => log::error!("could not queue a capture: {e}"),
                        }
                    }
                });
            }

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
                        crate::error::logged(
                            "clear reindex flag",
                            "fts_needs_reindex",
                            db.delete_kv("fts_needs_reindex"),
                        );
                    }
                } else {
                    log::info!("FTS index is up-to-date, skipping rebuild.");
                }
            }

            // Initialize Chat Engine
            app.manage(chat_engine::ChatEngineState::default());
            chat_engine::init_engine(app.handle().clone());
            // On a phone the loop above barely runs: the system stops the app
            // moments after the screen goes off. The week ahead is handed to
            // the system's own scheduler instead, and kept in step with the
            // vault. On a desktop this does nothing — see `scheduler`.
            calendar::scheduler::watch(app.handle().clone());

            // App Lock
            app.manage(commands::app_lock::AppLockState::default());

            // P2P Sync
            app.manage(sync_cmds::P2pSyncState::default());

            // Not on Android: that build is free and has no licence to renew,
            // so the heartbeat would be a network call home on every launch for
            // no reason — and one a Data Safety declaration would have to
            // account for.
            #[cfg(all(feature = "official-build", not(target_os = "android")))]
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    log::info!("Running background license heartbeat check...");
                    // Try heartbeat. We ignore errors since it could be offline.
                    // If revoked, the command itself will delete the local license file.
                    let _ = commands::license_cmds::heartbeat_license(app_handle).await;
                });
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
            nodes::get_node_summaries,
            nodes::get_events_in_range,
            nodes::get_event_series,
            nodes::convert_event_time,
            nodes::export_calendar_ics,
            nodes::read_calendar_ics,
            people_commands::read_contact_columns,
            people_commands::read_contacts,
            people_commands::find_contact_duplicates,
            people_commands::merge_contact,
            people_commands::last_contact_dates,
            people_commands::person_connections,
            people_commands::person_interactions,
            people_commands::person_brief,
            people_commands::path_between_people,
            people_commands::migrate_people_storage,
            people_commands::export_contacts,
            nodes::match_event_uids,
            nodes::search_event_occurrences,
            nodes::get_tasks_in_range,
            calendar_subs::list_calendar_subscriptions,
            calendar_subs::add_calendar_subscription,
            calendar_subs::set_calendar_subscription_enabled,
            calendar_subs::set_calendar_subscription_remind,
            calendar_subs::rename_calendar_subscription,
            calendar_subs::remove_calendar_subscription,
            calendar_subs::refresh_calendar_subscription,
            calendar_subs::refresh_calendar_subscriptions,
            nodes::count_nodes,
            nodes::count_inbox_caps,
            nodes::get_linked_nodes,
            nodes::get_node_block,
            nodes::get_node_headings,
            nodes::create_block_reference,
            nodes::update_file_node_properties,
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
            files::cancel_file_scan,
            files::preview_file_identity_migration,
            files::extract_file_text,
            files::file_text_backlog,
            files::find_text_page,
            files::bulk_tag_files,
            files::record_photo_facts,
            files::list_cameras,
            files::export_highlights_to_note,
            files::query_file_page,
            files::query_file_ids,
            files::file_tag_counts,
            watcher::watch_file_sources,
            files::set_file_label,
            files::reveal_in_file_manager,
            files::save_file_collection,
            files::list_file_collections,
            files::delete_file_collection,
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
            nexus::run_node_query,
            nexus::search_notes,
            nexus::search_tasks,
            nexus::search_files,
            nexus::search_quickcaps,
            migration::apply_silent_migration,
            migration::migrate_quickcap_storage,
            migration::migrate_finance_storage,
            finance::upsert_finance_rows,
            trash::trash_node_file,
            trash::purge_trash,
            trash::list_trash,
            trash::restore_from_trash,
            trash::delete_trash_entry,
            vault_health::find_duplicate_notes,
            versions::list_node_versions,
            versions::read_node_version,
            versions::diff_node_version,
            versions::restore_node_version,
            thumbnails::save_thumbnail,
            thumbnails::list_thumbnails,
            capture::queue_capture,
            capture::import_handoff_captures,
            #[cfg(desktop)]
            set_tray_labels,
            #[cfg(desktop)]
            hide_to_background,
            capture::list_queued_captures,
            capture::drop_queued_capture,
            migration::get_migration_flag,
            migration::set_migration_flag,
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
            // Vault location (mobile) + backup
            commands::vault::resolve_mobile_vault_path,
            commands::vault::export_vault_archive,
            commands::vault::import_vault_archive,
            commands::vault::suggested_archive_name,
            // Diagnostics
            commands::diagnostics::diagnostics_info,
            commands::diagnostics::suggested_diagnostics_name,
            commands::diagnostics::export_diagnostics,
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
            feeds::feed_get_article,
            feeds::feed_search_articles,
            feeds::feed_get_unread_counts,
            feeds::feed_get_total_unread,
            feeds::feed_get_view_counts,
            feeds::feed_mark_read,
            feeds::feed_mark_all_read,
            feeds::feed_mark_read_bulk,
            feeds::feed_toggle_star,
            feeds::feed_toggle_read_later,
            feeds::feed_refresh,
            feeds::feed_discover,
            feeds::feed_fetch_article_content,
            feeds::feed_run_cleanup,
            feeds::feed_get_highlights,
            feeds::feed_add_highlight,
            feeds::feed_remove_highlight,
            feeds::feed_get_rules,
            feeds::feed_save_rules,
            feeds::feed_apply_rules,
            feeds::feed_cache_images,
            feeds::feed_state_sync,
            feeds::feed_start_scheduler,
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
            sync_cmds::sync_connect,
            sync_cmds::sync_full,
            sync_cmds::sync_disconnect,
            sync_cmds::sync_status,
            sync_cmds::sync_metrics,
            // Key Rotation
            sync_cmds::sync_current_epoch,
            sync_cmds::sync_revoke_device,
            // License
            license_cmds::get_license_state,
            license_cmds::get_hwid,
            license_cmds::activate_trial,
            license_cmds::activate_license_key,
            license_cmds::deactivate_license,
            license_cmds::refresh_license,
            license_cmds::heartbeat_license,
            // System
            open_app_log_folder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Clicking the Dock icon of an app with no open windows fires
            // this, and nothing else does. Without handling it, closing the
            // window and then clicking Synabit in the Dock does nothing at
            // all — the app is running, it simply has nowhere to appear.
            //
            // That only became possible when closing started hiding rather
            // than quitting, so the two belong together.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } = event
            {
                log::info!("dock reopen; has_visible_windows = {has_visible_windows}");
                if !has_visible_windows {
                    surface_main_window(app);
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (app, event);
            }
        });
}
