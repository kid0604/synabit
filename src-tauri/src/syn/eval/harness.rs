//! Running one task the way a person would.
//!
//! Each turn is sent through `send_message_inner`, the function every surface
//! of the app answers through. When a run stops to ask, the harness answers
//! as the app's buttons would — "just this once" for permission, the first
//! candidate for "which one" — and carries on, up to a few times a turn. How
//! often that happened is part of the result: a stop the task did not need is
//! friction, and the score counts it.

use std::time::Instant;

use crate::models::syn::{SynChatRequest, SynMessage, SynProvider};
use crate::syn::run::{Run, RunState};

use super::fixture;
use super::grade::{self, Evidence};
use super::tasks::Task;

/// How many times one turn may stop and be answered before the harness gives up.
const MAX_ANSWERS_PER_TURN: usize = 4;

/// One trial of one task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Outcome {
    pub task: &'static str,
    pub category: &'static str,
    /// Whether the task is in the `hard` tier.
    pub hard: bool,
    pub trial: u32,
    pub passed: bool,
    /// Checks the task asked for that failed.
    pub failures: Vec<String>,
    /// The universal safety checks that failed. Any one fails the task.
    pub safety: Vec<String>,
    /// Runs that stopped to ask.
    pub stops: usize,
    pub runs: usize,
    pub rounds: u32,
    pub tool_calls: u32,
    /// Tool calls that came back with an error, or repeated the call before them exactly.
    pub wasted_calls: u32,
    pub tokens: u64,
    pub wall_ms: u64,
    pub budget_exhausted: bool,
    /// The last run's footing, as `footing` names it.
    pub footing: Option<String>,
    /// Set when the run itself failed (provider down, panic), not the task.
    pub error: Option<String>,
    /// The tools called, in order, `!` marking one that failed — for reading a failure.
    pub trace: Vec<String>,
    /// The final answer, cut to a few hundred characters.
    pub answer: String,
    /// What the runs asked the web for, in order.
    pub visited: Vec<String>,
}

fn request(conversation: &str, message: &str, resume: Option<String>) -> SynChatRequest {
    serde_json::from_value(serde_json::json!({
        "conversation_id": conversation,
        "message": message,
        "model": null,
        "temperature": null,
    }))
    .map(|mut r: SynChatRequest| {
        r.resume_run = resume;
        r
    })
    .expect("a chat request")
}

fn latest(vault: &str, conversation: &str) -> Option<Run> {
    crate::syn::run::latest_for(vault, conversation)
}

/// Run `task` once against the provider the caller set with `use_provider`.
pub async fn run(task: &Task, trial: u32, provider: SynProvider, model: &str) -> Outcome {
    let started = Instant::now();
    let vault = fixture::build(task.preset, provider, model);
    let _web = super::web::serve(task.web.clone());
    let nodes_before = grade::nodes(&vault).iter().filter(|(_, ty, _, _, _)| !ty.starts_with("syn_")).count();

    let mut answers: Vec<SynMessage> = Vec::new();
    let mut seen_runs: Vec<String> = Vec::new();
    let mut error = None;
    let mut conversation = String::new();

    'turns: for turn in &task.turns {
        if conversation.is_empty() || turn.new_conversation {
            conversation = crate::syn::conversation::create_conversation(&vault.path, Some(task.id.to_string()))
                .expect("conversation")
                .id;
        }
        let mut message = turn.ask.to_string();
        let mut resume = None;
        for _ in 0..=MAX_ANSWERS_PER_TURN {
            let sent = crate::commands::syn::send_message_inner(
                vault.handle(),
                &vault.path,
                request(&conversation, &message, resume.take()),
                crate::syn::surface::Surface::App,
            )
            .await;
            match sent {
                Ok(answer) => answers.push(answer),
                Err(e) => {
                    error = Some(e.to_string());
                    break 'turns;
                }
            }
            let Some(last) = latest(&vault.path, &conversation) else { break };
            if !seen_runs.contains(&last.id) {
                seen_runs.push(last.id.clone());
            }
            match last.state {
                RunState::AwaitingConsent => {
                    let _ = crate::commands::syn::syn_answer_consent(
                        vault.path.clone(),
                        last.id.clone(),
                        crate::syn::consent::Answer::Once,
                    )
                    .await;
                    message = String::new();
                    resume = Some(last.id);
                }
                RunState::AwaitingChoice => {
                    let Some(first) = last.pending_choice.as_ref().and_then(|c| c.candidates.first()).cloned() else { break };
                    let _ = crate::commands::syn::syn_answer_choice(vault.path.clone(), last.id.clone(), first.id.clone()).await;
                    message = first.title.clone();
                    resume = Some(last.id);
                }
                _ => break,
            }
        }
    }

    // Every run this task made, helpers included, in the order they began.
    let mut runs: Vec<Run> = crate::syn::run::load_all(&vault.path).unwrap_or_default();
    runs.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    let top: Vec<Run> = runs.iter().filter(|r| r.parent_run_id.is_none()).cloned().collect();
    let visited = super::web::visited();

    let evidence = Evidence { vault: &vault, runs: &runs, answers: &answers, visited: &visited, nodes_before };
    let failures: Vec<String> = task.checks.iter().filter_map(|c| grade::check(c, &evidence).err()).collect();
    let safety = grade::universal(&evidence);

    let mut tool_calls = 0;
    let mut wasted = 0;
    for run in &runs {
        let mut previous: Option<(String, String)> = None;
        for step in run.steps.iter().filter(|s| s.kind == crate::syn::run::StepKind::ToolCall) {
            tool_calls += 1;
            let this = (step.tool.clone().unwrap_or_default(), step.args.as_ref().map(|a| a.to_string()).unwrap_or_default());
            if step.ok == Some(false) || previous.as_ref() == Some(&this) {
                wasted += 1;
            }
            previous = Some(this);
        }
    }

    let trace: Vec<String> = runs
        .iter()
        .flat_map(|r| r.steps.iter())
        .filter(|s| s.kind == crate::syn::run::StepKind::ToolCall)
        .map(|s| {
            let args: String = s.args.as_ref().map(|a| a.to_string()).unwrap_or_default().chars().take(400).collect();
            format!("{}{}({args})", s.tool.clone().unwrap_or_default(), if s.ok == Some(false) { "!" } else { "" })
        })
        .collect();
    let answer: String = answers.last().map(|m| m.content.chars().take(300).collect()).unwrap_or_default();

    Outcome {
        visited: visited.clone(),
        trace,
        answer,
        task: task.id,
        category: task.category.name(),
        hard: task.hard,
        trial,
        passed: error.is_none() && failures.is_empty() && safety.is_empty(),
        failures,
        safety,
        stops: grade::stops(&top),
        runs: top.len(),
        rounds: top.iter().map(|r| r.spent.iterations as u32).sum(),
        tool_calls,
        wasted_calls: wasted,
        tokens: runs.iter().map(|r| r.spent.tokens).sum(),
        wall_ms: started.elapsed().as_millis() as u64,
        budget_exhausted: top.iter().any(|r| r.state == RunState::BudgetExhausted),
        footing: top.last().and_then(|r| r.footing).map(|f| format!("{f:?}").to_lowercase()),
        error,
    }
}
