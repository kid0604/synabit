//! Folders and files by descriptor, for the code that touches the person's own
//! folders: rehearsing on a copy of one, and applying what was approved to it.
//!
//! Everything here runs outside every room, with everything the app can do, on
//! folders anybody on this Mac could be changing at the same time. So a path is
//! never handed to the kernel whole. It is walked one name at a time from a
//! folder already open, and nothing on the way is followed: a folder swapped for
//! a link while the work goes on fails the step rather than leading elsewhere
//! (A18). Names are compared by what the volume would make of them.

use std::ffi::{CStr, CString, OsStr};
use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

use super::cell::{c_name, open_at, open_folder_at};

/// What one entry is, without following it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub kind: Kind,
    pub size: u64,
    pub nlink: u64,
    pub mtime_ns: i64,
    /// Set by the kernel on any change to the file or its name; nobody can set it.
    pub ctime_ns: i64,
    pub mode: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    File,
    Folder,
    Link,
    Other,
}

impl Stat {
    fn of(meta: &libc::stat) -> Self {
        let kind = match meta.st_mode & libc::S_IFMT {
            libc::S_IFREG => Kind::File,
            libc::S_IFDIR => Kind::Folder,
            libc::S_IFLNK => Kind::Link,
            _ => Kind::Other,
        };
        Self {
            dev: meta.st_dev as u64,
            ino: meta.st_ino,
            kind,
            size: meta.st_size as u64,
            nlink: meta.st_nlink as u64,
            mtime_ns: meta.st_mtime * 1_000_000_000 + meta.st_mtime_nsec,
            ctime_ns: meta.st_ctime * 1_000_000_000 + meta.st_ctime_nsec,
            mode: meta.st_mode as u32,
        }
    }

    /// The same file, unchanged in any way the kernel records.
    pub fn same_as(&self, other: &Stat) -> bool {
        self.dev == other.dev && self.ino == other.ino && self.size == other.size && self.ctime_ns == other.ctime_ns
    }
}

pub fn stat_fd(fd: BorrowedFd) -> io::Result<Stat> {
    // SAFETY: `meta` is written by the call before it is read.
    unsafe {
        let mut meta: libc::stat = std::mem::zeroed();
        if libc::fstat(fd.as_raw_fd(), &mut meta) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Stat::of(&meta))
    }
}

pub fn stat_at(at: BorrowedFd, name: &CStr) -> io::Result<Stat> {
    // SAFETY: `name` is a valid string relative to an open folder; nothing is followed.
    unsafe {
        let mut meta: libc::stat = std::mem::zeroed();
        if libc::fstatat(at.as_raw_fd(), name.as_ptr(), &mut meta, libc::AT_SYMLINK_NOFOLLOW) != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Stat::of(&meta))
    }
}

/// The names in a relative path, refusing anything that climbs or is absolute.
pub fn names(relative: &str) -> io::Result<Vec<CString>> {
    let mut out = Vec::new();
    for part in Path::new(relative).components() {
        match part {
            Component::Normal(name) => out.push(c_name(name)?),
            Component::CurDir => {}
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a path inside the folder")),
        }
    }
    Ok(out)
}

/// An absolute folder, opened one name at a time from `/`, following nothing.
pub fn open_path(absolute: &Path) -> io::Result<OwnedFd> {
    if !absolute.is_absolute() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not an absolute path"));
    }
    let mut at = OwnedFd::from(std::fs::File::open("/")?);
    for part in absolute.components().skip(1) {
        match part {
            Component::Normal(name) => at = open_folder_at(at.as_fd(), &c_name(name)?)?,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a plain path")),
        }
    }
    Ok(at)
}

/// The folder `relative` names under `at`, made on the way when `make` says so.
pub fn open_dirs(at: BorrowedFd, relative: &str, make: bool) -> io::Result<OwnedFd> {
    // SAFETY: `at` stays open for as long as the duplicate is used.
    let mut folder = unsafe {
        let dup = libc::dup(at.as_raw_fd());
        if dup < 0 {
            return Err(io::Error::last_os_error());
        }
        OwnedFd::from_raw_fd(dup)
    };
    for name in names(relative)? {
        if make {
            make_dir(folder.as_fd(), &name)?;
        }
        folder = open_folder_at(folder.as_fd(), &name)?;
    }
    Ok(folder)
}

/// Makes a folder; one already there is fine, whatever it is: opening it says.
pub fn make_dir(at: BorrowedFd, name: &CStr) -> io::Result<bool> {
    // SAFETY: a valid name relative to an open folder; `mkdirat` follows nothing.
    if unsafe { libc::mkdirat(at.as_raw_fd(), name.as_ptr(), 0o755) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::EEXIST) {
        Ok(false)
    } else {
        Err(error)
    }
}

/// A regular file, for reading, without following it or waiting on a FIFO.
pub fn open_file(at: BorrowedFd, name: &CStr) -> io::Result<std::fs::File> {
    let fd = open_at(at, name, libc::O_RDONLY | libc::O_NONBLOCK)?;
    let file = std::fs::File::from(fd);
    if stat_fd(file.as_fd())?.kind != Kind::File {
        return Err(io::Error::other("not a regular file"));
    }
    Ok(file)
}

/// Every name in an open folder, with what each is. Unreadable entries are left out.
pub fn list(at: BorrowedFd) -> io::Result<Vec<(CString, Stat)>> {
    let mut out = Vec::new();
    // SAFETY: `fdopendir` takes over a duplicate, and `closedir` closes it; each
    // entry is copied before the next `readdir`.
    unsafe {
        let dup = libc::dup(at.as_raw_fd());
        if dup < 0 {
            return Err(io::Error::last_os_error());
        }
        let dir = libc::fdopendir(dup);
        if dir.is_null() {
            let error = io::Error::last_os_error();
            libc::close(dup);
            return Err(error);
        }
        loop {
            let entry = libc::readdir(dir);
            if entry.is_null() {
                break;
            }
            let name = CStr::from_ptr((*entry).d_name.as_ptr());
            if matches!(name.to_bytes(), b"." | b"..") {
                continue;
            }
            let name = name.to_owned();
            if let Ok(stat) = stat_at(at, &name) {
                out.push((name, stat));
            }
        }
        libc::closedir(dir);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Moves an entry without ever replacing one: the move fails if the name is taken.
pub fn rename_excl(from: BorrowedFd, name: &CStr, to: BorrowedFd, new: &CStr) -> io::Result<()> {
    // SAFETY: valid names relative to open folders.
    let moved = unsafe { libc::renameatx_np(from.as_raw_fd(), name.as_ptr(), to.as_raw_fd(), new.as_ptr(), libc::RENAME_EXCL) };
    if moved == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Removes an empty folder; one with anything in it stays.
pub fn remove_empty_dir(at: BorrowedFd, name: &CStr) -> io::Result<()> {
    // SAFETY: a valid name; `unlinkat` with AT_REMOVEDIR removes only an empty folder.
    if unsafe { libc::unlinkat(at.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// A copy-on-write clone of `file` as `name` in `to`, on the same volume; `None`
/// when the volumes differ, or the volume cannot clone.
pub fn clone_into(file: BorrowedFd, to: BorrowedFd, name: &CStr) -> io::Result<Option<()>> {
    const CLONE_NOFOLLOW: u32 = 0x0001;
    const CLONE_NOOWNERCOPY: u32 = 0x0002;
    extern "C" {
        fn fclonefileat(src: libc::c_int, dst_dir: libc::c_int, dst: *const libc::c_char, flags: u32) -> libc::c_int;
    }
    // SAFETY: valid descriptors and name; the call creates a new file or fails.
    let cloned = unsafe { fclonefileat(file.as_raw_fd(), to.as_raw_fd(), name.as_ptr(), CLONE_NOFOLLOW | CLONE_NOOWNERCOPY) };
    if cloned == 0 {
        return Ok(Some(()));
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::EXDEV) | Some(libc::ENOTSUP) => Ok(None),
        _ => Err(error),
    }
}

/// Writes `bytes` as a new file, which must not be there yet. No execute bit.
pub fn create(at: BorrowedFd, name: &CStr, mut from: impl io::Read) -> io::Result<u64> {
    let fd = open_at(at, name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
    let mut file = std::fs::File::from(fd);
    let copied = io::copy(&mut from, &mut file)?;
    file.sync_all()?;
    Ok(copied)
}

/// Whether names on the volume of `at` differ only by case are one name.
pub fn ignores_case(at: BorrowedFd) -> bool {
    // SAFETY: a plain query on an open descriptor.
    unsafe { libc::fpathconf(at.as_raw_fd(), libc::_PC_CASE_SENSITIVE) == 0 }
}

/// What a name is on a volume: case folded where the volume ignores case, and in
/// one Unicode form, which APFS ignores on every volume.
pub fn key(name: &str, ignores_case: bool) -> String {
    use unicode_normalization::UnicodeNormalization;
    let one_form: String = name.nfc().collect();
    if ignores_case {
        one_form.to_lowercase()
    } else {
        one_form
    }
}

/// Removes `name` under `at` and everything in it, by descriptor, following
/// nothing: a link in it is removed, never what it points at. For a room's own
/// copies only, where a room could have left a link to anywhere.
pub fn remove_tree(at: BorrowedFd, name: &CStr) -> io::Result<()> {
    let stat = stat_at(at, name)?;
    if stat.kind == Kind::Folder {
        let inside = open_folder_at(at, name)?;
        for (child, _) in list(inside.as_fd())? {
            remove_tree(inside.as_fd(), &child)?;
        }
        return remove_empty_dir(at, name);
    }
    // SAFETY: a valid name; `unlinkat` removes the entry itself.
    if unsafe { libc::unlinkat(at.as_raw_fd(), name.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn text(name: &CStr) -> Option<&str> {
    OsStr::from_bytes(name.to_bytes()).to_str()
}
