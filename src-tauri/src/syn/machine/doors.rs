//! Folders the person opened to the rooms: a door each (design §8).
//!
//! A door is a folder on this Mac that a sealed room may read and never write.
//! The person chooses it in the system's folder dialog, which a page cannot fill
//! in and a model cannot name, so the path never comes from anything but that
//! dialog. Each door is an entry in `approvals.json`, lasts as long as an
//! `Always` does, and is usable only from the app unless the person says it may
//! be used away from the computer too (A23): a lost phone must not be a way to
//! read what the folder holds.
//!
//! # What a door may not be
//!
//! The rooms never see what decides what Syn may do (I4): the vault, with its
//! `.synabit/`, `SYN.md` and Syn's own folders, and the app's data. So a folder
//! that holds any of those, or is inside one, is refused. So is `/`, the home
//! folder and anything holding it, `~/Library` (other programs' data, sessions
//! and keys, nobody's documents), and any folder whose own path goes through a
//! hidden one. Compared by what the folders are on disk, not by how their paths
//! are spelled: case, Unicode form, a link or a firmlink do not make two folders
//! two (A9).
//!
//! # The backstop
//!
//! Inside a door, names that are usually keys are hidden by the kernel rules
//! ([`BACKSTOP`], and [`super::profile`] writes them out). It is a backstop and
//! never a boundary: a key can be in any file. What it hides is counted when the
//! door opens, so the person is told.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration as Elapsed, Instant};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::AppResult;
use crate::syn::approvals::{self, Section};
use crate::syn::consent::ALWAYS_LASTS_DAYS;

/// One name a door hides, compared without regard to case.
#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Is(&'static str),
    Starts(&'static str),
    Ends(&'static str),
}

/// The design's list (§8), with `id_*` narrowed to the key files ssh makes:
/// as written it would also hide `id_card.pdf`, the thing somebody asked Syn to
/// find.
pub const BACKSTOP: &[Shape] = &[
    Shape::Is(".ssh"),
    Shape::Is(".aws"),
    Shape::Is(".gnupg"),
    Shape::Starts(".env"),
    Shape::Ends(".pem"),
    Shape::Starts("id_rsa"),
    Shape::Starts("id_dsa"),
    Shape::Starts("id_ecdsa"),
    Shape::Starts("id_ed25519"),
    Shape::Is(".netrc"),
    Shape::Is(".npmrc"),
    Shape::Is(".git-credentials"),
];

/// Whether one name in a path is one the backstop hides.
pub fn hides(name: &str) -> bool {
    let name = name.to_lowercase();
    BACKSTOP.iter().any(|shape| match shape {
        Shape::Is(n) => name == *n,
        Shape::Starts(n) => name.starts_with(n),
        Shape::Ends(n) => name.ends_with(n) && name.len() > n.len(),
    })
}

/// The backstop as kernel regexes, one for each shape. Letters match either
/// case: the data volume ignores case, but a drive formatted otherwise does not.
pub fn backstop_regexes() -> Vec<String> {
    let spelled = |n: &str| -> String {
        n.chars()
            .map(|c| match c {
                c if c.is_ascii_alphabetic() => format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase()),
                '.' => "\\.".to_string(),
                c => c.to_string(),
            })
            .collect()
    };
    BACKSTOP
        .iter()
        .map(|shape| match shape {
            Shape::Is(n) => format!("/{}(/|$)", spelled(n)),
            Shape::Starts(n) => format!("/{}[^/]*(/|$)", spelled(n)),
            Shape::Ends(n) => format!("/[^/]+{}(/|$)", spelled(n)),
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Door {
    /// Sixteen hex digits, so it can be a file's line and an argument as it is.
    pub id: String,
    /// The folder's own name, made unique among the doors; what the model calls it.
    pub name: String,
    /// Resolved when it was opened.
    pub path: String,
    pub opened: DateTime<Utc>,
    /// May be used from Telegram and in routines, not only in the app.
    #[serde(default)]
    pub away: bool,
    /// How many names the backstop hid when it was opened.
    #[serde(default)]
    pub hidden: usize,
}

impl Door {
    pub fn expires(&self) -> DateTime<Utc> {
        self.opened + Duration::days(ALWAYS_LASTS_DAYS)
    }
}

/// Why a folder cannot be a door. The screen says it through i18n.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Refused {
    #[error("that is not a folder that can be read")]
    NotAFolder,
    #[error("that folder holds the whole home folder")]
    TooWide,
    #[error("that folder holds the vault, or is inside it")]
    Vault,
    #[error("that folder holds the app's own data, or is inside it")]
    AppData,
    #[error("that folder is inside ~/Library, where programs keep their data and keys")]
    Library,
    #[error("that folder is a hidden one, or inside one")]
    Hidden,
}

/// What a door may not overlap: everything already resolved, where it exists.
#[derive(Clone, Debug)]
pub struct Protected {
    pub home: PathBuf,
    pub vault: PathBuf,
    /// The app's data, config, cache and log folders, the rooms' among them.
    pub app: Vec<PathBuf>,
}

impl Protected {
    pub fn of<R: tauri::Runtime>(app: &tauri::AppHandle<R>, vault: &str) -> Option<Self> {
        use tauri::Manager;
        let path = app.path();
        let home = path.home_dir().ok()?;
        let app = [path.app_data_dir(), path.app_config_dir(), path.app_local_data_dir(), path.app_cache_dir(), path.app_log_dir()]
            .into_iter()
            .flatten()
            .collect();
        Some(Self { home, vault: PathBuf::from(vault), app })
    }
}

/// The folder on disk a path is, followed through every link.
fn same(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// Whether `outer` is `inner` or one of the folders above it, by what is on disk.
fn contains(outer: &Path, inner: &Path) -> bool {
    let (Ok(outer), Ok(inner)) = (std::fs::metadata(outer), inner.canonicalize()) else {
        return false;
    };
    inner.ancestors().any(|step| std::fs::metadata(step).is_ok_and(|m| same(&m, &outer)))
}

fn overlaps(a: &Path, b: &Path) -> bool {
    contains(a, b) || contains(b, a)
}

/// The folder a door would read, resolved, or why not.
pub fn check(picked: &Path, protected: &Protected) -> Result<PathBuf, Refused> {
    let real = picked.canonicalize().map_err(|_| Refused::NotAFolder)?;
    if !real.is_dir() || !super::profile::usable(&real) {
        return Err(Refused::NotAFolder);
    }
    let hidden = real.components().any(|c| match c {
        Component::Normal(name) => name.to_str().is_none_or(|n| n.starts_with('.') || hides(n)),
        _ => false,
    });
    if hidden {
        return Err(Refused::Hidden);
    }
    if contains(&real, &protected.home) {
        return Err(Refused::TooWide);
    }
    if overlaps(&real, &protected.vault) {
        return Err(Refused::Vault);
    }
    if protected.app.iter().any(|app| overlaps(&real, app)) {
        return Err(Refused::AppData);
    }
    if overlaps(&real, &protected.home.join("Library")) {
        return Err(Refused::Library);
    }
    Ok(real)
}

/// How far a count of hidden names looks, so a door on a huge folder still opens
/// at once.
const LOOKED_AT: usize = 50_000;
const EXAMPLES: usize = 5;

/// The names in a folder the backstop hides: how many, and a few of them, by
/// their place in it. Links are not followed; a hidden folder is not gone into.
pub fn hidden_in(folder: &Path) -> (usize, Vec<String>) {
    let mut count = 0;
    let mut examples = Vec::new();
    let walk = walkdir::WalkDir::new(folder).follow_links(false).max_depth(12).into_iter();
    let mut looked = 0;
    let mut walk = walk;
    while let Some(entry) = walk.next() {
        looked += 1;
        if looked > LOOKED_AT {
            break;
        }
        let Ok(entry) = entry else { continue };
        if entry.depth() == 0 {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if hides(&name) {
            count += 1;
            if examples.len() < EXAMPLES {
                let shown = entry.path().strip_prefix(folder).unwrap_or(entry.path()).display().to_string();
                examples.push(super::sanitize::shown(&shown));
            }
            if entry.file_type().is_dir() {
                walk.skip_current_dir();
            }
        }
    }
    (count, examples)
}

fn load(vault: &str) -> Vec<Door> {
    approvals::read(vault, Section::Doors)
}

/// Every door that has not expired.
pub fn all(vault: &str, now: DateTime<Utc>) -> Vec<Door> {
    load(vault).into_iter().filter(|d| d.expires() > now).collect()
}

/// A folder's name, made one no other door has.
fn unique_name(folder: &Path, taken: &[Door]) -> String {
    let base = folder
        .file_name()
        .and_then(super::sanitize::file_name)
        .unwrap_or_else(|| "folder".to_string());
    let free = |n: &str| !taken.iter().any(|d| d.name.eq_ignore_ascii_case(n));
    if free(&base) {
        return base;
    }
    (2..).map(|i| format!("{base} {i}")).find(|n| free(n)).expect("a free name")
}

/// What opening a door found.
#[derive(Clone, Debug, Serialize)]
pub struct Opened {
    pub door: Door,
    /// A few of the names the backstop hides, for the person to be told.
    pub examples: Vec<String>,
}

/// Opens a folder the person chose, or opens it again: the same folder keeps its
/// name and its id, and its time starts over.
pub fn open(vault: &str, picked: &Path, protected: &Protected, now: DateTime<Utc>) -> Result<Opened, OpenFailed> {
    let real = check(picked, protected).map_err(OpenFailed::Refused)?;
    let (hidden, examples) = hidden_in(&real);
    let path = real.display().to_string();
    let mut doors = all(vault, now);
    let door = match doors.iter_mut().find(|d| d.path == path) {
        Some(door) => {
            door.opened = now;
            door.hidden = hidden;
            door.clone()
        }
        None => {
            let door = Door {
                id: hex::encode(Sha256::digest(format!("{path}\0{}", now.timestamp_nanos_opt().unwrap_or_default())))[..16].to_string(),
                name: unique_name(&real, &doors),
                path,
                opened: now,
                away: false,
                hidden,
            };
            doors.push(door.clone());
            door
        }
    };
    approvals::write(vault, Section::Doors, &doors).map_err(|e| OpenFailed::Write(e.to_string()))?;
    Ok(Opened { door, examples })
}

#[derive(Debug, thiserror::Error)]
pub enum OpenFailed {
    #[error("{0}")]
    Refused(Refused),
    #[error("the door could not be written down: {0}")]
    Write(String),
}

/// Closes a door. Whether there was one.
pub fn remove(vault: &str, id: &str, now: DateTime<Utc>) -> AppResult<bool> {
    let mut doors = all(vault, now);
    let before = doors.len();
    doors.retain(|d| d.id != id);
    approvals::write(vault, Section::Doors, &doors)?;
    Ok(doors.len() != before)
}

/// Whether a door may be used away from the computer. Whether there was one.
pub fn set_away(vault: &str, id: &str, away: bool, now: DateTime<Utc>) -> AppResult<bool> {
    let mut doors = all(vault, now);
    let Some(door) = doors.iter_mut().find(|d| d.id == id) else { return Ok(false) };
    door.away = away;
    approvals::write(vault, Section::Doors, &doors)?;
    Ok(true)
}

/// The door a model names, by its name or its id.
pub fn find<'a>(doors: &'a [Door], asked: &str) -> Option<&'a Door> {
    let asked = asked.trim();
    doors.iter().find(|d| d.id == asked || d.name.eq_ignore_ascii_case(asked))
}

/// The doors that may be read from here right now, each checked again: a vault
/// moved into one since it opened, or a link put where the folder was, closes it.
pub fn usable(vault: &str, at_the_app: bool, protected: &Protected, now: DateTime<Utc>) -> Vec<(Door, PathBuf)> {
    all(vault, now)
        .into_iter()
        .filter(|d| at_the_app || d.away)
        .filter_map(|d| {
            let real = check(Path::new(&d.path), protected).ok()?;
            (real == Path::new(&d.path)).then_some((d, real))
        })
        .collect()
}

/// A door the person has just picked on a card, for the call that asked for it
/// to take when the work carries on. One each, and only for a while: a pick
/// nobody took is not a door open for the next thing to ask.
static PICKED: Mutex<Option<HashMap<String, (String, Instant)>>> = Mutex::new(None);
const PICK_GOOD_FOR: Elapsed = Elapsed::from_secs(30 * 60);

pub fn picked_for(conversation: &str, id: &str) {
    let mut picked = PICKED.lock().unwrap_or_else(|e| e.into_inner());
    picked.get_or_insert_with(HashMap::new).insert(conversation.to_string(), (id.to_string(), Instant::now()));
}

pub fn take_pick(conversation: &str) -> Option<String> {
    let mut picked = PICKED.lock().unwrap_or_else(|e| e.into_inner());
    let (id, at) = picked.get_or_insert_with(HashMap::new).remove(conversation)?;
    (at.elapsed() < PICK_GOOD_FOR).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder that is not hidden: the system's temporary ones start with a dot.
    fn place() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::Builder::new().prefix("doors").tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        (dir, root)
    }

    fn protected(root: &Path) -> Protected {
        for folder in ["home/Library", "home/Documents/Vault/.synabit", "home/Documents/Taxes", "data/app"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        Protected { home: root.join("home"), vault: root.join("home/Documents/Vault"), app: vec![root.join("data/app")] }
    }

    /// A9: `~/Documents` with the vault inside is refused, however it is spelled
    /// or reached; a folder beside the vault is not.
    #[test]
    fn a_door_never_holds_the_vault_however_it_is_reached() {
        let (_dir, root) = place();
        let p = protected(&root);
        let docs = root.join("home/Documents");
        assert_eq!(check(&docs, &p), Err(Refused::Vault));
        assert_eq!(check(&root.join("HOME/documents"), &p), Err(Refused::Vault), "another case");
        std::os::unix::fs::symlink(&docs, root.join("docs-link")).unwrap();
        assert_eq!(check(&root.join("docs-link"), &p), Err(Refused::Vault), "through a link");
        assert_eq!(check(&root.join("home/Documents/Vault/Notes").parent().unwrap().join("."), &p), Err(Refused::Vault));
        std::fs::create_dir_all(root.join("home/Documents/Vault/Notes")).unwrap();
        assert_eq!(check(&root.join("home/Documents/Vault/Notes"), &p), Err(Refused::Vault), "inside it");
        // The vault reached through a link of its own is the same vault.
        std::os::unix::fs::symlink(root.join("home/Documents/Vault"), root.join("vault-link")).unwrap();
        let through = Protected { vault: root.join("vault-link"), ..p.clone() };
        assert_eq!(check(&docs, &through), Err(Refused::Vault));

        assert_eq!(check(&root.join("home/Documents/Taxes"), &p), Ok(root.join("home/Documents/Taxes")));
    }

    /// A firmlink spells one folder two ways; the data volume's own name for the
    /// home folder is still the home folder. Only on a Mac that has one.
    #[test]
    fn a_firmlinked_path_is_the_same_folder() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        let firm = Path::new("/System/Volumes/Data").join(home.strip_prefix("/").unwrap());
        if !firm.is_dir() {
            return;
        }
        assert!(contains(&firm, &home));
        let p = Protected { home: home.clone(), vault: home.join("nowhere"), app: vec![] };
        assert_eq!(check(&firm, &p), Err(Refused::TooWide));
        assert_eq!(check(Path::new("/"), &p), Err(Refused::TooWide));
    }

    #[test]
    fn what_decides_what_syn_may_do_is_never_a_door() {
        let (_dir, root) = place();
        let p = protected(&root);
        assert_eq!(check(&root.join("home"), &p), Err(Refused::TooWide));
        assert_eq!(check(&root, &p), Err(Refused::TooWide));
        assert_eq!(check(&root.join("data"), &p), Err(Refused::AppData));
        assert_eq!(check(&root.join("data/app"), &p), Err(Refused::AppData));
        assert_eq!(check(&root.join("home/Library"), &p), Err(Refused::Library));
        std::fs::create_dir_all(root.join("home/.config/gh")).unwrap();
        assert_eq!(check(&root.join("home/.config/gh"), &p), Err(Refused::Hidden));
        std::fs::create_dir_all(root.join("home/Documents/Taxes/.ssh")).unwrap();
        assert_eq!(check(&root.join("home/Documents/Taxes/.ssh"), &p), Err(Refused::Hidden));
        std::fs::write(root.join("home/Documents/file.txt"), "x").unwrap();
        assert_eq!(check(&root.join("home/Documents/file.txt"), &p), Err(Refused::NotAFolder));
        assert_eq!(check(&root.join("home/missing"), &p), Err(Refused::NotAFolder));
    }

    #[test]
    fn the_backstop_hides_key_shapes_in_any_case_and_nothing_else() {
        for hidden in [".ssh", ".SSH", ".aws", ".gnupg", ".env", ".env.local", "server.pem", "CERT.PEM", "id_rsa", "id_ed25519.pub", ".netrc", ".npmrc", ".git-credentials"] {
            assert!(hides(hidden), "{hidden}");
        }
        for shown in ["id_card.pdf", "env.txt", ".pem", "notes.md", "ssh-notes.txt", "my.envelope"] {
            assert!(!hides(shown), "{shown}");
        }
        let regexes = backstop_regexes();
        assert_eq!(regexes.len(), BACKSTOP.len());
        assert!(regexes.contains(&"/\\.[sS][sS][hH](/|$)".to_string()), "{regexes:?}");
    }

    #[test]
    fn opening_counts_what_the_backstop_hides() {
        let (_dir, root) = place();
        let p = protected(&root);
        let taxes = root.join("home/Documents/Taxes");
        std::fs::create_dir_all(taxes.join(".ssh")).unwrap();
        std::fs::write(taxes.join(".ssh/id_rsa"), "k").unwrap();
        std::fs::write(taxes.join(".env"), "k").unwrap();
        std::fs::write(taxes.join("2025.pdf"), "x").unwrap();
        let vault = root.join("home/Documents/Vault").display().to_string();
        let opened = open(&vault, &taxes, &p, Utc::now()).unwrap();
        assert_eq!(opened.door.hidden, 2, "the .ssh folder once, not its key again");
        assert_eq!(opened.door.name, "Taxes");
        assert!(!opened.door.away, "a door is the app's until said otherwise");
        assert!(opened.examples.iter().any(|e| e == ".env"), "{:?}", opened.examples);
    }

    #[test]
    fn a_door_lasts_as_an_always_does_and_opening_it_again_renews_it() {
        let (_dir, root) = place();
        let p = protected(&root);
        let vault = root.join("home/Documents/Vault").display().to_string();
        let taxes = root.join("home/Documents/Taxes");
        let then = Utc::now() - Duration::days(ALWAYS_LASTS_DAYS - 1);
        let first = open(&vault, &taxes, &p, then).unwrap().door;
        assert_eq!(all(&vault, Utc::now()).len(), 1);
        assert!(all(&vault, Utc::now() + Duration::days(2)).is_empty(), "expired");

        let again = open(&vault, &taxes, &p, Utc::now()).unwrap().door;
        assert_eq!((again.id.as_str(), again.name.as_str()), (first.id.as_str(), "Taxes"));
        assert_eq!(all(&vault, Utc::now() + Duration::days(2)).len(), 1);

        std::fs::create_dir_all(root.join("elsewhere/Taxes")).unwrap();
        let other = open(&vault, &root.join("elsewhere/Taxes"), &p, Utc::now()).unwrap().door;
        assert_eq!(other.name, "Taxes 2");
        assert_eq!(find(&all(&vault, Utc::now()), "taxes 2").map(|d| &d.id), Some(&other.id));
    }

    /// A23, the door's part: away from the app only a door the person allowed
    /// there; and a door is checked again whenever it is used.
    #[test]
    fn away_from_the_app_only_doors_allowed_there_and_each_is_checked_again() {
        let (_dir, root) = place();
        let p = protected(&root);
        let vault = root.join("home/Documents/Vault").display().to_string();
        let taxes = open(&vault, &root.join("home/Documents/Taxes"), &p, Utc::now()).unwrap().door;
        std::fs::create_dir_all(root.join("home/Pictures")).unwrap();
        let pictures = open(&vault, &root.join("home/Pictures"), &p, Utc::now()).unwrap().door;
        set_away(&vault, &pictures.id, true, Utc::now()).unwrap();

        let names = |at_the_app| usable(&vault, at_the_app, &p, Utc::now()).into_iter().map(|(d, _)| d.name).collect::<Vec<_>>();
        assert_eq!(names(true), ["Taxes", "Pictures"]);
        assert_eq!(names(false), ["Pictures"]);

        // The vault is moved into a door after it was opened: the door is shut.
        std::fs::rename(root.join("home/Documents/Vault"), root.join("home/Documents/Taxes/Vault")).unwrap();
        let moved = Protected { vault: root.join("home/Documents/Taxes/Vault"), ..p.clone() };
        let vault = moved.vault.display().to_string();
        assert!(usable(&vault, true, &moved, Utc::now()).iter().all(|(d, _)| d.id != taxes.id));

        assert!(remove(&vault, &pictures.id, Utc::now()).unwrap());
        assert!(!remove(&vault, &pictures.id, Utc::now()).unwrap());
    }

    #[test]
    fn a_pick_is_taken_once() {
        picked_for("conv-a", "0123456789abcdef");
        assert_eq!(take_pick("conv-b"), None);
        assert_eq!(take_pick("conv-a").as_deref(), Some("0123456789abcdef"));
        assert_eq!(take_pick("conv-a"), None);
    }
}
