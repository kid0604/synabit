use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Serialize, Deserialize, Debug)]
pub struct TagStat {
    pub name: String,
    pub total_count: usize,
    pub distribution: HashMap<String, usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VaultStats {
    pub total_items: usize,
    pub type_distribution: HashMap<String, usize>,
    pub tags: Vec<TagStat>,
}
