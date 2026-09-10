use crate::error::AppError;
use tauri::Emitter;
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

/// The user's standing instructions, from the file if there is one.
///
/// `{vault}/SYN.md` wins whenever it exists; `custom_system_prompt` is what a
/// vault written before the file existed still carries, and is moved into the
/// file the first time this runs. Two sources for one thing is how they drift,
/// so this is the only place either is read. See `syn::instructions`.
fn standing_instructions(vault_path: &str, settings: &SynSettings) -> Option<String> {
    if let Some(from_settings) = settings.custom_system_prompt.as_deref() {
        crate::syn::instructions::migrate(vault_path, from_settings);
    }
    // The retired `personality` setting, carried across once. A voice somebody
    // chose must not disappear in an upgrade with nothing saying why — the same
    // reason `custom_system_prompt` was moved rather than dropped.
    if let Some(chosen) = settings.personality.as_deref() {
        crate::syn::instructions::migrate_personality(vault_path, chosen);
    }
    crate::syn::instructions::load(vault_path)
        .or_else(|| settings.custom_system_prompt.clone())
        .as_deref()
        .and_then(crate::syn::instructions::block)
}

fn settings_for(vault_path: &str) -> SynSettings {
    crate::syn::settings::load_settings(vault_path).unwrap_or_default()
}

/// What every Syn command says when the switch is off.
///
/// One string, in one place, because the frontend matches on it to tell "Syn is
/// off" apart from "the model provider is unreachable" — two states that look
/// identical from the outside and mean opposite things about whether anything
/// is wrong. A message that drifted between two call sites would show the
/// user an error for a choice they made on purpose.
pub const SWITCHED_OFF: &str = "Syn is switched off";

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
    browser_state: tauri::State<'_, crate::syn::browser::Waiting>,
) -> Result<SynMessage, AppError> {
    // 1. Load settings (graceful fallback to defaults)
    let settings = crate::syn::settings::load_settings(&vault_path).unwrap_or_default();

    // Off means off, and it has to be enforced here rather than only on the
    // screen. A switch that hides the composer while the command still answers
    // is a switch that lies: the ask bar, a stale window and anything added
    // later all reach this function, and only this function can refuse them.
    //
    // Nothing is written before this returns — no conversation is touched, no
    // run file appears — so switching Syn off mid-thought leaves nothing behind.
    if !settings.enabled {
        return Err(AppError::General(SWITCHED_OFF.to_string()));
    }

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

    // 3. Create and append the user message — unless this is the same question
    //    being carried on after Syn stopped to ask permission.
    //
    //    Carrying on is not a new turn. Nobody typed anything: they pressed a
    //    button on a card, and the question still on the table is the one they
    //    already asked. Appending "" as a user message would put an empty
    //    bubble in the conversation and hand the model a turn with nothing in
    //    it. See `syn_answer_consent`.
    let carrying_on = request.resume_run.as_deref();
    let question = if carrying_on.is_some() {
        // The stopped run left an assistant turn with no words in it — that is
        // what `LoopEnd::NeedsConsent` assembles. Dropped rather than kept:
        // sending it back to the model is a turn that says nothing, and some
        // providers refuse an empty assistant message outright.
        if conv
            .messages
            .last()
            .is_some_and(|m| m.role == "assistant" && m.content.trim().is_empty())
        {
            conv.messages.pop();
        }

        let Some(asked) = conv.messages.iter().rev().find(|m| m.role == "user") else {
            return Err(AppError::General(
                "There is nothing to carry on with in this conversation".to_string(),
            ));
        };
        asked.content.clone()
    } else {
        let user_message = SynMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: request.message.clone(),
            model: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tokens: None,
            duration_ms: None,
            sources: None,
            footing: None,
            tool_calls_log: None,
            images: request.images.clone(),
        };
        conv.messages.push(user_message);
        request.message.clone()
    };

    // 4. Build RAG config from settings and run retrieval
    let config = if settings.rag_enabled {
        RagConfig {
            enabled: true,
            max_context_chars: settings.max_context_chars,
            include_finance: settings.include_finance,
            include_feeds: settings.include_feeds,
            graph_expansion_depth: settings.graph_expansion_depth,
            }
    } else {
        RagConfig {
            enabled: false,
            ..RagConfig::default()
        }
    };

    // Retrieval, memory and the skill index in one lock: they are all reads,
    // and the lock has to be gone before anything async below.
    let (retrieval, context_str, remembered, skill_index, thread_block, counted) = {
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

        // The skill index, on the same terms. Only what the user enabled is
        // named, because a name in this list is an invitation.
        let skill_index = crate::syn::skill::all(&db)
            .map(|skills| {
                crate::syn::skill::index_block(
                    &skills,
                    crate::syn::skill::INDEX_BUDGET_CHARS,
                )
            })
            .unwrap_or_else(|e| {
                log::warn!("[Syn] Could not read skills: {e}");
                None
            });

        // Is this a question the index already answers? Decided here, on the
        // same lock as everything else, and the query is run *now* rather than
        // asked for by the model — which is the whole of the instant tempo.
        let counted = crate::syn::tempo::countable_types(&db)
            .ok()
            .and_then(|types| crate::syn::tempo::of(&question, &types))
            .and_then(|instant| {
                let found = db.run_node_query(&crate::syn::tempo::query_for(&instant)).ok()?;
                let sample = crate::syn::tempo::sample(&found);
                Some(crate::syn::tempo::block(&instant, found.total, &sample))
            });

        // The open thread, if the question came from inside one. Read on this
        // lock with everything else, and best-effort for the same reason: a
        // thread that has been trashed since the window remembered it is a
        // reason to answer without it, not a reason to refuse the message.
        let thread_block = request
            .focus
            .as_ref()
            .and_then(|f| f.thread.as_deref())
            .and_then(|id| crate::syn::thread::get(&db, id))
            .map(|t| t.block());

        if settings.rag_enabled {
            let retrieval_result =
                rag::retrieve_context(&db, &question, &conv.messages, &config)?;
            let context_str = rag::format_context(&retrieval_result);
            (retrieval_result, context_str, remembered, skill_index, thread_block, counted)
        } else {
            (
                crate::models::syn::RetrievalResult {
                    context_chunks: Vec::new(),
                    total_tokens_estimate: 0,
                    sources: Vec::new(),
                },
                String::new(),
                remembered,
                skill_index,
                thread_block,
                counted,
            )
        }
    };

    // 5. Assemble the system prompt from its parts.
    //
    // The custom instructions used to be prepended here by hand, after the
    // prompt had already been built. They are a section of the plan now, so
    // there is one place that knows what the prompt is made of — and one place
    // that can report on it, which is what `syn_preview_prompt` reads.
    let standing = standing_instructions(&vault_path, &settings);
    // What is on screen includes the browsing pane, and the front end cannot
    // see it — it is a webview of the operating system's, beside the app rather
    // than inside it. Filled in here, where the app handle is.
    let focus = crate::syn::focus::with_the_pane(
        request.focus.clone(),
        crate::syn::pane::showing(&app),
    );
    let final_system_prompt = PromptPlan::for_chat(ChatPrompt {
        context: &context_str,
        custom: standing.as_deref(),
        skills: skill_index.as_deref(),
        memory: remembered.as_deref(),
        focus: focus.as_ref(),
        thread: thread_block.as_deref(), counted: None,
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
        footing: None,
        tool_calls_log: None,
        images: None,
    }];
    messages_for_llm.extend(conv.messages.iter().cloned());

    // 7. Everything this run is allowed to reach.
    //
    // Nothing, when the count is already in the prompt. A turn with tools would
    // spend a round deciding not to use them, which is the cost this tempo
    // exists to remove — see `syn::tempo`.
    let instant = counted.is_some();
    let registry = if instant { Registry::none() } else { Registry::for_chat() };

    // Use settings temperature as default, allow per-request override
    let temperature = request.temperature.or(Some(settings.temperature));

    // 8. Drive one run to an answer.
    //
    // The run is the record: the goal in the user's own words, the ceilings
    // this request may not exceed, and a transcript written as it happens. It
    // survives the app being closed, which the local variables it replaced did
    // not — so a request that fails now leaves something to read rather than
    // nothing at all.
    let mut budget = Budget::from_settings(&settings);
    if instant {
        // One round. There is nothing to come back for.
        budget.iterations = Some(1);
    }

    // What the stopped run was about to do, which is the thing the user just
    // gave permission for. Read off that run rather than worked out again — see
    // `run::Run::pending_call` for the transcript that made this necessary.
    let resume_call = carrying_on
        .and_then(|id| crate::syn::run::get_run(&vault_path, id).ok())
        .and_then(|stopped| stopped.pending_call);

    let mut run = Run::new(
        question.clone(),
        Some(request.conversation_id.clone()),
        budget,
    );
    run.tempo = if instant {
        crate::syn::tempo::Tempo::Instant
    } else {
        crate::syn::tempo::Tempo::Working
    };

    // Said before the work starts, not after: somebody who is about to wait
    // should know they are about to wait.
    if let Err(e) = app.emit(
        "syn-tempo",
        serde_json::json!({
            "conversation_id": request.conversation_id,
            "tempo": run.tempo,
        }),
    ) {
        log::error!("Failed to emit syn-tempo: {e}");
    }
    // Which piece of work this served, so that "do threads do anything" is a
    // question the runs can answer. See `thread::usage`.
    run.thread = request.focus.as_ref().and_then(|f| f.thread.clone());
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
                browser: &browser_state,
                resume_call,
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

    // Read before the move below. `assistant_message.sources` is only filled in
    // when no tool was used, so asking the message afterwards would report zero
    // retrieved for precisely the runs that had the most to stand on.
    let retrieved = retrieval.sources.len();

    if !retrieval.sources.is_empty() && !used_tools {
        assistant_message.sources = Some(retrieval.sources);
    }

    // What the answer turned out to be standing on. Decided from the transcript
    // and the tempo, both of which are already written down — nothing here asks
    // the model what it thinks it knew. See `syn::footing`.
    //
    // Onto both: the run keeps it so "how often was Syn guessing" stays
    // answerable after the conversation is deleted, and the message keeps it so
    // the mark is still there when the conversation is reopened tomorrow.
    // Only for a turn that produced an answer.
    //
    // A run that stopped to ask permission has no answer, and a footing is a
    // statement about *what an answer was standing on*. Marking those was the
    // first thing writing the footing down revealed: four of six runs in one
    // conversation were consent stops, and each landed in the tally as a
    // measured answer that had never been given.
    //
    // The emptiness of the reply is the test rather than the run's state,
    // because it is the same question the tally is asking. A run can end in
    // several ways with nothing said, and every one of them means the same
    // thing here.
    if !assistant_message.content.trim().is_empty() {
        // The work is finished, so a "just this once" said during it stops
        // standing. The next question is new work and is asked about again —
        // which is the whole difference between that answer and `Always`.
        crate::syn::consent::work_is_done(&request.conversation_id);

        let footing = crate::syn::footing::of(&run, &crate::syn::footing::Evidence { retrieved });
        run.footing = Some(footing);
        assistant_message.footing = Some(footing);

        // Written down, or it was never decided. `drive` saved the run for the
        // last time before this line existed, so the footing was computed here,
        // put on a struct that nothing saved again, and dropped — every run on
        // disk read `null`, and `footing::tally` counted the vault's whole
        // history as unmeasured. `ENOUGH_TO_MEAN_ANYTHING` was therefore never
        // reached and the screen showed nothing, for ever, while looking like
        // it was working.
        crate::syn::run::save_run_best_effort(&vault_path, &run);
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
        conv.meta.title = conversation::auto_title(&question);
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
        let asked = question.clone();
        let answered = assistant_message.content.clone();
        // Decided here, where the conversation is in hand: a correction needs
        // something to correct, and only this side knows whether the assistant
        // had already replied. See `syn::correction`.
        let corrected = crate::syn::correction::looks_like_one(
            &asked,
            conv.messages.iter().any(|m| m.role == "assistant"),
        );
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
                corrected,
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
        // A run that followed a skill and still went wrong may have something to
        // teach that skill. Whether it did is a fact on the transcript — a
        // failed call, or a ceiling — so nothing is asked of the model unless
        // there is.
        if let Some(name) = crate::syn::skill::skill_that_struggled(&run) {
            let struggling = state
                .lock()
                .ok()
                .and_then(|db| crate::syn::skill::all(&db).ok())
                .unwrap_or_default()
                .into_iter()
                .find(|s| s.name.trim().to_lowercase() == name.trim().to_lowercase())
                // One pending revision at a time. A second proposal on top of an
                // unread one is a queue nobody asked for, and the user would be
                // reviewing a change to a change.
                .filter(|s| s.pending_revision.is_none());

            if let Some(skill) = struggling {
                let provider = provider_for(&app, &settings).await;
                let vault = vault_path.clone();
                let model_name = model.clone();
                let num_ctx = settings.num_ctx;
                let goal = run.goal.clone();
                let went_wrong = crate::syn::skill::what_went_wrong(&run);
                let app_handle = app.clone();

                tauri::async_runtime::spawn(async move {
                    let Some(draft) = crate::syn::reflect::suggest_revision(
                        provider.as_ref(),
                        &model_name,
                        num_ctx,
                        &skill.name,
                        &skill.body,
                        &went_wrong,
                        &goal,
                    )
                    .await
                    else {
                        return;
                    };
                    if let Err(e) = stage_revision(&app_handle, &vault, &skill.id, &draft) {
                        log::warn!("[Syn] Could not stage a revision for `{}`: {e}", skill.name);
                        return;
                    }
                    log::info!(
                        "[Syn] Proposed a revision to `{}`, waiting to be read",
                        skill.name
                    );
                });
            }
        }

        // A run that did the same multi-step job twice may be worth writing
        // down. The decision that it repeated itself is arithmetic and already
        // made; the model is only asked to name the thing and write the steps,
        // and only when there is something to name — so the great majority of
        // runs, which repeat nothing, cost nothing here.
        //
        // Gated on the same switch as memory reflection. They are two jobs and
        // will want two switches, but adding a settings field is a migration
        // across both languages and this is the wrong change to bundle it with.
        // Stated rather than hidden: turning off reflection turns off both.
        if let Some(chain) = crate::syn::skill::repeated_chain(&run) {
            if !crate::syn::skill::already_proposed(&vault_path, &chain) {
                let provider = provider_for(&app, &settings).await;
                let vault = vault_path.clone();
                let model_name = model.clone();
                let goal = run.goal.clone();
                let run_id_for_skill = run.id.clone();
                let num_ctx = settings.num_ctx;
                let app_handle = app.clone();
                let taken: Vec<String> = state
                    .lock()
                    .ok()
                    .and_then(|db| crate::syn::skill::all(&db).ok())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| s.name)
                    .collect();

                tauri::async_runtime::spawn(async move {
                    let Some(draft) = crate::syn::reflect::suggest_skill(
                        provider.as_ref(),
                        &model_name,
                        num_ctx,
                        &goal,
                        &chain,
                        &taken,
                    )
                    .await
                    else {
                        // Recorded even so. The model looked at this shape and
                        // said no; asking it again on the next identical run
                        // would spend the same tokens for the same answer.
                        let _ = crate::syn::skill::remember_proposed(&vault, &chain);
                        return;
                    };

                    if let Err(e) =
                        write_suggested_skill(&app_handle, &vault, &draft, &chain, Some(&run_id_for_skill))
                    {
                        log::warn!("[Syn] Could not write the suggested skill: {e}");
                        return;
                    }
                    let _ = crate::syn::skill::remember_proposed(&vault, &chain);
                    log::info!(
                        "[Syn] Suggested a skill `{}`, turned off until reviewed",
                        draft.name
                    );
                });
            }
        }
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

/// Write a suggested skill into the vault, turned off.
///
/// Off, and it is the whole safeguard. An agent that enables its own skills is
/// an agent changing its behaviour without anybody knowing, which is what the
/// roadmap's N2 forbids — so this writes a file the user can read, edit and
/// switch on, and nothing else happens until they do.
///
/// `author: syn` so the screen can say where it came from, and `version: 1` so
/// the first edit somebody makes is a version they can roll back from.
fn write_suggested_skill(
    app: &tauri::AppHandle,
    vault_path: &str,
    draft: &crate::syn::reflect::SkillDraft,
    chain: &[String],
    source_run: Option<&str>,
) -> Result<(), AppError> {
    use tauri::Manager;
    let state = app.state::<crate::db::DbState>();
    let ctx = crate::syn::tools::ToolContext {
        db: &state,
        vault_path,
        app,
        run_id: None,
    };

    let mut properties = crate::syn::skill::frontmatter(
        &draft.name,
        &draft.description,
        &draft.when_to_use,
        crate::syn::skill::Tier::Prose,
        chain,
        "syn",
        false,
        1,
    );
    // The shape it came from, kept on the file so a person reading it can see
    // what Syn actually watched them do.
    if let Some(map) = properties.as_object_mut() {
        map.insert("from_chain".to_string(), serde_json::json!(chain));
        if let Some(run) = source_run {
            // So the trial can ask the same question the skill was invented to
            // answer, rather than a question somebody made up for it.
            map.insert("source_run".to_string(), serde_json::json!(run));
        }
    }

    crate::syn::tools::execute_tool(
        &ctx,
        "create_node",
        &serde_json::json!({
            "node_type": crate::syn::skill::SKILL_TYPE,
            "title": draft.name,
            "content": draft.steps,
            "properties": properties,
        }),
    )?;
    Ok(())
}

/// Put a proposed revision on the skill, without applying it.
///
/// Frontmatter, not the body. The skill is enabled — that is why it ran and why
/// it went wrong — so writing the new steps in would change behaviour the
/// moment they were written, which is an agent editing its own live procedure
/// while nobody is looking. It waits here until a person has read both.
fn stage_revision(
    app: &tauri::AppHandle,
    vault_path: &str,
    skill_id: &str,
    draft: &crate::syn::reflect::RevisionDraft,
) -> Result<(), AppError> {
    use tauri::Manager;
    let state = app.state::<crate::db::DbState>();
    let ctx = crate::syn::tools::ToolContext {
        db: &state,
        vault_path,
        app,
        run_id: None,
    };
    crate::syn::tools::execute_tool(
        &ctx,
        "update_node",
        &serde_json::json!({
            "node_id": skill_id,
            "properties": {
                "pending_revision": draft.steps,
                "revision_because": draft.because,
            },
        }),
    )?;
    Ok(())
}

/// What is wrong with each recipe, by skill id.
///
/// Checked on the screen rather than at run time. A recipe that only reports
/// its problems when the model reaches for it fails in front of the user, half
/// way through a job, in a place where the explanation is a tool result nobody
/// reads. Here it is a line under the skill, before it is ever switched on.
#[tauri::command]
pub async fn syn_recipe_problems(
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    let skills = {
        let db = state
            .lock()
            .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
        crate::syn::skill::all(&db)?
    };

    let known: Vec<String> = crate::syn::tools::get_tool_definitions()
        .into_iter()
        .map(|t| t.function.name)
        .collect();

    let mut found = std::collections::HashMap::new();
    for skill in skills {
        if skill.tier != crate::syn::skill::Tier::Recipe {
            continue;
        }
        let problems = match crate::syn::recipe::parse(&skill.body) {
            Ok(Some(recipe)) => crate::syn::recipe::problems(&recipe, &known),
            Ok(None) => vec![
                "this says it is a recipe but has no ```recipe block".to_string()
            ],
            Err(e) => vec![e],
        };
        if !problems.is_empty() {
            found.insert(skill.id, problems);
        }
    }
    Ok(found)
}

/// Start a skill the user will write.
///
/// The app's job here is a well-formed starting point, not a form. A skill is a
/// Markdown file and the place to write one is wherever they already edit their
/// notes; what they cannot be expected to know is which frontmatter keys mean
/// anything, so the template carries that documentation inside itself.
///
/// Turned off, like everything else that arrives without being read. The
/// difference from one Syn wrote is that this one needs no trial — it is theirs,
/// and they may switch it on the moment it says what they want.
#[tauri::command]
pub async fn syn_create_skill(
    app: tauri::AppHandle,
    vault_path: String,
    name: String,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::General("A skill needs a name".to_string()));
    }

    {
        let db = state
            .lock()
            .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
        if crate::syn::skill::find(&crate::syn::skill::all(&db)?, name).is_some() {
            return Err(AppError::General(format!(
                "There is already a skill called `{name}`"
            )));
        }
    }

    let ctx = crate::syn::tools::ToolContext {
        db: state.inner(),
        vault_path: &vault_path,
        app: &app,
        run_id: None,
    };
    let out = crate::syn::tools::execute_tool(
        &ctx,
        "create_node",
        &serde_json::json!({
            "node_type": crate::syn::skill::SKILL_TYPE,
            "title": name,
            "content": crate::syn::skill::starter_body(),
            "properties": crate::syn::skill::frontmatter(
                name,
                "",
                "",
                crate::syn::skill::Tier::Prose,
                &[],
                "user",
                false,
                1,
            ),
        }),
    )?;

    let id = serde_json::from_str::<serde_json::Value>(&out)
        .ok()
        .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(str::to_string))
        .unwrap_or_default();
    Ok(id)
}

/// One question, answered with the skill and without it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillTrial {
    pub question: String,
    pub without: String,
    pub with: String,
}

/// Try a skill before turning it on.
///
/// The roadmap's third step, and the one it says may not be skipped. It asks
/// the question the skill was invented to answer — the goal of the run that
/// produced it — twice: once with the vault as it stands, and once with this
/// skill's steps in front of the model.
///
/// The second arm hands the model the body directly rather than waiting for it
/// to call `load_skill`. That is deliberate and worth being clear about: the
/// question here is "if it follows these steps, is the answer better", not
/// "will it choose to". The second question is real and is what
/// `syn_skill_usage` counts, but it is not what somebody deciding whether to
/// trust a procedure needs to know first.
///
/// Recording the trial on the file is what makes step 4 enforceable: until this
/// has run, `Skill::may_be_enabled` says no.
#[tauri::command]
pub async fn syn_skill_trial(
    app: tauri::AppHandle,
    vault_path: String,
    skill_id: String,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<SkillTrial, AppError> {
    let settings = settings_for(&vault_path);
    // A trial drives a run of its own, so the switch has to reach it as well —
    // otherwise "Syn is off" would still be able to call tools from the skills
    // screen.
    if !settings.enabled {
        return Err(AppError::General(SWITCHED_OFF.to_string()));
    }
    let model = settings
        .default_model
        .clone()
        .ok_or_else(|| AppError::General("No model is configured".to_string()))?;

    let (skill, remembered, index_without) = {
        let db = state
            .lock()
            .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
        let skills = crate::syn::skill::all(&db)?;
        let skill = skills
            .iter()
            .find(|s| s.id == skill_id)
            .cloned()
            .ok_or_else(|| AppError::General(format!("No skill at {skill_id}")))?;
        let remembered = crate::syn::memory::all(&db)
            .map(|m| crate::syn::memory::memory_block(&m, crate::syn::memory::MEMORY_BUDGET_CHARS))
            .unwrap_or(None);
        let index = crate::syn::skill::index_block(&skills, crate::syn::skill::INDEX_BUDGET_CHARS);
        (skill, remembered, index)
    };

    // The question the skill exists to answer. Its own `when_to_use` is the
    // fallback, and a poor one — it describes the moment, not the request — but
    // a run can be pruned away and a trial should still be possible.
    let question = skill
        .source_run
        .as_deref()
        .and_then(|id| crate::syn::run::get_run(&vault_path, id).ok())
        .map(|run| run.goal)
        .filter(|goal| !goal.trim().is_empty())
        .unwrap_or_else(|| {
            if skill.when_to_use.trim().is_empty() {
                skill.description.clone()
            } else {
                skill.when_to_use.clone()
            }
        });

    // With the skill: it is named in the index and its steps are in front of
    // the model, as they would be after `load_skill`.
    let mut enabled_for_trial = skill.clone();
    enabled_for_trial.enabled = true;
    let index_with = crate::syn::skill::index_block(
        &[enabled_for_trial],
        crate::syn::skill::INDEX_BUDGET_CHARS,
    )
    .map(|index| {
        format!(
            "{index}\n\n=== THE STEPS OF `{}` ===\n{}\n=== END ===",
            skill.name, skill.body
        )
    });

    let provider = provider_for(&app, &settings).await;
    let ask = |skills: Option<&str>| {
        let standing = standing_instructions(&vault_path, &settings);
        let system = PromptPlan::for_chat(ChatPrompt {
            context: "",
            custom: standing.as_deref(),
            skills,
            memory: remembered.as_deref(), focus: None, thread: None, counted: None,
            budget_chars: DEFAULT_BUDGET_CHARS,
        })
        .render();
        vec![
            crate::syn::provider::ChatMessage::new("system", system),
            crate::syn::provider::ChatMessage::new("user", question.clone()),
        ]
    };

    let mut answers = Vec::new();
    for messages in [ask(index_without.as_deref()), ask(index_with.as_deref())] {
        let reply = provider
            .chat(crate::syn::provider::ChatRequest {
                model: &model,
                messages: &messages,
                temperature: Some(settings.temperature),
                num_ctx: settings.num_ctx,
                tools: None,
            })
            .await
            .map_err(|e| AppError::General(format!("The trial could not run: {e}")))?;
        answers.push(reply.content);
    }

    // Only now, and only because both halves came back: a trial that failed
    // half way should not unlock anything.
    {
        use tauri::Manager;
        let handle = app.clone();
        let db = handle.state::<crate::db::DbState>();
        let ctx = crate::syn::tools::ToolContext {
            db: &db,
            vault_path: &vault_path,
            app: &app,
            run_id: None,
        };
        crate::syn::tools::execute_tool(
            &ctx,
            "update_node",
            &serde_json::json!({
                "node_id": skill.id,
                "properties": { "trial_at": chrono::Utc::now().to_rfc3339()[..10].to_string() },
            }),
        )?;
    }

    Ok(SkillTrial {
        question,
        without: answers[0].clone(),
        with: answers[1].clone(),
    })
}

/// Everything the user has agreed to, or refused, on this device.
/// A page in the browsing window handing back what it is showing.
///
/// # The one command remote content may call
///
/// Everything else in this app is unreachable from a page — a capability's
/// `remote` field defaults to `None`, so `capabilities/*.json` grant local
/// content only, and `browser::the_browsing_window_grants_no_page_any_command`
/// keeps it that way.
///
/// This is the single exception, and it is shaped to be a boring one: it takes
/// a string and returns nothing. It cannot read the vault, touch a file, or
/// reach another command. The string is the page's own HTML, which was always
/// going to be written by a stranger — it goes straight into `web::wrap`, the
/// boundary that already assumes exactly that.
///
/// The nonce is why an advert in an iframe cannot answer first with a page
/// nobody asked for.
#[tauri::command]
pub async fn syn_browser_content(
    waiting: tauri::State<'_, crate::syn::browser::Waiting>,
    nonce: String,
    url: String,
    html: String,
) -> Result<(), AppError> {
    crate::syn::browser::accept(&waiting, &nonce, url, html);
    Ok(())
}

/// Everything Syn can reach, with what each one needs, what undoes it, what it
/// costs, how often it has been used, and whether it is switched on.
///
/// The question the inspector could not answer. It had *what did it do*, *what
/// was it told* and *what has it been allowed* — and no way to find out what it
/// can reach in the first place, which is the question people ask before they
/// decide to trust something rather than after.
///
/// It could not answer the next one either: *and may I change that*. The list
/// was a catalogue with no controls, so the only way to turn anything off was
/// to wait for Syn to ask about it — which, of twenty-nine tools, happened for
/// exactly one.
///
/// The usage tally is a whole-directory read. It costs about a megabyte of
/// JSON on a vault with a few hundred runs, on a panel somebody opened
/// deliberately, and it is what turns twenty-nine decisions into one.
#[tauri::command]
pub async fn syn_list_tools(
    vault_path: String,
) -> Result<Vec<crate::syn::registry::ToolCard>, AppError> {
    let ledger = crate::syn::consent::load(&vault_path);
    let now = chrono::Utc::now().to_rfc3339();
    let used = crate::syn::run::tool_usage(&vault_path);

    Ok(crate::syn::registry::catalogue(&ledger, &now)
        .into_iter()
        .map(|mut card| {
            if let Some((count, last)) = used.get(&card.name) {
                card.used = *count;
                card.last_used = Some(last.clone());
            }
            card
        })
        .collect())
}

/// Turn a whole kind of power on or off.
///
/// # Why the switch is a `Never` and not a setting of its own
///
/// Because the ledger already answers "what may Syn do", the Permissions tab
/// already shows it, and `decide` already reads it before anything runs. A
/// second list of enabled tools would be the same fact written twice, and two
/// places to edit one contract is how the two come to disagree — which this
/// codebase has watched happen often enough to have a rule about it.
///
/// So switching off records the same `Never` a person would have produced by
/// answering a card, and switching back on is the same revoke the Permissions
/// tab already offers. One record, two views of it.
///
/// The capability arrives from the screen as the value it was given, rather
/// than as a key the front end assembles. A scope string composed in
/// TypeScript would be a second copy of `scope_key`, and the first thing it
/// would do is drift.
#[tauri::command]
pub async fn syn_set_capability(
    vault_path: String,
    capability: crate::syn::consent::Capability,
    allowed: bool,
) -> Result<(), AppError> {
    if allowed {
        let Some(scope) = capability.scope_key() else {
            // Nothing to take back: `Spend` and `Execute` are asked every time
            // and no answer to them is kept. Silence rather than an error,
            // because "it is already on" is what the caller wanted.
            return Ok(());
        };
        return crate::syn::consent::revoke(&vault_path, &scope);
    }

    crate::syn::consent::record(
        &vault_path,
        &capability,
        crate::syn::consent::Answer::Never,
        chrono::Utc::now(),
    )
}

#[tauri::command]
pub async fn syn_list_grants(vault_path: String) -> Result<Vec<crate::syn::consent::Grant>, AppError> {
    Ok(crate::syn::consent::load(&vault_path).grants)
}

/// Take one of those back.
#[tauri::command]
pub async fn syn_revoke_grant(vault_path: String, scope: String) -> Result<(), AppError> {
    crate::syn::consent::revoke(&vault_path, &scope)
}

/// What Syn has done that reached past the vault.
#[tauri::command]
pub async fn syn_audit_log(vault_path: String) -> Result<Vec<crate::syn::audit::Entry>, AppError> {
    Ok(crate::syn::audit::read(&vault_path))
}

/// Say which one was meant.
///
/// Records the pick and puts the question away. It does **not** carry on with
/// the work — the same reasoning as `syn_answer_consent`: the person is in a
/// conversation, the natural way to say *go on* is to say it, and work
/// restarting behind them while they are still reading why it stopped is the
/// thing a card in the transcript exists to avoid.
///
/// What the screen does instead is put the answer in the composer, so saying
/// *go on* is one keystroke. See `syn::ambiguity`.
#[tauri::command]
pub async fn syn_answer_choice(
    vault_path: String,
    run_id: String,
    node_id: String,
) -> Result<(), AppError> {
    let mut run = crate::syn::run::get_run(&vault_path, &run_id)?;
    let Some(choice) = run.pending_choice.clone() else {
        // Already answered, or answered in another window. Not an error.
        return Ok(());
    };

    // Written into the transcript, not only cleared. "Syn stopped, and the
    // user said this one" is the whole record of a decision somebody made, and
    // a run that quietly resumed with no trace of being asked would be a run
    // nobody can audit.
    let named = choice
        .candidates
        .iter()
        .find(|c| c.id == node_id)
        .map(|c| c.title.clone())
        .unwrap_or_else(|| node_id.clone());
    run.note(
        run.spent.iterations,
        format!("Asked which of {}; the user said \"{named}\".", choice.candidates.len()),
    );

    run.pending_choice = None;
    crate::syn::run::save_run(&vault_path, &run)?;
    Ok(())
}

/// Answer the question a run stopped on.
///
/// Records the answer, clears the question, and says whether there was one —
/// `false` means it had already been answered, here or in another window.
///
/// # Why the answer has to travel
///
/// The run that asked has stopped for good; the work carries on as a new run.
/// So `Always` reaches it through the ledger, `Never` reaches it through the
/// ledger, and `Once` — which the ledger deliberately does not record — would
/// reach nothing at all. It was, until this, a button that changed nothing:
/// the card went away, no permission was granted, and the next attempt asked
/// the identical question.
///
/// `consent::allow_once` holds it in memory instead, keyed to this
/// conversation and spent the first time it is used.
///
/// # And why answering carries on
///
/// The earlier shape stopped here and left it to the user to say *go on*, so
/// that work would not restart while somebody was still reading why it
/// stopped. That reasoning is right about `syn_answer_choice`, where the answer
/// is a fact the next message has to carry. It is wrong here. Pressing **Just
/// this once** *is* saying go on, and asking somebody to then say it again in
/// words is asking the same question twice — which is what it felt like: a
/// question, an answer, and silence.
///
/// The carrying-on is the caller's, not this command's: it holds the stream and
/// the conversation. See `MessagesApp`.
#[tauri::command]
pub async fn syn_answer_consent(
    vault_path: String,
    run_id: String,
    answer: crate::syn::consent::Answer,
) -> Result<bool, AppError> {
    let mut run = crate::syn::run::get_run(&vault_path, &run_id)?;
    let Some(ask) = run.pending_consent.clone() else {
        // Already answered, or answered in another window. Not an error.
        return Ok(false);
    };

    crate::syn::consent::record(&vault_path, &ask.capability, answer, chrono::Utc::now())?;
    if answer == crate::syn::consent::Answer::Once {
        if let Some(conversation_id) = run.conversation_id.as_deref() {
            crate::syn::consent::allow_until_done(conversation_id, &ask.capability);
        }
    }
    crate::syn::audit::record_best_effort(
        &vault_path,
        &run_id,
        &ask.tool,
        &ask.capability,
        match answer {
            crate::syn::consent::Answer::Never => crate::syn::audit::Outcome::Refused,
            _ => crate::syn::audit::Outcome::Allowed,
        },
    );

    // Written into the transcript, not only cleared — the same reason as
    // `syn_answer_choice`. A run that was stopped, answered and carried on with
    // no record of being asked is a run nobody can audit afterwards.
    run.note(
        run.spent.iterations,
        format!(
            "Asked permission to {}; the user said {}.",
            ask.about,
            match answer {
                crate::syn::consent::Answer::Once => "just this once",
                crate::syn::consent::Answer::Always => "always",
                crate::syn::consent::Answer::Never => "never",
            }
        ),
    );

    run.pending_consent = None;
    crate::syn::run::save_run(&vault_path, &run)?;
    Ok(true)
}

/// Open a page for the person — beside the conversation, or in their browser.
///
/// The one door for *following a link*: a source chip under an answer, a link
/// inside an answer, a link in a note. They all look the same to whoever clicks
/// them, so they had better behave the same.
///
/// # Why the fallback lives here and not on the screen
///
/// Because there are two reasons the pane can say no and they deserve opposite
/// answers, and only this side knows which one happened.
///
/// **There is no room, or no pane on this platform.** A phone has no
/// `add_child` at all, and a window narrower than a conversation plus a browser
/// gets no pane by design. Neither is a decision about the *address*, so the
/// page goes to the browser the person already has. That is a layout answer.
///
/// **The address is refused.** `guard` is what keeps this app from being talked
/// into fetching `127.0.0.1` and the rest of the local network, and a link in
/// an answer is written by a model reading pages off the internet. Handing a
/// refused address to the browser holding every cookie the person owns would be
/// worse than the thing the guard was written to stop — so a refusal is a
/// refusal, and it is not quietly redirected.
///
/// A screen with its own fallback could not tell those apart, and would have
/// turned the second into the first.
#[tauri::command]
pub async fn syn_open_page(app: tauri::AppHandle, url: String) -> Result<f64, AppError> {
    crate::syn::browser::guard(&url)?;

    #[cfg(desktop)]
    {
        use tauri::Manager;
        let nonce = uuid::Uuid::new_v4().to_string();
        {
            let waiting = app.state::<crate::syn::browser::Waiting>();
            let mut pending: std::sync::MutexGuard<'_, crate::syn::browser::Pending> =
                waiting.lock().unwrap_or_else(|e| e.into_inner());
            pending.nonce = nonce.clone();
            pending.reply = None;
            pending.loaded = false;
        }
        // They clicked it, so the pane is theirs: it stays until they close it,
        // and the end of a run does not take it away.
        crate::syn::pane::opened_by_the_person(true);

        match crate::syn::pane::open(&app, &url, &nonce) {
            Ok(share) => {
                // Theirs, so it takes the keyboard and the pointer — see
                // `pane::focus_it` for what a window does with mouse-moved
                // events, and why a browser that never has the focus shows an
                // arrow over every link in it.
                crate::syn::pane::focus_it(&app);
                return Ok(share);
            }
            Err(e) => log::info!("[Syn] No pane for {url} ({e}); handing it to the browser"),
        }
    }

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::General(format!("Could not open {url}: {e}")))?;
    Ok(0.0)
}

/// Open the browsing pane on a page, inside the main window.
///
/// Called from the globe in the left rail and from the address bar above the
/// pane. Syn does not come through here — `browser::visit` calls `pane::open`
/// directly, because the two openings differ in exactly one thing and it
/// matters: **whose the pane is**. Pressed the globe and it is the person's, so
/// it stays until they close it; opened to look something up and it belongs to
/// the run. See `pane::syn_may_close_it`.
///
/// The question this was originally written to answer — does the layout hold
/// when the window is resized — was answered, and not the way it was asked.
/// The app's own webview cannot be moved on macOS at all: `set_bounds` returns
/// `Ok` and does nothing. The app draws itself narrower instead, and only the
/// pane is placed. See `syn::pane`.
#[tauri::command]
pub async fn syn_pane_open(app: tauri::AppHandle, url: String) -> Result<f64, AppError> {
    #[cfg(desktop)]
    {
        // A nonce per opening, exactly as `browser::visit` does: without one an
        // advert in an iframe could answer first and hand Syn a page nobody
        // asked for.
        use tauri::Manager;
        let nonce = uuid::Uuid::new_v4().to_string();
        {
            let waiting = app.state::<crate::syn::browser::Waiting>();
            let mut pending: std::sync::MutexGuard<'_, crate::syn::browser::Pending> =
                waiting.lock().unwrap_or_else(|e| e.into_inner());
            pending.nonce = nonce.clone();
            pending.reply = None;
            pending.loaded = false;
        }
        // The fraction of the window the pane takes. The app draws itself
        // narrower by exactly that much — see `syn::pane` for why the app's own
        // webview is not moved instead.
        // Pressed the globe, so it is theirs: it stays until they close it, and
        // the end of a run does not take it away. See `pane::syn_may_close_it`.
        crate::syn::pane::opened_by_the_person(true);
        let share = crate::syn::pane::open(&app, &url, &nonce)?;
        // As above: opened by hand, so it gets the focus a browser expects to
        // have. `pane::focus_it` says what that costs and what it buys.
        crate::syn::pane::focus_it(&app);
        Ok(share)
    }
    #[cfg(mobile)]
    {
        let _ = (app, url);
        Err(AppError::General(
            "The browsing pane is a desktop thing; `add_child` does not exist on mobile".into(),
        ))
    }
}

/// Drag the edge between the conversation and the browsing pane.
///
/// Takes the share the pointer is asking for and returns what the window could
/// actually give — the floors live in `syn::pane::layout`, so the screen never
/// has to hold a second copy of them.
#[tauri::command]
pub async fn syn_pane_resize(app: tauri::AppHandle, share: f64) -> Result<f64, AppError> {
    #[cfg(desktop)]
    {
        crate::syn::pane::drag_to(&app, share)
    }
    #[cfg(mobile)]
    {
        let _ = (app, share);
        Ok(0.0)
    }
}

/// Say how much of the app's width is furniture rather than conversation.
///
/// The icon rail, plus whichever mini-app sidebar is showing, plus nothing
/// else. `pane::layout` adds what a conversation needs and clamps the edge
/// against the sum.
///
/// # Why this crosses at all
///
/// Because the floor was measuring the app's whole webview and calling it the
/// conversation. Inside that webview sit sixty-four pixels of rail and a thread
/// list somebody can pull anywhere between 240 and 560, so a floor of 320 left
/// the conversation with less than nothing — and the pane could be dragged
/// clean over it, all the way to the sidebar.
///
/// How wide that furniture is right now is a fact only the screen has. The
/// policy stays in `pane`: this carries the fact to it, and does not carry an
/// opinion.
///
/// Re-arranging afterwards is the point rather than a side effect: a sidebar
/// pulled wider while the pane is open has to push the pane, or the two overlap
/// again by a different route.
#[tauri::command]
pub async fn syn_pane_room(app: tauri::AppHandle, chrome: u32) -> Result<f64, AppError> {
    crate::syn::pane::the_app_needs_room_for(chrome);

    #[cfg(desktop)]
    {
        Ok(crate::syn::pane::rearrange(&app))
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(0.0)
    }
}

/// What the browsing pane is showing, for the address bar to draw.
///
/// Asked once when the app starts, because the pane outlives a reload of the
/// front end: everything after that arrives as `syn-pane-page`. Without this
/// the bar would come back blank beside a pane still showing a page.
#[tauri::command]
pub async fn syn_pane_page(
    app: tauri::AppHandle,
) -> Result<Option<crate::syn::pane::Showing>, AppError> {
    Ok(crate::syn::pane::showing(&app))
}

/// The way back, and the way forward again.
///
/// `history.back()` in the page rather than a runtime call, because there is no
/// runtime call: neither Tauri nor wry exposes a webview's history. It is the
/// same thing a browser's own button does, and the navigation it causes goes
/// through `may_go_to` like every other.
#[tauri::command]
pub async fn syn_pane_back(app: tauri::AppHandle) -> Result<(), AppError> {
    step_history(&app, "back")
}

/// Forward again, having gone back.
#[tauri::command]
pub async fn syn_pane_forward(app: tauri::AppHandle) -> Result<(), AppError> {
    step_history(&app, "forward")
}

/// One step through the pane's history, in either direction.
///
/// `direction` is never user text — the two commands above pass a literal — so
/// there is nothing here for a page to steer. Written as one function anyway,
/// because two copies of an `eval` differing by one word is how the wrong word
/// gets into one of them.
fn step_history(app: &tauri::AppHandle, direction: &str) -> Result<(), AppError> {
    #[cfg(desktop)]
    {
        use tauri::Manager;
        let pane = app
            .get_webview(crate::syn::pane::PANE)
            .ok_or_else(|| AppError::General("There is no browsing pane".into()))?;
        pane.eval(format!("history.{direction}()"))
            .map_err(|e| AppError::General(format!("Could not go {direction}: {e}")))
    }
    #[cfg(mobile)]
    {
        let _ = (app, direction);
        Ok(())
    }
}

/// Put the browsing pane away and give the app its window back.
#[tauri::command]
pub async fn syn_pane_close(app: tauri::AppHandle) -> Result<(), AppError> {
    #[cfg(desktop)]
    {
        crate::syn::pane::close(&app)
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(())
    }
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

/// What a step actually returned, when its preview was not the whole of it.
///
/// `None` for the ordinary step, whose preview *is* the whole of it — the panel
/// then shows what it already has and offers nothing to expand.
///
/// # Why this is a command and not a bigger run file
///
/// `list_runs` parses every run in the directory, which is why there is a
/// retention cap at all. A page read of twenty-four thousand characters in each
/// of five steps would make opening the list slow, always, for the sake of
/// something almost nobody opens — so the whole results live in
/// `runs/results/{id}.json` and are fetched one at a time.
#[tauri::command]
pub async fn syn_run_result(
    vault_path: String,
    run_id: String,
    step: u32,
) -> Result<Option<String>, AppError> {
    crate::syn::run::load_result(&vault_path, &run_id, step)
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
/// Every skill in the vault, enabled or not.
///
/// Both, unlike the index the model is given. The screen is where a person
/// turns one on, and they cannot turn on something they cannot see.
#[tauri::command]
pub async fn syn_list_skills(
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<Vec<crate::syn::skill::Skill>, AppError> {
    let db = state
        .lock()
        .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
    crate::syn::skill::all(&db)
}

/// Which skills have actually been opened, and when.
///
/// The one number that says whether any of this works. A skill can be enabled,
/// indexed, well written and never once reached for, and nothing else in the
/// app would say so — the model deciding to skip it is a decision with no
/// trace. Memory spent weeks in exactly that state.
#[tauri::command]
pub async fn syn_skill_usage(
    vault_path: String,
) -> Result<Vec<crate::syn::skill::Usage>, AppError> {
    let runs = crate::syn::run::load_all(&vault_path)?;
    Ok(crate::syn::skill::usage(&runs))
}

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
///
/// `focus` is the screen to preview against, and the panel passes what the user
/// is actually looking at. Without it, the one screen that says what Syn is told
/// would be the one screen where the on-screen section is invisible — which is
/// the failure this command exists to prevent.
#[tauri::command]
pub async fn syn_preview_prompt(
    vault_path: String,
    message: Option<String>,
    focus: Option<crate::syn::focus::Focus>,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<PromptPreview, AppError> {
    let settings = settings_for(&vault_path);

    // The preview has to show the memory and skill sections too, or the one
    // screen that says what Syn is told would be the one place they are not
    // visible.
    let (remembered, skill_index) = {
        let db = state
            .lock()
            .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
        (
            crate::syn::memory::all(&db)
                .map(|m| {
                    crate::syn::memory::memory_block(&m, crate::syn::memory::MEMORY_BUDGET_CHARS)
                })
                .unwrap_or(None),
            crate::syn::skill::all(&db)
                .map(|s| {
                    crate::syn::skill::index_block(&s, crate::syn::skill::INDEX_BUDGET_CHARS)
                })
                .unwrap_or(None),
        )
    };

    let context = match message.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(message) if settings.rag_enabled => {
            let config = RagConfig {
                enabled: true,
                max_context_chars: settings.max_context_chars,
                include_finance: settings.include_finance,
                include_feeds: settings.include_feeds,
                graph_expansion_depth: settings.graph_expansion_depth,
                };
            let db = state
                .lock()
                .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
            let retrieved = rag::retrieve_context(&db, message, &[], &config)?;
            rag::format_context(&retrieved)
        }
        _ => String::new(),
    };

    let standing = standing_instructions(&vault_path, &settings);
    Ok(PromptPlan::for_chat(ChatPrompt {
        context: &context,
        custom: standing.as_deref(),
        skills: skill_index.as_deref(),
        memory: remembered.as_deref(),
        focus: focus.as_ref(), thread: None, counted: None,
        budget_chars: DEFAULT_BUDGET_CHARS,
    })
    .into())
}

// ═══════════════════════════════════════════════════════════════
//  THREADS — the work that is open between the two of them
// ═══════════════════════════════════════════════════════════════
//
// Three commands and no tools. A thread is an ordinary node, so `query_nodes`,
// `get_node` and `update_node` already reach it and adding tools that repeat
// them would cost tokens on every turn of every conversation for nothing.
//
// What is *not* ordinary is starting one, and that is deliberately a person's
// job rather than the model's. A tool the model has to think of calling is a
// tool that does not get called — `recall` went unused across fifteen real
// runs. So: a button calls this, and from then on the thread reaches the model
// by riding in the prompt. See `syn/thread.rs`.

/// Every thread in the vault, most recently moved first.
#[tauri::command]
pub async fn syn_list_threads(
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<Vec<crate::syn::thread::Thread>, AppError> {
    let db = state
        .lock()
        .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;
    crate::syn::thread::all(&db)
}

/// Start a thread, and answer with the node it became.
///
/// The body is the three questions work always has rather than an empty file,
/// because an empty document is one nobody writes in and both of them are meant
/// to.
#[tauri::command]
pub async fn syn_open_thread(
    app: tauri::AppHandle,
    vault_path: String,
    title: String,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<String, AppError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::General("A thread needs a name".into()));
    }

    // Through `execute_tool` rather than a write of its own. That is the path
    // every node Syn creates already takes — free path, frontmatter, index,
    // and the `node:created` event the open windows listen for — and a second
    // implementation of it is a second thing to keep correct.
    let ctx = crate::syn::tools::ToolContext {
        db: state.inner(),
        vault_path: &vault_path,
        app: &app,
        run_id: None,
    };

    let result = crate::syn::tools::execute_tool(
        &ctx,
        "create_node",
        &serde_json::json!({
            "node_type": crate::syn::thread::THREAD_TYPE,
            "title": title,
            "content": crate::syn::thread::starting_body(),
            "properties": crate::syn::thread::frontmatter(
                crate::syn::thread::State::Mine,
                None,
            ),
        }),
    )?;

    serde_json::from_str::<serde_json::Value>(&result)
        .ok()
        .and_then(|v| v.get("node_id").and_then(|id| id.as_str()).map(str::to_string))
        .ok_or_else(|| AppError::General(format!("The thread was not created: {result}")))
}

/// Move a thread: whose turn it is, and what it is waiting for.
///
/// A command rather than leaving it to `update_node`, because `state` is the
/// one field with a closed set of values and the one a typo turns into
/// `resting` silently. Everything else about a thread is edited as the node it
/// is.
#[tauri::command]
pub async fn syn_move_thread(
    app: tauri::AppHandle,
    vault_path: String,
    id: String,
    thread_state: String,
    waiting_for: Option<String>,
    state: tauri::State<'_, crate::db::DbState>,
) -> Result<(), AppError> {
    let parsed = crate::syn::thread::State::parse(&thread_state);
    if parsed.as_str() != thread_state.trim().to_lowercase() {
        return Err(AppError::General(format!(
            "`{thread_state}` is not a state a thread has"
        )));
    }

    let ctx = crate::syn::tools::ToolContext {
        db: state.inner(),
        vault_path: &vault_path,
        app: &app,
        run_id: None,
    };
    crate::syn::tools::execute_tool(
        &ctx,
        "update_node",
        &serde_json::json!({
            "node_id": id,
            "properties": crate::syn::thread::frontmatter(parsed, waiting_for.as_deref()),
        }),
    )?;
    Ok(())
}

/// How often Syn was standing on something, across the runs still on disk.
///
/// The one number `syn::footing` exists to produce. Collected since Nhát 5 and
/// shown by nothing — which is the fourth time this codebase has measured
/// something and left it where nobody would see it. See `footing::Tally`.
#[tauri::command]
pub async fn syn_footing_tally(
    vault_path: String,
) -> Result<crate::syn::footing::Tally, AppError> {
    let runs = crate::syn::run::load_all(&vault_path)?;
    Ok(crate::syn::footing::tally(&runs))
}

/// How often a thread was in front of Syn, and how often it wrote back.
///
/// Read off the run transcripts, which already record every tool call. Nothing
/// new is written to say this — the one field added was `Run::thread`, because
/// the prompt that carried the thread is rebuilt each turn and kept nowhere.
#[tauri::command]
pub async fn syn_thread_usage(
    vault_path: String,
) -> Result<crate::syn::thread::Stats, AppError> {
    Ok(crate::syn::thread::usage(&crate::syn::run::load_all(&vault_path)?))
}

/// What the user has told Syn to always do, as they wrote it.
///
/// The whole file, not the part that fits the prompt: somebody editing their
/// own instructions should see all of them. `syn::instructions::block` is what
/// decides how much is sent, and it says so when it cuts.
#[tauri::command]
pub async fn syn_get_instructions(vault_path: String) -> Result<String, AppError> {
    let settings = settings_for(&vault_path);
    if let Some(from_settings) = settings.custom_system_prompt.as_deref() {
        crate::syn::instructions::migrate(&vault_path, from_settings);
    }
    Ok(crate::syn::instructions::load(&vault_path)
        .or(settings.custom_system_prompt)
        .unwrap_or_default())
}

/// Write them. An empty body removes the file.
///
/// The setting is cleared at the same time, so that what is on screen and what
/// reaches the model cannot come from two places.
#[tauri::command]
pub async fn syn_save_instructions(vault_path: String, body: String) -> Result<(), AppError> {
    crate::syn::instructions::save(&vault_path, &body)
        .map_err(|e| AppError::General(format!("Could not write {}: {e}", crate::syn::instructions::FILE)))?;

    let mut settings = settings_for(&vault_path);
    if settings.custom_system_prompt.is_some() {
        settings.custom_system_prompt = None;
        crate::syn::settings::save_settings(&vault_path, &settings)?;
    }
    Ok(())
}

/// The four-question draft, for the editor to offer on an empty file.
///
/// A command rather than a copy of the text in TypeScript, for the reason every
/// other shared literal in this app is read across the boundary rather than
/// duplicated: two copies of a contract drift, and the half the user reads
/// would not be the half Syn is held to.
///
/// It only hands the text over. Nothing here writes it — the file appears when
/// the user saves, and until then they have agreed to nothing.
#[tauri::command]
pub async fn syn_instructions_template() -> Result<String, AppError> {
    Ok(crate::syn::instructions::TEMPLATE.to_string())
}

/// Where the file is, so the interface can say it.
#[tauri::command]
pub async fn syn_instructions_path(vault_path: String) -> Result<String, AppError> {
    Ok(crate::syn::instructions::path(&vault_path)
        .to_string_lossy()
        .to_string())
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
