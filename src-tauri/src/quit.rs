//! Quitting, after what is waiting to be written has been written.
//!
//! The tray's Quit (and the menu's, and Cmd+Q) ended the process at once.
//! The front end saves some things a moment after the last change — a
//! board's autosave runs two seconds later — and whatever was inside that
//! moment was lost. So the first request to quit is held: the front end is
//! told (`app:before-quit`), finishes its writes, and says so (`quit_ready`);
//! then the app quits for real. If it never answers — no window loaded, a
//! write that hangs — the app quits anyway after [`WAIT`]: quitting must
//! always work.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::Emitter;

/// The longest a quit waits for the front end.
const WAIT: Duration = Duration::from_secs(4);

static ASKED: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static LEAVING: AtomicBool = AtomicBool::new(false);

/// Called on every request to quit. True when this request is to be held
/// (the front end has been asked to finish); false when the app may go.
pub fn hold<R: tauri::Runtime>(app: &tauri::AppHandle<R>, code: Option<i32>) -> bool {
    if LEAVING.load(Ordering::SeqCst) {
        return false;
    }
    if ASKED.swap(true, Ordering::SeqCst) {
        // Already waiting: a second Cmd+Q does not start another wait.
        return true;
    }
    if app.emit("app:before-quit", ()).is_err() {
        ASKED.store(false, Ordering::SeqCst);
        return false;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        while !READY.load(Ordering::SeqCst) && start.elapsed() < WAIT {
            std::thread::sleep(Duration::from_millis(25));
        }
        if !READY.load(Ordering::SeqCst) {
            log::warn!("quit: the front end did not finish in {WAIT:?}; quitting anyway");
        }
        LEAVING.store(true, Ordering::SeqCst);
        app.exit(code.unwrap_or(0));
    });
    true
}

/// The front end has written what it was waiting to write.
#[tauri::command]
pub fn quit_ready() {
    READY.store(true, Ordering::SeqCst);
}
