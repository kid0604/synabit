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
//! # The answer I got wrong, and what the screen said
//!
//! I thought the fix was *shrink the app's webview instead of overlaying it* —
//! `Webview::set_bounds` is public and not behind `unstable`, so put the app on
//! the left and the browser on the right and nothing intersects.
//!
//! It does not work, and it fails **silently**. `wry-0.54.4`,
//! `src/wkwebview/mod.rs:1010`:
//!
//! ```text
//! pub fn set_bounds(&self, bounds: Rect) -> crate::Result<()> {
//!   #[cfg(target_os = "macos")]
//!   if self.is_child {          // ← only a child webview moves
//!       ... setFrame ...
//!   }
//!   Ok(())                       // ← the main webview: nothing, reported as success
//! }
//! ```
//!
//! On macOS the main webview cannot be moved at all. It is the window's content
//! view, wrapped in wry's own parent view with an AppKit autoresizing mask that
//! keeps it filling the window. `set_bounds` returns `Ok` and does nothing, so
//! the pane painted straight over the conversation and my own error handling
//! had nothing to report.
//!
//! # So the app gets out of the way in CSS, not in AppKit
//!
//! The main webview stays where it is — full window, untouched, resizing
//! natively. What changes is where the app *draws*:
//!
//! ```text
//! ┌──────────────────────────┬──────────────┐
//! │  app webview: full window                │
//! │  ┌───────────────────────┐  the browser  │
//! │  │ what the app draws    │  paints over  │
//! │  │ width: calc(100%-38%) │  the strip    │
//! │  └───────────────────────┘  left empty   │
//! └──────────────────────────┴──────────────┘
//! ```
//!
//! And the 69 `fixed inset-0` overlays come along for free, through one line of
//! CSS: a `transform` on an element makes it the **containing block for
//! `position: fixed` descendants**. So the app's root shrinks *and* becomes the
//! frame every overlay is measured against — all 69 confined without one of
//! them being edited.
//!
//! Which also means this no longer depends on `set_bounds` working anywhere.
//! One mechanism on every platform, rather than a different story per runtime.
//!
//! # How the pane keeps its share
//!
//! `auto_resize` in wry does not mean *fill the window*. It stores **rates** —
//! `x_rate`, `y_rate`, `width_rate`, `height_rate` — and reapplies them every
//! time the window changes size. That is exactly what a docked column is: a
//! fraction of the width, pinned to an edge, full height. So the pane is given
//! its rates once, at birth, and the runtime keeps them.
//!
//! Which is why there is no `WindowEvent::Resized` handler here. There was one,
//! and it was doing a job wry already does — badly, and against it.

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
pub const NARROWEST: u32 = 300;

/// The narrowest the app may be squeezed to, in logical pixels.
///
/// # Why these two are as small as they are
///
/// They were 380 and 520, and on an ordinary window that left **no room to
/// drag at all**. A 950-logical-pixel window — a 1900px window on a 2× display,
/// which is nothing unusual — gave the pane a range of 380 to 430. Fifty
/// pixels. And the default share put it at 380, the bottom of that range, so
/// pulling the edge *rightward* did nothing whatsoever: it was already there.
///
/// The mistake was treating a floor as a layout opinion. A floor exists to stop
/// something useless — a browser too narrow to read, a conversation squeezed to
/// a ribbon — not to decide the proportions on somebody's behalf. Whoever is
/// pulling the edge is deciding; these two only say where it stops being worth
/// doing.
pub const APP_KEEPS: u32 = 320;

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

    /// How much of the window's width the pane takes, as a fraction.
    ///
    /// This is what crosses to the frontend, rather than a pixel count, and the
    /// reason is that both sides then agree for free. `auto_resize` keeps the
    /// pane at a **rate** of the window; a CSS width in the same fraction stays
    /// correct through every resize with nothing to notify and nothing to drift.
    pub fn pane_share(&self) -> f64 {
        match self.pane {
            Some((_, _, pane_width, _)) => {
                let total = self.app.2.saturating_add(pane_width);
                if total == 0 {
                    0.0
                } else {
                    pane_width as f64 / total as f64
                }
            }
            None => 0.0,
        }
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
/// `wanted` is the share of the width asked for — `None` for no pane at all,
/// and `Some(SHARE)` for the default. A drag passes what the pointer is asking
/// for and gets back what the window can actually give.
pub fn layout(width: u32, height: u32, wanted: Option<f64>) -> Layout {
    let Some(share) = wanted else {
        return Layout::only_the_app(width, height);
    };

    // The app keeps its floor first: the conversation is what the pane is
    // there to sit beside.
    if width < APP_KEEPS.saturating_add(NARROWEST) {
        return Layout::only_the_app(width, height);
    }

    let asked = (width as f64 * share.clamp(0.0, 1.0)) as u32;
    let pane_width = asked
        .max(NARROWEST)
        .min(width.saturating_sub(APP_KEEPS));

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
pub fn arrange<R: tauri::Runtime>(app: &tauri::AppHandle<R>, wanted: Option<f64>) -> AppResult<Layout> {
    use tauri::Manager;

    // `get_webview_window` is deliberately not used — see `browser::app_window`
    // for what a second webview does to it, and what that broke.
    let main = crate::syn::browser::app_window(app)
        .ok_or_else(|| AppError::General("There is no main window to arrange".into()))?;

    let size = main
        .inner_size()
        .map_err(|e| AppError::General(format!("Could not measure the window: {e}")))?;
    let scale = main.scale_factor().unwrap_or(1.0);

    let logical = |v: u32| (v as f64 / scale) as u32;
    let plan = layout(logical(size.width), logical(size.height), wanted);

    // Nothing here touches the app's own webview. It cannot be moved on macOS
    // and does not need to be anywhere: it stays full-window and the *app*
    // draws itself narrower. See the header.
    //
    // Errors are reported rather than dropped. Two bugs on this path hid inside
    // a `let _ =` — a `set_bounds` that silently does nothing, and a pane the
    // manager could not find — and each cost a round of guessing that a log
    // line would have ended.
    if let Some((x, y, w, h)) = plan.pane {
        match app.get_webview(PANE) {
            Some(pane) => {
                pane.set_bounds(tauri::Rect {
                    position: tauri::LogicalPosition::new(x, y).into(),
                    size: tauri::LogicalSize::new(w, h).into(),
                })
                .map_err(|e| AppError::General(format!("Could not place the pane: {e}")))?;

                if let Err(e) = pane.set_auto_resize(true) {
                    log::warn!("[Syn] The pane will not keep its share on resize: {e}");
                }
            }
            None => log::warn!("[Syn] A layout wanted a pane and there is no `{PANE}` webview"),
        }
    }

    Ok(plan)
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
) -> AppResult<f64> {
    use tauri::Manager;

    crate::syn::browser::guard(url)?;
    let target = url::Url::parse(url).map_err(|e| AppError::General(format!("Bad address: {e}")))?;

    if let Some(pane) = app.get_webview(PANE) {
        pane.navigate(target)
            .map_err(|e| AppError::General(format!("Could not navigate the pane: {e}")))?;
        return Ok(arrange(app, Some(SHARE))?.pane_share());
    }

    let plan = arrange(app, Some(SHARE))?;
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

    let main = crate::syn::browser::app_window(app)
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

    Ok(plan.pane_share())
}

/// Drag the edge between the conversation and the pane.
///
/// The clamping lives here rather than on the screen, so there is one answer to
/// *how narrow may this get* — `layout` already holds `APP_KEEPS` and
/// `NARROWEST`, and a second copy in CSS would be a second opinion that drifts.
/// What comes back is what the window could actually give, which is what the
/// app then draws itself to.
///
/// # Called on every frame of a drag, and that is deliberate
///
/// There was a version of this that pushed the pane off the right edge for the
/// length of the pull, drew a line where the edge would land, and put the pane
/// back on release. It worked, and it looked terrible: the column went white
/// while the pane was away and flashed as it came back, every single time.
///
/// That trick existed to solve a real problem — the pane moves to meet the
/// pointer, which puts the pointer *on the pane*, and this app then receives no
/// mouse events at all. But the reason live dragging failed the first time was
/// not the mechanism. It was `APP_KEEPS` and `NARROWEST` being so tight that
/// the pane never moved at all, so its edge sat still while the pointer walked
/// onto it. With floors that leave room, the edge keeps up with the pointer and
/// the pointer stays on the app's side of it.
#[cfg(desktop)]
pub fn drag_to<R: tauri::Runtime>(app: &tauri::AppHandle<R>, share: f64) -> AppResult<f64> {
    Ok(arrange(app, Some(share))?.pane_share())
}


/// Put the pane away and give the app its window back.
#[cfg(desktop)]
pub fn close<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AppResult<()> {
    use tauri::Manager;

    if let Some(pane) = app.get_webview(PANE) {
        let _ = pane.close();
    }
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
        let plan = layout(1600, 900, Some(SHARE));
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
        let plan = layout(APP_KEEPS + NARROWEST - 1, 800, Some(SHARE));

        assert!(plan.pane.is_none(), "{plan:?}");
        assert_eq!(plan.app, (0, 0, APP_KEEPS + NARROWEST - 1, 800), "the app keeps all of it");
    }

    /// And the conversation keeps its floor before the pane gets its share.
    /// The pane exists to sit *beside* the conversation; squeezing the
    /// conversation to make room for it has the priority backwards.
    #[test]
    fn the_conversation_is_never_squeezed_past_its_floor() {
        for width in [900, 1000, 1200, 1600, 2560, 3840] {
            let plan = layout(width, 900, Some(SHARE));
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
        assert_eq!(layout(1600, 900, None), Layout::only_the_app(1600, 900));
        assert!(layout(1600, 900, None).pane.is_none());
    }

    /// A very wide window gives the pane a share rather than everything left
    /// over — a browser three feet wide is not more readable, and the
    /// conversation is still the thing being worked in.
    #[test]
    fn a_wide_window_does_not_give_the_pane_everything() {
        let (_, _, pane_width, _) = layout(3840, 1000, Some(SHARE)).pane.expect("room for both");
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

    /// The edge can be dragged, and the same floors hold whoever is pulling.
    ///
    /// The clamping is here rather than in CSS so that there is one answer to
    /// *how narrow may this get*. A copy on the screen would be a second
    /// opinion, and the two would part company the first time one of these
    /// constants moved.
    #[test]
    fn dragging_the_edge_still_obeys_both_floors() {
        // Pulled far too wide: the conversation keeps its floor.
        let greedy = layout(1600, 900, Some(0.95));
        assert_eq!(greedy.app.2, APP_KEEPS);
        assert_eq!(greedy.pane.expect("still open").2, 1600 - APP_KEEPS);

        // Pulled almost shut: the pane keeps its own.
        let squeezed = layout(1600, 900, Some(0.01));
        assert_eq!(squeezed.pane.expect("still open").2, NARROWEST);

        // And nonsense is clamped rather than believed.
        for share in [-1.0, 0.0, 1.0, 2.0, f64::NAN] {
            let plan = layout(1600, 900, Some(share));
            let (_, _, pane_width, _) = plan.pane.expect("open at any asking");
            assert!(
                (NARROWEST..=1600 - APP_KEEPS).contains(&pane_width),
                "share {share} produced {pane_width}"
            );
        }
    }

    /// What crosses to the screen is a fraction, and it has to describe the
    /// layout it came from — the app draws itself to `1 - share`, so a share
    /// that does not match leaves a gap or an overlap.
    #[test]
    fn the_share_describes_the_layout_it_came_from() {
        for width in [900, 1280, 1600, 2560] {
            let plan = layout(width, 900, Some(SHARE));
            let share = plan.pane_share();
            let drawn = (width as f64 * (1.0 - share)).round() as u32;

            assert!(
                drawn.abs_diff(plan.app.2) <= 1,
                "at {width} the app would draw {drawn} for a layout of {}",
                plan.app.2
            );
        }

        assert_eq!(layout(1600, 900, None).pane_share(), 0.0, "closed is zero");
    }

    /// A floor that leaves no room to drag is not a floor, it is a decision.
    ///
    /// This is the test that would have caught it. `APP_KEEPS` and `NARROWEST`
    /// were 520 and 380, and on a 950-logical-pixel window — a 1900px window on
    /// a 2× display, entirely ordinary — the pane could only be between 380 and
    /// 430. Fifty pixels, with the default share sitting at the bottom of it, so
    /// pulling the edge one way did nothing at all.
    #[test]
    fn the_floors_leave_room_to_actually_drag() {
        // Two hundred logical pixels of travel. An absolute number rather than
        // a share of the window, because what makes a drag worth doing is how
        // far the hand moves, not what fraction of the screen that is. The old
        // constants gave fifty on an ordinary window, which is what this bites
        // on.
        const ROOM_TO_PULL: u32 = 200;

        for width in [900u32, 950, 1280, 1600, 2560] {
            let widest = layout(width, 900, Some(1.0)).pane.expect("a pane at any width").2;
            let narrowest = layout(width, 900, Some(0.0)).pane.expect("a pane at any width").2;
            let room = widest - narrowest;

            assert!(
                room >= ROOM_TO_PULL,
                "at {width} the pane can only move {room}px, between {narrowest} and {widest}"
            );
        }
    }

    /// And the default sits inside that range rather than pinned to an end of
    /// it, so the edge moves both ways from where it opens.
    #[test]
    fn it_opens_somewhere_it_can_be_pulled_from_either_side() {
        for width in [950u32, 1280, 1600, 2560] {
            let opened = layout(width, 900, Some(SHARE)).pane.expect("open").2;
            let widest = layout(width, 900, Some(1.0)).pane.expect("open").2;
            let narrowest = layout(width, 900, Some(0.0)).pane.expect("open").2;

            assert!(
                opened > narrowest && opened < widest,
                "at {width} it opens at {opened}, pinned against {narrowest}..{widest}"
            );
        }
    }
}
