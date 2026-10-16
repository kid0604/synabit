//! What must always hold, checked from the Rust side, and how a breach is
//! written down.
//!
//! The rules are in `contracts/invariants.json`, which the front end reads too
//! (`src/shared/invariants`). A breach is one log line, `[invariant] {json}`,
//! in the same log the front end writes to, so `npm run audit` finds every
//! one in one place.
//!
//! Two are checked here because only this side can see them:
//!
//! - `ui.page-alive`. A page whose thread never comes back cannot say so: on
//!   2026-10-15 a click on a conversation held the WebView at full CPU until
//!   it was killed, and the log said nothing at all — the page's own timer
//!   only reports a block once it ends. The page beats every couple of
//!   seconds (`page_alive`); a beat that stops while the window is visible is
//!   seen here, and written down with where the page was.
//! - `syn.claims-match-tools`. A Telegram turn never touches the page.

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

const CONTRACT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../contracts/invariants.json"));

static CATALOG: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(CONTRACT).expect("contracts/invariants.json reads"));

/// Every rule's id, as the contract names them.
pub fn ids() -> Vec<String> {
    CATALOG["invariants"]
        .as_array()
        .map(|rules| rules.iter().filter_map(|r| r["id"].as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Write a breach to the log. `subject` is what it happened to — a run, a
/// route — and `detail` is counts and names, never what anybody wrote.
pub fn report(id: &str, subject: Option<&str>, detail: Value) {
    debug_assert!(ids().iter().any(|known| known == id), "`{id}` is not in contracts/invariants.json");
    let mut line = serde_json::json!({ "invariant": id, "detail": detail });
    if let Some(subject) = subject {
        line["subject"] = Value::String(subject.to_string());
    }
    log::warn!("[invariant] {line}");
}

/// How much of an answer is read for a claim: where "Đã lưu" goes.
const CLAIM_WINDOW: usize = 300;

/// Whether an answer says it did something. The front end holds the same rule
/// over the same words (`claimsAction`).
pub fn claims_action(answer: &str) -> bool {
    // Before any code fence: words after a diagram describe the diagram.
    let before_code = answer.split("```").next().unwrap_or_default();
    let head: String = before_code.chars().take(CLAIM_WINDOW).collect::<String>().to_lowercase();
    let words = CATALOG["claims"]["words"].as_array().cloned().unwrap_or_default();
    words.iter().filter_map(Value::as_str).any(|word| {
        head.match_indices(word).any(|(at, _)| {
            head[..at].chars().next_back().is_none_or(|before| !before.is_alphanumeric())
        })
    })
}

// ── The page's heartbeat ───────────────────────────────────────────────────

/// How often the page beats.
pub const BEAT_EVERY: Duration = Duration::from_secs(2);
/// Silence that counts as a page not answering: three beats missed.
const STALLED_AFTER: Duration = Duration::from_secs(6);

#[derive(Default)]
pub struct Heartbeat {
    state: Mutex<Beat>,
}

#[derive(Default)]
struct Beat {
    last: Option<Instant>,
    route: String,
    visible: bool,
    /// When a stall was written down, so it is written once and its end once.
    stalled_since: Option<Instant>,
}

/// What a look at the heartbeat finds.
#[derive(Debug, PartialEq, Eq)]
pub enum Seen {
    Fine,
    /// Silent this long, and not reported yet.
    Stalled(Duration),
}

/// Whether a page last heard from `since_beat` ago has stopped answering. A
/// hidden window is not expected to beat; a stall already written down is not
/// written again.
pub fn look(since_beat: Option<Duration>, visible: bool, already_reported: bool) -> Seen {
    match since_beat {
        Some(quiet) if visible && !already_reported && quiet >= STALLED_AFTER => Seen::Stalled(quiet),
        _ => Seen::Fine,
    }
}

impl Heartbeat {
    /// A beat from the page. One after a stall closes it, with how long it was.
    pub fn beat(&self, route: String, visible: bool) {
        let mut b = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if b.stalled_since.take().is_some() {
            let silent = b.last.map(|t| now.duration_since(t).as_millis() as u64).unwrap_or_default();
            report("ui.page-alive", Some(&b.route), serde_json::json!({ "recovered_after_ms": silent }));
        }
        b.last = Some(now);
        b.route = route;
        b.visible = visible;
    }

    /// Look once; write down a stall the first time it is seen.
    pub fn check(&self) {
        let mut b = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let quiet = b.last.map(|t| t.elapsed());
        if let Seen::Stalled(quiet) = look(quiet, b.visible, b.stalled_since.is_some()) {
            b.stalled_since = Some(Instant::now());
            report("ui.page-alive", Some(&b.route), serde_json::json!({ "silent_ms": quiet.as_millis() as u64 }));
        }
    }
}

/// Watch the page's heartbeat for as long as the app runs.
pub fn watch<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(BEAT_EVERY).await;
            app.state::<Heartbeat>().check();
        }
    });
}

/// The page saying it is still there: where it is, and whether it can be seen.
#[tauri::command]
pub fn page_alive(heartbeat: tauri::State<'_, Heartbeat>, route: String, visible: bool) {
    heartbeat.beat(route, visible);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contract_reads_and_names_its_rules() {
        let ids = ids();
        for id in ["note.roundtrip", "ui.page-alive", "syn.claims-match-tools"] {
            assert!(ids.iter().any(|i| i == id), "{id}");
        }
    }

    /// The cases in the contract, which the front end's test reads too: the
    /// two sides judge the same answers the same way.
    #[test]
    fn a_claim_is_read_as_the_contract_says() {
        for case in CATALOG["claims"]["cases"].as_array().expect("cases") {
            let answer = case["answer"].as_str().expect("answer");
            assert_eq!(claims_action(answer), case["claims"].as_bool().expect("claims"), "{answer}");
        }
        let far = format!("{} đã lưu", "x".repeat(400));
        assert!(!claims_action(&far), "past the first 300 characters");
    }

    #[test]
    fn a_stall_is_seen_once_and_only_while_visible() {
        let long = Some(Duration::from_secs(8));
        assert_eq!(look(long, true, false), Seen::Stalled(Duration::from_secs(8)));
        assert_eq!(look(long, true, true), Seen::Fine, "written down already");
        assert_eq!(look(long, false, false), Seen::Fine, "a hidden window does not beat");
        assert_eq!(look(Some(Duration::from_secs(3)), true, false), Seen::Fine);
        assert_eq!(look(None, true, false), Seen::Fine, "no page has beaten yet");
    }
}
