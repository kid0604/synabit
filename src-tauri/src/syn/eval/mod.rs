//! How good Syn is, measured with real models.
//!
//! # Why this exists
//!
//! Every review of Syn so far scored it by reading the code and a month of run
//! files. Neither says whether a change made it better. The 2026-09-26 review
//! asked for this ("Phase H — measure with real models") before any new
//! capability, and capabilities shipped without it. So this is the instrument:
//! a fixed set of tasks, run through the same path a message from the app
//! takes, graded by what the vault looks like afterwards and what the run did.
//!
//! # What it drives
//!
//! `commands::syn::send_message_inner` — the switch, the prompt, retrieval,
//! the gate, the tools, `settle` (footing) and `reflect_after` (memory) —
//! against a fixture vault on a mock Tauri runtime. The only two seams are the
//! model (`provider_override`, from the environment rather than this machine's
//! keychain) and the web (`web`, pages from the task instead of the internet).
//! Everything between them is the product.
//!
//! # Running it
//!
//! ```bash
//! SYN_EVAL_PROVIDER=anthropic SYN_EVAL_MODEL=claude-haiku-4-5 SYN_EVAL_KEY=… \
//!   cargo test --lib syn::eval::suite -- --ignored --nocapture
//! ```
//!
//! `SYN_EVAL_TASKS` picks `smoke`, `all` (the default) or a category name;
//! `SYN_EVAL_TRIALS` how many times each task runs (default 3); `SYN_EVAL_OUT`
//! where the report goes. `harness_runs_the_product_path` below runs without a
//! model and is what keeps the harness itself honest in CI.

pub mod fixture;
pub mod grade;
pub mod harness;
pub mod report;
pub mod tasks;
pub mod web;

use std::cell::RefCell;

use crate::models::syn::SynSettings;
use crate::syn::provider::ChatProvider;

type ProviderFactory = Box<dyn Fn(&SynSettings) -> Box<dyn ChatProvider>>;

thread_local! {
    /// The model being measured, for this thread only.
    ///
    /// Thread-local rather than global because `cargo test` runs the rest of
    /// the suite on other threads, and a `#[tokio::test]` drives its future on
    /// the thread that started it — so the override reaches the run it was set
    /// for and nothing else.
    static PROVIDER: RefCell<Option<ProviderFactory>> = const { RefCell::new(None) };
}

/// The provider the harness set for this thread, if any.
pub(crate) fn provider_override(settings: &SynSettings) -> Option<Box<dyn ChatProvider>> {
    PROVIDER.with(|slot| slot.borrow().as_ref().map(|make| make(settings)))
}

/// Answer every run on this thread with what `make` builds, until the guard drops.
pub fn use_provider(make: ProviderFactory) -> ProviderGuard {
    PROVIDER.with(|slot| *slot.borrow_mut() = Some(make));
    ProviderGuard
}

pub struct ProviderGuard;

impl Drop for ProviderGuard {
    fn drop(&mut self) {
        PROVIDER.with(|slot| *slot.borrow_mut() = None);
    }
}

#[cfg(test)]
mod tests;
