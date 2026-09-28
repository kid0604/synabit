//! A connector over stdio: a program on this computer, spoken to one JSON line at a time.
//!
//! # What runs, exactly
//!
//! The command the user typed, with the arguments they typed, as they typed
//! them. `tokio::process::Command` starts the program directly — **never a
//! shell**, so a `;` or a `$(…)` in an argument is that character in that
//! argument and nothing more — and never through `tauri-plugin-shell`, whose
//! scope rules are written for the webview and are the wrong place for a
//! decision the person made in Syn's settings.
//!
//! The environment is the app's own plus the variables the user named, whose
//! values come from the keychain. The program's stderr goes to the log, where
//! somebody diagnosing a server can read it; it never reaches the model.
//!
//! The process dies with the connection: `kill_on_drop`, and an explicit kill
//! when the server is switched off, removed or reconnected.
//!
//! Desktop only. A phone cannot start a program on itself, and the settings
//! screen says so rather than showing a connection error.

use std::process::Stdio as Piped;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

use super::client::{answer_to, reply_to_server, ConnectorError};

/// The longest line read from the program. A result bigger than this is not
/// one the model would be shown anyway, and an unbounded line is an unbounded
/// allocation chosen by the program.
const MAX_LINE: usize = 4 * 1024 * 1024;

/// How much of one stderr line reaches the log.
const MAX_LOG_LINE: usize = 500;

struct Io {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub struct Stdio {
    /// One exchange at a time. A tool call and its answer are a pair, and
    /// interleaving two would need a reader task and a table of waiters for a
    /// benefit — two calls to one local program at once — nobody has asked for.
    io: tokio::sync::Mutex<Io>,
    child: Mutex<Option<Child>>,
}

impl Stdio {
    /// Start the program. `label` names it in the log.
    pub fn spawn(label: &str, command: &str, args: &[String], env: &[(String, String)]) -> Result<Self, ConnectorError> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Piped::piped())
            .stdout(Piped::piped())
            .stderr(Piped::piped())
            .kill_on_drop(true);
        // No console window flashing up on Windows for a background program.
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);

        let mut child = cmd
            .spawn()
            .map_err(|e| ConnectorError::Unreachable(format!("could not start `{command}`: {e}")))?;

        let stdin = child.stdin.take().ok_or_else(|| ConnectorError::Unreachable("no stdin".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| ConnectorError::Unreachable("no stdout".into()))?;
        if let Some(stderr) = child.stderr.take() {
            let label = label.to_string();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line: String = line.chars().take(MAX_LOG_LINE).collect();
                    log::info!("[connector {label}] {line}");
                }
            });
        }

        Ok(Self {
            io: tokio::sync::Mutex::new(Io { stdin, stdout: BufReader::new(stdout) }),
            child: Mutex::new(Some(child)),
        })
    }

    /// Write a request and read lines until its answer arrives.
    ///
    /// Lines that are not JSON are passed over: some programs print a banner
    /// to stdout before they start speaking the protocol. A request from the
    /// program is answered (`ping`) or refused (anything else) and reading
    /// carries on.
    pub async fn exchange(&self, id: u64, message: &Value, within: Duration) -> Result<Value, ConnectorError> {
        tokio::time::timeout(within, async {
            let mut io = self.io.lock().await;
            write_line(&mut io.stdin, message).await?;
            loop {
                let Some(line) = read_line(&mut io.stdout).await? else {
                    return Err(ConnectorError::Unreachable("the server's program ended".into()));
                };
                let Ok(value) = serde_json::from_slice::<Value>(&line) else { continue };
                if let Some(answer) = answer_to(id, &value) {
                    return answer;
                }
                if let Some(reply) = reply_to_server(&value) {
                    write_line(&mut io.stdin, &reply).await?;
                }
            }
        })
        .await
        .map_err(|_| ConnectorError::Timeout)?
    }

    pub async fn notify(&self, message: &Value) -> Result<(), ConnectorError> {
        let mut io = self.io.lock().await;
        write_line(&mut io.stdin, message).await
    }

    /// Stop the program.
    pub fn close(&self) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(mut c) = child.take() {
                let _ = c.start_kill();
            }
        }
    }
}

async fn write_line(stdin: &mut ChildStdin, message: &Value) -> Result<(), ConnectorError> {
    // `Value::to_string` never contains a raw newline: one inside a string is
    // written `\n`, which is what makes a line a message.
    let mut line = message.to_string();
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| ConnectorError::Unreachable(format!("could not write to the server's program: {e}")))?;
    stdin
        .flush()
        .await
        .map_err(|e| ConnectorError::Unreachable(format!("could not write to the server's program: {e}")))
}

/// One line, at most `MAX_LINE` bytes. `None` when the program has closed stdout.
async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Option<Vec<u8>>, ConnectorError> {
    let mut line = Vec::new();
    let n = (&mut *reader)
        .take(MAX_LINE as u64)
        .read_until(b'\n', &mut line)
        .await
        .map_err(|e| ConnectorError::Unreachable(format!("could not read from the server's program: {e}")))?;
    if n == 0 {
        return Ok(None);
    }
    if !line.ends_with(b"\n") && n >= MAX_LINE {
        return Err(ConnectorError::Protocol("a message longer than this app reads".into()));
    }
    Ok(Some(line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_line_is_read_whole_and_the_end_is_noticed() {
        let mut input: &[u8] = b"{\"a\":1}\nlast";
        assert_eq!(read_line(&mut input).await.expect("read"), Some(b"{\"a\":1}\n".to_vec()));
        assert_eq!(read_line(&mut input).await.expect("read"), Some(b"last".to_vec()));
        assert_eq!(read_line(&mut input).await.expect("read"), None);
    }
}
