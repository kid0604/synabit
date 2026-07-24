use crate::error::AppError;
use crate::models::syn::{
    ModelInfo, OllamaStatus, RagConfig, SynChatRequest, SynConversation, SynConversationFull,
    SynMessage, SynSettings,
};
use crate::syn::{conversation, engine::SynEngine, rag};

// ═══════════════════════════════════════════════════════════════
//  OLLAMA STATUS & MODEL MANAGEMENT
// ═══════════════════════════════════════════════════════════════

/// Check if Ollama is running and reachable.
/// Uses the Ollama URL from vault settings (falls back to default if no vault).
#[tauri::command]
pub async fn syn_check_status(vault_path: String) -> Result<OllamaStatus, AppError> {
    let settings = crate::syn::settings::load_settings(&vault_path).unwrap_or_default();
    let engine = SynEngine::with_url(&settings.ollama_url);
    engine.check_status().await
}

/// List all locally available Ollama models.
#[tauri::command]
pub async fn syn_list_models(vault_path: String) -> Result<Vec<ModelInfo>, AppError> {
    let settings = crate::syn::settings::load_settings(&vault_path).unwrap_or_default();
    let engine = SynEngine::with_url(&settings.ollama_url);
    engine.list_models().await
}

/// Pull (download) a model from Ollama's registry.
/// Emits `syn-pull-progress` events during download.
#[tauri::command]
pub async fn syn_pull_model(
    app: tauri::AppHandle,
    vault_path: String,
    model_name: String,
) -> Result<(), AppError> {
    let settings = crate::syn::settings::load_settings(&vault_path).unwrap_or_default();
    let engine = SynEngine::with_url(&settings.ollama_url);
    engine.pull_model(&app, &model_name).await
}

/// Delete a locally stored model from Ollama.
#[tauri::command]
pub async fn syn_delete_model(vault_path: String, model_name: String) -> Result<(), AppError> {
    let settings = crate::syn::settings::load_settings(&vault_path).unwrap_or_default();
    let engine = SynEngine::with_url(&settings.ollama_url);
    engine.delete_model(&model_name).await
}

// ═══════════════════════════════════════════════════════════════
//  SETTINGS & CONFIGURATION
// ═══════════════════════════════════════════════════════════════

/// Get current Syn settings for the vault.
#[tauri::command]
pub async fn syn_get_settings(vault_path: String) -> Result<SynSettings, AppError> {
    crate::syn::settings::load_settings(&vault_path)
}

/// Save Syn settings for the vault.
#[tauri::command]
pub async fn syn_save_settings(
    vault_path: String,
    settings: SynSettings,
) -> Result<(), AppError> {
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

    // Determine which model to use (request override > conversation default > settings default > fallback)
    let model = request
        .model
        .clone()
        .or_else(|| conv.meta.model.clone())
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

            let retrieval_result = rag::retrieve_context(
                &db,
                &request.message,
                &conv.messages,
                &config,
            )?;

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

    // 8. Call Ollama with tool calling loop + final response
    let engine = SynEngine::with_url(&settings.ollama_url);
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
    let used_tools = assistant_message.tool_calls_log.as_ref().is_some_and(|l| !l.is_empty());
    if !retrieval.sources.is_empty() && !used_tools {
        assistant_message.sources = Some(retrieval.sources);
    }

    // 10. Add the assistant response to the conversation
    conv.messages.push(assistant_message.clone());

    // Update conversation model if not set
    if conv.meta.model.is_none() {
        conv.meta.model = Some(model);
    }

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
    SynEngine::cancel_pull();
}

// ═══════════════════════════════════════════════════════════════
//  CONVERSATION CRUD
// ═══════════════════════════════════════════════════════════════

/// List all conversations in the vault (metadata only, sorted by recency).
#[tauri::command]
pub async fn syn_list_conversations(
    vault_path: String,
) -> Result<Vec<SynConversation>, AppError> {
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
