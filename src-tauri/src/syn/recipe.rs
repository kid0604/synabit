//! A skill that runs the same way every time.
//!
//! # What this is, and what it deliberately is not
//!
//! A recipe is a declared list of tool calls with named parameters and one kind
//! of condition. It runs in Rust. The model fills the parameters and reads the
//! result; it does not decide the steps, because the steps were decided when
//! somebody wrote them down.
//!
//! That is the whole value. A `prose` skill costs an inference round per step
//! and does something slightly different each time; a recipe costs none and
//! does the same thing, which means it can be tested, and means a person can
//! read what will happen before it happens.
//!
//! **It is not Turing-complete, and that is a requirement rather than a
//! limitation.** There are no loops, no jumps, no arithmetic, no boolean
//! algebra. A condition can skip one step and nothing else. The roadmap puts it
//! plainly: if this format starts growing loops and branches, that is the
//! signal it wanted to be the `code` tier, and the answer is to stop rather
//! than to keep adding.
//!
//! The line is worth being able to state, because it will be pushed. A recipe
//! is a fixed-length sequence of calls whose length is known before it runs.
//! Anything that makes the length depend on the data is out.
//!
//! # Where it lives
//!
//! In the skill's own body, in a fenced ` ```recipe ` block of YAML. Not in
//! frontmatter: the body is the part a person reads, and a procedure they
//! cannot read beside its own explanation is a procedure they will not check.
//!
//! ```text
//! ```recipe
//! params:
//!   - name: since
//!     description: The date to summarise from, as YYYY-MM-DD.
//!     required: true
//! steps:
//!   - tool: query_nodes
//!     as: done
//!     args:
//!       type: task
//!       status: done
//!       updated_after: "{{since}}"
//!   - tool: create_node
//!     when: done is not empty
//!     args:
//!       node_type: note
//!       title: "Tổng kết từ {{since}}"
//!       content: "{{done}}"
//! ```
//! ```

use serde::{Deserialize, Serialize};

/// The fence that marks a recipe inside a skill body.
pub const FENCE: &str = "```recipe";

/// The tool that runs a recipe, named once so validation can refuse it.
pub const RUN_TOOL: &str = "run_recipe";

/// Tools a recipe may not call.
///
/// These reshape the vault itself — renaming a field across every node of a
/// kind, deleting a kind — and they are not steps in a job. They are decisions
/// about how somebody's data is organised, and a canned procedure fired with
/// parameters is the wrong shape for one.
///
/// Refusing them is also what lets `run_recipe` declare a single honest
/// capability. A recipe's real reach is the union of its steps', and a runner
/// that could reach anything would have to declare the most reaching thing it
/// could possibly do. With these out, that is `VaultWrite`.
pub const NOT_IN_A_RECIPE: [&str; 4] =
    ["rename_field", "delete_field", "rename_kind", "delete_kind"];

/// One value the caller supplies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Param {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// The only kind of condition a recipe has.
///
/// One shape, and no way to combine two of them. A condition may skip a step;
/// it may not choose between steps, jump, or repeat one. Every extension anyone
/// will want here — `and`, `or`, a comparison, a nested test — is the format
/// asking to become a programming language, and the answer is the `code` tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Presence {
    Empty,
    NotEmpty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// The step result or parameter being tested.
    pub subject: String,
    pub expect: Presence,
}

impl Condition {
    /// Parse `x is empty` or `x is not empty`, and nothing else.
    ///
    /// Strictly. A `when:` the runner does not understand is a validation error
    /// and not a step quietly always running — the failure mode of a silently
    /// ignored condition is a recipe that writes a note it was told not to, and
    /// nobody finds out until they read the vault.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let lowered = text.to_lowercase();
        for (suffix, expect) in [
            (" is not empty", Presence::NotEmpty),
            (" is empty", Presence::Empty),
        ] {
            if let Some(subject) = lowered.strip_suffix(suffix) {
                let subject = text[..subject.len()].trim().to_string();
                if subject.is_empty() {
                    return Err(format!("`{raw}` does not say what is empty"));
                }
                // One name, not a clause. Without this, `a is not empty and b
                // is empty` ends in ` is empty` and parses — subject `a is not
                // empty and b` — so a conjunction nobody supports would be
                // accepted and quietly tested against a binding that does not
                // exist, which reads as empty, which skips the step. The test
                // that named this case is the reason it is here.
                if subject.split_whitespace().count() != 1 {
                    return Err(format!(
                        "`{raw}` tests more than one thing. A recipe knows two \
                         conditions and no way to combine them: `<name> is empty` \
                         and `<name> is not empty`."
                    ));
                }
                return Ok(Condition { subject, expect });
            }
        }
        Err(format!(
            "`{raw}` is not a condition this understands. A recipe knows two: \
             `<name> is empty` and `<name> is not empty`."
        ))
    }

    /// Does this hold, given what has been bound so far?
    ///
    /// An unbound subject is empty. Validation has already refused a recipe
    /// naming something that cannot exist, so at this point an absent binding
    /// means a step was skipped and its result never happened.
    pub fn holds(&self, bindings: &std::collections::HashMap<String, serde_json::Value>) -> bool {
        let empty = bindings.get(&self.subject).is_none_or(is_empty);
        match self.expect {
            Presence::Empty => empty,
            Presence::NotEmpty => !empty,
        }
    }
}

/// Whether a bound value counts as nothing.
///
/// Tools answer in JSON, and "found nothing" arrives in several shapes — an
/// empty array, an object with an empty `results`, a zero count. A recipe
/// author should not have to know which shape a given tool prefers.
fn is_empty(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::String(s) => s.trim().is_empty(),
        serde_json::Value::Array(items) => items.is_empty(),
        serde_json::Value::Bool(b) => !b,
        serde_json::Value::Number(n) => n.as_f64() == Some(0.0),
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                return true;
            }
            // The shapes this app's own tools actually return.
            for key in ["results", "nodes", "memories", "items", "trash"] {
                if let Some(list) = map.get(key).and_then(|v| v.as_array()) {
                    return list.is_empty();
                }
            }
            if let Some(count) = map.get("total").or_else(|| map.get("count")) {
                return count.as_u64() == Some(0);
            }
            map.contains_key("error")
        }
    }
}

/// One call, in order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Step {
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
    /// A name for this step's result, so a later step can use it.
    #[serde(default, rename = "as")]
    pub bind: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
}

/// A procedure that runs the same way every time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Recipe {
    #[serde(default)]
    pub params: Vec<Param>,
    pub steps: Vec<Step>,
}

/// Pull the recipe out of a skill body, if it has one.
///
/// `Ok(None)` for a body with no recipe block, which is every `prose` skill and
/// is not an error. `Err` only when there is a block and it does not parse —
/// silence there would leave somebody with a `tier: recipe` skill that quietly
/// does nothing.
pub fn parse(body: &str) -> Result<Option<Recipe>, String> {
    let Some(start) = body.find(FENCE) else {
        return Ok(None);
    };
    let after = &body[start + FENCE.len()..];
    let Some(end) = after.find("```") else {
        return Err("the ```recipe block is never closed".to_string());
    };

    let yaml = after[..end].trim();
    if yaml.is_empty() {
        return Err("the ```recipe block is empty".to_string());
    }
    serde_yaml::from_str::<Recipe>(yaml)
        .map(Some)
        .map_err(|e| format!("the recipe is not readable YAML: {e}"))
}

/// Everything wrong with a recipe, in the order somebody would fix it.
///
/// All of it at once rather than the first problem. A person editing a skill in
/// a text editor and running it to see what breaks is being made to do the
/// checking a validator can do in a millisecond.
pub fn problems(recipe: &Recipe, known_tools: &[String]) -> Vec<String> {
    let mut problems = Vec::new();

    if recipe.steps.is_empty() {
        problems.push("a recipe with no steps does nothing".to_string());
    }

    let mut bound: Vec<String> = recipe.params.iter().map(|p| p.name.trim().to_string()).collect();
    for name in &bound {
        if name.is_empty() {
            problems.push("a parameter has no name".to_string());
        }
    }

    let mut seen: Vec<String> = Vec::new();
    for (index, step) in recipe.steps.iter().enumerate() {
        let at = index + 1;

        // A recipe may not run a recipe. One calling another is a call graph,
        // and a call graph is one edge away from recursion — which is the loop
        // this format does not have and must not acquire by accident. The
        // length of a recipe is knowable before it runs, and that stays true.
        if NOT_IN_A_RECIPE.contains(&step.tool.as_str()) {
            problems.push(format!(
                "step {at} calls `{}`, which reshapes the vault. That is a \
                 decision about how data is organised, not a step in a job.",
                step.tool
            ));
        }

        if step.tool == RUN_TOOL {
            problems.push(format!(
                "step {at} runs another recipe. A recipe is a fixed list of \
                 calls, and one that can call another is no longer fixed."
            ));
        }

        if !known_tools.iter().any(|t| t == &step.tool) {
            problems.push(format!(
                "step {at} calls `{}`, which is not a tool this app has",
                step.tool
            ));
        }

        if let Some(when) = &step.when {
            match Condition::parse(when) {
                Ok(condition) => {
                    if !bound.contains(&condition.subject) {
                        problems.push(format!(
                            "step {at} asks about `{}`, which nothing before it produced",
                            condition.subject
                        ));
                    }
                }
                Err(e) => problems.push(format!("step {at}: {e}")),
            }
        }

        for reference in references(&step.args) {
            if !bound.contains(&reference) {
                problems.push(format!(
                    "step {at} uses `{{{{{reference}}}}}`, which nothing before it produced"
                ));
            }
        }

        if let Some(name) = step.bind.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            if seen.iter().any(|s| s == name) || recipe.params.iter().any(|p| p.name.trim() == name) {
                problems.push(format!("step {at} reuses the name `{name}`"));
            }
            seen.push(name.to_string());
            bound.push(name.to_string());
        }
    }

    problems
}

/// Every `{{name}}` inside a value, in the order they appear.
fn references(value: &serde_json::Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(value, &mut |text| {
        let mut rest = text;
        while let Some(open) = rest.find("{{") {
            let Some(close) = rest[open..].find("}}") else { break };
            let name = rest[open + 2..open + close].trim().to_string();
            if !name.is_empty() {
                found.push(name);
            }
            rest = &rest[open + close + 2..];
        }
    });
    found
}

fn walk(value: &serde_json::Value, on_text: &mut impl FnMut(&str)) {
    match value {
        serde_json::Value::String(s) => on_text(s),
        serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, on_text)),
        serde_json::Value::Object(map) => map.values().for_each(|v| walk(v, on_text)),
        _ => {}
    }
}

/// Replace every `{{name}}` with what is bound to it.
///
/// A string that is *exactly* one reference takes the bound value whole, so a
/// step can hand an array to the next one without it becoming the text of an
/// array. Anything else is a substitution into text, and a non-string value
/// arrives as its JSON — visible, and the author can see what they got.
pub fn resolve(
    args: &serde_json::Value,
    bindings: &std::collections::HashMap<String, serde_json::Value>,
) -> serde_json::Value {
    match args {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if let Some(name) = trimmed
                .strip_prefix("{{")
                .and_then(|rest| rest.strip_suffix("}}"))
                .map(str::trim)
            {
                if let Some(value) = bindings.get(name) {
                    return value.clone();
                }
            }
            let mut out = String::new();
            let mut rest = text.as_str();
            while let Some(open) = rest.find("{{") {
                let Some(close) = rest[open..].find("}}") else { break };
                out.push_str(&rest[..open]);
                let name = rest[open + 2..open + close].trim();
                match bindings.get(name) {
                    Some(serde_json::Value::String(s)) => out.push_str(s),
                    Some(other) => out.push_str(&other.to_string()),
                    // Left as written. A hole in the output that reads
                    // `{{since}}` says which value was missing; a hole that
                    // reads nothing says only that something went wrong.
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &rest[open + close + 2..];
            }
            out.push_str(rest);
            serde_json::Value::String(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(|v| resolve(v, bindings)).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter().map(|(k, v)| (k.clone(), resolve(v, bindings))).collect(),
        ),
        other => other.clone(),
    }
}

/// What one step did.
#[derive(Debug, Clone, Serialize)]
pub struct Ran {
    pub tool: String,
    /// True when a `when:` said not to. Reported rather than omitted: a recipe
    /// that quietly did four of its five steps is a recipe nobody can debug.
    pub skipped: bool,
    pub ok: bool,
}

/// What a whole recipe did.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub steps: Vec<Ran>,
    /// Why it stopped early, when it did.
    pub stopped: Option<String>,
    /// What each named step produced, for the caller to read.
    pub bindings: std::collections::HashMap<String, serde_json::Value>,
}

/// Run a recipe, calling `call` for each step.
///
/// The caller supplies the dispatch. That keeps this module free of tool
/// plumbing and, more usefully, makes the order, the skipping and the binding
/// testable without a database — which is most of what can go wrong here.
///
/// It stops at the first failed step. A procedure is a sequence somebody wrote
/// because the steps depend on each other; carrying on past a broken one does
/// half of something, and half of a thing that touches a vault is worse than
/// none of it.
pub fn run(
    recipe: &Recipe,
    params: &serde_json::Map<String, serde_json::Value>,
    mut call: impl FnMut(&str, &serde_json::Value) -> Result<serde_json::Value, String>,
) -> Outcome {
    let mut bindings: std::collections::HashMap<String, serde_json::Value> = params
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut ran = Vec::new();

    // Missing required parameters stop it before anything happens, rather than
    // at whichever step first needed one.
    let missing: Vec<&str> = recipe
        .params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.trim())
        .filter(|name| !bindings.contains_key(*name) || bindings.get(*name).is_some_and(is_empty))
        .collect();
    if !missing.is_empty() {
        return Outcome {
            steps: Vec::new(),
            stopped: Some(format!("missing required parameter(s): {}", missing.join(", "))),
            bindings,
        };
    }

    for step in &recipe.steps {
        if let Some(when) = &step.when {
            match Condition::parse(when) {
                Ok(condition) if !condition.holds(&bindings) => {
                    ran.push(Ran { tool: step.tool.clone(), skipped: true, ok: true });
                    continue;
                }
                Ok(_) => {}
                // Validation refuses these before a recipe is runnable, so this
                // is a file edited by hand since. Stopping is right: a condition
                // nobody can read is not one to guess at.
                Err(e) => {
                    return Outcome {
                        steps: ran,
                        stopped: Some(e),
                        bindings,
                    };
                }
            }
        }

        let args = resolve(&step.args, &bindings);
        match call(&step.tool, &args) {
            Ok(value) => {
                ran.push(Ran { tool: step.tool.clone(), skipped: false, ok: true });
                if let Some(name) = step.bind.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                    bindings.insert(name.to_string(), value);
                }
            }
            Err(e) => {
                ran.push(Ran { tool: step.tool.clone(), skipped: false, ok: false });
                return Outcome {
                    steps: ran,
                    stopped: Some(format!("`{}` failed: {e}", step.tool)),
                    bindings,
                };
            }
        }
    }

    Outcome { steps: ran, stopped: None, bindings }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<String> {
        ["query_nodes", "create_node", "get_node"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    const SAMPLE: &str = "## Steps\n\n\
        ```recipe\n\
        params:\n\
        \x20 - name: since\n\
        \x20   required: true\n\
        steps:\n\
        \x20 - tool: query_nodes\n\
        \x20   as: done\n\
        \x20   args:\n\
        \x20     type: task\n\
        \x20     updated_after: \"{{since}}\"\n\
        \x20 - tool: create_node\n\
        \x20   when: done is not empty\n\
        \x20   args:\n\
        \x20     title: \"Tổng kết từ {{since}}\"\n\
        ```\n\n\
        Some prose after.";

    #[test]
    fn a_recipe_is_read_out_of_the_body_it_is_written_in() {
        let recipe = parse(SAMPLE).expect("it parses").expect("there is one");
        assert_eq!(recipe.params.len(), 1);
        assert_eq!(recipe.steps.len(), 2);
        assert_eq!(recipe.steps[0].bind.as_deref(), Some("done"));
        assert_eq!(recipe.steps[1].when.as_deref(), Some("done is not empty"));
        assert!(problems(&recipe, &tools()).is_empty(), "and it is sound");
    }

    /// A prose skill has no recipe, and that is not a failure.
    #[test]
    fn a_body_without_a_recipe_is_not_an_error() {
        assert_eq!(parse("## Steps\n1. Just do it").expect("no error"), None);
    }

    /// A block that does not parse says so rather than doing nothing.
    ///
    /// The silent version leaves somebody with a `tier: recipe` skill that
    /// looks enabled and never runs, and nothing to read about why.
    #[test]
    fn a_broken_block_is_an_error_and_not_a_shrug() {
        assert!(parse("```recipe\nsteps: [oh dear\n```").is_err());
        assert!(parse("```recipe\n\n```").is_err(), "an empty one too");
        assert!(parse("```recipe\nsteps: []").is_err(), "and an unclosed one");
    }

    /// Conditions are two shapes, and anything else is refused loudly.
    ///
    /// A `when:` quietly ignored means a step that runs when it was told not
    /// to — a recipe writing a note somebody excluded on purpose, found weeks
    /// later by reading the vault.
    #[test]
    fn a_condition_it_cannot_read_is_a_problem_not_a_shrug() {
        assert_eq!(
            Condition::parse("done is not empty").expect("reads"),
            Condition { subject: "done".into(), expect: Presence::NotEmpty }
        );
        assert_eq!(
            Condition::parse("  done   is   empty ").expect("reads"),
            Condition { subject: "done".into(), expect: Presence::Empty }
        );

        for nonsense in [
            "done is not empty and tasks is empty",
            "count > 3",
            "done",
            "is empty",
            "done was empty",
        ] {
            assert!(
                Condition::parse(nonsense).is_err(),
                "`{nonsense}` should be refused rather than guessed at"
            );
        }
    }

    /// Every problem at once, not the first one.
    #[test]
    fn a_recipe_is_told_everything_that_is_wrong_with_it() {
        let recipe = Recipe {
            params: vec![],
            steps: vec![
                Step {
                    tool: "invent_node".into(),
                    args: serde_json::json!({ "title": "{{nowhere}}" }),
                    bind: Some("a".into()),
                    when: Some("count > 3".into()),
                },
                Step {
                    tool: "create_node".into(),
                    args: serde_json::json!({}),
                    bind: Some("a".into()),
                    when: None,
                },
            ],
        };
        let found = problems(&recipe, &tools());

        assert!(found.iter().any(|p| p.contains("invent_node")), "{found:?}");
        assert!(found.iter().any(|p| p.contains("nowhere")), "{found:?}");
        assert!(found.iter().any(|p| p.contains("count > 3")), "{found:?}");
        assert!(found.iter().any(|p| p.contains("reuses the name")), "{found:?}");
    }

    /// A recipe may not reshape the vault.
    ///
    /// Renaming a field across every node of a kind is a decision about how
    /// somebody's data is organised. A canned procedure fired with parameters
    /// is the wrong shape for one — and keeping these out is what lets the
    /// runner declare one honest capability instead of the most reaching thing
    /// any recipe could possibly do.
    #[test]
    fn a_recipe_cannot_rename_or_delete_a_kind() {
        for tool in NOT_IN_A_RECIPE {
            let recipe = Recipe {
                params: vec![],
                steps: vec![Step {
                    tool: tool.into(),
                    args: serde_json::json!({}),
                    bind: None,
                    when: None,
                }],
            };
            let mut known = tools();
            known.push(tool.to_string());
            assert!(
                problems(&recipe, &known).iter().any(|p| p.contains("reshapes the vault")),
                "`{tool}` should be refused inside a recipe"
            );
        }
    }

    /// A recipe may not run a recipe.
    ///
    /// One calling another is a call graph, and a call graph is one edge from
    /// recursion — the loop this format does not have. The length of a recipe
    /// is knowable before it runs, and that has to stay true.
    #[test]
    fn a_recipe_cannot_call_a_recipe() {
        let recipe = Recipe {
            params: vec![],
            steps: vec![Step {
                tool: RUN_TOOL.into(),
                args: serde_json::json!({ "name": "itself" }),
                bind: None,
                when: None,
            }],
        };
        let mut tools = tools();
        tools.push(RUN_TOOL.to_string());
        assert!(
            problems(&recipe, &tools).iter().any(|p| p.contains("no longer fixed")),
            "refused even when the tool exists"
        );
    }

    /// A step may only use what came before it.
    #[test]
    fn a_step_cannot_use_a_result_that_has_not_happened_yet() {
        let recipe = Recipe {
            params: vec![],
            steps: vec![
                Step {
                    tool: "create_node".into(),
                    args: serde_json::json!({ "content": "{{later}}" }),
                    bind: None,
                    when: None,
                },
                Step {
                    tool: "query_nodes".into(),
                    args: serde_json::json!({}),
                    bind: Some("later".into()),
                    when: None,
                },
            ],
        };
        assert!(
            problems(&recipe, &tools()).iter().any(|p| p.contains("later")),
            "order is the only control flow there is, so it has to be real"
        );
    }

    #[test]
    fn a_whole_reference_keeps_the_shape_of_what_it_names() {
        let mut bindings = std::collections::HashMap::new();
        bindings.insert("done".to_string(), serde_json::json!(["a", "b"]));
        bindings.insert("since".to_string(), serde_json::json!("2026-09-01"));

        let out = resolve(
            &serde_json::json!({ "list": "{{done}}", "title": "Từ {{since}}" }),
            &bindings,
        );
        assert_eq!(out["list"], serde_json::json!(["a", "b"]), "an array stays an array");
        assert_eq!(out["title"], "Từ 2026-09-01");
    }

    /// A missing value leaves its own name behind.
    #[test]
    fn an_unfilled_hole_says_which_one_it_was() {
        let out = resolve(
            &serde_json::json!({ "title": "Từ {{since}}" }),
            &std::collections::HashMap::new(),
        );
        assert_eq!(
            out["title"], "Từ {{since}}",
            "a blank says only that something went wrong; a name says which"
        );
    }

    /// Emptiness is recognised in the shapes this app's tools actually return.
    #[test]
    fn found_nothing_is_recognised_however_a_tool_spells_it() {
        for nothing in [
            serde_json::json!([]),
            serde_json::json!({}),
            serde_json::json!({ "results": [] }),
            serde_json::json!({ "nodes": [] }),
            serde_json::json!({ "total": 0 }),
            serde_json::json!({ "error": "Node not found" }),
            serde_json::json!(""),
            serde_json::json!(null),
        ] {
            assert!(is_empty(&nothing), "{nothing} is nothing");
        }
        for something in [
            serde_json::json!({ "results": [1] }),
            serde_json::json!({ "total": 2 }),
            serde_json::json!(["a"]),
            serde_json::json!("text"),
        ] {
            assert!(!is_empty(&something), "{something} is something");
        }
    }

    fn recipe_of(steps: Vec<Step>, params: Vec<Param>) -> Recipe {
        Recipe { params, steps }
    }

    fn step(tool: &str, bind: Option<&str>, when: Option<&str>, args: serde_json::Value) -> Step {
        Step {
            tool: tool.into(),
            args,
            bind: bind.map(str::to_string),
            when: when.map(str::to_string),
        }
    }

    /// Steps run in order, and each may use what the ones before produced.
    #[test]
    fn a_recipe_runs_its_steps_in_order_and_passes_results_along() {
        let recipe = recipe_of(
            vec![
                step("query_nodes", Some("done"), None, serde_json::json!({ "type": "task" })),
                step("create_node", None, None, serde_json::json!({ "content": "{{done}}" })),
            ],
            vec![],
        );

        let mut seen: Vec<(String, serde_json::Value)> = Vec::new();
        let outcome = run(&recipe, &serde_json::Map::new(), |tool, args| {
            seen.push((tool.to_string(), args.clone()));
            Ok(serde_json::json!({ "results": ["a", "b"] }))
        });

        assert!(outcome.stopped.is_none());
        assert_eq!(seen[0].0, "query_nodes");
        assert_eq!(seen[1].0, "create_node");
        assert_eq!(
            seen[1].1["content"],
            serde_json::json!({ "results": ["a", "b"] }),
            "the second step was handed what the first produced, whole"
        );
    }

    /// A `when:` that does not hold skips its step, and says it did.
    ///
    /// Reported rather than omitted. A recipe that quietly did four of its five
    /// steps is one nobody can debug from its own output.
    #[test]
    fn a_step_whose_condition_fails_is_skipped_out_loud() {
        let recipe = recipe_of(
            vec![
                step("query_nodes", Some("done"), None, serde_json::json!({})),
                step("create_node", None, Some("done is not empty"), serde_json::json!({})),
            ],
            vec![],
        );

        let mut called = Vec::new();
        let outcome = run(&recipe, &serde_json::Map::new(), |tool, _| {
            called.push(tool.to_string());
            Ok(serde_json::json!({ "results": [] }))
        });

        assert_eq!(called, vec!["query_nodes"], "the second never ran");
        assert!(outcome.steps[1].skipped, "and the report says so");
        assert!(outcome.stopped.is_none(), "skipping is not stopping");
    }

    /// The first failure stops it.
    ///
    /// A procedure is a sequence whose steps depend on each other. Carrying on
    /// past a broken one does half of something, and half a thing that touches
    /// a vault is worse than none of it.
    #[test]
    fn a_failed_step_stops_the_rest() {
        let recipe = recipe_of(
            vec![
                step("query_nodes", None, None, serde_json::json!({})),
                step("create_node", None, None, serde_json::json!({})),
            ],
            vec![],
        );

        let mut called = Vec::new();
        let outcome = run(&recipe, &serde_json::Map::new(), |tool, _| {
            called.push(tool.to_string());
            Err("no such field".to_string())
        });

        assert_eq!(called, vec!["query_nodes"], "it did not go on");
        assert!(outcome.stopped.expect("a reason").contains("query_nodes"));
    }

    /// A missing parameter stops it before anything has happened.
    #[test]
    fn a_missing_required_parameter_stops_it_before_it_touches_anything() {
        let recipe = recipe_of(
            vec![step("create_node", None, None, serde_json::json!({ "t": "{{since}}" }))],
            vec![Param { name: "since".into(), description: String::new(), required: true }],
        );

        let mut called = 0;
        let outcome = run(&recipe, &serde_json::Map::new(), |_, _| {
            called += 1;
            Ok(serde_json::json!({}))
        });

        assert_eq!(called, 0, "nothing ran");
        assert!(outcome.stopped.expect("a reason").contains("since"));
    }

    /// The condition reads an unbound name as empty.
    #[test]
    fn a_step_that_never_ran_leaves_nothing_behind() {
        let condition = Condition::parse("done is empty").expect("reads");
        assert!(condition.holds(&std::collections::HashMap::new()));
    }
}

/// Check a recipe written in a file, with the real parser and the real rules.
///
/// A dev tool, and the reason it exists is the discipline this codebase keeps
/// arriving at: a sample handed over without being run through the checker is a
/// sample whose arguments are whatever somebody remembered the tools took.
///
/// ```bash
/// SYN_RECIPE_FILE=path/to/skill.md \
///   cargo test --lib check_a_recipe_file -- --ignored --nocapture
/// ```
#[cfg(test)]
mod check_a_recipe_file {
    use super::*;

    #[test]
    #[ignore = "reads a file named by the environment; run by hand"]
    fn against_the_real_rules() {
        let path = std::env::var("SYN_RECIPE_FILE").expect("SYN_RECIPE_FILE");
        let body = std::fs::read_to_string(&path).expect("the file");

        let known: Vec<String> = crate::syn::tools::get_tool_definitions()
            .into_iter()
            .map(|t| t.function.name)
            .collect();

        eprintln!("\n═══ {path}");
        match parse(&body) {
            Ok(None) => eprintln!("  no ```recipe block — this is a prose skill\n"),
            Err(e) => panic!("  it does not parse: {e}"),
            Ok(Some(recipe)) => {
                eprintln!("  params: {}", recipe.params.len());
                for (i, step) in recipe.steps.iter().enumerate() {
                    eprintln!(
                        "  step {}: {}{}{}",
                        i + 1,
                        step.tool,
                        step.bind.as_deref().map(|b| format!(" → {b}")).unwrap_or_default(),
                        step.when.as_deref().map(|w| format!("  [when {w}]")).unwrap_or_default(),
                    );
                }
                let problems = problems(&recipe, &known);
                if problems.is_empty() {
                    eprintln!("\n  sound.\n");
                } else {
                    for problem in &problems {
                        eprintln!("  ✗ {problem}");
                    }
                    panic!("{} problem(s)", problems.len());
                }
            }
        }
    }
}
