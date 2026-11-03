//! The one way out of a room that has a network: a port on this machine, open
//! only while that room runs, that Rust answers.
//!
//! A room with a network may reach exactly one address, `localhost:<port>`
//! (`net.sb`). What listens there is this: it takes `CONNECT host:443`, decides
//! whether that host is allowed, finds its address itself, refuses one on the
//! person's own network, and only then joins the two ends. Nothing in the room
//! resolves a name, so there is no DNS to leak through, and nothing but TLS goes
//! through, so a plain `GET http://` is refused rather than read.
//!
//! The proxy cannot see inside TLS, and does not try: it cannot tell a download
//! from an upload. That is acceptable only because no room with a network holds
//! the person's data (design §3, I1). What it can do is say where a room went:
//! every request, allowed or not, is written to the audit with the bytes each way.

use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// The longest a request line and its headers may be. A real client sends a
/// few hundred bytes.
const HEAD_MOST: usize = 8 << 10;
/// How long a client has to say where it wants to go, and the far end to answer.
const ASK_WITHIN: Duration = Duration::from_secs(15);
const CONNECT_WITHIN: Duration = Duration::from_secs(20);

/// Which hosts a room may reach.
#[derive(Clone, Debug)]
pub enum Allow {
    /// Only these, and their subdomains: a registry and where it keeps files.
    Hosts(Vec<String>),
    /// Any public host. For the helper's room (M6); nothing here uses it yet.
    Public,
}

impl Allow {
    fn admits(&self, host: &str) -> bool {
        match self {
            Allow::Public => true,
            Allow::Hosts(hosts) => hosts.iter().any(|h| host == h || host.ends_with(&format!(".{h}"))),
        }
    }
}

/// What the registries a preparing room may reach are, for Python: PyPI, where
/// PyPI keeps its files, and where `uv` downloads a Python from.
pub fn python_registries() -> Allow {
    Allow::Hosts(
        ["pypi.org", "files.pythonhosted.org", "github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"]
            .map(String::from)
            .to_vec(),
    )
}

/// One request, as the audit keeps it.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Went {
    pub at: String,
    pub room: String,
    pub host: String,
    pub port: u16,
    /// Why not, when it was refused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    pub sent: u64,
    pub received: u64,
}

/// A proxy for one room. Dropping it closes the port and every connection
/// through it.
pub struct Proxy {
    pub port: u16,
    listening: JoinHandle<()>,
    joined: Arc<Mutex<Vec<JoinHandle<()>>>>,
    went: Arc<Mutex<Vec<Went>>>,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.listening.abort();
        if let Ok(joined) = self.joined.lock() {
            for one in joined.iter() {
                one.abort();
            }
        }
    }
}

impl Proxy {
    /// Opens a port on the loopback address, chosen by the system, for `room`.
    /// `audit` is a file each request is appended to, one JSON object a line.
    pub async fn open(room: &str, allow: Allow, audit: &Path) -> std::io::Result<Self> {
        let listener = TcpListener::bind((IpAddr::from([127, 0, 0, 1]), 0)).await?;
        let port = listener.local_addr()?.port();
        let joined: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::default();
        let went: Arc<Mutex<Vec<Went>>> = Arc::default();
        let (room, audit) = (room.to_string(), audit.to_path_buf());
        let listening = {
            let (joined, went) = (joined.clone(), went.clone());
            tokio::spawn(async move {
                while let Ok((client, _)) = listener.accept().await {
                    let (allow, room, audit, went) = (allow.clone(), room.clone(), audit.clone(), went.clone());
                    let one = tokio::spawn(async move {
                        let record = serve(client, &allow, &room).await;
                        keep(&audit, &record);
                        if let Ok(mut went) = went.lock() {
                            went.push(record);
                        }
                    });
                    if let Ok(mut joined) = joined.lock() {
                        joined.retain(|j| !j.is_finished());
                        joined.push(one);
                    }
                }
            })
        };
        Ok(Self { port, listening, joined, went })
    }

    /// Every request so far, finished ones only.
    pub fn went(&self) -> Vec<Went> {
        self.went.lock().map(|w| w.clone()).unwrap_or_default()
    }
}

fn keep(audit: &Path, record: &Went) {
    let Ok(line) = serde_json::to_string(record) else { return };
    if let Some(folder) = audit.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(audit) {
        let _ = writeln!(file, "{line}");
    }
}

/// `CONNECT host:port HTTP/1.1`, and nothing else.
fn asked_for(head: &str) -> Result<(String, u16), String> {
    let line = head.lines().next().unwrap_or_default();
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err("not a request".into());
    };
    if method != "CONNECT" {
        return Err(format!("{method} is not CONNECT: only TLS goes through"));
    }
    let (host, port) = target.rsplit_once(':').ok_or("no port")?;
    let host = host.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
    let port: u16 = port.parse().map_err(|_| "not a port")?;
    if host.is_empty() || host.parse::<IpAddr>().is_ok() {
        return Err("an address, not a name: a room asks for a host by name".into());
    }
    Ok((host, port))
}

/// Where `host` is, on the public network only. A name that resolves to the
/// person's own network is refused whatever else it resolves to.
async fn public_address(host: &str, port: u16) -> Result<SocketAddr, String> {
    let found: Vec<SocketAddr> = tokio::time::timeout(CONNECT_WITHIN, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| "no answer for that name".to_string())?
        .map_err(|e| format!("no such host: {e}"))?
        .collect();
    if found.iter().any(|a| crate::syn::connector::instance::internal_ip(a.ip())) {
        return Err("it is on this machine's own network".into());
    }
    found.into_iter().next().ok_or_else(|| "no address for that name".into())
}

async fn serve(mut client: TcpStream, allow: &Allow, room: &str) -> Went {
    let mut went = Went {
        at: chrono::Utc::now().to_rfc3339(),
        room: room.to_string(),
        host: String::new(),
        port: 0,
        refused: None,
        sent: 0,
        received: 0,
    };
    let refuse = |went: &mut Went, why: String| {
        went.refused = Some(why);
    };

    let mut head = Vec::new();
    let read = tokio::time::timeout(ASK_WITHIN, async {
        let mut byte = [0u8; 1];
        while head.len() < HEAD_MOST && !head.ends_with(b"\r\n\r\n") {
            if client.read(&mut byte).await? == 0 {
                break;
            }
            head.push(byte[0]);
        }
        std::io::Result::Ok(())
    })
    .await;
    if !matches!(read, Ok(Ok(()))) || !head.ends_with(b"\r\n\r\n") {
        refuse(&mut went, "the request was not finished".into());
        let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
        return went;
    }

    let (host, port) = match asked_for(&String::from_utf8_lossy(&head)) {
        Ok(asked) => asked,
        Err(why) => {
            refuse(&mut went, why);
            let _ = client.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n").await;
            return went;
        }
    };
    went.host = host.clone();
    went.port = port;
    if port != 443 {
        refuse(&mut went, "only port 443".into());
    } else if !allow.admits(&host) {
        refuse(&mut went, "not a host this room may reach".into());
    }
    if went.refused.is_some() {
        let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
        return went;
    }
    let address = match public_address(&host, port).await {
        Ok(address) => address,
        Err(why) => {
            refuse(&mut went, why);
            let _ = client.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
            return went;
        }
    };
    let mut far = match tokio::time::timeout(CONNECT_WITHIN, TcpStream::connect(address)).await {
        Ok(Ok(far)) => far,
        _ => {
            refuse(&mut went, "the host did not answer".into());
            let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
            return went;
        }
    };
    if client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await.is_err() {
        return went;
    }
    if let Ok((sent, received)) = tokio::io::copy_bidirectional(&mut client, &mut far).await {
        went.sent = sent;
        went.received = received;
    }
    went
}

/// Where a room's requests are written down.
pub fn audit_file(root: &Path) -> PathBuf {
    root.join("audit.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_connect_to_a_name_is_a_request() {
        assert_eq!(asked_for("CONNECT pypi.org:443 HTTP/1.1\r\nHost: pypi.org\r\n\r\n"), Ok(("pypi.org".into(), 443)));
        assert_eq!(asked_for("CONNECT PyPI.org:443 HTTP/1.1\r\n\r\n").map(|a| a.0), Ok("pypi.org".into()));
        for bad in [
            "GET http://pypi.org/ HTTP/1.1\r\n\r\n",
            "CONNECT 10.0.0.1:443 HTTP/1.1\r\n\r\n",
            "CONNECT [::1]:443 HTTP/1.1\r\n\r\n",
            "CONNECT pypi.org HTTP/1.1\r\n\r\n",
            "",
        ] {
            assert!(asked_for(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_registry_list_admits_its_hosts_and_their_subdomains_only() {
        let allow = python_registries();
        assert!(allow.admits("pypi.org"));
        assert!(allow.admits("files.pythonhosted.org"));
        for not in ["evil.example", "pypi.org.evil.example", "notpypi.org", "githubusercontent.com"] {
            assert!(!allow.admits(not), "{not}");
        }
    }

    #[test]
    fn the_persons_own_network_is_never_public() {
        for ip in ["127.0.0.1", "10.1.2.3", "192.168.1.1", "172.16.0.1", "169.254.1.1", "100.64.0.1", "0.0.0.0", "::1", "fd00::1", "fe80::1", "::ffff:10.0.0.1", "224.0.0.1"] {
            assert!(crate::syn::connector::instance::internal_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "151.101.0.223", "2606:4700::1111"] {
            assert!(!crate::syn::connector::instance::internal_ip(ip.parse().unwrap()), "{ip}");
        }
    }

    async fn ask(port: u16, request: &str) -> String {
        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        client.write_all(request.as_bytes()).await.unwrap();
        let mut answer = vec![0u8; 64];
        let n = tokio::time::timeout(Duration::from_secs(30), client.read(&mut answer)).await.unwrap().unwrap();
        String::from_utf8_lossy(&answer[..n]).to_string()
    }

    /// What is refused is answered at once and written down; nothing refused
    /// reaches the network. `localhost` is a name that resolves to this machine.
    #[tokio::test]
    async fn what_is_refused_is_answered_and_written_down() {
        let dir = tempfile::tempdir().unwrap();
        let audit = audit_file(dir.path());
        let proxy = Proxy::open("c-test", Allow::Hosts(vec!["localhost".into(), "pypi.org".into()]), &audit).await.unwrap();

        assert!(ask(proxy.port, "GET http://pypi.org/ HTTP/1.1\r\n\r\n").await.starts_with("HTTP/1.1 405"));
        assert!(ask(proxy.port, "CONNECT evil.example:443 HTTP/1.1\r\n\r\n").await.starts_with("HTTP/1.1 403"));
        assert!(ask(proxy.port, "CONNECT pypi.org:80 HTTP/1.1\r\n\r\n").await.starts_with("HTTP/1.1 403"));
        assert!(ask(proxy.port, "CONNECT localhost:443 HTTP/1.1\r\n\r\n").await.starts_with("HTTP/1.1 403"));

        tokio::time::sleep(Duration::from_millis(200)).await;
        let went = proxy.went();
        assert_eq!(went.len(), 4, "{went:?}");
        assert!(went.iter().all(|w| w.refused.is_some() && w.sent == 0 && w.received == 0), "{went:?}");
        let local = went.iter().find(|w| w.host == "localhost").unwrap();
        assert!(local.refused.as_deref().unwrap().contains("own network"), "{local:?}");
        let written = std::fs::read_to_string(&audit).unwrap();
        assert_eq!(written.lines().count(), 4);
        assert!(written.contains("evil.example"));
    }

    /// Closing the proxy closes the port.
    #[tokio::test]
    async fn a_closed_proxy_is_not_there() {
        let dir = tempfile::tempdir().unwrap();
        let proxy = Proxy::open("c-test", python_registries(), &audit_file(dir.path())).await.unwrap();
        let port = proxy.port;
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_ok());
        drop(proxy);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    }

    /// The real thing, through a real registry. Needs the network.
    #[tokio::test]
    #[ignore = "reaches pypi.org; run by hand"]
    async fn a_registry_is_reached_through_it() {
        let dir = tempfile::tempdir().unwrap();
        let proxy = Proxy::open("c-test", python_registries(), &audit_file(dir.path())).await.unwrap();
        // Awaited, not blocking: the proxy answers on this same thread.
        let out = tokio::process::Command::new("/usr/bin/curl")
            .args(["-sS", "-o", "/dev/null", "-w", "%{http_code}", "--proxy"])
            .arg(format!("http://127.0.0.1:{}", proxy.port))
            .arg("https://pypi.org/simple/idna/")
            .output()
            .await
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "200");
        // Written down when the connection has finished on this side too.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let went = proxy.went();
        assert!(went.iter().any(|w| w.host == "pypi.org" && w.refused.is_none() && w.received > 0), "{went:?}");
    }
}
