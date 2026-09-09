use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════
//  OLLAMA CONNECTION & MODELS
// ═══════════════════════════════════════════════════════════════

/// Whether the configured provider answered, and what it said about itself.
///
/// Named for Ollama until Syn could only talk to Ollama. The fields were
/// already neutral; only `version` is Ollama-shaped, and a provider that does
/// not announce one leaves it `None`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProviderStatus {
    pub connected: bool,
    pub version: Option<String>,
    pub url: String,
    /// Whether this provider can pull and delete models on the user's behalf.
    ///
    /// True for Ollama, which hosts the weights. False for anything reached
    /// over an OpenAI-compatible API, where the model catalogue is the
    /// server's business — the UI must hide those buttons rather than offer
    /// them and fail.
    #[serde(default)]
    pub supports_model_management: bool,
}

/// Model info returned by Ollama's /api/tags endpoint.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub model: String,
    pub size: u64,
    pub digest: String,
    pub modified_at: String,
    pub details: Option<ModelDetails>,
}

/// Detailed model metadata from Ollama.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

// ═══════════════════════════════════════════════════════════════
//  CONVERSATIONS & MESSAGES
// ═══════════════════════════════════════════════════════════════

/// A single message in a Syn conversation.
/// A source reference from RAG retrieval, used for navigation in the frontend.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SourceRef {
    pub id: String,
    pub title: String,
    pub node_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynMessage {
    pub id: String,
    /// Role: "user", "assistant", or "system"
    pub role: String,
    pub content: String,
    pub model: Option<String>,
    pub timestamp: String,
    pub tokens: Option<u64>,
    pub duration_ms: Option<u64>,
    /// Source references from RAG retrieval (only present on assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<SourceRef>>,
    /// What this answer was standing on, decided from the run's transcript.
    ///
    /// On the message rather than only on the run because it has to survive
    /// reopening the conversation: a mark that appears while the answer streams
    /// and is gone tomorrow is worse than no mark, since the reader learns to
    /// disregard it. See `syn::footing`.
    ///
    /// `None` on user messages, and on every assistant message written before
    /// this existed — which is why it is an `Option` rather than defaulting to
    /// `Guessing`. "Nobody measured this one" and "this one was a guess" are
    /// different claims, and only the first is true of an old message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footing: Option<crate::syn::footing::Footing>,
    /// Tool calls made during this message (for display in frontend)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls_log: Option<Vec<SynToolCallEvent>>,
    /// Base64-encoded images attached to this message (for multimodal models)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

/// Conversation metadata (used for listing without loading all messages).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynConversation {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    /// Which provider `model` is a name for.
    ///
    /// A model name means nothing on its own: `gemma4:e4b` is a real model on
    /// Ollama and a 404 on OpenAI. A conversation started under one provider
    /// pins its model, and that pin outranks the default in settings — so
    /// without this, switching provider left every existing conversation
    /// sending a name the new endpoint had never heard of.
    ///
    /// `None` on conversations written before providers existed, and those are
    /// Ollama conversations.
    #[serde(default)]
    pub provider: Option<SynProvider>,
    pub message_count: usize,
    pub created_at: String,
    pub updated_at: String,
    pub pinned: bool,
}

/// Full conversation including messages (used for loading a single conversation).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynConversationFull {
    pub meta: SynConversation,
    pub messages: Vec<SynMessage>,
}

// ═══════════════════════════════════════════════════════════════
//  STREAMING & IPC EVENTS
// ═══════════════════════════════════════════════════════════════

/// Streaming token event payload — emitted via Tauri events during generation.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynStreamToken {
    pub conversation_id: String,
    pub message_id: String,
    pub token: String,
    pub done: bool,
}

/// Chat request sent from the frontend.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynChatRequest {
    pub conversation_id: String,
    pub message: String,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    /// Base64-encoded images to send with the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
    /// What the user was looking at when they asked.
    ///
    /// Part of the request rather than of the conversation, because it is true
    /// of one message and not of the next: the screen has moved on by the time
    /// the answer arrives. `None` from any caller that has no screen. See
    /// `syn::focus`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<crate::syn::focus::Focus>,
    /// The run that stopped for permission, now that it has an answer.
    ///
    /// Carrying on rather than asking a new question: `message` is empty,
    /// nobody typed anything, and the thing they want answered is still the
    /// last thing they said.
    ///
    /// The run's **id** rather than a flag, because the run holds the call it
    /// was about to make. Without that the resumed run starts from the user's
    /// message and works it out again — which searched DuckDuckGo a second time
    /// after the user had just granted permission to read a particular site.
    /// See `run::Run::pending_call`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_run: Option<String>,
}

/// Pull model progress event — emitted while downloading a model.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynPullProgress {
    pub model: String,
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
}

// ═══════════════════════════════════════════════════════════════
//  RAG (Retrieval-Augmented Generation)
// ═══════════════════════════════════════════════════════════════

/// A chunk of vault context retrieved for RAG.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContextChunk {
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub content: String,
    pub relevance_score: f64,
    pub metadata: Option<String>,
}

/// Result of the RAG retrieval pipeline.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RetrievalResult {
    pub context_chunks: Vec<ContextChunk>,
    pub total_tokens_estimate: usize,
    pub sources: Vec<SourceRef>,
}

/// RAG configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RagConfig {
    pub enabled: bool,
    pub max_context_chars: usize,
    pub include_finance: bool,
    pub include_feeds: bool,
    pub graph_expansion_depth: u8,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_context_chars: 12000,
            include_finance: true,
            include_feeds: true,
            graph_expansion_depth: 1,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  SETTINGS & CONFIGURATION
// ═══════════════════════════════════════════════════════════════

fn default_num_ctx() -> u32 {
    8192
}
fn default_max_history() -> usize {
    50
}
fn default_max_tool_iterations() -> u8 {
    12
}
fn default_memory_reflection() -> bool {
    true
}

/// On, for a fresh vault and for every vault written before the switch
/// existed. Somebody who installed a productivity app with an assistant in
/// it did not install it to find the assistant switched off.
fn default_enabled() -> bool {
    true
}

/// Which service Syn talks to.
///
/// Ollama is the default and stays the default. The second arm is not "the
/// cloud" — it is the OpenAI *request shape*, which is what OpenAI, OpenRouter,
/// Groq, vLLM, LM Studio and llama.cpp's own server all speak. Pointing it at
/// `http://localhost:8080/v1` is as valid as pointing it at api.openai.com, and
/// the local case needs no key.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SynProvider {
    #[default]
    Ollama,
    OpenAiCompat,
}

impl SynProvider {
    /// The stable string this provider's API key is filed under.
    pub fn key_slot(&self) -> &'static str {
        match self {
            SynProvider::Ollama => "ollama",
            SynProvider::OpenAiCompat => "openai_compat",
        }
    }
}

fn default_provider() -> SynProvider {
    SynProvider::Ollama
}

fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

/// User-configurable settings for Syn, persisted in `{vault}/Syn/settings.json`.
///
/// **This file lives inside the vault.** It syncs between devices, it is
/// readable in any editor, and on a vault kept in git it is committed. Nothing
/// secret may be added to this struct — an API key goes to `secrets.rs`, which
/// writes to the OS keychain and stays on the one machine.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynSettings {
    /// The switch that turns Syn off.
    ///
    /// # Why there has to be one
    ///
    /// Syn's surface has grown: threads live inside the Syn app, the ask bar
    /// opens over every other mini-app with a keystroke, and reflection looks
    /// at every exchange without being asked. None of that had an off position
    /// — the closest thing was disconnecting the model provider, which is not
    /// the same statement and leaves the app in an error state rather than a
    /// chosen one.
    ///
    /// # What it turns off, and what it deliberately does not
    ///
    /// Off means: no message is sent, no run is driven, no reflection looks
    /// back at anything, no memory reaches a prompt, and the ask bar does not
    /// open anywhere.
    ///
    /// It does **not** touch the app's own reminders. `chat_engine` sends
    /// "this task is overdue" as *Synabit System*, not as Syn — those are the
    /// calendar doing its job, and switching Syn off must not take them with
    /// it. The whole point of the switch is that everything which is not Syn
    /// keeps working; a switch that quietly stopped the reminders would be
    /// proof that Syn had grown into places it does not belong.
    ///
    /// Threads keep existing too, because they are ordinary vault nodes and
    /// were always readable in Things. Turning off the colleague does not
    /// delete the work.
    ///
    /// Absent in settings files written before this existed, and those vaults
    /// are ones where Syn was on.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    // Connection
    /// Which service to talk to. Absent in files written before providers
    /// existed, and those vaults are Ollama vaults.
    #[serde(default = "default_provider")]
    pub provider: SynProvider,
    pub ollama_url: String,
    /// Base URL for the OpenAI-compatible provider, including the version
    /// segment — the path `/chat/completions` is appended to it verbatim.
    #[serde(default = "default_openai_base_url")]
    pub openai_base_url: String,
    /// `reasoning_effort` to send, or `None` to send the field at all.
    ///
    /// Absent by default, and that has to stay the default: most servers
    /// speaking this API have never heard of the field and reject a request
    /// carrying it.
    ///
    /// It exists because of the opposite failure. A reasoning model reached
    /// through `/chat/completions` applies its own default effort, and several
    /// of them refuse function tools while it is set:
    ///
    /// > Function tools with reasoning_effort are not supported for
    /// > gpt-5.6-luna in /v1/chat/completions. To use function tools, use
    /// > /v1/responses or set reasoning_effort to 'none'.
    ///
    /// Syn always sends tools, so on those models this must be `none` — which
    /// buys tool calling at the cost of the reasoning. The real answer is the
    /// `/v1/responses` API, which is a different request shape and a separate
    /// piece of work.
    #[serde(default)]
    pub openai_reasoning_effort: Option<String>,
    pub default_model: Option<String>,

    // Generation
    pub temperature: f64,
    /// How many rounds of "ask, run tools, ask again" one message may take.
    ///
    /// Was 5, chosen when Syn could only talk to a local model over an 8K
    /// window, where each round is a full inference on the user's own CPU and
    /// five of them is already a long wait. Against a hosted model a round is
    /// a second or two, and 5 binds: a single observed investigation — find
    /// the overdue tasks in a project, read each one, write a note — used six
    /// calls, and the eval harness saw eight.
    ///
    /// Raised to twelve rather than removed. There has to be a ceiling: it is
    /// what stops a model that has decided to search forever from spending
    /// someone's money doing it. Streaming makes the wait legible now — the
    /// text arrives as it is written — so a longer ceiling costs patience
    /// rather than silence.
    ///
    /// Anyone on slow local hardware should lower it; the control is in Syn
    /// settings and accepts 1 to 20.
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: u8,
    /// Ollama context window size. Determines how much text the model can process at once.
    #[serde(default = "default_num_ctx")]
    pub num_ctx: u32,
    /// Maximum conversation history messages sent to LLM.
    #[serde(default = "default_max_history")]
    pub max_history_messages: usize,
    /// How much of a web page reaches the model at once, in characters.
    ///
    /// `None` means "let the provider decide", which is not the same as any
    /// particular number: a model on somebody's laptop and a hosted one want
    /// genuinely different answers. See `syn::web::page_chars`.
    ///
    /// It is a slice rather than a ceiling — a page longer than this arrives
    /// with an outline of what is past the cut and a way to read on.
    #[serde(default)]
    pub max_page_chars: Option<u32>,

    // RAG
    pub rag_enabled: bool,
    pub max_context_chars: usize,
    pub include_finance: bool,
    pub include_feeds: bool,
    pub graph_expansion_depth: u8,

    // Memory
    /// Whether Syn looks back at each exchange and proposes what to remember.
    ///
    /// On by default, because a memory nobody has to ask for is the point. It
    /// costs one extra completion per answered message — no tools, a small
    /// prompt — which is roughly a fifth of a main turn, and it is the sort of
    /// cost somebody should be able to switch off rather than discover.
    ///
    /// Absent in settings files written before this existed, and those vaults
    /// get it on, like a fresh one.
    #[serde(default = "default_memory_reflection")]
    pub memory_reflection: bool,

    /// Where Syn searches the web, when it can.
    ///
    /// The user's own endpoint — a SearXNG they run, or a paid API they have a
    /// key for. Nothing is bundled and nothing is scraped: parsing a search
    /// engine's HTML behind its back breaks on their next redesign and is not
    /// this app's to do.
    ///
    /// `None` means Syn has no search, and the tool is not offered at all —
    /// rather than offered and failing, which would spend tokens every turn
    /// describing something that cannot work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_url: Option<String>,

    // Personality
    /// Kept only so an existing settings file still deserialises, and so
    /// `instructions::migrate_personality` can carry a chosen voice into
    /// `SYN.md` once before it goes.
    ///
    /// Nothing reads it into a prompt any more. It picked one of three voices,
    /// two of which hard-coded Vietnamese and a pronoun pair on the user's
    /// behalf; the third was the rule that makes a bilingual app work, and that
    /// one now lives unconditionally in `prompt::IDENTITY`. How Syn talks to
    /// somebody belongs in their own words in `SYN.md`, where they can say
    /// anything rather than one of three things.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    pub custom_system_prompt: Option<String>,
}

impl Default for SynSettings {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            provider: SynProvider::Ollama,
            ollama_url: "http://localhost:11434".to_string(),
            openai_base_url: default_openai_base_url(),
            openai_reasoning_effort: None,
            default_model: None,
            temperature: 0.7,
            max_tool_iterations: default_max_tool_iterations(),
            num_ctx: 8192,
            max_page_chars: None,
            max_history_messages: 50,
            rag_enabled: true,
            max_context_chars: 12000,
            include_finance: true,
            include_feeds: true,
            graph_expansion_depth: 1,
            memory_reflection: default_memory_reflection(),
            search_url: None,
            personality: None,
            custom_system_prompt: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  FUNCTION CALLING / TOOL USE
// ═══════════════════════════════════════════════════════════════

/// Ollama tool definition (sent in chat request).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

/// Function metadata within a tool definition.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call made by the LLM in its response.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCall {
    /// The call's id, when the provider issues one.
    ///
    /// Ollama does not: it matches tool results to calls by position, and a
    /// `tool` message carries only a result. The OpenAI shape requires the
    /// opposite — every call has an id, and the result must name it in
    /// `tool_call_id` or the request is rejected. Skipped when absent so an
    /// assistant message echoed back to Ollama looks exactly as it did before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub function: ToolCallFunction,
}

/// The function name and arguments within a tool call.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Event emitted to frontend when Syn calls a tool.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynToolCallEvent {
    pub conversation_id: String,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub result_preview: String,
    pub iteration: u8,
}
