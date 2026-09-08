//! Things that are true about the work, that nobody asked about.
//!
//! # Why this is the first kind of initiative worth having
//!
//! "Proactive agent" usually means one that *runs* something while you are not
//! looking. That is the risky kind, and it is a later problem. The first and
//! safest kind is different: **noticing a fact about the work that arithmetic
//! can see.** No model, no background job with permissions, nothing new to
//! trust.
//!
//! Three such facts are visible in this vault today and nothing looks at any of
//! them:
//!
//! * A **thread waiting on the outside world** whose `last_moved` is three
//!   weeks old. Nobody is waiting on anybody — it is just dying. Nothing reads
//!   `last_moved` except the screen that lists threads.
//! * **Two memories that cannot both be true.** `memory::conflicting` finds
//!   same-`kind`-same-`subject` collisions at the moment one is written, and
//!   nothing has ever swept the set to say *these two disagree*.
//! * A **skill that has stopped working.** The "ran under a skill and still
//!   failed" detector exists per-run; nothing counts it over time.
//!
//! # The hard constraint
//!
//! **Noticing is not doing.** Nothing in this module fixes, closes, merges,
//! rewrites or disables anything. It produces a sentence and stops.
//!
//! That is exactly where a colleague differs from an automation. A colleague
//! says *"the FPT thing has been sitting for three weeks"*. They do not
//! silently go and chase FPT, and they do not close the thread on your behalf
//! because it looked dead to them.
//!
//! # Why it is arithmetic and not a model call
//!
//! Fourth time, same reason, and by now it is a measured position rather than a
//! preference: `skill::repeated_chain`, `correction` and `tempo` all beat asking
//! the model, and `docs/adr-rag-vs-agentic-2026-09-03.md` measured what asking a
//! local model buys. Beyond cost there is a harder argument here — if something
//! can only be noticed by asking a model, it is not certain enough to be worth
//! making somebody look up from their work for.
//!
//! # Why it is capped, and why each thing is said once
//!
//! An assistant that becomes "proactive" by talking more is one you switch off
//! within a week, and the switch now exists. So: at most `MOST_AT_ONCE` per
//! sweep, worst first, and every notice carries a stable key that the delivery
//! record recognises. Silence is the normal output.

use crate::syn::memory::Memory;
use crate::syn::run::Run;
use crate::syn::skill::Skill;
use crate::syn::thread::{State, Thread};

/// How long a piece of work sits before its stillness is itself a fact.
///
/// Three weeks. Not a week — plenty of real work is untouched for a week and
/// nothing is wrong. Not two months, by which point the person has either
/// forgotten it entirely or made their peace with it, and being told is
/// archaeology rather than news.
pub const STUCK_AFTER_DAYS: i64 = 21;

/// How many notices one sweep may produce.
///
/// Three. The cap is the feature: a sweep that finds eleven stuck threads and
/// announces all eleven has told the user nothing they can act on and has
/// taught them to ignore the next one.
pub const MOST_AT_ONCE: usize = 3;

/// How far back to look when asking what has already been said.
///
/// **Not** the reminder loop's own `CATCH_UP_DAYS`, and the difference is the
/// bug this constant exists to have avoided. That window is one day, because a
/// reminder missed while the machine was asleep should still arrive; reusing it
/// here would have made a thread stuck for three weeks announce itself afresh
/// every couple of days — the exact nagging the cap and the dateless key are
/// arranged to prevent.
///
/// Thirty days, which is as long as the delivery record survives. A thread that
/// is *still* dead a month later earns one more mention, and that is the right
/// cadence for a fact that has not changed.
pub const SAID_FOR_DAYS: i64 = 30;

/// How many uses a skill needs before its failures mean anything.
///
/// Four. Below that, "failed twice out of three" is one bad afternoon, and
/// calling it a degrading skill would send somebody to rewrite a procedure that
/// was fine.
pub const MIN_SKILL_USES: usize = 4;

/// Refuses to compile when a kind is added and `Kind::ALL` is not updated.
#[allow(dead_code)]
fn _every_kind_is_listed(kind: Kind) {
    match kind {
        Kind::StuckThread | Kind::ContradictedMemory | Kind::DegradingSkill => {}
    }
}

/// What sort of thing was noticed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    StuckThread,
    ContradictedMemory,
    DegradingSkill,
}

impl Kind {
    pub const ALL: [Kind; 3] = [
        Kind::StuckThread,
        Kind::ContradictedMemory,
        Kind::DegradingSkill,
    ];

    /// The `subtype` on the message, which the card switches its icon on.
    pub fn subtype(self) -> &'static str {
        match self {
            Kind::StuckThread => "syn_stuck_thread",
            Kind::ContradictedMemory => "syn_contradiction",
            Kind::DegradingSkill => "syn_degrading_skill",
        }
    }

    /// Which gets said first when more than `MOST_AT_ONCE` are found.
    ///
    /// Stuck work first, because it is the only one about the user's own work
    /// rather than about Syn's insides — a dead thread costs them something,
    /// and a confused memory or a flaky skill costs Syn's answers accuracy they
    /// will notice by other means. A contradiction outranks a skill for the
    /// same reason one step down: a wrong memory shapes every answer, and a bad
    /// skill only shapes the ones that reach for it.
    fn weight(self) -> u8 {
        match self {
            Kind::StuckThread => 3,
            Kind::ContradictedMemory => 2,
            Kind::DegradingSkill => 1,
        }
    }
}

/// One thing worth saying, and nothing done about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: Kind,
    /// What makes this the same notice next time round.
    ///
    /// Deliberately carries no date. A stuck thread keyed by how many days it
    /// has been stuck would be a new notice every single day, which is the
    /// nagging this module is arranged to avoid. Said once; the delivery record
    /// ages out after thirty days, so a thread that is *still* dead a month
    /// later earns one more mention and no more.
    pub key: String,
    pub title: String,
    pub text: String,
    /// The node to open.
    ///
    /// Was `None` for memories and skills, because neither type had a route —
    /// so half of what this module found arrived with nowhere to go and look,
    /// which makes a notice a complaint rather than something you can act on.
    /// `syn_memory` and `syn_skill` are routed now and both carry a link.
    ///
    /// Still `Option`, because a notice about something with no node behind it
    /// is a shape this should be able to express without inventing an id.
    pub target: Option<String>,
    pub target_type: Option<&'static str>,
}

/// Days between an RFC3339 timestamp and now, or `None` if it will not parse.
///
/// `None` rather than zero on a bad timestamp: a file somebody hand-edited into
/// an unparseable date should drop out of the sweep, not be announced as
/// infinitely stale.
fn days_since(when: &str, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    let then = chrono::DateTime::parse_from_rfc3339(when).ok()?;
    Some((now - then.with_timezone(&chrono::Utc)).num_days())
}

/// Work that has stopped moving.
///
/// Only `world` and `mine`, and the omission is the decision:
///
/// * **`world`** — waiting on somebody outside. Nobody is chasing it, and that
///   is the case the user genuinely cannot see: nothing on their screen counts
///   the days since they last heard back.
/// * **`mine`** — Syn's own move, dropped. The most embarrassing one to leave
///   unsaid, and the user has no way at all to know Syn was holding something.
/// * **`yours` is deliberately excluded.** Somebody knows what they owe.
///   Telling them again is not noticing, it is nagging, and it is the exact
///   failure that makes a person switch this off.
/// * `resting` is parked on purpose and `closed` is finished.
pub fn stuck_threads(threads: &[Thread], now: chrono::DateTime<chrono::Utc>) -> Vec<Notice> {
    threads
        .iter()
        .filter(|t| matches!(t.state, State::World | State::Mine))
        .filter_map(|t| {
            let days = days_since(&t.last_moved, now)?;
            if days < STUCK_AFTER_DAYS {
                return None;
            }
            let weeks = days / 7;
            let whose = match t.state {
                State::Mine => "It is still my move.",
                _ => "It is still waiting on somebody outside.",
            };
            let waiting = t
                .waiting_for
                .as_deref()
                .map(|w| format!(" Waiting for: {w}."))
                .unwrap_or_default();

            Some(Notice {
                kind: Kind::StuckThread,
                key: format!("syn-notice:stuck:{}", t.id),
                title: format!("Nothing has moved on \"{}\" for {weeks} weeks", t.title),
                text: format!(
                    "{whose}{waiting} I have not done anything about it — you may want to \
                     chase it, or close it."
                ),
                target: Some(t.id.clone()),
                target_type: Some(crate::syn::thread::THREAD_TYPE),
            })
        })
        .collect()
}

/// Memories that make a claim about the same thing and disagree.
///
/// Same `kind` and same `subject` — which is as far as two memories can be
/// compared without reading them, and the same rule `memory::conflicting` uses
/// at write time. The difference is when: that one asks about a collision as it
/// happens, and this sweeps a set that has been accumulating for months.
///
/// A pair where one `supersedes` the other is settled and is not reported. That
/// is the whole point of keeping the superseded one rather than overwriting it,
/// and a sweep that ignored it would announce every deliberate correction the
/// user ever made as a contradiction.
pub fn contradicted_memories(memories: &[Memory]) -> Vec<Notice> {
    use std::collections::HashMap;

    let mut groups: HashMap<(String, String), Vec<&Memory>> = HashMap::new();
    for memory in memories {
        // Only memories about a named subject. A `kind` with no subject is a
        // general note, and two general notes of the same kind disagreeing is
        // not something this can tell from the outside.
        let Some(subject) = memory.subject.as_deref().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        groups
            .entry((memory.kind.to_lowercase(), subject.to_lowercase()))
            .or_default()
            .push(memory);
    }

    let mut out: Vec<Notice> = groups
        .into_iter()
        .filter_map(|((kind, _), group)| {
            if group.len() < 2 {
                return None;
            }
            // Settled: something in the group replaces something else in it.
            let ids: std::collections::HashSet<&str> =
                group.iter().map(|m| m.id.as_str()).collect();
            if group
                .iter()
                .any(|m| m.supersedes.as_deref().is_some_and(|s| ids.contains(s)))
            {
                return None;
            }

            // Sorted so the key and the sentence are the same every sweep,
            // whatever order the index handed them over in.
            let mut sorted: Vec<&Memory> = group.clone();
            sorted.sort_by(|a, b| a.id.cmp(&b.id));
            let subject = sorted[0].subject.clone().unwrap_or_default();
            let said: Vec<String> = sorted
                .iter()
                .take(3)
                .map(|m| format!("· {}", m.body.trim().lines().next().unwrap_or("").trim()))
                .collect();

            Some(Notice {
                kind: Kind::ContradictedMemory,
                key: format!(
                    "syn-notice:contradiction:{}",
                    sorted.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join("|")
                ),
                title: format!("I am holding {} {kind}s about {subject}", sorted.len()),
                text: format!(
                    "{}\n\nThey may not all be true. I have not deleted or merged anything — \
                     which one is right is yours to say.",
                    said.join("\n")
                ),
                // The first of the group, because the reader has to land
                // somewhere and the list they land in holds the rest.
                target: Some(sorted[0].id.clone()),
                target_type: Some(crate::syn::memory::MEMORY_TYPE),
            })
        })
        .collect();

    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// Skills whose runs keep going wrong.
///
/// A run counts as a use when `load_skill` returned that skill's body, and as a
/// failure when any step in that run came back `ok: false`. That is the same
/// definition `skill::needs_revision` uses for one run; what is new is counting
/// it across the runs still on disk.
///
/// "Still on disk" and not "ever": runs are pruned at `run::KEEP_RUNS`, so this
/// is a recent record, and the sentence says *recently* rather than implying a
/// lifetime tally it cannot support.
/// `skills` is only for turning a name into a node id. The tally is read off
/// the runs; a skill the user has since deleted still counts against itself in
/// the count and simply arrives without a link, which is honest — the failures
/// happened.
pub fn degrading_skills(runs: &[Run], skills: &[Skill]) -> Vec<Notice> {
    use std::collections::HashMap;

    let mut tally: HashMap<String, (String, usize, usize)> = HashMap::new();

    for run in runs {
        let loaded: std::collections::HashSet<String> = run
            .steps
            .iter()
            .filter(|s| s.tool.as_deref() == Some(crate::syn::skill::LOAD_TOOL) && s.ok == Some(true))
            .filter_map(|s| {
                s.args
                    .as_ref()
                    .and_then(|a| a.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
            })
            .collect();

        if loaded.is_empty() {
            continue;
        }
        let went_wrong = run.steps.iter().any(|s| s.ok == Some(false));

        for name in loaded {
            let entry = tally
                .entry(name.to_lowercase())
                .or_insert_with(|| (name.clone(), 0, 0));
            entry.1 += 1;
            if went_wrong {
                entry.2 += 1;
            }
        }
    }

    let mut out: Vec<Notice> = tally
        .into_values()
        .filter(|(_, uses, failures)| {
            // Half or worse, over enough uses to mean something.
            *uses >= MIN_SKILL_USES && failures * 2 >= *uses
        })
        .map(|(name, uses, failures)| Notice {
            kind: Kind::DegradingSkill,
            key: format!("syn-notice:skill:{}", name.to_lowercase()),
            title: format!("`{name}` went wrong {failures} of the last {uses} times"),
            text: format!(
                "Something failed in {failures} of the {uses} runs that used this skill \
                 recently. I have not changed or disabled it — it is your procedure."
            ),
            target: skills
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(&name))
                .map(|s| s.id.clone()),
            target_type: Some(crate::syn::skill::SKILL_TYPE),
        })
        .collect();

    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// Everything worth saying, worst first, capped, minus whatever has been said.
///
/// `already_said` is the delivery record — the same table the calendar's
/// reminders use, because "things already announced" is what it holds and a
/// second table of the same shape is a second thing to keep pruned.
pub fn sweep(
    threads: &[Thread],
    memories: &[Memory],
    runs: &[Run],
    skills: &[Skill],
    now: chrono::DateTime<chrono::Utc>,
    already_said: &std::collections::HashSet<String>,
) -> Vec<Notice> {
    let mut found: Vec<Notice> = stuck_threads(threads, now)
        .into_iter()
        .chain(contradicted_memories(memories))
        .chain(degrading_skills(runs, skills))
        .filter(|n| !already_said.contains(&n.key))
        .collect();

    // Worst kind first; within a kind, the key, so a sweep that finds four
    // stuck threads reports the same three every time rather than a rotating
    // sample.
    found.sort_by(|a, b| {
        b.kind
            .weight()
            .cmp(&a.kind.weight())
            .then_with(|| a.key.cmp(&b.key))
    });
    found.truncate(MOST_AT_ONCE);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::syn::SynSettings;
    use crate::syn::registry::Reversal;
    use crate::syn::run::{Budget, Run};

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-09-06T09:00:00Z")
            .expect("valid")
            .with_timezone(&chrono::Utc)
    }

    fn days_ago(n: i64) -> String {
        (now() - chrono::Duration::days(n)).to_rfc3339()
    }

    fn thread(title: &str, state: State, moved: String) -> Thread {
        Thread {
            id: format!("SynThreads/{title}.md"),
            title: title.to_string(),
            body: String::new(),
            state,
            waiting_for: None,
            opened: days_ago(90),
            last_moved: moved,
        }
    }

    fn memory(id: &str, kind: &str, subject: &str, body: &str) -> Memory {
        Memory {
            id: id.to_string(),
            title: body.to_string(),
            body: body.to_string(),
            kind: kind.to_string(),
            subject: Some(subject.to_string()),
            confidence: 0.8,
            source_run: None,
            source_nodes: Vec::new(),
            first_seen: days_ago(30),
            last_confirmed: days_ago(3),
            review_after: None,
            pinned: false,
            supersedes: None,
        }
    }

    /// A run that opened a skill, and either went well or did not.
    fn run_using(skill: &str, went_wrong: bool) -> Run {
        let mut run = Run::new("hỏi", None, Budget::from_settings(&SynSettings::default()));
        run.record_tool(
            1,
            crate::syn::skill::LOAD_TOOL,
            serde_json::json!({ "name": skill }),
            true,
            Reversal::Nothing,
            "## body",
            2,
        );
        if went_wrong {
            run.record_tool(
                2,
                "query_nodes",
                serde_json::json!({}),
                false,
                Reversal::Nothing,
                "no such type",
                2,
            );
        }
        run
    }

    fn skill(name: &str) -> Skill {
        Skill {
            id: format!("SynSkills/{name}.md"),
            title: name.to_string(),
            name: name.to_string(),
            description: String::new(),
            when_to_use: String::new(),
            tier: crate::syn::skill::Tier::Prose,
            tools: Vec::new(),
            version: 1,
            author: "user".to_string(),
            enabled: true,
            source_run: None,
            trial_at: None,
            pending_revision: None,
            revision_because: None,
            body: String::new(),
        }
    }

    fn nothing_said() -> std::collections::HashSet<String> {
        std::collections::HashSet::new()
    }

    // ── stuck work ────────────────────────────────────────────────

    #[test]
    fn work_waiting_on_the_outside_world_for_three_weeks_is_a_fact() {
        let threads = [thread("Bảng giá Q4", State::World, days_ago(24))];
        let found = stuck_threads(&threads, now());
        assert_eq!(found.len(), 1);
        assert!(found[0].title.contains("Bảng giá Q4"), "{}", found[0].title);
        assert!(found[0].title.contains('3'), "it says how long: {}", found[0].title);
    }

    /// The one the user has no way at all of seeing.
    #[test]
    fn a_move_syn_owed_and_dropped_is_also_a_fact() {
        let threads = [thread("Đọc lại hợp đồng", State::Mine, days_ago(30))];
        let found = stuck_threads(&threads, now());
        assert_eq!(found.len(), 1);
        assert!(found[0].text.contains("still my move"), "{}", found[0].text);
    }

    /// Somebody knows what they owe. Telling them is nagging, and nagging is
    /// how this feature gets switched off.
    #[test]
    fn what_the_user_owes_is_never_mentioned() {
        let threads = [thread("Trả lời VPB", State::Yours, days_ago(60))];
        assert!(stuck_threads(&threads, now()).is_empty());
    }

    #[test]
    fn parked_and_finished_work_is_not_stuck() {
        let threads = [
            thread("Nghỉ đã", State::Resting, days_ago(60)),
            thread("Xong rồi", State::Closed, days_ago(60)),
        ];
        assert!(stuck_threads(&threads, now()).is_empty());
    }

    #[test]
    fn a_fortnight_is_not_yet_stuck() {
        let threads = [thread("Mới thôi", State::World, days_ago(14))];
        assert!(stuck_threads(&threads, now()).is_empty());
    }

    /// A hand-edited file with a broken date drops out rather than being
    /// announced as infinitely old.
    #[test]
    fn an_unreadable_date_is_not_announced() {
        let threads = [thread("Hỏng ngày", State::World, "not a date".to_string())];
        assert!(stuck_threads(&threads, now()).is_empty());
    }

    /// The key carries no date, so the same dead thread is one notice and not
    /// one per day.
    #[test]
    fn the_same_stuck_thread_is_the_same_notice_tomorrow() {
        let threads = [thread("Bảng giá Q4", State::World, days_ago(24))];
        let today = stuck_threads(&threads, now());
        let tomorrow = stuck_threads(&threads, now() + chrono::Duration::days(1));
        assert_eq!(today[0].key, tomorrow[0].key);
    }

    // ── memories that disagree ────────────────────────────────────

    #[test]
    fn two_claims_about_the_same_thing_are_worth_saying() {
        let memories = [
            memory("SynMemory/a.md", "preference", "cà phê", "Thích cà phê sữa"),
            memory("SynMemory/b.md", "preference", "Cà Phê", "Bỏ cà phê rồi"),
        ];
        let found = contradicted_memories(&memories);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].text.contains("Thích cà phê sữa"), "{}", found[0].text);
        assert!(found[0].text.contains("Bỏ cà phê rồi"), "{}", found[0].text);
    }

    /// The reason superseded memories are kept rather than overwritten. A sweep
    /// that ignored `supersedes` would report every deliberate correction the
    /// user ever made as a contradiction.
    #[test]
    fn a_correction_the_user_already_made_is_settled() {
        let mut newer = memory("SynMemory/b.md", "preference", "cà phê", "Bỏ cà phê rồi");
        newer.supersedes = Some("SynMemory/a.md".to_string());
        let memories = [
            memory("SynMemory/a.md", "preference", "cà phê", "Thích cà phê sữa"),
            newer,
        ];
        assert!(contradicted_memories(&memories).is_empty());
    }

    #[test]
    fn different_subjects_do_not_disagree() {
        let memories = [
            memory("SynMemory/a.md", "preference", "cà phê", "Thích cà phê sữa"),
            memory("SynMemory/b.md", "preference", "trà", "Ghét trà"),
        ];
        assert!(contradicted_memories(&memories).is_empty());
    }

    /// Two general notes of the same kind are not comparable from the outside.
    #[test]
    fn memories_about_nothing_in_particular_are_left_alone() {
        let mut a = memory("SynMemory/a.md", "instruction", "x", "Trả lời ngắn");
        let mut b = memory("SynMemory/b.md", "instruction", "x", "Trả lời dài");
        a.subject = None;
        b.subject = None;
        assert!(contradicted_memories(&[a, b]).is_empty());
    }

    // ── skills going wrong ────────────────────────────────────────

    #[test]
    fn a_skill_failing_half_the_time_is_worth_saying() {
        let runs: Vec<Run> = (0..6)
            .map(|i| run_using("weekly-review", i % 2 == 0))
            .collect();
        let found = degrading_skills(&runs, &[]);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(found[0].title.contains("weekly-review"), "{}", found[0].title);
        assert!(found[0].title.contains("3 of the last 6"), "{}", found[0].title);
    }

    /// One bad afternoon is not a degrading procedure.
    #[test]
    fn too_few_uses_to_mean_anything_says_nothing() {
        let runs: Vec<Run> = (0..3).map(|_| run_using("weekly-review", true)).collect();
        assert!(degrading_skills(&runs, &[]).is_empty());
    }

    #[test]
    fn a_skill_that_mostly_works_is_left_alone() {
        let runs: Vec<Run> = (0..8).map(|i| run_using("weekly-review", i == 0)).collect();
        assert!(degrading_skills(&runs, &[]).is_empty());
    }

    /// Failures in runs that never opened the skill are not its failures.
    #[test]
    fn a_run_that_never_opened_the_skill_is_not_counted_against_it() {
        let mut runs: Vec<Run> = (0..4).map(|_| run_using("weekly-review", false)).collect();
        for _ in 0..8 {
            let mut bare = Run::new("hỏi", None, Budget::from_settings(&SynSettings::default()));
            bare.record_tool(
                1,
                "query_nodes",
                serde_json::json!({}),
                false,
                Reversal::Nothing,
                "boom",
                1,
            );
            runs.push(bare);
        }
        assert!(degrading_skills(&runs, &[]).is_empty());
    }

    // ── the sweep itself ──────────────────────────────────────────

    /// An assistant that becomes "proactive" by talking more is one you switch
    /// off. Silence is the normal output, and the cap is the feature.
    #[test]
    fn eleven_dead_threads_are_still_only_three_sentences() {
        let threads: Vec<Thread> = (0..11)
            .map(|i| thread(&format!("việc {i}"), State::World, days_ago(40)))
            .collect();
        let found = sweep(&threads, &[], &[], &[], now(), &nothing_said());
        assert_eq!(found.len(), MOST_AT_ONCE);
    }

    /// And the same three, not a rotating sample — otherwise the cap would just
    /// spread eleven notices over four days.
    #[test]
    fn the_cap_keeps_the_same_three_rather_than_rotating() {
        let threads: Vec<Thread> = (0..11)
            .map(|i| thread(&format!("việc {i}"), State::World, days_ago(40)))
            .collect();
        let first = sweep(&threads, &[], &[], &[], now(), &nothing_said());
        let again = sweep(&threads, &[], &[], &[], now(), &nothing_said());
        assert_eq!(
            first.iter().map(|n| &n.key).collect::<Vec<_>>(),
            again.iter().map(|n| &n.key).collect::<Vec<_>>()
        );
    }

    /// The user's own work outranks Syn's insides.
    #[test]
    fn stuck_work_is_said_before_syns_own_housekeeping() {
        let threads = [thread("Bảng giá Q4", State::World, days_ago(40))];
        let memories = [
            memory("SynMemory/a.md", "preference", "cà phê", "Thích"),
            memory("SynMemory/b.md", "preference", "cà phê", "Ghét"),
        ];
        let runs: Vec<Run> = (0..6).map(|_| run_using("weekly-review", true)).collect();

        let found = sweep(&threads, &memories, &runs, &[], now(), &nothing_said());
        assert_eq!(
            found.iter().map(|n| n.kind).collect::<Vec<_>>(),
            vec![Kind::StuckThread, Kind::ContradictedMemory, Kind::DegradingSkill]
        );
    }

    #[test]
    fn what_has_already_been_said_is_not_said_again() {
        let threads = [thread("Bảng giá Q4", State::World, days_ago(40))];
        let said: std::collections::HashSet<String> =
            stuck_threads(&threads, now()).into_iter().map(|n| n.key).collect();
        assert!(sweep(&threads, &[], &[], &[], now(), &said).is_empty());
    }

    #[test]
    fn a_quiet_vault_produces_nothing() {
        assert!(sweep(&[], &[], &[], &[], now(), &nothing_said()).is_empty());
    }

    /// The constraint the whole nhát is built on, as a test: a notice is a
    /// sentence and a pointer, and there is nowhere in this struct to put an
    /// action.
    #[test]
    fn a_notice_can_only_say_something() {
        let threads = [thread("Bảng giá Q4", State::World, days_ago(40))];
        let found = sweep(&threads, &[], &[], &[], now(), &nothing_said());
        let notice = &found[0];

        assert!(
            notice.text.contains("I have not done anything about it"),
            "it says it did nothing:\n{}",
            notice.text
        );
        // And the pointer is somewhere the app can actually go.
        assert_eq!(notice.target_type, Some("syn_thread"));
        assert!(notice.target.is_some());
    }

    /// Every notice now lands somewhere. This was the half of noticing that
    /// pointed at nothing: `syn_memory` and `syn_skill` had no route, so the
    /// card saying *"I am holding two contradictory things about you"* arrived
    /// with no way to go and look at either of them.
    #[test]
    fn every_notice_can_be_followed_to_the_thing_it_is_about() {
        let threads = [thread("Bảng giá Q4", State::World, days_ago(40))];
        let memories = [
            memory("SynMemory/a.md", "preference", "cà phê", "Thích"),
            memory("SynMemory/b.md", "preference", "cà phê", "Ghét"),
        ];
        let runs: Vec<Run> = (0..6).map(|_| run_using("weekly-review", true)).collect();
        let skills = [skill("weekly-review")];

        let found = sweep(&threads, &memories, &runs, &skills, now(), &nothing_said());
        assert_eq!(found.len(), 3);
        for notice in &found {
            assert!(notice.target.is_some(), "nowhere to go:\n{notice:#?}");
            assert!(notice.target_type.is_some(), "{notice:#?}");
        }

        // And the types are the ones the frontend has routes for.
        let types: Vec<&str> = found.iter().filter_map(|n| n.target_type).collect();
        assert_eq!(types, vec!["syn_thread", "syn_memory", "syn_skill"]);
    }

    /// A skill deleted since the runs that used it still counts against itself
    /// — the failures happened — and simply arrives without a link.
    #[test]
    fn a_skill_that_is_gone_is_still_counted_and_simply_has_no_link() {
        let runs: Vec<Run> = (0..6).map(|_| run_using("weekly-review", true)).collect();
        let found = degrading_skills(&runs, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target, None);
    }

    /// A notice must not consider itself said for longer than the record that
    /// remembers it. Past that the row is pruned and the same sentence arrives
    /// again — a month later, when nobody would connect the two.
    #[test]
    fn nothing_is_remembered_for_longer_than_the_record_lasts() {
        assert!(SAID_FOR_DAYS <= crate::db::reminders::KEEP_DAYS);
    }

    /// Every type a notice points at has a route on the other side.
    ///
    /// This is the guard for the failure that made half of noticing useless:
    /// the notice carried a `target_type`, `routeForNode` returned `null` for
    /// it, and `handleNotificationAction` logged a warning and did nothing. No
    /// crash, no red anything — just a button that quietly did not work.
    #[test]
    fn the_frontend_can_open_everything_a_notice_points_at() {
        let source = include_str!("../../../src/shared/nodeRoutes.ts");
        let block = source
            .split("const ROUTE_FOR_NODE_TYPE: Readonly<Record<string, string>> = {")
            .nth(1)
            .expect("the route map is declared")
            .split("};")
            .next()
            .expect("the declaration closes");

        for node_type in [
            crate::syn::thread::THREAD_TYPE,
            crate::syn::memory::MEMORY_TYPE,
            crate::syn::skill::SKILL_TYPE,
        ] {
            assert!(
                block.lines().any(|l| l.trim().starts_with(&format!("{node_type}:"))),
                "`{node_type}` has no route, so a notice about one lands nowhere"
            );
        }
    }

    /// The card has to know every subtype this can produce.
    ///
    /// A kind added here without a branch there renders as a generic speech
    /// bubble — which is not a crash, and is therefore exactly the sort of
    /// thing nobody notices for months. Read out of the component rather than
    /// listed twice.
    #[test]
    fn the_card_draws_every_kind_this_can_produce() {
        let source = include_str!("../../../src/mini-apps/messages/components/NotificationCard.vue");
        for kind in Kind::ALL {
            assert!(
                source.contains(kind.subtype()),
                "NotificationCard.vue has no branch for `{}`",
                kind.subtype()
            );
        }
        assert!(
            source.contains("startsWith('syn_')"),
            "and it tells a notice apart from a reminder by the prefix"
        );
    }

    /// The subtypes are what the card switches its icon on, and they are
    /// distinct from the calendar's.
    #[test]
    fn every_kind_has_its_own_subtype() {
        let subtypes: std::collections::HashSet<&str> =
            Kind::ALL.iter().map(|k| k.subtype()).collect();
        assert_eq!(subtypes.len(), Kind::ALL.len());
        for subtype in subtypes {
            assert!(subtype.starts_with("syn_"), "{subtype} could collide with a reminder");
        }
    }
}
