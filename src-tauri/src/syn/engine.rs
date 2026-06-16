//! Ollama HTTP client for the Syn (Local AI Chat) feature.
//!
//! Handles all communication with the Ollama REST API:
//! - Status checks, model listing, model pull/delete
//! - Streaming chat completions with token-by-token event emission

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::error::{AppError, AppResult};
use crate::models::syn::{
    ModelDetails, ModelInfo, OllamaStatus, SynMessage, SynPullProgress, SynStreamToken,
    SynToolCallEvent, ToolCall, ToolDefinition,
};

/// Default Ollama base URL (local instance).
const OLLAMA_BASE_URL: &str = "http://localhost:11434";

/// Per-conversation stop flags. Each active streaming conversation gets its own AtomicBool.
static STOP_FLAGS: std::sync::LazyLock<RwLock<HashMap<String, Arc<AtomicBool>>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Global flag for cancelling model pull operations.
static PULL_CANCEL: std::sync::LazyLock<AtomicBool> =
    std::sync::LazyLock::new(|| AtomicBool::new(false));

/// Get or create a stop flag for a conversation.
fn get_stop_flag(conversation_id: &str) -> Arc<AtomicBool> {
    {
        let flags = STOP_FLAGS.read().unwrap();
        if let Some(flag) = flags.get(conversation_id) {
            return Arc::clone(flag);
        }
    }
    let flag = Arc::new(AtomicBool::new(false));
    let mut flags = STOP_FLAGS.write().unwrap();
    flags.insert(conversation_id.to_string(), Arc::clone(&flag));
    flag
}

/// Remove the stop flag for a completed conversation.
fn remove_stop_flag(conversation_id: &str) {
    let mut flags = STOP_FLAGS.write().unwrap();
    flags.remove(conversation_id);
}

// ═══════════════════════════════════════════════════════════════
//  OLLAMA API RESPONSE TYPES (internal, not exposed to frontend)
// ═══════════════════════════════════════════════════════════════

/// Response from GET /api/version
#[derive(Deserialize)]
struct OllamaVersionResponse {
    version: String,
}

/// Response from GET /api/tags
#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelEntry>,
}

/// A single model entry from /api/tags
#[derive(Deserialize)]
struct OllamaModelEntry {
    name: String,
    model: String,
    size: u64,
    digest: String,
    modified_at: String,
    details: Option<OllamaModelDetails>,
}

/// Model details from /api/tags
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

/// Message payload inside a streaming chunk.
#[derive(Deserialize)]
struct OllamaChatMessage {
    #[serde(default)]
    content: String,
    /// Tool calls returned by the model (only present in non-streaming mode).
    tool_calls: Option<Vec<ToolCall>>,
}

/// A single chunk from Ollama's streaming /api/pull response.
#[derive(Deserialize)]
struct OllamaPullChunk {
    status: String,
    completed: Option<u64>,
    total: Option<u64>,
}

/// Request body for /api/chat
#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatRequestMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaChatOptions>,
    /// Tool definitions sent to Ollama for function calling.
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
}

/// A message in the /api/chat request body.
#[derive(Serialize, Clone)]
struct OllamaChatRequestMessage {
    role: String,
    content: String,
    /// Tool calls made by the assistant (echoed back to Ollama).
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    /// Base64-encoded images for vision/multimodal models.
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

/// Options for /api/chat (temperature, context window, etc.)
#[derive(Serialize)]
struct OllamaChatOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    /// Context window size in tokens. Ollama defaults to 2048, which is too small
    /// for RAG + tool definitions + conversation history.
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

// ═══════════════════════════════════════════════════════════════
//  SYN ENGINE
// ═══════════════════════════════════════════════════════════════

fn build_pruned_history(history: &[SynMessage], max_msgs: usize) -> Vec<OllamaChatRequestMessage> {
    let mut messages = Vec::new();
    if history.len() > max_msgs {
        let has_system = history.first().map(|m| m.role == "system").unwrap_or(false);
        if has_system {
            messages.push(OllamaChatRequestMessage {
                role: history[0].role.clone(),
                content: history[0].content.clone(),
                tool_calls: None,
                images: history[0].images.clone(),
            });
        }
        
        let skip_count = history.len() - max_msgs;
        let start_idx = if has_system { std::cmp::max(1, skip_count) } else { skip_count };
        
        messages.extend(history[start_idx..].iter().map(|m| OllamaChatRequestMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: None,
            images: m.images.clone(),
        }));
    } else {
        messages = history.iter().map(|m| OllamaChatRequestMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            tool_calls: None,
            images: m.images.clone(),
        }).collect();
    }
    messages
}

/// The Syn engine — a stateless HTTP client wrapper around Ollama.
pub struct SynEngine {
    client: reqwest::Client,
    base_url: String,
}

impl SynEngine {
    /// Create a new engine pointing at the default Ollama URL.
    pub fn new() -> Self {
        Self::with_url(OLLAMA_BASE_URL)
    }

    /// Create a new engine with a custom Ollama URL.
    pub fn with_url(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            base_url: base_url.to_string(),
        }
    }

    // ───────────────────────────────────────────────────────────
    //  Status & Model Management
    // ───────────────────────────────────────────────────────────

    /// Check Ollama connectivity by hitting GET /api/version.
    pub async fn check_status(&self) -> AppResult<OllamaStatus> {
        let url = format!("{}/api/version", self.base_url);

        // Use a lightweight client with short timeouts for status checks.
        // The main self.client has a 300s timeout (needed for streaming chat),
        // which would cause polling to hang when Ollama isn't running.
        let status_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        match status_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: OllamaVersionResponse = resp.json().await.map_err(|e| {
                    AppError::General(format!("Failed to parse Ollama version response: {}", e))
                })?;
                Ok(OllamaStatus {
                    connected: true,
                    version: Some(body.version),
                    url: self.base_url.clone(),
                })
            }
            Ok(resp) => {
                log::warn!("Ollama responded with status {}", resp.status());
                Ok(OllamaStatus {
                    connected: false,
                    version: None,
                    url: self.base_url.clone(),
                })
            }
            Err(e) => {
                log::info!("Ollama not reachable: {}", e);
                Ok(OllamaStatus {
                    connected: false,
                    version: None,
                    url: self.base_url.clone(),
                })
            }
        }
    }

    /// List all locally available models via GET /api/tags.
    pub async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.base_url);

        let resp = self.client.get(&url).send().await.map_err(|e| {
            AppError::General(format!("Failed to connect to Ollama: {}", e))
        })?;

        if !resp.status().is_success() {
            return Err(AppError::General(format!(
                "Ollama /api/tags returned status {}",
                resp.status()
            )));
        }

        let body: OllamaTagsResponse = resp.json().await.map_err(|e| {
            AppError::General(format!("Failed to parse Ollama tags response: {}", e))
        })?;

        let models = body
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
            .collect();

        Ok(models)
    }

    /// Pull (download) a model via POST /api/pull with streaming progress events.
    pub async fn pull_model(
        &self,
        app: &tauri::AppHandle,
        model_name: &str,
    ) -> AppResult<()> {
        let url = format!("{}/api/pull", self.base_url);

        log::info!("Pulling model: {}", model_name);

        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "name": model_name,
                "stream": true
            }))
            .send()
            .await
            .map_err(|e| {
                AppError::General(format!("Failed to start model pull: {}", e))
            })?;

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
            // Check cancel flag
            if PULL_CANCEL.load(Ordering::SeqCst) {
                PULL_CANCEL.store(false, Ordering::SeqCst);
                log::info!("Model pull cancelled by user: {}", model_name);
                return Err(AppError::General("Model pull cancelled".into()));
            }
            let chunk = chunk_result.map_err(|e| {
                AppError::General(format!("Stream error during model pull: {}", e))
            })?;

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Parse newline-delimited JSON chunks
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
            .map_err(|e| {
                AppError::General(format!("Failed to delete model: {}", e))
            })?;

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

    // ───────────────────────────────────────────────────────────
    //  Streaming Chat Completion
    // ───────────────────────────────────────────────────────────

    /// Send a message to Ollama and stream the response token-by-token.
    ///
    /// - Builds the messages array from the conversation history.
    /// - Streams response via Ollama's newline-delimited JSON.
    /// - Emits `syn-stream-token` Tauri event for each token.
    /// - Checks `STOP_FLAG` between chunks to support cancellation.
    /// - Returns the complete assistant `SynMessage` when done.
    pub async fn send_message(
        &self,
        app: &tauri::AppHandle,
        conversation_id: &str,
        message_id: &str,
        history: &[SynMessage],
        model: &str,
        temperature: Option<f64>,
        num_ctx: u32,
        max_history: usize,
    ) -> AppResult<SynMessage> {
        let url = format!("{}/api/chat", self.base_url);

        // Reset the stop flag before starting
        let stop_flag = get_stop_flag(conversation_id);
        stop_flag.store(false, Ordering::SeqCst);

        // Build the messages array from conversation history (limit to max 50 messages)
        let messages = build_pruned_history(history, max_history);

        let request_body = OllamaChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
            options: Some(OllamaChatOptions { temperature, num_ctx: Some(num_ctx) }),
            tools: None,
        };

        let resp = self
            .client
            .post(&url)
            .json(&request_body)
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

        let start_time = std::time::Instant::now();
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut full_response = String::new();
        let mut total_tokens: u64 = 0;
        let mut total_duration_ns: u64 = 0;

        while let Some(chunk_result) = stream.next().await {
            // Check if generation was cancelled
            if stop_flag.load(Ordering::SeqCst) {
                log::info!("Generation stopped by user for conversation {}", conversation_id);

                // Emit a final "done" event so the frontend knows streaming ended
                let stop_event = SynStreamToken {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    token: String::new(),
                    done: true,
                };
                let _ = app.emit("syn-stream-token", &stop_event);
                break;
            }

            let chunk = chunk_result.map_err(|e| {
                AppError::General(format!("Stream error during chat: {}", e))
            })?;

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Parse newline-delimited JSON chunks from Ollama
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<OllamaChatChunk>(&line) {
                    Ok(chat_chunk) => {
                        // Extract token content
                        if let Some(ref msg) = chat_chunk.message {
                            if !msg.content.is_empty() {
                                full_response.push_str(&msg.content);

                                // Emit streaming token event to the frontend
                                let token_event = SynStreamToken {
                                    conversation_id: conversation_id.to_string(),
                                    message_id: message_id.to_string(),
                                    token: msg.content.clone(),
                                    done: false,
                                };
                                if let Err(e) = app.emit("syn-stream-token", &token_event) {
                                    log::error!("Failed to emit stream token event: {}", e);
                                }
                            }
                        }

                        // Capture final statistics when done
                        if chat_chunk.done {
                            if let Some(eval_count) = chat_chunk.eval_count {
                                total_tokens = eval_count;
                            }
                            if let Some(duration) = chat_chunk.total_duration {
                                total_duration_ns = duration;
                            }

                            // Emit final done event
                            let done_event = SynStreamToken {
                                conversation_id: conversation_id.to_string(),
                                message_id: message_id.to_string(),
                                token: String::new(),
                                done: true,
                            };
                            if let Err(e) = app.emit("syn-stream-token", &done_event) {
                                log::error!("Failed to emit stream done event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse chat chunk: {} — raw: {}", e, line);
                    }
                }
            }
        }

        remove_stop_flag(conversation_id);

        let elapsed_ms = start_time.elapsed().as_millis() as u64;

        // Use Ollama's reported duration if available, otherwise wall-clock time
        let duration_ms = if total_duration_ns > 0 {
            total_duration_ns / 1_000_000
        } else {
            elapsed_ms
        };

        let assistant_message = SynMessage {
            id: message_id.to_string(),
            role: "assistant".to_string(),
            content: full_response,
            model: Some(model.to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            tokens: if total_tokens > 0 { Some(total_tokens) } else { None },
            duration_ms: Some(duration_ms),
            sources: None,
            tool_calls_log: None,
            images: None,
        };

        Ok(assistant_message)
    }

    /// Stop generation for a specific conversation (or all if id is None).
    pub fn stop_generation(conversation_id: Option<&str>) {
        match conversation_id {
            Some(id) => {
                if let Ok(flags) = STOP_FLAGS.read() {
                    if let Some(flag) = flags.get(id) {
                        flag.store(true, Ordering::SeqCst);
                        log::info!("Stop generation flag set for conversation {}", id);
                    }
                }
            }
            None => {
                // Backward-compatible: stop all
                if let Ok(flags) = STOP_FLAGS.read() {
                    for (id, flag) in flags.iter() {
                        flag.store(true, Ordering::SeqCst);
                        log::info!("Stop generation flag set for conversation {}", id);
                    }
                }
            }
        }
    }

    /// Cancel an ongoing model pull operation.
    pub fn cancel_pull() {
        PULL_CANCEL.store(true, Ordering::SeqCst);
        log::info!("Pull cancel flag set");
    }

    // ───────────────────────────────────────────────────────────
    //  Function Calling / Tool Use (Phase 3)
    // ───────────────────────────────────────────────────────────

    /// Send a message with tool calling support.
    ///
    /// Implements the multi-turn tool calling loop:
    /// 1. Call Ollama non-streaming with tool definitions
    /// 2. If model returns tool_calls, execute them against the DB
    /// 3. Append tool results and call again
    /// 4. When model responds with text (no tool calls), stream the final response
    ///
    /// The `db_state` is the `Mutex<DbBridge>` — locked only during tool execution
    /// (fast, <10ms each), not during Ollama HTTP calls.
    pub async fn send_message_with_tools(
        &self,
        app: &tauri::AppHandle,
        conversation_id: &str,
        message_id: &str,
        history: &[SynMessage],
        model: &str,
        temperature: Option<f64>,
        tools: &[ToolDefinition],
        db_state: &std::sync::Mutex<crate::db::DbBridge>,
        vault_path: &str,
        max_iterations: u8,
        num_ctx: u32,
        max_history: usize,
    ) -> AppResult<SynMessage> {
        // Convert SynMessage history into internal request messages
        let mut working_messages = build_pruned_history(history, max_history);

        let mut tool_call_log: Vec<SynToolCallEvent> = Vec::new();

        for iteration in 0..max_iterations {
            log::info!(
                "[Syn Tools] Iteration {}/{} for conversation {}",
                iteration + 1,
                max_iterations,
                conversation_id
            );

            // 1. Call Ollama non-streaming with tools
            let response = self
                .call_non_streaming(&working_messages, model, temperature, Some(tools), num_ctx)
                .await?;

            let msg = match response.message {
                Some(m) => m,
                None => {
                    log::warn!("[Syn Tools] No message in Ollama response, breaking");
                    break;
                }
            };

            // 2. Check if the model made tool calls
            if let Some(ref model_tool_calls) = msg.tool_calls {
                if model_tool_calls.is_empty() {
                    // Empty tool_calls array — model is done, proceed to streaming
                    log::info!("[Syn Tools] Empty tool_calls, streaming final response");
                    break;
                }

                log::info!(
                    "[Syn Tools] Model made {} tool call(s)",
                    model_tool_calls.len()
                );

                // Append the assistant message that contains the tool calls
                working_messages.push(OllamaChatRequestMessage {
                    role: "assistant".to_string(),
                    content: msg.content.clone(),
                    tool_calls: Some(model_tool_calls.clone()),
                    images: None,
                });

                // Execute each tool call
                for tc in model_tool_calls {
                    // Lock DB only during tool execution (fast path)
                    let result = {
                        let db = db_state.lock().map_err(|e| {
                            AppError::General(format!("DB lock error during tool call: {}", e))
                        })?;
                        let ctx = crate::syn::tools::ToolContext {
                            db: &db,
                            vault_path,
                            app,
                        };
                        crate::syn::tools::execute_tool(
                            &ctx,
                            &tc.function.name,
                            &tc.function.arguments,
                        )
                        .unwrap_or_else(|e| {
                            serde_json::json!({"error": format!("{}", e)}).to_string()
                        })
                    }; // DB lock dropped here

                    // Emit event to frontend for live tool call display
                    let event = SynToolCallEvent {
                        conversation_id: conversation_id.to_string(),
                        tool_name: tc.function.name.clone(),
                        tool_args: tc.function.arguments.clone(),
                        result_preview: result.chars().take(4000).collect(),
                        iteration,
                    };
                    if let Err(e) = app.emit("syn-tool-call", &event) {
                        log::error!("Failed to emit syn-tool-call event: {}", e);
                    }
                    tool_call_log.push(event);

                    // Append the tool result message
                    working_messages.push(OllamaChatRequestMessage {
                        role: "tool".to_string(),
                        content: result,
                        tool_calls: None,
                        images: None,
                    });
                }

                // Continue loop — let model process tool results
                continue;
            }

            // 3. No tool calls — model wants to respond with text.
            //    We already have the non-streamed text, but for better UX we
            //    re-stream it. However, if the response already has content,
            //    we can just use it directly to avoid a redundant call.
            if !msg.content.is_empty() {
                log::info!(
                    "[Syn Tools] Final text response received ({} chars), using directly",
                    msg.content.len()
                );

                // Emit the content as streaming tokens for frontend consistency
                let token_event = SynStreamToken {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    token: msg.content.clone(),
                    done: false,
                };
                let _ = app.emit("syn-stream-token", &token_event);

                let done_event = SynStreamToken {
                    conversation_id: conversation_id.to_string(),
                    message_id: message_id.to_string(),
                    token: String::new(),
                    done: true,
                };
                let _ = app.emit("syn-stream-token", &done_event);

                let mut final_msg = SynMessage {
                    id: message_id.to_string(),
                    role: "assistant".to_string(),
                    content: msg.content,
                    model: Some(model.to_string()),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    tokens: response.eval_count,
                    duration_ms: response.total_duration.map(|ns| ns / 1_000_000),
                    sources: None,
                    tool_calls_log: None,
                    images: None,
                };

                if !tool_call_log.is_empty() {
                    final_msg.tool_calls_log = Some(tool_call_log);
                }

                return Ok(final_msg);
            }

            // Empty content and no tool calls — unusual, break and fall through
            log::warn!("[Syn Tools] Response has no content and no tool calls");
            break;
        }

        // Max iterations reached or early break — stream a final response without tools
        log::info!(
            "[Syn Tools] Streaming final response (after {} tool calls)",
            tool_call_log.len()
        );

        // Convert working_messages back to SynMessage format for send_message
        let syn_messages: Vec<SynMessage> = working_messages
            .iter()
            .enumerate()
            .map(|(i, m)| SynMessage {
                id: format!("tool-msg-{}", i),
                role: m.role.clone(),
                content: m.content.clone(),
                model: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
                tokens: None,
                duration_ms: None,
                sources: None,
                tool_calls_log: None,
                images: None,
            })
            .collect();

        let mut final_msg = self
            .send_message(app, conversation_id, message_id, &syn_messages, model, temperature, num_ctx, max_history)
            .await?;

        if !tool_call_log.is_empty() {
            final_msg.tool_calls_log = Some(tool_call_log);
        }

        Ok(final_msg)
    }

    /// Make a non-streaming call to Ollama's /api/chat endpoint.
    ///
    /// Used during the tool calling loop where we need the complete response
    /// (including `tool_calls`) before proceeding.
    async fn call_non_streaming(
        &self,
        messages: &[OllamaChatRequestMessage],
        model: &str,
        temperature: Option<f64>,
        tools: Option<&[ToolDefinition]>,
        num_ctx: u32,
    ) -> AppResult<OllamaChatChunk> {
        let url = format!("{}/api/chat", self.base_url);

        let request_body = OllamaChatRequest {
            model: model.to_string(),
            messages: messages.to_vec(),
            stream: false,
            options: Some(OllamaChatOptions { temperature, num_ctx: Some(num_ctx) }),
            tools: tools.map(|t| t.to_vec()),
        };

        log::info!(
            "[Syn Tools] Non-streaming call to Ollama with {} messages",
            messages.len()
        );

        let resp = self
            .client
            .post(&url)
            .json(&request_body)
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
            AppError::General(format!("Failed to parse Ollama non-streaming response: {}", e))
        })?;

        Ok(chunk)
    }
}
