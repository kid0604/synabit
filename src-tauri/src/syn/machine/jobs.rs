//! Long work in a room (design §5.1): a video that takes twenty minutes.
//!
//! A run has ten minutes of its own (`run::DEFAULT_WALL_MS`), so a job cannot be
//! a command the run waits on. `machine_start` starts it and returns; the job
//! outlives the call and the run, and `machine_poll` reads where it has got to,
//! waiting a while if asked. When it ends with nobody waiting, the person is told
//! (`Rooms::on_job_end`). Stopping it is `machine_stop`, the stop button while
//! Syn is waiting on it, the screen, the tray, and quitting.
//!
//! A job runs in the conversation's room, under the same profile and ceilings as
//! any command. Two things are its own:
//!
//! - **Nothing else runs in its room meanwhile.** A command ends by clearing the
//!   room it ran in, which would end the job too, and a file kept while the job
//!   writes it is not the file checked. Reading and writing files there goes on.
//! - **It is paused, not waited for, while a room with a network is open** (I8,
//!   design §5.8): every process in it is stopped by the kernel before the port
//!   opens, and goes on after it has closed. The screen and `machine_poll` say so.
//!
//! What the job prints is kept as a tail, outside the room. It is somebody
//! else's words exactly as a command's output is: `machine::tool` marks and
//! taints around it the same way.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt};

use super::cell::Cell;
use super::watch::Over;

/// Jobs running at once, on the whole Mac. *Provisional.*
pub const MOST_AT_ONCE: usize = 3;
/// Bytes of output kept, from the end.
const TAIL: usize = 64 << 10;
/// Jobs that have ended, kept for `machine_poll` to read.
const KEPT_ENDED: usize = 16;

/// How a job ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum Ended {
    /// None when a signal ended it.
    Exited { code: Option<i32> },
    TimedOut,
    Over { over: Over },
    /// Somebody stopped it: the model, the person, or the app on its way out.
    Stopped,
    Failed { why: String },
}

pub struct Job {
    pub id: String,
    /// The room's name.
    pub room: String,
    /// The conversation it was started in, or the run when there is none.
    pub scope: String,
    /// As the model wrote it. Shown only through `sanitize`.
    pub command: String,
    pub started: chrono::DateTime<chrono::Utc>,
    pub(super) cell: Cell,
    tail: Mutex<Vec<u8>>,
    end: Mutex<Option<(Ended, chrono::DateTime<chrono::Utc>)>>,
    ended: tokio::sync::Notify,
    paused: AtomicBool,
    pub(super) stopping: AtomicBool,
    /// Polls waiting on it right now: somebody will see it end.
    waiting: AtomicUsize,
}

impl Job {
    pub(super) fn new(room: &str, scope: &str, command: &str, cell: Cell) -> Self {
        Self {
            id: format!("j{:08x}", rand::random::<u32>()),
            room: room.to_string(),
            scope: scope.to_string(),
            command: command.to_string(),
            started: chrono::Utc::now(),
            cell,
            tail: Mutex::new(Vec::new()),
            end: Mutex::new(None),
            ended: tokio::sync::Notify::new(),
            paused: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            waiting: AtomicUsize::new(0),
        }
    }

    pub fn ended(&self) -> Option<Ended> {
        self.end.lock().ok().and_then(|end| end.as_ref().map(|(how, _)| how.clone()))
    }

    pub fn running(&self) -> bool {
        self.ended().is_none()
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub(super) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    /// Whether a poll was waiting on it when it ended.
    pub fn watched(&self) -> bool {
        self.waiting.load(Ordering::SeqCst) > 0
    }

    pub fn seconds(&self) -> i64 {
        let until = self.end.lock().ok().and_then(|end| end.as_ref().map(|(_, at)| *at)).unwrap_or_else(chrono::Utc::now);
        (until - self.started).num_seconds().max(0)
    }

    pub(super) fn finish(&self, how: Ended) {
        if let Ok(mut end) = self.end.lock() {
            if end.is_none() {
                *end = Some((how, chrono::Utc::now()));
            }
        }
        self.ended.notify_waiters();
    }

    /// Waits until it ends, or `wait` passes, or `stop` says so. True when it
    /// has ended.
    pub async fn wait(&self, wait: std::time::Duration, stop: &(dyn Fn() -> bool + Sync)) -> bool {
        self.waiting.fetch_add(1, Ordering::SeqCst);
        let deadline = tokio::time::Instant::now() + wait;
        let done = loop {
            if !self.running() {
                break true;
            }
            if stop() || tokio::time::Instant::now() >= deadline {
                break false;
            }
            let _ = tokio::time::timeout(std::time::Duration::from_millis(250), self.ended.notified()).await;
        };
        self.waiting.fetch_sub(1, Ordering::SeqCst);
        done
    }

    /// What it has printed, from the end.
    pub fn tail_text(&self) -> String {
        let bytes = self.tail.lock().map(|t| t.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// The tail as lines. A progress meter rewrites one line with `\r`, and
    /// only its last state is worth reading.
    pub fn last_lines(&self, most: usize) -> String {
        let text = self.tail_text();
        let lines: Vec<&str> = text.split(['\n', '\r']).map(str::trim_end).filter(|l| !l.is_empty()).collect();
        lines[lines.len().saturating_sub(most)..].join("\n")
    }

    /// Where it has got to, as its own output says. A hint read from what it
    /// prints, never a measurement.
    pub fn progress(&self) -> Option<Value> {
        progress(&self.tail_text())
    }

    fn keep(&self, chunk: &[u8]) {
        if let Ok(mut tail) = self.tail.lock() {
            tail.extend_from_slice(chunk);
            if tail.len() > TAIL {
                let cut = tail.len() - TAIL;
                tail.drain(..cut);
            }
        }
    }
}

/// Reads a pipe into the job's tail until it closes.
pub(super) fn read_into(pipe: Option<impl AsyncRead + Unpin + Send + 'static>, job: Arc<Job>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(mut pipe) = pipe else { return };
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk).await {
            if n == 0 {
                return;
            }
            job.keep(&chunk[..n]);
        }
    })
}

/// The last percentage the output gave, or ffmpeg's last `time=`, whichever
/// it said last. Only while it runs: x264 ends with a summary full of
/// percentages that are not how far it got.
fn progress(text: &str) -> Option<Value> {
    match (last_percent(text), last_ffmpeg_time(text)) {
        (Some((at, _)), Some((after, shown, seconds))) if after > at => Some(json!({ "time": shown, "seconds_done": seconds })),
        (Some((_, percent)), _) => Some(json!({ "percent": percent })),
        (None, Some((_, shown, seconds))) => Some(json!({ "time": shown, "seconds_done": seconds })),
        (None, None) => None,
    }
}

fn last_percent(text: &str) -> Option<(usize, f64)> {
    let bytes = text.as_bytes();
    let mut at = text.len();
    while let Some(found) = text[..at].rfind('%') {
        let mut start = found;
        while start > 0 && (bytes[start - 1].is_ascii_digit() || bytes[start - 1] == b'.') {
            start -= 1;
        }
        if let Ok(value) = text[start..found].parse::<f64>() {
            if (0.0..=100.0).contains(&value) {
                return Some((found, value));
            }
        }
        at = found;
    }
    None
}

/// `time=00:01:23.45`, as ffmpeg prints it, in seconds too.
fn last_ffmpeg_time(text: &str) -> Option<(usize, String, f64)> {
    let mut at = text.len();
    while let Some(found) = text[..at].rfind("time=") {
        let rest = &text[found + 5..];
        let shown: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == ':' || *c == '.').collect();
        let parts: Vec<&str> = shown.split(':').collect();
        if let [h, m, s] = parts.as_slice() {
            if let (Ok(h), Ok(m), Ok(s)) = (h.parse::<f64>(), m.parse::<f64>(), s.parse::<f64>()) {
                return Some((found, shown.clone(), ((h * 3600.0 + m * 60.0 + s) * 100.0).round() / 100.0));
            }
        }
        at = found;
    }
    None
}

/// Every job this launch has, running or lately ended.
#[derive(Default)]
pub struct Jobs {
    all: Mutex<Vec<Arc<Job>>>,
}

impl Jobs {
    pub(super) fn add(&self, job: Arc<Job>) {
        if let Ok(mut all) = self.all.lock() {
            all.push(job);
            // The oldest that have ended go first; a running job is never dropped.
            while all.iter().filter(|j| !j.running()).count() > KEPT_ENDED {
                if let Some(at) = all.iter().position(|j| !j.running()) {
                    all.remove(at);
                }
            }
        }
    }

    pub fn find(&self, id: &str) -> Option<Arc<Job>> {
        self.all.lock().ok()?.iter().find(|j| j.id == id).cloned()
    }

    pub fn running(&self) -> Vec<Arc<Job>> {
        self.all.lock().map(|all| all.iter().filter(|j| j.running()).cloned().collect()).unwrap_or_default()
    }

    pub fn running_in(&self, room: &str) -> Option<Arc<Job>> {
        self.running().into_iter().find(|j| j.room == room)
    }
}

/// A job, as the screen lists it.
#[derive(Clone, Debug, Serialize)]
pub struct JobView {
    pub id: String,
    /// Its first line, made safe to show and cut short.
    pub command: String,
    pub seconds: i64,
    pub paused: bool,
    pub progress: Option<Value>,
}

impl From<&Job> for JobView {
    fn from(job: &Job) -> Self {
        let first: String = job.command.lines().next().unwrap_or_default().chars().take(120).collect();
        Self {
            id: job.id.clone(),
            command: super::sanitize::shown(&first),
            seconds: job.seconds(),
            paused: job.paused(),
            progress: job.progress(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_read_from_the_last_thing_it_said() {
        let ffmpeg = "frame=  100 fps= 50 time=00:00:04.00 bitrate=1\rframe=  900 fps= 50 time=00:01:23.45 bitrate=1 speed=2.1x\r";
        assert_eq!(progress(ffmpeg), Some(json!({ "time": "00:01:23.45", "seconds_done": 83.45 })));
        assert_eq!(progress("10% done\n47.5% done\n"), Some(json!({ "percent": 47.5 })));
        assert_eq!(progress("disk 300% full, then 12%"), Some(json!({ "percent": 12.0 })));
        assert_eq!(progress("100%|█████| 5/5\n"), Some(json!({ "percent": 100.0 })));
        assert_eq!(progress("no numbers here; 50 % apart"), None);
        assert_eq!(progress("time=garbage"), None);
        // Whichever it said last.
        assert_eq!(progress("frame=1 time=00:00:01.00\r 40% of the files\n"), Some(json!({ "percent": 40.0 })));
        let then_time = "[libx264] 50.0%\nframe=1 time=00:00:01.00\r";
        assert_eq!(progress(then_time), Some(json!({ "time": "00:00:01.00", "seconds_done": 1.0 })));
    }
}
