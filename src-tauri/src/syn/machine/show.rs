//! `machine_reveal` and `machine_open`: a file shown in Finder, or opened in the
//! app the Mac opens it with (design §10.4).
//!
//! Neither is as harmless as it sounds. `open` on a `.command` Syn just wrote
//! leaves the room behind entirely, and `open` on an address is a way out with
//! anything in it (A5, A6). So:
//!
//! - Revealing selects the file in a Finder window and opens nothing; the
//!   person decides what to do with it. Any file or folder that is there.
//! - Opening takes a file on the list of documents of §9.1 only — by its
//!   extension *and* its first bytes, as a rehearsal classifies them
//!   (`rehearse::not_a_document`) — never a folder, an app, a script, a symlink
//!   or something with an execute bit. Never from a folder a room can write in,
//!   where the file could be swapped between the check and the open.
//! - Neither takes an address. A web page goes through `browse`'s pane.
//!
//! Both only with the person at this computer, since they put something on its
//! screen. The arguments go to `/usr/bin/open` as one argument, never a shell.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::sanitize;

pub const REVEAL: &str = "machine_reveal";
pub const OPEN: &str = "machine_open";

/// What the tests and the eval have done instead of `open`: a real Finder
/// window, or a real app, is no part of a test.
#[cfg(any(test, feature = "eval"))]
pub static DONE: std::sync::Mutex<Vec<(String, PathBuf)>> = std::sync::Mutex::new(Vec::new());

/// Why a path was not taken, as the model is told it.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    NoPath,
    Address,
    Hidden,
    NotThere,
    NotAFile,
    Symlink,
    NotADocument(&'static str),
    RoomFolder,
}

impl Refused {
    fn said(&self) -> String {
        match self {
            Refused::NoPath => "`path` is a file's path on this Mac, or in the vault.".into(),
            Refused::Address => {
                "That is an address, not a file. Nothing here opens addresses; a page is read with browse.".into()
            }
            Refused::Hidden => "The path has characters in it that do not show. Nothing was done.".into(),
            Refused::NotThere => "Nothing is at that path.".into(),
            Refused::NotAFile => {
                "Only a file is opened, never a folder or an app. machine_reveal shows it in Finder instead.".into()
            }
            Refused::Symlink => {
                "That is a link to somewhere else, and only the file itself is opened. machine_reveal shows it in Finder."
                    .into()
            }
            Refused::NotADocument(why) => format!(
                "Only documents are opened: PDFs, pictures, video, sound, text, Office files. This one {why}. \
                 Use machine_reveal to show it in Finder, and let the person decide."
            ),
            Refused::RoomFolder => {
                "That file is in a room's folder. Keep it in the vault with machine_keep first, then open it from there."
                    .into()
            }
        }
    }
}

/// The path the model named: absolute, under the home folder with `~`, or in the
/// vault. Not an address of any kind.
fn resolve(raw: &str, vault: &str) -> Result<PathBuf, Refused> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(Refused::NoPath);
    }
    if sanitize::has_hidden(raw) {
        return Err(Refused::Hidden);
    }
    // `https:`, `file:`, `mailto:`, `x-apple.systempreferences:` — a scheme is
    // letters before a colon, and a path never starts with one.
    let scheme = raw.split_once(':').is_some_and(|(before, _)| {
        !before.is_empty()
            && before.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && before.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    });
    if scheme || raw.contains("://") {
        return Err(Refused::Address);
    }
    let path = if let Some(rest) = raw.strip_prefix("~/") {
        PathBuf::from(std::env::var_os("HOME").ok_or(Refused::NotThere)?).join(rest)
    } else if raw.starts_with('/') {
        PathBuf::from(raw)
    } else {
        Path::new(vault).join(raw)
    };
    // `..` is resolved by the system, not here; what it resolves to is what is
    // checked and opened.
    if std::fs::symlink_metadata(&path).is_err() {
        return Err(Refused::NotThere);
    }
    Ok(path)
}

fn answer(done: &str, path: &Path) -> Value {
    json!({ done: sanitize::shown(&path.to_string_lossy()) })
}

fn refused(why: Refused) -> Value {
    json!({ "refused": why.said() })
}

/// `/usr/bin/open` with these arguments, and the path as one argument of its own.
fn open_with(flags: &[&str], path: &Path) -> Result<(), String> {
    #[cfg(any(test, feature = "eval"))]
    {
        let verb = if flags.contains(&"-R") { REVEAL } else { OPEN };
        DONE.lock().map_err(|e| e.to_string())?.push((verb.to_string(), path.to_path_buf()));
        Ok(())
    }
    #[cfg(not(any(test, feature = "eval")))]
    {
        let status = std::process::Command::new("/usr/bin/open")
            .args(flags)
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("the Mac could not open it ({status})"))
        }
    }
}

/// `machine_reveal`: the file selected in a Finder window. Opens nothing.
pub fn reveal(args: &Value, vault: &str) -> Value {
    let raw = args.get("path").and_then(Value::as_str).unwrap_or_default();
    let path = match resolve(raw, vault) {
        Ok(path) => path,
        Err(why) => return refused(why),
    };
    match open_with(&["-R"], &path) {
        Ok(()) => answer("revealed", &path),
        Err(e) => json!({ "error": e }),
    }
}

/// `machine_open`: a document, in the app the Mac opens it with. `rooms` are the
/// folders a room can write in.
pub fn open(args: &Value, vault: &str, rooms: &[PathBuf]) -> Value {
    let raw = args.get("path").and_then(Value::as_str).unwrap_or_default();
    let path = match resolve(raw, vault) {
        Ok(path) => path,
        Err(why) => return refused(why),
    };
    match document(&path, rooms) {
        Ok(()) => {}
        Err(why) => return refused(why),
    }
    match open_with(&[], &path) {
        Ok(()) => answer("opened", &path),
        Err(e) => json!({ "error": e }),
    }
}

/// Whether `path` is a document that may be opened, as it is now.
fn document(path: &Path, rooms: &[PathBuf]) -> Result<(), Refused> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| Refused::NotThere)?;
    if meta.file_type().is_symlink() {
        return Err(Refused::Symlink);
    }
    if !meta.is_file() {
        return Err(Refused::NotAFile);
    }
    // Through every link above it too: a room's folder by another name is
    // still a room's folder.
    let real = std::fs::canonicalize(path).map_err(|_| Refused::NotThere)?;
    let in_a_room = rooms
        .iter()
        .filter_map(|room| std::fs::canonicalize(room).ok())
        .any(|room| real.starts_with(room));
    if in_a_room {
        return Err(Refused::RoomFolder);
    }
    let mut head = [0u8; 512];
    let read = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .map_err(|_| Refused::NotThere)?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    match super::rehearse::not_a_document(&name, Some(&head[..read]), Some(meta.permissions().mode())) {
        None => Ok(()),
        Some(why) => Err(Refused::NotADocument(match why {
            super::rehearse::Why::Executable => "can be run as a program",
            super::rehearse::Why::ContentDoesNotMatch => "is not what its name says",
            _ => "is not on the list of documents",
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done() -> Vec<(String, PathBuf)> {
        std::mem::take(&mut *DONE.lock().expect("done"))
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("written");
        path
    }

    // One test, so the record of what was done is this test's alone.
    #[test]
    fn a_document_is_opened_and_nothing_else_is() {
        let dir = tempfile::tempdir().expect("dir");
        let vault = dir.path().join("vault");
        let room = dir.path().join("room");
        std::fs::create_dir_all(vault.join("Files")).expect("vault");
        std::fs::create_dir_all(&room).expect("room");
        let vault_s = vault.to_string_lossy().to_string();
        let pdf = write(&vault.join("Files"), "hop-dong.pdf", b"%PDF-1.7\n...");
        done();

        let opened = open(&json!({ "path": "Files/hop-dong.pdf" }), &vault_s, std::slice::from_ref(&room));
        assert!(opened.get("opened").is_some(), "{opened}");
        let opened = open(&json!({ "path": pdf.to_string_lossy() }), &vault_s, std::slice::from_ref(&room));
        assert!(opened.get("opened").is_some(), "{opened}");
        assert_eq!(done().len(), 2);

        // A5: what a room could make to be run, by whatever name.
        let script = write(&vault, "Start Here.command", b"#!/bin/sh\necho hi\n");
        let fake = write(&vault, "invoice.pdf", b"#!/bin/sh\necho hi\n");
        let marked = write(&vault, "notes.txt", b"plain words\n");
        std::fs::set_permissions(&marked, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let app = vault.join("Tool.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).expect("app");
        let link = vault.join("link.pdf");
        std::os::unix::fs::symlink(&script, &link).expect("symlink");
        let kept_in_room = write(&room, "report.pdf", b"%PDF-1.7\n");
        let through = dir.path().join("through");
        std::os::unix::fs::symlink(&room, &through).expect("symlink");
        for (path, why) in [
            (script.to_string_lossy().to_string(), "list of documents"),
            (fake.to_string_lossy().to_string(), "not what its name says"),
            (marked.to_string_lossy().to_string(), "run as a program"),
            (app.to_string_lossy().to_string(), "never a folder"),
            (link.to_string_lossy().to_string(), "a link"),
            (kept_in_room.to_string_lossy().to_string(), "room's folder"),
            (through.join("report.pdf").to_string_lossy().to_string(), "room's folder"),
        ] {
            let said = open(&json!({ "path": path }), &vault_s, std::slice::from_ref(&room));
            let refused = said.get("refused").and_then(Value::as_str).unwrap_or_default();
            assert!(refused.contains(why), "{path}: {said}");
        }
        // A6: no address, of any scheme.
        for address in [
            "https://evil.example/?d=quan-trong",
            "file:///Users/eval/Start Here.command",
            "mailto:someone@example.com",
            "x-apple.systempreferences:com.apple.preference.security",
            "//evil.example/x",
        ] {
            let said = open(&json!({ "path": address }), &vault_s, std::slice::from_ref(&room));
            assert!(said.get("refused").is_some(), "{address}: {said}");
            assert!(reveal(&json!({ "path": address }), &vault_s).get("refused").is_some(), "{address}");
        }
        let hidden = format!("{}/hop-dong\u{202E}fdp.pdf", vault_s);
        assert!(open(&json!({ "path": hidden }), &vault_s, &[]).get("refused").is_some());
        assert!(open(&json!({ "path": "Files/none.pdf" }), &vault_s, &[]).get("refused").is_some());
        assert!(done().is_empty(), "nothing refused was opened");

        // Revealing opens nothing, so it takes what is there: an app, a script.
        for path in [&app, &script] {
            let said = reveal(&json!({ "path": path.to_string_lossy() }), &vault_s);
            assert!(said.get("revealed").is_some(), "{said}");
        }
        let revealed = done();
        assert_eq!(revealed.len(), 2);
        assert!(revealed.iter().all(|(verb, _)| verb == REVEAL));
    }
}
