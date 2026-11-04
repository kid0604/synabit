//! Rehearsing on a copy, and saying what it would change (design §9).
//!
//! Syn may tidy a folder the person opened, but never on the folder itself. The
//! folder is cloned into the room ([`rehearse`]) and the room does whatever it
//! does to the clone. Then the room is stopped and Rust compares the clone with
//! what it wrote down when it made it ([`propose`]): what moved, what was
//! renamed, what is gone, what is new. That is the proposal the person sees, in
//! Rust's words, and the only thing [`super::apply`] will ever do.
//!
//! # Telling what happened from structure, not from names
//!
//! A clone keeps none of the originals' inodes, but each file in it has one of
//! its own, written down beside the original's. A file the room moved keeps it,
//! so a move is a move and never "one new, one gone": renaming `A.txt` to
//! `a.txt` on a volume that ignores case cannot become a deletion that takes the
//! new file with it (A17). Two files with the same bytes are two files. A file
//! the room wrote anew is new, whatever it is called.
//!
//! What the room changed is copied out while it is stopped, before anyone is
//! shown anything, so nothing the room does afterwards reaches the folder (A16).
//!
//! # What is never proposed
//!
//! Names the backstop hides were never cloned, links and other special files
//! neither, so they are never "gone". A folder of code (`.git`) is refused for
//! now: every line in it runs later, and the branch it needs is a step of its own.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::io::{self, Read};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::cell::Cell;
use super::fd::{self, Kind, Stat};
use super::rooms::Rooms;
use super::sanitize;

/// How many entries a rehearsal takes; past that, rehearse a part of the folder.
pub const MOST_ENTRIES: usize = 200_000;
/// Bytes copied when the folder is on another volume and cannot be cloned.
pub const MOST_COPIED: u64 = 2 << 30;
/// Bytes a clone may hold at all: the room's disk ceiling counts them.
pub const MOST_CLONED: u64 = 16 << 30;
/// What may come out of the room as new content, in one proposal.
pub const MOST_STAGED: u64 = 4 << 30;

#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("that folder is a folder of code (it has .git), and rehearsing on code is not done yet")]
    Code,
    #[error("that folder has more than {MOST_ENTRIES} entries; rehearse a part of it with `folder`")]
    TooMany,
    #[error("that folder is too large to copy into the room; rehearse a part of it with `folder`")]
    TooLarge,
    #[error("`folder` is not a folder inside the door")]
    NotInside,
    #[error("there is no rehearsal '{0}' in this room")]
    Unknown(String),
    #[error("the room's copy is not there any more; rehearse again")]
    CopyGone,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// One thing in the folder when it was rehearsed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    /// Under the rehearsed folder, by `/`.
    pub path: String,
    pub stat: Stat,
    /// Hidden by the backstop: not cloned, never touched.
    #[serde(default)]
    pub hidden: bool,
    /// The clone's own, when there is one. None is "left as it is": a link, a
    /// hidden name, or a name the room's volume could not hold beside another.
    pub copy: Option<Stat>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub room: String,
    pub door: String,
    pub door_name: String,
    /// The folder rehearsed, resolved.
    pub root: String,
    pub root_stat: Stat,
    /// Its name in the room's folder.
    pub copy: String,
    pub ignores_case: bool,
    pub made: DateTime<Utc>,
    pub entries: Vec<Entry>,
    /// Names that are not text, which are left as they are.
    pub unnamed: usize,
}

fn place(rooms: &Rooms, id: &str) -> PathBuf {
    rooms.root().join("rehearsals").join(id)
}

fn usable_id(id: &str) -> bool {
    id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn load(rooms: &Rooms, id: &str) -> Result<Manifest, Refused> {
    if !usable_id(id) {
        return Err(Refused::Unknown(sanitize::shown(id)));
    }
    let text = std::fs::read_to_string(place(rooms, id).join("manifest.json")).map_err(|_| Refused::Unknown(id.to_string()))?;
    serde_json::from_str(&text).map_err(|e| Refused::Io(io::Error::other(e)))
}

/// The room's `work` folder, opened after the room is put back together.
fn work_of(cell: &Cell) -> io::Result<OwnedFd> {
    cell.restore()?;
    fd::open_path(&cell.work())
}

struct Walk {
    entries: Vec<Entry>,
    copied: u64,
    cloned: u64,
    unnamed: usize,
}

impl Walk {
    /// Writes down `at`, and makes its clone under `to`, one folder at a time.
    fn folder(&mut self, at: BorrowedFd, prefix: &str, to: Option<BorrowedFd>, depth: usize) -> Result<(), Refused> {
        if depth > 64 {
            return Err(Refused::TooMany);
        }
        for (name, stat) in fd::list(at)? {
            let Some(text) = fd::text(&name).map(str::to_string) else {
                self.unnamed += 1;
                continue;
            };
            if text.eq_ignore_ascii_case(".git") {
                return Err(Refused::Code);
            }
            if self.entries.len() >= MOST_ENTRIES {
                return Err(Refused::TooMany);
            }
            let path = if prefix.is_empty() { text.clone() } else { format!("{prefix}/{text}") };
            let hidden = super::doors::hides(&text);
            let mut entry = Entry { path: path.clone(), stat, hidden, copy: None };
            if hidden || !matches!(stat.kind, Kind::File | Kind::Folder) {
                self.entries.push(entry);
                continue;
            }
            match stat.kind {
                Kind::Folder => {
                    let below = super::cell::open_folder_at(at, &name)?;
                    let copy = match to {
                        Some(to) if fd::make_dir(to, &name)? => Some(super::cell::open_folder_at(to, &name)?),
                        // Two names the room's volume holds as one: the second
                        // stays out of the clone, and as it is.
                        _ => None,
                    };
                    if let Some(copy) = &copy {
                        entry.copy = Some(fd::stat_fd(copy.as_fd())?);
                    }
                    self.entries.push(entry);
                    self.folder(below.as_fd(), &path, copy.as_ref().map(|c| c.as_fd()), depth + 1)?;
                }
                _ => {
                    if let Some(to) = to {
                        entry.copy = self.file(at, &name, to, &stat)?;
                    }
                    self.entries.push(entry);
                }
            }
        }
        Ok(())
    }

    fn file(&mut self, at: BorrowedFd, name: &CString, to: BorrowedFd, stat: &Stat) -> Result<Option<Stat>, Refused> {
        let Ok(source) = fd::open_file(at, name) else { return Ok(None) };
        self.cloned += stat.size;
        if self.cloned > MOST_CLONED {
            return Err(Refused::TooLarge);
        }
        let made = match fd::clone_into(source.as_fd(), to, name) {
            Ok(Some(())) => true,
            Ok(None) => false,
            Err(e) if e.raw_os_error() == Some(libc::EEXIST) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if !made {
            self.copied += stat.size;
            if self.copied > MOST_COPIED {
                return Err(Refused::TooLarge);
            }
            match fd::create(to, name, source.take(stat.size)) {
                Ok(_) => {}
                Err(e) if e.raw_os_error() == Some(libc::EEXIST) => return Ok(None),
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Some(fd::stat_at(to, name)?))
    }
}

/// What a rehearsal is, for the model.
#[derive(Clone, Debug, Serialize)]
pub struct Rehearsed {
    pub rehearsal: String,
    /// Under the room's folder.
    pub at: String,
    pub files: usize,
    pub folders: usize,
    pub left_as_they_are: usize,
}

/// Clones `within` of a door into the room, and writes down what it was.
pub fn rehearse(rooms: &Rooms, cell: &Cell, room: &str, door: &super::doors::Door, door_path: &Path, within: Option<&str>) -> Result<Rehearsed, Refused> {
    let door_fd = fd::open_path(door_path)?;
    let within = within.map(str::trim).filter(|w| !w.is_empty() && *w != ".").unwrap_or("");
    let root_fd = fd::open_dirs(door_fd.as_fd(), within, false).map_err(|_| Refused::NotInside)?;
    // A door that is a repository, rehearsed in a part of it, is still code.
    let mut above = fd::open_dirs(door_fd.as_fd(), "", false)?;
    for name in fd::names(within).map_err(|_| Refused::NotInside)? {
        if fd::stat_at(above.as_fd(), c".git").is_ok() {
            return Err(Refused::Code);
        }
        above = super::cell::open_folder_at(above.as_fd(), &name)?;
    }
    let root = door_path.join(within);

    // Nothing in the room runs while its folder is written into.
    let _frozen = rooms.freeze(cell)?;
    let work = work_of(cell)?;
    let base = root.file_name().and_then(sanitize::file_name).unwrap_or_else(|| door.name.clone());
    let copy = (1..)
        .map(|i| if i == 1 { base.clone() } else { format!("{base} {i}") })
        .find(|n| CString::new(n.as_str()).is_ok_and(|c| fd::stat_at(work.as_fd(), &c).is_err()))
        .expect("a free name");
    let copy_c = CString::new(copy.as_str()).map_err(|_| Refused::NotInside)?;
    fd::make_dir(work.as_fd(), &copy_c)?;
    let copy_fd = super::cell::open_folder_at(work.as_fd(), &copy_c)?;

    let mut walk = Walk { entries: Vec::new(), copied: 0, cloned: 0, unnamed: 0 };
    let walked = walk.folder(root_fd.as_fd(), "", Some(copy_fd.as_fd()), 0);
    if let Err(refused) = walked {
        let _ = std::fs::remove_dir_all(cell.work().join(&copy));
        return Err(refused);
    }

    let id = hex::encode(&Sha256::digest(format!("{room}\0{}\0{}", root.display(), Utc::now().timestamp_nanos_opt().unwrap_or_default()))[..8]);
    let manifest = Manifest {
        id: id.clone(),
        room: room.to_string(),
        door: door.id.clone(),
        door_name: door.name.clone(),
        root: root.display().to_string(),
        root_stat: fd::stat_fd(root_fd.as_fd())?,
        copy: copy.clone(),
        ignores_case: fd::ignores_case(root_fd.as_fd()),
        made: Utc::now(),
        entries: walk.entries,
        unnamed: walk.unnamed,
    };
    let folder = place(rooms, &id);
    std::fs::create_dir_all(&folder)?;
    crate::path_utils::write_atomic(&folder.join("manifest.json"), serde_json::to_string(&manifest).map_err(io::Error::other)?.as_bytes())?;
    rooms.mark_outside(room)?;

    let count = |kind: Kind| manifest.entries.iter().filter(|e| e.copy.is_some() && e.stat.kind == kind).count();
    Ok(Rehearsed {
        rehearsal: id,
        at: copy,
        files: count(Kind::File),
        folders: count(Kind::Folder),
        left_as_they_are: manifest.entries.iter().filter(|e| e.copy.is_none()).count() + manifest.unnamed,
    })
}

// ── The proposal ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum What {
    /// Same folder, another name.
    Rename,
    /// Another folder.
    Move,
    /// To the Trash.
    Trash,
    /// A file the room made.
    New,
    /// The room changed what is in it: the old one goes to the Trash.
    Change,
    NewFolder,
    /// To the Trash, if nothing is left in it.
    RemoveFolder,
}

/// Whether a person can be shown it as plainly harmless.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// On the list of documents, and its content says so: chosen already.
    Data,
    /// Anything else: a script, a setting, no extension, a mismatch. Not chosen,
    /// and confirmed in the system's own dialog if chosen.
    Unknown,
    /// Cannot be applied as it is, and is only shown.
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    NotADocument,
    Executable,
    ContentDoesNotMatch,
    /// Another thing is, or will be, at that name.
    NameTaken,
    /// The file has more than one name in the room.
    SecondName,
    /// The room made a link or a special file.
    NotAFile,
    /// The original changed after the rehearsal.
    ChangedSince,
    TooLarge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Op {
    pub n: usize,
    pub what: What,
    pub from: Option<String>,
    pub to: Option<String>,
    pub size: u64,
    pub class: Class,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Why>,
    /// The original, as it must still be when this is applied.
    pub original: Option<Stat>,
    /// New content, copied out of the room: its hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Waiting,
    Applied,
    Declined,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub room: String,
    pub door_name: String,
    pub root: String,
    pub root_stat: Stat,
    pub vault: String,
    pub conversation: Option<String>,
    pub made: DateTime<Utc>,
    pub state: State,
    pub ops: Vec<Op>,
    /// Entries nothing happened to.
    pub unchanged: usize,
    /// What the room read before it made these: true for every door.
    pub read_outside: bool,
}

pub fn staged_file(rooms: &Rooms, id: &str, n: usize) -> PathBuf {
    place(rooms, id).join("staged").join(n.to_string())
}

pub fn load_proposal(rooms: &Rooms, id: &str) -> Option<Proposal> {
    if !usable_id(id) {
        return None;
    }
    serde_json::from_str(&std::fs::read_to_string(place(rooms, id).join("proposal.json")).ok()?).ok()
}

pub fn save_proposal(rooms: &Rooms, proposal: &Proposal) -> io::Result<()> {
    crate::path_utils::write_atomic(
        &place(rooms, &proposal.id).join("proposal.json"),
        serde_json::to_string(proposal).map_err(io::Error::other)?.as_bytes(),
    )
}

/// Every proposal still waiting for this vault.
pub fn waiting(rooms: &Rooms, vault: &str) -> Vec<Proposal> {
    let Ok(entries) = std::fs::read_dir(rooms.root().join("rehearsals")) else { return Vec::new() };
    let mut out: Vec<Proposal> = entries
        .flatten()
        .filter_map(|e| load_proposal(rooms, e.file_name().to_str()?))
        .filter(|p| p.state == State::Waiting && p.vault == vault)
        .collect();
    out.sort_by_key(|p| p.made);
    out
}

/// What the clone holds now, by the clone's own inode.
struct After {
    files: HashMap<u64, Vec<(String, Stat)>>,
    folders: HashSet<String>,
    odd: Vec<(String, Stat)>,
}

fn look(at: BorrowedFd, prefix: &str, after: &mut After, depth: usize) -> io::Result<()> {
    if depth > 64 {
        return Ok(());
    }
    for (name, stat) in fd::list(at)? {
        let Some(text) = fd::text(&name) else { continue };
        let path = if prefix.is_empty() { text.to_string() } else { format!("{prefix}/{text}") };
        match stat.kind {
            Kind::File => after.files.entry(stat.ino).or_default().push((path, stat)),
            Kind::Folder => {
                after.folders.insert(path.clone());
                let below = super::cell::open_folder_at(at, &name)?;
                look(below.as_fd(), &path, after, depth + 1)?;
            }
            _ => after.odd.push((path, stat)),
        }
    }
    Ok(())
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

fn leaf(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, l)| l)
}

/// A file's first bytes, by descriptor.
fn head_at(root: BorrowedFd, path: &str) -> Option<Vec<u8>> {
    let folder = fd::open_dirs(root, parent(path), false).ok()?;
    let file = fd::open_file(folder.as_fd(), &CString::new(leaf(path)).ok()?).ok()?;
    let mut head = Vec::new();
    file.take(4096).read_to_end(&mut head).ok()?;
    Some(head)
}

fn hash_at(root: BorrowedFd, path: &str) -> Option<String> {
    let folder = fd::open_dirs(root, parent(path), false).ok()?;
    let mut file = fd::open_file(folder.as_fd(), &CString::new(leaf(path)).ok()?).ok()?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).ok()?;
    Some(hex::encode(hasher.finalize()))
}

/// Copies a file out of the stopped room, by descriptor, and its hash.
fn stage(rooms: &Rooms, id: &str, n: usize, copy: BorrowedFd, path: &str) -> io::Result<(u64, String)> {
    let folder = fd::open_dirs(copy, parent(path), false)?;
    let name = CString::new(leaf(path)).map_err(io::Error::other)?;
    let file = fd::open_file(folder.as_fd(), &name)?;
    if fd::stat_fd(file.as_fd())?.nlink != 1 {
        return Err(io::Error::other("a second name"));
    }
    let to = staged_file(rooms, id, n);
    std::fs::create_dir_all(to.parent().expect("a folder"))?;
    let staged = fd::open_path(to.parent().expect("a folder"))?;
    let name = CString::new(n.to_string()).map_err(io::Error::other)?;
    let _ = std::fs::remove_file(&to);
    if fd::clone_into(file.as_fd(), staged.as_fd(), &name)?.is_none() {
        fd::create(staged.as_fd(), &name, &file)?;
    }
    let mut hasher = Sha256::new();
    let size = io::copy(&mut std::fs::File::open(&to)?, &mut hasher)?;
    Ok((size, hex::encode(hasher.finalize())))
}

/// Stops the room, compares its copy with the folder as it was, and keeps the
/// proposal and whatever new content it needs, outside the room.
pub fn propose(rooms: &Rooms, cell: &Cell, id: &str, vault: &str, conversation: Option<&str>) -> Result<Proposal, Refused> {
    let manifest = load(rooms, id)?;
    if manifest.room != cell.room().file_name().and_then(|n| n.to_str()).unwrap_or_default() {
        return Err(Refused::Unknown(id.to_string()));
    }
    let _frozen = rooms.freeze(cell)?;
    let work = work_of(cell)?;
    let copy = CString::new(manifest.copy.as_str()).map_err(|_| Refused::CopyGone)?;
    let copy = super::cell::open_folder_at(work.as_fd(), &copy).map_err(|_| Refused::CopyGone)?;
    let mut after = After { files: HashMap::new(), folders: HashSet::new(), odd: Vec::new() };
    look(copy.as_fd(), "", &mut after, 0)?;
    let root = fd::open_path(Path::new(&manifest.root)).ok();

    let mut ops: Vec<Op> = Vec::new();
    let mut unchanged = 0;
    let mut known: HashSet<u64> = HashSet::new();
    let mut staged_bytes = 0u64;
    let op = |what, from: Option<&str>, to: Option<&str>, size, original| Op {
        n: 0,
        what,
        from: from.map(str::to_string),
        to: to.map(str::to_string),
        size,
        class: Class::Data,
        why: None,
        original,
        sha256: None,
    };

    for entry in &manifest.entries {
        let Some(copied) = entry.copy else { continue };
        known.insert(copied.ino);
        match entry.stat.kind {
            Kind::Folder => {
                if after.folders.contains(&entry.path) {
                    unchanged += 1;
                } else {
                    ops.push(op(What::RemoveFolder, Some(&entry.path), None, 0, Some(entry.stat)));
                }
            }
            Kind::File => match after.files.get(&copied.ino).map(Vec::as_slice) {
                None | Some([]) => ops.push(op(What::Trash, Some(&entry.path), None, entry.stat.size, Some(entry.stat))),
                Some([(path, now)]) => {
                    let moved = *path != entry.path;
                    let touched = now.ctime_ns != copied.ctime_ns || now.size != copied.size;
                    let changed = touched && match &root {
                        Some(root) => hash_at(copy.as_fd(), path) != hash_at(root.as_fd(), &entry.path),
                        None => true,
                    };
                    if changed {
                        let mut change = op(What::Change, Some(&entry.path), Some(path), now.size, Some(entry.stat));
                        if now.nlink != 1 {
                            change.class = Class::Blocked;
                            change.why = Some(Why::SecondName);
                        }
                        ops.push(change);
                    } else if moved {
                        let what = if parent(path) == parent(&entry.path) { What::Rename } else { What::Move };
                        ops.push(op(what, Some(&entry.path), Some(path), entry.stat.size, Some(entry.stat)));
                    } else {
                        unchanged += 1;
                    }
                }
                Some(many) => {
                    for (path, _) in many {
                        let mut blocked = op(What::Move, Some(&entry.path), Some(path), entry.stat.size, Some(entry.stat));
                        blocked.class = Class::Blocked;
                        blocked.why = Some(Why::SecondName);
                        ops.push(blocked);
                    }
                }
            },
            _ => {}
        }
    }
    let had: HashSet<&str> = manifest.entries.iter().filter(|e| e.stat.kind == Kind::Folder).map(|e| e.path.as_str()).collect();
    let mut new_folders: Vec<&String> = after.folders.iter().filter(|f| !had.contains(f.as_str())).collect();
    new_folders.sort();
    for folder in new_folders {
        ops.push(op(What::NewFolder, None, Some(folder), 0, None));
    }
    let mut new_files: Vec<&(String, Stat)> =
        after.files.iter().filter(|(ino, _)| !known.contains(ino)).flat_map(|(_, at)| at.iter()).collect();
    new_files.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, stat) in new_files {
        let mut new = op(What::New, None, Some(path), stat.size, None);
        if stat.nlink != 1 {
            new.class = Class::Blocked;
            new.why = Some(Why::SecondName);
        }
        ops.push(new);
    }
    for (path, _) in &after.odd {
        if !manifest.entries.iter().any(|e| e.path == *path) {
            let mut odd = op(What::New, None, Some(path), 0, None);
            odd.class = Class::Blocked;
            odd.why = Some(Why::NotAFile);
            ops.push(odd);
        }
    }

    for (n, op) in ops.iter_mut().enumerate() {
        op.n = n;
    }
    // New content out of the stopped room, before anything is shown.
    for op in ops.iter_mut().filter(|o| matches!(o.what, What::New | What::Change) && o.class != Class::Blocked) {
        staged_bytes += op.size;
        if staged_bytes > MOST_STAGED {
            op.class = Class::Blocked;
            op.why = Some(Why::TooLarge);
            continue;
        }
        match stage(rooms, id, op.n, copy.as_fd(), op.to.as_deref().unwrap_or_default()) {
            Ok((size, hash)) => {
                op.size = size;
                op.sha256 = Some(hash);
            }
            Err(_) => {
                op.class = Class::Blocked;
                op.why = Some(Why::NotAFile);
            }
        }
    }
    classify(&manifest, &mut ops, root.as_ref().map(|r| r.as_fd()), |n| std::fs::read(staged_file(rooms, id, n)).ok().map(|mut b| {
        b.truncate(4096);
        b
    }));

    let proposal = Proposal {
        id: id.to_string(),
        room: manifest.room.clone(),
        door_name: manifest.door_name.clone(),
        root: manifest.root.clone(),
        root_stat: manifest.root_stat,
        vault: vault.to_string(),
        conversation: conversation.map(str::to_string),
        made: Utc::now(),
        state: State::Waiting,
        ops,
        unchanged,
        read_outside: rooms.carries_outside(&manifest.room),
    };
    save_proposal(rooms, &proposal)?;
    Ok(proposal)
}

// ── Classification by an allowlist (design §9.1) ───────────────────────

/// Documents, by extension, with what their first bytes must look like.
const DOCUMENTS: &[(&str, Sniff)] = &[
    ("pdf", Sniff::Starts(b"%PDF")),
    ("png", Sniff::Starts(b"\x89PNG")),
    ("jpg", Sniff::Starts(b"\xFF\xD8\xFF")),
    ("jpeg", Sniff::Starts(b"\xFF\xD8\xFF")),
    ("gif", Sniff::Starts(b"GIF8")),
    ("webp", Sniff::Riff(b"WEBP")),
    ("heic", Sniff::Box),
    ("heif", Sniff::Box),
    ("tif", Sniff::Tiff),
    ("tiff", Sniff::Tiff),
    ("bmp", Sniff::Starts(b"BM")),
    ("mp4", Sniff::Box),
    ("m4v", Sniff::Box),
    ("mov", Sniff::Box),
    ("m4a", Sniff::Box),
    ("mp3", Sniff::Mp3),
    ("wav", Sniff::Riff(b"WAVE")),
    ("flac", Sniff::Starts(b"fLaC")),
    ("txt", Sniff::Text),
    ("md", Sniff::Text),
    ("csv", Sniff::Text),
    ("tsv", Sniff::Text),
    ("rtf", Sniff::Starts(b"{\\rtf")),
    ("docx", Sniff::Zip),
    ("xlsx", Sniff::Zip),
    ("pptx", Sniff::Zip),
    ("odt", Sniff::Zip),
    ("ods", Sniff::Zip),
    ("epub", Sniff::Zip),
    ("zip", Sniff::Zip),
];

/// Folder names that are programs, however much they look like folders.
const BUNDLES: &[&str] = &["app", "command", "workflow", "action", "prefpane", "kext", "bundle", "framework", "plugin", "pkg", "mpkg"];

#[derive(Clone, Copy, Debug)]
enum Sniff {
    Starts(&'static [u8]),
    Riff(&'static [u8]),
    /// An ISO media file: `ftyp` at byte 4.
    Box,
    Tiff,
    Mp3,
    Zip,
    Text,
}

impl Sniff {
    fn holds(self, head: &[u8]) -> bool {
        match self {
            Sniff::Starts(magic) => head.starts_with(magic),
            Sniff::Riff(kind) => head.starts_with(b"RIFF") && head.get(8..12) == Some(kind),
            Sniff::Box => head.get(4..8) == Some(b"ftyp"),
            Sniff::Tiff => head.starts_with(b"II*\0") || head.starts_with(b"MM\0*"),
            Sniff::Mp3 => head.starts_with(b"ID3") || (head.len() > 1 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0),
            Sniff::Zip => head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06"),
            Sniff::Text => {
                !head.contains(&0)
                    && match std::str::from_utf8(head) {
                        Ok(_) => true,
                        // Cut in the middle of a character at the end of what was read.
                        Err(e) => e.error_len().is_none(),
                    }
            }
        }
    }
}

fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Why a file under `name` with `head` is not a document, if it is not one.
pub fn not_a_document(name: &str, head: Option<&[u8]>, mode: Option<u32>) -> Option<Why> {
    if mode.is_some_and(|m| m & 0o111 != 0) {
        return Some(Why::Executable);
    }
    let Some(ext) = extension(name) else { return Some(Why::NotADocument) };
    let Some((_, sniff)) = DOCUMENTS.iter().find(|(e, _)| *e == ext) else {
        return Some(Why::NotADocument);
    };
    match head {
        Some(head) if !sniff.holds(head) => Some(Why::ContentDoesNotMatch),
        _ => None,
    }
}

fn classify(manifest: &Manifest, ops: &mut [Op], root: Option<BorrowedFd>, staged: impl Fn(usize) -> Option<Vec<u8>>) {
    let key = |path: &str| path.split('/').map(|part| fd::key(part, manifest.ignores_case)).collect::<Vec<_>>().join("/");

    // What will be at each name, if everything here is applied: what is left
    // alone, and every op's end. Two at one name block each other.
    let mut leaving: HashSet<String> = HashSet::new();
    for op in ops.iter().filter(|o| o.class != Class::Blocked) {
        if let (Some(from), true) = (&op.from, matches!(op.what, What::Rename | What::Move | What::Trash | What::Change | What::RemoveFolder)) {
            leaving.insert(key(from));
        }
    }
    let mut taken: HashMap<String, usize> = HashMap::new();
    for entry in &manifest.entries {
        let k = key(&entry.path);
        if !leaving.contains(&k) && entry.stat.kind != Kind::Folder {
            *taken.entry(k).or_default() += 1;
        }
    }
    for op in ops.iter().filter(|o| matches!(o.what, What::Rename | What::Move | What::New | What::Change) && o.class != Class::Blocked) {
        if let Some(to) = &op.to {
            *taken.entry(key(to)).or_default() += 1;
        }
    }

    for op in ops.iter_mut().filter(|o| o.class != Class::Blocked) {
        if let Some(to) = op.to.as_deref().filter(|_| op.what != What::NewFolder) {
            if taken.get(&key(to)).copied().unwrap_or(0) > 1 {
                op.class = Class::Blocked;
                op.why = Some(Why::NameTaken);
                continue;
            }
        }
        let why = match op.what {
            What::NewFolder | What::RemoveFolder => {
                let name = leaf(op.to.as_deref().or(op.from.as_deref()).unwrap_or_default());
                let bundle = extension(name).is_some_and(|e| BUNDLES.contains(&e.as_str()));
                (bundle || super::doors::hides(name)).then_some(Why::NotADocument)
            }
            What::Trash => not_a_document(leaf(op.from.as_deref().unwrap_or_default()), None, op.original.map(|o| o.mode)),
            What::Rename | What::Move => {
                let from = op.from.as_deref().unwrap_or_default();
                let to = leaf(op.to.as_deref().unwrap_or_default());
                let head = root.and_then(|r| head_at(r, from));
                not_a_document(to, head.as_deref(), op.original.map(|o| o.mode))
            }
            What::New | What::Change => {
                let to = leaf(op.to.as_deref().unwrap_or_default());
                not_a_document(to, staged(op.n).as_deref(), None)
            }
        };
        if let Some(why) = why {
            op.class = Class::Unknown;
            op.why = Some(why);
        }
    }
}

/// Removes a rehearsal's copy and everything kept for it.
pub fn forget(rooms: &Rooms, cell: &Cell, id: &str) {
    if let Ok(manifest) = load(rooms, id) {
        let _ = std::fs::remove_dir_all(cell.work().join(&manifest.copy));
    }
    if usable_id(id) {
        let _ = std::fs::remove_dir_all(place(rooms, id));
    }
}
