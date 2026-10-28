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

use super::{check, look, store};
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

/// One part per pattern, in the order of [`store::PATTERNS`]: what a visual of
/// that pattern must have, and one written by hand to copy from. A model
/// copies an example more closely than it follows a rule (plan §2.1).
const PATTERN_GUIDES: [&str; 6] = [
    include_str!("guides/patterns/step.md"),
    include_str!("guides/patterns/compare.md"),
    include_str!("guides/patterns/decompose.md"),
    include_str!("guides/patterns/zoom.md"),
    include_str!("guides/patterns/areas.md"),
    include_str!("guides/patterns/explore.md"),
];

/// What the lint finds that breaks a visual rather than mars it: a script that
/// stopped, nothing drawn, NaN on screen, a claim of its own that does not
/// hold, a number of its brief it does not show. Never kept. The rest — text
/// over text, a label cut off, a control that does nothing — is sent back to
/// be fixed while tries are left, and kept with a note on the last.
const BREAKS: &[&str] = &["error", "empty", "nan", "expect", "proof", "hang", "lint"];

/// What the lint finds that is only told, never a reason to draw again: a
/// number of the brief the visual reports under another name is bookkeeping.
/// Sent back, it cost 14 of 157 tries on 2026-10-28 (Q4) and the visual
/// nothing.
const NOTED_ONLY: &[&str] = &["unreported"];

/// How a refusal for what the lint found begins, to be told from the others
/// in a run's steps.
const LOOKED_AT: &str = "Drawn where the person cannot see it and looked at";

/// How long a brief's point may be.
const MAX_POINT: usize = 240;

/// Whether a run is offered the two tools at all.
pub fn offered(capability: &Capability, settings: &SynSettings, surface: Surface, sub_run: bool) -> bool {
    capability.visuals && settings.visuals && cfg!(desktop) && surface == Surface::App && !sub_run
}

fn pattern_index(pattern: &str) -> Option<usize> {
    store::PATTERNS.iter().position(|p| *p == pattern)
}

/// The guide for one kind and one pattern: what every visual needs, what that
/// kind does, then what that pattern must have, with its example.
pub fn guide(args: &Value) -> String {
    let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("").trim();
    let (kind, pattern) = (text("kind"), text("pattern"));
    let specific = match kind {
        "diagram" => DIAGRAM,
        "chart" => CHART,
        "explainer" => EXPLAINER,
        "simulation" => SIMULATION,
        _ => {
            return json!({ "error": format!("`kind` is one of {}.", store::KINDS.join(", ")) }).to_string();
        }
    };
    let Some(at) = pattern_index(pattern) else {
        return json!({ "error": format!("`pattern` is one of {}: how the visual explains.", store::PATTERNS.join(", ")) }).to_string();
    };
    json!({ "kind": kind, "pattern": pattern, "guide": format!("{CORE}\n{specific}\n{}", PATTERN_GUIDES[at]) }).to_string()
}

/// The `brief` of a `show_visual` call, or what is wrong with it.
fn brief(args: &Value, title: &str) -> Result<store::Brief, String> {
    let Some(brief) = args.get("brief").filter(|b| b.is_object()) else {
        return Err("`brief` is missing: say what the visual is for before drawing it — `point`, `pattern` and, if numbers prove the point, `proof`.".into());
    };
    let point = brief.get("point").and_then(Value::as_str).map(str::trim).unwrap_or("");
    if point.split_whitespace().count() < 4 || point.chars().count() > MAX_POINT {
        return Err(format!(
            "`brief.point` is one sentence, at most {MAX_POINT} characters, saying what the person will see — \"the overshoot stays near 9% however many terms\" — not the topic."
        ));
    }
    if point.eq_ignore_ascii_case(title) {
        return Err("`brief.point` is what the visual makes the person see, not its title.".into());
    }
    let pattern = brief.get("pattern").and_then(Value::as_str).map(str::trim).unwrap_or("");
    if pattern_index(pattern).is_none() {
        return Err(format!("`brief.pattern` is one of {}.", store::PATTERNS.join(", ")));
    }
    let mut proof = Vec::new();
    for item in brief.get("proof").and_then(Value::as_array).into_iter().flatten() {
        let name = item.get("name").and_then(Value::as_str).map(str::trim).unwrap_or("");
        let Some(value) = item.get("value").and_then(Value::as_f64).filter(|v| v.is_finite()) else {
            return Err("Each `brief.proof` item is `{ \"name\": …, \"value\": <a number> }`.".into());
        };
        if name.is_empty() {
            return Err("Each `brief.proof` item has a `name`.".into());
        }
        proof.push(store::Proof { name: name.to_string(), value });
    }
    Ok(store::Brief { point: point.to_string(), pattern: pattern.to_string(), proof })
}

/// What the run has done that `show_visual` depends on.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tried {
    /// `visual_guide` answered in this run.
    pub guide_read: bool,
    /// `show_visual` calls refused since the last one that kept a visual.
    pub refused_since_shown: usize,
    /// The patterns whose part of the guide was read, one bit each in the
    /// order of [`store::PATTERNS`].
    pub patterns_read: u8,
    /// Since the last visual kept, one was refused for what the lint found:
    /// what only mars a visual has been sent back once already.
    pub looked_since_shown: bool,
    /// The `title` and `kind` of the last call since the last visual kept
    /// that sent them. A retry is often sent as a patch — on 2026-10-28 every
    /// call that came back without its title sent only `brief` and `html` — and
    /// those are what it meant to leave as they were.
    pub before: Option<(String, String)>,
}

impl Tried {
    fn read(&self, pattern: &str) -> bool {
        pattern_index(pattern).is_some_and(|at| self.patterns_read & (1 << at) != 0)
    }
}

impl Tried {
    /// Read off the run's steps, oldest first.
    pub fn from_steps(steps: &[crate::syn::run::Step]) -> Tried {
        let calls = steps.iter().filter(|s| s.kind == crate::syn::run::StepKind::ToolCall);
        let guides = calls.clone().filter(|s| s.tool.as_deref() == Some(GUIDE_TOOL) && s.ok == Some(true));
        let guide_read = guides.clone().next().is_some();
        let patterns_read = guides
            .filter_map(|s| s.args.as_ref()?.get("pattern")?.as_str().and_then(|p| pattern_index(p.trim())))
            .fold(0u8, |bits, at| bits | (1 << at));
        let refused: Vec<&crate::syn::run::Step> = calls
            .filter(|s| s.tool.as_deref() == Some(SHOW_TOOL))
            .rev()
            .take_while(|s| s.ok == Some(false))
            .collect();
        let looked_since_shown = refused.iter().any(|s| s.preview.contains(LOOKED_AT) || s.full.as_deref().is_some_and(|f| f.contains(LOOKED_AT)));
        let field = |s: &crate::syn::run::Step, key: &str| {
            s.args.as_ref()?.get(key)?.as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
        };
        let before = refused.iter().find_map(|s| Some((field(s, "title")?, field(s, "kind")?)));
        Tried { guide_read, refused_since_shown: refused.len(), patterns_read, looked_since_shown, before }
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

/// Arguments that did not arrive as a JSON object: their length and why they
/// do not parse. A provider keeps what it could not parse as a string
/// (`openai::arguments_to_value`), and a call read as having no `title` was
/// told about its title — ten times on 2026-10-28, each the retry after a
/// visual was sent back, each a try lost to the wrong fix.
fn unreadable(args: &Value) -> Option<(usize, String)> {
    match args {
        Value::Object(_) => None,
        Value::String(raw) => Some((
            raw.chars().count(),
            serde_json::from_str::<Value>(raw).err().map(|e| e.to_string()).unwrap_or_else(|| "not an object".into()),
        )),
        other => Some((other.to_string().chars().count(), "not an object".into())),
    }
}

/// `show_visual`: look at the fragment, keep it, say what happened. What
/// comes back is the tool's answer and, when a visual was kept, what it was.
pub async fn show<R: tauri::Runtime>(
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
    if let Some((chars, error)) = unreadable(args) {
        return refused(format!(
            "The call's arguments did not arrive as JSON ({error}; {chars} characters received). Most likely the call was cut off because the visual is too long, or a quote or a backslash in the HTML was not escaped. Call show_visual again with every field — title, kind, brief, html — and a shorter visual."
        ));
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
    let (mut title, mut kind, html) = (text("title"), text("kind"), args.get("html").and_then(Value::as_str).unwrap_or(""));
    // A retry that left out what it did not change keeps what it sent before.
    if let Some((title_before, kind_before)) = &tried.before {
        if title.is_empty() {
            title = title_before;
        }
        if kind.is_empty() {
            kind = kind_before;
        }
    }
    if title.is_empty() {
        return refused(format!("`title` is what the visual shows, in at most {MAX_TITLE} characters."));
    }
    // Too long, it is cut rather than refused: a refusal cost a try for a
    // title five times on 2026-10-28 (Q4), and the try was the visual's last.
    let cut: String;
    let title = if title.chars().count() > MAX_TITLE {
        cut = format!("{}…", title.chars().take(MAX_TITLE - 1).collect::<String>().trim_end());
        cut.as_str()
    } else {
        title
    };
    if !store::KINDS.contains(&kind) {
        return refused(format!("`kind` is one of {}.", store::KINDS.join(", ")));
    }
    let brief = match brief(args, title) {
        Ok(brief) => brief,
        Err(said) => return refused(said),
    };
    if !tried.read(&brief.pattern) {
        return refused(format!(
            "Call {GUIDE_TOOL} with pattern \"{0}\" first: it says what a {0} visual must have, with one to copy from.",
            brief.pattern
        ));
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
    let tries_left = MAX_TRIES.saturating_sub(tried.refused_since_shown + 1);
    let looked = look::look(app, html, &brief.proof).await;
    let mut remaining: Vec<look::Problem> = Vec::new();
    if let look::Looked::Seen(lint) = &looked {
        let refusing: Vec<&look::Problem> = lint.problems.iter().filter(|p| !NOTED_ONLY.contains(&p.check.as_str())).collect();
        // What breaks the visual is never kept. What only mars it is sent back
        // once: sent back twice, the model gave up drawing in 6 of 8 visuals
        // lost on 2026-10-28 (Q4), and a visual with a label over a line is
        // better than none. On the last try it is kept too.
        let breaks = refusing.iter().any(|p| BREAKS.contains(&p.check.as_str()));
        if breaks || (!refusing.is_empty() && tries_left > 0 && !tried.looked_since_shown) {
            let content = json!({
                "error": format!("{LOOKED_AT}: not shown. Fix these and call show_visual again."),
                "problems": lint.problems,
                // After `error`, which a run's step keeps the start of.
                "failed_claims": lint.expects.iter().filter(|c| !c.ok).collect::<Vec<_>>(),
                "tries_left": tries_left,
            });
            return (content.to_string(), None);
        }
        remaining = lint.problems.clone();
    }
    let draft = store::Draft { title, kind, brief: Some(&brief), html, question: asked.question, conversation: asked.conversation, model: Some(asked.model) };
    match store::save(app, db, vault, &draft) {
        Ok(id) => {
            let mut content = json!({
                "shown": true,
                "visual_id": id,
                "warnings": checked.warnings,
                "note": "Shown to the user under your answer, and kept. Say in a sentence or two what it shows and how to use it; do not repeat its numbers. A number your answer must name — one the person asked for — is one the visual reports below, quoted as it is, or one worked out with `compute` from the same formula; never one from your script or worked in your head.",
            });
            match &looked {
                look::Looked::Seen(lint) => {
                    content["reported"] = Value::Object(lint.reported.clone());
                    if !remaining.is_empty() {
                        content["kept_with"] = json!(remaining);
                    }
                }
                look::Looked::Not(why) => content["not_looked_at"] = json!(why),
            }
            (content.to_string(), Some(Shown { id, title: title.to_string(), kind: kind.to_string() }))
        }
        Err(e) => refused(format!("The visual could not be kept: {e}")),
    }
}

/// The titles of the visuals an earlier answer drew, read off its tool log:
/// a kept one's arguments carry its id ([`compact`]).
///
/// A visual's numbers are worked out by its script, which no model can run.
/// Asked on 2026-10-28 which way of paying a loan cost less, `gpt-6-luna` read
/// the script of the visual it had drawn and wrote three totals, none of them
/// the visual's. So a conversation that drew one is sent `compute`, and the
/// answer that drew it says so in the history.
pub fn drawn(m: &crate::models::syn::SynMessage) -> Vec<String> {
    m.tool_calls_log
        .iter()
        .flatten()
        .filter(|c| c.tool_name == SHOW_TOOL && c.tool_args.get("visual_id").is_some())
        .map(|c| c.tool_args.get("title").and_then(Value::as_str).unwrap_or_default().to_string())
        .collect()
}

/// What is kept of a `show_visual` call in place of its arguments: the HTML
/// lives in the visual's node, once. A refused call keeps the start of its
/// HTML and its length, enough to see what was tried.
pub fn compact(args: &Value, shown: Option<&Shown>) -> Value {
    if let Some((chars, error)) = unreadable(args) {
        // Both ends and where the parse stopped: enough to tell a call cut off
        // from one with a quote left unescaped.
        let raw = args.as_str().unwrap_or("");
        let end: String = raw.chars().rev().take(400).collect::<Vec<_>>().into_iter().rev().collect();
        return json!({
            "unreadable": true,
            "raw_chars": chars,
            "json_error": error,
            "raw_start": raw.chars().take(400).collect::<String>(),
            "raw_end": end,
        });
    }
    match shown {
        Some(shown) => json!({ "visual_id": shown.id, "title": shown.title, "kind": shown.kind }),
        None => {
            let html = args.get("html").and_then(Value::as_str).unwrap_or("");
            json!({
                // Which fields it sent: a retry has come back without its title.
                "keys": args.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
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
            let args = if *tool == GUIDE_TOOL { json!({ "kind": "simulation", "pattern": "explore" }) } else { json!({}) };
            run.record_tool(1, tool, args, *ok, crate::syn::registry::Reversal::Nothing, content, 1);
        }
        run.steps
    }

    /// A guide read for `pattern`, as a run records it.
    fn guide_for(pattern: &str) -> Vec<Step> {
        let mut run = crate::syn::run::Run::new("q", None, crate::syn::run::Budget::from_settings(&Default::default()));
        run.record_tool(1, GUIDE_TOOL, json!({ "kind": "chart", "pattern": pattern }), true, crate::syn::registry::Reversal::Nothing, "{}", 1);
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
    fn the_guide_is_the_core_the_kind_and_the_pattern() {
        for kind in store::KINDS {
            for pattern in store::PATTERNS {
                let v: Value = serde_json::from_str(&guide(&json!({ "kind": kind, "pattern": pattern }))).expect("json");
                let text = v["guide"].as_str().expect("guide");
                assert!(text.contains("# Drawing a visual in Synabit"), "{kind}");
                assert!(text.contains(&format!("# Pattern: {pattern}")), "{kind} {pattern}");
                assert!(text.contains("```html\n"), "{pattern} comes with an example");
            }
        }
        let v: Value = serde_json::from_str(&guide(&json!({ "kind": "poster", "pattern": "step" }))).expect("json");
        assert!(v.get("error").is_some());
        let v: Value = serde_json::from_str(&guide(&json!({ "kind": "chart" }))).expect("json");
        assert!(v["error"].as_str().is_some_and(|e| e.contains("pattern")), "no pattern: {v}");
    }

    #[test]
    fn the_patterns_read_are_counted() {
        let mut both = guide_for("zoom");
        both.extend(guide_for("step"));
        let tried = Tried::from_steps(&both);
        assert!(tried.read("zoom") && tried.read("step"));
        assert!(!tried.read("areas") && !tried.read("poster"));
    }

    /// The brief is the visual's design, said before drawing (plan
    /// 2026-10-28, §2.1): a point that is not the topic, and a pattern whose
    /// guide was read.
    #[tokio::test]
    async fn a_visual_is_kept_only_with_a_brief_and_its_pattern_read() {
        use tauri::Manager;
        let (_dir, vault, app) = app();
        let db = app.state::<crate::db::DbState>();
        let read = Tried { guide_read: true, refused_since_shown: 0, patterns_read: 0, looked_since_shown: false, before: None };
        let zoom = Tried::from_steps(&guide_for("zoom"));
        let with = |brief: Value| json!({ "title": "Gibbs", "kind": "explainer", "html": "<svg role=\"img\"></svg>", "brief": brief });
        let point = "The overshoot stays near 9% however many terms are added";

        for (args, why) in [
            (json!({ "title": "Gibbs", "kind": "explainer", "html": "<svg></svg>" }), "`brief` is missing"),
            (with(json!({ "point": "Gibbs", "pattern": "zoom" })), "`brief.point`"),
            (with(json!({ "point": "Fourier and Gibbs", "pattern": "zoom" })), "`brief.point`"),
            (with(json!({ "point": point, "pattern": "poster" })), "`brief.pattern`"),
            (with(json!({ "point": point, "pattern": "zoom", "proof": [{ "name": "overshoot" }] })), "`brief.proof`"),
        ] {
            let (said, shown) = show(app.handle(), db.inner(), &vault, true, zoom.clone(), &ASKED, &args).await;
            assert!(shown.is_none() && said.contains(why), "{why}: {said}");
        }
        let brief = json!({ "point": point, "pattern": "zoom", "proof": [{ "name": "overshoot", "value": 1.0895 }] });
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, read.clone(), &ASKED, &with(brief.clone())).await;
        assert!(shown.is_none() && said.contains("with pattern") && said.contains("zoom"), "its guide first: {said}");

        let (said, shown) = show(app.handle(), db.inner(), &vault, true, zoom.clone(), &ASKED, &with(brief)).await;
        let shown = shown.unwrap_or_else(|| panic!("kept: {said}"));
        let back = store::load(std::path::Path::new(&vault), &shown.id).expect("read back");
        assert_eq!(back.pattern.as_deref(), Some("zoom"));
        assert_eq!(back.point.as_deref(), Some(point));
    }

    /// The guide names only what the kit and `/lib/` really have.
    #[test]
    fn the_guide_promises_nothing_the_frame_lacks() {
        let all = [&[CORE, DIAGRAM, CHART, EXPLAINER, SIMULATION][..], &PATTERN_GUIDES[..]].concat().concat();
        for path in ["/lib/d3.v7.min.js", "/lib/katex.min.js", "/lib/katex.min.css"] {
            assert!(all.contains(path));
        }
        let kit = super::super::assets::KIT_JS;
        for api in ["synabit.number", "synabit.ask", "synabit.ready", "synabit.onContext", "synabit.color", "synabit.expect", "synabit.report", "synabit.chart", "synabit.controls", "synabit.stats", "synabit.layout"] {
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
        assert_eq!(Tried::from_steps(&done), Tried { guide_read: true, refused_since_shown: 2, patterns_read: 1 << 5, looked_since_shown: false, before: None });
        assert!(!Tried::from_steps(&steps(&[(GUIDE_TOOL, false)])).guide_read, "a guide call that failed");
    }

    /// A refusal for what the lint found is told from one for anything else,
    /// and only since the last visual kept.
    #[test]
    fn a_refusal_for_what_was_looked_at_is_remembered() {
        let mut run = crate::syn::run::Run::new("q", None, crate::syn::run::Budget::from_settings(&Default::default()));
        let looked = json!({ "error": format!("{LOOKED_AT}: not shown."), "problems": [] }).to_string();
        run.record_tool(1, SHOW_TOOL, json!({}), false, crate::syn::registry::Reversal::Nothing, "{\"error\":\"`kind` is one of …\"}", 1);
        assert!(!Tried::from_steps(&run.steps).looked_since_shown, "refused for its kind");
        run.record_tool(1, SHOW_TOOL, json!({}), false, crate::syn::registry::Reversal::Nothing, &looked, 1);
        assert!(Tried::from_steps(&run.steps).looked_since_shown);
        run.record_tool(1, SHOW_TOOL, json!({}), true, crate::syn::registry::Reversal::Nothing, "{\"shown\":true}", 1);
        assert!(!Tried::from_steps(&run.steps).looked_since_shown, "a visual was kept since");
    }

    /// A title too long is cut, not refused: the try is the visual's.
    #[tokio::test]
    async fn a_long_title_is_cut() {
        use tauri::Manager;
        let (_dir, vault, app) = app();
        let db = app.state::<crate::db::DbState>();
        let long = "Con lắc đơn ".repeat(30);
        let brief = json!({ "point": "The period depends on the length, not the mass", "pattern": "explore" });
        let args = json!({ "title": long, "kind": "simulation", "brief": brief, "html": "<input type=\"range\"><svg></svg>" });
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried::from_steps(&guide_for("explore")), &ASKED, &args).await;
        let shown = shown.unwrap_or_else(|| panic!("kept: {said}"));
        assert_eq!(shown.title.chars().count(), MAX_TITLE, "{}", shown.title);
        assert!(shown.title.ends_with('…'));
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried::from_steps(&guide_for("explore")), &ASKED, &json!({ "title": " ", "kind": "simulation", "brief": brief, "html": "<svg></svg>" })).await;
        assert!(shown.is_none() && said.contains("`title`"), "an empty one is still refused: {said}");
    }

    /// Arguments that are not JSON are said to be that — not a missing title —
    /// and what is kept of them shows where they broke.
    #[tokio::test]
    async fn arguments_that_are_not_json_are_named() {
        use tauri::Manager;
        let (_dir, vault, app) = app();
        let db = app.state::<crate::db::DbState>();
        let cut = Value::String(format!("{{\"title\":\"Con lắc\",\"kind\":\"simulation\",\"html\":\"<svg>{}", "<p></p>".repeat(200)));
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried::from_steps(&guide_for("explore")), &ASKED, &cut).await;
        assert!(shown.is_none());
        assert!(said.contains("did not arrive as JSON") && said.contains("EOF while parsing"), "{said}");
        assert!(!said.contains("`title` is what"), "not told about its title: {said}");

        let kept = compact(&cut, None);
        assert_eq!(kept["unreadable"], json!(true));
        assert!(kept["json_error"].as_str().is_some_and(|e| e.starts_with("EOF while parsing")), "{kept}");
        assert_eq!(kept["raw_end"].as_str().map(|e| e.chars().count()), Some(400));
        assert!(kept["raw_start"].as_str().is_some_and(|s| s.starts_with("{\"title\"")));
        assert!(unreadable(&json!({ "title": "x" })).is_none());
    }

    /// A retry sent as a patch — only `brief` and `html`, as every call that
    /// came back without its title was on 2026-10-28 — keeps the title and
    /// kind it sent before, but only since the last visual kept.
    #[tokio::test]
    async fn a_retry_keeps_the_title_and_kind_it_sent_before() {
        use tauri::Manager;
        let (_dir, vault, app) = app();
        let db = app.state::<crate::db::DbState>();
        let brief = json!({ "point": "The period depends on the length, not the mass", "pattern": "explore" });
        let first = json!({ "title": "Con lắc đơn", "kind": "simulation", "brief": brief, "html": "<svg></svg>" });
        let mut steps = guide_for("explore");
        let mut run = crate::syn::run::Run::new("q", None, crate::syn::run::Budget::from_settings(&Default::default()));
        run.steps = steps.clone();
        run.record_tool(1, SHOW_TOOL, compact(&first, None), false, crate::syn::registry::Reversal::Nothing, "{\"error\":\"Drawn where…\"}", 1);
        steps = run.steps.clone();

        let patch = json!({ "brief": brief, "html": "<input type=\"range\"><svg></svg>" });
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried::from_steps(&steps), &ASKED, &patch).await;
        let shown = shown.unwrap_or_else(|| panic!("kept: {said}"));
        assert_eq!((shown.title.as_str(), shown.kind.as_str()), ("Con lắc đơn", "simulation"));

        // Once a visual is kept, the next one is a new visual and says its own.
        run.record_tool(1, SHOW_TOOL, compact(&patch, Some(&shown)), true, crate::syn::registry::Reversal::Nothing, "{\"shown\":true}", 1);
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried::from_steps(&run.steps), &ASKED, &patch).await;
        assert!(shown.is_none() && said.contains("`title`"), "{said}");
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

    /// The app, managed to look at a draft as the page would and to find `lint`.
    fn looking(app: &tauri::App<tauri::test::MockRuntime>, lint: look::Lint) {
        use tauri::{Listener, Manager};
        app.manage(look::Looker::Frame);
        app.listen_any(look::EVENT, move |event| {
            let token: Value = serde_json::from_str(event.payload()).expect("payload");
            look::syn_visual_looked(token["token"].as_str().expect("token").to_string(), lint.clone());
        });
    }

    fn problem(check: &str) -> look::Problem {
        look::Problem { check: check.into(), detail: format!("{check} found") }
    }

    /// PR Q4: what the lint finds goes back to the model while it has tries
    /// left; what breaks a visual is never kept; what only mars it is kept on
    /// the last try, with what remains said. What it reports is in the answer.
    #[tokio::test]
    async fn a_visual_is_looked_at_before_it_is_kept() {
        use tauri::Manager;
        let brief = json!({ "point": "The period depends on the length, not the mass", "pattern": "explore" });
        let args = json!({ "title": "Con lắc", "kind": "simulation", "brief": brief, "html": "<input type=\"range\"><svg></svg>" });
        let read = Tried::from_steps(&guide_for("explore"));
        let last = Tried { refused_since_shown: MAX_TRIES - 1, ..read.clone() };
        let once = Tried { refused_since_shown: 1, looked_since_shown: true, ..read.clone() };
        let kept = |vault: &str| std::fs::read_dir(std::path::Path::new(vault).join(store::FOLDER)).map(|d| d.count()).unwrap_or(0);

        for (lint, tried, keeps, says) in [
            (look::Lint { problems: vec![problem("overlap")], ..Default::default() }, read.clone(), false, "overlap found"),
            (look::Lint { problems: vec![problem("nan")], ..Default::default() }, last.clone(), false, "nan found"),
            (look::Lint { problems: vec![problem("expect")], ..Default::default() }, last.clone(), false, "expect found"),
            (look::Lint { problems: vec![problem("overlap")], ..Default::default() }, last.clone(), true, "kept_with"),
            // Sent back once already: what only mars it is kept now, with a note.
            (look::Lint { problems: vec![problem("overlap")], ..Default::default() }, once.clone(), true, "kept_with"),
            (look::Lint { problems: vec![problem("nan")], ..Default::default() }, once.clone(), false, "nan found"),
            // A name the visual reports otherwise is told, never drawn again for.
            (look::Lint { problems: vec![problem("unreported")], ..Default::default() }, read.clone(), true, "unreported found"),
        ] {
            let (_dir, vault, app) = app();
            looking(&app, lint);
            let db = app.state::<crate::db::DbState>();
            let (said, shown) = show(app.handle(), db.inner(), &vault, true, tried, &ASKED, &args).await;
            assert_eq!(shown.is_some(), keeps, "{said}");
            assert_eq!(kept(&vault), usize::from(keeps), "{said}");
            assert!(said.contains(says), "{says}: {said}");
        }

        let (_dir, vault, app) = app();
        let mut reported = serde_json::Map::new();
        reported.insert("chu kỳ (s)".into(), json!(2.006));
        looking(&app, look::Lint { reported, ..Default::default() });
        let db = app.state::<crate::db::DbState>();
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, read.clone(), &ASKED, &args).await;
        assert!(shown.is_some(), "{said}");
        let said: Value = serde_json::from_str(&said).expect("json");
        assert_eq!(said["reported"]["chu kỳ (s)"], json!(2.006), "{said}");
        assert!(said.get("not_looked_at").is_none());
    }

    const ASKED: Asked<'static> = Asked { question: "con lắc", conversation: Some("c1"), model: "gpt-6-luna" };

    #[tokio::test]
    async fn a_visual_is_kept_only_after_the_guide_and_the_check() {
        use tauri::Manager;
        let (_dir, vault, app) = app();
        let db = app.state::<crate::db::DbState>();
        let brief = json!({ "point": "The period depends on the length, not the mass", "pattern": "explore" });
        let good = json!({ "title": "Con lắc", "kind": "simulation", "brief": brief, "html": "<input type=\"range\"><script src=\"/lib/d3.v7.min.js\"></script>" });
        let read = Tried::from_steps(&guide_for("explore"));

        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried::default(), &ASKED, &good).await;
        assert!(shown.is_none() && said.contains(GUIDE_TOOL), "the guide first: {said}");

        let cdn = json!({ "title": "Con lắc", "kind": "simulation", "brief": brief, "html": "<script src=\"https://cdn.jsdelivr.net/npm/chart.js\"></script>" });
        let (said, shown) = show(app.handle(), db.inner(), &vault, true, read.clone(), &ASKED, &cdn).await;
        assert!(shown.is_none() && said.contains("points outside"), "{said}");
        assert!(!std::path::Path::new(&vault).join(store::FOLDER).exists() || std::fs::read_dir(std::path::Path::new(&vault).join(store::FOLDER)).unwrap().next().is_none(), "nothing kept");

        let (said, shown) = show(app.handle(), db.inner(), &vault, true, Tried { refused_since_shown: MAX_TRIES, ..read.clone() }, &ASKED, &good).await;
        assert!(shown.is_none() && said.contains("tries"), "{said}");

        let (said, shown) = show(app.handle(), db.inner(), &vault, false, read.clone(), &ASKED, &good).await;
        assert!(shown.is_none() && said.contains("not available"), "{said}");

        let (said, shown) = show(app.handle(), db.inner(), &vault, true, read.clone(), &ASKED, &good).await;
        let shown = shown.expect("kept");
        assert!(said.contains(&shown.id));
        let back = store::load(std::path::Path::new(&vault), &shown.id).expect("read back");
        assert_eq!(back.question, "con lắc");
        assert_eq!(back.conversation.as_deref(), Some("c1"));
    }
}
