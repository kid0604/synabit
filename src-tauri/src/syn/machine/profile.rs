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
const NET: &str = include_str!("net.sb");

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

/// What a room is allowed to see of the machine.
#[derive(Clone, Debug)]
pub struct Profile {
    /// The room's own folder, the only place it can write.
    pub room: PathBuf,
    /// Read-only tools, shared between rooms.
    pub tools: PathBuf,
    pub homebrew: PathBuf,
    pub reach: Reach,
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
        args
    }

    fn text(&self, folders_above: usize) -> String {
        #[cfg(test)]
        if let Some(rules) = &self.rules {
            return rules.clone();
        }
        let mut text = String::from(CLOSED);
        text.push_str("(allow file-read-metadata file-test-existence\n");
        for waypoint in WAYPOINTS {
            text.push_str(&format!("  (literal \"{waypoint}\")\n"));
        }
        text.push_str("  (literal (param \"HB\"))\n");
        for i in 0..folders_above {
            text.push_str(&format!("  (literal (param \"UP{i}\"))\n"));
        }
        text.push_str(")\n");
        if matches!(self.reach, Reach::Proxy(_)) {
            text.push_str(NET);
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
