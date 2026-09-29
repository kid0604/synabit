//! `synabit-safe run`: a command, with secrets from the Safe in its
//! environment — section 10.6 of the design.
//!
//! ```text
//! GITHUB_TOKEN=safe:github-token synabit-safe run -- gh repo list
//! synabit-safe run --env AWS_SECRET_ACCESS_KEY=safe:aws.secret -- terraform plan
//! ```
//!
//! Every variable whose value is `safe:<reference>` — given with `--env` or
//! already in the environment — is resolved by the running app, after the user
//! says yes on a card naming the command and each secret. The command then runs
//! with the values in its environment, in place of this process, so its exit
//! code and signals are its own. The values are never printed. Only the
//! variables asked for are set from the reply: whatever answers on the socket
//! cannot slip `PATH` or `LD_PRELOAD` in beside them.
//!
//! This file is shared: the app's library compiles it for the server's types,
//! and the `synabit-safe` binary compiles it on its own (`#[path]`), so it uses
//! nothing but the standard library and serde — the binary stays small and
//! does not carry the app.
//!
//! A reference is an item's Syn name (`github-token`) or its title
//! (`GitHub token`), optionally with `.<field label>` for a field other than
//! the item's first hidden one.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const SOCKET_NAME: &str = "safe-cli.sock";
/// Where the app keeps its data — `tauri.conf.json`'s identifier.
pub const APP_IDENTIFIER: &str = "com.synabit.app";
pub const PREFIX: &str = "safe:";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRequest {
    pub v: u8,
    pub command: Vec<String>,
    pub cwd: String,
    /// Variable name → the reference after `safe:`.
    pub secrets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunReply {
    pub ok: bool,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl Drop for RunReply {
    fn drop(&mut self) {
        // Best effort without the zeroize crate: this file builds on its own.
        for v in self.env.values_mut() {
            // SAFETY: overwriting a String's bytes with zeros keeps it UTF-8.
            unsafe { v.as_bytes_mut().iter_mut().for_each(|b| *b = 0) };
        }
    }
}

/// The socket the app listens on: `$SYNABIT_SAFE_SOCK`, or beside the app's
/// data where Tauri puts it on this platform.
pub fn socket() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SYNABIT_SAFE_SOCK") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let data = if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local").join("share"))
    };
    Some(data.join(APP_IDENTIFIER).join(SOCKET_NAME))
}

/// What the command line asks for: the variables to resolve and the command.
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    pub secrets: BTreeMap<String, String>,
    pub command: Vec<String>,
}

pub const USAGE: &str = "usage: synabit-safe run [--env NAME=safe:<item>]... -- <command> [args...]\n\
  Variables already set to safe:<item> in the environment are resolved too.\n\
  <item> is an item's name for Syn or its title, optionally .<field>.";

/// Read `run [--env NAME=safe:ref]... -- command...`, plus every variable in
/// `environment` whose value is a `safe:` reference.
pub fn parse(args: &[String], environment: impl IntoIterator<Item = (String, String)>) -> Result<Parsed, String> {
    let mut it = args.iter();
    if it.next().map(String::as_str) != Some("run") {
        return Err(USAGE.into());
    }
    let mut secrets: BTreeMap<String, String> = environment
        .into_iter()
        .filter_map(|(k, v)| v.strip_prefix(PREFIX).map(|r| (k, r.to_string())))
        .collect();
    let mut command = Vec::new();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--" => {
                command.extend(it.by_ref().cloned());
                break;
            }
            "--env" | "-e" => {
                let pair = it.next().ok_or("--env needs NAME=safe:<item>")?;
                let (name, value) = pair.split_once('=').ok_or("--env needs NAME=safe:<item>")?;
                let reference = value.strip_prefix(PREFIX).ok_or_else(|| format!("--env {name}: the value must start with {PREFIX}"))?;
                secrets.insert(name.to_string(), reference.to_string());
            }
            other => return Err(format!("unexpected `{other}`\n{USAGE}")),
        }
    }
    if command.is_empty() {
        return Err(USAGE.into());
    }
    if secrets.is_empty() {
        return Err(format!("nothing to resolve: no --env and no variable set to {PREFIX}<item>"));
    }
    Ok(Parsed { secrets, command })
}

/// Ask the app at `socket` to resolve a request. One line of JSON each way.
#[cfg(unix)]
pub fn ask(socket: &std::path::Path, request: &RunRequest) -> std::io::Result<RunReply> {
    use std::io::{BufRead, BufReader, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(socket)?;
    let mut line = serde_json::to_vec(request).map_err(std::io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    let mut reader = BufReader::new(stream);
    let mut answer = String::new();
    reader.read_line(&mut answer)?;
    let reply = serde_json::from_str(&answer).map_err(std::io::Error::other);
    // The line held the values too.
    // SAFETY: zeros keep it UTF-8.
    unsafe { answer.as_bytes_mut().iter_mut().for_each(|b| *b = 0) };
    reply
}

/// The whole of `synabit-safe`: parse, ask, and run. `replace` swaps this
/// process for the command (the binary does); otherwise it is waited for and
/// its exit code returned (the tests do).
#[cfg(unix)]
pub fn run(args: &[String], replace: bool) -> i32 {
    let parsed = match parse(args, std::env::vars()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("synabit-safe: {e}");
            return 2;
        }
    };
    let Some(socket) = socket() else {
        eprintln!("synabit-safe: cannot tell where the app's socket is; set SYNABIT_SAFE_SOCK");
        return 2;
    };
    let request = RunRequest {
        v: 1,
        command: parsed.command.clone(),
        cwd: std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default(),
        secrets: parsed.secrets,
    };
    let reply = match ask(&socket, &request) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "synabit-safe: could not reach Synabit at {} ({e}). Is it running, with the command line turned on in Safe's settings?",
                socket.display()
            );
            return 1;
        }
    };
    if !reply.ok {
        eprintln!("synabit-safe: {}", reply.error.as_deref().unwrap_or("refused"));
        return 1;
    }
    if let Some(missing) = request.secrets.keys().find(|name| !reply.env.contains_key(*name)) {
        eprintln!("synabit-safe: Synabit did not give a value for {missing}; nothing was run");
        return 1;
    }
    let mut cmd = std::process::Command::new(&parsed.command[0]);
    cmd.args(&parsed.command[1..]).envs(reply.env.iter().filter(|(name, _)| request.secrets.contains_key(*name)));
    if replace {
        use std::os::unix::process::CommandExt;
        let e = cmd.exec();
        eprintln!("synabit-safe: could not run `{}`: {e}", parsed.command[0]);
        return 127;
    }
    match cmd.status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("synabit-safe: could not run `{}`: {e}", parsed.command[0]);
            127
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn references_come_from_flags_and_from_the_environment() {
        let env = vec![("GH_TOKEN".to_string(), "safe:github-token".to_string()), ("HOME".into(), "/home/x".into())];
        let p = parse(&args(&["run", "--env", "AWS=safe:aws.secret", "--", "gh", "repo", "list"]), env).unwrap();
        assert_eq!(p.command, args(&["gh", "repo", "list"]));
        assert_eq!(p.secrets.get("GH_TOKEN").map(String::as_str), Some("github-token"));
        assert_eq!(p.secrets.get("AWS").map(String::as_str), Some("aws.secret"));
        assert!(!p.secrets.contains_key("HOME"));
    }

    #[test]
    fn a_command_line_that_asks_for_nothing_or_runs_nothing_is_refused() {
        assert!(parse(&args(&["run", "--", "ls"]), vec![]).is_err(), "nothing to resolve");
        assert!(parse(&args(&["run", "--env", "A=safe:x"]), vec![]).is_err(), "no command");
        assert!(parse(&args(&["run", "--env", "A=plain", "--", "ls"]), vec![]).is_err(), "not a reference");
        assert!(parse(&args(&["get", "x"]), vec![]).is_err());
    }
}
