//! Stopping to ask *which one*, when the model is about to pick for you.
//!
//! # The moment this exists for
//!
//! You say *"xoá cái note về hợp đồng"*. Syn searches, finds three, picks one,
//! and removes it. Sometimes it picks right. When it does not, the first you
//! know is that something you wanted is gone.
//!
//! A colleague does not do that. They say *"there are three — which?"*
//!
//! # Why this is not a tool
//!
//! The obvious shape is an `ask_user` tool the model reaches for when unsure.
//! That shape is measured and it fails: `recall` went uncalled across fifteen
//! real runs, and the lesson written down from it is that **a tool the model
//! has to think of calling is a tool that does not get called**. Worse here
//! than anywhere — a model confident enough to pick one of three is exactly a
//! model that does not feel it needs to ask.
//!
//! So the engine decides, from what it has already watched happen: a query
//! returned several, and the next destructive call names one of them. That is
//! arithmetic on the run, the fifth time that has beaten asking the model.
//!
//! # Why this does not contradict "vault writes never ask"
//!
//! `Capability::VaultWrite` says, deliberately: *never asks, because trash and
//! version history put every one of these back.* That answers **"may I?"**.
//!
//! This asks **"which?"** — a different question, and reversibility is no
//! answer to it. Restoring from the trash only helps somebody who *notices*
//! the wrong note went, and the whole trouble with a wrong pick is that it
//! looks exactly like a right one.
//!
//! # What it deliberately does not do
//!
//! It does not ask when the run has seen one candidate, when the target was
//! never among the candidates (the model got the id from somewhere else, which
//! is a different problem), or when the tool only reads. Asking about
//! everything is how a person learns to answer without looking.

use serde::{Deserialize, Serialize};

/// The tools worth stopping in front of.
///
/// Named rather than derived from `Capability`, because capability answers a
/// different question. `create_node` and `remember` are `VaultWrite` too and
/// neither can be ambiguous: they have no target, they make one.
///
/// `update_node` is here with `trash_node` because editing the wrong note is
/// quieter than deleting it — nothing disappears, so nothing prompts anybody to
/// go looking.
pub const ASK_BEFORE: &[&str] = &["trash_node", "update_node"];

/// One thing the run found and could have meant.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The node's path, which is what the tool call would name.
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
}

/// The question the run stopped on.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// What was about to happen, so the card can say *delete* or *change*.
    pub tool: String,
    /// Everything the last query turned up, in the order it turned them up.
    pub candidates: Vec<Candidate>,
    /// Which one the model was about to act on.
    ///
    /// Shown as the pre-selected answer rather than hidden: it is usually right,
    /// and a question that makes somebody redo the model's work from scratch is
    /// a question they will stop answering.
    pub chose: String,
    pub asked_at: String,
}

/// The nodes a `query_nodes` result named.
///
/// Reads the untruncated result, in the engine, at the moment it comes back —
/// not the transcript's `preview`, which is cut at `run::MAX_STEP_PREVIEW` and
/// would leave this parsing half a JSON document and guessing about the rest.
pub fn candidates_from(result: &str) -> Vec<Candidate> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(result) else {
        return Vec::new();
    };
    let Some(rows) = value.get("results").and_then(|r| r.as_array()) else {
        return Vec::new();
    };

    rows.iter()
        .filter_map(|row| {
            let id = row.get("id")?.as_str()?.to_string();
            Some(Candidate {
                title: row
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default()
                    .to_string(),
                node_type: row
                    .get("type")
                    .and_then(|t| t.as_str())
                    .map(str::to_string),
                id,
            })
        })
        .collect()
}

/// Whether this call is a pick among several, and what the choices were.
///
/// `seen` is what the run's most recent multi-row query returned. `None` — no
/// question — whenever any of the reasons above applies.
pub fn should_ask(
    tool: &str,
    args: &serde_json::Value,
    seen: &[Candidate],
    now: &str,
) -> Option<Choice> {
    if !ASK_BEFORE.contains(&tool) || seen.len() < 2 {
        return None;
    }

    // A batch is not a pick. Naming six ids is somebody being specific, and
    // stopping to ask *which of these six you named* would be absurd.
    if args.get("node_ids").is_some() {
        return None;
    }

    let target = args.get("node_id")?.as_str()?;

    // The target has to be one of the things the run just found. An id from
    // anywhere else — the prompt, a memory, thin air — is not a choice among
    // candidates, and dressing it up as one would put a question in front of
    // the user that misdescribes what is happening.
    if !seen.iter().any(|c| c.id == target) {
        return None;
    }

    Some(Choice {
        tool: tool.to_string(),
        candidates: seen.to_vec(),
        chose: target.to_string(),
        asked_at: now.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> &'static str {
        "2026-09-06T09:00:00Z"
    }

    fn three() -> Vec<Candidate> {
        vec![
            Candidate { id: "Notes/a.md".into(), title: "Hợp đồng FPT".into(), node_type: Some("note".into()) },
            Candidate { id: "Notes/b.md".into(), title: "Hợp đồng VPB".into(), node_type: Some("note".into()) },
            Candidate { id: "Notes/c.md".into(), title: "Hợp đồng thuê nhà".into(), node_type: Some("note".into()) },
        ]
    }

    /// The whole point: three notes, one about to go, and the user is asked.
    #[test]
    fn removing_one_of_three_is_a_question() {
        let ask = should_ask(
            "trash_node",
            &serde_json::json!({ "node_id": "Notes/b.md" }),
            &three(),
            now(),
        )
        .expect("it stops");

        assert_eq!(ask.tool, "trash_node");
        assert_eq!(ask.chose, "Notes/b.md");
        assert_eq!(ask.candidates.len(), 3);
    }

    /// Editing the wrong note is quieter than deleting it — nothing vanishes,
    /// so nothing prompts anybody to go and look.
    #[test]
    fn changing_one_of_several_is_a_question_too() {
        assert!(should_ask(
            "update_node",
            &serde_json::json!({ "node_id": "Notes/a.md", "properties": {"status": "done"} }),
            &three(),
            now(),
        )
        .is_some());
    }

    /// One result is not a choice.
    #[test]
    fn a_single_match_is_not_ambiguous() {
        let one = vec![three()[0].clone()];
        assert!(should_ask("trash_node", &serde_json::json!({ "node_id": "Notes/a.md" }), &one, now()).is_none());
        assert!(should_ask("trash_node", &serde_json::json!({ "node_id": "Notes/a.md" }), &[], now()).is_none());
    }

    /// Naming six ids is somebody being specific. Asking *which of the six you
    /// named* would be absurd.
    #[test]
    fn a_batch_is_not_a_pick() {
        assert!(should_ask(
            "trash_node",
            &serde_json::json!({ "node_ids": ["Notes/a.md", "Notes/b.md"] }),
            &three(),
            now(),
        )
        .is_none());
    }

    /// An id from outside the candidates is a different problem, and dressing
    /// it up as a choice would put a question on screen that misdescribes what
    /// is about to happen.
    #[test]
    fn an_id_the_run_never_saw_is_not_a_choice_among_these() {
        assert!(should_ask(
            "trash_node",
            &serde_json::json!({ "node_id": "Notes/somewhere-else.md" }),
            &three(),
            now(),
        )
        .is_none());
    }

    /// Reading is never a question. Asking about everything is how somebody
    /// learns to answer without looking.
    #[test]
    fn nothing_that_only_reads_ever_asks() {
        for tool in ["query_nodes", "get_node", "recall", "list_schemas", "look_back"] {
            assert!(
                should_ask(tool, &serde_json::json!({ "node_id": "Notes/a.md" }), &three(), now())
                    .is_none(),
                "{tool} stopped to ask"
            );
        }
    }

    /// And neither does making something new, which has no target to be wrong
    /// about.
    #[test]
    fn creating_something_is_never_a_choice() {
        for tool in ["create_node", "remember", "create_transaction"] {
            assert!(
                should_ask(tool, &serde_json::json!({ "node_id": "Notes/a.md" }), &three(), now())
                    .is_none(),
                "{tool} stopped to ask"
            );
        }
    }

    // ── reading the query result ──────────────────────────────────

    #[test]
    fn the_candidates_come_out_of_a_real_query_result() {
        let result = serde_json::json!({
            "columns": ["title", "updated_at"],
            "results": [
                { "id": "Notes/a.md", "type": "note", "title": "Hợp đồng FPT", "columns": [] },
                { "id": "Tasks/b.md", "type": "task", "title": "Ký hợp đồng", "columns": [] },
            ],
            "total_matches": 2,
        })
        .to_string();

        let found = candidates_from(&result);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].id, "Notes/a.md");
        assert_eq!(found[0].title, "Hợp đồng FPT");
        assert_eq!(found[1].node_type.as_deref(), Some("task"));
    }

    /// A tool that answered with something else, or with an error, contributes
    /// no candidates rather than a panic.
    #[test]
    fn anything_that_is_not_a_query_result_names_nobody() {
        assert!(candidates_from("not json at all").is_empty());
        assert!(candidates_from(r#"{"error":"Node not found"}"#).is_empty());
        assert!(candidates_from(r#"{"results":[]}"#).is_empty());
        assert!(candidates_from(r#"{"results":[{"title":"no id"}]}"#).is_empty());
    }
}
