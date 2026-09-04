//! Driving one run to an answer.
//!
//! This file used to be an Ollama HTTP client with a tool-calling loop around
//! it. The client moved to `provider/`, and what was left was the loop: prune
//! the history to fit, ask for a completion, run whatever tools came back, ask
//! again, and stream the answer out as Tauri events.
//!
//! What has changed is what the loop is *about*. It used to be about a message:
//! everything it knew lived in local variables for the length of one `async fn`
//! and was gone when it returned, so there was nothing to read afterwards,
//! nothing to resume, and no ceiling but a count of rounds. It is now about a
//! `Run`, which is written to the vault as it goes — see `run.rs` for why.
//!
//! Cancellation lives here rather than in a provider because it is a property
//! of the work, not of a connection. The registry of live runs below is also
//! how `list_runs` tells a run this process is driving from one left behind by
//! a process that ended.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use tauri::Emitter;

use crate::error::AppResult;
use crate::models::syn::{SynMessage, SynStreamToken, SynToolCallEvent};
use crate::syn::provider::{ChatMessage, ChatProvider, ChatReply, ChatRequest, StreamSink};
use crate::syn::registry::{Registry, RunContext};
use crate::syn::run::{Run, RunState};

// ═══════════════════════════════════════════════════════════════
//  RUNS THIS PROCESS IS DRIVING
// ═══════════════════════════════════════════════════════════════

/// A run in flight, and the flag that stops it.
struct Live {
    /// The conversation it belongs to, so that a stop aimed at a chat can find
    /// the run behind it. `None` for a run nobody is watching.
    conversation_id: Option<String>,
    stop: Arc<AtomicBool>,
}

/// Every run being driven right now, keyed by run id.
///
/// Keyed by run rather than by conversation, which is what it used to be. A
/// conversation is a place runs happen, and one day more than one will be able
/// to happen at once; the thing being stopped is always a run.
static LIVE_RUNS: std::sync::LazyLock<RwLock<HashMap<String, Live>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

fn register_run(run_id: &str, conversation_id: Option<String>) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut live = LIVE_RUNS.write().unwrap_or_else(|e| e.into_inner());
    live.insert(
        run_id.to_string(),
        Live {
            conversation_id,
            stop: Arc::clone(&stop),
        },
    );
    stop
}

fn unregister_run(run_id: &str) {
    let mut live = LIVE_RUNS.write().unwrap_or_else(|e| e.into_inner());
    live.remove(run_id);
}

/// Whether this process is driving that run.
///
/// Read by `run::list_runs`, which uses it to tell a run that is genuinely
/// working from one whose process was closed mid-flight — those read back as
/// `Working` from disk and are not.
pub fn is_live(run_id: &str) -> bool {
    LIVE_RUNS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(run_id)
}

/// Ask a run to stop at the next safe point.
pub fn stop_run(run_id: &str) {
    let live = LIVE_RUNS.read().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = live.get(run_id) {
        entry.stop.store(true, Ordering::SeqCst);
        log::info!("[Syn] Stop requested for run {run_id}");
    }
}

/// Stop whatever is running for a conversation, or everything when `None`.
///
/// The shape the frontend has always called: the user presses stop on a chat
/// and does not know what a run is.
pub fn stop_conversation(conversation_id: Option<&str>) {
    let live = LIVE_RUNS.read().unwrap_or_else(|e| e.into_inner());
    for (run_id, entry) in live.iter() {
        let matches = match conversation_id {
            Some(wanted) => entry.conversation_id.as_deref() == Some(wanted),
            None => true,
        };
        if matches {
            entry.stop.store(true, Ordering::SeqCst);
            log::info!("[Syn] Stop requested for run {run_id}");
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  HISTORY
// ═══════════════════════════════════════════════════════════════

/// Trim a conversation to the last `max_msgs`, keeping the system prompt.
///
/// The system message carries the personality and the RAG context, so dropping
/// it because it happens to be the oldest would quietly change who the
/// assistant is halfway through a long conversation.
fn build_pruned_history(history: &[SynMessage], max_msgs: usize) -> Vec<ChatMessage> {
    let as_chat = |m: &SynMessage| ChatMessage {
        role: m.role.clone(),
        content: m.content.clone(),
        tool_calls: None,
        tool_call_id: None,
        images: m.images.clone(),
    };

    if history.len() <= max_msgs {
        return history.iter().map(as_chat).collect();
    }

    let mut messages = Vec::new();
    let has_system = history.first().map(|m| m.role == "system").unwrap_or(false);
    if has_system {
        messages.push(as_chat(&history[0]));
    }

    let skip_count = history.len() - max_msgs;
    let start_idx = if has_system {
        std::cmp::max(1, skip_count)
    } else {
        skip_count
    };

    messages.extend(history[start_idx..].iter().map(as_chat));
    messages
}

/// Whether a tool's JSON result is an answer rather than a refusal.
///
/// `execute_tool` turns a failure into `{"error": …}` rather than propagating
/// it, so that the model can read what went wrong and try something else. That
/// makes every call an `Ok`, and this is the only way to tell the two apart for
/// the transcript. A result that is not JSON at all — a truncated one, most
/// likely — counts as an answer, because it is one.
fn tool_succeeded(content: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| v.get("error").cloned())
        .is_none()
}

/// Why the driving loop stopped without an answer in hand.
enum LoopEnd {
    /// A ceiling in the run's budget was reached; the name of the one that
    /// fired.
    Ceiling(&'static str),
    /// The user pressed stop.
    Cancelled,
    /// The model returned neither text nor a tool call, which is not an answer
    /// and not a request either.
    DeadEnd,
}

// ═══════════════════════════════════════════════════════════════
//  THE ENGINE
// ═══════════════════════════════════════════════════════════════

/// Everything one run needs that is not part of the run itself.
///
/// A struct rather than twelve arguments. The previous shape had eleven and a
/// `#[allow(clippy::too_many_arguments)]` over it, and every one of them was
/// positional — two `&str`s in a row that could be swapped without the compiler
/// noticing.
pub struct DriveRequest<'a, R: tauri::Runtime> {
    pub app: &'a tauri::AppHandle<R>,
    /// The id the streamed tokens are labelled with, so the frontend can put
    /// them in the right bubble.
    pub message_id: &'a str,
    /// The conversation so far, including the system message.
    pub history: &'a [SynMessage],
    pub model: &'a str,
    pub temperature: Option<f64>,
    pub registry: &'a Registry<R>,
    pub db: &'a crate::db::DbState,
    pub vault_path: &'a str,
    pub num_ctx: u32,
    pub max_history: usize,
}

/// The loop, over whichever provider it was given.
pub struct SynEngine {
    provider: Box<dyn ChatProvider>,
}

impl SynEngine {
    pub fn new(provider: Box<dyn ChatProvider>) -> Self {
        Self { provider }
    }

    pub fn provider(&self) -> &dyn ChatProvider {
        self.provider.as_ref()
    }

    /// Stop whatever is running for a conversation, or everything when `None`.
    ///
    /// Kept as an associated function because that is how the command calls it.
    pub fn stop_generation(conversation_id: Option<&str>) {
        stop_conversation(conversation_id);
    }

    // ───────────────────────────────────────────────────────────
    //  Driving a run
    // ───────────────────────────────────────────────────────────

    /// Take a run from its goal to an answer.
    ///
    /// The loop:
    /// 1. Check the budget. Out of anything means stop asking for tools.
    /// 2. Ask for a completion with the tool definitions.
    /// 3. If the model asked for tools, run them and append the results.
    /// 4. Repeat.
    /// 5. When it answers with text instead, that is the answer.
    ///
    /// Every one of those steps is written to the run's transcript as it
    /// happens, so that a run which is cancelled, crashes, or is still going
    /// when the app is closed can be read afterwards as far as it got.
    ///
    /// The database lock is taken inside each tool, for as long as that tool
    /// needs it, and never across an HTTP call.
    pub async fn drive<R: tauri::Runtime>(
        &self,
        run: &mut Run,
        req: DriveRequest<'_, R>,
    ) -> AppResult<SynMessage> {
        let stop = register_run(&run.id, run.conversation_id.clone());
        let started = std::time::Instant::now();

        run.model = Some(req.model.to_string());
        run.provider = Some(self.provider.id());
        crate::syn::run::save_run_best_effort(req.vault_path, run);

        let result = self.drive_inner(run, &req, &stop, started).await;

        run.spent.wall_ms = started.elapsed().as_millis() as u64;
        match &result {
            Ok(_) => run.finish(RunState::Done),
            Err(e) => run.fail(e.to_string()),
        }
        crate::syn::run::save_run_best_effort(req.vault_path, run);
        unregister_run(&run.id);

        result
    }

    async fn drive_inner<R: tauri::Runtime>(
        &self,
        run: &mut Run,
        req: &DriveRequest<'_, R>,
        stop: &Arc<AtomicBool>,
        started: std::time::Instant,
    ) -> AppResult<SynMessage> {
        let mut working = build_pruned_history(req.history, req.max_history);
        // Kept alongside the run's own steps because it is what a `SynMessage`
        // carries and what the chat bubble draws. The transcript is the record;
        // this is the view the conversation file already had.
        let mut tool_log: Vec<SynToolCallEvent> = Vec::new();

        let watchers = Watchers {
            app: req.app,
            conversation_id: run.conversation_id.clone(),
            message_id: req.message_id.to_string(),
        };
        let sink = |text: &str, done: bool| watchers.send(text, done);
        let stream = TokenStream::new(&sink);
        let emit_token = |token: &str| stream.push(token);
        let stop_check = {
            let flag = Arc::clone(stop);
            move || flag.load(Ordering::SeqCst)
        };

        // Cloned rather than borrowed for the same reason the watchers own
        // their strings: every step below writes to the run.
        let run_id = run.id.clone();
        let conversation_id = run.conversation_id.clone();
        let ctx = RunContext {
            run_id: &run_id,
            db: req.db,
            vault_path: req.vault_path,
            app: req.app,
        };
        let tools = req.registry.definitions(&ctx);

        let ended: LoopEnd = 'drive: loop {
            run.spent.wall_ms = started.elapsed().as_millis() as u64;
            if let Some(which) = run.budget.exceeded_by(&run.spent) {
                break 'drive LoopEnd::Ceiling(which);
            }

            let iteration = run.spent.iterations;
            run.spent.iterations = run.spent.iterations.saturating_add(1);

            let request = || ChatRequest {
                model: req.model,
                messages: &working,
                temperature: req.temperature,
                num_ctx: req.num_ctx,
                tools: Some(&tools),
            };

            // Stream the turn when the provider can report tool calls that way.
            // It usually has nothing to say while reaching for a tool, so in
            // practice this streams exactly the turn the user is waiting to
            // read, and the answer appears as it is written.
            let turn_started = std::time::Instant::now();
            let reply = if self.provider.streams_tool_calls() {
                let sink = StreamSink {
                    on_token: &emit_token,
                    stop_requested: &stop_check,
                };
                self.provider.chat_streaming(request(), &sink).await?
            } else {
                self.provider.chat(request()).await?
            };
            let turn_ms = turn_started.elapsed().as_millis() as u64;

            // Stop is checked here, after the turn, whoever the provider is.
            //
            // It used to be checked only inside the streaming branch — which
            // meant that on Ollama, the one provider that cannot stream tool
            // calls, pressing stop during a tool-using turn did nothing at all.
            // The flag was set, the loop went round again, and the only thing
            // that ever noticed was the final answer. On a local model taking
            // twelve rounds, that is a stop button which does not stop.
            if stop_check() {
                if !reply.content.is_empty() {
                    run.record_assistant(iteration, &reply.content, reply.tokens, turn_ms);
                }
                run.note(iteration, "Stopped by the user.");
                run.finish(RunState::Cancelled);
                crate::syn::run::save_run_best_effort(req.vault_path, run);
                stream.done();
                return Ok(assemble(req.message_id, req.model, reply, started, tool_log));
            }

            if reply.tool_calls.is_empty() {
                // No tools wanted. Text is the answer; empty is a dead end.
                if reply.content.is_empty() {
                    run.note(
                        iteration,
                        "The model returned neither text nor a tool call. Asking once more \
                         without tools.",
                    );
                    crate::syn::run::save_run_best_effort(req.vault_path, run);
                    break 'drive LoopEnd::DeadEnd;
                }

                run.record_assistant(iteration, &reply.content, reply.tokens, turn_ms);
                crate::syn::run::save_run_best_effort(req.vault_path, run);

                // Only replay when nothing was streamed. A provider that
                // streams tool calls has already sent every token of this
                // answer, and emitting it again appends the reply to itself on
                // screen.
                if !self.provider.streams_tool_calls() {
                    emit_token(&reply.content);
                }
                stream.done();

                return Ok(assemble(req.message_id, req.model, reply, started, tool_log));
            }

            // Words said on the way to reaching for a tool are part of the
            // record even though they are not the answer.
            if !reply.content.is_empty() {
                run.record_assistant(iteration, &reply.content, reply.tokens, turn_ms);
            }

            // The assistant turn that asked for the tools has to go back into
            // the history, or the results that follow answer nothing.
            working.push(ChatMessage {
                role: "assistant".to_string(),
                content: reply.content.clone(),
                tool_calls: Some(reply.tool_calls.clone()),
                tool_call_id: None,
                images: None,
            });

            for tc in &reply.tool_calls {
                run.spent.wall_ms = started.elapsed().as_millis() as u64;
                if let Some(which) = run.budget.exceeded_by(&run.spent) {
                    break 'drive LoopEnd::Ceiling(which);
                }
                // A round can ask for a long chain of tools, and somebody who
                // pressed stop should not have to wait for all of them.
                if stop_check() {
                    break 'drive LoopEnd::Cancelled;
                }

                let call_started = std::time::Instant::now();
                let outcome =
                    req.registry
                        .execute(&ctx, &tc.function.name, &tc.function.arguments);

                let (content, reversal) = match outcome {
                    Ok(o) => (o.content, o.reversal),
                    Err(e) => (
                        serde_json::json!({ "error": format!("{e}") }).to_string(),
                        crate::syn::registry::Reversal::Nothing,
                    ),
                };

                run.record_tool(
                    iteration,
                    &tc.function.name,
                    tc.function.arguments.clone(),
                    tool_succeeded(&content),
                    reversal,
                    &content,
                    call_started.elapsed().as_millis() as u64,
                );
                crate::syn::run::save_run_best_effort(req.vault_path, run);

                let event = SynToolCallEvent {
                    conversation_id: conversation_id.clone().unwrap_or_default(),
                    tool_name: tc.function.name.clone(),
                    tool_args: tc.function.arguments.clone(),
                    result_preview: content.chars().take(4000).collect(),
                    iteration,
                };
                if let Err(e) = req.app.emit("syn-tool-call", &event) {
                    log::error!("Failed to emit syn-tool-call event: {e}");
                }
                tool_log.push(event);

                working.push(ChatMessage {
                    role: "tool".to_string(),
                    content,
                    tool_calls: None,
                    // Carried from the call it answers. Ollama drops it; an
                    // OpenAI-shaped endpoint refuses the message without it.
                    tool_call_id: tc.id.clone(),
                    images: None,
                });
            }
        };

        // Out of something. Ask once more without tools so the model has to
        // answer with words rather than reach for another call.
        //
        // Recorded in the transcript rather than only logged, because reaching
        // here means the investigation was cut short: the answer that follows
        // is built on however much the model got to, and it has no way to say
        // so. A user looking at a confidently wrong answer and an operator
        // reading a log are looking for the same line, and only one of them
        // has a log.
        match ended {
            LoopEnd::Cancelled => {
                // Nothing more is asked for. The user wanted it to stop, and
                // asking the model one more question is not stopping.
                run.note(run.spent.iterations, "Stopped by the user.");
                run.finish(RunState::Cancelled);
                crate::syn::run::save_run_best_effort(req.vault_path, run);
                stream.done();
                return Ok(assemble(
                    req.message_id,
                    req.model,
                    ChatReply::default(),
                    started,
                    tool_log,
                ));
            }
            LoopEnd::Ceiling(which) => {
                let message = ceiling_message(which, run);
                log::warn!("[Syn] {message}");
                run.note(run.spent.iterations, &message);
                run.finish(RunState::BudgetExhausted);
                crate::syn::run::save_run_best_effort(req.vault_path, run);
            }
            LoopEnd::DeadEnd => {}
        }

        let mut final_msg = self
            .answer_without_tools(run, req, stop, started, &working)
            .await?;

        if !tool_log.is_empty() {
            final_msg.tool_calls_log = Some(tool_log);
        }
        Ok(final_msg)
    }

    /// One completion with no tools offered, streamed.
    ///
    /// The way out of a run that has used up its budget: the model can no
    /// longer reach for anything, so it has to say what it knows.
    async fn answer_without_tools<R: tauri::Runtime>(
        &self,
        run: &mut Run,
        req: &DriveRequest<'_, R>,
        stop: &Arc<AtomicBool>,
        started: std::time::Instant,
        working: &[ChatMessage],
    ) -> AppResult<SynMessage> {
        let watchers = Watchers {
            app: req.app,
            conversation_id: run.conversation_id.clone(),
            message_id: req.message_id.to_string(),
        };
        let sink = |text: &str, done: bool| watchers.send(text, done);
        let stream = TokenStream::new(&sink);
        let emit_token = |token: &str| stream.push(token);
        let stop_check = {
            let flag = Arc::clone(stop);
            move || flag.load(Ordering::SeqCst)
        };

        let turn_started = std::time::Instant::now();
        let result = self
            .provider
            .chat_streaming(
                ChatRequest {
                    model: req.model,
                    messages: working,
                    temperature: req.temperature,
                    num_ctx: req.num_ctx,
                    tools: None,
                },
                &StreamSink {
                    on_token: &emit_token,
                    stop_requested: &stop_check,
                },
            )
            .await;

        stream.done();

        if stop_check() {
            run.note(run.spent.iterations, "Stopped by the user.");
            run.finish(RunState::Cancelled);
        }

        let reply = result?;
        run.record_assistant(
            run.spent.iterations,
            &reply.content,
            reply.tokens,
            turn_started.elapsed().as_millis() as u64,
        );
        crate::syn::run::save_run_best_effort(req.vault_path, run);

        Ok(assemble(req.message_id, req.model, reply, started, Vec::new()))
    }

}

/// How much text may pile up before it is sent on.
///
/// Roughly a short line. Small enough that the answer still appears to type
/// itself; large enough that a run of one-token deltas does not become a run of
/// events.
const FLUSH_AFTER_CHARS: usize = 48;

/// How long a partial buffer may wait.
///
/// Fifty milliseconds is twenty updates a second, which reads as continuous
/// and is four times fewer than a fast model produces on its own.
const FLUSH_AFTER: std::time::Duration = std::time::Duration::from_millis(50);

/// Whether the buffer should go now.
///
/// Separated from the sending so it can be tested without a Tauri app, which
/// is the only interesting part: everything else here is a string being
/// appended to.
fn should_flush(buffered: usize, since_last: std::time::Duration) -> bool {
    buffered >= FLUSH_AFTER_CHARS || since_last >= FLUSH_AFTER
}

/// Gathers tokens into batches on their way to whoever is watching.
///
/// # Why this is not one event per token
///
/// The provider calls back once per delta, and a delta from an OpenAI-shaped
/// endpoint is one model token — two to four characters. A 3,800 character
/// answer is therefore twelve to fifteen hundred Tauri events, each of which
/// crosses the IPC boundary, is parsed, appends to a `ref`, invalidates a
/// computed, re-renders the bubble and schedules a `nextTick`.
///
/// Measured what could be measured first, because guessing is how the last
/// three fixes in this session went wrong: turning the finished answer into
/// sanitised HTML costs 2.29ms, and at the 100ms debounce that already exists
/// it is about 2% of one core across a sixteen second stream. So the markdown
/// is *not* the cost, and the remaining suspects — Vue patching, `v-html`
/// replacing a large subtree, layout, the smooth-scroll animation — cannot be
/// measured from a test without a browser.
///
/// Batching is the honest response to that. It does not claim to know which of
/// those dominates; it makes the whole chain run four times less often, which
/// helps whichever one it is. The observed symptom was a WebView at 62% CPU
/// with the message half-drawn while the answer had already been complete on
/// disk for sixteen seconds.
struct TokenStream<'a> {
    /// Where a batch goes. `(text, done)`.
    emit: &'a (dyn Fn(&str, bool) + Send + Sync),
    pending: std::sync::Mutex<String>,
    last: std::sync::Mutex<std::time::Instant>,
}

impl<'a> TokenStream<'a> {
    fn new(emit: &'a (dyn Fn(&str, bool) + Send + Sync)) -> Self {
        Self {
            emit,
            pending: std::sync::Mutex::new(String::new()),
            last: std::sync::Mutex::new(std::time::Instant::now()),
        }
    }

    fn push(&self, token: &str) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.push_str(token);

        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if !should_flush(pending.chars().count(), last.elapsed()) {
            return;
        }
        let batch = std::mem::take(&mut *pending);
        *last = std::time::Instant::now();
        drop(pending);
        drop(last);
        (self.emit)(&batch, false);
    }

    /// Send whatever is left. Called before `done`, so the last few characters
    /// of an answer are never held back waiting for a token that never comes.
    fn flush(&self) {
        let batch = {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *pending)
        };
        if !batch.is_empty() {
            (self.emit)(&batch, false);
        }
    }

    /// Flush, then say the answer is finished.
    ///
    /// The frontend leaves a message in its streaming state until it sees
    /// `done`. It has to arrive on every path out — stopped, finished, or
    /// failed — or the UI keeps a spinner forever.
    fn done(&self) {
        self.flush();
        (self.emit)("", true);
    }
}

/// Somewhere to send streamed tokens, or nowhere.
///
/// Deliberately owns its strings and borrows only the app handle. Built from a
/// `Run` but not holding one: the loop writes to the run on every step, and a
/// closure borrowing it would make that impossible.
///
/// A run with no conversation has nobody watching, and emitting into an event
/// nothing listens for is how a background run would spend its time — so that
/// case sends nothing at all.
struct Watchers<'a, R: tauri::Runtime> {
    app: &'a tauri::AppHandle<R>,
    conversation_id: Option<String>,
    message_id: String,
}

impl<R: tauri::Runtime> Watchers<'_, R> {
    fn send(&self, token: &str, done: bool) {
        let Some(conversation_id) = &self.conversation_id else {
            return;
        };
        let event = SynStreamToken {
            conversation_id: conversation_id.clone(),
            message_id: self.message_id.clone(),
            token: token.to_string(),
            done,
        };
        if let Err(e) = self.app.emit("syn-stream-token", &event) {
            log::error!("Failed to emit stream token event: {e}");
        }
    }

}

/// What to tell a person when a run stopped short.
fn ceiling_message(which: &'static str, run: &Run) -> String {
    match which {
        "iterations" => format!(
            "Reached the ceiling of {} rounds after {} tool call(s) — answering from an \
             unfinished investigation. Raise `max_tool_iterations` in Syn settings if this \
             keeps happening.",
            run.budget.iterations.unwrap_or(0),
            run.spent.tool_calls
        ),
        "tool_calls" => format!(
            "Reached the ceiling of {} tool calls — answering from an unfinished \
             investigation.",
            run.budget.tool_calls.unwrap_or(0)
        ),
        "wall_ms" => format!(
            "Ran for {} seconds, which is this run's limit — answering from an unfinished \
             investigation.",
            run.spent.wall_ms / 1000
        ),
        "tokens" => "Reached this run's token ceiling — answering from an unfinished \
                     investigation."
            .to_string(),
        other => format!("Stopped at the {other} limit — answering from an unfinished investigation."),
    }
}

/// Build the assistant message the conversation stores.
///
/// Prefers the provider's own timing when it reports one — Ollama measures
/// generation, which excludes time spent waiting on a queue — and falls back to
/// the wall clock, which is all a provider that says nothing leaves to go on.
fn assemble(
    message_id: &str,
    model: &str,
    reply: ChatReply,
    started: std::time::Instant,
    tool_log: Vec<SynToolCallEvent>,
) -> SynMessage {
    SynMessage {
        id: message_id.to_string(),
        role: "assistant".to_string(),
        content: reply.content,
        model: Some(model.to_string()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        tokens: reply.tokens.filter(|n| *n > 0),
        duration_ms: Some(
            reply
                .duration_ms
                .unwrap_or_else(|| started.elapsed().as_millis() as u64),
        ),
        sources: None,
        tool_calls_log: Some(tool_log).filter(|l| !l.is_empty()),
        images: None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// How many events a streamed answer becomes.
    ///
    /// The number that mattered: a 3,797 character answer arrived as one Tauri
    /// event per model token, and each event crosses the IPC boundary, appends
    /// to a `ref`, invalidates a computed, re-renders the bubble and schedules
    /// a `nextTick`. The WebView was still at 62% CPU with the message
    /// half-drawn sixteen seconds after the answer was complete on disk.
    ///
    /// Measured first, and the obvious suspect was cleared: turning the
    /// finished answer into sanitised HTML costs 2.29ms, which at the 100ms
    /// debounce already in place is about 2% of one core. What is left —
    /// Vue patching, `v-html` replacing a subtree, layout, the smooth scroll —
    /// cannot be measured without a browser. Batching does not pick between
    /// them; it makes the whole chain run far less often.
    #[test]
    fn a_streamed_answer_becomes_far_fewer_events_than_it_has_tokens() {
        let batches: std::sync::Mutex<Vec<(String, bool)>> = std::sync::Mutex::new(Vec::new());
        let sink = |text: &str, done: bool| {
            batches
                .lock()
                .expect("lock")
                .push((text.to_string(), done));
        };
        let stream = TokenStream::new(&sink);

        // A model token is two to four characters; this is the shape of a real
        // one, at the length that caused the problem.
        let tokens: Vec<String> = (0..1_200).map(|i| format!("{} ", i % 90)).collect();
        for token in &tokens {
            stream.push(token);
        }
        stream.done();

        let sent = batches.lock().expect("lock");
        let events = sent.len();
        let text: String = sent.iter().filter(|(_, done)| !done).map(|(t, _)| t.as_str()).collect();

        // Nothing is lost or reordered: the batches concatenate to exactly what
        // went in. This is the part that would be unforgivable to get wrong.
        assert_eq!(text, tokens.concat(), "the answer must survive batching intact");

        // Exactly one `done`, and it is last — the frontend leaves a message in
        // its streaming state until it sees one.
        assert_eq!(sent.iter().filter(|(_, done)| *done).count(), 1);
        assert!(sent.last().expect("something was sent").1, "done comes last");

        assert!(
            events < tokens.len() / 4,
            "batching should cut the event count by at least four; {} tokens became {events} events",
            tokens.len()
        );
        eprintln!("\n── {} tokens → {events} events ──\n", tokens.len());
    }

    /// The tail of an answer is never held back waiting for a token that is
    /// not coming.
    #[test]
    fn a_short_answer_still_arrives_whole() {
        let batches: std::sync::Mutex<Vec<(String, bool)>> = std::sync::Mutex::new(Vec::new());
        let sink = |text: &str, done: bool| {
            batches.lock().expect("lock").push((text.to_string(), done));
        };
        let stream = TokenStream::new(&sink);

        stream.push("Vâng");
        stream.push(", ");
        stream.push("xong rồi.");
        stream.done();

        let sent = batches.lock().expect("lock");
        let text: String = sent.iter().filter(|(_, d)| !d).map(|(t, _)| t.as_str()).collect();
        assert_eq!(text, "Vâng, xong rồi.");
        assert!(sent.last().expect("sent something").1);
    }

    /// The two reasons to send: enough has piled up, or enough time has passed.
    #[test]
    fn a_batch_goes_when_it_is_full_or_when_it_is_old() {
        assert!(!should_flush(1, std::time::Duration::ZERO));
        assert!(should_flush(FLUSH_AFTER_CHARS, std::time::Duration::ZERO));
        assert!(should_flush(1, FLUSH_AFTER));
        assert!(!should_flush(
            FLUSH_AFTER_CHARS - 1,
            FLUSH_AFTER - std::time::Duration::from_millis(1)
        ));
    }

    fn msg(role: &str, content: &str) -> SynMessage {
        SynMessage {
            id: String::new(),
            role: role.to_string(),
            content: content.to_string(),
            model: None,
            timestamp: String::new(),
            tokens: None,
            duration_ms: None,
            sources: None,
            tool_calls_log: None,
            images: None,
        }
    }

    #[test]
    fn a_short_history_is_passed_through_whole() {
        let history = vec![msg("user", "a"), msg("assistant", "b")];
        let out = build_pruned_history(&history, 50);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].content, "a");
    }

    /// The system message carries the personality and the RAG context. Losing
    /// it because it is the oldest would change who the assistant is, silently,
    /// partway through a long conversation.
    #[test]
    fn the_system_message_survives_pruning() {
        let mut history = vec![msg("system", "you are Syn")];
        for i in 0..10 {
            history.push(msg("user", &format!("q{}", i)));
        }

        let out = build_pruned_history(&history, 4);
        assert_eq!(out[0].role, "system");
        assert_eq!(out[0].content, "you are Syn");
        // The most recent turn is always the last one kept.
        assert_eq!(out.last().expect("kept something").content, "q9");
    }

    #[test]
    fn pruning_without_a_system_message_keeps_the_tail() {
        let history: Vec<SynMessage> = (0..10).map(|i| msg("user", &format!("q{}", i))).collect();
        let out = build_pruned_history(&history, 3);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].content, "q7");
        assert_eq!(out[2].content, "q9");
    }

    /// Images ride along with the message they belong to; a vision model that
    /// receives the text without the picture answers confidently about nothing.
    #[test]
    fn images_travel_with_their_message() {
        let mut m = msg("user", "what is this");
        m.images = Some(vec!["iVBORw0KGgo".into()]);

        let out = build_pruned_history(&[m], 50);
        assert_eq!(out[0].images.as_deref(), Some(&["iVBORw0KGgo".to_string()][..]));
    }
}

/// Gate 1: can the assistant actually drive?
///
/// The roadmap put a gate after the provider work with one criterion — Syn
/// finishes a job of several steps against a real vault without being walked
/// through it — and a gate is worthless if it is judged by impression. This
/// runs the job.
///
/// `#[ignore]`d, and it has to be. It spends money on the user's own API key,
/// it needs a network, and it is not deterministic: the same prompt is allowed
/// to reach the same end by a different route. CI must never run it.
///
/// ```bash
/// cargo test --lib gate_one -- --ignored --nocapture
/// ```
///
/// It writes into a temporary vault, never the user's. The provider and model
/// are read from the real settings so that what is measured is the setup the
/// user actually has.
#[cfg(test)]
mod gate_one {
    use super::*;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use crate::models::syn::SynProvider;

    /// A vault with a shape the job needs: a project, tasks under it, some
    /// overdue and some not, and noise that must not be swept up.
    fn seed(vault: &std::path::Path) -> DbBridge {
        let db = DbBridge::new_in_memory_full().expect("schema");

        let write = |db: &DbBridge, rel: &str, node_type: &str, title: &str, props: serde_json::Value| {
            let path = vault.join(rel);
            std::fs::create_dir_all(path.parent().expect("has a parent")).expect("mkdir");
            std::fs::write(
                &path,
                crate::commands::nodes::markdown_with_frontmatter(title, node_type, &props, ""),
            )
            .expect("write");
            db.upsert_node(&NodeMetadata {
                id: rel.to_string(),
                node_type: node_type.to_string(),
                title: title.to_string(),
                content: String::new(),
                properties: props,
                created_at: "2026-08-01T00:00:00Z".to_string(),
                updated_at: "2026-08-01T00:00:00Z".to_string(),
                timestamp: 0,
                blocks: None,
            })
            .expect("upsert");
        };

        write(&db, "Projects/Apollo.md", "project", "Apollo", serde_json::json!({}));

        // Overdue, on Apollo — the three the job is about.
        for (file, title, due) in [
            ("Tasks/wiring.md", "Rewire the launch page", "2026-08-10"),
            ("Tasks/copy.md", "Rewrite the pricing copy", "2026-08-14"),
            ("Tasks/legal.md", "Legal review of the terms", "2026-08-20"),
        ] {
            write(
                &db,
                file,
                "task",
                title,
                serde_json::json!({
                    "status": "todo",
                    "due_date": due,
                    "project": "Apollo",
                    "tags": ["apollo"],
                }),
            );
        }

        // Noise, each a different way of not qualifying.
        write(&db, "Tasks/done.md", "task", "Ship the beta", serde_json::json!({
            "status": "done", "due_date": "2026-08-05", "project": "Apollo", "tags": ["apollo"],
        }));
        write(&db, "Tasks/future.md", "task", "Plan the launch party", serde_json::json!({
            "status": "todo", "due_date": "2026-12-01", "project": "Apollo", "tags": ["apollo"],
        }));
        write(&db, "Tasks/other.md", "task", "Renew the domain", serde_json::json!({
            "status": "todo", "due_date": "2026-08-02", "project": "Zephyr", "tags": ["zephyr"],
        }));

        db
    }

    #[tokio::test]
    #[ignore = "spends real API credit and needs a network; run by hand"]
    async fn syn_finishes_a_job_of_several_steps_without_being_walked_through_it() {
        let vault_dir = tempfile::tempdir().expect("temp vault");
        let vault = vault_dir.path();
        let db = seed(vault);

        // The user's own configuration, so the gate measures their setup.
        let settings = crate::syn::settings::load_settings(
            &std::env::var("SYN_GATE_VAULT").unwrap_or_else(|_| {
                format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default())
            }),
        )
        .expect("the real Syn settings");

        let provider: Box<dyn ChatProvider> = match settings.provider {
            SynProvider::Ollama => Box::new(crate::syn::provider::ollama::OllamaProvider::new(
                &settings.ollama_url,
            )),
            SynProvider::OpenAiCompat => Box::new(
                crate::syn::provider::openai::OpenAiCompatProvider::new(
                    &settings.openai_base_url,
                    crate::secrets::SecretManager::get_syn_api_key(None, "openai_compat"),
                    settings.openai_reasoning_effort.clone(),
                ),
            ),
        };

        let model = settings
            .default_model
            .clone()
            .expect("a default model must be configured to run the gate");

        eprintln!("\n── gate 1 ──────────────────────────────────────");
        eprintln!("provider   {:?}", settings.provider);
        eprintln!("model      {model}");
        eprintln!("iterations {}", settings.max_tool_iterations);
        eprintln!("vault      {}", vault.display());

        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app")
            .handle()
            .clone();

        // One instruction, several steps, and nothing naming a tool. Finding
        // the tasks, judging which are overdue against today, reading the
        // project, writing a note and linking it are all left to the model.
        let job = "Find every task in the Apollo project that is past its due date and \
                   still not done. Then create one note titled 'Apollo — overdue' that \
                   lists them with their due dates, and tag it #apollo. \
                   Today is 2026-08-29.";

        let history = vec![
            SynMessage {
                id: "sys".into(),
                role: "system".into(),
                content: crate::syn::prompt::PromptPlan::for_chat(crate::syn::prompt::ChatPrompt { context: "", personality: "auto", custom: None, memory: None, budget_chars: crate::syn::prompt::DEFAULT_BUDGET_CHARS })
                .render(),
                model: None,
                timestamp: String::new(),
                tokens: None,
                duration_ms: None,
                sources: None,
                tool_calls_log: None,
                images: None,
            },
            SynMessage {
                id: "u1".into(),
                role: "user".into(),
                content: job.into(),
                model: None,
                timestamp: String::new(),
                tokens: None,
                duration_ms: None,
                sources: None,
                tool_calls_log: None,
                images: None,
            },
        ];

        let engine = SynEngine::new(provider);
        let db_state = std::sync::Mutex::new(db);
        let started = std::time::Instant::now();

        let registry = crate::syn::registry::Registry::for_chat();
        let mut run = Run::new(job, None, crate::syn::run::Budget::from_settings(&settings));

        let reply = engine
            .drive(
                &mut run,
                DriveRequest {
                    app: &app,
                    message_id: "gate-1-msg",
                    history: &history,
                    model: &model,
                    temperature: Some(settings.temperature),
                    registry: &registry,
                    db: &db_state,
                    vault_path: vault.to_str().expect("utf8 vault path"),
                    num_ctx: settings.num_ctx,
                    max_history: settings.max_history_messages,
                },
            )
            .await
            .expect("the job runs to an answer");

        // The transcript the run left behind, which is the new half of what
        // this gate is checking: the work is readable after the fact.
        eprintln!(
            "\n── the run ─────────────────────────────────────\nstate {:?}  steps {}  \
             tool calls {}  rounds {}",
            run.state,
            run.steps.len(),
            run.spent.tool_calls,
            run.spent.iterations
        );

        let calls = reply.tool_calls_log.clone().unwrap_or_default();
        eprintln!("\n── what it did ─────────────────────────────────");
        for (i, c) in calls.iter().enumerate() {
            eprintln!(
                "{:>2}. {:<20} {}",
                i + 1,
                c.tool_name,
                serde_json::to_string(&c.tool_args).unwrap_or_default()
            );
            eprintln!("    → {}", c.result_preview.chars().take(220).collect::<String>());
        }
        eprintln!("\n── what it said ────────────────────────────────\n{}", reply.content);

        // What actually landed in the vault, which is the only thing that counts.
        let mut written = Vec::new();
        for entry in walkdir::WalkDir::new(vault).into_iter().flatten() {
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(vault)
                    .expect("under the vault")
                    .to_string_lossy()
                    .to_string();
                // The run's own transcript lives under `Syn/`. It is this
                // app's bookkeeping, not something the assistant wrote, and
                // listing it here would put a few hundred lines of JSON in
                // the middle of the output a person reads to judge the gate.
                if rel.starts_with("Syn/") {
                    continue;
                }
                if !matches!(
                    rel.as_str(),
                    "Projects/Apollo.md"
                        | "Tasks/wiring.md"
                        | "Tasks/copy.md"
                        | "Tasks/legal.md"
                        | "Tasks/done.md"
                        | "Tasks/future.md"
                        | "Tasks/other.md"
                ) {
                    written.push((rel, std::fs::read_to_string(entry.path()).unwrap_or_default()));
                }
            }
        }

        eprintln!("\n── what it wrote ───────────────────────────────");
        for (rel, body) in &written {
            eprintln!("── {rel}\n{body}");
        }
        eprintln!("── {} tool call(s), {:?} ──\n", calls.len(), started.elapsed());

        // The gate itself. Deliberately loose about *how* — a different route
        // to the same result is a pass, and pinning the route would test the
        // model's habits rather than whether it can drive.
        assert!(!calls.is_empty(), "it never reached for a tool");

        let note = written
            .iter()
            .find(|(_, body)| body.contains("type: note"))
            .map(|(_, body)| body.clone())
            .unwrap_or_default();
        assert!(!note.is_empty(), "no note was written; the job did not finish");

        for overdue in ["Rewire the launch page", "Rewrite the pricing copy", "Legal review"] {
            assert!(note.contains(overdue), "the note is missing `{overdue}`:\n{note}");
        }
        for excluded in ["Ship the beta", "Plan the launch party", "Renew the domain"] {
            assert!(
                !note.contains(excluded),
                "`{excluded}` is not overdue on Apollo and should not be listed:\n{note}"
            );
        }
    }
}

/// What the loop does, against a model that says exactly what it is told to.
///
/// None of this was testable before. The loop took eleven arguments, held its
/// state in local variables, and talked to a real HTTP endpoint — so the only
/// way to find out what it did with a tool result was to watch a conversation
/// and read a log. A run is a value, and a provider is a trait, which together
/// make the interesting cases ordinary tests: the transcript records what
/// happened, the ceilings fire, and a cancelled run is readable as far as it
/// got.
#[cfg(test)]
mod driving {
    use super::*;
    use crate::db::DbBridge;
    use crate::models::syn::{
        ModelInfo, ProviderStatus, SynProvider, ToolCall, ToolCallFunction,
    };
    use crate::syn::provider::ChatProvider;
    use crate::syn::registry::Registry;
    use crate::syn::run::{Budget, RunState, StepKind};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// A model that answers from a script.
    struct Scripted {
        replies: Mutex<VecDeque<ChatReply>>,
        /// Builds the reply to keep giving once the script runs out. A model
        /// that has decided to search forever is the case the ceilings exist
        /// for. A factory rather than a value because `ChatReply` is a plain
        /// wire type and does not need to be `Clone` for a test's convenience.
        #[allow(clippy::type_complexity)]
        looping: Mutex<Option<Box<dyn Fn() -> ChatReply + Send + Sync>>>,
        /// How many steps the transcript on disk held at the start of each
        /// call, so a test can prove it was written *during* the run rather
        /// than at the end of it.
        transcript_seen: Mutex<Vec<usize>>,
        /// Run each call before answering, for the cancellation test.
        before_reply: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
        vault: String,
        run_id: String,
    }

    impl Scripted {
        fn new(vault: &str, run_id: &str, replies: Vec<ChatReply>) -> Self {
            Self {
                replies: Mutex::new(replies.into()),
                looping: Mutex::new(None),
                transcript_seen: Mutex::new(Vec::new()),
                before_reply: Mutex::new(None),
                vault: vault.to_string(),
                run_id: run_id.to_string(),
            }
        }

        fn looping_on(
            vault: &str,
            run_id: &str,
            reply: impl Fn() -> ChatReply + Send + Sync + 'static,
        ) -> Self {
            let s = Self::new(vault, run_id, Vec::new());
            *s.looping.lock().expect("lock") = Some(Box::new(reply));
            s
        }

        /// `tools_offered` is what a real model sees, and it changes what a
        /// real model does: asked without tools, it has to answer in words.
        /// The mock behaves the same way, or the ceiling path — whose whole
        /// purpose is to force exactly that — cannot be tested.
        fn next(&self, tools_offered: bool) -> ChatReply {
            // Read the transcript before answering. The run is mid-flight, so
            // whatever is on disk now was written by the loop as it went.
            let steps = crate::syn::run::get_run(&self.vault, &self.run_id)
                .map(|r| r.steps.len())
                .unwrap_or(0);
            self.transcript_seen.lock().expect("lock").push(steps);

            if let Some(hook) = self.before_reply.lock().expect("lock").as_ref() {
                hook();
            }

            if !tools_offered {
                return text("that is everything I could find");
            }

            let queued = self.replies.lock().expect("lock").pop_front();
            match queued {
                Some(reply) => reply,
                None => match self.looping.lock().expect("lock").as_ref() {
                    Some(build) => build(),
                    None => text("that is everything"),
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl ChatProvider for Scripted {
        fn id(&self) -> SynProvider {
            SynProvider::Ollama
        }
        async fn check_status(&self) -> AppResult<ProviderStatus> {
            unreachable!("the loop never asks")
        }
        async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
            unreachable!("the loop never asks")
        }
        async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply> {
            Ok(self.next(req.tools.is_some()))
        }
        async fn chat_streaming(
            &self,
            req: ChatRequest<'_>,
            sink: &StreamSink<'_>,
        ) -> AppResult<ChatReply> {
            let reply = self.next(req.tools.is_some());
            (sink.on_token)(&reply.content);
            Ok(reply)
        }
    }

    fn text(content: &str) -> ChatReply {
        ChatReply {
            content: content.to_string(),
            tool_calls: Vec::new(),
            tokens: Some(7),
            duration_ms: None,
        }
    }

    fn calls(tool: &str, args: serde_json::Value) -> ChatReply {
        ChatReply {
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: Some("call-1".into()),
                function: ToolCallFunction {
                    name: tool.to_string(),
                    arguments: args,
                },
            }],
            tokens: None,
            duration_ms: None,
        }
    }

    fn app() -> tauri::AppHandle<tauri::test::MockRuntime> {
        tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app")
            .handle()
            .clone()
    }

    fn history(ask: &str) -> Vec<SynMessage> {
        vec![SynMessage {
            id: "u1".into(),
            role: "user".into(),
            content: ask.into(),
            model: None,
            timestamp: String::new(),
            tokens: None,
            duration_ms: None,
            sources: None,
            tool_calls_log: None,
            images: None,
        }]
    }

    fn budget(iterations: u8) -> Budget {
        Budget {
            iterations: Some(iterations),
            tool_calls: Some(50),
            tokens: None,
            wall_ms: Some(60_000),
        }
    }

    /// Everything the loop did, in order, with what it would take to undo each
    /// step. This is the record that did not exist before.
    #[tokio::test]
    async fn a_transcript_records_what_happened_and_how_to_undo_it() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8").to_string();
        let db: crate::db::DbState = Mutex::new(DbBridge::new_in_memory_full().expect("schema"));

        let mut run = Run::new("what tasks are open", Some("conv-1".into()), budget(12));
        let provider = Scripted::new(
            &vault,
            &run.id,
            vec![
                calls("query_nodes", serde_json::json!({ "query": "type:task" })),
                text("You have nothing open."),
            ],
        );

        let app = app();
        let registry = Registry::for_chat();
        let engine = SynEngine::new(Box::new(provider));

        let reply = engine
            .drive(
                &mut run,
                DriveRequest {
                    app: &app,
                    message_id: "m1",
                    history: &history("what tasks are open"),
                    model: "scripted",
                    temperature: None,
                    registry: &registry,
                    db: &db,
                    vault_path: &vault,
                    num_ctx: 8192,
                    max_history: 50,
                },
            )
            .await
            .expect("drives to an answer");

        assert_eq!(reply.content, "You have nothing open.");
        assert_eq!(run.state, RunState::Done);
        assert_eq!(run.spent.tool_calls, 1);

        let kinds: Vec<StepKind> = run.steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, vec![StepKind::ToolCall, StepKind::Assistant]);

        let tool_step = &run.steps[0];
        assert_eq!(tool_step.tool.as_deref(), Some("query_nodes"));
        assert_eq!(tool_step.ok, Some(true));
        assert_eq!(
            tool_step.reversal,
            Some(crate::syn::registry::Reversal::Nothing),
            "a read changes nothing, and the transcript should say so"
        );

        // And it is on disk, not only in memory.
        let stored = crate::syn::run::get_run(&vault, &run.id).expect("stored");
        assert_eq!(stored.steps.len(), 2);
        assert_eq!(stored.goal, "what tasks are open");
        assert_eq!(stored.model.as_deref(), Some("scripted"));
    }

    /// The gate for this phase: the transcript is written *while* the run is
    /// going, so a run that never reaches the end is still readable.
    ///
    /// Proven from inside — the model reads the transcript file before each of
    /// its own answers, so the counts it saw are what was on disk mid-flight.
    /// A transcript written on completion would show zero every time.
    #[tokio::test]
    async fn the_transcript_is_written_while_the_run_is_still_going() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8").to_string();
        let db: crate::db::DbState = Mutex::new(DbBridge::new_in_memory_full().expect("schema"));

        let mut run = Run::new("look twice", Some("conv-1".into()), budget(12));
        let provider = std::sync::Arc::new(Scripted::new(
            &vault,
            &run.id,
            vec![
                calls("query_nodes", serde_json::json!({ "query": "type:task" })),
                calls("list_schemas", serde_json::json!({})),
                text("done"),
            ],
        ));

        let app = app();
        let registry = Registry::for_chat();
        let engine = SynEngine::new(Box::new(SharedProvider(provider.clone())));

        engine
            .drive(
                &mut run,
                DriveRequest {
                    app: &app,
                    message_id: "m1",
                    history: &history("look twice"),
                    model: "scripted",
                    temperature: None,
                    registry: &registry,
                    db: &db,
                    vault_path: &vault,
                    num_ctx: 8192,
                    max_history: 50,
                },
            )
            .await
            .expect("drives");

        let seen = provider.transcript_seen.lock().expect("lock").clone();
        assert_eq!(seen.len(), 3, "the script was used three times");
        assert_eq!(seen[0], 0, "nothing had happened yet");
        assert_eq!(seen[1], 1, "the first tool call was already on disk");
        assert_eq!(seen[2], 2, "so was the second");
    }

    /// Running out of rounds used to be a `log::warn!` and nothing else, so the
    /// only person who could learn that an answer came from a cut-short
    /// investigation was somebody reading a log file.
    #[tokio::test]
    async fn a_run_that_runs_out_of_rounds_says_so_where_the_user_can_read_it() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8").to_string();
        let db: crate::db::DbState = Mutex::new(DbBridge::new_in_memory_full().expect("schema"));

        let mut run = Run::new("search forever", Some("conv-1".into()), budget(2));
        let provider = Scripted::looping_on(&vault, &run.id, || {
            calls("query_nodes", serde_json::json!({ "query": "type:task" }))
        });

        let app = app();
        let registry = Registry::for_chat();
        let engine = SynEngine::new(Box::new(provider));

        let reply = engine
            .drive(
                &mut run,
                DriveRequest {
                    app: &app,
                    message_id: "m1",
                    history: &history("search forever"),
                    model: "scripted",
                    temperature: None,
                    registry: &registry,
                    db: &db,
                    vault_path: &vault,
                    num_ctx: 8192,
                    max_history: 50,
                },
            )
            .await
            .expect("still answers");

        assert_eq!(run.state, RunState::BudgetExhausted);
        assert_eq!(run.spent.iterations, 2, "it stopped at the ceiling");

        let note = run
            .steps
            .iter()
            .find(|s| s.kind == StepKind::Note)
            .expect("the ceiling is recorded in the transcript");
        assert!(
            note.preview.contains("ceiling of 2 rounds"),
            "the note should name the ceiling that fired: {}",
            note.preview
        );

        // And the user still gets words rather than silence.
        assert!(!reply.content.is_empty());
    }

    /// A ceiling on tool calls is not the same ceiling as one on rounds: a
    /// single round asking for fifty tools is inside the round limit and is
    /// exactly the case this catches.
    #[tokio::test]
    async fn a_round_that_asks_for_too_many_tools_at_once_is_stopped() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8").to_string();
        let db: crate::db::DbState = Mutex::new(DbBridge::new_in_memory_full().expect("schema"));

        let mut run = Run::new(
            "one greedy round",
            Some("conv-1".into()),
            Budget {
                iterations: Some(12),
                tool_calls: Some(3),
                tokens: None,
                wall_ms: Some(60_000),
            },
        );

        let greedy = ChatReply {
            content: String::new(),
            tool_calls: (0..10)
                .map(|i| ToolCall {
                    id: Some(format!("call-{i}")),
                    function: ToolCallFunction {
                        name: "list_schemas".into(),
                        arguments: serde_json::json!({}),
                    },
                })
                .collect(),
            tokens: None,
            duration_ms: None,
        };

        let app = app();
        let registry = Registry::for_chat();
        let engine = SynEngine::new(Box::new(Scripted::new(&vault, &run.id, vec![greedy])));

        engine
            .drive(
                &mut run,
                DriveRequest {
                    app: &app,
                    message_id: "m1",
                    history: &history("one greedy round"),
                    model: "scripted",
                    temperature: None,
                    registry: &registry,
                    db: &db,
                    vault_path: &vault,
                    num_ctx: 8192,
                    max_history: 50,
                },
            )
            .await
            .expect("answers");

        assert_eq!(run.state, RunState::BudgetExhausted);
        assert_eq!(run.spent.tool_calls, 3, "it stopped at the tool ceiling");
        assert_eq!(run.spent.iterations, 1, "inside a single round");
    }

    /// Stop means stop, and what happened up to that point is kept.
    #[tokio::test]
    async fn a_cancelled_run_keeps_what_it_had_got_to() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8").to_string();
        let db: crate::db::DbState = Mutex::new(DbBridge::new_in_memory_full().expect("schema"));

        let mut run = Run::new("stop me", Some("conv-9".into()), budget(12));
        let run_id = run.id.clone();

        let provider = Scripted::new(
            &vault,
            &run.id,
            vec![
                calls("list_schemas", serde_json::json!({})),
                text("never reached"),
            ],
        );
        // Pressed while the second turn is in flight, which is where a real
        // stop lands: after some work has been done and before the rest is.
        // Pressing it before the first turn would be a fair test of the flag
        // and no test at all of whether the work already done is kept.
        let turns = std::sync::atomic::AtomicUsize::new(0);
        *provider.before_reply.lock().expect("lock") = Some(Box::new(move || {
            if turns.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                stop_run(&run_id);
            }
        }));

        let app = app();
        let registry = Registry::for_chat();
        let engine = SynEngine::new(Box::new(provider));

        engine
            .drive(
                &mut run,
                DriveRequest {
                    app: &app,
                    message_id: "m1",
                    history: &history("stop me"),
                    model: "scripted",
                    temperature: None,
                    registry: &registry,
                    db: &db,
                    vault_path: &vault,
                    num_ctx: 8192,
                    max_history: 50,
                },
            )
            .await
            .expect("returns what it had");

        assert_eq!(run.state, RunState::Cancelled);
        let stored = crate::syn::run::get_run(&vault, &run.id).expect("stored");
        assert!(
            stored.steps.iter().any(|s| s.tool.as_deref() == Some("list_schemas")),
            "the work done before the stop is still in the transcript"
        );
        assert!(!is_live(&run.id), "a finished run is no longer live");
    }

    /// A tool the model invented is reported back to it as an error rather than
    /// silently doing nothing, and the transcript marks the step as failed.
    #[tokio::test]
    async fn a_tool_that_does_not_exist_is_recorded_as_a_failure() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8").to_string();
        let db: crate::db::DbState = Mutex::new(DbBridge::new_in_memory_full().expect("schema"));

        let mut run = Run::new("invent a tool", Some("conv-1".into()), budget(12));
        let engine = SynEngine::new(Box::new(Scripted::new(
            &vault,
            &run.id,
            vec![
                calls("send_email", serde_json::json!({ "to": "nobody" })),
                text("I cannot do that."),
            ],
        )));

        let app = app();
        let registry = Registry::for_chat();
        engine
            .drive(
                &mut run,
                DriveRequest {
                    app: &app,
                    message_id: "m1",
                    history: &history("invent a tool"),
                    model: "scripted",
                    temperature: None,
                    registry: &registry,
                    db: &db,
                    vault_path: &vault,
                    num_ctx: 8192,
                    max_history: 50,
                },
            )
            .await
            .expect("recovers");

        let step = &run.steps[0];
        assert_eq!(step.tool.as_deref(), Some("send_email"));
        assert_eq!(step.ok, Some(false));
        assert!(step.preview.contains("Unknown tool"));
        assert_eq!(run.state, RunState::Done, "an invented tool is not a failed run");
    }

    /// Sharing one scripted provider between the engine and the test, so the
    /// test can read what the provider observed after the run.
    struct SharedProvider(std::sync::Arc<Scripted>);

    #[async_trait::async_trait]
    impl ChatProvider for SharedProvider {
        fn id(&self) -> SynProvider {
            self.0.id()
        }
        async fn check_status(&self) -> AppResult<ProviderStatus> {
            self.0.check_status().await
        }
        async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
            self.0.list_models().await
        }
        async fn chat(&self, req: ChatRequest<'_>) -> AppResult<ChatReply> {
            self.0.chat(req).await
        }
        async fn chat_streaming(
            &self,
            req: ChatRequest<'_>,
            sink: &StreamSink<'_>,
        ) -> AppResult<ChatReply> {
            self.0.chat_streaming(req, sink).await
        }
    }
}
