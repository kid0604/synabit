use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct QuickCapMetadata {
    pub id: String,
    pub date: String,
    pub content: String,
    pub path: String,
}
