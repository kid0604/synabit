use crate::error::AppError;
use crate::models::syn::{
    ModelInfo, ProviderStatus, RagConfig, SynChatRequest, SynConversation, SynConversationFull,
    SynMessage, SynProvider, SynSettings,
};
use crate::syn::provider::{ollama::OllamaProvider, openai::OpenAiCompatProvider, ChatProvider};
use crate::syn::{conversation, engine::SynEngine, rag};

/// Build the provider the vault's settings ask for.
///
/// The API key is fetched here, from the keychain, rather than read out of
/// `settings` — it is never in `settings`, on purpose. See the doc comment on
/// `SynSettings`.
fn provider_for(app: &tauri::AppHandle, settings: &SynSettings) -> Box<dyn ChatProvider> {
    match settings.provider {
        SynProvider::Ollama => Box::new(OllamaProvider::new(&settings.ollama_url)),
        SynProvider::OpenAiCompat => Box::new(OpenAiCompatProvider::new(
            &settings.openai_base_url,
            crate::secrets::SecretManager::get_syn_api_key(
                Some(app),
                SynProvider::OpenAiCompat.key_slot(),
            ),
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
    provider_for(&app, &settings).check_status().await
}

/// List the models the configured provider will accept.
#[tauri::command]
pub async fn syn_list_models(
    app: tauri::AppHandle,
    vault_path: String,
) -> Result<Vec<ModelInfo>, AppError> {
    let settings = settings_for(&vault_path);
    provider_for(&app, &settings).list_models().await
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

    let (retrieval, system_prompt) = if settings.rag_enabled {
        // RAG enabled — retrieve context from vault (DB lock is scoped)
        let (retrieval_result, sys_prompt) = {
            let db = state
                .lock()
                .map_err(|e| AppError::General(format!("DB lock error: {}", e)))?;

            let retrieval_result =
                rag::retrieve_context(&db, &request.message, &conv.messages, &config)?;

            let context_str = rag::format_context(&retrieval_result);
            let sys_prompt = rag::build_system_prompt(&context_str, &config.personality);

            (retrieval_result, sys_prompt)
        }; // DB lock is dropped here — safe to call async engine below

        (retrieval_result, sys_prompt)
    } else {
        // RAG disabled — build a basic system prompt with personality but no vault context
        let sys_prompt = rag::build_system_prompt("", &settings.personality);
        let empty_retrieval = crate::models::syn::RetrievalResult {
            context_chunks: Vec::new(),
            total_tokens_estimate: 0,
            sources: Vec::new(),
        };
        (empty_retrieval, sys_prompt)
    };

    // 5. Handle custom system prompt — prepend if set
    let final_system_prompt = if let Some(ref custom) = settings.custom_system_prompt {
        if custom.is_empty() {
            system_prompt
        } else {
            format!("{}\n\n{}", custom, system_prompt)
        }
    } else {
        system_prompt
    };

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

    // 7. Get tool definitions for function calling
    let tool_defs = crate::syn::tools::get_tool_definitions();

    // Use settings temperature as default, allow per-request override
    let temperature = request.temperature.or(Some(settings.temperature));

    // 8. Run the tool-calling loop against whichever provider is configured
    let engine = SynEngine::new(provider_for(&app, &settings));
    let assistant_message_id = uuid::Uuid::new_v4().to_string();

    let mut assistant_message = engine
        .send_message_with_tools(
            &app,
            &request.conversation_id,
            &assistant_message_id,
            &messages_for_llm,
            &model,
            temperature,
            &tool_defs,
            state.inner(),
            &vault_path,
            settings.max_tool_iterations,
            settings.num_ctx,
            settings.max_history_messages,
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
    conv.meta.model = Some(model);
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

    // Return the assistant message
    Ok(assistant_message)
}

/// Signal the engine to stop the current generation.
#[tauri::command]
pub async fn syn_stop_generation(conversation_id: Option<String>) -> Result<(), AppError> {
    SynEngine::stop_generation(conversation_id.as_deref());
    Ok(())
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
