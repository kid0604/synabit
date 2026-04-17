use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct FrontMatter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NoteMetadata {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub date: String,
    pub tags: Vec<String>,
    pub path: String,
    pub pinned: bool,
    pub content: String,
}
