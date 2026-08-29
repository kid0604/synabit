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
//!
//! Streaming is a callback rather than a `Stream`, so the trait stays
//! object-safe and the caller keeps deciding what a token means — today that
//! is a Tauri event, and the provider does not need to know it.

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

/// What came back, whether it was streamed or not.
#[derive(Debug, Default)]
pub struct ChatReply {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    /// Tokens generated, when the provider reports it.
    pub tokens: Option<u64>,
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
