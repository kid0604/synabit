//! The browser, inside the window rather than beside it.
//!
//! # Why not a second window
//!
//! Because the answer to *"do you actually look at it"* came back **yes**, with
//! a prediction that browsing is going to be most of what Syn does. A thing
//! glanced at for three seconds can live anywhere. A thing that is **read**
//! belongs next to what it is answering.
//!
//! # The mistake this module exists because I stopped making
//!
//! Twice I priced this as expensive, and twice for the same wrong reason: a
//! child webview is an OS view that paints **above** HTML, and this app has
//! **69 files** using `fixed inset-0`. Hooking every one of them to hide the
//! pane is not something anybody maintains — the modal added next month forgets,
//! and the browser silently covers it.
//!
//! That was the wrong question. The right one is *why are they overlapping at
//! all?* `Webview::set_bounds` is not gated behind `unstable`; only `add_child`
//! is. So:
//!
//! ```text
//! ┌──────────────────────────┬──────────────┐
//! │  the app's own webview   │  the browser │
//! │  set_bounds(left)        │  add_child   │
//! │  ← all 69 overlays live  │  (right)     │
//! │    here, untouched       │              │
//! └──────────────────────────┴──────────────┘
//! ```
//!
//! **Shrink, do not overlay.** Nothing intersects, so nothing is occluded, so
//! none of the 69 need changing — they simply live in a narrower viewport, and
//! the app is already responsive. That is what Electron apps do, and Tauri can
//! do it too.
//!
//! # What the gate found
//!
//! Opening and closing hold: the app's webview goes to the left and **stays
//! there**, which was the expensive question. Resizing the window did not, and
//! the reason is worth writing down because it is the opposite of a bug in the
//! runtime — it is the runtime already doing this job, and me fighting it.
//!
//! `auto_resize` in wry does not mean *fill the window*. It stores **rates** —
//! `x_rate`, `y_rate`, `width_rate`, `height_rate` — and reapplies them every
//! time the window changes size. That is exactly what a docked column is: a
//! fraction of the width, pinned to an edge, full height.
//!
//! And `set_bounds` does **not** recompute those rates. So the app's webview,
//! created window-filling with rates of `1.0`, kept them after being moved to
//! the left — and the next resize snapped it back over the pane, while a
//! `WindowEvent::Resized` handler of mine tried to put it back. Two things
//! moving the same view on the same event.
//!
//! So there is no handler now. Both webviews get `set_auto_resize(true)` after
//! being placed, wry keeps the proportions, and the layout survives a resize
//! because nothing is arguing with it.

use crate::error::{AppError, AppResult};

/// The label of the webview holding the page.
///
/// The same label the separate window used, deliberately: `browser::may_call`
/// — the lock that stops a page calling this app's commands — is keyed on the
/// **webview** label precisely so that moving the browser inside the main
/// window does not silently unlock it.
pub const PANE: &str = crate::syn::browser::WINDOW;

/// How much of the window the pane takes, when there is room to choose.
pub const SHARE: f64 = 0.38;

/// The narrowest the pane is worth drawing, in logical pixels.
///
/// Below this a page reflows into a column of single words and stops being
/// something anybody can read — at which point it is a picture of browsing
/// rather than browsing.
pub const NARROWEST: u32 = 380;

/// The narrowest the app may be squeezed to, in logical pixels.
///
/// The conversation is the thing the pane exists to sit beside. Squeezing it
/// past its own layout to make room for the browser gets the priority backwards.
pub const APP_KEEPS: u32 = 520;

/// Two rectangles, in the units the caller measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Where the app's own webview goes.
    pub app: (i32, i32, u32, u32),
    /// Where the browser goes, or `None` when the window is too narrow to hold
    /// both.
    pub pane: Option<(i32, i32, u32, u32)>,
}

impl Layout {
    /// The whole window to the app, which is also how it started.
    pub fn only_the_app(width: u32, height: u32) -> Self {
        Layout { app: (0, 0, width, height), pane: None }
    }
}

/// Where each webview goes, for a window this size.
///
/// Pure arithmetic, in logical pixels, so the part that can be checked is
/// checked — the part that cannot is what the window does with the answer.
///
/// A window too narrow for both gets **no pane at all** rather than two
/// unusable slivers. Refusing to open is honest; opening something nobody can
/// read is not.
pub fn layout(width: u32, height: u32, wanted: bool) -> Layout {
    if !wanted {
        return Layout::only_the_app(width, height);
    }

    let share = (width as f64 * SHARE) as u32;
    let pane_width = share.max(NARROWEST);

    // The app keeps its floor first: the conversation is what the pane is
    // there to sit beside.
    if width < APP_KEEPS.saturating_add(NARROWEST) {
        return Layout::only_the_app(width, height);
    }
    let pane_width = pane_width.min(width.saturating_sub(APP_KEEPS));

    let app_width = width.saturating_sub(pane_width);
    Layout {
        app: (0, 0, app_width, height),
        pane: Some((app_width as i32, 0, pane_width, height)),
    }
}

// ═══════════════════════════════════════════════════════════════
//  PUTTING IT ON THE SCREEN
// ═══════════════════════════════════════════════════════════════

/// Lay both webviews out for the window's current size.
///
/// Best effort on each move: a webview that has gone, or a runtime that refuses
/// a bounds change, is a pane that looks wrong rather than an app that stops.
#[cfg(desktop)]
pub fn arrange<R: tauri::Runtime>(app: &tauri::AppHandle<R>, wanted: bool) -> AppResult<Layout> {
    use tauri::Manager;

    let main = app
        .get_webview_window(crate::syn::browser::MAIN_WINDOW)
        .ok_or_else(|| AppError::General("There is no main window to arrange".into()))?;

    let size = main
        .inner_size()
        .map_err(|e| AppError::General(format!("Could not measure the window: {e}")))?;
    let scale = main.scale_factor().unwrap_or(1.0);

    let logical = |v: u32| (v as f64 / scale) as u32;
    let plan = layout(logical(size.width), logical(size.height), wanted);

    let rect = |(x, y, w, h): (i32, i32, u32, u32)| tauri::Rect {
        position: tauri::LogicalPosition::new(x, y).into(),
        size: tauri::LogicalSize::new(w, h).into(),
    };

    place(main.as_ref(), rect(plan.app), "the app's webview");

    // Only ever moves what is already there. Making the pane and taking it away
    // belong to `open` and `close`.
    if let (Some(bounds), Some(pane)) = (plan.pane, app.get_webview(PANE)) {
        place(&pane, rect(bounds), "the pane");
    }

    Ok(plan)
}

/// Put a webview somewhere, and teach it to stay in proportion there.
///
/// The second half is the part that was missing. `set_bounds` moves a webview
/// and leaves its `auto_resize` rates alone — so a webview created to fill the
/// window keeps rates of `1.0`, and the next resize snaps it straight back over
/// whatever was placed beside it. `set_auto_resize(true)` recomputes the rates
/// from where it is *now*, which is what makes a docked column survive a drag.
///
/// Best effort on both: a runtime that refuses is a pane that looks wrong, not
/// an app that stops.
#[cfg(desktop)]
fn place<R: tauri::Runtime>(webview: &tauri::webview::Webview<R>, to: tauri::Rect, what: &str) {
    if let Err(e) = webview.set_bounds(to) {
        log::warn!("[Syn] {what} would not move: {e}");
        return;
    }
    if let Err(e) = webview.set_auto_resize(true) {
        log::warn!("[Syn] {what} will not keep its share when the window resizes: {e}");
    }
}

/// Open the pane on a page, making it if it is not there.
///
/// Everything the separate window was given, given again — the empty jar, the
/// reader script, the navigation guard, no downloads, no popups. `browser` is
/// where each of those is explained; none of it is new here, which is the
/// point: this is the same browser in a different place.
#[cfg(desktop)]
pub fn open<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
    nonce: &str,
) -> AppResult<()> {
    use tauri::Manager;

    crate::syn::browser::guard(url)?;
    let target = url::Url::parse(url).map_err(|e| AppError::General(format!("Bad address: {e}")))?;

    if let Some(pane) = app.get_webview(PANE) {
        return pane
            .navigate(target)
            .map_err(|e| AppError::General(format!("Could not navigate the pane: {e}")));
    }

    let plan = arrange(app, true)?;
    let Some((x, y, w, h)) = plan.pane else {
        return Err(AppError::General(
            "The window is too narrow to show a browser beside the conversation".into(),
        ));
    };

    let jar = app
        .path()
        .app_local_data_dir()
        .map_err(|e| AppError::General(format!("No data directory: {e}")))?
        .join(crate::syn::browser::JAR);

    let main = app
        .get_webview_window(crate::syn::browser::MAIN_WINDOW)
        .ok_or_else(|| AppError::General("There is no main window to sit in".into()))?;

    let builder = tauri::webview::WebviewBuilder::new(PANE, tauri::WebviewUrl::External(target))
        .data_directory(jar)
        .initialization_script(crate::syn::browser::reader_script(nonce))
        .on_navigation(crate::syn::browser::may_go_to)
        .on_download(|_, _| crate::syn::browser::NOTHING_LEAVES_THE_WINDOW)
        .on_new_window(|url, _| {
            log::warn!("[Syn] A page tried to open a window at {url}");
            tauri::webview::NewWindowResponse::Deny
        })
        .browser_extensions_enabled(false)
        .on_page_load(|webview, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                if let Some(waiting) = webview.app_handle().try_state::<crate::syn::browser::Waiting>()
                {
                    crate::syn::browser::note_loaded(&waiting);
                }
            }
        });

    let pane = main
        .as_ref()
        .window()
        .add_child(
            builder,
            tauri::LogicalPosition::new(x, y),
            tauri::LogicalSize::new(w, h),
        )
        .map_err(|e| AppError::General(format!("Could not open the pane: {e}")))?;

    // Its share of the window, kept by the runtime from here on.
    if let Err(e) = pane.set_auto_resize(true) {
        log::warn!("[Syn] The pane will not keep its share when the window resizes: {e}");
    }

    Ok(())
}

/// Put the pane away and give the app its window back.
#[cfg(desktop)]
pub fn close<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AppResult<()> {
    use tauri::Manager;

    if let Some(pane) = app.get_webview(PANE) {
        let _ = pane.close();
    }
    arrange(app, false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout that makes the whole design work: side by side, touching, and
    /// filling the window between them. No intersection is the entire reason
    /// the 69 overlays need no changing.
    #[test]
    fn the_two_never_overlap_and_leave_no_gap() {
        let plan = layout(1600, 900, true);
        let (ax, _, aw, ah) = plan.app;
        let (px, _, pw, ph) = plan.pane.expect("there is room for both");

        assert_eq!(ax, 0);
        assert_eq!(px, aw as i32, "the pane starts exactly where the app ends");
        assert_eq!(aw + pw, 1600, "and together they are the window");
        assert_eq!((ah, ph), (900, 900), "both full height");
    }

    /// A window too narrow for both gets no pane rather than two slivers.
    /// Refusing to open is honest; opening something nobody can read is not.
    #[test]
    fn a_window_too_narrow_for_both_keeps_the_conversation() {
        let plan = layout(APP_KEEPS + NARROWEST - 1, 800, true);

        assert!(plan.pane.is_none(), "{plan:?}");
        assert_eq!(plan.app, (0, 0, APP_KEEPS + NARROWEST - 1, 800), "the app keeps all of it");
    }

    /// And the conversation keeps its floor before the pane gets its share.
    /// The pane exists to sit *beside* the conversation; squeezing the
    /// conversation to make room for it has the priority backwards.
    #[test]
    fn the_conversation_is_never_squeezed_past_its_floor() {
        for width in [900, 1000, 1200, 1600, 2560, 3840] {
            let plan = layout(width, 900, true);
            let (_, _, app_width, _) = plan.app;
            assert!(
                app_width >= APP_KEEPS,
                "at {width} the conversation was cut to {app_width}"
            );
            if let Some((_, _, pane_width, _)) = plan.pane {
                assert!(pane_width >= NARROWEST, "at {width} the pane was {pane_width}");
            }
        }
    }

    /// Not wanted is the whole window, which is also how the app starts and
    /// what it must go back to when the pane closes.
    #[test]
    fn closing_it_gives_the_window_back() {
        assert_eq!(layout(1600, 900, false), Layout::only_the_app(1600, 900));
        assert!(layout(1600, 900, false).pane.is_none());
    }

    /// A very wide window gives the pane a share rather than everything left
    /// over — a browser three feet wide is not more readable, and the
    /// conversation is still the thing being worked in.
    #[test]
    fn a_wide_window_does_not_give_the_pane_everything() {
        let (_, _, pane_width, _) = layout(3840, 1000, true).pane.expect("room for both");
        assert!(pane_width < 3840 / 2, "the pane took half a very wide window: {pane_width}");
    }

    /// The pane is the same webview label the separate window used, and that is
    /// load-bearing: `browser::may_call` keys the lock on the webview label so
    /// that moving the browser inside the main window cannot silently unlock
    /// it. A different label here would open the door quietly.
    #[test]
    fn the_pane_is_still_the_webview_the_lock_is_written_about() {
        assert_eq!(PANE, crate::syn::browser::WINDOW);
        assert!(!crate::syn::browser::may_call(PANE, "trash_node"));
        assert!(crate::syn::browser::may_call(PANE, crate::syn::browser::THE_ONE_DOOR));
    }

    /// And it is built with every guard the separate window has. These are not
    /// two browsers; they are one browser in two places, and a hook set on one
    /// and forgotten on the other is how they drift apart.
    #[cfg(desktop)]
    #[test]
    fn the_pane_is_built_with_the_same_guards_as_the_window() {
        let source = include_str!("pane.rs");
        let built = source
            .split("WebviewBuilder::new(PANE")
            .nth(1)
            .expect("the pane is still built here");
        let built = built.split("main.as_ref()").next().unwrap_or(built);

        for guard in [
            ".data_directory(",
            ".initialization_script(",
            ".on_navigation(",
            ".on_download(",
            ".on_new_window(",
            ".browser_extensions_enabled(false)",
        ] {
            assert!(built.contains(guard), "the pane is built without {guard}");
        }
    }
}
