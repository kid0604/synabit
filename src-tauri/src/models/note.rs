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
    pub timestamp: i64,
    pub tags: Vec<String>,
    pub path: String,
    pub pinned: bool,
    pub content: String,
    #[serde(default)]
    pub is_task: bool,
    #[serde(default)]
    pub is_event: bool,
    #[serde(default)]
    pub has_reminder: bool,
    #[serde(default)]
    pub is_done: bool,
    #[serde(default)]
    pub raw_frontmatter: String,
}
