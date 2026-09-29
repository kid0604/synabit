//! The real `synabit-safe` binary against a stand-in for the app's socket:
//! what it sends, that the command it runs gets the value, that its exit code
//! is the command's, and that a refusal runs nothing.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;

/// Answer one request with `reply`, handing back what was asked.
fn one_request(sock: PathBuf, reply: &'static str) -> std::thread::JoinHandle<serde_json::Value> {
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        (&stream).write_all(reply.as_bytes()).unwrap();
        (&stream).write_all(b"\n").unwrap();
        serde_json::from_str(&line).unwrap()
    })
}

fn sock(tag: &str) -> PathBuf {
    // macOS keeps socket paths under 104 bytes; a temp dir can be long.
    PathBuf::from("/tmp").join(format!("synabit-safe-test-{tag}-{}.sock", std::process::id()))
}

#[test]
fn the_command_runs_with_the_secret_and_keeps_its_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let path = sock("ok");
    let server = one_request(path.clone(), r#"{"ok":true,"env":{"GH_TOKEN":"cli-canary"}}"#);

    let run = Command::new(env!("CARGO_BIN_EXE_synabit-safe"))
        .args(["run", "--", "sh", "-c"])
        .arg(format!("printf %s \"$GH_TOKEN\" > '{}'; exit 7", out.display()))
        .env("SYNABIT_SAFE_SOCK", &path)
        .env("GH_TOKEN", "safe:github-token")
        .current_dir(dir.path())
        .output()
        .unwrap();

    let asked = server.join().unwrap();
    assert_eq!(asked["secrets"]["GH_TOKEN"], "github-token");
    assert_eq!(asked["command"][0], "sh");
    assert_eq!(asked["cwd"].as_str().map(|c| std::fs::canonicalize(c).unwrap()), Some(std::fs::canonicalize(dir.path()).unwrap()));
    assert_eq!(run.status.code(), Some(7), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "cli-canary");
    let printed = [run.stdout, run.stderr].concat();
    assert!(!String::from_utf8_lossy(&printed).contains("cli-canary"), "the value was printed");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_refusal_runs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out");
    let path = sock("no");
    let server = one_request(path.clone(), r#"{"ok":false,"error":"not allowed in Synabit"}"#);

    let run = Command::new(env!("CARGO_BIN_EXE_synabit-safe"))
        .args(["run", "--env", "T=safe:x", "--", "touch"])
        .arg(&out)
        .env("SYNABIT_SAFE_SOCK", &path)
        .output()
        .unwrap();

    server.join().unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("not allowed"));
    assert!(!out.exists(), "the command ran anyway");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn no_app_listening_is_said_plainly() {
    let run = Command::new(env!("CARGO_BIN_EXE_synabit-safe"))
        .args(["run", "--env", "T=safe:x", "--", "true"])
        .env("SYNABIT_SAFE_SOCK", sock("none"))
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("could not reach Synabit"));
}
