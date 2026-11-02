//! Finding what a room left running.
//!
//! A command can fork twice, call `setsid`, close every descriptor and `chdir
//! /`. Its parent is then launchd, it has left the process group, and `lsof`
//! has nothing to connect it to the room. The kernel still knows which profile
//! it runs under, so the question goes to the kernel: could this process write
//! into that room's folder?
//!
//! Two things make the answer trustworthy, both measured:
//!
//! - **The path has to exist.** `sandbox_check` answers "no" for a path that is
//!   not there, even inside the room. A room can delete its own folder, and its
//!   daemon then disappears from the scan, so [`room_processes`] puts the folder
//!   back from outside first.
//! - **A bait outside every room.** System daemons run sandboxed with broad
//!   write access and would otherwise match; none of the room's processes can
//!   write to the bait.

use std::collections::BTreeSet;
use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::cell::{Cell, Limits, Ran};

/// From `<sys/proc_info.h>`; the libc crate has the function but not this.
const PROC_UID_ONLY: u32 = 4;
const SANDBOX_FILTER_NONE: libc::c_int = 0;
const SANDBOX_FILTER_PATH: libc::c_int = 1;

const STOP_ROUNDS: usize = 64;
const KILL_POLLS: usize = 50;
const KILL_POLL: Duration = Duration::from_millis(40);

extern "C" {
    /// Private to Apple, like `sandbox-exec` itself, which is why
    /// [`super::selftest`] checks that it still answers.
    fn sandbox_check(
        pid: libc::pid_t,
        operation: *const libc::c_char,
        filter: libc::c_int,
        ...
    ) -> libc::c_int;
}

/// Every process of this user.
fn own_processes() -> io::Result<Vec<libc::pid_t>> {
    // SAFETY: `pids` is valid for the size passed, and getuid cannot fail.
    unsafe {
        let uid = libc::getuid();
        let mut pids = vec![0 as libc::pid_t; 16384];
        let bytes = libc::proc_listpids(
            PROC_UID_ONLY,
            uid,
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int,
        );
        if bytes < 0 {
            return Err(io::Error::last_os_error());
        }
        pids.truncate(bytes as usize / std::mem::size_of::<libc::pid_t>());
        pids.retain(|&pid| pid != 0);
        Ok(pids)
    }
}

pub fn process_count() -> io::Result<usize> {
    own_processes().map(|pids| pids.len())
}

/// 0 is allowed, 1 is denied, and anything else is no answer: the process has gone.
fn check_write(pid: libc::pid_t, path: &CString) -> libc::c_int {
    // SAFETY: both strings outlive the call, and the variadic argument is the
    // one the path filter reads.
    unsafe {
        sandbox_check(
            pid,
            c"file-write-data".as_ptr(),
            SANDBOX_FILTER_PATH,
            path.as_ptr(),
        )
    }
}

fn sandboxed(pid: libc::pid_t) -> bool {
    // SAFETY: a null operation with no filter only asks whether it is sandboxed.
    unsafe { sandbox_check(pid, std::ptr::null(), SANDBOX_FILTER_NONE) == 1 }
}

/// The processes running under this room's profile, wherever they have gone.
/// `bait` must be a file outside every room; an error means the scan cannot be
/// trusted, and nothing that depends on it should go ahead.
pub fn room_processes(cell: &Cell, bait: &Path) -> io::Result<Vec<libc::pid_t>> {
    cell.restore()?;
    std::fs::metadata(bait)?;
    let c_path = |path: &Path| {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))
    };
    let room = c_path(cell.room())?;
    let bait = c_path(bait)?;
    Ok(own_processes()?
        .into_iter()
        // Denied, not merely "not allowed". A process that can write everywhere
        // and exits between these two calls gives no answer about the bait, and
        // was being counted as this room's, on the way to being signalled.
        .filter(|&pid| sandboxed(pid) && check_write(pid, &room) == 0 && check_write(pid, &bait) == 1)
        .collect())
}

fn signal(pids: impl IntoIterator<Item = libc::pid_t>, signal: libc::c_int) {
    for pid in pids {
        // SAFETY: a plain signal; a pid that has since gone only returns ESRCH.
        unsafe { libc::kill(pid, signal) };
    }
}

/// Ends everything the room still has running, and says how many that was.
///
/// Stopped first and killed afterwards, scanning until no new process turns up:
/// killing a process that is forking in a loop only ever catches the last round.
/// An error means something may still be running.
pub fn clear_room(cell: &Cell, bait: &Path) -> io::Result<usize> {
    let mut stopped = BTreeSet::new();
    let mut settled = false;
    for _ in 0..STOP_ROUNDS {
        let found = room_processes(cell, bait)?;
        let new: Vec<_> = found.into_iter().filter(|p| !stopped.contains(p)).collect();
        if new.is_empty() {
            settled = true;
            break;
        }
        signal(new.iter().copied(), libc::SIGSTOP);
        stopped.extend(new);
    }
    signal(stopped.iter().copied(), libc::SIGKILL);
    if !settled {
        return Err(io::Error::other("the room starts processes faster than they stop"));
    }
    for _ in 0..KILL_POLLS {
        let left = room_processes(cell, bait)?;
        if left.is_empty() {
            return Ok(stopped.len());
        }
        signal(left, libc::SIGKILL);
        std::thread::sleep(KILL_POLL);
    }
    Err(io::Error::other("a process in the room would not die"))
}

/// How much of the machine a room may hold. Both are measured from outside, since
/// macOS has nothing like a cgroup and `ulimit -v` was measured to do nothing:
/// 800 MB was granted under a limit of 200.
#[derive(Clone, Copy, Debug)]
pub struct Ceiling {
    pub memory: u64,
    pub disk: u64,
    pub poll: Duration,
}

impl Default for Ceiling {
    /// Provisional, until there is real work to measure against.
    fn default() -> Self {
        Self {
            memory: 8 << 30,
            disk: 20 << 30,
            poll: Duration::from_secs(1),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Over {
    Memory { bytes: u64 },
    Disk { bytes: u64 },
}

/// Resident memory of these processes. One that has gone counts for nothing.
fn memory_of(pids: &[libc::pid_t]) -> u64 {
    pids.iter()
        .map(|&pid| {
            // SAFETY: `info` is a plain struct of integers, zero is a valid value
            // for all of them, and the size passed is its own.
            unsafe {
                let mut info: libc::proc_taskinfo = std::mem::zeroed();
                let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
                let got = libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDTASKINFO,
                    0,
                    (&raw mut info).cast(),
                    size,
                );
                if got == size {
                    info.pti_resident_size
                } else {
                    0
                }
            }
        })
        .sum()
}

/// Space under `folder`, stopping as soon as it is past `enough`. The larger of a
/// file's length and its blocks, since a filesystem may allocate after the write.
/// A sparse file counts for more than it takes, which errs the right way. Never
/// less than a block each, since a million empty files fill a disk too. A link is
/// counted and not followed.
fn disk_of(folder: &Path, enough: u64) -> u64 {
    const LEAST: u64 = 4096;
    let mut used = 0;
    let mut waiting = vec![folder.to_path_buf()];
    while let Some(next) = waiting.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            used += (meta.blocks() * 512).max(meta.len()).max(LEAST);
            if used > enough {
                return used;
            }
            if meta.is_dir() {
                waiting.push(entry.path());
            }
        }
    }
    used
}

fn over(cell: &Cell, bait: &Path, ceiling: Ceiling) -> io::Result<Option<Over>> {
    let bytes = memory_of(&room_processes(cell, bait)?);
    if bytes > ceiling.memory {
        return Ok(Some(Over::Memory { bytes }));
    }
    let bytes = disk_of(cell.room(), ceiling.disk);
    if bytes > ceiling.disk {
        return Ok(Some(Over::Disk { bytes }));
    }
    Ok(None)
}

#[derive(Debug)]
pub struct Watched {
    pub ran: Ran,
    /// The ceiling it went through, when that is why it stopped.
    pub over: Option<Over>,
}

/// Runs a command with the room's ceilings being measured the whole time, and
/// clears the room when it is done: a command does not leave a daemon behind.
/// The kernel's rules hold whether or not this is watching. This only keeps a room
/// from taking the machine down with it. A scan that fails ends the command too,
/// because then nothing is being watched.
pub async fn run_watched(
    cell: &Cell,
    bait: &Path,
    script: &str,
    limits: Limits,
    ceiling: Ceiling,
) -> io::Result<Watched> {
    let measure = |check: fn(&Cell, &Path, Ceiling) -> io::Result<Option<Over>>| {
        let (cell, bait) = (cell.clone(), bait.to_path_buf());
        tokio::task::spawn_blocking(move || check(&cell, &bait, ceiling))
    };
    let clear = || {
        let (cell, bait) = (cell.clone(), bait.to_path_buf());
        tokio::task::spawn_blocking(move || clear_room(&cell, &bait))
    };
    let run = cell.run(script, limits);
    tokio::pin!(run);
    let mut found = None;
    let ran = loop {
        tokio::select! {
            ran = &mut run => break ran?,
            // Goes on after the first time. A room already past its ceiling is
            // found before the command has even reached the sandbox, when there is
            // nothing yet to stop, and it must not then be left to run.
            () = tokio::time::sleep(ceiling.poll) => {
                match measure(over).await.map_err(io::Error::other)? {
                    Ok(None) => {}
                    Ok(Some(past)) => {
                        found.get_or_insert(past);
                        clear().await.map_err(io::Error::other)??;
                    }
                    Err(error) => {
                        let _ = clear().await;
                        return Err(error);
                    }
                }
            }
        }
    };
    clear().await.map_err(io::Error::other)??;
    if found.is_none() {
        found = measure(over).await.map_err(io::Error::other)??;
    }
    Ok(Watched { ran, over: found })
}

/// Proof that nothing was running in a room when this was made. What leaves a
/// room is read only with one of these: a file checked while a daemon can still
/// rewrite it has not been checked. Holds as long as nobody runs the room again.
#[derive(Debug)]
pub struct Frozen<'a> {
    cell: &'a Cell,
}

impl Frozen<'_> {
    pub fn cell(&self) -> &Cell {
        self.cell
    }
}

pub fn freeze<'a>(cell: &'a Cell, bait: &Path) -> io::Result<Frozen<'a>> {
    clear_room(cell, bait)?;
    Ok(Frozen { cell })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syn::machine::profile::Reach;

    struct Room {
        _dir: tempfile::TempDir,
        cell: Cell,
        bait: std::path::PathBuf,
    }

    fn room() -> Room {
        let dir = tempfile::tempdir().unwrap();
        let cell = Cell::make(
            &dir.path().join("Application Support/c"),
            &dir.path().join("t"),
            Reach::Closed,
        )
        .unwrap();
        let bait = dir.path().join("bait");
        std::fs::write(&bait, "x").unwrap();
        Room {
            _dir: dir,
            cell,
            bait,
        }
    }

    const MB: u64 = 1 << 20;
    /// Both then wait, so that a stop is not lost to the command simply finishing.
    fn hold_memory(wait: u32) -> String {
        format!(r#"perl -e '$x = "a" x (200 * 1024 * 1024); sleep {wait}'; echo end"#)
    }
    fn fill_disk(wait: u32) -> String {
        format!("dd if=/dev/urandom of=big bs=1048576 count=30; sleep {wait}; echo end")
    }

    fn ceiling(memory: u64, disk: u64) -> Ceiling {
        Ceiling {
            memory,
            disk,
            poll: Duration::from_millis(100),
        }
    }

    async fn watched(room: &Room, script: &str, ceiling: Ceiling) -> Watched {
        run_watched(&room.cell, &room.bait, script, Limits::default(), ceiling)
            .await
            .unwrap()
    }

    /// The same command under each ceiling: stopped under a low one, untouched
    /// under a high one, so what stopped it was the ceiling.
    #[tokio::test]
    async fn a_room_past_its_memory_is_stopped() {
        let room = room();
        let free = watched(&room, &hold_memory(1), ceiling(4096 * MB, 4096 * MB)).await;
        assert_eq!(free.over, None, "{free:?}");
        assert!(free.ran.stdout.contains("end"), "{free:?}");

        let held = watched(&room, &hold_memory(60), ceiling(100 * MB, 4096 * MB)).await;
        assert!(matches!(held.over, Some(Over::Memory { .. })), "{held:?}");
    }

    #[tokio::test]
    async fn a_room_past_its_disk_is_stopped() {
        let room = room();
        let free = watched(&room, &fill_disk(0), ceiling(4096 * MB, 4096 * MB)).await;
        assert_eq!(free.over, None, "{free:?}");
        assert!(free.ran.stdout.contains("end"), "{free:?}");

        // `big` is still there from the first run, so this room starts over its
        // ceiling: on a busy machine, before the command is in the sandbox at all.
        let started = std::time::Instant::now();
        let held = watched(&room, &fill_disk(60), ceiling(4096 * MB, 10 * MB)).await;
        assert!(matches!(held.over, Some(Over::Disk { .. })), "{held:?}");
        assert!(started.elapsed() < Duration::from_secs(45), "ran to the end: {held:?}");
    }

    #[test]
    fn many_empty_files_count_and_a_link_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let inside = dir.path().join("inside");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&inside).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("large"), vec![1u8; 1 << 20]).unwrap();
        std::os::unix::fs::symlink(&outside, inside.join("link")).unwrap();
        assert!(disk_of(&inside, u64::MAX) < 1 << 20);

        for i in 0..100 {
            std::fs::write(inside.join(i.to_string()), "").unwrap();
        }
        assert!(disk_of(&inside, u64::MAX) >= 100 * 4096);
        assert!(disk_of(&inside, 4096) < 100 * 4096, "it should stop early");
    }

    #[tokio::test]
    async fn a_command_does_not_leave_a_daemon_behind() {
        let room = room();
        let got = watched(
            &room,
            &crate::syn::machine::selftest::daemon("sleep 20"),
            ceiling(4096 * MB, 4096 * MB),
        )
        .await;
        assert!(got.ran.ok(), "{got:?}");
        assert!(room_processes(&room.cell, &room.bait).unwrap().is_empty());
    }
}
