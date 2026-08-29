//! Ollama, speaking its own API.
//!
//! Lifted out of `engine.rs` unchanged in behaviour: the same endpoints, the
//! same newline-delimited JSON framing, the same tolerance for a chunk that
//! will not parse. What left with it is the assumption that Ollama is the only
//! thing Syn can talk to.
//!
//! Model management — pulling and deleting weights — is deliberately *not* on
//! `ChatProvider`. It is not a chat concern and no OpenAI-compatible server
//! has an equivalent; a trait method with a default that always errors would
//! only move the failure later. It stays here as an inherent method, and the
//! commands that offer it name Ollama when they call it.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::error::{AppError, AppResult};
use crate::models::syn::{
    ModelDetails, ModelInfo, ProviderStatus, SynProvider, SynPullProgress, ToolCall,
    ToolDefinition,
};
use crate::syn::provider::{
    chat_client, probe_client, ChatMessage, ChatProvider, ChatReply, ChatRequest, StreamSink,
};

/// Default Ollama base URL (local instance).
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Global flag for cancelling model pull operations.
static PULL_CANCEL: std::sync::LazyLock<AtomicBool> =
    std::sync::LazyLock::new(|| AtomicBool::new(false));

// ═══════════════════════════════════════════════════════════════
//  WIRE TYPES (internal, never exposed past this module)
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct OllamaVersionResponse {
    version: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(Deserialize)]
struct OllamaModelEntry {
    name: String,
    model: String,
    size: u64,
    digest: String,
    modified_at: String,
    details: Option<OllamaModelDetails>,
}

#[derive(Deserialize)]
struct OllamaModelDetails {
    format: Option<String>,
    family: Option<String>,
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

/// A single chunk from Ollama's streaming /api/chat response.
#[derive(Deserialize)]
struct OllamaChatChunk {
    message: Option<OllamaChatMessage>,
    done: bool,
    total_duration: Option<u64>,
    eval_count: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaChatMessage {
    #[serde(default)]
    content: String,
    /// Tool calls returned by the model (only present in non-streaming mode).
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize)]
struct OllamaPullChunk {
    status: String,
    completed: Option<u64>,
    total: Option<u64>,
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatRequestMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaChatOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
}

#[derive(Serialize, Clone)]
struct OllamaChatRequestMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    /// Base64-encoded images for vision/multimodal models.
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Serialize)]
struct OllamaChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    /// Context window size in tokens. Ollama defaults to 2048, which is too
    /// small for RAG + tool definitions + conversation history.
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

/// `tool_call_id` is dropped on the way in: Ollama has no field for it and
/// pairs a result with its call by position instead.
impl From<&ChatMessage> for OllamaChatRequestMessage {
    fn from(m: &ChatMessage) -> Self {
        Self {
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: m.tool_calls.clone(),
            images: m.images.clone(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  PROVIDER
// ═══════════════════════════════════════════════════════════════

pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: chat_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    fn body(&self, req: &ChatRequest<'_>, stream: bool) -> OllamaChatRequest {
        OllamaChatRequest {
            model: req.model.to_string(),
            messages: req.messages.iter().map(Into::into).collect(),
            stream,
            options: Some(OllamaChatOptions {
                temperature: req.temperature,
                num_ctx: Some(req.num_ctx),
            }),
            tools: req.tools.map(|t| t.to_vec()),
        }
    }

    /// Pull (download) a model via POST /api/pull with streaming progress events.
    pub async fn pull_model(&self, app: &tauri::AppHandle, model_name: &str) -> AppResult<()> {
        let url = format!("{}/api/pull", self.base_url);

        log::info!("Pulling model: {}", model_name);

        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({ "name": model_name, "stream": true }))
            .send()
            .await
            .map_err(|e| AppError::General(format!("Failed to start model pull: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::General(format!(
                "Ollama /api/pull returned status {}: {}",
                status, body
            )));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            if PULL_CANCEL.load(Ordering::SeqCst) {
                PULL_CANCEL.store(false, Ordering::SeqCst);
                log::info!("Model pull cancelled by user: {}", model_name);
                return Err(AppError::General("Model pull cancelled".into()));
            }
            let chunk = chunk_result
                .map_err(|e| AppError::General(format!("Stream error during model pull: {}", e)))?;

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Ok(pull_chunk) = serde_json::from_str::<OllamaPullChunk>(&line) {
                    let progress = SynPullProgress {
                        model: model_name.to_string(),
                        status: pull_chunk.status,
                        completed: pull_chunk.completed,
                        total: pull_chunk.total,
                    };

                    if let Err(e) = app.emit("syn-pull-progress", &progress) {
                        log::error!("Failed to emit pull progress event: {}", e);
                    }
                }
            }
        }

        log::info!("Model pull completed: {}", model_name);
        Ok(())
    }

    /// Delete a model via DELETE /api/delete.
    pub async fn delete_model(&self, model_name: &str) -> AppResult<()> {
        let url = format!("{}/api/delete", self.base_url);

        log::info!("Deleting model: {}", model_name);

        let resp = self
            .client
            .delete(&url)
            .json(&serde_json::json!({ "name": model_name }))
            .send()
            .await
            .map_err(|e| AppError::General(format!("Failed to delete model: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::General(format!(
                "Ollama /api/delete returned status {}: {}",
                status, body
            )));
        }

        log::info!("Model deleted: {}", model_name);
        Ok(())
    }

    /// Cancel an ongoing model pull operation.
    pub fn cancel_pull() {
        PULL_CANCEL.store(true, Ordering::SeqCst);
        log::info!("Pull cancel flag set");
    }
}

#[async_trait]
impl ChatProvider for OllamaProvider {
    fn id(&self) -> SynProvider {
        SynProvider::Ollama
    }

    async fn check_status(&self) -> AppResult<ProviderStatus> {
        let url = format!("{}/api/version", self.base_url);

        let offline = |version: Option<String>, connected: bool| ProviderStatus {
            connected,
            version,
            url: self.base_url.clone(),
            supports_model_management: true,
        };

        match probe_client().get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: OllamaVersionResponse = resp.json().await.map_err(|e| {
                    AppError::General(format!("Failed to parse Ollama version response: {}", e))
                })?;
                Ok(offline(Some(body.version), true))
            }
            Ok(resp) => {
                log::warn!("Ollama responded with status {}", resp.status());
                Ok(offline(None, false))
            }
            Err(e) => {
                log::info!("Ollama not reachable: {}", e);
                Ok(offline(None, false))
            }
        }
    }

    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.base_url);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::General(format!("Failed to connect to Ollama: {}", e)))?;

        if !resp.status().is_success() {
            return Err(AppError::General(format!(
                "Ollama /api/tags returned status {}",
                resp.status()
            )));
        }

        let body: OllamaTagsResponse = resp.json().await.map_err(|e| {
            AppError::General(format!("Failed to parse Ollama tags response: {}", e))
        })?;

        Ok(body
            .models
            .into_iter()
            .map(|m| ModelInfo {
                name: m.name,
                model: m.model,
                size: m.size,
                digest: m.digest,
                modified_at: m.modified_at,
                details: m.details.map(|d| ModelDetails {
                    format: d.format,
                    family: d.family,
                    parameter_size: d.parameter_size,
                    quantization_level: d.quantization_level,
                }),
            })
            .collect())
    }

    async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply> {
        let url = format!("{}/api/chat", self.base_url);

        log::info!(
            "[Syn] Non-streaming call to Ollama with {} messages",
            req.messages.len()
        );

        let resp = self
            .client
            .post(&url)
            .json(&self.body(&req, false))
            .send()
            .await
            .map_err(|e| {
                AppError::General(format!("Failed to connect to Ollama for tool call: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::General(format!(
                "Ollama /api/chat (non-streaming) returned status {}: {}",
                status, body
            )));
        }

        let chunk: OllamaChatChunk = resp.json().await.map_err(|e| {
            AppError::General(format!(
                "Failed to parse Ollama non-streaming response: {}",
                e
            ))
        })?;

        let msg = chunk.message;
        Ok(ChatReply {
            content: msg.as_ref().map(|m| m.content.clone()).unwrap_or_default(),
            tool_calls: msg.and_then(|m| m.tool_calls).unwrap_or_default(),
            tokens: chunk.eval_count,
            // Filtered rather than mapped: Ollama sometimes reports a
            // `total_duration` of zero, and `Some(0)` would be shown to the
            // user as a reply that took no time. `None` lets the caller fall
            // back to the wall clock, which is what it did before.
            duration_ms: chunk.total_duration.filter(|ns| *ns > 0).map(|ns| ns / 1_000_000),
        })
    }

    async fn chat_streaming(
        &self,
        req: ChatRequest<'_>,
        sink: &StreamSink<'_>,
    ) -> AppResult<ChatReply> {
        let url = format!("{}/api/chat", self.base_url);

        let resp = self
            .client
            .post(&url)
            .json(&self.body(&req, true))
            .send()
            .await
            .map_err(|e| {
                AppError::General(format!("Failed to connect to Ollama for chat: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::General(format!(
                "Ollama /api/chat returned status {}: {}",
                status, body
            )));
        }

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut reply = ChatReply::default();

        while let Some(chunk_result) = stream.next().await {
            if (sink.stop_requested)() {
                break;
            }

            let chunk = chunk_result
                .map_err(|e| AppError::General(format!("Stream error during chat: {}", e)))?;

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Ollama frames its stream as newline-delimited JSON.
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<OllamaChatChunk>(&line) {
                    Ok(chat_chunk) => {
                        if let Some(ref msg) = chat_chunk.message {
                            if !msg.content.is_empty() {
                                reply.content.push_str(&msg.content);
                                (sink.on_token)(&msg.content);
                            }
                        }

                        if chat_chunk.done {
                            reply.tokens = chat_chunk.eval_count;
                            reply.duration_ms = chat_chunk
                                .total_duration
                                .filter(|ns| *ns > 0)
                                .map(|ns| ns / 1_000_000);
                        }
                    }
                    // A chunk that will not parse is skipped rather than
                    // fatal: losing a token is recoverable, and abandoning a
                    // half-written answer is not.
                    Err(e) => {
                        log::warn!("Failed to parse chat chunk: {} — raw: {}", e, line);
                    }
                }
            }
        }

        Ok(reply)
    }
}
