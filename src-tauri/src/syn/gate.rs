//! What happens to one tool call, decided before anything runs.
//!
//! # Why this is its own function
//!
//! The decision used to be written inline in `SynEngine::drive_inner`: seven
//! checks, each an `if` with its own `continue` or `break`, interleaved with
//! the code that carried out whatever they decided. It worked, and it had two
//! costs that grew with every check added.
//!
//! * **It could only be tested by driving a run.** Every rule — a web read
//!   refuses `trash_node`, a `Never` beats a "just this once", a surface that
//!   is not offered a tool refuses it before anybody is asked — needed a
//!   scripted provider, a mock app and a seeded vault to observe, and the order
//!   of the rules could only be seen by reading the loop.
//! * **Everything the roadmap adds next goes exactly here.** Background runs
//!   that may not grant themselves anything, sub-runs with narrower tools, a
//!   tool call carried over from a stopped run: each is one more question asked
//!   of a call before it runs. Adding them to a six-hundred-line loop is how
//!   the next check lands in the wrong place.
//!
//! So the questions are asked here, in one pure function over a snapshot of
//! the run, and the loop only acts on the answer. Nothing here touches the
//! disk, the clock or the network: the ledger, the time and the run's state
//! come in as arguments, and a test can ask any combination of them.
//!
//! # The order, and why it is this order
//!
//! 1. **Taint.** A run that has read something from outside may only read and
//!    create. First, because nothing after it should be able to reopen a door
//!    it closed — not a grant, not a dry run. See `syn::taint`.
//! 2. **Surface.** A tool the place the question came from is not offered is
//!    not a permission anybody there can grant, and asking would park the run
//!    on a card on a screen nobody is looking at. See `syn::surface`.
//! 3. **Which one.** Before consent, because it is not a permission question:
//!    running the ledger for it would file "may I change the vault" in the
//!    audit log for a call that never happened. See `syn::ambiguity`.
//! 4. **Consent.** Has the user agreed to this sort of power? Before the skill
//!    budget, because a refusal should not be spent out of an allowance.
//! 5. **What running means.** A dry run describes instead of acting; `browse`
//!    is driven by the engine because it is async; a third skill body is
//!    refused; anything else goes to the registry.

use serde_json::Value;

use crate::syn::ambiguity::{Candidate, Choice};
use crate::syn::audit::Outcome;
use crate::syn::consent::{Ask, Capability, Decision, Ledger};
use crate::syn::surface::Surface;

/// The run, as far as deciding one call needs to see it.
pub struct View<'a> {
    /// Whether the run has read something written outside the vault.
    pub tainted: bool,
    pub surface: Surface,
    /// What the most recent `query_nodes` turned up, whole.
    pub seen: &'a [Candidate],
    pub ledger: &'a Ledger,
    /// Whether "just this once" was said for this capability earlier in the
    /// same piece of work. See `consent::allowed_until_done`.
    pub allowed_until_done: &'a dyn Fn(&Capability) -> bool,
    /// Skill bodies this run has already opened.
    pub skills_opened: usize,
    pub plan_only: bool,
    pub now: &'a str,
}

/// What to do with the call.
#[derive(Debug)]
pub enum Gate {
    /// Not run. `said` goes back to the model as the call's result; `note`,
    /// when there is one, goes in the transcript.
    Refuse { said: String, note: Option<String> },
    /// Stop and ask the user which one they meant.
    Choose(Box<Choice>),
    /// Stop and ask the user whether this may be done.
    Ask(Box<Ask>),
    /// Go ahead, in this way.
    Go(How),
}

/// What going ahead means for this call.
#[derive(Debug, PartialEq, Eq)]
pub enum How {
    /// A dry run: this is what comes back instead of the tool running.
    Describe(String),
    /// The engine's own async ladder. See `engine::browse`.
    Browse,
    /// The run's own list of steps, which only the engine can change.
    Plan,
    /// Answered with this refusal, but recorded as a call the run made — it is
    /// a budget, not a rule, and the transcript should show it was reached.
    OverSkillBudget(String),
    /// The registry runs it.
    Execute,
}

/// The decision, and what the audit log should say about it.
#[derive(Debug)]
pub struct Decided {
    pub gate: Gate,
    /// `None` when there is nothing to record: a vault capability, or a call
    /// turned away before any capability was weighed.
    pub audit: Option<Outcome>,
}

impl Decided {
    fn quietly(gate: Gate) -> Self {
        Self { gate, audit: None }
    }
}

/// Decide one call.
///
/// `capability` is what the registry says the call would use, or `None` for a
/// tool it does not know — which is not refused here: the registry answers an
/// unknown tool with an error the model can read, and that is the better place
/// for it.
pub fn decide(tool: &str, args: &Value, capability: Option<&Capability>, view: &View<'_>) -> Decided {
    // 1. Taint.
    if view.tainted && !crate::syn::taint::allowed_after_reading(tool) {
        return Decided::quietly(Gate::Refuse {
            said: crate::syn::taint::refusal(tool),
            note: Some(format!(
                "Refused `{tool}`: this run has read something from outside the vault."
            )),
        });
    }

    // 2. Surface.
    if !view.surface.offers(tool, capability) {
        return Decided {
            gate: Gate::Refuse {
                said: format!(
                    "`{tool}` is not available when the question comes from {}. Do not look \
                     for another way; say plainly that this has to be done in the app.",
                    view.surface.label()
                ),
                note: Some(format!("Refused `{tool}`: not available from {}.", view.surface.label())),
            },
            audit: capability.map(|_| Outcome::Refused),
        };
    }

    // 3. Which one.
    if let Some(choice) = crate::syn::ambiguity::should_ask(tool, args, view.seen, view.now) {
        return Decided::quietly(Gate::Choose(Box::new(choice)));
    }

    // 4. Consent.
    let mut audit = None;
    if let Some(capability) = capability {
        let mut decision = crate::syn::consent::decide(capability, view.ledger, view.now);
        // Only ever `Ask` to `Allow`. A `Never` recorded in between is a
        // decision made later about the same thing, and later wins.
        if decision == Decision::Ask && (view.allowed_until_done)(capability) {
            decision = Decision::Allow;
        }
        audit = Some(crate::syn::audit::outcome_of(&decision));

        match decision {
            Decision::Allow => {}
            Decision::Ask => {
                return Decided {
                    gate: Gate::Ask(Box::new(Ask::about(tool, capability, view.now))),
                    audit,
                };
            }
            // Told, not hidden. A silent failure would have the model try
            // again by another route, which is the opposite of respecting a no.
            Decision::Refuse => {
                return Decided {
                    gate: Gate::Refuse {
                        said: format!(
                            "The user has said never to {}. Do not ask again and do not look \
                             for another way.",
                            capability.describe()
                        ),
                        note: None,
                    },
                    audit,
                };
            }
        }
    }

    // 5. What running means.
    //
    // A dry run describes only the steps whose undoing is somebody else's
    // problem. Reads and reversible writes go ahead, because a plan built
    // without looking is a guess.
    let only_describing = view.plan_only
        && capability.is_some_and(|c| {
            matches!(
                crate::syn::registry::reversal_of(c),
                crate::syn::registry::Reversal::Manual { .. } | crate::syn::registry::Reversal::Irreversible
            )
        });

    let how = if only_describing {
        let about = capability.map(|c| c.describe()).unwrap_or_else(|| tool.to_string());
        How::Describe(
            serde_json::json!({
                "planned": format!(
                    "This is a dry run. `{tool}` would {about}, with these arguments. Nothing \
                     was done. Carry on planning as though it had worked."
                ),
                "arguments": args,
            })
            .to_string(),
        )
    } else if tool == crate::syn::tools::BROWSE_TOOL {
        How::Browse
    } else if tool == crate::syn::tools::PLAN_TOOL {
        How::Plan
    } else if tool == crate::syn::skill::LOAD_TOOL && view.skills_opened >= crate::syn::skill::BODIES_PER_RUN {
        // A budget over a run, and the run is what this sees. Refused rather
        // than errored: "not this time, use what you have" is a sentence the
        // model can act on.
        How::OverSkillBudget(
            serde_json::json!({
                "refused": format!(
                    "You have already opened {} skills in this run, which is the limit. Work \
                     from what you have read, or answer without a skill.",
                    crate::syn::skill::BODIES_PER_RUN
                ),
            })
            .to_string(),
        )
    } else {
        How::Execute
    };

    Decided { gate: Gate::Go(how), audit }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syn::consent::{Answer, Grant};

    const NOW: &str = "2026-09-26T10:00:00+00:00";

    fn never_until_done(_: &Capability) -> bool {
        false
    }
    fn always_until_done(_: &Capability) -> bool {
        true
    }

    fn view<'a>(ledger: &'a Ledger, until_done: &'a dyn Fn(&Capability) -> bool) -> View<'a> {
        View {
            tainted: false,
            surface: Surface::App,
            seen: &[],
            ledger,
            allowed_until_done: until_done,
            skills_opened: 0,
            plan_only: false,
            now: NOW,
        }
    }

    fn ledger(capability: &Capability, answer: Answer) -> Ledger {
        Ledger {
            grants: vec![Grant {
                scope: capability.scope_key().expect("a scope"),
                about: capability.describe(),
                answer,
                granted_at: "2026-09-01T00:00:00+00:00".into(),
                expires_at: None,
            }],
        }
    }

    fn args() -> Value {
        serde_json::json!({ "node_id": "Notes/a.md" })
    }

    #[test]
    fn an_ordinary_read_goes_to_the_registry() {
        let empty = Ledger::default();
        let d = decide("get_node", &args(), Some(&Capability::VaultRead), &view(&empty, &never_until_done));
        assert!(matches!(d.gate, Gate::Go(How::Execute)), "{d:?}");
        assert_eq!(d.audit, Some(Outcome::Allowed));
    }

    /// First, and before a grant: nothing after it may reopen what it closed.
    #[test]
    fn taint_refuses_before_anything_is_weighed() {
        let granted = ledger(&Capability::VaultWrite, Answer::Always);
        let mut v = view(&granted, &always_until_done);
        v.tainted = true;
        let d = decide("trash_node", &args(), Some(&Capability::VaultWrite), &v);
        assert!(matches!(d.gate, Gate::Refuse { note: Some(_), .. }), "{d:?}");
        assert_eq!(d.audit, None, "no capability was weighed, so nothing to log");

        let read = decide("get_node", &args(), Some(&Capability::VaultRead), &v);
        assert!(matches!(read.gate, Gate::Go(How::Execute)), "reading is still reading");
    }

    #[test]
    fn a_surface_refusal_is_logged_and_never_asks() {
        let empty = Ledger::default();
        let mut v = view(&empty, &never_until_done);
        v.surface = Surface::Telegram;
        let d = decide("browse", &serde_json::json!({ "what": "x" }), Some(&Capability::Browse), &v);
        assert!(matches!(d.gate, Gate::Refuse { .. }), "{d:?}");
        assert_eq!(d.audit, Some(Outcome::Refused));
    }

    /// "Which one" comes before consent, so no permission line is written for
    /// a call that may never happen.
    #[test]
    fn which_one_is_asked_before_whether() {
        let empty = Ledger::default();
        let seen = vec![
            Candidate { id: "Notes/a.md".into(), title: "A".into(), node_type: Some("note".into()) },
            Candidate { id: "Notes/b.md".into(), title: "B".into(), node_type: Some("note".into()) },
        ];
        let mut v = view(&empty, &never_until_done);
        v.seen = &seen;
        let d = decide("trash_node", &args(), Some(&Capability::VaultWrite), &v);
        assert!(matches!(d.gate, Gate::Choose(_)), "{d:?}");
        assert_eq!(d.audit, None);
    }

    #[test]
    fn a_power_nobody_has_granted_is_asked_about() {
        let empty = Ledger::default();
        let d = decide("browse", &serde_json::json!({ "what": "x" }), Some(&Capability::Browse), &view(&empty, &never_until_done));
        assert!(matches!(d.gate, Gate::Ask(_)), "{d:?}");
        assert_eq!(d.audit, Some(Outcome::Asked));
    }

    #[test]
    fn just_this_once_carries_through_the_same_work() {
        let empty = Ledger::default();
        let d = decide("browse", &serde_json::json!({ "what": "x" }), Some(&Capability::Browse), &view(&empty, &always_until_done));
        assert!(matches!(d.gate, Gate::Go(How::Browse)), "{d:?}");
        assert_eq!(d.audit, Some(Outcome::Allowed));
    }

    /// A `Never` said later beats a "just this once" said earlier.
    #[test]
    fn never_beats_just_this_once() {
        let refused = ledger(&Capability::Browse, Answer::Never);
        let d = decide("browse", &serde_json::json!({ "what": "x" }), Some(&Capability::Browse), &view(&refused, &always_until_done));
        assert!(matches!(d.gate, Gate::Refuse { note: None, .. }), "{d:?}");
        assert_eq!(d.audit, Some(Outcome::Refused));
    }

    #[test]
    fn a_third_skill_is_refused_as_a_budget_not_a_rule() {
        let empty = Ledger::default();
        let mut v = view(&empty, &never_until_done);
        v.skills_opened = crate::syn::skill::BODIES_PER_RUN;
        let d = decide(crate::syn::skill::LOAD_TOOL, &serde_json::json!({ "name": "x" }), Some(&Capability::VaultRead), &v);
        assert!(matches!(d.gate, Gate::Go(How::OverSkillBudget(_))), "{d:?}");
    }

    /// A dry run still reads and still makes reversible changes; only what
    /// cannot be taken back is described instead.
    #[test]
    fn a_dry_run_describes_only_what_cannot_be_undone() {
        let empty = Ledger::default();
        let until = |_: &Capability| true;
        let mut v = view(&empty, &until);
        v.plan_only = true;

        let write = decide("create_node", &args(), Some(&Capability::VaultWrite), &v);
        assert!(matches!(write.gate, Gate::Go(How::Execute)), "{write:?}");

        let run_code = decide("run_code", &args(), Some(&Capability::Execute), &v);
        match run_code.gate {
            Gate::Go(How::Describe(said)) => assert!(said.contains("dry run"), "{said}"),
            other => panic!("expected a description, got {other:?}"),
        }
    }
}
