use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeMetadata {
    pub id: String,        // Relative path in Vault (e.g., "Projects/Synabit.md")
    pub node_type: String, // e.g., "note", "task", "project", "habit", "contact"
    pub title: String,     // Extracted from Frontmatter or filename
    pub content: String,   // Raw content (Markdown or JSON/Canvas string)
    pub properties: Value, // Parsed YAML Frontmatter or JSON metadata
    pub created_at: String,
    pub updated_at: String,
    pub timestamp: i64, // Used for cache invalidation
    #[serde(skip)]
    pub blocks: Option<Vec<(String, String)>>, // Block-level contents
}
