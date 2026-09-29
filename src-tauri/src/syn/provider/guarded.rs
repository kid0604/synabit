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
        // The arguments of calls the model made, echoed back to it: a value
        // it copied out of a note into a call goes back guarded too.
        for (j, call) in m.tool_calls.iter().flatten().enumerate() {
            let mut arguments = call.function.arguments.clone();
            if redact_strings(&mut arguments) > 0 {
                let all = out.get_or_insert_with(|| messages.to_vec());
                if let Some(calls) = all[i].tool_calls.as_mut() {
                    calls[j].function.arguments = arguments;
                }
            }
        }
    }
    if let Some(all) = &out {
        let n = all.iter().zip(messages).filter(|(a, b)| a.content != b.content || a.tool_calls.as_ref().map(|c| serde_json::to_string(c).ok()) != b.tool_calls.as_ref().map(|c| serde_json::to_string(c).ok())).count();
        log::info!("[Safe] hid what looked like a secret in {n} message(s) before sending them to the model");
    }
    out
}

/// Every string in `value`, guarded. Returns how many secrets were hidden.
fn redact_strings(value: &mut serde_json::Value) -> usize {
    match value {
        serde_json::Value::String(s) => {
            let (text, hidden) = crate::safe::bridge::redact(s);
            if hidden > 0 {
                *s = text;
            }
            hidden
        }
        serde_json::Value::Array(items) => items.iter_mut().map(redact_strings).sum(),
        serde_json::Value::Object(map) => map.values_mut().map(redact_strings).sum(),
        _ => 0,
    }
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
    fn a_key_in_a_tool_calls_arguments_is_hidden_too() {
        let mut asked = msg("");
        asked.tool_calls = Some(vec![crate::models::syn::ToolCall {
            id: Some("c1".into()),
            function: crate::models::syn::ToolCallFunction {
                name: "connector__x__post".into(),
                arguments: serde_json::json!({ "body": { "text": "key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123" }, "auth": "{{safe:x}}" }),
            },
            thought_signature: None,
        }]);
        let cleaned = clean(&[asked]).expect("something was hidden");
        let args = cleaned[0].tool_calls.as_ref().unwrap()[0].function.arguments.to_string();
        assert!(!args.contains("sk-ant-api03"), "{args}");
        assert!(args.contains("{{safe:x}}"), "a placeholder is not a secret: {args}");
    }

    #[test]
    fn nothing_to_hide_costs_no_copy() {
        assert!(clean(&[msg("plain words"), msg("{{safe:github-token}} is a placeholder, not a secret")]).is_none());
    }
}
