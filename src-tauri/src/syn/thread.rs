//! A piece of work that is open between the user and Syn.
//!
//! # The gap this fills
//!
//! There are two units of work in this app and neither is this one. A `Run` is
//! a unit of *computation*: it has a budget, it ends, and when it ends it is
//! history. A `Task` belongs to the user, lives in the Tasks app, and is
//! something they tick off.
//!
//! Between them sits the thing a colleague is actually for: *the pricing
//! question*, *getting ready for Friday*, *the forty unread articles*. It
//! spans days, several conversations and many runs. Nobody completes it in one
//! sitting and nobody ticks it off. Until now it existed only in the user's
//! head, which meant Syn started from nothing every time it was mentioned.
//!
//! # Why a thread is an ordinary node, and memory is not
//!
//! `syn_memory` and `syn_skill` are hidden from `list_schemas` by
//! `tools::is_internal_type`, because they are Syn's own bookkeeping: a vault
//! should not report that its owner "keeps" forty memories beside their notes.
//!
//! A thread is the opposite. It is the user's work, written down. So it is a
//! normal type: it appears in Things, it is found by Nexus, it is a node in the
//! graph, `query_nodes` reaches it, and the existing tools read and write it
//! without any of them being told it exists.
//!
//! # Why this module adds no tools
//!
//! Because it does not need any, and every tool costs tokens on every turn of
//! every conversation — the reason twenty tools once became twelve.
//!
//! * Listing threads is `query_nodes` with `type:syn_thread`.
//! * Noting a finding is `update_node`, which the assistant already reaches for
//!   more than any other write.
//! * What it touches is wikilinks in the body, which `get_linked_nodes`
//!   already follows.
//!
//! What is left is *opening* one, and that is deliberately not the model's job.
//! A tool the model has to think of calling is a tool that does not get called:
//! `recall` went unused across fifteen real runs, and the skill-proposal
//! detector fired on none of seventeen. A thread is started by a person
//! pressing a button, and from then on it reaches the model the way memory
//! does — by riding in the prompt, not by waiting to be looked up.
//!
//! # Why the frontmatter has two fields
//!
//! The design this was built from listed six: title, state, opened,
//! last_moved, waiting_for, touches, runs. Four of them were already answered
//! by the node itself, and storing them again would mean keeping two copies
//! correct with only one of them maintained:
//!
//! | Wanted | Where it already lives |
//! | --- | --- |
//! | `title` | `NodeMetadata::title` |
//! | `opened` | `created_at` |
//! | `last_moved` | `updated_at`, maintained by the index on every write |
//! | `touches` | wikilinks in the body, and `node_edges` |
//!
//! `last_moved` is the one that mattered. Because writes go through the generic
//! `update_node`, a `last_moved` field would have been updated by nothing and
//! would have gone stale immediately — a date that says a thread moved last
//! week when it moved this morning is worse than no date.
//!
//! So: `state`, and `waiting_for`. Everything else the vault knew already.

use serde::{Deserialize, Serialize};

use crate::db::DbBridge;
use crate::error::AppResult;
use crate::models::node::NodeMetadata;

/// The `type:` a thread carries.
///
/// Prefixed like `syn_memory` and `syn_skill`, and for the reason the user gave
/// when memory was named: `thread` is an ordinary word and this is their vault.
/// Somebody tracking sewing, or forum threads, or a thread of an argument has
/// every right to a kind by that name.
pub const THREAD_TYPE: &str = "syn_thread";

/// Where threads are filed.
///
/// Top-level, not under `Syn/`. `is_in_unscanned_dir` skips anything named
/// `Syn`, so a thread written there would be indexed by the write that created
/// it and dropped by the next full scan — present on disk, correct, and
/// unreachable. The same trap `SynSkills` was moved out of.
pub const THREAD_FOLDER: &str = "SynThreads";

/// How much of the open thread rides in the prompt.
///
/// Three thousand characters, against memory's 3,200 and retrieval's 12,000.
/// One thread, not all of them: the others are found by searching, and a
/// prompt carrying every open piece of work would be mostly about work this
/// question is not.
///
/// Past it the body is cut and the prompt says so, the way a long selection is
/// — a silently truncated thread is how Syn confidently reports that something
/// was never decided when the decision is in the part that was dropped.
pub const THREAD_BUDGET_CHARS: usize = 3_000;

/// Who the work is waiting on.
///
/// Not `done`/`not done`, which is the language of a task. The question a
/// colleague can always answer is *whose move is it*, and these are the
/// answers to that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Syn is working on it.
    Mine,
    /// Syn has handed it back; the user's move.
    Yours,
    /// Waiting on something outside — a reply, a build, a person.
    World,
    /// Alive, nobody's move right now.
    #[default]
    Resting,
    /// Finished. Kept, not deleted.
    Closed,
}

/// Refuses to compile when a state is added and `ALL` is not updated.
///
/// Only the compiler knows every variant; a test that enumerates them is
/// enumerating the list it is meant to check.
#[allow(dead_code)]
fn _every_state_is_listed(state: State) {
    match state {
        State::Mine | State::Yours | State::World | State::Resting | State::Closed => {}
    }
}

impl State {
    pub const ALL: [State; 5] = [
        State::Mine,
        State::Yours,
        State::World,
        State::Resting,
        State::Closed,
    ];

    /// Anything unrecognised rests.
    ///
    /// Failing towards the state that claims nothing. A typo that landed a
    /// thread in `yours` would have Syn tell somebody they owe an answer they
    /// were never asked for; a typo that lands it in `resting` costs nothing
    /// but a thread that waits to be picked up.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "mine" => State::Mine,
            "yours" => State::Yours,
            "world" => State::World,
            "closed" => State::Closed,
            _ => State::Resting,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            State::Mine => "mine",
            State::Yours => "yours",
            State::World => "world",
            State::Resting => "resting",
            State::Closed => "closed",
        }
    }

    /// How the prompt says it, in the second person Syn is speaking in.
    fn describe(self) -> &'static str {
        match self {
            State::Mine => "you are working on it",
            State::Yours => "it is back with the user",
            State::World => "it is waiting on something outside",
            State::Resting => "it is alive but nobody's move",
            State::Closed => "it is finished",
        }
    }

    /// Whether it still counts as open work.
    pub fn is_open(self) -> bool {
        !matches!(self, State::Closed)
    }
}

/// One piece of work, open between the two of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Vault-relative path, which is how every node tool addresses it.
    pub id: String,
    pub title: String,
    /// The prose the two of them keep. Not parsed: it is a document, and a
    /// document with a schema is a form.
    pub body: String,
    pub state: State,
    /// What it is waiting for, in whoever's words wrote it down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_for: Option<String>,
    /// From the node. Not stored twice — see the note at the top of this file.
    pub opened: String,
    pub last_moved: String,
}

impl Thread {
    /// Read a thread out of the node it is stored as.
    ///
    /// Tolerant, because these are files people will edit by hand: a missing
    /// state rests, a missing `waiting_for` is simply not waiting on anything
    /// nameable, and neither is worth refusing to load somebody's own file
    /// over.
    pub fn from_node(node: &NodeMetadata) -> Self {
        Self {
            id: node.id.clone(),
            title: node.title.clone(),
            body: node.content.trim().to_string(),
            state: node
                .properties
                .get("state")
                .and_then(|v| v.as_str())
                .map(State::parse)
                .unwrap_or_default(),
            waiting_for: node
                .properties
                .get("waiting_for")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            opened: node.created_at.clone(),
            last_moved: node.updated_at.clone(),
        }
    }

    /// The block that rides in the prompt when this thread is the one open.
    ///
    /// It says three things, and the third is the one that earns its place:
    /// what the work is, whose move it is, and that **what is written here has
    /// already been established**. Without that last sentence a model reads the
    /// body as more retrieved context and goes and looks the same things up
    /// again — which is the failure the thread exists to prevent.
    pub fn block(&self) -> String {
        let mut out = String::from("\n=== THE WORK THIS BELONGS TO ===\n");

        out.push_str(&format!(
            "\"{}\" — open since {}, last moved {}. Right now {}.\n",
            self.title,
            self.opened.split('T').next().unwrap_or(&self.opened),
            self.last_moved.split('T').next().unwrap_or(&self.last_moved),
            self.state.describe(),
        ));

        if let Some(waiting) = &self.waiting_for {
            out.push_str(&format!("Waiting for: {waiting}\n"));
        }

        out.push_str(
            "What follows was worked out already, by the two of you. Build on it rather than \
             deriving it again, and do not treat it as everything the vault knows.\n",
        );

        if self.body.is_empty() {
            out.push_str("(Nothing written down yet.)\n");
        } else {
            let total = self.body.chars().count();
            let shown: String = self.body.chars().take(THREAD_BUDGET_CHARS).collect();
            out.push_str("---\n");
            out.push_str(&shown);
            out.push_str("\n---\n");
            if total > THREAD_BUDGET_CHARS {
                out.push_str(&format!(
                    "(The first {THREAD_BUDGET_CHARS} characters of {total}. Read all of \
                     `{}` with `get_node` before saying what it does or does not contain.)\n",
                    self.id,
                ));
            }
        }

        out.push_str(&format!(
            "Add what you find to `{}` with `update_node` rather than only saying it, or the \
             next conversation starts over.\n=== END WORK ===\n\n",
            self.id,
        ));

        out
    }
}

/// The frontmatter a new thread is written with.
///
/// Two keys. Everything else a thread needs to say about itself is already on
/// the node — see the table at the top of this file.
pub fn frontmatter(state: State, waiting_for: Option<&str>) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    props.insert("state".into(), serde_json::json!(state.as_str()));
    if let Some(waiting) = waiting_for.map(str::trim).filter(|s| !s.is_empty()) {
        props.insert("waiting_for".into(), serde_json::json!(waiting));
    }
    serde_json::Value::Object(props)
}

/// Every thread in the vault, most recently moved first.
pub fn all(db: &DbBridge) -> AppResult<Vec<Thread>> {
    let mut threads: Vec<Thread> = db
        .get_nodes_by_type(THREAD_TYPE)?
        .iter()
        .map(Thread::from_node)
        .collect();
    threads.sort_by(|a, b| b.last_moved.cmp(&a.last_moved));
    Ok(threads)
}

/// One thread by its path, or `None` when it is not there any more.
///
/// `None` rather than an error, because the common way to reach this is a
/// thread id remembered by a window that was open while the file was trashed.
/// That is not a failure worth refusing a question over — the question is
/// answered without the thread.
pub fn get(db: &DbBridge, id: &str) -> Option<Thread> {
    all(db).ok()?.into_iter().find(|t| t.id == id)
}

/// The body a thread starts with.
///
/// Headings rather than an empty file, because an empty document is one nobody
/// writes in and these are meant to be written in by both of them. The three
/// are the questions a piece of work always has: what it is, what is known, and
/// what is left.
pub fn starting_body() -> &'static str {
    "## What this is\n\n\n## What we found\n\n\n## Still open\n\n"
}

// ═══════════════════════════════════════════════════════════════
//  WHETHER ANY OF THIS IS USED
// ═══════════════════════════════════════════════════════════════

/// What the runs say about one thread, and about threads in general.
///
/// # Why this exists at all
///
/// Because the two things this repository keeps finding are a feature nobody
/// reaches and a measurement that was wrong in a plausible way. `recall` went
/// uncalled across fifteen real runs; the skill-proposal detector fired on none
/// of seventeen. Both were found by counting, and neither would have been found
/// by using the app and forming an impression.
///
/// Threads are the same shape of bet: the model is *told* to write what it
/// finds back into the thread, and being told is not the same as doing it —
/// `docs/adr-rag-vs-agentic-2026-09-03.md` measured a prompt instruction that
/// changed nothing at all and removed it again. So the question is not whether
/// a thread reached the model. It is **how often a run that had one wrote
/// anything back**, and until that number exists there is nothing to decide on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Usage {
    /// Runs whose prompt carried this thread.
    pub runs: u32,
    /// Of those, runs that successfully wrote into it.
    pub wrote_back: u32,
}

/// The counts, per thread and in total.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Stats {
    /// Keyed by thread id. Includes threads since trashed — a run that happened
    /// is a run that happened, and dropping it would flatter the total.
    pub per_thread: std::collections::BTreeMap<String, Usage>,
    /// Every run on disk, which is the denominator that says how rare this is.
    pub runs_total: u32,
    pub runs_in_a_thread: u32,
    pub runs_that_wrote_back: u32,
}

/// Whether a step is a successful write into `into`.
///
/// `update_node` and not `create_node`: creating a node under `SynThreads/` is
/// starting a thread, not adding to one, and counting it would score the act of
/// making a thread as evidence that threads get used.
fn wrote_into(step: &crate::syn::run::Step, into: &str) -> bool {
    if step.ok != Some(true) || step.tool.as_deref() != Some("update_node") {
        return false;
    }
    step.args
        .as_ref()
        .and_then(|a| a.get("node_id"))
        .and_then(|v| v.as_str())
        .is_some_and(|id| id == into)
}

/// Read the counts off the runs.
///
/// A pure function over runs the caller has already loaded, so it can be tested
/// against runs built in memory rather than against whatever happens to be in
/// somebody's vault.
pub fn usage(runs: &[crate::syn::run::Run]) -> Stats {
    let mut stats = Stats {
        runs_total: runs.len() as u32,
        ..Default::default()
    };

    for run in runs {
        let Some(thread) = run.thread.as_deref() else {
            continue;
        };
        stats.runs_in_a_thread += 1;

        let entry = stats.per_thread.entry(thread.to_string()).or_default();
        entry.runs += 1;

        if run.steps.iter().any(|step| wrote_into(step, thread)) {
            entry.wrote_back += 1;
            stats.runs_that_wrote_back += 1;
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(title: &str, body: &str, props: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: format!("{THREAD_FOLDER}/{title}.md"),
            node_type: THREAD_TYPE.to_string(),
            title: title.to_string(),
            content: body.to_string(),
            properties: props,
            created_at: "2026-09-01T08:00:00Z".into(),
            updated_at: "2026-09-06T09:30:00Z".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    // ─── the counting ───────────────────────────────────────

    fn run_in(thread: Option<&str>, steps: Vec<crate::syn::run::Step>) -> crate::syn::run::Run {
        let mut run = crate::syn::run::Run::new("ask", None, crate::syn::run::Budget { iterations: None, tool_calls: None, tokens: None, wall_ms: None });
        run.thread = thread.map(str::to_string);
        run.steps = steps;
        run
    }

    fn tool_step(tool: &str, node_id: &str, ok: bool) -> crate::syn::run::Step {
        crate::syn::run::Step {
            index: 0,
            kind: crate::syn::run::StepKind::ToolCall,
            iteration: 0,
            tool: Some(tool.to_string()),
            args: Some(json!({ "node_id": node_id })),
            ok: Some(ok),
            reversal: None,
            preview: String::new(),
            tokens: None,
            ms: 0,
            at: "2026-09-06T00:00:00Z".to_string(),
        }
    }

    const T: &str = "SynThreads/Pricing.md";

    #[test]
    fn a_run_outside_any_thread_counts_only_towards_the_total() {
        let stats = usage(&[run_in(None, vec![])]);
        assert_eq!(stats.runs_total, 1);
        assert_eq!(stats.runs_in_a_thread, 0);
        assert!(stats.per_thread.is_empty());
    }

    #[test]
    fn a_run_that_wrote_back_is_counted_once_for_the_thread_and_once_overall() {
        let stats = usage(&[run_in(Some(T), vec![tool_step("update_node", T, true)])]);

        assert_eq!(stats.runs_in_a_thread, 1);
        assert_eq!(stats.runs_that_wrote_back, 1);
        assert_eq!(stats.per_thread[T], Usage { runs: 1, wrote_back: 1 });
    }

    /// The number this whole thing exists to produce: a run that had the thread
    /// in front of it and wrote nothing back.
    #[test]
    fn a_run_that_only_read_counts_as_not_having_written_back() {
        let stats = usage(&[run_in(Some(T), vec![tool_step("get_node", T, true)])]);

        assert_eq!(stats.runs_in_a_thread, 1);
        assert_eq!(stats.runs_that_wrote_back, 0);
        assert_eq!(stats.per_thread[T], Usage { runs: 1, wrote_back: 0 });
    }

    /// Creating a node under `SynThreads/` is *starting* a thread. Counting it
    /// would score the act of making one as evidence that threads get used.
    #[test]
    fn making_a_thread_is_not_writing_into_one() {
        let stats = usage(&[run_in(Some(T), vec![tool_step("create_node", T, true)])]);
        assert_eq!(stats.runs_that_wrote_back, 0);
    }

    /// A call that came back an error is not a write, and counting it would
    /// make the instrument report success it never saw.
    #[test]
    fn a_failed_write_is_not_a_write() {
        let stats = usage(&[run_in(Some(T), vec![tool_step("update_node", T, false)])]);
        assert_eq!(stats.runs_that_wrote_back, 0);
    }

    /// Writing to a *different* thread does not count for this one. Without
    /// this the number would be "wrote into some thread", which is a different
    /// and much easier claim.
    #[test]
    fn writing_into_another_thread_does_not_count_for_this_one() {
        let other = "SynThreads/Friday.md";
        let stats = usage(&[run_in(Some(T), vec![tool_step("update_node", other, true)])]);

        assert_eq!(stats.per_thread[T], Usage { runs: 1, wrote_back: 0 });
        assert_eq!(stats.runs_that_wrote_back, 0);
    }

    /// Two writes in one run are one run that wrote back, not two.
    #[test]
    fn a_run_is_counted_once_however_many_times_it_wrote() {
        let stats = usage(&[run_in(
            Some(T),
            vec![tool_step("update_node", T, true), tool_step("update_node", T, true)],
        )]);
        assert_eq!(stats.per_thread[T], Usage { runs: 1, wrote_back: 1 });
    }

    #[test]
    fn the_totals_add_up_across_threads_and_runs() {
        let other = "SynThreads/Friday.md";
        let stats = usage(&[
            run_in(None, vec![]),
            run_in(Some(T), vec![tool_step("update_node", T, true)]),
            run_in(Some(T), vec![]),
            run_in(Some(other), vec![tool_step("update_node", other, true)]),
        ]);

        assert_eq!(stats.runs_total, 4);
        assert_eq!(stats.runs_in_a_thread, 3);
        assert_eq!(stats.runs_that_wrote_back, 2);
        assert_eq!(stats.per_thread[T], Usage { runs: 2, wrote_back: 1 });
        assert_eq!(stats.per_thread[other], Usage { runs: 1, wrote_back: 1 });
        assert_eq!(
            stats.per_thread.values().map(|u| u.runs).sum::<u32>(),
            stats.runs_in_a_thread,
            "the per-thread counts have to add up to the total"
        );
    }

    /// The trap `SynSkills` was moved out of, checked rather than remembered.
    ///
    /// `Syn/` is the obvious home — conversations, settings and run transcripts
    /// all live there — and it fails silently. The scan skips any path segment
    /// named exactly `Syn`, so a thread written there is indexed by the write
    /// that creates it, works for the rest of the session, and is dropped by
    /// the next full scan: on disk, correct, and unreachable.
    #[test]
    fn threads_are_filed_somewhere_the_vault_scan_will_look() {
        assert!(
            !crate::commands::nodes::is_in_unscanned_dir(&format!("{THREAD_FOLDER}/x.md")),
            "`{THREAD_FOLDER}` is skipped by the vault scan, so threads filed there \
             survive until the next rescan and then vanish"
        );
    }

    /// The word `thread` belongs to whoever owns the vault — sewing, forums,
    /// the thread of an argument.
    #[test]
    fn the_unprefixed_word_is_left_for_the_user() {
        assert_eq!(THREAD_TYPE, "syn_thread");
        assert_ne!(THREAD_TYPE, "thread");
        assert_ne!(THREAD_FOLDER, "Threads");
    }

    /// A thread is the user's work, not Syn's bookkeeping, so unlike a memory
    /// or a skill it is reported by `list_schemas` and reachable by the generic
    /// tools. That is what lets this module ship without adding any.
    #[test]
    fn a_thread_is_not_hidden_the_way_syns_own_storage_is() {
        assert!(
            !crate::syn::tools::is_internal_type(THREAD_TYPE),
            "a thread is the user's work; hiding it would also hide it from \
             `query_nodes`, which is the only way anything reads one"
        );
        assert!(crate::syn::tools::is_internal_type("syn_memory"));
        assert!(crate::syn::tools::is_internal_type("syn_skill"));
    }

    #[test]
    fn a_thread_reads_its_dates_off_the_node_rather_than_its_own_fields() {
        let t = Thread::from_node(&node("Pricing", "body", json!({ "state": "yours" })));

        assert_eq!(t.opened, "2026-09-01T08:00:00Z");
        assert_eq!(t.last_moved, "2026-09-06T09:30:00Z");
    }

    /// The whole reason the dates are not stored: writes go through the generic
    /// `update_node`, which knows nothing about threads. A `last_moved`
    /// property would be updated by nobody.
    #[test]
    fn the_date_it_last_moved_follows_an_ordinary_write() {
        let mut n = node("Pricing", "body", json!({ "state": "mine" }));
        n.updated_at = "2026-12-25T00:00:00Z".to_string();

        assert_eq!(Thread::from_node(&n).last_moved, "2026-12-25T00:00:00Z");
    }

    #[test]
    fn a_missing_state_rests_rather_than_claiming_somebody_owes_something() {
        assert_eq!(Thread::from_node(&node("A", "", json!({}))).state, State::Resting);
        assert_eq!(State::parse("nonsense"), State::Resting);
        assert_eq!(State::parse(""), State::Resting);
    }

    #[test]
    fn every_state_survives_the_round_trip() {
        for state in State::ALL {
            assert_eq!(State::parse(state.as_str()), state, "{}", state.as_str());
        }
    }

    #[test]
    fn only_closed_is_not_open() {
        for state in State::ALL {
            assert_eq!(state.is_open(), state != State::Closed, "{}", state.as_str());
        }
    }

    #[test]
    fn the_block_says_what_the_work_is_and_whose_move_it_is() {
        let t = Thread::from_node(&node(
            "Giá cho bản Pro",
            "Minh phản đối per-seat vì khách team nhỏ.",
            json!({ "state": "yours", "waiting_for": "quyết per-seat hay flat" }),
        ));
        let block = t.block();

        assert!(block.contains("Giá cho bản Pro"), "{block}");
        assert!(block.contains("back with the user"), "{block}");
        assert!(block.contains("quyết per-seat hay flat"), "{block}");
        assert!(block.contains("Minh phản đối"), "{block}");
    }

    /// The sentence that earns the section its tokens. Without it the body
    /// reads as more retrieved context and the model re-derives it.
    #[test]
    fn the_block_says_the_work_was_already_done() {
        let block = Thread::from_node(&node("A", "found this", json!({}))).block();
        assert!(block.contains("Build on it rather than deriving it again"), "{block}");
    }

    /// A thread nobody writes back into is a thread that stops being worth
    /// having, so the prompt names the way to write to it.
    #[test]
    fn the_block_says_how_to_add_to_it() {
        let block = Thread::from_node(&node("A", "x", json!({}))).block();
        assert!(block.contains("update_node"), "{block}");
        assert!(block.contains("SynThreads/A.md"), "the path to write to:\n{block}");
    }

    #[test]
    fn an_empty_thread_says_so_rather_than_showing_empty_fences() {
        let block = Thread::from_node(&node("A", "", json!({}))).block();
        assert!(block.contains("Nothing written down yet"), "{block}");
        assert!(!block.contains("---"), "no empty fences:\n{block}");
    }

    /// Truncation that does not announce itself is how Syn reports that
    /// something was never decided when the decision was in the cut part.
    #[test]
    fn a_long_thread_is_cut_and_says_so() {
        let long = "z".repeat(THREAD_BUDGET_CHARS + 500);
        let block = Thread::from_node(&node("Long", &long, json!({}))).block();

        assert!(
            block.contains(&format!("characters of {}", THREAD_BUDGET_CHARS + 500)),
            "{block}"
        );
        assert!(block.contains("`get_node`"), "{block}");
        assert!(
            block.contains("before saying what it does or does not contain"),
            "{block}"
        );
    }

    #[test]
    fn the_frontmatter_carries_two_keys_and_no_more() {
        let props = frontmatter(State::Mine, Some("  a reply  "));
        let map = props.as_object().expect("an object");

        assert_eq!(map.get("state").and_then(|v| v.as_str()), Some("mine"));
        assert_eq!(map.get("waiting_for").and_then(|v| v.as_str()), Some("a reply"));
        assert_eq!(map.len(), 2, "nothing the node already knows: {map:?}");
    }

    #[test]
    fn nothing_to_wait_for_writes_no_key() {
        for empty in [None, Some(""), Some("   ")] {
            let props = frontmatter(State::Resting, empty);
            assert_eq!(props.as_object().expect("an object").len(), 1, "{empty:?}");
        }
    }

    /// An empty document is one nobody writes in, and both of them are meant to.
    #[test]
    fn a_new_thread_starts_with_the_questions_work_always_has() {
        let body = starting_body();
        assert!(body.contains("## What this is"));
        assert!(body.contains("## What we found"));
        assert!(body.contains("## Still open"));
    }
}
