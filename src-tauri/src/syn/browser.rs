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

/// The file endings that are also country codes.
///
/// `.md` is Moldova, `.rs` is Serbia, `.sh` is Saint Helena, `.py` is Paraguay,
/// `.pl` is Poland. This vault is written in Markdown and this app is written
/// in Rust and TypeScript, so `notes.md` and `engine.rs` are strings that
/// genuinely turn up around here — and a rule that reads a bare host as an
/// address would send a visible browser, carrying a session, to Moldova.
///
/// The ones that are not TLDs at all are listed too. They cost nothing and they
/// say what the list is for.
const NOT_SITES: &[&str] = &[
    "md", "rs", "ts", "js", "py", "pl", "sh", "json", "toml", "yaml", "yml", "txt", "csv", "log",
    "lock", "css", "html", "png", "jpg", "jpeg", "pdf", "svg", "vue",
];

/// The address a person named, if they named one.
///
/// # The question this answers, which is not the one above
///
/// `looks_like_a_url` asks *is this already an address*. This asks the question
/// the transcript actually posed: somebody said **"go to vnexpress"**, the model
/// passed `vnexpress.net`, and Syn took a domain name to a search engine and
/// asked it to find the site whose address it was already holding. Two
/// navigations, a page of results, and a chance to pick the wrong one — to
/// reach a place it could have gone to directly.
///
/// # Why it is still narrow
///
/// The old comment was right that a bare `example.com` is often a company
/// somebody is asking about rather than a site they want opened, and that
/// guessing wrong sends a **visible** browser somewhere nobody asked for. So
/// this fires only on a string that is nothing *but* a host — one token, no
/// spaces, a real-looking label and ending. `mu everton kết quả` is a question;
/// `bongdanet.co` is a place.
///
/// A `/path` is allowed after the host, because half the addresses anybody
/// names have one and dropping it would land on a front page instead.
pub fn address_of(text: &str) -> Option<String> {
    let text = text.trim();
    if looks_like_a_url(text) {
        return Some(text.to_string());
    }

    // One token. Anything with a space in it is a sentence, and a sentence is a
    // question no matter how much of it looks like a domain.
    if text.is_empty() || text.split_whitespace().count() != 1 {
        return None;
    }
    // `user@host` is an address of the other kind.
    if text.contains('@') || text.contains("..") || text.contains("://") {
        return None;
    }

    let (host, path) = match text.find('/') {
        Some(at) => (&text[..at], &text[at..]),
        None => (text, ""),
    };
    let host = host.to_ascii_lowercase();

    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 || labels.iter().any(|l| l.is_empty()) {
        return None;
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return None;
    }

    let tld = labels[labels.len() - 1];
    let looks_like_a_tld =
        (2..=24).contains(&tld.len()) && tld.chars().all(|c| c.is_ascii_alphabetic());
    if !looks_like_a_tld || NOT_SITES.contains(&tld) {
        return None;
    }

    let guessed = format!("https://{host}{path}");
    // Parsed rather than trusted: this string is about to be handed to a
    // browser with a session in it.
    url::Url::parse(&guessed).ok().map(|_| guessed)
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
    // A front page is not an empty page. It came back with forty stories and
    // little prose, which is what a front page *is* — and calling that "nothing
    // readable" is what sent a run to the browsing window to read the same
    // fifty characters again, and then to a search engine for a headline it had
    // already been handed.
    if let crate::syn::web::Shape::Index { stories, .. } = page.shape {
        if stories >= crate::syn::web::ENOUGH_TO_BE_A_LIST {
            return true;
        }
    }
    page.text.chars().count() >= ENOUGH_TEXT && has_a_sentence(&page.text)
}

/// How many words make a line a sentence rather than a label.
///
/// Eight. Measured on three real pages, and the gap is not close:
///
/// | page | characters | longest line |
/// |------|-----------:|-------------:|
/// | `liveboard.cafef.vn`, a share-price board | 507 | **4 words** |
/// | GenK's front page | 50 | 12 words |
/// | *This Week in Rust* #667 | 13,709 | 63 words |
///
/// A page built by JavaScript arrives as its own furniture: column headings,
/// menu items, nothing longer than a label. `Mã · Trần · Sàn · T.C · Bên mua ·
/// Khớp lệnh` and eighty more lines like it, and not one row of prices — those
/// are put there by a script this rung never runs.
pub const A_SENTENCE: usize = 8;

/// Whether anything on this page is written rather than labelled.
///
/// # The read this exists to reject
///
/// Asked the price of a share, Syn fetched Vietnam's live price board and got
/// five hundred characters of empty table: every column heading, no rows.
/// Five hundred is comfortably past `ENOUGH_TEXT`, so the cheap rung called it
/// a good read and **the browsing window was never opened** — the one rung that
/// runs the page's scripts and would have had the number. Syn told the person
/// it could not get the price and handed them a link. They clicked it, and the
/// pane showed them the table, filled in, one rung away.
///
/// A character count cannot tell a page from its own scaffolding. A line of
/// eight words can: scaffolding does not have sentences in it.
fn has_a_sentence(text: &str) -> bool {
    text.lines().any(|line| line.split_whitespace().count() >= A_SENTENCE)
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

/// Where the app itself lives, learnt from the app itself.
///
/// Not a constant, because it is `tauri://localhost` in a bundle and
/// `http://localhost:1420` in development, and a hard-coded pair of those is
/// two more things to keep true.
static HOME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Remember where the app started, so it can be told apart from everywhere else.
pub fn note_home(url: &str) {
    if let Ok(parsed) = url::Url::parse(url) {
        *HOME.lock().unwrap_or_else(|e| e.into_inner()) = Some(parsed.origin().ascii_serialization());
    }
}

/// Whether a page belongs to the app rather than to the internet.
///
/// `None` for home means nothing has started yet, and nothing is judged: better
/// to let a page through than to bounce the app off its own first load.
pub fn is_the_app(url: &str) -> bool {
    let Some(home) = HOME.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return true;
    };
    url::Url::parse(url)
        .map(|u| u.origin().ascii_serialization() == home)
        .unwrap_or(true)
}

/// Put the app back if something took it somewhere else.
///
/// # Why this exists behind a listener that already prevents it
///
/// `App.vue` intercepts clicks on external links, which is the fix and covers
/// what actually happened. This covers what it cannot: a `location.href` from
/// anywhere in the app, a form that posts away, a component that handles a
/// click before the document sees it. The failure it prevents is total — the
/// app's webview showing a news site, with no chrome, no back button and no
/// route home short of quitting — so it is worth a second answer.
///
/// A recovery rather than a refusal, because the app's window is built from
/// `tauri.conf.json` and `on_navigation` belongs to a builder. Reloading the app
/// loses what was on screen, which is a bad outcome and a much better one than
/// a window that cannot be got back.
///
/// The pane is not this webview and is not touched: it is exactly where a page
/// from the internet is supposed to be.
pub fn stay_home<R: tauri::Runtime>(webview: &tauri::Webview<R>, url: &str) {
    if webview.label() != MAIN_WINDOW || url == "about:blank" || is_the_app(url) {
        return;
    }

    log::error!("[Syn] The app was navigated to {url}; putting it back");

    let home = HOME.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(home) = home.and_then(|h| url::Url::parse(&h).ok()) {
        let _ = webview.navigate(home);
    }
}

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
    /// The links offered by the last page read, so that opening one is a move.
    ///
    /// # Why a number is worth having at all
    ///
    /// Not to save characters in the call — the addresses are already in the
    /// model's context, so `browse("3")` and `browse("https://…")` cost the same
    /// round trip. It is worth having because it is what **clicking** is: the
    /// page offered a list, and the answer to a list is an index into it.
    ///
    /// A model that has to copy a ninety-character Vietnamese slug back out of
    /// its own context is a model that will eventually copy it slightly wrong,
    /// and a slightly wrong address is a 404 that looks like a dead site.
    ///
    /// One page at a time, like everything else here: one window, one page, one
    /// person watching. `offered_by` is which page they came from, so a stale
    /// number cannot be answered with somebody else's link.
    pub offered: Vec<crate::syn::web::Link>,
    pub offered_by: String,
    /// The page last read, whole, so that reading on is not fetching again.
    ///
    /// A long page arrives in slices — see `web::page_chars` — and the slices
    /// come out of this rather than off the network. The page was fetched once;
    /// reading further into it is looking again at what is already in hand,
    /// which is what a person does with a long article and what Syn had no way
    /// of doing at all.
    pub reading: Option<crate::syn::web::Page>,
    /// How far into it has been sent so far.
    ///
    /// Separate from the page, because the page kept here is the **whole** one
    /// and the model has seen a window onto it. Slicing the slice was the first
    /// version of this and it read the same paragraphs twice.
    pub read_to: usize,
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

/// Remember what a page offered, so a number can mean one of them.
pub fn note_offered(waiting: &Waiting, from: &str, links: &[crate::syn::web::Link]) {
    let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    pending.offered = links.to_vec();
    pending.offered_by = from.to_string();
}

/// The link a number means, if it means one.
///
/// `None` for anything that is not a number, and for a number outside the list
/// — a model that says "7" when six were offered has miscounted, and opening
/// the sixth instead would be answering a question nobody asked.
pub fn offered_link(waiting: &Waiting, what: &str) -> Option<crate::syn::web::Link> {
    let n: usize = what.trim().parse().ok()?;
    if n == 0 {
        return None;
    }
    let pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    pending.offered.get(n - 1).cloned()
}

/// Keep the page just read, whole, and say how much of it went out.
///
/// `whole` is the page before any cut; `sent` is the slice the model was given.
/// Both are needed and neither can be derived from the other: the first is what
/// reading on reads, the second is where reading on starts.
pub fn note_reading(waiting: &Waiting, whole: &crate::syn::web::Page, sent: &crate::syn::web::Page) {
    let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    pending.read_to = sent.from + sent.text.chars().count();
    pending.reading = Some(whole.clone());
}

/// Move the mark, the page in hand being unchanged.
///
/// Reading on does not re-read the page, so there is nothing new to keep — only
/// a new answer to *how far have we got*.
pub fn note_read_to(waiting: &Waiting, sent: &crate::syn::web::Page) {
    let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    pending.read_to = sent.from + sent.text.chars().count();
}

/// Forget the page in hand.
///
/// Called when a search happens: "more" has to mean the last page somebody
/// asked for by address, and a search reads two pages that were chosen for the
/// model rather than by it. Continuing one of those on the word "more" would be
/// a guess, and a silent one.
pub fn nothing_in_hand(waiting: &Waiting) {
    let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    pending.reading = None;
    pending.read_to = 0;
}

/// What `browse` was asked to do with the page already in hand, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Onwards {
    /// Read on from where the last slice stopped.
    More,
    /// Go to the part with this heading, at this offset.
    Part(usize),
}

/// The word a model uses to turn a page.
pub const READ_ON: &str = "more";

/// Whether this is an instruction about the page in hand rather than a new one.
///
/// # Why a heading is matched by its words and not by a number
///
/// Because the words are what the model has just been shown, and a number
/// would be one more thing to keep in step between what was printed and what
/// this answers to. Matched loosely — case-folded, and a prefix counts —
/// because a model quoting a heading back will often quote the first half of
/// it, and refusing that would be pedantry dressed as safety. Nothing
/// dangerous is on the other side of this: it is an offset into a string that
/// has already been fetched.
pub fn onwards(waiting: &Waiting, what: &str) -> Option<Onwards> {
    let asked = what.trim().to_lowercase();
    if asked.is_empty() {
        return None;
    }

    let pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    let reading = pending.reading.as_ref()?;

    if asked == READ_ON {
        return Some(Onwards::More);
    }

    reading
        .outline
        .iter()
        .find(|h| {
            let heading = h.text.to_lowercase();
            heading == asked || heading.starts_with(&asked) || asked.starts_with(&heading)
        })
        .map(|h| Onwards::Part(h.at))
}

/// The page in hand, sliced as asked.
pub fn read_on(waiting: &Waiting, how: &Onwards, cap: usize) -> Option<crate::syn::web::Page> {
    let pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    let reading = pending.reading.as_ref()?.clone();

    let from = match how {
        // Where the last slice stopped, which is the only thing "more" can
        // sensibly mean and the only thing a reader means by it.
        Onwards::More => pending.read_to,
        Onwards::Part(at) => *at,
    };

    // Past the end is not a slice, it is the end. Saying so beats handing back
    // an empty page that looks like a page with nothing on it.
    if from >= reading.whole {
        return None;
    }
    Some(reading.slice(from, cap))
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

    let nonce = arm(waiting);

    // Whose it is, before it exists. A pane already on screen is the person's
    // and Syn is only borrowing it; one Syn opens for itself goes away with the
    // run. `pane::open` does not change this, so it has to be said here — and
    // only when there is nothing to claim.
    if app.get_webview(crate::syn::pane::PANE).is_none() {
        crate::syn::pane::opened_by_the_person(false);
    }

    crate::syn::pane::open(app, url, &nonce)?;

    {
        let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
        pending.opened_at = Some(std::time::Instant::now());
    }

    // Wait for *this page*, not for a number. `SETTLE_MS` is the ceiling for a
    // page that never says it loaded, not the wait itself.
    let waited_from = std::time::Instant::now();
    while waited_from.elapsed() < std::time::Duration::from_millis(SETTLE_MS) {
        if waiting.lock().unwrap_or_else(|e| e.into_inner()).loaded {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // And then let its JavaScript run, which is the part that mattered.
    tokio::time::sleep(std::time::Duration::from_millis(AFTER_LOAD_MS)).await;

    harvest(app, waiting, &nonce, url).await
}

/// Arm the window to answer, and say with which nonce.
///
/// A nonce per read, checked on arrival. Without it an advert in an iframe
/// could answer first and hand Syn a page it never asked for.
fn arm(waiting: &Waiting) -> String {
    let nonce = uuid::Uuid::new_v4().to_string();
    let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
    pending.nonce = nonce.clone();
    pending.reply = None;
    pending.loaded = false;
    nonce
}

/// Read the page the pane is **already showing**, without navigating.
///
/// # Why not just navigate to the same address
///
/// Because the page on screen is not the page at that address. It has been
/// scrolled, it has run its scripts, it may be behind a login the person did
/// themselves — that is the whole reason a browser is here rather than a fetch.
/// Sending it to its own URL again throws all of that away and asks the site
/// for it a second time.
///
/// And it is what a browser being *live state* means. The page survived the
/// turn; reading it should not cost a round trip to the internet.
#[cfg(desktop)]
pub async fn read_showing<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    waiting: &Waiting,
) -> AppResult<Read> {
    let showing = crate::syn::pane::showing(app)
        .ok_or_else(|| AppError::General("There is no page open to read".into()))?;

    // No navigation, so nothing to wait for: it loaded before this run started.
    // `opened_at` is deliberately left alone — this run opened nothing, and
    // `close_when_done` must not tidy away a pane it did not put there.
    let nonce = arm(waiting);
    harvest(app, waiting, &nonce, &showing.url).await
}

/// Ask the pane for its document until it answers, or until patience runs out.
///
/// Split from the navigating path so reading what is already open is the same
/// code rather than a second copy of it that drifts.
#[cfg(desktop)]
async fn harvest<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    waiting: &Waiting,
    nonce: &str,
    url: &str,
) -> AppResult<Read> {
    use tauri::Manager;

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(PATIENCE_MS);
    while std::time::Instant::now() < deadline {
        // Gone means somebody shut it, which is the stop button. Giving up here
        // rather than polling an absent label for the rest of the twenty
        // seconds is the difference between a control and a delay.
        let Some(pane) = app.get_webview(crate::syn::pane::PANE) else {
            return Err(AppError::General(CLOSED_ON_IT.to_string()));
        };
        // Re-injected each time: on a pane that already existed the
        // initialization script belongs to the *previous* navigation, and
        // carries the previous nonce.
        let _ = pane.eval(format!(
            "{}\nwindow.__synRead && window.__synRead();",
            reader_script(nonce)
        ));

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let landed = {
            let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
            pending.reply.take()
        };
        if let Some(read) = landed {
            // Left open. `close_when_done` shuts it when the run ends — and
            // only if it was Syn's to shut.
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

/// Whether two addresses name the same page.
///
/// Used to decide whether the pane is already showing what `browse` was asked
/// for, and so whether to read the screen instead of the internet.
///
/// Fragments are dropped because `#section` is a place *within* a page, and a
/// trailing slash because `https://vnexpress.net` and `https://vnexpress.net/`
/// are the same front page written by two different hands — the model's and the
/// webview's, which is exactly the pair being compared here.
///
/// Query strings are **kept**: `?q=one` and `?q=two` are two different pages
/// however similar they look.
pub fn same_place(a: &str, b: &str) -> bool {
    fn tidy(raw: &str) -> Option<String> {
        let mut url = url::Url::parse(raw).ok()?;
        url.set_fragment(None);
        let text = url.to_string();
        Some(text.strip_suffix('/').unwrap_or(&text).to_string())
    }

    match (tidy(a), tidy(b)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}

/// The least time the pane is on screen before anything closes it.
///
/// Six seconds. Rule 1 does not say the browser exists, it says a person can
/// watch it — and something that appears and vanishes inside a few hundred
/// milliseconds is not something anybody watched. This is the floor that makes
/// the rule true rather than approximately true.
pub const AT_LEAST_MS: u64 = 6_000;

/// How much longer it has to stay up, having been open this long.
///
/// Separated from the closing so the arithmetic can be tested without a pane.
pub fn still_owed(open_for: std::time::Duration) -> std::time::Duration {
    std::time::Duration::from_millis(AT_LEAST_MS).saturating_sub(open_for)
}

/// Shut the browsing pane, the run being over.
///
/// Called at the end of a run rather than at the end of a page read: a run that
/// reads three pages should show one browser, not flicker one three times.
///
/// Two things stop it. **Whose it is** — a pane the person opened is not Syn's
/// to tidy away at the end of an answer, and `pane::syn_may_close_it` is that
/// question. And **`AT_LEAST_MS`** — a run that finished faster than a person
/// can look is exactly the case this exists for.
#[cfg(desktop)]
pub fn close_when_done<R: tauri::Runtime>(app: &tauri::AppHandle<R>, waiting: &Waiting) {
    use tauri::Manager;

    let opened_at = {
        let mut pending = waiting.lock().unwrap_or_else(|e| e.into_inner());
        pending.opened_at.take()
    };

    // No pane was opened by this run, so there is nothing of this run's to
    // close. Anything on screen belongs to somebody else.
    let Some(opened_at) = opened_at else { return };

    // And the person's browser is not Syn's to close. Syn borrowed a pane that
    // was already open; it hands it back rather than shutting it.
    if !crate::syn::pane::syn_may_close_it() {
        return;
    }

    let owed = still_owed(opened_at.elapsed());
    if owed.is_zero() {
        let _ = crate::syn::pane::close(app);
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(owed).await;

        // Claimed again while this was waiting means a new run is using the
        // pane, and shutting it now would close a page half way through being
        // read. `opened_at` is set by every visit, so its presence is exactly
        // the question "does this belong to somebody else now".
        let taken = app.try_state::<Waiting>().is_some_and(|waiting| {
            waiting.lock().unwrap_or_else(|e| e.into_inner()).opened_at.is_some()
        });
        if taken || !crate::syn::pane::syn_may_close_it() {
            return;
        }

        let _ = crate::syn::pane::close(&app);
    });
}

/// Nothing to close on a platform that never opened one.
#[cfg(mobile)]
pub fn close_when_done<R: tauri::Runtime>(_app: &tauri::AppHandle<R>, _waiting: &Waiting) {}

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
            // Prose, not a hundred and fifty of the same character: a page is
            // kept for having something written on it, and `nnnn…` is not
            // something anybody wrote.
            text: "Một câu về giá cổ phiếu và thị trường trong phiên hôm nay. ".repeat(4),
            truncated: false,
            shape: crate::syn::web::Shape::default(),
            published_at: String::new(),
            author: String::new(),
            outline: Vec::new(),
            whole: 0,
            from: 0,
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
            shape: crate::syn::web::Shape::default(),
            published_at: String::new(),
            author: String::new(),
            outline: Vec::new(),
            whole: 0,
            from: 0,
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

    /// It sits inside the app now, so nothing here computes a place for it.
    ///
    /// There was arithmetic for putting a separate window beside the app —
    /// flush to the screen edge rather than half off it, matching its height.
    /// All of it went when the browser moved in: `syn::pane` decides the
    /// layout, in the window's own coordinates, and there is no second window
    /// left to place.
    #[test]
    fn the_browser_is_laid_out_by_the_pane_and_not_by_this() {
        let module = include_str!("browser.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("there is a module");

        assert!(
            !module.contains("WebviewWindowBuilder"),
            "a second window would be a second thing to watch, and the whole \
             safety argument rests on there being one"
        );
        assert!(module.contains("crate::syn::pane::open"), "it browses in the pane");
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

    /// And nothing steals the keyboard.
    ///
    /// This used to also require `.focused(false)`, which was about a *second
    /// window* appearing in front of whatever somebody was typing into. There
    /// is no second window now; a pane inside the app has nothing to come in
    /// front of. What survives is the half that still means something: nothing
    /// here takes focus away.
    #[test]
    fn it_is_visible_without_taking_the_keyboard() {
        let source = include_str!("browser.rs");
        let body = source
            .split("async fn open_and_read")
            .nth(1)
            .expect("it is still here")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        assert!(!body.contains("set_focus()"), "reading a page must not take focus");
        assert!(
            !include_str!("pane.rs").contains("set_focus()"),
            "and neither must opening the pane"
        );
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

    // ── one door for following a link ─────────────────────────────

    /// A refusal must not become an opening.
    ///
    /// `syn_open_page` falls back to the person's own browser when there is no
    /// pane — a phone, or a window too narrow for both. It must not do that
    /// when the *address* was refused: handing `127.0.0.1` to the browser
    /// holding every cookie they own is worse than the thing `guard` was
    /// written to stop.
    ///
    /// Read off the source, because the alternative is a phone and a router.
    #[test]
    fn the_guard_runs_before_anything_can_fall_back_to_the_browser() {
        let source = include_str!("../commands/syn.rs");
        let body = source
            .split("pub async fn syn_open_page")
            .nth(1)
            .and_then(|rest| rest.split("\n/// Open the browsing pane").next())
            .expect("syn_open_page is there");

        let guarded = body.find("browser::guard(&url)?").expect("the address is guarded");
        let opener = body.find("open_url").expect("and there is a fallback to guard");
        assert!(
            guarded < opener,
            "the guard must run first, or a refused address reaches the person's browser"
        );
    }

    // ── turning the page ──────────────────────────────────────────

    fn a_page_in_hand() -> Waiting {
        let page = crate::syn::web::reduce(
            // Long enough for readability to accept it as content: a few
            // words is a candidate it discards.
            &format!(
                r#"<html><body><article><h1>Mở đầu</h1><p>{a}</p>
                   <h2>Phần giữa</h2><p>{b}</p>
                   <h2>Kết luận</h2><p>{c}</p></article></body></html>"#,
                a = "AAAA ".repeat(60),
                b = "BBBB ".repeat(60),
                c = "CCCC ".repeat(60),
            ),
            "https://genk.vn/a.chn",
        );
        let sent = page.clone().trimmed_to(20);
        let waiting = Waiting::default();
        note_reading(&waiting, &page, &sent);
        waiting
    }

    /// A long page used to have an unreachable second half: `browse` on the
    /// same address returned the same opening, so Syn summarised what it had
    /// and called it the article.
    #[test]
    fn more_reads_on_from_where_the_last_slice_stopped() {
        let waiting = a_page_in_hand();

        assert_eq!(onwards(&waiting, "more"), Some(Onwards::More));
        assert_eq!(onwards(&waiting, "  MORE "), Some(Onwards::More));

        let next = read_on(&waiting, &Onwards::More, 20).expect("there is more");
        assert_eq!(next.from, 20);
        assert!(!next.text.is_empty());
    }

    /// Jumping, which for a long page beats four slices: a research article is
    /// read by going to the part that matters, not by starting at the top.
    #[test]
    fn a_heading_is_somewhere_to_be_sent() {
        let waiting = a_page_in_hand();

        let Some(Onwards::Part(at)) = onwards(&waiting, "Kết luận") else {
            panic!("a heading names a place");
        };
        let part = read_on(&waiting, &Onwards::Part(at), 200).expect("it is in the page");
        assert!(part.text.contains("CCCC"), "{:?}", part.text);

        // Quoted back in a different case, or only half quoted, still lands.
        assert!(matches!(onwards(&waiting, "kết luận"), Some(Onwards::Part(_))));
        assert!(matches!(onwards(&waiting, "Phần"), Some(Onwards::Part(_))));
    }

    /// And everything else is a new question, not an instruction about this
    /// page. A heading nobody wrote must not be answered with the nearest one.
    #[test]
    fn anything_that_is_not_a_part_of_this_page_is_left_alone() {
        let waiting = a_page_in_hand();

        for what in ["https://genk.vn/other", "kết quả mu everton", "1", ""] {
            assert!(onwards(&waiting, what).is_none(), "took {what:?} as a page turn");
        }
    }

    /// With nothing read, `more` means nothing — and a search puts it back to
    /// nothing, because `more` has to mean the last page asked for by address.
    #[test]
    fn there_is_no_more_of_a_page_nobody_is_reading() {
        let waiting = Waiting::default();
        assert!(onwards(&waiting, "more").is_none());

        let held = a_page_in_hand();
        assert!(onwards(&held, "more").is_some());
        nothing_in_hand(&held);
        assert!(onwards(&held, "more").is_none());
    }

    /// Past the end is the end, not a page with nothing on it.
    #[test]
    fn the_end_of_a_page_says_so_rather_than_coming_back_empty() {
        let waiting = a_page_in_hand();
        let whole = {
            let p = waiting.lock().unwrap();
            p.reading.as_ref().unwrap().whole
        };

        assert!(read_on(&waiting, &Onwards::Part(whole), 100).is_none());
        assert!(read_on(&waiting, &Onwards::Part(whole + 500), 100).is_none());
    }

    // ── the app must not be navigable away from ───────────────────

    /// The window went to a news site and the app was gone: no sidebar, no
    /// conversation, no way back, because the way back is the app.
    ///
    /// One test rather than three, because `HOME` is one static and three tests
    /// would take turns rewriting it under each other.
    #[test]
    fn a_page_from_the_internet_is_not_the_app() {
        // Before the app has loaded there is nothing to compare against, and
        // bouncing the window off its own first page would be worse than
        // anything this prevents.
        *HOME.lock().unwrap() = None;
        assert!(is_the_app("https://genk.vn/"));

        note_home("http://localhost:1420/index.html");
        assert!(is_the_app("http://localhost:1420/"));
        assert!(is_the_app("http://localhost:1420/index.html#/messages"));
        assert!(!is_the_app("https://genk.vn/poco-f9-ultra.chn"));
        assert!(
            !is_the_app("https://localhost:1420/"),
            "a different scheme is a different origin"
        );

        // And the same again as the app is actually shipped.
        note_home("tauri://localhost");
        assert!(is_the_app("tauri://localhost/index.html"));
        assert!(!is_the_app("https://genk.vn/"));
    }

    // ── clicking ──────────────────────────────────────────────────

    fn offering(urls: &[&str]) -> Waiting {
        let waiting = Waiting::default();
        let links: Vec<crate::syn::web::Link> = urls
            .iter()
            .map(|u| crate::syn::web::Link {
                text: format!("story at {u}"),
                url: (*u).to_string(),
                region: crate::syn::web::Region::Content,
                heading: Some(3),
            })
            .collect();
        note_offered(&waiting, "https://genk.vn/", &links);
        waiting
    }

    /// The page handed over a numbered list; the answer to a numbered list is a
    /// number. Asking the model to copy a ninety-character Vietnamese slug back
    /// out of its own context instead is asking for one that is eventually a
    /// character wrong.
    #[test]
    fn a_number_means_the_link_with_that_number() {
        let waiting = offering(&["https://genk.vn/a", "https://genk.vn/b"]);

        assert_eq!(offered_link(&waiting, "1").unwrap().url, "https://genk.vn/a");
        assert_eq!(offered_link(&waiting, " 2 ").unwrap().url, "https://genk.vn/b");
    }

    /// A model that says "7" when two were offered has miscounted, and opening
    /// the second instead would answer a question nobody asked.
    #[test]
    fn a_number_nobody_offered_means_nothing() {
        let waiting = offering(&["https://genk.vn/a", "https://genk.vn/b"]);

        for what in ["0", "3", "-1", "1.5", "2026", "one"] {
            assert!(offered_link(&waiting, what).is_none(), "took {what} as a link");
        }
    }

    /// And a question that happens to be about a year is still a question.
    #[test]
    fn nothing_is_offered_before_a_page_has_been_read() {
        let waiting = Waiting::default();
        assert!(offered_link(&waiting, "1").is_none());
    }

    // ── a place, or a question ────────────────────────────────────

    /// The failure this fixes, in one line.
    ///
    /// The person said *"go to vnexpress"*, the model passed the domain, and
    /// Syn took a domain name to a search engine to ask where that site was.
    #[test]
    fn a_domain_somebody_named_is_a_place_to_go() {
        assert_eq!(
            address_of("vnexpress.net").as_deref(),
            Some("https://vnexpress.net")
        );
        assert_eq!(
            address_of("www.bongdanet.co").as_deref(),
            Some("https://www.bongdanet.co")
        );
        // The path survives, or every named address lands on a front page.
        assert_eq!(
            address_of("vnexpress.net/kinh-doanh").as_deref(),
            Some("https://vnexpress.net/kinh-doanh")
        );
        // Already an address, and unchanged — not re-guessed.
        assert_eq!(
            address_of("http://example.org/a").as_deref(),
            Some("http://example.org/a")
        );
    }

    /// And the other half, which is the half that keeps a visible browser from
    /// being sent somewhere nobody asked for.
    #[test]
    fn a_question_is_still_a_question() {
        for text in [
            "kết quả mu everton",
            "what is apple.com's revenue",
            "tin tức hôm nay",
            "syn.rs",
            "notes.md",
            "config.toml",
            "anh@example.com",
            "1.5",
            "",
            "   ",
        ] {
            assert!(
                address_of(text).is_none(),
                "took a question to a browser: {text:?}"
            );
        }
    }

    /// `.md` is Moldova and this vault is written in Markdown.
    ///
    /// A rule that reads a bare host as an address would send a visible
    /// browser, carrying whatever the person has logged into, to a country-code
    /// domain because somebody named a file.
    #[test]
    fn a_filename_is_not_a_country() {
        for name in ["plan.md", "engine.rs", "app.vue", "main.py", "build.sh"] {
            assert!(address_of(name).is_none(), "{name} is a file");
        }
    }

    // ── is it already on the screen ───────────────────────────────

    #[test]
    fn the_same_page_written_two_ways_is_the_same_page() {
        assert!(same_place("https://vnexpress.net", "https://vnexpress.net/"));
        assert!(same_place(
            "https://vnexpress.net/a#top",
            "https://vnexpress.net/a"
        ));
        assert!(same_place("https://VnExpress.net", "https://vnexpress.net"));
    }

    #[test]
    fn a_different_query_is_a_different_page() {
        assert!(!same_place(
            "https://duckduckgo.com/?q=one",
            "https://duckduckgo.com/?q=two"
        ));
        assert!(!same_place("https://a.com/one", "https://a.com/two"));
        assert!(!same_place("not a url", "https://a.com"));
    }

    /// Reading the screen must not navigate, or the page stops being the one
    /// the person had — scrolled, past a consent wall, logged in.
    #[test]
    fn reading_what_is_open_goes_nowhere() {
        let source = include_str!("browser.rs");
        let body = source
            .split("pub async fn read_showing")
            .nth(1)
            .and_then(|rest| rest.split("async fn harvest").next())
            .expect("read_showing is there");

        assert!(
            !body.contains("pane::open") && !body.contains("navigate"),
            "reading the open page must not send it anywhere: {body}"
        );
    }

    /// A page whose scripts have not run is its own furniture.
    ///
    /// Vietnam's live price board, fetched the cheap way: every column heading
    /// and not one row of prices. Five hundred characters — comfortably past
    /// `ENOUGH_TEXT` — so this used to call it a good read, and the one rung
    /// that runs the page's scripts was never reached. Syn told the person it
    /// could not get the price and handed them a link; they clicked it and the
    /// pane showed them the table, filled in.
    #[test]
    fn an_empty_table_is_not_a_page_that_was_read() {
        let shell = crate::syn::web::Page {
            url: "https://liveboard.cafef.vn/".into(),
            title: "Bảng giá chứng khoán trực tuyến".into(),
            // The real thing, as it arrived: headings, one per line, nothing else.
            text: "Mã\nTrần\nSàn\nT.C\nBên mua\nKhớp lệnh\nBên bán\nCao\nThấp\nĐTNN\n\
                   Giá 3\nKL 3\nGiá 2\nKL 2\nGiá 1\nKL 1\n+/-\n%\nGiá\nKL\nTổng KL\n\
                   Tùy chỉnh hiển thị\nĐồ thị kỹ thuật\nHồ sơ\nThanh khoản\nLàm lại Lưu"
                .into(),
            truncated: false,
            shape: crate::syn::web::Shape::Article { words: 60 },
            published_at: String::new(),
            author: String::new(),
            outline: Vec::new(),
            whole: 507,
            from: 0,
        };

        assert!(
            shell.text.chars().count() >= ENOUGH_TEXT,
            "it is long enough to have fooled a character count"
        );
        assert!(!worth_keeping(&shell), "and it is still nothing but column headings");
    }

    /// And a front page still passes, on its stories rather than its prose —
    /// fifty characters and fifty-two things to read is a successful read.
    #[test]
    fn a_front_page_still_passes_on_its_stories() {
        let front = crate::syn::web::Page {
            url: "https://genk.vn/".into(),
            title: "GenK".into(),
            text: "POCO F9 Ultra và phép thử lớn nhất trong 8 năm qua".into(),
            truncated: false,
            shape: crate::syn::web::Shape::Index { stories: 52, others: 31 },
            published_at: String::new(),
            author: String::new(),
            outline: Vec::new(),
            whole: 50,
            from: 0,
        };
        assert!(worth_keeping(&front));
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
        // The builder moved to `pane.rs` when the browser moved into the
        // window. The rules did not move — they are what this file is about —
        // so this reads the place they are now applied.
        let built = include_str!("pane.rs")
            .split("WebviewBuilder::new(PANE")
            .nth(1)
            .expect("the browser is still built there");
        let built = built.split("main\n").next().unwrap_or(built);

        for hook in [".on_navigation(", ".on_download(", ".on_new_window("] {
            assert!(built.contains(hook), "the browser is built without {hook}");
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

        assert!(
            include_str!("pane.rs").contains(".on_page_load("),
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
