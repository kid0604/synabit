use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct EventFrontMatter {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub event_date: String,
    #[serde(default)]
    pub event_time: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventMetadata {
    pub id: String,
    pub title: String,
    pub event_date: String,
    pub event_time: String,
    pub location: String,
    pub tags: Vec<String>,
    pub content: String,
    pub path: String,
    pub created_at: String,
}
