//! Attacking the room, and refusing to believe a failure that proves nothing.
//!
//! Seatbelt is a private interface and Apple may change it, so the room has to
//! be tested on the machine it runs on. The hard part is not the attacks. The
//! first time this was measured, the profile killed `sandbox-exec` itself, every
//! attack failed, and the room looked perfectly sealed. The next time, `curl`
//! died on a missing config file long before it reached the network.
//!
//! So an attack only counts as stopped when:
//!
//! - the room ran a real command first and printed what it was told to;
//! - a **control** close to the attack, using the same tool on something the
//!   room is allowed, succeeded in the same room.
//!
//! Anything else is reported as broken, not as held. Every step runs inside a
//! room, on a machine somebody is using: no attack is ever run outside one to
//! prove it would work there, because that means opening apps, sending Apple
//! Events and taking a screenshot. An attack that got through does as little as
//! possible, and writes nothing that is not removed afterwards.

use std::io;
use std::net::TcpListener;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use super::cell::{Cell, Limits, Ran};
use super::profile::Reach;
use super::watch;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The attack failed and its control worked.
    Held,
    /// The attack worked.
    Leaked,
    /// The attack failed, but so did its control, so that means nothing.
    NoControl,
    /// Something this test needs is not on this machine.
    Skipped,
}

#[derive(Clone, Debug, Serialize)]
pub struct Line {
    pub id: &'static str,
    pub what: String,
    pub verdict: Verdict,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub os: String,
    /// False unless the room started and nothing leaked or lost its control.
    pub sealed: bool,
    /// Why the room could not even start, when it could not.
    pub broken: Option<String>,
    pub lines: Vec<Line>,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut text = format!("macOS {}: sealed = {}\n", self.os, self.sealed);
        if let Some(broken) = &self.broken {
            text.push_str(&format!("broken: {broken}\n"));
        }
        for line in &self.lines {
            text.push_str(&format!(
                "{:?}\t{}\t{}\t{}\n",
                line.verdict, line.id, line.what, line.detail
            ));
        }
        text
    }
}

/// One attack and the control that makes its failure mean something.
struct Attack {
    id: &'static str,
    what: &'static str,
    attack: String,
    control: String,
    /// Asks the system for something a person would notice if it got through.
    /// Left out when a test runs the attacks in a room that is meant to leak.
    loud: bool,
    /// Skipped when this is not on the machine: `cat` of a missing file fails
    /// for the wrong reason, and no control can tell.
    needs: Option<PathBuf>,
}

const QUICK: Limits = Limits {
    wall: Duration::from_secs(10),
    cpu: 30,
    output: 1 << 16,
};

fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// Tries a lock from perl, which every macOS has: rooms share system files, and
/// a lock on one is a signal the other room can read.
fn lock_script(path: &str) -> String {
    format!(
        r#"perl -e 'use Fcntl ":flock"; open(F, "<", "{path}") or exit 2; flock(F, LOCK_EX|LOCK_NB) or exit 1' "#
    )
}

/// Runs the whole set in rooms under `root`. Cleans up after itself, and only
/// inside `root` and the few named files an attack that got through would leave.
pub async fn run(root: &Path) -> io::Result<Report> {
    run_with(root, |_| {}, false).await
}

async fn run_with(root: &Path, adjust: impl Fn(&mut Cell), quiet_only: bool) -> io::Result<Report> {
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let tools = root.join("tools");
    let bait = root.join("bait");
    std::fs::write(&bait, "outside every room")?;

    let mut a = Cell::make(&root.join("cells/a"), &tools, Reach::Closed)?;
    let mut b = Cell::make(&root.join("cells/b"), &tools, Reach::Closed)?;
    adjust(&mut a);
    adjust(&mut b);

    let os = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Nothing below means anything unless a room can run a command at all.
    let hello = a.run("echo room-is-running", QUICK).await?;
    if !hello.ok() || hello.stdout.trim() != "room-is-running" {
        return Ok(Report {
            os,
            sealed: false,
            broken: Some(format!("exit {:?}: {}", hello.code, hello.stderr.trim())),
            lines: Vec::new(),
        });
    }

    let mut lines = Vec::new();
    lines.extend(run_attacks(&a, &bait, quiet_only).await?);
    if !quiet_only {
        lines.extend(check_network(&a, &root, &tools).await?);
        lines.push(check_signals(&a, &b, &bait).await?);
        lines.push(check_descriptor(&a, &bait).await?);
        lines.push(check_limits(&a).await?);
        lines.push(check_daemon(&a, &b, &bait).await?);
        watch::clear_room(&a, &bait)?;
        watch::clear_room(&b, &bait)?;
    }

    let sealed = lines
        .iter()
        .all(|line| matches!(line.verdict, Verdict::Held | Verdict::Skipped));
    let _ = std::fs::remove_dir_all(root.join("cells"));
    Ok(Report {
        os,
        sealed,
        broken: None,
        lines,
    })
}

fn attacks(cell: &Cell, canary: &Path, home: &Path) -> Vec<Attack> {
    let c = quote(canary);
    let plain = |id, what, attack: String, control: &str| Attack {
        id,
        what,
        attack,
        control: control.to_string(),
        loud: false,
        needs: None,
    };
    let loud = |id, what, attack: String, control: &str| Attack {
        id,
        what,
        attack,
        control: control.to_string(),
        loud: true,
        needs: None,
    };
    let needing = |id, what, attack: String, control: &str, path: PathBuf| Attack {
        id,
        what,
        attack,
        control: control.to_string(),
        loud: true,
        needs: Some(path),
    };
    let stray = home.join(".synabit-room-selftest");
    let agents = home.join("Library/LaunchAgents");
    let brew = cell.profile.homebrew.join("var");
    vec![
        plain("A2", "read a file outside", format!("cat {c}"), "cat own"),
        plain(
            "A2",
            "read outside through a symlink",
            format!("ln -sf {c} sl; cat sl"),
            "ln -sf own ok; cat ok",
        ),
        plain(
            "A2",
            "hard link a file from outside",
            format!("ln {c} hl"),
            "ln own ok2",
        ),
        plain(
            "A26",
            "write a new file in the home folder",
            format!("echo x > {}", quote(&stray)),
            "echo x > mine",
        ),
        needing(
            "A2",
            "list ~/.ssh",
            format!("ls {}", quote(&home.join(".ssh"))),
            "ls .",
            home.join(".ssh"),
        ),
        loud(
            "A2",
            "list the home folder",
            format!("ls {}", quote(home)),
            "ls .",
        ),
        needing(
            "A2",
            "list ~/Desktop",
            format!("ls {}", quote(&home.join("Desktop"))),
            "ls .",
            home.join("Desktop"),
        ),
        needing(
            "A26",
            "list the keychains",
            format!("ls {}", quote(&home.join("Library/Keychains"))),
            "ls .",
            home.join("Library/Keychains"),
        ),
        needing(
            "A26",
            "write a LaunchAgent",
            format!(
                "echo x > {}/com.synabit.room-selftest.plist",
                quote(&agents)
            ),
            "echo x > mine",
            agents,
        ),
        needing(
            "A2",
            "list Homebrew's data",
            format!("ls {}", quote(&brew)),
            "ls .",
            brew,
        ),
        needing(
            "A2",
            "list /usr/local",
            "ls /usr/local".into(),
            "ls .",
            "/usr/local".into(),
        ),
        plain(
            "A2",
            "read /etc/hosts",
            "cat /private/etc/hosts".into(),
            "cat /private/etc/passwd",
        ),
        plain(
            "A30",
            "lock a shared system file",
            lock_script("/usr/lib/dyld"),
            &lock_script("own"),
        ),
        plain(
            "A30",
            "lock /etc/passwd",
            lock_script("/private/etc/passwd"),
            &lock_script("own"),
        ),
        plain(
            "A1",
            "reach the internet by name",
            "curl -s --max-time 4 -o /dev/null https://example.com".into(),
            "curl --version",
        ),
        plain(
            "A1",
            "reach the internet by address",
            "nc -z -G 3 1.1.1.1 443".into(),
            "nc -z -G 1 -h 2>/dev/null; true",
        ),
        loud(
            "A25",
            "start a launchd job",
            "launchctl submit -l com.synabit.room-selftest -- /usr/bin/true".into(),
            "launchctl version",
        ),
        // A setuid program cannot start in a room at all, and `sudo -V` is
        // refused exactly as `sudo` is. The control is a program that can.
        loud("A25", "sudo", "sudo -n true".into(), "/usr/bin/id"),
        loud(
            "A25",
            "send an Apple Event",
            r#"osascript -e 'tell application "Finder" to get name of startup disk'"#.into(),
            "osascript -e 'return 1'",
        ),
        loud(
            "A25",
            "open an app",
            "open -g -a Finder".into(),
            "ls /usr/bin/open",
        ),
        loud(
            "A25",
            "capture the screen",
            "screencapture -x shot.png && test -s shot.png".into(),
            "ls /usr/sbin/screencapture",
        ),
    ]
}

async fn run_attacks(cell: &Cell, bait: &Path, quiet_only: bool) -> io::Result<Vec<Line>> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let mut lines = Vec::new();
    std::fs::write(cell.work().join("own"), "the room's own file")?;
    for attack in attacks(cell, bait, &home) {
        if quiet_only && attack.loud {
            continue;
        }
        if let Some(path) = &attack.needs {
            if !path.exists() {
                lines.push(Line {
                    id: attack.id,
                    what: attack.what.to_string(),
                    verdict: Verdict::Skipped,
                    detail: format!("{} is not on this machine", path.display()),
                });
                continue;
            }
        }
        let control = cell.run(&attack.control, QUICK).await?;
        let tried = cell.run(&attack.attack, QUICK).await?;
        lines.push(judge(attack.id, attack.what, &control, &tried));
    }
    let _ = std::fs::remove_file(home.join(".synabit-room-selftest"));
    let _ = std::fs::remove_file(home.join("Library/LaunchAgents/com.synabit.room-selftest.plist"));
    let _ = std::process::Command::new("/bin/launchctl")
        .args(["remove", "com.synabit.room-selftest"])
        .stderr(std::process::Stdio::null())
        .status();
    Ok(lines)
}

fn judge(id: &'static str, what: &str, control: &Ran, tried: &Ran) -> Line {
    let (verdict, detail) = if tried.ok() {
        (Verdict::Leaked, "it worked".to_string())
    } else if !control.ok() {
        (
            Verdict::NoControl,
            format!(
                "control exit {:?}: {}",
                control.code,
                first_line(&control.stderr)
            ),
        )
    } else {
        (
            Verdict::Held,
            format!("exit {:?}: {}", tried.code, first_line(&tried.stderr)),
        )
    };
    Line {
        id,
        what: what.to_string(),
        verdict,
        detail,
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(120)
        .collect()
}

/// The same `nc` is refused by the closed room and let through by a room with a
/// proxy, so the failure cannot be `nc`, the port, or the machine.
async fn check_network(closed: &Cell, root: &Path, tools: &Path) -> io::Result<Vec<Line>> {
    let allowed = TcpListener::bind("127.0.0.1:0")?;
    let other = TcpListener::bind("127.0.0.1:0")?;
    let allowed_port = allowed.local_addr()?.port();
    let other_port = other.local_addr()?.port();
    let net = Cell::make(&root.join("cells/net"), tools, Reach::Proxy(allowed_port))?;
    // Not under `root`: a socket's path has to fit in 104 bytes.
    let socket_path = PathBuf::from(format!(
        "/private/tmp/synabit-room-{}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&socket_path);
    let socket = UnixListener::bind(&socket_path)?;
    socket.set_nonblocking(true)?;

    let nc = |port: u16| format!("nc -z -G 2 127.0.0.1 {port}");
    let control = net.run(&nc(allowed_port), QUICK).await?;
    let mut lines = vec![
        judge("A24", "a local port, from a closed room", &control, &closed.run(&nc(allowed_port), QUICK).await?),
        judge("A24", "another local port, from a room with a proxy", &control, &net.run(&nc(other_port), QUICK).await?),
        judge(
            "A24",
            "a Unix socket outside the room",
            &control,
            &closed.run(&format!("nc -U {} </dev/null", quote(&socket_path)), QUICK).await?,
        ),
        judge(
            "A24",
            "listen on a port",
            &closed.run("perl -e 'use Socket; socket(S, PF_INET, SOCK_STREAM, 0) and exit 0; exit 3'", QUICK).await?,
            &closed
                .run("perl -e 'use Socket; socket(S, PF_INET, SOCK_STREAM, 0) or exit 2; bind(S, sockaddr_in(0, INADDR_LOOPBACK)) or exit 1'", QUICK)
                .await?,
        ),
    ];
    // `nc` exits 0 on some versions whatever happened. The listener cannot lie.
    if socket.accept().is_ok() {
        lines[2].verdict = Verdict::Leaked;
        lines[2].detail = "the socket was connected to".to_string();
    }
    let _ = std::fs::remove_file(&socket_path);
    Ok(lines)
}

/// A room cannot signal a process in another room, or the app.
async fn check_signals(a: &Cell, b: &Cell, bait: &Path) -> io::Result<Line> {
    let holder = {
        let b = b.clone();
        tokio::spawn(async move { b.run("sleep 8 & echo $! > pid; wait", QUICK).await })
    };
    let pid_file = b.work().join("pid");
    let mut other = None;
    for _ in 0..100 {
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
        {
            other = Some(pid);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let Some(other) = other else {
        return Ok(Line {
            id: "A27",
            what: "signal another room".to_string(),
            verdict: Verdict::NoControl,
            detail: "the other room never started".to_string(),
        });
    };
    let control = a.run("sleep 3 & kill -0 $!", QUICK).await?;
    let script = format!("kill -0 {other} || kill -0 {}", std::process::id());
    let tried = a.run(&script, QUICK).await?;
    watch::clear_room(b, bait)?;
    let _ = holder.await;
    Ok(judge(
        "A27",
        "signal another room, or the app",
        &control,
        &tried,
    ))
}

/// A descriptor the app left open, which the kernel would not check again.
async fn check_descriptor(cell: &Cell, bait: &Path) -> io::Result<Line> {
    use std::os::fd::AsRawFd;
    let file = std::fs::File::open(bait)?;
    // SAFETY: `file` is open until the end of this function.
    unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, 0) };
    let control = cell.run("cat own", QUICK).await?;
    let tried = cell
        .run(&format!("cat <&{}", file.as_raw_fd()), QUICK)
        .await?;
    Ok(judge(
        "A37",
        "read a descriptor the app left open",
        &control,
        &tried,
    ))
}

/// The limits are checked by reading them back. Running a real fork bomb would
/// reach the user-wide limit and stop other apps from starting anything.
async fn check_limits(cell: &Cell) -> io::Result<Line> {
    let ran = cell.run("ulimit -u; ulimit -t", QUICK).await?;
    let values: Vec<u64> = ran
        .stdout
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    let running = watch::process_count()? as u64;
    let held = values.len() == 2
        && values[0] > running
        && values[0] < running + 4096
        && values[1] == QUICK.cpu;
    Ok(Line {
        id: "A28",
        what: "process and CPU limits are in force".to_string(),
        verdict: if held { Verdict::Held } else { Verdict::Leaked },
        detail: format!("{} processes running, limits {values:?}", running),
    })
}

/// A daemon that has left everything behind is still found, and still found
/// after deleting its room's folder. Another room's scan does not claim it.
async fn check_daemon(a: &Cell, b: &Cell, bait: &Path) -> io::Result<Line> {
    let what = "find a daemon that left the group and deleted its folder".to_string();
    let line = |verdict, detail: String| Line {
        id: "A32",
        what: what.clone(),
        verdict,
        detail,
    };
    let started = a
        .run(
            r#"perl -e 'use POSIX; fork && exit; setsid(); fork && exit; chdir "/"; POSIX::close($_) for 0..64; sleep 15'"#,
            QUICK,
        )
        .await?;
    if !started.ok() {
        return Ok(line(
            Verdict::NoControl,
            format!("exit {:?}: {}", started.code, first_line(&started.stderr)),
        ));
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    let here = watch::room_processes(a, bait)?;
    let elsewhere = watch::room_processes(b, bait)?;
    let _ = std::fs::remove_dir_all(a.room());
    let after = watch::room_processes(a, bait)?;
    let cleared = watch::clear_room(a, bait)?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let left = watch::room_processes(a, bait)?;

    let detail = format!(
        "found {}, other room {}, after deleting the folder {}, cleared {cleared}, left {}",
        here.len(),
        elsewhere.len(),
        after.len(),
        left.len()
    );
    let found = here.len() == 1 && elsewhere.is_empty() && after == here && left.is_empty();
    Ok(line(
        if found {
            Verdict::Held
        } else {
            Verdict::NoControl
        },
        detail,
    ))
}

/// For a bundled build made with the `machine-selftest` feature: runs in place
/// of the app, touching none of its data, because what has to be measured is
/// this binary, started by LaunchServices from inside its signed bundle.
#[cfg(feature = "machine-selftest")]
pub fn run_as_the_app() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let root = home.join("Library/Application Support/com.synabit.machine-selftest");
    let report = tokio::runtime::Runtime::new()
        .map_err(|e| e.to_string())
        .and_then(|runtime| {
            runtime
                .block_on(run(&root.join("run")))
                .map_err(|e| e.to_string())
        });
    let text = match report {
        Ok(report) => report.summary(),
        Err(error) => format!("sealed = false\nselftest could not run: {error}\n"),
    };
    let _ = std::fs::write(root.join("report.txt"), text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// The real thing, on whatever machine runs the tests. In CI this is what
    /// notices when a new macOS changes Seatbelt.
    #[tokio::test]
    async fn the_room_is_sealed() {
        let dir = dir();
        let report = run(&dir.path().join("Application Support/selftest"))
            .await
            .unwrap();
        assert!(report.sealed, "{}", report.summary());
        assert!(!report.lines.is_empty());
        for id in [
            "A1", "A2", "A24", "A25", "A26", "A27", "A28", "A30", "A32", "A37",
        ] {
            assert!(report.lines.iter().any(|l| l.id == id), "{id} did not run");
        }
    }

    /// The failure this whole module exists for: a profile that kills the room
    /// must read as broken, not as every attack having failed.
    #[tokio::test]
    async fn a_profile_that_does_not_compile_is_broken_not_sealed() {
        let dir = dir();
        let report = run_with(
            dir.path(),
            |cell| cell.profile.rules = Some("(version 1)(deny file-no-such-rule)".into()),
            false,
        )
        .await
        .unwrap();
        assert!(!report.sealed);
        assert!(report.broken.is_some(), "{}", report.summary());
    }

    #[tokio::test]
    async fn a_profile_that_stops_everything_is_broken_not_sealed() {
        let dir = dir();
        let report = run_with(
            dir.path(),
            |cell| cell.profile.rules = Some("(version 1)(deny default)".into()),
            false,
        )
        .await
        .unwrap();
        assert!(!report.sealed);
        assert!(report.broken.is_some(), "{}", report.summary());
    }

    /// And a room that leaks has to be caught leaking. Only the quiet attacks:
    /// the loud ones would really open things on the machine running the test.
    #[tokio::test]
    async fn a_room_with_no_rules_is_caught() {
        let dir = dir();
        let report = run_with(
            dir.path(),
            |cell| cell.profile.rules = Some("(version 1)(allow default)".into()),
            true,
        )
        .await
        .unwrap();
        assert!(!report.sealed, "{}", report.summary());
        let leaked = |what: &str| {
            report
                .lines
                .iter()
                .any(|l| l.what == what && l.verdict == Verdict::Leaked)
        };
        assert!(leaked("read a file outside"), "{}", report.summary());
        assert!(
            leaked("read outside through a symlink"),
            "{}",
            report.summary()
        );
        assert!(
            leaked("write a new file in the home folder"),
            "{}",
            report.summary()
        );
        assert!(leaked("lock a shared system file"), "{}", report.summary());
    }

    /// A control that fails turns a blocked attack into "this proves nothing".
    #[test]
    fn a_failed_attack_with_a_failed_control_is_not_held() {
        let ran = |code| Ran {
            code: Some(code),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        };
        assert_eq!(
            judge("A", "x", &ran(1), &ran(1)).verdict,
            Verdict::NoControl
        );
        assert_eq!(judge("A", "x", &ran(0), &ran(1)).verdict, Verdict::Held);
        assert_eq!(judge("A", "x", &ran(1), &ran(0)).verdict, Verdict::Leaked);
    }
}
