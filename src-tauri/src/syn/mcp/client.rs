//! Speaking MCP: JSON-RPC 2.0 messages, the handshake, and the three calls
//! Syn makes — `initialize`, `tools/list`, `tools/call`.
//!
//! # Why written here rather than taken from an SDK
//!
//! What Syn needs of the protocol is small: a handshake, a paginated list and a
//! call, over two transports. Everything else an SDK brings — resources,
//! prompts, sampling, a server side, its own async runtime assumptions — is
//! either deferred on purpose or a door this app does not want open (sampling
//! would let a server ask *Syn's* model to write things). The whole client is a
//! few hundred lines that can be read in one sitting, with no new crate in the
//! Android binary: `reqwest`, `tokio` and `serde_json` are already there.
//!
//! # What this module does not decide
//!
//! Whether a call may happen, what it costs, what the result may do to the rest
//! of the run. Those are `syn::gate`, `syn::consent` and `syn::taint`. This is
//! only the wire.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{json, Value};

/// The version asked for. The server answers with the one it will speak.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Versions whose three calls are the same shape as far as Syn reads them.
///
/// Older servers still answer `2024-11-05`, and nothing Syn uses changed
/// between these. A version outside the list is refused rather than guessed
/// at: a server speaking a protocol this code has not read is a server whose
/// answers it cannot be sure it understands.
pub const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// How long the handshake may take. A server that cannot say hello in fifteen
/// seconds is not one to wait on in the middle of somebody's question.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// How long listing tools may take, per page.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// How long one tool call may take.
///
/// A minute. Longer than a page load because a tool may do real work — search
/// an issue tracker, run a query — and shorter than a person's patience with a
/// spinner that has stopped meaning anything.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// How many pages of tools are read before giving up on the rest.
///
/// A server that paginates past this is offering more tools than any model
/// could choose between, and a cursor that never ends is the shape of a
/// server stuck in a loop.
const MAX_PAGES: usize = 20;

/// How many tools one server may offer.
pub const MAX_TOOLS: usize = 200;

/// Something went wrong, and whether the server's own words are in it.
///
/// The distinction is the one `syn::taint` cares about: an error the server
/// *wrote* is content from outside, as much as a result is; an error this code
/// wrote — a timeout, a refused connection — carries nothing of theirs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    /// Never reached the server, or the server's answer was not read. Our words.
    Unreachable(String),
    /// Nothing came back in time.
    Timeout,
    /// The server has forgotten the session. Connect again.
    SessionGone,
    /// The server answered with a JSON-RPC error. `message` is **its** words.
    Rpc { code: i64, message: String },
    /// The server answered with something that is not MCP. Our words.
    Protocol(String),
    /// A stdio server on a platform that cannot start one.
    DesktopOnly,
}

impl McpError {
    /// Whether anything in this error was written by the server.
    pub fn server_wrote_it(&self) -> bool {
        matches!(self, McpError::Rpc { .. })
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpError::Unreachable(why) => write!(f, "{why}"),
            McpError::Timeout => write!(f, "the server did not answer in time"),
            McpError::SessionGone => write!(f, "the server ended the session"),
            McpError::Rpc { code, message } => write!(f, "the server answered with an error ({code}): {message}"),
            McpError::Protocol(why) => write!(f, "the server's answer was not MCP: {why}"),
            McpError::DesktopOnly => write!(f, "this server runs a program, which needs the desktop app"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  FRAMING
// ═══════════════════════════════════════════════════════════════

/// A request, which expects an answer carrying the same `id`.
pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// A notification, which expects nothing.
pub fn notification(method: &str) -> Value {
    json!({ "jsonrpc": "2.0", "method": method })
}

/// The answer to request `id`, if `message` holds it.
///
/// `message` may be one object or a batch; either may hold other things too —
/// a server's own notifications, or its requests to us — which are not the
/// answer and are passed over. An id is matched as a number or as the same
/// number written as a string, because servers have been seen doing both.
pub fn answer_to(id: u64, message: &Value) -> Option<Result<Value, McpError>> {
    if let Some(batch) = message.as_array() {
        return batch.iter().find_map(|m| answer_to(id, m));
    }
    let object = message.as_object()?;
    let matches = match object.get("id") {
        Some(Value::Number(n)) => n.as_u64() == Some(id),
        Some(Value::String(s)) => s == &id.to_string(),
        _ => false,
    };
    // A request *from* the server can reuse our id; it has a method and is not
    // an answer.
    if !matches || object.contains_key("method") {
        return None;
    }
    if let Some(error) = object.get("error") {
        return Some(Err(McpError::Rpc {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no message")
                .chars()
                .take(2_000)
                .collect(),
        }));
    }
    Some(Ok(object.get("result").cloned().unwrap_or(Value::Null)))
}

/// What to say back to a request the server sent us, if anything.
///
/// Only `ping` is answered with a result. Everything else — `sampling`,
/// `roots/list`, `elicitation` — is refused as a method we do not have,
/// because this client never declared those capabilities, and a server asking
/// anyway is a server whose request should not be guessed at.
pub fn reply_to_server(message: &Value) -> Option<Value> {
    let object = message.as_object()?;
    let method = object.get("method")?.as_str()?;
    let id = object.get("id")?.clone();
    Some(if method == "ping" {
        json!({ "jsonrpc": "2.0", "id": id, "result": {} })
    } else {
        json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "Method not found" } })
    })
}

/// Server-sent events, fed in as the bytes arrive.
///
/// Only `data:` lines matter to MCP; an event is the `data:` lines between two
/// blank lines, joined by newlines. `event:`, `id:` and `retry:` are read past,
/// and a comment line (`:`) is a keep-alive.
#[derive(Default)]
pub struct SseParser {
    pending: String,
    data: Vec<String>,
}

impl SseParser {
    /// Feed a chunk; get back every event it completed.
    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.pending.push_str(chunk);
        let mut events = Vec::new();
        while let Some(end) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=end).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(rest) = line.strip_prefix("data:") {
                self.data.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
        }
        events
    }

    /// Whatever event was still open when the stream ended.
    pub fn finish(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.pending);
        let events = self.push(&(rest + "\n\n"));
        events.into_iter().next()
    }
}

// ═══════════════════════════════════════════════════════════════
//  THE CONVERSATION
// ═══════════════════════════════════════════════════════════════

/// One of the two ways to reach a server.
pub enum Transport {
    Http(super::transport_http::Http),
    #[cfg(desktop)]
    Stdio(super::transport_stdio::Stdio),
}

impl Transport {
    async fn exchange(&self, id: u64, message: &Value, within: Duration) -> Result<Value, McpError> {
        match self {
            Transport::Http(t) => t.exchange(id, message, within).await,
            #[cfg(desktop)]
            Transport::Stdio(t) => t.exchange(id, message, within).await,
        }
    }

    async fn notify(&self, message: &Value) -> Result<(), McpError> {
        match self {
            Transport::Http(t) => t.notify(message).await,
            #[cfg(desktop)]
            Transport::Stdio(t) => t.notify(message).await,
        }
    }

    fn agreed(&self, version: &str) {
        match self {
            Transport::Http(t) => t.agreed(version),
            #[cfg(desktop)]
            Transport::Stdio(_) => {}
        }
    }

    async fn close(&self) {
        match self {
            Transport::Http(t) => t.close().await,
            #[cfg(desktop)]
            Transport::Stdio(t) => t.close(),
        }
    }
}

/// A tool, as a server described it.
///
/// Every field here was written by the server. `read_only` in particular is
/// the server's *claim* — it decides only which question the consent card
/// asks, never whether one is asked. See `provider::capability_for`.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub read_only: bool,
}

/// What a tool call gave back, flattened to text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallResult {
    pub text: String,
    /// The server said the tool failed. Its text is still its words.
    pub is_error: bool,
}

/// A server, after the handshake.
pub struct Session {
    transport: Transport,
    next_id: AtomicU64,
    pub protocol: String,
}

impl Session {
    /// Say hello, agree a version, and tell the server we are ready.
    pub async fn open(transport: Transport) -> Result<Self, McpError> {
        let mut session = Session { transport, next_id: AtomicU64::new(1), protocol: String::new() };
        let result = session
            .call(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    // Nothing declared: no sampling, no roots, no elicitation.
                    // A server cannot ask Syn's model to write for it, cannot
                    // be told where the vault is, and cannot put a form in
                    // front of the person.
                    "capabilities": {},
                    "clientInfo": { "name": "Synabit", "version": env!("CARGO_PKG_VERSION") },
                }),
                CONNECT_TIMEOUT,
            )
            .await?;
        let version = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| McpError::Protocol("no protocol version in the answer to initialize".into()))?;
        if !SUPPORTED_VERSIONS.contains(&version) {
            session.transport.close().await;
            return Err(McpError::Protocol(format!("unsupported protocol version {version:?}")));
        }
        session.protocol = version.to_string();
        session.transport.agreed(version);
        session.transport.notify(&notification("notifications/initialized")).await?;
        Ok(session)
    }

    async fn call(&self, method: &str, params: Value, within: Duration) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.transport.exchange(id, &request(id, method, params), within).await
    }

    /// Every tool the server offers, reading every page.
    pub async fn list_tools(&self) -> Result<Vec<RemoteTool>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let page = self.call("tools/list", params, LIST_TIMEOUT).await?;
            let listed = page
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpError::Protocol("tools/list gave no list of tools".into()))?;
            tools.extend(listed.iter().filter_map(tool_from));
            if tools.len() >= MAX_TOOLS {
                tools.truncate(MAX_TOOLS);
                break;
            }
            cursor = page.get("nextCursor").and_then(Value::as_str).map(str::to_string);
            if cursor.as_deref().is_none_or(str::is_empty) {
                break;
            }
        }
        Ok(tools)
    }

    /// Call one tool.
    pub async fn call_tool(&self, name: &str, arguments: &Value) -> Result<CallResult, McpError> {
        let arguments = if arguments.is_object() { arguments.clone() } else { json!({}) };
        let result = self
            .call("tools/call", json!({ "name": name, "arguments": arguments }), CALL_TIMEOUT)
            .await?;
        Ok(CallResult {
            text: render(&result),
            is_error: result.get("isError").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    /// Let the server go: end the HTTP session, stop the process.
    pub async fn close(&self) {
        self.transport.close().await;
    }
}

/// A tool out of a `tools/list` page, or `None` for one without a name.
fn tool_from(value: &Value) -> Option<RemoteTool> {
    let name = value.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    Some(RemoteTool {
        name: name.to_string(),
        description: value
            .get("description")
            .and_then(Value::as_str)
            .or_else(|| value.get("title").and_then(Value::as_str))
            .unwrap_or_default()
            .trim()
            .to_string(),
        input_schema: value.get("inputSchema").cloned().unwrap_or_else(|| json!({ "type": "object" })),
        read_only: value
            .get("annotations")
            .and_then(|a| a.get("readOnlyHint"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// A `tools/call` result as text a model can read.
///
/// Text is kept. Pictures and sound are named and left out: no model Syn talks
/// to is given images from a tool result today, and base64 in a text channel is
/// thousands of tokens of nothing. A resource link is listed with its address,
/// so the person can open it; an embedded resource is kept when it is text.
pub fn render(result: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut links: Vec<String> = Vec::new();
    let mut any_text = false;

    for block in result.get("content").and_then(Value::as_array).into_iter().flatten() {
        let kind = block.get("type").and_then(Value::as_str).unwrap_or_default();
        match kind {
            "text" => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    any_text = true;
                    parts.push(text.to_string());
                }
            }
            "image" => parts.push("[image omitted]".to_string()),
            "audio" => parts.push("[audio omitted]".to_string()),
            "resource_link" => {
                let uri = block.get("uri").and_then(Value::as_str).unwrap_or_default();
                let name = block
                    .get("title")
                    .or_else(|| block.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or(uri);
                links.push(format!("- {name}: {uri}"));
            }
            "resource" => {
                let resource = block.get("resource").cloned().unwrap_or(Value::Null);
                let uri = resource.get("uri").and_then(Value::as_str).unwrap_or_default();
                match resource.get("text").and_then(Value::as_str) {
                    Some(text) => {
                        any_text = true;
                        parts.push(text.to_string());
                    }
                    None => parts.push(format!("[file omitted: {uri}]")),
                }
            }
            other => parts.push(format!("[{} omitted]", if other.is_empty() { "content" } else { other })),
        }
    }

    // Structured output is sent alongside a text copy by servers that follow
    // the spec, and instead of one by some that do not. Only used when there is
    // no text: the same thing twice is twice the tokens.
    if !any_text {
        if let Some(structured) = result.get("structuredContent").filter(|v| !v.is_null()) {
            parts.push(structured.to_string());
        }
    }

    if !links.is_empty() {
        parts.push(format!("Links:\n{}", links.join("\n")));
    }
    if parts.is_empty() {
        return "(the tool returned nothing)".to_string();
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_found_by_its_id_among_other_messages() {
        let batch = json!([
            { "jsonrpc": "2.0", "method": "notifications/progress", "params": {} },
            { "jsonrpc": "2.0", "id": 7, "method": "ping" },
            { "jsonrpc": "2.0", "id": 7, "result": { "ok": true } },
        ]);
        assert_eq!(answer_to(7, &batch), Some(Ok(json!({ "ok": true }))));
        assert_eq!(answer_to(8, &batch), None);
        assert_eq!(answer_to(3, &json!({ "jsonrpc": "2.0", "id": "3", "result": 1 })), Some(Ok(json!(1))));
    }

    #[test]
    fn an_error_answer_carries_the_servers_words_and_says_so() {
        let answer = answer_to(1, &json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32602, "message": "bad" } }));
        let error = answer.expect("found").expect_err("an error");
        assert!(error.server_wrote_it());
        assert!(!McpError::Timeout.server_wrote_it());
        assert!(!McpError::Unreachable("x".into()).server_wrote_it());
    }

    #[test]
    fn only_ping_is_answered_and_everything_else_is_refused() {
        let pong = reply_to_server(&json!({ "jsonrpc": "2.0", "id": 4, "method": "ping" })).expect("answered");
        assert_eq!(pong["result"], json!({}));
        let no = reply_to_server(&json!({ "jsonrpc": "2.0", "id": 5, "method": "sampling/createMessage" }))
            .expect("answered");
        assert_eq!(no["error"]["code"], -32601);
        assert!(reply_to_server(&json!({ "jsonrpc": "2.0", "method": "notifications/x" })).is_none());
    }

    #[test]
    fn server_sent_events_are_read_across_chunks() {
        let mut sse = SseParser::default();
        assert!(sse.push("event: message\ndata: {\"a\":").is_empty());
        let events = sse.push("1}\r\n\r\n: keep-alive\n\ndata: x\ndata: y\n\n");
        assert_eq!(events, vec!["{\"a\":1}".to_string(), "x\ny".to_string()]);
        assert!(sse.push("data: tail").is_empty());
        assert_eq!(sse.finish(), Some("tail".to_string()));
    }

    #[test]
    fn a_result_keeps_its_text_names_what_it_leaves_out_and_lists_its_links() {
        let result = json!({ "content": [
            { "type": "text", "text": "Two issues." },
            { "type": "image", "data": "AAAA", "mimeType": "image/png" },
            { "type": "resource_link", "uri": "https://jira.example/browse/A-1", "name": "A-1" },
            { "type": "resource", "resource": { "uri": "file:///x.txt", "text": "inside" } },
            { "type": "resource", "resource": { "uri": "file:///x.bin", "blob": "AAAA" } },
        ]});
        let text = render(&result);
        assert!(text.contains("Two issues."), "{text}");
        assert!(text.contains("[image omitted]"), "{text}");
        assert!(!text.contains("AAAA"), "no base64 reaches the model: {text}");
        assert!(text.contains("- A-1: https://jira.example/browse/A-1"), "{text}");
        assert!(text.contains("inside"), "{text}");
        assert!(text.contains("[file omitted: file:///x.bin]"), "{text}");
    }

    #[test]
    fn structured_output_is_used_only_when_there_is_no_text() {
        let both = json!({ "content": [{ "type": "text", "text": "{\"n\":1}" }], "structuredContent": { "n": 1 } });
        assert_eq!(render(&both), "{\"n\":1}");
        let only = json!({ "content": [], "structuredContent": { "n": 1 } });
        assert_eq!(render(&only), "{\"n\":1}");
        assert_eq!(render(&json!({})), "(the tool returned nothing)");
    }

    #[test]
    fn a_tool_is_read_only_only_when_it_says_so() {
        let said = tool_from(&json!({ "name": "search", "annotations": { "readOnlyHint": true } })).expect("a tool");
        assert!(said.read_only);
        let unsaid = tool_from(&json!({ "name": "send", "inputSchema": { "type": "object" } })).expect("a tool");
        assert!(!unsaid.read_only, "silence is not a promise");
        assert!(tool_from(&json!({ "name": "  " })).is_none());
    }
}
