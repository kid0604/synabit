//! The room, as the model reaches it: run a command, read and change its files,
//! bring a vault item in, keep a file it made.
//!
//! # What crosses, and what that does to the run
//!
//! A room has no network, so nothing in it can leave except by the ways named in
//! the design (§4.3): what goes to the model, and a file kept in the vault. What
//! is left is to keep the model honest about whose words it is holding, and that
//! is one fact per room, kept outside it ([`Rooms::mark_outside`]):
//!
//! - **In.** Anything written into a room while the run has read somebody else's
//!   words marks the room: a command, a file, an edit, a vault item. A file taken
//!   from the vault marks it too, since anybody may have written a file.
//! - **Out.** Everything a marked room says taints the run that hears it, today's
//!   and every later one in the conversation. It never goes to
//!   [`crate::syn::taint::Destinations::note_seen_in`]: a script that prints an
//!   address must not make that address one `browse` may open (A10).
//!
//! Vault data comes in through [`crate::syn::tools::execute_tool`] with the model
//! as the caller, so the app lock and a note's stamp apply as they do to any read
//! (A20). A Markdown file goes back out as a note through `create_node`, which
//! refuses Syn's own kinds and stamps what a tainted run writes (A19).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::Manager;

use super::cell::{Cell, Limits};
use super::export::{self, Out, Refused};
use super::inside;
use super::rooms::{Rooms, Unavailable};
use super::sanitize;
use super::watch::{Over, Watched};
use crate::db::Hold;
use crate::syn::taint::{Origin, Taint};
use crate::syn::tools::{execute_tool, ToolContext, ROOM_EDIT, ROOM_ENV, ROOM_KEEP, ROOM_READ, ROOM_RUN, ROOM_TAKE, ROOM_WRITE};

/// Characters of each of stdout and stderr shown, before the window scales it.
const SHOWN: usize = 8_000;
/// Default and longest wait for one command. Longer work is for `machine_start`.
const WAIT: u64 = 120;
const LONGEST_WAIT: u64 = 600;
/// What `machine_read` shows at most, in bytes.
const READ_MOST: u64 = 200_000;
/// The most the model may write in one call, and what `machine_edit` will edit.
const WRITE_MOST: usize = 4 << 20;
/// What may come in from the vault, and go back out to it, in one file.
const TAKE_MOST: u64 = 2 << 30;
const KEEP_MOST: u64 = 1 << 30;
/// Entries of `work` listed after a command.
const LISTED: usize = 40;
/// How often a waiting command looks at the stop button.
const STOP_POLL: Duration = Duration::from_millis(250);

/// One call, with what it needs of the run.
pub struct Call<'a, R: tauri::Runtime> {
    /// With the run's taint as the model: the lock and the stamps apply.
    pub ctx: &'a ToolContext<'a, R>,
    pub taint: &'a Taint,
    /// The conversation, or the run when there is none: one room each.
    pub scope: &'a str,
    /// The person pressed stop.
    pub stop: &'a (dyn Fn() -> bool + Sync),
}

fn error(said: impl Into<String>) -> String {
    json!({ "error": said.into() }).to_string()
}

fn refused(error: &Refused, path: &str) -> String {
    let shown = sanitize::shown(path);
    let said = match error {
        Refused::NotInside => format!("'{shown}' is not a path inside the room. Paths are relative to its folder, like 'out/report.csv'."),
        Refused::Missing => format!("There is no '{shown}' in the room."),
        Refused::Link => format!("'{shown}' is a link, and links are not followed out of the room. Copy the file itself there."),
        Refused::NotAFile => format!("'{shown}' is not a regular file."),
        Refused::HardLinked => format!("'{shown}' has a second name in the room; copy it to a new file first."),
        other => format!("'{shown}': {other}"),
    };
    json!({ "error": said }).to_string()
}

/// The room for this conversation, and `cell` beside it when the model asks for
/// a second one. Hashed, so a conversation's id never has to be a folder name.
pub fn name_for(scope: &str, cell: Option<&str>) -> Result<String, String> {
    let hash = hex::encode(Sha256::digest(scope.as_bytes()));
    match cell.map(str::trim).filter(|c| !c.is_empty()) {
        None => Ok(format!("c{}", &hash[..16])),
        Some(c) if c.len() <= 16 && c.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') => {
            Ok(format!("c{}-{c}", &hash[..16]))
        }
        Some(_) => Err("`cell` is a short name of lower-case letters, digits and dashes, like \"try-2\".".into()),
    }
}

fn str_arg<'v>(args: &'v Value, key: &str) -> Option<&'v str> {
    args.get(key).and_then(Value::as_str)
}

/// The room's answer, whole, or cut in the middle with how much was cut said.
/// Bytes that are not text are not shown at all: they are no use to a model and
/// the most room for something to hide in.
fn shown_stream(text: &str, budget: usize) -> Value {
    if text.is_empty() {
        return Value::Null;
    }
    let odd = text.chars().filter(|&c| c == '\u{FFFD}' || c == '\0').count();
    if odd > 0 && odd * 100 >= text.chars().count().max(1) {
        return json!(format!("[{} bytes that are not text, not shown; write them to a file instead]", text.len()));
    }
    let count = text.chars().count();
    if count <= budget {
        return json!(text);
    }
    let head: String = text.chars().take(budget * 3 / 4).collect();
    let tail: String = text.chars().skip(count - budget / 4).collect();
    json!(format!(
        "{head}\n[… {} characters cut; send the output to a file and read it with machine_read …]\n{tail}",
        count - budget
    ))
}

/// Why a command failed, in words the model can act on rather than retry. Read
/// from the messages programs print, so a hint and never a verdict.
fn hints(ran: &super::cell::Ran, over: Option<Over>) -> Vec<Value> {
    let mut found = Vec::new();
    let said = ran.stderr.to_ascii_lowercase();
    let network = [
        "could not resolve host",
        "nodename nor servname",
        "name or service not known",
        "temporary failure in name resolution",
        "network is unreachable",
        "no route to host",
        "getaddrinfo",
        "failed to establish a new connection",
        "connection refused",
    ];
    if network.iter().any(|n| said.contains(n)) {
        found.push(json!({
            "what": "network",
            "try": "Nothing in the room reaches the network, by design. Do not retry or look for a way round it. Get what is needed another way (browse, or ask the person), then bring it in with machine_take.",
        }));
    }
    if said.contains("operation not permitted") || said.contains("permission denied") {
        found.push(json!({
            "what": "outside the room",
            "try": "The room sees only its own folder and the programs on this Mac; everything else is refused. Work on copies inside the room: bring a vault item in with machine_take.",
        }));
    }
    if said.contains("command not found") || (said.contains("no such file or directory") && ran.code == Some(127)) {
        found.push(json!({
            "what": "program",
            "try": "That program is not installed on this Mac, and nothing can be installed from inside the room. Use what is there (sh, python3, perl, sqlite3, and whatever Homebrew has); for a Python package, see machine_env.",
        }));
    }
    // A Python package that is not there is the one thing that can be added:
    // measured 2026-11-03, a model that was not told so wrote a PDF by hand.
    if said.contains("modulenotfounderror") || said.contains("no module named") {
        found.push(json!({
            "what": "package",
            "try": "That Python package is not installed. Ask for it with machine_env (the user approves the list), then run the command again with `env` set to the id it returns.",
        }));
    }
    match over {
        Some(Over::Memory { bytes }) => found.push(json!({
            "what": "memory",
            "try": format!("Stopped at {} MB of memory. Work in smaller pieces.", bytes >> 20),
        })),
        Some(Over::Disk { bytes }) => found.push(json!({
            "what": "disk",
            "try": format!("Stopped at {} MB on disk. Remove what is not needed, or work in smaller pieces.", bytes >> 20),
        })),
        None => {}
    }
    if ran.timed_out {
        found.push(json!({
            "what": "time",
            "try": "It ran out of time and was stopped. Give a longer timeout_s, up to 600, or do less in one command.",
        }));
    }
    found
}

/// Runs one call. The answer is the JSON the model reads.
pub async fn call<R: tauri::Runtime>(call: &Call<'_, R>, tool: &str, args: &Value) -> String {
    let Some(rooms) = call.ctx.app.try_state::<Arc<Rooms>>().map(|r| r.inner().clone()) else {
        return error("The room could not be set up when the app started, so no command can run. Say so; there is no other way to run one.");
    };
    if tool == ROOM_ENV {
        return make_env(&rooms, call, args).await;
    }
    let name = match name_for(call.scope, str_arg(args, "cell")) {
        Ok(name) => name,
        Err(said) => return error(said),
    };
    let cell = match rooms.open(&name).await {
        Ok(cell) => cell,
        Err(Unavailable::NotSealed(why)) => {
            return json!({
                "error": "Commands cannot run on this Mac right now: the room failed its safety check, so it is switched off.",
                "why": why,
                "refused": [{ "what": "room", "try": "Say that plainly. Do not look for another way to run it." }],
            })
            .to_string()
        }
        Err(other) => return error(other.to_string()),
    };
    let room = Room { rooms, cell: &cell, name: &name };
    match tool {
        ROOM_RUN => room.run(call, args).await,
        ROOM_READ => room.read(call, args),
        ROOM_WRITE => room.write(call, args),
        ROOM_EDIT => room.edit(call, args),
        ROOM_TAKE => room.take(call, args),
        ROOM_KEEP => room.keep(call, args).await,
        other => error(format!("{other} is not a room tool")),
    }
}

struct Room<'a> {
    rooms: Arc<Rooms>,
    cell: &'a Cell,
    name: &'a str,
}

impl Room<'_> {
    /// Whatever goes in now, the stranger may have chosen.
    fn going_in(&self, taint: &Taint) -> Result<(), String> {
        if taint.is_set() {
            self.rooms
                .mark_outside(self.name)
                .map_err(|e| format!("The room could not be marked, so nothing was put in it: {e}"))?;
        }
        Ok(())
    }

    /// What the room says is somebody else's, once it holds anything that was.
    fn coming_out(&self, taint: &Taint) {
        if self.rooms.carries_outside(self.name) {
            taint.set();
        }
    }

    async fn run<R: tauri::Runtime>(&self, call: &Call<'_, R>, args: &Value) -> String {
        let Some(command) = str_arg(args, "command").filter(|c| !c.trim().is_empty()) else {
            return error("`command` is the shell command to run.");
        };
        if command.contains('\0') {
            return error("A command cannot hold a NUL character.");
        }
        // An environment first on the PATH. Its id is sixteen hex digits, checked,
        // so it cannot close the quotes it is put in.
        let script = match str_arg(args, "env").map(str::trim).filter(|e| !e.is_empty()) {
            None => command.to_string(),
            Some(id) => match super::env::bin_of(self.rooms.root(), id) {
                Some(bin) => format!("PATH='{}':\"$PATH\"; export PATH PYTHONNOUSERSITE=1 PYTHONDONTWRITEBYTECODE=1\n{command}", bin.display()),
                None => return error(format!("There is no environment '{}'. Make one with machine_env, and use the id it returns.", sanitize::shown(id))),
            },
        };
        let command = script.as_str();
        let wait = args.get("timeout_s").and_then(Value::as_u64).unwrap_or(WAIT).clamp(1, LONGEST_WAIT);
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
        let limits = Limits {
            wall: Duration::from_secs(wait),
            // Per process and counted over every thread, so ffmpeg on every core
            // would spend a single core's allowance in a tenth of the time.
            cpu: wait * cores + 60,
            ..Limits::default()
        };
        if let Err(said) = self.going_in(call.taint) {
            return error(said);
        }

        let running = self.rooms.run(self.cell, command, limits);
        tokio::pin!(running);
        let watched: Watched = loop {
            tokio::select! {
                done = &mut running => match done {
                    Ok(watched) => break watched,
                    Err(e) => return error(format!("The command could not be run: {e}")),
                },
                () = tokio::time::sleep(STOP_POLL) => {
                    if (call.stop)() {
                        // The command's future goes when this returns; what it
                        // started is ended first, from outside, by the kernel's
                        // own list of what is in the room.
                        let (rooms, cell) = (self.rooms.clone(), self.cell.clone());
                        let _ = tokio::task::spawn_blocking(move || rooms.clear(&cell)).await;
                        return json!({ "stopped": true, "note": "The person pressed stop; everything in the room was ended." }).to_string();
                    }
                }
            }
        };

        self.coming_out(call.taint);
        let budget = crate::syn::room::scaled(SHOWN);
        let ran = &watched.ran;
        let mut out = json!({
            "exit": ran.code,
            "stdout": shown_stream(&ran.stdout, budget),
            "stderr": shown_stream(&ran.stderr, budget / 2),
        });
        if let Ok((files, all)) = inside::list(self.cell, LISTED) {
            out["files"] = json!(files);
            if all > files.len() {
                out["files_not_listed"] = json!(all - files.len());
            }
        }
        let hints = hints(ran, watched.over);
        if !hints.is_empty() {
            out["refused"] = json!(hints);
        }
        if ran.code.is_none() && !ran.timed_out && watched.over.is_none() {
            out["note"] = json!("It was ended by a signal.");
        }
        out.to_string()
    }

    fn read<R: tauri::Runtime>(&self, call: &Call<'_, R>, args: &Value) -> String {
        let Some(path) = str_arg(args, "path") else {
            return error("`path` is the file to read, inside the room.");
        };
        let (bytes, more) = match inside::read(self.cell, Path::new(path), READ_MOST) {
            Ok(read) => read,
            Err(e) => return refused(&e, path),
        };
        self.coming_out(call.taint);
        let Ok(text) = String::from_utf8(bytes) else {
            return error(format!("'{}' is not text. Look at it with a command, like `file` or `head -c`.", sanitize::shown(path)));
        };
        let lines: Vec<&str> = text.lines().collect();
        let (from, to) = match args.get("lines").and_then(Value::as_str).map(|l| l.split_once('-')) {
            Some(Some((a, b))) => (a.trim().parse::<usize>().unwrap_or(1).max(1), b.trim().parse::<usize>().unwrap_or(lines.len())),
            _ => (1, lines.len()),
        };
        let picked = lines.iter().skip(from - 1).take(to.saturating_sub(from - 1)).copied().collect::<Vec<_>>().join("\n");
        json!({
            "path": path,
            "text": shown_stream(&picked, crate::syn::room::scaled(SHOWN * 4)),
            "lines": lines.len(),
            "cut": more,
        })
        .to_string()
    }

    fn write<R: tauri::Runtime>(&self, call: &Call<'_, R>, args: &Value) -> String {
        let (Some(path), Some(content)) = (str_arg(args, "path"), str_arg(args, "content")) else {
            return error("`path` inside the room and the file's whole `content`.");
        };
        if content.len() > WRITE_MOST {
            return error("That is too much for one call. Write it in parts with a command, or make it with a script.");
        }
        if let Err(said) = self.going_in(call.taint) {
            return error(said);
        }
        match inside::write(self.cell, Path::new(path), content.as_bytes()) {
            Ok(()) => json!({ "written": path, "bytes": content.len() }).to_string(),
            Err(e) => refused(&e, path),
        }
    }

    fn edit<R: tauri::Runtime>(&self, call: &Call<'_, R>, args: &Value) -> String {
        let (Some(path), Some(old), Some(new)) = (str_arg(args, "path"), str_arg(args, "old"), str_arg(args, "new")) else {
            return error("`path` inside the room, the `old` text exactly as it is, and the `new` text to put there.");
        };
        if old.is_empty() {
            return error("`old` cannot be empty; to write a whole file use machine_write.");
        }
        let (bytes, more) = match inside::read(self.cell, Path::new(path), WRITE_MOST as u64) {
            Ok(read) => read,
            Err(e) => return refused(&e, path),
        };
        let Ok(text) = String::from_utf8(bytes).map_err(|_| ()).and_then(|t| if more { Err(()) } else { Ok(t) }) else {
            return error("That file is not text, or is too large to edit this way. Use a command.");
        };
        match text.matches(old).count() {
            1 => {}
            0 => return error("`old` is not in the file. Read it again with machine_read and copy the text exactly."),
            n => return error(format!("`old` is in the file {n} times. Give more of the text around it, so it is in there once.")),
        }
        if let Err(said) = self.going_in(call.taint) {
            return error(said);
        }
        match inside::write(self.cell, Path::new(path), text.replacen(old, new, 1).as_bytes()) {
            Ok(()) => json!({ "edited": path }).to_string(),
            Err(e) => refused(&e, path),
        }
    }

    /// A vault item, copied in. Read as the model, so whatever the lock hides is
    /// refused here as it would be anywhere, and a stamped note taints the run.
    fn take<R: tauri::Runtime>(&self, call: &Call<'_, R>, args: &Value) -> String {
        let (name, bytes) = match fetch(call, args) {
            Ok(fetched) => fetched,
            Err(said) => return said,
        };
        if let Err(said) = self.going_in(call.taint) {
            return error(said);
        }
        let at = match str_arg(args, "as").map(str::trim).filter(|a| !a.is_empty()) {
            Some(given) => given.to_string(),
            None => name,
        };
        match inside::write(self.cell, Path::new(&at), &bytes) {
            Ok(()) => json!({
                "taken": at,
                "bytes": bytes.len(),
                "_note": "A copy, in the room's folder. Changing it changes nothing in the vault; keep a result with machine_keep.",
            })
            .to_string(),
            Err(e) => refused(&e, &at),
        }
    }

    /// A file the room made, into the vault. The room is stopped first, so the
    /// file read is the file kept.
    async fn keep<R: tauri::Runtime>(&self, call: &Call<'_, R>, args: &Value) -> String {
        let Some(path) = str_arg(args, "path") else {
            return error("`path` is the file in the room to keep.");
        };
        match crate::syn::tools::locked_refusal(call.ctx, ROOM_KEEP, args) {
            Ok(Some(why)) => return crate::syn::locks::refusal_json(&why),
            Err(e) => return error(e.to_string()),
            Ok(None) => {}
        }
        let (rooms, cell, relative) = (self.rooms.clone(), self.cell.clone(), PathBuf::from(path));
        let out = tokio::task::spawn_blocking(move || {
            let frozen = rooms.freeze(&cell).map_err(Refused::Io)?;
            export::read(&frozen, &relative, KEEP_MOST)
        })
        .await;
        let out = match out {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return refused(&e, path),
            Err(e) => return error(format!("It could not be read: {e}")),
        };
        // Before anything is written: a stamp is what a tainted run's notes carry.
        self.coming_out(call.taint);
        keep_in_vault(call, args, out)
    }
}

/// An environment, made by Rust in a room that reaches only the registry. What
/// comes back is the list of what is in it; if it failed, the installer's last
/// lines, which are the registry's words and taint the run.
async fn make_env<R: tauri::Runtime>(rooms: &Arc<Rooms>, call: &Call<'_, R>, args: &Value) -> String {
    let packages = match crate::syn::packages::parse(args) {
        Ok(packages) => packages,
        Err(said) => return error(said),
    };
    let stamp = chrono::Utc::now().timestamp_millis();
    let name = format!("env-{}", &hex::encode(Sha256::digest(format!("{}{stamp}", call.scope)))[..12]);
    match super::env::make(rooms, rooms.root(), &name, &packages).await {
        Ok(made) => json!({
            "env": made.id,
            "python": made.python,
            "packages": made.packages,
            "note": "Use it with machine_run's `env`. Nothing can be added to it from a room; another list is another environment.",
        })
        .to_string(),
        Err(super::env::Failed::Step { step, said }) => {
            call.taint.set();
            json!({ "error": format!("Making the environment failed while {step}."), "said": sanitize::shown(&said) }).to_string()
        }
        Err(other) => error(format!("The environment could not be made: {other}")),
    }
}

/// Syn's own folders and files: what decides what Syn may do (I4).
fn syn_own(relative: &str) -> bool {
    let first = relative.trim_start_matches("./").split('/').next().unwrap_or_default();
    first.starts_with('.')
        || first.eq_ignore_ascii_case(crate::syn::instructions::FILE)
        || [crate::syn::skill::SKILL_FOLDER, crate::syn::connector::manifest::CONNECTOR_FOLDER, "SynTools"]
            .iter()
            .any(|own| first.eq_ignore_ascii_case(own))
}

/// The bytes of what `args` names, and the name they go under in the room.
fn fetch<R: tauri::Runtime>(call: &Call<'_, R>, args: &Value) -> Result<(String, Vec<u8>), String> {
    if let Some(handle) = str_arg(args, "handle").map(str::trim).filter(|h| !h.is_empty()) {
        return fetch_table(call, handle);
    }
    let given = |key| str_arg(args, key).map(str::trim).filter(|n| !n.is_empty());
    let Some(asked) = given("node_id").or_else(|| given("path")) else {
        return Err(error("Give `node_id` for a note or a file in the vault, `path` for a file by where it is, or `handle` for a table a tool returned (r1, r2…)."));
    };
    let asked = asked.trim_start_matches("./");
    if syn_own(asked) {
        return Err(error("Syn's own files do not go into the room."));
    }
    // A path names the node there, when the vault knows it, so the lock asks
    // the same question of it. A note's id is its path already.
    let indexed = crate::path_utils::resolve_safe_path(call.ctx.vault_path, asked).ok().and_then(|abs| {
        let db = call.ctx.db.lock().ok()?;
        db.file_location_at(&abs.to_string_lossy()).ok().flatten().map(|l| l.node_id)
    });
    let node_id = indexed.as_deref().unwrap_or(asked);
    if indexed.is_none() && given("node_id").is_none() {
        if let Some(found) = unindexed_file(call, asked)? {
            return Ok(found);
        }
    }
    // Through the table every model read goes through: the lock refuses here,
    // and the note's stamp lands on the run.
    let read = execute_tool(call.ctx, "get_node", &json!({ "node_id": node_id })).map_err(|e| error(e.to_string()))?;
    let node: Value = serde_json::from_str(&read).map_err(|_| read.clone())?;
    if node.get("error").is_some() || node.get("refused").is_some() || node.get("id").is_none() {
        return Err(read);
    }
    let kind = node.get("type").and_then(Value::as_str).unwrap_or_default();
    let title = node.get("title").and_then(Value::as_str).unwrap_or("item");

    let db = call.ctx.db.hold();
    let located = db.file_locations_for_node(node_id).unwrap_or_default();
    drop(db);
    if let Some(location) = located.first().filter(|_| kind == "file") {
        // A file is whoever made it's words, whatever it is called.
        call.taint.note(Origin::Outside);
        let path = PathBuf::from(&location.abs_path);
        let size = std::fs::metadata(&path).map_err(|e| error(format!("The file could not be read: {e}")))?.len();
        if size > TAKE_MOST {
            return Err(error(format!("That file is {} MB, more than the room takes at once.", size >> 20)));
        }
        let name = path.file_name().and_then(sanitize::file_name).unwrap_or_else(|| "file".into());
        let bytes = std::fs::read(&path).map_err(|e| error(format!("The file could not be read: {e}")))?;
        return Ok((name, bytes));
    }

    // A note: the file as it is, frontmatter and all, not the shortened copy
    // `get_node` shows.
    let abs = crate::path_utils::resolve_safe_path(call.ctx.vault_path, node_id).map_err(|e| error(e.to_string()))?;
    let bytes = std::fs::read(&abs).map_err(|e| error(format!("The note could not be read: {e}")))?;
    let stem = sanitize::file_name(std::ffi::OsStr::new(title)).unwrap_or_else(|| "note".into());
    let name = if Path::new(&stem).extension().is_some() { stem } else { format!("{stem}.md") };
    Ok((name, bytes))
}

/// A file in the vault the index has not reached yet, by its path. Whatever the
/// Files app's lock hides, this is: it is asked as a read of the Files app.
fn unindexed_file<R: tauri::Runtime>(call: &Call<'_, R>, relative: &str) -> Result<Option<(String, Vec<u8>)>, String> {
    let Ok(abs) = crate::path_utils::resolve_safe_path(call.ctx.vault_path, relative) else {
        return Ok(None);
    };
    let is_note = Path::new(relative).extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    if is_note || !abs.is_file() || relative.split('/').any(|part| part.starts_with('.')) {
        return Ok(None);
    }
    match crate::syn::tools::locked_refusal(call.ctx, "read_spreadsheet", &json!({ "path": relative })) {
        Ok(Some(why)) => return Err(crate::syn::locks::refusal_json(&why)),
        Err(e) => return Err(error(e.to_string())),
        Ok(None) => {}
    }
    let size = std::fs::metadata(&abs).map_err(|e| error(format!("The file could not be read: {e}")))?.len();
    if size > TAKE_MOST {
        return Err(error(format!("That file is {} MB, more than the room takes at once.", size >> 20)));
    }
    call.taint.note(Origin::Outside);
    let name = abs.file_name().and_then(sanitize::file_name).unwrap_or_else(|| "file".into());
    let bytes = std::fs::read(&abs).map_err(|e| error(format!("The file could not be read: {e}")))?;
    Ok(Some((name, bytes)))
}

fn csv_cell(value: &Value) -> String {
    let text = match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

/// A table this conversation brought back, as CSV with a header row.
fn fetch_table<R: tauri::Runtime>(call: &Call<'_, R>, handle: &str) -> Result<(String, Vec<u8>), String> {
    let db = call.ctx.db.hold();
    let set = crate::syn::resultset::load(db.conn(), call.scope, handle, chrono::Utc::now().timestamp())
        .map_err(|e| error(e.to_string()))?
        .ok_or_else(|| error(format!("There is no table {handle} in this conversation, or it has expired.")))?;
    drop(db);
    // What the model may not see, the room may not show it.
    if set.exposure != crate::syn::resultset::Exposure::Full {
        return Err(error("That table is shown as totals only, so its rows do not go into the room. Use crunch on it."));
    }
    match set.source.kind.as_str() {
        "connector" => call.taint.note(Origin::Connector(set.source.instance.clone().unwrap_or_default())),
        "vault" | "crunch" => {}
        _ => call.taint.note(Origin::Outside),
    }
    let mut text = set.columns.iter().map(|c| csv_cell(&json!(c.name))).collect::<Vec<_>>().join(",");
    text.push('\n');
    for row in &set.rows {
        text.push_str(&row.iter().map(csv_cell).collect::<Vec<_>>().join(","));
        text.push('\n');
    }
    Ok((format!("{handle}.csv"), text.into_bytes()))
}

/// Where in the vault, and under what name. Only ever a new file.
fn keep_in_vault<R: tauri::Runtime>(call: &Call<'_, R>, args: &Value, out: Out) -> String {
    let given = str_arg(args, "name").map(|n| n.trim().replace('\\', "/")).filter(|n| !n.is_empty());
    let wanted = given.unwrap_or_else(|| out.name.clone());
    let wanted = wanted.trim_start_matches("./").to_string();
    if wanted.starts_with('/') || wanted.split('/').any(|part| part.is_empty() || part.starts_with('.')) || syn_own(&wanted) {
        return error(format!("'{}' is not a place in the vault a file can be kept. Give a name like 'Report.pdf'.", sanitize::shown(&wanted)));
    }
    let (folder, file) = match wanted.rsplit_once('/') {
        Some((folder, file)) => (folder.to_string(), file.to_string()),
        None => ("assets".to_string(), wanted.clone()),
    };
    let Some(file) = sanitize::file_name(std::ffi::OsStr::new(&file)) else {
        return error("That name cannot be a file name.");
    };
    let ext = Path::new(&file).extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();

    match ext.as_str() {
        // A note is made the way every note a model makes is: Syn's own kinds are
        // refused whatever the frontmatter says, and a tainted run's note carries
        // its stamp. The room's frontmatter is the note's text, not its own.
        "md" | "markdown" => {
            let Ok(text) = String::from_utf8(out.bytes) else {
                return error("That file is not text, so it cannot be a note.");
            };
            let title = Path::new(&file).file_stem().and_then(|s| s.to_str()).unwrap_or("Note").to_string();
            match execute_tool(call.ctx, "create_node", &json!({ "node_type": "note", "title": title, "content": text })) {
                Ok(made) => made,
                Err(e) => error(e.to_string()),
            }
        }
        // The vault reads these as records of its own.
        "json" | "canvas" => error(format!(
            "A .{ext} file in the vault is read as one of the app's own records, so it is not kept from the room. Keep it as .txt, or say what it holds."
        )),
        _ => {
            let relative = format!("{folder}/{file}");
            let abs = match crate::path_utils::resolve_safe_path(call.ctx.vault_path, &relative) {
                Ok(abs) => abs,
                Err(e) => return error(e.to_string()),
            };
            let Some(parent) = abs.parent() else { return error("No folder for that name.") };
            if let Err(e) = std::fs::create_dir_all(parent) {
                return error(format!("The folder could not be made: {e}"));
            }
            let named = Out { name: file, ..out };
            match export::save(&named, parent) {
                Ok(saved) => {
                    let file_id = crate::commands::files::index_written_file(call.ctx.db, &saved).ok().flatten();
                    json!({
                        "kept": relative,
                        "file_id": file_id,
                        "bytes": named.bytes.len(),
                        "_note": "A new file in the Files app; nothing was overwritten.",
                    })
                    .to_string()
                }
                Err(Refused::AlreadyThere) => json!({
                    "error": format!("'{relative}' already exists, and this never overwrites a file."),
                    "suggestion": crate::commands::nodes::free_node_path(Path::new(call.ctx.vault_path), &relative),
                })
                .to_string(),
                Err(e) => error(format!("It could not be kept: {e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversation_has_its_room_and_a_room_has_a_folder_name() {
        let a = name_for("conv-1", None).unwrap();
        assert_eq!(a, name_for("conv-1", None).unwrap());
        assert_ne!(a, name_for("conv-2", None).unwrap());
        assert_eq!(name_for("conv-1", Some("try-2")).unwrap(), format!("{a}-try-2"));
        assert_eq!(name_for("conv-1", Some(" ")).unwrap(), a);
        for bad in ["../x", "A", "a b", "x/y", &"x".repeat(17)] {
            assert!(name_for("conv-1", Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn long_output_is_cut_in_the_middle_and_says_so() {
        let text: String = (0..2000).map(|i| format!("{i}\n")).collect();
        let shown = shown_stream(&text, 400).as_str().unwrap().to_string();
        assert!(shown.starts_with("0\n1\n"));
        assert!(shown.ends_with("1999\n"));
        assert!(shown.contains("characters cut"));
        assert!(shown_stream("\0\0\u{FFFD}abc", 100).as_str().unwrap().contains("not text"));
        assert_eq!(shown_stream("", 100), Value::Null);
    }

    #[test]
    fn syn_own_files_are_told_apart() {
        for own in ["SYN.md", "syn.md", "SynSkills/x.md", "SynConnectors/a.md", "SynTools/t/run.py", ".synabit/x", "./.git/config"] {
            assert!(syn_own(own), "{own}");
        }
        for theirs in ["Notes/SYN.md", "assets/a.pdf", "Synthesis.md"] {
            assert!(!syn_own(theirs), "{theirs}");
        }
    }

    #[test]
    fn a_blocked_network_is_named_and_not_retried() {
        let ran = super::super::cell::Ran {
            code: Some(6),
            stdout: String::new(),
            stderr: "curl: (6) Could not resolve host: evil.example".into(),
            timed_out: false,
        };
        let found = hints(&ran, None);
        assert_eq!(found[0]["what"], "network");
        assert!(found[0]["try"].as_str().unwrap().contains("Do not retry"));
    }

    #[test]
    fn a_missing_package_points_at_the_one_way_to_add_it() {
        let ran = super::super::cell::Ran {
            code: Some(1),
            stdout: String::new(),
            stderr: "Traceback (most recent call last):\n  File \"<stdin>\", line 1\nModuleNotFoundError: No module named 'reportlab'".into(),
            timed_out: false,
        };
        let found = hints(&ran, None);
        assert_eq!(found[0]["what"], "package");
        assert!(found[0]["try"].as_str().unwrap().contains("machine_env"));
    }

    #[test]
    fn a_csv_cell_is_quoted_when_it_has_to_be() {
        assert_eq!(csv_cell(&json!("a,b")), "\"a,b\"");
        assert_eq!(csv_cell(&json!("say \"hi\"")), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell(&json!(12.5)), "12.5");
        assert_eq!(csv_cell(&Value::Null), "");
    }
}
