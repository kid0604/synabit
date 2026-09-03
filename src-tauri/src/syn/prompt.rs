//! What Syn is told, as named parts with a budget rather than one `format!`.
//!
//! # Why this is not a string any more
//!
//! The system prompt was a single `format!` in `rag.rs`, and it was a good one
//! — it teaches the *shape* of the vault rather than listing the tools, so it
//! does not go stale every time the tool list changes. What it could not do is
//! grow. Three things are queued behind it: remembered facts, an index of
//! skills, and whatever a later phase needs to say. Each of those is variable
//! in length, each competes for the same context window, and a `format!` has no
//! way to express that competition or to report on it.
//!
//! So the prompt is a list of sections now. Each one knows its own name and
//! whether it can be dropped; the plan knows the budget and what it dropped to
//! stay inside it. Rendering is concatenation, in order, and nothing else.
//!
//! # It renders exactly what it used to
//!
//! Every literal here was moved, not rewritten, and
//! `rag::tests::the_system_prompt_matches_its_snapshot` compares the result
//! byte for byte against text captured before the move. A refactor that changes
//! what the assistant is told while claiming to move code is the specific
//! failure that test exists to prevent.
//!
//! # Budgeting in characters
//!
//! Characters, not tokens, because nothing here has a tokenizer and pulling one
//! in to divide by four is not worth the dependency. Characters are counted
//! exactly and tokens are reported as an estimate at four characters each,
//! which is within about 15% for English and worse for Vietnamese — good enough
//! to decide what to drop, and labelled as an estimate everywhere it is shown
//! so nobody reads it as a measurement.

use serde::Serialize;

/// Characters per token, for the estimate shown alongside the exact count.
const CHARS_PER_TOKEN: usize = 4;

/// What the fixed sections cost, measured rather than guessed.
///
/// Identity, personality, rules, today and tool shape came to 5,190 characters
/// when this was written — the snapshots in `testdata/` are the measurement,
/// and `the_fixed_sections_still_cost_what_the_budget_assumes` fails if they
/// drift far from it. Rounded up, because the number is a premise for the
/// budget below and not a fact about any particular day.
const FIXED_SECTIONS_CHARS: usize = 5_500;

/// What retrieval is allowed to add, at the default in `SynSettings`.
///
/// A user who raises `max_context_chars` raises what the prompt costs without
/// raising this, and the breakdown will show the budget being exceeded. That is
/// the correct behaviour: they asked for more context than the budget was set
/// for, and the panel says so rather than silently cutting what they asked for.
const DEFAULT_CONTEXT_CHARS: usize = 12_000;

/// Room kept for sections that do not exist yet.
///
/// Remembered facts and an index of skills are the next two things that will
/// want space here, and both are variable in length. Reserving for them now is
/// what makes the breakdown honest before they arrive: a budget that exactly
/// fits what is already there would show every prompt as 100% full and say
/// nothing about whether there is room for more.
const HEADROOM_CHARS: usize = 8_000;

/// The default ceiling on the whole system prompt.
///
/// Roughly 6,300 tokens by the four-characters-each estimate — which is worth
/// reading against the default Ollama context window of 8,192. The system
/// prompt alone can take three quarters of it before the conversation has said
/// anything, and that is the strongest argument in favour of sending less of
/// it to a small local model. See
/// `docs/adr-rag-vs-agentic-2026-09-03.md`.
pub const DEFAULT_BUDGET_CHARS: usize =
    FIXED_SECTIONS_CHARS + DEFAULT_CONTEXT_CHARS + HEADROOM_CHARS;

// ═══════════════════════════════════════════════════════════════
//  THE PARTS
// ═══════════════════════════════════════════════════════════════

/// Which part of the prompt a section is.
///
/// Ordering of the enum is the ordering in the prompt, and `for_chat` builds
/// them in this order — `Custom` first because that is where the user's own
/// instructions went when they were prepended by the caller, and moving them
/// would change a prompt somebody has already tuned.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// `custom_system_prompt` from the vault's settings.
    Custom,
    /// Who Syn is and what Synabit is.
    Identity,
    /// Which language and register to answer in.
    Personality,
    /// How to cite, what not to fabricate, how to draw a chart.
    Rules,
    /// Today's date, which the model cannot know.
    Today,
    /// What the vault is shaped like and which tool reaches what.
    ToolShape,
    /// Chunks retrieved for this question.
    VaultContext,
}

impl SectionKind {
    /// A human-readable name, for the breakdown the user is shown.
    pub fn label(self) -> &'static str {
        match self {
            SectionKind::Custom => "Your own instructions",
            SectionKind::Identity => "Identity",
            SectionKind::Personality => "Personality",
            SectionKind::Rules => "Rules",
            SectionKind::Today => "Today",
            SectionKind::ToolShape => "Tools and vault shape",
            SectionKind::VaultContext => "Retrieved context",
        }
    }

    /// Whether dropping this section would change who the assistant is, rather
    /// than how much it knows going in.
    ///
    /// Only the retrieved context is droppable, and only because it is
    /// explicitly a sample the model is told to search past. Everything else
    /// either defines the assistant or is something it cannot recover by
    /// looking — the date most of all.
    pub fn is_required(self) -> bool {
        !matches!(self, SectionKind::VaultContext)
    }
}

/// One part of the prompt, already rendered.
#[derive(Debug, Clone)]
pub struct Section {
    pub kind: SectionKind,
    pub body: String,
}

/// What a section cost, for the screen that shows where the window went.
#[derive(Serialize, Debug, Clone)]
pub struct SectionCost {
    pub kind: SectionKind,
    pub label: &'static str,
    pub chars: usize,
    /// Characters divided by four. An estimate, and named as one.
    pub est_tokens: usize,
    /// True when this section was left out to stay inside the budget.
    pub dropped: bool,
}

// ═══════════════════════════════════════════════════════════════
//  THE LITERALS
// ═══════════════════════════════════════════════════════════════
//
//  Moved verbatim from `rag::build_system_prompt`. Do not reflow them: the
//  snapshot test compares the rendered result byte for byte, and a stray
//  newline here is a failing test rather than a silent change, which is the
//  point.

fn identity() -> &'static str {
    IDENTITY
}

fn rules() -> &'static str {
    RULES
}

/// What the tools are and what the vault is shaped like.
///
/// Named for the shape rather than the tools because that is what it teaches.
/// The list of tool *names* is sent separately, as definitions; this is the
/// part that says a book and a task are the same kind of thing.
fn tool_shape() -> &'static str {
    TOOL_SHAPE
}

/// Which language and register to answer in.
///
/// `auto` is the default and the fallback: an unrecognised value adapts to the
/// user rather than picking one of the two Vietnamese registers on their behalf.
fn personality_instructions(personality: &str) -> &'static str {
    match personality {
        "casual" => PERSONALITY_CASUAL,
        "professional" => PERSONALITY_PROFESSIONAL,
        _ => PERSONALITY_AUTO,
    }
}

/// Today, as the machine's own clock reads it.
///
/// Local rather than UTC, because the question "what is due today" is asked
/// about the day the user is having.
fn today() -> String {
    let now = chrono::Local::now();
    format!(
        "- Today's date: {} ({})\n\n",
        now.format("%Y-%m-%d"),
        now.format("%A")
    )
}

/// Retrieved chunks, wrapped in the instructions about how to read them.
///
/// Empty context renders as nothing at all, not as an empty section with a
/// heading — a heading saying there is context, above no context, is worse than
/// silence.
fn vault_context(context: &str) -> String {
    if context.is_empty() {
        return String::new();
    }
    format!("{}{}{}", CONTEXT_PREFIX, context, CONTEXT_SUFFIX)
}

const IDENTITY: &str = r#"You are Syn, a personal AI assistant embedded in the Synabit productivity app. Synabit is a second-brain/productivity tool that stores notes, tasks, events, contacts, files, RSS feeds, and financial records.

"#;

const PERSONALITY_AUTO: &str = r#"Match the user's language and communication style. If they write in Vietnamese, respond in Vietnamese. If they write in English, respond in English. If they use casual language (tao/mày), be casual back. If they are formal, be formal.

"#;

const PERSONALITY_CASUAL: &str = r#"Respond in Vietnamese with a casual, friendly tone. Use informal pronouns (tao/mày) when the user does. Be witty and conversational, like a close friend.

"#;

const PERSONALITY_PROFESSIONAL: &str = r#"Respond in Vietnamese with a professional, polite tone. Use formal pronouns (tôi/bạn). Be clear, structured, and respectful.

"#;

const RULES: &str = r#"Key rules:
- When referencing vault data, ALWAYS use [[Title]] notation with the HUMAN-READABLE TITLE (not the file path or ID). Example: 'I found [[Ghi chú họp team]] which mentions...' WRONG: [[Notes/22440d7a-84c5-433b-982c-04b906591253.md]] — NEVER use file paths in links. RIGHT: [[Ghi chú họp team]] — always use the note/task/event title.
- If information is not in the provided context, say so honestly — do not fabricate.
- Keep responses concise and actionable.
- You can see the user's notes, tasks, events, contacts, feeds, and finances.
- For tasks and events, pay attention to dates, priorities, and statuses.
- CHARTS: You can render charts using Mermaid syntax in code blocks. When the user asks for charts, graphs, or data visualization, output a fenced code block with language 'mermaid'. Supported types: pie, xychart-beta (bar charts), flowchart, sequence, gantt, timeline. Example for spending breakdown:
```mermaid
pie title Monthly Spending
"Food" : 45
"Transport" : 20
"Bills" : 35
```
For bar charts use xychart-beta:
```mermaid
xychart-beta
title "Income vs Expense"
x-axis ["Jan", "Feb", "Mar"]
y-axis "Amount" 0 --> 5000000
bar [1000000, 2000000, 1500000]
bar [800000, 1500000, 1200000]
```
"#;

const TOOL_SHAPE: &str = r#"Tool usage guidelines:
- You have tools. USE THEM rather than guessing or answering from memory when the request involves finding, listing, creating or changing the user's data.
- Almost everything in this vault is a node: notes, tasks, events, people, projects, and any type this user invented. `query_nodes` finds them and `get_node` reads one in full.
- If you do not know what the user keeps, or are unsure a type or field exists, call `list_schemas` first. It tells you every type in this vault and the fields each one actually uses. Do this before inventing a field name.
- Query syntax: `type:task status:todo sort:due_date`, `type:book rating:>3`, `#work due_date:<2026-09-01`, plus free words for full-text search. `limit:` caps results; check `total_matches` before saying how many there are.
- To create anything: `create_node` with the type, title and fields. Match the field names `list_schemas` reports for that type.
- To change anything — mark a task done, set a due date, add a tag: `update_node`. Send only the fields that change; everything else is kept. Find the node with `query_nodes` first to get its id.
- `get_linked_nodes` follows links out of and into a node. Use it for 'what else is related to this', which no query can express.
- To remove anything: `trash_node`. It goes to the vault's trash, not gone — `list_trash` shows what is there and `restore_node` puts one back. Removing several things is one call each. Never say you cannot delete.
- Every save is kept. `list_versions` shows how a node looked before, and `restore_version` puts it back. Reach for these when the user says an edit was wrong, including one you just made.
- To change the SHAPE of a type rather than one node — rename a field on every task, remove a field everywhere, rename or remove a whole type: `rename_field`, `delete_field`, `rename_kind`, `delete_kind`. These touch many files at once, so each one works in two steps: call it WITHOUT `confirm_nodes` to get the count, tell the user what it will affect, then call again passing that exact number. A user who wants a type gone but made it by accident usually wants `rename_kind`, which keeps everything they wrote — offer that before `delete_kind`.
- For files, images, documents or PDFs: `search_files`. It searches inside documents as well as filenames. Example: "tìm ảnh", "find PDFs". To read what a document actually says, `read_file_text` — `get_node` gives you only the vault's record of the file, not its contents.
- For articles from RSS feeds: `search_feed_articles`, and `update_feed_article` to mark one read, starred or read-later. These are not nodes.
- `list_schemas` also reports `app_storage`. That is Synabit's own bookkeeping — never create or edit those, and never count them when telling the user what the vault holds.
- FINANCE is the exception to all of the above: transactions live inside a month node as a list, not as nodes of their own, so the generic tools cannot reach them.
1. Call `get_finance_summary` FIRST to learn the real accounts and categories.
2. Then `create_transaction` with the amount, category and account.
3. Example: "nay đi chợ hết 150k" → create_transaction(amount=150000, category="Food & Dining", note="Đi chợ").
4. To review history, `get_transactions` with the month parameter.
- ALWAYS confirm what you created or changed, with the details from the result.
- Do NOT reply with text alone when a tool can give a concrete answer.
- Call tools FIRST, then summarize the results for the user.
"#;

const CONTEXT_PREFIX: &str = r#"

=== VAULT CONTEXT ===
A few things from the user's vault that looked relevant to this question. They are a starting point, not the answer, and this is a sample rather than everything that matches.
- If what you need is here, use it and do not search again.
- If the question asks how many, how much, or anything else that has to be counted or added up, this cannot answer it. Use `query_nodes` and read `total_matches`.
- If nothing here answers the question, search rather than saying you could not find anything.

"#;

const CONTEXT_SUFFIX: &str = r#"=== END CONTEXT ==="#;

// ═══════════════════════════════════════════════════════════════
//  THE PLAN
// ═══════════════════════════════════════════════════════════════

/// The sections that make up one system prompt, and what they cost.
#[derive(Debug, Clone)]
pub struct PromptPlan {
    sections: Vec<Section>,
    /// Sections left out to stay inside the budget, in the order they were cut.
    dropped: Vec<SectionKind>,
    budget_chars: usize,
}

impl PromptPlan {
    /// The prompt for one turn of a chat.
    ///
    /// `custom` is the user's own instructions from settings. It goes first,
    /// which is where the caller used to put it — `format!("{custom}\n\n{prompt}")`
    /// in `syn_send_message`. That composition lives here now, so there is one
    /// place that knows what the prompt is made of.
    pub fn for_chat(
        context: &str,
        personality: &str,
        custom: Option<&str>,
        budget_chars: usize,
    ) -> Self {
        let mut sections = Vec::new();

        if let Some(custom) = custom.map(str::trim).filter(|c| !c.is_empty()) {
            sections.push(Section {
                kind: SectionKind::Custom,
                body: format!("{custom}\n\n"),
            });
        }
        sections.push(Section { kind: SectionKind::Identity, body: identity().to_string() });
        sections.push(Section {
            kind: SectionKind::Personality,
            body: personality_instructions(personality).to_string(),
        });
        sections.push(Section { kind: SectionKind::Rules, body: rules().to_string() });
        sections.push(Section { kind: SectionKind::Today, body: today() });
        sections.push(Section { kind: SectionKind::ToolShape, body: tool_shape().to_string() });

        let ctx = vault_context(context);
        if !ctx.is_empty() {
            sections.push(Section { kind: SectionKind::VaultContext, body: ctx });
        }

        let mut plan = Self { sections, dropped: Vec::new(), budget_chars };
        plan.fit();
        plan
    }

    /// Drop optional sections, largest first, until the whole thing fits.
    ///
    /// Largest first rather than lowest priority because there is only one
    /// droppable kind today, and when there are several the useful question is
    /// which one buys back the most room. A plan that still does not fit after
    /// dropping everything optional is rendered over budget rather than
    /// mutilated: cutting the rules in half to hit a number would produce an
    /// assistant that is confidently wrong about how to cite a note.
    fn fit(&mut self) {
        while self.chars() > self.budget_chars {
            let biggest = self
                .sections
                .iter()
                .enumerate()
                .filter(|(_, s)| !s.kind.is_required())
                .max_by_key(|(_, s)| s.body.chars().count())
                .map(|(i, s)| (i, s.kind));

            match biggest {
                Some((index, kind)) => {
                    self.sections.remove(index);
                    self.dropped.push(kind);
                }
                None => {
                    log::warn!(
                        "[Syn] The system prompt is {} characters against a budget of {}, \
                         and everything left is required. Sending it anyway.",
                        self.chars(),
                        self.budget_chars
                    );
                    return;
                }
            }
        }
    }

    /// The prompt, as the model receives it.
    pub fn render(&self) -> String {
        self.sections.iter().map(|s| s.body.as_str()).collect()
    }

    pub fn chars(&self) -> usize {
        self.sections.iter().map(|s| s.body.chars().count()).sum()
    }

    pub fn budget_chars(&self) -> usize {
        self.budget_chars
    }

    /// An estimate of the whole prompt in tokens. See the module comment.
    pub fn est_tokens(&self) -> usize {
        self.chars() / CHARS_PER_TOKEN
    }

    /// What each section cost, in the order they appear, with anything dropped
    /// listed after — so a user reading it sees both what was sent and what was
    /// left out.
    pub fn breakdown(&self) -> Vec<SectionCost> {
        let mut costs: Vec<SectionCost> = self
            .sections
            .iter()
            .map(|s| {
                let chars = s.body.chars().count();
                SectionCost {
                    kind: s.kind,
                    label: s.kind.label(),
                    chars,
                    est_tokens: chars / CHARS_PER_TOKEN,
                    dropped: false,
                }
            })
            .collect();

        costs.extend(self.dropped.iter().map(|kind| SectionCost {
            kind: *kind,
            label: kind.label(),
            chars: 0,
            est_tokens: 0,
            dropped: true,
        }));

        costs
    }
}

/// A whole prompt and where its room went, for the screen that shows it.
///
/// The point of shipping this in the same phase as the plan: a prompt assembled
/// from parts is a prompt somebody has to be able to look at. Once a section is
/// filled by a skill or a remembered fact, "why did Syn do that" is answered by
/// reading what it was actually told, and there was previously no way to.
#[derive(Serialize, Debug, Clone)]
pub struct PromptPreview {
    /// The prompt, verbatim.
    pub text: String,
    pub chars: usize,
    pub est_tokens: usize,
    pub budget_chars: usize,
    pub sections: Vec<SectionCost>,
}

impl From<PromptPlan> for PromptPreview {
    fn from(plan: PromptPlan) -> Self {
        Self {
            text: plan.render(),
            chars: plan.chars(),
            est_tokens: plan.est_tokens(),
            budget_chars: plan.budget_chars(),
            sections: plan.breakdown(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The budget is built on a measurement, and this is the measurement.
    ///
    /// `FIXED_SECTIONS_CHARS` is a premise, not a preference: it says what the
    /// unavoidable part of the prompt costs, and everything else in the budget
    /// is reasoned from it. Someone who adds two thousand characters of rules
    /// has changed that premise, and should have to notice.
    #[test]
    fn the_fixed_sections_still_cost_what_the_budget_assumes() {
        let fixed = PromptPlan::for_chat("", "auto", None, DEFAULT_BUDGET_CHARS).chars();

        assert!(
            fixed <= FIXED_SECTIONS_CHARS,
            "the fixed sections now cost {fixed} characters against a budget premise of \
             {FIXED_SECTIONS_CHARS}. Raise FIXED_SECTIONS_CHARS deliberately, and read what \
             that does to DEFAULT_BUDGET_CHARS against an 8,192-token context window."
        );
        assert!(
            fixed > FIXED_SECTIONS_CHARS / 2,
            "the fixed sections cost {fixed} characters, far under the {FIXED_SECTIONS_CHARS} \
             the budget is built on. If half the prompt has gone, that is either a very good \
             change or an accident, and either way the budget should be recomputed."
        );
    }

    /// The whole prompt, byte for byte, against text captured before it was
    /// broken into sections.
    ///
    /// This is the test that made the split safe. `PromptPlan` moved seven
    /// literals out of one `format!` in `rag.rs`, and the failure mode of that
    /// kind of move — a lost newline, a reflowed line, a section in the wrong
    /// order — is invisible in review and changes what the assistant is told.
    ///
    /// The date line is masked; it is the one part that differs on every run.
    ///
    /// Regenerate deliberately, by deleting `src/syn/testdata/` and running
    /// this once. Anything that changes those files is changing Syn's
    /// behaviour, and the commit should say so.
    #[test]
    fn the_system_prompt_matches_its_snapshot() {
        let today = regex::Regex::new(r"- Today's date: [^\n]*").expect("valid");

        for personality in ["auto", "casual", "professional"] {
            for (label, context) in [("bare", ""), ("with-context", "some context")] {
                let rendered =
                    PromptPlan::for_chat(context, personality, None, DEFAULT_BUDGET_CHARS).render();
                let masked = today.replace_all(&rendered, "- Today's date: <DATE>");

                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/syn/testdata")
                    .join(format!("system-prompt.{personality}.{label}.txt"));

                // Re-blessing is deliberate and leaves a trace in the shell
                // history that made it happen:
                //
                //   SYN_BLESS_SNAPSHOTS=1 cargo test --lib the_system_prompt
                //
                // Deliberately not "write the file if it is absent", which is
                // what this did before. That blesses silently on a fresh
                // checkout, so the one run where the snapshot could have caught
                // a bad change is the run where it writes it down as correct.
                if std::env::var_os("SYN_BLESS_SNAPSHOTS").is_some() {
                    std::fs::create_dir_all(path.parent().expect("has a parent"))
                        .expect("testdata dir");
                    std::fs::write(&path, masked.as_ref()).expect("write snapshot");
                    eprintln!("blessed {}", path.display());
                    continue;
                }

                let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                    panic!(
                        "snapshot {} is missing ({e}). If the prompt changed on purpose, \
                         re-run with SYN_BLESS_SNAPSHOTS=1 and say so in the commit message.",
                        path.display()
                    )
                });

                assert_eq!(
                    masked.as_ref(),
                    expected,
                    "the system prompt for {personality}/{label} no longer matches {}",
                    path.display()
                );
            }
        }
    }

    /// The composition the caller used to do by hand: custom instructions,
    /// blank line, then the prompt. Somebody has tuned a prompt against that
    /// ordering, and moving it here must not move it on the page.
    #[test]
    fn a_custom_prompt_comes_first_and_is_followed_by_a_blank_line() {
        let plan = PromptPlan::for_chat("", "auto", Some("Always answer in haiku."), DEFAULT_BUDGET_CHARS);
        let rendered = plan.render();
        assert!(rendered.starts_with("Always answer in haiku.\n\nYou are Syn,"));
    }

    /// A settings file with an empty string in it is a settings file with no
    /// custom prompt. Rendering it would put two blank lines at the top of
    /// every prompt for no reason.
    #[test]
    fn an_empty_custom_prompt_adds_no_section() {
        for empty in [Some(""), Some("   "), None] {
            let plan = PromptPlan::for_chat("", "auto", empty, DEFAULT_BUDGET_CHARS);
            assert!(plan.render().starts_with("You are Syn,"), "{empty:?}");
            assert!(!plan.breakdown().iter().any(|c| c.kind == SectionKind::Custom));
        }
    }

    #[test]
    fn no_context_means_no_context_section() {
        let plan = PromptPlan::for_chat("", "auto", None, DEFAULT_BUDGET_CHARS);
        assert!(!plan.render().contains("VAULT CONTEXT"));
        assert!(!plan.breakdown().iter().any(|c| c.kind == SectionKind::VaultContext));
    }

    #[test]
    fn context_is_wrapped_in_the_instructions_for_reading_it() {
        let plan = PromptPlan::for_chat("a note about ducks", "auto", None, DEFAULT_BUDGET_CHARS);
        let rendered = plan.render();
        assert!(rendered.contains("=== VAULT CONTEXT ==="));
        assert!(rendered.contains("a note about ducks"));
        assert!(rendered.trim_end().ends_with("=== END CONTEXT ==="));
    }

    /// An unknown personality adapts rather than picking a Vietnamese register
    /// on the user's behalf.
    #[test]
    fn an_unrecognised_personality_falls_back_to_adapting() {
        let plan = PromptPlan::for_chat("", "klingon", None, DEFAULT_BUDGET_CHARS);
        assert!(plan.render().contains("Match the user's language"));
    }

    #[test]
    fn the_breakdown_accounts_for_every_character_that_was_sent() {
        let plan = PromptPlan::for_chat("ctx", "casual", Some("be brief"), DEFAULT_BUDGET_CHARS);
        let counted: usize = plan.breakdown().iter().filter(|c| !c.dropped).map(|c| c.chars).sum();
        assert_eq!(counted, plan.render().chars().count());
        assert_eq!(counted, plan.chars());
    }

    /// The budget takes the retrieved context, which the model is told to
    /// search past, and never the rules.
    #[test]
    fn a_tight_budget_drops_context_and_keeps_the_rules() {
        let plan = PromptPlan::for_chat(&"x".repeat(5000), "auto", None, 6000);
        let rendered = plan.render();
        assert!(!rendered.contains("VAULT CONTEXT"));
        assert!(rendered.contains("Key rules:"));
        assert!(rendered.contains("Today's date"));
        assert!(plan
            .breakdown()
            .iter()
            .any(|c| c.kind == SectionKind::VaultContext && c.dropped));
    }

    /// Nothing required is ever cut. A budget smaller than the fixed parts is a
    /// misconfiguration, and the honest response is to go over it rather than
    /// to send an assistant that has forgotten how to cite a note.
    #[test]
    fn an_impossible_budget_goes_over_rather_than_cutting_what_matters() {
        let plan = PromptPlan::for_chat("ctx", "auto", None, 10);
        let rendered = plan.render();
        assert!(rendered.contains("Key rules:"));
        assert!(rendered.contains("Tool usage guidelines:"));
        assert!(plan.chars() > plan.budget_chars());
    }

    /// Vietnamese is where a byte-counting mistake would show up first.
    #[test]
    fn costs_are_counted_in_characters_not_bytes() {
        let plan = PromptPlan::for_chat("", "auto", Some("đường"), DEFAULT_BUDGET_CHARS);
        let custom = plan
            .breakdown()
            .into_iter()
            .find(|c| c.kind == SectionKind::Custom)
            .expect("the custom section is there");
        assert_eq!(custom.chars, "đường\n\n".chars().count());
    }
}
