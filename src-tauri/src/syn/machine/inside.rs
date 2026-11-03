//! A room's files, read and written from outside it.
//!
//! The model reads and edits files in a room, and Rust brings vault data in. Both
//! run outside the room with everything the app can do, while whatever is in the
//! room may still be running and may have put a link where a folder was. So every
//! path is walked one descriptor at a time from the room's `work` folder, nothing
//! is followed, and only a regular file with one name is opened. A link that
//! points at `~/.ssh` is refused rather than read or written through.
//!
//! Reading here is not leaving: what is read goes to the model, as the room's
//! stdout does. A file that leaves for the vault or a folder goes through
//! [`super::export`], which also stops the room first.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

use super::cell::{c_name, open_at, open_folder_at, Cell};
use super::export::Refused;
use super::sanitize;

/// The folder names on the way, and the file's own.
fn parts(relative: &Path) -> Result<(Vec<&OsStr>, &OsStr), Refused> {
    let mut names = Vec::new();
    for part in relative.components() {
        match part {
            Component::Normal(name) => names.push(name),
            Component::CurDir => {}
            _ => return Err(Refused::NotInside),
        }
    }
    let last = names.pop().ok_or(Refused::NotInside)?;
    Ok((names, last))
}

pub(super) fn refused_open(error: std::io::Error) -> Refused {
    match error.raw_os_error() {
        Some(libc::ENOENT) => Refused::Missing,
        Some(libc::ELOOP) => Refused::Link,
        // A socket refuses to open at all, and a link met on the way to a folder
        // can answer as "not a folder" rather than as a link.
        Some(libc::ENOTDIR) | Some(libc::EOPNOTSUPP) | Some(libc::ENXIO) => Refused::NotAFile,
        _ => Refused::Io(error),
    }
}

/// The folder `folders` names under `work`, made on the way when `make` says so.
fn folder(cell: &Cell, folders: &[&OsStr], make: bool) -> Result<OwnedFd, Refused> {
    cell.restore()?;
    let above = cell.room().parent().ok_or(Refused::NotInside)?;
    let above = File::open(above)?;
    let room_name = cell.room().file_name().ok_or(Refused::NotInside)?;
    let room = open_folder_at(above.as_fd(), &c_name(room_name)?).map_err(refused_open)?;
    let mut at = open_folder_at(room.as_fd(), c"work").map_err(refused_open)?;
    for name in folders {
        let name = c_name(name)?;
        if make {
            // SAFETY: `name` is a valid string; `mkdirat` acts on the entry and
            // follows nothing. An entry already there is opened, or refused, below.
            unsafe { libc::mkdirat(std::os::fd::AsRawFd::as_raw_fd(&at), name.as_ptr(), 0o755) };
        }
        at = open_folder_at(at.as_fd(), &name).map_err(refused_open)?;
    }
    Ok(at)
}

/// Opened for reading, once it is known to be a regular file with one name.
/// O_NONBLOCK so that a FIFO with no writer answers instead of waiting forever;
/// it changes nothing for a regular file.
pub(super) fn open_file(cell: &Cell, relative: &Path) -> Result<File, Refused> {
    let (folders, last) = parts(relative)?;
    let at = folder(cell, &folders, false)?;
    let file = open_at(at.as_fd(), &c_name(last)?, libc::O_RDONLY | libc::O_NONBLOCK)
        .map_err(refused_open)?;
    let file = File::from(file);
    let meta = file.metadata()?;
    if !meta.file_type().is_file() {
        return Err(Refused::NotAFile);
    }
    if meta.nlink() != 1 {
        return Err(Refused::HardLinked);
    }
    Ok(file)
}

/// At most `max` bytes of a file, and whether there was more.
pub fn read(cell: &Cell, relative: &Path, max: u64) -> Result<(Vec<u8>, bool), Refused> {
    let file = open_file(cell, relative)?;
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    let more = bytes.len() as u64 > max;
    bytes.truncate(max as usize);
    Ok((bytes, more))
}

/// Writes a whole file, making the folders on the way. Over a file already there
/// only if it is a regular one with one name: it is opened first and checked, and
/// only then emptied, so a hard link to something else is never cut short.
pub fn write(cell: &Cell, relative: &Path, bytes: &[u8]) -> Result<(), Refused> {
    let (folders, last) = parts(relative)?;
    let at = folder(cell, &folders, true)?;
    let fd = open_at(at.as_fd(), &c_name(last)?, libc::O_WRONLY | libc::O_CREAT | libc::O_NONBLOCK)
        .map_err(refused_open)?;
    let mut file = File::from(fd);
    let meta = file.metadata()?;
    if !meta.file_type().is_file() {
        return Err(Refused::NotAFile);
    }
    if meta.nlink() != 1 {
        return Err(Refused::HardLinked);
    }
    file.set_len(0)?;
    file.write_all(bytes)?;
    Ok(())
}

/// One entry of the room's `work` folder, as the model is told of it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Entry {
    /// With everything hidden written out: the room chose it.
    pub name: String,
    pub bytes: u64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
}

/// What is at the top of `work`, at most `most` entries by name. Read from the
/// descriptor, not the path, which the room could have swapped for a link to a
/// folder whose names are not its to tell. Nothing is followed: a link is listed
/// as what it is, at its own size.
pub fn list(cell: &Cell, most: usize) -> Result<(Vec<Entry>, usize), Refused> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;

    let at = folder(cell, &[], false)?;
    let raw = at.as_raw_fd();
    let mut entries = Vec::new();
    // SAFETY: `fdopendir` takes the descriptor over, and `closedir` closes it;
    // each `dirent` is read before the next call to `readdir`. `fstatat` is given
    // a valid name relative to the same open folder, and never follows a link.
    unsafe {
        let listed = libc::dup(raw);
        if listed < 0 {
            return Err(Refused::Io(std::io::Error::last_os_error()));
        }
        let dir = libc::fdopendir(listed);
        if dir.is_null() {
            let error = std::io::Error::last_os_error();
            libc::close(listed);
            return Err(Refused::Io(error));
        }
        loop {
            let entry = libc::readdir(dir);
            if entry.is_null() {
                break;
            }
            let name = std::ffi::CStr::from_ptr((*entry).d_name.as_ptr());
            if matches!(name.to_bytes(), b"." | b"..") {
                continue;
            }
            let mut meta: libc::stat = std::mem::zeroed();
            if libc::fstatat(raw, name.as_ptr(), &mut meta, libc::AT_SYMLINK_NOFOLLOW) != 0 {
                continue;
            }
            let is_folder = meta.st_mode & libc::S_IFMT == libc::S_IFDIR;
            entries.push(Entry {
                name: sanitize::shown(&OsStr::from_bytes(name.to_bytes()).to_string_lossy()),
                bytes: if is_folder { 0 } else { meta.st_size as u64 },
                folder: is_folder,
            });
        }
        libc::closedir(dir);
    }
    drop(at);
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let all = entries.len();
    entries.truncate(most);
    Ok((entries, all))
}

#[cfg(test)]
mod tests {
    use super::super::cell::Limits;
    use super::super::profile::Reach;
    use super::*;

    fn room() -> (tempfile::TempDir, Cell) {
        let dir = tempfile::tempdir().unwrap();
        let cell = Cell::make(&dir.path().join("c"), &dir.path().join("t"), Reach::Closed).unwrap();
        (dir, cell)
    }

    fn secret(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let secret = dir.path().canonicalize().unwrap().join("id_ed25519");
        std::fs::write(&secret, "PRIVATE KEY").unwrap();
        secret
    }

    #[tokio::test]
    async fn what_is_written_is_what_the_room_reads() {
        let (_dir, cell) = room();
        write(&cell, Path::new("in/data.csv"), b"a,b\n1,2\n").unwrap();
        let ran = cell.run("cat in/data.csv", Limits::default()).await.unwrap();
        assert_eq!(ran.stdout, "a,b\n1,2\n");

        let ran = cell.run("printf 'from the room' > out.txt", Limits::default()).await.unwrap();
        assert!(ran.ok());
        assert_eq!(read(&cell, Path::new("out.txt"), 100).unwrap(), (b"from the room".to_vec(), false));
        assert_eq!(read(&cell, Path::new("out.txt"), 4).unwrap(), (b"from".to_vec(), true));
    }

    /// The room puts a link where a file was, and where a folder was. Neither is
    /// read through, nor written through: the secret stays as it was.
    #[tokio::test]
    async fn a_link_the_room_left_is_neither_read_nor_written_through() {
        let (dir, cell) = room();
        let secret = secret(&dir);
        let outside = dir.path().canonicalize().unwrap();
        let script = format!(
            "ln -s '{}' key.txt; ln -s '{}' up",
            secret.display(),
            outside.display()
        );
        assert!(cell.run(&script, Limits::default()).await.unwrap().ok());

        assert!(matches!(read(&cell, Path::new("key.txt"), 100), Err(Refused::Link)));
        assert!(matches!(write(&cell, Path::new("key.txt"), b"x"), Err(Refused::Link)));
        assert!(matches!(
            read(&cell, Path::new("up/id_ed25519"), 100),
            Err(Refused::NotAFile | Refused::Link)
        ));
        assert!(matches!(
            write(&cell, Path::new("up/id_ed25519"), b"x"),
            Err(Refused::NotAFile | Refused::Link)
        ));
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "PRIVATE KEY");
        assert!(!outside.join("x").exists());
    }

    #[tokio::test]
    async fn a_fifo_answers_rather_than_hangs() {
        let (_dir, cell) = room();
        assert!(cell.run("mkfifo pipe", Limits::default()).await.unwrap().ok());
        let got = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            (read(&cell, Path::new("pipe"), 10), write(&cell, Path::new("pipe"), b"x"))
        })
        .await
        .expect("answered");
        assert!(matches!(got.0, Err(Refused::NotAFile)), "{:?}", got.0);
        assert!(matches!(got.1, Err(Refused::NotAFile | Refused::Io(_))), "{:?}", got.1);
    }

    #[test]
    fn nothing_leads_out_of_work() {
        let (_dir, cell) = room();
        for path in ["../x", "/etc/passwd", "", "a/../../b"] {
            assert!(matches!(write(&cell, Path::new(path), b"x"), Err(Refused::NotInside)), "{path}");
            assert!(matches!(read(&cell, Path::new(path), 1), Err(Refused::NotInside)), "{path}");
        }
    }

    #[tokio::test]
    async fn the_listing_shows_hidden_characters_and_follows_nothing() {
        let (dir, cell) = room();
        let secret = secret(&dir);
        let script = format!("printf abc > \"$(printf 'a\\342\\200\\256txt.exe')\"; ln -s '{}' k; mkdir sub", secret.display());
        assert!(cell.run(&script, Limits::default()).await.unwrap().ok());
        let (entries, all) = list(&cell, 10).unwrap();
        assert_eq!(all, 3);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a\\u{202E}txt.exe"), "{names:?}");
        let link = entries.iter().find(|e| e.name == "k").unwrap();
        assert!(link.bytes < 200, "the link's own size, not the secret's: {link:?}");
        assert!(entries.iter().any(|e| e.name == "sub" && e.folder));
    }
}
