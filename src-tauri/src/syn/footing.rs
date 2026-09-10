//! What an answer is standing on.
//!
//! # Why hedging in prose is not enough
//!
//! Today uncertainty is a word: *"có thể là…"*, *"theo tôi thấy…"*. That has
//! two problems. It cannot be read by anything but a person — no screen can
//! colour it, no count can tell you how often Syn was guessing this week — and
//! a model that hedges every sentence has made the hedge meaningless.
//!
//! So it becomes a state with three values, attached to the answer, decided by
//! arithmetic on the run's own transcript.
//!
//! # Why the transcript decides and not the model
//!
//! Because the model is the least reliable witness to its own confidence, and
//! because `docs/adr-rag-vs-agentic-2026-09-03.md` measured what asking a local
//! model for a self-report is worth: an instruction written for one specific
//! failure changed nothing at all and was reverted.
//!
//! The transcript already knows. A run that called `query_nodes`, got a result
//! and answered has something to point at. A run that called nothing and
//! answered about the vault has nothing but the model. That is the same kind of
//! arithmetic `skill::repeated_chain`, `correction` and `tempo` are built on,
//! and it is the third time in a row it has beaten asking.
//!
//! # What each state actually claims
//!
//! The claims are deliberately narrow, because a state that overclaims is worse
//! than no state:
//!
//! * **`Grounded`** — *something was read that you can read too.* A tool that
//!   only looks came back, or the answer is an exact count this app computed
//!   from the index before the model was asked. It does **not** claim the
//!   answer is correct. The model can still misread what came back; what it
//!   cannot do is have invented the source.
//!
//!   A tool that *writes* does not count, and getting that wrong was this
//!   module's first bug: `remember` succeeding made an answer `Grounded` on a
//!   turn that had looked at nothing.
//! * **`Inferred`** — *reasoned over a sample nobody chose deliberately.* The
//!   retrieval step put vault material in front of the model and the model
//!   never looked anything up. That sample is the top few chunks for the
//!   question — it may simply not contain the answer, and the prompt says so
//!   in as many words. This is the state where an answer sounds sourced and is
//!   not.
//! * **`Guessing`** — *nothing from this vault held it up.* No tool, no
//!   retrieval, no count. It may still be a good answer to a general question.
//!   It is not an answer about the user's own data, whatever it sounds like.
//!
//! # What it deliberately is not
//!
//! Not a number. `confidence: 0.73` is unfalsifiable, unauditable, and read as
//! a probability by everybody who sees one. Three states, each with a sentence
//! saying what it means, is a claim somebody can disagree with — which is the
//! only kind worth making.

use serde::{Deserialize, Serialize};

use crate::syn::run::{Run, StepKind};
use crate::syn::tempo::Tempo;

/// Refuses to compile when a variant is added and `ALL` is not updated.
///
/// A test cannot do this job: to enumerate the variants it would have to hold
/// the list being checked. Only the compiler knows them all.
#[allow(dead_code)]
fn _every_variant_is_listed(footing: Footing) {
    match footing {
        Footing::Grounded | Footing::Inferred | Footing::Guessing => {}
    }
}

/// What an answer is standing on.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Footing {
    /// Something was read, or this app counted it from the index. There is a
    /// source, and it can be looked at again. A tool that only *wrote* does not
    /// put an answer here — see `of`.
    Grounded,
    /// Retrieval put material in front of the model and nothing was looked up.
    /// Plausible, unchecked — and the sample may not have held the answer.
    Inferred,
    /// Nothing from the vault held this up. The default, because a run that
    /// recorded nothing did nothing.
    #[default]
    Guessing,
}

impl Footing {
    /// Every variant, for the frontend guard and for anything that has to
    /// enumerate them.
    pub const ALL: [Footing; 3] = [Footing::Grounded, Footing::Inferred, Footing::Guessing];

    /// The wire name, which is also the i18n key suffix the frontend uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Footing::Grounded => "grounded",
            Footing::Inferred => "inferred",
            Footing::Guessing => "guessing",
        }
    }
}

/// What the run reached for, as far as the transcript can tell.
///
/// A struct rather than three positional booleans, because `of(run, true,
/// false)` is a call nobody can read and two of the three are easy to swap.
pub struct Evidence {
    /// How many chunks retrieval put in the prompt. Taken from the retrieval
    /// result rather than from the message, because the message only carries
    /// sources when no tool was used — so reading it would report zero for
    /// exactly the runs that had the most to stand on.
    pub retrieved: usize,
}

/// Decide what this run's answer is standing on.
///
/// Reads only what is already written down: the steps, and the tempo that was
/// chosen before the work started.
pub fn of(run: &Run, evidence: &Evidence) -> Footing {
    // An instant turn is a count this app computed from the index with a query
    // it recorded. That is the most checkable answer the app produces — more so
    // than a tool call, because no model stood between the number and the
    // index — and it never involves a tool, so without this it would read as
    // the least.
    if run.tempo == Tempo::Instant {
        return Footing::Grounded;
    }

    // A *look*, not any tool that happened to succeed.
    //
    // This counted every successful call, which made the module lie in exactly
    // the way it exists to prevent: "ghi nhớ giúp tao là tao ghét hành" calls
    // `remember`, `remember` succeeds, and the answer was marked `Grounded` —
    // claiming there is a source to check about a turn that looked at nothing
    // at all. Writing something down is evidence that something happened, and
    // it is not evidence that anything was read.
    //
    // `Reversal::Nothing` is the registry's own word for "read something and
    // changed nothing", which is precisely the distinction wanted, and it is
    // already recorded on every step. The table was there the whole time — see
    // `registry::Registry::table`, where `query_nodes` is `VaultRead` and
    // `remember` is `VaultWrite`.
    let looked = run.steps.iter().any(|step| {
        step.kind == StepKind::ToolCall
            && step.ok == Some(true)
            && step.reversal == Some(crate::syn::registry::Reversal::Nothing)
    });

    if looked {
        return Footing::Grounded;
    }

    // Tools that were called and all failed land here rather than in
    // `Grounded`: an answer built after every lookup errored is standing on
    // whatever the model already believed, which is the same place a `Guessing`
    // answer stands — unless retrieval had put something in front of it.
    if evidence.retrieved > 0 {
        return Footing::Inferred;
    }

    Footing::Guessing
}

/// How often Syn was standing on something, across the runs still on disk.
///
/// # Why this exists at all
///
/// Because the same thing has now happened four times in this codebase and
/// each time it was invisible: `recall` went uncalled across fifteen runs, the
/// skill detector fired on none of seventeen, the thread counter reads zero of
/// twenty-five — and every one of those was collected and then never shown to
/// anybody. `Run::footing` was written the same way and was heading for the
/// same place.
///
/// A number nobody is shown is a number nobody acts on. So this is the answer
/// to the one question the whole module is for: **how often am I guessing?**
#[derive(serde::Serialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// Runs that recorded a footing at all.
    pub measured: usize,
    pub grounded: usize,
    pub inferred: usize,
    pub guessing: usize,
    /// Runs written before footings existed.
    ///
    /// Counted separately and never folded into `guessing`. Those runs were not
    /// guesses — nobody looked. Merging the two would open the screen by
    /// reporting the vault's entire history as guesswork, which is the exact
    /// kind of confident wrong number this module exists to stop.
    pub unmeasured: usize,
}

impl Tally {
    /// Whether the reading is worth showing yet.
    ///
    /// Below a handful of measured runs the percentage swings wildly and says
    /// more about the afternoon than about Syn. The screen stays quiet until
    /// there is something to read.
    pub const ENOUGH_TO_MEAN_ANYTHING: usize = 5;

    pub fn worth_showing(&self) -> bool {
        self.measured >= Self::ENOUGH_TO_MEAN_ANYTHING
    }
}

/// Count the runs by what they turned out to be standing on.
pub fn tally(runs: &[Run]) -> Tally {
    let mut out = Tally::default();
    for run in runs {
        match run.footing {
            None => out.unmeasured += 1,
            Some(footing) => {
                out.measured += 1;
                match footing {
                    Footing::Grounded => out.grounded += 1,
                    Footing::Inferred => out.inferred += 1,
                    Footing::Guessing => out.guessing += 1,
                }
            }
        }
    }
    out
}

/// The sentence Syn is told to live up to, for the prompt.
///
/// Stated as a rule about *behaviour* rather than as a request for a label:
/// nothing here asks the model to report its footing, because the transcript
/// decides that. What it asks for is the thing a label cannot do — saying out
/// loud, in the answer, when the answer is not standing on anything.
pub const RULE: &str = r#"On being unsure, and on disagreeing:
- When you have not looked anything up and the question is about this user's own data, say that you have not looked, in one clause, before answering. Do not present a guess in the same voice as a result.
- When you looked and found nothing, say you found nothing. An empty result is an answer; inventing a plausible one is not.
- When you think the user is wrong or that what they are asking for will not work, say so once, briefly, and say why. Then do what they asked. Do not raise it again in the same conversation, and do not soften it into a question — a colleague says "I think this is wrong, but it's your call" and then gets on with it.

"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syn::registry::Reversal;
    use crate::models::syn::SynSettings;
    use crate::syn::run::{Budget, Run, RunState};

    fn run_with(tempo: Tempo) -> Run {
        let mut run = Run::new("hỏi gì đó", None, Budget::from_settings(&SynSettings::default()));
        run.tempo = tempo;
        run.state = RunState::Done;
        run
    }

    /// Through the real recorder, and with the reversal the registry would
    /// actually give this tool — which is the difference between a look and a
    /// write, and the thing the first version of this test helper hid by
    /// hard-coding `Nothing` for everything.
    fn called(run: &mut Run, tool: &str, ok: bool) {
        let reversal = crate::syn::registry::Registry::<tauri::Wry>::for_chat()
            .capability_of(tool, &serde_json::Value::Null)
            .map(|c| crate::syn::registry::reversal_of(&c))
            .unwrap_or(Reversal::Nothing);
        run.record_tool(
            1,
            tool,
            serde_json::json!({}),
            ok,
            reversal,
            if ok { "[]" } else { "no such type" },
            3,
        );
    }

    fn nothing() -> Evidence {
        Evidence { retrieved: 0 }
    }

    fn retrieved(n: usize) -> Evidence {
        Evidence { retrieved: n }
    }

    /// The plain case: it looked, so there is something to look at again.
    #[test]
    fn a_tool_that_returned_is_something_to_point_at() {
        let mut run = run_with(Tempo::Working);
        called(&mut run, "query_nodes", true);
        assert_eq!(of(&run, &nothing()), Footing::Grounded);
    }

    /// An empty result still grounds the answer. "You have no overdue tasks" is
    /// a fact about the vault, arrived at by asking the vault.
    #[test]
    fn a_query_that_matched_nothing_still_counts_as_having_looked() {
        let mut run = run_with(Tempo::Working);
        called(&mut run, "query_nodes", true);
        assert_eq!(of(&run, &nothing()), Footing::Grounded);
    }

    /// The count the instant tempo produces never touches a tool, and it is the
    /// most checkable number this app makes: Rust ran the query against the
    /// index and the query is written down.
    #[test]
    fn a_counted_answer_is_grounded_without_any_tool_at_all() {
        let run = run_with(Tempo::Instant);
        assert!(run.steps.iter().all(|s| s.kind != StepKind::ToolCall));
        assert_eq!(of(&run, &nothing()), Footing::Grounded);
    }

    /// The state that matters most: it *sounds* sourced. Retrieval put a
    /// handful of chunks in the prompt, the model reasoned over them, and
    /// nothing confirmed that the chunks held the answer.
    #[test]
    fn reasoning_over_retrieved_chunks_is_not_the_same_as_looking() {
        let run = run_with(Tempo::Working);
        assert_eq!(of(&run, &retrieved(4)), Footing::Inferred);
    }

    #[test]
    fn nothing_at_all_is_a_guess() {
        let run = run_with(Tempo::Working);
        assert_eq!(of(&run, &nothing()), Footing::Guessing);
    }

    /// Every lookup erroring is not evidence. The answer that follows stands
    /// where a guess stands, and saying otherwise is the one lie this module
    /// exists to prevent.
    #[test]
    fn tools_that_all_failed_ground_nothing() {
        let mut run = run_with(Tempo::Working);
        called(&mut run, "query_nodes", false);
        called(&mut run, "get_node", false);
        assert_eq!(of(&run, &nothing()), Footing::Guessing);
        assert_eq!(of(&run, &retrieved(3)), Footing::Inferred);
    }

    /// One that worked is enough, whatever else went wrong around it.
    #[test]
    fn one_call_that_worked_outweighs_several_that_did_not() {
        let mut run = run_with(Tempo::Working);
        called(&mut run, "query_nodes", false);
        called(&mut run, "get_node", true);
        called(&mut run, "read_file_text", false);
        assert_eq!(of(&run, &nothing()), Footing::Grounded);
    }

    /// Text the model produced on its way to an answer is not a lookup.
    #[test]
    fn the_model_talking_to_itself_is_not_evidence() {
        let mut run = run_with(Tempo::Working);
        run.record_assistant(1, "để tôi xem…", Default::default(), 5);
        assert_eq!(of(&run, &nothing()), Footing::Guessing);
    }

    /// The bug this module shipped with, as a case.
    ///
    /// "Remember that I hate onions" calls `remember`, `remember` succeeds, and
    /// the turn looked at nothing whatsoever. Counting it as `Grounded` claimed
    /// there was a source to check — which is the one lie this whole module was
    /// built to prevent, made by the module itself.
    #[test]
    fn writing_something_down_is_not_the_same_as_having_looked() {
        let mut run = run_with(Tempo::Working);
        called(&mut run, "remember", true);
        assert_eq!(of(&run, &nothing()), Footing::Guessing);
    }

    #[test]
    fn creating_and_editing_are_not_looking_either() {
        for tool in ["create_node", "update_node", "trash_node", "delete_kind"] {
            let mut run = run_with(Tempo::Working);
            called(&mut run, tool, true);
            assert_eq!(of(&run, &nothing()), Footing::Guessing, "{tool} grounded nothing");
        }
    }

    /// And the ordinary case still holds: a write after a look is grounded on
    /// the look.
    #[test]
    fn a_look_followed_by_a_write_is_still_grounded() {
        let mut run = run_with(Tempo::Working);
        called(&mut run, "query_nodes", true);
        called(&mut run, "update_node", true);
        assert_eq!(of(&run, &nothing()), Footing::Grounded);
    }

    /// Every tool the registry calls a read grounds an answer, and every tool
    /// it calls a write does not. Read off the registry rather than listed
    /// here, so a new tool cannot arrive on the wrong side of this quietly.
    #[test]
    fn the_registry_decides_what_counts_as_looking() {
        for tool in ["query_nodes", "get_node", "recall", "search_files", "get_transactions"] {
            let mut run = run_with(Tempo::Working);
            called(&mut run, tool, true);
            assert_eq!(of(&run, &nothing()), Footing::Grounded, "{tool} is a read");
        }
    }

    /// A run written before this existed reads back as the honest answer rather
    /// than the flattering one.
    #[test]
    fn the_default_is_the_one_that_claims_least() {
        assert_eq!(Footing::default(), Footing::Guessing);
    }

    // ── the tally ─────────────────────────────────────────────────

    fn measured(footing: Footing) -> Run {
        let mut run = run_with(Tempo::Working);
        run.footing = Some(footing);
        run
    }

    #[test]
    fn the_tally_counts_each_state_separately() {
        let runs = [
            measured(Footing::Grounded),
            measured(Footing::Grounded),
            measured(Footing::Inferred),
            measured(Footing::Guessing),
        ];
        let t = tally(&runs);
        assert_eq!(t.measured, 4);
        assert_eq!((t.grounded, t.inferred, t.guessing), (2, 1, 1));
        assert_eq!(t.unmeasured, 0);
    }

    /// The distinction the whole screen depends on. A run from before footings
    /// existed was not a guess — nobody looked. Folding it into `guessing`
    /// would open the panel reporting the vault's whole history as guesswork.
    #[test]
    fn runs_from_before_this_existed_are_not_counted_as_guesses() {
        let runs = [run_with(Tempo::Working), measured(Footing::Grounded)];
        let t = tally(&runs);
        assert_eq!(t.unmeasured, 1);
        assert_eq!(t.guessing, 0);
        assert_eq!(t.measured, 1);
    }

    /// Below a handful the percentage says more about the afternoon than about
    /// Syn, so the screen stays quiet.
    #[test]
    fn a_reading_too_small_to_mean_anything_is_not_shown() {
        let few: Vec<Run> = (0..Tally::ENOUGH_TO_MEAN_ANYTHING - 1)
            .map(|_| measured(Footing::Grounded))
            .collect();
        assert!(!tally(&few).worth_showing());

        let enough: Vec<Run> = (0..Tally::ENOUGH_TO_MEAN_ANYTHING)
            .map(|_| measured(Footing::Grounded))
            .collect();
        assert!(tally(&enough).worth_showing());
    }

    #[test]
    fn a_vault_with_no_runs_says_nothing() {
        assert_eq!(tally(&[]), Tally::default());
        assert!(!tally(&[]).worth_showing());
    }

    /// The wire names are what the frontend switches on and what the i18n keys
    /// are built from, so they are checked rather than assumed.
    #[test]
    fn the_wire_names_are_the_ones_the_frontend_reads() {
        for footing in Footing::ALL {
            assert_eq!(
                serde_json::to_value(footing).expect("serialises"),
                serde_json::json!(footing.as_str())
            );
        }
    }

    /// The rule states behaviour rather than asking for a label — the label is
    /// arithmetic, and asking a model for one is what the ADR measured and
    /// reverted.
    #[test]
    fn the_rule_asks_for_behaviour_and_not_for_a_self_report() {
        assert!(!RULE.contains("footing"), "it never asks for the label:\n{RULE}");
        assert!(RULE.contains("say that you have not looked"), "{RULE}");
        assert!(RULE.contains("say so once"), "the disagreement is bounded:\n{RULE}");
        assert!(
            RULE.contains("Then do what they asked"),
            "and it ends in doing the work:\n{RULE}"
        );
    }

    /// A footing that is decided and not written down was never decided.
    ///
    /// This is the bug it exists for, and it hid perfectly: `of()` was right,
    /// the message carried the mark, the screen rendered it — and `run.footing`
    /// was set after `drive` had saved the run for the last time, so nothing
    /// ever wrote it. Forty runs on disk, forty `null`, `tally` counting the
    /// vault's whole history as unmeasured, and `worth_showing` therefore false
    /// for ever. Everything looked like it was working.
    ///
    /// So this asserts the property the screen actually depends on — it comes
    /// back off the disk — rather than that a field was assigned.
    #[test]
    fn a_footing_has_to_survive_being_saved() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_str().expect("utf8");

        let mut run = crate::syn::run::Run::new(
            "did it look anything up",
            Some("conv-1".into()),
            crate::syn::run::Budget::from_settings(&crate::models::syn::SynSettings::default()),
        );
        run.footing = Some(Footing::Grounded);
        crate::syn::run::save_run(vault, &run).expect("saved");

        let read = crate::syn::run::get_run(vault, &run.id).expect("read back");
        assert_eq!(
            read.footing,
            Some(Footing::Grounded),
            "a footing that does not survive the disk is one the tally never sees"
        );

        assert_eq!(tally(&[read]).measured, 1);
    }

    /// And the command that decides it has to save afterwards.
    ///
    /// `drive` writes the run for the last time before the footing is known, so
    /// there has to be one more write after it. Nothing fails to compile if
    /// that write goes away, and nothing fails at run time either — the screen
    /// simply goes quiet and stays quiet.
    #[test]
    fn the_send_path_writes_the_run_again_once_the_footing_is_known() {
        let source = include_str!("../commands/syn.rs");
        let after = source
            .split("run.footing = Some(footing);")
            .nth(1)
            .expect("the footing is still decided there");

        assert!(
            after
                .split("// 10.")
                .next()
                .unwrap_or(after)
                .contains("save_run_best_effort"),
            "the run has to be written again after its footing is decided"
        );
    }

    /// A run that never answered has no answer to be standing on anything.
    ///
    /// Writing the footing down revealed this immediately: of six runs in one
    /// conversation, four were consent stops with no reply at all, and each
    /// arrived in the tally as a measured answer. `tally` is asked "how often
    /// was Syn guessing" — a run that said nothing is not an instance of
    /// anything, and counting it makes the only number on that screen wrong.
    #[test]
    fn the_send_path_only_marks_a_turn_that_actually_answered() {
        let source = include_str!("../commands/syn.rs");
        let at = source
            .find("run.footing = Some(footing);")
            .expect("the footing is still decided there");
        let before = &source[..at];

        let guard = before
            .rfind("if !assistant_message.content.trim().is_empty() {")
            .expect("the footing has to be decided only for a turn that said something");

        assert!(
            !before[guard..].contains("\n    }"),
            "the guard must still be open where the footing is set"
        );
    }
}
