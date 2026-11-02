//! Running a command in a room.
//!
//! The environment is blank apart from what is set here: an API key in the
//! app's environment must not be one `env` away. Output is read by this process
//! through pipes, because `sandbox-exec` cannot write to a file outside the room
//! and because a command must not choose where its output goes.

use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
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
    /// The largest one file may get, held by the kernel. [`watch`] measures the
    /// whole folder, but only once a second, and a disk fills faster than that.
    pub file: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            wall: Duration::from_secs(120),
            cpu: 600,
            output: 1 << 20,
            file: 20 << 30,
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
    ///
    /// Only the folder above is resolved. `dir` itself may be a link a room left
    /// there, and following it would make that link's target the room. Not `pub`
    /// outside this module: everyone else gets a room from [`super::rooms`].
    pub(super) fn make(dir: &Path, tools: &Path, reach: Reach) -> io::Result<Self> {
        let (Some(above), Some(name)) = (dir.parent(), dir.file_name()) else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "no parent"));
        };
        std::fs::create_dir_all(above)?;
        std::fs::create_dir_all(tools)?;
        let room = above.canonicalize()?.join(name);
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
    ///
    /// By descriptor, because a room can also put a link where its folder was,
    /// and this runs with everything the app can do. The folder above the room
    /// is not the room's to change.
    pub fn restore(&self) -> io::Result<()> {
        let room = self.room();
        let (Some(above), Some(name)) = (room.parent(), room.file_name()) else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "no parent"));
        };
        std::fs::create_dir_all(above)?;
        let above = std::fs::File::open(above)?;
        let room = real_folder(above.as_fd(), &c_name(name)?)?;
        real_folder(room.as_fd(), c"work")?;
        real_folder(room.as_fd(), c"tmp")?;
        Ok(())
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
        let file = limits.file;

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
                limit(libc::RLIMIT_FSIZE, file)?;
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

pub(super) fn c_name(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))
}

/// Opens without following a link in the last step. Always close-on-exec.
pub(super) fn open_at(parent: BorrowedFd, name: &CStr, flags: libc::c_int) -> io::Result<OwnedFd> {
    // SAFETY: `name` is a valid string for the call, and a descriptor that
    // `openat` returns is ours alone to own.
    unsafe {
        let fd = libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o644 as libc::c_uint,
        );
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

pub(super) fn open_folder_at(parent: BorrowedFd, name: &CStr) -> io::Result<OwnedFd> {
    open_at(parent, name, libc::O_RDONLY | libc::O_DIRECTORY)
}

/// The folder called `name`, a real one, whatever the room left there. `mkdirat`
/// and `unlinkat` act on the entry itself and never follow it. A few tries, since
/// a daemon may be swapping it back; the caller gets an error rather than a link.
fn real_folder(parent: BorrowedFd, name: &CStr) -> io::Result<OwnedFd> {
    for _ in 0..8 {
        // SAFETY: `name` is a valid string; failure is read from errno below.
        if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o755) } != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EEXIST) {
                return Err(error);
            }
        }
        match open_folder_at(parent, name) {
            Ok(folder) => return Ok(folder),
            Err(error) => match error.raw_os_error() {
                Some(libc::ELOOP) | Some(libc::ENOTDIR) => {
                    // SAFETY: as above. If it is already gone, the next try makes it.
                    unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
                }
                Some(libc::ENOENT) => {}
                _ => return Err(error),
            },
        }
    }
    Err(io::Error::other("the room keeps replacing its folder"))
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

    /// A room can write to its own path, so it can put a link there. What runs
    /// outside the room must not then make folders wherever the link points.
    #[tokio::test]
    async fn a_room_that_becomes_a_link_does_not_steer_the_app() {
        let (dir, cell) = cell();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let room = cell.room().display().to_string();
        let script = format!(
            "cd /; rm -rf '{room}'; ln -s '{}' '{room}'",
            elsewhere.canonicalize().unwrap().display()
        );
        let ran = cell.run(&script, Limits::default()).await.unwrap();
        assert!(ran.ok(), "{ran:?}");
        assert!(cell.room().is_symlink(), "the room did not manage it");

        cell.restore().unwrap();

        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        assert!(!cell.room().is_symlink());
        assert!(cell.work().is_dir());
    }

    #[tokio::test]
    async fn making_a_room_does_not_follow_a_link_left_in_its_place() {
        let (dir, cell) = cell();
        let elsewhere = dir.path().canonicalize().unwrap().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::fs::remove_dir_all(cell.room()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, cell.room()).unwrap();

        let again = Cell::make(cell.room(), &cell.profile.tools, Reach::Closed).unwrap();

        assert_eq!(again.room(), cell.room());
        assert!(!again.room().is_symlink());
        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
    }

    /// The control is the first file, under the limit, in the same room.
    #[tokio::test]
    async fn the_kernel_stops_a_file_past_its_limit() {
        let (_dir, cell) = cell();
        let limits = Limits {
            file: 1 << 20,
            ..Limits::default()
        };
        let size = |name: &str| cell.work().join(name).metadata().unwrap().len();

        let small = "dd if=/dev/zero of=small bs=1024 count=512";
        assert!(cell.run(small, limits).await.unwrap().ok());
        assert_eq!(size("small"), 512 << 10);

        let large = "dd if=/dev/zero of=large bs=1024 count=4096";
        assert!(!cell.run(large, limits).await.unwrap().ok());
        assert!(size("large") <= 1 << 20, "{}", size("large"));
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
