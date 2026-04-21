use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileMetadata {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size: i64, // in bytes
    pub created_at: String,
    pub modified_at: String,
    pub tags: Vec<String>,
    pub source_type: String, // e.g., "local"
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileSource {
    pub id: String,
    pub path: String,
    pub name: String,
}

// Dummy structs for compatibility until frontend is fully refactored
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileItem {
    pub id: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct FileManagerSettings {
    pub tracked_sources: Vec<String>,
}
