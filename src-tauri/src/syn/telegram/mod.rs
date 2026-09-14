//! Syn, through a Telegram bot.
//!
//! Another surface onto the same assistant, not a second assistant. Every
//! message ends up in `commands::syn::send_message_inner` with
//! `Surface::Telegram`, which is the same code a question typed into the app
//! goes through — the switch, the prompt, the ceilings, the reflection — with
//! a narrower set of tools. See `syn::surface`.
//!
//! What lives here is only what is Telegram's own:
//!
//! - `api` — the nine Bot API methods this uses, over `reqwest`.
//! - `gate` — what an update is, decided without reading what it says.
//! - `inbox` — messages kept until Syn has answered them.
//! - `pairing` — which one Telegram account may talk to this bot.
//! - `remind` — reminders the computer shows, sent to the phone as well.
//! - `render` — an answer in the HTML Telegram can show.
//! - `service` — the poller, the commands, and the hand-off to Syn.
//!
//! The design, and why it is shaped like this rather than like Hermes'
//! gateway, is `docs/syn-over-telegram-2026-09-13.md`.

pub mod api;
pub mod gate;
pub mod inbox;
pub mod pairing;
pub mod remind;
pub mod render;
mod service;
pub mod staging;
mod words;

pub use remind::hand_over;
pub use service::{
    forget, restart, start, status, PairedView, Problem, Status, TelegramState, STATUS_EVENT,
    TOKEN_SLOT, USERNAME_KEY,
};
