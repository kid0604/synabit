//! Carrying out a declared operation over SSH: a fixed command on a machine
//! the person owns — a Raspberry Pi on the desk, a NAS, a server.
//!
//! The executor for `transport: ssh`. `docs/syn-connectors-2026-10-08.md`
//! §3.1 drew it; this is it. What it may do is fixed here, not in the
//! declaration:
//!
//! - **The command is the declaration's.** Each operation names its program
//!   and arguments as words; a `{{param}}` fills one word and nothing else.
//!   Every word is quoted for a POSIX shell (`quote`), so a value is that
//!   text and never a `;`, a `$(…)` or a second command. And it is a POSIX
//!   shell that reads it, whatever the account logs in with: the command
//!   line is handed to `/bin/sh -s` on its input (`SHELL`), never to the
//!   login shell. fish reads `\'` inside single quotes as an escape, cmd.exe
//!   reads single quotes as nothing; given the line, either would let a
//!   value out of its quotes. Given `/bin/sh -s`, every shell runs the same
//!   program, and one without it fails rather than guesses. A
//!   filled word that would start with `-` is refused: an option is the
//!   declaration's to write, not the model's. The program is never filled,
//!   and a shell or an interpreter cannot be handed a filled word
//!   (`command_problems`) — `sh -c {{anything}}` is a way to run anything,
//!   which is what a declaration exists to rule out.
//! - **The machine is the one the person met.** There is no authority for an
//!   SSH host key, so the first connection is the meeting: the card shows the
//!   key's fingerprint and the person says it is their machine. From then on
//!   a different key is refused — whoever answers at that address now, it is
//!   not what was approved. The pin is kept with this device's approval, as a
//!   certificate pinned for `http` is.
//! - **A public host stays public**, as for `http`: a name approved as public
//!   that resolves to this machine or the local network is refused.
//! - **The login goes to that machine only**, after its key checked out, and
//!   never into the command or what the run records. Password, or a private
//!   key pasted on the card (Ed25519 or ECDSA, no passphrase — RSA is left out
//!   of the build).
//! - **Time and size have ceilings.** A connection that does not answer, a
//!   command that does not finish, an answer past a megabyte: each stops and
//!   says so. A command cut off by the clock may still be running there —
//!   nothing here can promise otherwise — and the answer says that too.
//!
//! # What nothing proves
//!
//! That a command only reads. HTTP has `GET`; a shell has nothing like it.
//! So an ssh operation is at most `read_attested`, and that only once the
//! person vouches for it — until then every command is a send, asked about
//! per command (`instance::effect_here`, `declared::capability`).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey};
use russh::{client, ChannelMsg, Disconnect};
use serde_json::Value;

use super::http::{Failed, Fetched};
use super::instance::{Instance, Network};
use super::manifest::{Decoder, Manifest, Operation};

/// The port when the address names none.
pub const PORT: u16 = 22;
/// How long reaching the machine and logging in may take.
const CONNECT_WITHIN: Duration = Duration::from_secs(12);
/// How long one command may run.
const RUN_WITHIN: Duration = Duration::from_secs(120);
/// The most of a command's output that is read; the rest is cut off and said so.
const MAX_OUT: usize = 1024 * 1024;
const MAX_ERR: usize = 64 * 1024;
/// The most words one command may have.
pub const MAX_WORDS: usize = 64;

/// What the login shell is asked to run: a POSIX shell reading the command
/// line from its input. The same three words to bash, zsh, fish, csh or
/// anything else, with nothing in them to quote.
pub const SHELL: &str = "/bin/sh -s";

/// Sent before every command: the directories a login puts on `PATH` and
/// `/bin/sh -s` does not — where `pip --user`, `npm -g` and nvm put
/// programs. Appended, so nothing there stands in for a system program.
/// Without it `openclaw`, installed with npm on the Pi, was `not found`
/// (exit 127) to every command that named it (2026-10-14).
pub const PREAMBLE: &str = r#"for d in "$HOME"/.nvm/versions/node/*/bin "$HOME/.npm-global/bin" "$HOME/.local/bin" "$HOME/bin" /usr/local/sbin /usr/local/bin /usr/sbin /sbin /snap/bin; do [ -d "$d" ] && PATH="$PATH:$d"; done; export PATH"#;

/// The operation every ssh connector has besides those it declares: one
/// command line, written by the model, run as written.
///
/// # Why there is one at all
///
/// A declaration exists so that what runs is what the person read. Over ssh
/// that turned out to be two approvals per command — the connector drafted
/// again and approved again, then the command itself — and the model, faced
/// with that, told the person it could not run `htop` rather than ask
/// (2026-10-14). The person reads the line either way: this one is shown on
/// the card exactly as it will run, asked about every time, never
/// remembered, and a yes to it is a yes to that line alone (`gate`). Only in
/// the app — a send is never offered to a routine or a chat.
///
/// What only looks (`reads_only`) is a read, asked about once like any read
/// of the machine.
pub const RUN_OP: &str = "run";
/// The longest line `run` sends.
const MAX_LINE: usize = 4_000;

/// The line `run` was given, or why it is not one.
pub fn run_line(args: &Value) -> Result<String, String> {
    let line = args.get("command").and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if line.is_empty() {
        return Err("`run` needs `command`, the command line to run".into());
    }
    if line.len() > MAX_LINE {
        return Err(format!("the command line is longer than {MAX_LINE} characters"));
    }
    if line.contains('\0') {
        return Err("the command line holds a NUL".into());
    }
    Ok(line.to_string())
}

/// Programs that only look, and what may follow them: `None` for anything
/// that is not an option to write or follow, `Some` for the only first
/// arguments allowed. Each one, given only plain words, changes nothing on the
/// machine and finishes on its own.
const LOOKS: &[(&str, Option<&[&str]>)] = &[
    ("uptime", None),
    ("df", None),
    ("du", None),
    ("free", None),
    ("uname", None),
    ("whoami", None),
    ("id", None),
    ("ps", None),
    ("pgrep", None),
    ("lsblk", None),
    ("lscpu", None),
    ("lsusb", None),
    ("nproc", None),
    ("sensors", None),
    ("ls", None),
    ("which", None),
    ("pwd", None),
    ("hostname", Some(&["-I", "-i", "-f", "-s"])),
    ("vcgencmd", Some(&["measure_temp", "measure_volts", "measure_clock", "get_throttled", "get_mem", "version"])),
    ("systemctl", Some(&["status", "is-active", "is-enabled", "is-failed", "list-units", "list-timers", "list-unit-files", "show", "--failed"])),
    ("journalctl", None),
    ("top", None),
    ("ip", Some(&["addr", "a", "address", "route", "r", "link", "-br", "-brief", "-4", "-6"])),
];

/// Options after which a looking program does something else, or never ends.
const LOOKS_BUT: &[&str] = &[
    "-f", "--follow", "--rotate", "--vacuum-size", "--vacuum-time", "--vacuum-files", "--flush", "--sync", "--relinquish-var",
    "--setup-keys", "--update-catalog", "-F", "set", "add", "del", "delete", "flush", "change", "replace",
];

/// Whether `line` only looks: one program from `LOOKS`, plain words after it,
/// and nothing a shell would read as more than words. `top` only in batch
/// mode, with a count, or it never finishes.
pub fn reads_only(line: &str) -> bool {
    let plain = |w: &str| !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "_./:=@,+%-".contains(c));
    let words: Vec<&str> = line.split_ascii_whitespace().collect();
    let Some((program, rest)) = words.split_first() else { return false };
    if !words.iter().all(|w| plain(w)) || rest.iter().any(|w| LOOKS_BUT.contains(&w.split('=').next().unwrap_or(w))) {
        return false;
    }
    let Some((_, first)) = LOOKS.iter().find(|(p, _)| p == program) else { return false };
    if let Some(allowed) = first {
        // `hostname NAME` sets the name; bare, it reads it.
        match rest.first() {
            None => {}
            Some(w) if allowed.contains(w) => {}
            Some(_) => return false,
        }
        if *program == "systemctl" && rest.is_empty() {
            return true;
        }
    }
    if *program == "top" {
        let has = |f: &str| rest.iter().any(|w| w.starts_with('-') && !w.starts_with("--") && w.contains(f));
        return has("b") && has("n");
    }
    true
}

/// Programs that run what they are handed as code. A filled word may follow
/// none of them: `bash -c {{x}}`, `python3 -c {{x}}`, `awk {{x}}` each run
/// whatever the model wrote. Written as a declaration, the same work is a
/// command per thing to do, which the person reads before approving.
///
/// Some of them only by way of a shell: `su -c`, `ssh host cmd`, `watch`,
/// `flock -c`, `script -c` hand the rest to one.
const INTERPRETERS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "ksh", "mksh", "fish", "csh", "tcsh", "ash", "busybox", "python", "python2", "python3",
    "perl", "ruby", "node", "nodejs", "deno", "bun", "php", "lua", "tclsh", "awk", "gawk", "mawk", "nawk", "eval",
    "osascript", "pwsh", "powershell", "su", "runuser", "ssh", "watch", "flock", "script",
    // Run what they are handed by way of a script, a query or a command
    // language of their own: `sed`'s `e`, `sqlite3`'s `.shell`, `psql -c`,
    // an editor's `-c`, a container's `exec`.
    "sed", "gsed", "ed", "ex", "vi", "vim", "nvim", "emacs", "nano", "sqlite3", "mysql", "mariadb", "psql", "mongo",
    "mongosh", "redis-cli", "gdb", "lldb", "make", "expect", "rscript", "julia", "systemd-run", "docker", "podman",
    "kubectl", "nerdctl", "lxc", "tmux", "screen", "parallel", "strace", "ltrace", "unshare", "pkexec", "at",
    "batch", "crontab", "busybox", "toybox", "dash", "elvish", "nu", "xonsh",
    // `git -c core.fsmonitor=…`, `git clone ext::…`: a setting or an address
    // that runs a program.
    "git",
];

/// Interpreters that are often named with their version: `python3.11`,
/// `perl5.36`, `ruby3.2`. A name starting with one of these is one.
const INTERPRETER_STEMS: &[&str] = &["python", "perl", "ruby", "php", "node", "lua", "tclsh", "wish", "pwsh", "julia"];

/// Options after which `find` runs the words that follow.
const FIND_RUNS: &[&str] = &["-exec", "-execdir", "-ok", "-okdir"];

/// Programs that run the program named after them: the runner, how many
/// words come before that program that are not options (`timeout 10 …`), and
/// its options that take a value (`sudo -u www …`). A filled word may not be
/// where the program goes. An option not listed is taken to stand alone.
const RUNNERS: &[(&str, usize, &[&str])] = &[
    ("sudo", 0, &["-u", "-g", "-h", "-p", "-C", "-D", "-r", "-t", "-U", "-T", "--user", "--group", "--host", "--prompt", "--chdir"]),
    ("doas", 0, &["-u", "-C"]),
    ("env", 0, &["-u", "-C", "--unset", "--chdir"]),
    ("exec", 0, &["-a"]),
    ("nohup", 0, &[]),
    ("nice", 0, &["-n", "--adjustment"]),
    ("ionice", 0, &["-c", "-n", "-p", "--class", "--classdata"]),
    ("time", 0, &["-f", "-o", "--format", "--output"]),
    ("xargs", 0, &["-I", "-n", "-P", "-L", "-d", "-E", "-s", "-a", "--max-args", "--max-procs", "--delimiter", "--arg-file"]),
    ("stdbuf", 0, &["-i", "-o", "-e"]),
    ("setsid", 0, &[]),
    ("command", 0, &[]),
    ("builtin", 0, &[]),
    ("nsenter", 0, &["-t", "-S", "-G", "--target", "--setuid", "--setgid"]),
    ("timeout", 1, &["-s", "-k", "--signal", "--kill-after"]),
    ("chrt", 1, &[]),
    ("taskset", 1, &[]),
    ("chroot", 1, &["--userspec", "--groups"]),
];

/// Who to log in as, where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub user: String,
    pub host: String,
    pub port: u16,
}

/// `ssh://user@host[:port]`, read: a user, a host, nothing else.
pub fn target(base_url: &str) -> Result<Target, String> {
    let u = url::Url::parse(base_url.trim()).map_err(|e| format!("`{base_url}` is not an address like ssh://pi@raspberrypi.local: {e}"))?;
    if u.scheme() != "ssh" {
        return Err(format!("an ssh connector's address starts with ssh://, like ssh://pi@raspberrypi.local (not {base_url})"));
    }
    if u.password().is_some() {
        return Err("a password in the address is not allowed; it is typed on the card".into());
    }
    let user = urlencoding::decode(u.username()).map(|s| s.into_owned()).unwrap_or_default();
    if user.is_empty() {
        return Err(format!("the address names no user to log in as: ssh://<user>@{}", u.host_str().unwrap_or("host")));
    }
    let host = u.host_str().map(|h| h.trim_matches(|c| c == '[' || c == ']').to_string()).filter(|h| !h.is_empty());
    let Some(host) = host else { return Err("the address has no host".into()) };
    if !(u.path().is_empty() || u.path() == "/") || u.query().is_some() || u.fragment().is_some() {
        return Err("an ssh address is a user and a host — no path, query or fragment".into());
    }
    Ok(Target { user, host, port: u.port().unwrap_or(PORT) })
}

fn program_name(word: &str) -> String {
    word.rsplit('/').next().unwrap_or(word).to_ascii_lowercase()
}

fn is_interpreter(word: &str) -> bool {
    let name = program_name(word);
    INTERPRETERS.contains(&name.as_str()) || INTERPRETER_STEMS.iter().any(|stem| name.starts_with(stem))
}

/// What is wrong with a command as declared.
pub fn command_problems(command: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let Some(program) = command.first() else {
        return vec!["an ssh operation names its command as words: [program, arg, …]".into()];
    };
    if command.len() > MAX_WORDS {
        out.push(format!("at most {MAX_WORDS} words"));
    }
    if program.trim().is_empty() || program.contains("{{") {
        out.push("the program is written in the declaration, never filled from a parameter".into());
    }
    if command.iter().any(|w| w.chars().any(|c| c.is_control() && c != '\t')) {
        out.push("a word cannot hold a control character or a line break".into());
    }
    let filled = |w: &String| w.contains("{{");
    // A value in an option's word — `--to-command={{x}}`, `-I{{x}}` — is a
    // value the option interprets, which may be a program. Options are the
    // declaration's, whole; a value is a word of its own.
    if command.iter().skip(1).any(|w| w.starts_with('-') && filled(w)) {
        out.push("a value cannot be part of an option's word (`--x={{v}}`); write the option, then the value as its own word".into());
    }
    if let Some(i) = command.iter().position(|w| FIND_RUNS.contains(&w.as_str())) {
        if command[i + 1..].iter().any(filled) {
            out.push(format!("`{}` runs the words after it, so no filled word may follow it", command[i]));
        }
    }
    if let Some(i) = command.iter().position(|w| is_interpreter(w)) {
        if command[i + 1..].iter().any(filled) {
            out.push(format!(
                "`{}` runs what it is given as code, so no filled word may follow it; declare a command for each thing to do",
                command[i]
            ));
        }
    }
    for (i, w) in command.iter().enumerate() {
        let Some((_, before, valued)) = RUNNERS.iter().find(|(r, ..)| *r == program_name(w)) else { continue };
        // Walk to the program it runs: past its options, their values, and
        // the words it takes first. A filled word on the way may be it.
        let mut words = command[i + 1..].iter();
        let mut owed = *before;
        let mut options_over = false;
        while let Some(x) = words.next() {
            if filled(x) {
                out.push(format!("the program `{w}` runs is written in the declaration, never filled"));
                break;
            }
            if !options_over && x == "--" {
                options_over = true;
            } else if !options_over && is_assignment(x) {
                // `env LANG=C prog`, `sudo VAR=1 prog`: a setting, not the program.
            } else if !options_over && x.starts_with('-') {
                if valued.contains(&x.as_str()) {
                    let value = words.next();
                    if value.is_some_and(filled) {
                        out.push(format!("the program `{w}` runs is written in the declaration, never filled"));
                        break;
                    }
                }
            } else if owed > 0 {
                owed -= 1;
            } else {
                break;
            }
        }
    }
    out
}

/// `NAME=value`, as `env` and `sudo` take before the program.
fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// One word, quoted for a POSIX shell: inside single quotes nothing is
/// special, and a single quote is closed, escaped and reopened.
pub fn quote(word: &str) -> String {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c)) {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The command line to send: each word filled from `args` and quoted.
///
/// A word that is one placeholder and nothing else is left out when its
/// parameter is not given — an optional argument — and becomes one word per
/// item when it is given a list. A placeholder inside a word is filled in
/// place.
pub fn command_line(op: &Operation, args: &Value) -> Result<String, String> {
    if !command_problems(&op.command).is_empty() {
        return Err(format!("`{}` is not a command this executor runs", op.name));
    }
    let mut words: Vec<String> = Vec::new();
    for (i, word) in op.command.iter().enumerate() {
        let names = super::manifest::placeholders(word);
        if names.is_empty() {
            words.push(word.clone());
            continue;
        }
        let alone = names.len() == 1 && word.trim() == format!("{{{{{}}}}}", names[0]);
        if alone {
            let arg = args.get(&names[0]).filter(|v| !v.is_null());
            let given: Vec<String> = match arg {
                None => Vec::new(),
                Some(Value::Array(items)) => items.iter().map(|x| value_text(x).ok_or_else(|| format!("`{}` is a list of plain values", names[0]))).collect::<Result<_, _>>()?,
                Some(v) => vec![value_text(v).ok_or_else(|| format!("`{}` is a plain value", names[0]))?],
            };
            if given.is_empty() && op.params.get(&names[0]).is_some_and(|p| p.required) {
                return Err(format!("`{}` needs `{}`", op.name, names[0]));
            }
            for g in given {
                words.push(checked(&names[0], g, i)?);
            }
            continue;
        }
        // `--lines={{n}}` with no `n` is left out whole, rather than sent half.
        let mut filled = Some(word.clone());
        for name in &names {
            match args.get(name).filter(|v| !v.is_null()) {
                Some(v) => {
                    let v = value_text(v).ok_or_else(|| format!("`{name}` is a plain value"))?;
                    filled = filled.map(|f| f.replace(&format!("{{{{{name}}}}}"), &v));
                }
                None if op.params.get(name).is_some_and(|p| p.required) => return Err(format!("`{}` needs `{name}`", op.name)),
                None => filled = None,
            }
        }
        let Some(filled) = filled else { continue };
        // An option the declaration wrote stays one; a value cannot make one.
        if word.starts_with('-') {
            words.push(checked("a filled word", filled, 0)?);
        } else {
            words.push(checked(&names[0], filled, i)?);
        }
    }
    Ok(words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" "))
}

/// The line `op` sends with `args`: `run`'s as written, any other's filled
/// from its declaration. A declared operation always has a command
/// (`manifest::problems`), so only `run` reaches the first arm.
pub fn line_of(op: &Operation, args: &Value) -> Result<String, String> {
    if op.command.is_empty() && op.name == RUN_OP {
        run_line(args)
    } else {
        command_line(op, args)
    }
}

/// A filled word, refused when it would be an option or holds what no word may.
fn checked(name: &str, value: String, at: usize) -> Result<String, String> {
    if at > 0 && value.starts_with('-') {
        return Err(format!("`{name}` would start with `-` and be read as an option; options are written in the declaration"));
    }
    if value.contains('\0') {
        return Err(format!("`{name}` holds a NUL"));
    }
    Ok(value)
}

/// How to log in: what the person pasted, read.
pub enum Login {
    Password(String),
    Key(Box<PrivateKey>),
}

/// A pasted key's lines put back: a password field keeps one line, so a key
/// pasted into one arrives with its line breaks turned to spaces or gone.
fn reflowed(pem: &str) -> Option<String> {
    let begin = pem.find("-----BEGIN ")?;
    let head_end = pem[begin + 11..].find("-----")? + begin + 11 + 5;
    let head = &pem[begin..head_end];
    let end = pem.find("-----END ")?;
    let body: String = pem[head_end..end].chars().filter(|c| !c.is_whitespace()).collect();
    let tail_end = pem[end + 9..].find("-----").map(|i| end + 9 + i + 5)?;
    let tail = &pem[end..tail_end];
    let lines: Vec<String> = body.as_bytes().chunks(70).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
    Some(format!("{head}\n{}\n{tail}\n", lines.join("\n")))
}

/// A password, or a private key when what was pasted is one.
pub fn login(secret: &str) -> Result<Login, String> {
    if !secret.contains("PRIVATE KEY-----") {
        return Ok(Login::Password(secret.to_string()));
    }
    let pem = reflowed(secret).ok_or("the private key is cut short; paste all of it, BEGIN and END lines included")?;
    match russh::keys::decode_secret_key(&pem, None) {
        Ok(k) => Ok(Login::Key(Box::new(k))),
        Err(russh::keys::Error::KeyIsEncrypted) => {
            Err("this key has a passphrase; remove it with `ssh-keygen -p -f <file>` for a key kept only for this, or log in with the password".into())
        }
        Err(e) => Err(format!("the private key could not be read ({e}); Ed25519 and ECDSA keys are supported, RSA is not")),
    }
}

/// A host key as `ssh-keygen -l` prints it: `SHA256:…`.
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Accepts the server's key only when it is the pinned one, and remembers
/// what it was shown either way.
struct Pinned {
    pin: Option<String>,
    shown: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Pinned {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        let fp = fingerprint(key);
        let ok = self.pin.as_deref() == Some(fp.as_str());
        if let Ok(mut shown) = self.shown.lock() {
            *shown = Some(fp);
        }
        Ok(ok)
    }
}

/// Why connecting failed, and the host key it was shown when that is why.
pub struct NotIn {
    pub failed: Failed,
    pub host_key: Option<String>,
}

fn not_in(said: String, code: &'static str) -> NotIn {
    NotIn { failed: Failed { said, answered: false, code: Some(code) }, host_key: None }
}

/// The addresses to reach, judged as `http` judges a host: a public one must
/// not turn out to be this machine or the local network.
async fn addresses(t: &Target, network: Network) -> Result<Vec<std::net::SocketAddr>, NotIn> {
    let found: Vec<std::net::SocketAddr> = tokio::time::timeout(CONNECT_WITHIN, tokio::net::lookup_host((t.host.as_str(), t.port)))
        .await
        .map_err(|_| not_in(format!("{} could not be looked up in time", t.host), "unreachable"))?
        .map_err(|e| not_in(format!("{} could not be found: {e}", t.host), "unreachable"))?
        .collect();
    let kept: Vec<_> = match network {
        Network::Internal => found,
        Network::Public => found.into_iter().filter(|a| !crate::feed_engine::fetcher::is_private_ip(a.ip())).collect(),
    };
    if kept.is_empty() {
        return Err(not_in(
            format!("{} is approved as a public host, and it resolves to this machine or a private network. Approve it as a local network if that is what it is.", t.host),
            "unreachable",
        ));
    }
    Ok(kept)
}

/// Reach the machine, check its key against the pin, log in.
async fn session(m: &Manifest, inst: &Instance, secret: Option<&str>) -> Result<(client::Handle<Pinned>, Target), NotIn> {
    let t = target(&m.base_url).map_err(|said| not_in(said, "server_error"))?;
    let secret = secret.filter(|s| !s.is_empty()).ok_or_else(|| not_in(format!("{} needs its password or key", m.name), "credential_rejected"))?;
    let login = login(secret).map_err(|said| not_in(said, "credential_rejected"))?;
    let addrs = addresses(&t, inst.network).await?;

    let stream = tokio::time::timeout(CONNECT_WITHIN, tokio::net::TcpStream::connect(&addrs[..]))
        .await
        .map_err(|_| not_in(format!("{}:{} did not answer in time", t.host, t.port), "unreachable"))?
        .map_err(|e| not_in(format!("{}:{} could not be reached: {e}", t.host, t.port), "unreachable"))?;

    let shown = Arc::new(Mutex::new(None));
    let handler = Pinned { pin: inst.pinned_cert.clone(), shown: shown.clone() };
    let config = Arc::new(client::Config { inactivity_timeout: Some(RUN_WITHIN + CONNECT_WITHIN), ..Default::default() });
    let connected = tokio::time::timeout(CONNECT_WITHIN, client::connect_stream(config, stream, handler)).await;
    let shown = shown.lock().ok().and_then(|s| s.clone());
    let mut handle = match connected {
        Ok(Ok(h)) => h,
        Err(_) => return Err(not_in(format!("{} did not finish the SSH handshake in time", t.host), "unreachable")),
        Ok(Err(e)) => {
            // Shown a key that is not the pin: the meeting, or a stranger.
            if let Some(fp) = shown.filter(|fp| inst.pinned_cert.as_deref() != Some(fp.as_str())) {
                let (code, said) = match &inst.pinned_cert {
                    None => ("host_key_unknown", format!("{} has not been met yet. Its key is {fp}; the person confirms it is their machine on the card.", t.host)),
                    Some(pin) => (
                        "host_key_changed",
                        format!("{} answered with the key {fp}, not the {pin} that was approved. It may have been reinstalled — or it is not the same machine.", t.host),
                    ),
                };
                return Err(NotIn { failed: Failed { said, answered: false, code: Some(code) }, host_key: Some(fp) });
            }
            return Err(not_in(format!("{}: {e}", t.host), "unreachable"));
        }
    };

    let authed = tokio::time::timeout(CONNECT_WITHIN, async {
        match login {
            Login::Password(p) => handle.authenticate_password(t.user.clone(), p).await,
            // No RSA in the build, so no hash to choose.
            Login::Key(k) => handle.authenticate_publickey(t.user.clone(), PrivateKeyWithHashAlg::new(Arc::new(*k), None)).await,
        }
    })
    .await;
    match authed {
        Ok(Ok(r)) if r.success() => Ok((handle, t)),
        Ok(Ok(_)) => Err(NotIn {
            failed: Failed { said: format!("{} did not accept the login for {}", t.host, t.user), answered: true, code: Some("credential_rejected") },
            host_key: None,
        }),
        Ok(Err(e)) => Err(not_in(format!("{}: logging in failed: {e}", t.host), "unreachable")),
        Err(_) => Err(not_in(format!("{} did not answer the login in time", t.host), "unreachable")),
    }
}

/// Reach and log in, and nothing else — the connect card's try, before any
/// command has been allowed. The host key it was shown comes back when that
/// is what stopped it.
pub async fn check(m: &Manifest, inst: &Instance, secret: Option<&str>) -> Result<(), NotIn> {
    let (handle, _) = session(m, inst, secret).await?;
    let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
    Ok(())
}

/// What a command printed, and how it ended.
#[derive(Debug, Default)]
pub struct Ran {
    pub out: Vec<u8>,
    pub err: Vec<u8>,
    pub exit: Option<u32>,
    /// Past `MAX_OUT`, or past the clock.
    pub cut: Option<&'static str>,
}

async fn exec(handle: &client::Handle<Pinned>, line: &str) -> Result<Ran, String> {
    let mut channel = handle.channel_open_session().await.map_err(|e| format!("no session: {e}"))?;
    channel.exec(true, SHELL).await.map_err(|e| format!("the command was not started: {e}"))?;
    channel.data_bytes(format!("{PREAMBLE}\n{line}\n").into_bytes()).await.map_err(|e| format!("the command was not sent: {e}"))?;
    channel.eof().await.map_err(|e| format!("the command was not sent: {e}"))?;
    let mut ran = Ran::default();
    let reading = async {
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    ran.out.extend_from_slice(&data);
                    if ran.out.len() > MAX_OUT {
                        ran.out.truncate(MAX_OUT);
                        ran.cut = Some("output");
                        break;
                    }
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    if ran.err.len() < MAX_ERR {
                        ran.err.extend_from_slice(&data[..data.len().min(MAX_ERR - ran.err.len())]);
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => ran.exit = Some(exit_status),
                ChannelMsg::Close => break,
                _ => {}
            }
        }
    };
    if tokio::time::timeout(RUN_WITHIN, reading).await.is_err() {
        ran.cut = Some("time");
    }
    let _ = channel.close().await;
    Ok(ran)
}

/// Carry out `op` with `args`: one command, its output made into a table.
pub async fn run(m: &Manifest, inst: &Instance, op: &Operation, args: &Value, secret: Option<&str>) -> Result<Fetched, Failed> {
    let line = line_of(op, args).map_err(Failed::from)?;
    let (handle, t) = session(m, inst, secret).await.map_err(|n| n.failed)?;
    let ran = exec(&handle, &line).await;
    let _ = handle.disconnect(Disconnect::ByApplication, "", "en").await;
    let ran = ran.map_err(|said| Failed { said: format!("{}: {said}", m.name), answered: true, code: Some("server_error") })?;
    let asked = format!("ssh {}@{}: {line}", t.user, t.host);
    read(m, op, ran, asked, secret)
}

/// A command's output as a table, or why it is not one.
pub fn read(m: &Manifest, op: &Operation, ran: Ran, asked: String, secret: Option<&str>) -> Result<Fetched, Failed> {
    let scrub = |b: &[u8]| {
        let text = String::from_utf8_lossy(b).into_owned();
        match secret {
            Some(s) if s.len() >= 4 => text.replace(s, "[hidden]"),
            _ => text,
        }
    };
    let out = scrub(&ran.out);
    let err = scrub(&ran.err);
    if ran.cut == Some("time") {
        return Err(Failed {
            said: format!("{}: `{}` did not finish within {} seconds and was cut off; it may still be running there.", m.name, op.name, RUN_WITHIN.as_secs()),
            answered: true,
            code: Some("server_error"),
        });
    }
    match ran.exit {
        Some(0) => {}
        code => {
            let how = code.map_or("ended without an exit status".to_string(), |c| format!("exited with {c}"));
            let mut said: String = format!("{}: `{}` {how}.", m.name, op.name);
            for (label, text) in [("stderr", &err), ("stdout", &out)] {
                if !text.trim().is_empty() {
                    said.push_str(&format!("\n{label}: {}", text.trim().chars().take(2_000).collect::<String>()));
                }
            }
            return Err(Failed { said, answered: true, code: Some("command_failed") });
        }
    }
    // Text unless the declaration says what the output is; a command declared
    // to print JSON that printed something else is read as the text it is.
    let answer = match op.decoder.unwrap_or(m.decoder) {
        Decoder::Json => serde_json::from_str::<Value>(&out).unwrap_or_else(|_| serde_json::json!({ "text": out })),
        d => super::decode::decode(d, out.as_bytes()).map_err(|said| Failed { said: format!("{}: {said}", m.name), answered: true, code: Some("shape_changed") })?,
    };
    let (columns, rows, mut missing) = super::pick::table(&answer, &op.result);
    if ran.cut == Some("output") {
        missing.push(format!("the output was longer than {} KB and was cut off there", MAX_OUT / 1024));
    }
    Ok(Fetched { columns, rows, missing, asked })
}

#[cfg(test)]
#[path = "ssh_tests.rs"]
pub(crate) mod tests;
