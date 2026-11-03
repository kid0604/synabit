//! The one way a file leaves a room.
//!
//! The kernel holds the room, but this runs outside it with everything the app
//! can do, so this is what a room aims at: a link to `~/.ssh/id_ed25519` called
//! `out.txt`, a FIFO that hangs the reader forever, a file swapped after it was
//! checked. So the room is stopped first ([`Frozen`]), the path is walked one
//! descriptor at a time without following anything, only a regular file is read,
//! and what comes out is bytes. Nothing the room set on the file comes with them.
//!
//! Where the bytes may go is not decided here. [`save`] is for a folder somebody
//! chose. A file bound for the vault goes through its gate for creating files
//! instead, and never into Syn's own folders.

use std::ffi::{CStr, CString};
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::cell::open_at;
use super::inside;
use super::sanitize;
use super::watch::Frozen;

const QUARANTINE: &CStr = c"com.apple.quarantine";

#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("that is not a path inside the room")]
    NotInside,
    #[error("no such file")]
    Missing,
    #[error("that is a link")]
    Link,
    #[error("that is not a regular file")]
    NotAFile,
    #[error("that file has another name somewhere")]
    HardLinked,
    #[error("that file is larger than {max} bytes")]
    TooLarge { max: u64 },
    #[error("that file has no name it can be saved under")]
    BadName,
    #[error("a file with that name is already there")]
    AlreadyThere,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// A file that has left a room. Holds no path back into it.
#[derive(Debug)]
pub struct Out {
    /// Safe to save under.
    pub name: String,
    /// The name as the room wrote it, with everything visible, for a card.
    pub shown: String,
    pub bytes: Vec<u8>,
}

/// Reads `relative`, a path under the room's `work` folder.
pub fn read(frozen: &Frozen, relative: &Path, max: u64) -> Result<Out, Refused> {
    let last = relative.file_name().ok_or(Refused::NotInside)?;
    // Walked by descriptor, following nothing, and nothing is running to change
    // what was found. See [`inside`].
    let file = inside::open_file(frozen.cell(), relative)?;
    let meta = file.metadata()?;
    if meta.size() > max {
        return Err(Refused::TooLarge { max });
    }
    let mut bytes = Vec::with_capacity(meta.size() as usize);
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(Refused::TooLarge { max });
    }

    Ok(Out {
        name: sanitize::file_name(last).ok_or(Refused::BadName)?,
        shown: sanitize::shown(&last.to_string_lossy()),
        bytes,
    })
}

/// Marks a file as having come from outside, so Gatekeeper stops it when somebody
/// opens it. This is what actually stops a room's `.app` or `.command`: without
/// it they run at a double click. Same value as was measured against Gatekeeper.
fn quarantine(file: BorrowedFd) -> io::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_secs())
        .unwrap_or(0);
    let value = format!("0081;{now:x};Synabit;");
    // SAFETY: the name and value are valid for the lengths given.
    let result = unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            QUARANTINE.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn write_marked(fd: OwnedFd, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::File::from(fd);
    file.write_all(bytes)?;
    quarantine(file.as_fd())?;
    file.sync_all()
}

/// Saves into `folder`, a real folder that is not a room's, never over anything
/// already there. Appears whole or not at all, with no execute bit and only the
/// quarantine mark.
pub fn save(out: &Out, folder: &Path) -> Result<PathBuf, Refused> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let folder_fd = std::fs::File::open(folder)?;
    let dir = folder_fd.as_raw_fd();
    let name = CString::new(out.name.as_str()).map_err(|_| Refused::BadName)?;
    let part = CString::new(format!(
        ".synabit-export-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
    .map_err(|_| Refused::BadName)?;

    let fd = open_at(
        folder_fd.as_fd(),
        &part,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
    )?;
    let finish = write_marked(fd, &out.bytes).and_then(|()| {
        // SAFETY: both names are valid strings relative to a descriptor we hold.
        let renamed = unsafe {
            libc::renameatx_np(dir, part.as_ptr(), dir, name.as_ptr(), libc::RENAME_EXCL)
        };
        if renamed == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    });
    if let Err(error) = finish {
        // SAFETY: as above; this name is the one this function just created.
        unsafe { libc::unlinkat(dir, part.as_ptr(), 0) };
        return Err(if error.raw_os_error() == Some(libc::EEXIST) {
            Refused::AlreadyThere
        } else {
            Refused::Io(error)
        });
    }
    Ok(folder.join(&out.name))
}

#[cfg(test)]
mod tests {
    use super::super::cell::{Cell, Limits};
    use super::super::profile::Reach;
    use super::super::watch;
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::time::Duration;

    const MAX: u64 = 1 << 20;

    struct Room {
        dir: tempfile::TempDir,
        cell: Cell,
        bait: PathBuf,
    }

    impl Room {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            // Short: a socket's path has to fit in 104 bytes.
            let cell = Cell::make(&dir.path().join("c"), &dir.path().join("t"), Reach::Closed)
                .unwrap();
            let bait = dir.path().join("bait");
            std::fs::write(&bait, "outside every room").unwrap();
            Self { dir, cell, bait }
        }

        /// Runs `script` in the room, requires that it worked, and freezes.
        async fn after(&self, script: &str) -> Frozen<'_> {
            let ran = self.cell.run(script, Limits::default()).await.unwrap();
            assert!(ran.ok(), "{ran:?}");
            watch::freeze(&self.cell, &self.bait).unwrap()
        }

        fn secret(&self) -> PathBuf {
            let secret = self.dir.path().canonicalize().unwrap().join("id_ed25519");
            std::fs::write(&secret, "PRIVATE KEY").unwrap();
            secret
        }
    }

    fn xattr(path: &Path, name: &str) -> Option<String> {
        let out = std::process::Command::new("/usr/bin/xattr")
            .args(["-p", name])
            .arg(path)
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// The control: everything below is only worth anything if this works.
    #[tokio::test]
    async fn a_plain_file_comes_out() {
        let room = Room::new();
        let frozen = room.after("mkdir -p a/b; printf hello > a/b/out.txt").await;
        let out = read(&frozen, Path::new("a/b/out.txt"), MAX).unwrap();
        assert_eq!(out.bytes, b"hello");
        assert_eq!(out.name, "out.txt");
        assert_eq!(out.shown, "out.txt");
    }

    /// A3.
    #[tokio::test]
    async fn a_link_to_a_secret_is_refused() {
        let room = Room::new();
        let secret = room.secret();
        let frozen = room
            .after(&format!("ln -s '{}' out.txt", secret.display()))
            .await;
        let got = read(&frozen, Path::new("out.txt"), MAX);
        assert!(matches!(got, Err(Refused::Link)), "{got:?}");
    }

    #[tokio::test]
    async fn a_link_on_the_way_to_a_file_is_refused() {
        let room = Room::new();
        let secret = room.secret();
        let above = secret.parent().unwrap().display().to_string();
        let frozen = room.after(&format!("ln -s '{above}' up")).await;
        let got = read(&frozen, Path::new("up/id_ed25519"), MAX);
        assert!(
            matches!(got, Err(Refused::Link | Refused::NotAFile)),
            "{got:?}"
        );
    }

    #[tokio::test]
    async fn a_work_folder_swapped_for_a_link_is_not_followed() {
        let room = Room::new();
        let secret = room.secret();
        let above = secret.parent().unwrap().display().to_string();
        let frozen = room
            .after(&format!("cd ..; rm -rf work; ln -s '{above}' work"))
            .await;
        let got = read(&frozen, Path::new("id_ed25519"), MAX);
        assert!(matches!(got, Err(Refused::Missing)), "{got:?}");
    }

    /// A4. On a thread, so that a hang fails this test instead of the whole run.
    #[tokio::test]
    async fn a_fifo_does_not_hang_the_reader() {
        let room = Room::new();
        room.after("mkfifo out.txt").await;
        let cell = room.cell.clone();
        let bait = room.bait.clone();
        let (sent, got) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let frozen = watch::freeze(&cell, &bait).unwrap();
            let _ = sent.send(read(&frozen, Path::new("out.txt"), MAX).map(|o| o.bytes));
        });
        let got = got.recv_timeout(Duration::from_secs(5)).expect("it hung");
        assert!(matches!(got, Err(Refused::NotAFile)), "{got:?}");
    }

    #[tokio::test]
    async fn a_socket_or_a_folder_is_not_a_file() {
        let room = Room::new();
        let frozen = room.after("mkdir folder").await;
        let _socket = UnixListener::bind(room.cell.work().join("s")).unwrap();
        for name in ["s", "folder"] {
            let got = read(&frozen, Path::new(name), MAX);
            assert!(matches!(got, Err(Refused::NotAFile)), "{name}: {got:?}");
        }
    }

    #[tokio::test]
    async fn a_file_with_a_second_name_is_refused() {
        let room = Room::new();
        let frozen = room.after("echo x > one; ln one two").await;
        let got = read(&frozen, Path::new("one"), MAX);
        assert!(matches!(got, Err(Refused::HardLinked)), "{got:?}");
    }

    #[tokio::test]
    async fn a_file_past_the_ceiling_is_refused() {
        let room = Room::new();
        let frozen = room.after("printf 12345 > out").await;
        assert_eq!(read(&frozen, Path::new("out"), 5).unwrap().bytes, b"12345");
        let got = read(&frozen, Path::new("out"), 4);
        assert!(matches!(got, Err(Refused::TooLarge { max: 4 })), "{got:?}");
    }

    #[tokio::test]
    async fn a_path_out_of_the_room_is_refused() {
        let room = Room::new();
        let secret = room.secret();
        let frozen = room.after("true").await;
        for path in [
            secret.as_path(),
            Path::new("../../id_ed25519"),
            Path::new("."),
            Path::new(""),
        ] {
            let got = read(&frozen, path, MAX);
            assert!(matches!(got, Err(Refused::NotInside)), "{path:?}: {got:?}");
        }
    }

    /// What a daemon was doing stops before anything is read.
    #[tokio::test]
    async fn nothing_is_running_once_a_room_is_frozen() {
        let room = Room::new();
        let ran = room
            .cell
            .run(
                &super::super::selftest::daemon(
                    r#"while (1) { open(F, ">", "out"); print F "changing"; close(F); select(undef, undef, undef, 0.01); }"#,
                ),
                Limits::default(),
            )
            .await
            .unwrap();
        assert!(ran.ok(), "{ran:?}");
        // The control: it really is out there, writing.
        assert_eq!(watch::room_processes(&room.cell, &room.bait).unwrap().len(), 1);
        for _ in 0..500 {
            if room.cell.work().join("out").exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(room.cell.work().join("out").exists());

        let frozen = watch::freeze(&room.cell, &room.bait).unwrap();

        assert_eq!(
            watch::room_processes(frozen.cell(), &room.bait).unwrap(),
            Vec::<libc::pid_t>::new()
        );
        let before = std::fs::metadata(room.cell.work().join("out")).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let after = std::fs::metadata(room.cell.work().join("out")).unwrap().modified().unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn a_hostile_name_is_visible_on_the_card_and_harmless_on_disk() {
        let room = Room::new();
        let frozen = room.after("printf x > \"$(printf 'a\\342\\200\\256gpj.exe')\"").await;
        let out = read(&frozen, Path::new("a\u{202E}gpj.exe"), MAX).unwrap();
        assert_eq!(out.shown, "a\\u{202E}gpj.exe");
        assert_eq!(out.name, "a_gpj.exe");
    }

    /// A34, as far as a test can go on a machine somebody is using: Gatekeeper
    /// itself would put up a dialog. What it was measured against is this mark.
    #[tokio::test]
    async fn a_saved_file_keeps_its_bytes_and_nothing_else() {
        let room = Room::new();
        let frozen = room
            .after("printf '#!/bin/sh\\n' > run.command; chmod 755 run.command; xattr -w com.synabit.room hello run.command")
            .await;
        let inside = room.cell.work().join("run.command");
        // The control: the room really did set both of these.
        assert_eq!(xattr(&inside, "com.synabit.room").as_deref(), Some("hello"));
        assert_ne!(inside.metadata().unwrap().permissions().mode() & 0o111, 0);

        let folder = room.dir.path().join("Downloads");
        std::fs::create_dir(&folder).unwrap();
        let out = read(&frozen, Path::new("run.command"), MAX).unwrap();
        let saved = save(&out, &folder).unwrap();

        assert_eq!(std::fs::read(&saved).unwrap(), b"#!/bin/sh\n");
        assert_eq!(saved.metadata().unwrap().permissions().mode() & 0o111, 0);
        assert_eq!(xattr(&saved, "com.synabit.room"), None);
        let mark = xattr(&saved, "com.apple.quarantine").unwrap();
        assert!(mark.starts_with("0081;") && mark.ends_with(";Synabit;"), "{mark}");
    }

    #[tokio::test]
    async fn a_save_never_replaces_what_is_there_and_leaves_nothing_behind() {
        let room = Room::new();
        let frozen = room.after("printf new > out.txt").await;
        let folder = room.dir.path().join("Downloads");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("out.txt"), "old").unwrap();

        let out = read(&frozen, Path::new("out.txt"), MAX).unwrap();
        let got = save(&out, &folder);

        assert!(matches!(got, Err(Refused::AlreadyThere)), "{got:?}");
        assert_eq!(std::fs::read(folder.join("out.txt")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
    }
}
