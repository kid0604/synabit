use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ChecklistItem {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct TaskFrontMatter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub is_transferred: bool,
    #[serde(default)]
    pub transferred_to: String,
    #[serde(default)]
    pub track_progress: bool,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub due_date: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub source_link: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub checklist: Vec<ChecklistItem>,
    #[serde(default)]
    pub completed_at: String,
    #[serde(flatten)]
    pub custom_fields: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TaskMetadata {
    pub id: String,
    pub title: String,
    pub status: String,
    pub is_transferred: bool,
    pub transferred_to: String,
    pub track_progress: bool,
    pub project_id: String,
    pub priority: String,
    pub start_date: String,
    pub due_date: String,
    pub comment: String,
    pub source_link: String,
    pub tags: Vec<String>,
    pub checklist: Vec<ChecklistItem>,
    pub content: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: String,
    pub custom_fields: HashMap<String, serde_json::Value>,
}
