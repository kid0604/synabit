use crate::error::AppError;
use crate::models::syn::{
    ModelInfo, ProviderStatus, RagConfig, SynChatRequest, SynConversation, SynConversationFull,
    SynMessage, SynProvider, SynSettings,
};
use crate::syn::engine::DriveRequest;
use crate::syn::prompt::{ChatPrompt, PromptPlan, PromptPreview, DEFAULT_BUDGET_CHARS};
use crate::syn::provider::{ollama::OllamaProvider, openai::OpenAiCompatProvider, ChatProvider};
use crate::syn::registry::Registry;
use crate::syn::run::{Budget, Run, RunSummary};
use crate::syn::{conversation, engine::SynEngine, rag};

/// Build the provider the vault's settings ask for.
///
/// The API key is fetched here, from the keychain, rather than read out of
/// `settings` — it is never in `settings`, on purpose. See the doc comment on
/// `SynSettings`.
/// How long a keychain read may take before the app gives up on it.
///
/// The keychain is not a file read. macOS decides whether the *binary asking*
/// is allowed near the item, and when it is not sure it puts a dialog in front
/// of a person and blocks the caller until they answer — with no timeout, on
/// whatever thread asked.
///
/// A development build hits this constantly: `tauri dev` recompiles on every
/// Rust change, and a recompiled binary is a different binary, so the "always
/// allow" granted to the last one does not cover it. Every rebuild earns a new
/// prompt.
///
/// The cost was the whole app. `syn_check_status` reads the key before it
/// touches the network, so a pending dialog left that command open forever —
/// visible in a Web Inspector Network panel as a `syn_check_status` with no
/// size and no time, still spinning. The Rust process sat idle, the timeline
/// was empty, and it read as a freeze.
///
/// Eight seconds: long enough that somebody who sees the dialog and clicks it
/// is served, short enough that somebody who does not is not held hostage.
const KEYCHAIN_PATIENCE: std::time::Duration = std::time::Duration::from_secs(8);

/// The API key, or `None` if the keychain will not answer promptly.
///
/// `spawn_blocking` because the read blocks a thread, and the timeout because
/// a blocked thread must not become a blocked command. Answering `None` is the
/// honest outcome: without a key the provider reports "not connected", which
/// is a screen the user can act on, rather than a spinner that never resolves.
async fn api_key_for(app: &tauri::AppHandle, slot: &'static str) -> Option<String> {
    let handle = app.clone();
    let read = tokio::task::spawn_blocking(move || {
        crate::secrets::SecretManager::get_syn_api_key(Some(&handle), slot)
    });

    match tokio::time::timeout(KEYCHAIN_PATIENCE, read).await {
        Ok(Ok(key)) => key,
        Ok(Err(e)) => {
            log::warn!("[Syn] Keychain read panicked: {e}");
            None
        }
        Err(_) => {
            log::warn!(
                "[Syn] The keychain did not answer within {}s — most likely a macOS \
                 permission dialog is waiting. Carrying on without the key; approve it and \
                 the next check will pick it up.",
                KEYCHAIN_PATIENCE.as_secs()
            );
            None
        }
    }
}

async fn provider_for(app: &tauri::AppHandle, settings: &SynSettings) -> Box<dyn ChatProvider> {
    match settings.provider {
        SynProvider::Ollama => Box::new(OllamaProvider::new(&settings.ollama_url)),
        SynProvider::OpenAiCompat => Box::new(OpenAiCompatProvider::new(
            &settings.openai_base_url,
            api_key_for(app, SynProvider::OpenAiCompat.key_slot()).await,
            settings.openai_reasoning_effort.clone(),
        )),
    }
}

fn settings_for(vault_path: &str) -> SynSettings {
    crate::syn::settings::load_settings(vault_path).unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════
//  PROVIDER STATUS & MODEL MANAGEMENT
// ═══════════════════════════════════════════════════════════════

/// Check whether the configured provider is reachable.
#[tauri::command]
pub async fn syn_check_status(
    app: tauri::AppHandle,
    vault_path: String,
) -> Result<ProviderStatus, AppError> {
    let settings = settings_for(&vault_path);
    provider_for(&app, &settings).await.check_status().await
}

/// List the models the configured provider will accept.
#[tauri::command]
pub async fn syn_list_models(
    app: tauri::AppHandle,
    vault_path: String,
) -> Result<Vec<ModelInfo>, AppError> {
    let settings = settings_for(&vault_path);
    provider_for(&app, &settings).await.list_models().await
}

/// Pull (download) a model from Ollama's registry.
/// Emits `syn-pull-progress` events during download.
///
/// Ollama by name, not through the provider: hosting weights is something
/// Ollama does and an OpenAI-compatible endpoint does not, so this command
/// talks to Ollama whatever the vault's chat provider happens to be. The UI
/// hides it when `ProviderStatus::supports_model_management` is false.
#[tauri::command]
pub async fn syn_pull_model(
    app: tauri::AppHandle,
    vault_path: String,
    model_name: String,
) -> Result<(), AppError> {
    let settings = settings_for(&vault_path);
    OllamaProvider::new(&settings.ollama_url)
        .pull_model(&app, &model_name)
        .await
}

/// Delete a locally stored model from Ollama.
#[tauri::command]
pub async fn syn_delete_model(vault_path: String, model_name: String) -> Result<(), AppError> {
    let settings = settings_for(&vault_path);
    OllamaProvider::new(&settings.ollama_url)
        .delete_model(&model_name)
        .await
}

// ═══════════════════════════════════════════════════════════════
//  SETTINGS & CONFIGURATION
// ═══════════════════════════════════════════════════════════════

/// Store the API key for a provider, or clear it when `key` is blank.
///
/// The key goes to the OS keychain and never to the vault. There is
/// deliberately no command that reads one back: the frontend needs to know
/// *whether* a key is set, never what it is.
#[tauri::command]
pub async fn syn_set_api_key(
    app: tauri::AppHandle,
    provider: SynProvider,
    key: String,
) -> Result<(), AppError> {
    crate::secrets::SecretManager::set_syn_api_key(Some(&app), provider.key_slot(), &key)
        .map_err(AppError::General)
}

/// Whether a key is stored for a provider.
#[tauri::command]
pub async fn syn_has_api_key(
    app: tauri::AppHandle,
    provider: SynProvider,
) -> Result<bool, AppError> {
    Ok(crate::secrets::SecretManager::has_syn_api_key(
        Some(&app),
        provider.key_slot(),
    ))
}

/// Get current Syn settings for the vault.
#[tauri::command]
pub async fn syn_get_settings(vault_path: String) -> Result<SynSettings, AppError> {
    crate::syn::settings::load_settings(&vault_path)
}

/// Save Syn settings for the vault.
#[tauri::command]
pub async fn syn_save_settings(vault_path: String, settings: SynSettings) -> Result<(), AppError> {
    crate::syn::settings::save_settings(&vault_path, &settings)
}

// ═══════════════════════════════════════════════════════════════
//  CHAT / STREAMING (with RAG + Settings)
// ═══════════════════════════════════════════════════════════════

/// Send a message and stream the AI response, with RAG vault context.
///
/// Flow:
/// 1. Load settings from vault
/// 2. Load the conversation from disk
/// 3. Add the user message to the conversation
/// 4. Run RAG retrieval pipeline to gather vault context (if enabled)
/// 5. Build system prompt with personality + vault context + custom prompt
/// 6. Call Ollama with streaming (emits `syn-stream-token` events)
/// 7. Attach RAG sources to the assistant response
/// 8. Save the conversation back to disk
/// 9. Auto-generate a title if this is the first user message
/// 10. Return the assistant's complete SynMessage
#[tauri::command]
pub async fn syn_send_message(
    app: tauri::AppHandle,
    vault_path: String,
    request: SynChatRequest,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<SynMessage, AppError> {
    // 1. Load settings (graceful fallback to defaults)
    let settings = crate::syn::settings::load_settings(&vault_path).unwrap_or_default();

    // 2. Load existing conversation
    let mut conv = conversation::get_conversation(&vault_path, &request.conversation_id)?;

    // Which model to use: what this send asked for, then what the conversation
    // has been using, then the vault default.
    //
    // The conversation's pin is only honoured while it still means something.
    // A model name is only valid for the provider it came from — `gemma4:e4b`
    // is a real model on Ollama and a 404 on OpenAI — so a conversation
    // started under a different provider has its pin ignored rather than sent
    // to an endpoint that has never heard of it. A conversation written before
    // providers existed records none, and those were all Ollama.
    let pinned_provider = conv.meta.provider.unwrap_or(SynProvider::Ollama);
    let conversation_model = if pinned_provider == settings.provider {
        conv.meta.model.clone()
    } else {
        if let Some(stale) = &conv.meta.model {
            log::info!(
                "[Syn] Ignoring `{}`, pinned to this conversation under {:?}, now that the provider is {:?}",
                stale,
                pinned_provider,
                settings.provider
            );
        }
        None
    };

    let model = request
        .model
        .clone()
        .or(conversation_model)
        .or_else(|| settings.default_model.clone())
        .unwrap_or_else(|| "llama3.2".to_string());

    // 3. Create and append the user message
    let user_message = SynMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: request.message.clone(),
        model: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tokens: None,
        duration_ms: None,
        sources: None,
        tool_calls_log: None,
        images: request.images.clone(),
    };
    conv.messages.push(user_message);

    // 4. Build RAG config from settings and run retrieval
    let config = if settings.rag_enabled {
        RagConfig {
            enabled: true,
            max_context_chars: settings.max_context_chars,
            include_finance: settings.include_finance,
            include_feeds: settings.include_feeds,
            graph_expansion_depth: settings.graph_expansion_depth,
            personality: settings.personality.clone(),
        }
    } else {
        RagConfig {
            enabled: false,
            ..RagConfig::default()
        }
    };

    // Retrieval and memory in one lock, because they are both reads and the
    // lock has to be gone before anything async below.
    let (retrieval, context_str, remembered) = {
        let db = state
            .lock()
            .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;

        // What Syn remembers is not conditional on `rag_enabled`. That setting
        // is about searching the vault for this question; a pinned memory is
        // what Syn knows about the person, and turning off retrieval should not
        // give them an assistant that has forgotten their name.
        let remembered = crate::syn::memory::all(&db)
            .map(|memories| {
                crate::syn::memory::memory_block(
                    &memories,
                    crate::syn::memory::MEMORY_BUDGET_CHARS,
                )
            })
            .unwrap_or_else(|e| {
                // Best effort: an unreadable memory store is a reason to answer
                // without it, not a reason to refuse the message.
                log::warn!("[Syn] Could not read memories: {e}");
                None
            });

        if settings.rag_enabled {
            let retrieval_result =
                rag::retrieve_context(&db, &request.message, &conv.messages, &config)?;
            let context_str = rag::format_context(&retrieval_result);
            (retrieval_result, context_str, remembered)
        } else {
            (
                crate::models::syn::RetrievalResult {
                    context_chunks: Vec::new(),
                    total_tokens_estimate: 0,
                    sources: Vec::new(),
                },
                String::new(),
                remembered,
            )
        }
    };

    // 5. Assemble the system prompt from its parts.
    //
    // The custom instructions used to be prepended here by hand, after the
    // prompt had already been built. They are a section of the plan now, so
    // there is one place that knows what the prompt is made of — and one place
    // that can report on it, which is what `syn_preview_prompt` reads.
    let final_system_prompt = PromptPlan::for_chat(ChatPrompt {
        context: &context_str,
        personality: &settings.personality,
        custom: settings.custom_system_prompt.as_deref(),
        memory: remembered.as_deref(),
        budget_chars: DEFAULT_BUDGET_CHARS,
    })
    .render();

    // 6. Build messages for LLM: system prompt + conversation history
    // The system prompt is NOT saved to the conversation file — it's rebuilt each time
    let mut messages_for_llm = vec![SynMessage {
        id: "system".to_string(),
        role: "system".to_string(),
        content: final_system_prompt,
        model: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tokens: None,
        duration_ms: None,
        sources: None,
        tool_calls_log: None,
        images: None,
    }];
    messages_for_llm.extend(conv.messages.iter().cloned());

    // 7. Everything this run is allowed to reach.
    let registry = Registry::for_chat();

    // Use settings temperature as default, allow per-request override
    let temperature = request.temperature.or(Some(settings.temperature));

    // 8. Drive one run to an answer.
    //
    // The run is the record: the goal in the user's own words, the ceilings
    // this request may not exceed, and a transcript written as it happens. It
    // survives the app being closed, which the local variables it replaced did
    // not — so a request that fails now leaves something to read rather than
    // nothing at all.
    let mut run = Run::new(
        request.message.clone(),
        Some(request.conversation_id.clone()),
        Budget::from_settings(&settings),
    );
    crate::syn::run::prune_runs(&vault_path);

    let engine = SynEngine::new(provider_for(&app, &settings).await);
    let assistant_message_id = uuid::Uuid::new_v4().to_string();

    let mut assistant_message = engine
        .drive(
            &mut run,
            DriveRequest {
                app: &app,
                message_id: &assistant_message_id,
                history: &messages_for_llm,
                model: &model,
                temperature,
                registry: &registry,
                db: state.inner(),
                vault_path: &vault_path,
                num_ctx: settings.num_ctx,
                max_history: settings.max_history_messages,
            },
        )
        .await?;

    // 9. Attach RAG sources — but only if the LLM didn't use tool calling.
    //    When tools were used, their results are more precise than RAG context,
    //    so showing RAG sources alongside tool results is just noise.
    let used_tools = assistant_message
        .tool_calls_log
        .as_ref()
        .is_some_and(|l| !l.is_empty());
    if !retrieval.sources.is_empty() && !used_tools {
        assistant_message.sources = Some(retrieval.sources);
    }

    // 10. Add the assistant response to the conversation
    conv.messages.push(assistant_message.clone());

    // Record what answered, so the conversation keeps using it — and record
    // the provider with it, since the name alone does not identify a model.
    // Rewritten rather than only filled in: a conversation that has just
    // switched provider must not keep pointing at the old one's model.
    conv.meta.model = Some(model.clone());
    conv.meta.provider = Some(settings.provider);

    // Update message count
    conv.meta.message_count = conv.messages.len();

    // Auto-generate title if this is the first user message
    // (message_count == 2 means: 1 user + 1 assistant, i.e., first exchange)
    let is_first_exchange = conv.messages.iter().filter(|m| m.role == "user").count() == 1;
    if is_first_exchange {
        conv.meta.title = conversation::auto_title(&request.message);
    }

    // Save the conversation
    conversation::save_conversation(&vault_path, &conv)?;

    // 11. Look back at the exchange and propose what might be worth keeping.
    //
    // Spawned rather than awaited. The user has their answer — it streamed
    // while the run was driving — and making them wait another second or two
    // for a background suggestion would be charging them for a feature that is
    // supposed to cost them nothing but tokens.
    //
    // Only for a run that finished. A cancelled or failed exchange is not
    // evidence of anything, and reflecting on one would propose memories drawn
    // from work the user stopped.
    if settings.memory_reflection && run.state == crate::syn::run::RunState::Done {
        let provider = provider_for(&app, &settings).await;
        let vault = vault_path.clone();
        let model_name = model.clone();
        let asked = request.message.clone();
        let answered = assistant_message.content.clone();
        let run_id = run.id.clone();
        let conversation_id = request.conversation_id.clone();
        let num_ctx = settings.num_ctx;
        // Read before spawning: the state guard is not `Send`, and the memories
        // are what the reflector is told not to propose again.
        let existing = state
            .lock()
            .ok()
            .and_then(|db| crate::syn::memory::all(&db).ok())
            .unwrap_or_default();

        tauri::async_runtime::spawn(async move {
            let proposals = crate::syn::reflect::reflect(
                provider.as_ref(),
                &model_name,
                num_ctx,
                &asked,
                &answered,
                &existing,
                &run_id,
                Some(&conversation_id),
            )
            .await;

            // Every outcome says something, including the empty one. Silence
            // used to mean either "it ran and found nothing worth keeping" —
            // which the reflection prompt calls the normal answer — or "it
            // never ran at all", and those two look identical from outside
            // while meaning opposite things. A quiet feature that cannot be
            // told apart from a dead one is a feature nobody can debug.
            //
            // Nothing-proposed and everything-was-a-duplicate are also kept
            // apart. `proposal::add` dedups against the queue only — not
            // against memories already saved, which the reflection prompt
            // handles by listing them and asking for neither again. So a run
            // that proposes two things and queues none is re-suggesting what
            // the user has not answered yet, which is worth seeing rather than
            // hiding behind the same zero.
            let proposed = proposals.len();
            match crate::syn::proposal::add(&vault, proposals) {
                Ok(0) if proposed == 0 => {
                    log::info!("[Syn] Reflection ran, proposed nothing (run {run_id})");
                }
                Ok(0) => log::info!(
                    "[Syn] Reflection proposed {proposed} thing(s), all already in the \
                     queue (run {run_id})"
                ),
                Ok(n) => log::info!(
                    "[Syn] Reflection proposed {n} thing(s) to remember (run {run_id})"
                ),
                Err(e) => log::warn!("[Syn] Could not queue proposals: {e}"),
            }
        });
    } else {
        log::info!(
            "[Syn] Reflection skipped: {}",
            if settings.memory_reflection {
                "the run did not finish"
            } else {
                "turned off in settings"
            }
        );
    }

    // Return the assistant message
    Ok(assistant_message)
}

/// Signal the engine to stop the current generation.
#[tauri::command]
pub async fn syn_stop_generation(conversation_id: Option<String>) -> Result<(), AppError> {
    SynEngine::stop_generation(conversation_id.as_deref());
    Ok(())
}

/// Stop one run by its own id.
///
/// The chat presses stop on a conversation and knows nothing about runs; the
/// runs panel presses stop on the run in front of it.
#[tauri::command]
pub async fn syn_cancel_run(run_id: String) -> Result<(), AppError> {
    crate::syn::engine::stop_run(&run_id);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════
//  RUNS
// ═══════════════════════════════════════════════════════════════

/// Every run in the vault, newest first.
///
/// `is_live` is handed in so a run left `Working` by a process that has since
/// ended is reported as interrupted rather than as still going.
#[tauri::command]
pub async fn syn_list_runs(vault_path: String) -> Result<Vec<RunSummary>, AppError> {
    crate::syn::run::list_runs(&vault_path, crate::syn::engine::is_live)
}

/// One run in full, including every step.
#[tauri::command]
pub async fn syn_get_run(vault_path: String, run_id: String) -> Result<Run, AppError> {
    crate::syn::run::get_run(&vault_path, &run_id)
}

#[tauri::command]
pub async fn syn_delete_run(vault_path: String, run_id: String) -> Result<(), AppError> {
    crate::syn::run::delete_run(&vault_path, &run_id)
}

// ═══════════════════════════════════════════════════════════════
//  MEMORY
// ═══════════════════════════════════════════════════════════════

/// Everything Syn remembers, most recently confirmed first.
///
/// One typed command rather than letting the screen read raw nodes: a memory
/// has a dozen frontmatter keys with defaults and clamping, and a screen that
/// re-derived those would be a second opinion about what a memory is — the
/// sort that agrees until it does not. Editing goes the other way, through the
/// ordinary node write path, because a memory is an ordinary node and that
/// path already has versions, sync and a trash behind it.
#[tauri::command]
pub async fn syn_list_memories(
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<Vec<crate::syn::memory::Memory>, AppError> {
    let db = state
        .lock()
        .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
    crate::syn::memory::all(&db)
}

/// What the pinned memories cost, against what they are allowed.
///
/// Shown on the memory screen so that pinning is visibly a budget rather than
/// a checkbox: a user who pins their twentieth memory should be able to see
/// that the earlier ones are being dropped before it happens to them.
#[derive(serde::Serialize)]
pub struct MemoryBudget {
    /// Everything remembered. All of it rides in the prompt until the budget
    /// bites, so this — not `pinned` — is what the user is looking at when they
    /// ask what Syn is working from.
    pub total: usize,
    /// How many of those are pinned, which now decides only who survives a cut.
    pub pinned: usize,
    pub chars: usize,
    pub budget_chars: usize,
    /// Memories that do not fit and are being left out of the prompt.
    pub dropped: usize,
}

#[tauri::command]
pub async fn syn_memory_budget(
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<MemoryBudget, AppError> {
    let memories = {
        let db = state
            .lock()
            .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
        crate::syn::memory::all(&db)?
    };
    let total = memories.len();
    let pinned = memories.iter().filter(|m| m.pinned).count();
    let block = crate::syn::memory::memory_block(
        &memories,
        crate::syn::memory::MEMORY_BUDGET_CHARS,
    );
    let chars = block.as_deref().map(|b| b.chars().count()).unwrap_or(0);
    // Counted by the module that renders the lines. Counting `- [` here was
    // wrong the moment instructions stopped carrying a `[kind]` label, and it
    // was wrong quietly: the number just drifted.
    let shown = block
        .as_deref()
        .map(crate::syn::memory::lines_shown)
        .unwrap_or(0);

    Ok(MemoryBudget {
        total,
        pinned,
        chars,
        budget_chars: crate::syn::memory::MEMORY_BUDGET_CHARS,
        dropped: total.saturating_sub(shown),
    })
}

/// What Syn would like to remember, waiting to be allowed to.
#[tauri::command]
pub async fn syn_list_proposals(
    vault_path: String,
) -> Result<Vec<crate::syn::proposal::Proposal>, AppError> {
    Ok(crate::syn::proposal::list(&vault_path))
}

/// Accept one: write the memory, and take it out of the tray.
///
/// The memory is written through the same tool the assistant calls, so a
/// memory the user approved and one Syn was told directly are the same kind of
/// thing on disk — same provenance fields, same folder, same history.
#[tauri::command]
pub async fn syn_accept_proposal(
    app: tauri::AppHandle,
    vault_path: String,
    proposal_id: String,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<(), AppError> {
    let Some(p) = crate::syn::proposal::take(&vault_path, &proposal_id)? else {
        // Already gone — a second click, or two windows. Not an error.
        return Ok(());
    };

    // A proposal may say it replaces something. The reflector names the entry
    // by its text, because text is what it was shown; the id is resolved here,
    // where the memories actually are.
    let replaced = p.supersedes.as_deref().and_then(|body| {
        let wanted = body.trim().to_lowercase();
        let db = state.lock().ok()?;
        crate::syn::memory::all(&db)
            .ok()?
            .into_iter()
            .find(|m| m.body.trim().to_lowercase() == wanted)
            .map(|m| m.id)
    });

    let ctx = crate::syn::tools::ToolContext {
        db: state.inner(),
        vault_path: &vault_path,
        app: &app,
        run_id: Some(&p.source_run),
    };
    crate::syn::tools::execute_tool(
        &ctx,
        "remember",
        &serde_json::json!({
            "body": p.body,
            "kind": p.kind,
            "subject": p.subject,
            "confidence": p.confidence,
            // Explicit, against `remember`'s default. A memory Syn proposed and
            // the user merely agreed to should not outrank one the user asked
            // for by name when the budget eventually has to choose.
            "pinned": false,
            "supersedes": replaced,
        }),
    )?;

    // Retire the old one only after the new one is written, and by trashing it
    // rather than deleting it: the user accepted a replacement, not a loss, and
    // `restore_node` is the way back if the replacement turns out to be wrong.
    if let Some(old) = replaced {
        if let Err(e) = crate::syn::tools::execute_tool(
            &ctx,
            "trash_node",
            &serde_json::json!({ "node_id": old }),
        ) {
            // The new memory is already saved. Failing the whole accept here
            // would leave the user unable to accept anything, so this reports
            // and moves on: two memories that disagree is a worse prompt, not a
            // broken one.
            log::warn!("[Syn] Superseded memory {old} could not be retired: {e}");
        }
    }
    Ok(())
}

/// Decline one. Nothing is written, and nothing is left behind.
#[tauri::command]
pub async fn syn_dismiss_proposal(
    vault_path: String,
    proposal_id: String,
) -> Result<(), AppError> {
    // Taking it out of the queue is not enough. Reflection runs after every
    // message and is free to suggest the same thing again; without a record of
    // the refusal it will, and the user declines it a second time.
    crate::syn::proposal::dismiss(&vault_path, &proposal_id)?;
    Ok(())
}

#[tauri::command]
pub async fn syn_clear_proposals(vault_path: String) -> Result<(), AppError> {
    crate::syn::proposal::clear(&vault_path)
}

// ═══════════════════════════════════════════════════════════════
//  WHAT SYN IS ACTUALLY TOLD
// ═══════════════════════════════════════════════════════════════

/// The system prompt this vault would send, and where its room goes.
///
/// `message` is optional: with one, retrieval runs and the preview includes the
/// context that question would pull in, which is the only way to see how much
/// of the window retrieval is taking. Without one, it is the fixed part.
#[tauri::command]
pub async fn syn_preview_prompt(
    vault_path: String,
    message: Option<String>,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<PromptPreview, AppError> {
    let settings = settings_for(&vault_path);

    // The preview has to show the memory section too, or the one screen that
    // says what Syn is told would be the one place it is not visible.
    let remembered = {
        let db = state
            .lock()
            .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
        crate::syn::memory::all(&db)
            .map(|m| crate::syn::memory::memory_block(&m, crate::syn::memory::MEMORY_BUDGET_CHARS))
            .unwrap_or(None)
    };

    let context = match message.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(message) if settings.rag_enabled => {
            let config = RagConfig {
                enabled: true,
                max_context_chars: settings.max_context_chars,
                include_finance: settings.include_finance,
                include_feeds: settings.include_feeds,
                graph_expansion_depth: settings.graph_expansion_depth,
                personality: settings.personality.clone(),
            };
            let db = state
                .lock()
                .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
            let retrieved = rag::retrieve_context(&db, message, &[], &config)?;
            rag::format_context(&retrieved)
        }
        _ => String::new(),
    };

    Ok(PromptPlan::for_chat(ChatPrompt {
        context: &context,
        personality: &settings.personality,
        custom: settings.custom_system_prompt.as_deref(),
        memory: remembered.as_deref(),
        budget_chars: DEFAULT_BUDGET_CHARS,
    })
    .into())
}

/// Cancel an ongoing model pull.
#[tauri::command]
pub fn syn_cancel_pull() {
    OllamaProvider::cancel_pull();
}

// ═══════════════════════════════════════════════════════════════
//  CONVERSATION CRUD
// ═══════════════════════════════════════════════════════════════

/// List all conversations in the vault (metadata only, sorted by recency).
#[tauri::command]
pub async fn syn_list_conversations(vault_path: String) -> Result<Vec<SynConversation>, AppError> {
    conversation::list_conversations(&vault_path)
}

/// Load a full conversation by ID (metadata + all messages).
#[tauri::command]
pub async fn syn_get_conversation(
    vault_path: String,
    conversation_id: String,
) -> Result<SynConversationFull, AppError> {
    conversation::get_conversation(&vault_path, &conversation_id)
}

/// Create a new empty conversation.
#[tauri::command]
pub async fn syn_create_conversation(
    vault_path: String,
    title: Option<String>,
) -> Result<SynConversation, AppError> {
    conversation::create_conversation(&vault_path, title)
}

/// Delete a conversation by ID.
#[tauri::command]
pub async fn syn_delete_conversation(
    vault_path: String,
    conversation_id: String,
) -> Result<(), AppError> {
    conversation::delete_conversation(&vault_path, &conversation_id)
}

/// Rename a conversation.
#[tauri::command]
pub async fn syn_rename_conversation(
    vault_path: String,
    conversation_id: String,
    title: String,
) -> Result<(), AppError> {
    conversation::rename_conversation(&vault_path, &conversation_id, &title)
}

/// Toggle pin status of a conversation.
#[tauri::command]
pub async fn syn_pin_conversation(
    vault_path: String,
    conversation_id: String,
    pinned: bool,
) -> Result<(), AppError> {
    conversation::pin_conversation(&vault_path, &conversation_id, pinned)
}

/// Export a conversation as markdown.
#[tauri::command]
pub async fn syn_export_conversation(
    vault_path: String,
    conversation_id: String,
) -> Result<String, AppError> {
    conversation::export_conversation_markdown(&vault_path, &conversation_id)
}
