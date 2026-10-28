//! A visual looked at as drawn, before it is kept (plan 2026-10-28, §2.3, PR Q4).
//!
//! `check` reads the HTML and knows what the frame refuses; it cannot know what
//! the script draws. `synabit.lint` in the kit can: it runs in the page, after
//! the visual has drawn, and finds the faults no question is needed to see —
//! a script that threw, NaN on screen, text over text, a claim made with
//! `synabit.expect` that does not hold. Here a draft is drawn where nobody sees
//! it and that lint is read back, so the model is told what is wrong while it
//! can still fix it, rather than the person meeting it.
//!
//! # Who looks
//!
//! The app manages a [`Looker`]:
//!
//! - [`Looker::Frame`], in the app: the page draws the draft in a hidden frame
//!   on `synvisual://…/draft/<token>`, the same origin, policy and sandbox as a
//!   kept visual, and sends what the lint said back with
//!   [`syn_visual_looked`]. The draft page runs the lint itself on load
//!   (`protocol`), so nothing is sent into the frame that a kept visual's frame
//!   is not sent.
//! - [`Looker::Script`], in the eval, where there is no webview: WebKit through
//!   `e2e/visualTrial.mjs`, the script the measurements use.
//!
//! With neither, or past [`WITHIN`], the visual is kept unlooked and the run is
//! told so. What the frame sends back is what a page the model wrote said
//! about itself: it decides only whether the model is asked to fix its own
//! visual, never what the visual may reach — that is `check` and the frame's
//! policy.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::store::Proof;

/// How long the app waits for a hidden frame to draw a draft and lint it. A
/// visual with a few controls takes about a second (each control is used and
/// given 120 ms to redraw); past this it is kept unlooked rather than hold the
/// answer.
pub const WITHIN: Duration = Duration::from_secs(4);

/// How long the eval's script may take: WebKit is started for each draft.
const SCRIPT_WITHIN: Duration = Duration::from_secs(90);

/// The event the app's page listens for to draw a draft.
pub const EVENT: &str = "syn-visual-look";

/// Who looks at a draft. See the module's doc.
pub enum Looker {
    Frame,
    Script { repo: std::path::PathBuf },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub check: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claim {
    pub name: String,
    pub ok: bool,
    #[serde(default)]
    pub actual: Value,
    #[serde(default)]
    pub expected: Value,
}

/// What the lint found, as the kit sends it (`synabit/lint`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Lint {
    #[serde(default)]
    pub problems: Vec<Problem>,
    #[serde(default)]
    pub expects: Vec<Claim>,
    #[serde(default)]
    pub reported: serde_json::Map<String, Value>,
}

impl Lint {
    /// What came from a page, cut to what the model needs to read: a page can
    /// say anything about itself, and as much of it as it likes.
    pub fn bounded(mut self) -> Lint {
        let cut = |s: &mut String, n: usize| {
            if s.chars().count() > n {
                *s = s.chars().take(n).collect();
            }
        };
        self.problems.truncate(30);
        for p in &mut self.problems {
            cut(&mut p.check, 30);
            cut(&mut p.detail, 300);
        }
        self.expects.truncate(30);
        for c in &mut self.expects {
            cut(&mut c.name, 80);
            for v in [&mut c.actual, &mut c.expected] {
                if !(v.is_number() || v.is_boolean() || v.as_str().is_some_and(|s| s.len() <= 80)) {
                    *v = Value::Null;
                }
            }
        }
        self.reported = self
            .reported
            .into_iter()
            .filter(|(k, v)| k.chars().count() <= 80 && (v.is_number() || v.as_str().is_some_and(|s| s.chars().count() <= 80)))
            .take(50)
            .collect();
        self
    }
}

/// What looking at a draft came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Looked {
    Seen(Lint),
    /// Not looked at, and why.
    Not(String),
}

/// A draft waiting to be drawn, by its token.
struct Draft {
    html: String,
    proof: Vec<Proof>,
}

fn drafts() -> &'static Mutex<HashMap<String, Draft>> {
    static DRAFTS: OnceLock<Mutex<HashMap<String, Draft>>> = OnceLock::new();
    DRAFTS.get_or_init(Default::default)
}

fn waiting() -> &'static Mutex<HashMap<String, tokio::sync::oneshot::Sender<Lint>>> {
    static WAITING: OnceLock<Mutex<HashMap<String, tokio::sync::oneshot::Sender<Lint>>>> = OnceLock::new();
    WAITING.get_or_init(Default::default)
}

/// The draft page for `token`, for `protocol`: the fragment, then the kit told
/// to lint it once it has drawn — every control used, and the brief's proof
/// read against what the visual reports.
pub fn draft_page(token: &str) -> Option<String> {
    let drafts = drafts().lock().ok()?;
    let draft = drafts.get(token)?;
    let proof = serde_json::to_string(&draft.proof).unwrap_or_else(|_| "[]".into());
    Some(format!(
        "{}\n<script>synabit.ready.then(() => synabit.lint({{ controls: true, proof: {proof} }}));</script>",
        draft.html
    ))
}

/// A token is 32 lower-case hex digits, as a visual's id is.
pub fn is_token(token: &str) -> bool {
    super::store::is_id(token)
}

/// What the app's page sends back once the hidden frame linted a draft.
#[tauri::command]
pub fn syn_visual_looked(token: String, lint: Lint) {
    if let Some(waiting) = waiting().lock().ok().and_then(|mut w| w.remove(&token)) {
        let _ = waiting.send(lint.bounded());
    }
}

/// Look at `html` as drawn, with the brief's `proof`.
pub async fn look<R: tauri::Runtime>(app: &tauri::AppHandle<R>, html: &str, proof: &[Proof]) -> Looked {
    use tauri::Manager;
    let Some(looker) = app.try_state::<Looker>() else {
        return Looked::Not("there is nothing here to draw it in".into());
    };
    match looker.inner() {
        Looker::Frame => in_frame(app, html, proof).await,
        Looker::Script { repo } => by_script(repo, html, proof).await,
    }
}

async fn in_frame<R: tauri::Runtime>(app: &tauri::AppHandle<R>, html: &str, proof: &[Proof]) -> Looked {
    use tauri::Emitter;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    if let (Ok(mut drafts), Ok(mut waiting)) = (drafts().lock(), waiting().lock()) {
        drafts.insert(token.clone(), Draft { html: html.to_string(), proof: proof.to_vec() });
        waiting.insert(token.clone(), tx);
    } else {
        return Looked::Not("the draft could not be held".into());
    }
    let started = std::time::Instant::now();
    let looked = match app.emit(EVENT, serde_json::json!({ "token": token })) {
        Err(e) => Looked::Not(format!("the page was not reached: {e}")),
        Ok(()) => match tokio::time::timeout(WITHIN, rx).await {
            Ok(Ok(lint)) => Looked::Seen(lint),
            Ok(Err(_)) => Looked::Not("the page let it go".into()),
            Err(_) => Looked::Not(format!("it was not drawn within {} s", WITHIN.as_secs())),
        },
    };
    if let Ok(mut drafts) = drafts().lock() {
        drafts.remove(&token);
    }
    if let Ok(mut waiting) = waiting().lock() {
        waiting.remove(&token);
    }
    // What looking costs the answer it is part of (gate item 6: under 1.5 s).
    match &looked {
        Looked::Seen(lint) => log::info!("[syn-visual] looked at a draft in {} ms: {} problem(s)", started.elapsed().as_millis(), lint.problems.len()),
        Looked::Not(why) => log::info!("[syn-visual] a draft was not looked at after {} ms: {why}", started.elapsed().as_millis()),
    }
    looked
}

/// Debug builds: with `SYN_VISUAL_LOOK_TRIAL` naming a folder of fragments,
/// look at each in the running app's hidden frame, write what each took and
/// found to `SYN_VISUAL_LOOK_OUT`, and quit. The real path end to end — the
/// webview, `SynVisualLooker.vue`, `synvisual://…/draft/` — timed, which no
/// test without the app can do.
#[cfg(debug_assertions)]
pub fn trial_on_start<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let Ok(folder) = std::env::var("SYN_VISUAL_LOOK_TRIAL") else { return };
    let out = std::env::var("SYN_VISUAL_LOOK_OUT").unwrap_or_else(|_| format!("{folder}/looked.json"));
    tauri::async_runtime::spawn(async move {
        let mut pages: Vec<std::path::PathBuf> = std::fs::read_dir(&folder)
            .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "html")).collect())
            .unwrap_or_default();
        pages.sort();
        // The page loads, and its looker starts listening, some seconds in.
        let mut ready = false;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if matches!(in_frame(&app, "<p>ready</p>", &[]).await, Looked::Seen(_)) {
                ready = true;
                break;
            }
        }
        let mut results = Vec::new();
        for page in pages.iter().filter(|_| ready) {
            let html = std::fs::read_to_string(page).unwrap_or_default();
            for round in 1..=3 {
                let started = std::time::Instant::now();
                let looked = in_frame(&app, &html, &[]).await;
                let ms = started.elapsed().as_millis();
                let (seen, problems) = match &looked {
                    Looked::Seen(lint) => (true, lint.problems.iter().map(|p| p.check.clone()).collect::<Vec<_>>()),
                    Looked::Not(why) => (false, vec![why.clone()]),
                };
                results.push(serde_json::json!({ "page": page.file_name().map(|n| n.to_string_lossy().to_string()), "round": round, "ms": ms, "seen": seen, "problems": problems }));
            }
        }
        let _ = std::fs::write(&out, serde_json::to_string_pretty(&serde_json::json!({ "ready": ready, "results": results })).unwrap_or_default());
        app.exit(0);
    });
}

#[cfg(desktop)]
async fn by_script(repo: &std::path::Path, html: &str, proof: &[Proof]) -> Looked {
    // A folder of its own under the system's temporary one, removed after:
    // `tempfile` is a crate for tests only, and this is compiled into the app.
    let dir = std::env::temp_dir().join(format!("synabit-look-{}", uuid::Uuid::new_v4().simple()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Looked::Not(format!("no folder to draw it in: {e}"));
    }
    struct Gone(std::path::PathBuf);
    impl Drop for Gone {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _gone = Gone(dir.clone());
    let page = dir.join("draft.html");
    let out = dir.join("lint.json");
    if std::fs::write(&page, super::assets::standalone(html, "light")).is_err() {
        return Looked::Not("the draft could not be written".into());
    }
    let proof = serde_json::to_string(proof).unwrap_or_else(|_| "[]".into());
    let run = tokio::process::Command::new("node")
        .current_dir(repo)
        .arg("e2e/visualTrial.mjs")
        .arg("--json")
        .arg(&out)
        .arg("--proof")
        .arg(&proof)
        .arg(&page)
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(SCRIPT_WITHIN, run).await {
        Err(_) => return Looked::Not("WebKit did not finish".into()),
        Ok(Err(e)) => return Looked::Not(format!("WebKit could not be started: {e}")),
        Ok(Ok(_)) => {}
    }
    let results: Vec<Lint> = std::fs::read_to_string(&out).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    match results.into_iter().next() {
        Some(lint) => Looked::Seen(lint.bounded()),
        None => Looked::Not("WebKit said nothing".into()),
    }
}

#[cfg(not(desktop))]
async fn by_script(_repo: &std::path::Path, _html: &str, _proof: &[Proof]) -> Looked {
    Looked::Not("no WebKit here".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page says what it likes about itself; what reaches the model is cut
    /// to size, and only numbers and short strings are kept of what it reports.
    #[test]
    fn what_a_page_says_is_bounded() {
        let lint: Lint = serde_json::from_value(serde_json::json!({
            "problems": (0..100).map(|i| serde_json::json!({ "check": "overlap", "detail": "x".repeat(1000 + i) })).collect::<Vec<_>>(),
            "expects": [{ "name": "price", "ok": false, "actual": { "a": 1 }, "expected": 24.4 }],
            "reported": { "price": 25.2, "label": "y".repeat(500), "list": [1, 2] },
        }))
        .expect("lint");
        let lint = lint.bounded();
        assert_eq!(lint.problems.len(), 30);
        assert!(lint.problems.iter().all(|p| p.detail.chars().count() == 300));
        assert_eq!(lint.expects[0].actual, Value::Null);
        assert_eq!(lint.expects[0].expected, serde_json::json!(24.4));
        assert_eq!(lint.reported.keys().collect::<Vec<_>>(), vec!["price"]);
    }

    /// The draft page lints itself once drawn, with the brief's proof.
    #[test]
    fn a_draft_page_lints_itself_with_its_proof() {
        let token = uuid::Uuid::new_v4().simple().to_string();
        drafts().lock().unwrap().insert(
            token.clone(),
            Draft { html: "<p>x</p>".into(), proof: vec![Proof { name: "price".into(), value: 25.2 }] },
        );
        let page = draft_page(&token).expect("page");
        assert!(page.starts_with("<p>x</p>"));
        assert!(page.contains("synabit.lint({ controls: true, proof: [{\"name\":\"price\",\"value\":25.2}] })"), "{page}");
        drafts().lock().unwrap().remove(&token);
        assert!(draft_page(&token).is_none(), "gone once looked at");
    }

    /// With no one to look, a visual is kept unlooked, and the run says why.
    #[tokio::test]
    async fn with_no_looker_nothing_is_looked_at() {
        let app = tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets())).expect("app");
        assert!(matches!(look(app.handle(), "<p>x</p>", &[]).await, Looked::Not(_)));
    }

    /// In the app: the page is told, draws it, and what it sends back is the
    /// answer; one that never answers is waited for no longer than `WITHIN`.
    #[tokio::test]
    async fn the_frame_answers_by_its_token() {
        use tauri::{Listener, Manager};
        let app = tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets())).expect("app");
        app.manage(Looker::Frame);
        let seen = std::sync::Arc::new(Mutex::new(String::new()));
        let told = seen.clone();
        app.listen_any(EVENT, move |event| {
            let token: Value = serde_json::from_str(event.payload()).expect("payload");
            let token = token["token"].as_str().expect("token").to_string();
            *told.lock().unwrap() = token.clone();
            assert!(draft_page(&token).is_some_and(|p| p.contains("<svg>")), "the draft is served while it is looked at");
            syn_visual_looked(token, Lint { problems: vec![Problem { check: "nan".into(), detail: "NaN".into() }], ..Lint::default() });
        });
        match look(app.handle(), "<svg></svg>", &[]).await {
            Looked::Seen(lint) => assert_eq!(lint.problems[0].check, "nan"),
            other => panic!("{other:?}"),
        }
        let token = seen.lock().unwrap().clone();
        assert!(!drafts().lock().unwrap().contains_key(&token) && !waiting().lock().unwrap().contains_key(&token), "nothing is left held");
    }
}
