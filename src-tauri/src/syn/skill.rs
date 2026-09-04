//! What Syn knows how to do, as opposed to what it knows.
//!
//! # Why a skill is a node
//!
//! The same argument as `memory.rs`, and it is worth repeating because it is
//! the argument this whole app rests on: a skill is a Markdown file with
//! frontmatter, filed in the user's vault, full-text indexed, versioned in the
//! CRDT log, recoverable from the trash, synced end-to-end encrypted, and
//! openable in any editor.
//!
//! For memory that mattered because a claim about somebody they cannot read is
//! not something they can disagree with. For skills it matters more. A skill
//! changes what the assistant *does*, and `version` plus the existing
//! `list_versions` and `restore_version` mean a change of behaviour is a diff
//! the user can read and undo. An agent whose behaviour lives in a store only
//! it can read is an agent that can change its mind without telling anyone.
//!
//! # Progressive disclosure, and the reason to distrust it
//!
//! The design is an index in the prompt — name, description, when to use it —
//! and a `load_skill` tool the model calls to fetch a body. Bodies are too
//! large to inline forty of them, so unlike memory there is no option to send
//! everything.
//!
//! That is exactly the shape that just failed. `docs/adr-memory-shape-2026-09-04.md`
//! records `recall` being called zero times across fifteen real runs, so the
//! only memory Syn had ever written never once reached it. The index here can
//! fail the same way, quietly, and no gate in the roadmap tests for it: all
//! three ask about cost and quality, none asks whether a skill ever fires.
//!
//! So `skill_was_loaded` exists, and the first thing to measure about this
//! feature is not whether skills help but whether they are opened at all.

use serde::{Deserialize, Serialize};

use crate::db::DbBridge;
use crate::error::AppResult;
use crate::models::node::NodeMetadata;

/// The `type:` a skill carries.
///
/// Prefixed, for the reason the user gave when `memory` was named: `skill` is
/// an ordinary word and this is their vault. Somebody tracking skills they are
/// learning — a language, an instrument, a climbing grade — has every right to
/// a kind called `skill`, and would collide with this one in both the type and
/// the folder. The roadmap writes `type: skill`; this follows the naming rule
/// the user set during P2 instead, which is the later and better decision.
pub const SKILL_TYPE: &str = "syn_skill";

/// Where skills are filed.
///
/// Top-level, not under `Syn/`. `is_in_unscanned_dir` skips anything named
/// `Syn`, so a skill written there would be indexed by the write that created
/// it and dropped by the next full scan — on disk, correct, and unreachable.
/// That trap cost an afternoon once already.
pub const SKILL_FOLDER: &str = "SynSkills";

/// How much of the index may ride in *every* prompt.
///
/// The roadmap budgets roughly 25 tokens a skill and forty skills at about a
/// thousand tokens. This is that, in characters, with room for the heading.
/// Past it the least recently used go first and the prompt says how many were
/// left out, because a skill silently absent from the index is a skill the
/// model cannot know exists.
pub const INDEX_BUDGET_CHARS: usize = 4_200;

/// How many skill bodies one run may open.
///
/// Two. A body is a procedure, not a sentence, and three of them crowd out the
/// vault context that tells the model what it is working on. The cap is also
/// what keeps a model that has decided skills are interesting from reading the
/// whole library instead of doing the task.
pub const BODIES_PER_RUN: usize = 2;

/// The tool that opens a body. Named once, because the engine enforces the
/// per-run ceiling by matching on it and a literal in two places is a rename
/// waiting to go half-done.
pub const LOAD_TOOL: &str = "load_skill";

/// Where a skill runs, and how much it is trusted to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Instructions loaded into the prompt. The model does the work.
    Prose,
    /// A declared sequence of tool calls, run by Rust. Deterministic, testable,
    /// and costs no inference — the tier other frameworks skip and the one most
    /// real skills of a productivity app actually want.
    Recipe,
    /// Sandboxed JS or WASM. Desktop only, opt-in, and not built yet.
    Code,
}

impl Tier {
    /// Anything unrecognised is `prose`.
    ///
    /// A skill the user hand-wrote with a typo in `tier:` should behave like
    /// instructions, which is the tier that cannot do anything on its own —
    /// failing open towards *less* capability, never more.
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "recipe" => Tier::Recipe,
            "code" => Tier::Code,
            _ => Tier::Prose,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Prose => "prose",
            Tier::Recipe => "recipe",
            Tier::Code => "code",
        }
    }
}

/// One thing Syn knows how to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Vault-relative path, which is also how every node tool addresses it.
    pub id: String,
    pub title: String,
    /// The handle `load_skill` takes. Unique within a vault by convention, and
    /// checked when one is written.
    pub name: String,
    pub description: String,
    /// When the model should reach for it. The index's whole job is to make
    /// this readable in one line.
    pub when_to_use: String,
    pub tier: Tier,
    /// The tools this skill expects to use, for the user to read before
    /// enabling it. Not enforcement — enforcement is P4's business.
    pub tools: Vec<String>,
    pub version: u32,
    /// `user` or `syn`. A skill the assistant proposed is read differently from
    /// one the user wrote, and the screen says which.
    pub author: String,
    /// Disabled skills do not appear in the index at all. Anything Syn proposes
    /// starts here, and only a person moves it.
    pub enabled: bool,
    /// The steps, in Markdown. What `load_skill` returns.
    pub body: String,
}

impl Skill {
    pub fn from_node(node: &NodeMetadata) -> Self {
        let props = &node.properties;
        let text = |key: &str| {
            props
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or_default()
                .to_string()
        };

        // The name falls back to the title, and the title to the filename, so a
        // skill somebody wrote by hand without frontmatter is still loadable
        // rather than invisible.
        let name = {
            let declared = text("name");
            if declared.is_empty() {
                node.title.trim().to_string()
            } else {
                declared
            }
        };

        Skill {
            id: node.id.clone(),
            title: node.title.clone(),
            name,
            description: text("description"),
            when_to_use: text("when_to_use"),
            tier: Tier::parse(&text("tier")),
            tools: props
                .get("tools")
                .and_then(|v| v.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            version: props
                .get("version")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .max(1) as u32,
            author: {
                let declared = text("author").to_lowercase();
                if declared == "syn" {
                    "syn".to_string()
                } else {
                    "user".to_string()
                }
            },
            // Absent means off. A skill that arrived by sync, or was written by
            // a hand that forgot the field, should not start changing behaviour
            // because nobody said not to.
            enabled: props
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            body: node.content.clone(),
        }
    }

    /// The one line this skill occupies in the prompt's index.
    pub fn line(&self) -> String {
        let when = if self.when_to_use.is_empty() {
            String::new()
        } else {
            format!(" — use when: {}", self.when_to_use)
        };
        let what = if self.description.is_empty() {
            String::new()
        } else {
            format!(": {}", self.description)
        };
        format!("- {}{what}{when}", self.name)
    }
}

/// Every skill in the vault, newest name order irrelevant — sorted by name so
/// the index is stable between prompts and a snapshot of it means something.
pub fn all(db: &DbBridge) -> AppResult<Vec<Skill>> {
    let mut skills: Vec<Skill> = db
        .get_nodes_by_type(SKILL_TYPE)?
        .iter()
        .map(Skill::from_node)
        .collect();
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(skills)
}

/// Find one by the handle `load_skill` was given.
///
/// Unicode case folding, because this app's users write Vietnamese and
/// `eq_ignore_ascii_case` cannot fold `Ọ` — a mistake this codebase has now
/// made in three separate places.
pub fn find<'a>(skills: &'a [Skill], name: &str) -> Option<&'a Skill> {
    let wanted = name.trim().to_lowercase();
    skills.iter().find(|s| s.name.trim().to_lowercase() == wanted)
}

/// The table that rides in every prompt, or `None` when nothing is enabled.
///
/// `None` rather than an empty string, for the reason `memory_block` returns
/// `None`: a vault with no skills sends byte for byte the prompt it sent before
/// this module existed, and the snapshot tests assert it.
///
/// Only enabled skills appear. A disabled one is not merely unusable — the
/// model is not told it exists, because a name in the index is an invitation.
pub fn index_block(skills: &[Skill], budget_chars: usize) -> Option<String> {
    let enabled: Vec<&Skill> = skills.iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return None;
    }

    let header = "\n\n=== WHAT YOU KNOW HOW TO DO ===\n\
         Procedures written down for you, by this person or by you and approved \
         by them. This is the whole list — there is nothing else.\n\
         - The line is a summary. Call `load_skill` with the name to read the \
         steps before following them.\n\
         - Do not guess at the steps from the description. If it is worth doing \
         by a skill, it is worth reading first.\n";
    let footer = "=== END ===";

    let mut used = header.chars().count() + footer.chars().count();
    let mut lines = Vec::new();
    let mut dropped = 0usize;

    for skill in &enabled {
        let line = skill.line();
        let cost = line.chars().count() + 1;
        if used + cost > budget_chars {
            dropped += 1;
            continue;
        }
        used += cost;
        lines.push(line);
    }

    if lines.is_empty() {
        log::warn!(
            "[Syn] {} enabled skills, none of which fit a {} character index budget",
            enabled.len(),
            budget_chars
        );
        return None;
    }

    let mut block = String::from(header);
    block.push_str(&lines.join("\n"));
    block.push('\n');
    if dropped > 0 {
        block.push_str(&format!(
            "({dropped} more skills did not fit in this list and were left out.)\n"
        ));
    }
    block.push_str(footer);
    Some(block)
}

/// The frontmatter a new skill is written with.
#[allow(clippy::too_many_arguments)]
pub fn frontmatter(
    name: &str,
    description: &str,
    when_to_use: &str,
    tier: Tier,
    tools: &[String],
    author: &str,
    enabled: bool,
    version: u32,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "description": description,
        "when_to_use": when_to_use,
        "tier": tier.as_str(),
        "tools": tools,
        "author": author,
        "enabled": enabled,
        "version": version.max(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(title: &str, body: &str, props: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: format!("{SKILL_FOLDER}/{title}.md"),
            node_type: SKILL_TYPE.to_string(),
            title: title.to_string(),
            content: body.to_string(),
            properties: props,
            created_at: "2026-09-01T00:00:00Z".into(),
            updated_at: "2026-09-01T00:00:00Z".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn skill(name: &str, enabled: bool) -> Skill {
        Skill::from_node(&node(
            name,
            "## Steps\n1. do the thing",
            serde_json::json!({
                "name": name,
                "description": "Tổng kết tuần từ task và calendar.",
                "when_to_use": "Khi người dùng nói \"tổng kết tuần\".",
                "tier": "prose",
                "enabled": enabled,
                "version": 1,
            }),
        ))
    }

    /// The folder has to be one the vault scan actually walks.
    ///
    /// `Syn/` reads better and is silently wrong: the scan skips it, so a file
    /// written there works for the rest of the session and vanishes from the
    /// index at the next full scan. Memory learned this the expensive way.
    #[test]
    fn skills_are_filed_somewhere_the_vault_scan_will_look() {
        assert!(
            !crate::commands::nodes::is_in_unscanned_dir(&format!("{SKILL_FOLDER}/a.md")),
            "`{SKILL_FOLDER}` must not be a directory the scan skips"
        );
    }

    /// The type is prefixed so the user keeps the plain word.
    #[test]
    fn the_unprefixed_word_is_left_for_the_user() {
        assert_ne!(SKILL_TYPE, "skill");
        assert!(SKILL_TYPE.starts_with("syn_"));
        assert_ne!(SKILL_FOLDER, "Skills");
    }

    /// Absent `enabled:` means off.
    ///
    /// The direction matters. A skill that arrives by sync from another device,
    /// or that somebody wrote by hand and left the field out of, must not start
    /// changing what the assistant does because nobody said not to.
    #[test]
    fn a_skill_that_does_not_say_is_off() {
        let quiet = Skill::from_node(&node("quiet", "body", serde_json::json!({})));
        assert!(!quiet.enabled);
        assert_eq!(quiet.name, "quiet", "the title stands in for a missing name");
        assert_eq!(quiet.tier, Tier::Prose, "and the least capable tier stands in");
        assert_eq!(quiet.author, "user");
    }

    /// An unreadable tier is prose, never something with more reach.
    #[test]
    fn an_unknown_tier_fails_towards_doing_less() {
        for raw in ["", "RECIPE ", "code", "réçipe", "wasm", "shell"] {
            let parsed = Tier::parse(raw);
            assert!(
                matches!(parsed, Tier::Prose | Tier::Recipe | Tier::Code),
                "`{raw}` parsed to something"
            );
        }
        assert_eq!(Tier::parse("wasm"), Tier::Prose, "unknown means prose");
        assert_eq!(Tier::parse("RECIPE "), Tier::Recipe, "case and space are noise");
    }

    /// A disabled skill is not in the index at all.
    ///
    /// Not merely unusable: unmentioned. A name in the index is an invitation,
    /// and the user turning something off is the one control they have over
    /// what the assistant will reach for.
    #[test]
    fn a_disabled_skill_is_not_offered() {
        let block = index_block(
            &[skill("weekly-review", true), skill("inbox-zero", false)],
            INDEX_BUDGET_CHARS,
        )
        .expect("one is enabled");

        assert!(block.contains("weekly-review"));
        assert!(
            !block.contains("inbox-zero"),
            "a disabled skill is not named to the model:\n{block}"
        );
    }

    #[test]
    fn nothing_enabled_means_no_block_rather_than_an_empty_one() {
        assert!(index_block(&[], INDEX_BUDGET_CHARS).is_none());
        assert!(index_block(&[skill("off", false)], INDEX_BUDGET_CHARS).is_none());
    }

    /// Over budget, the loss is declared.
    #[test]
    fn skills_that_do_not_fit_the_index_are_counted_out_loud() {
        let many: Vec<Skill> = (0..60).map(|i| skill(&format!("skill-{i:02}"), true)).collect();
        let block = index_block(&many, 1_200).expect("some fit");

        assert!(
            block.contains("did not fit in this list"),
            "a skill missing from the index is one the model cannot know exists:\n{block}"
        );
    }

    /// Finding by name folds case the way Vietnamese needs.
    #[test]
    fn a_name_is_found_whatever_case_it_was_asked_in() {
        let skills = [skill("Tổng-Kết-Tuần", true)];
        assert!(find(&skills, "tổng-kết-tuần").is_some());
        assert!(find(&skills, "  TỔNG-KẾT-TUẦN  ").is_some());
        assert!(find(&skills, "tong-ket-tuan").is_none(), "tones are not noise");
    }
}

/// `load_skill`, through the real tool dispatch and a real database.
///
/// The unit tests above prove the index and the parsing. These prove the door
/// the model actually reaches for, which is the part `memory.rs` learned was
/// worth testing end to end: `recall` was correct in isolation for weeks while
/// nothing in production ever opened it.
#[cfg(test)]
mod through_the_tools {
    use super::*;
    use crate::db::DbBridge;
    use tauri::Manager;

    struct Harness {
        _dir: tempfile::TempDir,
        vault: String,
        app: tauri::AppHandle<tauri::test::MockRuntime>,
    }

    fn harness() -> Harness {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = std::fs::canonicalize(dir.path())
            .expect("canonical vault")
            .to_str()
            .expect("utf8")
            .to_string();
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        let handle = app.handle().clone();
        handle.manage(crate::db::DbState::new(
            DbBridge::new_in_memory_full().expect("schema"),
        ));
        Harness { _dir: dir, vault, app: handle }
    }

    impl Harness {
        fn call(&self, tool: &str, args: serde_json::Value) -> serde_json::Value {
            let state = self.app.state::<crate::db::DbState>();
            let ctx = crate::syn::tools::ToolContext {
                db: &state,
                vault_path: &self.vault,
                app: &self.app,
                run_id: Some("run-under-test"),
            };
            let out = crate::syn::tools::execute_tool(&ctx, tool, &args).expect("the tool runs");
            serde_json::from_str(&out).expect("the tool returns JSON")
        }

        /// Write a skill the way the Skills screen will: an ordinary node.
        fn write(&self, name: &str, enabled: bool, steps: &str) {
            self.call(
                "create_node",
                serde_json::json!({
                    "node_type": SKILL_TYPE,
                    "title": name,
                    "content": steps,
                    "properties": frontmatter(
                        name,
                        "Tổng kết tuần từ task và calendar.",
                        "Khi người dùng nói \"tổng kết tuần\".",
                        Tier::Prose,
                        &["query_nodes".to_string()],
                        "user",
                        enabled,
                        1,
                    ),
                }),
            );
        }

        fn skills(&self) -> Vec<Skill> {
            let state = self.app.state::<crate::db::DbState>();
            let db = state.lock().expect("lock");
            all(&db).expect("read skills")
        }
    }

    #[test]
    fn a_skill_is_a_file_and_its_steps_come_back_whole() {
        let h = harness();
        h.write("weekly-review", true, "## Steps\n1. query_nodes for done tasks\n2. write the note");

        let skills = h.skills();
        assert_eq!(skills.len(), 1, "the skill was written and indexed");
        assert!(
            skills[0].id.starts_with(SKILL_FOLDER),
            "filed where the scan will find it: {}",
            skills[0].id
        );

        let out = h.call(LOAD_TOOL, serde_json::json!({ "name": "weekly-review" }));
        assert_eq!(out["name"], "weekly-review");
        assert_eq!(out["tier"], "prose");
        assert!(
            out["steps"].as_str().expect("steps").contains("query_nodes for done tasks"),
            "the body arrives whole, not summarised: {out}"
        );
    }

    /// A name that is not there answers with the names that are.
    ///
    /// The alternative — "not found" and nothing else — makes the model guess
    /// again, and it guesses the same wrong name surprisingly often.
    #[test]
    fn a_wrong_name_is_answered_with_the_right_ones() {
        let h = harness();
        h.write("weekly-review", true, "steps");
        h.write("inbox-zero", true, "steps");

        let out = h.call(LOAD_TOOL, serde_json::json!({ "name": "weekly_review" }));
        assert!(out["error"].is_string(), "it says no: {out}");
        let offered = out["available"].as_array().expect("the list");
        assert_eq!(offered.len(), 2, "and says what there is instead: {out}");
    }

    /// Turning a skill off means the model cannot use it, not merely that the
    /// index omits it — a name can be guessed, or remembered from a past run.
    #[test]
    fn a_disabled_skill_refuses_to_open() {
        let h = harness();
        h.write("dangerous-thing", false, "## Steps\n1. do not");

        let out = h.call(LOAD_TOOL, serde_json::json!({ "name": "dangerous-thing" }));
        assert!(
            out["error"].as_str().is_some_and(|e| e.contains("turned off")),
            "a disabled skill is refused by name: {out}"
        );
        assert!(out["steps"].is_null(), "and its steps do not leak: {out}");
    }

    /// The index the prompt carries names exactly the enabled ones.
    #[test]
    fn the_index_names_what_is_on_and_nothing_else() {
        let h = harness();
        h.write("on-one", true, "steps");
        h.write("off-one", false, "steps");
        h.write("on-two", true, "steps");

        let block = index_block(&h.skills(), INDEX_BUDGET_CHARS).expect("two are on");
        assert!(block.contains("on-one") && block.contains("on-two"));
        assert!(!block.contains("off-one"), "the off one is unmentioned:\n{block}");
    }
}
