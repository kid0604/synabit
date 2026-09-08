//! How long a question is going to take, decided before it is asked.
//!
//! # The problem, measured
//!
//! *"Có bao nhiêu task chưa xong"* and *"đọc hết feed tuần này rồi tổng hợp"* go
//! through the same twelve-round loop. The first one is a single count against
//! an index that already holds the answer, and it pays for the second: ask the
//! model, get a `query_nodes` back, run it, ask again, get an answer. Two full
//! round trips for a number the database can produce in a millisecond.
//!
//! A colleague answers the first instantly and says *"give me a few minutes"* to
//! the second. That is not speed for its own sake — it is **knowing how heavy
//! the work is**, and saying so.
//!
//! # Where the design this came from was wrong
//!
//! `docs/syn-from-assistant-to-colleague-2026-09-06.md` says an instant question
//! is one that "parses into a structured query with no significant free text",
//! and points at `search::parse_query`. That is not what `parse_query` does with
//! prose. It reads *query syntax* — `type:task status:todo` — and a person
//! typing a question writes none of it. Fed "có bao nhiêu task chưa xong" it
//! returns six free-text terms and no filters at all.
//!
//! So the premise had to be replaced rather than implemented. What makes this
//! tractable is a fact about the app rather than about parsing:
//!
//! > **The Vietnamese interface keeps the type names in English.** `vi.json`
//! > says *"Tasks quá hạn"* and *"Viết note mới"*, the same way it keeps
//! > `vault`, `token`, `prompt` and `recipe`. So a Vietnamese question about
//! > tasks contains the literal word `task`, and the types themselves come from
//! > the vault via `observed_schemas` rather than from a list in the code.
//!
//! That leaves two things to recognise, and both are short lists: **the shape of
//! a counting question**, and **a type this vault actually has**.
//!
//! # Why being wrong here is cheap
//!
//! Because the fast path is not a different answer, it is a different amount of
//! work. Misjudging a question as instant costs one round with no tools, after
//! which the ordinary loop runs anyway — the model simply says it needs to look.
//! Misjudging one as ordinary costs what everything costs today. Neither
//! produces a wrong answer, which is why this may be arithmetic rather than a
//! model call.
//!
//! A model call would also be self-defeating: spending an inference round to
//! decide whether to spend inference rounds turns every instant question into an
//! ordinary one.

use crate::db::DbBridge;
use crate::error::AppResult;
use crate::search::ParsedQuery;

/// How much work a question is expected to be.
///
/// Two, not the three the design names. `Background` belongs to the phase that
/// builds triggers; an arm with nothing producing it is a claim the code cannot
/// keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tempo {
    /// The index already holds the answer. One round, no tools.
    Instant,
    /// Everything else: the loop, with tools, for as long as the budget allows.
    Working,
}

/// How far into a question the counting words are looked for.
///
/// The whole message, unlike `correction::OPENING_CHARS`. A correction is an
/// opening move and a question is not: *"trong vault của tao có bao nhiêu task
/// chưa xong"* puts the counting words in the middle, which is ordinary.
///
/// The cap is against a pasted document rather than against a sentence.
const MAX_QUESTION_CHARS: usize = 400;

/// Words that ask for a number or a list.
///
/// Bilingual, and each is a phrase rather than a bare word where a bare word
/// would be ambiguous. `list` alone appears in *"list out the steps"*, which is
/// not a question about the vault — but paired with a type name below, it is.
const COUNTING: &[&str] = &[
    // Vietnamese
    "bao nhiêu",
    "bn ",
    "mấy ",
    "liệt kê",
    "đếm",
    "có những",
    "danh sách",
    "tổng cộng",
    // English
    "how many",
    "how much",
    "count ",
    "list ",
    "number of",
    "total ",
];

/// Words that narrow a count to unfinished work.
///
/// Only `status`, and only for tasks. Every other filter a question might imply
/// — a date range, a project, a tag — needs judgement about what the words mean,
/// and judgement is what the ordinary loop is for. This one is here because
/// "how many tasks" almost always means the ones that are not done, and getting
/// it wrong is the difference between 4 and 126 — a number the doc comment on
/// `node_query.rs` records the assistant getting wrong in production.
const UNFINISHED: &[&str] = &[
    "chưa xong",
    "chưa làm",
    "còn lại",
    "chưa hoàn thành",
    "quá hạn",
    "not done",
    "unfinished",
    "outstanding",
    "left to do",
    "overdue",
    "remaining",
];

/// A question that can be answered from the index, and the query that answers it.
#[derive(Debug, Clone, PartialEq)]
pub struct Instant {
    /// The type being counted, as the vault spells it.
    pub node_type: String,
    /// Whether the question asked only for unfinished ones.
    pub unfinished: bool,
}

fn normalised(message: &str) -> String {
    message
        .trim()
        .chars()
        .take(MAX_QUESTION_CHARS)
        .collect::<String>()
        .to_lowercase()
}

/// Whether the message asks for a count or a list.
fn asks_for_a_count(text: &str) -> bool {
    COUNTING.iter().any(|w| text.contains(w))
}

/// The vault type this question names, if it names exactly one.
///
/// *Exactly* one, on purpose. "how many tasks and events" is two questions, and
/// answering half of it quickly is worse than answering all of it properly.
///
/// Types come from the vault rather than from a list here, so a person who
/// invented `book` gets the fast path for books without anybody adding them.
/// Longest first, so `finance_month` is not matched as `finance`.
fn type_named<'a>(text: &str, types: &'a [String]) -> Option<&'a str> {
    let mut sorted: Vec<&String> = types.iter().collect();
    sorted.sort_by_key(|t| std::cmp::Reverse(t.len()));

    let mut found: Option<&str> = None;
    for candidate in sorted {
        // Both the type and its plural, because people say "tasks".
        let hit = text.contains(candidate.as_str())
            || text.contains(&format!("{candidate}s"));
        if !hit {
            continue;
        }
        // A longer type already matched and contains this one — `finance_month`
        // matched, so `finance` matching too is the same hit read twice.
        if found.is_some_and(|f| f.contains(candidate.as_str())) {
            continue;
        }
        if found.is_some() {
            return None; // two different types named: not one question.
        }
        found = Some(candidate);
    }
    found
}

/// Which tempo this question is, and what to run if it is instant.
///
/// `types` is what the vault holds, from `observed_schemas`. Internal types are
/// expected to be filtered out by the caller: nobody asks how many `json` they
/// have, and `syn_memory` is Syn's own bookkeeping.
pub fn of(message: &str, types: &[String]) -> Option<Instant> {
    let text = normalised(message);
    if !asks_for_a_count(&text) {
        return None;
    }
    let node_type = type_named(&text, types)?;

    Some(Instant {
        node_type: node_type.to_string(),
        unfinished: UNFINISHED.iter().any(|w| text.contains(w)),
    })
}

/// The query that answers it.
///
/// Built as a `ParsedQuery` directly rather than by writing `type:task` and
/// parsing it back — the struct is the interface, and going through the text
/// form would mean depending on the query language's spelling to ask a question
/// in Rust.
pub fn query_for(instant: &Instant) -> ParsedQuery {
    ParsedQuery {
        type_filter: Some(instant.node_type.clone()),
        // `-status:done` rather than `status:todo`, and the difference is
        // load-bearing: `node_query.rs` records the assistant answering 7 and 0
        // for a real number of 4, because a task that never had a status at all
        // satisfies "not finished" and does not satisfy "is todo".
        property_exclusions: if instant.unfinished {
            vec![("status".to_string(), "done".to_string())]
        } else {
            Vec::new()
        },
        ..Default::default()
    }
}

/// The types a question could plausibly be about.
///
/// Read from the vault, minus the ones that are storage rather than something
/// anybody keeps — the same list `list_schemas` hides for the same reason.
pub fn countable_types(db: &DbBridge) -> AppResult<Vec<String>> {
    Ok(db
        .observed_schemas(1)?
        .into_iter()
        .map(|(node_type, _, _)| node_type)
        .filter(|t| !crate::syn::tools::is_internal_type(t))
        .collect())
}

/// A few of the matching titles, so the answer can name some of them.
///
/// A handful, not the page: the number is the answer and the titles are
/// courtesy. Listing forty would also invite the model to read the list as the
/// set, which is the failure the block below is written against.
pub fn sample(result: &crate::db::QueryResult) -> String {
    const SHOWN: usize = 5;

    if result.rows.is_empty() {
        return String::new();
    }
    let titles: Vec<&str> = result
        .rows
        .iter()
        .take(SHOWN)
        .map(|r| r.title.as_str())
        .collect();

    let more = result.total.saturating_sub(titles.len());
    let tail = if more > 0 {
        format!(" (and {more} more, not listed)")
    } else {
        String::new()
    };
    format!("Some of them: {}{tail}\n", titles.join(", "))
}

/// What the prompt is told when the answer was fetched before it was asked for.
///
/// It says the number is exact and where it came from, because the failure this
/// replaces is the model reading a sample of rows as if it were the whole set —
/// `node_query.rs` has it reporting "2 tasks out of 126" from a page of two.
pub fn block(instant: &Instant, total: usize, sample: &str) -> String {
    let scope = if instant.unfinished {
        format!("`{}` that are not done", instant.node_type)
    } else {
        format!("`{}`", instant.node_type)
    };

    format!(
        "\n=== ALREADY COUNTED ===\n\
         This question was recognised as a count, so the query was run before you \
         were asked. There are **{total}** {scope} in the vault. That number is the \
         database's own total, not the length of a list — use it exactly as it is.\n\
         {sample}\n\
         Answer from this. You have no tools on this turn; if the question needs \
         something this does not cover, say what you would have to look up and the \
         user will ask again.\n\
         === END COUNT ===\n\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types() -> Vec<String> {
        ["note", "task", "event", "person", "project", "book", "finance_month"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// The question this whole cut exists for.
    #[test]
    fn a_count_over_a_type_the_vault_has_is_instant() {
        let got = of("có bao nhiêu task chưa xong", &types()).expect("instant");
        assert_eq!(got.node_type, "task");
        assert!(got.unfinished);
    }

    #[test]
    fn it_works_in_both_languages() {
        for asked in [
            "how many tasks are not done?",
            "có bao nhiêu task chưa xong",
            "liệt kê task quá hạn",
            "count the tasks left to do",
        ] {
            let got = of(asked, &types()).unwrap_or_else(|| panic!("missed: {asked}"));
            assert_eq!(got.node_type, "task", "{asked}");
            assert!(got.unfinished, "{asked}");
        }
    }

    /// Without a narrowing word it is every one of them, which is a different
    /// and equally answerable question.
    #[test]
    fn a_plain_count_is_not_narrowed() {
        let got = of("tao có bao nhiêu note", &types()).expect("instant");
        assert_eq!(got.node_type, "note");
        assert!(!got.unfinished);
    }

    /// The lucky fact this is built on: the Vietnamese interface keeps the type
    /// names in English, so a Vietnamese question contains the English word.
    #[test]
    fn a_vietnamese_question_still_names_the_type_in_english() {
        assert!(of("bao nhiêu task", &types()).is_some());
        assert!(of("liệt kê giúp tao mấy event tuần này", &types()).is_some());
    }

    /// Types come from the vault, so a kind nobody wrote code for gets the fast
    /// path without anybody adding it here.
    #[test]
    fn a_type_the_user_invented_counts_too() {
        let mine = vec!["animal".to_string(), "note".to_string()];
        assert_eq!(of("how many animals do I have", &mine).unwrap().node_type, "animal");
    }

    #[test]
    fn a_type_this_vault_does_not_have_is_not_instant() {
        assert!(of("how many recipes do I have", &types()).is_none());
    }

    /// Answering half of a two-part question quickly is worse than answering
    /// all of it properly.
    #[test]
    fn two_types_named_is_not_one_question() {
        assert!(of("how many tasks and events do I have", &types()).is_none());
    }

    /// `finance_month` must not be read as `finance` matching separately.
    #[test]
    fn a_longer_type_is_not_double_counted_as_its_prefix() {
        let with_prefix = vec!["finance".to_string(), "finance_month".to_string()];
        assert_eq!(
            of("how many finance_month entries", &with_prefix).unwrap().node_type,
            "finance_month",
        );
    }

    /// Everything that is not a count. Being wrong here only costs a round, but
    /// the direction still matters: an instant turn has no tools.
    #[test]
    fn ordinary_questions_are_not_instant() {
        for asked in [
            "tóm tắt giúp tao note về pricing",
            "viết một task mới cho thứ Sáu",
            "what did I decide about pricing, and who disagreed?",
            "đọc hết feed tuần này rồi tổng hợp",
            "sửa lại đoạn này cho gọn",
            "",
        ] {
            assert!(of(asked, &types()).is_none(), "wrongly instant: {asked}");
        }
    }

    /// A counting word with no type is a question about something else.
    #[test]
    fn counting_without_a_type_is_not_instant() {
        assert!(of("how many should I do today?", &types()).is_none());
        assert!(of("liệt kê các bước", &types()).is_none());
    }

    /// The exclusion, not the equality. A task that never had a status is
    /// unfinished, and `status:todo` says it is not — the difference between
    /// 4 and 0 in a number this app got wrong in production.
    #[test]
    fn unfinished_asks_for_not_done_rather_than_todo() {
        let q = query_for(&Instant { node_type: "task".into(), unfinished: true });

        assert_eq!(q.type_filter.as_deref(), Some("task"));
        assert_eq!(q.property_exclusions, vec![("status".into(), "done".into())]);
        assert!(q.status_filter.is_none(), "never the equality form");
    }

    #[test]
    fn a_plain_count_filters_only_by_type() {
        let q = query_for(&Instant { node_type: "note".into(), unfinished: false });
        assert_eq!(q.type_filter.as_deref(), Some("note"));
        assert!(q.property_exclusions.is_empty());
    }

    /// The saving, stated as the property that produces it.
    ///
    /// An instant turn runs against a registry with nothing in it and a budget
    /// of one round. Both halves matter: tools alone would let the model spend a
    /// round reaching for one, and rounds alone would let it ask for a tool that
    /// does not exist. The pair is what makes it one round trip instead of two.
    #[test]
    fn an_instant_turn_has_no_tools_and_one_round() {
        let source = include_str!("../commands/syn.rs");
        let block = source
            .split("let instant = counted.is_some();")
            .nth(1)
            .expect("the instant decision is still made")
            .split("let mut run = Run::new")
            .next()
            .expect("and a run is still built after it");

        assert!(block.contains("Registry::none()"), "no tools:\n{block}");
        assert!(block.contains("iterations = Some(1)"), "one round:\n{block}");
    }

    /// A count that reaches the model must not also be droppable, or a tight
    /// budget produces a turn with no tools and no answer.
    #[test]
    fn the_count_is_never_dropped_to_save_room() {
        assert!(crate::syn::prompt::SectionKind::Counted.is_required());
    }

    /// The sample is courtesy; the total is the answer. Reading the list as the
    /// set is the failure `node_query.rs` records in production.
    #[test]
    fn the_sample_says_how_many_it_left_out() {
        let result = crate::db::QueryResult {
            columns: Vec::new(),
            rows: (0..3)
                .map(|i| crate::db::QueryRow {
                    id: format!("Tasks/{i}.md"),
                    node_type: "task".into(),
                    title: format!("task {i}"),
                    cells: Vec::new(),
                })
                .collect(),
            total: 9,
            query_time_ms: 0,
        };

        let sample = sample(&result);
        assert!(sample.contains("task 0"), "{sample}");
        assert!(sample.contains("and 6 more"), "{sample}");
    }

    #[test]
    fn nothing_matching_produces_no_sample() {
        let empty = crate::db::QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            total: 0,
            query_time_ms: 0,
        };
        assert!(sample(&empty).is_empty());
    }

    /// The number has to be presented as the total, or the model reports the
    /// length of the sample — which `node_query.rs` records it doing.
    #[test]
    fn the_block_says_the_number_is_the_real_total() {
        let block = block(&Instant { node_type: "task".into(), unfinished: true }, 4, "");

        assert!(block.contains("**4**"), "{block}");
        assert!(block.contains("not done"), "{block}");
        assert!(
            block.contains("not the length of a list"),
            "it has to say what the number is:\n{block}"
        );
    }

    /// An instant turn has no tools, so the model needs a way out that is not
    /// making something up.
    #[test]
    fn the_block_gives_a_way_out_when_the_count_is_not_the_answer() {
        let block = block(&Instant { node_type: "note".into(), unfinished: false }, 12, "");
        assert!(block.contains("no tools on this turn"), "{block}");
        assert!(block.contains("say what you would have to look up"), "{block}");
    }
}
