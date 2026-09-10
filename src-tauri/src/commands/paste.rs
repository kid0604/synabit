//! Pasting as plain text on macOS.
//!
//! Copy one column of a table in a browser and the clipboard holds two
//! things: the table as HTML, cells and borders and all, and the same values
//! as plain text, a line each. An ordinary paste takes the HTML. Shift+paste
//! is how you ask for the text instead, and ProseMirror already honours it.
//!
//! What it cannot honour is a paste that never happens. WebView2 binds
//! Ctrl+Shift+V to "paste and match style" itself. WKWebView binds nothing to
//! Cmd+Shift+V, and the app's Edit menu does not either, so on a Mac the keys
//! arrive as a keydown and nothing follows it.
//!
//! The editor catches that keydown and lands here, and this asks the webview
//! for the paste it would have done had the keys been bound:
//! `pasteAsPlainText:`, the editing command behind Safari's "Paste and Match
//! Style". Doing it natively rather than reading the clipboard from script
//! matters. `navigator.clipboard.readText()` in a WKWebView puts a "Paste"
//! bubble under the cursor that has to be clicked every time, while the native
//! command is a paste the user asked for and asks nothing. It also arrives as
//! a real `paste` event carrying `text/plain` alone, so everything the editor
//! does with a paste still applies.

use crate::error::{AppError, AppResult};

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn paste_as_plain_text(webview: tauri::Webview) -> AppResult<()> {
    use objc2::runtime::AnyObject;
    use objc2::{msg_send, sel};

    let (done, outcome) = tokio::sync::oneshot::channel::<bool>();
    webview
        .with_webview(move |platform| {
            // SAFETY: on macOS `inner()` is the WKWebView this webview wraps,
            // and `with_webview` runs this on the main thread, where AppKit
            // wants to be spoken to.
            let wk = unsafe { &*(platform.inner() as *const AnyObject) };
            // Asked first because messaging a selector WebKit does not
            // implement raises an Objective-C exception, and that aborts the
            // process rather than failing the paste.
            let known: bool = unsafe { msg_send![wk, respondsToSelector: sel!(pasteAsPlainText:)] };
            if known {
                let _: () = unsafe { msg_send![wk, pasteAsPlainText: None::<&AnyObject>] };
            }
            let _ = done.send(known);
        })
        .map_err(|e| AppError::General(format!("could not reach the webview: {e}")))?;

    match outcome.await {
        Ok(true) => Ok(()),
        Ok(false) => Err(AppError::UnsupportedCapability(
            "this WebKit has no pasteAsPlainText:".into(),
        )),
        Err(_) => Err(AppError::General("the webview went away before pasting".into())),
    }
}

/// Everywhere else the webview has the shortcut already, and the editor does
/// not call this. Present so the command list is the same on every platform.
#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub async fn paste_as_plain_text() -> AppResult<()> {
    Err(AppError::UnsupportedCapability(
        "plain-text paste is the webview's own shortcut on this platform".into(),
    ))
}
