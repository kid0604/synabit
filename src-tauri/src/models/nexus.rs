use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct NexusItem {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub preview: String,
    pub tags: Vec<String>,
    pub date: String,
    pub path: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub status: Option<String>,
}
