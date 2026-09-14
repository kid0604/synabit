//! The part of the Telegram Bot API this app uses, and nothing more.
//!
//! Written against `reqwest` rather than a bot framework: a handful of methods
//! and plain JSON are not worth a framework's dependency tree.
//!
//! # The token is in every URL
//!
//! The Bot API authenticates by putting the token in the path. `reqwest` puts
//! the URL into its errors, and errors get logged — so every error leaving this
//! module has had its URL taken off first (`without_url`), and nothing here
//! logs a request or a response.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

const BASE: &str = "https://api.telegram.org/bot";

/// How long one `getUpdates` waits for something to arrive.
pub const POLL_SECONDS: u64 = 50;

/// The most the public Bot API lets a bot download.
pub const MAX_DOWNLOAD_BYTES: u64 = 20 * 1024 * 1024;

fn network(e: reqwest::Error) -> ApiError {
    ApiError::Network(e.without_url().to_string())
}

/// Where a file can be downloaded from, as `getFile` answers.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteFile {
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub file_size: Option<u64>,
}

/// What went wrong, in the terms the service has to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Something else is polling with this token — the same bot running on
    /// another computer. Telegram allows one.
    Conflict,
    /// The token is wrong, or was revoked in BotFather.
    Unauthorized,
    /// Telegram refused the request itself. Most often: HTML it could not parse.
    BadRequest(String),
    /// Too many requests. Try again after this many seconds.
    RetryAfter(u64),
    /// Telegram could not be reached, or answered with something unexpected.
    Network(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Conflict => write!(f, "another process is polling with this token"),
            ApiError::Unauthorized => write!(f, "Telegram did not accept the token"),
            ApiError::BadRequest(why) => write!(f, "Telegram refused the request: {why}"),
            ApiError::RetryAfter(seconds) => write!(f, "rate limited for {seconds}s"),
            ApiError::Network(why) => write!(f, "could not reach Telegram: {why}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub id: i64,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

impl User {
    /// A name to show a person, from whatever Telegram has.
    pub fn display_name(&self) -> String {
        let full = match &self.last_name {
            Some(last) => format!("{} {last}", self.first_name),
            None => self.first_name.clone(),
        };
        let full = full.trim().to_string();
        if !full.is_empty() {
            return full;
        }
        self.username.clone().unwrap_or_else(|| self.id.to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Chat {
    pub id: i64,
    /// `private`, `group`, `supergroup` or `channel`.
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Message {
    pub message_id: i64,
    #[serde(default)]
    pub from: Option<User>,
    pub chat: Chat,
    #[serde(default)]
    pub text: Option<String>,
    /// The words under a photo or a file.
    #[serde(default)]
    pub caption: Option<String>,
    /// Who wrote it, when it was forwarded.
    ///
    /// Kept as JSON: an origin comes in four shapes, and all that is wanted out
    /// of any of them is a name. See `gate::origin_name`.
    #[serde(default)]
    pub forward_origin: Option<Value>,
    /// Everything else — a photo, a file, the date. Read only to ask whether
    /// something other than text arrived.
    #[serde(flatten)]
    pub rest: serde_json::Map<String, Value>,
}

impl Message {
    /// Whether it carries something other than words.
    pub fn has_attachment(&self) -> bool {
        [
            "photo", "document", "voice", "audio", "video", "video_note", "sticker", "animation",
        ]
        .iter()
        .any(|kind| self.rest.contains_key(*kind))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub callback_query: Option<CallbackQuery>,
}

/// A button pressed under one of the bot's messages.
#[derive(Debug, Clone, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: User,
    /// The message the button was under.
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub data: Option<String>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    #[serde(default = "Option::default")]
    result: Option<T>,
    #[serde(default)]
    error_code: Option<u16>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parameters: Option<Parameters>,
}

#[derive(Deserialize)]
struct Parameters {
    #[serde(default)]
    retry_after: Option<u64>,
}

/// Turn Telegram's envelope into a result.
fn interpret<T>(envelope: Envelope<T>) -> Result<T, ApiError> {
    if envelope.ok {
        return envelope
            .result
            .ok_or_else(|| ApiError::Network("an answer with no result in it".to_string()));
    }
    let description = envelope.description.unwrap_or_default();
    Err(match envelope.error_code {
        Some(409) => ApiError::Conflict,
        // A malformed token is a 404 on the method rather than a 401.
        Some(401) | Some(404) => ApiError::Unauthorized,
        Some(429) => ApiError::RetryAfter(
            envelope.parameters.and_then(|p| p.retry_after).unwrap_or(5),
        ),
        Some(400) | Some(403) => ApiError::BadRequest(description),
        _ => ApiError::Network(description),
    })
}

/// A bot, as this app talks to it.
#[derive(Clone)]
pub struct Api {
    client: reqwest::Client,
    base: String,
    /// Downloads live under a different path, with the token in it too.
    files: String,
}

impl Api {
    pub fn new(token: &str) -> Result<Self, ApiError> {
        let client = reqwest::Client::builder()
            // Longer than the long poll, or every quiet minute is an error.
            .timeout(Duration::from_secs(POLL_SECONDS + 20))
            .build()
            .map_err(|e| ApiError::Network(e.without_url().to_string()))?;
        Ok(Self {
            client,
            base: format!("{BASE}{}", token.trim()),
            files: format!("https://api.telegram.org/file/bot{}", token.trim()),
        })
    }

    pub async fn get_file(&self, file_id: &str) -> Result<RemoteFile, ApiError> {
        self.call("getFile", json!({ "file_id": file_id })).await
    }

    /// Download what `get_file` pointed at, refusing anything over the limit.
    pub async fn download(&self, file_path: &str) -> Result<Vec<u8>, ApiError> {
        let response = self
            .client
            .get(format!("{}/{file_path}", self.files))
            .send()
            .await
            .map_err(network)?;
        if !response.status().is_success() {
            return Err(ApiError::Network(format!("the download answered {}", response.status())));
        }
        let too_large = || ApiError::BadRequest("larger than a bot may download".to_string());
        if response.content_length().is_some_and(|n| n > MAX_DOWNLOAD_BYTES) {
            return Err(too_large());
        }
        let bytes = response.bytes().await.map_err(network)?;
        if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err(too_large());
        }
        Ok(bytes.to_vec())
    }

    async fn call<T: DeserializeOwned>(&self, method: &str, body: Value) -> Result<T, ApiError> {
        let response = self
            .client
            .post(format!("{}/{method}", self.base))
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.without_url().to_string()))?;
        let status = response.status();
        let envelope: Envelope<T> = response
            .json()
            .await
            .map_err(|e| ApiError::Network(format!("{status}: {}", e.without_url())))?;
        interpret(envelope)
    }

    pub async fn get_me(&self) -> Result<User, ApiError> {
        self.call("getMe", json!({})).await
    }

    /// What has arrived since `offset`, waiting up to `POLL_SECONDS` for it.
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<Update>, ApiError> {
        self.call(
            "getUpdates",
            json!({
                "offset": offset,
                "timeout": POLL_SECONDS,
                "allowed_updates": ["message", "callback_query"],
            }),
        )
        .await
    }

    /// Send one message of HTML.
    ///
    /// If Telegram cannot parse it, the same words go again as plain text: an
    /// answer with its bold missing is better than no answer.
    pub async fn send(&self, chat_id: i64, html: &str) -> Result<(), ApiError> {
        self.send_reply(chat_id, html, None).await
    }

    /// The same, quoting one of the person's messages above it.
    ///
    /// For an answer that arrives after others have: a phone shows which
    /// question it belongs to, and tapping the quote scrolls back to it.
    pub async fn send_reply(&self, chat_id: i64, html: &str, reply_to: Option<i64>) -> Result<(), ApiError> {
        match self.send_message(chat_id, html, true, reply_to).await {
            Err(ApiError::BadRequest(why)) if why.contains("parse") => {
                self.send_message(chat_id, &super::render::plain(html), false, reply_to).await
            }
            Err(ApiError::RetryAfter(seconds)) if seconds <= 30 => {
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                self.send_message(chat_id, html, true, reply_to).await
            }
            other => other,
        }
    }

    async fn send_message(&self, chat_id: i64, text: &str, html: bool, reply_to: Option<i64>) -> Result<(), ApiError> {
        let mut body = json!({
            "chat_id": chat_id,
            "text": text,
            "link_preview_options": { "is_disabled": true },
        });
        if html {
            body["parse_mode"] = json!("HTML");
        }
        if let Some(message_id) = reply_to {
            // A question deleted meanwhile still gets its answer, unquoted.
            body["reply_parameters"] = json!({ "message_id": message_id, "allow_sending_without_reply": true });
        }
        self.call::<Value>("sendMessage", body).await.map(|_| ())
    }

    /// Send a message with buttons under it, one row per `rows` entry.
    ///
    /// Returns the message's id, so it can be edited once a button is pressed.
    pub async fn send_buttons(
        &self,
        chat_id: i64,
        html: &str,
        rows: &[Vec<(String, String)>],
    ) -> Result<i64, ApiError> {
        let keyboard: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|(text, data)| json!({ "text": text, "callback_data": data }))
                    .collect()
            })
            .collect();
        let sent: Message = self
            .call(
                "sendMessage",
                json!({
                    "chat_id": chat_id,
                    "text": html,
                    "parse_mode": "HTML",
                    "link_preview_options": { "is_disabled": true },
                    "reply_markup": { "inline_keyboard": keyboard },
                }),
            )
            .await?;
        Ok(sent.message_id)
    }

    /// Replace a message's text. Its buttons go with the old text.
    pub async fn edit(&self, chat_id: i64, message_id: i64, html: &str) -> Result<(), ApiError> {
        self.call::<Value>(
            "editMessageText",
            json!({ "chat_id": chat_id, "message_id": message_id, "text": html, "parse_mode": "HTML" }),
        )
        .await
        .map(|_| ())
    }

    /// Stop the spinner on a pressed button, with a short note when there is one.
    pub async fn answer_callback(&self, callback_id: &str, text: Option<&str>) -> Result<(), ApiError> {
        let mut body = json!({ "callback_query_id": callback_id });
        if let Some(text) = text {
            body["text"] = json!(text);
        }
        self.call::<Value>("answerCallbackQuery", body).await.map(|_| ())
    }

    /// "typing…" under the bot's name, for about five seconds.
    pub async fn typing(&self, chat_id: i64) -> Result<(), ApiError> {
        self.call::<Value>("sendChatAction", json!({ "chat_id": chat_id, "action": "typing" }))
            .await
            .map(|_| ())
    }

    /// Put one reaction on a message, replacing any the bot left before.
    pub async fn react(&self, chat_id: i64, message_id: i64, emoji: &str) -> Result<(), ApiError> {
        self.call::<Value>(
            "setMessageReaction",
            json!({
                "chat_id": chat_id,
                "message_id": message_id,
                "reaction": [{ "type": "emoji", "emoji": emoji }],
            }),
        )
        .await
        .map(|_| ())
    }

    /// The menu a person sees when they type `/`.
    pub async fn set_commands(&self, commands: &[(&str, &str)]) -> Result<(), ApiError> {
        let commands: Vec<Value> = commands
            .iter()
            .map(|(command, description)| json!({ "command": command, "description": description }))
            .collect();
        self.call::<Value>("setMyCommands", json!({ "commands": commands }))
            .await
            .map(|_| ())
    }

    pub async fn leave(&self, chat_id: i64) -> Result<(), ApiError> {
        self.call::<Value>("leaveChat", json!({ "chat_id": chat_id }))
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(json: &str) -> Envelope<Value> {
        serde_json::from_str(json).expect("an envelope")
    }

    /// The two answers the service stops for are told apart from everything
    /// else, because they need a person and retrying cannot fix them.
    #[test]
    fn the_errors_the_service_acts_on_are_recognised() {
        assert_eq!(
            interpret(envelope(r#"{"ok":false,"error_code":409,"description":"Conflict: terminated by other getUpdates request"}"#)),
            Err(ApiError::Conflict)
        );
        assert_eq!(
            interpret(envelope(r#"{"ok":false,"error_code":401,"description":"Unauthorized"}"#)),
            Err(ApiError::Unauthorized)
        );
        assert_eq!(
            interpret(envelope(r#"{"ok":false,"error_code":404,"description":"Not Found"}"#)),
            Err(ApiError::Unauthorized),
            "a malformed token is a 404"
        );
        assert_eq!(
            interpret(envelope(r#"{"ok":false,"error_code":429,"description":"Too Many Requests","parameters":{"retry_after":7}}"#)),
            Err(ApiError::RetryAfter(7))
        );
        assert!(matches!(
            interpret(envelope(r#"{"ok":false,"error_code":400,"description":"Bad Request: can't parse entities"}"#)),
            Err(ApiError::BadRequest(why)) if why.contains("parse")
        ));
        assert_eq!(interpret(envelope(r#"{"ok":true,"result":true}"#)), Ok(Value::Bool(true)));
    }

    /// A photo is told apart from a message that only has words, and a
    /// forwarded message keeps where it came from.
    #[test]
    fn a_message_says_what_it_carries() {
        let photo: Message = serde_json::from_str(
            r#"{"message_id":1,"chat":{"id":5,"type":"private"},"date":0,"photo":[{"file_id":"x"}],"caption":"hoá đơn"}"#,
        )
        .expect("a message");
        assert!(photo.has_attachment());
        assert!(photo.text.is_none());

        let forwarded: Message = serde_json::from_str(
            r#"{"message_id":2,"chat":{"id":5,"type":"private"},"date":0,"text":"hello","forward_origin":{"type":"hidden_user","sender_user_name":"An","date":0}}"#,
        )
        .expect("a message");
        assert!(!forwarded.has_attachment());
        assert!(forwarded.forward_origin.is_some());
    }

    /// The token must not come back out of an error.
    #[test]
    fn an_api_never_prints_its_token() {
        let api = Api::new("123456:SECRET-token").expect("an api");
        let shown = format!("{}", ApiError::Network("timeout".into()));
        assert!(!shown.contains("SECRET"));
        assert!(api.base.ends_with("123456:SECRET-token"), "the token is where the API expects it");
    }
}
