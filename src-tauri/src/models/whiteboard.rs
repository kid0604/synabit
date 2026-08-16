use serde::{Deserialize, Serialize};

/// A board, as the frontend asks for it.
///
/// Boards are stored as ordinary nodes; this is the shape the whiteboard
/// commands hand back, kept because the whiteboard UI is written against it.
/// `id` and `path` are the same string — the board's path inside the vault.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WhiteboardMetadata {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub content: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
}
