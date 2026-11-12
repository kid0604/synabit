//! The rules a room runs under, as arguments for `sandbox-exec`.
//!
//! The text of the profile never contains a path. A folder called `a") (allow
//! default` would otherwise become a rule, and the app's own folder already has
//! a space in it. Every path goes through `-D` and is read back with `(param)`,
//! including the folders above the room, which a process has to be able to
//! stat to find its way down.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const CLOSED: &str = include_str!("closed.sb");
/// What a room with a network sees: the system's own volume and its own folder,
/// nothing else on the data volume. No Homebrew, no command-line tools, no shared
/// `tools/`, no `/private/etc`, not even `/private/var/select`, which `/bin/sh`
/// reads: a sealed room reads those, and reading leaves an access time a room
/// with a network could read after it (design §5.8, A36). So nothing runs in it
/// through a shell; Rust runs one program.
const BARE: &str = include_str!("bare.sb");
const NET: &str = include_str!("net.sb");
/// LibreOffice's pipe and LaunchServices ([`Profile::office`]).
const OFFICE: &str = "(allow file-write* (literal \"/private/tmp\") (regex #\"^/private/tmp/OSL_PIPE_[0-9]+_SingleOfficeIPC_[0-9a-f]+$\"))\n\
(allow network-bind (local unix-socket (path-regex #\"^/private/tmp/OSL_PIPE_[0-9]+_SingleOfficeIPC_[0-9a-f]+$\")))\n\
(allow mach-lookup (global-name \"com.apple.coreservices.launchservicesd\"))\n";

/// Stat-only, so tools can walk down to what they are allowed to read. The
/// folders themselves stay unlistable: `/usr/local` and Homebrew's `var` hold
/// other programs' data.
const WAYPOINTS: &[&str] = &[
    "/",
    "/private",
    "/var",
    "/tmp",
    "/opt",
    "/etc",
    "/usr",
    "/Library",
    "/Library/Developer",
];

/// Whether a room can reach anything at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    Closed,
    /// One local port, where the proxy listens. The proxy decides the rest.
    Proxy(u16),
}

/// How much of the machine a room sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sight {
    /// The programs on this Mac: Homebrew, the command-line tools, `tools/`.
    Machine,
    /// The system volume and its own folder. For a room with a network.
    Bare,
}

/// What a room is allowed to see of the machine.
#[derive(Clone, Debug)]
pub struct Profile {
    /// The room's own folder, the only place it can write.
    pub room: PathBuf,
    /// Read-only tools, shared between rooms.
    pub tools: PathBuf,
    pub homebrew: PathBuf,
    pub reach: Reach,
    pub sight: Sight,
    /// Folders the person opened (`super::doors`), read-only. Only a room that
    /// sees the machine gets them; a room with a network never does.
    pub doors: Vec<PathBuf>,
    /// Refuses the computer's name, which usually holds the person's (design
    /// §7.2, A33). Only for the helper's room: `os.uname()` fails whole without
    /// it, and `uv` with it, so the room that installs packages, which holds no
    /// model, keeps it (measured 2026-11-05). See `super::outside`.
    pub nameless: bool,
    /// Application bundles a room may read and run, nothing else of the folder
    /// they are in: LibreOffice for Utilities (`utilities::room`). Like Homebrew's
    /// folders, these hold programs, not anybody's data. Only a sealed room that
    /// sees the machine gets them.
    pub programs: Vec<PathBuf>,
    /// What LibreOffice needs besides its bundle, measured on macOS 26.5 by
    /// taking each away in turn (2026-11-12): its single-instance pipe, which it
    /// puts in `/tmp` because a room's own temporary folder is too long a path
    /// for a socket; and LaunchServices, without which it aborts. Nothing else:
    /// no network, no other service, no file outside the room.
    pub office: bool,
    /// Replaces the rules, so a test can prove a broken profile is noticed.
    #[cfg(test)]
    pub rules: Option<String>,
}

impl Profile {
    pub fn new(room: PathBuf, tools: PathBuf, reach: Reach) -> Self {
        Self {
            room,
            tools,
            homebrew: homebrew(),
            reach,
            sight: Sight::Machine,
            doors: Vec::new(),
            nameless: false,
            programs: Vec::new(),
            office: false,
            #[cfg(test)]
            rules: None,
        }
    }

    /// Arguments for `sandbox-exec`, everything before the command.
    pub fn args(&self) -> Vec<OsString> {
        let above: Vec<&Path> = self.room.ancestors().skip(1).collect();
        let mut args: Vec<OsString> = vec!["-p".into(), self.text(above.len()).into()];
        let mut define = |name: &str, value: &OsStr| {
            let mut pair = OsString::from(format!("{name}="));
            pair.push(value);
            args.push("-D".into());
            args.push(pair);
        };
        define("WS", self.room.as_os_str());
        define("TOOLS", self.tools.as_os_str());
        define("HB", self.homebrew.as_os_str());
        let port = match self.reach {
            Reach::Proxy(port) => port,
            // Unused by the closed rules, and `(param)` of nothing is an error.
            Reach::Closed => 1,
        };
        define("PROXY", OsStr::new(&port.to_string()));
        for (i, folder) in above.iter().enumerate() {
            define(&format!("UP{i}"), folder.as_os_str());
        }
        let (doors, door_steps) = self.doors();
        for (i, door) in doors.iter().enumerate() {
            define(&format!("DOOR{i}"), door.as_os_str());
        }
        for (i, step) in door_steps.iter().enumerate() {
            define(&format!("DUP{i}"), step.as_os_str());
        }
        let (programs, program_steps) = self.programs();
        for (i, program) in programs.iter().enumerate() {
            define(&format!("PROG{i}"), program.as_os_str());
        }
        for (i, step) in program_steps.iter().enumerate() {
            define(&format!("PUP{i}"), step.as_os_str());
        }
        args
    }

    /// The bundles this room runs, and every folder above them, once each.
    fn programs(&self) -> (Vec<&Path>, Vec<&Path>) {
        if self.sight != Sight::Machine || matches!(self.reach, Reach::Proxy(_)) {
            return (Vec::new(), Vec::new());
        }
        let programs: Vec<&Path> = self.programs.iter().map(PathBuf::as_path).filter(|p| usable(p)).collect();
        let mut steps: Vec<&Path> = Vec::new();
        for step in programs.iter().flat_map(|p| p.ancestors().skip(1)) {
            if !steps.contains(&step) && !WAYPOINTS.contains(&step.to_str().unwrap_or("")) {
                steps.push(step);
            }
        }
        (programs, steps)
    }

    /// The doors this room reads, and every folder above them, once each, for a
    /// program to stat on its way down.
    fn doors(&self) -> (Vec<&Path>, Vec<&Path>) {
        if self.sight != Sight::Machine {
            return (Vec::new(), Vec::new());
        }
        let doors: Vec<&Path> = self.doors.iter().map(PathBuf::as_path).filter(|d| usable(d)).collect();
        let mut steps: Vec<&Path> = Vec::new();
        for step in doors.iter().flat_map(|d| d.ancestors().skip(1)) {
            if !steps.contains(&step) {
                steps.push(step);
            }
        }
        (doors, steps)
    }

    fn text(&self, folders_above: usize) -> String {
        #[cfg(test)]
        if let Some(rules) = &self.rules {
            return rules.clone();
        }
        let mut text = String::from(match self.sight {
            Sight::Machine => CLOSED,
            Sight::Bare => BARE,
        });
        text.push_str("(allow file-read-metadata file-test-existence\n");
        for waypoint in WAYPOINTS {
            text.push_str(&format!("  (literal \"{waypoint}\")\n"));
        }
        if self.sight == Sight::Machine {
            text.push_str("  (literal (param \"HB\"))\n");
        }
        for i in 0..folders_above {
            text.push_str(&format!("  (literal (param \"UP{i}\"))\n"));
        }
        text.push_str(")\n");
        if matches!(self.reach, Reach::Proxy(_)) {
            text.push_str(NET);
        }
        if self.nameless {
            text.push_str("(deny sysctl-read (sysctl-name \"kern.hostname\"))\n");
            // The login name, which the kernel keeps for the session the app was
            // opened in: `logname` says it whatever the directory service
            // refuses. A test run from a session without one never sees it.
            text.push_str("(deny syscall-unix (syscall-number SYS_getlogin))\n");
        }
        let (programs, program_steps) = self.programs();
        if !programs.is_empty() {
            text.push_str("(allow file-read* file-test-existence file-map-executable\n");
            for i in 0..programs.len() {
                text.push_str(&format!("  (subpath (param \"PROG{i}\"))\n"));
            }
            text.push_str(")\n(allow file-read-metadata file-test-existence\n");
            for i in 0..program_steps.len() {
                text.push_str(&format!("  (literal (param \"PUP{i}\"))\n"));
            }
            text.push_str(")\n");
            if self.office {
                text.push_str(OFFICE);
            }
        }
        let (doors, steps) = self.doors();
        if !doors.is_empty() {
            let each = |name: &str, count: usize, rule: &str| -> String {
                (0..count).map(|i| format!("  ({rule} (param \"{name}{i}\"))\n")).collect()
            };
            text.push_str("(allow file-read* file-test-existence\n");
            text.push_str(&each("DOOR", doors.len(), "subpath"));
            text.push_str(")\n(allow file-read-metadata file-test-existence\n");
            text.push_str(&each("DUP", steps.len(), "literal"));
            // Last, so it wins over the door it narrows. Never a boundary: a key
            // can be in any file (design §8).
            text.push_str(")\n(deny file-read* file-test-existence\n  (require-all\n    (require-any\n");
            text.push_str(&each("DOOR", doors.len(), "subpath").replace("  (", "      ("));
            text.push_str("    )\n    (require-any\n");
            for regex in super::doors::backstop_regexes() {
                text.push_str(&format!("      (regex #\"{regex}\")\n"));
            }
            text.push_str("    )))\n");
        }
        text
    }
}

/// Where Homebrew lives, if it does. The rules name its folders either way, and
/// a folder that is not there is simply never matched.
fn homebrew() -> PathBuf {
    let intel = Path::new("/usr/local/bin/brew");
    let silicon = Path::new("/opt/homebrew/bin/brew");
    if !silicon.exists() && intel.exists() {
        PathBuf::from("/usr/local")
    } else {
        PathBuf::from("/opt/homebrew")
    }
}

/// Rejects what cannot be a parameter. `-D` splits nowhere after the first `=`,
/// but a NUL ends the argument at exec.
pub fn usable(path: &Path) -> bool {
    path.is_absolute() && !path.as_os_str().as_bytes().contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_read_and_run_and_nothing_else_is_granted_without_asking() {
        let mut p = Profile::new("/tmp/room".into(), "/tmp/tools".into(), Reach::Closed);
        assert!(!p.text(0).contains("file-map-executable\n  (subpath (param \"PROG"));
        p.programs = vec!["/Applications/LibreOffice.app".into()];
        let text = p.text(0);
        assert!(text.contains("(subpath (param \"PROG0\"))"));
        assert!(!text.contains("OSL_PIPE"), "the pipe only when asked for");
        p.office = true;
        assert!(p.text(0).contains("launchservicesd"));
        // A room with a network never runs programs this way.
        p.reach = Reach::Proxy(1);
        assert!(!p.text(0).contains("PROG0") && !p.text(0).contains("OSL_PIPE"));
    }

    fn profile(room: &str) -> Profile {
        Profile::new(room.into(), "/t".into(), Reach::Closed)
    }

    #[test]
    fn a_path_never_reaches_the_text_of_the_rules() {
        let hostile = "/tmp/a\") (allow default) (literal \"b/room";
        let args = profile(hostile).args();
        let text = args[1].to_str().unwrap();
        assert!(!text.contains("allow default"), "{text}");
        assert!(args
            .iter()
            .any(|a| a.to_str() == Some(&format!("WS={hostile}"))));
    }

    #[test]
    fn every_folder_above_the_room_can_be_walked_through() {
        let args = profile("/a/b/room").args();
        for (name, folder) in [("UP0", "/a/b"), ("UP1", "/a"), ("UP2", "/")] {
            assert!(args
                .iter()
                .any(|a| a.to_str() == Some(&format!("{name}={folder}"))));
            assert!(args[1]
                .to_str()
                .unwrap()
                .contains(&format!("(param \"{name}\")")));
        }
    }

    #[test]
    fn only_the_helpers_room_is_refused_the_computers_name() {
        let mut room = profile("/a/room");
        assert!(!room.args()[1].to_str().unwrap().contains("kern.hostname"));
        assert!(!room.args()[1].to_str().unwrap().contains("SYS_getlogin"));
        room.nameless = true;
        assert!(room.args()[1].to_str().unwrap().contains("(deny sysctl-read (sysctl-name \"kern.hostname\"))"));
        assert!(room.args()[1].to_str().unwrap().contains("(deny syscall-unix (syscall-number SYS_getlogin))"));
    }

    #[test]
    fn only_a_room_with_a_proxy_has_any_network_rule() {
        let closed = profile("/a/room").args();
        assert!(!closed[1].to_str().unwrap().contains("network"));
        let mut net = profile("/a/room");
        net.reach = Reach::Proxy(18080);
        let args = net.args();
        assert!(args[1].to_str().unwrap().contains("network-outbound"));
        assert!(args.iter().any(|a| a.to_str() == Some("PROXY=18080")));
    }
}
