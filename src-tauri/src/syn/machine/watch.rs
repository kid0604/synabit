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
use std::path::Path;
use std::time::Duration;

use super::cell::Cell;

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

fn may_write(pid: libc::pid_t, path: &CString) -> bool {
    // SAFETY: both strings outlive the call, and the variadic argument is the
    // one the path filter reads.
    unsafe {
        sandbox_check(
            pid,
            c"file-write-data".as_ptr(),
            SANDBOX_FILTER_PATH,
            path.as_ptr(),
        ) == 0
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
        .filter(|&pid| sandboxed(pid) && may_write(pid, &room) && !may_write(pid, &bait))
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
