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
    /// Every turn's answer whole, in order, for `tests::ask`; not in the report.
    #[serde(skip)]
    pub full_answer: String,
    /// What the runs asked the web for, in order.
    pub visited: Vec<String>,
    /// Every visual the trial left, as a page that stands alone, for
    /// `quality` to draw and look at; not in the report.
    #[serde(skip)]
    pub visual_pages: Vec<String>,
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

/// Write `history` into the conversation as turns already taken, a minute apart
/// and a day before now, so the first turn is asked after them.
fn hold(vault: &str, conversation: &str, history: &[super::tasks::Said]) {
    let mut full = crate::syn::conversation::get_conversation(vault, conversation).expect("the conversation");
    let start = chrono::Utc::now() - chrono::Duration::days(1);
    for (i, said) in history.iter().enumerate() {
        let message: SynMessage = serde_json::from_value(serde_json::json!({
            "id": format!("held-{i}"),
            "role": said.role,
            "content": said.content,
            "timestamp": (start + chrono::Duration::minutes(i as i64)).to_rfc3339(),
        }))
        .expect("a message");
        full.messages.push(message);
    }
    crate::syn::conversation::save_conversation(vault, &full).expect("history written");
}

fn latest(vault: &str, conversation: &str) -> Option<Run> {
    crate::syn::run::latest_for(vault, conversation)
}

/// Run `task` once against the provider the caller set with `use_provider`.
pub async fn run(task: &Task, trial: u32, provider: SynProvider, model: &str) -> Outcome {
    let started = Instant::now();
    let vault = fixture::build(task.preset, provider, model);
    if !task.visuals {
        let mut settings = crate::syn::settings::load_settings(&vault.path).expect("settings");
        settings.visuals = false;
        crate::syn::settings::save_settings(&vault.path, &settings).expect("settings");
    }
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
    let pi = if task.ssh { Some(crate::syn::connector::ssh::tests::pi().await) } else { None };
    let pi_host = pi.as_ref().map(|(port, ..)| format!("127.0.0.1:{port}")).unwrap_or_default();
    if task.pi_connected {
        super::connectors::connect_pi(&vault, &pi_host).await;
    }
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
            // What it already held, before the first turn only.
            if answers.is_empty() && !task.history.is_empty() {
                hold(&vault.path, &conversation, &task.history);
            }
        }
        // `BASE` in what the person says is the REST server's address, as in
        // a declaration: a spec to connect to is named by where it is.
        let mut message = match rest.as_ref() {
            Some(r) => turn.ask.replace("BASE", &r.base),
            None => turn.ask.to_string(),
        }
        .replace("PIHOST", &pi_host);
        let mut resume = None;
        let mut carried_on = 0;
        for _ in 0..=MAX_ANSWERS_PER_TURN {
            let sent = crate::commands::syn::send_message_inner(
                vault.handle(),
                &vault.path,
                request(&conversation, &message, resume.take()),
                task.surface,
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
                        // A machine over SSH: whatever Syn named its inputs,
                        // the person types its address and their password.
                        let typed = |i: &crate::syn::connector::manifest::Input| {
                            given(&i.name).or_else(|| {
                                (task.ssh && i.kind == crate::syn::connector::manifest::InputKind::Address).then(|| format!("ssh://pi@{pi_host}"))
                            })
                        };
                        let inputs: std::collections::BTreeMap<String, String> = m
                            .inputs
                            .iter()
                            .filter(|i| i.kind != crate::syn::connector::manifest::InputKind::Secret)
                            .filter_map(|i| typed(i).map(|v| (i.name.clone(), v)))
                            .collect();
                        let secret = m
                            .auth
                            .secret()
                            .and_then(|name| given(name).or_else(|| task.ssh.then(|| crate::syn::connector::ssh::tests::PASSWORD.to_string())));
                        if let (Some(name), Some(value)) = (m.auth.secret(), secret.as_ref()) {
                            crate::syn::connector::declared::TEST_SECRETS
                                .lock()
                                .expect("secrets")
                                .insert(crate::commands::connector::secret_slot(&m.id, name), value.clone());
                        }
                        ask_counted = super::connectors::asked_on_card(&m);
                        let network = crate::syn::connector::instance::network_for(&m.with_inputs(&inputs).base_url);
                        let mut pinned = None;
                        // Twice at most: the second time with the machine's key
                        // pinned, as "This is my machine" does when it is shown.
                        for _ in 0..2 {
                            let Ok(inst) = crate::syn::connector::instance::approve(
                                &vault.path,
                                &m,
                                crate::syn::connector::instance::Choices { network, inputs: inputs.clone(), pinned_cert: pinned.clone(), ..Default::default() },
                                &chrono::Utc::now().to_rfc3339(),
                            ) else {
                                break;
                            };
                            let tried = crate::commands::connector::probe(&m, &inst, secret.as_deref()).await;
                            if verbose() {
                                eprintln!("[eval] {} probe of `{}`: ok {} code {:?} said {:?}", task.id, m.id, tried.ok, tried.code, tried.said);
                            }
                            approved = tried.ok;
                            match (tried.code.as_deref(), tried.cert) {
                                (Some("host_key_unknown"), Some(key)) if pinned.is_none() => pinned = Some(key),
                                _ => break,
                            }
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
                // A stretch of work that reached its limit, carried on as the
                // person would with "Carry on" — up to twice a turn.
                RunState::BudgetExhausted if task.carries_on && carried_on < 2 => {
                    carried_on += 1;
                    message = "Carry on from where you stopped.".to_string();
                    resume = Some(last.id);
                }
                _ => {
                    // Skills it wrote wait on a card under the answer: kept as
                    // the task says a person would, or let go.
                    for id in &last.drafted_skills {
                        let state = vault.db();
                        let _ = crate::syn::drafting::answer(vault.handle(), state.inner(), &vault.path, id, task.approves_skills);
                    }
                    // Changes held after reading wait on a card at the end of
                    // the work: approved as the task says a person would, or
                    // let go. See `syn::changes`.
                    if let Some(set) = last.changes.as_ref().filter(|c| c.awaiting()) {
                        let approved: Vec<String> = if task.approves_changes {
                            set.changes.iter().map(|c| c.hash.clone()).collect()
                        } else {
                            Vec::new()
                        };
                        let _ = crate::commands::syn::answer_changes(vault.handle(), vault.db().inner(), &vault.path, &last.id, &approved);
                    }
                    break;
                }
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
    keep_visuals(&vault, task.id, trial);
    let visual_pages = grade::visuals(&vault).iter().map(|html| standalone(html)).collect();

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
    if verbose() {
        for (n, run) in runs.iter().enumerate() {
            eprintln!("[eval] {} trial {trial} run {n} ({:?})", task.id, run.state);
            for step in run.steps.iter().filter(|s| s.kind == crate::syn::run::StepKind::ToolCall) {
                let preview: String = step.preview.chars().take(300).collect();
                eprintln!("[eval]   {}{} = {preview}", step.tool.clone().unwrap_or_default(), if step.ok == Some(false) { "!" } else { "" });
            }
        }
    }

    Outcome {
        visual_pages,
        visited: visited.clone(),
        full_answer: answers.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n\n── next turn ──\n\n"),
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

/// `SYN_EVAL_KEEP=<dir>`: every visual a trial left, as the page "Download"
/// writes — `<task>-<trial>-<n>.html`, standing alone — so a visual that
/// failed a check can be looked at after its vault is gone.
fn keep_visuals(vault: &fixture::Vault, task: &str, trial: u32) {
    let Some(dir) = std::env::var_os("SYN_EVAL_KEEP").filter(|d| !d.is_empty()) else { return };
    let _ = std::fs::create_dir_all(&dir);
    for (n, html) in grade::visuals(vault).iter().enumerate() {
        let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{task}-{trial}-{}.html", n + 1)), standalone(html));
    }
}

/// A visual as the page "Download" writes: standing alone, in the light theme.
fn standalone(html: &str) -> String {
    #[cfg(desktop)]
    return crate::syn::visual::assets::standalone(html, "light");
    #[cfg(not(desktop))]
    return html.to_string();
}

/// `SYN_EVAL_VERBOSE=1`: what each probe of a connector came to, and what each
/// tool call returned, on stderr. The report keeps only the calls; when a task
/// fails on what a tool *said*, this is where it shows.
fn verbose() -> bool {
    std::env::var("SYN_EVAL_VERBOSE").is_ok_and(|v| !v.is_empty())
}
