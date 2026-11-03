//! `machine_status`: how this Mac is doing. Battery, disk, memory, load, and what
//! is taking the memory. Nothing here can be changed from here, and none of it
//! says what is in a file.

use std::ffi::{CStr, CString};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use super::sanitize;
use super::watch;

const GB: f64 = (1u64 << 30) as f64;
/// How many processes are named. A name says what somebody is doing, so only what
/// answers "why is it slow".
const NAMED: usize = 5;

fn gb(bytes: u64) -> f64 {
    (bytes as f64 / GB * 10.0).round() / 10.0
}

fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program)
        .args(args)
        .env_clear()
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `pmset -g batt`. None on a Mac with no battery.
fn parse_battery(text: &str) -> Option<Value> {
    let line = text.lines().find(|line| line.contains('%'))?;
    let percent: u64 = line
        .split('%')
        .next()?
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    let state = line.split(';').nth(1).map_or("", str::trim);
    Some(json!({
        "percent": percent,
        "state": state,
        "on_power": text.contains("'AC Power'"),
    }))
}

/// Used memory as Activity Monitor adds it up, from `vm_stat`: what apps hold,
/// what cannot be paged out, and what is compressed.
fn parse_memory_used(text: &str) -> Option<u64> {
    let page: u64 = text
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |label: &str| -> Option<u64> {
        text.lines()
            .find(|line| line.starts_with(label))?
            .split(':')
            .nth(1)?
            .trim()
            .trim_end_matches('.')
            .parse()
            .ok()
    };
    let used = pages("Pages active")?
        + pages("Pages wired down")?
        + pages("Pages occupied by compressor")?;
    Some(used * page)
}

fn sysctl_u64(name: &CStr) -> Option<u64> {
    let mut value: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: `value` is valid for `size` bytes, and no new value is being set.
    let got = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&raw mut value).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (got == 0).then_some(value)
}

fn disk(path: &str) -> Option<Value> {
    let path = CString::new(path).ok()?;
    // SAFETY: `info` is plain data, and `statfs` fills it for a valid path string.
    let info = unsafe {
        let mut info: libc::statfs = std::mem::zeroed();
        (libc::statfs(path.as_ptr(), &mut info) == 0).then_some(info)
    }?;
    let block = u64::from(info.f_bsize);
    Some(json!({
        "free_gb": gb(info.f_bavail * block),
        "total_gb": gb(info.f_blocks * block),
    }))
}

fn load() -> Option<Value> {
    let mut loads = [0f64; 3];
    // SAFETY: `loads` holds the three values asked for.
    let got = unsafe { libc::getloadavg(loads.as_mut_ptr(), 3) };
    (got == 3).then(|| json!({ "one_minute": (loads[0] * 100.0).round() / 100.0 }))
}

/// What the eval has this Mac say, so that a real machine's battery and process
/// names are not sent to a model to grade an answer, and the answer can be checked.
#[cfg(any(test, feature = "eval"))]
pub static PRETEND: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

pub fn run() -> Value {
    #[cfg(any(test, feature = "eval"))]
    if let Some(said) = PRETEND.lock().ok().and_then(|pretend| pretend.clone()) {
        return said;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let (count, heaviest) = watch::heaviest_processes(NAMED).unwrap_or_default();
    let heaviest: Vec<Value> = heaviest
        .into_iter()
        .map(|(name, bytes)| json!({ "name": sanitize::shown(&name), "memory_gb": gb(bytes) }))
        .collect();
    json!({
        "battery": output("/usr/bin/pmset", &["-g", "batt"]).as_deref().and_then(parse_battery),
        "disk": disk(&home),
        "memory": {
            "total_gb": sysctl_u64(c"hw.memsize").map(gb),
            "used_gb": output("/usr/bin/vm_stat", &[]).as_deref().and_then(parse_memory_used).map(gb),
        },
        "cpu": {
            "cores": sysctl_u64(c"hw.ncpu").or_else(|| {
                let mut value: i32 = 0;
                let mut size = std::mem::size_of::<i32>();
                // SAFETY: as in `sysctl_u64`, for the 32-bit value this one is.
                let got = unsafe {
                    libc::sysctlbyname(
                        c"hw.ncpu".as_ptr(),
                        (&raw mut value).cast(),
                        &mut size,
                        std::ptr::null_mut(),
                        0,
                    )
                };
                (got == 0).then_some(value as u64)
            }),
            "load": load(),
        },
        "processes": { "count": count, "using_most_memory": heaviest },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_battery_is_read_in_each_state() {
        let on_battery = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t64%; discharging; 3:21 remaining present: true\n";
        let got = parse_battery(on_battery).unwrap();
        assert_eq!(got["percent"], 64);
        assert_eq!(got["state"], "discharging");
        assert_eq!(got["on_power"], false);

        let charged = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t100%; charged; 0:00 remaining present: true\n";
        let got = parse_battery(charged).unwrap();
        assert_eq!(got["percent"], 100);
        assert_eq!(got["on_power"], true);
    }

    #[test]
    fn a_mac_with_no_battery_says_so_by_having_none() {
        assert_eq!(parse_battery("Now drawing from 'AC Power'\n"), None);
    }

    #[test]
    fn memory_in_use_is_what_activity_monitor_counts() {
        let text = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:     100.\nPages active:   10.\nPages inactive:  50.\nPages wired down:   5.\nPages occupied by compressor:   2.\n";
        assert_eq!(parse_memory_used(text), Some(17 * 16384));
        assert_eq!(parse_memory_used("nonsense"), None);
    }

    /// On this machine, whatever it is. Every number has to have come back, except
    /// the battery, which a desktop or a CI machine does not have.
    #[test]
    fn this_machine_answers() {
        let got = run();
        assert!(got["disk"]["total_gb"].as_f64().unwrap() > 0.0, "{got}");
        assert!(got["disk"]["free_gb"].as_f64().unwrap() >= 0.0, "{got}");
        assert!(got["memory"]["total_gb"].as_f64().unwrap() > 0.0, "{got}");
        assert!(got["memory"]["used_gb"].as_f64().unwrap() > 0.0, "{got}");
        assert!(got["cpu"]["cores"].as_u64().unwrap() > 0, "{got}");
        assert!(got["processes"]["count"].as_u64().unwrap() > 0, "{got}");
        let named = got["processes"]["using_most_memory"].as_array().unwrap();
        assert!(!named.is_empty() && named.len() <= NAMED, "{got}");
    }
}
