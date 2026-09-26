//! What a model can do, by its name.
//!
//! # Why a table
//!
//! Until now everything that depended on the model asked the *provider*
//! instead: a web page is 8,000 characters for a local model and 24,000 for a
//! hosted one, `num_ctx` is 8,192 whatever is loaded, and the privacy sentence
//! under the provider picker is one of three fixed strings. That is the right
//! answer for none of the interesting cases. Claude Haiku holds 200,000 tokens
//! and Claude Opus five times that; qwen3 on Ollama holds 40,000 and llama3 —
//! the first one — 8,192; an "OpenAI-compatible" endpoint at
//! `http://localhost:8080/v1` is llama.cpp on this laptop, and saying the
//! messages "go to the endpoint you choose" when that endpoint is this machine
//! is a privacy sentence that undersells the privacy.
//!
//! So this is a pure lookup from a provider, a model id and where the endpoint
//! is, to a `Capability`. It sends nothing and reads nothing, which is what
//! lets it be tested exhaustively and asked from anywhere — the settings
//! screen for a tier table, the prompt builder for a budget, the engine for a
//! window to compare `usage.input` against.
//!
//! # How a name is matched
//!
//! By family prefix, after the decoration is taken off: a Gemini `models/`
//! collection prefix, an OpenRouter `anthropic/` or `meta-llama/` owner, an
//! Ollama `:8b` tag, and upper case. Model ids change every few weeks and the
//! families change every year or two, so a prefix that knows `claude-opus-`
//! and reads the version after it is right about the next Opus the day it
//! ships, where a list of exact ids would be wrong about it.
//!
//! # When it does not know
//!
//! It says so — `known: false` — and assumes little: 8,192 tokens and no
//! tools, vision or reasoning on this machine, where that is what a small
//! local model genuinely has; 32,768 tokens and the same on a hosted endpoint,
//! where the window is almost certainly larger but a budget sized to a guess
//! that is too big is one that silently loses the system prompt. Better to be
//! told the model can do more than it gets than to find out it can do less.
//!
//! The numbers are what each maker publishes as the window. For Ollama that is
//! what the weights can hold, not what the server will give them: Ollama gives
//! what `num_ctx` asks for, and choosing that number is what this table is
//! for.

use serde::Serialize;

use crate::models::syn::{SynProvider, SynSettings};

/// What one model can do, and where it runs.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// How many tokens the model can hold — prompt, history, tools and reply.
    pub context_window_tokens: u32,
    /// Whether it calls tools natively.
    pub tools: bool,
    /// Whether it can read an image.
    pub vision: bool,
    /// Whether it thinks before it answers — and bills for it.
    pub reasoning: bool,
    /// Whether the messages leave this machine to reach it.
    pub hosted: bool,
    /// Whether the name was recognised. When false, everything above is the
    /// conservative guess described in the module docs, and the UI should say
    /// so rather than present it as a fact.
    pub known: bool,
}

/// What an unrecognised model on this machine is assumed to hold.
pub const UNKNOWN_LOCAL_WINDOW: u32 = 8_192;

/// What an unrecognised hosted model is assumed to hold.
pub const UNKNOWN_HOSTED_WINDOW: u32 = 32_768;

const K128: u32 = 131_072;
const M1: u32 = 1_000_000;

/// Whether the settings send messages off this machine.
///
/// `SynProvider::is_local` answers for the provider, and for three of the four
/// that is the whole answer: Ollama is here, Gemini and Anthropic are not. The
/// OpenAI shape is the exception, because it is a request shape and not a
/// place — pointed at `localhost` it is llama.cpp or LM Studio on this laptop,
/// and nothing leaves. Only a loopback address counts: a machine on the LAN is
/// somebody's computer, and "this machine" is the promise being made.
pub fn is_hosted(settings: &SynSettings) -> bool {
    match settings.provider {
        SynProvider::OpenAiCompat => !crate::timeline::media::is_loopback(&settings.openai_base_url),
        other => !other.is_local(),
    }
}

/// The capability of the model the settings would use, or of `model` if given.
pub fn for_settings(settings: &SynSettings, model: Option<&str>) -> Capability {
    let model = model.or(settings.default_model.as_deref()).unwrap_or("");
    of(model, is_hosted(settings))
}

/// The capability of `model`, served from this machine or not.
///
/// The provider does not appear because the name is what decides: `llama3.1`
/// is the same model whether Ollama or llama.cpp serves it, and `claude-` is
/// Claude whether it arrives through Anthropic or OpenRouter.
pub fn of(model: &str, hosted: bool) -> Capability {
    let id = bare(model);
    match family(&id) {
        Some((context_window_tokens, tools, vision, reasoning)) => Capability {
            context_window_tokens,
            tools,
            vision,
            reasoning,
            hosted,
            known: true,
        },
        None => Capability {
            context_window_tokens: if hosted { UNKNOWN_HOSTED_WINDOW } else { UNKNOWN_LOCAL_WINDOW },
            tools: false,
            vision: false,
            reasoning: false,
            hosted,
            known: false,
        },
    }
}

/// A model id without its decoration: lower case, no owner or collection
/// prefix, no Ollama tag.
fn bare(model: &str) -> String {
    let id = model.trim().to_ascii_lowercase();
    let id = id.rsplit('/').next().unwrap_or(&id).to_string();
    // `llama3.1:8b-instruct-q4_K_M` — the tag is size and quantisation, never
    // family, except where it is the only place a size-dependent fact lives,
    // which this table does not try to follow.
    id.split(':').next().unwrap_or(&id).to_string()
}

/// `(window, tools, vision, reasoning)` for a recognised family.
fn family(id: &str) -> Option<(u32, bool, bool, bool)> {
    if id.starts_with("claude-") {
        return Some(claude(id));
    }
    if id.starts_with("gemini-") {
        return gemini(id);
    }
    if let Some(found) = openai(id) {
        return Some(found);
    }
    open_weights(id)
}

/// The numeric parts of a dotted or dashed version, in order: `4-6` and `4.6`
/// both read `[4, 6]`. A date suffix (`20250929`) is not a version and stops
/// the reading.
fn version(rest: &str) -> Vec<u32> {
    rest.split(['-', '.'])
        .map_while(|part| part.parse::<u32>().ok().filter(|n| *n < 1000))
        .collect()
}

/// Claude. Every Claude this app can reach calls tools and reads images, and
/// every one since 3.7 can think.
///
/// The window is 1M for Opus and Sonnet from 4.6 on, and for Fable and Mythos;
/// 200K for Haiku and for the Opus and Sonnet before 4.6. Read from the
/// version rather than listed, so a Sonnet 5.5 is 1M the day it ships.
fn claude(id: &str) -> (u32, bool, bool, bool) {
    let rest = id.trim_start_matches("claude-");
    // The old naming put the version first: `claude-3-5-sonnet-20241022`.
    if rest.starts_with('3') {
        let v = version(rest);
        let thinks = v.as_slice() >= [3, 7].as_slice();
        return (200_000, true, true, thinks);
    }
    let (line, after) = rest.split_once('-').unwrap_or((rest, ""));
    let v = version(after);
    let at_least = |major: u32, minor: u32| {
        let (a, b) = (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0));
        (a, b) >= (major, minor)
    };
    let window = match line {
        "fable" | "mythos" => M1,
        "opus" | "sonnet" if at_least(4, 6) => M1,
        _ => 200_000,
    };
    (window, true, true, true)
}

/// Gemini. 2.0 onwards hold a million tokens; 2.5 onwards think. 1.5 Pro held
/// two million, and is long retired, but a name is a name.
fn gemini(id: &str) -> Option<(u32, bool, bool, bool)> {
    let v = version(id.trim_start_matches("gemini-"));
    let major = *v.first()?;
    let minor = v.get(1).copied().unwrap_or(0);
    let window = if major == 1 && id.contains("pro") { 2_000_000 } else { M1 };
    let thinks = (major, minor) >= (2, 5);
    Some((window, true, true, thinks))
}

/// OpenAI's own names, wherever they are served from.
fn openai(id: &str) -> Option<(u32, bool, bool, bool)> {
    // Most specific first: `gpt-4o` and `gpt-4.1` both start with `gpt-4`.
    if id.starts_with("gpt-oss") {
        return Some((K128, true, false, true));
    }
    if id.starts_with("gpt-5") {
        return Some((400_000, true, true, true));
    }
    if id.starts_with("gpt-4.1") {
        return Some((1_047_576, true, true, false));
    }
    if id.starts_with("gpt-4o") || id.starts_with("gpt-4-turbo") || id.starts_with("chatgpt-4o") {
        return Some((128_000, true, true, false));
    }
    if id.starts_with("gpt-4") {
        return Some((8_192, true, false, false));
    }
    if id.starts_with("gpt-3.5") {
        return Some((16_385, true, false, false));
    }
    // The o-series: `o1`, `o3`, `o3-mini`, `o4-mini`. A letter and a digit,
    // so `ollama` and `open-mistral` are not mistaken for one.
    let mut chars = id.chars();
    if chars.next() == Some('o') && chars.next().is_some_and(|c| c.is_ascii_digit()) {
        let vision = !id.contains("mini") || id.starts_with("o4");
        return Some((200_000, true, vision, true));
    }
    None
}

/// The open-weight families people run on Ollama, llama.cpp and LM Studio.
///
/// Windows are what the model card gives; tools and vision are what Ollama's
/// own library marks the family with. Where a family's later versions differ
/// from its first — llama3 held 8,192 tokens and llama3.1 holds 128K — the
/// later ones are matched first.
fn open_weights(id: &str) -> Option<(u32, bool, bool, bool)> {
    // (prefix, window, tools, vision, reasoning), most specific first.
    const TABLE: &[(&str, u32, bool, bool, bool)] = &[
        ("llama3.2-vision", K128, false, true, false),
        ("llama3.1", K128, true, false, false),
        ("llama3.2", K128, true, false, false),
        ("llama3.3", K128, true, false, false),
        ("llama4", 1_048_576, true, true, false),
        ("llama3", 8_192, false, false, false),
        ("llama2", 4_096, false, false, false),
        ("qwen3-coder", 262_144, true, false, false),
        ("qwen3-vl", 262_144, true, true, true),
        ("qwen3", 40_960, true, false, true),
        ("qwen2.5vl", 128_000, false, true, false),
        ("qwen2.5-vl", 128_000, false, true, false),
        ("qwen2.5", 32_768, true, false, false),
        ("qwq", 40_960, true, false, true),
        ("gemma3n", 32_768, false, false, false),
        ("gemma3", K128, false, true, false),
        ("gemma2", 8_192, false, false, false),
        ("mistral-small", K128, true, true, false),
        ("mistral-nemo", K128, true, false, false),
        ("mistral-large", K128, true, false, false),
        ("magistral", 40_000, true, false, true),
        ("mistral", 32_768, true, false, false),
        ("mixtral", 32_768, true, false, false),
        ("devstral", K128, true, false, false),
        ("deepseek-r1", K128, true, false, true),
        ("deepseek-v3", K128, true, false, false),
        ("deepseek-coder-v2", 163_840, false, false, false),
        ("phi4", 16_384, false, false, false),
        ("phi3", K128, false, false, false),
        ("llava", 4_096, false, true, false),
        ("granite3", K128, true, false, false),
        ("command-r", K128, true, false, false),
    ];
    TABLE
        .iter()
        .find(|(prefix, ..)| id.starts_with(prefix))
        .map(|&(_, window, tools, vision, reasoning)| (window, tools, vision, reasoning))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(model: &str) -> u32 {
        of(model, true).context_window_tokens
    }

    /// Opus and Sonnet from 4.6 on hold a million tokens; Haiku and the older
    /// ones hold 200K. Read from the version, so the next one is right the day
    /// it ships — a list would be wrong about it until somebody edited it.
    #[test]
    fn claude_is_read_from_its_version() {
        assert_eq!(window("claude-opus-5"), M1);
        assert_eq!(window("claude-opus-5-5"), M1);
        assert_eq!(window("claude-sonnet-5"), M1);
        assert_eq!(window("claude-sonnet-4-6"), M1);
        assert_eq!(window("claude-fable-5-1"), M1);
        assert_eq!(window("claude-haiku-4-5"), 200_000);
        assert_eq!(window("claude-haiku-4-5-20251001"), 200_000);
        assert_eq!(window("claude-sonnet-4-5-20250929"), 200_000, "a date is not a minor version");
        assert_eq!(window("claude-opus-4-1"), 200_000);
        assert_eq!(window("claude-3-5-sonnet-20241022"), 200_000);

        let opus = of("claude-opus-5", true);
        assert!(opus.tools && opus.vision && opus.reasoning && opus.known);
        assert!(!of("claude-3-5-haiku-20241022", true).reasoning, "3.5 did not think");
        assert!(of("claude-3-7-sonnet-20250219", true).reasoning, "3.7 did");
    }

    /// The same Claude through OpenRouter, with dots and an owner prefix.
    #[test]
    fn decoration_is_taken_off_before_matching() {
        assert_eq!(window("anthropic/claude-sonnet-4.6"), M1);
        assert_eq!(window("models/gemini-3.8-flash"), M1);
        assert_eq!(window("  Llama3.1:8B-instruct-q4_K_M "), K128);
        assert_eq!(window("meta-llama/llama3.3"), K128);
    }

    #[test]
    fn gemini_holds_a_million_and_thinks_from_2_5() {
        let flash = of("gemini-3.8-flash", true);
        assert_eq!(flash.context_window_tokens, M1);
        assert!(flash.tools && flash.vision && flash.reasoning);
        assert!(!of("gemini-2.0-flash", true).reasoning);
        assert!(of("gemini-2.5-pro", true).reasoning);
    }

    /// `gpt-4o` and `gpt-4.1` both begin `gpt-4`, and are nothing alike.
    #[test]
    fn openai_names_match_most_specific_first() {
        assert_eq!(window("gpt-4.1-mini"), 1_047_576);
        assert_eq!(window("gpt-4o-mini"), 128_000);
        assert_eq!(window("gpt-4"), 8_192);
        assert_eq!(window("gpt-5.6-luna"), 400_000);
        assert!(of("gpt-5", true).reasoning);
        assert!(of("o3", true).reasoning);
        assert!(of("o4-mini", true).vision);
        assert!(!of("o3-mini", true).vision);
        assert!(of("gpt-oss:20b", false).reasoning);
    }

    /// "o" and a digit is the o-series; "o" and a letter is something else.
    #[test]
    fn a_name_that_starts_with_o_is_not_automatically_the_o_series() {
        assert!(!of("olmo2", false).known);
        assert!(!of("openchat", false).known);
    }

    /// llama3 held 8K and llama3.1 holds 128K; the first must not swallow the
    /// second.
    #[test]
    fn open_weight_families_match_the_later_version_first() {
        assert_eq!(window("llama3"), 8_192);
        assert_eq!(window("llama3:8b"), 8_192);
        assert_eq!(window("llama3.1"), K128);
        assert_eq!(window("qwen2.5:7b"), 32_768);
        assert_eq!(window("qwen3:30b-a3b"), 40_960);
        assert!(of("qwen3", false).reasoning);
        assert!(of("gemma3:4b", false).vision);
        assert!(!of("gemma3:4b", false).tools);
        assert!(of("mistral", false).tools);
        assert!(of("deepseek-r1:14b", false).reasoning);
        assert!(of("llama3.2-vision", false).vision);
        assert!(!of("llama3.2-vision", false).tools);
    }

    /// Unknown is said, and assumed small: a budget sized to a guess that is
    /// too big is one that silently loses the system prompt.
    #[test]
    fn an_unknown_model_is_a_conservative_guess_and_says_so() {
        let here = of("my-finetune", false);
        assert_eq!(here.context_window_tokens, UNKNOWN_LOCAL_WINDOW);
        assert!(!here.known && !here.tools && !here.vision && !here.reasoning && !here.hosted);

        let there = of("some-new-model", true);
        assert_eq!(there.context_window_tokens, UNKNOWN_HOSTED_WINDOW);
        assert!(!there.known && there.hosted);

        assert!(!of("", true).known, "no model chosen is not a model");
    }

    /// Hosted follows the provider, except the OpenAI shape pointed at this
    /// machine — llama.cpp on the laptop sends nothing anywhere.
    #[test]
    fn hosted_is_where_the_messages_go_not_which_shape_they_take() {
        let with = |provider, url: &str| SynSettings {
            provider,
            openai_base_url: url.to_string(),
            ..SynSettings::default()
        };
        assert!(!is_hosted(&with(SynProvider::Ollama, "https://api.openai.com/v1")));
        assert!(is_hosted(&with(SynProvider::Gemini, "http://localhost:8080/v1")));
        assert!(is_hosted(&with(SynProvider::Anthropic, "http://localhost:8080/v1")));
        assert!(is_hosted(&with(SynProvider::OpenAiCompat, "https://api.openai.com/v1")));
        assert!(!is_hosted(&with(SynProvider::OpenAiCompat, "http://localhost:8080/v1")));
        assert!(!is_hosted(&with(SynProvider::OpenAiCompat, "http://127.0.0.1:1234/v1")));
        assert!(
            is_hosted(&with(SynProvider::OpenAiCompat, "http://192.168.1.20:8080/v1")),
            "another computer on the network is not this machine"
        );
    }

    /// What the settings screen will ask: the configured model, or another.
    #[test]
    fn the_settings_answer_for_their_own_model_unless_asked_about_another() {
        let s = SynSettings {
            provider: SynProvider::Anthropic,
            default_model: Some("claude-haiku-4-5".into()),
            ..SynSettings::default()
        };
        assert_eq!(for_settings(&s, None).context_window_tokens, 200_000);
        assert_eq!(for_settings(&s, Some("claude-opus-5")).context_window_tokens, M1);
        assert!(for_settings(&s, None).hosted);
    }
}
