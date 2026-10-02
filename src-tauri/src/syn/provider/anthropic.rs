//! Claude, spoken to in Anthropic's own Messages API.
//!
//! # Why not through the OpenAI-compatible provider
//!
//! Anthropic does serve an OpenAI-shaped endpoint, and pointing
//! `OpenAiCompatProvider` at it answers the first question. What it cannot do
//! is the two things that make Claude worth choosing for a tool loop:
//!
//! - **Prompt caching.** Every round of Syn's loop re-sends the system prompt
//!   and nearly sixteen thousand characters of tool declarations. `cache_control`
//!   marks that prefix, and from the second round on it is read back at a tenth
//!   of the price instead of paid for again. The compatible endpoint has no
//!   field to say it with.
//! - **Thinking across a tool call.** Current Claude models think before they
//!   act, and a thinking block has to come back — unchanged, signature and all
//!   — ahead of the `tool_use` it led to, or the next round loses the reasoning
//!   that asked for the tool. There is nowhere to put one in the OpenAI shape.
//!
//! # What is different from the other three, and handled only here
//!
//! 1. **The system prompt is top-level `system`**, a block array, and the only
//!    roles in `messages` are `user` and `assistant`.
//! 2. **A tool result is a `tool_result` block in a `user` turn.** Results of
//!    parallel calls go back in *one* user turn — splitting them across several
//!    teaches the model to stop making parallel calls — and every `tool_use`
//!    must be answered in the turn right after it, or the request is a 400.
//! 3. **Ids are strict.** A `tool_use` id must match `^[a-zA-Z0-9_-]+$`. An id
//!    another provider minted is cleaned to fit, the same way on both sides so
//!    the pair still matches; a call with none, from an Ollama-era history, is
//!    given one, and its result is paired by position — the way Ollama pairs.
//! 4. **Temperature is not sent to current models.** Opus 4.7 onwards, Sonnet
//!    5, Fable and every Claude after them refuse sampling parameters with a
//!    400. See `sends_temperature`.
//! 5. **Thinking blocks ride on the first tool call.** See "Thinking" below.
//!
//! # Caching
//!
//! Three breakpoints, of the four allowed: on the last tool declaration, on
//! the system prompt, and on the last block of the conversation. The cache is
//! a prefix match in the order tools → system → messages, so the first caches
//! the declarations, the second those plus the prompt, and the third the whole
//! conversation so far — which is what makes round five of a tool loop cheap:
//! everything before round five's new tool result is read, not paid for. The
//! fourth breakpoint is left free for a caller that wants one.
//!
//! # Thinking
//!
//! `thinking` is **not sent**, and that is a decision rather than an omission.
//! The current models — Opus 5 and later, Sonnet 5, Fable — think adaptively by
//! default, and Opus 5.5 and Fable refuse a request that tries to switch it
//! off. Older models that need an explicit `{type: "adaptive"}` or a
//! `budget_tokens` would each need their own rule, and the app has no setting
//! for any of it: `openai_reasoning_effort` belongs to the OpenAI shape and
//! means something else there. So each model runs as its maker ships it. A
//! setting for `output_config.effort` is the natural next step, and this file is
//! where it would go.
//!
//! What cannot be left to the default is **replay**. When a model thinks and
//! then calls a tool, the thinking block has to come back in that assistant
//! turn, before the call, or the reasoning is lost — and on the newest models
//! the block is bound to the conversation that produced it. `ChatMessage` has
//! no field for it and `engine.rs` is not this file's to change, so it travels
//! the way Gemini's signature does: in `ToolCall::thought_signature` on the
//! first call of the turn, as the whole assistant turn exactly as Anthropic
//! sent it, prefixed with `TURN_CARRIER`. Replaying it verbatim keeps the
//! order of thinking, text and calls byte for byte, which is what the binding
//! check compares.
//!
//! Two ways that can still go wrong, and what happens:
//!
//! - **The turn was changed on the way back** — a call dropped, or a call
//!   resumed on its own after a permission card. The carried turn is used only
//!   if its calls, by id and in order, are still the message's calls;
//!   otherwise the thinking is left out and the turn is rebuilt from the
//!   message, which every model accepts. When the calls match, the carried
//!   turn wins over the message's text as well: it is what the model actually
//!   said, and the binding check compares it byte for byte.
//! - **The history before it was changed** — a result shortened, a round
//!   summarised. The API refuses the thinking block with a 400 naming it. The
//!   request is sent once more with every thinking block removed, which is the
//!   recovery Anthropic documents; the model answers that round without the
//!   reasoning it had carried.
//!
//! # Structured output
//!
//! `ChatRequest::json_schema` becomes `output_config.format` — Anthropic's
//! constrained decoding, which holds the reply to the schema rather than
//! asking nicely. It requires `additionalProperties: false` on every object,
//! and a schema written for Ollama's `format` rarely says so; it is added
//! where absent, which is what a schema for a reply meant anyway. A schema the
//! API still will not take is a 400, and the callers that set one already ask
//! again without it. A forced tool was the older way to get JSON out of
//! Claude and is not used: forced `tool_choice` is a 400 on Opus 5.5 and Fable.

use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::error::{AppError, AppResult};
use crate::models::syn::{
    ModelInfo, ProviderStatus, SynProvider, ToolCall, ToolCallFunction, ToolDefinition,
};
use crate::syn::provider::retry;
use crate::syn::provider::{
    chat_client, probe_client, ChatMessage, ChatProvider, ChatReply, ChatRequest, StreamSink,
    Usage,
};

/// Where the Anthropic API lives. Paths — `/v1/messages` — are added to it.
pub const ANTHROPIC_API: &str = "https://api.anthropic.com";

/// The API version every request declares.
pub const API_VERSION: &str = "2023-06-01";

/// The model offered first when a vault has not chosen one.
///
/// Anthropic's recommendation for general use. The model list comes back
/// newest first, and the newest is not the one to start somebody on — it is
/// often the most expensive tier — so this is moved to the front of the list
/// the picker falls back to. Somebody who wants another picks it once, and
/// `default_models` remembers it.
pub const DEFAULT_MODEL: &str = "claude-opus-5";

/// What marks a `thought_signature` as an Anthropic assistant turn.
///
/// Gemini carries its own signature in the same field. Each provider reads only
/// its own, so a conversation that moved between them sends neither the
/// other's.
pub const TURN_CARRIER: &str = "anthropic-turn:";

/// How much the model may write, streaming and not.
///
/// Required by the API. Generous when streaming — a thinking model spends some
/// of it before writing anything, and a stream only times out when it goes
/// quiet (`CHAT_SILENCE`), however long it runs — and half that when waiting
/// for the whole reply at once, which has to arrive inside one silence. Both fit every Claude this app can reach.
const MAX_TOKENS_STREAMING: u32 = 32_000;
const MAX_TOKENS_WAITING: u32 = 16_000;

/// The prefix on a call id this file made up, for a call that had none.
const MINTED: &str = "syn_call_";

/// What a call that was never run is answered with.
const NOT_RUN: &str = "Not run: the work stopped before this call was made.";

// ═══════════════════════════════════════════════════════════════
//  WIRE TYPES — what comes back
// ═══════════════════════════════════════════════════════════════

#[derive(Deserialize)]
struct MessageResponse {
    #[serde(default)]
    content: Vec<Value>,
    stop_reason: Option<String>,
    usage: Option<WireUsage>,
}

/// What a turn cost, in Anthropic's words.
///
/// Every field optional because a stream reports them in two halves: input on
/// `message_start`, output on `message_delta`.
#[derive(Deserialize, Default, Clone, Copy)]
struct WireUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

impl WireUsage {
    /// Take whatever a later report says, keeping what it does not.
    fn merge(&mut self, later: WireUsage) {
        self.input_tokens = later.input_tokens.or(self.input_tokens);
        self.output_tokens = later.output_tokens.or(self.output_tokens);
        self.cache_creation_input_tokens =
            later.cache_creation_input_tokens.or(self.cache_creation_input_tokens);
        self.cache_read_input_tokens = later.cache_read_input_tokens.or(self.cache_read_input_tokens);
    }
}

impl From<WireUsage> for Usage {
    /// Normalised so that `input` means everything sent, as it does for every
    /// other provider.
    ///
    /// Anthropic's `input_tokens` is only the part after the last cache
    /// breakpoint — the uncached remainder. The prefix is reported beside it,
    /// in two fields: `cache_read_input_tokens`, served from the cache at a
    /// tenth of the price, and `cache_creation_input_tokens`, written to it at a
    /// quarter more than the price. Copied across as it stands, a fully cached
    /// turn would read as a turn of a few hundred tokens, and the window check
    /// the engine is growing would never fire.
    ///
    /// So all three are added for `input`; the reads are `input_cached`; and the
    /// writes stay inside `input` without a field of their own, because they are
    /// what the cache costs on the first round and the point is the rounds
    /// after. Thinking is inside `output_tokens` and not reported apart, so
    /// `output_hidden` is silence, not zero.
    fn from(u: WireUsage) -> Self {
        let parts = [u.input_tokens, u.cache_creation_input_tokens, u.cache_read_input_tokens];
        let input = parts
            .iter()
            .any(Option::is_some)
            .then(|| parts.iter().map(|p| p.unwrap_or(0)).sum());
        Usage {
            input,
            input_cached: u.cache_read_input_tokens,
            output: u.output_tokens,
            output_hidden: None,
        }
    }
}

/// One server-sent event.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    MessageStart {
        message: StartedMessage,
    },
    ContentBlockStart {
        index: usize,
        content_block: Value,
    },
    ContentBlockDelta {
        index: usize,
        delta: Delta,
    },
    MessageDelta {
        #[serde(default)]
        delta: MessageDeltaBody,
        usage: Option<WireUsage>,
    },
    Error {
        error: WireError,
    },
    /// `content_block_stop`, `message_stop`, `ping`, and whatever is added
    /// next. Nothing to do for any of them.
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct StartedMessage {
    usage: Option<WireUsage>,
}

#[derive(Deserialize, Default)]
struct MessageDeltaBody {
    stop_reason: Option<String>,
}

/// One piece of a content block. The wire names are `text_delta`,
/// `input_json_delta` and so on; the suffix is dropped here.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum Delta {
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "input_json_delta")]
    InputJson { partial_json: String },
    #[serde(rename = "thinking_delta")]
    Thinking { thinking: String },
    #[serde(rename = "signature_delta")]
    Signature { signature: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct WireError {
    #[serde(rename = "type")]
    kind: String,
    message: String,
}

#[derive(Deserialize)]
struct ModelsPage {
    #[serde(default)]
    data: Vec<ModelEntry>,
    #[serde(default)]
    has_more: bool,
    last_id: Option<String>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
    display_name: Option<String>,
    created_at: Option<String>,
}

// ═══════════════════════════════════════════════════════════════
//  CONVERSION — what goes out
// ═══════════════════════════════════════════════════════════════

/// An id Anthropic will accept: `^[a-zA-Z0-9_-]+$`.
///
/// Applied to both halves of a pair — the `tool_use` and the `tool_result` —
/// so two ids that matched before still match after.
fn clean_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

/// Whether to send the user's temperature at all.
///
/// Only to models known to take it: Claude 3, Haiku 4.5, Sonnet before 5 and
/// Opus before 4.7. From Opus 4.7 and Sonnet 5 on — and on every Fable —
/// sampling parameters are a 400, and Syn sends 0.7 by default. Decided by
/// what the model is known to accept rather than what it is known to refuse,
/// so the next Claude is left alone by default, as Gemini 4 is in `gemini`.
fn sends_temperature(model: &str) -> bool {
    let id = model.trim().to_ascii_lowercase();
    let Some(rest) = id.strip_prefix("claude-") else { return false };
    if rest.starts_with('3') {
        return true;
    }
    let (line, after) = rest.split_once('-').unwrap_or((rest, ""));
    let v: Vec<u32> = after
        .split(['-', '.'])
        .map_while(|p| p.parse::<u32>().ok().filter(|n| *n < 1000))
        .collect();
    let version = (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0));
    match line {
        "opus" => version < (4, 7),
        "sonnet" | "haiku" => version < (5, 0),
        _ => false,
    }
}

/// The media type and payload of an image, whichever form it was kept in.
fn image_block(img: &str) -> Value {
    let (media_type, data) = match img.strip_prefix("data:").and_then(|rest| rest.split_once(";base64,")) {
        Some((declared, data)) => (declared.to_string(), data),
        None => (crate::syn::provider::media_type_of(img).to_string(), img),
    };
    json!({ "type": "image", "source": { "type": "base64", "media_type": media_type, "data": data } })
}

/// Text and images for a user turn. Images first: Claude reads a picture best
/// when the question about it comes after.
fn user_blocks(m: &ChatMessage) -> Vec<Value> {
    let mut blocks: Vec<Value> = m.images.iter().flatten().map(|i| image_block(i)).collect();
    if !m.content.trim().is_empty() {
        blocks.push(json!({ "type": "text", "text": m.content }));
    }
    blocks
}

/// The assistant turn this file carried out of a reply, if the message still
/// has it.
fn carried_turn(m: &ChatMessage) -> Option<Vec<Value>> {
    let first = m.tool_calls.as_ref()?.first()?;
    let raw = first.thought_signature.as_deref()?.strip_prefix(TURN_CARRIER)?;
    serde_json::from_str(raw).ok()
}

/// Blocks for an assistant message, and the ids of the calls in it.
///
/// A turn carried out of a reply is sent back exactly as it came, when its
/// calls are still this message's calls. Otherwise it is rebuilt — text, then
/// calls — and whatever thinking there was is left out, because a thinking
/// block in front of calls it did not lead to is worse than none.
fn assistant_blocks(
    m: &ChatMessage,
    minted: &mut usize,
    keep_thinking: bool,
) -> (Vec<Value>, Vec<String>) {
    let calls = m.tool_calls.as_deref().unwrap_or_default();

    let ids: Vec<String> = calls
        .iter()
        .map(|c| match c.id.as_deref().map(clean_id).filter(|id| !id.is_empty()) {
            Some(id) => id,
            None => {
                *minted += 1;
                format!("{MINTED}{minted}")
            }
        })
        .collect();

    if let Some(carried) = carried_turn(m) {
        let carried_ids: Vec<String> = carried
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .filter_map(|b| b["id"].as_str().map(clean_id))
            .collect();
        if carried_ids == ids {
            let blocks = carried
                .into_iter()
                .filter(|b| keep_thinking || !is_thinking(b))
                .collect();
            return (blocks, ids);
        }
    }

    let mut blocks = Vec::new();
    if !m.content.trim().is_empty() {
        blocks.push(json!({ "type": "text", "text": m.content }));
    }
    for (call, id) in calls.iter().zip(&ids) {
        let input = match &call.function.arguments {
            Value::Object(_) => call.function.arguments.clone(),
            // A string holding JSON, from a history the OpenAI shape wrote.
            Value::String(s) => serde_json::from_str::<Value>(s)
                .ok()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({ "input": s })),
            Value::Null => json!({}),
            other => json!({ "input": other }),
        };
        blocks.push(json!({ "type": "tool_use", "id": id, "name": call.function.name, "input": input }));
    }
    (blocks, ids)
}

/// A tool block as plain text, for a request that declares no tools.
fn as_text(block: Value) -> Value {
    match block["type"].as_str() {
        Some("tool_use") => json!({
            "type": "text",
            "text": format!("[Called `{}` with {}]", block["name"].as_str().unwrap_or("a tool"), block["input"]),
        }),
        Some("tool_result") => {
            let said = match &block["content"] {
                Value::String(text) => text.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|p| p["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
                other => other.to_string(),
            };
            json!({ "type": "text", "text": format!("[Result]\n{}", text_or_placeholder(&said)) })
        }
        _ => block,
    }
}

fn is_thinking(block: &Value) -> bool {
    matches!(block["type"].as_str(), Some("thinking" | "redacted_thinking"))
}

/// A tool result, with any images it brought.
fn result_block(id: &str, m: &ChatMessage) -> Value {
    let content = match m.images.as_deref() {
        Some(images) if !images.is_empty() => {
            let mut parts = vec![json!({ "type": "text", "text": text_or_placeholder(&m.content) })];
            parts.extend(images.iter().map(|i| image_block(i)));
            Value::Array(parts)
        }
        _ => Value::String(text_or_placeholder(&m.content)),
    };
    json!({ "type": "tool_result", "tool_use_id": id, "content": content })
}

/// A tool that answered nothing still answered; an empty string is a 400.
fn text_or_placeholder(text: &str) -> String {
    if text.trim().is_empty() { "(no output)".to_string() } else { text.to_string() }
}

/// One turn being put together.
struct Turn {
    role: &'static str,
    blocks: Vec<Value>,
}

/// Add blocks to the conversation, merging into the last turn if it is the
/// same role. Results of parallel calls have to share one user turn.
fn push(turns: &mut Vec<Turn>, role: &'static str, blocks: Vec<Value>) {
    if blocks.is_empty() {
        return;
    }
    match turns.last_mut() {
        Some(last) if last.role == role => last.blocks.extend(blocks),
        _ => turns.push(Turn { role, blocks }),
    }
}

/// Answer every call still waiting, in the turn right after it.
///
/// Anthropic refuses a request in which a `tool_use` is not answered by the
/// next user turn. The engine answers any call it did not get to, and this is
/// the same promise kept at the last place it can be — for a history that ends
/// on calls, or that went on past them.
fn answer_waiting(turns: &mut Vec<Turn>, waiting: &mut Vec<String>) {
    if waiting.is_empty() {
        return;
    }
    let answers: Vec<Value> = waiting
        .drain(..)
        .map(|id| json!({ "type": "tool_result", "tool_use_id": id, "content": NOT_RUN }))
        .collect();
    match turns.last_mut() {
        // Results go first in a user turn, so these go after any already there
        // and before anything else.
        Some(last) if last.role == "user" => {
            let at = last.blocks.iter().take_while(|b| b["type"] == "tool_result").count();
            last.blocks.splice(at..at, answers);
        }
        _ => turns.push(Turn { role: "user", blocks: answers }),
    }
}

/// `additionalProperties: false` on every object that does not say otherwise.
///
/// Structured output requires it, and a schema written for Ollama's `format`
/// rarely states it. For the shape of a reply it is what was meant anyway.
fn closed(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out: Map<String, Value> =
                map.iter().map(|(k, v)| (k.clone(), closed(v))).collect();
            let is_object = out.get("type").is_some_and(|t| t == "object") || out.contains_key("properties");
            if is_object && !out.contains_key("additionalProperties") {
                out.insert("additionalProperties".into(), Value::Bool(false));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(closed).collect()),
        other => other.clone(),
    }
}

/// The request body, from the messages the tool loop holds.
///
/// `keep_thinking` is false only on the one retry after the API refused a
/// replayed thinking block. See the module docs.
fn request_body(req: &ChatRequest<'_>, stream: bool, keep_thinking: bool) -> Value {
    let mut system: Vec<&str> = Vec::new();
    let mut turns: Vec<Turn> = Vec::new();
    let mut waiting: Vec<String> = Vec::new();
    let mut minted = 0usize;
    let mut past_the_top = false;

    for m in req.messages {
        if m.role == "system" && !past_the_top {
            if !m.content.trim().is_empty() {
                system.push(&m.content);
            }
            continue;
        }
        past_the_top = true;

        match m.role.as_str() {
            "assistant" => {
                // Calls from the turn before that nobody answered are answered
                // now, before another assistant turn can follow them.
                answer_waiting(&mut turns, &mut waiting);
                let (blocks, ids) = assistant_blocks(m, &mut minted, keep_thinking);
                push(&mut turns, "assistant", blocks);
                waiting = ids;
            }
            "tool" => {
                let named = m.tool_call_id.as_deref().map(clean_id);
                // By id when it names a waiting call; otherwise the next one
                // waiting, which is how Ollama pairs and how a history with no
                // ids was written.
                let at = named
                    .as_ref()
                    .and_then(|id| waiting.iter().position(|w| w == id))
                    .or_else(|| (!waiting.is_empty()).then_some(0));
                match at {
                    Some(at) => {
                        let id = waiting.remove(at);
                        push(&mut turns, "user", vec![result_block(&id, m)]);
                    }
                    // A result for a call this history does not show. As a
                    // `tool_result` it would be a 400; as text it is still read.
                    None => push(
                        &mut turns,
                        "user",
                        vec![json!({ "type": "text", "text": format!("Result of an earlier tool call: {}", text_or_placeholder(&m.content)) })],
                    ),
                }
            }
            _ => {
                answer_waiting(&mut turns, &mut waiting);
                push(&mut turns, "user", user_blocks(m));
            }
        }
    }
    answer_waiting(&mut turns, &mut waiting);

    // Sent without tools — the last round after a ceiling, or a compaction —
    // the history still holds the loop's calls and their results, and the API
    // refuses `tool_use` or `tool_result` blocks in a request that declares no
    // tools. So they go as what they were: text saying what was called and
    // what came back. Thinking goes with them, since what it led to is no
    // longer a call.
    if req.tools.is_none_or(|t| t.is_empty()) {
        for turn in &mut turns {
            turn.blocks = std::mem::take(&mut turn.blocks)
                .into_iter()
                .filter(|b| !is_thinking(b))
                .map(as_text)
                .collect();
        }
    }

    // The third breakpoint: everything so far, so the next round of the loop
    // reads it back instead of paying for it again. Not on a thinking block,
    // which cannot carry one.
    if let Some(last) = turns.last_mut().and_then(|t| t.blocks.iter_mut().rev().find(|b| !is_thinking(b))) {
        if let Some(block) = last.as_object_mut() {
            block.insert("cache_control".into(), json!({ "type": "ephemeral" }));
        }
    }

    let mut body = Map::new();
    body.insert("model".into(), json!(req.model));
    body.insert(
        "max_tokens".into(),
        json!(if stream { MAX_TOKENS_STREAMING } else { MAX_TOKENS_WAITING }),
    );
    body.insert(
        "messages".into(),
        Value::Array(
            turns
                .into_iter()
                .map(|t| json!({ "role": t.role, "content": t.blocks }))
                .collect(),
        ),
    );
    if stream {
        body.insert("stream".into(), json!(true));
    }

    if !system.is_empty() {
        body.insert(
            "system".into(),
            json!([{ "type": "text", "text": system.join("\n\n"), "cache_control": { "type": "ephemeral" } }]),
        );
    }

    if let Some(tools) = req.tools.filter(|t| !t.is_empty()) {
        body.insert("tools".into(), Value::Array(declarations(tools)));
    }

    if let Some(t) = req.temperature.filter(|_| sends_temperature(req.model)) {
        body.insert("temperature".into(), json!(t));
    }

    if let Some(schema) = req.json_schema {
        body.insert(
            "output_config".into(),
            json!({ "format": { "type": "json_schema", "schema": closed(schema) } }),
        );
    }

    Value::Object(body)
}

/// Syn's tool declarations as Claude reads them, with the first breakpoint on
/// the last one — so the declarations, which never change within a vault, are
/// the start of the cached prefix.
fn declarations(tools: &[ToolDefinition]) -> Vec<Value> {
    let mut out: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.function.name,
                "description": t.function.description,
                "input_schema": t.function.parameters,
            })
        })
        .collect();
    if let Some(Value::Object(last)) = out.last_mut() {
        last.insert("cache_control".into(), json!({ "type": "ephemeral" }));
    }
    out
}

// ═══════════════════════════════════════════════════════════════
//  READING A REPLY
// ═══════════════════════════════════════════════════════════════

/// One content block as it arrives.
enum Block {
    Text(String),
    Thinking { thinking: String, signature: String },
    Redacted(String),
    ToolUse { id: String, name: String, input: Value, json: String },
    /// Anything else — a server tool's block, a type added later. Kept so the
    /// indices of the rest stay right, and otherwise ignored.
    Other,
}

impl Block {
    fn from_start(block: &Value) -> Block {
        let s = |k: &str| block[k].as_str().unwrap_or_default().to_string();
        match block["type"].as_str() {
            Some("text") => Block::Text(s("text")),
            Some("thinking") => Block::Thinking { thinking: s("thinking"), signature: s("signature") },
            Some("redacted_thinking") => Block::Redacted(s("data")),
            Some("tool_use") => Block::ToolUse {
                id: s("id"),
                name: s("name"),
                input: block.get("input").cloned().unwrap_or(Value::Null),
                json: String::new(),
            },
            _ => Block::Other,
        }
    }

    /// The call's input: the streamed JSON when there was any, else what the
    /// block started with. Malformed JSON reaches the tool as a string it can
    /// reject, the way the OpenAI shape handles it — not as a dropped call.
    fn input(input: &Value, json: &str) -> Value {
        if json.trim().is_empty() {
            return match input {
                Value::Null => json!({}),
                other => other.clone(),
            };
        }
        serde_json::from_str(json).unwrap_or_else(|_| Value::String(json.to_string()))
    }

    /// As the API would want it back.
    fn to_wire(&self) -> Option<Value> {
        match self {
            Block::Text(text) if !text.trim().is_empty() => Some(json!({ "type": "text", "text": text })),
            Block::Text(_) | Block::Other => None,
            Block::Thinking { thinking, signature } => {
                Some(json!({ "type": "thinking", "thinking": thinking, "signature": signature }))
            }
            Block::Redacted(data) => Some(json!({ "type": "redacted_thinking", "data": data })),
            Block::ToolUse { id, name, input, json } => {
                Some(json!({ "type": "tool_use", "id": id, "name": name, "input": Block::input(input, json) }))
            }
        }
    }
}

/// A reply being put together, from one response or from a stream.
#[derive(Default)]
struct Assembly {
    blocks: Vec<(usize, Block)>,
    usage: WireUsage,
    stop_reason: Option<String>,
    duration_ms: Option<u64>,
}

impl Assembly {
    fn start(&mut self, index: usize, block: &Value) {
        self.blocks.push((index, Block::from_start(block)));
    }

    /// Take in one delta. Returns text for the sink, if any.
    fn delta(&mut self, index: usize, delta: Delta) -> Option<String> {
        let (_, block) = self.blocks.iter_mut().find(|(i, _)| *i == index)?;
        match (block, delta) {
            (Block::Text(text), Delta::Text { text: more }) => {
                text.push_str(&more);
                (!more.is_empty()).then_some(more)
            }
            (Block::ToolUse { json, .. }, Delta::InputJson { partial_json }) => {
                json.push_str(&partial_json);
                None
            }
            (Block::Thinking { thinking, .. }, Delta::Thinking { thinking: more }) => {
                thinking.push_str(&more);
                None
            }
            (Block::Thinking { signature, .. }, Delta::Signature { signature: more }) => {
                signature.push_str(&more);
                None
            }
            _ => None,
        }
    }

    /// Take in a whole response, as the non-streaming call returns it.
    fn whole(response: MessageResponse) -> Assembly {
        let mut a = Assembly {
            usage: response.usage.unwrap_or_default(),
            stop_reason: response.stop_reason,
            ..Default::default()
        };
        for (i, block) in response.content.iter().enumerate() {
            a.start(i, block);
        }
        a
    }

    /// The reply, or the reason there is none.
    fn finish(mut self) -> AppResult<ChatReply> {
        self.blocks.sort_by_key(|(i, _)| *i);

        let content: String = self
            .blocks
            .iter()
            .filter_map(|(_, b)| match b { Block::Text(t) => Some(t.as_str()), _ => None })
            .collect();

        let thought = self.blocks.iter().any(|(_, b)| matches!(b, Block::Thinking { .. } | Block::Redacted(_)));

        let mut tool_calls: Vec<ToolCall> = self
            .blocks
            .iter()
            .filter_map(|(_, b)| match b {
                Block::ToolUse { id, name, input, json } => Some(ToolCall {
                    id: Some(id.clone()),
                    function: ToolCallFunction { name: name.clone(), arguments: Block::input(input, json) },
                    thought_signature: None,
                }),
                _ => None,
            })
            .collect();

        // The turn rides on the first call, so it can come back as it was. Only
        // when there was thinking: without it, rebuilding the turn from the
        // message is exact enough.
        if thought {
            if let Some(first) = tool_calls.first_mut() {
                let wire: Vec<Value> = self.blocks.iter().filter_map(|(_, b)| b.to_wire()).collect();
                first.thought_signature =
                    Some(format!("{TURN_CARRIER}{}", Value::Array(wire)));
            }
        }

        if content.trim().is_empty() && tool_calls.is_empty() {
            if let Some(reason) = self.stop_reason.as_deref().filter(|r| *r != "end_turn") {
                let why = match reason {
                    "refusal" => "Claude declined to answer this".to_string(),
                    "max_tokens" => "Claude ran out of room before writing an answer".to_string(),
                    other => format!("Claude stopped without answering ({other})"),
                };
                return Err(AppError::General(why));
            }
        }

        Ok(ChatReply { content, tool_calls, usage: self.usage.into(), duration_ms: self.duration_ms })
    }
}

/// Turn a non-2xx into a sentence worth reading.
///
/// Anthropic's error bodies are `{"type": "error", "error": {"type": …,
/// "message": …}}`; the message is lifted out rather than showing the JSON.
fn explain(status: reqwest::StatusCode, body: &str, what: &str) -> AppError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| body.to_string());

    let hint = match status.as_u16() {
        401 => " — check the API key in Syn settings",
        403 => " — this key is not allowed to do that",
        402 => " — the account needs credit; see console.anthropic.com",
        400 if message.contains("credit balance") => " — the account needs credit; see console.anthropic.com",
        404 => " — Anthropic does not have that model, or not for this key; pick another one",
        429 => " — the key's rate limit is used up for now; wait a moment",
        529 => " — Anthropic is overloaded right now; try again shortly",
        _ => "",
    };

    AppError::General(format!("{what} returned {status}{hint}: {message}"))
}

/// Whether a refusal is about a replayed thinking block, which a request with
/// the thinking taken out would not have.
fn refused_the_thinking(error: &AppError) -> bool {
    let said = error.to_string();
    said.contains("400") && (said.contains("`thinking`") || said.contains("signature") || said.contains("thinking block"))
}

/// Whether an `error` event inside a 200 stream is one worth asking again for.
fn stream_error_is_transient(kind: &str) -> bool {
    matches!(kind, "overloaded_error" | "api_error" | "rate_limit_error" | "timeout_error")
}

/// The complete `data:` payloads in the buffer, leaving any partial record.
///
/// The same framing as Gemini's stream; the `event:` line is ignored because
/// every payload names its own `type`.
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

/// How a stream ended.
enum Streamed {
    Done(Assembly),
    /// It broke. `spoken` is whether any text reached the sink first — if it
    /// did, asking again would show that text twice.
    Broke { error: AppError, transient: bool, spoken: bool },
}

// ═══════════════════════════════════════════════════════════════
//  PROVIDER
// ═══════════════════════════════════════════════════════════════

pub struct AnthropicProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl AnthropicProvider {
    pub fn new(api_key: Option<String>) -> Self {
        Self::at(ANTHROPIC_API, api_key)
    }

    /// The same, somewhere other than Anthropic — which in practice means a
    /// test.
    pub fn at(base_url: &str, api_key: Option<String>) -> Self {
        Self {
            client: chat_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()),
        }
    }

    fn authorize(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = req.header("anthropic-version", API_VERSION);
        match &self.api_key {
            Some(key) => req.header("x-api-key", key),
            None => req,
        }
    }

    fn no_key() -> AppError {
        AppError::General(
            "Anthropic needs an API key. Add one in Syn settings — console.anthropic.com issues them."
                .into(),
        )
    }

    /// Send the request, retrying transient failures, and once more without
    /// thinking blocks if the API refused one. `Ok(None)` is stopped.
    async fn post(
        &self,
        req: &ChatRequest<'_>,
        stream: bool,
        stop: &(dyn Fn() -> bool + Send + Sync),
    ) -> AppResult<Option<reqwest::Response>> {
        if self.api_key.is_none() {
            return Err(Self::no_key());
        }
        let url = format!("{}/v1/messages", self.base_url);

        let send = |body: Value| {
            let url = &url;
            async move {
                retry::send(
                    "Anthropic",
                    stop,
                    &|| self.authorize(self.client.post(url).json(&body)),
                    &|e| AppError::General(format!("Failed to reach Anthropic: {e}")),
                    &|status, body| explain(status, body, "Anthropic"),
                )
                .await
            }
        };

        let body = request_body(req, stream, true);
        let carried_thinking = body["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["content"].as_array())
            .flatten()
            .any(is_thinking);

        match send(body).await {
            Err(e) if carried_thinking && refused_the_thinking(&e) => {
                log::info!("[Syn] Anthropic refused a replayed thinking block ({e}); sending the round without thinking");
                send(request_body(req, stream, false)).await
            }
            other => other,
        }
    }

    /// Read a stream through to its end, or to the point it broke.
    async fn read_stream(&self, resp: reqwest::Response, sink: &StreamSink<'_>) -> Streamed {
        let mut stream = resp.bytes_stream();
        let mut buffer = String::new();
        let mut a = Assembly::default();
        let mut spoken = false;

        while let Some(Some(chunk)) = retry::unless_stopped(sink.stop_requested, stream.next()).await {
            if (sink.stop_requested)() {
                break;
            }
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(e) => {
                    return Streamed::Broke {
                        error: AppError::General(format!("Anthropic stream broke: {e}")),
                        transient: false,
                        spoken,
                    }
                }
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            for payload in take_records(&mut buffer) {
                let event = match serde_json::from_str::<Event>(&payload) {
                    Ok(event) => event,
                    // A record that will not parse costs a token, not the
                    // answer — the same tolerance the other providers have.
                    Err(e) => {
                        log::warn!("Unreadable Anthropic event: {e} — raw: {payload}");
                        continue;
                    }
                };
                match event {
                    Event::MessageStart { message } => {
                        if let Some(u) = message.usage {
                            a.usage.merge(u);
                        }
                    }
                    Event::ContentBlockStart { index, content_block } => a.start(index, &content_block),
                    Event::ContentBlockDelta { index, delta } => {
                        if let Some(text) = a.delta(index, delta) {
                            spoken = true;
                            (sink.on_token)(&text);
                        }
                    }
                    Event::MessageDelta { delta, usage } => {
                        if delta.stop_reason.is_some() {
                            a.stop_reason = delta.stop_reason;
                        }
                        if let Some(u) = usage {
                            a.usage.merge(u);
                        }
                    }
                    // Anthropic can fail after the 200: an `overloaded_error`
                    // as the first event is the common one.
                    Event::Error { error } => {
                        return Streamed::Broke {
                            transient: stream_error_is_transient(&error.kind),
                            error: AppError::General(format!(
                                "Anthropic stopped mid-answer ({}): {}",
                                error.kind, error.message
                            )),
                            spoken,
                        }
                    }
                    Event::Other => {}
                }
            }
        }

        Streamed::Done(a)
    }
}

#[async_trait]
impl ChatProvider for AnthropicProvider {
    fn id(&self) -> SynProvider {
        SynProvider::Anthropic
    }

    /// Yes: `tool_use` blocks stream like any other, their input as JSON
    /// fragments reassembled in `Assembly`.
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

        if self.api_key.is_none() {
            return Ok(status(false));
        }

        // The cheapest call that proves both that Anthropic is reachable and
        // that the key is good.
        let url = format!("{}/v1/models?limit=1", self.base_url);
        match self.authorize(probe_client().get(&url)).send().await {
            Ok(resp) if resp.status().is_success() => Ok(status(true)),
            Ok(resp) => {
                log::warn!("Anthropic answered /v1/models with {}", resp.status());
                Ok(status(false))
            }
            Err(e) => {
                log::info!("Anthropic not reachable: {e}");
                Ok(status(false))
            }
        }
    }

    /// Every model the key can use, `DEFAULT_MODEL` first.
    ///
    /// Everything `/v1/models` lists can chat, so nothing is filtered. It comes
    /// back newest first, and the picker falls back to the first entry when a
    /// vault has chosen nothing — see `DEFAULT_MODEL` for why that is moved.
    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        if self.api_key.is_none() {
            return Err(Self::no_key());
        }

        let mut found: Vec<ModelEntry> = Vec::new();
        let mut after: Option<String> = None;

        // Pages until Anthropic says there are no more, with a ceiling so a
        // server that always says there are cannot keep this going.
        for _ in 0..10 {
            let mut url = format!("{}/v1/models?limit=1000", self.base_url);
            if let Some(id) = &after {
                url.push_str(&format!("&after_id={}", urlencoding::encode(id)));
            }

            let resp = self
                .authorize(crate::syn::provider::catalogue_client().get(&url))
                .send()
                .await
                .map_err(|e| AppError::General(format!("Failed to reach Anthropic: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(explain(status, &body, "Listing Anthropic models"));
            }

            let page: ModelsPage = resp
                .json()
                .await
                .map_err(|e| AppError::General(format!("Failed to read Anthropic's model list: {e}")))?;

            found.extend(page.data);
            match page.last_id.filter(|_| page.has_more) {
                Some(id) => after = Some(id),
                None => break,
            }
        }

        if let Some(at) = found.iter().position(|m| m.id == DEFAULT_MODEL) {
            let chosen = found.remove(at);
            found.insert(0, chosen);
        }

        Ok(found
            .into_iter()
            .map(|m| ModelInfo {
                name: m.id.clone(),
                model: m.id,
                // Nothing is local, so there is no size and no digest.
                size: 0,
                digest: String::new(),
                modified_at: m.created_at.unwrap_or_default(),
                details: m.display_name.map(|shown| crate::models::syn::ModelDetails {
                    format: None,
                    family: Some(shown),
                    parameter_size: None,
                    quantization_level: None,
                }),
            })
            .collect())
    }

    async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply> {
        self.chat_stoppable(req, &retry::never).await
    }

    async fn chat_stoppable(
        &self,
        req: ChatRequest<'_>,
        stop: &(dyn Fn() -> bool + Send + Sync),
    ) -> AppResult<ChatReply> {
        let started = std::time::Instant::now();
        let Some(resp) = self.post(&req, false, stop).await? else {
            return Ok(ChatReply::default());
        };
        let Some(read) = retry::unless_stopped(stop, resp.json::<MessageResponse>()).await else {
            return Ok(ChatReply::default());
        };
        let body = read.map_err(|e| AppError::General(format!("Failed to read Anthropic's reply: {e}")))?;

        let mut a = Assembly::whole(body);
        a.duration_ms = Some(started.elapsed().as_millis() as u64);
        a.finish()
    }

    /// Streamed, and asked again if the stream fails before a word of it has
    /// been shown — Anthropic's `overloaded_error` can arrive as the first
    /// event of a 200, where the retry in `post` cannot see it. Once text has
    /// reached the sink, a break is reported instead: asking again would show
    /// the first half of the answer twice.
    async fn chat_streaming(
        &self,
        req: ChatRequest<'_>,
        sink: &StreamSink<'_>,
    ) -> AppResult<ChatReply> {
        let started = std::time::Instant::now();
        let mut failed = 0;

        loop {
            let Some(resp) = self.post(&req, true, sink.stop_requested).await? else {
                return Ok(ChatReply::default());
            };

            match self.read_stream(resp, sink).await {
                Streamed::Done(mut a) => {
                    a.duration_ms = Some(started.elapsed().as_millis() as u64);
                    // Stopped mid-way: what arrived is kept, and a refusal
                    // cannot be read into a turn that was cut short.
                    if (sink.stop_requested)() {
                        a.stop_reason = None;
                    }
                    return a.finish();
                }
                Streamed::Broke { error, transient, spoken } => {
                    failed += 1;
                    if spoken || !transient || failed >= retry::ATTEMPTS {
                        return Err(error);
                    }
                    let wait = retry::wait_before_retry(failed, None, retry::jitter());
                    log::info!("[Syn] {error}; nothing was shown yet, trying again in {:.1}s", wait.as_secs_f64());
                    if !retry::pause(wait, sink.stop_requested).await {
                        return Ok(ChatReply::default());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage::new(role, content)
    }

    fn call(id: Option<&str>, name: &str) -> ToolCall {
        ToolCall {
            id: id.map(str::to_string),
            function: ToolCallFunction { name: name.into(), arguments: json!({ "q": 1 }) },
            thought_signature: None,
        }
    }

    fn answer(id: Option<&str>, content: &str) -> ChatMessage {
        let mut m = msg("tool", content);
        m.tool_call_id = id.map(str::to_string);
        m
    }

    fn asking(calls: Vec<ToolCall>) -> ChatMessage {
        let mut m = msg("assistant", "");
        m.tool_calls = Some(calls);
        m
    }

    /// A round of the tool loop, which declares its tools.
    fn request<'a>(messages: &'a [ChatMessage], model: &'a str) -> ChatRequest<'a> {
        let tools: &'static [ToolDefinition] = Box::leak(vec![tool("query_nodes")].into_boxed_slice());
        ChatRequest { model, messages, temperature: Some(0.7), num_ctx: 8192, tools: Some(tools), json_schema: None }
    }

    /// A request that declares none.
    fn bare<'a>(messages: &'a [ChatMessage], model: &'a str) -> ChatRequest<'a> {
        ChatRequest { model, messages, temperature: Some(0.7), num_ctx: 8192, tools: None, json_schema: None }
    }

    fn tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".into(),
            function: crate::models::syn::FunctionDefinition {
                name: name.into(),
                description: format!("{name} things"),
                parameters: json!({ "type": "object", "properties": { "q": { "type": "string" } } }),
            },
        }
    }

    fn roles(body: &Value) -> Vec<String> {
        body["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap().to_string()).collect()
    }

    /// The system prompt leaves the message list, and carries a breakpoint.
    #[test]
    fn the_system_prompt_is_top_level_and_cached() {
        let history = [msg("system", "Bạn là Syn."), msg("user", "chào")];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);

        assert_eq!(body["system"][0]["text"], "Bạn là Syn.");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(roles(&body), ["user"]);
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], MAX_TOKENS_STREAMING);
    }

    /// Tools, then system, then the conversation: a breakpoint at the end of
    /// each, so round five of a loop reads rounds one to four from the cache.
    #[test]
    fn the_declarations_and_the_conversation_end_on_breakpoints() {
        let tools = [tool("query_nodes"), tool("get_node")];
        let history = [msg("system", "S"), msg("user", "q")];
        let mut req = request(&history, "claude-opus-5");
        req.tools = Some(&tools);
        let body = request_body(&req, false, true);

        let declared = body["tools"].as_array().unwrap();
        assert_eq!(declared[0]["input_schema"]["properties"]["q"]["type"], "string");
        assert!(declared[0].get("cache_control").is_none(), "only the last one");
        assert_eq!(declared[1]["cache_control"]["type"], "ephemeral");

        let last = body["messages"][0]["content"].as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["cache_control"]["type"], "ephemeral");
    }

    /// Calls become `tool_use`, results become `tool_result` in a user turn,
    /// and results of parallel calls share one turn, in the order asked.
    #[test]
    fn a_tool_round_becomes_use_and_result_blocks_in_one_user_turn() {
        let history = [
            msg("user", "tìm"),
            asking(vec![call(Some("toolu_a"), "first"), call(Some("toolu_b"), "second")]),
            answer(Some("toolu_a"), "{\"n\":1}"),
            answer(Some("toolu_b"), "2"),
        ];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);

        assert_eq!(roles(&body), ["user", "assistant", "user"], "{body:#}");
        let asked = body["messages"][1]["content"].as_array().unwrap();
        assert_eq!(asked[0]["type"], "tool_use");
        assert_eq!(asked[0]["id"], "toolu_a");
        assert_eq!(asked[0]["input"], json!({ "q": 1 }));
        assert_eq!(asked[1]["name"], "second");

        let results = body["messages"][2]["content"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["tool_use_id"], "toolu_a");
        assert_eq!(results[0]["content"], "{\"n\":1}");
        assert_eq!(results[1]["tool_use_id"], "toolu_b");
    }

    /// An Ollama-era history has no ids. Each call is given one, and its
    /// result is paired by position — the way Ollama pairs.
    #[test]
    fn calls_without_ids_are_given_ids_and_paired_in_order() {
        let history = [
            msg("user", "q"),
            asking(vec![call(None, "first"), call(None, "second")]),
            answer(None, "1"),
            answer(None, "2"),
        ];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        let asked = body["messages"][1]["content"].as_array().unwrap();
        let results = body["messages"][2]["content"].as_array().unwrap();
        assert_eq!(results[0]["tool_use_id"], asked[0]["id"]);
        assert_eq!(results[1]["tool_use_id"], asked[1]["id"]);
        assert_ne!(asked[0]["id"], asked[1]["id"]);
    }

    /// An id minted elsewhere with characters Anthropic refuses is cleaned the
    /// same way on both sides, so the pair still matches.
    #[test]
    fn a_foreign_id_is_cleaned_on_both_sides() {
        let history = [msg("user", "q"), asking(vec![call(Some("call:7/x"), "f")]), answer(Some("call:7/x"), "ok")];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        assert_eq!(body["messages"][1]["content"][0]["id"], "call_7_x");
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "call_7_x");
    }

    /// Every `tool_use` must be answered in the next turn, or it is a 400. A
    /// history that ends on a call, or goes on past one, answers it as not run.
    #[test]
    fn a_call_nobody_answered_is_answered_as_not_run() {
        let history = [msg("user", "q"), asking(vec![call(Some("c9"), "browse")])];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        assert_eq!(roles(&body), ["user", "assistant", "user"]);
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "c9");
        assert_eq!(body["messages"][2]["content"][0]["content"], NOT_RUN);

        // Answered one of two, then the person spoke: the missing result goes
        // with the other, ahead of the words.
        let history = [
            msg("user", "q"),
            asking(vec![call(Some("a"), "f"), call(Some("b"), "g")]),
            answer(Some("a"), "1"),
            msg("user", "thôi"),
        ];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        assert_eq!(roles(&body), ["user", "assistant", "user"]);
        let turn = body["messages"][2]["content"].as_array().unwrap();
        assert_eq!(turn[0]["tool_use_id"], "a");
        assert_eq!(turn[1]["tool_use_id"], "b");
        assert_eq!(turn[2]["text"], "thôi");
    }

    /// A result for a call the history does not show would be a 400 as a
    /// `tool_result`. As text it is still read.
    #[test]
    fn an_orphan_result_is_kept_as_text() {
        let history = [msg("user", "q"), answer(Some("gone"), "{\"x\":1}")];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        assert_eq!(roles(&body), ["user"]);
        let turn = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(turn[1]["type"], "text");
        assert!(turn[1]["text"].as_str().unwrap().contains("{\"x\":1}"));
    }

    /// Images go as base64 blocks, with their type — declared or read from the
    /// bytes — and ahead of the question about them.
    #[test]
    fn images_become_base64_blocks_before_the_text() {
        let mut m = msg("user", "Ảnh này là gì?");
        m.images = Some(vec!["iVBORw0KGgoAAAA".into(), "data:image/webp;base64,UklGRxxxx".into()]);
        let history = [m];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["media_type"], "image/png");
        assert_eq!(blocks[0]["source"]["data"], "iVBORw0KGgoAAAA");
        assert_eq!(blocks[1]["source"]["media_type"], "image/webp");
        assert_eq!(blocks[1]["source"]["data"], "UklGRxxxx");
        assert_eq!(blocks[2]["text"], "Ảnh này là gì?");
    }

    /// Opus 4.7 onwards, Sonnet 5 and Fable refuse sampling parameters; the
    /// older ones take them. And the next Claude is left alone by default.
    #[test]
    fn temperature_goes_only_to_models_that_take_it() {
        for model in ["claude-haiku-4-5", "claude-sonnet-4-6", "claude-opus-4-6", "claude-sonnet-4-5-20250929", "claude-3-7-sonnet-20250219"] {
            assert!(sends_temperature(model), "{model}");
        }
        for model in ["claude-opus-5", "claude-opus-5-5", "claude-opus-4-7", "claude-opus-4-8", "claude-sonnet-5", "claude-fable-5-1", "claude-nova-1"] {
            assert!(!sends_temperature(model), "{model}");
        }
        let history = [msg("user", "q")];
        assert!(request_body(&request(&history, "claude-opus-5"), true, true).get("temperature").is_none());
        assert_eq!(request_body(&request(&history, "claude-haiku-4-5"), true, true)["temperature"], 0.7);
    }

    /// Structured output, with every object closed as the API requires.
    #[test]
    fn a_schema_becomes_output_config_with_every_object_closed() {
        let schema = json!({
            "type": "object",
            "properties": { "moment": { "type": "object", "properties": { "at": { "type": "string" } } } },
            "required": ["moment"]
        });
        let history = [msg("user", "q")];
        let mut req = request(&history, "claude-opus-5");
        req.json_schema = Some(&schema);
        let body = request_body(&req, false, true);

        let format = &body["output_config"]["format"];
        assert_eq!(format["type"], "json_schema");
        assert_eq!(format["schema"]["additionalProperties"], false);
        assert_eq!(format["schema"]["properties"]["moment"]["additionalProperties"], false);

        // A schema that already says otherwise is left saying it.
        let open = json!({ "type": "object", "additionalProperties": true });
        assert_eq!(closed(&open)["additionalProperties"], true);
    }

    // ── reading a stream ─────────────────────────────────────────

    /// A recorded stream: thinking, a sentence, and a call whose input arrives
    /// in three fragments that split a key and a Vietnamese word.
    fn recorded_stream() -> String {
        [
            r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[],"model":"claude-opus-5","usage":{"input_tokens":120,"cache_creation_input_tokens":0,"cache_read_input_tokens":9000,"output_tokens":1}}}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"SIG-abc"}}"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":0}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Để tôi "}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"tìm."}}"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":1}"#,
            r#"event: ping
data: {"type":"ping"}"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_01","name":"query_nodes","input":{}}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"que"}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"ry\": \"sách "}}"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"hay\"}"}}"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":2}"#,
            r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"tool_use","stop_sequence":null},"usage":{"output_tokens":85}}"#,
            r#"event: message_stop
data: {"type":"message_stop"}"#,
        ]
        .join("\n\n")
            + "\n\n"
    }

    fn assemble(stream: &str) -> Assembly {
        let mut buffer = stream.to_string();
        let mut a = Assembly::default();
        for payload in take_records(&mut buffer) {
            match serde_json::from_str::<Event>(&payload).expect("every event parses") {
                Event::MessageStart { message } => a.usage.merge(message.usage.unwrap_or_default()),
                Event::ContentBlockStart { index, content_block } => a.start(index, &content_block),
                Event::ContentBlockDelta { index, delta } => {
                    a.delta(index, delta);
                }
                Event::MessageDelta { delta, usage } => {
                    a.stop_reason = delta.stop_reason;
                    a.usage.merge(usage.unwrap_or_default());
                }
                Event::Error { .. } | Event::Other => {}
            }
        }
        a
    }

    #[test]
    fn a_split_tool_input_is_reassembled() {
        let reply = assemble(&recorded_stream()).finish().unwrap();
        assert_eq!(reply.content, "Để tôi tìm.");
        assert_eq!(reply.tool_calls.len(), 1);
        assert_eq!(reply.tool_calls[0].id.as_deref(), Some("toolu_01"));
        assert_eq!(reply.tool_calls[0].function.name, "query_nodes");
        assert_eq!(reply.tool_calls[0].function.arguments, json!({ "query": "sách hay" }));
    }

    /// Input and cache reads arrive on `message_start`, output on
    /// `message_delta`. `input` is everything sent, cache reads included.
    #[test]
    fn usage_is_gathered_from_both_ends_of_the_stream() {
        let reply = assemble(&recorded_stream()).finish().unwrap();
        assert_eq!(reply.usage.input, Some(9_120));
        assert_eq!(reply.usage.input_cached, Some(9_000));
        assert_eq!(reply.usage.output, Some(85));
        assert_eq!(reply.usage.output_hidden, None, "thinking is inside output, not reported apart");
    }

    /// Cache writes are input too — sent and billed — but not cached input.
    #[test]
    fn a_cache_write_is_input_but_not_cached_input() {
        let usage: Usage = WireUsage {
            input_tokens: Some(50),
            output_tokens: Some(10),
            cache_creation_input_tokens: Some(8_000),
            cache_read_input_tokens: Some(0),
        }
        .into();
        assert_eq!(usage.input, Some(8_050));
        assert_eq!(usage.input_cached, Some(0));
        assert!(Usage::from(WireUsage::default()).is_silent(), "silence is not zero");
    }

    /// The whole turn, thinking included, rides on the first call and comes
    /// back byte for byte — thinking, text, call, in the order they came.
    #[test]
    fn a_thinking_turn_goes_back_exactly_as_it_came() {
        let reply = assemble(&recorded_stream()).finish().unwrap();
        let carried = reply.tool_calls[0].thought_signature.clone().expect("the turn is carried");
        assert!(carried.starts_with(TURN_CARRIER));

        let mut asked = msg("assistant", &reply.content);
        asked.tool_calls = Some(reply.tool_calls.clone());
        let history = [msg("user", "q"), asked, answer(Some("toolu_01"), "{}")];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);

        let turn = body["messages"][1]["content"].as_array().unwrap();
        let kinds: Vec<&str> = turn.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["thinking", "text", "tool_use"], "{body:#}");
        assert_eq!(turn[0]["signature"], "SIG-abc");
        assert_eq!(turn[2]["input"], json!({ "query": "sách hay" }));

        // The retry after a refused thinking block sends the same turn without it.
        let bare = request_body(&request(&history, "claude-opus-5"), true, false);
        let kinds: Vec<&str> =
            bare["messages"][1]["content"].as_array().unwrap().iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["text", "tool_use"]);
    }

    /// A turn whose calls changed on the way back is rebuilt, and its thinking
    /// is left out rather than put in front of calls it did not lead to.
    #[test]
    fn a_turn_whose_calls_changed_is_rebuilt_without_its_thinking() {
        let reply = assemble(&recorded_stream()).finish().unwrap();
        let mut changed = reply.tool_calls[0].clone();
        changed.id = Some("toolu_other".into());
        let mut asked = msg("assistant", "");
        asked.tool_calls = Some(vec![changed]);
        let history = [msg("user", "q"), asked, answer(Some("toolu_other"), "{}")];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        let turn = body["messages"][1]["content"].as_array().unwrap();
        assert!(turn.iter().all(|b| !is_thinking(b)), "{body:#}");
        assert_eq!(turn[0]["id"], "toolu_other");
    }

    /// Gemini's signature, left on a call by a conversation that moved here, is
    /// not mistaken for a carried turn.
    #[test]
    fn another_providers_signature_is_ignored() {
        let mut c = call(Some("c1"), "f");
        c.thought_signature = Some("GEMINI-SIG".into());
        let history = [msg("user", "q"), asking(vec![c]), answer(Some("c1"), "{}")];
        let body = request_body(&request(&history, "claude-opus-5"), true, true);
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
    }

    /// No thinking, nothing carried: the turn is rebuilt exactly enough.
    #[test]
    fn a_turn_without_thinking_carries_nothing() {
        let a = Assembly::whole(
            serde_json::from_value(json!({
                "content": [{ "type": "tool_use", "id": "toolu_9", "name": "get_node", "input": { "id": "x" } }],
                "stop_reason": "tool_use",
                "usage": { "input_tokens": 10, "output_tokens": 5 }
            }))
            .unwrap(),
        );
        let reply = a.finish().unwrap();
        assert!(reply.tool_calls[0].thought_signature.is_none());
        assert_eq!(reply.tool_calls[0].function.arguments, json!({ "id": "x" }));
    }

    /// Nothing written and a reason given is a refusal or a cut-off, and saying
    /// nothing would hide it.
    #[test]
    fn an_empty_refusal_is_said_rather_than_shown_as_silence() {
        let refused = Assembly::whole(serde_json::from_value(json!({ "content": [], "stop_reason": "refusal" })).unwrap());
        assert!(refused.finish().unwrap_err().to_string().contains("declined"));

        let cut = Assembly::whole(serde_json::from_value(json!({ "content": [], "stop_reason": "max_tokens" })).unwrap());
        assert!(cut.finish().unwrap_err().to_string().contains("ran out of room"));

        let fine = Assembly::whole(
            serde_json::from_value(json!({ "content": [{ "type": "text", "text": "Xong." }], "stop_reason": "end_turn" })).unwrap(),
        );
        assert_eq!(fine.finish().unwrap().content, "Xong.");
    }

    #[test]
    fn an_error_says_what_anthropic_said() {
        let e = explain(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
            "Anthropic",
        );
        let said = e.to_string();
        assert!(said.contains("invalid x-api-key"), "{said}");
        assert!(said.contains("check the API key"), "{said}");
        assert!(!said.contains("\"type\""), "the JSON is not shown: {said}");

        let busy = explain(reqwest::StatusCode::from_u16(529).unwrap(), r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#, "Anthropic");
        assert!(busy.to_string().contains("overloaded"), "{busy}");
    }

    /// A 400 about a replayed thinking block is the one that is sent again
    /// without thinking. Any other 400 is not.
    #[test]
    fn only_a_refused_thinking_block_is_retried_without_thinking() {
        let bound = explain(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"messages.1.content.0: Invalid `signature` in `thinking` block. The block is bound to a different conversation."}}"#,
            "Anthropic",
        );
        assert!(refused_the_thinking(&bound));

        let other = explain(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: must be positive"}}"#,
            "Anthropic",
        );
        assert!(!refused_the_thinking(&other));
    }

    #[test]
    fn an_overloaded_stream_is_worth_asking_again_and_a_bad_request_is_not() {
        assert!(stream_error_is_transient("overloaded_error"));
        assert!(stream_error_is_transient("api_error"));
        assert!(!stream_error_is_transient("invalid_request_error"));
    }

    // ── against a server ──────────────────────────────────────────

    /// A server that answers each request with the next canned response and
    /// keeps what it was sent.
    async fn serve(
        replies: Vec<(u16, &'static str, String)>,
    ) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let kept = seen.clone();

        tokio::spawn(async move {
            for (status, content_type, body) in replies {
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
                    "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\nretry-after: 0\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.ok();
            }
        });

        (format!("http://{addr}"), seen)
    }

    /// End to end: an overloaded 529 is asked again, the stream is read, the
    /// key goes in `x-api-key` with the version header, and the second round
    /// carries the thinking block back.
    #[tokio::test]
    async fn a_tool_loop_survives_a_529_and_carries_its_thinking_forward() {
        let overloaded = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#.to_string();
        let second = [
            r#"data: {"type":"message_start","message":{"usage":{"input_tokens":30,"cache_read_input_tokens":9000,"output_tokens":1}}}"#,
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Có 3 note."}}"#,
            r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":6}}"#,
        ]
        .join("\n\n")
            + "\n\n";

        let (base, seen) = serve(vec![
            (529, "application/json", overloaded),
            (200, "text/event-stream", recorded_stream()),
            (200, "text/event-stream", second),
        ])
        .await;
        let provider = AnthropicProvider::at(&base, Some("sk-ant-KEY".into()));

        let tokens = std::sync::Mutex::new(String::new());
        let on_token = |t: &str| tokens.lock().unwrap().push_str(t);
        let never = || false;
        let sink = StreamSink { on_token: &on_token, stop_requested: &never };

        let mut history = vec![msg("system", "Bạn là Syn."), msg("user", "sách hay")];
        let one = provider.chat_streaming(request(&history, "claude-opus-5"), &sink).await.expect("round one");
        assert_eq!(one.tool_calls.len(), 1);
        assert_eq!(tokens.lock().unwrap().as_str(), "Để tôi tìm.", "shown once, not twice");

        let mut asked = msg("assistant", &one.content);
        asked.tool_calls = Some(one.tool_calls.clone());
        history.push(asked);
        history.push(answer(one.tool_calls[0].id.as_deref(), "{\"total\":3}"));

        let two = provider.chat_streaming(request(&history, "claude-opus-5"), &sink).await.expect("round two");
        assert_eq!(two.content, "Có 3 note.");
        assert_eq!(two.usage.input_cached, Some(9_000));

        let requests = seen.lock().unwrap().clone();
        assert_eq!(requests.len(), 3, "the 529 was asked again");
        let (head, body) = requests[2].split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("POST /v1/messages"), "{head}");
        let lower = head.to_lowercase();
        assert!(lower.contains("x-api-key: sk-ant-key"), "{head}");
        assert!(lower.contains("anthropic-version: 2023-06-01"), "{head}");

        let sent: Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["messages"][1]["content"][0]["type"], "thinking", "{sent:#}");
        assert_eq!(sent["messages"][1]["content"][0]["signature"], "SIG-abc");
        assert_eq!(sent["messages"][2]["content"][0]["tool_use_id"], "toolu_01");
    }

    /// A 400 is the request's own fault and is not sent again.
    #[tokio::test]
    async fn a_bad_request_is_reported_at_once() {
        let bad = r#"{"type":"error","error":{"type":"invalid_request_error","message":"model: unknown"}}"#.to_string();
        let (base, seen) = serve(vec![(400, "application/json", bad)]).await;
        let provider = AnthropicProvider::at(&base, Some("K".into()));
        let history = [msg("user", "q")];
        let e = provider.chat(request(&history, "claude-nope")).await.unwrap_err().to_string();
        assert!(e.contains("model: unknown"), "{e}");
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    /// The default model is offered first, whatever order the list came in.
    #[tokio::test]
    async fn the_model_list_puts_the_default_first() {
        let page = json!({
            "data": [
                { "type": "model", "id": "claude-fable-5-1", "display_name": "Claude Fable 5.1", "created_at": "2026-08-01T00:00:00Z" },
                { "type": "model", "id": "claude-opus-5", "display_name": "Claude Opus 5", "created_at": "2026-05-01T00:00:00Z" },
                { "type": "model", "id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5", "created_at": "2025-10-01T00:00:00Z" }
            ],
            "has_more": false,
            "first_id": "claude-fable-5-1",
            "last_id": "claude-haiku-4-5"
        })
        .to_string();
        let (base, _) = serve(vec![(200, "application/json", page)]).await;

        let models = AnthropicProvider::at(&base, Some("K".into())).list_models().await.unwrap();
        let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["claude-opus-5", "claude-fable-5-1", "claude-haiku-4-5"]);
    }

    /// Without a key there is nothing to ask, and the message says where one
    /// comes from.
    #[tokio::test]
    async fn no_key_is_said_plainly_and_nothing_is_sent() {
        let p = AnthropicProvider::at("http://127.0.0.1:9", None);
        assert!(!p.check_status().await.unwrap().connected);
        let e = p.list_models().await.unwrap_err().to_string();
        assert!(e.contains("API key"), "{e}");
    }

    /// P2: the last round without tools, or a compaction, still carries the
    /// loop's calls. With no tools declared those blocks are a 400, so they go
    /// as text.
    #[test]
    fn a_request_without_tools_carries_the_calls_as_text() {
        let history = [
            msg("user", "tìm sách"),
            asking(vec![call(Some("toolu_1"), "query_nodes")]),
            answer(Some("toolu_1"), "{\"total\":3}"),
        ];
        let body = request_body(&bare(&history, "claude-opus-5"), false, true);
        let text = body.to_string();
        assert!(!text.contains("\"tool_use\"") && !text.contains("\"tool_result\""), "{body:#}");
        assert!(text.contains("query_nodes") && text.contains("total"), "what happened is still said: {body:#}");
        assert_eq!(roles(&body), ["user", "assistant", "user"]);
    }
}
