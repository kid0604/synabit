//! The two tools a model draws with: `visual_guide`, then `show_visual`.
//!
//! # Why two, and why the guide is its own tool
//!
//! How a visual is drawn here — the kit, the colours, what the frame refuses,
//! how a simulation is kept honest — is a few thousand characters a model
//! needs only when it is about to draw. Claude's own visuals keep that guide
//! behind a tool the model calls first, so it costs nothing on every question
//! that is answered in words; this does the same. `show_visual` is refused
//! until the guide was read in the run.
//!
//! # Who is offered them
//!
//! Only where everything holds (plan §7): a model that draws
//! (`Capability::visuals`), the person's `visuals` setting on, a desktop, a
//! run asked in the app — not Telegram, not a routine — and not a helper.
//! [`offered`] is the one place that says so; the engine leaves both tools out
//! otherwise, and `show_visual` asks again before it keeps anything.
//!
//! # What is kept of a call
//!
//! The model's HTML is kept once, in the visual's node. A call's arguments are
//! otherwise kept three times — the run's step, the conversation's tool log,
//! the taint's record of what the model wrote — so the engine keeps
//! [`compact`] of them instead, and never counts the HTML as a destination.

use serde_json::{json, Value};

use super::{check, store};
use crate::models::syn::SynSettings;
use crate::syn::provider::capability::Capability;
use crate::syn::surface::Surface;

pub const GUIDE_TOOL: &str = "visual_guide";
pub const SHOW_TOOL: &str = "show_visual";
pub const TOOLS: [&str; 2] = [GUIDE_TOOL, SHOW_TOOL];

/// Tries at one visual: the first and two fixes. After that the answer is
/// words.
pub const MAX_TRIES: usize = 3;

/// How long a visual's title may be.
const MAX_TITLE: usize = 120;

const CORE: &str = include_str!("guides/core.md");
const DIAGRAM: &str = include_str!("guides/diagram.md");
const CHART: &str = include_str!("guides/chart.md");
const EXPLAINER: &str = include_str!("guides/explainer.md");
const SIMULATION: &str = include_str!("guides/simulation.md");

/// Whether a run is offered the two tools at all.
pub fn offered(capability: &Capability, settings: &SynSettings, surface: Surface, sub_run: bool) -> bool {
    capability.visuals && settings.visuals && cfg!(desktop) && surface == Surface::App && !sub_run
}

/// The guide for one kind: what every visual needs, then what that kind does.
pub fn guide(args: &Value) -> String {
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("").trim();
    let specific = match kind {
        "diagram" => DIAGRAM,
        "chart" => CHART,
        "explainer" => EXPLAINER,
        "simulation" => SIMULATION,
        _ => {
            return json!({ "error": format!("`kind` is one of {}.", store::KINDS.join(", ")) }).to_string();
        }
    };
    json!({ "kind": kind, "guide": format!("{CORE}\n{specific}") }).to_string()
}

/// What the run has done that `show_visual` depends on.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tried {
    /// `visual_guide` answered in this run.
    pub guide_read: bool,
    /// `show_visual` calls refused since the last one that kept a visual.
    pub refused_since_shown: usize,
}

impl Tried {
    /// Read off the run's steps, oldest first.
    pub fn from_steps(steps: &[crate::syn::run::Step]) -> Tried {
        let calls = steps.iter().filter(|s| s.kind == crate::syn::run::StepKind::ToolCall);
        let guide_read = calls.clone().any(|s| s.tool.as_deref() == Some(GUIDE_TOOL) && s.ok == Some(true));
        let refused_since_shown = calls
            .filter(|s| s.tool.as_deref() == Some(SHOW_TOOL))
            .rev()
            .take_while(|s| s.ok == Some(false))
            .count();
        Tried { guide_read, refused_since_shown }
    }
}

/// A visual `show_visual` kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub id: String,
    pub title: String,
    pub kind: String,
}

/// Where a visual came from, for its node.
pub struct Asked<'a> {
    pub question: &'a str,
    pub conversation: Option<&'a str>,
    pub model: &'a str,
}

fn refused(said: impl Into<String>) -> (String, Option<Shown>) {
    (json!({ "error": said.into() }).to_string(), None)
}

const NOT_HERE: &str = "Visuals are not available here. Answer in words; a Mermaid diagram if a picture is needed.";

/// `visual_guide`, in a run: the guide where visuals are offered, refused
/// where they are not. A run not offered the tool can still name it, and the
/// guide would tell it how to draw what it may not show.
pub fn read_guide(offered_here: bool, args: &Value) -> String {
    if !offered_here {
        return refused(NOT_HERE).0;
    }
    guide(args)
}

/// `show_visual`: look at the fragment, keep it, say what happened. What
/// comes back is the tool's answer and, when a visual was kept, what it was.
pub fn show<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    db: &crate::db::DbState,
    vault: &str,
    offered_here: bool,
    tried: Tried,
    asked: &Asked<'_>,
    args: &Value,
) -> (String, Option<Shown>) {
    if !offered_here {
        return refused(NOT_HERE);
    }
    if !tried.guide_read {
        return refused(format!("Call {GUIDE_TOOL} with this visual's kind first: it says how a visual is drawn here."));
    }
    if tried.refused_since_shown >= MAX_TRIES {
        return refused(format!(
            "{MAX_TRIES} tries at this visual were refused. Stop drawing it; answer in words and say a visual could not be made."
        ));
    }
    let text = |key: &str| args.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("");
    let (title, kind, html) = (text("title"), text("kind"), args.get("html").and_then(Value::as_str).unwrap_or(""));
    if title.is_empty() || title.chars().count() > MAX_TITLE {
        return refused(format!("`title` is what the visual shows, in at most {MAX_TITLE} characters."));
    }
    if !store::KINDS.contains(&kind) {
        return refused(format!("`kind` is one of {}.", store::KINDS.join(", ")));
    }
    let checked = check::check(html);
    if !checked.passed() {
        let content = json!({
            "error": "Not shown. Fix these and call show_visual again.",
            "problems": checked.problems,
            "warnings": checked.warnings,
            "tries_left": MAX_TRIES.saturating_sub(tried.refused_since_shown + 1),
        });
        return (content.to_string(), None);
    }
    let draft = store::Draft { title, kind, html, question: asked.question, conversation: asked.conversation, model: Some(asked.model) };
    match store::save(app, db, vault, &draft) {
        Ok(id) => (
            json!({
                "shown": true,
                "visual_id": id,
                "warnings": checked.warnings,
                "note": "Shown to the user under your answer, and kept. Say in a sentence or two what it shows and how to use it; do not repeat its numbers.",
            })
            .to_string(),
            Some(Shown { id, title: title.to_string(), kind: kind.to_string() }),
        ),
        Err(e) => refused(format!("The visual could not be kept: {e}")),
    }
}

/// What is kept of a `show_visual` call in place of its arguments: the HTML
/// lives in the visual's node, once. A refused call keeps the start of its
/// HTML and its length, enough to see what was tried.
pub fn compact(args: &Value, shown: Option<&Shown>) -> Value {
    match shown {
        Some(shown) => json!({ "visual_id": shown.id, "title": shown.title, "kind": shown.kind }),
        None => {
            let html = args.get("html").and_then(Value::as_str).unwrap_or("");
            json!({
                "title": args.get("title").cloned().unwrap_or(Value::Null),
                "kind": args.get("kind").cloned().unwrap_or(Value::Null),
                "html_chars": html.chars().count(),
                "html_start": html.chars().take(400).collect::<String>(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syn::run::Step;

    /// The steps a run records for these calls, oldest first.
    fn steps(calls: &[(&str, bool)]) -> Vec<Step> {
        let mut run = crate::syn::run::Run::new("q", None, crate::syn::run::Budget::from_settings(&Default::default()));
        for (tool, ok) in calls {
            let content = if *ok { "{\"ok\":true}" } else { "{\"error\":\"no\"}" };
            run.record_tool(1, tool, json!({}), *ok, crate::syn::registry::Reversal::Nothing, content, 1);
        }
        run.steps
    }

    fn strong() -> Capability {
        crate::syn::provider::capability::of("gpt-6-luna", true)
    }

    /// Every condition, one at a time: the tools are there only when all hold.
    #[test]
    fn offered_only_where_everything_holds() {
        let on = SynSettings::default();
        let off = SynSettings { visuals: false, ..SynSettings::default() };
        let small = crate::syn::provider::capability::of("gemma4:e2b", false);
        assert_eq!(offered(&strong(), &on, Surface::App, false), cfg!(desktop));
        assert!(!offered(&small, &on, Surface::App, false), "a model that does not draw");
        assert!(!offered(&strong(), &off, Surface::App, false), "the setting off");
        assert!(!offered(&strong(), &on, Surface::Telegram, false), "from Telegram");
        assert!(!offered(&strong(), &on, Surface::Routine, false), "a routine");
        assert!(!offered(&strong(), &on, Surface::App, true), "a helper");
    }

    #[test]
    fn the_guide_is_the_core_and_the_kind() {
        for kind in store::KINDS {
            let v: Value = serde_json::from_str(&guide(&json!({ "kind": kind }))).expect("json");
            let text = v["guide"].as_str().expect("guide");
            assert!(text.contains("# Drawing a visual in Synabit"), "{kind}");
            assert!(text.len() > CORE.len() + 200, "{kind}");
        }
        let v: Value = serde_json::from_str(&guide(&json!({ "kind": "poster" }))).expect("json");
        assert!(v.get("error").is_some());
    }

    /// The guide names only what the kit and `/lib/` really have.
    #[test]
    fn the_guide_promises_nothing_the_frame_lacks() {
        let all = [CORE, DIAGRAM, CHART, EXPLAINER, SIMULATION].concat();
        for path in ["/lib/d3.v7.min.js", "/lib/katex.min.js", "/lib/katex.min.css"] {
            assert!(all.contains(path));
        }
        let kit = super::super::assets::KIT_JS;
        for api in ["synabit.number", "synabit.ask", "synabit.ready", "synabit.onContext", "synabit.color"] {
            assert!(all.contains(api), "{api} in the guide");
            assert!(kit.contains(api.trim_start_matches("synabit.")), "{api} in the kit");
        }
        let css = super::super::assets::KIT_CSS;
        for token in ["--text", "--text-2", "--muted", "--surface-alt", "--accent", "--grid", "--c6", ".stat", ".row", ".muted"] {
            assert!(css.contains(token), "{token} in kit.css");
        }
        assert!(!all.contains("cdnjs") && !all.contains("Chart.js"), "no library the frame does not have");
    }

    #[test]
    fn tries_are_counted_since_the_last_visual_kept() {
        assert_eq!(Tried::from_steps(&[]), Tried::default());
        let done = steps(&[(GUIDE_TOOL, true), (SHOW_TOOL, false), (SHOW_TOOL, true), (SHOW_TOOL, false), (SHOW_TOOL, false)]);
        assert_eq!(Tried::from_steps(&done), Tried { guide_read: true, refused_since_shown: 2 });
        assert!(!Tried::from_steps(&steps(&[(GUIDE_TOOL, false)])).guide_read, "a guide call that failed");
    }

    #[test]
    fn what_is_kept_of_a_call_holds_no_html() {
        let html = "<p>".repeat(5000);
        let args = json!({ "title": "T", "kind": "chart", "html": html });
        let shown = Shown { id: "0123456789abcdef0123456789abcdef".into(), title: "T".into(), kind: "chart".into() };
        let kept = compact(&args, Some(&shown));
        assert_eq!(kept, json!({ "visual_id": shown.id, "title": "T", "kind": "chart" }));
        let tried = compact(&args, None);
        assert_eq!(tried["html_chars"], 15000);
        assert_eq!(tried["html_start"].as_str().unwrap().chars().count(), 400);
    }

    fn app() -> (tempfile::TempDir, String, tauri::App<tauri::test::MockRuntime>) {
        use tauri::Manager;
        let dir = tempfile::tempdir().expect("vault");
        let vault = std::fs::canonicalize(dir.path()).expect("canonical").to_string_lossy().to_string();
        let app = tauri::test::mock_builder().build(tauri::test::mock_context(tauri::test::noop_assets())).expect("app");
        app.manage::<crate::db::DbState>(std::sync::Mutex::new(crate::db::DbBridge::new_in_memory_full().expect("db")));
        (dir, vault, app)
    }

    const ASKED: Asked<'static> = Asked { question: "con lắc", conversation: Some("c1"), model: "gpt-6-luna" };

    #[test]
    fn a_visual_is_kept_only_after_the_guide_and_the_check() {
        use tauri::Manager;
        let (_dir, vault, app) = app();
        let db = app.state::<crate::db::DbState>();
        let good = json!({ "title": "Con lắc", "kind": "simulation", "html": "<input type=\"range\"><script src=\"/lib/d3.v7.min.js\"></script>" });
        let read = Tried { guide_read: true, refused_since_shown: 0 };

        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried::default(), &ASKED, &good);
        assert!(shown.is_none() && said.contains(GUIDE_TOOL), "the guide first: {said}");

        let cdn = json!({ "title": "Con lắc", "kind": "simulation", "html": "<script src=\"https://cdn.jsdelivr.net/npm/chart.js\"></script>" });
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, read, &ASKED, &cdn);
        assert!(shown.is_none() && said.contains("points outside"), "{said}");
        assert!(!std::path::Path::new(&vault).join(store::FOLDER).exists() || std::fs::read_dir(std::path::Path::new(&vault).join(store::FOLDER)).unwrap().next().is_none(), "nothing kept");

        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried { guide_read: true, refused_since_shown: MAX_TRIES }, &ASKED, &good);
        assert!(shown.is_none() && said.contains("tries"), "{said}");

        let (said, shown) = show(app.handle(), db.inner(), &vault, false, read, &ASKED, &good);
        assert!(shown.is_none() && said.contains("not available"), "{said}");

        let (said, shown) = show(app.handle(), db.inner(), &vault, true, read, &ASKED, &good);
        let shown = shown.expect("kept");
        assert!(said.contains(&shown.id));
        let back = store::load(std::path::Path::new(&vault), &shown.id).expect("read back");
        assert_eq!(back.question, "con lắc");
        assert_eq!(back.conversation.as_deref(), Some("c1"));
    }
}
