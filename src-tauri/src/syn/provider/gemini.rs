//! Google's Gemini, spoken in its own shape.
//!
//! # Why not the OpenAI-compatible endpoint Google also serves
//!
//! Because it breaks exactly where Syn lives. Gemini 3 signs every function
//! call it makes with a `thoughtSignature` — its reasoning, encrypted — and a
//! request that replays the call without that signature is refused:
//!
//! > 400 INVALID_ARGUMENT: Function call is missing a thought_signature
//!
//! On the compatible endpoint the signature travels in a non-standard
//! `extra_content.google` field on the tool call. Clients that rebuild tool
//! calls field by field drop it — VS Code, Codex, Open WebUI and goose all
//! shipped that bug — and so would `OpenAiCompatProvider`, whose `ToolCall` is
//! exactly that kind of rebuild. The first round of a question works. The
//! second fails. Syn is a tool loop; the second round is most questions.
//!
//! Speaking `generateContent` directly makes the signature an ordinary field
//! on an ordinary part, and it gets the rest of the API as Google documents it:
//! the real cache and reasoning counts, the real capability list, and error
//! bodies that say what went wrong.
//!
//! # What is different from the other two, and handled only here
//!
//! 1. **No system message.** The system prompt is `systemInstruction`, outside
//!    `contents`, which holds only `user` and `model` turns.
//! 2. **No `tool` role.** A tool result is a `functionResponse` part inside a
//!    `user` turn, and it names the *function*, not the call — so the name has
//!    to be recovered from the call it answers.
//! 3. **Results of parallel calls go back together**, in one turn, in the order
//!    the calls were made. Consecutive messages of one role are merged.
//! 4. **The signature rides on the call.** Carried out of the reply in
//!    `ToolCall::thought_signature` and put back on the same part.
//! 5. **Temperature is left alone on Gemini 3.** Google: *"we strongly
//!    recommend keeping the temperature parameter at its default value of 1.0
//!    … setting it below 1.0 may lead to unexpected behavior, such as looping or
//!    degraded performance."* Syn's default is 0.7. See `sends_temperature`.
//!
//! # And one thing to know
//!
//! Google's documentation now files `generateContent` under "legacy", beside a
//! newer Interactions API. It is not deprecated — the reference marks nothing —
//! and it is the stable surface everything else is built on. The day it is,
//! this file is where that change goes, and nothing outside it has to know.

use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};
use crate::models::syn::{
    ModelInfo, ProviderStatus, SynProvider, ToolCall, ToolCallFunction, ToolDefinition,
};
use crate::syn::provider::{
    chat_client, probe_client, ChatMessage, ChatProvider, ChatReply, ChatRequest, StreamSink,
    Usage,
};

/// Where the Gemini API lives, version segment included.
pub const GEMINI_API: &str = "https://generativelanguage.googleapis.com/v1beta";

/// The prefix on a call id this file made up.
///
/// `generateContent` often returns function calls with no `id`, and the tool
/// loop needs one to pair a result with its call. So one is minted — and never
/// sent back to Google, which did not issue it and has no reason to accept it.
const MINTED: &str = "syn-gemini-call-";

// ═══════════════════════════════════════════════════════════════
//  WIRE TYPES — what comes back
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GenerateResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    usage_metadata: Option<UsageMetadata>,
    prompt_feedback: Option<PromptFeedback>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<Content>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Content {
    #[serde(default)]
    parts: Vec<Part>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Part {
    text: Option<String>,
    /// A summary of the model's own reasoning. Only sent when asked for with
    /// `includeThoughts`, which this never does — but a part that is one must
    /// not end up in the answer if a model sends it anyway.
    #[serde(default)]
    thought: bool,
    thought_signature: Option<String>,
    function_call: Option<FunctionCall>,
}

#[derive(Deserialize)]
struct FunctionCall {
    name: String,
    #[serde(default)]
    args: Value,
    id: Option<String>,
}

/// What a turn cost, in Gemini's own words.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageMetadata {
    prompt_token_count: Option<u64>,
    cached_content_token_count: Option<u64>,
    candidates_token_count: Option<u64>,
    thoughts_token_count: Option<u64>,
    tool_use_prompt_token_count: Option<u64>,
}

impl From<UsageMetadata> for Usage {
    /// Normalised to the shape OpenAI reports, so `Usage::charged` means one
    /// thing whichever provider filled it.
    ///
    /// OpenAI's `completion_tokens` *includes* the reasoning it then reports
    /// again as `reasoning_tokens`. Gemini reports the two apart:
    /// `candidatesTokenCount` is what was written and `thoughtsTokenCount` is
    /// the thinking, both billed as output. Copied across field for field,
    /// `charged` would leave the thinking out — and a reasoning model that
    /// looks cheap is the specific mistake `Usage` exists to prevent.
    fn from(u: UsageMetadata) -> Self {
        let add = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
        Usage {
            // Built-in tool prompts are input the model was charged for; the
            // function calling Syn does never produces any, so this is almost
            // always the prompt alone.
            input: add(u.prompt_token_count, u.tool_use_prompt_token_count),
            input_cached: u.cached_content_token_count,
            output: add(u.candidates_token_count, u.thoughts_token_count),
            output_hidden: u.thoughts_token_count,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptFeedback {
    block_reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelsPage {
    #[serde(default)]
    models: Vec<ModelEntry>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelEntry {
    /// `models/gemini-3.8-flash` — the collection prefix included.
    name: String,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════
//  CONVERSION — what goes out
// ═══════════════════════════════════════════════════════════════

/// The path segment for a model: `models/<id>`, and never `models/models/<id>`.
fn model_path(model: &str) -> String {
    let id = model.trim().trim_start_matches("models/");
    format!("models/{id}")
}

/// Whether to send the user's temperature at all.
///
/// Only to Gemini 1 and 2. For Gemini 3 Google is unusually direct — keep it at
/// the default of 1.0, because lower values *"may lead to unexpected behavior,
/// such as looping or degraded performance."* Syn's default is 0.7, which is
/// sensible for every other model this app talks to and exactly wrong here.
///
/// Decided by what the model is *not* rather than what it is, so a Gemini 4 is
/// left alone by default instead of being sent a value its maker warned
/// against for the generation before it.
fn sends_temperature(model: &str) -> bool {
    let id = model.trim().trim_start_matches("models/");
    id.starts_with("gemini-1") || id.starts_with("gemini-2")
}

/// A tool result as a `functionResponse` wants it: an object.
///
/// Syn's tools answer with text that is usually JSON. An object goes across as
/// itself. Anything else — an array, a bare string, prose — is wrapped under
/// `output`, which is the key Gemini's own documentation reads a function's
/// result from when there is no other structure to go on.
fn response_object(content: &str) -> Value {
    match serde_json::from_str::<Value>(content) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(other) => json!({ "output": other }),
        Err(_) => json!({ "output": content }),
    }
}

/// Arguments as the tools want them: an object, even when the model sent none.
fn arguments_of(args: Value) -> Value {
    match args {
        Value::Null => json!({}),
        other => other,
    }
}

/// Parts for one message, in the order the model should read them.
fn parts_of(m: &ChatMessage, names: &mut NameBook) -> Vec<Value> {
    let mut parts = Vec::new();

    match m.role.as_str() {
        "tool" => {
            let name = names.answer(m.tool_call_id.as_deref());
            let mut response = Map::new();
            response.insert("name".into(), Value::String(name));
            response.insert("response".into(), response_object(&m.content));
            if let Some(id) = m.tool_call_id.as_deref().filter(|id| !id.starts_with(MINTED)) {
                response.insert("id".into(), Value::String(id.to_string()));
            }
            parts.push(json!({ "functionResponse": Value::Object(response) }));
        }
        _ => {
            if !m.content.is_empty() {
                parts.push(json!({ "text": m.content }));
            }
            for img in m.images.iter().flatten() {
                let data = img.split_once(";base64,").map(|(_, d)| d).unwrap_or(img);
                parts.push(json!({
                    "inlineData": {
                        "mimeType": crate::syn::provider::media_type_of(data),
                        "data": data,
                    }
                }));
            }
            for call in m.tool_calls.iter().flatten() {
                names.asked(call);
                let mut function_call = Map::new();
                function_call.insert("name".into(), Value::String(call.function.name.clone()));
                function_call.insert("args".into(), arguments_of(call.function.arguments.clone()));
                if let Some(id) = call.id.as_deref().filter(|id| !id.starts_with(MINTED)) {
                    function_call.insert("id".into(), Value::String(id.to_string()));
                }

                let mut part = Map::new();
                part.insert("functionCall".into(), Value::Object(function_call));
                // Back on the very part it came in on. This is the field whose
                // absence is a 400 on the next request.
                if let Some(signature) = &call.thought_signature {
                    part.insert("thoughtSignature".into(), Value::String(signature.clone()));
                }
                parts.push(Value::Object(part));
            }
        }
    }

    parts
}

/// Which function each outstanding call was to.
///
/// A `functionResponse` has to name the function; Syn's tool message carries
/// only the call's id. So names are noted as calls go past and looked up as
/// their answers do. By id when there is one, and otherwise in order — the
/// same way Ollama pairs them, and the order Gemini expects them back in.
#[derive(Default)]
struct NameBook {
    by_id: Vec<(Option<String>, String)>,
    next: usize,
}

impl NameBook {
    fn asked(&mut self, call: &ToolCall) {
        self.by_id.push((call.id.clone(), call.function.name.clone()));
    }

    fn answer(&mut self, id: Option<&str>) -> String {
        if let Some(id) = id {
            if let Some(found) = self.by_id.iter().position(|(known, _)| known.as_deref() == Some(id)) {
                self.next = found + 1;
                return self.by_id[found].1.clone();
            }
        }
        let name = self
            .by_id
            .get(self.next)
            .map(|(_, name)| name.clone())
            // A result with no call before it is a history this app did not
            // write. Gemini still needs a name; the honest one is "unknown".
            .unwrap_or_else(|| "unknown".to_string());
        self.next += 1;
        name
    }
}

/// The request body, from the messages the tool loop holds.
///
/// Leading system messages become `systemInstruction`. A system message later
/// in the history — nothing writes one today, but a history is data — goes in
/// as a `user` turn rather than being moved to the top, where it would change
/// meaning by changing place.
fn request_body(req: &ChatRequest<'_>) -> Value {
    let mut system: Vec<&str> = Vec::new();
    let mut contents: Vec<(String, Vec<Value>)> = Vec::new();
    let mut names = NameBook::default();
    let mut past_the_top = false;

    for m in req.messages {
        if m.role == "system" && !past_the_top {
            if !m.content.trim().is_empty() {
                system.push(&m.content);
            }
            continue;
        }
        past_the_top = true;

        let role = if m.role == "assistant" { "model" } else { "user" };
        let parts = parts_of(m, &mut names);
        if parts.is_empty() {
            continue;
        }

        // One turn per run of the same role. Results of parallel calls have
        // to arrive together, in one turn, or Gemini cannot pair them.
        match contents.last_mut() {
            Some((last, existing)) if last == role => existing.extend(parts),
            _ => contents.push((role.to_string(), parts)),
        }
    }

    close_the_last_turn(&mut contents);

    let mut body = Map::new();
    body.insert(
        "contents".into(),
        Value::Array(
            contents
                .into_iter()
                .map(|(role, parts)| json!({ "role": role, "parts": parts }))
                .collect(),
        ),
    );

    if !system.is_empty() {
        body.insert(
            "systemInstruction".into(),
            json!({ "parts": [{ "text": system.join("\n\n") }] }),
        );
    }

    if let Some(tools) = req.tools.filter(|t| !t.is_empty()) {
        body.insert("tools".into(), json!([{ "functionDeclarations": declarations(tools) }]));
    }

    if let Some(t) = req.temperature.filter(|_| sends_temperature(req.model)) {
        body.insert("generationConfig".into(), json!({ "temperature": t }));
    }

    Value::Object(body)
}

/// Never let a request end on a model turn that asked for something.
///
/// Newer Gemini models refuse it outright — *"Requests ending with a model turn
/// are not supported"* — where older ones tolerated it. The engine answers any
/// call it did not get to (`engine::answer_the_unanswered`), and this is the
/// same promise kept at the last place it can be: whatever path through the
/// tool loop a future change opens, a request with calls left hanging goes out
/// with each one answered as not run, rather than as a 400 and an empty reply.
///
/// A model turn of plain text is left alone. Nothing here writes one last, and
/// inventing a user's words to follow it would be worse than the error.
fn close_the_last_turn(contents: &mut Vec<(String, Vec<Value>)>) {
    let Some((role, parts)) = contents.last() else { return };
    if role != "model" {
        return;
    }
    let waiting: Vec<Value> = parts
        .iter()
        .filter_map(|p| p.get("functionCall"))
        .map(|call| {
            let mut response = Map::new();
            response.insert("name".into(), call.get("name").cloned().unwrap_or(Value::Null));
            response.insert(
                "response".into(),
                json!({ "output": "Not run: the work stopped before this call was made." }),
            );
            if let Some(id) = call.get("id") {
                response.insert("id".into(), id.clone());
            }
            json!({ "functionResponse": Value::Object(response) })
        })
        .collect();
    if !waiting.is_empty() {
        contents.push(("user".to_string(), waiting));
    }
}

/// Syn's tool declarations, as Gemini reads them.
///
/// `parametersJsonSchema` rather than `parameters`. The second takes an
/// OpenAPI subset and rejects keywords a JSON Schema may carry; the first takes
/// JSON Schema itself, which is what these already are — so they cross without
/// being translated, and a keyword added to a tool one day does not become a
/// 400 the next.
fn declarations(tools: &[ToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            json!({
                "name": t.function.name,
                "description": t.function.description,
                "parametersJsonSchema": t.function.parameters,
            })
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════
//  READING A REPLY
// ═══════════════════════════════════════════════════════════════

/// A reply being put together, from one response or from a stream of them.
#[derive(Default)]
struct Assembly {
    reply: ChatReply,
    finish: Option<String>,
    blocked: Option<String>,
}

impl Assembly {
    /// Take in one response. Returns the text it added, for streaming.
    fn absorb(&mut self, response: GenerateResponse) -> String {
        let mut added = String::new();

        // Each chunk of a stream carries the running totals; the last one is
        // the whole turn.
        if let Some(usage) = response.usage_metadata {
            self.reply.usage = usage.into();
        }
        if let Some(reason) = response.prompt_feedback.and_then(|f| f.block_reason) {
            self.blocked = Some(reason);
        }

        let Some(candidate) = response.candidates.into_iter().next() else {
            return added;
        };
        if candidate.finish_reason.is_some() {
            self.finish = candidate.finish_reason;
        }

        for part in candidate.content.map(|c| c.parts).unwrap_or_default() {
            if let Some(call) = part.function_call {
                let id = call
                    .id
                    .unwrap_or_else(|| format!("{MINTED}{}", self.reply.tool_calls.len()));
                self.reply.tool_calls.push(ToolCall {
                    id: Some(id),
                    function: ToolCallFunction {
                        name: call.name,
                        arguments: arguments_of(call.args),
                    },
                    thought_signature: part.thought_signature,
                });
                continue;
            }
            if part.thought {
                continue;
            }
            if let Some(text) = part.text.filter(|t| !t.is_empty()) {
                self.reply.content.push_str(&text);
                added.push_str(&text);
            }
        }

        added
    }

    /// The reply, or the reason there is none.
    ///
    /// A turn that produced neither words nor calls is not an empty answer. It
    /// is a refusal — safety, recitation, a malformed call — and showing the
    /// person nothing would hide the one thing they need to know.
    fn finish(self) -> AppResult<ChatReply> {
        let empty = self.reply.content.trim().is_empty() && self.reply.tool_calls.is_empty();
        if empty {
            if let Some(reason) = self.blocked {
                return Err(AppError::General(format!(
                    "Gemini refused the request ({reason}) before answering it"
                )));
            }
            if let Some(reason) = self.finish.filter(|r| r != "STOP") {
                return Err(AppError::General(format!(
                    "Gemini stopped without answering ({reason})"
                )));
            }
        }
        Ok(self.reply)
    }
}

/// Turn a non-2xx into a sentence worth reading.
///
/// Gemini's error bodies are `{"error": {"message": …, "status": …}}`, and the
/// message is the useful part — "API key not valid", "quota exceeded" — so it
/// is lifted out rather than showing a paragraph of JSON.
fn explain(status: reqwest::StatusCode, body: &str, what: &str) -> AppError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.to_string());

    let hint = match status.as_u16() {
        400 if message.contains("API key") => " — check the API key in Syn settings",
        401 | 403 => " — check the API key in Syn settings",
        404 => " — Gemini does not have that model; pick another one",
        429 => " — the key's quota is used up for now; wait, or use a key with more",
        _ => "",
    };

    AppError::General(format!("{what} returned {status}{hint}: {message}"))
}

// ═══════════════════════════════════════════════════════════════
//  PROVIDER
// ═══════════════════════════════════════════════════════════════

pub struct GeminiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl GeminiProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self::at(GEMINI_API, api_key)
    }

    /// The same, somewhere other than Google — which in practice means a test.
    pub fn at(base_url: &str, api_key: Option<String>) -> Self {
        Self {
            client: chat_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()),
        }
    }

    /// The key goes in a header, never in the URL.
    ///
    /// Google accepts `?key=` as well, and every example that uses it puts a
    /// credential in something that is logged, cached and shown in error
    /// messages — including this app's own, which quote the URL they failed
    /// on.
    fn authorize(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => req.header("x-goog-api-key", key),
            None => req,
        }
    }

    fn no_key() -> AppError {
        AppError::General(
            "Gemini needs an API key. Add one in Syn settings — aistudio.google.com issues them."
                .into(),
        )
    }

    async fn post(&self, req: &ChatRequest<'_>, stream: bool) -> AppResult<reqwest::Response> {
        if self.api_key.is_none() {
            return Err(Self::no_key());
        }

        let url = if stream {
            format!("{}/{}:streamGenerateContent?alt=sse", self.base_url, model_path(req.model))
        } else {
            format!("{}/{}:generateContent", self.base_url, model_path(req.model))
        };

        let resp = self
            .authorize(self.client.post(&url).json(&request_body(req)))
            .send()
            .await
            .map_err(|e| AppError::General(format!("Failed to reach Gemini: {e}")))?;

        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(explain(status, &body, "Gemini"))
    }
}

#[async_trait]
impl ChatProvider for GeminiProvider {
    fn id(&self) -> SynProvider {
        SynProvider::Gemini
    }

    /// Yes: a function call arrives whole inside a streamed chunk, so the tool
    /// loop can stream every round and the answer appears as it is written.
    fn streams_tool_calls(&self) -> bool {
        true
    }

    async fn check_status(&self) -> AppResult<ProviderStatus> {
        let status = |connected: bool| ProviderStatus {
            connected,
            version: None,
            url: self.base_url.clone(),
            supports_model_management: false,
        };

        // Without a key there is nothing to ask; the answer is known.
        if self.api_key.is_none() {
            return Ok(status(false));
        }

        // The cheapest call that proves both that Google is reachable and that
        // the key is good.
        let url = format!("{}/models?pageSize=1", self.base_url);
        match self.authorize(probe_client().get(&url)).send().await {
            Ok(resp) if resp.status().is_success() => Ok(status(true)),
            Ok(resp) => {
                log::warn!("Gemini answered /models with {}", resp.status());
                Ok(status(false))
            }
            Err(e) => {
                log::info!("Gemini not reachable: {e}");
                Ok(status(false))
            }
        }
    }

    /// The models that can answer a chat, as Gemini itself says.
    ///
    /// Filtered by `supportedGenerationMethods`, which is the one thing the
    /// OpenAI shape cannot offer and the reason the picker reads names there.
    /// Here the capability is stated, so a model that does not generate
    /// content — an embedding model, an image model on another method — is
    /// left out rather than guessed about.
    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        if self.api_key.is_none() {
            return Err(Self::no_key());
        }

        let mut found = Vec::new();
        let mut page: Option<String> = None;

        // Pages until Google stops handing out tokens, with a ceiling so a
        // server that always does cannot keep this going.
        for _ in 0..10 {
            let mut url = format!("{}/models?pageSize=1000", self.base_url);
            if let Some(token) = &page {
                url.push_str(&format!("&pageToken={}", urlencoding::encode(token)));
            }

            let resp = self
                .authorize(crate::syn::provider::catalogue_client().get(&url))
                .send()
                .await
                .map_err(|e| AppError::General(format!("Failed to reach Gemini: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(explain(status, &body, "Listing Gemini models"));
            }

            let listed: ModelsPage = resp
                .json()
                .await
                .map_err(|e| AppError::General(format!("Failed to read Gemini's model list: {e}")))?;

            found.extend(
                listed
                    .models
                    .into_iter()
                    .filter(|m| m.supported_generation_methods.iter().any(|g| g == "generateContent")),
            );

            match listed.next_page_token.filter(|t| !t.is_empty()) {
                Some(token) => page = Some(token),
                None => break,
            }
        }

        Ok(found
            .into_iter()
            .map(|m| {
                let id = m.name.trim_start_matches("models/").to_string();
                ModelInfo {
                    name: id.clone(),
                    model: id,
                    // Nothing is local, so there is no size and no digest.
                    size: 0,
                    digest: String::new(),
                    modified_at: String::new(),
                    details: None,
                }
            })
            .collect())
    }

    async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply> {
        let started = std::time::Instant::now();
        let resp = self.post(&req, false).await?;

        let body: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| AppError::General(format!("Failed to read Gemini's reply: {e}")))?;

        let mut assembly = Assembly::default();
        assembly.absorb(body);
        assembly.reply.duration_ms = Some(started.elapsed().as_millis() as u64);
        assembly.finish()
    }

    async fn chat_streaming(
        &self,
        req: ChatRequest<'_>,
        sink: &StreamSink<'_>,
    ) -> AppResult<ChatReply> {
        let started = std::time::Instant::now();
        let resp = self.post(&req, true).await?;

        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut assembly = Assembly::default();

        while let Some(chunk) = stream.next().await {
            if (sink.stop_requested)() {
                break;
            }
            let chunk = chunk.map_err(|e| AppError::General(format!("Gemini stream broke: {e}")))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            for payload in take_records(&mut buffer) {
                match serde_json::from_str::<GenerateResponse>(&payload) {
                    Ok(parsed) => {
                        let text = assembly.absorb(parsed);
                        if !text.is_empty() {
                            (sink.on_token)(&text);
                        }
                    }
                    // A record that will not parse costs a token, not the
                    // answer — the same tolerance the other providers have.
                    Err(e) => log::warn!("Unreadable Gemini chunk: {e} — raw: {payload}"),
                }
            }
        }

        assembly.reply.duration_ms = Some(started.elapsed().as_millis() as u64);
        assembly.finish()
    }
}

/// The complete `data:` payloads in the buffer, leaving any partial record.
///
/// Server-sent events: records end at a blank line. A chunk from the network
/// can end in the middle of one — or in the middle of a multi-byte character
/// in a Vietnamese answer — so only whole records are taken and the rest waits
/// for the next chunk.
fn take_records(buffer: &mut String) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(split) = buffer.find("\n\n").or_else(|| buffer.find("\r\n\r\n")) {
        let sep = if buffer[split..].starts_with("\r\n\r\n") { 4 } else { 2 };
        let record: String = buffer.drain(..split + sep).collect();
        let data: Vec<&str> = record
            .lines()
            .filter_map(|l| l.trim_end().strip_prefix("data:"))
            .map(str::trim)
            .collect();
        if !data.is_empty() {
            out.push(data.join("\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage::new(role, content)
    }

    fn call(id: Option<&str>, name: &str, signature: Option<&str>) -> ToolCall {
        ToolCall {
            id: id.map(str::to_string),
            function: ToolCallFunction { name: name.into(), arguments: json!({ "q": 1 }) },
            thought_signature: signature.map(str::to_string),
        }
    }

    fn request<'a>(messages: &'a [ChatMessage], model: &'a str) -> ChatRequest<'a> {
        ChatRequest { model, messages, temperature: Some(0.7), num_ctx: 8192, tools: None }
    }

    /// The one that matters. A call replayed without its signature is a 400
    /// on the next request — the second round of every question that uses a
    /// tool, which in Syn is most of them.
    #[test]
    fn a_signature_goes_back_on_the_part_it_came_in_on() {
        let mut asked = msg("assistant", "");
        asked.tool_calls = Some(vec![call(Some("c1"), "query_nodes", Some("SIG-abc"))]);
        let mut answered = msg("tool", "{\"total\":3}");
        answered.tool_call_id = Some("c1".into());

        let history = [msg("user", "tìm sách"), asked, answered];
        let body = request_body(&request(&history, "gemini-3.8-flash"));

        let part = &body["contents"][1]["parts"][0];
        assert_eq!(part["functionCall"]["name"], "query_nodes");
        assert_eq!(part["thoughtSignature"], "SIG-abc", "{body:#}");
    }

    /// And it comes out of a reply onto the call, so there is something to
    /// send back.
    #[test]
    fn a_signature_is_carried_out_of_the_reply() {
        let response: GenerateResponse = serde_json::from_value(json!({
            "candidates": [{ "content": { "parts": [
                { "functionCall": { "name": "query_nodes", "args": { "q": "x" } },
                  "thoughtSignature": "SIG-xyz" }
            ]}}]
        }))
        .unwrap();

        let mut a = Assembly::default();
        a.absorb(response);
        let reply = a.finish().unwrap();

        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].thought_signature.as_deref(), Some("SIG-xyz"));
        assert_eq!(reply.tool_calls[0].function.arguments, json!({ "q": "x" }));
    }

    /// Gemini has no system message and no `tool` role.
    #[test]
    fn roles_become_the_two_gemini_has() {
        let mut asked = msg("assistant", "để xem");
        asked.tool_calls = Some(vec![call(Some("c1"), "get_node", None)]);
        let mut answered = msg("tool", "{\"title\":\"A\"}");
        answered.tool_call_id = Some("c1".into());

        let history = [msg("system", "Bạn là Syn."), msg("user", "hỏi"), asked, answered];
        let body = request_body(&request(&history, "gemini-3.8-flash"));

        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "Bạn là Syn.");
        let roles: Vec<&str> =
            body["contents"].as_array().unwrap().iter().map(|c| c["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "model", "user"]);

        let response = &body["contents"][2]["parts"][0]["functionResponse"];
        assert_eq!(response["name"], "get_node", "the name is recovered from the call");
        assert_eq!(response["response"], json!({ "title": "A" }));
    }

    /// Results of parallel calls go back together, in the order asked, or
    /// Gemini cannot pair them.
    #[test]
    fn results_of_parallel_calls_arrive_in_one_turn() {
        let mut asked = msg("assistant", "");
        asked.tool_calls = Some(vec![
            call(Some("a"), "first", Some("SIG")),
            call(Some("b"), "second", None),
        ]);
        let mut one = msg("tool", "1");
        one.tool_call_id = Some("a".into());
        let mut two = msg("tool", "2");
        two.tool_call_id = Some("b".into());

        let history = [msg("user", "q"), asked, one, two];
        let body = request_body(&request(&history, "gemini-3.8-flash"));

        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3, "{body:#}");
        let answers = contents[2]["parts"].as_array().unwrap();
        assert_eq!(answers[0]["functionResponse"]["name"], "first");
        assert_eq!(answers[1]["functionResponse"]["name"], "second");
        // A bare number is not an object; it goes under `output`.
        assert_eq!(answers[0]["functionResponse"]["response"], json!({ "output": 1 }));
    }

    /// An id this file minted is for pairing inside Syn, and is never sent to
    /// Google, which did not issue it.
    #[test]
    fn a_minted_id_never_leaves_the_app() {
        let response: GenerateResponse = serde_json::from_value(json!({
            "candidates": [{ "content": { "parts": [
                { "functionCall": { "name": "query_nodes", "args": {} } }
            ]}}]
        }))
        .unwrap();
        let mut a = Assembly::default();
        a.absorb(response);
        let reply = a.finish().unwrap();
        let id = reply.tool_calls[0].id.clone().unwrap();
        assert!(id.starts_with(MINTED), "an id is needed to pair the result: {id}");

        let mut asked = msg("assistant", "");
        asked.tool_calls = Some(reply.tool_calls.clone());
        let mut answered = msg("tool", "{}");
        answered.tool_call_id = Some(id);
        let history = [msg("user", "q"), asked, answered];
        let body = request_body(&request(&history, "gemini-3.8-flash"));

        assert!(body["contents"][1]["parts"][0]["functionCall"].get("id").is_none());
        assert!(body["contents"][2]["parts"][0]["functionResponse"].get("id").is_none());
        assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["name"], "query_nodes");
    }

    /// "Requests ending with a model turn are not supported" — the 400 that
    /// emptied an answer the first time a run in this vault reached its
    /// ceiling. A history that ends on a call nobody answered goes out with
    /// the call answered as not run.
    #[test]
    fn a_request_never_ends_on_a_call_nobody_answered() {
        let mut asked = msg("assistant", "");
        asked.tool_calls = Some(vec![call(Some("c9"), "browse", Some("SIG"))]);
        let history = [msg("user", "giá đỉnh 52 tuần"), asked];
        let body = request_body(&request(&history, "gemini-3.8-flash"));

        let contents = body["contents"].as_array().unwrap();
        let last = contents.last().unwrap();
        assert_eq!(last["role"], "user", "{body:#}");
        assert_eq!(last["parts"][0]["functionResponse"]["name"], "browse");
        assert_eq!(last["parts"][0]["functionResponse"]["id"], "c9");
    }

    /// And a history that already ends properly is not touched.
    #[test]
    fn a_history_that_ends_on_a_result_is_left_as_it_is() {
        let mut asked = msg("assistant", "");
        asked.tool_calls = Some(vec![call(Some("c1"), "browse", None)]);
        let mut answered = msg("tool", "{}");
        answered.tool_call_id = Some("c1".into());
        let history = [msg("user", "q"), asked, answered];
        let body = request_body(&request(&history, "gemini-3.8-flash"));
        assert_eq!(body["contents"].as_array().unwrap().len(), 3);
    }

    /// Google's advice for Gemini 3 is to leave temperature at 1.0; lower
    /// values loop. Syn's default is 0.7.
    #[test]
    fn temperature_is_left_alone_on_gemini_3() {
        let history = [msg("user", "q")];
        assert!(request_body(&request(&history, "gemini-3.8-flash")).get("generationConfig").is_none());
        assert!(request_body(&request(&history, "gemini-4-pro")).get("generationConfig").is_none());
        assert_eq!(
            request_body(&request(&history, "models/gemini-2.5-flash"))["generationConfig"]["temperature"],
            0.7
        );
    }

    /// Tools cross as JSON Schema, untranslated.
    #[test]
    fn tools_are_declared_with_their_json_schema() {
        let tools = vec![ToolDefinition {
            tool_type: "function".into(),
            function: crate::models::syn::FunctionDefinition {
                name: "query_nodes".into(),
                description: "Search".into(),
                parameters: json!({ "type": "object", "properties": { "q": { "type": "string" } } }),
            },
        }];
        let history = [msg("user", "q")];
        let req = ChatRequest {
            model: "gemini-3.8-flash",
            messages: &history,
            temperature: None,
            num_ctx: 0,
            tools: Some(&tools),
        };
        let body = request_body(&req);
        let d = &body["tools"][0]["functionDeclarations"][0];
        assert_eq!(d["name"], "query_nodes");
        assert_eq!(d["parametersJsonSchema"]["properties"]["q"]["type"], "string");
        assert!(d.get("parameters").is_none(), "the OpenAPI subset is not used");
    }

    /// Thinking is billed as output. Copied field for field it would drop out
    /// of `charged`, and a reasoning model would look cheap.
    #[test]
    fn thinking_is_counted_as_output_and_as_hidden() {
        let usage: Usage = serde_json::from_value::<UsageMetadata>(json!({
            "promptTokenCount": 9000,
            "cachedContentTokenCount": 7000,
            "candidatesTokenCount": 300,
            "thoughtsTokenCount": 1200,
            "totalTokenCount": 10500
        }))
        .unwrap()
        .into();

        assert_eq!(usage.input, Some(9000));
        assert_eq!(usage.input_cached, Some(7000));
        assert_eq!(usage.output, Some(1500));
        assert_eq!(usage.output_hidden, Some(1200));
        assert_eq!(usage.charged(), Some(10500), "the same total Gemini reports");
    }

    /// A turn with no words and no calls is a refusal, and saying nothing would
    /// hide it.
    #[test]
    fn an_empty_refusal_is_said_rather_than_shown_as_silence() {
        let mut a = Assembly::default();
        a.absorb(serde_json::from_value(json!({
            "promptFeedback": { "blockReason": "SAFETY" }
        })).unwrap());
        assert!(a.finish().unwrap_err().to_string().contains("SAFETY"));

        let mut b = Assembly::default();
        b.absorb(serde_json::from_value(json!({
            "candidates": [{ "finishReason": "MALFORMED_FUNCTION_CALL", "content": { "parts": [] } }]
        })).unwrap());
        assert!(b.finish().unwrap_err().to_string().contains("MALFORMED_FUNCTION_CALL"));

        // A normal stop with words is an answer, whatever else is set.
        let mut c = Assembly::default();
        c.absorb(serde_json::from_value(json!({
            "candidates": [{ "finishReason": "STOP", "content": { "parts": [{ "text": "Xong." }] } }]
        })).unwrap());
        assert_eq!(c.finish().unwrap().content, "Xong.");
    }

    /// Thinking parts, if a model sends them, are not the answer.
    #[test]
    fn a_thought_is_not_part_of_the_answer() {
        let mut a = Assembly::default();
        a.absorb(serde_json::from_value(json!({
            "candidates": [{ "content": { "parts": [
                { "text": "let me think…", "thought": true },
                { "text": "Câu trả lời." }
            ]}}]
        })).unwrap());
        assert_eq!(a.finish().unwrap().content, "Câu trả lời.");
    }

    /// A network chunk can end in the middle of a record, or of a character.
    #[test]
    fn a_stream_is_read_in_whole_records_only() {
        let mut buffer = String::from("data: {\"a\":1}\n\ndata: {\"b\"");
        assert_eq!(take_records(&mut buffer), vec!["{\"a\":1}".to_string()]);
        assert_eq!(buffer, "data: {\"b\"");

        buffer.push_str(":2}\r\n\r\n");
        assert_eq!(take_records(&mut buffer), vec!["{\"b\":2}".to_string()]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn a_model_id_becomes_one_path_segment() {
        assert_eq!(model_path("gemini-3.8-flash"), "models/gemini-3.8-flash");
        assert_eq!(model_path("models/gemini-3.8-flash"), "models/gemini-3.8-flash");
    }

    #[test]
    fn an_error_says_what_google_said() {
        let e = explain(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":{"code":400,"message":"API key not valid. Please pass a valid API key.","status":"INVALID_ARGUMENT"}}"#,
            "Gemini",
        );
        let said = e.to_string();
        assert!(said.contains("API key not valid"), "{said}");
        assert!(said.contains("check the API key"), "{said}");
        assert!(!said.contains("\"code\""), "the JSON is not shown: {said}");
    }

    // ── against a server ──────────────────────────────────────────

    /// A server that answers each request with the next canned body and keeps
    /// what it was sent, so a test can read the second request of a tool loop.
    async fn serve(
        replies: Vec<(&'static str, String)>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let kept = seen.clone();

        tokio::spawn(async move {
            for (content_type, body) in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut raw = Vec::new();
                let mut buf = [0u8; 8192];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    raw.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&raw).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length").then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if raw.len() >= end + 4 + length {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                kept.lock().unwrap().push(String::from_utf8_lossy(&raw).to_string());
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.ok();
            }
        });

        (format!("http://{addr}/v1beta"), seen)
    }

    /// The whole reason this provider exists, end to end: the first round of a
    /// tool loop streams back a signed call, and the second round sends the
    /// signature back on it — with the key in a header, never the URL.
    #[tokio::test]
    async fn a_tool_loop_carries_the_signature_into_the_next_request() {
        let first = [
            r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"query_nodes","args":{"q":"splunk"}},"thoughtSignature":"SIG-round-1"}]}}]}"#,
            r#"data: {"candidates":[{"finishReason":"STOP","content":{"parts":[]}}],"usageMetadata":{"promptTokenCount":900,"candidatesTokenCount":20,"thoughtsTokenCount":80}}"#,
        ]
        .join("\n\n")
            + "\n\n";
        let second = r#"data: {"candidates":[{"content":{"parts":[{"text":"Có 3 note về Splunk."}]},"finishReason":"STOP"}]}"#.to_string() + "\n\n";

        let (base, seen) = serve(vec![
            ("text/event-stream", first),
            ("text/event-stream", second),
        ])
        .await;
        let provider = GeminiProvider::at(&base, Some("KEY-123".into()));

        let tokens = std::sync::Mutex::new(String::new());
        let on_token = |t: &str| tokens.lock().unwrap().push_str(t);
        let never = || false;
        let sink = StreamSink { on_token: &on_token, stop_requested: &never };

        let mut history = vec![msg("system", "Bạn là Syn."), msg("user", "tìm note về splunk")];
        let one = provider
            .chat_streaming(request(&history, "gemini-3.8-flash"), &sink)
            .await
            .expect("round one");

        assert_eq!(one.tool_calls.len(), 1);
        assert_eq!(one.tool_calls[0].thought_signature.as_deref(), Some("SIG-round-1"));
        assert_eq!(one.usage.output, Some(100));

        let mut asked = msg("assistant", &one.content);
        asked.tool_calls = Some(one.tool_calls.clone());
        let mut answered = msg("tool", "{\"total\":3}");
        answered.tool_call_id = one.tool_calls[0].id.clone();
        history.push(asked);
        history.push(answered);

        let two = provider
            .chat_streaming(request(&history, "gemini-3.8-flash"), &sink)
            .await
            .expect("round two");
        assert_eq!(two.content, "Có 3 note về Splunk.");
        assert_eq!(tokens.lock().unwrap().as_str(), "Có 3 note về Splunk.");

        let requests = seen.lock().unwrap().clone();
        assert_eq!(requests.len(), 2);

        let (head, body) = requests[1].split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("POST /v1beta/models/gemini-3.8-flash:streamGenerateContent?alt=sse"), "{head}");
        assert!(head.to_lowercase().contains("x-goog-api-key: key-123"), "{head}");
        assert!(!head.contains("key=KEY-123"), "the key is never in the URL");

        let sent: Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["contents"][1]["parts"][0]["thoughtSignature"], "SIG-round-1", "{sent:#}");
        assert_eq!(sent["contents"][2]["parts"][0]["functionResponse"]["name"], "query_nodes");
    }

    /// Only models that can generate content are offered, and by their id.
    #[tokio::test]
    async fn the_model_list_is_what_can_chat() {
        let page = json!({
            "models": [
                { "name": "models/gemini-3.8-flash", "supportedGenerationMethods": ["generateContent", "countTokens"] },
                { "name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"] }
            ]
        })
        .to_string();
        let (base, _) = serve(vec![("application/json", page)]).await;

        let models = GeminiProvider::at(&base, Some("K".into())).list_models().await.unwrap();
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["gemini-3.8-flash"]);
    }

    /// Without a key there is nothing to ask, and the message says where one
    /// comes from.
    #[tokio::test]
    async fn no_key_is_said_plainly_and_nothing_is_sent() {
        let p = GeminiProvider::at("http://127.0.0.1:9", None);
        assert!(!p.check_status().await.unwrap().connected);
        let e = p.list_models().await.unwrap_err().to_string();
        assert!(e.contains("API key"), "{e}");
    }
}
