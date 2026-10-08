//! The app's own frame: its windows, the vault it has open, and the one place
//! every command call passes through.
//!
//! None of this belongs to a mini-app, and it used to live in `syn::browser`
//! only because the browsing pane was the first webview anybody thought to
//! lock. The lock now covers every webview, so it lives with the windows.
//!
//! * [`gate`] — who may call what, and with which vault and which paths.
//! * [`vault`] — the vault the app has open, decided here and never taken on
//!   a webview's word.
//! * [`dialogs`] — file dialogs opened from Rust, so a path a command writes to
//!   is one the person picked rather than one a script typed.

pub mod dialogs;
pub mod gate;
pub mod vault;

pub use gate::may_call;

/// The app's main window, and the webview in it that holds the app.
pub const MAIN_WINDOW: &str = "main";

/// The app's own window, however many webviews are inside it.
///
/// # Why not `get_webview_window("main")`
///
/// Because it stops working the moment a second webview joins that window.
/// `tauri-2.10.3/src/window/mod.rs:1083`:
///
/// ```text
/// pub(crate) fn is_webview_window(&self) -> bool {
///     self.webviews().iter().all(|w| w.label() == self.label())
/// }
/// ```
///
/// `get_webview_window` returns `Some` only for a window whose webviews are
/// *all* named after it. Docking the browsing pane makes that false for ever,
/// and every `get_webview_window("main")` in the app quietly starts answering
/// `None` — including the two that unminimise and focus the window when
/// somebody clicks the Dock icon. Opening a browser broke the Dock icon, and
/// said nothing.
///
/// A `Window` is what all of those actually wanted anyway: `show`, `hide`,
/// `set_focus`, `inner_size` and `add_child` all live there. The
/// `WebviewWindow` wrapper only adds the webview half, which none of them use.
pub fn app_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Option<tauri::window::Window<R>> {
    use tauri::Manager;
    app.get_webview(MAIN_WINDOW).map(|webview| webview.window())
}
