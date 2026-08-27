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
    pub people: Vec<String>,
    pub source_type: String, // e.g., "local"
    /// What a camera wrote into the picture, when there was one. Read in the
    /// front end from bytes it was already holding — see `src/shared/exif.ts`.
    pub camera: Option<String>,
    pub shot_at: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// A colour mark — a rating without words, for sorting a shoot before you
    /// know what to call any of it.
    pub label: Option<String>,
}

impl FileMetadata {
    /// Convert a NodeMetadata (node_type="file") back into FileMetadata
    pub fn from_node(node: &crate::models::node::NodeMetadata) -> Option<Self> {
        let props = &node.properties;
        Some(FileMetadata {
            id: node.id.clone(),
            // Absent on a node that arrived through sync, which carries no
            // paths by design — see `publish_file_metadata`. The caller listing
            // files fills it in from `file_locations`; a file with no location
            // on this device has no path here and is not something to open.
            path: props
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            filename: node.title.clone(),
            extension: props
                .get("extension")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            size: props.get("size").and_then(|v| v.as_i64()).unwrap_or(0),
            created_at: node.created_at.clone(),
            modified_at: node.updated_at.clone(),
            tags: props
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            people: props
                .get("people")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            source_type: props
                .get("source_type")
                .and_then(|v| v.as_str())
                .unwrap_or("local")
                .to_string(),
            camera: props.get("camera").and_then(|v| v.as_str()).map(String::from),
            shot_at: props.get("shot_at").and_then(|v| v.as_str()).map(String::from),
            width: props.get("width").and_then(|v| v.as_i64()),
            height: props.get("height").and_then(|v| v.as_i64()),
            label: props.get("label").and_then(|v| v.as_str()).map(String::from),
        })
    }

    /// Convert this FileMetadata into a NodeMetadata for storage in the nodes table
    pub fn to_node(&self) -> crate::models::node::NodeMetadata {
        use serde_json::json;
        crate::models::node::NodeMetadata {
            id: self.id.clone(),
            node_type: "file".to_string(),
            title: self.filename.clone(),
            content: String::new(),
            properties: json!({
                "path": self.path,
                "extension": self.extension,
                "size": self.size,
                "source_type": self.source_type,
                "tags": self.tags,
                "people": self.people,
            }),
            created_at: self.created_at.clone(),
            updated_at: self.modified_at.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            blocks: None,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct DuplicateGroup {
    pub filename: String,
    pub extension: String,
    pub size: i64,
    pub count: usize,
    pub files: Vec<FileMetadata>,
    pub wasted_bytes: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct DuplicateReport {
    pub groups: Vec<DuplicateGroup>,
    pub total_groups: usize,
    pub total_duplicate_files: usize,
    pub total_wasted_bytes: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileSource {
    pub id: String,
    pub path: String,
    pub name: String,
}
