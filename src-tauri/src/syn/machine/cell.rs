//! Running a command in a room.
//!
//! The environment is blank apart from what is set here: an API key in the
//! app's environment must not be one `env` away. Output is read by this process
//! through pipes, because `sandbox-exec` cannot write to a file outside the room
//! and because a command must not choose where its output goes.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};

use super::profile::{usable, Profile, Reach};
use super::watch;

/// How far a command may get before it is stopped.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub wall: Duration,
    /// Seconds on the CPU, inherited by everything it starts.
    pub cpu: u64,
    /// Bytes kept of each of stdout and stderr. The rest is read and dropped,
    /// so a command never blocks on a full pipe.
    pub output: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            wall: Duration::from_secs(120),
            cpu: 600,
            output: 1 << 20,
        }
    }
}

/// `RLIMIT_NPROC` counts every process of this user, so it has to sit above how
/// many there already are or nothing could start.
const SPARE_PROCESSES: u64 = 512;

/// How long after a command exits its pipes may stay open. A daemon that kept
/// stdout would otherwise hold the reader forever.
const DRAIN: Duration = Duration::from_millis(200);

#[derive(Debug)]
pub struct Ran {
    /// None when a signal ended it.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl Ran {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// A room: a folder, and the rules that make it the only place there is.
#[derive(Clone, Debug)]
pub struct Cell {
    pub profile: Profile,
}

impl Cell {
    /// Makes the room's folders and resolves its path. The kernel matches
    /// resolved paths, so a room under `/tmp` would otherwise match nothing.
    pub fn make(dir: &Path, tools: &Path, reach: Reach) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        std::fs::create_dir_all(tools)?;
        let room = dir.canonicalize()?;
        let tools = tools.canonicalize()?;
        if !usable(&room) || !usable(&tools) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "unusable path"));
        }
        let cell = Self {
            profile: Profile::new(room, tools, reach),
        };
        cell.restore()?;
        Ok(cell)
    }

    pub fn room(&self) -> &Path {
        &self.profile.room
    }

    pub fn work(&self) -> PathBuf {
        self.room().join("work")
    }

    /// Puts the folders back. A room can delete its own, and a scan needs them
    /// to exist (see [`watch`]), so this runs from outside before each.
    pub fn restore(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.work())?;
        std::fs::create_dir_all(self.room().join("tmp"))
    }

    /// Runs `/bin/sh -c script` in the room's `work` folder.
    pub async fn run(&self, script: &str, limits: Limits) -> io::Result<Ran> {
        self.restore()?;
        let room = self.room();
        let path = format!(
            "{}:/usr/bin:/bin:/usr/sbin:/sbin:{}",
            self.profile.tools.join("bin").display(),
            self.profile.homebrew.join("bin").display(),
        );
        let processes = watch::process_count()? as u64 + SPARE_PROCESSES;
        let cpu = limits.cpu;

        let mut command = tokio::process::Command::new("/usr/sbin/taskpolicy");
        command
            .args(["-c", "utility", "/usr/bin/sandbox-exec"])
            .args(self.profile.args())
            .args(["/bin/sh", "-c", script])
            .current_dir(self.work())
            .env_clear()
            .env("PATH", path)
            .env("HOME", self.work())
            .env("TMPDIR", room.join("tmp"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        // SAFETY: only calls setrlimit, getdtablesize and fcntl, which are
        // async-signal-safe, and allocates nothing between fork and exec.
        unsafe {
            command.pre_exec(move || {
                limit(libc::RLIMIT_NPROC, processes)?;
                limit(libc::RLIMIT_CPU, cpu)?;
                limit(libc::RLIMIT_CORE, 0)?;
                close_inherited();
                Ok(())
            });
        }

        let mut child = command.spawn()?;
        let group = child.id().map(|pid| pid as libc::pid_t);
        let out = keep(child.stdout.take(), limits.output);
        let err = keep(child.stderr.take(), limits.output);

        let waited = tokio::time::timeout(limits.wall, child.wait()).await;
        let (code, timed_out) = match waited {
            Ok(status) => (status?.code(), false),
            Err(_) => (None, true),
        };
        // What is left of the group goes too. What left the group is for
        // `watch` to find; holding the kernel's rules does not depend on it.
        if let Some(group) = group {
            // SAFETY: a plain signal to a group this function started.
            unsafe { libc::killpg(group, libc::SIGKILL) };
        }
        if timed_out {
            let _ = child.wait().await;
        }
        tokio::time::sleep(DRAIN).await;
        Ok(Ran {
            code,
            stdout: out.take(),
            stderr: err.take(),
            timed_out,
        })
    }
}

/// A descriptor is a door the profile never sees: Seatbelt checks a file when it
/// is opened, not when an inherited descriptor is read. So a database the app
/// has open, opened by a library that did not ask for close-on-exec, would be
/// readable from the room. Nothing but stdin, stdout and stderr crosses exec.
fn close_inherited() {
    // SAFETY: fcntl on a descriptor that is not open only returns EBADF.
    unsafe {
        for fd in 3..libc::getdtablesize() {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
}

fn limit(which: libc::c_int, wanted: u64) -> io::Result<()> {
    let mut current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `current` is a valid rlimit for the duration of both calls.
    unsafe {
        if libc::getrlimit(which, &mut current) != 0 {
            return Err(io::Error::last_os_error());
        }
        let value = wanted.min(current.rlim_max);
        let new = libc::rlimit {
            rlim_cur: value,
            rlim_max: value,
        };
        if libc::setrlimit(which, &new) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Output as it has arrived so far. A shared buffer rather than the reader's
/// return value, because the reader may never finish (see [`DRAIN`]).
struct Kept {
    bytes: Arc<Mutex<Vec<u8>>>,
    reader: tokio::task::JoinHandle<()>,
}

impl Kept {
    fn take(self) -> String {
        self.reader.abort();
        let bytes = self.bytes.lock().map(|b| b.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

fn keep(pipe: Option<impl AsyncRead + Unpin + Send + 'static>, cap: usize) -> Kept {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let shared = bytes.clone();
    let reader = tokio::spawn(async move {
        let Some(mut pipe) = pipe else { return };
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk).await {
            if n == 0 {
                return;
            }
            if let Ok(mut kept) = shared.lock() {
                let room = cap.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..n.min(room)]);
            }
        }
    });
    Kept { bytes, reader }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell() -> (tempfile::TempDir, Cell) {
        let dir = tempfile::tempdir().unwrap();
        let room = dir.path().join("Application Support/cell");
        let cell = Cell::make(&room, &dir.path().join("tools"), Reach::Closed).unwrap();
        (dir, cell)
    }

    #[tokio::test]
    async fn the_environment_is_blank_and_home_is_the_room() {
        std::env::set_var("SYNABIT_ROOM_TEST_SECRET", "x");
        let (_dir, cell) = cell();
        let ran = cell.run("env", Limits::default()).await.unwrap();
        assert!(ran.ok(), "{ran:?}");
        assert!(
            !ran.stdout.contains("SYNABIT_ROOM_TEST_SECRET"),
            "{}",
            ran.stdout
        );
        let home = format!("HOME={}", cell.work().display());
        assert!(ran.stdout.contains(&home), "{}", ran.stdout);
    }

    #[tokio::test]
    async fn a_command_past_its_time_is_stopped() {
        let (_dir, cell) = cell();
        let limits = Limits {
            wall: Duration::from_millis(300),
            ..Limits::default()
        };
        let ran = cell.run("sleep 30", limits).await.unwrap();
        assert!(ran.timed_out, "{ran:?}");
    }

    #[tokio::test]
    async fn output_past_the_cap_is_dropped_and_does_not_block() {
        let (_dir, cell) = cell();
        let limits = Limits {
            output: 10,
            ..Limits::default()
        };
        let ran = cell
            .run("head -c 200000 /dev/zero | tr '\\0' x", limits)
            .await
            .unwrap();
        assert!(ran.ok(), "{ran:?}");
        assert_eq!(ran.stdout.len(), 10);
    }

    #[tokio::test]
    async fn a_daemon_holding_stdout_does_not_hold_the_result() {
        let (_dir, cell) = cell();
        let started = std::time::Instant::now();
        let ran = cell
            .run(
                "perl -e 'use POSIX; fork && exit; setsid(); sleep 5' & echo done",
                Limits::default(),
            )
            .await
            .unwrap();
        assert!(ran.stdout.contains("done"), "{ran:?}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn only_three_descriptors_cross_into_the_room() {
        use std::os::fd::AsRawFd;
        let (dir, cell) = cell();
        let secret = dir.path().join("secret");
        std::fs::write(&secret, "x").unwrap();
        let file = std::fs::File::open(&secret).unwrap();
        // What a careless library does: the descriptor survives exec.
        // SAFETY: `file` is open for the length of this test.
        unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, 0) };
        let script = format!("cat <&{}", file.as_raw_fd());
        let ran = cell.run(&script, Limits::default()).await.unwrap();
        assert!(!ran.ok(), "{ran:?}");
        assert!(ran.stdout.is_empty(), "{ran:?}");
    }

    #[tokio::test]
    async fn a_limit_applies_inside() {
        let (_dir, cell) = cell();
        let ran = cell
            .run("ulimit -t; ulimit -c", Limits::default())
            .await
            .unwrap();
        assert_eq!(
            ran.stdout.split_whitespace().collect::<Vec<_>>(),
            ["600", "0"],
            "{ran:?}"
        );
    }
}
