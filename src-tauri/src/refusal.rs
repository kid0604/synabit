//! Why a question could not be answered, in a form a screen can translate.
//!
//! # Why this is not a `String`
//!
//! This engine's whole bet is that a question it cannot answer is **refused
//! with a sentence that teaches**, rather than quietly answered as a different
//! question. Every step of `docs/query-grammar-2026-09-20.md` leans on it.
//!
//! Those sentences were English, hard-coded in Rust, inside an app whose every
//! other word has an `en` and a `vi`. So the part of the engine that does the
//! most teaching was the one part that could not speak to the person using it.
//!
//! A refusal is therefore a **code and its arguments**. Rust keeps the English
//! — for logs, for the golden file, for callers with no screen — and the
//! locale files keep the rest. `{0}` and `{1}` are positional, which is what
//! `vue-i18n` calls list interpolation, so one template serves both sides.
//!
//! # What stops the two sides drifting
//!
//! [`tests::every_refusal_is_translated`] reads `en.json` and `vi.json` and
//! fails if a code is missing from either, or if the English there has drifted
//! from the English here. A translation table nothing checks is a translation
//! table that rots.

use std::fmt;

/// One reason, named rather than written out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    code: &'static str,
    args: Vec<String>,
}

impl Refusal {
    fn of(code: &'static str, args: impl IntoIterator<Item = String>) -> Self {
        Refusal { code, args: args.into_iter().collect() }
    }

    /// The name a locale file looks it up by.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// What goes into `{0}`, `{1}`, … on either side.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    // ── the grammar ─────────────────────────────────────────────
    pub fn renamed(was: &str, now: &str) -> Self {
        Self::of("renamed", [was.into(), now.into()])
    }
    pub fn nothing_to_join(token: &str) -> Self {
        Self::of("nothing_to_join", [token.into()])
    }
    pub fn or_needs_both_sides() -> Self {
        Self::of("or_needs_both_sides", [])
    }
    pub fn not_sortable(key: &str) -> Self {
        Self::of("not_sortable", [key.into()])
    }
    pub fn not_a_column(name: &str) -> Self {
        Self::of("not_a_column", [name.into()])
    }
    pub fn not_a_row_count(text: &str) -> Self {
        Self::of("not_a_row_count", [text.into()])
    }
    pub fn size_needs_a_comparison(value: &str) -> Self {
        Self::of("size_needs_a_comparison", [value.into()])
    }
    pub fn not_a_size(text: &str) -> Self {
        Self::of("not_a_size", [text.into()])
    }
    pub fn same_day_needs_a_day() -> Self {
        Self::of("same_day_needs_a_day", [])
    }

    // ── the pipeline's own words ────────────────────────────────
    pub fn pipe_needs_a_stage() -> Self {
        Self::of("pipe_needs_a_stage", [])
    }
    pub fn not_a_stage(word: &str) -> Self {
        Self::of("not_a_stage", [word.into()])
    }
    pub fn stage_cannot_yet(stage: &str, word: &str) -> Self {
        Self::of("stage_cannot_yet", [stage.into(), word.into()])
    }
    pub fn stage_needs_what(stage: &str) -> Self {
        Self::of("stage_needs_what", [stage.into()])
    }
    pub fn stats_needs_by() -> Self {
        Self::of("stats_needs_by", [])
    }
    pub fn seq_needs_by() -> Self {
        Self::of("seq_needs_by", [])
    }
    pub fn ask_needs_a_number() -> Self {
        Self::of("ask_needs_a_number", [])
    }
    pub fn sort_needs_a_key() -> Self {
        Self::of("sort_needs_a_key", [])
    }
    pub fn head_needs_a_number() -> Self {
        Self::of("head_needs_a_number", [])
    }
    pub fn top_needs_a_number_and_key() -> Self {
        Self::of("top_needs_a_number_and_key", [])
    }

    // ── `| where` ───────────────────────────────────────────────
    pub fn where_needs_a_comparison() -> Self {
        Self::of("where_needs_a_comparison", [])
    }
    pub fn where_not_a_comparison(left: &str) -> Self {
        Self::of("where_not_a_comparison", [left.into()])
    }
    pub fn where_nothing_on_the_right(what: &str) -> Self {
        Self::of("where_nothing_on_the_right", [what.into()])
    }
    pub fn where_unknown_operator(operator: &str) -> Self {
        Self::of("where_unknown_operator", [operator.into()])
    }
    pub fn where_bracket_open() -> Self {
        Self::of("where_bracket_open", [])
    }
    pub fn where_nothing_to_join(token: &str) -> Self {
        Self::of("where_nothing_to_join", [token.into()])
    }

    // ── what one path cannot answer and another can ─────────────
    pub fn nothing_to_match() -> Self {
        Self::of("nothing_to_match", [])
    }
    pub fn or_in_the_query_bar() -> Self {
        Self::of("or_in_the_query_bar", [])
    }
    pub fn ask_in_the_query_bar(what: &str) -> Self {
        Self::of("ask_in_the_query_bar", [what.into()])
    }
    pub fn beyond_the_search_box(what: &str) -> Self {
        Self::of("beyond_the_search_box", [what.into()])
    }
    pub fn event_field_on_nodes(field: &str) -> Self {
        Self::of("event_field_on_nodes", [field.into()])
    }
    pub fn node_field_on_events(field: &str) -> Self {
        Self::of("node_field_on_events", [field.into()])
    }

    // ── times ───────────────────────────────────────────────────
    pub fn not_a_time(text: &str, how: &str) -> Self {
        Self::of("not_a_time", [text.into(), how.into()])
    }
    pub fn not_an_anniversary(text: &str, how: &str) -> Self {
        Self::of("not_an_anniversary", [text.into(), how.into()])
    }

    // ── the pipeline, running ───────────────────────────────────
    pub fn no_day_to_gather_by(bucket: &str) -> Self {
        Self::of("no_day_to_gather_by", [bucket.into()])
    }
    pub fn no_day_to_follow() -> Self {
        Self::of("no_day_to_follow", [])
    }
    pub fn not_a_column_of_this_answer(name: &str, columns: &str) -> Self {
        Self::of("not_a_column_of_this_answer", [name.into(), columns.into()])
    }
    pub fn needs_the_vault() -> Self {
        Self::of("needs_the_vault", [])
    }
    pub fn would_spend(room: usize, lines: usize) -> Self {
        Self::of("would_spend", [room.to_string(), lines.to_string()])
    }
    pub fn ask_only_once() -> Self {
        Self::of("ask_only_once", [])
    }
}

/// Every reason, and what it says in English.
///
/// The one place a refusal is written out. `{0}` and `{1}` are filled from
/// [`Refusal::args`], in order.
pub const SAYINGS: &[(&str, &str)] = &[
    ("renamed", "'{0}' is now '{1}'"),
    ("nothing_to_join", "'{0}' has nothing to join onto"),
    ("or_needs_both_sides", "'OR' needs something on both sides"),
    ("not_sortable", "'{0}' is not something a query can sort by"),
    ("not_a_column", "'{0}' is not a column a query can show"),
    ("not_a_row_count", "'{0}' is not a number of rows"),
    (
        "size_needs_a_comparison",
        "'size:{0}' has to say which way — write size:>={0} or size:<={0}",
    ),
    ("not_a_size", "'{0}' is not a size to compare against"),
    ("same_day_needs_a_day", "same-day-as() needs a day — `same-day-as(today)`"),
    ("pipe_needs_a_stage", "'|' needs something after it"),
    ("not_a_stage", "'{0}' is not something a question can do"),
    ("stage_cannot_yet", "{0} cannot work out '{1}' yet"),
    ("stage_needs_what", "{0} needs to be told what to work out"),
    ("stats_needs_by", "stats needs `by` and something to gather under"),
    ("seq_needs_by", "seq needs `by` and something to follow through time"),
    ("ask_needs_a_number", "ask needs a number of lines to keep"),
    ("sort_needs_a_key", "sort needs something to sort by"),
    ("head_needs_a_number", "head needs a number of rows"),
    (
        "top_needs_a_number_and_key",
        "top needs a number and `by` something — `top 5 by count`",
    ),
    ("where_needs_a_comparison", "where needs something to compare"),
    ("where_not_a_comparison", "'{0}' is not a comparison — write `{0} > 5`"),
    ("where_nothing_on_the_right", "'{0}' has nothing on the right of it"),
    ("where_unknown_operator", "'{0}' is not a way of comparing two things"),
    ("where_bracket_open", "a bracket was opened and not closed in where"),
    ("where_nothing_to_join", "'{0}' has nothing to join onto in where"),
    ("nothing_to_match", "A query needs something to match on."),
    (
        "or_in_the_query_bar",
        "a question with OR in it has to be asked in the query bar",
    ),
    ("ask_in_the_query_bar", "{0} has to be asked in the query bar"),
    (
        "beyond_the_search_box",
        "{0} is more than this search box can ask. Ask it in the query bar.",
    ),
    (
        "event_field_on_nodes",
        "{0} asks about an event, and this question is about nodes. Start it with `events`, or drop it.",
    ),
    (
        "node_field_on_events",
        "{0} asks about a node, and this question is about the timeline. Ask it without the timeline's words, or drop it.",
    ),
    ("not_a_time", "'{0}' is not a time. {1}"),
    ("not_an_anniversary", "'{0}' is not a day to take the anniversary of. {1}"),
    (
        "no_day_to_gather_by",
        "there is no day in this answer to gather by {0}. Ask for one — `columns:when,title` — or gather by a field instead.",
    ),
    (
        "no_day_to_follow",
        "there is no day in this answer to follow through time. Ask for one — `columns:when,who`.",
    ),
    (
        "not_a_column_of_this_answer",
        "'{0}' is not one of the columns of this answer ({1}). Ask for it with columns: first.",
    ),
    (
        "needs_the_vault",
        "reading the notes' own words needs the vault, and this question was asked somewhere there is none",
    ),
    (
        "would_spend",
        "`ask {0}` would send {1} lines to a model. A question that spends money is not run by opening it — ask for it deliberately.",
    ),
    (
        "ask_only_once",
        "a question may ask once. Two calls to a model in one question cost twice and explain half.",
    ),
];

/// The English for one code, with its arguments filled in.
fn say(code: &str, args: &[String]) -> String {
    let template = SAYINGS
        .iter()
        .find(|(name, _)| *name == code)
        .map(|(_, text)| *text)
        // Not reachable through a constructor, and not worth a panic in a
        // person's face: the code alone still says more than nothing.
        .unwrap_or(code);
    fill(template, args)
}

/// `{0}`, `{1}`, … replaced by the arguments, in order.
pub fn fill(template: &str, args: &[String]) -> String {
    let mut out = template.to_string();
    for (n, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{n}}}"), arg);
    }
    out
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&say(self.code, &self.args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_says_itself_with_its_arguments_in_it() {
        assert_eq!(Refusal::renamed("notes", "nodes").to_string(), "'notes' is now 'nodes'");
        assert_eq!(
            Refusal::would_spend(15, 208).to_string(),
            "`ask 15` would send 208 lines to a model. A question that spends money is not \
             run by opening it — ask for it deliberately."
        );
        // The same argument twice, which `size:` needs.
        assert!(Refusal::size_needs_a_comparison("4")
            .to_string()
            .contains("size:>=4 or size:<=4"));
    }

    #[test]
    fn no_code_is_written_down_twice() {
        let mut seen: Vec<&str> = SAYINGS.iter().map(|(code, _)| *code).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "a code appears twice in SAYINGS");
    }

    /// **The gate.** A translation table nothing checks is a translation table
    /// that rots: a refusal added in Rust and forgotten in the locales would
    /// reach the screen in English, which is the whole thing this module
    /// exists to stop.
    #[test]
    fn every_refusal_is_translated() {
        let en: serde_json::Value =
            serde_json::from_str(include_str!("../../src/i18n/locales/en.json")).expect("en.json");
        let vi: serde_json::Value =
            serde_json::from_str(include_str!("../../src/i18n/locales/vi.json")).expect("vi.json");
        let text = |locale: &serde_json::Value, code: &str| -> Option<String> {
            locale
                .get("query")?
                .get("refused")?
                .get(code)?
                .as_str()
                .map(str::to_string)
        };

        let mut missing: Vec<String> = Vec::new();
        for (code, english) in SAYINGS {
            match text(&en, code) {
                None => missing.push(format!("en.json has no query.refused.{code}")),
                // And the English in the locale is the English here. Two copies
                // that may differ are worse than one, so they may not differ.
                Some(there) if there != *english => missing.push(format!(
                    "en.json's {code} has drifted:\n  rust: {english}\n  json: {there}"
                )),
                Some(_) => {}
            }
            if text(&vi, code).is_none() {
                missing.push(format!("vi.json has no query.refused.{code}"));
            }
        }
        assert!(missing.is_empty(), "\n{}", missing.join("\n"));
    }
}
