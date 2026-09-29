//! Every provider, with the leak guard in front of it.
//!
//! There is no single place a request leaves by — the engine, reflection,
//! memory, the timeline reader each call a provider — but every provider is
//! made by `for_settings`, and that returns this wrapper. So everything sent to
//! any model passes `safe::bridge::redact` first: the values of an open Safe,
//! and the shapes of known keys, replaced by a marker. A note with a password
//! pasted into it can be read by Syn; the password does not go with it.
//!
//! Local models too. Ollama runs on this machine, but what it is sent is also
//! what the conversation keeps, in a file in the vault.

use async_trait::async_trait;

use super::{ChatMessage, ChatProvider, ChatReply, ChatRequest, ModelInfo, ProviderStatus, StreamSink};
use crate::error::AppResult;
use crate::models::syn::SynProvider;

pub struct Guarded(pub Box<dyn ChatProvider>);

/// The messages with every secret hidden, or `None` when there was nothing to
/// hide — the common case, which then costs no copy.
pub fn clean(messages: &[ChatMessage]) -> Option<Vec<ChatMessage>> {
    let mut out: Option<Vec<ChatMessage>> = None;
    for (i, m) in messages.iter().enumerate() {
        let (text, hidden) = crate::safe::bridge::redact(&m.content);
        if hidden > 0 {
            let all = out.get_or_insert_with(|| messages.to_vec());
            all[i].content = text;
        }
    }
    if let Some(all) = &out {
        let n = all.iter().zip(messages).filter(|(a, b)| a.content != b.content).count();
        log::info!("[Safe] hid what looked like a secret in {n} message(s) before sending them to the model");
    }
    out
}

fn with<'a>(req: &ChatRequest<'a>, messages: &'a [ChatMessage]) -> ChatRequest<'a> {
    ChatRequest {
        model: req.model,
        messages,
        temperature: req.temperature,
        num_ctx: req.num_ctx,
        tools: req.tools,
        json_schema: req.json_schema,
    }
}

#[async_trait]
impl ChatProvider for Guarded {
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
        match clean(req.messages) {
            Some(cleaned) => self.0.chat(with(&req, &cleaned)).await,
            None => self.0.chat(req).await,
        }
    }

    async fn chat_stoppable(&self, req: ChatRequest<'_>, stop: &(dyn Fn() -> bool + Send + Sync)) -> AppResult<ChatReply> {
        match clean(req.messages) {
            Some(cleaned) => self.0.chat_stoppable(with(&req, &cleaned), stop).await,
            None => self.0.chat_stoppable(req, stop).await,
        }
    }

    async fn chat_streaming(&self, req: ChatRequest<'_>, sink: &StreamSink<'_>) -> AppResult<ChatReply> {
        match clean(req.messages) {
            Some(cleaned) => self.0.chat_streaming(with(&req, &cleaned), sink).await,
            None => self.0.chat_streaming(req, sink).await,
        }
    }

    fn streams_tool_calls(&self) -> bool {
        self.0.streams_tool_calls()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(content: &str) -> ChatMessage {
        ChatMessage { role: "user".into(), content: content.into(), tool_calls: None, tool_call_id: None, images: None }
    }

    #[test]
    fn a_key_in_a_message_is_hidden_before_it_leaves() {
        let messages = vec![msg("hello"), msg("my key is sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123, use it")];
        let cleaned = clean(&messages).expect("something was hidden");
        assert_eq!(cleaned[0].content, "hello");
        assert!(!cleaned[1].content.contains("sk-ant"), "{}", cleaned[1].content);
        assert!(messages[1].content.contains("sk-ant"), "the conversation itself is not rewritten here");
    }

    #[test]
    fn nothing_to_hide_costs_no_copy() {
        assert!(clean(&[msg("plain words"), msg("{{safe:github-token}} is a placeholder, not a secret")]).is_none());
    }
}
