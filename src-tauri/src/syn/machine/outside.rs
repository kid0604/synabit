//! The helper's room: a shell with the Internet, for a model that knows nothing.
//!
//! A sealed room has the person's data and no network. This is the other one
//! (design §7.2): the network, through the proxy, to any public host, and
//! nothing of the person's. What is in it is what the helper was told, its
//! `goal`, and whatever it fetched. So a page that talks the helper round has
//! nothing to take but the goal, which the person saw.
//!
//! Nothing here may say who the person is, either. The room is
//! [`Sight::Bare`](super::profile::Sight::Bare): the system's own volume and its
//! own folder, no Homebrew, no `/etc/passwd`, no directory service, and the
//! computer's name refused (`bare.sb`). Its folder is under the user's
//! temporary folder, whose path has no name in it, and a place whose path
//! would is refused rather than used. Measured 2026-11-05 on macOS 26.5.2:
//! `hostname` and `uname -n` are refused, `id -un` says 501, `scutil` gives
//! the model's default name rather than the one the person chose. `logname`
//! is refused too, found on macOS 15 over SSH on 2026-11-06: the kernel keeps
//! the session's login name, and a test run here had none to give.
//!
//! The system's `curl` does not run here: its LibreSSL reads
//! `/private/etc/ssl/openssl.cnf`, which a sealed room may read too (§5.8). So
//! the room has `uv`, and Python through it, with the certificates copied in.
//!
//! The room lives for one helper's work, inside one window of
//! [`Rooms::with_outside`], so no sealed room runs meanwhile (I8). What leaves it
//! leaves one way: [`hand_up`], a file read by descriptor from the frozen room
//! ([`super::export`]) into the asking run's sealed room, which is then marked
//! as holding somebody else's words. The helper's own answer comes back to the
//! run that asked as findings, and taints it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::cell::{Cell, Limits};
use super::export::{self, Refused};
use super::proxy::Allow;
use super::rooms::{Rooms, Unavailable};
use super::sanitize;
use super::tool::{error, refused, shown_stream, str_arg, LISTED, LONGEST_WAIT, SHOWN, STOP_POLL, WAIT};
use super::watch::Watched;
use crate::syn::tools::{ROOM_EDIT, ROOM_KEEP, ROOM_READ, ROOM_RUN, ROOM_WRITE};

/// The folder files handed up land in, inside the asking run's room.
pub const HANDED_UP: &str = "from-helper";

/// What one file handed up may be.
const HAND_UP_MOST: u64 = 1 << 30;

/// The certificates Python is pointed at, copied into the room.
const CERTIFICATES: &str = "/private/etc/ssl/cert.pem";


/// The user's temporary folder, as the system gives it, and only when its path
/// does not hold the user's name.
fn temporary() -> Option<PathBuf> {
    let mut buffer = vec![0u8; 1024];
    // SAFETY: the buffer is as long as the length given, and confstr writes a
    // NUL-terminated string into it, or returns 0.
    let written = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buffer.as_mut_ptr().cast(), buffer.len()) };
    if written == 0 || written > buffer.len() {
        return None;
    }
    buffer.truncate(written - 1);
    let folder = PathBuf::from(String::from_utf8(buffer).ok()?).canonicalize().ok()?;
    names_nobody(&folder).then_some(folder)
}

/// The person's login name and home folder, from the directory, not from an
/// environment anybody could have set.
fn who() -> (Option<String>, Option<PathBuf>) {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: getpwuid returns a pointer into static storage or null; it is read
    // at once, before anything else could call it on this thread.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() {
            return (None, None);
        }
        let name = (!(*entry).pw_name.is_null()).then(|| CStr::from_ptr((*entry).pw_name).to_string_lossy().to_string());
        let home = (!(*entry).pw_dir.is_null())
            .then(|| PathBuf::from(std::ffi::OsStr::from_bytes(CStr::from_ptr((*entry).pw_dir).to_bytes())));
        (name, home)
    }
}

/// Whether a path says nothing of who the person is: not their login name, not
/// their home folder or its name.
fn names_nobody(path: &Path) -> bool {
    let (login, home) = who();
    let text = path.to_string_lossy().to_lowercase();
    let named = [login, home.as_deref().and_then(Path::file_name).and_then(|n| n.to_str()).map(str::to_string)]
        .into_iter()
        .flatten()
        .map(|n| n.to_lowercase())
        .any(|n| n.len() >= 2 && text.contains(&n));
    !named && !home.is_some_and(|home| path.starts_with(home))
}

/// Where this install's helper rooms go: `<temporary>/synabit-outside/<id>`,
/// the id kept beside the other rooms so a crash's leftovers are found again.
/// The id is random, not derived from anything: a hash of the app's folder
/// would be a hash of the person's name.
pub fn place_for(root: &Path) -> Option<PathBuf> {
    let base = temporary()?;
    let id_file = root.join("outside-id");
    let id = match std::fs::read_to_string(&id_file) {
        Ok(id) if id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit()) => id,
        _ => {
            let id = uuid::Uuid::new_v4().simple().to_string()[..16].to_string();
            std::fs::write(&id_file, &id).ok()?;
            id
        }
    };
    let place = base.join("synabit-outside").join(id);
    std::fs::create_dir_all(&place).ok()?;
    Some(place)
}

#[cfg(test)]
thread_local! {
    static ALLOW: std::cell::RefCell<Option<Allow>> = const { std::cell::RefCell::new(None) };
}

/// Where the helper's room may go: any public host. An eval or a test sets it
/// narrower for this thread.
pub fn allow() -> Allow {
    #[cfg(test)]
    if let Some(allow) = ALLOW.with(|a| a.borrow().clone()) {
        return allow;
    }
    Allow::Public
}

/// For this thread's helper rooms until the guard drops.
#[cfg(test)]
pub fn allow_only(allow: Allow) -> impl Drop {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOW.with(|a| *a.borrow_mut() = None);
        }
    }
    ALLOW.with(|a| *a.borrow_mut() = Some(allow));
    Reset
}

/// One helper's work in progress: its room, the room it reports to, and what
/// it has handed up so far.
pub struct Errand {
    pub cell: Cell,
    /// The sealed room of the run that sent it, where files go.
    pub parent_room: String,
    pub handed: Mutex<Vec<String>>,
}

static ERRANDS: LazyLock<Mutex<HashMap<String, Arc<Errand>>>> = LazyLock::new(Default::default);

/// The errand a run is on, if it is a helper that was sent out.
pub fn errand_of(run_id: &str) -> Option<Arc<Errand>> {
    ERRANDS.lock().ok()?.get(run_id).cloned()
}

/// Until it drops, `run_id`'s tools go to this room.
pub struct OnErrand(String);

impl Drop for OnErrand {
    fn drop(&mut self) {
        if let Ok(mut all) = ERRANDS.lock() {
            all.remove(&self.0);
        }
    }
}

pub fn send(run_id: &str, errand: Errand) -> OnErrand {
    if let Ok(mut all) = ERRANDS.lock() {
        all.insert(run_id.to_string(), Arc::new(errand));
    }
    OnErrand(run_id.to_string())
}

/// The room's name for a helper run: a folder name, and nothing of the run's id
/// a page could ask about.
pub fn name_for(run_id: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("o{}", &hex::encode(Sha256::digest(run_id.as_bytes()))[..16])
}

/// Opens the helper's room for as long as `work` takes: the window for the
/// network, the certificates, `uv`, and where `uv` keeps what it fetches.
pub async fn go<T, F, Fut>(rooms: &Rooms, name: &str, work: F) -> Result<T, Unavailable>
where
    F: FnOnce(Cell) -> Fut,
    Fut: std::future::Future<Output = T>,
{
    // Fetched, and checked against its pinned hash, before the window opens:
    // no sealed room waits on a download.
    let uv = super::env::uv(rooms.root()).await.ok();
    rooms
        .with_outside(name, |cell: Cell| async move {
            let cell = match furnish(cell, uv.as_deref()).await {
                Ok(cell) => cell,
                Err(e) => {
                    log::warn!("[machine] the helper's room could not be furnished: {e}");
                    return Err(e.to_string());
                }
            };
            Ok(work(cell).await)
        })
        .await?
        .map_err(Unavailable::Setup)
}

/// Copies what the room needs into its own folders, as new files: nothing in
/// it is an inode a sealed room has read (§5.8, A36). And tells every command
/// where they are. Then Python, fetched by `uv` from inside the room, as the
/// environments are (`super::env`), so the helper does not spend its rounds on
/// it.
async fn furnish(mut cell: Cell, uv: Option<&Path>) -> io::Result<Cell> {
    use std::os::unix::fs::PermissionsExt;
    let room = cell.room().to_path_buf();
    let ssl = room.join("ssl");
    std::fs::create_dir_all(&ssl)?;
    std::fs::write(ssl.join("cert.pem"), std::fs::read(CERTIFICATES)?)?;
    let bin = cell.profile.tools.join("bin");
    std::fs::create_dir_all(&bin)?;
    if let Some(uv) = uv {
        std::fs::write(bin.join("uv"), std::fs::read(uv)?)?;
        std::fs::set_permissions(bin.join("uv"), std::fs::Permissions::from_mode(0o555))?;
    }
    let port = match cell.profile.reach {
        super::profile::Reach::Proxy(port) => port,
        super::profile::Reach::Closed => return Err(io::Error::other("the helper's room has no network")),
    };
    let proxy = OsString::from(format!("http://127.0.0.1:{port}"));
    let certificates: OsString = ssl.join("cert.pem").into();
    let uv_home = room.join("tmp").join("uv");
    cell.env = vec![
        ("https_proxy".into(), proxy.clone()),
        ("HTTPS_PROXY".into(), proxy),
        ("SSL_CERT_FILE".into(), certificates.clone()),
        ("REQUESTS_CA_BUNDLE".into(), certificates),
        ("UV_NO_CONFIG".into(), "1".into()),
        ("UV_CACHE_DIR".into(), uv_home.join("cache").into()),
        ("UV_PYTHON_INSTALL_DIR".into(), uv_home.join("python").into()),
        ("UV_PYTHON_PREFERENCE".into(), "only-managed".into()),
        ("UV_TOOL_DIR".into(), uv_home.join("tools").into()),
        ("PYTHONDONTWRITEBYTECODE".into(), "1".into()),
    ];
    if uv.is_some() {
        if let Err(e) = python(&mut cell, &uv_home.join("python")).await {
            // The helper still has the web and a shell; it is told Python is
            // missing when it reaches for it.
            log::warn!("[machine] no Python in the helper's room: {e}");
        }
    }
    Ok(cell)
}

/// `uv python install`, with the room's network, and a `sitecustomize` that
/// answers `os.uname()`: the computer's name is refused here (`bare.sb`), and
/// without it `uname` fails whole, which takes `platform` and `uv` itself down
/// with it (measured 2026-11-05). Everything but the name is this machine's.
async fn python(cell: &mut Cell, folder: &Path) -> io::Result<()> {
    let uv = cell.profile.tools.join("bin").join("uv");
    let limits = Limits { wall: Duration::from_secs(180), cpu: 600, ..Limits::default() };
    let ran = cell.exec(&uv, &["python".into(), "install".into(), crate::syn::packages::PYTHON.into()], &[], limits).await?;
    if !ran.ok() {
        return Err(io::Error::other(format!("uv could not fetch Python: {}", ran.stderr.lines().last().unwrap_or_default())));
    }
    let installed = super::env::python_in(folder).ok_or_else(|| io::Error::other("uv left no Python"))?;
    let home = folder.join(installed);
    let version = crate::syn::packages::PYTHON;
    // In the standard library's own folder, which is on every `sys.path`: a
    // venv `uv` makes for a command does not read the base's site-packages,
    // and `uv` asks an interpreter about itself in isolated mode, which skips
    // `PYTHONPATH` (both measured 2026-11-05).
    let library = home.join("lib").join(format!("python{version}"));
    let meta = std::fs::symlink_metadata(&library)?;
    if !meta.is_dir() {
        return Err(io::Error::other("no standard library folder"));
    }
    std::fs::write(library.join("sitecustomize.py"), uname_shim())?;
    let path = format!(
        "{}:{}:/usr/bin:/bin:/usr/sbin:/sbin",
        home.join("bin").display(),
        cell.profile.tools.join("bin").display()
    );
    cell.env.push(("PATH".into(), path.into()));
    cell.env.push(("UV_PYTHON".into(), home.join("bin").join("python3").into()));
    Ok(())
}

/// The Python that stands in for `os.uname()`, with this machine's answers but
/// its name.
fn uname_shim() -> String {
    // SAFETY: `uname` fills the struct it is given, or fails and leaves it zeroed.
    let mut name: libc::utsname = unsafe { std::mem::zeroed() };
    unsafe { libc::uname(&mut name) };
    let field = |chars: &[libc::c_char]| -> String {
        let bytes: Vec<u8> = chars.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
        String::from_utf8_lossy(&bytes).replace(['\\', '"'], "")
    };
    format!(
        "# Written by Synabit. This room may not read the computer's name, and\n\
         # os.uname() fails without it, so it answers with \"localhost\".\n\
         import os\n\
         try:\n    os.uname()\nexcept OSError:\n\
         \x20   _answer = os.uname_result((\"{}\", \"localhost\", \"{}\", \"{}\", \"{}\"))\n\
         \x20   os.uname = lambda: _answer\n",
        field(&name.sysname),
        field(&name.release),
        field(&name.version),
        field(&name.machine),
    )
}

/// One of the helper's tools, in its room. The answer is the JSON its model reads.
pub async fn call(rooms: &Arc<Rooms>, errand: &Errand, tool: &str, args: &Value, stop: &(dyn Fn() -> bool + Sync)) -> String {
    let cell = &errand.cell;
    match tool {
        ROOM_RUN => run(rooms, cell, args, stop).await,
        ROOM_READ => super::tool::read_file(cell, args, || {}),
        ROOM_WRITE => super::tool::write_file(cell, args, || Ok(())),
        ROOM_EDIT => super::tool::edit_file(cell, args, || Ok(())),
        ROOM_KEEP => hand_up(rooms, errand, args).await,
        other => error(format!("`{other}` is not one of the helper's tools.")),
    }
}

async fn run(rooms: &Arc<Rooms>, cell: &Cell, args: &Value, stop: &(dyn Fn() -> bool + Sync)) -> String {
    let Some(command) = str_arg(args, "command").filter(|c| !c.trim().is_empty()) else {
        return error("`command` is the shell command to run.");
    };
    if command.contains('\0') {
        return error("A command cannot hold a NUL character.");
    }
    let wait = args.get("timeout_s").and_then(Value::as_u64).unwrap_or(WAIT).clamp(1, LONGEST_WAIT);
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
    let limits = Limits { wall: Duration::from_secs(wait), cpu: wait * cores + 60, ..Limits::default() };

    let running = rooms.run_outside(cell, command, limits);
    tokio::pin!(running);
    let watched: Watched = loop {
        tokio::select! {
            done = &mut running => match done {
                Ok(watched) => break watched,
                Err(e) => return error(format!("The command could not be run: {e}")),
            },
            () = tokio::time::sleep(STOP_POLL) => {
                if stop() {
                    let (rooms, cell) = (rooms.clone(), cell.clone());
                    let _ = tokio::task::spawn_blocking(move || rooms.clear(&cell)).await;
                    return json!({ "stopped": true, "note": "Stopped; everything in the room was ended." }).to_string();
                }
            }
        }
    };

    let budget = crate::syn::room::scaled(SHOWN);
    let ran = &watched.ran;
    let mut out = json!({
        "exit": ran.code,
        "stdout": shown_stream(&ran.stdout, budget),
        "stderr": shown_stream(&ran.stderr, budget / 2),
    });
    if let Ok((files, all)) = super::inside::list(cell, LISTED) {
        out["files"] = json!(files);
        if all > files.len() {
            out["files_not_listed"] = json!(all - files.len());
        }
    }
    let hints = hints(&ran.stderr, ran.code);
    if !hints.is_empty() {
        out["refused"] = json!(hints);
    }
    if ran.timed_out {
        out["note"] = json!("It ran out of time and was stopped. Give a longer timeout_s, up to 600, or do less in one command.");
    }
    out.to_string()
}

/// What this room cannot do, said so the helper does not try it again.
fn hints(stderr: &str, code: Option<i32>) -> Vec<Value> {
    let said = stderr.to_ascii_lowercase();
    let mut found = Vec::new();
    if said.contains("openssl.cnf") || (said.contains("curl") && said.contains("not permitted")) {
        found.push(json!({
            "what": "curl",
            "try": "curl and wget do not work in this room. Fetch with Python: `uv run --no-project python script.py`, using urllib, which goes through the proxy already set.",
        }));
    }
    if said.contains("403") && said.contains("tunnel") {
        found.push(json!({
            "what": "network",
            "try": "That address was refused: only public hosts over HTTPS are reachable, never this computer or its local network.",
        }));
    }
    if said.contains("command not found") || code == Some(127) {
        found.push(json!({
            "what": "program",
            "try": "Only the system's own programs are here (bash and the usual command-line tools), and `uv`. For Python: `uv run --no-project python -c '…'`, or `uv run --no-project --with pandas python script.py` for a package.",
        }));
    }
    found
}

/// A file from the helper's room into the sealed room of the run that sent it,
/// under `from-helper/`. The helper's room is frozen first, so what is read is
/// what is handed; read by descriptor, a regular file only (§6.2). The sealed
/// room is marked: what it holds now is somebody else's.
async fn hand_up(rooms: &Arc<Rooms>, errand: &Errand, args: &Value) -> String {
    let Some(path) = str_arg(args, "path") else {
        return error("`path` is the file in your room to hand to the run that asked.");
    };
    let (owned, cell, relative) = (rooms.clone(), errand.cell.clone(), PathBuf::from(path));
    let out = tokio::task::spawn_blocking(move || {
        let frozen = owned.freeze(&cell).map_err(Refused::Io)?;
        export::read(&frozen, &relative, HAND_UP_MOST)
    })
    .await;
    let out = match out {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return refused(&e, path),
        Err(e) => return error(format!("It could not be read: {e}")),
    };
    let wanted = str_arg(args, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&out.name);
    let Some(file) = sanitize::file_name(std::ffi::OsStr::new(wanted.rsplit('/').next().unwrap_or(wanted))) else {
        return error("That name cannot be a file name.");
    };
    let parent = match rooms.open(&errand.parent_room).await {
        Ok(parent) => parent,
        Err(e) => return error(format!("The room of the run that asked could not be opened: {e}")),
    };
    if let Err(e) = rooms.mark_outside(&errand.parent_room) {
        return error(format!("The room of the run that asked could not be marked, so nothing was handed up: {e}"));
    }
    let at = free_name(&parent, &file);
    match super::inside::write(&parent, Path::new(&at), &out.bytes) {
        Ok(()) => {
            if let Ok(mut handed) = errand.handed.lock() {
                handed.push(at.clone());
            }
            json!({
                "handed_up": at,
                "bytes": out.bytes.len(),
                "_note": "The run that asked will find it at that path in its own room. Say so in your findings.",
            })
            .to_string()
        }
        Err(e) => refused(&e, &at),
    }
}

/// `from-helper/<file>`, or with a number before the extension if that is taken.
fn free_name(parent: &Cell, file: &str) -> String {
    let taken = |name: &str| super::inside::read(parent, Path::new(name), 0).is_ok();
    let first = format!("{HANDED_UP}/{file}");
    if !taken(&first) {
        return first;
    }
    let (stem, ext) = match file.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (file, String::new()),
    };
    (2..1000)
        .map(|n| format!("{HANDED_UP}/{stem}-{n}{ext}"))
        .find(|name| !taken(name))
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_that_names_the_person_is_not_used() {
        let (login, home) = who();
        let (login, home) = (login.unwrap(), home.unwrap());
        assert!(!names_nobody(&home.join("Library/Caches")));
        assert!(!names_nobody(&PathBuf::from(format!("/private/var/folders/{login}/T"))));
        assert!(names_nobody(Path::new("/private/var/folders/f6/vhrxt7xs1m778pr2mdhp4dnr0000gn/T")));
    }

    #[test]
    fn the_rooms_folder_is_the_temporary_one_and_names_nobody() {
        let dir = tempfile::tempdir().unwrap();
        let place = place_for(dir.path()).expect("a place");
        assert!(names_nobody(&place), "{}", place.display());
        assert!(place.starts_with(temporary().unwrap()));
        // The same install finds the same place again, after a crash too.
        assert_eq!(place_for(dir.path()), Some(place.clone()));
        let _ = std::fs::remove_dir_all(place);
    }

    #[test]
    fn a_helpers_room_has_a_folder_name_that_says_nothing_of_the_run() {
        let name = name_for("run-1234");
        assert_eq!(name.len(), 17);
        assert!(!name.contains("1234"));
        assert_ne!(name, name_for("run-1235"));
    }

    #[test]
    fn curl_is_named_as_not_working_here() {
        let said = hints("Auto configuration failed\n…fopen('/private/etc/ssl/openssl.cnf', 'rb')", Some(1));
        assert_eq!(said[0]["what"], "curl");
        assert_eq!(hints("bash: wget: command not found", Some(127))[0]["what"], "program");
    }

    fn rooms() -> (tempfile::TempDir, Arc<Rooms>) {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::at(&dir.path().join("Application Support/machine")).unwrap();
        (dir, Arc::new(rooms))
    }

    fn hostname() -> String {
        let mut buffer = [0u8; 256];
        // SAFETY: the buffer is as long as said, and gethostname NUL-terminates.
        unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
        String::from_utf8_lossy(&buffer[..end]).to_string()
    }

    /// A33, the room's half: a page that tells the helper to find out whose
    /// computer this is finds nothing to send. Every way measured on 2026-11-05
    /// that a program learns the name, the login or the machine, run for real,
    /// with the answers checked against what this test can see from outside.
    #[tokio::test]
    async fn the_helpers_room_does_not_say_whose_computer_it_is() {
        let (_dir, rooms) = rooms();
        let _only = allow_only(Allow::Hosts(vec![]));
        let (login, home) = who();
        let (login, home) = (login.unwrap(), home.unwrap());
        let host = hostname();
        let short = host.split('.').next().unwrap().to_string();
        assert!(!short.is_empty(), "the control: this side can read it");

        let script = "hostname; uname -a; id; id -un; whoami; logname; echo \"$USER $LOGNAME $HOME\"; pwd; \
                      sysctl kern.hostname; scutil --get ComputerName; scutil --get LocalHostName; \
                      cat /etc/passwd; dscl . -list /Users; ls /Users; ls /opt/homebrew /usr/local/Cellar; \
                      cat ~/.ssh/id_ed25519; env; echo done";
        let told = go(&rooms, "who", |cell: Cell| {
            let rooms = rooms.clone();
            async move { rooms.run_outside(&cell, script, Limits::default()).await.unwrap() }
        })
        .await
        .unwrap();
        let said = format!("{}\n{}", told.ran.stdout, told.ran.stderr);
        assert!(told.ran.stdout.contains("done"), "the control: the script ran to the end: {said}");
        let lower = said.to_lowercase();
        for secret in [login.to_lowercase(), short.to_lowercase(), home.display().to_string().to_lowercase()] {
            assert!(!lower.contains(&secret), "the room said {secret:?}:\n{said}");
        }
        // `logname` by name too: this side may have no login to compare with,
        // and the app, opened from the Dock, always has one.
        for refused in ["/etc/passwd: operation not permitted", "/users: operation not permitted", "logname: operation not permitted"] {
            assert!(lower.contains(refused), "{refused}: {said}");
        }
        // Refused where there is one; a blank Mac has none to refuse.
        let homebrew = if Path::new("/opt/homebrew").exists() { "/opt/homebrew: operation not permitted" } else { "/opt/homebrew: no such file or directory" };
        assert!(lower.contains(homebrew), "{homebrew}: {said}");
    }

    /// What a helper hands up lands in the asking run's sealed room, under its
    /// own folder, and that room is marked: whatever it says now is somebody
    /// else's words. A link the helper made to a file of the person's is not
    /// followed out.
    #[tokio::test]
    async fn a_file_handed_up_lands_in_the_asking_room_and_marks_it() {
        let (_dir, rooms) = rooms();
        let _only = allow_only(Allow::Hosts(vec![]));
        let parent = "cparent";
        assert!(!rooms.carries_outside(parent));
        let handed = go(&rooms, "hand", |cell: Cell| {
            let rooms = rooms.clone();
            async move {
                let made = rooms
                    .run_outside(&cell, "printf 'a,b\\n1,2\\n' > prices.csv; ln -s /etc/hosts link.txt", Limits::default())
                    .await
                    .unwrap();
                assert!(made.ran.ok(), "{:?}", made.ran);
                let errand = Errand { cell, parent_room: parent.into(), handed: Mutex::new(Vec::new()) };
                let first = hand_up(&rooms, &errand, &json!({ "path": "prices.csv" })).await;
                let again = hand_up(&rooms, &errand, &json!({ "path": "prices.csv" })).await;
                let link = hand_up(&rooms, &errand, &json!({ "path": "link.txt" })).await;
                let list = errand.handed.lock().unwrap().clone();
                (first, again, link, list)
            }
        })
        .await
        .unwrap();
        let (first, again, link, list) = handed;
        assert!(first.contains("from-helper/prices.csv"), "{first}");
        assert!(again.contains("from-helper/prices-2.csv"), "a second one does not overwrite: {again}");
        assert!(link.contains("link"), "{link}");
        assert!(!link.contains("handed_up"), "{link}");
        assert_eq!(list, ["from-helper/prices.csv", "from-helper/prices-2.csv"]);

        let room = rooms.open(parent).await.unwrap();
        let (bytes, _) = super::super::inside::read(&room, Path::new("from-helper/prices.csv"), 100).unwrap();
        assert_eq!(bytes, b"a,b\n1,2\n");
        assert!(rooms.carries_outside(parent));
    }

    /// The window closes behind the helper: its room's folder is gone, and with
    /// it everything it fetched.
    #[tokio::test]
    async fn nothing_of_the_helpers_room_is_left_after() {
        let (_dir, rooms) = rooms();
        let _only = allow_only(Allow::Hosts(vec![]));
        let room = go(&rooms, "gone", |cell: Cell| async move { cell.room().to_path_buf() }).await.unwrap();
        assert!(names_nobody(&room), "{}", room.display());
        assert!(!room.exists());
    }

    /// The network, for real: Python fetched by `uv` through the proxy, and a
    /// page read with it. Ignored, as it reaches the Internet; run by hand.
    #[tokio::test]
    #[ignore = "reaches the Internet"]
    async fn the_helper_fetches_a_page_with_python_through_the_proxy() {
        let (_dir, rooms) = rooms();
        let script = "python3 -c \"import urllib.request as u, platform; print(platform.node(), u.urlopen('https://pypi.org/simple/pip/').status)\"; \
                      uv run --no-project --with six python -c \"import six; print(six.__name__)\"";
        let ran = go(&rooms, "net", |cell: Cell| {
            let rooms = rooms.clone();
            async move {
                let limits = Limits { wall: Duration::from_secs(300), ..Limits::default() };
                rooms.run_outside(&cell, script, limits).await.unwrap()
            }
        })
        .await
        .unwrap();
        assert!(ran.ran.stdout.contains("localhost 200"), "{:?}", ran.ran);
        assert!(ran.ran.stdout.trim().ends_with("six"), "a package, through uv: {:?}", ran.ran);
    }
}
