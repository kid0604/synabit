//! The app's side of `synabit-safe run`: a socket that resolves `safe:`
//! references, once the user says yes. See `safe::cli` for the other side.
//!
//! Owner-only socket, same-user peers only, one request per connection, off
//! until the user turns it on. Each request shows a card naming the command,
//! the folder it runs in and every secret it wants — the values go to that
//! command's environment and nowhere else, and no answer within a minute is
//! a no.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use super::cli::{RunReply, RunRequest};
use super::item::SecretString;

/// A request line longer than this is not one.
const MAX_REQUEST: usize = 64 * 1024;

pub trait Resolver {
    /// The values for `request`, or why not. Asking the user happens here.
    fn resolve(&self, request: &RunRequest) -> Result<BTreeMap<String, SecretString>, String>;
}

pub async fn serve(path: PathBuf, resolver: Arc<dyn Resolver + Send + Sync>) -> std::io::Result<()> {
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
        if !stream.peer_cred().map(|c| c.uid() == me).unwrap_or(false) {
            log::warn!("[Safe] refused a command-line connection from another user");
            continue;
        }
        let resolver = resolver.clone();
        tokio::spawn(async move {
            if let Err(e) = one(stream, resolver).await {
                log::debug!("[Safe] command-line request ended: {e}");
            }
        });
    }
}

async fn one(stream: tokio::net::UnixStream, resolver: Arc<dyn Resolver + Send + Sync>) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    let mut reader = BufReader::new(read).take(MAX_REQUEST as u64);
    reader.read_line(&mut line).await?;
    let reply = match serde_json::from_str::<RunRequest>(&line) {
        Ok(request) if request.v == 1 && !request.command.is_empty() => {
            tokio::task::spawn_blocking(move || match resolver.resolve(&request) {
                Ok(values) => RunReply { ok: true, env: values.into_iter().map(|(k, v)| (k, v.expose().to_string())).collect(), error: None },
                Err(e) => RunReply { ok: false, env: BTreeMap::new(), error: Some(e) },
            })
            .await
            .map_err(std::io::Error::other)?
        }
        _ => RunReply { ok: false, env: BTreeMap::new(), error: Some("not a request this Synabit understands".into()) },
    };
    let mut out = zeroize::Zeroizing::new(serde_json::to_vec(&reply).map_err(std::io::Error::other)?);
    out.push(b'\n');
    write.write_all(&out).await
}

// ─── in the app ──────────────────────────────────────────

pub const APPROVE_EVENT: &str = "safe://cli-approve";

pub struct SafeCli {
    pub app: tauri::AppHandle,
}

impl Resolver for SafeCli {
    fn resolve(&self, request: &RunRequest) -> Result<BTreeMap<String, SecretString>, String> {
        use tauri::Emitter;
        let session = super::session::global();
        let vault = session.open_vault().ok_or("the Safe is locked; unlock it in Synabit and try again")?;
        let values = session
            .peek(&vault, |u| {
                if !u.settings().cli {
                    return Err(super::session::SafeError::Failed("the command line is turned off in Safe's settings".into()));
                }
                request
                    .secrets
                    .iter()
                    .map(|(name, reference)| u.cli_value(reference).map(|v| (name.clone(), v)).map_err(super::session::SafeError::Failed))
                    .collect::<Result<BTreeMap<_, _>, _>>()
            })
            .map_err(|e| e.to_string())?;
        let question = super::approvals::ask();
        let card = serde_json::json!({
            "id": question.id,
            "command": request.command.join(" ").chars().take(300).collect::<String>(),
            "cwd": request.cwd,
            "secrets": request.secrets.iter().map(|(n, r)| format!("{n} ← {r}")).collect::<Vec<_>>(),
        });
        let yes = if self.app.emit(APPROVE_EVENT, card).is_ok() { question.wait(super::approvals::WAIT) } else { question.abandon() };
        if yes {
            Ok(values)
        } else {
            Err("not allowed in Synabit".into())
        }
    }
}

static RUNNING: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = std::sync::Mutex::new(None);

pub fn start(app: &tauri::AppHandle) {
    use tauri::Manager;
    let mut running = RUNNING.lock().unwrap_or_else(|p| p.into_inner());
    if running.is_some() {
        return;
    }
    let Ok(dir) = app.path().app_data_dir() else { return };
    let path = dir.join(super::cli::SOCKET_NAME);
    let resolver: Arc<dyn Resolver + Send + Sync> = Arc::new(SafeCli { app: app.clone() });
    *running = Some(tauri::async_runtime::spawn(async move {
        if let Err(e) = serve(path, resolver).await {
            log::error!("[Safe] the command-line socket stopped: {e}");
        }
    }));
}

pub fn stop(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(task) = RUNNING.lock().unwrap_or_else(|p| p.into_inner()).take() {
        task.abort();
        if let Ok(dir) = app.path().app_data_dir() {
            let _ = std::fs::remove_file(dir.join(super::cli::SOCKET_NAME));
        }
    }
}

pub fn running() -> bool {
    RUNNING.lock().unwrap_or_else(|p| p.into_inner()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(bool);
    impl Resolver for Fixed {
        fn resolve(&self, request: &RunRequest) -> Result<BTreeMap<String, SecretString>, String> {
            if !self.0 {
                return Err("not allowed in Synabit".into());
            }
            Ok(request.secrets.keys().map(|k| (k.clone(), SecretString::new("s3cret-canary".into()))).collect())
        }
    }

    /// The command runs with the value in its environment; nothing prints it.
    #[tokio::test]
    async fn a_command_gets_its_secret_and_a_refusal_runs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let sock = std::env::temp_dir().join(format!("sc-{}.sock", std::process::id()));
        let server = tokio::spawn(serve(sock.clone(), Arc::new(Fixed(true))));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let out = dir.path().join("out");
        let script = format!("printf %s \"$TOKEN\" > '{}'", out.display());
        let request = RunRequest { v: 1, command: vec!["sh".into(), "-c".into(), script.clone()], cwd: "/".into(), secrets: [("TOKEN".to_string(), "github-token".to_string())].into() };
        let s = sock.clone();
        let reply = tokio::task::spawn_blocking(move || super::super::cli::ask(&s, &request)).await.unwrap().unwrap();
        assert!(reply.ok);
        let status = std::process::Command::new("sh").arg("-c").arg(&script).envs(reply.env.iter()).status().unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "s3cret-canary");
        server.abort();
        let _ = std::fs::remove_file(&sock);

        let sock = std::env::temp_dir().join(format!("sd-{}.sock", std::process::id()));
        let server = tokio::spawn(serve(sock.clone(), Arc::new(Fixed(false))));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let request = RunRequest { v: 1, command: vec!["true".into()], cwd: "/".into(), secrets: [("T".to_string(), "x".to_string())].into() };
        let s = sock.clone();
        let reply = tokio::task::spawn_blocking(move || super::super::cli::ask(&s, &request)).await.unwrap().unwrap();
        assert!(!reply.ok && reply.env.is_empty());
        server.abort();
        let _ = std::fs::remove_file(&sock);
    }
}
