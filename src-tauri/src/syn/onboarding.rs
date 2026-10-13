//! The first thing anybody does with Syn: give it a brain.
//!
//! # What this replaces
//!
//! A provider dropdown, a URL field, a key field, a save, a model list that
//! appeared only after saving, a model to choose from names nobody outside the
//! field can read, a second save, and a conversation to start by hand. Six
//! steps, four of them decisions a person has no way to make.
//!
//! Now: paste the key. Which provider it belongs to is in the key itself — each
//! issues keys with its own prefix — so is the address to send it to. Which
//! model to use is the best one the key can reach that does what Syn needs
//! (tools, a large window), from the capability table this app already keeps.
//! The person can change either later; neither is theirs to work out first.
//!
//! # What stays the person's decision
//!
//! Sending notes to a company's model. Pasting a key from that company is that
//! decision, made knowingly — the card says where the notes will go before the
//! button is pressed — and nothing here switches provider any other way.

use serde::Serialize;

use crate::models::syn::{ModelInfo, SynProvider};

/// Which provider issued a key, and where its API is, by the key's own prefix.
pub fn provider_for_key(key: &str) -> Option<(SynProvider, Option<&'static str>)> {
    let key = key.trim();
    if key.len() < 20 || key.contains(char::is_whitespace) {
        return None;
    }
    if key.starts_with("sk-ant-") {
        Some((SynProvider::Anthropic, None))
    } else if key.starts_with("AIza") {
        Some((SynProvider::Gemini, None))
    } else if key.starts_with("sk-") {
        Some((SynProvider::OpenAiCompat, Some("https://api.openai.com/v1")))
    } else {
        None
    }
}

/// Words in a model's name that mark it as not the one to talk to: smaller,
/// or for something other than conversation.
const NOT_FOR_CHAT: &[&str] = &[
    "embed", "embedding", "tts", "audio", "transcribe", "whisper", "realtime", "image", "dall", "moderation",
    "search", "instruct", "vision-preview", "aqa",
];
const SMALLER: &[&str] = &["mini", "nano", "lite", "haiku", "small", "flash-8b", "tiny"];

/// The best model a key can reach for Syn: one known to call tools, the
/// largest window, the full model over its smaller siblings, the newest name
/// last of all. `None` when nothing listed can talk.
pub fn best_model(models: &[ModelInfo]) -> Option<String> {
    let mut usable: Vec<(bool, bool, u32, bool, String)> = models
        .iter()
        .map(|m| if m.model.trim().is_empty() { m.name.clone() } else { m.model.clone() })
        .filter(|id| {
            let lower = id.to_ascii_lowercase();
            !NOT_FOR_CHAT.iter().any(|w| lower.contains(w))
        })
        .map(|id| {
            let cap = crate::syn::provider::capability::of(&id, true);
            let lower = id.to_ascii_lowercase();
            let full = !SMALLER.iter().any(|w| lower.contains(w));
            (cap.known && cap.tools, full, cap.context_window_tokens, !lower.contains("preview"), id)
        })
        .collect();
    usable.sort();
    usable.pop().map(|(.., id)| id)
}

/// What connecting came to, for the card.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Connected {
    pub provider: SynProvider,
    pub model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(ids: &[&str]) -> Vec<ModelInfo> {
        ids.iter()
            .map(|id| ModelInfo { name: id.to_string(), model: id.to_string(), size: 0, digest: String::new(), modified_at: String::new(), details: None })
            .collect()
    }

    #[test]
    fn a_key_says_whose_it_is() {
        assert_eq!(provider_for_key("sk-ant-api03-abcdefghijklmnopqrstuvwxyz").map(|p| p.0), Some(SynProvider::Anthropic));
        assert_eq!(provider_for_key("AIzaSyD-abcdefghijklmnopqrstuvwx").map(|p| p.0), Some(SynProvider::Gemini));
        assert_eq!(
            provider_for_key("sk-proj-abcdefghijklmnopqrstuvwxyz"),
            Some((SynProvider::OpenAiCompat, Some("https://api.openai.com/v1")))
        );
        assert_eq!(provider_for_key("hello"), None);
        assert_eq!(provider_for_key("sk-ant-api03 with spaces in it abcdef"), None, "pasted with something else");
        assert_eq!(provider_for_key("ghp_abcdefghijklmnopqrstuvwxyz0123"), None, "not a model provider's");
    }

    /// The full model that calls tools, over a smaller sibling, an embedding
    /// model, and a name nothing is known about.
    #[test]
    fn the_best_model_is_the_full_one_that_calls_tools() {
        let pick = best_model(&listed(&[
            "text-embedding-3-large",
            "claude-haiku-4-5",
            "claude-sonnet-4-5",
            "some-unknown-model",
            "gpt-4o-mini-tts",
        ]));
        assert_eq!(pick.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(best_model(&listed(&["text-embedding-3-small"])), None);
    }
}
