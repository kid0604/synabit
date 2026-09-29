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
//! 3. **Plan first.** A run asked for a plan describes every change instead of
//!    making it, before anybody is asked which one or whether: describing
//!    something needs no permission, and a card asking for one would be a
//!    question about a thing that is not going to happen.
//! 4. **Which one.** Before consent, because it is not a permission question:
//!    running the ledger for it would file "may I change the vault" in the
//!    audit log for a call that never happened. See `syn::ambiguity`.
//! 5. **Consent.** Has the user agreed to this sort of power? Before the skill
//!    budget, because a refusal should not be spent out of an allowance.
//! 6. **What running means.** `browse` and a connector's tool are driven by
//!    the engine because they are async; a third skill body is refused;
//!    anything else goes to the registry.

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
    /// Whether this is a sub-run: reads only, asks nobody. See `syn::delegate`.
    pub sub_run: bool,
    pub now: &'a str,
    /// Whether the Safe item `handle` may go to the server behind `tool`:
    /// `Ok(destination)` as a consent scope names it, or the sentence to tell
    /// the model. See `safe::bridge::may_send`.
    pub safe: &'a dyn Fn(&str, &str) -> Result<(String, String), String>,
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
    /// Hand the work to a sub-run. See `syn::delegate`.
    Delegate,
    /// Load a group of tools. See `syn::toolset`.
    FindTools,
    /// A tool on a connector: a network request or a program's answer,
    /// which the engine awaits, like `Browse`. See `syn::connector::call`.
    Connector,
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
    //
    // Connectors (Phase F): every tool on an outside server is refused once the run
    // has read anything from outside, reads included — what a read sends goes
    // to the server too. The allowlist below already refuses them, since no
    // `connector__` name is on it; this says why in words that fit, and puts the
    // refusal on the record, because an attempt to reach a server after
    // reading something is exactly what the audit log is for. See `syn::connector`.
    if view.tainted && crate::syn::connector::is_connector_tool(tool) {
        return Decided {
            gate: Gate::Refuse {
                said: crate::syn::connector::refused_after_reading(tool),
                note: Some(format!(
                    "Refused `{tool}`: this run has read something from outside, and a connector is outside."
                )),
            },
            audit: capability.map(|_| Outcome::Refused),
        };
    }
    if view.tainted && !crate::syn::taint::allowed_after_reading(tool) {
        return Decided::quietly(Gate::Refuse {
            said: crate::syn::taint::refusal(tool),
            note: Some(format!(
                "Refused `{tool}`: this run has read something from outside the vault."
            )),
        });
    }

    // 1½. A helper reads and looks things up, and nothing else. Before the
    // surface, because it is narrower than any surface.
    if view.sub_run && !crate::syn::delegate::may_use(tool, capability) {
        return Decided::quietly(Gate::Refuse {
            said: crate::syn::delegate::refusal(tool),
            note: Some(format!("Refused `{tool}`: a helper only reads.")),
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

    // 3. Plan first. Reads still run — a plan built without looking is a
    // guess — and so does writing the plan down. Everything else is described.
    let only_looks = matches!(capability, None | Some(Capability::VaultRead) | Some(Capability::Browse));
    if view.plan_only && !only_looks {
        let about = capability.map(|c| c.describe()).unwrap_or_else(|| tool.to_string());
        return Decided::quietly(Gate::Go(How::Describe(
            serde_json::json!({
                "planned": format!(
                    "Plan mode: nothing that changes anything runs until the user approves. \
                     `{tool}` would {about}, with these arguments. Put it in the plan and carry \
                     on as though it had worked."
                ),
                "arguments": args,
            })
            .to_string(),
        )));
    }

    // 4. Which one.
    if let Some(choice) = crate::syn::ambiguity::should_ask(tool, args, view.seen, view.now) {
        return Decided::quietly(Gate::Choose(Box::new(choice)));
    }

    // 5. Consent.
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
            // Nobody is watching a helper, so a card it raised would be on no
            // screen. It is told, and says so in what it hands back.
            Decision::Ask if view.sub_run => {
                return Decided {
                    gate: Gate::Refuse {
                        said: crate::syn::delegate::cannot_ask(&capability.describe()),
                        note: Some(format!("Refused `{tool}`: it needs permission, and a helper cannot ask.")),
                    },
                    audit: Some(Outcome::Refused),
                };
            }
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

    // 5½. Secrets. A connector call carrying `{{safe:…}}` sends a value from
    // the Safe, and each (item, server) pair is its own permission — asked
    // after the server's own, so the card says the thing only once it is the
    // one thing left to decide. What the item allows is the Safe's to say;
    // the model is not asked.
    if crate::syn::connector::is_connector_tool(tool) {
        for placeholder in crate::safe::egress::find(args) {
            let (destination, label) = match (view.safe)(tool, &placeholder.handle) {
                Ok(d) => d,
                Err(said) => {
                    return Decided {
                        gate: Gate::Refuse {
                            said,
                            note: Some(format!("Refused `{tool}`: it asked for the Safe item `{}`.", placeholder.handle)),
                        },
                        audit: Some(Outcome::Refused),
                    }
                }
            };
            let secret = Capability::UseSecret { item: placeholder.handle.clone(), destination, label };
            let mut decision = crate::syn::consent::decide(&secret, view.ledger, view.now);
            if decision == Decision::Ask && (view.allowed_until_done)(&secret) {
                decision = Decision::Allow;
            }
            match decision {
                Decision::Allow => {}
                Decision::Ask if view.sub_run => {
                    return Decided {
                        gate: Gate::Refuse {
                            said: crate::syn::delegate::cannot_ask(&secret.describe()),
                            note: None,
                        },
                        audit: Some(Outcome::Refused),
                    }
                }
                Decision::Ask => {
                    return Decided { gate: Gate::Ask(Box::new(Ask::about(tool, &secret, view.now))), audit: Some(Outcome::Asked) }
                }
                Decision::Refuse => {
                    return Decided {
                        gate: Gate::Refuse {
                            said: format!(
                                "The user has said never to {}. Do not ask again and do not look for another way.",
                                secret.describe()
                            ),
                            note: None,
                        },
                        audit: Some(Outcome::Refused),
                    }
                }
            }
        }
    }

    // 6. What running means.
    let how = if tool == crate::syn::tools::BROWSE_TOOL {
        How::Browse
    } else if tool == crate::syn::tools::PLAN_TOOL {
        How::Plan
    } else if tool == crate::syn::delegate::TOOL {
        How::Delegate
    } else if tool == crate::syn::toolset::FIND_TOOL {
        How::FindTools
    } else if crate::syn::connector::is_connector_tool(tool) && capability.is_some() {
        // Connectors (Phase F): a request to another machine, awaited by the engine.
        How::Connector
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
            sub_run: false,
            now: NOW,
            safe: &|_, h| Err(format!("no Safe here ({h})")),
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

    /// A helper reads and nothing else, and never raises a card nobody would see.
    #[test]
    fn a_helper_reads_and_asks_nobody() {
        let empty = Ledger::default();
        let mut v = view(&empty, &never_until_done);
        v.sub_run = true;

        let read = decide("get_node", &args(), Some(&Capability::VaultRead), &v);
        assert!(matches!(read.gate, Gate::Go(How::Execute)), "{read:?}");
        for (tool, capability) in [("create_node", Capability::VaultWrite), (crate::syn::delegate::TOOL, Capability::VaultRead)] {
            let d = decide(tool, &args(), Some(&capability), &v);
            assert!(matches!(d.gate, Gate::Refuse { .. }), "{tool}: {d:?}");
        }
        // Browsing with nothing granted would ask; a helper is refused instead.
        let browse = decide("browse", &serde_json::json!({ "what": "x" }), Some(&Capability::Browse), &v);
        match browse.gate {
            Gate::Refuse { said, .. } => assert!(said.contains("cannot ask"), "{said}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// Plan mode looks, and describes every change instead of making it —
    /// before asking whether, because describing needs no permission.
    #[test]
    fn plan_mode_reads_and_describes_every_change() {
        let empty = Ledger::default();
        let mut v = view(&empty, &never_until_done);
        v.plan_only = true;

        let read = decide("get_node", &args(), Some(&Capability::VaultRead), &v);
        assert!(matches!(read.gate, Gate::Go(How::Execute)), "{read:?}");
        let plan = decide(crate::syn::tools::PLAN_TOOL, &serde_json::json!({}), Some(&Capability::VaultRead), &v);
        assert!(matches!(plan.gate, Gate::Go(How::Plan)), "{plan:?}");

        for (tool, capability) in [("create_node", Capability::VaultWrite), ("delete_kind", Capability::VaultStructural), ("run_code", Capability::Execute)] {
            let d = decide(tool, &args(), Some(&capability), &v);
            match d.gate {
                Gate::Go(How::Describe(said)) => assert!(said.contains("Plan mode"), "{said}"),
                other => panic!("{tool}: expected a description, got {other:?}"),
            }
            assert_eq!(d.audit, None, "nothing was asked, so nothing to log");
        }
    }

}
