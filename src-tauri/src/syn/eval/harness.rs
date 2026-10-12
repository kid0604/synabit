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
    let _servers = super::connectors::start(&vault.path, &task.connectors).await;
    let rest = if task.rest.is_empty() {
        None
    } else {
        let routes = task
            .rest
            .iter()
            .map(|(path, body)| (*path, crate::syn::connector::fake_rest::Answer { status: "200 OK", headers: String::new(), body: body.clone() }))
            .chain(["/sheets/values", "/mail/send"].map(|p| (p, crate::syn::connector::fake_rest::Answer::json(serde_json::json!({ "ok": true })))))
            .collect();
        Some(crate::syn::connector::fake_rest::serve(routes).await)
    };
    super::connectors::declare(&vault, &task.declared, rest.as_ref().map(|r| r.base.as_str()));
    for (path, text) in &task.files {
        let at = match path.strip_prefix("@downloads/") {
            Some(rest) => std::path::Path::new(&super::connectors::downloads(&vault)).join(rest),
            None => std::path::Path::new(&vault.path).join(path),
        };
        if let Some(dir) = at.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(&at, text).expect("a task's file");
    }
    let nodes_before = grade::nodes(&vault).iter().filter(|(_, ty, _, _, _)| !ty.starts_with("syn_")).count();

    let mut answers: Vec<SynMessage> = Vec::new();
    let mut seen_runs: Vec<String> = Vec::new();
    let mut error = None;
    let mut conversation = String::new();
    // How many things the connect cards asked the person to type.
    let mut card_questions = 0usize;

    'turns: for turn in &task.turns {
        if conversation.is_empty() || turn.new_conversation {
            conversation = crate::syn::conversation::create_conversation(&vault.path, Some(task.id.to_string()))
                .expect("conversation")
                .id;
        }
        // `BASE` in what the person says is the REST server's address, as in
        // a declaration: a spec to connect to is named by where it is.
        let mut message = match rest.as_ref() {
            Some(r) => turn.ask.replace("BASE", &r.base),
            None => turn.ask.to_string(),
        };
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
                RunState::AwaitingApproval if last.pending_connector.is_some() => {
                    // As the person would on the card: approved on this
                    // device, as a local server, then answered.
                    let id = last.pending_connector.clone().unwrap_or_default();
                    let drafted = {
                        let state = vault.db();
                        let db = state.lock().unwrap_or_else(|e| e.into_inner());
                        crate::syn::connector::manifest::all(&db)
                            .unwrap_or_default()
                            .into_iter()
                            .find_map(|f| f.manifest.ok().filter(|m| m.id == id))
                    };
                    // The connect card, as the person fills it: the task's values for
                    // the inputs (`BASE` is the REST server), the key to the keychain,
                    // the network worked out from the address, then one try.
                    let base = rest.as_ref().map(|r| r.base.clone()).unwrap_or_default();
                    let mut approved = false;
                    let mut ask_counted = 0usize;
                    if let Some(m) = drafted {
                        let given = |name: &str| task.connect.iter().find(|(k, _)| *k == name).map(|(_, v)| v.replace("BASE", &base));
                        let inputs: std::collections::BTreeMap<String, String> = m
                            .inputs
                            .iter()
                            .filter(|i| i.kind != crate::syn::connector::manifest::InputKind::Secret)
                            .filter_map(|i| given(&i.name).map(|v| (i.name.clone(), v)))
                            .collect();
                        let secret = m.auth.secret().and_then(given);
                        if let (Some(name), Some(value)) = (m.auth.secret(), secret.as_ref()) {
                            crate::syn::connector::declared::TEST_SECRETS
                                .lock()
                                .expect("secrets")
                                .insert(crate::commands::connector::secret_slot(&m.id, name), value.clone());
                        }
                        ask_counted = super::connectors::asked_on_card(&m);
                        let network = crate::syn::connector::instance::network_for(&m.with_inputs(&inputs).base_url);
                        if let Ok(inst) = crate::syn::connector::instance::approve(
                            &vault.path,
                            &m,
                            crate::syn::connector::instance::Choices { network, inputs, ..Default::default() },
                            &chrono::Utc::now().to_rfc3339(),
                        ) {
                            approved = crate::commands::connector::probe(&m, &inst, secret.as_deref()).await.ok;
                        }
                    }
                    card_questions += ask_counted;
                    let _ = crate::commands::syn::syn_answer_connector(vault.path.clone(), last.id.clone(), approved).await;
                    message = String::new();
                    resume = Some(last.id);
                }
                RunState::AwaitingApproval => {
                    // As the card's "approve all" would: every request Rust
                    // did not flag.
                    let approvable: Vec<String> = last
                        .writes
                        .as_ref()
                        .map(|p| p.requests.iter().filter(|r| r.problems.is_empty()).map(|r| r.hash.clone()).collect())
                        .unwrap_or_default();
                    let _ = crate::commands::syn::syn_answer_writes(vault.path.clone(), last.id.clone(), approvable).await;
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

    let sent = rest.as_ref().map(|r| r.requests()).unwrap_or_default();
    let evidence = Evidence { vault: &vault, runs: &runs, answers: &answers, visited: &visited, nodes_before, sent: &sent, card_questions };
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
