use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════
//  OLLAMA CONNECTION & MODELS
// ═══════════════════════════════════════════════════════════════

/// Status of the Ollama connection.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OllamaStatus {
    pub connected: bool,
    pub version: Option<String>,
    pub url: String,
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
    /// Source titles from RAG retrieval (only present on assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
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
    pub sources: Vec<String>,
}

/// RAG configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RagConfig {
    pub enabled: bool,
    pub max_context_chars: usize,
    pub include_finance: bool,
    pub include_feeds: bool,
    pub graph_expansion_depth: u8,
    pub personality: String,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_context_chars: 12000,
            include_finance: true,
            include_feeds: true,
            graph_expansion_depth: 1,
            personality: "auto".to_string(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  SETTINGS & CONFIGURATION
// ═══════════════════════════════════════════════════════════════

fn default_num_ctx() -> u32 { 8192 }
fn default_max_history() -> usize { 50 }

/// User-configurable settings for Syn, persisted in `{vault}/Syn/settings.json`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SynSettings {
    // Connection
    pub ollama_url: String,
    pub default_model: Option<String>,

    // Generation
    pub temperature: f64,
    pub max_tool_iterations: u8,
    /// Ollama context window size. Determines how much text the model can process at once.
    #[serde(default = "default_num_ctx")]
    pub num_ctx: u32,
    /// Maximum conversation history messages sent to LLM.
    #[serde(default = "default_max_history")]
    pub max_history_messages: usize,

    // RAG
    pub rag_enabled: bool,
    pub max_context_chars: usize,
    pub include_finance: bool,
    pub include_feeds: bool,
    pub graph_expansion_depth: u8,

    // Personality
    pub personality: String,
    pub custom_system_prompt: Option<String>,
}

impl Default for SynSettings {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".to_string(),
            default_model: None,
            temperature: 0.7,
            max_tool_iterations: 5,
            num_ctx: 8192,
            max_history_messages: 50,
            rag_enabled: true,
            max_context_chars: 12000,
            include_finance: true,
            include_feeds: true,
            graph_expansion_depth: 1,
            personality: "auto".to_string(),
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
