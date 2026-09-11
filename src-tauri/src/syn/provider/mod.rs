//! What Syn needs from a language model, and nothing about who provides it.
//!
//! `engine.rs` used to be an Ollama client with a tool-calling loop wrapped
//! around it. The loop is the valuable part and it is provider-neutral: prune
//! the history, ask for a completion, run whatever tools came back, ask again.
//! Only the four calls underneath it were Ollama-shaped.
//!
//! This module is those four calls. Each provider owns its own wire types and
//! converts at this boundary, because the two shapes disagree in ways that are
//! invisible until they are not:
//!
//! - Ollama sends tool arguments as a JSON **object**; OpenAI sends them as a
//!   **string** holding JSON.
//! - Ollama matches a tool result to its call by position; OpenAI rejects a
//!   `tool` message that does not name a `tool_call_id`.
//! - Ollama takes images as a sibling `images` array of base64; OpenAI takes
//!   them as `image_url` parts inside the content.
//! - `num_ctx` is a thing you ask Ollama for. Everywhere else the context
//!   window is a property of the model and there is nothing to send.
//! - Gemini has no `tool` role and no system message: tool results are
//!   `functionResponse` parts in a `user` turn, the system prompt is a separate
//!   `systemInstruction`, and every function call carries a signature that has
//!   to come back verbatim. See `gemini`.
//!
//! Streaming is a callback rather than a `Stream`, so the trait stays
//! object-safe and the caller keeps deciding what a token means — today that
//! is a Tauri event, and the provider does not need to know it.

pub mod gemini;
pub mod ollama;
pub mod openai;

use async_trait::async_trait;

use crate::error::AppResult;
use crate::models::syn::{ModelInfo, ProviderStatus, SynProvider, ToolCall, ToolDefinition};

/// One message on the way to a model.
///
/// Deliberately not `SynMessage`: that is what a conversation on disk holds,
/// with ids, timestamps, token counts and RAG sources that no provider wants.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// `system`, `user`, `assistant` or `tool`.
    pub role: String,
    pub content: String,
    /// Set on an assistant message that asked for tools, echoed back so the
    /// model can see what it requested.
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Set on a `tool` message, naming the call this is the result of. Ollama
    /// ignores it; the OpenAI shape requires it.
    pub tool_call_id: Option<String>,
    /// Base64-encoded images, for vision models.
    pub images: Option<Vec<String>>,
}

impl ChatMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            images: None,
        }
    }
}

/// One completion request.
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [ChatMessage],
    pub temperature: Option<f64>,
    /// Context window to ask for. Only Ollama can be told; other providers
    /// ignore it, and that is the point of having more than one.
    pub num_ctx: u32,
    pub tools: Option<&'a [ToolDefinition]>,
}

/// What a turn cost, as the provider counted it.
///
/// # Why four numbers and not one
///
/// It was one — "tokens generated" — and that was already the wrong shape
/// before a second provider arrived.
///
/// **Input was never counted at all.** Every turn re-sends the system prompt,
/// nearly sixteen thousand characters of tool declarations, the conversation so
/// far and every page read into it; the reply is a few hundred words. The token
/// budget was therefore measuring the small half, and on a streamed
/// OpenAI-compatible request it was measuring nothing, because usage is not
/// sent unless it is asked for. Every run on disk says `tokens: 0`.
///
/// **And input is no longer one number.** Anthropic splits it into cache writes
/// and cache reads, Gemini reports `cachedContentTokenCount`, OpenAI reports
/// `prompt_tokens_details.cached_tokens` — and cached input is priced at a
/// fraction of fresh input. For an app that sends the same tool declarations on
/// every single turn, that distinction is most of the bill. One number would
/// say the prompt is enormous and hide that nearly all of it is cheap.
///
/// **Output has the same problem in reverse.** Reasoning tokens are charged and
/// never shown — `reasoning_tokens` on OpenAI, `thoughtsTokenCount` on Gemini.
/// Counting only what was written makes a reasoning model look cheap.
///
/// `None` everywhere a provider says nothing, which is different from zero and
/// has to stay different: zero is a measurement.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    /// Everything sent: prompt, tools, conversation, tool results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    /// How much of `input` the provider served from its own cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cached: Option<u64>,
    /// What the model wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    /// Reasoning that was charged for and nobody was shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_hidden: Option<u64>,
}

impl Usage {
    /// What the turn is charged, as far as anything here can tell.
    ///
    /// `None` when the provider said nothing at all, because a budget that
    /// treats silence as zero is a budget that never stops anything — which is
    /// exactly what happened for as long as this was one field nobody filled.
    pub fn charged(&self) -> Option<u64> {
        match (self.input, self.output) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        }
    }

    /// Whether the provider reported anything.
    pub fn is_silent(&self) -> bool {
        self.input.is_none()
            && self.input_cached.is_none()
            && self.output.is_none()
            && self.output_hidden.is_none()
    }
}

/// What came back, whether it was streamed or not.
#[derive(Debug, Default)]
pub struct ChatReply {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    /// What it cost, as the provider counted it.
    pub usage: Usage,
    /// Generation time as the provider measured it. `None` means the caller
    /// should fall back to its own wall clock.
    pub duration_ms: Option<u64>,
}

/// Where streamed tokens go, and how a provider learns it should stop.
///
/// Both are borrowed closures rather than owned state so that a provider can
/// neither hold on to them past the call nor decide what stopping means.
pub struct StreamSink<'a> {
    /// Called once per token, with the text to append.
    pub on_token: &'a (dyn Fn(&str) + Send + Sync),
    /// Consulted between chunks. Returning true abandons the stream and keeps
    /// whatever text arrived so far.
    pub stop_requested: &'a (dyn Fn() -> bool + Send + Sync),
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    /// Which provider this is, for logging and for the settings round-trip.
    fn id(&self) -> SynProvider;

    /// Is it reachable, and what does it say about itself?
    ///
    /// Answers rather than fails: "not connected" is the expected state on a
    /// machine where Ollama is not running, and the UI polls this.
    async fn check_status(&self) -> AppResult<ProviderStatus>;

    /// The models this provider will accept as a `model` argument.
    async fn list_models(&self) -> AppResult<Vec<ModelInfo>>;

    /// One completion, waited for in full.
    ///
    /// This is the call the tool loop makes: it needs the whole reply,
    /// including any `tool_calls`, before it can decide what to do next.
    async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply>;

    /// One completion, delivered token by token through `sink`.
    async fn chat_streaming(
        &self,
        req: ChatRequest<'_>,
        sink: &StreamSink<'_>,
    ) -> AppResult<ChatReply>;

    /// Whether `chat_streaming` reports tool calls as well as text.
    ///
    /// The tool loop has to see `tool_calls` before it can decide what to do
    /// next, so a provider that only returns them from a non-streaming call
    /// forces the loop to be non-streaming — and then the answer arrives as
    /// one block after a long silence, because the loop already has the whole
    /// text by the time it is allowed to emit any of it.
    ///
    /// Ollama is that provider: its streamed chunks carry content only. The
    /// OpenAI shape streams tool calls as deltas, so the loop can stream
    /// throughout and the user sees words as they are generated.
    fn streams_tool_calls(&self) -> bool {
        false
    }
}

/// The provider these settings describe, holding the key they need.
///
/// # Why one function
///
/// This `match` was written out four times — the command that serves the app,
/// and three measurement harnesses — and each copy had to learn about a new
/// provider on its own. The copy that is forgotten is not a compile error in
/// the others; it is a harness that silently measures the wrong thing. So there
/// is one, and the caller only has to find the key.
///
/// `api_key` is the key filed under `settings.provider.key_slot()`, or `None`.
/// Reading it is left to the caller because the app reads the keychain on a
/// blocking thread with a timeout and a test harness does not.
pub fn for_settings(
    settings: &crate::models::syn::SynSettings,
    api_key: Option<String>,
) -> Box<dyn ChatProvider> {
    match settings.provider {
        SynProvider::Ollama => Box::new(ollama::OllamaProvider::new(&settings.ollama_url)),
        SynProvider::OpenAiCompat => Box::new(openai::OpenAiCompatProvider::new(
            &settings.openai_base_url,
            api_key,
            settings.openai_reasoning_effort.clone(),
        )),
        SynProvider::Gemini => Box::new(gemini::GeminiProvider::new(api_key)),
    }
}

/// The media type of a base64 payload, read from its first bytes.
///
/// The vault stores raw base64 with no note of what it is, and every hosted API
/// wants one declared — OpenAI in a data URI, Gemini as `inlineData.mimeType`.
/// Guessing `jpeg` for a PNG is rejected by some servers and silently
/// mis-decoded by others, so the magic numbers are worth the twelve lines.
pub(crate) fn media_type_of(b64: &str) -> &'static str {
    if b64.starts_with("iVBORw0KGgo") {
        "image/png"
    } else if b64.starts_with("R0lGOD") {
        "image/gif"
    } else if b64.starts_with("UklGR") {
        "image/webp"
    } else {
        // "/9j/" is JPEG, and it is also the sane default: it is what a photo
        // captured or pasted on any of these platforms actually is.
        "image/jpeg"
    }
}

/// The HTTP client every provider uses for chat.
///
/// Five minutes, because a large local model on a laptop genuinely takes that
/// long to answer, and a timeout that fires mid-generation looks to the user
/// exactly like a crash.
pub(crate) fn chat_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// The client for asking a provider *about itself*.
///
/// Listing models is not generating text, and it must not be given the
/// patience of something that is. `list_models` used `chat_client`, so a
/// provider that accepted the connection and then went quiet held the request
/// for five minutes — and the Messages screen awaits it before it will show
/// anything, so the whole app sat behind a spinner with "No chat selected" and
/// nothing clickable. A Web Inspector timeline of that state is empty: the main
/// thread is not busy, it is waiting.
///
/// This is the same lesson `probe_client` below already records for status
/// checks. The fix reached one of the two calls that needed it and not the
/// other, which is a shape this codebase has hit before.
///
/// Longer than a liveness probe, because a catalogue can be a few hundred
/// entries over a slow link, and short enough that a screen may wait for it.
pub(crate) fn catalogue_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default()
}

/// A short-tempered client for liveness checks.
///
/// Separate from `chat_client` on purpose: status is polled, and polling with
/// a five-minute timeout means every check against a machine where nothing is
/// listening hangs for five minutes.
pub(crate) fn probe_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(3))
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default()
}
#[cfg(test)]
mod usage_tests {
    use super::Usage;

    /// Silence is not zero. A provider that reports nothing must not be read as
    /// a turn that cost nothing — which is exactly what happened for as long as
    /// this was one unfilled field: every run on disk says `tokens: 0`, and the
    /// token ceiling has never stopped anything.
    #[test]
    fn a_provider_that_says_nothing_is_not_a_turn_that_cost_nothing() {
        let silent = Usage::default();
        assert!(silent.is_silent());
        assert_eq!(silent.charged(), None);

        let measured = Usage { input: Some(0), output: Some(0), ..Default::default() };
        assert!(!measured.is_silent());
        assert_eq!(measured.charged(), Some(0), "zero is a measurement");
    }

    /// The whole turn, not the reply. Input is most of what a turn costs here —
    /// the prompt, sixteen thousand characters of tool declarations, the
    /// conversation and every page read into it, re-sent on every iteration.
    #[test]
    fn what_a_turn_is_charged_is_both_halves() {
        let u = Usage { input: Some(9_000), output: Some(300), ..Default::default() };
        assert_eq!(u.charged(), Some(9_300));

        // Half a measurement still beats none: a provider that reports only one
        // side is counted for the side it reported.
        assert_eq!(Usage { output: Some(300), ..Default::default() }.charged(), Some(300));
        assert_eq!(Usage { input: Some(9_000), ..Default::default() }.charged(), Some(9_000));
    }

    /// Cached input is charged at a fraction of fresh input, and this app sends
    /// the same tool declarations every single turn — so the split is most of
    /// the bill, and a single number would hide it.
    #[test]
    fn the_cached_share_is_kept_apart_from_the_rest() {
        let u = Usage {
            input: Some(10_000),
            input_cached: Some(9_400),
            output: Some(200),
            output_hidden: Some(1_500),
        };

        // `charged` stays the total the provider counts: cached input is inside
        // `input`, not beside it, and adding it again would double-count.
        assert_eq!(u.charged(), Some(10_200));
        assert_eq!(u.input_cached, Some(9_400));
        assert_eq!(u.output_hidden, Some(1_500), "reasoning is billed and never shown");
    }

    /// A run written before any of this still reads.
    #[test]
    fn an_old_run_without_a_breakdown_still_parses() {
        let u: Usage = serde_json::from_str("{}").expect("an absent breakdown is silence");
        assert!(u.is_silent());
        assert_eq!(serde_json::to_string(&u).unwrap(), "{}", "and writes nothing back");
    }
}

