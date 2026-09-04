//! What Syn remembers between conversations.
//!
//! # Why a memory is a node
//!
//! Every agent framework that grows a memory reaches for a store of its own — a
//! vector database, a JSON blob, a table nobody else reads. This app already
//! has the thing they are all approximating: a folder of Markdown files with
//! frontmatter, indexed for full text, versioned in a CRDT log, recoverable
//! from a trash, synced end-to-end encrypted between devices, and openable in
//! any editor.
//!
//! So a memory is a node. `type: memory`, filed under `Memory/`. It costs
//! nothing to build and it inherits all of that — but the reason is not
//! economy. It is that the user has to be able to read what Syn believes about
//! them, disagree with it, and delete it, using tools they already have. A
//! memory in a database Syn alone can read is a claim about somebody that they
//! cannot inspect. This one is a file with their name on it.
//!
//! # What is not here
//!
//! No embeddings. Recall runs over the same FTS5 index everything else uses,
//! because at the scale of a personal vault it is enough and because a second
//! retrieval mechanism is a second thing to keep correct. If recall turns out
//! to be the reason memory does not help, that is the moment to add one — and
//! `docs/adr-rag-vs-agentic-2026-09-03.md` is the shape that argument should
//! take: measure first, then choose the instrument.

use serde::{Deserialize, Serialize};

use crate::db::DbBridge;
use crate::error::AppResult;
use crate::models::node::NodeMetadata;

/// The `type:` a memory carries.
///
/// Prefixed, and it has to be. `memory` is an ordinary word and this is the
/// user's vault: somebody keeping track of what they remember about a language,
/// or a card deck, has every right to a kind called `memory`, and they would
/// collide with this one in both the type and the folder. The app's own kinds
/// are already prefixed this way — `finance_month`, `finance_config` — and this
/// is one of the app's own.
pub const MEMORY_TYPE: &str = "syn_memory";

/// Where memories are filed.
///
/// A top-level folder rather than something under `Syn/`, which would have read
/// better and would have been silently wrong: `is_in_unscanned_dir` skips
/// anything named `Syn`, so a memory written there is indexed by the write that
/// creates it and then dropped by the next full scan of the vault. The file
/// would sit on disk, correct and unreachable.
pub const MEMORY_FOLDER: &str = "SynMemory";

/// How much remembered text may ride in *every* prompt.
///
/// Roughly 800 tokens. Pinned memories are the ones that are true regardless of
/// what is being asked — a name, a timezone, how somebody wants to be spoken
/// to — and they are charged for on every message of every conversation, which
/// is what makes the ceiling necessary rather than tidy. Over it, the oldest
/// confirmations are dropped first and the user is told, because a memory that
/// is silently not being used is worse than one that was never written.
pub const MEMORY_BUDGET_CHARS: usize = 3_200;

/// How many memories a contextual recall may add to one prompt.
///
/// Small on purpose. These are selected by a full-text match on the current
/// question, which is a guess; six wrong guesses cost more than they buy.
pub const RECALL_LIMIT: usize = 6;

/// The kinds Syn suggests, in the order they are offered.
///
/// A list of suggestions, not a schema. Nothing refuses a memory carrying a
/// kind that is not here — the same rule `NodeType::Other` follows, for the
/// same reason: this is the user's vault, and a word they chose is data, not an
/// error.
pub const SUGGESTED_KINDS: &[&str] = &[
    "fact",
    "preference",
    "instruction",
    "relationship",
    "project",
];

/// One thing Syn has been told or worked out, kept between conversations.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Memory {
    /// Where the file sits, which is also how every other tool addresses it.
    pub id: String,
    /// A short name for what this is about, shown in lists.
    pub title: String,
    /// The memory itself, in the user's own language.
    pub body: String,
    /// `fact`, `preference`, … or whatever was written.
    pub kind: String,
    /// Who or what it is about, when that is a single nameable thing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// How sure Syn is, 0 to 1.
    ///
    /// Not a probability and not treated as one. It is a sort order and a
    /// reason to ask again, and it is written down mostly so the user can see
    /// that Syn is less sure about some of this than about the rest.
    pub confidence: f64,
    /// The run that produced it, so the transcript can be read back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_run: Option<String>,
    /// The nodes it was drawn from. A memory with no source is a memory nobody
    /// can check.
    #[serde(default)]
    pub source_nodes: Vec<String>,
    pub first_seen: String,
    pub last_confirmed: String,
    /// When this should be asked about again, if ever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_after: Option<String>,
    /// Whether it rides in every prompt rather than being recalled.
    pub pinned: bool,
    /// A memory this one replaces, kept rather than overwritten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
}

fn prop_str(node: &NodeMetadata, key: &str) -> Option<String> {
    node.properties
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
}

impl Memory {
    /// Read a memory out of the node it is stored as.
    ///
    /// Tolerant on the way in, because these files are meant to be edited by
    /// hand and half of them will eventually be. A missing `kind` is a fact, a
    /// missing `confidence` is certainty, and a missing date is the epoch —
    /// none of which is worth refusing to load somebody's own file over.
    pub fn from_node(node: &NodeMetadata) -> Self {
        Self {
            id: node.id.clone(),
            title: node.title.clone(),
            body: node.content.trim().to_string(),
            kind: prop_str(node, "kind").unwrap_or_else(|| "fact".to_string()),
            subject: prop_str(node, "subject"),
            confidence: node
                .properties
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
            source_run: prop_str(node, "source_run"),
            source_nodes: node
                .properties
                .get("source_nodes")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            first_seen: prop_str(node, "first_seen").unwrap_or_else(|| node.created_at.clone()),
            last_confirmed: prop_str(node, "last_confirmed")
                .unwrap_or_else(|| node.updated_at.clone()),
            review_after: prop_str(node, "review_after"),
            pinned: node
                .properties
                .get("pinned")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            supersedes: prop_str(node, "supersedes"),
        }
    }

    /// One line, as the prompt carries it.
    ///
    /// The kind is included because it changes how the line should be read: a
    /// `preference` is something to honour, an `instruction` is something to
    /// obey, and a `fact` is something that may simply have stopped being true.
    pub fn line(&self) -> String {
        let subject = self
            .subject
            .as_deref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        let hedge = if self.confidence < 0.6 { " — unsure" } else { "" };
        format!("- [{}{subject}] {}{hedge}", self.kind, self.body)
    }

    /// The same memory rendered as something to obey, not something to know.
    ///
    /// No `[kind]` tag: the heading it sits under already says these are
    /// instructions, and a bullet reading `- [instruction] ...` invites being
    /// read as one more piece of trivia about the person, which is exactly the
    /// failure this rendering exists to fix.
    pub fn directive(&self) -> String {
        let subject = self
            .subject
            .as_deref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        let hedge = if self.confidence < 0.6 { " — unsure" } else { "" };
        format!("- {}{subject}{hedge}", self.body)
    }

    /// Is this a standing instruction rather than something merely true?
    ///
    /// Unicode case folding, not `eq_ignore_ascii_case`. The kind is a string
    /// in a Markdown file the user can edit by hand, and ASCII folding is the
    /// bug this codebase keeps rediscovering in Vietnamese.
    pub fn is_instruction(&self) -> bool {
        self.kind.trim().to_lowercase() == "instruction"
    }

    /// A preference is not binding the way an instruction is, but its absence
    /// shows in an answer sooner than a plain fact's does — so when something
    /// has to go, it goes after this.
    pub fn is_preference(&self) -> bool {
        self.kind.trim().to_lowercase() == "preference"
    }

    /// Whether this is past the date it asked to be checked again.
    ///
    /// Dates are compared as strings, which is correct for `YYYY-MM-DD` and is
    /// the same thing the query engine does with `due_date`.
    pub fn is_stale(&self, today: &str) -> bool {
        self.review_after
            .as_deref()
            .is_some_and(|when| when < today)
    }
}

/// Every memory in the vault, most recently confirmed first.
pub fn all(db: &DbBridge) -> AppResult<Vec<Memory>> {
    let mut memories: Vec<Memory> = db
        .get_nodes_by_type(MEMORY_TYPE)?
        .iter()
        .map(Memory::from_node)
        .collect();
    memories.sort_by(|a, b| b.last_confirmed.cmp(&a.last_confirmed));
    Ok(memories)
}

/// The block that rides in every prompt, or `None` when there is nothing to say.
///
/// Every memory goes in, not only the pinned ones. The reason is measured. When
/// this function sent only what was pinned, everything else reached the model
/// solely through the `recall` tool — and across fifteen real runs and
/// twenty-six tool calls, `recall` was never called once. The single memory Syn
/// had written was unpinned, so the only thing it had ever remembered had never
/// reached it. `docs/adr-memory-shape-2026-09-04.md` has the numbers, including
/// the four eval questions that a `recall`-only path answered well in a harness
/// and cannot answer at all in the app.
///
/// So `pinned` stops meaning "exists as far as the model is concerned" and
/// starts meaning "survives eviction". At the length of a real memory, fifty of
/// them fit this budget; below that the flag changes nothing, which is correct,
/// because it is a tie-breaker and there is no tie.
///
/// Returning `None` rather than an empty string is what keeps a vault with no
/// memories sending byte for byte the prompt it sent before this module
/// existed — a property asserted by the snapshot tests in `prompt.rs`.
///
/// The block instructs rather than merely listing, and separates the memories
/// that bind from the memories that inform. That is also measured: in the P2
/// model eval, a memory reading *always write to the team in Vietnamese, even
/// when asked in English* sat in the prompt and lost to an English request,
/// because the request is in the last message and the memory was one bullet in
/// a flat list some thousands of characters earlier. Facts do not have this
/// problem — asked for a seat number, a model uses the seat number it was
/// given. Instructions do, because they compete with how a question is phrased
/// rather than with what it asks for.
///
/// Eviction order, when the budget bites: instructions never lose to a fact;
/// within a group, pinned outrank unpinned, preferences outrank plain facts,
/// and the least recently confirmed goes first. That last clause is the only
/// decay this module has — a memory nobody has confirmed in a long time loses
/// its place, and never its file.
pub fn memory_block(memories: &[Memory], budget_chars: usize) -> Option<String> {
    if memories.is_empty() {
        return None;
    }

    let header = "\n\n=== WHAT YOU REMEMBER ===\n\
         Things you have been told or worked out before, kept between \
         conversations. They are not notes in this vault; they are what you \
         know about this person.\n\
         Before you answer, check this against what was asked: if anything here \
         bears on the answer, the answer has to reflect it.\n\
         - If one contradicts what they say now, they are right. Say so, and \
         use `remember` to record the correction.\n\
         - Do not repeat these back unprompted. Act on them.\n";
    let instructions_head = "\nHOW THEY WANT YOU TO WORK — these hold even when the \
         request is worded as though they do not apply:\n";
    let facts_head = "\nWHAT YOU KNOW ABOUT THEM:\n";
    let footer = "=== END ===";

    let (mut instructions, mut facts): (Vec<&Memory>, Vec<&Memory>) =
        memories.iter().partition(|m| m.is_instruction());

    // Whoever is sorted first is budgeted first, and so is the last to be cut.
    instructions.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(b.last_confirmed.cmp(&a.last_confirmed))
    });
    facts.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then(b.is_preference().cmp(&a.is_preference()))
            .then(b.last_confirmed.cmp(&a.last_confirmed))
    });

    // Both headings are reserved up front when their group has anything in it.
    // If every entry in a group is then dropped for size, a few characters go
    // unspent — which errs under the budget, the safe direction.
    let mut used = header.chars().count() + footer.chars().count();
    if !instructions.is_empty() {
        used += instructions_head.chars().count();
    }
    if !facts.is_empty() {
        used += facts_head.chars().count();
    }
    let mut dropped = 0usize;

    // Instructions first, and not only for reading order: whatever budget they
    // take is budget a fact cannot take from them.
    let mut instruction_lines = Vec::new();
    for memory in &instructions {
        let line = memory.directive();
        let cost = line.chars().count() + 1;
        if used + cost > budget_chars {
            dropped += 1;
            continue;
        }
        used += cost;
        instruction_lines.push(line);
    }

    let mut fact_lines = Vec::new();
    for memory in &facts {
        let line = memory.line();
        let cost = line.chars().count() + 1;
        if used + cost > budget_chars {
            dropped += 1;
            continue;
        }
        used += cost;
        fact_lines.push(line);
    }

    if instruction_lines.is_empty() && fact_lines.is_empty() {
        // Every memory is individually too large for the budget. Saying nothing
        // here would be indistinguishable from having none.
        log::warn!(
            "[Syn] {} memories, none of which fit a {} character budget",
            memories.len(),
            budget_chars
        );
        return None;
    }

    let mut block = String::from(header);
    if !instruction_lines.is_empty() {
        block.push_str(instructions_head);
        block.push_str(&instruction_lines.join("\n"));
        block.push('\n');
    }
    if !fact_lines.is_empty() {
        block.push_str(facts_head);
        block.push_str(&fact_lines.join("\n"));
        block.push('\n');
    }
    if dropped > 0 {
        block.push_str(&format!(
            "({dropped} more memories did not fit and were left out.)\n"
        ));
    }
    block.push_str(footer);
    Some(block)
}

/// Make a rendered block smaller by at least `free_at_least` characters.
///
/// `None` when it cannot be done while leaving anything worth sending — the
/// caller drops the block instead.
///
/// This exists because the prompt trimmer removes whole sections, so a budget
/// tight enough to touch memory made Syn forget *everything* rather than the
/// least important thing. That cliff got likelier exactly as memory got more
/// valuable, and it is invisible from inside a conversation: nothing about the
/// reply says the assistant was handed none of what it knows.
///
/// Eviction runs backwards through the order `memory_block` wrote: the last
/// fact goes first, and instructions go only when the facts have run out.
pub fn shrink_block(block: &str, free_at_least: usize) -> Option<String> {
    const NOTE: &str = " more memories did not fit and were left out.)";

    let mut lines: Vec<String> = block.lines().map(str::to_string).collect();

    // Where each group's entries begin. Everything before the first heading is
    // preamble, and its two `- ` bullets are rules, not memories.
    let head_of = |needle: &str| lines.iter().position(|l| l.contains(needle));
    let instructions_at = head_of("HOW THEY WANT YOU TO WORK");
    let facts_at = head_of("WHAT YOU KNOW ABOUT THEM");

    let entries = |from: Option<usize>, until: Option<usize>, lines: &[String]| -> Vec<usize> {
        let Some(start) = from else { return Vec::new() };
        let end = until.unwrap_or(lines.len());
        (start + 1..end.min(lines.len()))
            .filter(|i| lines[*i].starts_with("- "))
            .collect()
    };

    // Facts first, last one first; then instructions, last one first.
    let mut order: Vec<usize> = entries(facts_at, None, &lines);
    order.reverse();
    let mut instructions: Vec<usize> = entries(instructions_at, facts_at, &lines);
    instructions.reverse();
    order.extend(instructions);

    let total = order.len();
    if total <= 1 {
        return None;
    }

    let mut freed = 0usize;
    let mut cut: Vec<usize> = Vec::new();
    for index in order {
        // Never take the last one standing: a block with a heading and no
        // memories under it is noise with a footer.
        if cut.len() + 1 >= total {
            break;
        }
        freed += lines[index].chars().count() + 1;
        cut.push(index);
        if freed >= free_at_least {
            break;
        }
    }
    if freed < free_at_least {
        return None;
    }

    let already: usize = lines
        .iter()
        .find(|l| l.contains(NOTE))
        .and_then(|l| l.trim_start_matches('(').split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    lines.retain(|l| !l.contains(NOTE));

    let mut cut = cut;
    cut.sort_unstable();
    for index in cut.iter().rev() {
        lines.remove(*index);
    }

    // A heading whose entries have all gone says nothing true.
    let orphaned: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            (l.contains("HOW THEY WANT YOU TO WORK") || l.contains("WHAT YOU KNOW ABOUT THEM"))
                && !lines.get(i + 1).is_some_and(|next| next.starts_with("- "))
        })
        .map(|(i, _)| i)
        .collect();
    for index in orphaned.iter().rev() {
        lines.remove(*index);
    }

    let note = format!("({} more memories did not fit and were left out.)", already + cut.len());
    match lines.iter().position(|l| l.starts_with("=== END")) {
        Some(footer) => lines.insert(footer, note),
        None => lines.push(note),
    }
    Some(lines.join("\n"))
}

/// How many memory lines a rendered block actually shows.
///
/// Counted here rather than at the call site because the two line shapes are
/// this module's business: a fact renders `- [kind] ...` and an instruction
/// renders `- ...`, and a caller counting `- [` silently undercounts every
/// instruction. That is exactly the bug this function was extracted to end.
pub fn lines_shown(block: &str) -> usize {
    block
        .split_once("HOW THEY WANT YOU TO WORK")
        .or_else(|| block.split_once("WHAT YOU KNOW ABOUT THEM"))
        .map(|(_, rest)| rest.lines().filter(|l| l.starts_with("- ")).count())
        .unwrap_or(0)
}

/// Memories that make a claim about the same thing this one does.
///
/// Same `kind` and same `subject`, which is as far as two memories can be
/// compared without reading them. Used to *ask* rather than to overwrite: an
/// assistant that silently changes its mind about somebody, and cannot say when
/// or why, is the thing this whole module is arranged to avoid.
pub fn conflicting<'a>(
    memories: &'a [Memory],
    kind: &str,
    subject: Option<&str>,
) -> Vec<&'a Memory> {
    memories
        .iter()
        .filter(|m| {
            m.kind.eq_ignore_ascii_case(kind)
                && match (m.subject.as_deref(), subject) {
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    (None, None) => true,
                    _ => false,
                }
        })
        .collect()
}

/// The frontmatter a new memory is written with.
#[allow(clippy::too_many_arguments)]
pub fn frontmatter(
    kind: &str,
    subject: Option<&str>,
    confidence: f64,
    source_run: Option<&str>,
    source_nodes: &[String],
    pinned: bool,
    supersedes: Option<&str>,
    today: &str,
) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    props.insert("kind".into(), serde_json::json!(kind));
    if let Some(subject) = subject.filter(|s| !s.trim().is_empty()) {
        props.insert("subject".into(), serde_json::json!(subject));
    }
    props.insert(
        "confidence".into(),
        serde_json::json!(confidence.clamp(0.0, 1.0)),
    );
    if let Some(run) = source_run {
        props.insert("source_run".into(), serde_json::json!(run));
    }
    if !source_nodes.is_empty() {
        props.insert("source_nodes".into(), serde_json::json!(source_nodes));
    }
    props.insert("first_seen".into(), serde_json::json!(today));
    props.insert("last_confirmed".into(), serde_json::json!(today));
    props.insert("pinned".into(), serde_json::json!(pinned));
    if let Some(old) = supersedes {
        props.insert("supersedes".into(), serde_json::json!(old));
    }
    serde_json::Value::Object(props)
}

/// Today, as the machine's clock reads it, for the dates above.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Remembering and recalling, through the tools the assistant actually calls.
///
/// The unit tests above exercise the shapes; this exercises the path — a
/// memory written by `remember` lands in the vault as a file, comes back
/// through `recall`, and reaches the prompt. Everything in between is the part
/// that was never going to be right by inspection.
#[cfg(test)]
mod through_the_tools {
    use super::*;
    use crate::db::DbBridge;
    use tauri::Manager;

    struct Harness {
        _dir: tempfile::TempDir,
        vault: String,
        /// The app owns the database, because the shared write path reaches for
        /// it through `app.state()` rather than taking it as an argument. A
        /// harness that only handed one to `ToolContext` panicked on the first
        /// write, which is the sort of thing an end-to-end test exists to hit.
        app: tauri::AppHandle<tauri::test::MockRuntime>,
    }

    fn harness() -> Harness {
        let dir = tempfile::tempdir().expect("temp vault");
        // Canonicalised, and it has to be. The write path resolves the vault
        // to its real location while `to_relative` strips the string it was
        // given, and on macOS a temp dir is `/var/...` for a real
        // `/private/var/...`. When those disagree `to_relative` silently falls
        // back to the absolute path, the node is indexed under an id no
        // relative lookup will ever match, and `trash_node` on a note the
        // assistant just created answers "Node not found".
        //
        // A harness artifact here, but the same mismatch is reachable by any
        // real vault kept under a symlink, and nothing warns.
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

        fn memories(&self) -> Vec<Memory> {
            let state = self.app.state::<crate::db::DbState>();
            let db = state.lock().expect("lock");
            all(&db).expect("read memories")
        }
    }

    #[test]
    fn a_remembered_thing_becomes_a_file_anyone_can_read() {
        let h = harness();
        let out = h.call(
            "remember",
            serde_json::json!({
                "body": "Thích họp buổi sáng, không họp sau 16h.",
                "kind": "preference",
                "subject": "Minh",
                "pinned": true,
            }),
        );
        assert_eq!(out["success"], true);

        let id = out["id"].as_str().expect("an id");
        assert!(
            id.starts_with("SynMemory/"),
            "filed apart from anywhere a user's own kind would land, got {id}"
        );

        // The point of storing it as a file: it can be read without this app.
        let on_disk = std::fs::read_to_string(std::path::Path::new(&h.vault).join(id))
            .expect("the file is there");
        assert!(on_disk.contains("type: syn_memory"));
        assert!(on_disk.contains("Thích họp buổi sáng"));
        assert!(on_disk.contains("subject: Minh"));

        let m = &h.memories()[0];
        assert_eq!(m.kind, "preference");
        assert!(m.pinned);
        // Provenance, so a claim about somebody can be traced to the run that
        // made it.
        assert_eq!(m.source_run.as_deref(), Some("run-under-test"));
        assert_eq!(m.first_seen, today());
    }

    #[test]
    fn a_memory_with_no_body_is_refused_rather_than_written_empty() {
        let h = harness();
        for args in [
            serde_json::json!({}),
            serde_json::json!({ "body": "" }),
            serde_json::json!({ "body": "   " }),
        ] {
            let out = h.call("remember", args);
            assert!(out.get("error").is_some(), "expected a refusal, got {out}");
        }
        assert!(h.memories().is_empty());
    }

    /// The rule this feature is arranged around: a second claim about the same
    /// thing is a question for the user, not something to resolve quietly.
    #[test]
    fn a_second_claim_about_the_same_thing_is_reported_not_resolved() {
        let h = harness();
        h.call(
            "remember",
            serde_json::json!({ "body": "Thích họp sáng", "kind": "preference", "subject": "Minh" }),
        );
        let out = h.call(
            "remember",
            serde_json::json!({ "body": "Thích họp chiều", "kind": "preference", "subject": "Minh" }),
        );

        let clashes = out["existing_claims_about_the_same_thing"]
            .as_array()
            .expect("an array");
        assert_eq!(clashes.len(), 1);
        assert_eq!(clashes[0]["body"], "Thích họp sáng");
        assert!(out["message"].as_str().expect("a message").contains("ask which is right"));

        // Both are kept. Nothing was overwritten.
        assert_eq!(h.memories().len(), 2);
    }

    #[test]
    fn a_claim_about_someone_else_is_not_a_clash() {
        let h = harness();
        h.call(
            "remember",
            serde_json::json!({ "body": "Thích họp sáng", "kind": "preference", "subject": "Minh" }),
        );
        let out = h.call(
            "remember",
            serde_json::json!({ "body": "Thích họp chiều", "kind": "preference", "subject": "Lan" }),
        );
        assert!(out["existing_claims_about_the_same_thing"]
            .as_array()
            .expect("an array")
            .is_empty());
    }

    #[test]
    fn recall_finds_by_word_kind_and_subject() {
        let h = harness();
        h.call("remember", serde_json::json!({ "body": "Sống ở Hà Nội", "kind": "fact" }));
        h.call(
            "remember",
            serde_json::json!({ "body": "Thích cà phê đen", "kind": "preference", "subject": "Minh" }),
        );
        h.call(
            "remember",
            serde_json::json!({ "body": "Luôn trả lời bằng tiếng Việt", "kind": "instruction" }),
        );

        assert_eq!(h.call("recall", serde_json::json!({}))["total_matches"], 3);
        assert_eq!(
            h.call("recall", serde_json::json!({ "kind": "preference" }))["total_matches"],
            1
        );
        assert_eq!(
            h.call("recall", serde_json::json!({ "subject": "Minh" }))["total_matches"],
            1
        );

        let by_word = h.call("recall", serde_json::json!({ "query": "cà phê" }));
        assert_eq!(by_word["total_matches"], 1);
        assert_eq!(by_word["memories"][0]["body"], "Thích cà phê đen");
    }

    /// A pinned memory outranks a newer unpinned one, because pinning is the
    /// user saying it matters regardless of what is being asked.
    #[test]
    fn recall_puts_the_pinned_ones_first() {
        let h = harness();
        h.call("remember", serde_json::json!({ "body": "Không quan trọng lắm" }));
        h.call("remember", serde_json::json!({ "body": "Tên là Minh", "pinned": true }));

        let out = h.call("recall", serde_json::json!({}));
        assert_eq!(out["memories"][0]["body"], "Tên là Minh");
        assert_eq!(out["memories"][0]["pinned"], true);
    }

    /// Forgetting needs no tool of its own: a memory is a node, and the node
    /// tools already remove one and put it back.
    #[test]
    fn trashing_a_memory_forgets_it_and_restoring_brings_it_back() {
        let h = harness();
        let id = h
            .call("remember", serde_json::json!({ "body": "Ghét hành" }))["id"]
            .as_str()
            .expect("an id")
            .to_string();

        h.call("trash_node", serde_json::json!({ "node_id": id }));
        assert!(h.memories().is_empty(), "trashing forgets it");

        // The route the model has to take, rather than the one that seemed
        // obvious: `restore_node` is addressed by where the file went, not by
        // where it was, so `list_trash` is a required step and not a courtesy.
        // Asserted because a test that guessed the argument name passed
        // nothing and reported a missing feature.
        let listed = h.call("list_trash", serde_json::json!({}));
        assert_eq!(listed["total_in_trash"], 1);
        let entry = &listed["trash"][0];
        assert_eq!(entry["type"], MEMORY_TYPE);
        assert_eq!(entry["was_at"], id);

        h.call(
            "restore_node",
            serde_json::json!({ "trash_path": entry["trash_path"] }),
        );
        assert_eq!(h.memories().len(), 1, "restoring remembers it again");
    }

    /// The whole point: what was remembered reaches the prompt. All of it.
    ///
    /// This test used to assert the opposite of its second half — that an
    /// unpinned memory does *not* ride in every prompt — and it passed, and the
    /// behaviour it protected meant the only memory Syn had ever written never
    /// once reached Syn. Unpinned was reachable solely through `recall`, and
    /// `recall` was never called in fifteen real runs.
    #[test]
    fn everything_remembered_reaches_the_prompt() {
        let h = harness();
        h.call(
            "remember",
            serde_json::json!({ "body": "Tên là Minh, ở Hà Nội", "pinned": true }),
        );
        h.call("remember", serde_json::json!({ "body": "Đang đọc quyển Sapiens" }));

        let block = memory_block(&h.memories(), MEMORY_BUDGET_CHARS).expect("there are memories");
        let rendered = crate::syn::prompt::PromptPlan::for_chat(crate::syn::prompt::ChatPrompt {
            context: "",
            personality: "auto",
            custom: None,
            memory: Some(&block),
            budget_chars: crate::syn::prompt::DEFAULT_BUDGET_CHARS,
        })
        .render();

        assert!(rendered.contains("Tên là Minh"), "the pinned one is there");
        assert!(
            rendered.contains("Sapiens"),
            "and so is the unpinned one — being unpinned decides who is cut \
             first, not who exists:\n{rendered}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Comfortably more than any block these tests build.
    const DEFAULT_TEST_BUDGET: usize = 3_200;

    fn node(title: &str, body: &str, props: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: format!("{MEMORY_FOLDER}/{title}.md"),
            node_type: MEMORY_TYPE.to_string(),
            title: title.to_string(),
            content: body.to_string(),
            properties: props,
            created_at: "2026-09-01T00:00:00Z".into(),
            updated_at: "2026-09-01T00:00:00Z".into(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn memory(body: &str, pinned: bool, confirmed: &str) -> Memory {
        Memory::from_node(&node(
            "m",
            body,
            serde_json::json!({
                "kind": "preference",
                "pinned": pinned,
                "last_confirmed": confirmed,
                "confidence": 0.9,
            }),
        ))
    }

    fn of_kind(body: &str, kind: &str, confirmed: &str) -> Memory {
        Memory::from_node(&node(
            "m",
            body,
            serde_json::json!({
                "kind": kind,
                "pinned": true,
                "last_confirmed": confirmed,
                "confidence": 0.9,
            }),
        ))
    }

    /// An instruction is rendered as one, and stands apart from what is merely
    /// true about the person.
    ///
    /// This is the fix for a measured failure, not a tidiness preference. In the
    /// P2 model eval, *always write to the team in Vietnamese, even when asked
    /// in English* sat in the prompt as `- [instruction] ...`, one bullet in a
    /// flat list, and the English phrasing of the request won. A fact never
    /// loses that way: asked for a seat number, the model uses the seat number.
    #[test]
    fn an_instruction_is_told_apart_from_a_fact() {
        let block = memory_block(
            &[
                of_kind("Ghét hành.", "preference", "2026-09-02"),
                of_kind(
                    "Luôn viết cho team bằng tiếng Việt, kể cả khi được hỏi bằng tiếng Anh.",
                    "instruction",
                    "2026-09-01",
                ),
            ],
            DEFAULT_TEST_BUDGET,
        )
        .expect("something is pinned");

        let instructions = block
            .find("HOW THEY WANT YOU TO WORK")
            .expect("instructions have their own heading");
        let facts = block
            .find("WHAT YOU KNOW ABOUT THEM")
            .expect("the rest have theirs");
        assert!(
            instructions < facts,
            "what binds comes before what merely informs:\n{block}"
        );

        let line = block
            .lines()
            .find(|l| l.contains("tiếng Việt"))
            .expect("the instruction is in there");
        assert!(
            !line.contains("[instruction]"),
            "an instruction reads as a directive, not as one more labelled \
             fact about them: {line}"
        );
        assert!(
            block.contains("- [preference] Ghét hành."),
            "a fact keeps its label:\n{block}"
        );
    }

    /// A fact can never crowd out an instruction.
    ///
    /// Budget is spent on instructions first. Without that the drop order is
    /// recency, and the oldest standing instruction — usually the one the
    /// person has lived with longest — is the first thing to go.
    ///
    /// The budget is calibrated from the block itself rather than guessed. A
    /// guessed number produced a test that passed whichever order the code
    /// used, because it was tight enough to drop the fact either way: it
    /// asserted the right thing about a situation that could not distinguish
    /// the two.
    #[test]
    fn an_instruction_keeps_its_place_when_a_fact_cannot() {
        // The instruction must render longer than the fact, or "one line fits"
        // is satisfied by either of them and the order stops mattering again.
        let instruction = of_kind(
            "Không dùng emoji trong bất cứ nội dung nào gửi ra ngoài cho khách.",
            "instruction",
            "2026-01-01",
        );
        let fact = of_kind("Thích cà phê đen không đường.", "fact", "2026-09-09");
        let both = [fact.clone(), instruction.clone()];

        let roomy = memory_block(&both, 10_000).expect("both fit");
        assert!(roomy.contains("emoji") && roomy.contains("cà phê"), "both fit");

        let fact_line = fact.line().chars().count() + 1;
        assert!(
            instruction.directive().chars().count() + 1 > fact_line,
            "the instruction has to be the longer line for this test to bite"
        );

        // Exactly one line short of holding both.
        let budget = roomy.chars().count() - fact_line;
        let block = memory_block(&both, budget).expect("one fits");

        assert!(
            block.contains("emoji"),
            "the instruction survives, though it is the stalest and the longer:\n{block}"
        );
        assert!(
            !block.contains("cà phê"),
            "the fact is what gets dropped:\n{block}"
        );
        assert!(
            block.contains("did not fit and were left out"),
            "and the loss is declared:\n{block}"
        );
    }

    /// The folder has to be one the vault scan actually walks.
    ///
    /// `Syn/` was the obvious home — it is where conversations, settings and
    /// run transcripts live — and it would have failed silently. The scan skips
    /// anything named `Syn`, so a memory written there is indexed by the write
    /// that creates it, works perfectly for the rest of the session, and is
    /// dropped by the next full scan. The file stays on disk, correct and
    /// unreachable, and nothing reports it.
    #[test]
    fn memories_are_filed_somewhere_the_vault_scan_will_look() {
        assert!(
            !crate::commands::nodes::is_in_unscanned_dir(&format!("{MEMORY_FOLDER}/x.md")),
            "`{MEMORY_FOLDER}` is skipped by the vault scan, so memories filed there \
             survive until the next rescan and then vanish"
        );
    }

    /// The word `memory` belongs to whoever owns the vault.
    #[test]
    fn the_unprefixed_word_is_left_for_the_user() {
        assert_eq!(MEMORY_TYPE, "syn_memory");
        assert_ne!(MEMORY_TYPE, "memory");
        assert_ne!(MEMORY_FOLDER, "Memory");
    }

    /// These files are meant to be edited by hand, so half of them eventually
    /// will be. A missing key is a default, never a refusal to load.
    #[test]
    fn a_hand_written_memory_with_almost_nothing_in_it_still_loads() {
        let m = Memory::from_node(&node("m", "Tao thích cà phê đen", serde_json::json!({})));
        assert_eq!(m.kind, "fact");
        assert_eq!(m.confidence, 1.0);
        assert!(!m.pinned);
        assert_eq!(m.body, "Tao thích cà phê đen");
        assert_eq!(m.first_seen, "2026-09-01T00:00:00Z");
    }

    #[test]
    fn a_kind_nobody_listed_is_kept_as_written() {
        let m = Memory::from_node(&node("m", "x", serde_json::json!({ "kind": "thói quen" })));
        assert_eq!(m.kind, "thói quen", "an invented kind is data, not an error");
    }

    #[test]
    fn confidence_outside_zero_to_one_is_brought_back_inside_it() {
        for (given, want) in [(5.0, 1.0), (-2.0, 0.0), (0.4, 0.4)] {
            let m = Memory::from_node(&node("m", "x", serde_json::json!({ "confidence": given })));
            assert_eq!(m.confidence, want);
        }
    }

    /// The property that keeps every existing prompt snapshot valid: a vault
    /// with no memories adds nothing at all.
    ///
    /// It is now *only* emptiness that does this. A vault holding one unpinned
    /// memory used to render nothing either, which is what made the feature
    /// inert; the second assertion here is the difference.
    #[test]
    fn no_memories_means_no_block_rather_than_an_empty_one() {
        assert!(memory_block(&[], MEMORY_BUDGET_CHARS).is_none());
        assert!(
            memory_block(&[memory("not pinned", false, "2026-09-01")], MEMORY_BUDGET_CHARS)
                .is_some(),
            "an unpinned memory is still a memory the model is given"
        );
    }

    #[test]
    fn both_a_pinned_and_an_unpinned_memory_reach_the_block() {
        let block = memory_block(
            &[
                memory("họp buổi sáng thôi", true, "2026-09-01"),
                memory("thích trà hơn cà phê", false, "2026-09-02"),
            ],
            MEMORY_BUDGET_CHARS,
        )
        .expect("there are memories");

        assert!(block.contains("họp buổi sáng thôi"));
        assert!(block.contains("thích trà hơn cà phê"));
        assert!(block.contains("WHAT YOU REMEMBER"));
    }

    /// Pinning decides the order of the queue for the axe, nothing else.
    #[test]
    fn pinned_is_the_last_to_be_cut_not_the_only_one_present() {
        let pinned = memory("cái này đã ghim", true, "2020-01-01");
        let fresh = memory("cái này mới hơn nhưng không ghim", false, "2026-09-09");
        let both = [fresh.clone(), pinned.clone()];

        let roomy = memory_block(&both, 10_000).expect("both fit");
        assert!(roomy.contains("đã ghim") && roomy.contains("mới hơn"), "both fit");

        // One line short of holding both.
        let budget = roomy.chars().count() - (fresh.line().chars().count() + 1);
        let tight = memory_block(&both, budget).expect("one fits");
        assert!(
            tight.contains("đã ghim"),
            "the pinned one survives even though it is the stalest:\n{tight}"
        );
        assert!(!tight.contains("mới hơn"), "the unpinned one is cut:\n{tight}");
    }

    /// Over budget, what survives is what was most recently confirmed — and the
    /// prompt says how many were left out, because a pinned memory silently not
    /// in play is worse than one that was never written.
    #[test]
    fn over_budget_the_stalest_go_and_the_loss_is_declared() {
        let long = "x".repeat(200);
        let memories: Vec<Memory> = (1..=9)
            .map(|i| memory(&long, true, &format!("2026-09-{i:02}")))
            .collect();

        let block = memory_block(&memories, 800).expect("some fit");
        assert!(block.contains("did not fit and were left out"));
        assert!(block.chars().count() <= 800 + 80, "block was {} chars", block.chars().count());
    }

    #[test]
    fn a_low_confidence_memory_is_hedged_where_the_model_can_see_it() {
        let unsure = Memory::from_node(&node(
            "m",
            "có thể đã đổi việc",
            serde_json::json!({ "kind": "fact", "confidence": 0.3 }),
        ));
        assert!(unsure.line().contains("unsure"));

        let sure = Memory::from_node(&node(
            "m",
            "sống ở Hà Nội",
            serde_json::json!({ "kind": "fact", "confidence": 0.95 }),
        ));
        assert!(!sure.line().contains("unsure"));
    }

    #[test]
    fn a_subject_is_shown_so_two_memories_about_two_people_do_not_read_alike() {
        let m = Memory::from_node(&node(
            "m",
            "thích họp sáng",
            serde_json::json!({ "kind": "preference", "subject": "Minh" }),
        ));
        assert_eq!(m.line(), "- [preference (Minh)] thích họp sáng");
    }

    #[test]
    fn a_claim_about_the_same_thing_is_found_so_it_can_be_asked_about() {
        let existing = vec![
            Memory::from_node(&node(
                "a",
                "thích họp sáng",
                serde_json::json!({ "kind": "preference", "subject": "Minh" }),
            )),
            Memory::from_node(&node(
                "b",
                "thích họp chiều",
                serde_json::json!({ "kind": "preference", "subject": "Lan" }),
            )),
            Memory::from_node(&node(
                "c",
                "sống ở Hà Nội",
                serde_json::json!({ "kind": "fact", "subject": "Minh" }),
            )),
        ];

        let clash = conflicting(&existing, "preference", Some("Minh"));
        assert_eq!(clash.len(), 1);
        assert_eq!(clash[0].body, "thích họp sáng");

        // A different kind about the same person is not a contradiction.
        assert_eq!(conflicting(&existing, "instruction", Some("Minh")).len(), 0);
        // Nor is the same kind about somebody else.
        assert_eq!(conflicting(&existing, "preference", Some("Hùng")).len(), 0);
    }

    #[test]
    fn a_review_date_in_the_past_makes_a_memory_stale() {
        let m = Memory::from_node(&node(
            "m",
            "x",
            serde_json::json!({ "review_after": "2026-08-01" }),
        ));
        assert!(m.is_stale("2026-09-03"));
        assert!(!m.is_stale("2026-07-01"));

        // No review date is a memory that never goes stale, not one that
        // always is.
        let forever = Memory::from_node(&node("m", "x", serde_json::json!({})));
        assert!(!forever.is_stale("2099-01-01"));
    }

    #[test]
    fn new_frontmatter_carries_its_own_provenance() {
        let props = frontmatter(
            "preference",
            Some("Minh"),
            0.8,
            Some("run-1"),
            &["Notes/a.md".to_string()],
            true,
            None,
            "2026-09-03",
        );
        assert_eq!(props["kind"], "preference");
        assert_eq!(props["subject"], "Minh");
        assert_eq!(props["source_run"], "run-1");
        assert_eq!(props["source_nodes"][0], "Notes/a.md");
        assert_eq!(props["first_seen"], "2026-09-03");
        assert_eq!(props["last_confirmed"], "2026-09-03");
        assert_eq!(props["pinned"], true);
        assert!(props.get("supersedes").is_none(), "absent, not null");
    }
}


/// Does remembering change the answer?
///
/// The gate P2 was given asks for a number: on a fixed set of questions, the
/// version with memory should be better on several and worse on none. That
/// question has two halves which fail differently, and the lesson from
/// `docs/adr-rag-vs-agentic-2026-09-03.md` is to measure them apart — there,
/// four separate defects in the *scorer* each produced a plausible number, and
/// every one was found by reading an answer marked wrong and disagreeing with
/// the mark.
///
/// So this module holds the deterministic half: given these memories and this
/// question, does the right thing reach the prompt at all? That is free, it is
/// exact, and it is a precondition for the model half being worth paying for.
/// A memory that never reaches the prompt cannot help, and measuring a model
/// against it would measure nothing but noise.
#[cfg(test)]
mod does_memory_reach_the_model {
    use super::*;
    use crate::db::DbBridge;
    use tauri::Manager;

    /// One question, and the memory that should be in play when it is asked.
    ///
    /// `pub(super)` so the model half reads this same list. Two lists would
    /// drift, and the deterministic half exists precisely to say whether the
    /// model half is worth paying for.
    pub(super) struct Case {
        pub ask: &'static str,
        /// The memory that answers it, as it would be written.
        pub memory: &'static str,
        pub pinned: bool,
        /// What a correct answer turns on, for the person reading the model
        /// half's table.
        pub turns_on: &'static str,
        /// Whether the memory *should* change the answer at all.
        ///
        /// `false` marks a control: the memory is in the prompt and the right
        /// thing to do with it is nothing. Without controls this eval only
        /// measures whether memory leaks into an answer, and a memory system
        /// that dragged itself into every reply would score perfectly on the
        /// other eighteen.
        pub matters: bool,
    }

    pub(super) const CASES: &[Case] = &[
        // ── Pinned, and the answer turns on them ──────────────────────
        Case {
            ask: "Đặt lịch họp với Minh chiều mai lúc 5h được không?",
            memory: "Minh không họp sau 16h.",
            pinned: true,
            turns_on: "pushing back rather than booking 17:00",
            matters: true,
        },
        Case {
            ask: "Gợi ý cho tao mấy món ăn tối nay",
            memory: "Ghét hành, không ăn được hành trong bất cứ món nào.",
            pinned: true,
            turns_on: "not suggesting something with onion",
            matters: true,
        },
        Case {
            ask: "Draft a short note to the team about the release",
            memory: "Luôn viết cho team bằng tiếng Việt, kể cả khi được hỏi bằng tiếng Anh.",
            pinned: true,
            turns_on: "answering in Vietnamese despite the English request",
            matters: true,
        },
        Case {
            ask: "Sắp xếp cho tao chỗ ăn trưa gần văn phòng",
            memory: "Ăn chay từ tháng 1/2026, không ăn thịt và cá.",
            pinned: true,
            turns_on: "suggesting somewhere a vegetarian can actually eat",
            matters: true,
        },
        Case {
            ask: "Nhắc tao chuẩn bị gì cho buổi review thứ Sáu?",
            memory: "Buổi review thứ Sáu hàng tuần cần bản demo chạy được, không phải slide.",
            pinned: true,
            turns_on: "asking for a working demo, not a deck",
            matters: true,
        },
        Case {
            ask: "Pick a departure time for the Tokyo trip",
            memory: "Không bao giờ bay chuyến khởi hành trước 9h sáng.",
            pinned: true,
            turns_on: "not proposing a 06:00 departure",
            matters: true,
        },
        Case {
            ask: "Tao muốn mua laptop mới, gợi ý đi",
            memory: "Chỉ dùng macOS, không cân nhắc Windows hay Linux.",
            pinned: true,
            turns_on: "staying inside macOS rather than listing ThinkPads",
            matters: true,
        },
        Case {
            ask: "Viết caption cho bài đăng sản phẩm",
            memory: "Không dùng emoji trong bất cứ nội dung nào gửi ra ngoài.",
            pinned: true,
            turns_on: "a caption with no emoji in it",
            matters: true,
        },
        Case {
            ask: "Đặt bàn tối thứ Bảy cho 4 người",
            memory: "Vợ bị dị ứng hải sản nặng.",
            pinned: true,
            turns_on: "steering away from a seafood restaurant",
            matters: true,
        },
        Case {
            ask: "What time should I schedule the standup?",
            memory: "Team ở ba múi giờ: Hà Nội, Berlin, San Francisco.",
            pinned: true,
            turns_on: "finding an hour that works across all three",
            matters: true,
        },
        Case {
            ask: "Tao nên tập gì hôm nay?",
            memory: "Chấn thương đầu gối phải, tránh chạy bộ và squat nặng.",
            pinned: true,
            turns_on: "not prescribing running or heavy squats",
            matters: true,
        },
        Case {
            ask: "Gợi ý quà sinh nhật cho mẹ",
            memory: "Mẹ 68 tuổi, thích làm vườn, không dùng smartphone.",
            pinned: true,
            turns_on: "gardening rather than a gadget",
            matters: true,
        },
        Case {
            ask: "Đặt lịch nha sĩ cho tao tuần sau",
            memory: "Chỉ đi khám được vào sáng thứ Ba và sáng thứ Năm.",
            pinned: true,
            turns_on: "offering a Tuesday or Thursday morning",
            matters: true,
        },
        Case {
            ask: "Giải thích cho tao thuật toán Dijkstra",
            memory: "Là dân IT; giải thích kỹ thuật thì đi thẳng vào chi tiết, \
                     không cần ví dụ đời thường.",
            pinned: true,
            turns_on: "going technical instead of opening with a postman analogy",
            matters: true,
        },
        // ── Unpinned: they reach the model only if `recall` finds them ──
        Case {
            ask: "Tao nên đọc quyển nào tiếp theo?",
            memory: "Đang đọc dở quyển Sapiens, chưa xong chương 4.",
            pinned: false,
            turns_on: "recalling the unfinished book rather than inventing one",
            matters: true,
        },
        Case {
            ask: "Dự án Everest đang đến đâu rồi?",
            memory: "Dự án Everest bị hoãn đến quý 2 vì thiếu ngân sách.",
            pinned: false,
            turns_on: "knowing it slipped to Q2, not guessing",
            matters: true,
        },
        Case {
            ask: "Ghế của tao trên chuyến VN310 là ghế nào?",
            memory: "Trên chuyến VN310 tao ngồi ghế 14A.",
            pinned: false,
            turns_on: "answering 14A rather than saying it cannot know",
            matters: true,
        },
        Case {
            ask: "What did we decide about the Postgres migration?",
            memory: "Đã quyết hoãn migration sang Postgres, ở lại SQLite đến hết 2026.",
            pinned: false,
            turns_on: "reporting the decision to stay on SQLite",
            matters: true,
        },
        // ── Controls: the memory is right there, and the job is to ignore
        //    it. A `with` answer that differs here is the failure. ─────────
        Case {
            ask: "HTTP 429 nghĩa là gì?",
            memory: "Thích uống cà phê đen không đường.",
            pinned: true,
            turns_on: "explaining the status code without dragging coffee in",
            matters: false,
        },
        Case {
            ask: "Convert 15 kilometres to miles",
            memory: "Sinh nhật vào ngày 12 tháng 3.",
            pinned: true,
            turns_on: "just doing the arithmetic; the birthday is irrelevant",
            matters: false,
        },
    ];

    fn harness() -> (tempfile::TempDir, String, tauri::AppHandle<tauri::test::MockRuntime>) {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = std::fs::canonicalize(dir.path())
            .expect("canonical")
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
        (dir, vault, handle)
    }

    fn remember(
        handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        vault: &str,
        case: &Case,
    ) {
        let state = handle.state::<crate::db::DbState>();
        let ctx = crate::syn::tools::ToolContext {
            db: &state,
            vault_path: vault,
            app: handle,
            run_id: Some("eval"),
        };
        crate::syn::tools::execute_tool(
            &ctx,
            "remember",
            &serde_json::json!({ "body": case.memory, "pinned": case.pinned }),
        )
        .expect("remembers");
    }

    /// The table the gate's first criterion needs the deterministic half of.
    ///
    /// For each case: is the memory in the prompt Syn is sent, or reachable by
    /// the `recall` the prompt tells it to use? Anything answered "no" here is
    /// a question the model cannot get right for reasons that have nothing to
    /// do with the model.
    #[test]
    fn every_case_can_reach_its_memory() {
        let (_dir, vault, handle) = harness();
        for case in CASES {
            remember(&handle, &vault, case);
        }

        let memories = {
            let state = handle.state::<crate::db::DbState>();
            let db = state.lock().expect("lock");
            all(&db).expect("read")
        };
        assert_eq!(memories.len(), CASES.len(), "every memory was written");

        let block = memory_block(&memories, MEMORY_BUDGET_CHARS).expect("something is pinned");
        let prompt = crate::syn::prompt::PromptPlan::for_chat(crate::syn::prompt::ChatPrompt {
            context: "",
            personality: "auto",
            custom: None,
            memory: Some(&block),
            budget_chars: crate::syn::prompt::DEFAULT_BUDGET_CHARS,
        })
        .render();

        eprintln!("\n── can each question reach its memory? ──────────────────────");
        eprintln!("{:<52} {:>8} {:>9}", "question", "in prompt", "recallable");

        let state = handle.state::<crate::db::DbState>();
        let mut unreachable = Vec::new();

        for case in CASES {
            let in_prompt = prompt.contains(case.memory);

            // What the model would get by calling `recall` with the question.
            let ctx = crate::syn::tools::ToolContext {
                db: &state,
                vault_path: &vault,
                app: &handle,
                run_id: None,
            };
            let out = crate::syn::tools::execute_tool(
                &ctx,
                "recall",
                &serde_json::json!({ "query": case.ask }),
            )
            .expect("recalls");
            let recallable = out.contains(case.memory);

            eprintln!(
                "{:<52} {:>8} {:>9}",
                case.ask.chars().take(52).collect::<String>(),
                if in_prompt { "yes" } else { "—" },
                if recallable { "yes" } else { "—" },
            );
            if !in_prompt && !recallable {
                unreachable.push(case.ask);
            }
        }
        eprintln!();

        assert!(
            unreachable.is_empty(),
            "these questions cannot reach their memory by either route, so no model could \
             answer them from it: {unreachable:?}"
        );
    }

    /// A distinctive memory is findable however many pinned ones sit above it.
    ///
    /// The regression this guards is not hypothetical: `recall` filtered on
    /// "shares a substring with any query word", then sorted pinned-first and
    /// took six. Once a vault held six pinned memories, the six slots were
    /// always theirs — and they are the memories already riding in every
    /// prompt, so `recall` spent its whole budget handing back what the model
    /// had. `every_case_can_reach_its_memory` caught it on the one question
    /// naming the rarest word in the set, "Everest".
    #[test]
    fn a_rare_word_is_found_past_a_crowd_of_pinned_memories() {
        let (_dir, vault, handle) = harness();

        let noise = Case {
            ask: "",
            memory: "",
            pinned: true,
            turns_on: "",
            matters: true,
        };
        // Noise that genuinely competes: every one shares "dự" and "án" with
        // the question, so the crowd survives the match and the order is what
        // decides. Noise sharing nothing would be filtered out before ranking
        // ever ran, and the test would pass against the very bug it names.
        for body in [
            "Dự án Alpha đã bàn giao xong.",
            "Dự án Beta đang chờ duyệt ngân sách.",
            "Dự án Gamma do Minh phụ trách.",
            "Dự án Delta dùng Postgres.",
            "Dự án Epsilon hoãn vô thời hạn.",
            "Dự án Zeta cần demo hàng tuần.",
            "Dự án Eta đã đóng.",
            "Dự án Theta đang tuyển người.",
        ] {
            remember(&handle, &vault, &Case { memory: body, ..noise });
        }
        remember(
            &handle,
            &vault,
            &Case {
                memory: "Dự án Everest bị hoãn đến quý 2 vì thiếu ngân sách.",
                pinned: false,
                ..noise
            },
        );

        let state = handle.state::<crate::db::DbState>();
        let ctx = crate::syn::tools::ToolContext {
            db: &state,
            vault_path: &vault,
            app: &handle,
            run_id: None,
        };
        let out = crate::syn::tools::execute_tool(
            &ctx,
            "recall",
            &serde_json::json!({ "query": "Dự án Everest đang đến đâu rồi?" }),
        )
        .expect("recalls");

        assert!(
            out.contains("Everest"),
            "the memory naming the asked-about project must come back even though \
             eight pinned memories were written after it: {out}"
        );
    }

    /// Every memory rides in every prompt; pinning only decides who goes first.
    ///
    /// This test formerly asserted the reverse — that an unpinned memory stays
    /// out of the prompt until `recall` fetches it — and the assertion was
    /// true, exact, and describing the defect. `docs/adr-memory-shape-2026-09-04.md`
    /// records what it cost: `recall` was called zero times in fifteen real
    /// runs, so "waits to be recalled" meant "never arrives".
    ///
    /// The rewrite is the contract. If it is hard to state, the design is wrong.
    #[test]
    fn every_memory_rides_in_every_prompt_and_pinned_ones_are_cut_last() {
        let (_dir, vault, handle) = harness();
        for case in CASES {
            remember(&handle, &vault, case);
        }

        let memories = {
            let state = handle.state::<crate::db::DbState>();
            let db = state.lock().expect("lock");
            all(&db).expect("read")
        };
        let block = memory_block(&memories, MEMORY_BUDGET_CHARS).expect("there are memories");

        for case in CASES {
            assert!(
                block.contains(case.memory),
                "`{}` is in the prompt whether or not it is pinned",
                case.memory
            );
        }

        // And when the budget bites, pinning is what decides. One line short of
        // holding everything: the casualty is an unpinned memory.
        let shortest_unpinned = memories
            .iter()
            .filter(|m| !m.pinned)
            .min_by_key(|m| m.line().chars().count())
            .expect("the case list has unpinned entries");
        let budget = block.chars().count() - (shortest_unpinned.line().chars().count() + 1);
        let tight = memory_block(&memories, budget).expect("most still fit");

        let cut: Vec<&Memory> = memories
            .iter()
            .filter(|m| !tight.contains(m.body.as_str()))
            .collect();
        assert!(!cut.is_empty(), "something had to go at this budget");
        assert!(
            cut.iter().all(|m| !m.pinned),
            "only unpinned memories are cut while any remain: {:?}",
            cut.iter().map(|m| &m.body).collect::<Vec<_>>()
        );
    }
}


/// The model half of the gate: does remembering change the answer?
///
/// `#[ignore]`d for the reasons the RAG A/B is — it spends real API credit,
/// needs a network, and is not deterministic. CI must never run it.
///
/// ```bash
/// cargo test --lib answers_with_and_without -- --ignored --nocapture
/// ```
///
/// It prints both answers side by side and leaves the judgement to a person.
/// No score: most of these turn on something no substring knows — "did it push
/// back rather than book 17:00" — and the lesson of
/// `docs/adr-rag-vs-agentic-2026-09-03.md` is that a scorer which is
/// confidently wrong does more damage than no scorer, because it is wrong with
/// a number attached. Four separate defects in that one were each found by
/// reading an answer marked wrong and disagreeing with the mark.
#[cfg(test)]
mod memory_changes_the_answer {
    use super::does_memory_reach_the_model::CASES;
    use super::*;
    use crate::db::DbBridge;
    use crate::models::syn::SynProvider;
    use crate::syn::provider::{ChatMessage, ChatProvider, ChatRequest};
    use tauri::Manager;

    #[tokio::test]
    #[ignore = "spends real API credit and needs a network; run by hand"]
    async fn answers_with_and_without_what_syn_remembers() {
        let settings = crate::syn::settings::load_settings(
            &std::env::var("SYN_EVAL_VAULT").unwrap_or_else(|_| {
                format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default())
            }),
        )
        .expect("the real Syn settings");
        let model = settings
            .default_model
            .clone()
            .expect("a default model must be configured");

        let build = || -> Box<dyn ChatProvider> {
            match settings.provider {
                SynProvider::Ollama => Box::new(
                    crate::syn::provider::ollama::OllamaProvider::new(&settings.ollama_url),
                ),
                SynProvider::OpenAiCompat => Box::new(
                    crate::syn::provider::openai::OpenAiCompatProvider::new(
                        &settings.openai_base_url,
                        crate::secrets::SecretManager::get_syn_api_key(None, "openai_compat"),
                        settings.openai_reasoning_effort.clone(),
                    ),
                ),
            }
        };

        // Several runs per arm, because one cannot tell a change from a
        // coin toss. Three runs of the twenty cases caught the case that came
        // out fail, then win, then worse while nothing about it changed — and
        // that only showed up because the whole eval happened to be run three
        // times for other reasons. Repetition is now the default, not luck.
        let runs: usize = std::env::var("SYN_EVAL_RUNS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(3);

        eprintln!("\n═══ with and without what Syn remembers ═══════");
        eprintln!("provider {:?}   model {model}", settings.provider);
        eprintln!(
            "{runs} run(s) per arm — {} API calls. SYN_EVAL_RUNS changes it.\n",
            CASES.len() * 2 * runs
        );

        for case in CASES {
            let dir = tempfile::tempdir().expect("temp vault");
            let vault = std::fs::canonicalize(dir.path())
                .expect("canonical")
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

            {
                let state = handle.state::<crate::db::DbState>();
                let ctx = crate::syn::tools::ToolContext {
                    db: &state,
                    vault_path: &vault,
                    app: &handle,
                    run_id: Some("eval"),
                };
                crate::syn::tools::execute_tool(
                    &ctx,
                    "remember",
                    &serde_json::json!({ "body": case.memory, "pinned": case.pinned }),
                )
                .expect("remembers");
            }

            // One route now. Everything remembered rides in the prompt, so the
            // pinned/unpinned split this eval was built around no longer names
            // two different journeys — which is the change §1 of the ADR made,
            // and measuring the old split would measure a world that is gone.
            //
            // `case.pinned` is still honoured when writing, because it still
            // decides what survives a cut. At one memory per case it decides
            // nothing, which is the point.
            let block = {
                let memories = {
                    let state = handle.state::<crate::db::DbState>();
                    let db = state.lock().expect("lock");
                    all(&db).expect("read")
                };
                memory_block(&memories, MEMORY_BUDGET_CHARS)
            };

            eprintln!("── {}", case.ask);
            eprintln!(
                "   remembers: {}  [{}, {}]",
                case.memory,
                if case.pinned { "pinned" } else { "not pinned" },
                if case.matters { "should change the answer" } else { "CONTROL: should not" },
            );
            eprintln!("   turns on:  {}\n", case.turns_on);

            for (label, memory) in [("without", None), ("with", block.as_deref())] {
                let mut answers: Vec<String> = Vec::new();
                let system = crate::syn::prompt::PromptPlan::for_chat(
                    crate::syn::prompt::ChatPrompt {
                        context: "",
                        personality: &settings.personality,
                        custom: None,
                        memory,
                        budget_chars: crate::syn::prompt::DEFAULT_BUDGET_CHARS,
                    },
                )
                .render();

                let messages = vec![
                    ChatMessage::new("system", system),
                    ChatMessage::new("user", case.ask),
                ];
                for _ in 0..runs {
                    let text = match build()
                        .chat(ChatRequest {
                            model: &model,
                            messages: &messages,
                            temperature: Some(settings.temperature),
                            num_ctx: settings.num_ctx,
                            tools: None,
                        })
                        .await
                    {
                        Ok(r) => r.content,
                        Err(e) => format!("<error: {e}>"),
                    };
                    eprintln!("   [{label:>7}] {}\n", text.replace('\n', "\n             "));
                    answers.push(text);
                }

                // A stability line, and deliberately not a score. It says how
                // much this arm moved on its own; it says nothing about whether
                // any of it was good, because the lesson of
                // `docs/adr-rag-vs-agentic-2026-09-03.md` is that a scorer
                // which is confidently wrong does more damage than none.
                if runs > 1 {
                    let mut distinct: Vec<&String> = answers.iter().collect();
                    distinct.sort();
                    distinct.dedup();
                    let lengths: Vec<usize> = answers.iter().map(|a| a.chars().count()).collect();
                    eprintln!(
                        "   {label:>7}: {} of {runs} answers distinct, {}–{} chars\n",
                        distinct.len(),
                        lengths.iter().min().copied().unwrap_or(0),
                        lengths.iter().max().copied().unwrap_or(0),
                    );
                }
            }
        }

        eprintln!(
            "── Read the runs, not the pair. The gate asks whether `with` is better on \
             several cases and worse on none — and a case whose own arm disagrees with \
             itself across runs cannot answer either half. Judge a case only where the \
             runs within an arm agree; where they do not, the case is telling you about \
             the model's temperature and not about memory. ──\n"
        );
    }
}
