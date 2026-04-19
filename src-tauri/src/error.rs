use serde::Serialize;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Walkdir error: {0}")]
    WalkDir(#[from] walkdir::Error),

    #[error("Path not found or invalid: {0}")]
    InvalidPath(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("General application error: {0}")]
    General(String),
}

// Convert AppError into a structure that Tauri can serialize and send to JS.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct ErrorDetail {
            code: String,
            message: String,
        }

        let (code, message) = match self {
            AppError::Io(err) => ("IO_ERROR".to_string(), err.to_string()),
            AppError::Json(err) => ("JSON_ERROR".to_string(), err.to_string()),
            AppError::WalkDir(err) => ("WALKDIR_ERROR".to_string(), err.to_string()),
            AppError::InvalidPath(msg) => ("INVALID_PATH".to_string(), msg.clone()),
            AppError::AuthFailed(msg) => ("AUTH_FAILED".to_string(), msg.clone()),
            AppError::SyncError(msg) => ("SYNC_ERROR".to_string(), msg.clone()),
            AppError::General(msg) => ("GENERAL_ERROR".to_string(), msg.clone()),
        };

        let detail = ErrorDetail { code, message };
        detail.serialize(serializer)
    }
}

// Convenience alias for our Result type
pub type AppResult<T> = Result<T, AppError>;
