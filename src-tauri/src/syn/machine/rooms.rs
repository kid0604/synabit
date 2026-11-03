//! All the rooms on this machine, and whether there may be any.
//!
//! This is the only way the rest of the app gets a room. A room that has not
//! passed [`selftest`] on this macOS, with this build's rules, is not handed out:
//! Apple can change Seatbelt in any update, and a profile that stopped holding
//! would otherwise go on looking like one. A profile that does not compile, as on
//! macOS 13, is the same failure. Nothing removes a rule to make it start.
//!
//! The check attacks real rooms and takes seconds, so a pass is written down
//! against the app version and the exact macOS build, and asked for again when
//! either changes or when the switch is turned on. A failure is never written
//! down, so it is measured again at the next launch.
//!
//! Stopping needs none of that. It works from the folders on disk rather than
//! from anything this launch remembers, so it also finds what a crashed one left.

use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::sync::Mutex;

use super::cell::{Cell, Limits};
use super::profile::Reach;
use super::selftest::{self, Report, Verdict};
use super::watch::{self, Ceiling, Frozen, Watched};

/// Why there is no room to be had. English, for the log and for the model's
/// refusal; a screen says it through i18n.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Unavailable {
    #[error("the room failed its self-check on this macOS: {0}")]
    NotSealed(String),
    #[error("that is not a name for a room")]
    BadName,
    #[error("could not set up the room: {0}")]
    Setup(String),
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    /// None until something has needed a room in this launch.
    pub sealed: Option<bool>,
    pub broken: Option<String>,
    /// Processes in rooms right now, wherever they have got to.
    pub running: usize,
}

pub struct Rooms {
    root: PathBuf,
    tools: PathBuf,
    bait: PathBuf,
    ceiling: Ceiling,
    /// This launch's answer. Also what keeps two callers from running the attack
    /// set at once.
    known: Mutex<Option<Result<(), String>>>,
    /// Replaces the rules, so a test can prove a broken profile closes the door.
    #[cfg(test)]
    rules: Option<String>,
}

/// What a pass is good for. None means it cannot be told, and so is never trusted.
fn fingerprint() -> Option<String> {
    let ask = |flag: &str| {
        let out = std::process::Command::new("/usr/bin/sw_vers")
            .arg(flag)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !text.is_empty()).then_some(text)
    };
    Some(format!(
        "{} {} {}",
        env!("CARGO_PKG_VERSION"),
        ask("-productVersion")?,
        ask("-buildVersion")?
    ))
}

/// A room's name becomes a folder, so it cannot be anything but a name.
fn usable_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What the report says went wrong, in one line.
fn why_not(report: &Report) -> String {
    if let Some(broken) = &report.broken {
        return broken.clone();
    }
    report
        .lines
        .iter()
        .find(|line| !matches!(line.verdict, Verdict::Held | Verdict::Skipped))
        .map(|line| format!("{} {}: {:?}", line.id, line.what, line.verdict))
        .unwrap_or_else(|| "no attack ran".to_string())
}

impl Rooms {
    pub fn at(root: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let bait = root.join("bait");
        std::fs::write(&bait, "outside every room")?;
        Ok(Self {
            tools: root.join("tools"),
            bait,
            ceiling: Ceiling::default(),
            known: Mutex::new(None),
            #[cfg(test)]
            rules: None,
            root,
        })
    }

    fn cells(&self) -> PathBuf {
        self.root.join("cells")
    }

    fn pass_file(&self) -> PathBuf {
        self.root.join("sealed")
    }

    fn cell_at(&self, name: &str) -> io::Result<Cell> {
        Cell::make(&self.cells().join(name), &self.tools, Reach::Closed)
    }

    /// Attacks the room now, whatever is written down, and keeps the answer.
    pub async fn check(&self) -> Report {
        let mut known = self.known.lock().await;
        self.check_holding(&mut known).await
    }

    async fn check_holding(&self, known: &mut Option<Result<(), String>>) -> Report {
        let place = self.root.join("selftest");
        let measured = selftest::run_with(
            &place,
            |_cell| {
                #[cfg(test)]
                if let Some(rules) = &self.rules {
                    _cell.profile.rules = Some(rules.clone());
                }
            },
            false,
        )
        .await;
        let _ = std::fs::remove_dir_all(&place);
        let report = measured.unwrap_or_else(|error| Report {
            os: String::new(),
            sealed: false,
            broken: Some(format!("the self-check could not run: {error}")),
            lines: Vec::new(),
        });
        // Written or removed before anyone is told, so that a room which has just
        // failed can never be vouched for by an older pass.
        let _ = std::fs::remove_file(self.pass_file());
        if report.sealed {
            if let Some(print) = fingerprint() {
                let _ = std::fs::write(self.pass_file(), print);
            }
            *known = Some(Ok(()));
        } else {
            *known = Some(Err(why_not(&report)));
        }
        report
    }

    /// Whether a room may be handed out, checking first if that is not yet known.
    pub async fn ready(&self) -> Result<(), Unavailable> {
        let mut known = self.known.lock().await;
        if known.is_none() {
            let written = std::fs::read_to_string(self.pass_file()).ok();
            if written.is_some() && written == fingerprint() {
                *known = Some(Ok(()));
            } else {
                self.check_holding(&mut known).await;
            }
        }
        match known.as_ref() {
            Some(Ok(())) => Ok(()),
            Some(Err(why)) => Err(Unavailable::NotSealed(why.clone())),
            None => Err(Unavailable::NotSealed("it was never checked".to_string())),
        }
    }

    /// A room by name, made if it is new, once it is known to hold.
    pub async fn open(&self, name: &str) -> Result<Cell, Unavailable> {
        if !usable_name(name) {
            return Err(Unavailable::BadName);
        }
        self.ready().await?;
        self.cell_at(name)
            .map_err(|error| Unavailable::Setup(error.to_string()))
    }

    /// Runs a command, watched against this machine's ceilings.
    pub async fn run(&self, cell: &Cell, script: &str, limits: Limits) -> io::Result<Watched> {
        watch::run_watched(cell, &self.bait, script, limits, self.ceiling).await
    }

    /// Stops everything in one room, for a file to leave it.
    pub fn freeze<'a>(&self, cell: &'a Cell) -> io::Result<Frozen<'a>> {
        watch::freeze(cell, &self.bait)
    }

    /// Ends whatever is running in one room. Blocks, as [`Self::stop_all`] does.
    pub fn clear(&self, cell: &Cell) -> io::Result<usize> {
        watch::clear_room(cell, &self.bait)
    }

    /// Where a room's one remembered fact is kept: outside every room's folder,
    /// so no command can take it back.
    fn mark_of(&self, name: &str) -> Option<PathBuf> {
        usable_name(name).then(|| self.root.join("outside").join(name))
    }

    /// The room now holds somebody else's words: a file, a page's figures, or a
    /// command a run wrote after reading them. Never unset; what is in a room
    /// cannot be told apart once it is there.
    pub fn mark_outside(&self, name: &str) -> io::Result<()> {
        let mark = self.mark_of(name).ok_or_else(|| io::Error::other("not a name for a room"))?;
        if let Some(folder) = mark.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(mark, "")
    }

    /// Whether what comes out of the room is somebody else's words.
    pub fn carries_outside(&self, name: &str) -> bool {
        self.mark_of(name).is_some_and(|mark| mark.exists())
    }

    /// Every room with a folder, this launch's or not. A room can put a link
    /// where its folder was, and [`Cell::make`] puts the folder back.
    fn every_cell(&self) -> io::Result<Vec<Cell>> {
        let entries = match std::fs::read_dir(self.cells()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut cells = Vec::new();
        for entry in entries {
            let name = entry?.file_name();
            if let Some(name) = name.to_str().filter(|name| usable_name(name)) {
                cells.push(self.cell_at(name)?);
            }
        }
        Ok(cells)
    }

    /// Stops everything in every room, without asking anything or anyone. Goes on
    /// past a room it cannot clear, and reports the first such failure afterwards.
    /// Blocks for as long as the kernel takes, so not for an async thread.
    pub fn stop_all(&self) -> io::Result<usize> {
        let mut stopped = 0;
        let mut failed = None;
        for cell in self.every_cell()? {
            match watch::clear_room(&cell, &self.bait) {
                Ok(count) => stopped += count,
                Err(error) => failed = failed.or(Some(error)),
            }
        }
        match failed {
            Some(error) => Err(error),
            None => Ok(stopped),
        }
    }

    pub async fn status(&self) -> Status {
        let known = self.known.lock().await.clone();
        let running = self.every_cell().map_or(0, |cells| {
            cells
                .iter()
                .filter_map(|cell| watch::room_processes(cell, &self.bait).ok())
                .map(|pids| pids.len())
                .sum()
        });
        Status {
            sealed: known.as_ref().map(Result::is_ok),
            broken: known.and_then(Result::err),
            running,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_daemon() -> String {
        selftest::daemon(r#"chdir "/"; sleep 20"#)
    }

    fn rooms() -> (tempfile::TempDir, Rooms) {
        let dir = tempfile::tempdir().unwrap();
        let rooms = Rooms::at(&dir.path().join("Application Support/machine")).unwrap();
        (dir, rooms)
    }

    #[tokio::test]
    async fn a_name_cannot_lead_out_of_the_rooms_folder() {
        let (_dir, rooms) = rooms();
        for name in ["", "..", "../up", "a/b", "/abs", "a b", &"x".repeat(65)] {
            assert_eq!(rooms.open(name).await.err(), Some(Unavailable::BadName), "{name}");
        }
    }

    /// What the whole gate is for: a profile that does not compile, as on macOS
    /// 13, gets no room, and leaves nothing that would vouch for one later.
    #[tokio::test]
    async fn no_room_is_handed_out_when_the_check_fails() {
        let (_dir, mut rooms) = rooms();
        rooms.rules = Some("(version 1)(deny file-no-such-rule)".into());

        let got = rooms.open("a").await.err();

        assert!(matches!(got, Some(Unavailable::NotSealed(_))), "{got:?}");
        assert!(!rooms.pass_file().exists());
        assert!(!rooms.cells().join("a").exists());
        let status = rooms.status().await;
        assert_eq!(status.sealed, Some(false));
        assert!(status.broken.is_some());
    }

    /// A room with no rules leaks rather than fails to start. Running the full set
    /// in one would really open things on this machine, so this is the report.
    #[test]
    fn a_report_that_leaked_says_what_leaked() {
        let report = Report {
            os: "26".into(),
            sealed: false,
            broken: None,
            lines: vec![selftest::Line {
                id: "A2",
                what: "read a file outside".into(),
                verdict: Verdict::Leaked,
                detail: String::new(),
            }],
        };
        assert_eq!(why_not(&report), "A2 read a file outside: Leaked");
    }

    /// One test, since each real check takes seconds: a pass is written down, is
    /// believed on the next launch without attacking anything, and stops being
    /// believed when the macOS or the app it was measured on changes. Broken
    /// rules on the later launches are how it shows whether a check really ran.
    #[tokio::test]
    async fn a_pass_holds_for_this_build_of_macos_and_no_other() {
        let (dir, rooms) = rooms();
        let root = dir.path().join("Application Support/machine");
        rooms.ready().await.unwrap();
        assert!(rooms.open("a").await.unwrap().work().is_dir());
        assert_eq!(
            std::fs::read_to_string(rooms.pass_file()).ok(),
            fingerprint()
        );

        let mut next_launch = Rooms::at(&root).unwrap();
        next_launch.rules = Some("(version 1)(deny file-no-such-rule)".into());
        next_launch.ready().await.unwrap();

        std::fs::write(rooms.pass_file(), "1.0 25.0 25A1").unwrap();
        let mut after_an_update = Rooms::at(&root).unwrap();
        after_an_update.rules = Some("(version 1)(deny file-no-such-rule)".into());
        assert!(after_an_update.ready().await.is_err());
        assert!(!rooms.pass_file().exists(), "the old pass was left behind");
    }

    #[tokio::test]
    async fn stopping_finds_what_left_the_group_in_every_room() {
        let (_dir, rooms) = rooms();
        let a = rooms.cell_at("a").unwrap();
        let b = rooms.cell_at("b").unwrap();
        for cell in [&a, &b] {
            assert!(cell.run(&a_daemon(), Limits::default()).await.unwrap().ok());
        }
        assert_eq!(rooms.status().await.running, 2);

        assert_eq!(rooms.stop_all().unwrap(), 2);

        assert_eq!(rooms.status().await.running, 0);
        assert_eq!(rooms.stop_all().unwrap(), 0);
    }

    /// A launch after a crash knows nothing except what is on disk, and one of
    /// the rooms it finds has put a link where its folder was.
    #[tokio::test]
    async fn a_new_launch_clears_what_the_last_one_left() {
        let (dir, rooms) = rooms();
        let root = dir.path().join("Application Support/machine");
        let elsewhere = dir.path().canonicalize().unwrap().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let a = rooms.cell_at("a").unwrap();
        let room = a.room().display().to_string();
        let script = format!(
            "{}; cd /; rm -rf '{room}'; ln -s '{}' '{room}'",
            a_daemon(),
            elsewhere.display()
        );
        assert!(a.run(&script, Limits::default()).await.unwrap().ok());
        assert!(a.room().is_symlink());

        let next_launch = Rooms::at(&root).unwrap();
        assert_eq!(next_launch.stop_all().unwrap(), 1);

        assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        assert!(!a.room().is_symlink());
    }

    /// The mark outlives the launch, and the room it marks cannot remove it.
    #[tokio::test]
    async fn a_room_cannot_forget_what_it_was_given() {
        let (dir, rooms) = rooms();
        let root = dir.path().join("Application Support/machine");
        let a = rooms.cell_at("a").unwrap();
        assert!(!rooms.carries_outside("a"));
        rooms.mark_outside("a").unwrap();

        let mark = rooms.root.join("outside/a");
        let ran = a
            .run(&format!("rm -f '{}'; : > '{}.x'", mark.display(), mark.display()), Limits::default())
            .await
            .unwrap();
        assert!(!ran.ok(), "{ran:?}");

        let next_launch = Rooms::at(&root).unwrap();
        assert!(next_launch.carries_outside("a"));
        assert!(!next_launch.carries_outside("b"));
        assert!(!next_launch.carries_outside("../a"));
    }

    /// Nothing has ever needed a room: finding that out costs no attack.
    #[tokio::test]
    async fn stopping_with_no_rooms_does_nothing_and_checks_nothing() {
        let (_dir, rooms) = rooms();
        assert_eq!(rooms.stop_all().unwrap(), 0);
        assert_eq!(rooms.status().await.sealed, None);
        assert!(!rooms.root.join("selftest").exists());
    }
}
