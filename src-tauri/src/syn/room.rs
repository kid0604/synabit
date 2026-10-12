//! How much one thing Syn reads may take of the model's window.
//!
//! # Why this exists
//!
//! Every cut in Syn was a fixed number of characters — 12,000 for a
//! connector's answer, 24,000 for a page, 40,000 for a tool result — and
//! almost every one of them was argued against Ollama's default 8,192-token
//! window, which is the right worry for a model on a laptop and the wrong one
//! for a hosted model holding a million tokens. On those the cuts were the
//! limit: an answer the model could have read whole arrived as its first
//! tenth, and it said so or, worse, did not.
//!
//! So the cuts are now a share of the window the run is actually driving, with
//! the old numbers as the floor. A model with a small window is treated
//! exactly as before; a large one is given room in proportion.
//!
//! # How the window reaches code that does not know it
//!
//! The engine knows the window (`SynEngine::window_tokens`); the tools, the
//! connector and the page reader do not, and threading it through every call
//! would touch two dozen constructors. It is set for the run's task instead,
//! with `scope`, and read with [`scaled`]. Code running outside a run — the
//! Nexus screen calling a tool, a test — gets the floor.

/// The most one result may take, whatever the window: about 125,000 tokens.
/// Past this a single answer is not something to read but something to
/// compute over, and `resultset` + `crunch` are for that.
pub const CEILING_CHARS: usize = 500_000;

/// The share of the window one result may take. A tenth: a run reads several
/// things, and the conversation and the prompt need the rest.
const SHARE: f64 = 0.10;

/// Characters a token is taken to be, as everywhere else in Syn.
const CHARS_PER_TOKEN: f64 = 4.0;

tokio::task_local! {
    static WINDOW: u32;
}

/// Run `work` with `window_tokens` as the window every cut is measured against.
pub async fn scope<F: std::future::Future>(window_tokens: u32, work: F) -> F::Output {
    WINDOW.scope(window_tokens, work).await
}

/// The window of the run this code is part of, if it is part of one.
pub fn window() -> Option<u32> {
    WINDOW.try_with(|w| *w).ok()
}

/// What one result may take: `floor` on a small window or outside a run, a
/// tenth of the window on a large one, never past [`CEILING_CHARS`].
pub fn scaled(floor: usize) -> usize {
    scaled_in(window(), floor)
}

/// The same, for a window given rather than looked up.
pub fn scaled_in(window_tokens: Option<u32>, floor: usize) -> usize {
    match window_tokens {
        Some(w) => {
            let share = (w as f64 * CHARS_PER_TOKEN * SHARE) as usize;
            share.min(CEILING_CHARS).max(floor)
        }
        None => floor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_window_keeps_the_old_numbers_and_a_large_one_gets_room() {
        assert_eq!(scaled_in(None, 12_000), 12_000);
        assert_eq!(scaled_in(Some(8_192), 12_000), 12_000, "a laptop's window: as before");
        assert_eq!(scaled_in(Some(200_000), 12_000), 80_000);
        assert_eq!(scaled_in(Some(1_000_000), 12_000), 400_000);
        assert_eq!(scaled_in(Some(2_000_000), 12_000), CEILING_CHARS);
    }

    #[tokio::test]
    async fn the_window_is_the_runs_own() {
        assert_eq!(window(), None);
        let inside = scope(1_000_000, async { scaled(12_000) }).await;
        assert_eq!(inside, 400_000);
        assert_eq!(scaled(12_000), 12_000, "outside the run again");
    }
}
