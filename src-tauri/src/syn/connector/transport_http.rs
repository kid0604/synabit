//! A connector over Streamable HTTP: one POST per message, answered with JSON or with a
//! stream of server-sent events.
//!
//! # Why the address is not put through Syn's public-only guard
//!
//! `browse` refuses loopback and private addresses, because a page could steer
//! it at the router or at Ollama. An connector is the opposite case: very
//! often it *is* on this machine — a local bridge to a notes app, a database,
//! a company VPN — and the person typed its address into settings themselves.
//! Nobody else chooses where these requests go.
//!
//! So the rule is narrower and stricter than the guard: **exactly the URL the
//! user configured, and nowhere else.** Redirects are not followed — a server
//! answering 3xx is a server pointing somewhere nobody configured, and following
//! it would carry the configured secret header along. TLS is reqwest's normal
//! verification; there is no switch to turn it off.

use std::sync::Mutex;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;

use super::client::{answer_to, ConnectorError, SseParser};

/// The most an answer may be. A tool result the model will see at most a few
/// thousand characters of does not need to be read past this.
const MAX_BYTES: usize = 4 * 1024 * 1024;

const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";

/// Headers the user may not set, because this client sets them and a second
/// copy would be a question of which one the server reads.
pub const RESERVED_HEADERS: &[&str] =
    &["accept", "content-type", "content-length", "host", SESSION_HEADER, VERSION_HEADER];

pub struct Http {
    client: reqwest::Client,
    url: url::Url,
    /// Configured headers, secrets included. Held in memory for the life of the
    /// connection, never written anywhere.
    headers: Vec<(String, String)>,
    session: Mutex<Option<String>>,
    version: Mutex<Option<String>>,
}

impl Http {
    pub fn new(url: &str, headers: Vec<(String, String)>) -> Result<Self, ConnectorError> {
        let url = url::Url::parse(url.trim())
            .map_err(|e| ConnectorError::Unreachable(format!("the address is not a URL: {e}")))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ConnectorError::Unreachable("only http and https addresses are used".into()));
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("Synabit/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ConnectorError::Unreachable(format!("could not set up the connection: {e}")))?;
        Ok(Self { client, url, headers, session: Mutex::new(None), version: Mutex::new(None) })
    }

    /// The version both sides agreed, sent on every request after the handshake.
    pub fn agreed(&self, version: &str) {
        if let Ok(mut v) = self.version.lock() {
            *v = Some(version.to_string());
        }
    }

    fn session_id(&self) -> Option<String> {
        self.session.lock().ok().and_then(|s| s.clone())
    }

    fn build(&self, method: reqwest::Method) -> reqwest::RequestBuilder {
        let mut builder = self.client.request(method, self.url.clone());
        for (name, value) in &self.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if let Some(session) = self.session_id() {
            builder = builder.header(SESSION_HEADER, session);
        }
        if let Some(version) = self.version.lock().ok().and_then(|v| v.clone()) {
            builder = builder.header(VERSION_HEADER, version);
        }
        builder
    }

    async fn post(&self, message: &Value) -> Result<reqwest::Response, ConnectorError> {
        let had_session = self.session_id().is_some();
        let response = self
            .build(reqwest::Method::POST)
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .body(message.to_string())
            .send()
            .await
            .map_err(|e| ConnectorError::Unreachable(format!("could not reach the server: {}", without_url(&e))))?;

        if let Some(session) = response.headers().get(SESSION_HEADER).and_then(|v| v.to_str().ok()) {
            if let Ok(mut s) = self.session.lock() {
                *s = Some(session.to_string());
            }
        }

        let status = response.status();
        if status.is_redirection() {
            return Err(ConnectorError::Unreachable(format!(
                "the server answered {status} and pointed elsewhere; only the address in settings is used"
            )));
        }
        if status == reqwest::StatusCode::NOT_FOUND && had_session {
            if let Ok(mut s) = self.session.lock() {
                *s = None;
            }
            return Err(ConnectorError::SessionGone);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ConnectorError::Unreachable(format!(
                "the server refused the request ({status}); check the header and its secret"
            )));
        }
        // The body is not read: it is the server's words, and this message goes
        // to the model and the settings screen as ours.
        if !status.is_success() {
            return Err(ConnectorError::Unreachable(format!("the server answered {status}")));
        }
        Ok(response)
    }

    /// Send a request and read until its answer arrives.
    pub async fn exchange(&self, id: u64, message: &Value, within: Duration) -> Result<Value, ConnectorError> {
        tokio::time::timeout(within, async {
            let response = self.post(message).await?;
            let streamed = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|t| t.to_ascii_lowercase().starts_with("text/event-stream"));
            if streamed {
                read_events(response, id).await
            } else {
                let body = crate::feed_engine::fetcher::read_capped(response, MAX_BYTES)
                    .await
                    .map_err(ConnectorError::Unreachable)?;
                let message: Value = serde_json::from_slice(&body)
                    .map_err(|_| ConnectorError::Protocol("the answer was not JSON".into()))?;
                answer_to(id, &message)
                    .unwrap_or_else(|| Err(ConnectorError::Protocol("the answer did not answer the request".into())))
            }
        })
        .await
        .map_err(|_| ConnectorError::Timeout)?
    }

    /// Send a notification. The server answers 202 with nothing to read.
    pub async fn notify(&self, message: &Value) -> Result<(), ConnectorError> {
        tokio::time::timeout(super::client::CONNECT_TIMEOUT, self.post(message))
            .await
            .map_err(|_| ConnectorError::Timeout)?
            .map(|_| ())
    }

    /// End the session, if the server gave one. Best effort: a server that has
    /// gone away has ended it already.
    pub async fn close(&self) {
        if self.session_id().is_none() {
            return;
        }
        let _ = tokio::time::timeout(Duration::from_secs(3), self.build(reqwest::Method::DELETE).send()).await;
        if let Ok(mut s) = self.session.lock() {
            *s = None;
        }
    }
}

/// Read a stream of events until one of them answers `id`.
///
/// Bytes are split into lines before they are decoded, so a character cut in
/// two by a chunk boundary arrives whole.
async fn read_events(response: reqwest::Response, id: u64) -> Result<Value, ConnectorError> {
    let mut stream = response.bytes_stream();
    let mut parser = SseParser::default();
    let mut pending: Vec<u8> = Vec::new();
    let mut read = 0usize;

    let consider = |event: String| -> Option<Result<Value, ConnectorError>> {
        let message: Value = serde_json::from_str(&event).ok()?;
        answer_to(id, &message)
    };

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ConnectorError::Unreachable(format!("the stream broke: {}", without_url(&e))))?;
        read += chunk.len();
        if read > MAX_BYTES {
            return Err(ConnectorError::Protocol("the answer was larger than this app reads".into()));
        }
        pending.extend_from_slice(&chunk);
        if let Some(cut) = pending.iter().rposition(|b| *b == b'\n') {
            let whole: Vec<u8> = pending.drain(..=cut).collect();
            for event in parser.push(&String::from_utf8_lossy(&whole)) {
                if let Some(answer) = consider(event) {
                    return answer;
                }
            }
        }
    }
    parser.push(&String::from_utf8_lossy(&pending));
    if let Some(answer) = parser.finish().and_then(consider) {
        return answer;
    }
    Err(ConnectorError::Protocol("the stream ended without an answer".into()))
}

/// A reqwest error without the URL in it. The URL may carry a token in its
/// query, and this message is shown on screen and handed to the model.
fn without_url(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "connection refused or not found"
    } else if e.is_body() || e.is_decode() {
        "the answer could not be read"
    } else {
        "the request failed"
    };
    kind.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_addresses_are_accepted() {
        assert!(Http::new("https://connector.example/mcp", vec![]).is_ok());
        assert!(Http::new("http://127.0.0.1:8080/mcp", vec![]).is_ok());
        assert!(Http::new("file:///etc/passwd", vec![]).is_err());
        assert!(Http::new("not a url", vec![]).is_err());
    }
}
