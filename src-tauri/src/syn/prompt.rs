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
///
/// Raised again to 6,500 when the prompt learned to say what Syn can and
/// cannot do about the internet — 6,233 measured. That line exists because
/// leaving `web_search` out when no endpoint is configured was right and left
/// the model unable to explain itself: asked for a football score it said *"I
/// have no live web data"*, which is vague and slightly false. About 85
/// estimated tokens a turn buys an answer somebody can act on instead. See
/// `web_line`.
///
/// Raised from 5,500 to 6,000 when `footing::RULE` joined the rules section:
/// the fixed sections went to 5,887. That rule costs about 175 estimated tokens
/// on **every** turn, which is the honest price of Syn saying out loud when it
/// has not looked anything up, and of it disagreeing once instead of never. The
/// knock-on is `DEFAULT_BUDGET_CHARS` at 26,000 — roughly 6,500 estimated
/// tokens against Ollama's default 8,192 window, so about four fifths of it
/// before the conversation has said a word. That number was already the
/// strongest argument for sending a small local model less, and this makes it
/// slightly stronger rather than changing it in kind.
const FIXED_SECTIONS_CHARS: usize = 6_500;

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
    /// How to cite, what not to fabricate, how to draw a chart.
    Rules,
    /// Today's date, which the model cannot know.
    Today,
    /// Which screen the user is on and what they have highlighted.
    ///
    /// Beside `Today` because it is the same kind of fact: true right now,
    /// unknowable from any tool, and carried with the question rather than
    /// fetched. See `focus.rs`.
    Focus,
    /// A count run before the model was asked, for a question that was one.
    ///
    /// Required rather than droppable, and not folded into `VaultContext`: that
    /// section is wrapped in instructions calling it *a sample you may need to
    /// search past*, which is the opposite of what an exact total is. Dropping
    /// it would leave a turn that has no tools and no answer.
    Counted,
    /// The open piece of work this question belongs to.
    ///
    /// After `Focus` because it is the same situation described one level up:
    /// the screen says where the user is, this says what they are in the middle
    /// of. See `thread.rs`.
    Thread,
    /// What the vault is shaped like and which tool reaches what.
    ToolShape,
    /// Pinned memories, and any recalled for this question.
    Memory,
    /// The one-line index of skills the user has enabled.
    Skills,
    /// Chunks retrieved for this question.
    VaultContext,
}

impl SectionKind {
    /// A human-readable name, for the breakdown the user is shown.
    pub fn label(self) -> &'static str {
        match self {
            SectionKind::Custom => "Your own instructions",
            SectionKind::Identity => "Identity",
            SectionKind::Rules => "Rules",
            SectionKind::Today => "Today",
            SectionKind::Focus => "What is on screen",
            SectionKind::Counted => "Already counted",
            SectionKind::Thread => "The work this belongs to",
            SectionKind::ToolShape => "Tools and vault shape",
            SectionKind::Memory => "What Syn remembers",
            SectionKind::Skills => "What Syn knows how to do",
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
    ///
    /// `Focus` is required for that second reason and is safe to be, because it
    /// is bounded: `focus::MAX_SELECTION_CHARS` is the only part that varies and
    /// it is capped. A dropped focus would not shrink the prompt much and would
    /// turn "rewrite this paragraph" back into a sentence with no referent —
    /// which is the failure it exists to remove.
    pub fn is_required(self) -> bool {
        !matches!(
            self,
            SectionKind::VaultContext
                | SectionKind::Memory
                | SectionKind::Skills
                | SectionKind::Thread
        )
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

/// How to cite, what not to fabricate — and how to behave when there is
/// nothing to cite.
///
/// `footing::RULE` joins on here rather than becoming a section of its own for
/// two reasons. It is the same kind of instruction: *do not present a guess in
/// the voice of a result* is the sentence directly under *do not fabricate*,
/// and splitting them would put a rule about honesty two headings away from
/// the other rule about honesty. And a `SectionKind::Footing` sitting beside
/// `SectionKind::Focus` is two nearly identical names for entirely different
/// things — what is on screen, and what an answer stands on — in a list people
/// read quickly.
fn rules() -> String {
    format!("{RULES}\n{}", crate::syn::footing::RULE)
}

/// What the tools are and what the vault is shaped like.
///
/// Named for the shape rather than the tools because that is what it teaches.
/// The list of tool *names* is sent separately, as definitions; this is the
/// part that says a book and a task are the same kind of thing.
fn tool_shape() -> String {
    format!("{TOOL_SHAPE}{}", web_line())
}

/// What Syn can do about the internet, said out loud.
///
/// # Why the prompt says this at all
///
/// Asked *"what was the score yesterday"*, Syn once answered **"I have no live
/// web data in this conversation"** — vague, slightly false, and useless. The
/// behaviour was right and the sentence was not, which is this codebase's
/// recurring failure caught one turn earlier than usual.
///
/// # Why it no longer says "if"
///
/// It had two branches, because searching needed an endpoint somebody had
/// configured. `syn::browser` removed that: a search happens in a real window
/// the user can watch, so there is no vault in which Syn cannot search and no
/// state left to explain away.
///
/// One line, always the same, is what a capability with no configuration looks
/// like from inside a prompt.
fn web_line() -> &'static str {
    "- THE WEB: `browse` looks something up or reads a page — pass a question to search for, or an http address to read. It opens a window the user can watch. Never invent a URL: pass the question instead and let the search find it.\n"
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

/// Who Syn is, and the one thing about *how* it speaks that is not the user's
/// to choose.
///
/// The second paragraph used to be a section of its own, picked from three by a
/// `personality` setting. Two of those three hard-coded Vietnamese and a
/// pronoun pair on the user's behalf; the third — the default — is this, and it
/// is not a personality at all. It is the rule that makes a bilingual app work.
/// So it stays, unconditionally, and the *choice* is gone: how Syn talks to
/// somebody is now something they write in their own words in `SYN.md`, where
/// they can say anything rather than one of three things. See
/// `instructions::TEMPLATE`.
const IDENTITY: &str = r#"You are Syn, a personal AI assistant embedded in the Synabit productivity app. Synabit is a second-brain/productivity tool that stores notes, tasks, events, contacts, files, RSS feeds, and financial records.

Match the user's language and communication style. If they write in Vietnamese, respond in Vietnamese. If they write in English, respond in English. If they use casual language (tao/mày), be casual back. If they are formal, be formal.

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

/// What one chat turn's prompt is built from.
///
/// A struct rather than a growing argument list, for the reason `DriveRequest`
/// is one: the next two things that want a section here — a skill index, and
/// whatever follows it — are fields, not another pair of positional `Option`s
/// that can be swapped without the compiler noticing.
pub struct ChatPrompt<'a> {
    /// Chunks retrieved for this question, already formatted.
    pub context: &'a str,
    /// The user's own standing instructions, from settings.
    pub custom: Option<&'a str>,
    /// The skill index, already formatted by `skill::index_block`.
    pub skills: Option<&'a str>,
    /// What Syn remembers, already formatted by `memory::memory_block`.
    pub memory: Option<&'a str>,
    /// The open thread, already formatted by `Thread::block`.
    ///
    /// Pre-rendered like memory and skills, because rendering it needs the
    /// database and this module has never needed one. `Focus` carries the id;
    /// the caller does the lookup.
    pub thread: Option<&'a str>,
    /// A count already run, from `tempo::block`.
    pub counted: Option<&'a str>,
    /// Which screen the user is on and what they have highlighted.
    ///
    /// Borrowed rather than owned like the rest, and `None` for every caller
    /// that has no screen — a background run, an eval, a reflection turn. Those
    /// render exactly the prompt they rendered before this field existed.
    pub focus: Option<&'a crate::syn::focus::Focus>,
    pub budget_chars: usize,
}

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
    pub fn for_chat(p: ChatPrompt<'_>) -> Self {
        let ChatPrompt {
            context,
            custom,
            skills,
            memory,
            focus,
            thread,
            counted,
            budget_chars,
        } = p;
        let mut sections = Vec::new();

        if let Some(custom) = custom.map(str::trim).filter(|c| !c.is_empty()) {
            sections.push(Section {
                kind: SectionKind::Custom,
                body: format!("{custom}\n\n"),
            });
        }
        sections.push(Section { kind: SectionKind::Identity, body: identity().to_string() });
        sections.push(Section { kind: SectionKind::Rules, body: rules() });
        sections.push(Section { kind: SectionKind::Today, body: today() });

        // Absent rather than empty when there is no screen, on the same terms
        // as memory and skills below: a heading announcing what is on screen,
        // above nothing, tells the model something false.
        if let Some(block) = focus.and_then(crate::syn::focus::Focus::block) {
            sections.push(Section { kind: SectionKind::Focus, body: block });
        }

        if let Some(counted) = counted.filter(|c| !c.trim().is_empty()) {
            sections.push(Section { kind: SectionKind::Counted, body: counted.to_string() });
        }

        if let Some(thread) = thread.filter(|t| !t.trim().is_empty()) {
            sections.push(Section { kind: SectionKind::Thread, body: thread.to_string() });
        }

        sections.push(Section { kind: SectionKind::ToolShape, body: tool_shape() });

        // Absent rather than empty when nothing is remembered, which is what
        // keeps a vault with no memories sending byte for byte the prompt it
        // sent before memory existed. The snapshots assert it.
        if let Some(skills) = skills.filter(|s| !s.trim().is_empty()) {
            sections.push(Section {
                kind: SectionKind::Skills,
                body: skills.to_string(),
            });
        }

        if let Some(memory) = memory.filter(|m| !m.trim().is_empty()) {
            sections.push(Section {
                kind: SectionKind::Memory,
                body: memory.to_string(),
            });
        }

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
            // Retrieved context goes before remembered facts, whatever their
            // sizes. Context is a sample of the vault that the model is told it
            // may need to search past, and it can go and look again; a memory
            // is something it was told and cannot recover by searching.
            let droppable = |kind: SectionKind| match kind {
                SectionKind::VaultContext => Some(0),
                // Skills before memory. Losing the index means Syn does a task
                // its own way instead of the way the user wrote down; losing
                // memory means it gets the person wrong — and the memories that
                // matter most are constraints, like an allergy. Doing a job
                // clumsily is recoverable in a way that is not.
                SectionKind::Skills => Some(1),
                SectionKind::Memory => Some(2),
                _ => None,
            };
            let biggest = self
                .sections
                .iter()
                .enumerate()
                .filter_map(|(i, s)| droppable(s.kind).map(|rank| (rank, i, s)))
                .min_by_key(|(rank, _, s)| (*rank, usize::MAX - s.body.chars().count()))
                .map(|(_, i, s)| (i, s.kind));

            match biggest {
                // Memory shrinks before it goes. The trimmer removes whole
                // sections, and for this one that meant forgetting everything
                // rather than the least important thing — a cliff that gets
                // likelier exactly as memory gets more valuable, and that is
                // invisible from inside a conversation. `memory::shrink_block`
                // knows the eviction order, which is the same order it wrote
                // the block in.
                Some((index, SectionKind::Memory)) => {
                    let over = self.chars().saturating_sub(self.budget_chars);
                    match crate::syn::memory::shrink_block(&self.sections[index].body, over) {
                        Some(smaller) => self.sections[index].body = smaller,
                        None => {
                            self.sections.remove(index);
                            self.dropped.push(SectionKind::Memory);
                        }
                    }
                }
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
    /// What the tool declarations cost, which is not part of the prompt text.
    ///
    /// # Why a panel about the prompt has to report something that is not in it
    ///
    /// Because the question people bring here is *what does one turn cost*, and
    /// the answer was under-reported by more than the whole fixed prompt. The
    /// tool list does not go through `PromptPlan` — it is the `tools` field of
    /// the request, beside `messages` — so every number on this screen was
    /// exactly right and the screen as a whole was misleading.
    ///
    /// The measured figures when this was added: the fixed sections came to
    /// 5,887 characters and the twenty-seven tool declarations to 18,022.
    /// **Declaring the tools cost three times the entire fixed prompt**, and
    /// nothing had ever said so.
    pub tools: ToolPayload,
}

/// What the tool declarations cost, measured rather than estimated.
#[derive(Serialize, Debug, Clone, Default)]
pub struct ToolPayload {
    pub count: usize,
    /// Serialised JSON length — what actually goes on the wire.
    pub chars: usize,
    pub est_tokens: usize,
    /// The ceiling this is held to. See `tools::PAYLOAD_BUDGET_CHARS`.
    pub budget_chars: usize,
}

impl PromptPreview {
    /// Prompt plus tools, which is what a turn actually costs before anybody
    /// has said anything.
    ///
    /// Worth having as one number because neither half is the answer on its
    /// own, and against Ollama's default 8,192-token window the sum is the
    /// figure that decides whether a conversation fits at all.
    pub fn total_est_tokens(&self) -> usize {
        self.est_tokens + self.tools.est_tokens
    }
}

impl From<PromptPlan> for PromptPreview {
    fn from(plan: PromptPlan) -> Self {
        Self {
            text: plan.render(),
            chars: plan.chars(),
            est_tokens: plan.est_tokens(),
            budget_chars: plan.budget_chars(),
            sections: plan.breakdown(),
            tools: crate::syn::tools::payload_cost(),
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
        let fixed = PromptPlan::for_chat(ChatPrompt { context: "", custom: None, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: DEFAULT_BUDGET_CHARS }).chars();

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

        // One prompt, not three. The loop used to be parameterised by the
        // personality setting; that setting is gone, so the six snapshots
        // become two and the four naming a register were deleted with it.
        {
            for (label, context) in [("bare", ""), ("with-context", "some context")] {
                let rendered =
                    PromptPlan::for_chat(ChatPrompt {
                        context,
                        custom: None,
                        skills: None, memory: None, focus: None, thread: None, counted: None,
                        budget_chars: DEFAULT_BUDGET_CHARS,
                    }).render();
                let masked = today.replace_all(&rendered, "- Today's date: <DATE>");

                let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/syn/testdata")
                    .join(format!("system-prompt.{label}.txt"));

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
                    "the system prompt for {label} no longer matches {}",
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
        let plan = PromptPlan::for_chat(ChatPrompt { context: "", custom: Some("Always answer in haiku."), skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: DEFAULT_BUDGET_CHARS });
        let rendered = plan.render();
        assert!(rendered.starts_with("Always answer in haiku.\n\nYou are Syn,"));
    }

    /// A settings file with an empty string in it is a settings file with no
    /// custom prompt. Rendering it would put two blank lines at the top of
    /// every prompt for no reason.
    #[test]
    fn an_empty_custom_prompt_adds_no_section() {
        for empty in [Some(""), Some("   "), None] {
            let plan = PromptPlan::for_chat(ChatPrompt { context: "", custom: empty, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: DEFAULT_BUDGET_CHARS });
            assert!(plan.render().starts_with("You are Syn,"), "{empty:?}");
            assert!(!plan.breakdown().iter().any(|c| c.kind == SectionKind::Custom));
        }
    }

    #[test]
    fn no_context_means_no_context_section() {
        let plan = PromptPlan::for_chat(ChatPrompt { context: "", custom: None, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: DEFAULT_BUDGET_CHARS });
        assert!(!plan.render().contains("VAULT CONTEXT"));
        assert!(!plan.breakdown().iter().any(|c| c.kind == SectionKind::VaultContext));
    }

    #[test]
    fn context_is_wrapped_in_the_instructions_for_reading_it() {
        let plan = PromptPlan::for_chat(ChatPrompt { context: "a note about ducks", custom: None, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: DEFAULT_BUDGET_CHARS });
        let rendered = plan.render();
        assert!(rendered.contains("=== VAULT CONTEXT ==="));
        assert!(rendered.contains("a note about ducks"));
        assert!(rendered.trim_end().ends_with("=== END CONTEXT ==="));
    }

    #[test]
    fn the_breakdown_accounts_for_every_character_that_was_sent() {
        let plan = PromptPlan::for_chat(ChatPrompt { context: "ctx", custom: Some("be brief"), skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: DEFAULT_BUDGET_CHARS });
        let counted: usize = plan.breakdown().iter().filter(|c| !c.dropped).map(|c| c.chars).sum();
        assert_eq!(counted, plan.render().chars().count());
        assert_eq!(counted, plan.chars());
    }

    /// The budget takes the retrieved context, which the model is told to
    /// search past, and never the rules.
    #[test]
    fn a_tight_budget_drops_context_and_keeps_the_rules() {
        let plan = PromptPlan::for_chat(ChatPrompt { context: &"x".repeat(5000), custom: None, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: 6000 });
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
        let plan = PromptPlan::for_chat(ChatPrompt { context: "ctx", custom: None, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: 10 });
        let rendered = plan.render();
        assert!(rendered.contains("Key rules:"));
        assert!(rendered.contains("Tool usage guidelines:"));
        assert!(plan.chars() > plan.budget_chars());
    }

    fn plan(context: &str, memory: Option<&str>, budget: usize) -> PromptPlan {
        PromptPlan::for_chat(ChatPrompt {
            context,
            custom: None,
            skills: None,
            memory,
            focus: None, thread: None, counted: None,
            budget_chars: budget,
        })
    }

    /// The property that let memory ship without re-blessing a single
    /// snapshot: a vault that remembers nothing sends exactly what it sent
    /// before this section existed.
    #[test]
    fn nothing_remembered_adds_no_section() {
        for empty in [None, Some(""), Some("   ")] {
            let p = plan("", empty, DEFAULT_BUDGET_CHARS);
            assert!(!p.breakdown().iter().any(|c| c.kind == SectionKind::Memory), "{empty:?}");
        }
    }

    /// Order is meaning here. Memory goes after the rules and the tools, so the
    /// model knows how to act before it is told what it knows, and before the
    /// retrieved context, so a standing fact is not buried under a sample.
    #[test]
    fn memory_sits_between_the_tools_and_the_retrieved_context() {
        let rendered = plan("some context", Some("=== WHAT YOU REMEMBER ===\n- [fact] x"), DEFAULT_BUDGET_CHARS)
            .render();

        let tools = rendered.find("Tool usage guidelines:").expect("tools are there");
        let memory = rendered.find("WHAT YOU REMEMBER").expect("memory is there");
        let context = rendered.find("VAULT CONTEXT").expect("context is there");
        assert!(tools < memory, "memory must come after the tools");
        assert!(memory < context, "memory must come before the retrieved context");
    }

    /// Refuses to compile when a `SectionKind` is added and `ALL` is not.
    ///
    /// The same trick `RunState` uses, and for the same reason: only the
    /// compiler knows every variant, so a test that enumerates them is
    /// enumerating the list it is meant to be checking.
    #[allow(dead_code)]
    fn _every_section_is_listed(kind: SectionKind) {
        match kind {
            SectionKind::Custom
            | SectionKind::Identity
            | SectionKind::Rules
            | SectionKind::Today
            | SectionKind::Focus
            | SectionKind::Counted
            | SectionKind::Thread
            | SectionKind::ToolShape
            | SectionKind::Memory
            | SectionKind::Skills
            | SectionKind::VaultContext => {}
        }
    }

    const ALL: [SectionKind; 11] = [
        SectionKind::Custom,
        SectionKind::Identity,
        SectionKind::Rules,
        SectionKind::Today,
        SectionKind::Focus,
        SectionKind::Counted,
        SectionKind::Thread,
        SectionKind::ToolShape,
        SectionKind::Memory,
        SectionKind::Skills,
        SectionKind::VaultContext,
    ];

    /// The panel that says what Syn is told must have a name for every part of
    /// it.
    ///
    /// This list had already fallen two behind before anyone noticed: `memory`
    /// and `skills` both shipped without reaching the union, so the one screen
    /// built to make the prompt legible could not name two of its sections. A
    /// section the panel has no word for is a section nobody can debug.
    #[test]
    fn the_frontend_knows_every_section_the_prompt_can_have() {
        let source = std::fs::read_to_string("../src/mini-apps/messages/types.ts")
            .expect("the messages types should be readable from src-tauri");

        let union = source
            .split("export type PromptSectionKind =")
            .nth(1)
            .and_then(|rest| rest.split(';').next())
            .expect("types.ts should still declare a PromptSectionKind union");

        let declared: Vec<&str> = union
            .split('|')
            .map(|part| part.trim().trim_matches(|c| c == '\'' || c == '"'))
            .filter(|part| !part.is_empty())
            .collect();

        assert!(
            declared.len() >= 8,
            "parsed too few kinds from the frontend union — the parsing broke, not the code: \
             {declared:?}"
        );

        // Serialised as snake_case, which is what the frontend receives.
        for kind in ALL {
            let wire = serde_json::to_value(kind)
                .expect("a section kind serialises")
                .as_str()
                .expect("as a string")
                .to_string();
            assert!(
                declared.contains(&wire.as_str()),
                "the prompt can contain a `{wire}` section and the frontend union does not list \
                 it: {declared:?}"
            );
        }

        // And the other way, which this test did not check until a section was
        // removed. `personality` stayed in the union after the enum lost it —
        // a kind the frontend can draw and Rust can never send, which is dead
        // code that reads as a feature. Neither direction is safe alone.
        let ours: Vec<String> = ALL
            .iter()
            .map(|k| {
                serde_json::to_value(k)
                    .expect("serialises")
                    .as_str()
                    .expect("as a string")
                    .to_string()
            })
            .collect();
        for kind in &declared {
            assert!(
                ours.iter().any(|k| k == kind),
                "the frontend lists a `{kind}` section that the prompt can never produce: {ours:?}"
            );
        }
    }

    /// What is on screen sits with the date, above everything the assistant
    /// would have to go and look for. Both are facts about right now that no
    /// tool can answer.
    #[test]
    fn what_is_on_screen_sits_with_the_date() {
        let focus = crate::syn::focus::Focus {
            app: "note".into(),
            node: Some("Notes/pricing.md".into()),
            node_title: None,
            selection: Some("per-seat cho team nhỏ".into()),
            thread: None,
            browsing: None,
        };
        let rendered = PromptPlan::for_chat(ChatPrompt {
            context: "some context",
            custom: None,
            skills: None,
            memory: None,
            focus: Some(&focus), thread: None, counted: None,
            budget_chars: DEFAULT_BUDGET_CHARS,
        })
        .render();

        let today = rendered.find("Today's date").expect("the date is there");
        let screen = rendered.find("ON SCREEN").expect("the screen is there");
        let tools = rendered.find("Tool usage guidelines:").expect("tools are there");
        assert!(today < screen, "the screen comes after the date");
        assert!(screen < tools, "and before anything it would have to look up");
        assert!(rendered.contains("per-seat cho team nhỏ"), "{rendered}");
    }

    /// The work sits just after the screen: same situation, one level up.
    #[test]
    fn the_work_sits_after_the_screen_and_before_the_tools() {
        let focus = crate::syn::focus::Focus {
            app: "note".into(),
            node: Some("Notes/pricing.md".into()),
            node_title: None,
            selection: None,
            thread: Some("SynThreads/Pricing.md".into()),
            browsing: None,
        };
        let rendered = PromptPlan::for_chat(ChatPrompt {
            context: "",
            custom: None,
            skills: None,
            memory: None,
            focus: Some(&focus),
            thread: Some("\n=== THE WORK THIS BELONGS TO ===\nPricing\n"), counted: None,
            budget_chars: DEFAULT_BUDGET_CHARS,
        })
        .render();

        let screen = rendered.find("ON SCREEN").expect("the screen is there");
        let work = rendered.find("THE WORK THIS BELONGS TO").expect("the work is there");
        let tools = rendered.find("Tool usage guidelines:").expect("tools are there");
        assert!(screen < work, "the work comes after the screen");
        assert!(work < tools, "and before the tools");
    }

    /// A question asked outside any thread renders no work section — the same
    /// rule memory, skills and the screen all follow.
    #[test]
    fn no_thread_means_no_work_section() {
        let kinds: Vec<_> = plan("ctx", None, DEFAULT_BUDGET_CHARS)
            .breakdown()
            .into_iter()
            .map(|c| c.kind)
            .collect();
        assert!(!kinds.contains(&SectionKind::Thread), "{kinds:?}");
    }

    /// A thread id that resolved to nothing must not leave a heading behind.
    #[test]
    fn an_empty_thread_block_adds_no_section() {
        for empty in [Some(""), Some("   "), None] {
            let kinds: Vec<_> = PromptPlan::for_chat(ChatPrompt {
                context: "",
                custom: None,
                skills: None,
                memory: None,
                focus: None,
                thread: empty,
                counted: None,
                budget_chars: DEFAULT_BUDGET_CHARS,
            })
            .breakdown()
            .into_iter()
            .map(|c| c.kind)
            .collect();
            assert!(!kinds.contains(&SectionKind::Thread), "{empty:?} -> {kinds:?}");
        }
    }

    /// The breakdown is the one screen that says where the window went. A
    /// section absent from it is a section nobody can find when it misfires.
    #[test]
    fn the_screen_shows_up_in_the_breakdown() {
        let focus = crate::syn::focus::Focus {
            app: "task".into(),
            node: None,
            node_title: None,
            selection: None,
            thread: None,
            browsing: None,
        };
        let costs = PromptPlan::for_chat(ChatPrompt {
            context: "",
            custom: None,
            skills: None,
            memory: None,
            focus: Some(&focus), thread: None, counted: None,
            budget_chars: DEFAULT_BUDGET_CHARS,
        })
        .breakdown();

        let screen = costs
            .iter()
            .find(|c| c.kind == SectionKind::Focus)
            .expect("the on-screen section is accounted for");
        assert!(screen.chars > 0);
        assert!(!screen.dropped, "it is required, so it is never cut");
    }

    /// A request that carries no screen renders what it always rendered. This
    /// is what let the byte-exact snapshots survive the change.
    #[test]
    fn no_screen_means_no_section() {
        let kinds: Vec<_> = plan("ctx", None, DEFAULT_BUDGET_CHARS)
            .breakdown()
            .into_iter()
            .map(|c| c.kind)
            .collect();
        assert!(!kinds.contains(&SectionKind::Focus), "{kinds:?}");
    }

    /// Under pressure the sample goes before the standing fact, whatever their
    /// sizes. The model can search the vault again; it cannot re-derive
    /// something it was told once.
    #[test]
    fn a_tight_budget_gives_up_context_before_it_gives_up_memory() {
        let big_context = "c".repeat(4000);
        let small_memory = "=== WHAT YOU REMEMBER ===\n- [preference] họp buổi sáng";

        let p = plan(&big_context, Some(small_memory), FIXED_SECTIONS_CHARS + 300);
        let kinds: Vec<_> = p.breakdown().into_iter().filter(|c| c.dropped).map(|c| c.kind).collect();
        assert_eq!(kinds, vec![SectionKind::VaultContext]);
        assert!(p.render().contains("họp buổi sáng"), "memory should have survived");
    }

    fn a_memory(body: &str, kind: &str, pinned: bool) -> crate::syn::memory::Memory {
        crate::syn::memory::Memory {
            id: format!("SynMemory/{body}.md"),
            title: body.to_string(),
            body: body.to_string(),
            kind: kind.to_string(),
            subject: None,
            confidence: 0.9,
            pinned,
            first_seen: "2026-09-01".into(),
            last_confirmed: "2026-09-01".into(),
            source_run: None,
            source_nodes: Vec::new(),
            review_after: None,
            supersedes: None,
        }
    }

    fn plan_with_skills(context: &str, skills: &str, memory: &str, budget: usize) -> PromptPlan {
        PromptPlan::for_chat(ChatPrompt {
            context,
            custom: None,
            skills: Some(skills),
            memory: Some(memory), focus: None, thread: None, counted: None,
            budget_chars: budget,
        })
    }

    /// The skill index reaches the model, and sits where it can be read.
    #[test]
    fn what_syn_knows_how_to_do_is_in_the_prompt() {
        let block = crate::syn::skill::index_block(
            &[],
            crate::syn::skill::INDEX_BUDGET_CHARS,
        );
        assert!(block.is_none(), "no skills, no section — the snapshots depend on it");

        let rendered = plan_with_skills(
            "",
            "=== WHAT YOU KNOW HOW TO DO ===\n- weekly-review: sums the week",
            "=== WHAT YOU REMEMBER ===\n- [fact] họ tên là Minh",
            DEFAULT_BUDGET_CHARS,
        )
        .render();

        let skills_at = rendered.find("KNOW HOW TO DO").expect("the index is there");
        let memory_at = rendered.find("WHAT YOU REMEMBER").expect("memory is there");
        assert!(
            skills_at < memory_at,
            "what it can do comes before what it knows, so the procedure is read \
             in the light of the person"
        );
    }

    /// Under pressure the index goes before memory does.
    ///
    /// Losing the index means Syn does a task its own way instead of the way
    /// the user wrote down. Losing memory means it gets the person wrong, and
    /// the memories that matter most are constraints — an allergy, an injury.
    /// Doing a job clumsily is recoverable in a way that is not.
    #[test]
    fn a_tight_budget_gives_up_skills_before_it_gives_up_memory() {
        let skills = format!("=== WHAT YOU KNOW HOW TO DO ===\n{}", "s".repeat(3000));
        let memory = "=== WHAT YOU REMEMBER ===\n- [fact] vợ dị ứng hải sản";

        // Derived rather than a literal: the number means "room for the
        // memory line and nothing else", and a hardcoded 6,000 stopped meaning
        // that the moment the fixed sections grew.
        let p = plan_with_skills("", &skills, memory, FIXED_SECTIONS_CHARS + 300);
        let dropped: Vec<_> = p.breakdown().into_iter().filter(|c| c.dropped).map(|c| c.kind).collect();

        assert_eq!(dropped, vec![SectionKind::Skills], "only the index went");
        assert!(
            p.render().contains("dị ứng hải sản"),
            "the constraint survives:\n{}",
            p.render()
        );
    }

    /// Memory gives up its least important entries before it gives up itself.
    ///
    /// The trimmer removes whole sections, so before this a budget tight enough
    /// to reach memory made Syn forget everything it knew about the person
    /// rather than the least important thing — and nothing in the reply says so.
    ///
    /// Built from a real `memory_block` rather than a hand-written string, so
    /// that a change to the block's format shows up here instead of leaving the
    /// test asserting things about a shape nothing produces any more.
    #[test]
    fn a_tight_budget_shrinks_memory_rather_than_forgetting_everything() {
        use crate::syn::memory::{memory_block, MEMORY_BUDGET_CHARS};

        let memories = vec![
            a_memory("Luôn trả lời bằng tiếng Việt.", "instruction", true),
            a_memory("Ghét hành.", "preference", false),
            a_memory("Sinh nhật 12 tháng 3.", "fact", false),
            a_memory("Vợ dị ứng hải sản.", "fact", false),
        ];
        let block = memory_block(&memories, MEMORY_BUDGET_CHARS).expect("a block");

        // No context, so memory is the only thing the trimmer can reach.
        let roomy = plan("", Some(&block), 100_000);
        let shortest = memories
            .iter()
            .map(|m| m.line().chars().count() + 1)
            .min()
            .expect("some memories");
        let p = plan("", Some(&block), roomy.chars() - shortest);

        let dropped: Vec<_> = p.breakdown().into_iter().filter(|c| c.dropped).map(|c| c.kind).collect();
        assert!(
            !dropped.contains(&SectionKind::Memory),
            "the section shrinks instead of going: {dropped:?}"
        );

        let rendered = p.render();
        assert!(
            rendered.contains("Luôn trả lời bằng tiếng Việt."),
            "the instruction is the last thing to go:\n{rendered}"
        );
        assert!(
            memories.iter().any(|m| !rendered.contains(m.body.as_str())),
            "something was actually given up:\n{rendered}"
        );
        assert!(
            rendered.contains("did not fit and were left out"),
            "and the loss is declared:\n{rendered}"
        );
    }

    /// And when dropping the context is not enough, memory goes too — rather
    /// than the rules, which are never dropped.
    #[test]
    fn memory_goes_before_anything_that_defines_the_assistant() {
        let huge_memory = "=== WHAT YOU REMEMBER ===\n".to_string() + &"m".repeat(4000);
        let p = plan("ctx", Some(&huge_memory), 6000);

        let dropped: Vec<_> = p.breakdown().into_iter().filter(|c| c.dropped).map(|c| c.kind).collect();
        assert!(dropped.contains(&SectionKind::Memory));
        assert!(p.render().contains("Key rules:"));
        assert!(p.render().contains("Tool usage guidelines:"));
    }

    /// Vietnamese is where a byte-counting mistake would show up first.
    #[test]
    fn costs_are_counted_in_characters_not_bytes() {
        let plan = PromptPlan::for_chat(ChatPrompt { context: "", custom: Some("đường"), skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: DEFAULT_BUDGET_CHARS });
        let custom = plan
            .breakdown()
            .into_iter()
            .find(|c| c.kind == SectionKind::Custom)
            .expect("the custom section is there");
        assert_eq!(custom.chars, "đường\n\n".chars().count());
    }
}
