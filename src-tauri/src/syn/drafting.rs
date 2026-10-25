//! Syn writing a skill for itself, and asking whether to keep it.
//!
//! # What this changes
//!
//! A skill — a procedure Syn follows — was the person's to write: a template
//! in a settings drawer, frontmatter to learn, a body in Markdown. The one way
//! Syn wrote one was by noticing it had repeated itself, and even then it wrote
//! prose into a drawer, and the person had to find it, try it and switch it on.
//!
//! Now Syn writes it when asked — *"từ giờ mỗi khi tao bảo tổng kết tuần thì…"*
//! — or when it notices it did the same job twice, and asks on a card under its
//! answer: keep this way of doing it? That is the whole of what the person
//! does.
//!
//! # What keeps it safe
//!
//! - **Nothing is live until the person says so.** A skill Syn writes is
//!   written switched off (`enabled: false`, `author: syn`), and only
//!   `answer` — the card's button — switches it on. The model cannot: a patch
//!   that turns on an unapproved Syn skill is refused (`tools::skill_patch_problem`).
//! - **The card is the file.** What the card shows is read back from the skill
//!   as written — its steps, the tools it calls and whether each reads or
//!   changes things — not from anything the model said about it.
//! - **What was read before is named.** A skill rides into future prompts, so
//!   a page that talked Syn into writing one would be an instruction that
//!   outlives the conversation. Writing one after reading is allowed — it is
//!   inert — but the card says what had been read, the way held changes do.
//! - **A recipe is checked before it is kept.** `recipe::problems`, the same
//!   check the skills screen runs, and nothing that reshapes a kind or is run
//!   by the engine can be a step.
//! - **Only Syn's own unapproved drafts can be rewritten.** A skill the person
//!   wrote, or one already approved, keeps its name.

use serde::Serialize;
use serde_json::Value;

use crate::syn::skill::{self, Skill, Tier};

/// The tool.
pub const TOOL: &str = "skill_draft";

/// Emitted when a run ends having written skills, for the card.
pub const EVENT: &str = "syn-skills-proposed";

/// Frontmatter: the conversation a draft was proposed in, so its card is shown
/// there.
pub const PROPOSED_IN: &str = "proposed_in";
/// Frontmatter: what the run had read from outside before writing it.
pub const READ_BEFORE: &str = "read_before";
/// Frontmatter: when the person kept it.
pub const APPROVED_AT: &str = "approved_at";

/// What the model asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    /// The procedure in words, for the person and for Syn following it.
    pub steps: String,
    /// A fixed list of calls, when the job is always the same calls.
    pub recipe: Option<crate::syn::recipe::Recipe>,
}

fn text(args: &Value, key: &str) -> String {
    args.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string()
}

/// Read the call. Errors are said in words the model can act on.
pub fn parse(args: &Value, known_tools: &[String]) -> Result<Draft, String> {
    let name = text(args, "name");
    if name.is_empty() || name.chars().count() > 60 {
        return Err("Give the skill a short name (at most 60 characters) the user would recognise.".into());
    }
    let when_to_use = text(args, "when_to_use");
    if when_to_use.is_empty() {
        return Err("Say in `when_to_use` when this skill should be used — the words the user would ask with.".into());
    }
    let steps = text(args, "steps");
    let recipe = match args.get("recipe") {
        None | Some(Value::Null) => None,
        Some(Value::String(yaml)) if yaml.trim().is_empty() => None,
        Some(Value::String(yaml)) => Some(
            serde_yaml::from_str::<crate::syn::recipe::Recipe>(yaml)
                .map_err(|e| format!("`recipe` is not readable YAML: {e}"))?,
        ),
        Some(other) => Some(
            serde_json::from_value::<crate::syn::recipe::Recipe>(other.clone())
                .map_err(|e| format!("`recipe` is not a recipe: {e}"))?,
        ),
    };
    if let Some(r) = &recipe {
        let problems = crate::syn::recipe::problems(r, known_tools);
        if !problems.is_empty() {
            return Err(format!("The recipe cannot be kept: {}", problems.join("; ")));
        }
    }
    if steps.is_empty() && recipe.is_none() {
        return Err("Write the procedure in `steps`, and give `recipe` too when it is always the same calls.".into());
    }
    Ok(Draft { name, description: text(args, "description"), when_to_use, steps, recipe })
}

/// The skill's body: the steps in words, then the recipe block if there is one.
pub fn body(draft: &Draft) -> String {
    let mut out = draft.steps.trim().to_string();
    if let Some(r) = &draft.recipe {
        let yaml = serde_yaml::to_string(r).unwrap_or_default();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(crate::syn::recipe::FENCE);
        out.push('\n');
        out.push_str(yaml.trim_end());
        out.push_str("\n```\n");
    }
    out
}

/// The tools a draft calls: the recipe's steps, in order, once each.
fn tools_of(draft: &Draft) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in draft.recipe.iter().flat_map(|r| r.steps.iter()) {
        if !out.contains(&s.tool) {
            out.push(s.tool.clone());
        }
    }
    out
}

/// Write the draft into the vault, switched off. Returns the skill's node id.
pub fn write<R: tauri::Runtime>(
    ctx: &crate::syn::tools::ToolContext<'_, R>,
    draft: &Draft,
    conversation_id: Option<&str>,
    source_run: Option<&str>,
    read_before: &[String],
) -> Result<String, String> {
    let existing = {
        let db = ctx.db.lock().map_err(|e| e.to_string())?;
        skill::find(&skill::all(&db).map_err(|e| e.to_string())?, &draft.name).cloned()
    };
    if let Some(s) = &existing {
        if s.author != "syn" || s.enabled {
            return Err(format!(
                "There is already a skill called `{}` that the user keeps. Choose another name, or tell the user what \
                 you would change in it.",
                s.name
            ));
        }
    }
    let tier = if draft.recipe.is_some() { Tier::Recipe } else { Tier::Prose };
    let version = existing.as_ref().map_or(1, |s| s.version + 1);
    let mut properties =
        skill::frontmatter(&draft.name, &draft.description, &draft.when_to_use, tier, &tools_of(draft), "syn", false, version);
    if let Some(map) = properties.as_object_mut() {
        if let Some(c) = conversation_id {
            map.insert(PROPOSED_IN.into(), Value::from(c));
        }
        if let Some(r) = source_run {
            map.insert("source_run".into(), Value::from(r));
        }
        map.insert(READ_BEFORE.into(), serde_json::json!(read_before));
    }
    // The app writing, not the model: Syn's own kinds have their own door, and
    // this is the door for a skill. See `taint::reserved_type`.
    let app_ctx = crate::syn::tools::ToolContext { db: ctx.db, vault_path: ctx.vault_path, app: ctx.app, run_id: ctx.run_id, model: None };
    let said = match &existing {
        Some(s) => crate::syn::tools::execute_tool(
            &app_ctx,
            "update_node",
            &serde_json::json!({ "node_id": s.id, "content": body(draft), "properties": properties }),
        ),
        None => crate::syn::tools::execute_tool(
            &app_ctx,
            "create_node",
            &serde_json::json!({ "node_type": skill::SKILL_TYPE, "title": draft.name, "content": body(draft), "properties": properties }),
        ),
    }
    .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&said).unwrap_or(Value::Null);
    if let Some(e) = v.get("error") {
        return Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()));
    }
    // The id as the index has it, found again by name: the one thing the
    // card, the answer and the run are all going to look it up by.
    let db = ctx.db.lock().map_err(|e| e.to_string())?;
    skill::find(&skill::all(&db).map_err(|e| e.to_string())?, &draft.name)
        .map(|s| s.id.clone())
        .or_else(|| v.get("id").and_then(Value::as_str).map(str::to_string))
        .ok_or_else(|| "The skill was written but could not be found again.".to_string())
}

// ═══════════════════════════════════════════════════════════════
//  FROM A JOB DONE TWICE
// ═══════════════════════════════════════════════════════════════

/// The same job, done twice in one run, as a recipe — when it can be one.
///
/// The two passes are compared call by call. What is the same both times is
/// written into the step; what differs becomes a parameter, named by the
/// argument it was. That is only a recipe if each difference was an *input* —
/// something said, not something a step before it found. A value that turns
/// up in an earlier step's result in the same pass was picked out of that
/// result, and a recipe cannot pick; then this says `None`, and the job stays
/// written in words.
///
/// Nothing a recipe cannot call can be a step: a tool the engine runs, or one
/// that reshapes a kind.
pub fn recipe_from_run(run: &crate::syn::run::Run, chain: &[String]) -> Option<crate::syn::recipe::Recipe> {
    let calls = ok_calls(run);
    let first = pass_at(&calls, chain, 0)?;
    let second = pass_at(&calls, chain, first + chain.len())?;
    recipe_from_passes(&calls[first..first + chain.len()], &calls[second..second + chain.len()], chain)
}

/// The same job, done in two runs, as a recipe — `recipe_from_run` for the
/// habit `skill::chain_across_runs` finds. `earlier` is the first pass and
/// `now` the second, so what `now` found on the way is what cannot be a
/// parameter.
pub fn recipe_across_runs(
    earlier: &crate::syn::run::Run,
    now: &crate::syn::run::Run,
    chain: &[String],
) -> Option<crate::syn::recipe::Recipe> {
    let (a, b) = (ok_calls(earlier), ok_calls(now));
    let first = pass_at(&a, chain, 0)?;
    let second = pass_at(&b, chain, 0)?;
    recipe_from_passes(&a[first..first + chain.len()], &b[second..second + chain.len()], chain)
}

fn ok_calls(run: &crate::syn::run::Run) -> Vec<&crate::syn::run::Step> {
    run.steps.iter().filter(|s| s.ok == Some(true) && s.tool.is_some()).collect()
}

/// Where `chain` next starts in `calls`, from `from` on.
fn pass_at(calls: &[&crate::syn::run::Step], chain: &[String], from: usize) -> Option<usize> {
    let len = chain.len();
    (from..calls.len()).find(|&i| {
        calls.get(i..i + len).is_some_and(|w| w.iter().zip(chain).all(|(s, t)| s.tool.as_deref() == Some(t)))
    })
}

/// Two passes of `chain`, call for call, as a recipe. See `recipe_from_run`.
fn recipe_from_passes(
    first: &[&crate::syn::run::Step],
    second: &[&crate::syn::run::Step],
    chain: &[String],
) -> Option<crate::syn::recipe::Recipe> {
    use crate::syn::recipe::{Param, Recipe, Step};
    if chain.iter().any(|t| crate::syn::policy::engine(t) || crate::syn::policy::of(t).is_none_or(|p| p.power == crate::syn::policy::Power::Structural)) {
        return None;
    }
    let len = chain.len();

    let mut params: Vec<Param> = Vec::new();
    // Each pair of values that differ is one parameter, wherever it appears.
    let mut named: Vec<((Value, Value), String)> = Vec::new();
    let mut steps = Vec::new();
    for k in 0..len {
        let a = first[k].args.clone().unwrap_or(Value::Null);
        let b = second[k].args.clone().unwrap_or(Value::Null);
        // What the second pass had found by this step: a value found there is
        // picked from a result, not given.
        let found: String = second[..k]
            .iter()
            .map(|s| s.full.clone().unwrap_or_else(|| s.preview.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        let args = merge(&a, &b, "value", &found, &mut params, &mut named)?;
        steps.push(Step { tool: chain[k].clone(), args, bind: None, when: None });
    }
    Some(Recipe { params, steps })
}

/// One call's arguments from both passes, with what differs made a parameter.
fn merge(
    a: &Value,
    b: &Value,
    key: &str,
    found: &str,
    params: &mut Vec<crate::syn::recipe::Param>,
    named: &mut Vec<((Value, Value), String)>,
) -> Option<Value> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            if x.len() != y.len() || x.keys().any(|k| !y.contains_key(k)) {
                return None;
            }
            let mut out = serde_json::Map::new();
            for (k, va) in x {
                out.insert(k.clone(), merge(va, &y[k], k, found, params, named)?);
            }
            Some(Value::Object(out))
        }
        (Value::Array(x), Value::Array(y)) if x == y => Some(a.clone()),
        _ if a == b => Some(a.clone()),
        (Value::Array(_), _) | (_, Value::Array(_)) | (Value::Object(_), _) | (_, Value::Object(_)) => None,
        _ => {
            let shown = match b {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if shown.chars().count() >= 3 && found.contains(&shown) {
                return None;
            }
            let pair = (a.clone(), b.clone());
            let name = match named.iter().find(|(p, _)| *p == pair) {
                Some((_, n)) => n.clone(),
                None => {
                    let mut n = key.to_string();
                    let mut i = 2;
                    while params.iter().any(|p| p.name == n) {
                        n = format!("{key}_{i}");
                        i += 1;
                    }
                    params.push(crate::syn::recipe::Param {
                        name: n.clone(),
                        description: format!("e.g. {shown}"),
                        required: true,
                    });
                    named.push((pair, n.clone()));
                    n
                }
            };
            Some(Value::String(format!("{{{{{name}}}}}")))
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  THE CARD
// ═══════════════════════════════════════════════════════════════

/// One call a skill makes, as the card shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Call {
    pub tool: String,
    /// `read`, `write`, `structural`, `browse`, or `other`.
    pub does: &'static str,
}

/// A skill Syn wrote, as its card shows it — read from the file.
#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub skill_id: String,
    pub conversation_id: Option<String>,
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    pub tier: Tier,
    /// The steps in words, without the recipe block.
    pub steps: String,
    pub calls: Vec<Call>,
    pub read_before: Vec<String>,
    pub version: u32,
}

fn does(tool: &str) -> &'static str {
    use crate::syn::policy::Power;
    match crate::syn::policy::of(tool).map(|p| p.power) {
        Some(Power::Read) => "read",
        Some(Power::Write) | Some(Power::DraftFromSpec) => "write",
        Some(Power::Structural) => "structural",
        Some(Power::Browse) => "browse",
        None => "other",
    }
}

/// The card for a skill, from the skill as it is in the vault.
pub fn proposal(s: &Skill, node: &crate::models::node::NodeMetadata) -> Proposal {
    let steps = match s.body.find(crate::syn::recipe::FENCE) {
        Some(at) => s.body[..at].trim().to_string(),
        None => s.body.trim().to_string(),
    };
    let tools: Vec<String> = match crate::syn::recipe::parse(&s.body) {
        Ok(Some(r)) => {
            let mut out: Vec<String> = Vec::new();
            for st in r.steps {
                if !out.contains(&st.tool) {
                    out.push(st.tool);
                }
            }
            out
        }
        _ => s.tools.clone(),
    };
    Proposal {
        skill_id: s.id.clone(),
        conversation_id: node.properties.get(PROPOSED_IN).and_then(Value::as_str).map(str::to_string),
        name: s.name.clone(),
        description: s.description.clone(),
        when_to_use: s.when_to_use.clone(),
        tier: s.tier,
        steps,
        calls: tools.into_iter().map(|t| Call { does: does(&t), tool: t }).collect(),
        read_before: node
            .properties
            .get(READ_BEFORE)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
        version: s.version,
    }
}

/// Every skill Syn wrote that is waiting for the person, newest last.
pub fn waiting(db: &crate::db::DbBridge) -> Vec<Proposal> {
    let Ok(nodes) = db.get_nodes_by_type(skill::SKILL_TYPE) else { return Vec::new() };
    let mut out: Vec<(String, Proposal)> = nodes
        .iter()
        .filter_map(|n| {
            let s = Skill::from_node(n);
            let asked = n.properties.get(PROPOSED_IN).is_some();
            (s.author == "syn" && !s.enabled && asked && n.properties.get(APPROVED_AT).is_none())
                .then(|| (n.updated_at.clone(), proposal(&s, n)))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, p)| p).collect()
}

/// The person's answer: keep it (switched on, and marked as theirs to have
/// approved), or not (to the trash, where it can be got back).
pub fn answer<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    db: &crate::db::DbState,
    vault_path: &str,
    skill_id: &str,
    keep: bool,
) -> Result<(), String> {
    let ctx = crate::syn::tools::ToolContext { db, vault_path, app, run_id: None, model: None };
    let said = if keep {
        crate::syn::tools::execute_tool(
            &ctx,
            "update_node",
            &serde_json::json!({
                "node_id": skill_id,
                "properties": { "enabled": true, APPROVED_AT: chrono::Utc::now().to_rfc3339() },
            }),
        )
    } else {
        crate::syn::tools::execute_tool(&ctx, "trash_node", &serde_json::json!({ "node_id": skill_id }))
    }
    .map_err(|e| e.to_string())?;
    if let Some(e) = serde_json::from_str::<Value>(&said).ok().and_then(|v| v.get("error").cloned()) {
        return Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string()));
    }
    // Kept on this device, by the person, on the card: allowed here as it is.
    if keep {
        let db = db.lock().map_err(|e| e.to_string())?;
        if let Some(s) = crate::syn::skill::all(&db).map_err(|e| e.to_string())?.into_iter().find(|s| s.id == skill_id) {
            crate::syn::skill::approve_here(vault_path, &s).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syn::run::{Budget, Run, Step as RunStep, StepKind};
    use serde_json::json;

    fn step(tool: &str, args: Value, result: &str) -> RunStep {
        RunStep {
            index: 0,
            kind: StepKind::ToolCall,
            iteration: 0,
            tool: Some(tool.into()),
            args: Some(args),
            ok: Some(true),
            reversal: None,
            preview: result.into(),
            full: None,
            tokens: None,
            usage: Default::default(),
            ms: 0,
            at: String::new(),
        }
    }

    fn run_of(steps: Vec<RunStep>) -> Run {
        let mut r = Run::new("x", None, Budget::from_settings(&crate::models::syn::SynSettings::default()));
        r.steps = steps;
        r
    }

    fn known() -> Vec<String> {
        crate::syn::tools::get_tool_definitions().into_iter().map(|d| d.function.name).collect()
    }

    /// What was said each time becomes a parameter; what was the same stays.
    #[test]
    fn a_job_done_twice_with_different_inputs_is_a_recipe() {
        let run = run_of(vec![
            step("query_nodes", json!({ "query": "type:task status:done since:2026-10-01" }), "[]"),
            step("create_node", json!({ "node_type": "note", "title": "Tổng kết 2026-10-01" }), "{}"),
            step("timeline", json!({ "span": "week" }), "{}"),
            step("query_nodes", json!({ "query": "type:task status:done since:2026-10-08" }), "[]"),
            step("create_node", json!({ "node_type": "note", "title": "Tổng kết 2026-10-08" }), "{}"),
            step("timeline", json!({ "span": "week" }), "{}"),
        ]);
        let chain = vec!["query_nodes".to_string(), "create_node".to_string(), "timeline".to_string()];
        let r = recipe_from_run(&run, &chain).expect("a recipe");
        assert_eq!(r.params.len(), 2, "{:?}", r.params);
        assert_eq!(r.steps[0].args["query"], json!("{{query}}"));
        assert_eq!(r.steps[1].args["node_type"], json!("note"), "the same both times stays");
        assert_eq!(r.steps[1].args["title"], json!("{{title}}"));
        assert!(crate::syn::recipe::problems(&r, &known()).is_empty(), "{:?}", crate::syn::recipe::problems(&r, &known()));
    }

    /// A value picked from what a step before found is not an input, and a
    /// recipe cannot pick: no recipe, and the job stays in words.
    #[test]
    fn a_value_picked_from_a_result_is_not_a_recipe() {
        let run = run_of(vec![
            step("query_nodes", json!({ "query": "Q3" }), r#"{"results":[{"id":"Notes/q3.md"}]}"#),
            step("get_node", json!({ "node_id": "Notes/q3.md" }), "{}"),
            step("list_schemas", json!({}), "{}"),
            step("query_nodes", json!({ "query": "Q4" }), r#"{"results":[{"id":"Notes/q4.md"}]}"#),
            step("get_node", json!({ "node_id": "Notes/q4.md" }), "{}"),
            step("list_schemas", json!({}), "{}"),
        ]);
        let chain = vec!["query_nodes".to_string(), "get_node".to_string(), "list_schemas".to_string()];
        assert!(recipe_from_run(&run, &chain).is_none());
    }

    /// The habit across conversations reads its recipe off two runs exactly as
    /// one run's two passes are read: what was said differently each week is
    /// a parameter, and the pass is found wherever it sits in each run.
    #[test]
    fn a_job_done_in_two_runs_is_a_recipe_the_same_way() {
        let last_week = run_of(vec![
            step("get_node", json!({ "node_id": "Notes/a.md" }), "{}"),
            step("query_nodes", json!({ "query": "type:task status:done since:2026-10-01" }), "[]"),
            step("create_node", json!({ "node_type": "note", "title": "Tổng kết 2026-10-01" }), "{}"),
            step("timeline", json!({ "span": "week" }), "{}"),
        ]);
        let this_week = run_of(vec![
            step("query_nodes", json!({ "query": "type:task status:done since:2026-10-08" }), "[]"),
            step("create_node", json!({ "node_type": "note", "title": "Tổng kết 2026-10-08" }), "{}"),
            step("timeline", json!({ "span": "week" }), "{}"),
        ]);
        let chain = vec!["query_nodes".to_string(), "create_node".to_string(), "timeline".to_string()];
        let r = recipe_across_runs(&last_week, &this_week, &chain).expect("a recipe");
        assert_eq!(r.params.len(), 2, "{:?}", r.params);
        assert_eq!(r.steps[0].args["query"], json!("{{query}}"));
        assert_eq!(r.steps[1].args["node_type"], json!("note"));
        assert!(recipe_across_runs(&last_week, &run_of(vec![]), &chain).is_none(), "no second pass, no recipe");
    }

    #[test]
    fn nothing_the_engine_runs_is_a_step() {
        let run = run_of(vec![]);
        assert!(recipe_from_run(&run, &["browse".to_string(), "get_node".to_string(), "create_node".to_string()]).is_none());
    }

    #[test]
    fn a_draft_is_read_and_checked() {
        let ok = parse(&json!({ "name": "Tổng kết tuần", "when_to_use": "tổng kết tuần", "steps": "1. Tìm task xong.\n2. Viết note." }), &known())
            .expect("prose");
        assert!(ok.recipe.is_none());
        assert!(parse(&json!({ "name": "x", "steps": "y" }), &known()).is_err(), "when to use is required");
        let bad = parse(
            &json!({ "name": "x", "when_to_use": "y", "recipe": "steps:\n  - tool: delete_kind\n    args: {}\n" }),
            &known(),
        );
        assert!(bad.unwrap_err().contains("reshapes"), "the recipe check runs");
        let r = parse(
            &json!({ "name": "x", "when_to_use": "y", "steps": "", "recipe": { "params": [{ "name": "q", "required": true }], "steps": [{ "tool": "query_nodes", "args": { "query": "{{q}}" } }] } }),
            &known(),
        )
        .expect("a recipe as JSON");
        let b = body(&r);
        assert!(b.contains("```recipe") && crate::syn::recipe::parse(&b).unwrap().is_some(), "{b}");
    }
}
