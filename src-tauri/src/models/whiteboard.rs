use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WhiteboardFrontMatter {
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

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
