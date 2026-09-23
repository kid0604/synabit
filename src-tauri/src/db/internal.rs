//! The kinds a person did not put there.
//!
//! # What this is for
//!
//! A vault holds what somebody wrote and, beside it, what the app needs to
//! work: where each RSS feed was last read to, a whiteboard's geometry, the
//! schema of a type, a PDF's highlights. All of it lives in the vault on
//! purpose — it syncs, it is inspectable, it is theirs — and **none of it is
//! something to search for**.
//!
//! Measured on a real vault before this existed: 449 of 972 nodes were type
//! `json`, and 306 of those were one RSS state file and 303 sync conflict
//! copies of it. Forty-six per cent of everything search ranked over, and
//! every one of them could come back for an ordinary word. One did, which is
//! how this was found: a search for *nhà bà nội* answered with
//! `…false, "t": "2026-09-10T10:54:41.042975+00:00"…`.
//!
//! # The rule already existed
//!
//! [`crate::syn::tools::is_internal_type`] has been the list for a long time —
//! the assistant has never been offered these kinds. What was missing is that
//! **the person looking at their own vault got no such courtesy**, and half
//! the rule was written out again as `node_type NOT LIKE 'finance_%'` inside
//! the query builder. One list, in one place, applied to both.
//!
//! # Asked for by name, it is answered
//!
//! Hiding is a default, not a refusal. `type:json` means somebody wants those,
//! and quietly returning nothing to a question that named a type is exactly
//! the silent wrong answer this codebase keeps digging out. So the filter
//! lifts the moment a question says which kind it is about — and that also
//! fixes `type:finance_month`, which used to contradict the old hard-coded
//! exclusion and come back empty.

use crate::query::{Expr, Field, Query, Term, Value};

/// SQL that leaves out the kinds nobody asked for, or nothing at all.
///
/// Appended to a `WHERE` that already has a condition in it, so it begins with
/// `AND`.
pub fn unless_asked_for(query: &Query) -> String {
    if names_a_kind(&query.filter) {
        return String::new();
    }
    let list = KINDS
        .iter()
        .map(|kind| format!("'{kind}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" AND node_type NOT IN ({list}) AND node_type NOT LIKE 'finance_%'")
}

/// The same, for the FTS path, whose column is called `item_type` and which
/// reads the flat view rather than the tree.
pub fn unless_asked_for_type(type_filter: Option<&str>) -> String {
    if type_filter.is_some() {
        return String::new();
    }
    let list = KINDS
        .iter()
        .map(|kind| format!("'{kind}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" AND item_type NOT IN ({list}) AND item_type NOT LIKE 'finance_%'")
}

/// The kinds, as `syn::tools::is_internal_type` lists them.
///
/// Written out rather than called per row: this goes into SQL, and a list of
/// nine strings in a `NOT IN` is something the query planner can use.
/// [`tests::the_two_lists_are_one_list`] fails if they ever differ.
const KINDS: &[&str] = &[
    "json",
    "canvas",
    "pdf_highlight",
    "pdf_drawing",
    "interaction",
    "schema",
    "view",
    "syn_memory",
    "syn_skill",
    // Not storage, but not a node anybody wrote either: what was read *out
    // of* the nodes. Asked for by the timeline's own source, `moments`, and by
    // name. See `timeline::moments`.
    "moment",
];

/// Whether the question says which kind it is about, anywhere in it.
fn names_a_kind(expr: &Expr) -> bool {
    match expr {
        Expr::Term(Term { field: Field::Kind, value: Value::Text(_) }) => true,
        Expr::Term(_) => false,
        Expr::Not(inner) => names_a_kind(inner),
        Expr::And(branches) | Expr::Or(branches) => branches.iter().any(names_a_kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parse;

    /// One list. The assistant's and the person's must not drift, or the
    /// vault will look like two different vaults depending who is asking.
    #[test]
    fn the_two_lists_are_one_list() {
        for kind in KINDS {
            assert!(
                crate::syn::tools::is_internal_type(kind),
                "'{kind}' is hidden from a person and offered to the assistant"
            );
        }
        // And the other direction, for the ones that are not a prefix rule.
        for kind in ["note", "task", "person", "file", "whiteboard", "lens"] {
            assert!(!crate::syn::tools::is_internal_type(kind), "{kind}");
            assert!(!KINDS.contains(&kind), "{kind}");
        }
    }

    #[test]
    fn an_ordinary_question_does_not_reach_them() {
        let sql = unless_asked_for(&parse("nhà bà nội"));
        assert!(sql.contains("NOT IN"), "{sql}");
        assert!(sql.contains("'json'"), "{sql}");
        assert!(sql.contains("finance_"), "{sql}");
    }

    /// Hiding is a default, not a refusal. Quietly answering nothing to a
    /// question that named a kind is the thing this codebase keeps digging
    /// out of itself.
    #[test]
    fn a_question_that_names_a_kind_gets_it() {
        for asked in ["type:json", "is:schema", "#work type:view", "(type:json OR #a)"] {
            assert_eq!(unless_asked_for(&parse(asked)), "", "'{asked}' was still filtered");
        }
        // Including the one the old hard-coded exclusion contradicted.
        assert_eq!(unless_asked_for(&parse("type:finance_month")), "");
    }
}
