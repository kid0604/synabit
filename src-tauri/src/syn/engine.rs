//! The Syn engine — the part that is the same whoever answers.
//!
//! This file used to be an Ollama HTTP client with a tool-calling loop around
//! it. The client moved to `provider/`, and what is left is the loop: prune the
//! history to fit, ask for a completion, run whatever tools came back, ask
//! again, and stream the answer out as Tauri events. None of that depends on
//! who is on the other end, which is the whole reason for the split.
//!
//! Cancellation lives here rather than in a provider because it is a property
//! of a *conversation*, not of a connection: the user presses stop on a chat,
//! and the provider is handed a closure that reports whether that happened.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use tauri::Emitter;

use crate::error::{AppError, AppResult};
use crate::models::syn::{SynMessage, SynStreamToken, SynToolCallEvent, ToolDefinition};
use crate::syn::provider::{ChatMessage, ChatProvider, ChatRequest, ChatReply, StreamSink};

/// Per-conversation stop flags. Each active streaming conversation gets its own AtomicBool.
static STOP_FLAGS: std::sync::LazyLock<RwLock<HashMap<String, Arc<AtomicBool>>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get or create a stop flag for a conversation.
fn get_stop_flag(conversation_id: &str) -> Arc<AtomicBool> {
    {
        let flags = STOP_FLAGS.read().unwrap();
        if let Some(flag) = flags.get(conversation_id) {
            return Arc::clone(flag);
        }
    }
    let flag = Arc::new(AtomicBool::new(false));
    let mut flags = STOP_FLAGS.write().unwrap();
    flags.insert(conversation_id.to_string(), Arc::clone(&flag));
    flag
}

/// Remove the stop flag for a completed conversation.
fn remove_stop_flag(conversation_id: &str) {
    let mut flags = STOP_FLAGS.write().unwrap();
    flags.remove(conversation_id);
}

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

/// The Syn engine — a conversation loop over whichever provider it was given.
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

    // ───────────────────────────────────────────────────────────
    //  Streaming Chat Completion
    // ───────────────────────────────────────────────────────────

    /// Send a message and stream the response token-by-token.
    ///
    /// - Builds the messages array from the conversation history.
    /// - Emits a `syn-stream-token` Tauri event for each token.
    /// - Checks the conversation's stop flag between chunks.
    /// - Returns the complete assistant `SynMessage` when done.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_message<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        conversation_id: &str,
        message_id: &str,
        history: &[SynMessage],
        model: &str,
        temperature: Option<f64>,
        num_ctx: u32,
        max_history: usize,
    ) -> AppResult<SynMessage> {
        let stop_flag = get_stop_flag(conversation_id);
        stop_flag.store(false, Ordering::SeqCst);

        let messages = build_pruned_history(history, max_history);
        let start_time = std::time::Instant::now();

        let emit_token = |token: &str| {
            let event = SynStreamToken {
                conversation_id: conversation_id.to_string(),
                message_id: message_id.to_string(),
                token: token.to_string(),
                done: false,
            };
            if let Err(e) = app.emit("syn-stream-token", &event) {
                log::error!("Failed to emit stream token event: {}", e);
            }
        };

        let stop_check = {
            let flag = Arc::clone(&stop_flag);
            move || flag.load(Ordering::SeqCst)
        };

        let sink = StreamSink {
            on_token: &emit_token,
            stop_requested: &stop_check,
        };

        let result = self
            .provider
            .chat_streaming(
                ChatRequest {
                    model,
                    messages: &messages,
                    temperature,
                    num_ctx,
                    tools: None,
                },
                &sink,
            )
            .await;

        // The frontend leaves a message in its streaming state until it sees
        // `done`. It has to arrive on every path out of here — stopped,
        // finished, or failed — or the UI keeps a spinner forever.
        let done_event = SynStreamToken {
            conversation_id: conversation_id.to_string(),
            message_id: message_id.to_string(),
            token: String::new(),
            done: true,
        };
        if let Err(e) = app.emit("syn-stream-token", &done_event) {
            log::error!("Failed to emit stream done event: {}", e);
        }

        remove_stop_flag(conversation_id);

        if stop_flag.load(Ordering::SeqCst) {
            log::info!(
                "Generation stopped by user for conversation {}",
                conversation_id
            );
        }

        let reply = result?;
        Ok(self.assemble(message_id, model, reply, start_time, None))
    }

    /// Build the assistant message the conversation stores.
    ///
    /// Prefers the provider's own timing when it reports one — Ollama measures
    /// generation, which excludes the time spent waiting on a queue — and
    /// falls back to the wall clock, which is all a provider that says nothing
    /// leaves to go on.
    fn assemble(
        &self,
        message_id: &str,
        model: &str,
        reply: ChatReply,
        start_time: std::time::Instant,
        tool_call_log: Option<Vec<SynToolCallEvent>>,
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
                    .unwrap_or_else(|| start_time.elapsed().as_millis() as u64),
            ),
            sources: None,
            tool_calls_log: tool_call_log.filter(|l| !l.is_empty()),
            images: None,
        }
    }

    /// Stop generation for a specific conversation (or all if id is None).
    pub fn stop_generation(conversation_id: Option<&str>) {
        let Ok(flags) = STOP_FLAGS.read() else {
            return;
        };
        match conversation_id {
            Some(id) => {
                if let Some(flag) = flags.get(id) {
                    flag.store(true, Ordering::SeqCst);
                    log::info!("Stop generation flag set for conversation {}", id);
                }
            }
            None => {
                for (id, flag) in flags.iter() {
                    flag.store(true, Ordering::SeqCst);
                    log::info!("Stop generation flag set for conversation {}", id);
                }
            }
        }
    }

    // ───────────────────────────────────────────────────────────
    //  Function Calling / Tool Use
    // ───────────────────────────────────────────────────────────

    /// Send a message with tool calling support.
    ///
    /// The loop:
    /// 1. Ask for a completion, non-streaming, with the tool definitions.
    /// 2. If the model asked for tools, run them against the DB and append the
    ///    results.
    /// 3. Repeat, up to `max_iterations`.
    /// 4. When the model answers with text instead, that is the answer.
    ///
    /// `db_state` is the `Mutex<DbBridge>` — locked only while a tool runs
    /// (fast, <10ms each), never across an HTTP call.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_message_with_tools<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        conversation_id: &str,
        message_id: &str,
        history: &[SynMessage],
        model: &str,
        temperature: Option<f64>,
        tools: &[ToolDefinition],
        db_state: &std::sync::Mutex<crate::db::DbBridge>,
        vault_path: &str,
        max_iterations: u8,
        num_ctx: u32,
        max_history: usize,
    ) -> AppResult<SynMessage> {
        let mut working_messages = build_pruned_history(history, max_history);
        let mut tool_call_log: Vec<SynToolCallEvent> = Vec::new();
        let start_time = std::time::Instant::now();

        let stop_flag = get_stop_flag(conversation_id);
        stop_flag.store(false, Ordering::SeqCst);

        let emit_token = |token: &str| {
            let event = SynStreamToken {
                conversation_id: conversation_id.to_string(),
                message_id: message_id.to_string(),
                token: token.to_string(),
                done: false,
            };
            if let Err(e) = app.emit("syn-stream-token", &event) {
                log::error!("Failed to emit stream token event: {}", e);
            }
        };
        let emit_done = || {
            let event = SynStreamToken {
                conversation_id: conversation_id.to_string(),
                message_id: message_id.to_string(),
                token: String::new(),
                done: true,
            };
            if let Err(e) = app.emit("syn-stream-token", &event) {
                log::error!("Failed to emit stream done event: {}", e);
            }
        };
        let stop_check = {
            let flag = Arc::clone(&stop_flag);
            move || flag.load(Ordering::SeqCst)
        };

        for iteration in 0..max_iterations {
            log::info!(
                "[Syn Tools] Iteration {}/{} for conversation {}",
                iteration + 1,
                max_iterations,
                conversation_id
            );

            let request = || ChatRequest {
                model,
                messages: &working_messages,
                temperature,
                num_ctx,
                tools: Some(tools),
            };

            // Stream the turn when the provider can report tool calls that
            // way. It usually has nothing to say while it is reaching for a
            // tool — `content` is empty on those turns — so in practice this
            // streams exactly the turn the user is waiting to read, and the
            // answer appears as it is written instead of all at once after a
            // silence.
            let reply = if self.provider.streams_tool_calls() {
                let sink = StreamSink {
                    on_token: &emit_token,
                    stop_requested: &stop_check,
                };
                let out = self.provider.chat_streaming(request(), &sink).await?;
                if (stop_check)() {
                    log::info!("Generation stopped by user during tool loop");
                    emit_done();
                    remove_stop_flag(conversation_id);
                    return Ok(self.assemble(
                        message_id,
                        model,
                        out,
                        start_time,
                        Some(tool_call_log),
                    ));
                }
                out
            } else {
                self.provider.chat(request()).await?
            };

            if reply.tool_calls.is_empty() {
                // No tools wanted. Text is the answer; empty is a dead end.
                if reply.content.is_empty() {
                    log::warn!("[Syn Tools] Response has no content and no tool calls");
                    break;
                }

                log::info!(
                    "[Syn Tools] Final text response received ({} chars), using directly",
                    reply.content.len()
                );

                // Only replay when nothing was streamed. A provider that
                // streams tool calls has already sent every token of this
                // answer, and emitting it again appends the whole reply to
                // itself on screen.
                if !self.provider.streams_tool_calls() {
                    let token_event = SynStreamToken {
                        conversation_id: conversation_id.to_string(),
                        message_id: message_id.to_string(),
                        token: reply.content.clone(),
                        done: false,
                    };
                    let _ = app.emit("syn-stream-token", &token_event);
                }
                emit_done();
                remove_stop_flag(conversation_id);

                return Ok(self.assemble(
                    message_id,
                    model,
                    reply,
                    start_time,
                    Some(tool_call_log),
                ));
            }

            log::info!(
                "[Syn Tools] Model made {} tool call(s)",
                reply.tool_calls.len()
            );

            // The assistant turn that asked for the tools has to go back in
            // the history, or the results that follow answer nothing.
            working_messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: reply.content.clone(),
                tool_calls: Some(reply.tool_calls.clone()),
                tool_call_id: None,
                images: None,
            });

            for tc in &reply.tool_calls {
                let result = {
                    let db = db_state.lock().map_err(|e| {
                        AppError::General(format!("DB lock error during tool call: {}", e))
                    })?;
                    let ctx = crate::syn::tools::ToolContext {
                        db: &db,
                        vault_path,
                        app,
                    };
                    crate::syn::tools::execute_tool(&ctx, &tc.function.name, &tc.function.arguments)
                        .unwrap_or_else(|e| {
                            serde_json::json!({ "error": format!("{}", e) }).to_string()
                        })
                }; // DB lock dropped here

                let event = SynToolCallEvent {
                    conversation_id: conversation_id.to_string(),
                    tool_name: tc.function.name.clone(),
                    tool_args: tc.function.arguments.clone(),
                    result_preview: result.chars().take(4000).collect(),
                    iteration,
                };
                if let Err(e) = app.emit("syn-tool-call", &event) {
                    log::error!("Failed to emit syn-tool-call event: {}", e);
                }
                tool_call_log.push(event);

                working_messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: result,
                    tool_calls: None,
                    // Carried from the call it answers. Ollama drops it;
                    // an OpenAI-shaped endpoint refuses the message without it.
                    tool_call_id: tc.id.clone(),
                    images: None,
                });
            }
        }

        // Out of iterations. Ask once more without tools so the model has to
        // answer with words rather than reach for another call.
        //
        // Loud, because reaching here means the investigation was cut short:
        // the answer that follows is built on however much the model had got
        // to, and it has no way to say so. A user seeing a confidently wrong
        // answer and an operator reading the log are looking for the same
        // line.
        log::warn!(
            "[Syn Tools] Hit the ceiling of {} iterations after {} tool call(s) —              answering from an unfinished investigation. Raise `max_tool_iterations`              in Syn settings if this recurs.",
            max_iterations,
            tool_call_log.len()
        );

        let syn_messages: Vec<SynMessage> = working_messages
            .iter()
            .enumerate()
            .map(|(i, m)| SynMessage {
                id: format!("tool-msg-{}", i),
                role: m.role.clone(),
                content: m.content.clone(),
                model: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
                tokens: None,
                duration_ms: None,
                sources: None,
                tool_calls_log: None,
                images: None,
            })
            .collect();

        let mut final_msg = self
            .send_message(
                app,
                conversation_id,
                message_id,
                &syn_messages,
                model,
                temperature,
                num_ctx,
                max_history,
            )
            .await?;

        if !tool_call_log.is_empty() {
            final_msg.tool_calls_log = Some(tool_call_log);
        }

        Ok(final_msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                content: crate::syn::rag::build_system_prompt("", "auto"),
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

        let reply = engine
            .send_message_with_tools(
                &app,
                "gate-1",
                "gate-1-msg",
                &history,
                &model,
                Some(settings.temperature),
                &crate::syn::tools::get_tool_definitions(),
                &db_state,
                vault.to_str().expect("utf8 vault path"),
                settings.max_tool_iterations,
                settings.num_ctx,
                settings.max_history_messages,
            )
            .await
            .expect("the job runs to an answer");

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
