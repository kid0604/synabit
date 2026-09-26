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
///
/// A body the harness put in the prompt (`chosen_for`) does not count against
/// this. Both reasons for the cap are already met another way: there is at
/// most one such body, it is held to `CHOSEN_BODY_CHARS`, and `PromptPlan::fit`
/// drops it before the index when room runs short — so it cannot crowd out the
/// context, and it is not the model browsing. Counting it would also mean the
/// harness spending half the model's allowance on a guess the model was told
/// it may ignore, leaving one `load_skill` for the skill that actually fits.
/// The worst case is three bodies in one run, one of them chosen by the harness.
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
    /// The run that prompted this skill, when Syn wrote it. Lets the trial ask
    /// the same question the skill was invented to answer.
    pub source_run: Option<String>,
    /// When this skill was last tried with and without, as `YYYY-MM-DD`.
    ///
    /// The roadmap's third step, given a place to live: a skill Syn wrote is not
    /// offered a switch until somebody has seen it answer something both ways.
    /// Kept on the file rather than in memory so the evidence travels with the
    /// skill and survives closing the app.
    ///
    /// A gate on the screen, not on the write path. The vault is the user's and
    /// this is a Markdown file with `enabled:` in the frontmatter — anybody who
    /// wants to turn a skill on in a text editor may, and should be able to.
    /// What this prevents is Syn enabling its own work, and what it offers the
    /// user is a reason not to do it blind.
    pub trial_at: Option<String>,
    /// A revision Syn is proposing, waiting on the user.
    ///
    /// Not applied. The skill is enabled — that is why it ran and why it went
    /// wrong — so rewriting the body would change behaviour the moment it was
    /// written, which is an agent editing its own live procedure without anyone
    /// knowing. It sits here until a person reads the two side by side.
    pub pending_revision: Option<String>,
    /// What went wrong that prompted the revision, in the model's own words.
    pub revision_because: Option<String>,
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
            source_run: props
                .get("source_run")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            pending_revision: props
                .get("pending_revision")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            revision_because: props
                .get("revision_because")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            trial_at: props
                .get("trial_at")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
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

impl Skill {
    /// May this be turned on yet?
    ///
    /// A skill the user wrote is theirs to enable whenever they like — they know
    /// what is in it, because they typed it. One Syn wrote has to have been
    /// tried first: the roadmap's third step, and the reason it gives is the one
    /// that matters. A person who has not seen a skill answer anything is being
    /// asked to trust a procedure on the strength of its own summary.
    ///
    /// Read by the screen, which is where it bites. Nothing on the write path
    /// consults it, deliberately: hand-editing a file in your own vault is not
    /// something this app gets to refuse.
    pub fn may_be_enabled(&self) -> bool {
        self.author != "syn" || self.trial_at.is_some()
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

/// The index's opening: its heading and the two rules that go with it.
///
/// A constant rather than a literal inside `index_block` so `lines_indexed`
/// can step over it. Its two `- ` bullets are rules, not skills, and a counter
/// that did not know where the rules end would report two skills in every
/// index — the undercount-by-shape `memory::lines_shown` was written to end,
/// the other way round.
const INDEX_HEADER: &str = "\n\n=== WHAT YOU KNOW HOW TO DO ===\n\
         Procedures written down for you, by this person or by you and approved \
         by them. This is the whole list — there is nothing else.\n\
         - The line is a summary. Call `load_skill` with the name to read the \
         steps before following them.\n\
         - Do not guess at the steps from the description. If it is worth doing \
         by a skill, it is worth reading first.\n";

/// How many skills a rendered index names.
///
/// Counted off the block as sent rather than off the store, for the reason
/// `memory::lines_shown` is: an index `PromptPlan::fit` cut offered nothing,
/// whatever is enabled. Zero for anything `index_block` did not write.
pub fn lines_indexed(block: &str) -> usize {
    block
        .strip_prefix(INDEX_HEADER)
        .map(|rest| rest.lines().filter(|l| l.starts_with("- ")).count())
        .unwrap_or(0)
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

    let header = INDEX_HEADER;
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

// ═══════════════════════════════════════════════════════════════
//  THE HARNESS PICKS
// ═══════════════════════════════════════════════════════════════
//
// The index plus `load_skill` is the shape `recall` failed in: a tool the
// model has to think of calling. `recall` went uncalled in fifteen runs of
// fifteen and the fix was to put memory in the prompt; `load_skill` went
// uncalled in seventeen of seventeen (`syn::stats`) and this is the same fix.
// Before the model is asked, the question is matched against what each
// enabled skill says about itself, and a skill that clearly fits has its
// steps put in the prompt — the model reads them because they are there, not
// because it decided to go and get them.
//
// Matched by words, in Rust, with no model call. Asking a model which skill
// fits would be a round trip on every message to answer a question that is
// usually "none", and it would be the model deciding again.

/// The most characters of a skill body the harness will put in a prompt unasked.
///
/// A procedure, not an essay: the starter skill is about 900 characters with
/// its documentation. Past this a body is left to `load_skill` — cutting a
/// procedure short to fit would hand the model the first half of the steps
/// with the authority of the whole.
pub const CHOSEN_BODY_CHARS: usize = 3_000;

/// Points a skill needs to be chosen. See `match_score`.
///
/// Four, because three is exactly "one Vietnamese compound": `tổng kết` is two
/// syllables and the pair they make, and it fits "tổng kết tuần" and "tổng kết
/// cuộc họp" alike. Four needs a third syllable (`tổng kết tuần`), or two long
/// English words (`weekly review`), or a compound and one more word.
pub const MATCH_THRESHOLD: usize = 4;

/// How far ahead of the next skill the chosen one must be.
///
/// Two skills that both nearly fit is the harness not knowing, and a wrong
/// procedure in the prompt is worse than none — the index still lists both,
/// and the model can read the question and choose.
pub const MATCH_MARGIN: usize = 2;

/// A question this long is mostly material — a pasted meeting, an article —
/// rather than a request, and its words say what the material is about, not
/// what to do with it. Only a quoted trigger phrase can choose a skill for it.
///
/// This gives something up knowingly: "làm biên bản: <the whole meeting>" does
/// not pick a minutes skill unless that skill has a quoted trigger. Scoring
/// only the opening words was the alternative, and the opening of pasted
/// material is as often a heading as a request.
const MOST_WORDS_TO_MATCH: usize = 40;

/// Words that say nothing about which skill fits, already folded.
///
/// Function words in both languages, the words a question is phrased with
/// ("giúp tao", "can you"), and the boilerplate a `when_to_use` is written in
/// ("khi người dùng nói", "when the user asks"). Folded, so some of these are
/// also other words — `hay` is *or* and *good*, `ho` is *they* and *on behalf
/// of* — and each was checked for a meaning a skill might be about. `chi` (to
/// spend, in "chi tiêu"), `ca` (in "cà phê"), `lan` (a name) and `thu` (income)
/// are deliberately not here.
const STOPWORDS: &[&str] = &[
    // Vietnamese
    "khi", "nguoi", "dung", "noi", "hoi", "la", "cua", "va", "cho", "toi", "tao", "minh",
    "ban", "nay", "do", "kia", "the", "nao", "gi", "co", "khong", "ko", "duoc", "voi", "mot",
    "nhung", "cac", "de", "thi", "ma", "se", "da", "dang", "roi", "giup", "gium", "dum",
    "hay", "lam", "oi", "nhe", "nha", "nhi", "ha", "trong", "ra", "vao", "len", "di", "ve",
    "tu", "bao", "nhieu", "sao", "vay", "ay", "cai", "con", "em", "anh", "no", "ho", "chung",
    "moi", "hon", "nua", "cung", "deu", "rat", "qua", "lai", "luc", "hom", "can", "xem",
    "biet", "muon", "phai", "neu", "tat", "sau", "truoc", "ky", "nang", "dua", "viec",
    "nhu",
    // English
    "an", "and", "or", "but", "of", "to", "for", "in", "on", "at", "by", "with", "from",
    "into", "about", "as", "is", "are", "was", "were", "be", "been", "am", "it", "its",
    "this", "that", "these", "those", "there", "here", "me", "my", "mine", "you", "your",
    "we", "our", "us", "he", "she", "they", "them", "their", "his", "her", "do", "does",
    "did", "done", "can", "could", "would", "should", "will", "may", "might", "must",
    "please", "want", "wants", "need", "needs", "like", "just", "also", "some", "any",
    "all", "each", "every", "what", "when", "where", "which", "who", "why", "how", "if",
    "then", "than", "so", "not", "no", "yes", "up", "out", "use", "used", "using", "user",
    "users", "ask", "asks", "asked", "asking", "say", "says", "said", "tell", "show",
    "give", "get", "make", "help", "run", "skill", "skills", "something", "thing",
    "things", "one",
];

/// What a piece of text contributes to matching: its content words, and each
/// pair of content words that stand next to each other once the filler
/// between them is gone.
///
/// Pairs are what make Vietnamese matchable. A syllable alone is ambiguous —
/// `kết` is in "tổng kết", "kết quả" and "kết nối" — and a pair is usually a
/// word: `tổng kết` is *summary*. Formed after the stopwords are removed, so
/// "tháng này tiêu" still pairs `tháng` with `tiêu`.
#[derive(Debug, Default)]
struct Terms {
    words: Vec<String>,
    pairs: std::collections::HashSet<(String, String)>,
    /// Every token, filler included, in order — for quoted trigger phrases,
    /// which are matched as written.
    sequence: Vec<String>,
}

fn is_content(word: &str) -> bool {
    word.chars().count() >= 2
        && !word.chars().all(|c| c.is_ascii_digit())
        && !STOPWORDS.contains(&word)
}

fn terms(text: &str) -> Terms {
    let folded = crate::search_fold::fold(text);
    let sequence: Vec<String> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let content: Vec<String> = sequence.iter().filter(|w| is_content(w)).cloned().collect();
    let pairs = content.windows(2).map(|p| (p[0].clone(), p[1].clone())).collect();
    let mut words = content;
    words.sort();
    words.dedup();
    Terms { words, pairs, sequence }
}

/// Phrases a skill's author put in quotes, as a trigger.
///
/// The starter template says to write `when_to_use` as *Khi người dùng nói
/// "tổng kết tuần"*, so the quote is already how people write these. A question
/// holding that phrase is the author's own rule firing, and it is the only
/// signal here that is not a guess.
fn quoted_phrases(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let normalised: String = text
        .chars()
        .map(|c| if matches!(c, '“' | '”' | '«' | '»') { '"' } else { c })
        .collect();
    for (i, inside) in normalised.split('"').enumerate() {
        if i % 2 == 1 {
            let phrase = terms(inside);
            if phrase.sequence.iter().any(|w| is_content(w)) {
                out.push(phrase.sequence);
            }
        }
    }
    out
}

/// Two content words that are the same word, give or take an ending.
///
/// Exact, or sharing their first six letters when both have at least six —
/// `summary`/`summarize`, `meeting`/`meetings`, `review`/`reviews`. Six is
/// where English endings start and Vietnamese syllables stop: a folded
/// syllable of six letters is the whole syllable (`truong`, `nguyen`), so for
/// Vietnamese this is equality in all but name.
fn same_word(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    a.len() >= 6 && b.len() >= 6 && a[..6] == b[..6]
}

/// Everything a skill says about itself: name, description, when to use it.
///
/// Each field on its own, then merged, so a pair never forms across the seam
/// between the end of the description and the start of `when_to_use`.
fn skill_terms(skill: &Skill) -> Terms {
    let mut merged = Terms::default();
    for text in [skill.name.as_str(), &skill.description, &skill.when_to_use] {
        let t = terms(text);
        merged.words.extend(t.words);
        merged.pairs.extend(t.pairs);
    }
    merged.words.sort();
    merged.words.dedup();
    merged
}

/// How well a question fits a skill, in points.
///
/// One point for each content word of the question the skill also uses, two
/// if the word has five letters or more, and one more for each adjacent pair
/// they share. The length rule is the difference between the two languages
/// this app is written in: an English word of that length usually means one
/// thing, while a Vietnamese syllable is short and means little until it has
/// a partner — which is what the pair point pays for.
///
/// A quoted trigger phrase found whole in the question is worth the threshold
/// on its own. See `quoted_phrases`.
pub fn match_score(skill: &Skill, question: &str) -> usize {
    let asked = terms(question);
    let offered = skill_terms(skill);

    let triggered = [&skill.when_to_use, &skill.description]
        .iter()
        .flat_map(|t| quoted_phrases(t))
        .any(|phrase| {
            !phrase.is_empty()
                && asked.sequence.windows(phrase.len()).any(|w| w == phrase.as_slice())
        });
    let bonus = if triggered { MATCH_THRESHOLD } else { 0 };
    // Counted with repeats: a pasted meeting says "decision" twenty times, and
    // it is its length, not its vocabulary, that makes it material.
    if asked.sequence.iter().filter(|w| is_content(w)).count() > MOST_WORDS_TO_MATCH {
        return bonus;
    }

    let words: usize = asked
        .words
        .iter()
        .filter(|q| offered.words.iter().any(|s| same_word(q, s)))
        .map(|q| if q.chars().count() >= 5 { 2 } else { 1 })
        .sum();
    let pairs = asked.pairs.intersection(&offered.pairs).count();
    bonus + words + pairs
}

/// The one skill that clearly fits this question, if there is one.
///
/// Only an enabled skill, for the reason the index lists only enabled ones:
/// the user turning a skill off is the one control they have over what Syn
/// reaches for, and a harness that injected a disabled skill would be
/// overriding it. A `code` skill is not runnable yet and is never chosen, and a
/// body over `CHOSEN_BODY_CHARS` or empty is left to `load_skill`.
///
/// Chosen when it clears `MATCH_THRESHOLD` and leads the next skill by
/// `MATCH_MARGIN`. Precision over recall throughout: a skill that should have
/// been chosen and was not is still in the index, where the model can load it
/// as before; a skill chosen wrongly is a procedure in the prompt for a
/// question it does not answer.
pub fn chosen_for<'a>(skills: &'a [Skill], question: &str) -> Option<&'a Skill> {
    let mut scored: Vec<(usize, &Skill)> = skills
        .iter()
        .filter(|s| s.enabled && s.tier != Tier::Code)
        .filter(|s| {
            let len = s.body.trim().chars().count();
            len > 0 && len <= CHOSEN_BODY_CHARS
        })
        .map(|s| (match_score(s, question), s))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    let (best, skill) = *scored.first()?;
    let runner_up = scored.get(1).map_or(0, |(score, _)| *score);
    (best >= MATCH_THRESHOLD && best >= runner_up + MATCH_MARGIN).then_some(skill)
}

/// The heading a chosen skill's section opens with.
const CHOSEN_HEADER: &str = "\n\n=== A SKILL FOR THIS QUESTION ===\n";

/// The section a chosen skill rides in.
///
/// Says how it got there. The model did not ask for this and nobody read the
/// question to choose it; a line saying so is what lets the model set it aside
/// when the words matched and the meaning did not, rather than bending the
/// answer to fit a procedure because it was handed one.
pub fn chosen_block(skill: &Skill) -> String {
    let how = if skill.tier == Tier::Recipe {
        format!(
            "It is a recipe: to use it, call `{}` with its name rather than doing the steps \
             yourself. The steps are below so you know what it will do.",
            crate::syn::recipe::RUN_TOOL
        )
    } else {
        format!("Its steps are below, so there is no need to call `{LOAD_TOOL}` for it.")
    };
    format!(
        "{CHOSEN_HEADER}`{}` was picked for this question because its description matches \
         the words of it. If it does not fit what was actually asked, ignore it. {how}\n\n{}\n\
         === END ===",
        skill.name,
        skill.body.trim()
    )
}

/// Which skill a chosen-skill section carries, read back off the section.
///
/// Off the section as sent, for the reason `lines_indexed` reads the index as
/// sent: `PromptPlan::fit` may have dropped it, and a skill that was chosen
/// and then cut was not in front of the model.
pub fn chosen_name(block: &str) -> Option<String> {
    let rest = block.strip_prefix(CHOSEN_HEADER)?.strip_prefix('`')?;
    rest.split_once('`').map(|(name, _)| name.to_string())
}

/// How often a skill has actually been opened, and when it last was.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub name: String,
    /// Times `load_skill` returned this skill's body, across the runs still on
    /// disk. Runs are pruned, so this is "recently" rather than "ever" — and
    /// the screen says so, because a number that quietly means something
    /// narrower than it reads is worse than no number.
    pub runs: usize,
    pub last_run: String,
    pub last_at: String,
}

/// Which skills have been opened, newest use first.
///
/// This is the number this whole feature has to answer for. A skill that is
/// enabled, indexed, well written and never opened is doing nothing, and there
/// is no other way to find that out — the model reaching for a skill is a
/// decision nobody sees. `recall` was in exactly that position for weeks.
pub fn usage(runs: &[crate::syn::run::Run]) -> Vec<Usage> {
    let mut seen: std::collections::HashMap<String, Usage> = std::collections::HashMap::new();

    for run in runs {
        for step in &run.steps {
            if step.tool.as_deref() != Some(LOAD_TOOL) || step.ok != Some(true) {
                continue;
            }
            let Some(name) = step
                .args
                .as_ref()
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_str())
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
            else {
                continue;
            };

            let entry = seen.entry(name.to_lowercase()).or_insert_with(|| Usage {
                name: name.clone(),
                runs: 0,
                last_run: run.id.clone(),
                last_at: run.created_at.clone(),
            });
            entry.runs += 1;
            // Runs arrive newest first, so the first one to mention a skill is
            // the most recent use of it.
            if run.created_at > entry.last_at {
                entry.last_at = run.created_at.clone();
                entry.last_run = run.id.clone();
            }
        }
    }

    let mut out: Vec<Usage> = seen.into_values().collect();
    out.sort_by(|a, b| b.last_at.cmp(&a.last_at));
    out
}

/// The shortest chain worth calling a skill.
///
/// Two steps repeated is a coincidence — `query_nodes` then `get_node` is what
/// half of all answers look like. Three is a shape.
pub const MIN_CHAIN: usize = 3;

/// A sequence of tool calls this run made more than once.
///
/// Found in Rust rather than by asking a model, and that is the whole point.
/// The roadmap has reflection notice the pattern, which would mean a model call
/// after every run to answer a question arithmetic answers exactly — and would
/// mean paying for it on the great majority of runs, which repeat nothing. A
/// deterministic detector is free, testable without a network, and cannot
/// hallucinate a pattern that was not there.
///
/// What it looks for is the longest sequence of at least `MIN_CHAIN` successful
/// tool calls that occurs at least twice without overlapping itself. Overlap
/// matters: `a b a b a` contains `a b a` twice by position, but the second
/// reading reuses steps the first already claimed, and a person watching would
/// see one wobble rather than two passes.
pub fn repeated_chain(run: &crate::syn::run::Run) -> Option<Vec<String>> {
    let calls: Vec<&str> = run
        .steps
        .iter()
        .filter(|s| s.ok == Some(true))
        .filter_map(|s| s.tool.as_deref())
        .collect();

    if calls.len() < MIN_CHAIN * 2 {
        return None;
    }

    // Longest first: a run that repeated five steps should be named by the five,
    // not by the three inside them.
    for len in (MIN_CHAIN..=calls.len() / 2).rev() {
        for start in 0..=calls.len() - len {
            let candidate = &calls[start..start + len];
            // Non-overlapping: the next occurrence may only begin after this
            // one has finished.
            let found = calls[start + len..]
                .windows(len)
                .any(|window| window == candidate);
            if found {
                return Some(candidate.iter().map(|s| s.to_string()).collect());
            }
        }
    }
    None
}

/// A run that followed a skill and still went wrong.
///
/// Returns the skill it was following, when there is one and the run has
/// something to learn from: a failed tool call, or a ceiling reached. Both are
/// facts on the transcript, so the decision that there is something to revise
/// costs no inference — the same reason `repeated_chain` is arithmetic.
///
/// A run that followed a skill and went fine teaches nothing. A run that went
/// wrong without following one has no skill to blame.
pub fn skill_that_struggled(run: &crate::syn::run::Run) -> Option<String> {
    let followed = run.steps.iter().find_map(|step| {
        if step.tool.as_deref() != Some(LOAD_TOOL) || step.ok != Some(true) {
            return None;
        }
        step.args
            .as_ref()
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
    })?;

    let something_failed = run.steps.iter().any(|s| s.ok == Some(false));
    // `error` carries what the engine had to say — a ceiling reached, a limit
    // hit — and an interrupted run stopped before it could finish the job the
    // skill described.
    let hit_a_ceiling = run.state == crate::syn::run::RunState::Interrupted
        || run.error.as_deref().is_some_and(|e| !e.trim().is_empty());

    (something_failed || hit_a_ceiling).then_some(followed)
}

/// What went wrong, in enough detail for a model to fix a procedure.
///
/// Names and error text, not the whole transcript. A revision is a change to a
/// list of steps; the useful evidence is which step broke and what it said.
pub fn what_went_wrong(run: &crate::syn::run::Run) -> String {
    let mut lines = Vec::new();
    for step in run.steps.iter().filter(|s| s.ok == Some(false)) {
        let tool = step.tool.as_deref().unwrap_or("(unknown tool)");
        let said = step.preview.trim();
        lines.push(if said.is_empty() {
            format!("- `{tool}` failed")
        } else {
            format!("- `{tool}` failed: {}", said.chars().take(300).collect::<String>())
        });
    }
    if let Some(reason) = run.error.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        lines.push(format!("- the run stopped early: {reason}"));
    }
    if lines.is_empty() {
        "- the run was interrupted before it finished".to_string()
    } else {
        lines.join("\n")
    }
}

/// How many proposed chains are remembered.
const KEEP_CHAINS: usize = 100;

fn chains_path(vault_path: &str) -> AppResult<std::path::PathBuf> {
    let dir = std::path::Path::new(vault_path).join("Syn");
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::error::AppError::General(format!("Failed to create Syn dir: {e}")))?;
    Ok(dir.join("skill-chains.json"))
}

/// Chains that have already been written up as a skill, whatever became of it.
///
/// The trap this closes is the one P2 walked into and had to be shown by a
/// failing test: proposals that are refused come back. If the guard were "no
/// skill exists with this shape", deleting a suggested skill would invite it
/// again on the next run that repeats the same three calls — which is the run
/// right after, since that is what made it repeat in the first place.
///
/// In `Syn/` and not a dotfile: a judgement about what is worth suggesting
/// travels with the vault, the way a decline does.
pub fn proposed_chains(vault_path: &str) -> Vec<Vec<String>> {
    let Ok(path) = chains_path(vault_path) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&content).unwrap_or_else(|e| {
        log::warn!("[Syn] Proposed-chain list is unreadable, treating it as empty: {e}");
        Vec::new()
    })
}

/// Has this shape already been offered once?
pub fn already_proposed(vault_path: &str, chain: &[String]) -> bool {
    proposed_chains(vault_path).iter().any(|c| c == chain)
}

/// Record that this shape has been offered, so it is not offered again.
pub fn remember_proposed(vault_path: &str, chain: &[String]) -> AppResult<()> {
    let mut chains = proposed_chains(vault_path);
    if chains.iter().any(|c| c == chain) {
        return Ok(());
    }
    chains.insert(0, chain.to_vec());
    chains.truncate(KEEP_CHAINS);
    let path = chains_path(vault_path)?;
    std::fs::write(&path, serde_json::to_string_pretty(&chains)?)?;
    Ok(())
}

/// The body a blank skill starts with.
///
/// Not an empty file. The roadmap's first gate criterion is that somebody can
/// write a skill that changes behaviour *without having to ask anyone*, and an
/// empty file with eight frontmatter keys they have never seen fails that on
/// the first screen. This is the documentation, placed where it will be read:
/// inside the thing being edited.
///
/// It is also a working skill as written — a person who changes nothing has a
/// skill that does something small and legible, which is a better starting
/// point than one that does nothing and has to be debugged before it can be
/// judged.
pub fn starter_body() -> String {
    "## Các bước\n\
     \n\
     1. `query_nodes` với `type:task status:done` để lấy việc đã xong.\n\
     2. `create_node` một note tiêu đề `Tổng kết` chứa danh sách đó.\n\
     \n\
     ## Định dạng đầu ra\n\
     \n\
     Một danh sách gạch đầu dòng, mỗi việc một dòng, không thêm lời bình.\n\
     \n\
     ## Bài học\n\
     \n\
     (Để trống. Đây là chỗ ghi lại những lần làm sai, để lần sau không lặp lại.)\n\
     \n\
     ---\n\
     \n\
     Sửa file này thoải mái — nó là một note trong vault của bạn, có lịch sử\n\
     phiên bản và thùng rác như mọi note khác. Các khoá ở đầu file:\n\
     \n\
     - `name` — tên Syn dùng để gọi kỹ năng này.\n\
     - `description` — một dòng nói nó làm gì.\n\
     - `when_to_use` — một dòng nói khi nào nên dùng. Syn chỉ thấy hai dòng này\n\
     \u{20}\u{20}cho tới khi nó mở kỹ năng ra, nên hãy viết chúng cho rõ. Một cụm từ\n\
     \u{20}\u{20}trong ngoặc kép, như \"tổng kết tuần\", là hiệu lệnh: câu hỏi có đúng cụm\n\
     \u{20}\u{20}đó thì Syn tự đọc kỹ năng này trước khi trả lời.\n\
     - `tier` — `prose` nghĩa là Syn tự làm theo hướng dẫn.\n\
     - `enabled` — `false` thì Syn không được biết kỹ năng này tồn tại.\n"
        .to_string()
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

    fn run_that_opened(names: &[&str], created_at: &str, ok: bool) -> crate::syn::run::Run {
        let mut run = crate::syn::run::Run::new(
            "test",
            None,
            crate::syn::run::Budget::from_settings(&crate::models::syn::SynSettings::default()),
        );
        run.created_at = created_at.to_string();
        for (i, name) in names.iter().enumerate() {
            run.record_tool(
                i as u8,
                LOAD_TOOL,
                serde_json::json!({ "name": name }),
                ok,
                crate::syn::registry::Reversal::Nothing,
                "{}",
                1,
            );
        }
        run
    }

    /// The number this whole feature has to answer for.
    ///
    /// A skill can be enabled, indexed, well written and never once opened, and
    /// nothing else in the app would say so: a model deciding to skip a skill
    /// leaves no trace. `recall` sat in exactly that state for weeks while its
    /// unit tests passed.
    #[test]
    fn a_skill_that_is_never_opened_appears_nowhere_in_the_usage() {
        let runs = vec![
            run_that_opened(&["weekly-review"], "2026-09-04T10:00:00Z", true),
            run_that_opened(&["weekly-review", "inbox-zero"], "2026-09-02T10:00:00Z", true),
        ];

        let used = usage(&runs);
        let names: Vec<&str> = used.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, vec!["weekly-review", "inbox-zero"], "newest use first");

        let weekly = &used[0];
        assert_eq!(weekly.runs, 2, "counted across runs");
        assert_eq!(weekly.last_at, "2026-09-04T10:00:00Z", "and dated by the newest");

        assert!(
            !used.iter().any(|u| u.name == "never-used"),
            "a skill nobody opened is absent, which is the point of the number"
        );
    }

    /// A failed open is not a use.
    #[test]
    fn asking_for_a_skill_and_getting_an_error_does_not_count() {
        let runs = vec![run_that_opened(&["typo-name"], "2026-09-04T10:00:00Z", false)];
        assert!(
            usage(&runs).is_empty(),
            "the model asked and got nothing back; it has not used a skill"
        );
    }

    fn run_calling(tools: &[&str]) -> crate::syn::run::Run {
        let mut run = crate::syn::run::Run::new(
            "test",
            None,
            crate::syn::run::Budget::from_settings(&crate::models::syn::SynSettings::default()),
        );
        for (i, tool) in tools.iter().enumerate() {
            run.record_tool(
                i as u8,
                tool,
                serde_json::json!({}),
                true,
                crate::syn::registry::Reversal::Nothing,
                "{}",
                1,
            );
        }
        run
    }

    /// A repeated chain is what a skill is for.
    #[test]
    fn a_sequence_done_twice_is_worth_writing_down() {
        let run = run_calling(&[
            "query_nodes", "get_node", "create_node",
            "query_nodes", "get_node", "create_node",
        ]);
        assert_eq!(
            repeated_chain(&run).expect("a chain"),
            vec!["query_nodes", "get_node", "create_node"]
        );
    }

    /// The longest repetition wins, not the shortest one inside it.
    ///
    /// A run that did five steps twice should be named by the five. Returning
    /// the three in the middle would propose a skill that does part of a job
    /// and stops, which is worse than proposing nothing.
    #[test]
    fn the_whole_repeated_shape_is_taken_not_a_piece_of_it() {
        let run = run_calling(&[
            "query_nodes", "get_node", "update_node", "create_node", "list_versions",
            "query_nodes", "get_node", "update_node", "create_node", "list_versions",
        ]);
        assert_eq!(repeated_chain(&run).expect("a chain").len(), 5);
    }

    /// Two steps is a coincidence, not a procedure.
    #[test]
    fn a_pair_repeated_is_not_a_skill() {
        let run = run_calling(&["query_nodes", "get_node", "query_nodes", "get_node"]);
        assert!(
            repeated_chain(&run).is_none(),
            "`query_nodes` then `get_node` is what half of all answers look like"
        );
    }

    /// Overlapping does not count as twice.
    ///
    /// `a b a b a` contains `a b a` at two positions, but the second reading
    /// reuses a step the first already claimed. Somebody watching would see one
    /// wobble, not two passes, and a skill proposed off it would be noise.
    #[test]
    fn a_chain_that_only_repeats_by_overlapping_itself_is_not_repeated() {
        let run = run_calling(&["a", "b", "a", "b", "a"]);
        assert!(repeated_chain(&run).is_none());
    }

    /// Failed calls are not part of a procedure.
    #[test]
    fn a_chain_of_errors_is_not_a_skill() {
        let mut run = run_calling(&["query_nodes", "get_node", "create_node"]);
        for tool in ["query_nodes", "get_node", "create_node"] {
            run.record_tool(
                9,
                tool,
                serde_json::json!({}),
                false,
                crate::syn::registry::Reversal::Nothing,
                "{}",
                1,
            );
        }
        assert!(
            repeated_chain(&run).is_none(),
            "a sequence that failed is not a sequence worth repeating"
        );
    }

    /// The template has to teach the thing the gate says nobody may have to ask.
    ///
    /// P3's first criterion is that somebody writes a skill that changes
    /// behaviour *without asking anyone*. An empty file with eight unfamiliar
    /// frontmatter keys fails that on the first screen, so the documentation
    /// lives inside the file being edited — and this checks it still names the
    /// keys that actually do something, rather than drifting into prose about
    /// keys that were renamed two refactors ago.
    #[test]
    fn a_new_skill_explains_itself_in_the_file() {
        let body = starter_body();
        for key in ["name", "description", "when_to_use", "tier", "enabled"] {
            assert!(
                body.contains(key),
                "the template should say what `{key}` is for, since nothing else will"
            );
        }
        assert!(
            body.contains("prose"),
            "and name the tier a hand-written skill actually gets"
        );
    }

    /// A blank skill is still a skill: parsed, named, and off.
    #[test]
    fn the_template_parses_into_something_usable() {
        let made = Skill::from_node(&node(
            "tong-ket-tuan",
            &starter_body(),
            frontmatter("tong-ket-tuan", "", "", Tier::Prose, &[], "user", false, 1),
        ));

        assert_eq!(made.name, "tong-ket-tuan");
        assert_eq!(made.author, "user");
        assert!(!made.enabled, "nothing arrives switched on");
        assert!(made.may_be_enabled(), "but it is theirs to switch on at once");
        assert!(!made.body.trim().is_empty());
    }

    /// Who may turn a skill on.
    ///
    /// The roadmap's steps 3 and 4 say a skill Syn wrote must be tried and then
    /// reviewed before it is enabled, and that neither may be skipped. This is
    /// the half a rule can carry: until a trial has happened, the answer is no.
    #[test]
    fn a_skill_syn_wrote_cannot_be_enabled_until_it_has_been_tried() {
        let mut mine = skill("weekly-review", false);
        mine.author = "user".into();
        assert!(mine.may_be_enabled(), "a skill I wrote is mine to switch on");

        let mut theirs = skill("weekly-review", false);
        theirs.author = "syn".into();
        assert!(
            !theirs.may_be_enabled(),
            "a procedure nobody has watched run is not one to trust on its own summary"
        );

        theirs.trial_at = Some("2026-09-04".into());
        assert!(theirs.may_be_enabled(), "and after a trial, it is theirs to judge");
    }

    fn run_following(skill: Option<&str>, failures: &[&str]) -> crate::syn::run::Run {
        let mut run = crate::syn::run::Run::new(
            "test",
            None,
            crate::syn::run::Budget::from_settings(&crate::models::syn::SynSettings::default()),
        );
        if let Some(name) = skill {
            run.record_tool(
                0,
                LOAD_TOOL,
                serde_json::json!({ "name": name }),
                true,
                crate::syn::registry::Reversal::Nothing,
                "{}",
                1,
            );
        }
        for tool in failures {
            run.record_tool(
                1,
                tool,
                serde_json::json!({}),
                false,
                crate::syn::registry::Reversal::Nothing,
                "{\"error\":\"no such field\"}",
                1,
            );
        }
        run
    }

    /// A revision is proposed only where there is something to learn from.
    #[test]
    fn only_a_run_that_followed_a_skill_and_still_went_wrong_asks_for_a_revision() {
        assert_eq!(
            skill_that_struggled(&run_following(Some("weekly-review"), &["query_nodes"])).as_deref(),
            Some("weekly-review"),
        );

        assert!(
            skill_that_struggled(&run_following(Some("weekly-review"), &[])).is_none(),
            "a skill that worked teaches nothing"
        );
        assert!(
            skill_that_struggled(&run_following(None, &["query_nodes"])).is_none(),
            "a run that went wrong without following a skill has no skill to blame"
        );
    }

    /// What the model is shown is the broken step, not the whole transcript.
    #[test]
    fn the_evidence_is_the_step_that_broke_and_what_it_said() {
        let told = what_went_wrong(&run_following(Some("s"), &["query_nodes", "update_node"]));
        assert!(told.contains("query_nodes") && told.contains("update_node"));
        assert!(told.contains("no such field"), "including what it said: {told}");
    }

    /// A shape offered once is never offered again.
    ///
    /// Not "no skill exists with this shape": deleting a suggested skill would
    /// then invite it back on the next run that repeats the same three calls,
    /// which is the very next run — repeating is what caused the suggestion.
    /// P2 had to be shown this by a failing test; this one is written first.
    #[test]
    fn a_chain_already_written_up_is_not_offered_twice() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path().to_str().expect("utf8");
        let chain = vec!["query_nodes".to_string(), "get_node".to_string(), "create_node".to_string()];

        assert!(!already_proposed(vault, &chain), "nothing offered yet");
        remember_proposed(vault, &chain).expect("recorded");
        assert!(already_proposed(vault, &chain), "and it stays recorded");

        // Even after the skill it produced is gone.
        remember_proposed(vault, &chain).expect("idempotent");
        assert_eq!(proposed_chains(vault).len(), 1, "recorded once, not twice");

        let other = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(!already_proposed(vault, &other), "a different shape is still new");
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
                model: None,
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


/// Would Syn have offered to write a skill, on the runs that actually happened?
///
/// A probe, not an assertion. It reads the real vault and uses the real
/// detector, because replicating the rule in a script to measure it would be a
/// second implementation free to disagree with the first — which is how a
/// measurement ends up being the broken thing.
///
/// ```bash
/// cargo test --lib what_syn_would_have_offered -- --ignored --nocapture
/// ```
#[cfg(test)]
mod what_syn_would_have_offered {
    use super::*;

    #[test]
    #[ignore = "reads the real vault; run by hand"]
    fn on_the_runs_that_actually_happened() {
        let vault = std::env::var("SYN_EVAL_VAULT").unwrap_or_else(|_| {
            format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default())
        });
        let runs = crate::syn::run::load_all(&vault).expect("the real runs");

        eprintln!("\n═══ would a skill have been proposed? ═══");
        eprintln!("runs on disk: {}\n", runs.len());

        let mut offered = 0;
        for run in &runs {
            let calls: Vec<&str> = run
                .steps
                .iter()
                .filter(|s| s.ok == Some(true))
                .filter_map(|s| s.tool.as_deref())
                .collect();
            match repeated_chain(run) {
                Some(chain) => {
                    offered += 1;
                    eprintln!("  {} → {}", &run.id[..8], chain.join(" → "));
                }
                None if calls.len() >= MIN_CHAIN * 2 => {
                    eprintln!("  {} → {} calls, no repetition", &run.id[..8], calls.len());
                }
                None => {}
            }
        }
        eprintln!("\n{offered} of {} runs would have prompted a skill.", runs.len());
        eprintln!("(runs with fewer than {} successful calls cannot, and are not listed)\n", MIN_CHAIN * 2);
    }
}

/// Whether the harness picks the right skill, and — the half that matters
/// more — whether it leaves the wrong ones alone.
///
/// The skills are the kind a person actually writes, in both languages, and
/// the questions are the kind they actually ask. The near misses are the
/// point: each shares words with a skill and asks for something else, and a
/// matcher that took them would be putting a procedure in the prompt for a
/// question it does not answer.
#[cfg(test)]
mod the_harness_picks {
    use super::*;

    fn skill(name: &str, description: &str, when: &str, enabled: bool) -> Skill {
        Skill::from_node(&NodeMetadata {
            id: format!("{SKILL_FOLDER}/{name}.md"),
            node_type: SKILL_TYPE.to_string(),
            title: name.to_string(),
            content: format!("## Các bước\n1. Làm việc của `{name}`.\n"),
            properties: serde_json::json!({
                "name": name,
                "description": description,
                "when_to_use": when,
                "tier": "prose",
                "enabled": enabled,
            }),
            created_at: "2026-09-01T00:00:00Z".into(),
            updated_at: "2026-09-01T00:00:00Z".into(),
            timestamp: 0,
            blocks: None,
        })
    }

    /// A library like one a person would have after a few weeks.
    fn library() -> Vec<Skill> {
        vec![
            skill(
                "tong-ket-tuan",
                "Tổng kết tuần từ task đã xong và lịch.",
                "Khi người dùng nói \"tổng kết tuần\" hoặc hỏi tuần này đã làm được gì.",
                true,
            ),
            skill(
                "bien-ban-hop",
                "Biến ghi chép cuộc họp thành biên bản có quyết định và việc cần làm.",
                "Sau một cuộc họp, khi người dùng dán ghi chép họp.",
                true,
            ),
            skill(
                "chi-tieu-thang",
                "Tổng hợp chi tiêu tháng theo danh mục.",
                "Khi hỏi \"tháng này tiêu bao nhiêu\".",
                true,
            ),
            skill(
                "weekly-review",
                "Summarise the week from finished tasks and the calendar.",
                "When the user asks for a weekly review.",
                true,
            ),
            skill(
                "meeting-notes",
                "Turn raw meeting notes into a summary with decisions and action items.",
                "After a meeting, when the user pastes notes.",
                true,
            ),
        ]
    }

    fn picked(skills: &[Skill], question: &str) -> Option<String> {
        chosen_for(skills, question).map(|s| s.name.clone())
    }

    fn scores(skills: &[Skill], question: &str) -> Vec<(String, usize)> {
        skills.iter().map(|s| (s.name.clone(), match_score(s, question))).collect()
    }

    #[test]
    fn a_question_that_plainly_asks_for_a_skill_gets_it() {
        let skills = library();
        for (question, want) in [
            // The author's own quoted trigger, with and without marks.
            ("Tổng kết tuần này giúp tao", "tong-ket-tuan"),
            ("tong ket tuan nay di", "tong-ket-tuan"),
            ("Tháng này tiêu bao nhiêu rồi?", "chi-tieu-thang"),
            // No quote to lean on: the words and the pairs they make.
            ("Đây là ghi chép cuộc họp sáng nay, làm biên bản giúp tao", "bien-ban-hop"),
            ("Can you do my weekly review?", "weekly-review"),
            ("Summarize these meeting notes I pasted", "meeting-notes"),
        ] {
            assert_eq!(
                picked(&skills, question).as_deref(),
                Some(want),
                "`{question}` scored {:?}",
                scores(&skills, question)
            );
        }
    }

    /// Each of these shares words with a skill and asks for something else.
    #[test]
    fn a_question_that_only_shares_words_with_a_skill_gets_none() {
        let skills = library();
        for question in [
            // "Tổng kết" is one compound, and it fits a week and a meeting alike.
            "Tổng kết cuộc họp hôm qua",
            // The week, but its future, not its review.
            "Tuần sau có lịch gì không?",
            // `review`, of something else entirely.
            "Review this pull request for me",
            // The calendar and the week, and still not a review.
            "What's on my calendar this week?",
            // Notes, not a meeting's.
            "Take notes on this article",
            "When is my next meeting?",
            // Nothing to do with any of them.
            "Minh sinh năm bao nhiêu?",
            "",
        ] {
            assert_eq!(
                picked(&skills, question),
                None,
                "`{question}` scored {:?}",
                scores(&skills, question)
            );
        }
    }

    /// The user turning a skill off is the one control they have over what
    /// Syn reaches for. The harness does not get to override it, even on the
    /// skill's own trigger phrase.
    #[test]
    fn a_disabled_skill_is_never_picked() {
        let mut skills = library();
        for s in skills.iter_mut() {
            if s.name == "tong-ket-tuan" {
                s.enabled = false;
            }
        }
        assert_eq!(picked(&skills, "tổng kết tuần"), None);
        assert_eq!(picked(&skills, "Tổng kết tuần này giúp tao"), None);
    }

    /// Two skills that both nearly fit is the harness not knowing, and it says
    /// nothing rather than guess. The index still lists both.
    #[test]
    fn two_skills_that_fit_alike_means_neither_is_picked() {
        let mut skills = library();
        skills.push(skill(
            "tong-ket-tuan-cho-sep",
            "Tổng kết tuần gửi sếp.",
            "Khi người dùng nói \"tổng kết tuần\".",
            true,
        ));
        assert_eq!(picked(&skills, "tổng kết tuần"), None, "{:?}", scores(&skills, "tổng kết tuần"));
    }

    /// A long message is mostly material, and its words say what the material
    /// is about rather than what to do with it.
    #[test]
    fn a_long_pasted_message_is_matched_only_by_a_quoted_trigger() {
        let skills = library();
        let pasted = format!(
            "Summary of the meeting notes: {}",
            "the team reviewed calendar decisions and action items for the weekly roadmap sync, \
             owners were named for every open question and dates agreed "
                .repeat(3)
        );
        assert_eq!(
            picked(&skills, &pasted),
            None,
            "a wall of matching words is not a request: {:?}",
            scores(&skills, &pasted)
        );

        let asked = format!(
            "tổng kết tuần giúp tao, đây là ghi chú: {}",
            "khách hàng phản hồi về giao diện mới và đội thiết kế sửa lại màu sắc "
                .repeat(4)
        );
        assert_eq!(picked(&skills, &asked).as_deref(), Some("tong-ket-tuan"));
    }

    /// A body too long to put in every matching prompt is left to `load_skill`,
    /// and one with nothing in it is not worth a section.
    #[test]
    fn a_body_too_long_or_empty_is_left_to_load_skill() {
        let mut long = skill("tong-ket-tuan", "Tổng kết tuần.", "Khi nói \"tổng kết tuần\".", true);
        assert_eq!(picked(std::slice::from_ref(&long), "tổng kết tuần").as_deref(), Some("tong-ket-tuan"));

        long.body = "x".repeat(CHOSEN_BODY_CHARS + 1);
        assert_eq!(picked(std::slice::from_ref(&long), "tổng kết tuần"), None);

        long.body = "  \n".into();
        assert_eq!(picked(std::slice::from_ref(&long), "tổng kết tuần"), None);
    }

    /// The section says what it is, how it got there and that it may be set
    /// aside — and the name can be read back off it for the numbers.
    #[test]
    fn the_chosen_block_explains_itself_and_names_its_skill() {
        let skills = library();
        let block = chosen_block(&skills[0]);
        assert!(block.contains("A SKILL FOR THIS QUESTION"));
        assert!(block.contains("ignore it"), "it may be set aside:\n{block}");
        assert!(block.contains(LOAD_TOOL), "and does not need loading:\n{block}");
        assert!(block.contains("Làm việc của `tong-ket-tuan`"), "the steps are there:\n{block}");
        assert_eq!(chosen_name(&block).as_deref(), Some("tong-ket-tuan"));
        assert_eq!(chosen_name("=== WHAT YOU KNOW HOW TO DO ==="), None);

        let mut recipe = skills[0].clone();
        recipe.tier = Tier::Recipe;
        assert!(chosen_block(&recipe).contains(crate::syn::recipe::RUN_TOOL));
    }

    /// The whole path: the library, the question, and the prompt the model
    /// gets — the picked skill's steps in this turn's half, after the date
    /// and before anything retrieved, and nothing in the cacheable half moved.
    #[test]
    fn the_picked_skill_reaches_the_prompt_in_this_turns_half() {
        use crate::syn::prompt::{ChatPrompt, PromptPlan, DEFAULT_BUDGET_CHARS, TURN_HEADING};

        let skills = library();
        let index = index_block(&skills, INDEX_BUDGET_CHARS);
        let chosen = chosen_for(&skills, "tổng kết tuần này").map(chosen_block);
        assert!(chosen.is_some());

        let base = || {
            PromptPlan::for_chat(ChatPrompt {
                context: "- [[Ghi chú]] một đoạn",
                custom: None,
                skills: index.as_deref(),
                memory: None,
                focus: None,
                thread: None,
                counted: None,
                timeline: None,
                budget_chars: DEFAULT_BUDGET_CHARS,
            })
        };
        let without = base();
        let with = base().with_chosen_skill(chosen.as_deref());
        let rendered = with.render();

        let seam = rendered.find(TURN_HEADING).expect("the seam");
        let skill_at = rendered.find("A SKILL FOR THIS QUESTION").expect("the skill is sent");
        let context_at = rendered.find("VAULT CONTEXT").expect("context is sent");
        assert!(seam < skill_at && skill_at < context_at, "{rendered}");
        assert_eq!(
            with.stable_prefix(),
            without.stable_prefix(),
            "a skill chosen per question must not move the cached half"
        );

        // Nothing matched, nothing added.
        let unmatched = chosen_for(&skills, "Minh sinh năm nào?").map(chosen_block);
        let none = base().with_chosen_skill(unmatched.as_deref());
        assert_eq!(none.render(), without.render());
    }
}
