use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub message_type: String, // e.g., "system"
    pub subtype: String,      // e.g., "task_due", "event_upcoming"
    pub timestamp: String,
    pub sender: ChatSender,
    pub content: ChatContent,
    pub read_receipt: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatSender {
    pub id: String,
    pub name: String,
    pub role: String, // e.g., "bot", "user"
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatContent {
    pub title: String,
    pub text: String,
    pub metadata: Value,
}
