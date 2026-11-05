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
use tokio::sync::{Mutex, RwLock};

use super::cell::{Cell, Limits};
use super::profile::Reach;
use super::proxy::{self, Allow, Proxy};
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
    /// Where the helper's rooms go (`super::outside`): a folder whose path
    /// does not hold the person's name. `None` when no such folder was found,
    /// and then there is no helper.
    outside: Option<PathBuf>,
    tools: PathBuf,
    bait: PathBuf,
    ceiling: Ceiling,
    /// This launch's answer. Also what keeps two callers from running the attack
    /// set at once.
    known: Mutex<Option<Result<(), String>>>,
    /// Every sealed command holds this to read for as long as it runs; a room
    /// with a network holds it to write for all of its time. So the two never run
    /// at once (I8): a channel they could share needs both ends running.
    window: RwLock<()>,
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
        let outside = super::outside::place_for(&root);
        Ok(Self {
            tools: root.join("tools"),
            outside,
            bait,
            ceiling: Ceiling::default(),
            known: Mutex::new(None),
            window: RwLock::new(()),
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

    /// Runs a command, watched against this machine's ceilings. Waits while a
    /// room with a network is open.
    pub async fn run(&self, cell: &Cell, script: &str, limits: Limits) -> io::Result<Watched> {
        let _sealed = self.window.read().await;
        watch::run_watched(cell, &self.bait, script, limits, self.ceiling).await
    }

    /// Opens a room with a network for as long as `work` takes, and nothing else
    /// in any room runs meanwhile (I8). In order: no sealed command may start or
    /// is still running; whatever any room left behind is ended and the kernel
    /// asked to confirm it; only then the port opens and the room is made. After
    /// `work`, its room is ended and confirmed empty before the port closes, and
    /// only then may a sealed room run again. If anything cannot be confirmed
    /// gone, the port never opens.
    ///
    /// `tools` is copied in by `work` itself; nothing shared is in reach.
    pub async fn with_network<T, F, Fut>(&self, name: &str, allow: Allow, work: F) -> Result<T, Unavailable>
    where
        F: FnOnce(Cell) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        if !usable_name(name) {
            return Err(Unavailable::BadName);
        }
        let place = self.root.join("prep").join(name);
        self.open_window(name, allow, &place, work).await
    }

    /// The helper's room (`super::outside`): the same window, to any public
    /// host, in a folder whose path does not name the person. The folder goes
    /// when the window closes; whatever was to leave it left during `work`.
    pub async fn with_outside<T, F, Fut>(&self, name: &str, work: F) -> Result<T, Unavailable>
    where
        F: FnOnce(Cell) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        if !usable_name(name) {
            return Err(Unavailable::BadName);
        }
        let Some(outside) = &self.outside else {
            return Err(Unavailable::Setup(
                "no folder was found for the helper's room whose path does not hold the person's name".into(),
            ));
        };
        let place = outside.join(name);
        let nameless = |mut cell: Cell| {
            cell.profile.nameless = true;
            work(cell)
        };
        let out = self.open_window(name, super::outside::allow(), &place, nameless).await;
        let _ = std::fs::remove_dir_all(&place);
        out
    }

    /// Runs a command in the room [`Self::with_outside`] handed to its `work`,
    /// which holds the window: so not through [`Self::run`], which would wait
    /// for the window it is inside. Refused for a sealed room.
    pub async fn run_outside(&self, cell: &Cell, script: &str, limits: Limits) -> io::Result<Watched> {
        if !matches!(cell.profile.reach, Reach::Proxy(_)) || cell.profile.sight != super::profile::Sight::Bare {
            return Err(io::Error::other("not a room with a network"));
        }
        watch::run_watched(cell, &self.bait, script, limits, self.ceiling).await
    }

    async fn open_window<T, F, Fut>(&self, name: &str, allow: Allow, place: &Path, work: F) -> Result<T, Unavailable>
    where
        F: FnOnce(Cell) -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        self.ready().await?;
        let _all = self.window.write().await;

        let cells = self.every_cell().map_err(|e| Unavailable::Setup(e.to_string()))?;
        let bait = self.bait.clone();
        let cleared = tokio::task::spawn_blocking(move || -> io::Result<()> {
            for cell in &cells {
                watch::clear_room(cell, &bait)?;
                if !watch::room_processes(cell, &bait)?.is_empty() {
                    return Err(io::Error::other("a sealed room still has something running"));
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| Unavailable::Setup(e.to_string()))?;
        cleared.map_err(|e| Unavailable::Setup(format!("the sealed rooms could not be stopped, so no network was opened: {e}")))?;

        let proxy = Proxy::open(name, allow, &proxy::audit_file(&self.root))
            .await
            .map_err(|e| Unavailable::Setup(e.to_string()))?;
        let _ = std::fs::remove_dir_all(place);
        let cell = Cell::make_bare(&place.join("room"), &place.join("tools"), proxy.port)
            .map_err(|e| Unavailable::Setup(e.to_string()))?;
        let out = work(cell.clone()).await;

        let bait = self.bait.clone();
        let ended = tokio::task::spawn_blocking(move || -> io::Result<()> {
            watch::clear_room(&cell, &bait)?;
            if !watch::room_processes(&cell, &bait)?.is_empty() {
                return Err(io::Error::other("the room with a network would not end"));
            }
            Ok(())
        })
        .await;
        drop(proxy);
        // The port is closed either way. A room that would not end is reported,
        // and the sealed rooms wait for nothing more: nothing can reach out now.
        match ended {
            Ok(Ok(())) => Ok(out),
            Ok(Err(e)) => Err(Unavailable::Setup(e.to_string())),
            Err(e) => Err(Unavailable::Setup(e.to_string())),
        }
    }

    /// The folder everything is kept under.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a room with a network keeps what it made, for the caller to take.
    pub fn prep_place(&self, name: &str) -> PathBuf {
        self.root.join("prep").join(name)
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

    /// Which doors a room reads: the ids, one a line, kept outside every room
    /// like its mark, so no command can add one.
    fn doors_file(&self, name: &str) -> Option<PathBuf> {
        usable_name(name).then(|| self.root.join("doors").join(name))
    }

    /// The room reads this door from now on. What is in a door is anybody's
    /// words, so the room is marked as well (design §6.1).
    pub fn attach_door(&self, name: &str, id: &str) -> io::Result<()> {
        if !(id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())) {
            return Err(io::Error::other("not a door's id"));
        }
        let file = self.doors_file(name).ok_or_else(|| io::Error::other("not a name for a room"))?;
        self.mark_outside(name)?;
        let mut ids = self.doors_of(name);
        if !ids.iter().any(|known| known == id) {
            ids.push(id.to_string());
        }
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(file, ids.join("\n"))
    }

    /// The doors a room has been given, whether or not they are still open.
    pub fn doors_of(&self, name: &str) -> Vec<String> {
        self.doors_file(name)
            .and_then(|file| std::fs::read_to_string(file).ok())
            .map(|text| text.lines().filter(|l| !l.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// Every room with a folder, this launch's or not. A room can put a link
    /// where its folder was, and [`Cell::make`] puts the folder back.
    fn every_cell(&self) -> io::Result<Vec<Cell>> {
        let entries: Vec<io::Result<std::fs::DirEntry>> = match std::fs::read_dir(self.cells()) {
            Ok(entries) => entries.collect(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        let mut cells = Vec::new();
        for entry in entries {
            let name = entry?.file_name();
            if let Some(name) = name.to_str().filter(|name| usable_name(name)) {
                cells.push(self.cell_at(name)?);
            }
        }
        // Rooms with a network, one a crash may have left: ended the same way.
        for folder in [Some(self.root.join("prep")), self.outside.clone()].into_iter().flatten() {
            if let Ok(entries) = std::fs::read_dir(folder) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name.to_str().is_some_and(usable_name) {
                        cells.push(Cell::make(&entry.path().join("room"), &entry.path().join("tools"), Reach::Closed)?);
                    }
                }
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

    fn exec(path: &str, args: &[&str]) -> (std::path::PathBuf, Vec<std::ffi::OsString>) {
        (path.into(), args.iter().map(|a| a.into()).collect())
    }

    /// A31: a sealed room leaves a daemon beating out a signal, and a sealed
    /// command is about to run. Before the port opens the daemon is gone, and the
    /// command waits until the port has closed.
    #[tokio::test]
    async fn nothing_sealed_runs_while_a_room_has_a_network() {
        let (_dir, rooms) = rooms();
        let rooms = std::sync::Arc::new(rooms);
        let a = rooms.cell_at("a").unwrap();
        let beat = a.work().join("beat");
        let daemon = selftest::daemon(&format!(
            r#"while (1) {{ open(B, ">>", "{}"); print B "x"; close(B); select(undef, undef, undef, 0.1) }}"#,
            beat.display()
        ));
        assert!(a.run(&daemon, Limits::default()).await.unwrap().ok());
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(std::fs::metadata(&beat).unwrap().len() > 0, "the control: it beats");

        let (opened_tx, opened_rx) = tokio::sync::oneshot::channel::<()>();
        let later = {
            let (rooms, b) = (rooms.clone(), rooms.cell_at("b").unwrap());
            tokio::spawn(async move {
                opened_rx.await.unwrap();
                rooms.run(&b, "date +%s%N", Limits::default()).await.unwrap();
                std::time::Instant::now()
            })
        };
        let silent = rooms
            .with_network("n1", Allow::Hosts(vec![]), |_cell| async {
                let _ = opened_tx.send(());
                let before = std::fs::metadata(&beat).map(|m| m.len()).unwrap_or(0);
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                let after = std::fs::metadata(&beat).map(|m| m.len()).unwrap_or(0);
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                (before == after, std::time::Instant::now())
            })
            .await
            .unwrap();
        assert!(silent.0, "the daemon still beat while the port was open");
        assert!(later.await.unwrap() > silent.1, "a sealed command ran while the port was open");
    }

    /// A36: what a sealed room reads on the data volume, a room with a network
    /// cannot even look at, so no access time a sealed room left there can reach
    /// it. The control: its own files and the system's programs it can.
    #[tokio::test]
    async fn a_room_with_a_network_cannot_see_what_a_sealed_room_reads() {
        let (_dir, rooms) = rooms();
        let shared_tools = rooms.tools.clone();
        std::fs::create_dir_all(&shared_tools).unwrap();
        std::fs::write(shared_tools.join("marker"), "x").unwrap();
        let mut shared: Vec<String> = [
            "/private/etc/ssl/cert.pem",
            "/private/etc/passwd",
            "/private/var/db/timezone",
            "/private/var/select/sh",
            "/private/var/db/xcode_select_link",
            "/opt/homebrew/bin",
            "/Library/Developer/CommandLineTools",
        ]
        .iter()
        .filter(|p| std::path::Path::new(p).exists() || std::path::Path::new(p).is_symlink())
        .map(|p| p.to_string())
        .collect();
        shared.push(shared_tools.join("marker").display().to_string());
        assert!(shared.len() >= 5, "{shared:?}");

        let seen = rooms
            .with_network("n2", Allow::Hosts(vec![]), |cell| async move {
                let mut seen = Vec::new();
                let (stat, mut args) = exec("/usr/bin/stat", &["-f", "%a"]);
                std::fs::write(cell.work().join("own"), "mine").unwrap();
                args.push("own".into());
                let own = cell.exec(&stat, &args, &[], Limits::default()).await.unwrap();
                seen.push(("own".to_string(), own.ok()));
                for path in &shared {
                    let (stat, mut args) = exec("/usr/bin/stat", &["-f", "%a"]);
                    args.push(path.into());
                    let ran = cell.exec(&stat, &args, &[], Limits::default()).await.unwrap();
                    seen.push((path.clone(), ran.ok()));
                }
                let (ls, args) = exec("/bin/ls", &["/usr/bin/true"]);
                seen.push(("/bin/ls".to_string(), cell.exec(&ls, &args, &[], Limits::default()).await.unwrap().ok()));
                seen
            })
            .await
            .unwrap();

        assert_eq!(seen[0], ("own".to_string(), true), "the control: {seen:?}");
        assert!(seen.last().unwrap().1, "the control: {seen:?}");
        for (path, could) in &seen[1..seen.len() - 1] {
            assert!(!could, "a room with a network could stat {path}");
        }
    }

    /// The port is the room's one way out, and the proxy decides the rest:
    /// a raw connection to an address fails, a request through the port is heard.
    #[tokio::test]
    async fn a_room_with_a_network_reaches_only_its_port() {
        let (_dir, rooms) = rooms();
        let said = rooms
            .with_network("n3", Allow::Hosts(vec![]), |cell| async move {
                let port = match cell.profile.reach {
                    Reach::Proxy(port) => port,
                    Reach::Closed => 0,
                };
                // Perl, from the system volume: the system's curl cannot start
                // here at all, since it reads its TLS settings from /private/etc.
                let (perl, args) = exec("/usr/bin/perl", &["-MIO::Socket::INET", "-e", r#"IO::Socket::INET->new(PeerAddr => "1.1.1.1:443", Timeout => 5) or exit 3"#]);
                let direct = cell.exec(&perl, &args, &[], Limits::default()).await.unwrap();
                let ask = format!(
                    r#"$s = IO::Socket::INET->new("127.0.0.1:{port}") or exit 3; print $s "CONNECT pypi.org:443 HTTP/1.1\r\n\r\n"; print scalar <$s>"#
                );
                let (perl, args) = exec("/usr/bin/perl", &["-MIO::Socket::INET", "-e", &ask]);
                let proxied = cell.exec(&perl, &args, &[], Limits::default()).await.unwrap();
                (direct, proxied)
            })
            .await
            .unwrap();
        assert!(!said.0.ok(), "a raw connection got out: {:?}", said.0);
        assert!(said.1.stdout.starts_with("HTTP/1.1 403"), "the port answered: {:?}", said.1);
        let audit = std::fs::read_to_string(proxy::audit_file(&rooms.root)).unwrap();
        assert!(audit.contains("pypi.org") && audit.contains("not a host this room may reach"), "{audit}");
    }

    /// A door under the kernel (design §8, A9): the room reads the folder and
    /// cannot change it; the backstop hides key-shaped names whatever their case;
    /// a link in the folder to the vault or to a key leads nowhere; a room not
    /// given the door, and a room with a network given it anyway, see none of it.
    #[tokio::test]
    async fn a_room_reads_its_door_and_nothing_past_it() {
        let (dir, rooms) = rooms();
        let root = dir.path().canonicalize().unwrap();
        let door = root.join("Taxes");
        let vault = root.join("Vault");
        std::fs::create_dir_all(door.join(".ssh")).unwrap();
        std::fs::create_dir_all(door.join("2025")).unwrap();
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(door.join("2025/income.txt"), "income 100").unwrap();
        std::fs::write(door.join(".ssh/id_ed25519"), "KEY").unwrap();
        std::fs::write(door.join(".env.local"), "TOKEN").unwrap();
        std::fs::write(door.join("server.PEM"), "PEM").unwrap();
        std::fs::write(vault.join("note.md"), "VAULT").unwrap();
        std::os::unix::fs::symlink(vault.join("note.md"), door.join("note-link.md")).unwrap();
        std::os::unix::fs::symlink(door.join(".ssh/id_ed25519"), door.join("2025/key-link")).unwrap();

        let a = rooms.cell_at("a").unwrap().with_doors(vec![door.clone()]);
        let at = |rel: &str| door.join(rel).display().to_string();
        let cat = |rel: &str| format!("cat '{}'", at(rel));
        let read = rooms.run(&a, &cat("2025/income.txt"), Limits::default()).await.unwrap();
        assert_eq!(read.ran.stdout, "income 100", "the control: {read:?}");
        let copied = rooms.run(&a, &format!("cp -R '{}' copy; cat copy/2025/income.txt", door.display()), Limits::default()).await.unwrap();
        assert!(copied.ran.stdout.contains("income 100"), "a copy into the room: {copied:?}");
        assert!(!a.work().join("copy/.env.local").exists() && !a.work().join("copy/.ssh").exists());

        for hidden in [".ssh/id_ed25519", ".SSH/ID_ED25519", ".env.local", ".ENV.local", "server.PEM", "note-link.md", "2025/key-link"] {
            let ran = rooms.run(&a, &cat(hidden), Limits::default()).await.unwrap();
            assert!(!ran.ran.ok() && ran.ran.stdout.is_empty(), "{hidden}: {ran:?}");
        }
        let wrote = rooms.run(&a, &format!("echo x > '{}'; rm -f '{}'", at("new.txt"), at("2025/income.txt")), Limits::default()).await.unwrap();
        assert!(!door.join("new.txt").exists() && door.join("2025/income.txt").exists(), "{wrote:?}");

        let b = rooms.cell_at("b").unwrap();
        assert!(!rooms.run(&b, &cat("2025/income.txt"), Limits::default()).await.unwrap().ran.ok(), "a room without the door");

        let door_for_net = door.clone();
        let networked = rooms
            .with_network("n4", Allow::Hosts(vec![]), |cell| async move {
                let cell = cell.with_doors(vec![door_for_net.clone()]);
                let (cat, mut args) = exec("/bin/cat", &[]);
                args.push(door_for_net.join("2025/income.txt").into());
                cell.exec(&cat, &args, &[], Limits::default()).await.unwrap()
            })
            .await
            .unwrap();
        assert!(!networked.ok(), "a room with a network read a door: {networked:?}");
    }

    #[tokio::test]
    async fn a_door_given_to_a_room_marks_it_and_stays_given() {
        let (dir, rooms) = rooms();
        let root = dir.path().join("Application Support/machine");
        assert!(rooms.doors_of("a").is_empty());
        rooms.attach_door("a", "0123456789abcdef").unwrap();
        rooms.attach_door("a", "0123456789abcdef").unwrap();
        assert!(rooms.attach_door("a", "../../x").is_err());
        assert!(rooms.attach_door("../a", "0123456789abcdef").is_err());
        let next_launch = Rooms::at(&root).unwrap();
        assert_eq!(next_launch.doors_of("a"), ["0123456789abcdef"]);
        assert!(next_launch.carries_outside("a"), "a door is somebody else's words");
        assert!(next_launch.doors_of("b").is_empty());
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
