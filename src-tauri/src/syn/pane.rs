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
/// The strip across the top of the pane that belongs to the app.
///
/// Thirty-six logical pixels, and the app draws an address bar in it: where
/// this page is, the way back, and the way out. A browser without those is a
/// place you can be taken and cannot leave.
///
/// It is **reserved from the pane, not overlaid on it**. The pane is a webview
/// of the operating system's and it draws over anything this app puts in the
/// same rectangle — that is the fact the whole side-by-side design was built
/// around, and a floating toolbar would rediscover it the hard way.
///
/// The frontend has to agree on this number to draw a bar that fits, and
/// `the_bar_is_the_same_height_on_both_sides` is what makes it agree.
pub const BAR: u32 = 36;

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
    // The bar comes off the top of the pane, and never off so much that the
    // pane has no height left — a window shorter than the bar is absurd, and
    // `saturating_sub` answering it with zero beats an underflow.
    let bar = BAR.min(height);
    Layout {
        app: (0, 0, app_width, height),
        pane: Some((app_width as i32, bar as i32, pane_width, height.saturating_sub(bar))),
    }
}

/// The share the pane has right now.
///
/// # Why this has to be remembered rather than worked out
///
/// A window resize must not change how the window is divided, so re-laying-out
/// after one needs the share that is in force — and `layout` takes the share as
/// an argument precisely so it holds no state. Reading it back off the pane's
/// own bounds would work and would be a measurement of the thing being
/// corrected, which is the wrong direction to take a number from.
///
/// Zero means no pane, which is also the state before the first one opens.
static SHARE_NOW: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn remember_share(share: f64) {
    SHARE_NOW.store(share.to_bits(), std::sync::atomic::Ordering::Relaxed);
}

fn share_now() -> Option<f64> {
    let share = f64::from_bits(SHARE_NOW.load(std::sync::atomic::Ordering::Relaxed));
    (share > 0.0).then_some(share)
}

/// Put the pane back where it belongs, the window having changed size.
///
/// # Why `auto_resize` is not enough on its own
///
/// It keeps every edge as a **rate** of the window — `tauri-runtime-wry`'s
/// `WebviewBounds` is four fractions — so the pane's *top* is kept at a
/// fraction of the height rather than at `BAR` pixels. Grow the window and the
/// gap grows with it, and a strip of app background opens under a bar that no
/// longer meets the page it belongs to.
///
/// Width is a fraction and is right to be one. Only the bar is a fixed number,
/// and this is what keeps it fixed.
#[cfg(desktop)]
pub fn keep_arranged<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(share) = share_now() else { return };
    if let Err(e) = arrange(app, Some(share)) {
        log::warn!("[Syn] The pane did not follow the window: {e}");
    }
}

// ═══════════════════════════════════════════════════════════════
//  PUTTING IT ON THE SCREEN
// ═══════════════════════════════════════════════════════════════

/// The event that tells the app how much room to leave.
///
/// # Why an event and not a return value
///
/// A return value only reaches whoever called. The globe calls, so the globe
/// learns — but when **Syn** opens the pane to look something up, nothing on
/// the screen called anything. The app went on drawing itself full width and
/// the pane painted straight over the conversation, which is exactly what a
/// browser appearing out of nowhere on top of your work looks like.
///
/// So the share is announced instead of returned. Whoever caused it, the app
/// hears the same thing.
pub const SHARE_CHANGED: &str = "syn-pane-share";

/// Say how much of the window the pane is taking now.
#[cfg(desktop)]
fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, share: f64) {
    use tauri::Emitter;
    if let Err(e) = app.emit(SHARE_CHANGED, share) {
        log::warn!("[Syn] Could not say how wide the pane is: {e}");
    }
}

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
        // `if let`, not a `match` with an empty arm: no pane yet simply means
        // `open` is about to make one. Nothing is wrong there — it used to log
        // a warning, which fired on every single opening, and that is how a log
        // stops being read.
        if let Some(pane) = app.get_webview(PANE) {
            pane.set_bounds(tauri::Rect {
                position: tauri::LogicalPosition::new(x, y).into(),
                size: tauri::LogicalSize::new(w, h).into(),
            })
            .map_err(|e| AppError::General(format!("Could not place the pane: {e}")))?;

            if let Err(e) = pane.set_auto_resize(true) {
                log::warn!("[Syn] The pane will not keep its share on resize: {e}");
            }
        }
    }

    remember_share(plan.pane_share());
    announce(app, plan.pane_share());
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
        // The title on screen belongs to the page being left. Cleared before
        // the navigation rather than after it, so there is no moment where the
        // bar reads the old title beside the new address.
        set_title(String::new());
        pane.navigate(target)
            .map_err(|e| AppError::General(format!("Could not navigate the pane: {e}")))?;
        let share = arrange(app, Some(SHARE))?.pane_share();
        announce_page(app);
        return Ok(share);
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
        // The title, which is the only part of `Showing` that has nowhere else
        // to be read from. It is also what the address bar shows: an address is
        // what a page *is*, a title is what it is *about*.
        .on_document_title_changed(|webview, title| {
            set_title(title);
            announce_page(webview.app_handle());
        })
        .on_page_load(|webview, payload| {
            match payload.event() {
                // A new document, so the old document's title is a lie until
                // the new one says otherwise. Cleared here rather than in
                // `on_navigation`, which also fires for navigations that are
                // refused and for ones that never arrive.
                tauri::webview::PageLoadEvent::Started => {
                    set_title(String::new());
                    announce_page(webview.app_handle());
                }
                tauri::webview::PageLoadEvent::Finished => {
                    if let Some(waiting) =
                        webview.app_handle().try_state::<crate::syn::browser::Waiting>()
                    {
                        crate::syn::browser::note_loaded(&waiting);
                    }
                    // The address is asked of the webview, so this says nothing
                    // new about *where* — but a redirect lands here and nowhere
                    // else, and the bar would otherwise still show what was
                    // typed rather than what arrived.
                    announce_page(webview.app_handle());
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

    // Now that it exists. The `arrange` above ran before `add_child` and so
    // announced a pane that was not there yet.
    announce(app, plan.pane_share());
    announce_page(app);
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


// ═══════════════════════════════════════════════════════════════
//  WHAT IS ON IT
// ═══════════════════════════════════════════════════════════════

/// The page the pane is showing.
///
/// # Why this type exists at all
///
/// Because without it the browser had no memory, and the transcript shows what
/// that costs. Syn opened `vnexpress.net`, read it, and told the person the
/// headline. The next message was *"read that article"* — the most ordinary
/// follow-up there is — and Syn went to **DuckDuckGo to search for the headline
/// it had just written itself**, because the page holding that link had died
/// with the previous run.
///
/// A browser whose page does not survive a turn is not a browser. It is a
/// fetch with a window around it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Showing {
    pub url: String,
    /// The document's own title, empty until the page says what it is.
    pub title: String,
}

/// The title of the page in the pane.
///
/// Only the title is kept here. The **address is asked of the webview** every
/// time — `Webview::url()` is authoritative and cannot drift, and a copy of it
/// in a static is one more thing that can be wrong after a redirect that
/// nothing told us about.
///
/// A title has no such source: it arrives once, in a callback, and there is
/// nowhere else to read it from.
static TITLE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// The page changed — a new address, or a title for the one already there.
pub const PAGE_CHANGED: &str = "syn-pane-page";

fn set_title(title: String) {
    *TITLE.lock().unwrap_or_else(|e| e.into_inner()) = title;
}

fn the_title() -> String {
    TITLE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// What the pane is showing, or `None` if there is no pane.
///
/// The one question `browse` could not ask, and the reason it had to rebuild
/// from a search box every single turn.
#[cfg(desktop)]
pub fn showing<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<Showing> {
    use tauri::Manager;

    let url = app.get_webview(PANE)?.url().ok()?.to_string();
    // A pane parked on `about:blank` is a pane showing nothing. Saying it is
    // showing a page would send Syn to read a blank document.
    if url == "about:blank" || url.is_empty() {
        return None;
    }
    Some(Showing { url, title: the_title() })
}

/// No pane on a platform that cannot have one.
#[cfg(mobile)]
pub fn showing<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> Option<Showing> {
    None
}

/// Tell the screen what the pane is on now.
#[cfg(desktop)]
fn announce_page<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Emitter;
    if let Err(e) = app.emit(PAGE_CHANGED, showing(app)) {
        log::warn!("[Syn] Could not say what the pane is showing: {e}");
    }
}

// ═══════════════════════════════════════════════════════════════
//  WHOSE PANE IT IS
// ═══════════════════════════════════════════════════════════════

/// Whether the person opened this pane, rather than Syn.
///
/// # Two lifetimes, and the difference is who opened it
///
/// **The person opened it** — they pressed the globe. That is somebody who
/// wants a browser, and it stays until they close it. It follows them into
/// Notes and back, because a page you were reading should still be there when
/// you return, and having to rebuild it is the exact complaint that made this
/// pane worth building.
///
/// **Syn opened it** — it went to look something up. That belongs to the turn,
/// and it goes away when the turn does. `browser::close_when_done` already does
/// this for the separate window.
///
/// The rule is *who opened it*, not *which mini-app is showing*. A pane that
/// vanished on leaving Syn would rebuild the page every time somebody glanced
/// at their notes — which is the disease, not the cure.
///
/// # Why this exists before anything needs it
///
/// `browse` still uses the separate window; nothing here opens a pane on Syn's
/// behalf yet. But the day it does, the end of a run will reach for whatever
/// pane is on screen and close it — taking with it the page the person was
/// reading, mid-sentence, because they happened to ask a question. Writing the
/// rule down after that has happened means finding it first.
static THE_PERSONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Mark the pane as opened by the person, or by Syn.
pub fn opened_by_the_person(theirs: bool) {
    THE_PERSONS.store(theirs, std::sync::atomic::Ordering::Relaxed);
}

/// Whether Syn may close the pane when a run finishes.
///
/// No, if the person opened it. Their browser is not Syn's to tidy away at the
/// end of an answer.
pub fn syn_may_close_it() -> bool {
    !THE_PERSONS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Put the pane away and give the app its window back.
#[cfg(desktop)]
pub fn close<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AppResult<()> {
    use tauri::Manager;

    if let Some(pane) = app.get_webview(PANE) {
        let _ = pane.close();
    }
    // Closed, so it is nobody's until somebody opens one again.
    opened_by_the_person(false);
    set_title(String::new());
    remember_share(0.0);
    announce(app, 0.0);
    announce_page(app);
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
        let (px, py, pw, ph) = plan.pane.expect("there is room for both");

        assert_eq!(ax, 0);
        assert_eq!(px, aw as i32, "the pane starts exactly where the app ends");
        assert_eq!(aw + pw, 1600, "and together they are the window");
        assert_eq!(ah, 900, "the app's webview is the whole window; it draws itself narrower");

        // The one place they do not touch, and it is deliberate: the app draws
        // the pane's address bar in that strip. Everything below it is the
        // browser's, and the browser's webview would draw over anything this
        // app put there.
        assert_eq!(py, BAR as i32, "the bar's worth of room, and no more");
        assert_eq!(py as u32 + ph, 900, "and the pane reaches the bottom");
    }

    /// A window with no room for a bar has no room for a browser either, but
    /// the arithmetic still has to produce a rectangle rather than an underflow.
    #[test]
    fn a_window_shorter_than_the_bar_still_produces_a_rectangle() {
        let plan = layout(1600, 10, Some(SHARE));
        let (_, py, _, ph) = plan.pane.expect("width is what decides, not height");
        assert_eq!((py, ph), (10, 0));
    }

    /// Two numbers that have to agree, in two languages.
    ///
    /// Rust reserves the strip; the front end draws in it. A change to one and
    /// not the other is a bar that floats above the page or overlaps it, and
    /// neither shows up in a type check.
    #[test]
    fn the_bar_is_the_same_height_on_both_sides() {
        let ts = include_str!("../../../src/shared/syn/pane.ts");
        let wanted = format!("export const PANE_BAR = {BAR};");
        assert!(
            ts.contains(&wanted),
            "`pane::BAR` is {BAR}, so `pane.ts` must say `{wanted}`"
        );
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

    // ── whose pane it is ──────────────────────────────────────────

    /// The person's browser is not Syn's to tidy away at the end of an answer.
    ///
    /// This exists before anything needs it, which is the point. `browse` still
    /// uses the separate window — but the day it opens a pane instead, the end
    /// of a run will reach for whatever is on screen and close it, taking the
    /// page somebody was reading mid-sentence because they happened to ask a
    /// question. A rule written after that has happened is a rule written after
    /// somebody has hunted for the cause.
    #[test]
    fn syn_does_not_close_a_pane_the_person_opened() {
        opened_by_the_person(true);
        assert!(!syn_may_close_it());

        opened_by_the_person(false);
        assert!(syn_may_close_it(), "one Syn opened for itself is Syn's to close");
    }

    /// And the rule is *who opened it*, never *which mini-app is showing*.
    ///
    /// A pane that vanished on leaving Syn would rebuild the page every time
    /// somebody glanced at their notes — which is the disease this pane was
    /// built to cure, not the cure.
    #[test]
    fn nothing_here_knows_or_cares_which_screen_is_showing() {
        // The module, not the file: a test that reads its own source finds the
        // words it is looking for in its own list of them. This one did, on the
        // first run.
        let source = include_str!("pane.rs");
        let module = source.split("#[cfg(test)]").next().expect("there is a module");

        for screen in ["mini_app", "active_app", "current_app", "route"] {
            assert!(
                !module.contains(screen),
                "the pane's lifetime must not depend on `{screen}`"
            );
        }
    }
}
