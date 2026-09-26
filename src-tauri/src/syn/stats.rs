//! How often what Syn has actually fires, in numbers.
//!
//! # Why this exists
//!
//! Because the same thing keeps happening in this codebase: a mechanism is
//! built, tested, shipped, and never runs. `recall` went uncalled across fifteen
//! real runs. The skill detector fired on none of seventeen. The thread counter
//! read zero of twenty-five. Each was found by somebody counting by hand, weeks
//! later, and each would have been visible in a day if a number had been on a
//! screen. The 5 September review named the gap G1 — *no number in the app
//! answers "how many times did Syn use memory or a skill in thirty days"* — and
//! ranked it the highest-value thing nobody had done.
//!
//! So this is that screen's arithmetic: one pure function over the runs already
//! on disk, which already record every tool call, every ending and every token,
//! plus the handful of facts about the prompt that only the prompt knew and that
//! `Run` now writes down (`memory_lines_sent` and its neighbours).
//!
//! # What it is not
//!
//! Not telemetry. Nothing here is sent anywhere; it is read from the vault, on
//! this device, when somebody opens the screen, and forgotten when they close
//! it. That is a promise the screen makes out loud, and the reason it can make
//! it is that there is nowhere in this module for a number to go but back to
//! the caller.
//!
//! Not history either. It covers the runs `run::KEEP_RUNS` keeps, so "all time"
//! means "since the oldest run still on disk", and the screen says which date
//! that is rather than letting the phrase claim more than it knows.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, TimeZone};
use serde::Serialize;

use crate::syn::prompt::{PromptPlan, SectionKind};
use crate::syn::run::{Run, RunState, StepKind};

/// How far back "recently" reaches.
///
/// Thirty days, because it is the window the review's question is asked in
/// and long enough that a tool used once a week shows up four times rather
/// than as a coin toss.
pub const RECENT_DAYS: i64 = 30;

/// The tools whose firing is the question.
///
/// Not every tool: the ones that are Syn's own machinery rather than plain
/// reads of the vault, each of which was built on a bet that the model would
/// reach for it. A `query_nodes` count says Syn searched; a `load_skill` count
/// says whether skills are anything but a list in the prompt.
pub const WATCHED_TOOLS: [&str; 7] = [
    crate::syn::skill::LOAD_TOOL,
    crate::syn::recipe::RUN_TOOL,
    "remember",
    "recall",
    crate::syn::tools::LOOK_BACK_TOOL,
    crate::syn::tools::BROWSE_TOOL,
    crate::syn::tools::PLAN_TOOL,
];

// ═══════════════════════════════════════════════════════════════
//  WHAT THE PROMPT CARRIED
// ═══════════════════════════════════════════════════════════════

/// What one turn's prompt carried, measured where it was assembled.
///
/// Its own value rather than five arguments because it is measured in one
/// step (`messages_for`) and written in another (`start_run`), and a struct
/// cannot have two of its numbers swapped without the compiler noticing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carried {
    pub memory_lines_sent: u32,
    pub memory_dropped: u32,
    pub sections_dropped: Vec<String>,
    pub retrieval_ms: Option<u64>,
    pub skills_indexed: u32,
    /// The skill whose steps the harness put in the prompt, by name — `None`
    /// when none was chosen, or when `fit` cut the section it rode in.
    ///
    /// Measured here with the rest, and **not yet written onto the run**: `Run`
    /// has no field for it. Without one, a run that followed an injected skill
    /// looks in `syn::stats` exactly like a run that ignored every skill, since
    /// no `load_skill` call appears in its transcript — and Gate D asks for
    /// "loaded *or injected*". `write_onto` says where it should go.
    pub skill_injected: Option<String>,
}

impl Carried {
    /// Measure a plan that is about to be sent.
    ///
    /// `memories` is how many the store holds — every one of them was handed
    /// to `memory_block`, so whatever the prompt does not show was left out,
    /// by the block's own budget or by `fit`. Counting it that way rather than
    /// by parsing the block's "N more … left out" line means there is one
    /// thing to get right, `memory::lines_shown`, instead of two.
    pub fn of(plan: &PromptPlan, memories: usize, retrieval_ms: Option<u64>) -> Self {
        let sent = plan
            .body(SectionKind::Memory)
            .map(crate::syn::memory::lines_shown)
            .unwrap_or(0);
        let skills = plan
            .body(SectionKind::Skills)
            .map(crate::syn::skill::lines_indexed)
            .unwrap_or(0);
        Self {
            memory_lines_sent: sent as u32,
            memory_dropped: memories.saturating_sub(sent) as u32,
            sections_dropped: plan.dropped().iter().map(name_of).collect(),
            retrieval_ms,
            skills_indexed: skills as u32,
            skill_injected: plan
                .body(SectionKind::ChosenSkill)
                .and_then(crate::syn::skill::chosen_name),
        }
    }

    /// Onto the run, before it is driven — the engine saves the run on its
    /// first step, so these reach the disk without anything else saving it.
    pub fn write_onto(self, run: &mut Run) {
        run.memory_lines_sent = Some(self.memory_lines_sent);
        run.memory_dropped = Some(self.memory_dropped);
        run.sections_dropped = self.sections_dropped;
        run.retrieval_ms = self.retrieval_ms;
        run.skills_indexed = Some(self.skills_indexed);
        // `skill_injected` belongs on the run as `Run::skill_injected:
        // Option<String>` (serde default, skipped when `None`), written here as
        // `run.skill_injected = self.skill_injected;`, and counted beside
        // `skills.loaded` in `stats`. Left for whoever owns `run.rs`.
    }
}

// ═══════════════════════════════════════════════════════════════
//  THE NUMBERS
// ═══════════════════════════════════════════════════════════════

/// How many rounds runs took, in the buckets a person reads at a glance.
///
/// Rounds rather than tool calls, because a round is what costs: each one
/// re-sends the whole conversation. A run of one round answered straight away;
/// a pile-up past ten is a model searching rather than answering.
#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Rounds {
    /// Never reached the model — failed or stopped before the first round.
    pub none: u32,
    pub one: u32,
    pub two: u32,
    pub three_to_five: u32,
    pub six_to_ten: u32,
    pub over_ten: u32,
}

impl Rounds {
    fn count(&mut self, rounds: u8) {
        match rounds {
            0 => self.none += 1,
            1 => self.one += 1,
            2 => self.two += 1,
            3..=5 => self.three_to_five += 1,
            6..=10 => self.six_to_ten += 1,
            _ => self.over_ten += 1,
        }
    }
}

/// How often one watched tool was reached for.
#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolFiring {
    pub tool: String,
    /// Every call, including the ones that came back an error.
    pub calls: u32,
    /// Runs in which it succeeded at least once. The number that says it is
    /// used, as opposed to tried.
    pub runs: u32,
}

/// Memory, as the prompts carried it.
#[derive(Serialize, Debug, Clone, Default)]
pub struct MemoryFiring {
    /// Runs whose prompt was measured at all. The denominator: runs from
    /// before this was recorded are not counted as runs without memory.
    pub measured: u32,
    /// Of those, runs whose prompt carried at least one remembered line.
    pub with_memory: u32,
    pub lines_sent: u32,
    pub lines_dropped: u32,
    /// Lines per run that carried any. `None` when none did.
    pub avg_lines: Option<f64>,
}

/// Whether the prompt had to give something up to fit.
#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptFiring {
    pub measured: u32,
    /// Runs where `fit` cut at least one section whole.
    pub any_dropped: u32,
    /// How often each section was the one cut, by `SectionKind` name.
    pub by_section: BTreeMap<String, u32>,
}

/// Skills: how often the index was on offer, and how often one was opened.
#[derive(Serialize, Debug, Clone, Default)]
pub struct SkillFiring {
    pub measured: u32,
    /// Runs whose prompt named at least one skill.
    pub offered: u32,
    /// Runs that loaded a skill successfully. Counted over every run, since a
    /// `load_skill` call is in the transcript whether or not the prompt was
    /// measured.
    pub loaded: u32,
    /// Which skills, by `skill::usage` — the same list the Skills tab reads.
    pub usage: Vec<crate::syn::skill::Usage>,
}

/// How long retrieval took before the model was asked.
#[derive(Serialize, Debug, Clone, Default)]
pub struct RetrievalTiming {
    /// Runs where retrieval ran and was timed.
    pub measured: u32,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<u64>,
}

/// Everything, over one stretch of runs.
#[derive(Serialize, Debug, Clone, Default)]
pub struct Period {
    pub runs: u32,
    /// By `Surface` name. Every surface is present, at zero if unused — a
    /// surface nobody uses is exactly the thing this is for.
    pub by_surface: BTreeMap<String, u32>,
    pub rounds: Rounds,
    /// By `RunState` name, every state present.
    pub ended: BTreeMap<String, u32>,
    /// Of the runs that ended `budget_exhausted`, which ceiling: `iterations`,
    /// `tool_calls`, `tokens`, `wall_ms`, or `unknown`. See `ceiling_of`.
    pub ceilings: BTreeMap<String, u32>,
    /// In `WATCHED_TOOLS` order.
    pub tools: Vec<ToolFiring>,
    pub memory: MemoryFiring,
    pub prompt: PromptFiring,
    pub skills: SkillFiring,
    pub retrieval: RetrievalTiming,
    /// Tokens charged, input and output, every round.
    pub tokens: u64,
    /// Of the input, how much the provider served from its cache.
    pub tokens_cached: u64,
    pub footing: crate::syn::footing::Tally,
}

/// Tokens charged on one day.
#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct DayTokens {
    /// `YYYY-MM-DD`, in the viewer's own timezone.
    pub day: String,
    pub runs: u32,
    pub tokens: u64,
    pub tokens_cached: u64,
}

/// The whole screen.
#[derive(Serialize, Debug, Clone, Default)]
pub struct Stats {
    pub recent_days: u32,
    /// The last `recent_days` days.
    pub recent: Period,
    /// Every run still on disk.
    pub on_disk: Period,
    /// One entry per day of the recent window, oldest first, empty days
    /// included — a gap in a list of days reads as a day that was not there.
    pub by_day: Vec<DayTokens>,
    /// When the oldest run on disk was started, which is what "all" means.
    pub oldest: Option<String>,
    /// How many runs the vault keeps before the oldest go.
    pub kept: u32,
}

/// Count the runs.
///
/// Pure, over runs the caller already loaded, so it is tested against runs
/// built in memory rather than whatever is in somebody's vault. `now` decides
/// both where the window starts and which timezone a day is in: a day is the
/// viewer's day, not UTC's, or a question asked at 6am in Hà Nội is charged
/// to yesterday.
pub fn stats<Tz: TimeZone>(runs: &[Run], now: DateTime<Tz>) -> Stats {
    let zone = now.timezone();
    let when = |run: &Run| -> Option<DateTime<Tz>> {
        DateTime::parse_from_rfc3339(&run.created_at).ok().map(|t| t.with_timezone(&zone))
    };
    let since = now.clone() - Duration::days(RECENT_DAYS);

    let recent: Vec<Run> = runs
        .iter()
        .filter(|run| when(run).is_some_and(|t| t >= since && t <= now))
        .cloned()
        .collect();

    let today = now.date_naive();
    let first = today - Duration::days(RECENT_DAYS - 1);
    let mut days: BTreeMap<NaiveDate, DayTokens> = (0..RECENT_DAYS)
        .map(|i| first + Duration::days(i))
        .map(|d| (d, DayTokens { day: d.format("%Y-%m-%d").to_string(), ..Default::default() }))
        .collect();
    for run in runs {
        let Some(day) = when(run).map(|t| t.date_naive()) else { continue };
        if let Some(entry) = days.get_mut(&day) {
            entry.runs += 1;
            entry.tokens += run.spent.tokens;
            entry.tokens_cached += cached(run);
        }
    }

    Stats {
        recent_days: RECENT_DAYS as u32,
        recent: period(&recent),
        on_disk: period(runs),
        by_day: days.into_values().collect(),
        oldest: runs.iter().map(|r| r.created_at.clone()).min(),
        kept: crate::syn::run::KEEP_RUNS as u32,
    }
}

fn period(runs: &[Run]) -> Period {
    let mut out = Period {
        runs: runs.len() as u32,
        by_surface: [
            crate::syn::surface::Surface::App,
            crate::syn::surface::Surface::Telegram,
            crate::syn::surface::Surface::Routine,
        ]
            .iter()
            .map(|s| (name_of(s), 0))
            .collect(),
        ended: RunState::ALL.iter().map(|s| (name_of(s), 0)).collect(),
        tools: WATCHED_TOOLS
            .iter()
            .map(|tool| ToolFiring { tool: tool.to_string(), ..Default::default() })
            .collect(),
        footing: crate::syn::footing::tally(runs),
        ..Default::default()
    };
    out.skills.usage = crate::syn::skill::usage(runs);

    let mut retrieval_total = 0u64;
    for run in runs {
        *out.by_surface.entry(name_of(&run.surface)).or_default() += 1;
        *out.ended.entry(name_of(&run.state)).or_default() += 1;
        out.rounds.count(run.spent.iterations);
        if let Some(which) = ceiling_of(run) {
            *out.ceilings.entry(which.to_string()).or_default() += 1;
        }

        for firing in &mut out.tools {
            let calls = run.steps.iter().filter(|s| s.tool.as_deref() == Some(&firing.tool)).count();
            firing.calls += calls as u32;
            if run.successful_calls_of(&firing.tool) > 0 {
                firing.runs += 1;
            }
        }

        // `memory_lines_sent` is the marker that this run's prompt was
        // measured at all: it is written as `Some(0)` when there was nothing
        // to remember, and never left out by a run that was measured.
        if let Some(sent) = run.memory_lines_sent {
            out.memory.measured += 1;
            out.memory.lines_sent += sent;
            out.memory.lines_dropped += run.memory_dropped.unwrap_or(0);
            if sent > 0 {
                out.memory.with_memory += 1;
            }

            out.prompt.measured += 1;
            if !run.sections_dropped.is_empty() {
                out.prompt.any_dropped += 1;
            }
            for section in &run.sections_dropped {
                *out.prompt.by_section.entry(section.clone()).or_default() += 1;
            }
        }

        if let Some(indexed) = run.skills_indexed {
            out.skills.measured += 1;
            if indexed > 0 {
                out.skills.offered += 1;
            }
        }
        if run.successful_calls_of(crate::syn::skill::LOAD_TOOL) > 0 {
            out.skills.loaded += 1;
        }

        if let Some(ms) = run.retrieval_ms {
            out.retrieval.measured += 1;
            retrieval_total += ms;
            out.retrieval.max_ms = Some(out.retrieval.max_ms.map_or(ms, |m| m.max(ms)));
        }

        out.tokens += run.spent.tokens;
        out.tokens_cached += cached(run);
    }

    out.memory.avg_lines = (out.memory.with_memory > 0)
        .then(|| out.memory.lines_sent as f64 / out.memory.with_memory as f64);
    out.retrieval.avg_ms = (out.retrieval.measured > 0)
        .then(|| retrieval_total as f64 / out.retrieval.measured as f64);
    out
}

/// Input tokens the provider served from its cache, across the run's steps.
fn cached(run: &Run) -> u64 {
    run.steps.iter().filter_map(|s| s.usage.input_cached).sum()
}

/// Which ceiling stopped a run, for a run that was stopped by one.
///
/// # Why this reads words, and why that is fragile
///
/// The engine knows which ceiling it hit — `Budget::exceeded_by` returns the
/// name — and writes it down only as a sentence: `ceiling_message` turns it
/// into a `Note` step ("Reached the ceiling of 12 rounds…") and the name itself
/// is not kept. A `Run::ceiling` field set where the run finishes would be the
/// honest fix, and it belongs in `engine.rs`, which this change does not touch.
///
/// So the note is read back. That works for as long as `ceiling_message`
/// keeps its wording, and **it will silently stop working the day somebody
/// rewords it**: every ceiling would then land in the fallback below. The
/// fallback asks the budget again, which is right for the round, time and
/// token ceilings as the run left them, and can name `iterations` for a run
/// whose last round was stopped by its tool count. Whatever neither can name
/// is counted as `unknown` rather than guessed, so a jump in `unknown` on the
/// screen is the sign the wording moved.
pub fn ceiling_of(run: &Run) -> Option<&'static str> {
    if run.state != RunState::BudgetExhausted {
        return None;
    }
    // Recorded by the engine, for every run since it could be. The sentence
    // below is only for runs written before.
    if let Some(which) = run.ceiling.as_deref() {
        return ["iterations", "tool_calls", "tokens", "wall_ms"].into_iter().find(|n| *n == which).or(Some("unknown"));
    }
    let from_note = run
        .steps
        .iter()
        .rev()
        .filter(|s| matches!(s.kind, StepKind::Note))
        .find_map(|s| ceiling_named_in(&s.preview));
    Some(from_note.or_else(|| run.budget.exceeded_by(&run.spent)).unwrap_or("unknown"))
}

/// The ceiling a note from `engine::ceiling_message` names, if it is one.
///
/// Mirrors that function's four sentences. Rounds are tested before tool calls
/// because the rounds sentence also mentions tool calls.
fn ceiling_named_in(text: &str) -> Option<&'static str> {
    if text.starts_with("Reached the ceiling of ") {
        if text.contains(" rounds after ") {
            return Some("iterations");
        }
        if text.contains(" tool calls ") {
            return Some("tool_calls");
        }
    }
    if text.starts_with("Ran for ") && text.contains("this run's limit") {
        return Some("wall_ms");
    }
    if text.starts_with("Reached this run's token ceiling") {
        return Some("tokens");
    }
    let named = text.strip_prefix("Stopped at the ")?.split(" limit").next()?;
    ["iterations", "tool_calls", "tokens", "wall_ms"].into_iter().find(|n| *n == named)
}

/// The name serde gives a value, which is the name the front end knows it by.
fn name_of<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════
//  TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::syn::SynSettings;
    use crate::syn::prompt::{ChatPrompt, DEFAULT_BUDGET_CHARS};
    use crate::syn::provider::Usage;
    use crate::syn::registry::Reversal;
    use crate::syn::run::Budget;
    use chrono::FixedOffset;

    /// Noon on 26 September, in Hà Nội.
    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-09-26T12:00:00+07:00").unwrap()
    }

    fn run_at(created_at: &str) -> Run {
        let mut run = Run::new("hỏi gì đó", None, Budget::from_settings(&SynSettings::default()));
        run.created_at = created_at.to_string();
        run.state = RunState::Done;
        run
    }

    fn tool(run: &mut Run, name: &str, ok: bool) {
        run.record_tool(0, name, serde_json::json!({ "name": "weekly-review" }), ok, Reversal::Nothing, "{}", 1);
    }

    #[test]
    fn nothing_on_disk_is_all_zeros_and_still_thirty_days() {
        let s = stats(&[], now());
        assert_eq!(s.recent.runs, 0);
        assert_eq!(s.by_day.len(), 30, "every day of the window, even an empty one");
        assert_eq!(s.by_day.last().unwrap().day, "2026-09-26");
        assert_eq!(s.oldest, None);
        assert_eq!(s.recent.memory.avg_lines, None, "no average of nothing");
        assert_eq!(s.recent.by_surface.get("telegram"), Some(&0), "an unused surface is shown at zero");
        assert_eq!(s.recent.ended.len(), RunState::ALL.len());
    }

    #[test]
    fn the_recent_window_is_thirty_days_and_on_disk_is_everything() {
        let runs = vec![
            run_at("2026-09-25T10:00:00Z"),
            run_at("2026-08-01T10:00:00Z"),
        ];
        let s = stats(&runs, now());
        assert_eq!(s.recent.runs, 1);
        assert_eq!(s.on_disk.runs, 2);
        assert_eq!(s.oldest.as_deref(), Some("2026-08-01T10:00:00Z"));
    }

    /// 23:30 UTC on the 25th is 06:30 on the 26th in Hà Nội, and that is the
    /// day the person asked on.
    #[test]
    fn a_day_is_the_viewers_day_and_not_utcs() {
        let mut run = run_at("2026-09-25T23:30:00Z");
        run.spent.tokens = 1_000;
        let s = stats(&[run], now());
        let today = s.by_day.last().unwrap();
        assert_eq!((today.day.as_str(), today.tokens, today.runs), ("2026-09-26", 1_000, 1));
    }

    #[test]
    fn tokens_are_summed_per_day_with_the_cached_part_beside_them() {
        let mut a = run_at("2026-09-20T03:00:00Z");
        a.record_assistant(0, "x", Usage { input: Some(900), input_cached: Some(600), output: Some(100), ..Default::default() }, 1);
        let mut b = run_at("2026-09-20T04:00:00Z");
        b.record_assistant(0, "y", Usage { input: Some(40), output: Some(10), ..Default::default() }, 1);
        let s = stats(&[a, b], now());
        let day = s.by_day.iter().find(|d| d.day == "2026-09-20").unwrap();
        assert_eq!((day.runs, day.tokens, day.tokens_cached), (2, 1_050, 600));
        assert_eq!(s.recent.tokens, 1_050);
        assert_eq!(s.recent.tokens_cached, 600);
    }

    #[test]
    fn rounds_fall_into_the_buckets_a_person_reads() {
        let runs: Vec<Run> = [0u8, 1, 2, 3, 5, 6, 10, 11, 25]
            .into_iter()
            .map(|n| {
                let mut r = run_at("2026-09-25T10:00:00Z");
                r.spent.iterations = n;
                r
            })
            .collect();
        let r = stats(&runs, now()).recent.rounds;
        assert_eq!(
            (r.none, r.one, r.two, r.three_to_five, r.six_to_ten, r.over_ten),
            (1, 1, 1, 2, 2, 2)
        );
    }

    #[test]
    fn runs_are_counted_by_where_they_came_from_and_how_they_ended() {
        let mut phone = run_at("2026-09-25T10:00:00Z");
        phone.surface = crate::syn::surface::Surface::Telegram;
        phone.state = RunState::AwaitingConsent;
        let app = run_at("2026-09-25T11:00:00Z");
        let s = stats(&[phone, app], now()).recent;
        assert_eq!(s.by_surface.get("telegram"), Some(&1));
        assert_eq!(s.by_surface.get("app"), Some(&1));
        assert_eq!(s.ended.get("awaiting_consent"), Some(&1));
        assert_eq!(s.ended.get("done"), Some(&1));
    }

    /// The sentences are copied from `engine::ceiling_message`. If this test
    /// still passes after that function is reworded, this copy was not updated
    /// with it — and the screen will start counting ceilings as `unknown`.
    #[test]
    fn a_ceiling_is_named_from_the_note_the_engine_left() {
        let cases = [
            ("Reached the ceiling of 12 rounds after 9 tool call(s) — answering from an unfinished investigation. Raise `max_tool_iterations` in Syn settings if this keeps happening.", "iterations"),
            ("Reached the ceiling of 48 tool calls — answering from an unfinished investigation.", "tool_calls"),
            ("Ran for 600 seconds, which is this run's limit — answering from an unfinished investigation.", "wall_ms"),
            ("Reached this run's token ceiling — answering from an unfinished investigation.", "tokens"),
            ("Stopped at the wall_ms limit — answering from an unfinished investigation.", "wall_ms"),
        ];
        for (note, expected) in cases {
            let mut run = run_at("2026-09-25T10:00:00Z");
            run.state = RunState::BudgetExhausted;
            run.note(3, "Condensed the 4 earlier messages of this conversation.");
            run.note(3, note);
            assert_eq!(ceiling_of(&run), Some(expected), "{note}");
        }
    }

    #[test]
    fn a_ceiling_with_no_note_is_asked_of_the_budget_or_left_unknown() {
        let mut run = run_at("2026-09-25T10:00:00Z");
        run.state = RunState::BudgetExhausted;
        run.budget.iterations = Some(3);
        run.spent.iterations = 3;
        assert_eq!(ceiling_of(&run), Some("iterations"));

        run.spent.iterations = 1;
        assert_eq!(ceiling_of(&run), Some("unknown"), "not guessed");

        run.state = RunState::Done;
        assert_eq!(ceiling_of(&run), None, "only a run a ceiling stopped has one");

        let mut stopped = run_at("2026-09-25T10:00:00Z");
        stopped.state = RunState::BudgetExhausted;
        stopped.note(2, "Reached the ceiling of 8 tool calls — answering from an unfinished investigation.");
        let s = stats(&[stopped], now()).recent;
        assert_eq!(s.ceilings.get("tool_calls"), Some(&1));
    }

    #[test]
    fn a_watched_tool_counts_its_calls_and_the_runs_it_worked_in() {
        let mut a = run_at("2026-09-25T10:00:00Z");
        tool(&mut a, "load_skill", false);
        tool(&mut a, "load_skill", true);
        tool(&mut a, "query_nodes", true);
        let mut b = run_at("2026-09-25T11:00:00Z");
        tool(&mut b, "load_skill", false);
        tool(&mut b, "remember", true);

        let s = stats(&[a, b], now()).recent;
        let of = |name: &str| s.tools.iter().find(|t| t.tool == name).cloned().unwrap();
        assert_eq!((of("load_skill").calls, of("load_skill").runs), (3, 1), "tried three times, worked in one run");
        assert_eq!(of("remember").runs, 1);
        assert_eq!(of("recall").calls, 0, "present at zero, which is the point");
        assert!(s.tools.iter().all(|t| t.tool != "query_nodes"), "only the watched ones");
        assert_eq!(s.skills.loaded, 1);
        assert_eq!(s.skills.usage.len(), 1, "the same list skill::usage gives");
    }

    /// An old run is not a run without memory. Counting it as one would open
    /// the screen on "memory reached the model in 0% of runs", which is the
    /// confident wrong number `footing` already learned not to show.
    #[test]
    fn a_run_from_before_the_prompt_was_measured_is_not_counted_as_carrying_nothing() {
        let old = run_at("2026-09-25T10:00:00Z");
        let mut none = run_at("2026-09-25T11:00:00Z");
        Carried::default().write_onto(&mut none);
        let mut some = run_at("2026-09-25T12:00:00Z");
        Carried {
            memory_lines_sent: 6,
            memory_dropped: 2,
            sections_dropped: vec!["vault_context".into()],
            retrieval_ms: Some(40),
            skills_indexed: 3,
            skill_injected: None,
        }
        .write_onto(&mut some);
        let mut other = run_at("2026-09-25T13:00:00Z");
        Carried { memory_lines_sent: 2, retrieval_ms: Some(80), ..Default::default() }.write_onto(&mut other);

        let s = stats(&[old, none, some, other], now()).recent;
        assert_eq!(s.memory.measured, 3, "the old run is not in the denominator");
        assert_eq!(s.memory.with_memory, 2);
        assert_eq!((s.memory.lines_sent, s.memory.lines_dropped), (8, 2));
        assert_eq!(s.memory.avg_lines, Some(4.0), "per run that carried any");
        assert_eq!((s.prompt.measured, s.prompt.any_dropped), (3, 1));
        assert_eq!(s.prompt.by_section.get("vault_context"), Some(&1));
        assert_eq!((s.skills.measured, s.skills.offered), (3, 1));
        assert_eq!((s.retrieval.measured, s.retrieval.avg_ms, s.retrieval.max_ms), (2, Some(60.0), Some(80)));
    }

    #[test]
    fn the_footing_tally_is_the_one_footing_keeps() {
        let mut a = run_at("2026-09-25T10:00:00Z");
        a.footing = Some(crate::syn::footing::Footing::Grounded);
        let b = run_at("2026-09-25T11:00:00Z");
        let s = stats(&[a, b], now()).recent;
        assert_eq!((s.footing.grounded, s.footing.unmeasured), (1, 1));
    }

    fn node(title: &str, body: &str, props: serde_json::Value) -> crate::models::node::NodeMetadata {
        crate::models::node::NodeMetadata {
            id: format!("x/{title}.md"),
            node_type: "x".into(),
            title: title.to_string(),
            content: body.to_string(),
            properties: props,
            created_at: "2026-09-01T00:00:00Z".into(),
            updated_at: "2026-09-01T00:00:00Z".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn memory(text: &str) -> crate::syn::memory::Memory {
        crate::syn::memory::Memory::from_node(&node(
            text,
            text,
            serde_json::json!({ "kind": "fact", "pinned": true, "confidence": 0.9, "last_confirmed": "2026-09-20" }),
        ))
    }

    fn plan(memory_block: Option<&str>, skills: Option<&str>, context: &str, budget: usize) -> PromptPlan {
        PromptPlan::for_chat(ChatPrompt {
            context,
            custom: None,
            skills,
            memory: memory_block,
            focus: None,
            thread: None,
            counted: None,
            timeline: None,
            budget_chars: budget,
        })
    }

    #[test]
    fn what_the_prompt_carried_is_counted_off_the_prompt_as_sent() {
        let memories: Vec<_> = ["một", "hai", "ba"].into_iter().map(memory).collect();
        let block = crate::syn::memory::memory_block(&memories, crate::syn::memory::MEMORY_BUDGET_CHARS);
        let skills = "\n\n=== WHAT YOU KNOW HOW TO DO ===\n- a\n=== END ===";
        let carried = Carried::of(&plan(block.as_deref(), Some(skills), "", DEFAULT_BUDGET_CHARS), 3, Some(12));
        assert_eq!((carried.memory_lines_sent, carried.memory_dropped), (3, 0));
        assert!(carried.sections_dropped.is_empty());
        assert_eq!(carried.retrieval_ms, Some(12));
    }

    #[test]
    fn a_memory_the_block_had_no_room_for_is_counted_as_left_out() {
        let memories: Vec<_> = (0..5).map(|i| memory(&format!("điều số {i} {}", "x".repeat(200)))).collect();
        // Room for the heading and about two lines.
        let block = crate::syn::memory::memory_block(&memories, 900).expect("some fit");
        let carried = Carried::of(&plan(Some(&block), None, "", DEFAULT_BUDGET_CHARS), memories.len(), None);
        assert!(carried.memory_lines_sent > 0 && carried.memory_lines_sent < 5);
        assert_eq!(carried.memory_lines_sent + carried.memory_dropped, 5);
    }

    #[test]
    fn a_section_fit_cut_is_named_and_what_it_held_counts_as_dropped() {
        let memories: Vec<_> = ["một"].into_iter().map(memory).collect();
        let block = crate::syn::memory::memory_block(&memories, crate::syn::memory::MEMORY_BUDGET_CHARS);
        let fixed = plan(None, None, "", DEFAULT_BUDGET_CHARS).chars();
        // Enough for the fixed prompt and nothing else: retrieval goes first.
        let carried = Carried::of(&plan(block.as_deref(), None, &"ngữ cảnh ".repeat(500), fixed + 10), 1, Some(5));
        assert!(carried.sections_dropped.contains(&"vault_context".to_string()), "{:?}", carried.sections_dropped);
        assert!(carried.sections_dropped.contains(&"memory".to_string()), "{:?}", carried.sections_dropped);
        assert_eq!((carried.memory_lines_sent, carried.memory_dropped), (0, 1));
    }

    #[test]
    fn the_skill_index_is_counted_without_its_two_rules() {
        let skill = |name: &str| {
            crate::syn::skill::Skill::from_node(&node(
                name,
                "## Steps\n1. do the thing",
                serde_json::json!({ "name": name, "description": "Tổng kết tuần.", "tier": "prose", "enabled": true, "version": 1 }),
            ))
        };
        let block = crate::syn::skill::index_block(&[skill("weekly-review"), skill("inbox-zero")], 4000).unwrap();
        assert_eq!(crate::syn::skill::lines_indexed(&block), 2);
        let carried = Carried::of(&plan(None, Some(&block), "", DEFAULT_BUDGET_CHARS), 0, None);
        assert_eq!(carried.skills_indexed, 2);
        assert_eq!(crate::syn::skill::lines_indexed("- not an index"), 0);
    }

    /// A skill the harness chose is named, so a run that followed it can be
    /// told from one that ignored every skill — and a chosen skill `fit` then
    /// cut was never in front of the model, so it is not named.
    #[test]
    fn a_chosen_skill_is_named_only_if_it_was_sent() {
        let skill = crate::syn::skill::Skill::from_node(&node(
            "tong-ket-tuan",
            "## Các bước\n1. `query_nodes` với `type:task status:done`.",
            serde_json::json!({ "name": "tong-ket-tuan", "tier": "prose", "enabled": true }),
        ));
        let block = crate::syn::skill::chosen_block(&skill);

        let sent = plan(None, None, "", DEFAULT_BUDGET_CHARS).with_chosen_skill(Some(&block));
        let carried = Carried::of(&sent, 0, None);
        assert_eq!(carried.skill_injected.as_deref(), Some("tong-ket-tuan"));

        let fixed = plan(None, None, "", DEFAULT_BUDGET_CHARS).chars();
        let cut = plan(None, None, "", fixed + 10).with_chosen_skill(Some(&block));
        let carried = Carried::of(&cut, 0, None);
        assert_eq!(carried.skill_injected, None);
        assert!(carried.sections_dropped.contains(&"chosen_skill".to_string()), "{:?}", carried.sections_dropped);

        let none = Carried::of(&plan(None, None, "", DEFAULT_BUDGET_CHARS), 0, None);
        assert_eq!(none.skill_injected, None);
    }
}
