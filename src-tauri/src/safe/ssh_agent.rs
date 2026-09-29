//! Safe's SSH agent: a Unix socket `ssh` and `git` can use, holding the
//! Ed25519 keys kept in the Safe. Section 10.5 of the design.
//!
//! Point `SSH_AUTH_SOCK` at [`socket_path`] and every `ssh`, `git push` and
//! `ssh-add -L` uses keys that never touch the disk unencrypted and never
//! leave this process: the agent signs, it does not hand keys out.
//!
//! # Who may connect
//!
//! The socket is created owner-only (0600) and every connection's peer is
//! checked to be the same user — another account on the machine cannot use
//! your keys even if it finds the socket.
//!
//! # When it signs
//!
//! Only while the Safe is open: locked, it lists no keys. And, unless the user
//! turned it off, only after they say yes to that signature in the app — the
//! card names the key and the user it logs in as. Every signature raises a
//! notification either way, so none happens unseen.
//!
//! macOS and Linux. Windows speaks the agent protocol over a named pipe, which
//! this build does not open yet.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::ssh::{self, Agent};

/// Where the socket lives: beside the app's own data, so it goes with the app.
pub fn socket_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("ssh-agent.sock")
}

/// Serve the agent protocol on `path` until the task is aborted.
///
/// A stale socket from an earlier run is removed first — binding over it
/// fails otherwise — but only if it is a socket: a file at that path that is
/// anything else is not ours to delete.
pub async fn serve(path: PathBuf, agent: Arc<dyn Agent + Send + Sync>) -> std::io::Result<()> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_socket() {
            std::fs::remove_file(&path)?;
        } else {
            return Err(std::io::Error::other(format!("{} exists and is not a socket", path.display())));
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    // SAFETY: getuid cannot fail.
    let me = unsafe { libc::getuid() };
    loop {
        let (stream, _) = listener.accept().await?;
        let same_user = stream.peer_cred().map(|c| c.uid() == me).unwrap_or(false);
        if !same_user {
            log::warn!("[Safe] refused an SSH agent connection from another user");
            continue;
        }
        let agent = agent.clone();
        tokio::spawn(async move {
            if let Err(e) = connection(stream, agent).await {
                log::debug!("[Safe] SSH agent connection ended: {e}");
            }
        });
    }
}

async fn connection(mut stream: tokio::net::UnixStream, agent: Arc<dyn Agent + Send + Sync>) -> std::io::Result<()> {
    loop {
        let mut len = [0u8; 4];
        if stream.read_exact(&mut len).await.is_err() {
            return Ok(()); // the client hung up
        }
        let len = u32::from_be_bytes(len) as usize;
        if len > ssh::MAX_MESSAGE {
            return Err(std::io::Error::other("message too large"));
        }
        let mut body = zeroize::Zeroizing::new(vec![0u8; len]);
        stream.read_exact(&mut body).await?;
        // Answering may wait on the user (the approval card), so it runs off
        // the async threads.
        let agent = agent.clone();
        let reply = tokio::task::spawn_blocking(move || ssh::handle(&body, agent.as_ref()))
            .await
            .map_err(std::io::Error::other)?;
        stream.write_all(&ssh::frame(&reply)).await?;
    }
}

// ─── in the app ──────────────────────────────────────────

/// The agent the app runs: keys from the open Safe, approval from the user.
pub struct SafeAgent {
    pub app: tauri::AppHandle,
}

pub const APPROVE_EVENT: &str = "safe://ssh-approve";
/// How long a signature waits for an answer before it is refused.
const APPROVE_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

static WAITING: std::sync::Mutex<Option<std::collections::HashMap<String, std::sync::mpsc::Sender<bool>>>> =
    std::sync::Mutex::new(None);

/// The user's answer to one approval card.
pub fn answer(id: &str, allow: bool) -> bool {
    let mut guard = WAITING.lock().unwrap_or_else(|p| p.into_inner());
    guard.as_mut().and_then(|m| m.remove(id)).is_some_and(|tx| tx.send(allow).is_ok())
}

fn settings() -> Option<super::session::Settings> {
    let session = super::session::global();
    let vault = session.open_vault()?;
    session.peek(&vault, |u| Ok(u.settings())).ok()
}

impl Agent for SafeAgent {
    fn keys(&self) -> Vec<(String, ssh::Ed25519Key)> {
        let session = super::session::global();
        let Some(vault) = session.open_vault() else { return Vec::new() };
        let pems = session.peek(&vault, |u| Ok(u.ssh_keys())).unwrap_or_default();
        pems.into_iter()
            .filter_map(|(title, pem)| match ssh::parse(pem.expose()) {
                Ok(key) => Some((title, key)),
                Err(e) => {
                    log::info!("[Safe] SSH agent skipped “{title}”: {e}");
                    None
                }
            })
            .collect()
    }

    fn allow(&self, request: &ssh::SignRequest) -> bool {
        use tauri::Emitter;
        use tauri_plugin_notification::NotificationExt;
        let Some(settings) = settings() else { return false };
        let who = request.login_as.as_deref().map(|u| format!(" as {u}")).unwrap_or_default();
        let allowed = if !settings.ssh_confirm {
            true
        } else {
            let id = hex::encode(super::crypto::random_bytes::<8>().unwrap_or_default());
            let (tx, rx) = std::sync::mpsc::channel();
            WAITING.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(Default::default).insert(id.clone(), tx);
            let shown = self
                .app
                .emit(APPROVE_EVENT, serde_json::json!({ "id": id, "request": request }))
                .is_ok();
            let answer = if shown { rx.recv_timeout(APPROVE_WAIT).unwrap_or(false) } else { false };
            WAITING.lock().unwrap_or_else(|p| p.into_inner()).as_mut().map(|m| m.remove(&id));
            answer
        };
        if allowed {
            // Every signature is seen, asked about or not.
            let _ = self.app.notification().builder().title("Safe · SSH").body(format!("Signed{who} with “{}”", request.key)).show();
        }
        allowed
    }
}

static RUNNING: std::sync::Mutex<Option<(tauri::async_runtime::JoinHandle<()>, PathBuf)>> = std::sync::Mutex::new(None);

/// Start the agent, if it is not running. Idempotent.
pub fn start(app: &tauri::AppHandle) {
    use tauri::Manager;
    let mut running = RUNNING.lock().unwrap_or_else(|p| p.into_inner());
    if running.is_some() {
        return;
    }
    let Ok(dir) = app.path().app_data_dir() else { return };
    let path = socket_path(&dir);
    let agent: Arc<dyn Agent + Send + Sync> = Arc::new(SafeAgent { app: app.clone() });
    let serving = path.clone();
    let task = tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(serving, agent).await {
            log::error!("[Safe] the SSH agent stopped: {e}");
        }
    });
    log::info!("[Safe] SSH agent listening at {}", path.display());
    *running = Some((task, path));
}

/// Stop the agent and remove its socket.
pub fn stop() {
    if let Some((task, path)) = RUNNING.lock().unwrap_or_else(|p| p.into_inner()).take() {
        task.abort();
        let _ = std::fs::remove_file(path);
    }
}

pub fn running_at() -> Option<PathBuf> {
    RUNNING.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map(|(_, p)| p.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safe::ssh::{Ed25519Key, SignRequest};

    struct One {
        pem: String,
        allow: bool,
    }

    impl Agent for One {
        fn keys(&self) -> Vec<(String, Ed25519Key)> {
            vec![("Deploy key".into(), ssh::parse(&self.pem).unwrap())]
        }
        fn allow(&self, _: &SignRequest) -> bool {
            self.allow
        }
    }

    fn have(tool: &str) -> bool {
        std::process::Command::new("sh").arg("-c").arg(format!("command -v {tool}")).output().is_ok_and(|o| o.status.success())
    }

    /// OpenSSH's own tools, talking to the agent: `ssh-add -L` lists the key,
    /// `ssh-keygen -Y sign` signs through it, and `ssh-keygen -Y verify`
    /// accepts the signature — the same path `git` commit signing and `ssh`
    /// logins take.
    #[tokio::test]
    async fn openssh_lists_and_signs_through_the_agent() {
        if !have("ssh-keygen") || !have("ssh-add") {
            eprintln!("OpenSSH is not installed here; skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("k");
        let made = std::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "test@synabit", "-f"])
            .arg(&key)
            .status()
            .unwrap();
        assert!(made.success());
        let pem = std::fs::read_to_string(&key).unwrap();
        let public = std::fs::read_to_string(key.with_extension("pub")).unwrap();
        std::fs::remove_file(&key).unwrap(); // only the agent can sign now

        // Socket paths are short on macOS (104 bytes); a temp dir can be long.
        let sock = std::env::temp_dir().join(format!("sa-{}.sock", std::process::id()));
        let server = tokio::spawn(serve(sock.clone(), Arc::new(One { pem, allow: true })));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let listed = tokio::process::Command::new("ssh-add").arg("-L").env("SSH_AUTH_SOCK", &sock).output().await.unwrap();
        let listed = String::from_utf8_lossy(&listed.stdout);
        let key_part = public.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
        assert!(listed.contains(&key_part), "ssh-add -L said: {listed}");

        let message = dir.path().join("message");
        std::fs::write(&message, b"signed through Safe\n").unwrap();
        let signed = tokio::process::Command::new("ssh-keygen")
            .args(["-Y", "sign", "-n", "file", "-f"])
            .arg(key.with_extension("pub"))
            .arg(&message)
            .env("SSH_AUTH_SOCK", &sock)
            .output()
            .await
            .unwrap();
        assert!(signed.status.success(), "{}", String::from_utf8_lossy(&signed.stderr));

        let signers = dir.path().join("allowed");
        std::fs::write(&signers, format!("test@synabit {key_part}\n")).unwrap();
        let verified = std::process::Command::new("ssh-keygen")
            .args(["-Y", "verify", "-n", "file", "-I", "test@synabit", "-f"])
            .arg(&signers)
            .arg("-s")
            .arg(message.with_extension("sig"))
            .stdin(std::fs::File::open(&message).unwrap())
            .output()
            .unwrap();
        assert!(verified.status.success(), "{}", String::from_utf8_lossy(&verified.stderr));

        server.abort();
        let _ = std::fs::remove_file(&sock);
    }

    #[tokio::test]
    async fn a_refused_signature_fails_in_openssh_too() {
        if !have("ssh-keygen") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("k");
        std::process::Command::new("ssh-keygen").args(["-q", "-t", "ed25519", "-N", "", "-f"]).arg(&key).status().unwrap();
        let pem = std::fs::read_to_string(&key).unwrap();
        std::fs::remove_file(&key).unwrap();
        let sock = std::env::temp_dir().join(format!("sb-{}.sock", std::process::id()));
        let server = tokio::spawn(serve(sock.clone(), Arc::new(One { pem, allow: false })));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let message = dir.path().join("m");
        std::fs::write(&message, b"x").unwrap();
        let signed = tokio::process::Command::new("ssh-keygen")
            .args(["-Y", "sign", "-n", "file", "-f"])
            .arg(key.with_extension("pub"))
            .arg(&message)
            .env("SSH_AUTH_SOCK", &sock)
            .output()
            .await
            .unwrap();
        assert!(!signed.status.success(), "a refused signature was made");
        server.abort();
        let _ = std::fs::remove_file(&sock);
    }

    #[tokio::test]
    async fn a_file_that_is_not_a_socket_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sock");
        std::fs::write(&path, b"mine").unwrap();
        let agent = Arc::new(One { pem: String::new(), allow: false });
        assert!(serve(path.clone(), agent).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"mine");
    }
}
