//! What the person approved, done to their folder by Rust (design §9, §9.2).
//!
//! Nothing in a room takes part: the room is stopped, and every change is a
//! rename or a new file made here, by descriptor, from the folder opened when
//! this began. Each file is checked again just before it is touched, and one
//! that changed since the rehearsal is left alone and said so (A18). Nothing is
//! ever written over: a name already taken fails the step, which puts the file
//! back where it was.
//!
//! # In order, and written down first
//!
//! Folders are made, new content is written beside the folder in a place of its
//! own, the files that move are taken out into that place and put where they
//! go, and only last does anything go to the Trash. Every step is written to the
//! journal before the next one, so an app that dies in the middle leaves a
//! journal that says exactly where everything is, and [`undo`] puts it back from
//! there, as it does for a batch that finished — within [`UNDO_DAYS`].

use std::ffi::{CStr, CString};
use std::io::{self, BufRead, Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::fd::{self, Stat};
use super::rehearse::{Class, Op, Proposal, What};
use super::sanitize;

/// How long a batch can be undone. *Provisional* (design §9.2).
pub const UNDO_DAYS: i64 = 30;

/// Where things go when they are thrown away.
#[derive(Clone, Debug)]
pub enum Trash {
    /// The person's: `~/.Trash` on the home volume, `.Trashes/<uid>` on another.
    System { home: PathBuf },
    /// One folder, for tests, on the volume of what they throw away.
    At(PathBuf),
}

impl Trash {
    fn folder_for(&self, root: BorrowedFd) -> io::Result<PathBuf> {
        match self {
            Trash::At(folder) => Ok(folder.clone()),
            Trash::System { home } => {
                let on = fd::stat_fd(root)?.dev;
                let home_trash = home.join(".Trash");
                if std::fs::metadata(home).map(|m| std::os::unix::fs::MetadataExt::dev(&m)).ok() == Some(on) {
                    std::fs::create_dir_all(&home_trash)?;
                    return Ok(home_trash);
                }
                // SAFETY: `info` is written by the call before it is read.
                let mount = unsafe {
                    let mut info: libc::statfs = std::mem::zeroed();
                    if libc::fstatfs(root.as_raw_fd(), &mut info) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    PathBuf::from(<std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(CStr::from_ptr(info.f_mntonname.as_ptr()).to_bytes()))
                };
                let trashes = mount.join(".Trashes");
                if !trashes.exists() {
                    std::fs::create_dir(&trashes)?;
                    std::fs::set_permissions(&trashes, std::os::unix::fs::PermissionsExt::from_mode(0o1333))?;
                }
                // SAFETY: a plain query.
                let mine = trashes.join(unsafe { libc::getuid() }.to_string());
                if !mine.exists() {
                    std::fs::create_dir(&mine)?;
                    std::fs::set_permissions(&mine, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
                }
                Ok(mine)
            }
        }
    }

    /// Moves `name` out of `at` into the Trash under a free name like it.
    fn put(&self, root: BorrowedFd, at: BorrowedFd, name: &CStr, shown: &str) -> io::Result<PathBuf> {
        let folder = self.folder_for(root)?;
        let shown = sanitize::file_name(std::ffi::OsStr::new(shown)).unwrap_or_else(|| "item".into());
        let (stem, ext) = match shown.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
            _ => (shown.clone(), String::new()),
        };
        for i in 1..200 {
            let candidate = if i == 1 { folder.join(&shown) } else { folder.join(format!("{stem} {i}{ext}")) };
            let to = CString::new(candidate.as_os_str().as_encoded_bytes()).map_err(io::Error::other)?;
            // SAFETY: valid names; the Trash's path is this app's to name, the
            // other end is a descriptor.
            let moved = unsafe { libc::renameatx_np(at.as_raw_fd(), name.as_ptr(), libc::AT_FDCWD, to.as_ptr(), libc::RENAME_EXCL) };
            if moved == 0 {
                return Ok(candidate);
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EEXIST) {
                return Err(error);
            }
        }
        Err(io::Error::other("no free name in the Trash"))
    }
}

/// One line of the journal. Paths are under the folder applied to.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Line {
    Begin { id: String, door_name: String, root: String, root_stat: Stat, staging: String, ops: Vec<Op>, at: DateTime<Utc> },
    /// A folder that was not there.
    Made { path: String },
    /// New content, written in the staging folder.
    Staged { n: usize, slot: String },
    /// A file taken out of its place into the staging folder.
    Out { n: usize, from: String, slot: String, ino: u64 },
    /// Put where it goes.
    In { n: usize, to: String, slot: String, ino: u64 },
    /// Could not be put, and was put back where it was.
    Back { n: usize, slot: String },
    Trashed { n: usize, from: String, at: String, ino: u64 },
    Skipped { n: usize, why: String },
    Finished { at: DateTime<Utc> },
    Undo { at: DateTime<Utc> },
    UndoFinished { at: DateTime<Utc>, left: Vec<String> },
}

struct Journal {
    file: std::fs::File,
}

impl Journal {
    fn create(path: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(path.parent().expect("a folder"))?;
        Ok(Self { file: std::fs::OpenOptions::new().create_new(true).append(true).open(path)? })
    }

    fn reopen(path: &Path) -> io::Result<Self> {
        Ok(Self { file: std::fs::OpenOptions::new().append(true).open(path)? })
    }

    /// Down on disk before the step it says was done is followed by another.
    fn say(&mut self, line: &Line) -> io::Result<()> {
        let mut text = serde_json::to_string(line).map_err(io::Error::other)?;
        text.push('\n');
        self.file.write_all(text.as_bytes())?;
        self.file.sync_data()
    }
}

pub fn journal_file(machine: &Path, id: &str) -> PathBuf {
    machine.join("journal").join(format!("{id}.jsonl"))
}

pub fn read_journal(path: &Path) -> io::Result<Vec<Line>> {
    let file = std::fs::File::open(path)?;
    // A line cut short by a crash is the one step that did not finish writing.
    Ok(io::BufReader::new(file).lines().map_while(Result::ok).filter_map(|l| serde_json::from_str(&l).ok()).collect())
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

fn leaf(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, l)| l)
}

fn c(text: &str) -> io::Result<CString> {
    CString::new(text).map_err(io::Error::other)
}

/// What came of applying.
#[derive(Clone, Debug, Serialize)]
pub struct Outcome {
    pub journal: String,
    pub done: Vec<usize>,
    pub skipped: Vec<(usize, String)>,
}

/// Copies staged content into the staging folder, checking it is what was shown.
fn put_staged(from: &Path, staging: BorrowedFd, slot: &CStr, sha256: &str) -> io::Result<()> {
    struct Hashing<R> {
        inner: R,
        hasher: Sha256,
    }
    impl<R: Read> Read for Hashing<R> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.hasher.update(&buf[..n]);
            Ok(n)
        }
    }
    let mut reading = Hashing { inner: std::fs::File::open(from)?, hasher: Sha256::new() };
    fd::create(staging, slot, &mut reading)?;
    let file = fd::open_file(staging, slot)?;
    super::export::quarantine(file.as_fd())?;
    if hex::encode(reading.hasher.finalize()) != sha256 {
        // SAFETY: a name this function just made, in a folder of its own.
        unsafe { libc::unlinkat(staging.as_raw_fd(), slot.as_ptr(), 0) };
        return Err(io::Error::other("the content is not what was shown"));
    }
    Ok(())
}

/// Makes the folders on the way to `path`, writing down each one made.
fn make_folders(root: BorrowedFd, path: &str, journal: &mut Journal) -> io::Result<OwnedFd> {
    let mut at = fd::open_dirs(root, "", false)?;
    let mut so_far = String::new();
    for name in path.split('/').filter(|p| !p.is_empty()) {
        so_far = if so_far.is_empty() { name.to_string() } else { format!("{so_far}/{name}") };
        let cname = c(name)?;
        if fd::make_dir(at.as_fd(), &cname)? {
            journal.say(&Line::Made { path: so_far.clone() })?;
        }
        at = super::cell::open_folder_at(at.as_fd(), &cname)?;
    }
    Ok(at)
}

/// Dies here in a test that measures what a crash leaves.
#[cfg(test)]
pub static DIE_AFTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);

fn step_done(count: &mut usize) {
    *count += 1;
    #[cfg(test)]
    if *count >= DIE_AFTER.load(std::sync::atomic::Ordering::SeqCst) {
        // SAFETY: ends this process at once, as a crash would.
        unsafe { libc::_exit(9) };
    }
}

/// Applies the ops numbered `approved`, of those that can be.
pub fn apply(machine: &Path, proposal: &Proposal, staged: impl Fn(usize) -> PathBuf, approved: &[usize], trash: &Trash) -> io::Result<Outcome> {
    let ops: Vec<Op> = proposal.ops.iter().filter(|o| approved.contains(&o.n) && o.class != Class::Blocked).cloned().collect();
    let root = fd::open_path(Path::new(&proposal.root))?;
    let now = fd::stat_fd(root.as_fd())?;
    if now.dev != proposal.root_stat.dev || now.ino != proposal.root_stat.ino {
        return Err(io::Error::other("the folder is not the one that was rehearsed"));
    }
    let staging = format!(".synabit-applying-{}", proposal.id);
    let mut journal = Journal::create(&journal_file(machine, &proposal.id))?;
    journal.say(&Line::Begin {
        id: proposal.id.clone(),
        door_name: proposal.door_name.clone(),
        root: proposal.root.clone(),
        root_stat: proposal.root_stat,
        staging: staging.clone(),
        ops: ops.clone(),
        at: Utc::now(),
    })?;
    let staging_c = c(&staging)?;
    if !fd::make_dir(root.as_fd(), &staging_c)? {
        return Err(io::Error::other("a folder from an earlier apply is still there"));
    }
    let hold = super::cell::open_folder_at(root.as_fd(), &staging_c)?;
    let hold = hold.as_fd();
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    let mut steps = 0usize;
    let skip = |journal: &mut Journal, n: usize, why: &str, skipped: &mut Vec<(usize, String)>| -> io::Result<()> {
        journal.say(&Line::Skipped { n, why: why.to_string() })?;
        skipped.push((n, why.to_string()));
        Ok(())
    };

    // Folders first, made where they go.
    for op in &ops {
        let wanted = match op.what {
            What::NewFolder => op.to.clone(),
            What::Rename | What::Move | What::New | What::Change => op.to.as_deref().map(|t| parent(t).to_string()),
            _ => None,
        };
        if let Some(path) = wanted.filter(|p| !p.is_empty()) {
            if let Err(e) = make_folders(root.as_fd(), &path, &mut journal) {
                skip(&mut journal, op.n, &format!("its folder could not be made: {e}"), &mut skipped)?;
            } else if op.what == What::NewFolder {
                done.push(op.n);
            }
            step_done(&mut steps);
        }
    }
    // New content, beside the folder.
    let mut ready = std::collections::HashSet::new();
    for op in ops.iter().filter(|o| matches!(o.what, What::New | What::Change)) {
        let slot = format!("n{}", op.n);
        match put_staged(&staged(op.n), hold, &c(&slot)?, op.sha256.as_deref().unwrap_or_default()) {
            Ok(()) => {
                journal.say(&Line::Staged { n: op.n, slot })?;
                ready.insert(op.n);
            }
            Err(e) => skip(&mut journal, op.n, &format!("its content could not be written: {e}"), &mut skipped)?,
        }
        step_done(&mut steps);
    }
    // What leaves its place, checked as it was rehearsed.
    let mut out = std::collections::HashSet::new();
    for op in ops.iter().filter(|o| matches!(o.what, What::Rename | What::Move | What::Trash | What::Change)) {
        if op.what == What::Change && !ready.contains(&op.n) {
            continue;
        }
        let from = op.from.as_deref().unwrap_or_default();
        let taken = (|| -> io::Result<u64> {
            let folder = fd::open_dirs(root.as_fd(), parent(from), false)?;
            let name = c(leaf(from))?;
            let stat = fd::stat_at(folder.as_fd(), &name)?;
            if !op.original.is_some_and(|o| o.same_as(&stat)) {
                return Err(io::Error::other("changed since"));
            }
            fd::rename_excl(folder.as_fd(), &name, hold, &c(&format!("o{}", op.n))?)?;
            Ok(stat.ino)
        })();
        match taken {
            Ok(ino) => {
                journal.say(&Line::Out { n: op.n, from: from.to_string(), slot: format!("o{}", op.n), ino })?;
                out.insert(op.n);
            }
            Err(e) => skip(&mut journal, op.n, &format!("it changed since the rehearsal, or is not there: {e}"), &mut skipped)?,
        }
        step_done(&mut steps);
    }
    // Put where they go.
    for op in ops.iter().filter(|o| matches!(o.what, What::Rename | What::Move | What::New | What::Change)) {
        let (slot, needs) = match op.what {
            What::New => (format!("n{}", op.n), ready.contains(&op.n)),
            What::Change => (format!("n{}", op.n), out.contains(&op.n)),
            _ => (format!("o{}", op.n), out.contains(&op.n)),
        };
        if !needs {
            continue;
        }
        let to = op.to.as_deref().unwrap_or_default();
        let slot_c = c(&slot)?;
        let ino = fd::stat_at(hold, &slot_c)?.ino;
        let put = fd::open_dirs(root.as_fd(), parent(to), false).and_then(|folder| fd::rename_excl(hold, &slot_c, folder.as_fd(), &c(leaf(to))?));
        match put {
            Ok(()) => {
                journal.say(&Line::In { n: op.n, to: to.to_string(), slot, ino })?;
                if op.what != What::Change {
                    done.push(op.n);
                }
            }
            Err(e) => {
                // Back where it was: a move or a change that cannot be made is not made.
                if matches!(op.what, What::Rename | What::Move | What::Change) {
                    let from = op.from.as_deref().unwrap_or_default();
                    let back = format!("o{}", op.n);
                    if fd::open_dirs(root.as_fd(), parent(from), false).and_then(|f| fd::rename_excl(hold, &c(&back)?, f.as_fd(), &c(leaf(from))?)).is_ok() {
                        journal.say(&Line::Back { n: op.n, slot: back })?;
                        out.remove(&op.n);
                    }
                }
                skip(&mut journal, op.n, &format!("something else has that name: {e}"), &mut skipped)?;
            }
        }
        step_done(&mut steps);
    }
    // The Trash, last.
    for op in ops.iter().filter(|o| matches!(o.what, What::Trash | What::Change) && out.contains(&o.n)) {
        let slot = format!("o{}", op.n);
        let from = op.from.as_deref().unwrap_or_default();
        let ino = fd::stat_at(hold, &c(&slot)?)?.ino;
        match trash.put(root.as_fd(), hold, &c(&slot)?, leaf(from)) {
            Ok(at) => {
                journal.say(&Line::Trashed { n: op.n, from: from.to_string(), at: at.display().to_string(), ino })?;
                done.push(op.n);
            }
            Err(e) => skip(&mut journal, op.n, &format!("it could not go to the Trash: {e}"), &mut skipped)?,
        }
        step_done(&mut steps);
    }
    // Folders emptied by the rest, deepest first; one with anything left stays.
    let mut folders: Vec<&Op> = ops.iter().filter(|o| o.what == What::RemoveFolder).collect();
    folders.sort_by_key(|o| std::cmp::Reverse(o.from.as_deref().map_or(0, |f| f.matches('/').count())));
    for op in folders {
        let from = op.from.as_deref().unwrap_or_default();
        let trashed = (|| -> io::Result<(PathBuf, u64)> {
            let above = fd::open_dirs(root.as_fd(), parent(from), false)?;
            let name = c(leaf(from))?;
            let stat = fd::stat_at(above.as_fd(), &name)?;
            if !op.original.is_some_and(|o| o.dev == stat.dev && o.ino == stat.ino) {
                return Err(io::Error::other("changed since"));
            }
            let inside = super::cell::open_folder_at(above.as_fd(), &name)?;
            if !fd::list(inside.as_fd())?.is_empty() {
                return Err(io::Error::other("it still holds something"));
            }
            Ok((trash.put(root.as_fd(), above.as_fd(), &name, leaf(from))?, stat.ino))
        })();
        match trashed {
            Ok((at, ino)) => {
                journal.say(&Line::Trashed { n: op.n, from: from.to_string(), at: at.display().to_string(), ino })?;
                done.push(op.n);
            }
            Err(e) => skip(&mut journal, op.n, &format!("the folder was left: {e}"), &mut skipped)?,
        }
        step_done(&mut steps);
    }
    // What was written and never placed is this app's own, and goes.
    for (name, _) in fd::list(hold)? {
        if name.to_bytes().starts_with(b"n") {
            // SAFETY: a name in a folder of this app's own making.
            unsafe { libc::unlinkat(hold.as_raw_fd(), name.as_ptr(), 0) };
        }
    }
    let _ = fd::remove_empty_dir(root.as_fd(), &staging_c);
    journal.say(&Line::Finished { at: Utc::now() })?;
    done.sort_unstable();
    Ok(Outcome { journal: proposal.id.clone(), done, skipped })
}

/// Where a batch stands, for the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    Finished,
    /// The app stopped before the batch finished: undo puts back what was done.
    Interrupted,
    Undone,
}

#[derive(Clone, Debug, Serialize)]
pub struct Batch {
    pub id: String,
    pub door_name: String,
    pub root: String,
    pub at: DateTime<Utc>,
    pub standing: Standing,
    pub moved: usize,
    pub trashed: usize,
    pub made: usize,
    pub can_undo: bool,
}

/// Every batch written down, newest first.
pub fn batches(machine: &Path) -> Vec<Batch> {
    let Ok(entries) = std::fs::read_dir(machine.join("journal")) else { return Vec::new() };
    let mut out: Vec<Batch> = entries
        .flatten()
        .filter_map(|e| {
            let lines = read_journal(&e.path()).ok()?;
            let Some(Line::Begin { id, door_name, root, at, ops, .. }) = lines.first().cloned() else { return None };
            let undone = lines.iter().any(|l| matches!(l, Line::UndoFinished { .. }));
            let finished = lines.iter().any(|l| matches!(l, Line::Finished { .. }));
            let standing = if undone { Standing::Undone } else if finished { Standing::Finished } else { Standing::Interrupted };
            let what = |n: usize| ops.iter().find(|o| o.n == n).map(|o| o.what);
            Some(Batch {
                id,
                door_name,
                root,
                at,
                standing,
                moved: lines.iter().filter(|l| matches!(l, Line::In { n, .. } if matches!(what(*n), Some(What::Rename | What::Move)))).count(),
                trashed: lines.iter().filter(|l| matches!(l, Line::Trashed { .. })).count(),
                made: lines.iter().filter(|l| matches!(l, Line::In { n, .. } if what(*n) == Some(What::New))).count(),
                can_undo: !undone && Utc::now() - at < Duration::days(UNDO_DAYS),
            })
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}

/// What undoing could not put back, each said in a line.
#[derive(Clone, Debug, Serialize)]
pub struct Undone {
    pub put_back: usize,
    pub left: Vec<String>,
}

/// Puts a batch back, finished or not: every step the journal says was done,
/// newest first, each checked to be the file the journal names.
pub fn undo(machine: &Path, id: &str, trash: &Trash) -> io::Result<Undone> {
    let path = journal_file(machine, id);
    let lines = read_journal(&path)?;
    let Some(Line::Begin { root: root_path, root_stat, staging, ops, at, .. }) = lines.first().cloned() else {
        return Err(io::Error::other("the journal has no beginning"));
    };
    if lines.iter().any(|l| matches!(l, Line::UndoFinished { .. })) {
        return Ok(Undone { put_back: 0, left: Vec::new() });
    }
    if Utc::now() - at >= Duration::days(UNDO_DAYS) {
        return Err(io::Error::other("that batch is too old to undo"));
    }
    let root = fd::open_path(Path::new(&root_path))?;
    let now = fd::stat_fd(root.as_fd())?;
    if now.dev != root_stat.dev || now.ino != root_stat.ino {
        return Err(io::Error::other("the folder is not the one that was changed"));
    }
    let mut journal = Journal::reopen(&path)?;
    journal.say(&Line::Undo { at: Utc::now() })?;
    let staging_c = c(&staging)?;
    let hold = super::cell::open_folder_at(root.as_fd(), &staging_c).ok();
    let what = |n: usize| ops.iter().find(|o| o.n == n).map(|o| o.what);
    let from_of = |n: usize| ops.iter().find(|o| o.n == n).and_then(|o| o.from.clone()).unwrap_or_default();
    let mut put_back = 0;
    let mut left = Vec::new();
    let mut handled_in = std::collections::HashSet::new();
    let mut placed_back = std::collections::HashSet::new();

    // Puts what is at `at` (a descriptor and a name) back at `from`, if it is `ino`.
    let restore = |at: BorrowedFd, name: &CStr, from: &str, ino: u64| -> io::Result<()> {
        if fd::stat_at(at, name)?.ino != ino {
            return Err(io::Error::other("not the same file"));
        }
        let folder = fd::open_dirs(root.as_fd(), parent(from), true)?;
        fd::rename_excl(at, name, folder.as_fd(), &c(leaf(from))?)
    };
    // The Trash by its path, which this app chose: a volume's `.Trashes` cannot
    // be listed, or opened, by its owner. The folder it goes back to is walked.
    let from_trash = |at: &Path, from: &str, ino: u64| -> io::Result<()> {
        if std::os::unix::fs::MetadataExt::ino(&std::fs::symlink_metadata(at)?) != ino {
            return Err(io::Error::other("not the same file"));
        }
        let folder = fd::open_dirs(root.as_fd(), parent(from), true)?;
        let source = CString::new(at.as_os_str().as_encoded_bytes()).map_err(io::Error::other)?;
        let name = c(leaf(from))?;
        // SAFETY: valid names; the far end is a descriptor.
        let moved = unsafe { libc::renameatx_np(libc::AT_FDCWD, source.as_ptr(), folder.as_raw_fd(), name.as_ptr(), libc::RENAME_EXCL) };
        if moved == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    };
    let in_line = |n: usize| {
        lines.iter().find_map(|l| match l {
            Line::In { n: m, to, ino, .. } if *m == n => Some((to.clone(), *ino)),
            _ => None,
        })
    };
    // A file put in its place: moved back, or, if it is new content, thrown away.
    let undo_in = |n: usize, to: &str, ino: u64| -> io::Result<()> {
        let folder = fd::open_dirs(root.as_fd(), parent(to), false)?;
        let name = c(leaf(to))?;
        match what(n) {
            Some(What::New | What::Change) => {
                if fd::stat_at(folder.as_fd(), &name)?.ino != ino {
                    return Err(io::Error::other("not the same file"));
                }
                trash.put(root.as_fd(), folder.as_fd(), &name, leaf(to)).map(|_| ())
            }
            _ => restore(folder.as_fd(), &name, &from_of(n), ino),
        }
    };

    for line in lines.iter().rev() {
        let result = match line {
            Line::Trashed { n, from, at, ino } => {
                // A change: its new content leaves the name before the old comes back.
                if what(*n) == Some(What::Change) {
                    if let Some((to, new_ino)) = in_line(*n) {
                        handled_in.insert(*n);
                        if let Err(e) = undo_in(*n, &to, new_ino) {
                            left.push(format!("{}: {e}", sanitize::shown(&to)));
                        }
                    }
                }
                from_trash(Path::new(at), from, *ino).map(|()| from.clone())
            }
            Line::In { n, to, ino, .. } if !handled_in.contains(n) => undo_in(*n, to, *ino).map(|()| to.clone()),
            Line::Out { n, from, slot, ino } => {
                let in_place = lines.iter().any(|l| matches!(l, Line::In { n: m, .. } | Line::Trashed { n: m, .. } | Line::Back { n: m, .. } if m == n));
                if in_place || placed_back.contains(n) {
                    continue;
                }
                placed_back.insert(*n);
                match &hold {
                    Some(hold) => restore(hold.as_fd(), &c(slot)?, from, *ino).map(|()| from.clone()),
                    None => Err(io::Error::other("the staging folder is gone")),
                }
            }
            Line::Made { path } => {
                let above = fd::open_dirs(root.as_fd(), parent(path), false);
                match above.and_then(|a| fd::remove_empty_dir(a.as_fd(), &c(leaf(path))?)) {
                    Ok(()) => Ok(path.clone()),
                    // A folder something else now uses is not this batch's to remove.
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        match result {
            Ok(_) => put_back += 1,
            Err(e) => left.push(format!("{}: {e}", sanitize::shown(&line_path(line)))),
        }
    }
    if let Some(hold) = &hold {
        for (name, _) in fd::list(hold.as_fd()).unwrap_or_default() {
            if name.to_bytes().starts_with(b"n") {
                // SAFETY: a name in a folder of this app's own making.
                unsafe { libc::unlinkat(hold.as_raw_fd(), name.as_ptr(), 0) };
            }
        }
        let _ = fd::remove_empty_dir(root.as_fd(), &staging_c);
    }
    journal.say(&Line::UndoFinished { at: Utc::now(), left: left.clone() })?;
    Ok(Undone { put_back, left })
}

fn line_path(line: &Line) -> String {
    match line {
        Line::Trashed { from, .. } | Line::Out { from, .. } => from.clone(),
        Line::In { to, .. } => to.clone(),
        Line::Made { path } => path.clone(),
        _ => String::new(),
    }
}
