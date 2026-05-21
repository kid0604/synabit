use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct ProjectFrontMatter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub due_date: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub custom_fields: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProjectMetadata {
    pub id: String,
    pub title: String,
    pub status: String,
    pub start_date: String,
    pub due_date: String,
    pub color: String,
    pub tags: Vec<String>,
    pub content: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
    pub custom_fields: HashMap<String, serde_json::Value>,
}
