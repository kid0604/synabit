//! A REST API on 127.0.0.1 that answers from a script, for tests of declared
//! connectors. Keeps every request it was sent, so a test can see exactly what
//! went out — the path, the query, the headers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// One canned answer: status, extra header lines, body.
#[derive(Clone)]
pub struct Answer {
    pub status: &'static str,
    pub headers: String,
    pub body: String,
}

impl Answer {
    pub fn json(body: serde_json::Value) -> Self {
        Answer { status: "200 OK", headers: String::new(), body: body.to_string() }
    }
}

pub struct Fake {
    pub base: String,
    pub seen: Arc<Mutex<Vec<String>>>,
}

impl Fake {
    /// Request lines received, like `GET /v1/daily?month=2026-09 HTTP/1.1`.
    pub fn lines(&self) -> Vec<String> {
        self.seen.lock().expect("lock").iter().filter_map(|r| r.lines().next().map(str::to_string)).collect()
    }
    pub fn requests(&self) -> Vec<String> {
        self.seen.lock().expect("lock").clone()
    }
}

/// Answer by `METHOD /path` or by path (without the query); anything else is a 404.
pub async fn serve(routes: HashMap<&'static str, Answer>) -> Fake {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let mut raw = Vec::new();
            let mut buf = [0u8; 8192];
            while let Ok(n) = socket.read(&mut buf).await {
                if n == 0 {
                    break;
                }
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
            }
            let request = String::from_utf8_lossy(&raw).to_string();
            kept.lock().expect("lock").push(request.clone());
            let path = request.split_whitespace().nth(1).unwrap_or("/").split('?').next().unwrap_or("/").to_string();
            // `METHOD /path` first, so a WebDAV PROPFIND and REPORT on one
            // address can answer as a real server does; then the path alone.
            let method = request.split_whitespace().next().unwrap_or("GET");
            let answer = routes.get(format!("{method} {path}").as_str()).or_else(|| routes.get(path.as_str())).cloned().unwrap_or(Answer { status: "404 Not Found", headers: String::new(), body: "{}".into() });
            let response = format!(
                "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{}connection: close\r\n\r\n{}",
                answer.status,
                answer.body.len(),
                answer.headers,
                answer.body
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    Fake { base: format!("http://{addr}"), seen }
}
