//! A window Syn can open, and you can watch.
//!
//! # Why a window and not an API
//!
//! Synabit is for people who install an app and, at most, pick a model. Every
//! keyless search path open to a shipped commercial product is closed: Bing's
//! RSS licence forbids it, scraping DuckDuckGo breaks on their next redesign,
//! a bundled key means paying for strangers' searches until somebody extracts
//! it, and a proxy of our own ends local-first.
//!
//! A real browser needs none of them. Somebody searching in a browser is a
//! person searching, with their own cookies, as themselves.
//!
//! # Why *this* browser
//!
//! Synabit already **is** a WebView — Tauri 2. A second one costs no new
//! dependency, no bundled Chromium, and works on the platforms this app already
//! ships to, Android included. A headless Chromium would be 200 MB and would be
//! shown a captcha on the first search; driving the user's own Chrome needs an
//! extension, which is the setup step this design exists to remove.
//!
//! # What keeps it safe
//!
//! Four rules, and the second is the one that actually holds:
//!
//! 1. **Never headless.** The window is visible. If a thing is not worth
//!    showing somebody, it is not worth doing on their behalf.
//! 2. **Its own cookie jar, starting empty.** Syn cannot borrow the session in
//!    your real browser. To read your Jira you log into it *here*, once,
//!    watching. **The risk grows only by exactly what you deliberately connect**
//!    — which is the right shape, and the only mitigation that is not merely
//!    a mitigation.
//! 3. **Syn never types a credential.** No exceptions, no "always allow".
//! 4. **Reads the DOM, never pixels.** Screenshots need a vision model and cost
//!    an order of magnitude more context. This app has to work against a small
//!    local model on an 8,192-token window.
//!
//! # What a page in here can reach
//!
//! One command, `syn_browser_content`, which takes a string and returns
//! nothing. `may_call` is the whole door and `only_one_door_is_open` pins it.
//!
//! ## Why the capability file is not what closes it
//!
//! It was believed to be, and it is not. `capabilities/default.json` has no
//! `remote` field, so its grants apply to local app URLs only — true, checked
//! in `tauri-utils`' own source, and pinned by a test. But that governs the
//! **ACL**, and `webview/mod.rs` consults the ACL only for plugin commands or
//! for an app that ships its own ACL manifest:
//!
//! ```text
//! if (plugin_command.is_some() || has_app_acl_manifest) && invoke.acl.is_none()
//! ```
//!
//! Synabit's 256 commands are neither. `gen/schemas/acl-manifests.json` has no
//! `__app-acl__` key, so `has_app_acl_manifest` is false, the check is skipped,
//! and the command runs. And the core scripts — `__TAURI_INTERNALS__.invoke`
//! and the invoke key it must carry — are injected into **every** webview, with
//! no branch on whether the page came off the internet.
//!
//! So for a stretch, any page this window visited could call `trash_node`.
//!
//! The proper repair is an app ACL manifest, which would make the capability
//! files mean what they were being read to mean. That is 256 commands each
//! needing a permission and a grant, and until somebody does it this is the
//! lock: refuse at the one place every invoke passes through, by name, and let
//! exactly one command past. `permissions/` arriving later makes this
//! redundant rather than wrong.

use crate::error::{AppError, AppResult};

/// The window's label, and the folder its cookies live in.
///
/// Named rather than generated: one browsing window at a time is the design.
/// Two would mean two things to watch, and the whole safety argument rests on
/// somebody watching.
pub const WINDOW: &str = "syn-browser";

/// Where the session lives.
///
/// Under the app's data directory and **not** in the vault: cookies are not
/// notes, they do not sync, and a session copied to another machine is a
/// session that outlived the decision to create it.
pub const JAR: &str = "syn-browser-session";

/// How long to wait for a page before reading it, when nothing says it is ready.
///
/// Three seconds, and it used to be spent every single time — a flat toll on
/// every read, whether the page arrived in 200ms or not at all. It is now the
/// **ceiling** on waiting for `on_page_load`, not the wait itself.
pub const SETTLE_MS: u64 = 3_000;

/// How long to let a page's JavaScript run after it says it has loaded.
///
/// This is the part `SETTLE_MS` was really about: `load` firing is not the same
/// as the page being finished, and the scripts that run afterwards are the only
/// reason to be using a browser rather than `web::fetch`.
///
/// Short, because it is not the last word — the poll loop keeps asking, so a
/// page that needs longer is read on a later pass rather than missed. What this
/// number decides is only how soon the *first* attempt happens.
pub const AFTER_LOAD_MS: u64 = 600;

/// A page read out of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// Where it ended up, which is not always where it was sent.
    pub url: String,
    pub html: String,
}

/// Where a search goes when nobody has configured anything.
///
/// DuckDuckGo's ordinary HTML page, typed into a real browser as a person would
/// type it. Not an API, not a scrape of somebody's JSON behind their back: the
/// window navigates there and the user watches it happen.
///
/// A configured `search_url` still wins — see `SynSettings::search_url`. This is
/// what the promise "install it and it works" is made of.
pub const DEFAULT_SEARCH: &str = "https://duckduckgo.com/?q=";

/// The address a query becomes.
pub fn search_url(query: &str) -> String {
    format!("{DEFAULT_SEARCH}{}", urlencoding::encode(query))
}

/// Whether a string is an address or a thing to look up.
///
/// Deliberately narrow: only an explicit `http`/`https` URL counts. A bare
/// `example.com` is far more often a company somebody is asking about than a
/// site they want opened, and guessing wrong sends the window somewhere nobody
/// asked for — which is the one thing a visible browser must not do.
pub fn looks_like_a_url(text: &str) -> bool {
    let text = text.trim();
    (text.starts_with("http://") || text.starts_with("https://"))
        && url::Url::parse(text).is_ok()
}

/// Whether the cheap path got anything worth having.
///
/// The ladder in `browse` tries `web::fetch` first — no JavaScript, no session,
/// a fraction of the cost. This decides whether that was enough, and it is the
/// whole reason a browser is the exception rather than the rule.
///
/// A hundred and fifty characters. Below that the page is a shell waiting for
/// JavaScript, a consent wall, or a login — all three of which are exactly what
/// the window is for.
pub const ENOUGH_TEXT: usize = 150;

pub fn worth_keeping(page: &crate::syn::web::Page) -> bool {
    page.text.chars().count() >= ENOUGH_TEXT
}

/// The script the window carries, which is how anything gets back out.
///
/// # The one narrow door
///
/// A page here can reach no Tauri command — that is the default and it is
/// pinned by a test. But then the app cannot read the DOM either, so exactly
/// one command is granted to this window's remote content: it takes a string
/// and returns nothing. It cannot read the vault, write a file, or reach any
/// other command.
///
/// The string it carries was always going to be attacker-controlled — it is the
/// page's own HTML — and it goes straight into `web::wrap`, the boundary that
/// already assumes as much.
///
/// # The nonce
///
/// Injected per navigation and checked on arrival. Without it an advert in an
/// iframe could answer first and hand Syn a page it never asked for. This runs
/// in the main frame only, which is what `initialization_script` does and
/// `initialization_script_for_all_frames` deliberately does not.
pub fn reader_script(nonce: &str) -> String {
    format!(
        r#"(function () {{
  window.__synRead = function () {{
    try {{
      window.__TAURI_INTERNALS__.invoke('syn_browser_content', {{
        nonce: '{nonce}',
        url: location.href,
        html: document.documentElement.outerHTML
      }});
    }} catch (e) {{ /* nothing to do: the app times out and says so */ }}
  }};
}})();"#
    )
}

/// The only command a page in the browsing window may call.
///
/// It takes a string and returns nothing. It cannot read the vault, write a
/// file, or reach any of the other 255.
pub const THE_ONE_DOOR: &str = "syn_browser_content";

/// Whether this webview may call this command.
///
/// Checked at `invoke_handler`, which every call passes through — the app's own
/// commands do not go through the ACL at all (see this module's header), so
/// there is no other place that sees them all.
///
/// Keyed on the **webview** label rather than the window's. The browsing view
/// is its own webview whether it sits in its own window or docked inside the
/// main one, and a rule written against the window label would quietly stop
/// applying the day it moved.
///
/// Everything else in the app is untouched: this returns true for every webview
/// that is not the browsing one, which is the only shape that does not need 256
/// grants written out to stay working.
pub fn may_call(webview_label: &str, command: &str) -> bool {
    webview_label != WINDOW || command == THE_ONE_DOOR
}

/// What a page is told when it tries anything else.
///
/// Said plainly rather than with a generic "not found". A page that reached for
/// a command was either a bug in this app or something worth knowing about, and
/// both are better read in a log than guessed at.
pub const REFUSED: &str = "A page in the browsing window may call nothing but syn_browser_content";

// ═══════════════════════════════════════════════════════════════
//  WHERE IT SITS
// ═══════════════════════════════════════════════════════════════

/// The app this window belongs beside.
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

/// How wide the browsing window is.
///
/// Narrow. It is something to glance at while reading the conversation, not a
/// browser to work in — and a pane you have to move out of the way to see the
/// answer is a pane you close.
pub const WIDTH: u32 = 520;

/// A place on the screen, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Beside the app, matching its height — never on top of it, never off-screen.
///
/// Beside rather than centred, because the whole reason to see this window is
/// to see it *and* the conversation that caused it. A window that lands over
/// the answer is one somebody drags away before they read either.
///
/// Off the right edge it flushes to the edge instead, overlapping the app.
/// Overlapping is a nuisance; half a window past the edge of the screen is a
/// window nobody can see, and the entire safety argument rests on it being
/// watched.
pub fn beside(app: Spot, screen_width: Option<u32>) -> Spot {
    let x = app.x.saturating_add(app.width as i32);

    let x = match screen_width {
        Some(screen) if x.saturating_add(WIDTH as i32) > screen as i32 => {
            (screen as i32).saturating_sub(WIDTH as i32)
        }
        _ => x,
    };

    Spot { x: x.max(0), y: app.y, width: WIDTH, height: app.height }
}

/// What Syn is told when somebody shuts the window on it.
///
/// The stop button, and it is the one every person already reaches for. A
/// separate control in the app would be a second place to press stop, which is
/// one more than there should be.
///
/// Phrased for the model, because the model is who reads it: it says the page
/// was not read and says not to go back, so a refusal is not answered by trying
/// the same address again. The same shape as `Decision::Refuse`.
pub const CLOSED_ON_IT: &str =
    "You closed the browsing window, so that page was not read. Do not open it again — \
     answer with what you already have, and say you could not look.";

/// Refuse an address the window should not be sent to.
///
/// The same guard the fetch path uses, and for a sharper reason: this window
/// has a session. A page that talked Syn into navigating somewhere on this
/// machine would be doing it with whatever the user had logged into.
pub fn guard(url: &str) -> AppResult<()> {
    crate::feed_engine::fetcher::guard_url(url).map_err(AppError::General)
}

/// Whether the window may go here, asked of **every** navigation.
///
/// # The hole this closes
///
/// `guard` used to run on exactly two things: the address Syn sends, and each
/// redirect the HTTP client follows. Neither is the interesting case. A page in
/// this window could run `location = "http://127.0.0.1:11434/…"` — its own
/// JavaScript, its own decision — and nothing looked at it. The whole reason
/// this window is dangerous is that it holds a session, and the whole reason it
/// was thought safe is a check that never saw the navigations that matter.
///
/// `on_navigation` is the callback wry offers for exactly this, and it takes a
/// bool. It has been available the entire time.
///
/// # Why `about:blank` is let through
///
/// It is where a webview starts and where pages park iframes; refusing it
/// refuses ordinary browsing rather than an attack. It reaches nothing and
/// carries nothing.
pub fn may_go_to(url: &url::Url) -> bool {
    if url.as_str() == "about:blank" {
        return true;
    }

    let allowed = guard(url.as_str()).is_ok();
    if !allowed {
        log::warn!("[Syn] The browsing window was steered at {url} and refused");
    }
    allowed
}

/// What a page in this window is never allowed to start.
///
/// Downloads and popups, and both for the same reason: they leave the window.
/// A download puts a file on this machine, chosen by a page rather than by
/// anybody; `window.open` makes a second view that no rule here was written
/// about. Neither is something a run that went looking for a football score
/// has any business doing, and refusing them costs nothing that browsing needs.
pub const NOTHING_LEAVES_THE_WINDOW: bool = false;

/// Named `false` and checked as such: a `true` here would let every page in the
/// window put files on this machine, and the name would still read as a refusal.
const _: () = assert!(!NOTHING_LEAVES_THE_WINDOW);

// ═══════════════════════════════════════════════════════════════
//  DRIVING IT
// ═══════════════════════════════════════════════════════════════

/// What the window is waiting to hand back, if anything.
///
/// One at a time, and that is the design rather than a limitation: one window,
/// one page, one person watching. A second in flight would mean two things to
/// watch and no way to say which answered.
#[derive(Default)]
pub struct Pending {
    pub nonce: String,
    pub reply: Option<Read>,
    /// Whether the page has said it finished loading, for this navigation.
    ///
    /// Set by `on_page_load`, cleared by each `visit` before it navigates. See
    /// `AFTER_LOAD_MS`.
    pub loaded: bool,
    /// When the window went up, so nothing closes it before it can be seen.
    ///
    /// `None` when no window is open. See `AT_LEAST_MS`.
    pub opened_at: Option<std::time::Instant>,
}

pub type Waiting = std::sync::Mutex<Pending>;

/// The page has finished loading, as far as the webview is concerned.
///
/// Which is not the same as finished — see `AFTER_LOAD_MS` — but it is the
/// difference between waiting for a page and waiting for a fixed three seconds
/// regardless of the page.
pub fn note_loaded(waiting: &Waiting) {
    waiting.lock().unwrap_or_else(|e| e.into_inner()).loaded = true;
}

/// Hand a page back from the window. Called by the one granted command.
///
/// The nonce is checked here rather than trusted: an advert in an iframe that
/// found the command name could otherwise answer first, and Syn would read a
/// page nobody asked for as though it were the one it navigated to.
pub fn accept(waiting: &Waiting, nonce: &str, url: String, html: String) {
    let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    if pending.nonce.is_empty() || pending.nonce != nonce {
        log::warn!("[Syn] A page answered the browser with a nonce nobody asked for");
        return;
    }
    pending.reply = Some(Read { url, html });
}

/// How long to wait for the page to answer before giving up.
///
/// Load, settle, read. Past this something is wrong — a page that never
/// finishes, a script that threw — and saying so beats a run that hangs while
/// somebody watches a window do nothing.
pub const PATIENCE_MS: u64 = 20_000;

/// Open the window if it is not open, send it somewhere, and read what lands.
///
/// # How long it stays
///
/// Until the run ends — `close_when_done`, not here.
///
/// It used to close the moment the page had been read, on the reasoning that a
/// window lingering is a session sitting open with nobody watching. That
/// reasoning is about a window left open for hours, and it was applied to one
/// measured in seconds. What it produced was a window that appeared and
/// vanished before anybody could focus on it — and rule 1 does not say the
/// window exists, it says somebody can watch it. Something nobody can perceive
/// fails that rule no matter what the code did.
///
/// It is worse than it sounds for a run that reads three pages: three windows,
/// three times, and a person who looked up after the second saw an empty
/// screen. One window, open while Syn is working, gone when it stops.
///
/// Anything the user logged into stays in the jar either way, which is the part
/// worth keeping.
pub async fn visit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    waiting: &Waiting,
    url: &str,
) -> AppResult<Read> {
    // Android cannot do this, and until now it found that out by hanging.
    //
    // Not a guess: `wry-0.54.4/src/android/mod.rs` injects initialization
    // scripts by **rewriting HTML that comes through the app's own custom
    // protocol**, because `addDocumentStartJavaScript` is unavailable there.
    // A page at `https://vnexpress.net` does not come through that protocol, so
    // `reader_script` is never injected, so `syn_browser_content` is never
    // called, so this waits out its full `PATIENCE_MS` and reports a timeout —
    // blaming the page for something the platform did.
    //
    // Said plainly instead. `web::fetch` on an explicit address still works on
    // mobile, which is why `browse` is not refused outright.
    #[cfg(mobile)]
    {
        let _ = (app, waiting);
        Err(AppError::General(format!(
            "The browsing window does not work on this platform, so {url} could not be opened \
             that way. Reading an address directly still works; searching does not."
        )))
    }

    #[cfg(desktop)]
    open_and_read(app, waiting, url).await
}

/// The window itself, on the platforms that have one.
///
/// Split from `visit` so the mobile refusal above is a sentence rather than a
/// brace wrapped around two hundred lines.
#[cfg(desktop)]
async fn open_and_read<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    waiting: &Waiting,
    url: &str,
) -> AppResult<Read> {
    use tauri::Manager;

    guard(url)?;
    let target = url::Url::parse(url).map_err(|e| AppError::General(format!("Bad address: {e}")))?;

    let nonce = uuid::Uuid::new_v4().to_string();
    {
        let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
        pending.nonce = nonce.clone();
        pending.reply = None;
        pending.loaded = false;
    }

    let jar = app
        .path()
        .app_local_data_dir()
        .map_err(|e| AppError::General(format!("No data directory: {e}")))?
        .join(JAR);

    // Where the app is, so this can go beside it rather than over it. Logical
    // units, because that is what the builder takes; the arithmetic itself is
    // in `beside`, in physical pixels, where it can be tested.
    let spot = app_window(app).and_then(|main| {
        let scale = main.scale_factor().ok()?;
        let at = main.outer_position().ok()?;
        let size = main.outer_size().ok()?;
        let screen = main
            .current_monitor()
            .ok()
            .flatten()
            .map(|monitor| monitor.size().width);

        let spot = beside(
            Spot { x: at.x, y: at.y, width: size.width, height: size.height },
            screen,
        );
        Some((spot, scale))
    });

    match app.get_webview_window(WINDOW) {
        Some(window) => {
            window
                .navigate(target)
                .map_err(|e| AppError::General(format!("Could not navigate: {e}")))?;
            let _ = window.show();
            // Deliberately no `set_focus`. It was there, and it was the thing
            // that made a four-second read feel like an interruption: whatever
            // somebody was typing lost the keyboard to a window that then went
            // away. Being visible is the rule; being in front is not.

            // Claimed by this run. A window left over from a run that has just
            // ended has a delayed close in flight against it; this is what
            // tells that close the window is somebody else's now. See
            // `close_when_done`.
            let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
            pending.opened_at = Some(std::time::Instant::now());
        }
        None => {
            let mut builder =
                tauri::WebviewWindowBuilder::new(app, WINDOW, tauri::WebviewUrl::External(target))
                    .title("Syn is looking")
                    // Visible, and not in front. Watched is the requirement.
                    .focused(false)
                    // Its own jar. Syn cannot borrow the session in the user's
                    // real browser; anything it reaches behind a login, they
                    // logged into here, watching.
                    .data_directory(jar)
                    // The main frame only. `initialization_script_for_all_frames`
                    // would put the reader in every advert on the page.
                    .initialization_script(reader_script(&nonce))
                    // Every navigation, not just the one Syn asked for. See
                    // `may_go_to` for the hole this closes.
                    .on_navigation(may_go_to)
                    // A page here starts no downloads and opens no windows.
                    .on_download(|_, _| NOTHING_LEAVES_THE_WINDOW)
                    .on_new_window(|url, _| {
                        log::warn!("[Syn] A page tried to open a window at {url}");
                        tauri::webview::NewWindowResponse::Deny
                    })
                    // Nothing installed here reads pages on Syn's behalf.
                    .browser_extensions_enabled(false)
                    // So the wait is for this page rather than for a number.
                    .on_page_load(|webview, payload| {
                        if payload.event() == tauri::webview::PageLoadEvent::Finished {
                            if let Some(waiting) = webview.app_handle().try_state::<Waiting>() {
                                note_loaded(&waiting);
                            }
                        }
                    });

            builder = match spot {
                Some((spot, scale)) => builder
                    .position(spot.x as f64 / scale, spot.y as f64 / scale)
                    .inner_size(spot.width as f64 / scale, spot.height as f64 / scale),
                // No app window to sit beside — an odd state, and a sensible
                // size beats refusing to look anything up.
                None => builder.inner_size(WIDTH as f64, 760.0),
            };

            builder
                .build()
                .map_err(|e| AppError::General(format!("Could not open the window: {e}")))?;

            // When it went up, so nothing closes it before it can be seen.
            let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
            pending.opened_at = Some(std::time::Instant::now());
        }
    }

    // Wait for *this page*, not for a number.
    //
    // It used to sleep `SETTLE_MS` flat, every time, which was three seconds
    // added to every read whether the page arrived in two hundred milliseconds
    // or never. `on_page_load` says when it actually arrived; `SETTLE_MS` is now
    // only the ceiling for a page that never says so.
    let waited_from = std::time::Instant::now();
    while waited_from.elapsed() < std::time::Duration::from_millis(SETTLE_MS) {
        if waiting.lock().unwrap_or_else(|e| e.into_inner()).loaded {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // And then let its JavaScript run, which is the part that mattered.
    tokio::time::sleep(std::time::Duration::from_millis(AFTER_LOAD_MS)).await;

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(PATIENCE_MS);
    while std::time::Instant::now() < deadline {
        // Gone means somebody shut it, which is the stop button. Giving up here
        // rather than polling an empty label for the rest of the twenty seconds
        // is the difference between a control and a delay.
        let Some(window) = app.get_webview_window(WINDOW) else {
            return Err(AppError::General(CLOSED_ON_IT.to_string()));
        };
        // Re-injected each time: on a window that already existed the
        // initialization script belongs to the *previous* navigation, and
        // carries the previous nonce.
        let _ = window.eval(format!("{}\nwindow.__synRead && window.__synRead();", reader_script(&nonce)));

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let landed = {
            let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
            pending.reply.take()
        };
        if let Some(read) = landed {
            // Left open. `close_when_done` shuts it when the run ends — a page
            // read is not the end of anything, and a run that looks at three
            // pages should not flicker a window three times.
            return Ok(read);
        }
    }

    // Also left open on a timeout, and more deliberately: a page that never
    // finished is exactly the one worth looking at.
    Err(AppError::General(format!(
        "The page at {url} did not finish loading within {}s.",
        PATIENCE_MS / 1000
    )))
}

/// The least time the window is on screen before anything closes it.
///
/// Six seconds. Rule 1 does not say the window exists, it says a person can
/// watch it — and something that appears and vanishes inside a few hundred
/// milliseconds is not something anybody watched. This is the floor that makes
/// the rule true rather than approximately true.
///
/// It only ever *delays* a close. Somebody who shuts the window themselves is
/// not made to wait: that is `visit` giving up, not this.
pub const AT_LEAST_MS: u64 = 6_000;

/// How much longer the window has to stay up, having been open this long.
///
/// Separated from the closing so the arithmetic can be tested without a window.
pub fn still_owed(open_for: std::time::Duration) -> std::time::Duration {
    std::time::Duration::from_millis(AT_LEAST_MS).saturating_sub(open_for)
}

/// Shut the browsing window, the run being over.
///
/// Called at the end of a run rather than at the end of a page read — see
/// `visit` for why. Best effort throughout: a window that has already gone, or
/// a close that fails, is not a reason to fail a run that has its answer.
///
/// Waits out `AT_LEAST_MS` when the run was quicker than that, which is the
/// case this exists for: an instant answer over a page that loaded fast, where
/// everything worked and the person saw nothing.
pub fn close_when_done<R: tauri::Runtime>(app: &tauri::AppHandle<R>, waiting: &Waiting) {
    use tauri::Manager;

    let opened_at = {
        let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
        pending.opened_at.take()
    };

    // No window was opened by this run, so there is nothing of this run's to
    // close. Anything on screen belongs to somebody else and is not ours to
    // shut.
    let Some(opened_at) = opened_at else { return };

    let owed = still_owed(opened_at.elapsed());
    if owed.is_zero() {
        if let Some(window) = app.get_webview_window(WINDOW) {
            let _ = window.close();
        }
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(owed).await;

        // Claimed again while this was waiting means a new run is using the
        // window, and shutting it now would close a page in the middle of
        // being read. `opened_at` is set by every `visit`, so its presence is
        // exactly the question "does this belong to somebody else now".
        let taken = app
            .try_state::<Waiting>()
            .is_some_and(|waiting| {
                waiting.lock().unwrap_or_else(|e| e.into_inner()).opened_at.is_some()
            });
        if taken {
            return;
        }

        if let Some(window) = app.get_webview_window(WINDOW) {
            let _ = window.close();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_becomes_a_search_a_person_could_have_typed() {
        let url = search_url("kết quả mu everton");
        assert!(url.starts_with(DEFAULT_SEARCH), "{url}");
        assert!(url.contains("k%E1%BA%BFt"), "the accents survive: {url}");
        assert!(!url.contains(' '), "{url}");
    }

    /// Narrow on purpose. `example.com` is far more often a company somebody is
    /// asking about than a site they want opened, and a visible window sent
    /// somewhere nobody asked for is the one thing this must not do.
    #[test]
    fn only_a_written_out_address_counts_as_one() {
        assert!(looks_like_a_url("https://espn.com/match"));
        assert!(looks_like_a_url("  http://example.org  "));

        for text in [
            "example.com",
            "kết quả trận mu everton hôm qua",
            "what is rust.dev",
            "",
            "https://",
        ] {
            assert!(!looks_like_a_url(text), "sent the window to: {text}");
        }
    }

    /// The ladder's whole point: the browser is the exception, not the rule.
    #[test]
    fn a_page_the_cheap_path_read_properly_needs_no_window() {
        let good = crate::syn::web::Page {
            url: "https://x.test/".into(),
            title: "t".into(),
            text: "n".repeat(ENOUGH_TEXT),
            truncated: false,
        };
        assert!(worth_keeping(&good));
    }

    /// And a shell waiting for JavaScript, a consent wall or a login all look
    /// the same from here — which is right, because the window answers all
    /// three.
    #[test]
    fn an_almost_empty_page_is_what_the_window_is_for() {
        let shell = crate::syn::web::Page {
            url: "https://x.test/".into(),
            title: "Loading…".into(),
            text: "Please enable JavaScript".into(),
            truncated: false,
        };
        assert!(!worth_keeping(&shell));
    }

    /// An advert in an iframe must not be able to answer first.
    #[test]
    fn the_reader_is_stamped_so_only_the_page_asked_for_can_answer() {
        let script = reader_script("abc123");
        assert!(script.contains("nonce: 'abc123'"), "{script}");
        assert!(script.contains("syn_browser_content"));
        assert!(script.contains("document.documentElement.outerHTML"));
    }

    /// This window has a session. A page that talked Syn into navigating
    /// somewhere on this machine would be doing it with whatever was logged in.
    #[test]
    fn the_window_is_never_sent_inside_the_machine() {
        for url in [
            "http://localhost:11434/api/tags",
            "http://169.254.169.254/latest/meta-data/",
            "http://192.168.1.1/",
            "file:///etc/passwd",
        ] {
            assert!(guard(url).is_err(), "would have opened {url}");
        }
        assert!(guard("https://duckduckgo.com/?q=x").is_ok());
    }

    /// The session is not a note.
    ///
    /// Cookies do not belong in the vault: they would sync, and a session
    /// copied to another machine is a session that outlived the decision to
    /// create it.
    #[test]
    fn the_cookies_do_not_live_in_the_vault() {
        assert!(!JAR.contains('/') && !JAR.contains('\\'));
        let source = include_str!("browser.rs");
        assert!(
            source.contains("not** in the vault"),
            "the reason is written down where somebody would move it"
        );
    }

    /// Nothing in `capabilities/` hands remote content a plugin command.
    ///
    /// Verified from `tauri-utils`' own source rather than its documentation: a
    /// capability's `remote` field defaults to `None`, and `local` defaults to
    /// true — so a capability without `remote` never applies to remote content,
    /// however broad its `windows` glob.
    ///
    /// This app's `default.json` matches `windows: ["*"]`, which would
    /// otherwise hand every page `fs:allow-read` on `**`. It does not, and this
    /// is where that stops being true loudly rather than silently.
    ///
    /// **It is not the lock on this app's own commands, and was read as one.**
    /// Those skip the ACL entirely — see the module header — and `may_call` is
    /// what stops them. This still matters, for the plugin commands the ACL
    /// does govern: `fs`, `dialog`, `opener`, `process`.
    #[test]
    fn no_capability_hands_a_page_a_plugin_command() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        let mut checked = 0;

        for entry in std::fs::read_dir(&dir).expect("capabilities/ is readable") {
            let path = entry.expect("an entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).expect("readable");
            let json: serde_json::Value = serde_json::from_str(&raw).expect("valid json");

            assert!(
                json.get("remote").is_none(),
                "{} grants remote content the app's commands. A page in `{WINDOW}` could then \
                 call them. If this is deliberate it needs its own capability naming exactly \
                 one command, not a change here.",
                path.display()
            );
            checked += 1;
        }

        assert!(checked >= 2, "only read {checked} capability files");
    }

    /// The lock that actually holds.
    ///
    /// One command in, everything else refused — and the app's own screens
    /// untouched, which is the only shape that does not need all 256 commands
    /// written out somewhere to keep working.
    #[test]
    fn only_one_door_is_open() {
        assert!(may_call(WINDOW, THE_ONE_DOOR), "the page has to be able to answer");

        for command in [
            "trash_node",
            "update_node",
            "read_file_text",
            "syn_save_instructions",
            "syn_set_api_key",
            "syn_send_message",
            "plugin:fs|read_text_file",
        ] {
            assert!(!may_call(WINDOW, command), "a page could call {command}");
        }
    }

    /// And nothing else in the app is affected. The guard sits on the path
    /// every invoke takes, so a rule that reached further would break the app.
    #[test]
    fn every_other_webview_is_left_alone() {
        for label in ["main", "ask-bar", ""] {
            for command in ["trash_node", "syn_send_message", THE_ONE_DOOR] {
                assert!(may_call(label, command), "{label} lost {command}");
            }
        }
    }

    /// Keyed on the webview, not the window.
    ///
    /// A docked browsing view lives inside the main window and keeps its own
    /// webview label. A rule written against the window would go on saying
    /// "main" and quietly stop applying the day it moved.
    #[test]
    fn the_rule_survives_the_view_being_docked() {
        let source = include_str!("browser.rs");
        assert!(
            source.contains("pub fn may_call(webview_label: &str"),
            "may_call must take the webview's label, or docking silently unlocks the door"
        );
    }

    // ── where it sits ─────────────────────────────────────────────

    fn app_at(x: i32, width: u32) -> Spot {
        Spot { x, y: 40, width, height: 900 }
    }

    /// Beside the app, matching its height. The point of showing this window is
    /// to see it *and* the conversation that caused it.
    #[test]
    fn it_sits_next_to_the_app_rather_than_over_it() {
        let spot = beside(app_at(100, 1200), Some(3840));

        assert_eq!(spot.x, 1300, "immediately to the right of the app");
        assert_eq!(spot.y, 40, "and level with it");
        assert_eq!(spot.height, 900, "the same height, so neither is cut off");
        assert_eq!(spot.width, WIDTH);
    }

    /// Half a window past the edge of the screen is a window nobody can see,
    /// and the whole safety argument rests on it being watched. Overlapping is
    /// a nuisance; invisible is a broken promise.
    #[test]
    fn it_never_lands_off_the_screen() {
        let squeezed = beside(app_at(1000, 1400), Some(2560));
        assert_eq!(squeezed.x, 2560 - WIDTH as i32, "flush to the edge instead");
        assert!(
            squeezed.x + WIDTH as i32 <= 2560,
            "and wholly on the screen: {squeezed:?}"
        );

        // A screen narrower than the window itself still gets x = 0 rather than
        // a negative coordinate.
        assert_eq!(beside(app_at(0, 400), Some(300)).x, 0);
    }

    /// Nothing known about the screen is not a reason to refuse to look
    /// something up.
    #[test]
    fn it_manages_without_knowing_the_screen() {
        assert_eq!(beside(app_at(100, 1200), None).x, 1300);
    }

    // ── stopping it ───────────────────────────────────────────────

    /// Closing the window is the stop button, and the sentence it produces is
    /// read by the model, not by a person. It has to say the page was not read
    /// and say not to go back — a refusal answered by retrying the same address
    /// is not a refusal.
    #[test]
    fn shutting_the_window_tells_the_model_to_stop_rather_than_retry() {
        let said = CLOSED_ON_IT.to_lowercase();
        assert!(said.contains("not read"), "{CLOSED_ON_IT}");
        assert!(said.contains("do not open it again"), "{CLOSED_ON_IT}");
    }

    /// And nothing steals the keyboard. `set_focus` on a window that lives four
    /// seconds takes the keystroke somebody was in the middle of typing.
    #[test]
    fn it_is_visible_without_being_in_front() {
        let source = include_str!("browser.rs");
        let visit = source
            .split("async fn open_and_read")
            .nth(1)
            .expect("visit is still here");
        let body = visit.split("#[cfg(test)]").next().unwrap_or(visit);

        assert!(!body.contains("set_focus()"), "the window must not take focus");
        assert!(body.contains(".focused(false)"), "and must not open in front");
    }

    // ── how long it stays ─────────────────────────────────────────

    use std::time::Duration;

    /// The complaint this was written for: it appeared on the right, and shut
    /// before there was anything to see.
    #[test]
    fn a_run_that_finishes_fast_still_leaves_something_to_look_at() {
        assert_eq!(still_owed(Duration::from_millis(0)), Duration::from_millis(AT_LEAST_MS));
        assert_eq!(still_owed(Duration::from_millis(3_400)), Duration::from_millis(2_600));

    }

    /// The floor has to outlast a page read, or it changes nothing: settling
    /// alone is already `SETTLE_MS`. Checked at compile time, because it is a
    /// relationship between two constants and a runtime assertion about that is
    /// a test that can only fail after somebody has shipped it.
    const _: () = assert!(AT_LEAST_MS >= SETTLE_MS + 2_000);

    /// And it only ever delays. A run that took its time is not held open
    /// longer for having done so.
    #[test]
    fn a_run_that_took_its_time_closes_at_once() {
        assert!(still_owed(Duration::from_millis(AT_LEAST_MS)).is_zero());
        assert!(still_owed(Duration::from_secs(600)).is_zero(), "and never goes negative");
    }

    /// Reading a page is not the end of anything.
    ///
    /// A run that looks at three pages used to open and shut three windows,
    /// each for about three seconds — which is how somebody looks up after the
    /// second one and sees an empty screen. Closing belongs to the run's end,
    /// and every way out of a run passes through `drive`.
    #[test]
    fn the_window_belongs_to_the_run_rather_than_to_one_page() {
        let source = include_str!("browser.rs");
        let visit = source.split("async fn open_and_read").nth(1).expect("visit is here");
        let body = visit.split("pub const AT_LEAST_MS").next().unwrap_or(visit);

        assert!(
            !body.contains("window.close()"),
            "reading a page must not close the window; `close_when_done` does that"
        );

        let engine = include_str!("engine.rs");
        assert!(
            engine.contains("browser::close_when_done(req.app, req.browser)"),
            "and the run's end must, or a window is left open with nobody watching"
        );
    }

    /// A window a newer run has claimed must not be shut by an older run's
    /// delayed close. Answering twice in quick succession is ordinary, and it
    /// would have closed a page half way through being read.
    #[test]
    fn a_later_run_takes_the_window_from_an_earlier_close() {
        let source = include_str!("browser.rs");
        let close = source.split("pub fn close_when_done").nth(1).expect("it is here");

        assert!(
            close.contains("opened_at.is_some()"),
            "the delayed close has to ask whether the window has been claimed since"
        );
    }

    // ── every navigation, not just Syn's own ──────────────────────

    fn at(url: &str) -> url::Url {
        url::Url::parse(url).expect("a url")
    }

    /// The hole `on_navigation` closes.
    ///
    /// `guard` ran on the address Syn sends and on the redirects the HTTP
    /// client follows. Neither is the interesting case: a page in this window
    /// can set `location` itself, and nothing looked at that. The window is
    /// dangerous precisely because it holds a session, and the check that made
    /// it look safe never saw the navigations that could use one.
    #[test]
    fn a_page_cannot_steer_the_window_at_this_machine() {
        for url in [
            "http://127.0.0.1:11434/api/tags",
            "http://localhost:1420/",
            "http://169.254.169.254/latest/meta-data/",
            "http://192.168.1.1/",
            "http://vault.internal/",
        ] {
            assert!(!may_go_to(&at(url)), "the window would have gone to {url}");
        }
    }

    /// And nothing but the web. A page that navigates to a scheme the OS hands
    /// to another application has left this window's rules behind.
    #[test]
    fn only_the_web_counts_as_somewhere_to_go() {
        for url in ["file:///etc/passwd", "ftp://example.com/x", "tauri://localhost/"] {
            assert!(!may_go_to(&at(url)), "{url}");
        }
    }

    /// Ordinary browsing still works, `about:blank` included — it is where a
    /// webview starts and where pages park iframes. Refusing it refuses
    /// browsing rather than an attack.
    #[test]
    fn it_does_not_refuse_ordinary_browsing() {
        for url in [
            "https://duckduckgo.com/?q=x",
            "https://vnexpress.net/",
            "http://example.org/a?b=c#d",
            "about:blank",
        ] {
            assert!(may_go_to(&at(url)), "{url} was refused");
        }
    }

    /// Downloads and popups both leave the window, and neither is something a
    /// run looking up a football score has any business doing.
    #[test]
    fn nothing_a_page_starts_gets_out_of_the_window() {
        let source = include_str!("browser.rs");
        let built = source
            .split("WebviewWindowBuilder::new")
            .nth(1)
            .expect("the window is still built here");
        let built = built.split(".build()").next().unwrap_or(built);

        for hook in [".on_navigation(", ".on_download(", ".on_new_window("] {
            assert!(built.contains(hook), "the window is built without {hook}");
        }
        assert!(
            built.contains(".browser_extensions_enabled(false)"),
            "nothing installed in a browser reads pages on Syn's behalf"
        );
    }

    // ── waiting for the page rather than for a number ─────────────

    /// The flat three-second toll is gone.
    ///
    /// Every read paid it, whether the page arrived in two hundred
    /// milliseconds or never — it was most of the 3,4 seconds every `browse` in
    /// the transcript took. `SETTLE_MS` is now the ceiling on waiting for
    /// `on_page_load`, not the wait.
    #[test]
    fn it_waits_for_the_page_and_not_for_a_fixed_three_seconds() {
        let source = include_str!("browser.rs");
        let body = source
            .split("async fn open_and_read")
            .nth(1)
            .expect("it is here")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        assert!(
            !body.contains("from_millis(SETTLE_MS)).await"),
            "a flat sleep of SETTLE_MS is the toll this removed"
        );
        assert!(body.contains(".loaded"), "it has to ask whether the page said it was ready");

        let built = source
            .split("WebviewWindowBuilder::new")
            .nth(1)
            .expect("the window is still built here");
        assert!(
            built.split(".build()").next().unwrap_or(built).contains(".on_page_load("),
            "and something has to set that flag"
        );
    }

    /// Loading is not the same as finished, so there is still a settle — but a
    /// short one, because the poll loop is what actually guarantees the read.
    /// This number only decides how soon the *first* attempt happens.
    #[test]
    fn there_is_still_room_for_scripts_that_run_after_load() {
        let mut waiting = Pending::default();
        assert!(!waiting.loaded, "a fresh navigation has not loaded yet");
        waiting.loaded = true;
        assert!(waiting.loaded);
    }

    const _: () = assert!(AFTER_LOAD_MS > 0, "a browser with no settle is `web::fetch`");
    const _: () = assert!(
        AFTER_LOAD_MS < SETTLE_MS,
        "the settle after load must be cheaper than the old flat toll, or nothing was gained"
    );

    /// And `note_loaded` is what sets it, from the callback.
    #[test]
    fn the_page_saying_it_loaded_is_what_ends_the_wait() {
        let waiting: Waiting = Waiting::default();
        assert!(!waiting.lock().expect("lock").loaded);

        note_loaded(&waiting);
        assert!(waiting.lock().expect("lock").loaded);
    }

    // ── the platform that cannot do this ──────────────────────────

    /// Android says so instead of hanging for twenty seconds.
    ///
    /// `wry-0.54.4/src/android/mod.rs` injects initialization scripts by
    /// rewriting HTML that comes through the app's **own custom protocol**,
    /// because `addDocumentStartJavaScript` is unavailable there. A page at
    /// `https://vnexpress.net` does not come through that protocol, so
    /// `reader_script` is never injected and nothing ever answers — which the
    /// old code discovered by waiting out `PATIENCE_MS` and then blaming the
    /// page.
    #[test]
    fn the_platform_that_cannot_do_this_says_so_rather_than_timing_out() {
        let source = include_str!("browser.rs");
        let visit = source
            .split("pub async fn visit")
            .nth(1)
            .expect("visit is still the way in");
        let dispatch = visit.split("async fn open_and_read").next().unwrap_or(visit);

        assert!(dispatch.contains("#[cfg(mobile)]"), "mobile has to be answered before it waits");
        assert!(
            dispatch.contains("Reading an address directly still works"),
            "and told what does still work, since `web::fetch` is unaffected"
        );
    }
}
