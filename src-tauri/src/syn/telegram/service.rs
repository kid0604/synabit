//! The bot itself: polling, its commands, and handing messages to Syn.
//!
//! # Shape
//!
//! One supervisor task for the life of the app. It reads the token; with none
//! it waits to be told something changed, and with one it polls until it is
//! told to stop, or until Telegram says it cannot go on — the token was
//! refused, or something else is polling with it. Those two need a person, so
//! it waits for one rather than retrying into the same wall.
//!
//! A second task, the drain, turns what the inbox holds into turns of a
//! conversation. It is apart from the poller so that `/stop` is read while Syn
//! is still working on the thing it stops.
//!
//! # Order
//!
//! An update is written to the inbox before the offset moves past it, and an
//! inbox entry is removed only after Syn has answered it. A crash between any
//! two steps costs a duplicate at worst, never a message.
//!
//! # The token is the switch
//!
//! There is no separate "enable" setting. The computer whose keychain holds the
//! token runs the bot; remove the token and it stops. See §8 of
//! `docs/syn-over-telegram-2026-09-13.md`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use crate::db::DbBridge;
use crate::error::AppResult;
use crate::models::syn::{SynChatRequest, SynMessage};
use crate::syn::surface::Surface;

use super::api::{Api, ApiError, Update};
use super::gate::{self, Command, Gate, Pressed};
use super::inbox::{self, Attachment, Entry, Kind};
use super::pairing;
use super::remind;
use super::render::{self, Placeholders};
use super::staging;
use super::words::{Kept, Words};

/// The keychain slot the token is filed under, beside the model providers'.
pub const TOKEN_SLOT: &str = "telegram_bot";

/// Sent to the front end whenever something the settings screen shows changed.
pub const STATUS_EVENT: &str = "telegram-status";

/// The bot's name, recorded when its token was accepted.
///
/// Also the cheap answer to "was this ever set up": with no name recorded the
/// keychain is not asked at all, so a person who never used the bot is never
/// shown a keychain prompt on its account.
pub const USERNAME_KEY: &str = "telegram:bot_username";

const CONVERSATION_PREFIX: &str = "telegram:chat:";

/// A card sent with buttons, by the nonce its buttons carry.
const CARD_PREFIX: &str = "telegram:card:";

/// How many notes a which-one card offers. A phone screen of buttons is
/// already a lot; past this the question is better asked in the app.
const MAX_CHOICES: usize = 8;

/// How long a reply may be and still be read as "saved" rather than as an
/// answer, when Syn kept something. The app's own line says that already.
const QUIET_CONFIRMATION_CHARS: usize = 120;

const NO_VAULT_RETRY: Duration = Duration::from_secs(30);
const NOT_READY_RETRY: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// How long to let the rest of an album arrive.
///
/// Five photos sent together are five updates, close together but not always
/// in one poll. Without a pause the first becomes a turn and the other four a
/// second one.
const ALBUM_WAIT: Duration = Duration::from_millis(1500);

/// How long a turn is waited for before it is left to finish on its own.
///
/// Measured rather than chosen: of 201 runs on the vault this was built
/// against, half answered within 6 seconds, nine in ten within 16, and nineteen
/// in twenty within 22. The slowest took 103. Twenty seconds leaves the
/// ordinary question alone — it is answered as it always was — and stops the
/// rare slow one from holding up what is sent after it.
const LONG_AFTER: Duration = Duration::from_secs(20);

/// How many turns may be left to finish on their own at once.
///
/// Each is a run against the model and the vault. Past this, the next slow one
/// is waited for as before, which is slower and never wrong.
const MOST_IN_BACKGROUND: usize = 2;

/// Where a turn is, between the drain waiting on it and its answer going out.
/// See [`to_background`] and [`finishing`].
const FOREGROUND: u8 = 0;
const BACKGROUND: u8 = 1;
const FINISHING: u8 = 2;

/// Why the bot is not doing its job, when it is not.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    /// The same token is polling somewhere else.
    Conflict,
    /// Telegram does not accept the token.
    Unauthorized,
    /// Telegram cannot be reached. Retried on its own.
    Network,
    /// A token was set up here, and the keychain did not hand it over — most
    /// often a macOS permission dialog nobody has answered yet. Retried on its
    /// own, so answering the dialog is all it takes.
    Keychain,
}

#[derive(Default)]
struct Live {
    running: bool,
    problem: Option<(Problem, String)>,
    api: Option<Api>,
}

/// What the bot's tasks share. Managed by the app.
pub struct TelegramState {
    /// Bumped to make the supervisor start again from the stored token.
    generation: watch::Sender<u64>,
    live: Mutex<Live>,
    draining: AtomicBool,
    retry_scheduled: AtomicBool,
    /// Whether "not ready" has been said since Syn last answered, so a model
    /// that is off for an hour is one message and not sixty.
    told_not_ready: AtomicBool,
    /// How many times `/stop` was sent. A turn that finishes under a different
    /// count from the one it started under was stopped, however many turns
    /// were running — which a single flag, taken by the first to finish, was
    /// not able to say.
    stops: AtomicU64,
    /// The messages a turn is answering right now, by update id, so the drain
    /// does not start them a second time while they are still in the inbox.
    in_flight: Mutex<HashSet<i64>>,
    /// How many turns are finishing on their own.
    background: AtomicUsize,
    /// Nothing new is started before this, after Syn could not answer.
    paused_until: Mutex<Option<Instant>>,
}

impl Default for TelegramState {
    fn default() -> Self {
        Self {
            generation: watch::channel(0).0,
            live: Mutex::default(),
            draining: AtomicBool::new(false),
            retry_scheduled: AtomicBool::new(false),
            told_not_ready: AtomicBool::new(false),
            stops: AtomicU64::new(0),
            in_flight: Mutex::default(),
            background: AtomicUsize::new(0),
            paused_until: Mutex::new(None),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PairedView {
    pub name: String,
    pub since: String,
}

/// What the settings screen shows.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    /// False on a phone: the bot runs on a computer.
    pub supported: bool,
    pub has_token: bool,
    pub bot_username: Option<String>,
    pub running: bool,
    pub problem: Option<Problem>,
    pub detail: Option<String>,
    pub paired: Option<PairedView>,
    pub pending: usize,
    /// Whether reminders are sent to the paired chat. See `remind`.
    pub reminders: bool,
}

enum Ended {
    /// Told to start again.
    Restart,
    /// Nothing to do until something changes.
    Idle,
    /// Could not start, for a reason that may pass: try again shortly.
    Soon,
}

/// How long to wait before asking the keychain again.
const KEYCHAIN_RETRY: Duration = Duration::from_secs(15);

enum Token {
    Ready(String),
    /// Never set up on this computer.
    None,
    /// Set up, and the keychain did not answer.
    Unreadable,
}

/// Start the supervisor. Desktop only; see `lib.rs`.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move { supervise(app).await });
}

/// Stop whatever is polling and start again from the token stored now.
pub fn restart(app: &AppHandle) {
    app.state::<TelegramState>()
        .generation
        .send_modify(|g| *g = g.wrapping_add(1));
}

/// Forget everything this computer knew about the bot. The token is the
/// caller's to remove.
pub fn forget(app: &AppHandle) {
    with_db(app, |db| {
        for key in [USERNAME_KEY, inbox::OFFSET_KEY] {
            crate::error::logged("forget the Telegram bot", key, db.delete_kv(key));
        }
        crate::error::logged("forget the Telegram pairing", "pairing", pairing::unpair(db));
        crate::error::logged("forget the Telegram inbox", "inbox", inbox::clear(db).map(|_| ()));
        for prefix in [CONVERSATION_PREFIX, CARD_PREFIX, remind::OUTBOX_PREFIX, remind::DONE_PREFIX] {
            if let Ok(rows) = db.get_kv_prefix(prefix) {
                for (key, _) in rows {
                    crate::error::logged("forget the Telegram bot", &key, db.delete_kv(&key));
                }
            }
        }
    });
    if let Ok(dir) = staging::dir(app) {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            log::warn!("[Telegram] Could not remove fetched files: {e}");
        }
    }
    emit_status(app);
}

/// What the settings screen shows, now.
pub async fn status(app: &AppHandle) -> Status {
    let username = with_db(app, |db| db.get_kv(USERNAME_KEY).ok().flatten());
    // From the name recorded beside the token, not from the keychain. Asking
    // the keychain here put a macOS dialog in front of whoever opened the
    // settings screen, and while it waited the screen said there was no token
    // and offered to take one. Whether the keychain answers is `Problem::Keychain`.
    let has_token = username.is_some();
    let (running, problem) = {
        let state = app.state::<TelegramState>();
        let live = state.live.lock().unwrap_or_else(|e| e.into_inner());
        (live.running, live.problem.clone())
    };
    let paired = with_db(app, pairing::paired);
    let pending = with_db(app, |db| inbox::waiting(db).map(|w| w.len()).unwrap_or(0));
    Status {
        supported: cfg!(desktop),
        has_token,
        bot_username: username,
        running,
        problem: problem.as_ref().map(|(p, _)| *p),
        detail: problem.map(|(_, detail)| detail).filter(|d| !d.is_empty()),
        paired: paired.map(|p| PairedView { name: p.name, since: p.paired_at }),
        pending,
        reminders: with_db(app, remind::is_on),
    }
}

pub(crate) fn with_db<T>(app: &AppHandle, f: impl FnOnce(&DbBridge) -> T) -> T {
    let state = app.state::<crate::db::DbState>();
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    f(&db)
}

fn emit_status(app: &AppHandle) {
    if let Err(e) = app.emit(STATUS_EVENT, ()) {
        log::warn!("[Telegram] Could not tell the window the status changed: {e}");
    }
}

fn update_live(app: &AppHandle, change: impl FnOnce(&mut Live)) {
    {
        let state = app.state::<TelegramState>();
        let mut live = state.live.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut live);
    }
    emit_status(app);
}

fn set_problem(app: &AppHandle, problem: Problem, detail: &str) {
    log::warn!("[Telegram] {problem:?}{}", if detail.is_empty() { String::new() } else { format!(": {detail}") });
    update_live(app, |live| {
        if problem != Problem::Network {
            live.running = false;
        }
        live.problem = Some((problem, detail.to_string()));
    });
}

fn clear_problem(app: &AppHandle) {
    let had = {
        let state = app.state::<TelegramState>();
        let mut live = state.live.lock().unwrap_or_else(|e| e.into_inner());
        live.problem.take().is_some()
    };
    if had {
        emit_status(app);
    }
}

pub(super) fn current_api(app: &AppHandle) -> Option<Api> {
    let state = app.state::<TelegramState>();
    let live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    live.api.clone()
}

pub(super) fn active_vault(app: &AppHandle) -> Option<String> {
    let state = app.state::<crate::chat_engine::ChatEngineState>();
    let path = state.active_vault_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
    path
}

async fn token(app: &AppHandle) -> Token {
    if with_db(app, |db| db.get_kv(USERNAME_KEY).ok().flatten()).is_none() {
        return Token::None;
    }
    match crate::commands::syn::api_key_for(app, TOKEN_SLOT).await {
        Some(token) => Token::Ready(token),
        None => Token::Unreadable,
    }
}

// ─────────────────────────────────────────────────────────────
//  Polling
// ─────────────────────────────────────────────────────────────

async fn supervise(app: AppHandle) {
    let mut generation = app.state::<TelegramState>().generation.subscribe();
    loop {
        let _ = generation.borrow_and_update();
        let ended = match token(&app).await {
            Token::Ready(token) => poll(&app, &token, &mut generation).await,
            Token::None => {
                update_live(&app, |live| live.problem = None);
                Ended::Idle
            }
            // Not the same as no token. The first version read a keychain that
            // did not answer as "never set up" and waited for somebody to
            // change the settings — so a development rebuild, which earns a
            // fresh macOS dialog, left the bot silent until the app was
            // restarted, while messages sat unread on Telegram's side.
            Token::Unreadable => {
                set_problem(&app, Problem::Keychain, "");
                Ended::Soon
            }
        };
        update_live(&app, |live| {
            live.running = false;
            live.api = None;
        });
        match ended {
            Ended::Restart => {}
            Ended::Idle => {
                if generation.changed().await.is_err() {
                    return;
                }
            }
            Ended::Soon => {
                tokio::select! {
                    changed = generation.changed() => if changed.is_err() { return },
                    _ = tokio::time::sleep(KEYCHAIN_RETRY) => {}
                }
            }
        }
    }
}

async fn poll(app: &AppHandle, token: &str, generation: &mut watch::Receiver<u64>) -> Ended {
    let words = Words::here();
    let api = match Api::new(token) {
        Ok(api) => api,
        Err(e) => {
            set_problem(app, Problem::Network, &e.to_string());
            return Ended::Idle;
        }
    };

    let mut backoff = Duration::from_secs(1);
    loop {
        match api.get_me().await {
            Ok(_) => break,
            Err(ApiError::Unauthorized) => {
                set_problem(app, Problem::Unauthorized, "");
                return Ended::Idle;
            }
            Err(e) => {
                set_problem(app, Problem::Network, &e.to_string());
                if wait(generation, &mut backoff).await {
                    return Ended::Restart;
                }
            }
        }
    }

    if let Err(e) = api.set_commands(&words.commands()).await {
        log::warn!("[Telegram] Could not set the command menu: {e}");
    }
    update_live(app, |live| {
        live.running = true;
        live.problem = None;
        live.api = Some(api.clone());
    });
    log::info!("[Telegram] Polling");
    // Files nothing waits for any more — left by a crash, or by a message
    // dropped while the app was not running.
    if let Ok(dir) = staging::dir(app) {
        let waiting = with_db(app, inbox::waiting).unwrap_or_default();
        staging::sweep(&dir, &waiting);
    }
    // Whatever was waiting when the app last closed.
    drain(app.clone());
    // And any reminder that came due while nothing was polling.
    remind::flush(app.clone());

    let mut backoff = Duration::from_secs(1);
    loop {
        let offset = with_db(app, inbox::offset);
        let result = tokio::select! {
            _ = generation.changed() => return Ended::Restart,
            result = api.get_updates(offset) => result,
        };
        match result {
            Ok(updates) => {
                clear_problem(app);
                let mut kept_all = true;
                for update in &updates {
                    // Not kept means not passed: the offset stays where it is,
                    // and Telegram hands the same update back next time.
                    if !handle(app, &api, &words, update).await {
                        kept_all = false;
                        break;
                    }
                    if let Err(e) = with_db(app, |db| inbox::set_offset(db, update.update_id + 1)) {
                        log::error!("[Telegram] Could not record how far it has read: {e}");
                        kept_all = false;
                        break;
                    }
                }
                if kept_all {
                    backoff = Duration::from_secs(1);
                } else if wait(generation, &mut backoff).await {
                    return Ended::Restart;
                }
            }
            Err(ApiError::Conflict) => {
                set_problem(app, Problem::Conflict, "");
                return Ended::Idle;
            }
            Err(ApiError::Unauthorized) => {
                set_problem(app, Problem::Unauthorized, "");
                return Ended::Idle;
            }
            Err(e) => {
                set_problem(app, Problem::Network, &e.to_string());
                if wait(generation, &mut backoff).await {
                    return Ended::Restart;
                }
            }
        }
    }
}

/// Sleep for the backoff and double it, unless told to start again first.
/// `true` means start again.
async fn wait(generation: &mut watch::Receiver<u64>, backoff: &mut Duration) -> bool {
    let restart = tokio::select! {
        _ = generation.changed() => true,
        _ = tokio::time::sleep(*backoff) => false,
    };
    *backoff = (*backoff * 2).min(MAX_BACKOFF);
    restart
}

/// One update. `false` when it could not be kept, so it must not be passed.
async fn handle(app: &AppHandle, api: &Api, words: &Words, update: &Update) -> bool {
    let now = chrono::Utc::now();
    let paired = with_db(app, pairing::paired);
    match gate::classify(update, paired.as_ref(), &now.to_rfc3339()) {
        Gate::Ignore => true,
        Gate::Leave { chat_id } => {
            if let Err(e) = api.leave(chat_id).await {
                log::info!("[Telegram] Could not leave a group: {e}");
            }
            true
        }
        Gate::Pair { code, user_id, chat_id, name } => {
            match with_db(app, |db| pairing::redeem(db, &code, user_id, chat_id, &name, now)) {
                Ok(true) => {
                    log::info!("[Telegram] Paired with a Telegram account");
                    say(api, chat_id, &words.paired(&tauri_plugin_os::hostname())).await;
                    emit_status(app);
                    true
                }
                // A wrong or stale code is answered the way a stranger is: not at all.
                Ok(false) => true,
                Err(e) => {
                    log::error!("[Telegram] Could not record a pairing: {e}");
                    false
                }
            }
        }
        Gate::Command { command, chat_id } => {
            run_command(app, api, words, command, chat_id).await;
            true
        }
        Gate::Answer(pressed) if pressed.data.starts_with(remind::DONE_BUTTON) => {
            remind::answer_done(app, api, words, &pressed).await
        }
        Gate::Answer(pressed) => answer_card(app, api, words, &pressed).await,
        Gate::ToSyn(entry) => {
            if let Err(e) = with_db(app, |db| inbox::put(db, &entry)) {
                log::error!("[Telegram] Could not keep a message: {e}");
                return false;
            }
            if let Err(e) = api.react(entry.chat_id, entry.message_id, "👀").await {
                log::info!("[Telegram] Could not mark a message as seen: {e}");
            }
            emit_status(app);
            drain(app.clone());
            true
        }
    }
}

/// Send one of the bot's own lines.
async fn say(api: &Api, chat_id: i64, text: &str) {
    say_to(api, chat_id, text, None).await
}

/// The same, as a reply to one of the person's messages.
async fn say_to(api: &Api, chat_id: i64, text: &str, reply_to: Option<i64>) {
    if let Err(e) = api.send_reply(chat_id, &render::escape(text), reply_to).await {
        log::warn!("[Telegram] Could not send a message: {e}");
    }
}

async fn run_command(app: &AppHandle, api: &Api, words: &Words, command: Command, chat_id: i64) {
    match command {
        Command::Help => say(api, chat_id, words.help()).await,
        Command::New => {
            let Some(vault) = active_vault(app) else {
                return say(api, chat_id, &words.not_ready(words.no_vault())).await;
            };
            match new_conversation(app, &vault, chat_id) {
                Ok(_) => say(api, chat_id, words.new_conversation()).await,
                Err(e) => say(api, chat_id, &words.not_ready(&e.to_string())).await,
            }
        }
        Command::Stop => {
            let working = {
                let state = app.state::<TelegramState>();
                let working = state.draining.load(Ordering::SeqCst)
                    || !state.in_flight.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
                if working {
                    state.stops.fetch_add(1, Ordering::SeqCst);
                }
                working
            };
            if working {
                if let Some(id) = mapped_conversation(app, chat_id) {
                    crate::syn::engine::stop_conversation(Some(id.as_str()));
                }
            }
            let dropping = with_db(app, inbox::waiting).unwrap_or_default();
            let dropped = with_db(app, inbox::clear).unwrap_or(0);
            if let Ok(dir) = staging::dir(app) {
                staging::discard(&dir, &dropping);
            }
            if working || dropped > 0 {
                say(api, chat_id, &words.stopped(dropped)).await;
            } else {
                say(api, chat_id, words.nothing_to_stop()).await;
            }
            emit_status(app);
        }
        Command::Last => {
            let last = active_vault(app)
                .zip(mapped_conversation(app, chat_id))
                .and_then(|(vault, id)| crate::syn::conversation::get_conversation(&vault, &id).ok())
                .and_then(|conversation| {
                    conversation
                        .messages
                        .into_iter()
                        .rev()
                        .find(|m| m.role == "assistant" && !m.content.trim().is_empty())
                });
            match last {
                Some(message) => {
                    send_answer(api, words, chat_id, &message, None).await;
                }
                None => say(api, chat_id, words.nothing_yet()).await,
            }
        }
        Command::Status => {
            let vault = active_vault(app);
            let syn_on = vault
                .as_deref()
                .is_some_and(|v| crate::syn::settings::load_settings(v).unwrap_or_default().enabled);
            let pending = waiting_now(app).len();
            let background = app.state::<TelegramState>().background.load(Ordering::SeqCst);
            let text = words.status(&tauri_plugin_os::hostname(), vault.is_some(), syn_on, pending, background);
            say(api, chat_id, &text).await;
        }
        Command::Unpair => {
            say(api, chat_id, words.unpaired()).await;
            if let Err(e) = with_db(app, pairing::unpair) {
                log::error!("[Telegram] Could not unpair: {e}");
            }
            emit_status(app);
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Conversations
// ─────────────────────────────────────────────────────────────

fn conversation_key(chat_id: i64) -> String {
    format!("{CONVERSATION_PREFIX}{chat_id}:conversation")
}

fn mapped_conversation(app: &AppHandle, chat_id: i64) -> Option<String> {
    with_db(app, |db| db.get_kv(&conversation_key(chat_id)).ok().flatten())
}

/// The conversation this chat writes into, made if there is none in this
/// vault — the vault may have changed since, or the conversation been deleted.
fn conversation_for(app: &AppHandle, vault: &str, chat_id: i64) -> AppResult<String> {
    if let Some(id) = mapped_conversation(app, chat_id) {
        if crate::syn::conversation::get_conversation(vault, &id).is_ok() {
            return Ok(id);
        }
    }
    new_conversation(app, vault, chat_id)
}

fn new_conversation(app: &AppHandle, vault: &str, chat_id: i64) -> AppResult<String> {
    // Named for where it is and when it began, and kept that way — see the
    // title rule in `send_message_inner`. Every message from the phone lands
    // here, so a title taken from the first one ("chào") names nothing.
    let title = format!("Telegram · {}", chrono::Local::now().format("%Y-%m-%d"));
    let meta = crate::syn::conversation::create_conversation(vault, Some(title))?;
    with_db(app, |db| db.set_kv(&conversation_key(chat_id), &meta.id))?;
    Ok(meta.id)
}

// ─────────────────────────────────────────────────────────────
//  Draining the inbox into Syn
// ─────────────────────────────────────────────────────────────

/// Start answering what is waiting, unless that is already happening.
fn drain(app: AppHandle) {
    if app.state::<TelegramState>().draining.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let retry = drain_all(&app).await;
        app.state::<TelegramState>().draining.store(false, Ordering::SeqCst);
        match retry {
            Some(after) => schedule(app, after),
            // Something may have arrived between the last look and the flag
            // coming down, and nothing else would notice it.
            None if !waiting_now(&app).is_empty() => drain(app),
            None => {}
        }
    });
}

fn schedule(app: AppHandle, after: Duration) {
    if app.state::<TelegramState>().retry_scheduled.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(after).await;
        app.state::<TelegramState>().retry_scheduled.store(false, Ordering::SeqCst);
        drain(app);
    });
}

/// Answer until nothing waits. `Some` means try again after that long.
///
/// One turn is waited for at a time, and only for so long. A turn still going
/// after [`LONG_AFTER`] is left to finish on its own: the person is told under
/// their message, the answer arrives later as a reply to it, and the next
/// message is started. Before, a question that took a minute held up
/// everything sent after it, and the only ways out were waiting and `/stop`.
///
/// Turns running side by side stay honest about each other. The conversation
/// takes each one whole when it finishes (`conversation::place_turn`), and a
/// run that starts while another is going is told what that one was asked
/// (`engine::underway`), so it neither starts the same work again nor says it
/// knows nothing about it.
async fn drain_all(app: &AppHandle) -> Option<Duration> {
    let words = Words::here();
    let staged = match staging::dir(app) {
        Ok(dir) => Some(dir),
        Err(e) => {
            log::warn!("[Telegram] Nowhere to keep fetched files: {e}");
            None
        }
    };
    let mut waited_for_album = false;
    loop {
        if let Some(left) = paused(app) {
            return Some(left);
        }
        let waiting = waiting_now(app);
        if waiting.is_empty() {
            if in_flight(app).is_empty() {
                app.state::<TelegramState>().told_not_ready.store(false, Ordering::SeqCst);
            }
            return None;
        }
        let Some(api) = current_api(app) else {
            return Some(NOT_READY_RETRY);
        };

        // Only what the paired chat sent. Anything else was kept under a
        // pairing that has since been undone, and nobody is there to answer.
        let paired = with_db(app, pairing::paired);
        let (ours, stale): (Vec<Entry>, Vec<Entry>) = waiting
            .into_iter()
            .partition(|entry| paired.as_ref().is_some_and(|p| p.chat_id == entry.chat_id));
        if !stale.is_empty() {
            crate::error::logged("drop stale Telegram messages", "inbox", with_db(app, |db| inbox::remove(db, &stale)));
            if let Some(dir) = &staged {
                staging::discard(dir, &stale);
            }
        }
        let Some(chat_id) = ours.first().map(|e| e.chat_id) else {
            continue;
        };
        // A pressed "allow once" carries a stopped run on, as a turn of its
        // own. What arrived before it is answered first; what arrived after it
        // waits for the next turn.
        let ours: Vec<Entry> = match ours.iter().position(|e| e.resume_run.is_some()) {
            Some(0) => ours.into_iter().take(1).collect(),
            Some(before) => ours.into_iter().take(before).collect(),
            None => ours,
        };
        if ours.iter().any(|e| e.album.is_some()) && !waited_for_album {
            waited_for_album = true;
            tokio::time::sleep(ALBUM_WAIT).await;
            continue;
        }
        waited_for_album = false;

        let Some(vault) = active_vault(app) else {
            tell_not_ready(app, &api, chat_id, &words.not_ready(words.no_vault())).await;
            return Some(NO_VAULT_RETRY);
        };
        let conversation_id = match conversation_for(app, &vault, chat_id) {
            Ok(id) => id,
            Err(e) => {
                tell_not_ready(app, &api, chat_id, &words.not_ready(&e.to_string())).await;
                return Some(NOT_READY_RETRY);
            }
        };

        let asked_in = ours[0].message_id;
        let phase = Arc::new(AtomicU8::new(FOREGROUND));
        set_in_flight(app, &ours, true);
        let job = Job {
            chat_id,
            entries: ours,
            vault,
            conversation_id,
            phase: Arc::clone(&phase),
            stops_seen: app.state::<TelegramState>().stops.load(Ordering::SeqCst),
        };
        let (done, mut finished) = tokio::sync::oneshot::channel();
        tauri::async_runtime::spawn(answer(app.clone(), api.clone(), staged.clone(), job, done));

        if tokio::time::timeout(LONG_AFTER, &mut finished).await.is_ok() {
            continue;
        }
        if take_background_slot(app) {
            if to_background(&phase) {
                log::info!("[Telegram] Left a slow turn to finish on its own");
                say_to(&api, chat_id, words.still_working(), Some(asked_in)).await;
                continue;
            }
            // Finished in the instant between the two.
            release_background_slot(app);
        }
        // As many are finishing on their own as may; this one is waited for.
        let _ = finished.await;
    }
}

/// One turn being answered, apart from the drain that started it.
struct Job {
    chat_id: i64,
    entries: Vec<Entry>,
    vault: String,
    conversation_id: String,
    phase: Arc<AtomicU8>,
    /// `TelegramState::stops` when it started.
    stops_seen: u64,
}

/// Answer one turn, deliver what came of it, and let the drain know.
async fn answer(
    app: AppHandle,
    api: Api,
    staged: Option<std::path::PathBuf>,
    job: Job,
    done: tokio::sync::oneshot::Sender<()>,
) {
    let words = Words::here();
    let stopped = |app: &AppHandle| app.state::<TelegramState>().stops.load(Ordering::SeqCst) != job.stops_seen;

    let typing = keep_typing(api.clone(), job.chat_id, Arc::clone(&job.phase));
    let prepared = prepare(&api, staged.as_deref(), &job.entries).await;
    let ask = |show_images: bool| SynChatRequest {
        conversation_id: job.conversation_id.clone(),
        message: inbox::merge(&job.entries, |a| prepared.line(a, show_images)),
        model: None,
        temperature: None,
        images: (show_images && !prepared.images.is_empty()).then(|| prepared.images.clone()),
        focus: None,
        resume_run: job.entries.first().and_then(|e| e.resume_run.clone()),
    };
    let mut result = crate::commands::syn::send_message_inner(&app, &job.vault, ask(true), Surface::Telegram).await;

    // Nothing says which models can read a picture — the model list carries
    // no such field — so the answer is what says. A turn with photos that
    // fails is asked once more with the photos described instead of shown.
    // A model that could not see them answers the second time; a failure
    // with another cause fails again and is reported as before.
    if let Err(e) = &result {
        if !prepared.images.is_empty() && !stopped(&app) {
            log::info!("[Telegram] Asking again without showing the photos: {e}");
            result = crate::commands::syn::send_message_inner(&app, &job.vault, ask(false), Surface::Telegram).await;
        }
    }
    typing.abort();
    let background = finishing(&job.phase);

    if stopped(&app) {
        crate::error::logged("drop stopped Telegram messages", "inbox", with_db(&app, |db| inbox::remove(db, &job.entries)));
        if let Some(dir) = &staged {
            staging::discard(dir, &job.entries);
        }
    } else {
        match result {
            Ok(message) => {
                app.state::<TelegramState>().told_not_ready.store(false, Ordering::SeqCst);
                let turn = Turn {
                    chat_id: job.chat_id,
                    entries: &job.entries,
                    vault: &job.vault,
                    conversation_id: &job.conversation_id,
                    // Answered after other messages have been: said under
                    // the question it answers, or it reads as an answer to
                    // whatever was sent last.
                    reply_to: background.then(|| job.entries[0].message_id),
                };
                deliver(&app, &api, &words, &turn, &message).await;
                if let Err(e) = with_db(&app, |db| inbox::remove(db, &job.entries)) {
                    log::error!("[Telegram] Could not clear answered messages: {e}");
                    pause(&app, NOT_READY_RETRY);
                } else if let Some(dir) = &staged {
                    // Answered: whatever `capture` kept is in the vault now,
                    // and what it did not keep was never meant to be.
                    staging::discard(dir, &job.entries);
                }
                emit_status(&app);
            }
            Err(e) => {
                let reason = if e.to_string().contains(crate::commands::syn::SWITCHED_OFF) {
                    words.syn_off().to_string()
                } else {
                    e.to_string()
                };
                tell_not_ready(&app, &api, job.chat_id, &words.not_ready(&reason)).await;
                pause(&app, NOT_READY_RETRY);
            }
        }
    }

    set_in_flight(&app, &job.entries, false);
    if background {
        release_background_slot(&app);
    }
    let _ = done.send(());
    // The drain may have stopped while this ran, with messages arriving since
    // — or this may have paused it, and the retry has to be scheduled.
    drain(app);
}

/// What waits in the inbox and is not being answered already.
fn waiting_now(app: &AppHandle) -> Vec<Entry> {
    let busy = in_flight(app);
    with_db(app, inbox::waiting)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| !busy.contains(&entry.update_id))
        .collect()
}

fn in_flight(app: &AppHandle) -> HashSet<i64> {
    let state = app.state::<TelegramState>();
    let busy = state.in_flight.lock().unwrap_or_else(|e| e.into_inner()).clone();
    busy
}

fn set_in_flight(app: &AppHandle, entries: &[Entry], busy: bool) {
    let state = app.state::<TelegramState>();
    let mut held = state.in_flight.lock().unwrap_or_else(|e| e.into_inner());
    for entry in entries {
        if busy {
            held.insert(entry.update_id);
        } else {
            held.remove(&entry.update_id);
        }
    }
}

fn take_background_slot(app: &AppHandle) -> bool {
    app.state::<TelegramState>()
        .background
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < MOST_IN_BACKGROUND).then_some(n + 1))
        .is_ok()
}

fn release_background_slot(app: &AppHandle) {
    let _ = app
        .state::<TelegramState>()
        .background
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
}

fn pause(app: &AppHandle, how_long: Duration) {
    let state = app.state::<TelegramState>();
    *state.paused_until.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + how_long);
}

/// How long is left of a pause, if one is on.
fn paused(app: &AppHandle) -> Option<Duration> {
    let state = app.state::<TelegramState>();
    let until = *state.paused_until.lock().unwrap_or_else(|e| e.into_inner());
    until.and_then(|until| until.checked_duration_since(Instant::now()))
}

/// Leave a turn to finish on its own. `false` if it is already finishing —
/// telling somebody an answer is on its way, just after it arrived, is wrong.
fn to_background(phase: &AtomicU8) -> bool {
    phase
        .compare_exchange(FOREGROUND, BACKGROUND, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// Claim a turn's answer for sending. `true` when it had been left to finish
/// on its own, so the answer goes out as a reply.
fn finishing(phase: &AtomicU8) -> bool {
    phase.swap(FINISHING, Ordering::SeqCst) == BACKGROUND
}

async fn tell_not_ready(app: &AppHandle, api: &Api, chat_id: i64, text: &str) {
    let first_time = !app.state::<TelegramState>().told_not_ready.swap(true, Ordering::SeqCst);
    if first_time {
        say(api, chat_id, text).await;
    }
}

/// The files of a turn, fetched, and what Syn is told about each.
#[derive(Default)]
struct Prepared {
    /// Pictures to show the model, in base64.
    images: Vec<String>,
    /// Attachment ids among `images`.
    shown: std::collections::HashSet<String>,
    /// Why an attachment could not be fetched, by id.
    failed: std::collections::HashMap<String, String>,
}

impl Prepared {
    /// The line Syn reads for one attachment.
    fn line(&self, attachment: &Attachment, images_shown: bool) -> String {
        let mut line = format!("[attachment {}: {}", attachment.id, attachment.describe());
        if let Some(why) = self.failed.get(&attachment.id) {
            line.push_str(&format!(" — could not be fetched ({why}), so it cannot be kept"));
        } else if attachment.is_image() {
            if images_shown && self.shown.contains(&attachment.id) {
                line.push_str(" — shown");
            } else {
                line.push_str(" — not shown to you");
            }
        } else if matches!(attachment.kind, Kind::Voice | Kind::Audio) {
            line.push_str(" — you cannot listen to it");
        }
        line.push(']');
        line
    }
}

async fn prepare(api: &Api, dir: Option<&std::path::Path>, entries: &[Entry]) -> Prepared {
    let mut prepared = Prepared::default();
    for attachment in entries.iter().flat_map(|e| &e.attachments) {
        let Some(dir) = dir else {
            prepared.failed.insert(attachment.id.clone(), "there is nowhere to put it".to_string());
            continue;
        };
        match staging::fetch(api, dir, attachment).await {
            Ok(_) if attachment.is_image() => {
                if let Some(picture) = staging::preview(api, dir, attachment).await {
                    prepared.images.push(picture);
                    prepared.shown.insert(attachment.id.clone());
                }
            }
            Ok(_) => {}
            Err(why) => {
                log::info!("[Telegram] Could not fetch an attachment: {why}");
                prepared.failed.insert(attachment.id.clone(), why);
            }
        }
    }
    prepared
}

fn keep_typing(api: Api, chat_id: i64, phase: Arc<AtomicU8>) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        // Only while the chat is waiting on it. A turn left to finish on its
        // own would otherwise show "typing…" under the next question, for as
        // long as it takes.
        while phase.load(Ordering::SeqCst) == FOREGROUND {
            if let Err(e) = api.typing(chat_id).await {
                log::debug!("[Telegram] Could not show typing: {e}");
            }
            // Telegram shows it for five seconds.
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
    })
}

/// What an answered turn becomes in the chat.
#[derive(Debug, PartialEq, Eq)]
struct Reply {
    /// What `capture` kept, as the person will be shown it.
    kept: Vec<Kept>,
    /// `capture` was called and kept nothing.
    lost: bool,
    /// Whether Syn's own words are sent as well.
    answer: bool,
    /// Syn stopped to ask something only the app can show.
    needs_the_app: bool,
}

/// What a `capture` kept, from what the call returned rather than from what the
/// model said about it afterwards. `None` when it kept nothing.
fn kept_by(call: &crate::models::syn::SynToolCallEvent) -> Option<Kept> {
    let result: serde_json::Value = serde_json::from_str(&call.result_preview).ok()?;
    if result.get("kept").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }
    let count = |key: &str| result.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
    Some(Kept {
        text: call.tool_args.get("text").and_then(serde_json::Value::as_str).map(glimpse).unwrap_or_default(),
        images: count("images"),
        audio: count("audio"),
        files: count("files"),
    })
}

/// The start of what was kept, short enough to read as one line on a phone.
fn glimpse(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    if line.chars().count() <= 80 {
        return line.to_string();
    }
    format!("{}…", line.chars().take(80).collect::<String>().trim_end())
}

fn reply_for(message: &SynMessage) -> Reply {
    let captures: Vec<_> = message
        .tool_calls_log
        .iter()
        .flatten()
        .filter(|call| call.tool_name == crate::syn::tools::CAPTURE_TOOL)
        .collect();
    let kept: Vec<Kept> = captures.iter().filter_map(|call| kept_by(call)).collect();
    let said = message.content.trim();
    // A short line beside a capture is Syn saying "saved", which the app's own
    // line already says — and says truthfully.
    let quiet = !captures.is_empty() && said.chars().count() <= QUIET_CONFIRMATION_CHARS;
    Reply {
        lost: !captures.is_empty() && kept.is_empty(),
        answer: !said.is_empty() && !quiet,
        needs_the_app: said.is_empty() && captures.is_empty(),
        kept,
    }
}

/// One answered turn, and where it belongs.
struct Turn<'a> {
    chat_id: i64,
    entries: &'a [Entry],
    vault: &'a str,
    conversation_id: &'a str,
    /// The message to answer under, when the answer is not the latest thing
    /// in the chat.
    reply_to: Option<i64>,
}

async fn deliver(app: &AppHandle, api: &Api, words: &Words, turn: &Turn<'_>, message: &SynMessage) {
    let (chat_id, entries) = (turn.chat_id, turn.entries);
    // Only the first message goes out as a reply; the rest follow it.
    let mut reply_to = turn.reply_to;
    let reply = reply_for(message);
    if reply.needs_the_app {
        // Stopped to ask. The question goes to the phone, with buttons, rather
        // than a line saying to go and find a computer.
        let asked = match pending_question(turn.vault, turn.conversation_id) {
            Some(run) => send_card(app, api, words, chat_id, &run).await,
            None => false,
        };
        if !asked {
            say_to(api, chat_id, words.needs_the_app(), reply_to.take()).await;
        }
        return;
    }
    // Said in a message, by the app. The first version marked a capture with a
    // 👍 reaction and nothing else, and on a phone that reads as no answer at
    // all: a bot's reaction brings no notification and is easy to miss, and
    // the person holding the phone has no other way to know it was done.
    if !reply.kept.is_empty() {
        say_to(api, chat_id, &words.kept(&reply.kept), reply_to.take()).await;
    }
    if reply.lost {
        say_to(api, chat_id, words.not_kept(), reply_to.take()).await;
    }
    if reply.answer && !send_answer(api, words, chat_id, message, reply_to.take()).await {
        return;
    }
    if reply.lost {
        return;
    }
    for entry in entries {
        if let Err(e) = api.react(chat_id, entry.message_id, "👍").await {
            log::info!("[Telegram] Could not mark a message as done: {e}");
        }
    }
}

// ─────────────────────────────────────────────────────────────
//  Cards: a stopped run's question, answered with a button
// ─────────────────────────────────────────────────────────────

/// What a card's buttons are about, kept until one is pressed.
#[derive(Serialize, Deserialize)]
struct Card {
    run_id: String,
    /// What the card says, so the answer can be written under it.
    text: String,
    /// The notes offered, `(id, title)`, in button order. Empty on a permission card.
    #[serde(default)]
    candidates: Vec<(String, String)>,
    consent: bool,
}

/// The question the conversation's latest run stopped on, if it stopped on one.
///
/// The latest run, not any run still waiting: answering clears the question
/// but leaves the run's state as it was, and an older unanswered run is not
/// what the message just delivered was about.
fn pending_question(vault: &str, conversation_id: &str) -> Option<crate::syn::run::Run> {
    let latest = crate::syn::run::list_runs(vault, crate::syn::engine::is_live)
        .ok()?
        .into_iter()
        .filter(|run| run.conversation_id.as_deref() == Some(conversation_id))
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at))?;
    let run = crate::syn::run::get_run(vault, &latest.id).ok()?;
    (run.pending_choice.is_some() || run.pending_consent.is_some()).then_some(run)
}

fn short(text: &str, most: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= most {
        return text.to_string();
    }
    format!("{}…", text.chars().take(most).collect::<String>().trim_end())
}

/// A button's data: `k:{nonce}:{option}`, well inside Telegram's 64 bytes.
fn parse_press(data: &str) -> Option<(&str, usize)> {
    let (nonce, option) = data.strip_prefix("k:")?.split_once(':')?;
    Some((nonce, option.parse().ok()?))
}

/// Ask a stopped run's question on the phone. `false` if there was none to ask
/// or it could not be sent.
async fn send_card(app: &AppHandle, api: &Api, words: &Words, chat_id: i64, run: &crate::syn::run::Run) -> bool {
    use base64::Engine;
    let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 6]>());
    let button = |text: &str, option: usize| (text.to_string(), format!("k:{nonce}:{option}"));

    let (text, rows, candidates, consent) = if let Some(choice) = &run.pending_choice {
        let offered: Vec<(String, String)> = choice
            .candidates
            .iter()
            .take(MAX_CHOICES)
            .map(|c| (c.id.clone(), c.title.clone()))
            .collect();
        let mut rows: Vec<Vec<(String, String)>> = offered
            .iter()
            .enumerate()
            .map(|(i, (_, title))| vec![button(&short(title, 40), i)])
            .collect();
        rows.push(vec![button(words.none_of_these(), offered.len())]);
        (words.which_one(&choice.tool, choice.candidates.len()), rows, offered, false)
    } else if let Some(ask) = &run.pending_consent {
        let rows = vec![vec![button(words.allow_once(), 0), button(words.decline(), 1)]];
        (words.may_i(&ask.about), rows, Vec::new(), true)
    } else {
        return false;
    };

    if let Err(e) = api.send_buttons(chat_id, &render::escape(&text), &rows).await {
        log::warn!("[Telegram] Could not send a question: {e}");
        return false;
    }
    let card = Card { run_id: run.id.clone(), text, candidates, consent };
    let kept = serde_json::to_string(&card)
        .map_err(crate::error::AppError::from)
        .and_then(|json| with_db(app, |db| db.set_kv(&format!("{CARD_PREFIX}{nonce}"), &json)));
    if let Err(e) = kept {
        // The question is on the phone either way; its buttons will say it is
        // no longer open, and the app still has it.
        log::error!("[Telegram] Could not keep a card: {e}");
    }
    true
}

/// Put a run's question away without carrying the work on, with the answer
/// written into its transcript — the same record the app's cards leave.
fn close_question(vault: &str, run_id: &str, consent: bool, said: &str) -> crate::error::AppResult<()> {
    let mut run = crate::syn::run::get_run(vault, run_id)?;
    let iteration = run.spent.iterations;
    if consent {
        run.pending_consent = None;
        run.note(iteration, format!("Asked permission on Telegram; the user said {said}."));
    } else {
        run.pending_choice = None;
        run.note(iteration, format!("Asked which one on Telegram; the user said {said}."));
    }
    crate::syn::run::save_run(vault, &run)
}

/// A button pressed on a card. `false` when what it asked for could not be kept.
async fn answer_card(app: &AppHandle, api: &Api, words: &Words, pressed: &Pressed) -> bool {
    let quiet = |text: Option<&'static str>| async move {
        if let Err(e) = api.answer_callback(&pressed.callback_id, text).await {
            log::info!("[Telegram] Could not answer a button: {e}");
        }
    };

    let Some((nonce, option)) = parse_press(&pressed.data) else {
        quiet(None).await;
        return true;
    };
    let key = format!("{CARD_PREFIX}{nonce}");
    let card: Option<Card> = with_db(app, |db| db.get_kv(&key).ok().flatten())
        .and_then(|raw| serde_json::from_str(&raw).ok());
    let (Some(card), Some(vault)) = (card, active_vault(app)) else {
        quiet(Some(words.card_gone())).await;
        return true;
    };

    // Answered already — in the app, or by a second press here.
    let open = crate::syn::run::get_run(&vault, &card.run_id).is_ok_and(|run| {
        if card.consent {
            run.pending_consent.is_some()
        } else {
            run.pending_choice.is_some()
        }
    });
    if !open {
        quiet(Some(words.card_gone())).await;
        let _ = api.edit(pressed.chat_id, pressed.message_id, &render::escape(&card.text)).await;
        crate::error::logged("forget a Telegram card", &key, with_db(app, |db| db.delete_kv(&key)));
        return true;
    }

    let follow_up = |text: String, resume_run: Option<String>| Entry {
        update_id: pressed.update_id,
        chat_id: pressed.chat_id,
        message_id: pressed.message_id,
        text,
        forwarded_from: None,
        received_at: chrono::Utc::now().to_rfc3339(),
        attachments: Vec::new(),
        album: None,
        resume_run,
    };

    let answered: crate::error::AppResult<(String, Option<Entry>)> = match (card.consent, card.candidates.get(option)) {
        // A note picked: recorded the way the app records it, then said to Syn
        // — on a phone, pressing the button *is* saying go on.
        (false, Some((id, title))) => crate::commands::syn::syn_answer_choice(vault.clone(), card.run_id.clone(), id.clone())
            .await
            .map(|_| (title.clone(), Some(follow_up(words.meant(title, id), None)))),
        (false, None) => close_question(&vault, &card.run_id, false, "none of these")
            .map(|_| (words.none_of_these().to_string(), None)),
        (true, _) if option == 0 => crate::commands::syn::syn_answer_consent(
            vault.clone(),
            card.run_id.clone(),
            crate::syn::consent::Answer::Once,
        )
        .await
        .map(|_| (words.allow_once().to_string(), Some(follow_up(String::new(), Some(card.run_id.clone()))))),
        (true, _) => close_question(&vault, &card.run_id, true, "no").map(|_| (words.decline().to_string(), None)),
    };

    let (said, next) = match answered {
        Ok(done) => done,
        Err(e) => {
            log::error!("[Telegram] Could not record an answer: {e}");
            quiet(None).await;
            return true;
        }
    };

    quiet(None).await;
    let settled = format!("{}\n\n→ {}", render::escape(&card.text), render::escape(&said));
    if let Err(e) = api.edit(pressed.chat_id, pressed.message_id, &settled).await {
        log::info!("[Telegram] Could not mark a card answered: {e}");
    }
    crate::error::logged("forget a Telegram card", &key, with_db(app, |db| db.delete_kv(&key)));

    if let Some(entry) = next {
        if let Err(e) = with_db(app, |db| inbox::put(db, &entry)) {
            log::error!("[Telegram] Could not keep the answer for Syn: {e}");
            return false;
        }
        emit_status(app);
        drain(app.clone());
    }
    true
}

async fn send_answer(api: &Api, words: &Words, chat_id: i64, message: &SynMessage, reply_to: Option<i64>) -> bool {
    let mut reply_to = reply_to;
    for chunk in compose(words, message) {
        if let Err(e) = api.send_reply(chat_id, &chunk, reply_to.take()).await {
            log::warn!("[Telegram] Could not deliver an answer: {e}");
            return false;
        }
    }
    true
}

/// An answer with what the app shows beside it: whether it was a guess, and
/// what it was drawn from.
fn compose(words: &Words, message: &SynMessage) -> Vec<String> {
    let mut markdown = String::new();
    if message.footing == Some(crate::syn::footing::Footing::Guessing) {
        markdown.push_str(&format!("⚠︎ *{}*\n\n", words.guessing()));
    }
    // No sources line. A message carries sources only when no tool was used —
    // they are the notes retrieval put in the prompt, not notes the answer was
    // read from — and on a phone they were noise twice over: five daily notes
    // under "chào", and five more under a wrong "not in the vault".
    markdown.push_str(message.content.trim());
    render::to_html(&markdown, &Placeholders { chart: words.chart(), image: words.image() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::syn::{SourceRef, SynToolCallEvent};

    fn answer(content: &str, tools: &[&str]) -> SynMessage {
        SynMessage {
            id: "m".into(),
            role: "assistant".into(),
            content: content.into(),
            model: None,
            timestamp: String::new(),
            tokens: None,
            duration_ms: None,
            sources: None,
            footing: None,
            tool_calls_log: (!tools.is_empty()).then(|| tools.iter().map(|tool| call(tool)).collect()),
            images: None,
        }
    }

    /// A tool call as the run records it. `capture-failed` is a capture whose
    /// write did not happen.
    fn call(tool: &str) -> SynToolCallEvent {
        let (name, args, returned) = match tool {
            "capture" => (
                "capture",
                serde_json::json!({ "text": "cap from telegram\nsecond line" }),
                r#"{"kept":true,"id":"capture:pending:000000000001"}"#,
            ),
            "capture-failed" => (
                "capture",
                serde_json::json!({ "text": "cap from telegram" }),
                r#"{"error":"DB Set KV Error: disk full"}"#,
            ),
            "capture-photos" => (
                "capture",
                serde_json::json!({ "attachments": ["a9-1", "a10-1"] }),
                r#"{"kept":true,"id":"capture:pending:000000000002","images":2,"audio":0,"files":0,"missing":[]}"#,
            ),
            other => (other, serde_json::json!({}), "{}"),
        };
        SynToolCallEvent {
            conversation_id: "c".into(),
            tool_name: name.to_string(),
            tool_args: args,
            result_preview: returned.to_string(),
            iteration: 0,
        }
    }

    /// Whether something was kept is told in words, by the app, from what the
    /// call returned — not by a reaction nobody notices, and not by the model.
    #[test]
    fn what_a_turn_becomes_in_the_chat() {
        let kept = reply_for(&answer("", &["capture"]));
        assert_eq!(kept.kept.len(), 1);
        assert_eq!(kept.kept[0].text, "cap from telegram", "the first line of what was kept");
        assert!(!kept.answer && !kept.needs_the_app && !kept.lost);

        // Photos alone are counted, from what the call returned.
        let photos = reply_for(&answer("", &["capture-photos"]));
        assert_eq!(photos.kept, vec![Kept { text: String::new(), images: 2, audio: 0, files: 0 }]);
        assert_eq!(Words::english().kept(&photos.kept), "✓ Kept 2 photos in QuickCap");

        // Syn's own short "saved" is not said twice.
        assert!(!reply_for(&answer("Đã lưu vào QuickCap.", &["capture"])).answer);

        // Kept, and something more to say: both go.
        let both = reply_for(&answer(&"Đã lưu, và đây là ghi chú liên quan. ".repeat(5), &["capture"]));
        assert!(both.answer);
        assert_eq!(both.kept.len(), 1);

        // Syn says it saved; the call says it did not. The call is believed.
        let lost = reply_for(&answer("Đã lưu!", &["capture-failed"]));
        assert!(lost.lost && lost.kept.is_empty() && !lost.answer, "{lost:?}");

        assert!(reply_for(&answer("Bạn có 3 task.", &["query_nodes"])).answer);
        assert!(reply_for(&answer("   ", &[])).needs_the_app);
        assert!(reply_for(&answer("", &["trash_node"])).needs_the_app);
    }

    #[test]
    fn a_long_capture_is_shown_as_one_short_line() {
        let long = "một ý rất dài ".repeat(20);
        let shown = glimpse(&long);
        assert!(shown.chars().count() <= 81, "{shown}");
        assert!(shown.ends_with('…'));
        let words = Words::english();
        let text = |text: &str, images, audio, files| Kept { text: text.into(), images, audio, files };
        assert_eq!(words.kept(&[text("cap from telegram", 0, 0, 0)]), "✓ Kept in QuickCap: “cap from telegram”");
        assert_eq!(
            words.kept(&[text("hoá đơn", 1, 1, 0)]),
            "✓ Kept in QuickCap: “hoá đơn”, with 1 photo, 1 recording"
        );
        assert_eq!(words.kept(&[text("a", 0, 0, 0), text("b", 0, 0, 0)]), "✓ Kept 2 items in QuickCap.");
    }

    /// A button says which card and which option, and nothing else passes.
    /// A slow turn is left to finish on its own at most once, and never after
    /// its answer has started going out; its answer knows which it was.
    #[test]
    fn a_turn_is_left_to_finish_on_its_own_only_while_nothing_has_been_sent() {
        let slow = AtomicU8::new(FOREGROUND);
        assert!(to_background(&slow));
        assert!(!to_background(&slow), "told twice");
        assert!(finishing(&slow), "its answer goes out as a reply");

        let quick = AtomicU8::new(FOREGROUND);
        assert!(!finishing(&quick), "a turn waited for answers as it always did");
        assert!(!to_background(&quick), "an answer already going out is not announced as coming");
    }

    #[test]
    fn a_button_names_its_card_and_option() {
        assert_eq!(parse_press("k:AbCdEfGh:3"), Some(("AbCdEfGh", 3)));
        assert_eq!(parse_press("k:AbCdEfGh:x"), None);
        assert_eq!(parse_press("something else"), None);
        assert!("k:AbCdEfGh:8".len() <= 64, "inside Telegram's limit for button data");
        assert_eq!(short("Một tiêu đề rất dài cho một ghi chú rất dài", 10), "Một tiêu đ…");
    }

    /// A guess says so. Retrieval's sample does not come along: it is not what
    /// the answer was read from.
    #[test]
    fn an_answer_carries_its_footing_and_not_the_retrieval_sample() {
        let mut message = answer("Có lẽ là thứ Ba.", &[]);
        message.footing = Some(crate::syn::footing::Footing::Guessing);
        message.sources = Some(vec![SourceRef { id: "a".into(), title: "2026-05-03".into(), node_type: "note".into() }]);

        let sent = compose(&Words::english(), &message).join("\n");
        assert!(sent.starts_with("⚠︎ <i>a guess</i>"), "{sent}");
        assert!(!sent.contains("2026-05-03"), "{sent}");
    }
}
