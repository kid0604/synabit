//! Tua lại, tier 3: every dated thing the vault already says, in one place.
//!
//! The design is `docs/timeline-2026-09-17.md`. These are the items
//! read straight out of fields that hold dates (§4.8.1), with no model and no
//! guessing, kept in `timeline.db` beside the vault cache (§4.7).
//!
//! - [`when`] reads a date to the precision anybody knows it.
//! - [`derive`] turns one node into the items it implies.
//! - [`store`] keeps them, catches up with the cache, and answers "what
//!   happened then".
//! - [`extract`] is tier 1: what a model reads out of the person's own
//!   words, kept as proposals until the person accepts them (Nhát E).
//! - [`reflect`] is decisions and looking back on them (Nhát F).
//! - [`fold`] turns a day's note into the box its events came in, and its
//!   pictures into evidence of them.
//! - [`presence`] is when an object was around, read off the events that
//!   name it: a worldline rather than a dated edge.
//! - [`magnitude`] is how big an event was, read from its core alone
//!   (`docs/timeline-2026-09-17.md` §4.8).
//! - [`media`] is transcripts and captions that stand in for recordings and
//!   pictures, and pictures grouped into moments (Nhát G).
//!
//! Nothing here writes to the vault. `Timeline/` is reserved for the tier-1
//! results later nhát will write there, and is kept out of the node index
//! from now on so those files never become notes.

pub mod asked;
pub mod asking;
pub mod blocks;
pub mod derive;
pub mod extract;
pub mod fold;
pub mod frame;
pub mod ledger;
pub mod magnitude;
pub mod media;
pub mod moments;
pub mod onthisday;
pub mod pin;
pub mod presence;
pub mod query;
pub mod quiet;
pub mod reader;
pub mod reset;
pub mod reflect;
pub mod store;
pub mod when;
pub mod year;

pub use store::TimelineStore;

/// Tauri-managed state. Only timeline commands lock it, and they lock it
/// before the vault cache, so the two locks are always taken in one order.
pub type TimelineState = std::sync::Mutex<TimelineStore>;

/// The vault folder that holds what the timeline derived rather than what the
/// user wrote.
pub const FOLDER: &str = "Timeline";

/// Whether a vault-relative path is one of the timeline's own files: JSON
/// under `Timeline/` at the vault root.
///
/// Only at the root: a user's own `Projects/Timeline/plan.md` is a note. And
/// only JSON, which is all the app writes there: a vault that already kept
/// notes in a root `Timeline/` folder keeps them as notes.
pub fn is_timeline_path(rel_id: &str) -> bool {
    let mut parts = rel_id.split(['/', '\\']);
    parts.next() == Some(FOLDER)
        && parts.next().is_some()
        && rel_id
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_folder_at_the_vault_root_is_the_timelines() {
        assert!(is_timeline_path("Timeline/2016/2016-05.macbook.json"));
        assert!(is_timeline_path("Timeline\\ledger\\macbook\\2026-09.json"));
        assert!(!is_timeline_path("Projects/Timeline/plan.md"));
        assert!(!is_timeline_path("Timeline.md"));
        assert!(!is_timeline_path("Timeline"));
        assert!(!is_timeline_path("Timeline/2019 trip.md"), "a note the user kept there");
        assert!(!is_timeline_path("Timelines/a.md"));
    }
}
