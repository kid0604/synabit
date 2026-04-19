use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileItem {
    pub id: String,
    pub name: String,
    pub extension: String,
    pub size_mb: f64,
    pub source_folder: String,
    pub date_modified: String,
    /// Note: despite the name, this field contains a relative path.
    pub path: String,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct FileManagerSettings {
    pub tracked_sources: Vec<String>,
}
