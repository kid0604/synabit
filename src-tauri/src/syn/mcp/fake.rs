//! An MCP server on 127.0.0.1 that answers from a script, for tests.
//!
//! Speaks just enough Streamable HTTP to be told apart from a real one only by
//! what it says: a session id on `initialize`, 202 for a notification, pages of
//! tools, and each tool's canned answer — as JSON or as server-sent events.
//! Keeps every request it was sent, headers and all, so a test can see what
//! went out as well as what came back.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Default)]
pub struct Script {
    /// Answer with server-sent events rather than JSON.
    pub sse: bool,
    pub tools: Vec<Value>,
    /// Tools per page of `tools/list`; 0 for all on one.
    pub page: usize,
    /// A tool's `tools/call` result, by name.
    pub answers: HashMap<String, Value>,
    /// Answer every request with this status and a `location`, as a server
    /// sending the client somewhere else would.
    pub redirect: bool,
}

pub struct Fake {
    pub url: String,
    /// Every request, raw: request line, headers, body.
    pub seen: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    /// The JSON-RPC messages that arrived, in order.
    pub fn messages(&self) -> Vec<Value> {
        self.seen
            .lock()
            .expect("lock")
            .iter()
            .filter_map(|raw| raw.split_once("\r\n\r\n").and_then(|(_, body)| serde_json::from_str(body).ok()))
            .collect()
    }

    /// How many times a tool was called.
    pub fn calls_to(&self, tool: &str) -> usize {
        self.messages()
            .iter()
            .filter(|m| m["method"] == "tools/call" && m["params"]["name"] == tool)
            .count()
    }
}

/// A tool as `tools/list` describes it.
pub fn tool(name: &str, read_only: bool) -> Value {
    json!({
        "name": name,
        "description": format!("The {name} tool."),
        "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } } },
        "annotations": { "readOnlyHint": read_only },
    })
}

/// A `tools/call` result holding one piece of text.
pub fn text(said: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": said }] })
}

/// Name a server in a vault, agree to it on this computer, and connect — what
/// saving it in settings does, without the keychain.
pub async fn install(vault: &str, name: &str, url: &str) -> super::config::Server {
    use super::config::{Server, TransportConfig};
    let server = Server {
        id: format!("id-{}", super::config::slug(name)),
        name: name.to_string(),
        transport: TransportConfig::Http { url: url.to_string(), secret_headers: vec![] },
        enabled: true,
    };
    let mut config = super::config::load(vault);
    config.servers.push(server.clone());
    super::config::save(vault, &config).expect("saved");
    super::config::trust_here(vault, &server).expect("trusted");
    super::refresh(vault, &HashMap::new()).await;
    server
}

pub async fn serve(script: Script) -> Fake {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let raw = read_request(&mut socket).await;
            kept.lock().expect("lock").push(raw.clone());
            let (status, content_type, body, extra) = answer(&script, &raw);
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\n{extra}connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    Fake { url: format!("http://{addr}/mcp"), seen }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    while let Ok(n) = socket.read(&mut buf).await {
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
    String::from_utf8_lossy(&raw).to_string()
}

fn answer(script: &Script, raw: &str) -> (&'static str, &'static str, String, String) {
    if script.redirect {
        return ("307 Temporary Redirect", "text/plain", String::new(), "location: http://127.0.0.1:9/elsewhere\r\n".into());
    }
    if raw.starts_with("DELETE") {
        return ("200 OK", "text/plain", String::new(), String::new());
    }
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or_default();
    let Ok(message) = serde_json::from_str::<Value>(body) else {
        return ("400 Bad Request", "text/plain", String::new(), String::new());
    };
    let Some(id) = message.get("id").cloned() else {
        // A notification.
        return ("202 Accepted", "text/plain", String::new(), String::new());
    };
    let method = message["method"].as_str().unwrap_or_default();
    let mut extra = String::new();
    let result = match method {
        "initialize" => {
            extra.push_str("mcp-session-id: session-1\r\n");
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "fake", "version": "1" },
            })
        }
        "tools/list" => {
            let from: usize = message["params"]["cursor"].as_str().and_then(|c| c.parse().ok()).unwrap_or(0);
            let page = if script.page == 0 { script.tools.len() } else { script.page };
            let to = (from + page).min(script.tools.len());
            let mut result = json!({ "tools": script.tools[from..to] });
            if to < script.tools.len() {
                result["nextCursor"] = json!(to.to_string());
            }
            result
        }
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or_default();
            script.answers.get(name).cloned().unwrap_or_else(|| text("ok"))
        }
        _ => {
            let error = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "Method not found" } });
            return ("200 OK", "application/json", error.to_string(), extra);
        }
    };
    let response = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    if script.sse {
        // Something that is not the answer first, as a server reporting
        // progress would, then the answer.
        let progress = json!({ "jsonrpc": "2.0", "method": "notifications/progress", "params": { "progress": 1 } });
        let body = format!("event: message\ndata: {progress}\n\nevent: message\ndata: {response}\n\n");
        ("200 OK", "text/event-stream", body, extra)
    } else {
        ("200 OK", "application/json", response.to_string(), extra)
    }
}
