//! Anything that speaks the OpenAI chat-completions shape.
//!
//! That is not a synonym for "the cloud". OpenAI, OpenRouter, Groq, Together,
//! vLLM, LM Studio and llama.cpp's own server all accept this request, so this
//! one provider covers both "use a frontier model" and "use the fast local
//! server I already run". The key is optional for exactly that reason: a
//! localhost endpoint wants no `Authorization` header, and sending an empty
//! one makes some servers refuse the request outright.
//!
//! Four differences from Ollama are handled here and nowhere else, because
//! each of them fails silently rather than loudly:
//!
//! 1. **Arguments are a string.** Ollama sends `{"city":"Hanoi"}`; this sends
//!    `"{\"city\":\"Hanoi\"}"`. Tools parse the object, so it is converted at
//!    the boundary in both directions.
//! 2. **Tool results must name their call.** A `tool` message without
//!    `tool_call_id` is a 400. Ollama pairs by position and has no such field,
//!    which is why `ToolCall::id` is an `Option` rather than a `String`.
//! 3. **Streamed tool calls arrive in pieces**, keyed by `index`, with the
//!    arguments string split across chunks. They have to be reassembled.
//! 4. **Images are content parts**, not a sibling array, and want a data URI
//!    with a media type the caller never recorded.

use std::collections::HashSet;
use std::sync::RwLock;

use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::syn::{
    ModelInfo, ProviderStatus, SynProvider, ToolCall, ToolCallFunction, ToolDefinition,
};
use crate::syn::provider::{
    chat_client, probe_client, ChatMessage, ChatProvider, ChatReply, ChatRequest, StreamSink,
};

/// Endpoint-and-model pairs that turned out to need `reasoning_effort: "none"`.
///
/// A reasoning model reached through `/chat/completions` applies its own
/// default effort, and refuses function tools while it is set:
///
/// > Function tools with reasoning_effort are not supported for gpt-5.6-luna
/// > in /v1/chat/completions. To use function tools, use /v1/responses or set
/// > reasoning_effort to 'none'.
///
/// Syn always sends tools, so those models need the field — and every other
/// server speaking this API rejects a request carrying a field it does not
/// know. Neither default is safe, and no list of model names stays correct.
///
/// So it is learned instead: the first request fails, is retried once with the
/// field, and the pair is remembered for the rest of the session. The user is
/// never asked, because the only honest way to phrase the question needs them
/// to have read the same error.
static NEEDS_EFFORT_NONE: std::sync::LazyLock<RwLock<HashSet<String>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashSet::new()));

// ═══════════════════════════════════════════════════════════════
//  WIRE TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    /// Only ever present when the user asked for it. Servers that do not know
    /// the field reject a request that carries it.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: Option<OpenAiResponseMessage>,
    delta: Option<OpenAiResponseMessage>,
}

#[derive(Deserialize)]
struct OpenAiResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Deserialize)]
struct OpenAiToolCall {
    /// Absent on every streamed chunk after the first for a given `index`.
    #[serde(default)]
    index: Option<usize>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiToolCallFunction>,
}

#[derive(Deserialize)]
struct OpenAiToolCallFunction {
    #[serde(default)]
    name: Option<String>,
    /// JSON, as a string, and in streaming mode only a fragment of one.
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    completion_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct OpenAiModelsResponse {
    #[serde(default)]
    data: Vec<OpenAiModelEntry>,
}

#[derive(Deserialize)]
struct OpenAiModelEntry {
    id: String,
    #[serde(default)]
    created: Option<i64>,
    #[serde(default)]
    owned_by: Option<String>,
}

// ═══════════════════════════════════════════════════════════════
//  CONVERSION
// ═══════════════════════════════════════════════════════════════

/// The media type of a base64 payload, read from its first bytes.
///
/// The vault stores raw base64 with no note of what it is, and a data URI has
/// to declare something. Guessing `jpeg` for a PNG is rejected by some servers
/// and silently mis-decoded by others, so the magic numbers are worth the
/// twelve lines.
fn sniff_media_type(b64: &str) -> &'static str {
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

/// Arguments as OpenAI wants them: a string holding JSON.
fn arguments_to_string(value: &serde_json::Value) -> String {
    match value {
        // Already a string — a model that returned one, or a round trip.
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Arguments as the tools want them: a parsed object.
///
/// A model that emits malformed JSON is not rare, and the failure must reach
/// the tool as an argument it can reject rather than as a dropped call — so an
/// unparseable string is passed through as a string and left for
/// `execute_tool` to complain about.
fn arguments_to_value(raw: &str) -> serde_json::Value {
    if raw.trim().is_empty() {
        return serde_json::json!({});
    }
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

fn to_wire_message(m: &ChatMessage) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    out.insert("role".into(), serde_json::Value::String(m.role.clone()));

    // Content is a plain string unless there are images to carry, in which
    // case it becomes an array of parts. Sending the array form always would
    // be valid for OpenAI and rejected by several of the compatible servers.
    match &m.images {
        Some(images) if !images.is_empty() => {
            let mut parts = vec![serde_json::json!({ "type": "text", "text": m.content })];
            for img in images {
                let url = if img.starts_with("data:") {
                    img.clone()
                } else {
                    format!("data:{};base64,{}", sniff_media_type(img), img)
                };
                parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": url }
                }));
            }
            out.insert("content".into(), serde_json::Value::Array(parts));
        }
        _ => {
            out.insert("content".into(), serde_json::Value::String(m.content.clone()));
        }
    }

    if let Some(calls) = &m.tool_calls {
        let wire: Vec<serde_json::Value> = calls
            .iter()
            .enumerate()
            .map(|(i, tc)| {
                serde_json::json!({
                    // An assistant message replayed from a conversation
                    // written before ids existed has none. Synthesising a
                    // stable one keeps the request valid; the model only ever
                    // uses it to match the result that follows.
                    "id": tc.id.clone().unwrap_or_else(|| format!("call_{}", i)),
                    "type": "function",
                    "function": {
                        "name": tc.function.name,
                        "arguments": arguments_to_string(&tc.function.arguments),
                    }
                })
            })
            .collect();
        out.insert("tool_calls".into(), serde_json::Value::Array(wire));
    }

    if m.role == "tool" {
        // Required. A `tool` message without it is a 400, so a message that
        // lost its id still names something rather than nothing.
        let id = m.tool_call_id.clone().unwrap_or_else(|| "call_0".to_string());
        out.insert("tool_call_id".into(), serde_json::Value::String(id));
    }

    serde_json::Value::Object(out)
}

/// Tool calls being reassembled from a stream, in arrival order.
#[derive(Default)]
struct ToolCallAccumulator {
    /// `(index, id, name, arguments-so-far)`.
    slots: Vec<(usize, Option<String>, String, String)>,
}

impl ToolCallAccumulator {
    fn absorb(&mut self, calls: &[OpenAiToolCall]) {
        for (position, call) in calls.iter().enumerate() {
            // Chunks after the first often omit `index`; falling back to the
            // position within this chunk is right for the single-call case and
            // the only guess available for the rest.
            let index = call.index.unwrap_or(position);
            let slot = match self.slots.iter_mut().find(|(i, ..)| *i == index) {
                Some(s) => s,
                None => {
                    self.slots.push((index, None, String::new(), String::new()));
                    self.slots.last_mut().expect("just pushed")
                }
            };

            if let Some(id) = &call.id {
                slot.1 = Some(id.clone());
            }
            if let Some(f) = &call.function {
                if let Some(name) = &f.name {
                    slot.2.push_str(name);
                }
                if let Some(args) = &f.arguments {
                    slot.3.push_str(args);
                }
            }
        }
    }

    fn finish(mut self) -> Vec<ToolCall> {
        self.slots.sort_by_key(|(i, ..)| *i);
        self.slots
            .into_iter()
            .filter(|(_, _, name, _)| !name.is_empty())
            .map(|(_, id, name, args)| ToolCall {
                id,
                function: ToolCallFunction {
                    name,
                    arguments: arguments_to_value(&args),
                },
            })
            .collect()
    }
}

fn from_wire_tool_calls(calls: Vec<OpenAiToolCall>) -> Vec<ToolCall> {
    let mut acc = ToolCallAccumulator::default();
    acc.absorb(&calls);
    acc.finish()
}

// ═══════════════════════════════════════════════════════════════
//  PROVIDER
// ═══════════════════════════════════════════════════════════════

pub struct OpenAiCompatProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    reasoning_effort: Option<String>,
}

impl OpenAiCompatProvider {
    /// `base_url` includes the version segment — `https://api.openai.com/v1`.
    ///
    /// An empty or whitespace-only key is treated as no key at all, so a
    /// half-filled settings field cannot produce an `Authorization: Bearer `
    /// header that a local server rejects.
    pub fn new(
        base_url: &str,
        api_key: Option<String>,
        reasoning_effort: Option<String>,
    ) -> Self {
        let blank_is_absent = |v: Option<String>| {
            v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
        };

        Self {
            client: chat_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: blank_is_absent(api_key),
            reasoning_effort: blank_is_absent(reasoning_effort),
        }
    }

    fn authorize(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => req.bearer_auth(key),
            None => req,
        }
    }

    fn body(
        &self,
        req: &ChatRequest<'_>,
        stream: bool,
        reasoning_effort: Option<String>,
    ) -> OpenAiChatRequest {
        OpenAiChatRequest {
            model: req.model.to_string(),
            messages: req.messages.iter().map(to_wire_message).collect(),
            stream,
            temperature: req.temperature,
            tools: req.tools.map(|t| t.to_vec()),
            reasoning_effort,
            // `num_ctx` is deliberately absent. The context window here is a
            // property of the model, not something the client asks for, and
            // an unknown field is rejected by some of these servers.
        }
    }

    fn memo_key(&self, model: &str) -> String {
        format!("{}|{}", self.base_url, model)
    }

    /// The effort to send: whatever the user pinned in settings, else `none`
    /// if this endpoint and model have already told us they need it.
    fn effort_for(&self, model: &str) -> Option<String> {
        if self.reasoning_effort.is_some() {
            return self.reasoning_effort.clone();
        }
        NEEDS_EFFORT_NONE
            .read()
            .ok()
            .filter(|known| known.contains(&self.memo_key(model)))
            .map(|_| "none".to_string())
    }

    fn remember_needs_none(&self, model: &str) {
        if let Ok(mut known) = NEEDS_EFFORT_NONE.write() {
            known.insert(self.memo_key(model));
        }
        log::info!(
            "[Syn] {} needs reasoning_effort=none to accept tools; sending it from now on",
            model
        );
    }

    /// POST the completion, and adapt once if the answer says to.
    ///
    /// The retry is deliberately narrow — one status, one field, one attempt,
    /// and only when nothing has been streamed yet, which is guaranteed
    /// because a non-2xx arrives before any body is read.
    async fn send(
        &self,
        req: &ChatRequest<'_>,
        stream: bool,
        what: &str,
    ) -> AppResult<reqwest::Response> {
        let url = format!("{}/chat/completions", self.base_url);
        let effort = self.effort_for(req.model);

        let post = |body: OpenAiChatRequest| {
            self.authorize(self.client.post(&url).json(&body)).send()
        };

        let resp = post(self.body(req, stream, effort.clone()))
            .await
            .map_err(|e| {
                AppError::General(format!("Failed to connect to {}: {}", self.base_url, e))
            })?;

        if resp.status().is_success() {
            return Ok(resp);
        }

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        let fixable = status == reqwest::StatusCode::BAD_REQUEST
            && body.contains("reasoning_effort")
            && effort.is_none();

        if !fixable {
            return Err(self.explain(status, &body, what));
        }

        self.remember_needs_none(req.model);

        let retried = post(self.body(req, stream, Some("none".to_string())))
            .await
            .map_err(|e| {
                AppError::General(format!("Failed to connect to {}: {}", self.base_url, e))
            })?;

        if retried.status().is_success() {
            return Ok(retried);
        }

        let status = retried.status();
        let body = retried.text().await.unwrap_or_default();
        Err(self.explain(status, &body, what))
    }

    /// Turn a non-2xx into a message worth reading.
    ///
    /// The hint has to come from the body, not the status alone. A 404 here is
    /// two completely different problems — a base URL missing its `/v1`, and a
    /// model the endpoint does not have — and telling someone to check their
    /// URL when the body says `model_not_found` sends them the wrong way.
    fn explain(&self, status: reqwest::StatusCode, body: &str, what: &str) -> AppError {
        let hint = match status.as_u16() {
            401 | 403 => " — check the API key",
            404 if body.contains("model_not_found") || body.contains("does not exist") => {
                " — this endpoint does not have that model; pick another one"
            }
            404 => " — check the base URL includes the version path, e.g. /v1",
            _ => "",
        };

        AppError::General(format!(
            "{} at {} returned status {}{}: {}",
            what, self.base_url, status, hint, body
        ))
    }
}

#[async_trait]
impl ChatProvider for OpenAiCompatProvider {
    fn id(&self) -> SynProvider {
        SynProvider::OpenAiCompat
    }

    /// Yes — `ToolCallAccumulator` exists precisely to reassemble them from
    /// the stream, so the tool loop never has to fall back to a blocking call.
    fn streams_tool_calls(&self) -> bool {
        true
    }

    async fn check_status(&self) -> AppResult<ProviderStatus> {
        // There is no `/version`. Listing models is the cheapest call that
        // proves both that the endpoint is there and that the key works.
        let url = format!("{}/models", self.base_url);

        let status = |connected: bool| ProviderStatus {
            connected,
            version: None,
            url: self.base_url.clone(),
            supports_model_management: false,
        };

        let request = self.authorize(probe_client().get(&url));
        match request.send().await {
            Ok(resp) if resp.status().is_success() => Ok(status(true)),
            Ok(resp) => {
                log::warn!(
                    "{} responded to /models with status {}",
                    self.base_url,
                    resp.status()
                );
                Ok(status(false))
            }
            Err(e) => {
                log::info!("{} not reachable: {}", self.base_url, e);
                Ok(status(false))
            }
        }
    }

    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        let url = format!("{}/models", self.base_url);

        let resp = self
            .authorize(self.client.get(&url))
            .send()
            .await
            .map_err(|e| AppError::General(format!("Failed to connect to {}: {}", self.base_url, e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(self.explain(status, &body, "Listing models"));
        }

        let body: OpenAiModelsResponse = resp
            .json()
            .await
            .map_err(|e| AppError::General(format!("Failed to parse the model list: {}", e)))?;

        Ok(body
            .data
            .into_iter()
            .map(|m| ModelInfo {
                name: m.id.clone(),
                model: m.id,
                // Nothing here is local, so there are no bytes on disk and no
                // digest. Zero is honest; the UI shows a size only when it has
                // one.
                size: 0,
                digest: String::new(),
                modified_at: m
                    .created
                    .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
                    .map(|d| d.to_rfc3339())
                    .unwrap_or_default(),
                details: m.owned_by.map(|owner| crate::models::syn::ModelDetails {
                    format: None,
                    family: Some(owner),
                    parameter_size: None,
                    quantization_level: None,
                }),
            })
            .collect())
    }

    async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply> {
        log::info!(
            "[Syn] Non-streaming call to {} with {} messages",
            self.base_url,
            req.messages.len()
        );

        let started = std::time::Instant::now();
        let resp = self.send(&req, false, "Chat completion").await?;

        let body: OpenAiChatResponse = resp
            .json()
            .await
            .map_err(|e| AppError::General(format!("Failed to parse the chat response: {}", e)))?;

        let message = body.choices.into_iter().next().and_then(|c| c.message);

        Ok(ChatReply {
            content: message
                .as_ref()
                .and_then(|m| m.content.clone())
                .unwrap_or_default(),
            tool_calls: message
                .and_then(|m| m.tool_calls)
                .map(from_wire_tool_calls)
                .unwrap_or_default(),
            tokens: body.usage.and_then(|u| u.completion_tokens),
            duration_ms: Some(started.elapsed().as_millis() as u64),
        })
    }

    async fn chat_streaming(
        &self,
        req: ChatRequest<'_>,
        sink: &StreamSink<'_>,
    ) -> AppResult<ChatReply> {
        let started = std::time::Instant::now();
        let resp = self.send(&req, true, "Chat completion").await?;

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut reply = ChatReply::default();
        let mut tools = ToolCallAccumulator::default();

        while let Some(chunk_result) = stream.next().await {
            if (sink.stop_requested)() {
                break;
            }

            let chunk = chunk_result
                .map_err(|e| AppError::General(format!("Stream error during chat: {}", e)))?;

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Server-sent events: records are separated by a blank line, and
            // the payload lines are prefixed `data: `. Anything else in a
            // record — comments, `event:` lines — is not part of this API.
            while let Some(split) = buffer.find("\n\n").or_else(|| buffer.find("\r\n\r\n")) {
                let sep_len = if buffer[split..].starts_with("\r\n\r\n") { 4 } else { 2 };
                let record = buffer[..split].to_string();
                buffer = buffer[split + sep_len..].to_string();

                for line in record.lines() {
                    let Some(payload) = line.trim().strip_prefix("data:") else {
                        continue;
                    };
                    let payload = payload.trim();

                    if payload.is_empty() || payload == "[DONE]" {
                        continue;
                    }

                    match serde_json::from_str::<OpenAiChatResponse>(payload) {
                        Ok(parsed) => {
                            if let Some(usage) = parsed.usage {
                                if let Some(n) = usage.completion_tokens {
                                    reply.tokens = Some(n);
                                }
                            }
                            for choice in parsed.choices {
                                // `delta` while streaming; a few servers send
                                // a whole `message` on the final record.
                                let Some(part) = choice.delta.or(choice.message) else {
                                    continue;
                                };
                                if let Some(text) = part.content {
                                    if !text.is_empty() {
                                        reply.content.push_str(&text);
                                        (sink.on_token)(&text);
                                    }
                                }
                                if let Some(calls) = part.tool_calls {
                                    tools.absorb(&calls);
                                }
                            }
                        }
                        // Same tolerance as the Ollama path: a record that
                        // will not parse costs a token, not the answer.
                        Err(e) => {
                            log::warn!("Failed to parse chat chunk: {} — raw: {}", e, payload);
                        }
                    }
                }
            }
        }

        reply.tool_calls = tools.finish();
        if reply.duration_ms.is_none() {
            reply.duration_ms = Some(started.elapsed().as_millis() as u64);
        }

        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_cross_the_boundary_both_ways() {
        let object = serde_json::json!({ "city": "Hà Nội", "n": 3 });
        let as_string = arguments_to_string(&object);
        assert_eq!(arguments_to_value(&as_string), object);
    }

    /// A model that already produced a string must not be double-encoded into
    /// `"\"{...}\""`, which parses back to a string and reaches the tool as a
    /// scalar it cannot read.
    #[test]
    fn a_string_argument_is_not_wrapped_again() {
        let already = serde_json::Value::String("{\"a\":1}".into());
        assert_eq!(arguments_to_string(&already), "{\"a\":1}");
    }

    #[test]
    fn empty_arguments_become_an_empty_object() {
        assert_eq!(arguments_to_value(""), serde_json::json!({}));
        assert_eq!(arguments_to_value("   "), serde_json::json!({}));
    }

    /// Malformed JSON reaches the tool rather than vanishing, so the failure
    /// is one the user can see and the model can be told about.
    #[test]
    fn malformed_arguments_survive_as_a_string() {
        let out = arguments_to_value("{\"a\": ");
        assert_eq!(out, serde_json::Value::String("{\"a\": ".into()));
    }

    #[test]
    fn a_streamed_tool_call_is_reassembled_from_its_pieces() {
        let mut acc = ToolCallAccumulator::default();

        acc.absorb(&[OpenAiToolCall {
            index: Some(0),
            id: Some("call_abc".into()),
            function: Some(OpenAiToolCallFunction {
                name: Some("query_nodes".into()),
                arguments: Some("{\"query\":".into()),
            }),
        }]);
        acc.absorb(&[OpenAiToolCall {
            index: Some(0),
            id: None,
            function: Some(OpenAiToolCallFunction {
                name: None,
                arguments: Some("\"type:book\"}".into()),
            }),
        }]);

        let calls = acc.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id.as_deref(), Some("call_abc"));
        assert_eq!(calls[0].function.name, "query_nodes");
        assert_eq!(
            calls[0].function.arguments,
            serde_json::json!({ "query": "type:book" })
        );
    }

    /// Two calls in one turn come back interleaved and out of order; they must
    /// end up separate and in index order, not concatenated into one.
    #[test]
    fn parallel_tool_calls_stay_separate_and_ordered() {
        let mut acc = ToolCallAccumulator::default();
        acc.absorb(&[
            OpenAiToolCall {
                index: Some(1),
                id: Some("b".into()),
                function: Some(OpenAiToolCallFunction {
                    name: Some("second".into()),
                    arguments: Some("{}".into()),
                }),
            },
            OpenAiToolCall {
                index: Some(0),
                id: Some("a".into()),
                function: Some(OpenAiToolCallFunction {
                    name: Some("first".into()),
                    arguments: Some("{}".into()),
                }),
            },
        ]);

        let calls = acc.finish();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "first");
        assert_eq!(calls[1].function.name, "second");
    }

    /// Some servers omit `index` entirely for a single call.
    #[test]
    fn a_call_without_an_index_still_arrives() {
        let mut acc = ToolCallAccumulator::default();
        acc.absorb(&[OpenAiToolCall {
            index: None,
            id: Some("x".into()),
            function: Some(OpenAiToolCallFunction {
                name: Some("only".into()),
                arguments: Some("{\"k\":1}".into()),
            }),
        }]);

        let calls = acc.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "only");
    }

    #[test]
    fn a_tool_message_names_the_call_it_answers() {
        let mut m = ChatMessage::new("tool", "42 rows");
        m.tool_call_id = Some("call_abc".into());

        let wire = to_wire_message(&m);
        assert_eq!(wire["tool_call_id"], "call_abc");
        assert_eq!(wire["content"], "42 rows");
    }

    /// The id is required, so a message that lost one still has to send
    /// something rather than omit the field and be refused.
    #[test]
    fn a_tool_message_without_an_id_still_sends_one() {
        let m = ChatMessage::new("tool", "result");
        let wire = to_wire_message(&m);
        assert!(wire.get("tool_call_id").is_some());
    }

    #[test]
    fn an_assistant_message_carries_its_calls_with_string_arguments() {
        let mut m = ChatMessage::new("assistant", "");
        m.tool_calls = Some(vec![ToolCall {
            id: Some("call_1".into()),
            function: ToolCallFunction {
                name: "create_node".into(),
                arguments: serde_json::json!({ "type": "book" }),
            },
        }]);

        let wire = to_wire_message(&m);
        let calls = wire["tool_calls"].as_array().expect("tool_calls array");
        assert_eq!(calls[0]["id"], "call_1");
        assert_eq!(calls[0]["type"], "function");
        // A string, not an object — this is the difference from Ollama.
        assert_eq!(calls[0]["function"]["arguments"], "{\"type\":\"book\"}");
    }

    /// Text-only messages must keep the plain string form: the parts array is
    /// valid for OpenAI itself and refused by several compatible servers.
    #[test]
    fn a_message_without_images_sends_plain_text() {
        let wire = to_wire_message(&ChatMessage::new("user", "xin chào"));
        assert!(wire["content"].is_string());
        assert_eq!(wire["content"], "xin chào");
    }

    #[test]
    fn images_become_data_uri_parts_with_a_sniffed_media_type() {
        let mut m = ChatMessage::new("user", "what is this");
        m.images = Some(vec!["iVBORw0KGgoAAAA".into()]);

        let wire = to_wire_message(&m);
        let parts = wire["content"].as_array().expect("content parts");
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(
            parts[1]["image_url"]["url"],
            "data:image/png;base64,iVBORw0KGgoAAAA"
        );
    }

    /// An image already given as a data URI is passed through, not nested
    /// inside a second one.
    #[test]
    fn an_existing_data_uri_is_left_alone() {
        let mut m = ChatMessage::new("user", "");
        m.images = Some(vec!["data:image/webp;base64,UklGRxx".into()]);

        let wire = to_wire_message(&m);
        let parts = wire["content"].as_array().expect("content parts");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/webp;base64,UklGRxx");
    }

    #[test]
    fn media_types_are_read_from_the_payload() {
        assert_eq!(sniff_media_type("iVBORw0KGgo…"), "image/png");
        assert_eq!(sniff_media_type("R0lGOD…"), "image/gif");
        assert_eq!(sniff_media_type("UklGR…"), "image/webp");
        assert_eq!(sniff_media_type("/9j/4AAQ…"), "image/jpeg");
        assert_eq!(sniff_media_type("unrecognised"), "image/jpeg");
    }

    /// A blank key must not produce `Authorization: Bearer `, which a local
    /// server rejects outright — the common case of a settings field that was
    /// opened and left empty.
    #[test]
    fn a_blank_key_is_no_key() {
        assert!(
            OpenAiCompatProvider::new("http://localhost:8080/v1", Some("   ".into()), None)
                .api_key
                .is_none()
        );
        assert!(
            OpenAiCompatProvider::new("http://localhost:8080/v1", Some(String::new()), None)
                .api_key
                .is_none()
        );
        assert_eq!(
            OpenAiCompatProvider::new("http://x/v1", Some("  sk-abc  ".into()), None).api_key,
            Some("sk-abc".to_string())
        );
    }

    #[test]
    fn a_trailing_slash_does_not_double_up_the_path() {
        let p = OpenAiCompatProvider::new("https://api.openai.com/v1/", None, None);
        assert_eq!(p.base_url, "https://api.openai.com/v1");
    }

    fn body_json(provider: &OpenAiCompatProvider, model: &str) -> serde_json::Value {
        let messages = [ChatMessage::new("user", "hi")];
        let req = ChatRequest {
            model,
            messages: &messages,
            temperature: None,
            num_ctx: 8192,
            tools: None,
        };
        serde_json::to_value(provider.body(&req, false, provider.effort_for(model)))
            .expect("serialises")
    }

    /// Most servers speaking this API have never heard of `reasoning_effort`
    /// and reject a request that carries it, so the field must be absent until
    /// something proves it is needed.
    #[test]
    fn reasoning_effort_is_absent_by_default() {
        let p = OpenAiCompatProvider::new("http://absent.test/v1", None, None);
        assert!(body_json(&p, "llama-3").get("reasoning_effort").is_none());

        let blank = OpenAiCompatProvider::new("http://absent.test/v1", None, Some("  ".into()));
        assert!(body_json(&blank, "llama-3").get("reasoning_effort").is_none());
    }

    /// A user who pinned one in the settings file gets it, and the learning is
    /// bypassed entirely.
    #[test]
    fn an_explicit_setting_is_sent_as_given() {
        let p = OpenAiCompatProvider::new("http://explicit.test/v1", None, Some("high".into()));
        assert_eq!(body_json(&p, "some-model")["reasoning_effort"], "high");
    }

    /// The learned case: once an endpoint has refused tools because of
    /// `reasoning_effort`, that pair sends `none` without being asked again.
    #[test]
    fn a_model_that_refused_tools_is_remembered() {
        let p = OpenAiCompatProvider::new("http://learned.test/v1", None, None);
        assert!(body_json(&p, "gpt-5.6-luna").get("reasoning_effort").is_none());

        p.remember_needs_none("gpt-5.6-luna");

        assert_eq!(body_json(&p, "gpt-5.6-luna")["reasoning_effort"], "none");
        // Remembered per model, not for the whole endpoint: a second model on
        // the same server may well reject the field.
        assert!(body_json(&p, "gpt-4o").get("reasoning_effort").is_none());
    }

    /// And per endpoint, not per model name — the same name on a different
    /// server is a different model with different rules.
    #[test]
    fn the_memory_does_not_leak_between_endpoints() {
        let one = OpenAiCompatProvider::new("http://one.test/v1", None, None);
        let two = OpenAiCompatProvider::new("http://two.test/v1", None, None);

        one.remember_needs_none("shared-name");

        assert_eq!(body_json(&one, "shared-name")["reasoning_effort"], "none");
        assert!(body_json(&two, "shared-name").get("reasoning_effort").is_none());
    }

    /// `num_ctx` is Ollama's word. Sending it here is an unknown field, which
    /// several of these servers refuse outright.
    #[test]
    fn num_ctx_never_reaches_an_openai_endpoint() {
        let p = OpenAiCompatProvider::new("http://ctx.test/v1", None, None);
        assert!(body_json(&p, "any").get("num_ctx").is_none());
    }

    /// A 404 is two different problems, and the body is what separates them.
    /// Telling someone to check their base URL when the endpoint said
    /// `model_not_found` sends them to the one setting that was already right.
    #[test]
    fn a_missing_model_is_not_reported_as_a_bad_url() {
        let p = OpenAiCompatProvider::new("https://api.openai.com/v1", None, None);

        let missing_model = p
            .explain(
                reqwest::StatusCode::NOT_FOUND,
                r#"{"error":{"message":"The model `gemma4:e4b` does not exist","code":"model_not_found"}}"#,
                "Chat completion",
            )
            .to_string();
        assert!(missing_model.contains("does not have that model"), "{missing_model}");
        assert!(!missing_model.contains("version path"), "{missing_model}");

        let bad_url = p
            .explain(reqwest::StatusCode::NOT_FOUND, "<html>404</html>", "Chat completion")
            .to_string();
        assert!(bad_url.contains("version path"), "{bad_url}");
    }

    #[test]
    fn a_bad_key_says_so() {
        let p = OpenAiCompatProvider::new("https://api.openai.com/v1", None, None);
        let msg = p
            .explain(reqwest::StatusCode::UNAUTHORIZED, "{}", "Chat completion")
            .to_string();
        assert!(msg.contains("check the API key"), "{msg}");
    }
}
