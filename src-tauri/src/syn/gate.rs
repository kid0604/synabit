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
    /// When everything the run has read came from one connector, its slug:
    /// that connector's read tools stay open. See `taint::Origin`.
    pub only_connector: Option<&'a str>,
    /// Connectors the person approved on their card in this piece of work.
    /// Their reads stay open after reading, on the same terms as
    /// `only_connector`'s. See `Run::approved_in_work`.
    pub approved_in_work: &'a [String],
    /// What a call back to that connector — or to one approved in this work —
    /// may carry: the person's words and the connector's own earlier answers.
    /// See `taint::only_echoes`.
    pub echoes: &'a [String],
    /// Hashes of the sends the person approved and that have not gone yet.
    /// See `syn::writes`.
    pub approved_writes: &'a [String],
    pub surface: Surface,
    /// What the most recent `query_nodes` turned up, whole.
    pub seen: &'a [Candidate],
    /// What the person said that this run answers.
    pub said: &'a str,
    pub ledger: &'a Ledger,
    /// Whether "just this once" was said for this capability earlier in the
    /// same piece of work. See `consent::allowed_until_done`.
    pub allowed_until_done: &'a dyn Fn(&Capability) -> bool,
    /// Whether "just this once" was said about this exact call — tool and
    /// arguments — earlier in the same work. See
    /// `consent::allow_call_until_done`.
    pub call_allowed_until_done: bool,
    /// Whether "not now" was said about this exact call earlier in the same
    /// work. See `consent::decline_call_until_done`.
    pub call_declined: bool,
    /// Whether a yes to one run of this exact call was said in the system's
    /// dialog and not spent yet. The only yes a command on this Mac goes by.
    /// See `consent::grant_one_run`.
    pub one_run_granted: bool,
    /// Skill bodies this run has already opened.
    pub skills_opened: usize,
    pub plan_only: bool,
    /// Whether this is a sub-run: reads only, asks nobody. See `syn::delegate`.
    pub sub_run: bool,
    /// Whether it is a helper sent out (`delegate_out`): the web and its own
    /// room, and nothing else. Also a sub-run.
    pub sent_out: bool,
    /// Whether this run has `delegate_out` among its tools. See
    /// `delegate::is_a_file`.
    pub sends_out: bool,
    pub now: &'a str,
    /// Whether the Safe item `handle` may go to the server behind `tool`:
    /// where it would go, or the sentence to tell the model. See
    /// `safe::bridge::may_send`.
    pub safe: &'a SafeCheck<'a>,
    /// Whether an earlier turn of the conversation read something untrusted.
    /// See `Run::untrusted_before`.
    pub untrusted_before: bool,
    /// Where `browse` may go without carrying anything out: links seen, hosts
    /// the person named. `None` where nobody keeps them (a test, a caller
    /// with no run). See `taint::browse_carries_nothing`.
    pub reach: Option<&'a crate::syn::taint::Destinations>,
    /// Whether the call changes something that rides in every prompt —
    /// SYN.md, a memory, a skill. See `tools::rides_prompt`.
    pub rides_prompt: bool,
}

/// `(tool, handle)` → where the item would go, or the sentence to tell the model.
pub type SafeCheck<'a> = dyn Fn(&str, &str) -> Result<crate::safe::bridge::Route, String> + 'a;

fn what_of(args: &Value) -> String {
    args.get("what").and_then(Value::as_str).unwrap_or_default().to_string()
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
    /// Send a helper out to the web, with a room of its own.
    DelegateOut,
    /// Load a group of tools. See `syn::toolset`.
    FindTools,
    /// A tool on a connector: a network request or a program's answer,
    /// which the engine awaits, like `Browse`. See `syn::connector::call`.
    Connector,
    /// A plan of sends, laid out for the person. See `syn::writes`.
    ProposeWrites,
    /// Syn writing a connector's declaration. See `connector::authoring`.
    DraftConnector,
    /// Syn asking the person to approve a connector it drafted.
    ProposeConnector,
    /// One reading call of a draft, before it is proposed.
    TryConnector,
    /// Answered with this refusal, but recorded as a call the run made — it is
    /// a budget, not a rule, and the transcript should show it was reached.
    OverSkillBudget(String),
    /// Syn writing a skill for itself, switched off. See `syn::drafting`.
    DraftSkill,
    /// Syn writing a routine, switched off. See `routine::draft`.
    DraftRoutine,
    /// Held for the person: a change to what was there, asked for after the
    /// run read somebody else's words. See `syn::changes`.
    Stage,
    /// A visual's guide, or a visual looked at and kept: both read the run,
    /// which says whether it was offered them. See `syn::visual::tool`.
    ShowVisual,
    /// A sealed room on this machine: a command awaited, or a file in or out of
    /// it. See `syn::machine::tool`.
    Cell,
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
    // 0. A helper sent out has the web and its own room, and nothing else. It
    // asks nobody: the card for `delegate_out` was the question, and nothing it
    // can reach holds anything of the person's, so what it has read changes
    // nothing it may do. See `syn::delegate`.
    if view.sent_out {
        if !crate::syn::delegate::out_may_use(tool) {
            return Decided::quietly(Gate::Refuse {
                said: crate::syn::delegate::out_refusal(tool),
                note: Some(format!("Refused `{tool}`: not one of a helper's tools out there.")),
            });
        }
        if tool == crate::syn::tools::BROWSE_TOOL && crate::syn::delegate::is_a_file(&what_of(args)) {
            return Decided::quietly(Gate::Refuse { said: crate::syn::delegate::fetch_it_in_the_room(), note: None });
        }
        let how = if tool == crate::syn::tools::BROWSE_TOOL { How::Browse } else { How::Cell };
        return Decided::quietly(Gate::Go(how));
    }
    // A file is fetched whole by a helper, while the run can still send one:
    // `browse` would read its start and close `delegate_out` behind it.
    if tool == crate::syn::tools::BROWSE_TOOL
        && view.sends_out
        && !view.tainted
        && !view.untrusted_before
        && crate::syn::delegate::is_a_file(&what_of(args))
    {
        return Decided::quietly(Gate::Refuse {
            said: crate::syn::delegate::fetch_it_whole(),
            note: Some("Pointed `browse` of a file at `delegate_out`.".into()),
        });
    }
    // A12. The goal is what goes out, so it is written only by a run that has
    // read nothing of anybody else's — in this turn or an earlier one.
    if tool == crate::syn::delegate::OUT_TOOL && (view.tainted || view.untrusted_before) {
        return Decided {
            gate: Gate::Refuse {
                said: crate::syn::delegate::out_after_reading(),
                note: Some(format!("Refused `{tool}`: the conversation has read somebody else's words.")),
            },
            audit: capability.map(|_| Outcome::Refused),
        };
    }

    // 1. Taint.
    //
    // Connectors (Phase F): every tool on an outside server is refused once the run
    // has read anything from outside, reads included — what a read sends goes
    // to the server too. The allowlist below already refuses them, since no
    // `connector__` name is on it; this says why in words that fit, and puts the
    // refusal on the record, because an attempt to reach a server after
    // reading something is exactly what the audit log is for. See `syn::connector`.
    //
    // Two exceptions. A run whose only outside reading is one connector's
    // answer may *read* that same connector again — list the projects, then
    // the issues of one. Nothing goes anywhere it was not already.
    //
    // And a connector the person approved on its card in this work may be
    // read after the work read anything: drafting one from its documentation
    // means reading the web first, and the connector the person had just said
    // yes to was otherwise out of reach (eval of 2026-10-26). Its host is
    // theirs, chosen on the card; what goes to it is held below to their own
    // words and its own answers, so neither the page nor the vault rides out.
    //
    // A write, or any other server, is still refused.
    let slug = crate::syn::connector::provider::server_slug_of(tool);
    let back_to_the_same_reader = view.tainted
        && crate::syn::connector::is_connector_tool(tool)
        && (view.only_connector.is_some_and(|only| slug == Some(only))
            || slug.is_some_and(|s| view.approved_in_work.iter().any(|a| a == s)))
        && matches!(capability, Some(Capability::NetRead { .. }));
    // …and only with words it already had, or the person's. A value lifted
    // from a note would be the vault leaving by the same door.
    if back_to_the_same_reader {
        if let Err(value) = crate::syn::taint::only_echoes(args, view.echoes) {
            let shown: String = value.chars().take(80).collect();
            return Decided {
                gate: Gate::Refuse {
                    said: format!(
                        "`{tool}` may be called in this run only with words from the user's question or \
                         from what this connector itself answered. \"{shown}\" is from neither — it would take \
                         something to the server that it did not have. Ask with values from its earlier answer, or \
                         tell the user what you found and let them ask in a new message."
                    ),
                    note: Some(format!("Refused `{tool}`: \"{shown}\" came from neither the question nor this connector.")),
                },
                audit: capability.map(|_| Outcome::Refused),
            };
        }
    }
    let same_connector_read = back_to_the_same_reader;

    // A send the person approved, exactly as approved, goes — after reading
    // too: approving a plan *is* the person saying where what was read may go.
    // Only in the app, and never from a helper run. See `syn::writes`.
    if crate::syn::connector::is_connector_tool(tool)
        && !view.sub_run
        && view.surface.someone_answers()
        && view.approved_writes.contains(&crate::syn::writes::hash(tool, args))
    {
        return Decided { gate: Gate::Go(How::Connector), audit: capability.map(|_| Outcome::Allowed) };
    }
    // What cannot be taken back is only ever sent through a plan.
    if crate::syn::connector::declared::is_irreversible(tool) {
        return Decided {
            gate: Gate::Refuse {
                said: format!(
                    "`{tool}` sends something that cannot be taken back, so it only goes through a plan the user \
                     approves. Call propose_writes with this request, exactly as you mean to send it."
                ),
                note: Some(format!("Refused `{tool}`: irreversible, and not in an approved plan.")),
            },
            audit: capability.map(|_| Outcome::Refused),
        };
    }
    // After reading, a send needs a plan: what was read may be in it.
    if view.tainted && crate::syn::connector::is_connector_tool(tool) && matches!(capability, Some(Capability::NetWrite { .. })) {
        return Decided {
            gate: Gate::Refuse {
                said: format!(
                    "`{tool}` sends something, and this run has read something from outside that may be in it. \
                     Propose it with propose_writes — every send you mean to make, with its exact arguments — \
                     and the user decides which go."
                ),
                note: Some(format!("Refused `{tool}`: a send after reading, outside a plan.")),
            },
            audit: capability.map(|_| Outcome::Refused),
        };
    }
    if view.tainted && crate::syn::connector::is_connector_tool(tool) && !same_connector_read {
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
    // What rides in every prompt — the person's standing instructions, what
    // Syn remembers, the skills it follows — changes only when the person
    // sees the change, read or not: one quiet edit there is every later
    // run's instruction, Telegram's and the routines' included. Held for them
    // in the app; elsewhere there is nobody to show it to. A plan describes it
    // like anything else.
    if view.rides_prompt && !view.plan_only {
        if !view.sub_run && view.surface.someone_answers() {
            return Decided::quietly(Gate::Go(How::Stage));
        }
        return Decided::quietly(Gate::Refuse {
            said: format!(
                "`{tool}` would change what Syn is told on every question — the user's instructions, a memory or \
                 a skill. That is changed only in the app, where the user sees the change. Say what you would \
                 change instead."
            ),
            note: Some(format!("Refused `{tool}`: it changes what rides in every prompt, away from the app.")),
        });
    }
    // A change to what was there, after reading: held for the person rather
    // than refused, where there is a person to show it to — the app, the run
    // they asked, not a helper or a plan. What was read may have asked for it,
    // so it happens only when they approve it, seeing what it does.
    if view.tainted
        && !view.sub_run
        && !view.plan_only
        && view.surface.someone_answers()
        && crate::syn::policy::staged_after_reading(tool)
    {
        return Decided::quietly(Gate::Go(How::Stage));
    }
    if view.tainted && !same_connector_read && !crate::syn::taint::allowed_after_reading(tool) {
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

    // 2½. A new folder is chosen in a dialog on this Mac, which nobody away from
    // it can answer. The tool itself goes everywhere the room does: a folder
    // the person allowed away from the computer is used by name.
    if matches!(capability, Some(Capability::Door)) && view.surface != Surface::App {
        return Decided {
            gate: Gate::Refuse {
                said: format!(
                    "A new folder can only be opened in the app, where the user chooses it. From {} use \
                     machine_door with `list: true` for the folders allowed here; if none fits, say it has to be \
                     done in the app.",
                    view.surface.label()
                ),
                note: Some(format!("Refused `{tool}`: a new folder is chosen in the app, not from {}.", view.surface.label())),
            },
            audit: Some(Outcome::Refused),
        };
    }

    // 3. Plan first. Reads still run — a plan built without looking is a
    // guess — and so does writing the plan down. Everything else is described.
    // A room changes nothing but itself, so working something out in one is
    // looking too; keeping what it made is not.
    let only_looks = matches!(
        capability,
        None | Some(Capability::VaultRead)
            | Some(Capability::Browse)
            | Some(Capability::MachineFiles)
            | Some(Capability::Cell)
            | Some(Capability::Door)
    );
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

    // 4. Which one, unless they have just said.
    if !crate::syn::ambiguity::already_said(view.said, args, view.seen) {
        if let Some(choice) = crate::syn::ambiguity::should_ask(tool, args, view.seen, view.now) {
            return Decided::quietly(Gate::Choose(Box::new(choice)));
        }
    }

    // 5. Consent.
    let mut audit = None;
    if let Some(capability) = capability {
        let sends = matches!(capability, Capability::NetWrite { .. });
        let may_remember = !sends || crate::syn::connector::declared::write_may_be_remembered(tool);
        let mut decision = crate::syn::consent::decide(capability, view.ledger, view.now);
        // A send said "always" to counts only where nothing in it can point
        // elsewhere (`write_may_be_remembered`), and not in a conversation
        // that has read a stranger's words: the "ok" this run answers may be
        // theirs, as `untrusted_before` says for a Safe item.
        if sends && decision == Decision::Allow && (!may_remember || view.untrusted_before) {
            decision = Decision::Ask;
        }
        // Only ever `Ask` to `Allow`. A `Never` recorded in between is a
        // decision made later about the same thing, and later wins.
        //
        // A yes to this very call, as the card showed it, always stands. A yes
        // to its scope does not after a stranger's words, nor for an ssh
        // `run`, whose every call is another command: it would carry other
        // arguments too.
        // A list of packages is asked about as the list it is: a yes to one is
        // no yes to the next, which would run other code in every room after.
        // A folder is asked for as the folder it will be: a yes to one is no
        // yes to choosing the next.
        let packages = matches!(capability, Capability::Packages | Capability::Door);
        // A command on this Mac goes by one yes to one run, and by nothing
        // else: not a yes to its scope, not one to the call for the rest of
        // the work or the session, not an "always" however it got into the
        // ledger (A7). Only "never" is taken from it.
        let one_run = matches!(capability, Capability::Execute);
        let per_call = (sends && crate::syn::connector::declared::asks_per_call(tool)) || packages || one_run;
        if one_run {
            if decision == Decision::Allow {
                decision = Decision::Ask;
            }
            if decision == Decision::Ask && view.one_run_granted {
                decision = Decision::Allow;
            }
        } else if decision == Decision::Ask
            && (view.call_allowed_until_done
                || ((view.allowed_until_done)(capability) && !packages && !(sends && (view.untrusted_before || per_call))))
        {
            decision = Decision::Allow;
        }
        // "Not now" to this call: refused for the rest of the work, not asked
        // again. See `consent::decline_call_until_done`.
        if decision == Decision::Ask && view.call_declined {
            return Decided {
                gate: Gate::Refuse {
                    said: "The user said not now to this exact call. It was not made; do not make it again or \
                           look for another way to the same thing in this answer. Say it was not done."
                        .to_string(),
                    note: Some(format!("Refused `{tool}`: the user said not now to this call.")),
                },
                audit: Some(Outcome::Refused),
            };
        }
        // Reading the web is asked about because an address can carry data
        // out. A search, a link a page offered, a site the person named, a
        // front page: none of those can, so they are not asked about. What
        // still is: an address the model put together, on a host nobody
        // named, with room in it for something. See `browse_carries_nothing`.
        //
        // Measured before this: 54 of 56 cards in one vault's month were
        // answered "just this once" within seconds, for searches and named
        // sites — a question that was never really a question.
        if decision == Decision::Ask
            && matches!(capability, Capability::Browse)
            && view.reach.is_some_and(|reach| crate::syn::taint::browse_carries_nothing(args, reach))
        {
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
                let mut ask = Ask::about(tool, capability, view.now);
                // No "always" on the card where it would not be honoured.
                ask.can_be_remembered &= may_remember;
                ask.per_call = per_call;
                return Decided { gate: Gate::Ask(Box::new(ask)), audit };
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
            let route = match (view.safe)(tool, &placeholder.handle) {
                Ok(route) => route,
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
            let secret = Capability::UseSecret {
                item: placeholder.handle.clone(),
                destination: route.destination,
                label: route.label,
                tool: route.tool,
                item_id: route.item_id,
            };
            let mut decision = crate::syn::consent::decide(&secret, view.ledger, view.now);
            // An "always" was said about ordinary use. In a conversation that
            // has read a stranger's words, the "ok" this run answers may be
            // theirs: the card is shown again, naming the item and the tool.
            if decision == Decision::Allow && view.untrusted_before {
                decision = Decision::Ask;
            }
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
    } else if tool == crate::syn::delegate::OUT_TOOL {
        How::DelegateOut
    } else if tool == crate::syn::toolset::FIND_TOOL {
        How::FindTools
    } else if tool == crate::syn::writes::TOOL {
        How::ProposeWrites
    } else if tool == crate::syn::connector::authoring::DRAFT_TOOL {
        How::DraftConnector
    } else if tool == crate::syn::connector::authoring::PROPOSE_TOOL {
        How::ProposeConnector
    } else if tool == crate::syn::connector::authoring::TRY_TOOL {
        How::TryConnector
    } else if tool == crate::syn::drafting::TOOL {
        How::DraftSkill
    } else if tool == crate::syn::routine::DRAFT_TOOL {
        How::DraftRoutine
    } else if tool == crate::syn::visual::tool::SHOW_TOOL || tool == crate::syn::visual::tool::GUIDE_TOOL {
        How::ShowVisual
    } else if crate::syn::tools::is_room_tool(tool) {
        How::Cell
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
            only_connector: None,
            approved_in_work: &[],
            echoes: &[],
            approved_writes: &[],
            surface: Surface::App,
            seen: &[],
            said: "",
            ledger,
            allowed_until_done: until_done,
            call_allowed_until_done: false,
            call_declined: false,
            skills_opened: 0,
            plan_only: false,
            sub_run: false,
            sent_out: false,
            sends_out: false,
            one_run_granted: false,
            untrusted_before: false,
            reach: None,
            rides_prompt: false,
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

    /// A7: a yes to one command is a yes to one run of it, said in the
    /// system's dialog. Nothing else lets one through.
    #[test]
    fn a_command_on_this_mac_goes_by_one_yes_to_one_run() {
        let cap = Capability::Execute;
        let line = serde_json::json!({ "command": "git push", "cwd": "~/code/synabit" });
        let run = |v: &View<'_>| decide("machine_host", &line, Some(&cap), v).gate;
        let empty = Ledger::default();
        // Running code has no scope of its own: the ledger files it as "execute".
        let said = |answer| Ledger {
            grants: vec![Grant {
                scope: "execute".into(),
                about: cap.describe(),
                answer,
                granted_at: "2026-09-01T00:00:00+00:00".into(),
                expires_at: None,
            }],
        };

        match run(&view(&empty, &never_until_done)) {
            Gate::Ask(ask) => assert!(ask.per_call && !ask.can_be_remembered, "{ask:?}"),
            other => panic!("expected a card, got {other:?}"),
        }
        // A yes to the scope earlier in the work is not a yes to this run.
        assert!(matches!(run(&view(&empty, &always_until_done)), Gate::Ask(_)));
        // Nor a yes to this very call for the work, or for the session.
        let mut said_for_the_work = view(&empty, &never_until_done);
        said_for_the_work.call_allowed_until_done = true;
        assert!(matches!(run(&said_for_the_work), Gate::Ask(_)));
        // Nor an "always", however it got into the ledger.
        let always = said(Answer::Always);
        assert!(matches!(run(&view(&always, &never_until_done)), Gate::Ask(_)));

        // The one thing that does.
        let mut granted = view(&empty, &never_until_done);
        granted.one_run_granted = true;
        assert!(!matches!(run(&granted), Gate::Ask(_) | Gate::Refuse { .. }), "{:?}", run(&granted));
        // And "never" still wins over it.
        let never = said(Answer::Never);
        let mut refused = view(&never, &never_until_done);
        refused.one_run_granted = true;
        assert!(matches!(run(&refused), Gate::Refuse { .. }));
        // "Not now" to this line holds for the work.
        let mut declined = view(&empty, &never_until_done);
        declined.call_declined = true;
        assert!(matches!(run(&declined), Gate::Refuse { .. }));
    }

    #[test]
    fn an_ordinary_read_goes_to_the_registry() {
        let empty = Ledger::default();
        let d = decide("get_node", &args(), Some(&Capability::VaultRead), &view(&empty, &never_until_done));
        assert!(matches!(d.gate, Gate::Go(How::Execute)), "{d:?}");
        assert_eq!(d.audit, Some(Outcome::Allowed));
    }

    /// "Always send" is not "always send anything to anyone": a send whose
    /// arguments nothing fixes — an MCP tool's, an ssh command's — is asked
    /// each time, an old "always" included, and the card does not offer one.
    #[test]
    fn a_send_with_free_arguments_is_never_remembered() {
        let send = Capability::NetWrite { domain: "Gmail".into(), tool: "send_email".into() };
        let granted = ledger(&send, Answer::Always);
        let v = view(&granted, &never_until_done);
        let d = decide("connector__gmail__send_email", &serde_json::json!({ "to": "a@b.c" }), Some(&send), &v);
        match d.gate {
            Gate::Ask(ask) => assert!(!ask.can_be_remembered, "no always on the card"),
            other => panic!("{other:?}"),
        }
    }

    /// What rides in every prompt changes only where the person sees it —
    /// clean run or not: held in the app and on the phone, refused in a
    /// routine or a helper, described in a plan.
    #[test]
    fn what_rides_in_every_prompt_is_held_even_in_a_clean_run() {
        let granted = ledger(&Capability::VaultWrite, Answer::Always);
        let mut v = view(&granted, &always_until_done);
        v.rides_prompt = true;
        assert!(matches!(decide("update_node", &args(), Some(&Capability::VaultWrite), &v).gate, Gate::Go(How::Stage)));
        let mut phone = view(&granted, &always_until_done);
        phone.rides_prompt = true;
        phone.surface = crate::syn::surface::Surface::Telegram;
        assert!(matches!(decide("update_node", &args(), Some(&Capability::VaultWrite), &phone).gate, Gate::Go(How::Stage)));
        let mut away = view(&granted, &always_until_done);
        away.rides_prompt = true;
        away.surface = crate::syn::surface::Surface::Routine;
        assert!(matches!(decide("update_node", &args(), Some(&Capability::VaultWrite), &away).gate, Gate::Refuse { .. }));
        let mut helper = view(&granted, &always_until_done);
        helper.rides_prompt = true;
        helper.sub_run = true;
        assert!(matches!(decide("update_node", &args(), Some(&Capability::VaultWrite), &helper).gate, Gate::Refuse { .. }));
        let mut plan = view(&granted, &always_until_done);
        plan.rides_prompt = true;
        plan.plan_only = true;
        assert!(matches!(decide("update_node", &args(), Some(&Capability::VaultWrite), &plan).gate, Gate::Go(How::Describe(_))));
    }

    /// First, and before a grant: nothing after it may reopen what it closed.
    #[test]
    fn taint_refuses_before_anything_is_weighed() {
        let granted = ledger(&Capability::VaultWrite, Answer::Always);
        let mut v = view(&granted, &always_until_done);
        v.tainted = true;
        // A change a person can be shown is held for them, in the app or on the phone…
        let d = decide("trash_node", &args(), Some(&Capability::VaultWrite), &v);
        assert!(matches!(d.gate, Gate::Go(How::Stage)), "{d:?}");
        assert_eq!(d.audit, None, "no capability was weighed, so nothing to log");
        // …and refused where there is nobody to show it to, or nothing to show.
        let d = decide("run_recipe", &args(), Some(&Capability::VaultWrite), &v);
        assert!(matches!(d.gate, Gate::Refuse { note: Some(_), .. }), "{d:?}");
        let mut on_the_phone = view(&granted, &always_until_done);
        on_the_phone.tainted = true;
        on_the_phone.surface = crate::syn::surface::Surface::Telegram;
        let d = decide("trash_node", &args(), Some(&Capability::VaultWrite), &on_the_phone);
        assert!(matches!(d.gate, Gate::Go(How::Stage)), "{d:?}");
        let mut a_routine = view(&granted, &always_until_done);
        a_routine.tainted = true;
        a_routine.surface = crate::syn::surface::Surface::Routine;
        assert!(matches!(decide("trash_node", &args(), Some(&Capability::VaultWrite), &a_routine).gate, Gate::Refuse { .. }));
        let mut a_helper = view(&granted, &always_until_done);
        a_helper.tainted = true;
        a_helper.sub_run = true;
        assert!(matches!(decide("trash_node", &args(), Some(&Capability::VaultWrite), &a_helper).gate, Gate::Refuse { .. }));
        let mut planning = view(&granted, &always_until_done);
        planning.tainted = true;
        planning.plan_only = true;
        assert!(!matches!(decide("trash_node", &args(), Some(&Capability::VaultWrite), &planning).gate, Gate::Go(How::Stage)));

        let read = decide("get_node", &args(), Some(&Capability::VaultRead), &v);
        assert!(matches!(read.gate, Gate::Go(How::Execute)), "reading is still reading");
    }

    #[test]
    fn a_surface_refusal_is_logged_and_never_asks() {
        let empty = Ledger::default();
        let mut v = view(&empty, &never_until_done);
        v.surface = Surface::Routine;
        let d = decide("rename_kind", &serde_json::json!({}), Some(&Capability::VaultStructural), &v);
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
    fn which_one_is_not_asked_of_somebody_who_just_said() {
        let empty = Ledger::default();
        let seen = vec![
            Candidate { id: "Notes/a.md".into(), title: "A".into(), node_type: Some("note".into()) },
            Candidate { id: "Notes/b.md".into(), title: "B".into(), node_type: Some("note".into()) },
        ];
        let mut v = view(&empty, &never_until_done);
        v.seen = &seen;
        let call = serde_json::json!({ "node_id": "Notes/b.md" });

        v.said = "I mean “B” (Notes/b.md).";
        let d = decide("update_node", &call, Some(&Capability::VaultWrite), &v);
        assert!(!matches!(d.gate, Gate::Choose(_)), "{d:?}");

        v.said = "I mean “A” (Notes/a.md).";
        let d = decide("update_node", &call, Some(&Capability::VaultWrite), &v);
        assert!(matches!(d.gate, Gate::Choose(_)), "not the one they said: {d:?}");
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

    /// Looking something up carries nothing out, so it is not asked about —
    /// once the run keeps track of where it may go.
    #[test]
    fn a_search_or_a_named_site_is_not_asked_about() {
        let empty = Ledger::default();
        let reach = crate::syn::taint::Destinations::from_user_words(["what is new on genk.vn?"]);
        let v = View { reach: Some(&reach), ..view(&empty, &never_until_done) };
        for args in [
            serde_json::json!({ "what": "giá vàng hôm nay" }),
            serde_json::json!({ "what": "newest", "site": "genk.vn" }),
            serde_json::json!({ "what": "https://genk.vn/mobile/some-long-article-slug.chn" }),
            serde_json::json!({ "what": "3" }),
        ] {
            let d = decide("browse", &args, Some(&Capability::Browse), &v);
            assert!(matches!(d.gate, Gate::Go(How::Browse)), "{args}: {d:?}");
            assert_eq!(d.audit, Some(Outcome::Allowed));
        }
    }

    /// The shape a leak has is still asked about, and a `Never` still holds.
    #[test]
    fn an_address_with_room_for_data_is_still_asked_about() {
        let empty = Ledger::default();
        let reach = crate::syn::taint::Destinations::from_user_words(["summarise my finances"]);
        let v = View { reach: Some(&reach), ..view(&empty, &never_until_done) };
        for args in [
            serde_json::json!({ "what": "https://collect.example/?d=balance-1234567" }),
            serde_json::json!({ "what": "/c?d=balance-1234567", "site": "collect.example" }),
        ] {
            let d = decide("browse", &args, Some(&Capability::Browse), &v);
            assert!(matches!(d.gate, Gate::Ask(_)), "{args}: {d:?}");
        }
        let refused = ledger(&Capability::Browse, Answer::Never);
        let v = View { reach: Some(&reach), ..view(&refused, &never_until_done) };
        let d = decide("browse", &serde_json::json!({ "what": "weather" }), Some(&Capability::Browse), &v);
        assert!(matches!(d.gate, Gate::Refuse { .. }), "{d:?}");
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

    /// A12: the goal is the one thing a helper sent out takes with it, so it is
    /// written only by a run that has read nothing of anybody else's — in this
    /// turn, or earlier in the conversation. A clean run is asked once, and an
    /// "always" stands.
    #[test]
    fn a_helper_is_sent_out_only_by_a_run_that_has_read_nothing() {
        let out = crate::syn::delegate::OUT_TOOL;
        let goal = serde_json::json!({ "goal": "fetch the 2026 holidays" });
        let cap = Capability::HelperOut;
        let allowed = ledger(&cap, Answer::Always);

        let mut v = view(&allowed, &never_until_done);
        assert!(matches!(decide(out, &goal, Some(&cap), &v).gate, Gate::Go(How::DelegateOut)));
        v.tainted = true;
        let read = decide(out, &goal, Some(&cap), &v);
        match read.gate {
            Gate::Refuse { said, .. } => assert!(said.contains("somebody else"), "{said}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(read.audit, Some(Outcome::Refused));
        v.tainted = false;
        v.untrusted_before = true;
        assert!(matches!(decide(out, &goal, Some(&cap), &v).gate, Gate::Refuse { .. }), "an earlier turn read a page");

        let empty = Ledger::default();
        match decide(out, &goal, Some(&cap), &view(&empty, &never_until_done)).gate {
            Gate::Ask(ask) => assert!(ask.can_be_remembered),
            other => panic!("expected a card, got {other:?}"),
        }
        // A helper does not send one of its own.
        let mut helper = view(&allowed, &never_until_done);
        helper.sub_run = true;
        assert!(matches!(decide(out, &goal, Some(&cap), &helper).gate, Gate::Refuse { .. }));
        // And a plan says it would.
        let mut plan = view(&allowed, &never_until_done);
        plan.plan_only = true;
        assert!(matches!(decide(out, &goal, Some(&cap), &plan).gate, Gate::Go(How::Describe(_))));
        // A `Never` turns it off.
        let never = ledger(&cap, Answer::Never);
        assert!(matches!(decide(out, &goal, Some(&cap), &view(&never, &never_until_done)).gate, Gate::Refuse { .. }));
    }

    /// Out there, the helper has the web and its own room and nothing else, and
    /// what it read changes nothing it may do: it holds nothing to lose. It asks
    /// nobody — not for the room, not for the browser.
    #[test]
    fn a_file_on_the_web_is_fetched_by_a_helper_while_one_can_be_sent() {
        let csv = serde_json::json!({ "what": "https://raw.githubusercontent.com/datasets/population/main/data/population.csv" });
        let page = serde_json::json!({ "what": "https://github.com/datasets/population" });
        let empty = Ledger::default();
        let browse = |args: &serde_json::Value, v: &View<'_>| decide("browse", args, Some(&Capability::Browse), v).gate;

        let mut v = view(&empty, &never_until_done);
        v.sends_out = true;
        match browse(&csv, &v) {
            Gate::Refuse { said, .. } => assert!(said.contains("delegate_out"), "{said}"),
            other => panic!("expected the helper, got {other:?}"),
        }
        assert!(!matches!(browse(&page, &v), Gate::Refuse { .. }), "a page is still read");
        // Once it has read, no helper can be sent: the file is read as it can be.
        v.tainted = true;
        assert!(!matches!(browse(&csv, &v), Gate::Refuse { .. }));
        v.tainted = false;
        v.untrusted_before = true;
        assert!(!matches!(browse(&csv, &v), Gate::Refuse { .. }));
        // Nor without `delegate_out`.
        let none = view(&empty, &never_until_done);
        assert!(!matches!(browse(&csv, &none), Gate::Refuse { .. }));
        // The helper itself downloads it in its room.
        let mut helper = view(&empty, &never_until_done);
        helper.sub_run = true;
        helper.sent_out = true;
        match browse(&csv, &helper) {
            Gate::Refuse { said, .. } => assert!(said.contains("python3"), "{said}"),
            other => panic!("expected the room, got {other:?}"),
        }
    }

    #[test]
    fn a_helper_sent_out_has_the_web_and_its_room_and_nothing_else() {
        let empty = Ledger::default();
        let mut v = view(&empty, &never_until_done);
        v.sub_run = true;
        v.sent_out = true;
        v.tainted = true;
        let browse = decide("browse", &serde_json::json!({ "what": "x", "site": "https://evil.example/?d=1" }), Some(&Capability::Browse), &v);
        assert!(matches!(browse.gate, Gate::Go(How::Browse)), "{browse:?}");
        for tool in [crate::syn::tools::ROOM_RUN, crate::syn::tools::ROOM_WRITE, crate::syn::tools::ROOM_KEEP] {
            let d = decide(tool, &serde_json::json!({ "command": "ls" }), Some(&Capability::Cell), &v);
            assert!(matches!(d.gate, Gate::Go(How::Cell)), "{tool}: {d:?}");
        }
        for (tool, capability) in [
            ("get_node", Capability::VaultRead),
            ("recall", Capability::VaultRead),
            (crate::syn::tools::ROOM_TAKE, Capability::Cell),
            ("safe_list", Capability::VaultRead),
            (crate::syn::delegate::OUT_TOOL, Capability::HelperOut),
            ("create_node", Capability::VaultWrite),
        ] {
            let d = decide(tool, &args(), Some(&capability), &v);
            assert!(matches!(d.gate, Gate::Refuse { .. }), "{tool}: {d:?}");
        }
        let connector = decide("connector__jira__search", &args(), Some(&Capability::NetRead { domain: "jira".into() }), &v);
        assert!(matches!(connector.gate, Gate::Refuse { .. }));
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

    /// A room changes nothing but itself, so a plan may work something out in
    /// one — asked about like any other use of it. Keeping what it made is a
    /// change, and is described.
    #[test]
    fn plan_mode_works_in_a_room_and_describes_keeping_from_it() {
        let allowed = ledger(&Capability::Cell, Answer::Always);
        let mut v = view(&allowed, &never_until_done);
        v.plan_only = true;
        let run = decide(crate::syn::tools::ROOM_RUN, &serde_json::json!({ "command": "ls" }), Some(&Capability::Cell), &v);
        assert!(matches!(run.gate, Gate::Go(How::Cell)), "{run:?}");
        let keep = decide(crate::syn::tools::ROOM_KEEP, &serde_json::json!({ "path": "a.pdf" }), Some(&Capability::VaultWrite), &v);
        assert!(matches!(keep.gate, Gate::Go(How::Describe(_))), "{keep:?}");

        let empty = Ledger::default();
        let first = decide(crate::syn::tools::ROOM_RUN, &serde_json::json!({ "command": "ls" }), Some(&Capability::Cell), &view(&empty, &never_until_done));
        assert!(matches!(first.gate, Gate::Ask(_)), "off until turned on: {first:?}");
    }

    /// A15: packages are never installed after reading — what was read may be
    /// what asked for them, and they would run in every room after. Otherwise
    /// asked each time, about the list, with no "this whole work" to it; and not
    /// at all where nobody can see the card, nor in a plan.
    #[test]
    fn packages_are_asked_about_each_time_and_never_after_reading() {
        let tool = crate::syn::tools::ROOM_ENV;
        let args = serde_json::json!({ "packages": ["python-docx"] });
        let empty = Ledger::default();

        let mut v = view(&empty, &always_until_done);
        let first = decide(tool, &args, Some(&Capability::Packages), &v);
        match first.gate {
            Gate::Ask(ask) => assert!(ask.per_call && !ask.can_be_remembered, "{ask:?}"),
            other => panic!("expected a card: {other:?}"),
        }

        v.tainted = true;
        assert!(matches!(decide(tool, &args, Some(&Capability::Packages), &v).gate, Gate::Refuse { .. }));

        let mut phone = view(&empty, &never_until_done);
        phone.surface = Surface::Telegram;
        assert!(matches!(decide(tool, &args, Some(&Capability::Packages), &phone).gate, Gate::Refuse { .. }));

        let mut plan = view(&empty, &never_until_done);
        plan.plan_only = true;
        assert!(matches!(decide(tool, &args, Some(&Capability::Packages), &plan).gate, Gate::Go(How::Describe(_))));

        let mut once = view(&empty, &never_until_done);
        once.call_allowed_until_done = true;
        assert!(matches!(decide(tool, &args, Some(&Capability::Packages), &once).gate, Gate::Go(How::Cell)));

        let off = ledger(&Capability::Packages, Answer::Never);
        assert!(matches!(decide(tool, &args, Some(&Capability::Packages), &view(&off, &never_until_done)).gate, Gate::Refuse { .. }));
    }

    /// A new folder: asked about every time, the folder chosen being the answer,
    /// and after reading too — the person picks it. Never away from the app, and
    /// shown in plan mode, since choosing a folder to read is looking. A folder
    /// already open goes as the room does.
    #[test]
    fn a_folder_is_asked_for_each_time_and_only_at_the_app() {
        let tool = crate::syn::tools::ROOM_DOOR;
        let args = serde_json::json!({});
        let empty = Ledger::default();

        let mut v = view(&empty, &always_until_done);
        v.tainted = true;
        match decide(tool, &args, Some(&Capability::Door), &v).gate {
            Gate::Ask(ask) => assert!(ask.per_call && !ask.can_be_remembered, "{ask:?}"),
            other => panic!("expected a card: {other:?}"),
        }
        let mut plan = view(&empty, &never_until_done);
        plan.plan_only = true;
        assert!(matches!(decide(tool, &args, Some(&Capability::Door), &plan).gate, Gate::Ask(_)));
        for surface in [Surface::Telegram, Surface::Routine] {
            let mut away = view(&empty, &never_until_done);
            away.surface = surface;
            assert!(matches!(decide(tool, &args, Some(&Capability::Door), &away).gate, Gate::Refuse { .. }), "{surface:?}");
        }
        let mut once = view(&empty, &never_until_done);
        once.call_allowed_until_done = true;
        assert!(matches!(decide(tool, &args, Some(&Capability::Door), &once).gate, Gate::Go(How::Cell)));
        let off = ledger(&Capability::Door, Answer::Never);
        assert!(matches!(decide(tool, &args, Some(&Capability::Door), &view(&off, &never_until_done)).gate, Gate::Refuse { .. }));

        let room = ledger(&Capability::Cell, Answer::Always);
        let mut phone = view(&room, &never_until_done);
        phone.surface = Surface::Telegram;
        let named = serde_json::json!({ "door": "Taxes" });
        assert!(matches!(decide(tool, &named, Some(&Capability::Cell), &phone).gate, Gate::Go(How::Cell)));
    }

    /// Reading a page does not close the room: whatever the page talks Syn
    /// into happens in a room with no way out, and is marked there.
    #[test]
    fn a_room_stays_open_after_reading() {
        let allowed = ledger(&Capability::Cell, Answer::Always);
        let mut v = view(&allowed, &never_until_done);
        v.tainted = true;
        for tool in crate::syn::tools::ROOM_TOOLS {
            let capability = crate::syn::policy::capability(tool, &serde_json::json!({})).expect("a row");
            let d = decide(tool, &serde_json::json!({}), Some(&capability), &v);
            assert!(matches!(d.gate, Gate::Go(How::Cell)), "{tool}: {d:?}");
        }
    }

}
