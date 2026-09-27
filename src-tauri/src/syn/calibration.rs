//! How many characters a token really is, learned from what providers charge.
//!
//! # Why four was never a measurement
//!
//! `prompt.rs` estimates tokens at four characters each, and says so. Four is
//! roughly right for English prose through an OpenAI tokenizer and wrong in
//! both directions elsewhere: Vietnamese with its diacritics usually costs
//! noticeably more tokens per character, code and JSON cost more, and every
//! provider's tokenizer is its own. A vault that talks Vietnamese to a local
//! Llama and one that talks English to Gemini are not spending the same
//! window, and the estimate said they were.
//!
//! Every hosted reply says what it charged for input (`Usage::input`), and
//! Syn knows exactly how many characters it sent. The ratio of the two is the
//! measurement the estimate was standing in for. This module keeps a running
//! average of it per vault, provider and model, and nothing else.
//!
//! # No language assumption
//!
//! The default is four, and it is only a starting point. Nothing here guesses
//! from the text whether it is Vietnamese; a vault that writes Vietnamese will
//! simply report a lower ratio, and the average follows it.
//!
//! # Not wired yet
//!
//! This module is pure plus a file. The engine is the only place that holds
//! both halves of a sample — the request it sent and the `Usage` that came back
//! — and it is where `record` belongs:
//!
//! - **When**: after the *first* provider reply of a turn, when that reply's
//!   `usage.input` is `Some`. Later rounds of the same turn re-send the same
//!   prefix plus tool results and add little that is new.
//! - **With what**: `chars` must be every character `usage.input` counts — the
//!   rendered system prompt, the conversation messages' content, and the
//!   serialised tool declarations (`tools::payload_cost().chars`). Passing the
//!   system prompt alone against a provider's whole input count would teach a
//!   ratio several times too small.
//! - **Skip** a turn that carried images. An image is hundreds of tokens and no
//!   characters, and one would drag the average toward the floor.
//!
//! Then `load(...).tokens_for(chars)` can replace `chars / 4` wherever an
//! estimate is shown or budgeted against.

use crate::error::AppResult;
use crate::models::syn::SynProvider;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the estimate starts, before any provider has said anything.
///
/// The same four `prompt.rs` uses, so a vault with no samples shows exactly
/// the numbers it showed before this existed.
pub const DEFAULT_CHARS_PER_TOKEN: f64 = 4.0;

/// The range any estimate is held to.
///
/// Below 1.5 characters a token, or above 6, is not a tokenizer — it is a
/// sample that counted the wrong characters: an image nobody skipped, tool
/// declarations left out of `chars`, a provider that reported cached input
/// separately. Clamping keeps one such sample from walking the average off a
/// cliff while still letting real Vietnamese (low) and real English (high)
/// through.
pub const MIN_CHARS_PER_TOKEN: f64 = 1.5;
pub const MAX_CHARS_PER_TOKEN: f64 = 6.0;

/// How much a new sample moves an estimate that is already settled.
///
/// A fifth: enough that switching the conversation from English to Vietnamese
/// shows within a handful of turns, little enough that one odd turn does not
/// swing it.
const WEIGHT: f64 = 0.2;

/// A request smaller than this says more about per-message overhead than about
/// the tokenizer, and is not used.
const SMALLEST_SAMPLE_TOKENS: u64 = 200;

/// One provider and model's learned ratio, and how many samples it rests on.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub chars_per_token: f64,
    pub samples: u32,
}

impl Default for Estimate {
    fn default() -> Self {
        Self { chars_per_token: DEFAULT_CHARS_PER_TOKEN, samples: 0 }
    }
}

impl Estimate {
    /// This estimate, having seen one more request.
    ///
    /// A plain mean for the first few samples, then an exponentially weighted
    /// one. Starting straight into a 0.2-weighted average would leave a vault
    /// that is really at 2.8 reporting 3.7 after its first answer, which is
    /// the default wearing the costume of a measurement. The mean lets the
    /// first samples count fully; the weight takes over once one sample is
    /// worth less than a fifth, so the estimate keeps following a vault whose
    /// language or model changes.
    ///
    /// Pure. A sample too small to mean anything, or with no tokens, returns
    /// the estimate unchanged rather than an error: the caller has nothing
    /// useful to do about it.
    pub fn observe(self, chars: usize, input_tokens: u64) -> Self {
        if input_tokens < SMALLEST_SAMPLE_TOKENS || chars == 0 {
            return self;
        }
        let sample = (chars as f64 / input_tokens as f64)
            .clamp(MIN_CHARS_PER_TOKEN, MAX_CHARS_PER_TOKEN);
        let samples = self.samples.saturating_add(1);
        let weight = (1.0 / f64::from(samples)).max(WEIGHT);
        let previous = self.chars_per_token.clamp(MIN_CHARS_PER_TOKEN, MAX_CHARS_PER_TOKEN);
        Self {
            chars_per_token: (previous + weight * (sample - previous))
                .clamp(MIN_CHARS_PER_TOKEN, MAX_CHARS_PER_TOKEN),
            samples,
        }
    }

    /// Characters, as this estimate reckons them in tokens. Rounded up, so an
    /// estimate errs toward a prompt costing more, which is the safe side of a
    /// budget.
    pub fn tokens_for(&self, chars: usize) -> usize {
        (chars as f64 / self.chars_per_token).ceil() as usize
    }
}

/// Every estimate a vault has learned, by provider and then model.
///
/// Nested rather than keyed by `"provider/model"`, because model names already
/// contain slashes (`openai/gpt-4o` through a router) and a key that has to be
/// parsed back is a key that will one day be parsed wrong.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct Calibration {
    #[serde(default)]
    estimates: BTreeMap<String, BTreeMap<String, Estimate>>,
}

fn path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join("Syn").join("calibration.json")
}

fn read(vault_path: &str) -> Calibration {
    // Unreadable or malformed is the same as absent: this file is a cache of
    // measurements, and losing it costs a few turns of relearning, not data.
    std::fs::read_to_string(path(vault_path))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The estimate for this vault, provider and model — the default if nothing
/// has been learned yet.
pub fn load(vault_path: &str, provider: &SynProvider, model: &str) -> Estimate {
    read(vault_path)
        .estimates
        .get(provider.key_slot())
        .and_then(|models| models.get(model))
        .copied()
        .unwrap_or_default()
}

/// Fold one reply into the stored estimate, and return what it became.
///
/// See the module comment for when to call this and what `chars` must count.
/// Written through a temporary file and a rename, like `settings.json`. Two
/// runs finishing at once can each read the old value and one sample is lost;
/// that is a running average missing a point, and not worth a lock.
pub fn record(
    vault_path: &str,
    provider: &SynProvider,
    model: &str,
    chars: usize,
    input_tokens: u64,
) -> AppResult<Estimate> {
    let mut all = read(vault_path);
    let slot = all
        .estimates
        .entry(provider.key_slot().to_string())
        .or_default()
        .entry(model.to_string())
        .or_default();
    let updated = slot.observe(chars, input_tokens);
    if updated == *slot {
        return Ok(updated);
    }
    *slot = updated;

    // Whole-file last writer wins across devices, which is acceptable here:
    // this is a running average, and losing the other device's last few samples
    // costs a few turns of relearning. `vault_json` stamps
    // `metadata.updated_at` so the newer copy is the one that wins.
    crate::syn::vault_json::write(&path(vault_path), &all)?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing learned means exactly the old arithmetic, so shipping this
    /// changes no number anybody is shown until a provider has answered.
    #[test]
    fn an_estimate_with_no_samples_is_the_old_four() {
        let fresh = Estimate::default();
        assert_eq!(fresh.chars_per_token, 4.0);
        assert_eq!(fresh.tokens_for(4000), 1000);
    }

    /// The first real reply counts in full. A vault that is actually at 2.8
    /// should not report 3.7 because the default was averaged in.
    #[test]
    fn the_first_sample_replaces_the_default() {
        let learned = Estimate::default().observe(28_000, 10_000);
        assert!((learned.chars_per_token - 2.8).abs() < 1e-9, "{learned:?}");
        assert_eq!(learned.samples, 1);
    }

    /// Settled estimates move by a fifth of the gap, so one odd turn nudges
    /// and a lasting change wins within a handful.
    #[test]
    fn a_settled_estimate_follows_a_change_without_lurching() {
        let settled = Estimate { chars_per_token: 4.0, samples: 50 };
        let once = settled.observe(25_000, 10_000);
        assert!((once.chars_per_token - 3.7).abs() < 1e-9, "{once:?}");

        let mut after = settled;
        for _ in 0..15 {
            after = after.observe(25_000, 10_000);
        }
        assert!((after.chars_per_token - 2.5).abs() < 0.1, "{after:?}");
    }

    /// A sample that could only come from counting the wrong characters is
    /// held to the range, and so is everything it produces.
    #[test]
    fn an_absurd_sample_cannot_push_the_estimate_out_of_range() {
        let low = Estimate::default().observe(100, 10_000);
        assert_eq!(low.chars_per_token, MIN_CHARS_PER_TOKEN);
        let high = Estimate::default().observe(1_000_000, 1_000);
        assert_eq!(high.chars_per_token, MAX_CHARS_PER_TOKEN);
    }

    /// No tokens, no characters, or a request too small to say anything
    /// about the tokenizer: the estimate stands.
    #[test]
    fn a_sample_that_says_nothing_changes_nothing() {
        let settled = Estimate { chars_per_token: 3.1, samples: 9 };
        assert_eq!(settled.observe(10_000, 0), settled);
        assert_eq!(settled.observe(0, 5_000), settled);
        assert_eq!(settled.observe(400, 100), settled);
    }

    /// Kept per provider and per model, because the same prompt is a
    /// different number of tokens to each tokenizer.
    #[test]
    fn estimates_are_kept_per_provider_and_model_and_survive_a_reload() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_str().expect("utf-8 path");

        assert_eq!(load(vault, &SynProvider::Gemini, "gemini-2.5-flash"), Estimate::default());

        record(vault, &SynProvider::Gemini, "gemini-2.5-flash", 30_000, 10_000).expect("saved");
        record(vault, &SynProvider::OpenAiCompat, "openai/gpt-4o", 45_000, 10_000).expect("saved");

        let gemini = load(vault, &SynProvider::Gemini, "gemini-2.5-flash");
        assert!((gemini.chars_per_token - 3.0).abs() < 1e-9, "{gemini:?}");
        let routed = load(vault, &SynProvider::OpenAiCompat, "openai/gpt-4o");
        assert!((routed.chars_per_token - 4.5).abs() < 1e-9, "a slash in the name is fine: {routed:?}");
        assert_eq!(
            load(vault, &SynProvider::Gemini, "gemini-2.5-pro"),
            Estimate::default(),
            "another model starts from the default"
        );
    }

    /// A damaged file is a cache that has to be relearned, not an error the
    /// turn has to handle.
    #[test]
    fn a_damaged_file_reads_as_nothing_learned() {
        let dir = tempfile::tempdir().expect("temp vault");
        std::fs::create_dir_all(dir.path().join("Syn")).expect("Syn dir");
        std::fs::write(dir.path().join("Syn").join("calibration.json"), "{not json").expect("write");
        let vault = dir.path().to_str().expect("utf-8 path");

        assert_eq!(load(vault, &SynProvider::Ollama, "llama3.2"), Estimate::default());
        let learned = record(vault, &SynProvider::Ollama, "llama3.2", 20_000, 10_000).expect("saved over it");
        assert_eq!(learned.samples, 1);
    }
}
