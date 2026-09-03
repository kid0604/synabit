//! A unit of work that outlives the message that started it.
//!
//! # Why this exists
//!
//! Until now the unit was a message: `syn_send_message` loaded a conversation,
//! retrieved context, looped over tools until it had an answer, saved, and
//! returned a `SynMessage`. Everything about that work lived in local variables
//! for the length of one `async fn` and was gone when it returned.
//!
//! That shape costs four things, and each of them is a feature somebody will
//! ask for:
//!
//! * **Nothing to read afterwards.** When Syn does the wrong thing there is no
//!   record but `log::info!`, which is on the user's machine, unstructured, and
//!   interleaved with sync and file-indexing. A tool call that returned
//!   something surprising cannot be looked at again.
//! * **Closing the app loses the work.** Not just the answer — the fact that
//!   the work happened at all.
//! * **Nothing can run unattended.** A scheduled or event-triggered job has no
//!   conversation to belong to, so the entry point cannot even be named.
//! * **No ceiling but the iteration count.** `max_tool_iterations` stops a
//!   model that has decided to search forever, but it counts rounds, not work:
//!   one round asking for fifty tools is within it.
//!
//! A `Run` is that missing thing. It has an id, a goal in the user's own words,
//! a state, a budget, and a transcript that is written as it happens.
//!
//! # Where a run lives, and why it is not a node
//!
//! `{vault}/Syn/runs/<id>.json`, beside the conversations, in the same shape
//! and by the same rules — the vault is the only place anything Syn produces is
//! allowed to live.
//!
//! It is deliberately **not** a node in the vault index. A node per run means a
//! node per message sent, which for somebody who talks to Syn fifty times a day
//! is eighteen thousand new files a year in a vault they also open in a file
//! browser. The thing a node would buy — `query_nodes` reaching runs, and graph
//! edges from a run to what it touched — is worth having, and it is worth having
//! for the handful of runs that produced something durable rather than for every
//! "what's on today". That selection needs a reason to select on, which is a
//! later problem than this one.
//!
//! # Why the transcript is flushed step by step
//!
//! Because the case it exists for is the case where the process does not reach
//! the end: cancelled, crashed, or quit. A transcript written on completion is a
//! transcript that is absent exactly when it is wanted. So each step costs one
//! small atomic write, and a run that was interrupted reads back as far as it
//! got.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::models::syn::SynProvider;

/// How much of a step's result is kept.
///
/// The model saw the whole thing — up to `tools::MAX_RESULT_CHARS` — and this
/// is the part a person is shown afterwards. Matching the cap the tool-call
/// event already used means the panel and the live view agree; keeping the full
/// result instead would put a query's entire output on disk for every run.
const MAX_STEP_PREVIEW: usize = 4000;

/// How many runs are kept before the oldest are removed.
///
/// Listing parses every file in the directory (see `list_runs`), so this number
/// is also what keeps that cheap. At two hundred runs of ordinary length it is
/// a few tens of milliseconds; somewhere past a thousand the listing would need
/// a header or an index of its own, and raising this without doing that is how
/// opening the panel becomes slow.
const KEEP_RUNS: usize = 200;

// ═══════════════════════════════════════════════════════════════
//  WHAT A RUN IS
// ═══════════════════════════════════════════════════════════════

/// What started a run.
///
/// One arm today, and the field exists anyway: a run written now has to still
/// deserialise when scheduled and event-driven runs arrive, and adding the
/// field later would mean either a migration or a `None` that means "user, we
/// think". The others are not listed until something produces them.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Somebody typed something and pressed send.
    #[default]
    User,
}

/// Where a run got to.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Being driven right now, by this process.
    Working,
    /// The model answered.
    Done,
    /// Something went wrong; `Run::error` says what.
    Failed,
    /// The user pressed stop.
    Cancelled,
    /// A limit in `Run::budget` was reached. Distinct from `Failed` because
    /// nothing went wrong — the answer is just built on less work than the
    /// model wanted to do, and the user is the one who decides whether that
    /// matters.
    BudgetExhausted,
    /// Found on disk as `Working` by a process that is not driving it.
    ///
    /// Which is to say: the app was closed, or crashed, in the middle. Written
    /// back once on the first listing after that, so the panel does not show a
    /// spinner for a run nothing is working on.
    Interrupted,
}

impl RunState {
    /// Whether this state means the run is over, however it ended.
    pub fn is_final(self) -> bool {
        !matches!(self, RunState::Working)
    }
}

/// What kind of thing happened.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// The model produced text — either the answer, or the words it said on
    /// its way to reaching for a tool.
    Assistant,
    /// One tool was called and returned. Call and result are one step rather
    /// than two because they are one event to anybody reading it, and because
    /// the pair is what carries the timing.
    ToolCall,
    /// The engine itself has something to say — a ceiling reached, a limit hit.
    ///
    /// These used to be `log::warn!` and nowhere else, which meant the only
    /// person who could find out that an answer came from a cut-short
    /// investigation was somebody reading a log file.
    Note,
}

/// One thing that happened, in order.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Step {
    pub index: u32,
    pub kind: StepKind,
    /// Which round of the ask-run-ask loop this belongs to.
    pub iteration: u8,
    /// The tool's name, on a `ToolCall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// What it was called with, on a `ToolCall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
    /// Whether a `ToolCall` returned something other than an error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// What it would take to undo this step.
    ///
    /// Here rather than only in the registry because the transcript is where
    /// somebody goes when they want to know what Syn did to their vault, and
    /// "this one created a node, and trash_node puts it back" is the sentence
    /// they are looking for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reversal: Option<crate::syn::registry::Reversal>,
    /// The opening of what came back, capped at `MAX_STEP_PREVIEW`.
    pub preview: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    pub ms: u64,
    pub at: String,
}

/// The parts of a step a caller supplies. `index` and `at` are not among them.
struct NewStep<'a> {
    kind: StepKind,
    iteration: u8,
    tool: Option<&'a str>,
    args: Option<Value>,
    ok: Option<bool>,
    reversal: Option<crate::syn::registry::Reversal>,
    preview: &'a str,
    tokens: Option<u64>,
    ms: u64,
}

impl NewStep<'_> {
    /// Everything absent, for the recorders that only fill in two or three
    /// fields. `kind` and `preview` are always overridden by the caller.
    fn blank() -> Self {
        Self {
            kind: StepKind::Note,
            iteration: 0,
            tool: None,
            args: None,
            ok: None,
            reversal: None,
            preview: "",
            tokens: None,
            ms: 0,
        }
    }
}

/// Ceilings for one run.
///
/// `None` means no ceiling of that kind, which is the honest default for
/// tokens: providers report them inconsistently and some not at all, so a
/// budget denominated in them would bind on one provider and never on another.
///
/// There is no money field. Adding one would mean either a price table this
/// app would have to keep correct as providers change their prices, or a number
/// the user types in and nothing checks. Neither is worth shipping before
/// anything can spend money without being asked — which is P4's problem.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Rounds of the ask-run-ask loop.
    ///
    /// This is `max_tool_iterations` from settings, and it lives here rather
    /// than as a separate argument because it is the same kind of thing as the
    /// rest: a ceiling on how much work one request may cause. Keeping it apart
    /// meant two ways to run out, reported two different ways, only one of
    /// which the user could see.
    #[serde(default)]
    pub iterations: Option<u8>,
    /// Total tool calls across every round.
    ///
    /// Not a second iteration ceiling. `max_tool_iterations` limits rounds, and
    /// a round may ask for several tools at once — so this is the backstop
    /// against one round asking for fifty, which the round ceiling does not see.
    #[serde(default)]
    pub tool_calls: Option<u32>,
    #[serde(default)]
    pub tokens: Option<u64>,
    /// Wall clock from the first ask to the last.
    #[serde(default)]
    pub wall_ms: Option<u64>,
}

/// Tool calls a round may ask for before the run is stopped, per round allowed.
///
/// Multiplied by `max_tool_iterations` to get the run's ceiling. Four is not a
/// measurement — it is a number large enough that no observed round has come
/// near it, chosen so this limit only ever fires on behaviour nobody intended.
const TOOL_CALLS_PER_ITERATION: u32 = 4;

/// How long one run may take before it is stopped.
///
/// Ten minutes. Long enough for a dozen rounds against a slow local model on
/// somebody's laptop, short enough that a run which has stopped making progress
/// does not sit there until the app is closed.
const DEFAULT_WALL_MS: u64 = 10 * 60 * 1000;

impl Budget {
    /// The ceilings a run gets from the vault's settings.
    pub fn from_settings(settings: &crate::models::syn::SynSettings) -> Self {
        Self {
            iterations: Some(settings.max_tool_iterations),
            tool_calls: Some(settings.max_tool_iterations as u32 * TOOL_CALLS_PER_ITERATION),
            tokens: None,
            wall_ms: Some(DEFAULT_WALL_MS),
        }
    }

    /// Which ceiling `spent` has reached, if any.
    ///
    /// Returns the name rather than a bool so the run can say which one, both
    /// in its own transcript and to the user. "Syn stopped" and "Syn stopped
    /// after ten minutes" are different messages.
    pub fn exceeded_by(&self, spent: &Spent) -> Option<&'static str> {
        if self.iterations.is_some_and(|cap| spent.iterations >= cap) {
            return Some("iterations");
        }
        if self.tool_calls.is_some_and(|cap| spent.tool_calls >= cap) {
            return Some("tool_calls");
        }
        if self.tokens.is_some_and(|cap| spent.tokens >= cap) {
            return Some("tokens");
        }
        if self.wall_ms.is_some_and(|cap| spent.wall_ms >= cap) {
            return Some("wall_ms");
        }
        None
    }
}

/// What a run has used so far.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Spent {
    pub iterations: u8,
    pub tool_calls: u32,
    pub tokens: u64,
    pub wall_ms: u64,
}

/// One piece of work, from the sentence that asked for it to whatever came out.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Run {
    pub id: String,
    /// The conversation this belongs to, or `None` for a run nobody is watching.
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// What the user asked for, in their words. Not a summary and not a
    /// rewrite: the point of keeping it is to be able to see, later, what was
    /// actually asked rather than what the model decided it meant.
    pub goal: String,
    #[serde(default)]
    pub trigger: Trigger,
    pub state: RunState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<SynProvider>,
    pub budget: Budget,
    #[serde(default)]
    pub spent: Spent,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Run {
    pub fn new(
        goal: impl Into<String>,
        conversation_id: Option<String>,
        budget: Budget,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id,
            goal: goal.into(),
            trigger: Trigger::User,
            state: RunState::Working,
            model: None,
            provider: None,
            budget,
            spent: Spent::default(),
            steps: Vec::new(),
            error: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Append a step, number it, and charge what it cost.
    ///
    /// Private, and the three recorders below are the way in. A caller that
    /// can set `index` is a caller that eventually skips one, and a caller
    /// that can set `kind` freely is one that records a tool call as an
    /// assistant turn and stops the budget from seeing it.
    fn push(&mut self, step: NewStep<'_>) {
        if matches!(step.kind, StepKind::ToolCall) {
            self.spent.tool_calls += 1;
        }
        self.spent.tokens += step.tokens.unwrap_or(0);

        let now = chrono::Utc::now().to_rfc3339();
        self.steps.push(Step {
            index: self.steps.len() as u32,
            kind: step.kind,
            iteration: step.iteration,
            tool: step.tool.map(str::to_string),
            args: step.args,
            ok: step.ok,
            reversal: step.reversal,
            preview: step.preview.chars().take(MAX_STEP_PREVIEW).collect(),
            tokens: step.tokens,
            ms: step.ms,
            at: now.clone(),
        });
        self.updated_at = now;
    }

    /// The model said something — the answer, or the words before a tool call.
    pub fn record_assistant(&mut self, iteration: u8, text: &str, tokens: Option<u64>, ms: u64) {
        self.push(NewStep {
            kind: StepKind::Assistant,
            iteration,
            preview: text,
            tokens,
            ms,
            ..NewStep::blank()
        });
    }

    /// One tool was called and came back.
    #[allow(clippy::too_many_arguments)]
    pub fn record_tool(
        &mut self,
        iteration: u8,
        tool: &str,
        args: Value,
        ok: bool,
        reversal: crate::syn::registry::Reversal,
        preview: &str,
        ms: u64,
    ) {
        self.push(NewStep {
            kind: StepKind::ToolCall,
            iteration,
            tool: Some(tool),
            args: Some(args),
            ok: Some(ok),
            reversal: Some(reversal),
            preview,
            tokens: None,
            ms,
        });
    }

    /// Record something the engine wants a person to know.
    pub fn note(&mut self, iteration: u8, text: impl AsRef<str>) {
        self.push(NewStep {
            kind: StepKind::Note,
            iteration,
            preview: text.as_ref(),
            ..NewStep::blank()
        });
    }

    /// Move to a final state, unless one was already reached.
    ///
    /// Idempotent on purpose: the engine has several ways out and each of them
    /// wants to say how it ended, but the first one to say it is the true one —
    /// a run cancelled by the user and then hitting its wall clock on the way
    /// out was cancelled.
    pub fn finish(&mut self, state: RunState) {
        if self.state.is_final() {
            return;
        }
        self.state = state;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        if self.state.is_final() {
            return;
        }
        self.error = Some(message.into());
        self.finish(RunState::Failed);
    }

    pub fn summary(&self) -> RunSummary {
        RunSummary {
            id: self.id.clone(),
            conversation_id: self.conversation_id.clone(),
            goal: self.goal.chars().take(200).collect(),
            trigger: self.trigger,
            state: self.state,
            model: self.model.clone(),
            step_count: self.steps.len(),
            tool_calls: self.spent.tool_calls,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

/// A run as a list needs it: everything except the transcript.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunSummary {
    pub id: String,
    pub conversation_id: Option<String>,
    pub goal: String,
    pub trigger: Trigger,
    pub state: RunState,
    pub model: Option<String>,
    pub step_count: usize,
    pub tool_calls: u32,
    pub created_at: String,
    pub updated_at: String,
}

// ═══════════════════════════════════════════════════════════════
//  ON DISK
// ═══════════════════════════════════════════════════════════════

fn runs_dir(vault_path: &str) -> AppResult<PathBuf> {
    let dir = Path::new(vault_path).join("Syn").join("runs");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::General(format!("Failed to create Syn/runs directory: {e}")))?;
    Ok(dir)
}

fn run_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// Write through a temp file, so a crash mid-write leaves the previous
/// transcript rather than half of the new one.
fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::General(format!("Failed to rename temp run file: {e}"))
    })?;
    Ok(())
}

/// Persist a run as it currently stands.
///
/// Called after every step, so it has to stay cheap: one serialisation and one
/// rename of a file measured in kilobytes.
pub fn save_run(vault_path: &str, run: &Run) -> AppResult<()> {
    let dir = runs_dir(vault_path)?;
    let json = serde_json::to_string_pretty(run)?;
    atomic_write(&run_path(&dir, &run.id), &json)
}

/// The same, but a failure is logged rather than propagated.
///
/// Used from inside the driving loop, where the run's own progress must not be
/// abandoned because a disk write failed. Losing the transcript is bad; losing
/// the answer the user is waiting for because the transcript could not be
/// written is worse.
pub fn save_run_best_effort(vault_path: &str, run: &Run) {
    if let Err(e) = save_run(vault_path, run) {
        log::warn!("[Syn] Could not write transcript for run {}: {}", run.id, e);
    }
}

pub fn get_run(vault_path: &str, id: &str) -> AppResult<Run> {
    let dir = runs_dir(vault_path)?;
    let path = run_path(&dir, id);
    if !path.exists() {
        return Err(AppError::General(format!("Run not found: {id}")));
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Every run on disk, newest first.
///
/// Also the place a run left `Working` by a process that is no longer running
/// is put right — see `RunState::Interrupted`. `live` answers whether this
/// process is currently driving a given id; anything else claiming to be
/// working is claiming it about a process that ended.
pub fn list_runs(vault_path: &str, live: impl Fn(&str) -> bool) -> AppResult<Vec<RunSummary>> {
    let dir = runs_dir(vault_path)?;
    let mut summaries = Vec::new();

    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[Syn] Skipping unreadable run file {:?}: {}", path.file_name(), e);
                continue;
            }
        };
        let mut run: Run = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("[Syn] Skipping corrupt run file {:?}: {}", path.file_name(), e);
                continue;
            }
        };

        if run.state == RunState::Working && !live(&run.id) {
            run.state = RunState::Interrupted;
            if let Err(e) = save_run(vault_path, &run) {
                log::warn!("[Syn] Could not mark run {} interrupted: {}", run.id, e);
            }
        }

        summaries.push(run.summary());
    }

    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(summaries)
}

/// Remove the oldest runs past `KEEP_RUNS`.
///
/// Called when a run is created rather than when one is saved: pruning reads
/// the whole directory, and doing that once per step would cost more than
/// everything else the loop does.
pub fn prune_runs(vault_path: &str) {
    let Ok(dir) = runs_dir(vault_path) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };

    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();

    if files.len() <= KEEP_RUNS {
        return;
    }

    // Oldest first, so the tail past the cap is what goes.
    files.sort_by_key(|(modified, _)| *modified);
    for (_, path) in files.iter().take(files.len() - KEEP_RUNS) {
        if let Err(e) = std::fs::remove_file(path) {
            log::warn!("[Syn] Could not prune old run {:?}: {}", path.file_name(), e);
        }
    }
}

pub fn delete_run(vault_path: &str, id: &str) -> AppResult<()> {
    let dir = runs_dir(vault_path)?;
    let path = run_path(&dir, id);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> Budget {
        Budget {
            iterations: Some(12),
            tool_calls: Some(4),
            tokens: None,
            wall_ms: Some(1000),
        }
    }

    #[test]
    fn steps_are_numbered_by_the_run_and_not_by_the_caller() {
        let mut run = Run::new("do a thing", None, budget());
        for _ in 0..3 {
            run.note(0, "x");
        }
        assert_eq!(
            run.steps.iter().map(|s| s.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    /// Only tool calls count against the tool budget. An assistant turn is not
    /// a tool call, and counting it would make the ceiling fire early on a
    /// conversation that never used a tool at all.
    #[test]
    fn only_tool_calls_are_charged_to_the_tool_budget() {
        let mut run = Run::new("g", None, budget());
        run.record_assistant(0, "hi", Some(10), 5);
        run.note(0, "note");
        assert_eq!(run.spent.tool_calls, 0);

        run.record_tool(
            0,
            "query_nodes",
            serde_json::json!({}),
            true,
            crate::syn::registry::Reversal::Nothing,
            "{}",
            3,
        );
        assert_eq!(run.spent.tool_calls, 1);
        assert_eq!(run.spent.tokens, 10);
    }

    #[test]
    fn a_budget_says_which_ceiling_it_was() {
        let b = Budget {
            iterations: Some(3),
            tool_calls: Some(2),
            tokens: Some(100),
            wall_ms: Some(500),
        };
        assert_eq!(b.exceeded_by(&Spent::default()), None);
        assert_eq!(
            b.exceeded_by(&Spent { tool_calls: 2, ..Default::default() }),
            Some("tool_calls")
        );
        assert_eq!(
            b.exceeded_by(&Spent { iterations: 3, ..Default::default() }),
            Some("iterations")
        );
        assert_eq!(
            b.exceeded_by(&Spent { wall_ms: 900, ..Default::default() }),
            Some("wall_ms")
        );
    }

    /// A `None` ceiling is no ceiling, not a ceiling of zero. Getting this
    /// backwards would stop every run before its first tool call.
    #[test]
    fn an_absent_ceiling_never_fires() {
        let b = Budget { iterations: None, tool_calls: None, tokens: None, wall_ms: None };
        let spent = Spent { iterations: 255, tool_calls: 9999, tokens: 9_999_999, wall_ms: 9_999_999 };
        assert_eq!(b.exceeded_by(&spent), None);
    }

    /// The first ending is the true one. A run the user cancelled must not be
    /// relabelled by whatever the loop notices on its way out.
    #[test]
    fn the_first_ending_wins() {
        let mut run = Run::new("g", None, budget());
        run.finish(RunState::Cancelled);
        run.finish(RunState::Done);
        run.fail("something else");
        assert_eq!(run.state, RunState::Cancelled);
        assert_eq!(run.error, None);
    }

    #[test]
    fn a_run_survives_a_round_trip_through_the_vault() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault = dir.path().to_str().expect("utf8");

        let mut run = Run::new("tổng kết tuần", Some("conv-1".into()), budget());
        run.model = Some("gpt-5.6-luna".into());
        run.record_tool(
            0,
            "query_nodes",
            serde_json::json!({ "query": "type:task" }),
            true,
            crate::syn::registry::Reversal::Nothing,
            "{\"total_matches\":3}",
            12,
        );
        save_run(vault, &run).expect("saves");

        let back = get_run(vault, &run.id).expect("loads");
        assert_eq!(back.goal, "tổng kết tuần");
        assert_eq!(back.steps.len(), 1);
        assert_eq!(back.steps[0].tool.as_deref(), Some("query_nodes"));
        assert_eq!(back.spent.tool_calls, 1);
        // The transcript is where somebody looks to find out what can be undone.
        assert_eq!(
            back.steps[0].reversal,
            Some(crate::syn::registry::Reversal::Nothing)
        );
    }

    /// The case the transcript exists for: the process died mid-run. Nothing
    /// is driving it, so the panel must not show it as working forever.
    #[test]
    fn a_run_nothing_is_driving_stops_claiming_to_be_working() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault = dir.path().to_str().expect("utf8");

        let run = Run::new("interrupted", None, budget());
        save_run(vault, &run).expect("saves");

        let listed = list_runs(vault, |_| false).expect("lists");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state, RunState::Interrupted);

        // And it was written back, not just reported.
        assert_eq!(get_run(vault, &run.id).expect("loads").state, RunState::Interrupted);
    }

    #[test]
    fn a_run_this_process_is_driving_is_left_alone() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault = dir.path().to_str().expect("utf8");

        let run = Run::new("working", None, budget());
        save_run(vault, &run).expect("saves");

        let listed = list_runs(vault, |_| true).expect("lists");
        assert_eq!(listed[0].state, RunState::Working);
    }

    #[test]
    fn a_corrupt_transcript_does_not_take_the_listing_with_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault = dir.path().to_str().expect("utf8");

        let run = Run::new("good", None, budget());
        save_run(vault, &run).expect("saves");
        std::fs::write(
            runs_dir(vault).expect("dir").join("broken.json"),
            "{ not json",
        )
        .expect("write");

        let listed = list_runs(vault, |_| true).expect("lists");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].goal, "good");
    }

    #[test]
    fn a_long_result_is_kept_only_as_far_as_the_preview_cap() {
        let mut run = Run::new("g", None, budget());
        let huge = "x".repeat(MAX_STEP_PREVIEW * 2);
        run.record_assistant(0, &huge, None, 0);
        assert_eq!(run.steps[0].preview.chars().count(), MAX_STEP_PREVIEW);
    }

    /// Multi-byte text must be cut on character boundaries, not byte ones —
    /// `String::truncate` on a Vietnamese tool result would panic.
    #[test]
    fn the_preview_cap_counts_characters_not_bytes() {
        let mut run = Run::new("g", None, budget());
        let vietnamese = "đường".repeat(MAX_STEP_PREVIEW);
        run.record_assistant(0, &vietnamese, None, 0);
        assert_eq!(run.steps[0].preview.chars().count(), MAX_STEP_PREVIEW);
    }
}

/// The two sides of the wire have to agree about what a run is.
///
/// Nothing links `RunState` to the union in `messages/types.ts`, and the cost
/// of them drifting is quiet: an unknown state falls through the panel's lookup
/// and draws a grey dot with no label, on a run that may well have failed. The
/// same arrangement `NodeType` and `SynSettings` already have, for the same
/// reason — a fact written down twice drifts, so one of the copies is read
/// rather than remembered.
#[cfg(test)]
mod agreement {
    use super::*;

    fn frontend_types() -> String {
        std::fs::read_to_string("../src/mini-apps/messages/types.ts")
            .expect("the messages types should be readable from src-tauri")
    }

    /// The union the panel switches on, as a list of its members.
    fn declared_union(source: &str, name: &str) -> Vec<String> {
        let body = source
            .split(&format!("export type {name} ="))
            .nth(1)
            .unwrap_or_else(|| panic!("types.ts should still declare `{name}`"))
            .split(';')
            .next()
            .expect("the declaration closes");

        body.split('|')
            .map(|part| {
                // Each arm may carry a `//` comment line above it.
                part.lines()
                    .map(str::trim)
                    .find(|l| l.starts_with('\'') || l.starts_with('"'))
                    .unwrap_or("")
                    .trim_matches(|c| c == '\'' || c == '"')
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }

    #[test]
    fn every_run_state_is_one_the_panel_can_draw() {
        let source = frontend_types();
        let declared = declared_union(&source, "RunState");

        let states = [
            RunState::Working,
            RunState::Done,
            RunState::Failed,
            RunState::Cancelled,
            RunState::BudgetExhausted,
            RunState::Interrupted,
        ];

        for state in states {
            let wire = serde_json::to_value(state)
                .expect("serialises")
                .as_str()
                .expect("a string")
                .to_string();
            assert!(
                declared.contains(&wire),
                "the backend can write run state `{wire}`, which `RunState` in types.ts does \
                 not list: {declared:?}"
            );
        }

        assert_eq!(
            declared.len(),
            states.len(),
            "types.ts lists {} run states and the backend has {}: {declared:?}",
            declared.len(),
            states.len()
        );
    }

    #[test]
    fn every_step_kind_is_one_the_panel_can_draw() {
        let source = frontend_types();
        let declared = declared_union(&source, "StepKind");

        let kinds = [StepKind::Assistant, StepKind::ToolCall, StepKind::Note];
        for kind in kinds {
            let wire = serde_json::to_value(kind)
                .expect("serialises")
                .as_str()
                .expect("a string")
                .to_string();
            assert!(
                declared.contains(&wire),
                "the backend can write step kind `{wire}`, which types.ts does not list: \
                 {declared:?}"
            );
        }
        assert_eq!(declared.len(), kinds.len());
    }

    /// The panel reads `reversal.kind` to decide what to say about undoing a
    /// step. A tag it does not know renders as nothing at all.
    #[test]
    fn every_reversal_tag_is_one_the_panel_can_read() {
        let source = frontend_types();
        let reversals = [
            crate::syn::registry::Reversal::Nothing,
            crate::syn::registry::Reversal::Automatic { how: "x".into() },
            crate::syn::registry::Reversal::Manual { how: "x".into() },
            crate::syn::registry::Reversal::Irreversible,
        ];

        for reversal in reversals {
            let tag = serde_json::to_value(&reversal)
                .expect("serialises")
                .get("kind")
                .and_then(|k| k.as_str())
                .expect("tagged with a `kind`")
                .to_string();
            assert!(
                source.contains(&format!("kind: '{tag}'")),
                "the backend can write reversal `{tag}`, which types.ts does not handle"
            );
        }
    }
}
