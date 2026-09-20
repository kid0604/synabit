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

    #[error("Unsupported capability: {0}")]
    UnsupportedCapability(String),

    /// An attachment is bigger than the pipeline will carry.
    ///
    /// Kept separate from `SyncError` because it is not a failure to retry:
    /// the file will be exactly as large on the next run. Callers skip it and
    /// say so once, rather than reporting it every sync forever.
    #[error("Attachment too large: {0}")]
    AssetTooLarge(String),

    #[error("General application error: {0}")]
    General(String),

    /// A question the engine will not answer, and why.
    ///
    /// Its own variant because it is not a failure: nothing went wrong, the
    /// question could not be asked. And because it carries a **code**, so the
    /// screen can say it in the language the rest of the app is in — see
    /// `crate::refusal`.
    #[error("{0}")]
    Refused(crate::refusal::Refusal),
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
            #[serde(skip_serializing_if = "Vec::is_empty")]
            args: Vec<String>,
        }

        let (code, message) = match self {
            AppError::Io(err) => ("IO_ERROR".to_string(), err.to_string()),
            AppError::Json(err) => ("JSON_ERROR".to_string(), err.to_string()),
            AppError::WalkDir(err) => ("WALKDIR_ERROR".to_string(), err.to_string()),
            AppError::InvalidPath(msg) => ("INVALID_PATH".to_string(), msg.clone()),
            AppError::AuthFailed(msg) => ("AUTH_FAILED".to_string(), msg.clone()),
            AppError::SyncError(msg) => ("SYNC_ERROR".to_string(), msg.clone()),
            AppError::UnsupportedCapability(msg) => {
                ("UNSUPPORTED_CAPABILITY".to_string(), msg.clone())
            }
            AppError::AssetTooLarge(msg) => ("ASSET_TOO_LARGE".to_string(), msg.clone()),
            AppError::General(msg) => ("GENERAL_ERROR".to_string(), msg.clone()),
            AppError::Refused(why) => (format!("REFUSED:{}", why.code()), why.to_string()),
        };

        // A refusal is not an error and is not logged as one: somebody typed a
        // question the engine cannot ask, which is the engine working.
        match self {
            AppError::Refused(why) => log::debug!("Refused [{}]: {}", why.code(), message),
            _ => log::error!("Backend Error [{}]: {}", code, message),
        }

        let detail = ErrorDetail {
            code,
            message,
            // The arguments, so the screen can fill in its own translation of
            // the sentence rather than showing this one.
            args: match self {
                AppError::Refused(why) => why.args().to_vec(),
                _ => Vec::new(),
            },
        };
        detail.serialize(serializer)
    }
}

// Convenience alias for our Result type
pub type AppResult<T> = Result<T, AppError>;

/// Report a best-effort write that failed, and say whether it did.
///
/// Some writes must not be allowed to fail a whole operation. Indexing one file
/// out of ten thousand, recording that a sync finished, marking a key migration
/// done — a failure there is worth knowing about, but aborting the caller over
/// it would do more harm than the failure itself.
///
/// The habit this replaces was `let _ = …`, which achieves the not-aborting
/// part by throwing the error away entirely. That left the database quietly
/// disagreeing with the disk, or a migration flag unset, with nothing anywhere
/// to say why — the first sign was a note that could not be found by searching
/// for it. Same control flow, but the failure leaves a trace.
///
/// `action` is what was being attempted and `subject` is what it was being
/// attempted on, so the log line reads as a sentence.
pub fn logged(action: &str, subject: &str, result: AppResult<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(e) => {
            log::warn!("could not {action} for '{subject}': {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    /// What a refusal looks like on the wire, which is what `shared/refusal.ts`
    /// reads. The two sides agree by this test and by the one in
    /// `crate::refusal`; nothing else makes them.
    #[test]
    fn a_refusal_crosses_with_its_code_and_its_arguments() {
        let refused = AppError::Refused(crate::refusal::Refusal::would_spend(15, 208));
        let wire: serde_json::Value = serde_json::to_value(&refused).expect("it serialises");

        assert_eq!(wire["code"], "REFUSED:would_spend");
        assert_eq!(wire["args"], serde_json::json!(["15", "208"]));
        assert!(
            wire["message"].as_str().expect("a message").contains("208 lines"),
            "the English is still there as a fallback: {wire}"
        );
    }

    /// And anything that is not a refusal keeps the shape it always had, with
    /// no `args` at all — an SQL failure is a bug report, not a sentence.
    #[test]
    fn an_ordinary_error_is_unchanged() {
        let wire: serde_json::Value =
            serde_json::to_value(AppError::General("Query error: disk I/O".into()))
                .expect("it serialises");
        assert_eq!(wire["code"], "GENERAL_ERROR");
        assert_eq!(wire["message"], "Query error: disk I/O");
        assert!(wire.get("args").is_none(), "{wire}");
    }


    /// The contract callers depend on: the answer tells them whether the write
    /// happened, and either way control comes back to them. Several callers
    /// combine these with `&=` across a group of writes, so a failure has to
    /// report itself as `false` without cutting the group short.
    #[test]
    fn a_failure_is_reported_as_false_and_a_success_as_true() {
        assert!(logged("do a thing", "subject", Ok(())));
        assert!(!logged(
            "do a thing",
            "subject",
            Err(AppError::General("nope".into()))
        ));
    }

    #[test]
    fn combining_results_keeps_every_write_attempted() {
        let mut attempted = 0;
        let mut attempt = |result: AppResult<()>| {
            attempted += 1;
            logged("write", "subject", result)
        };

        let mut ok = attempt(Ok(()));
        ok &= attempt(Err(AppError::General("first failure".into())));
        ok &= attempt(Ok(()));

        assert!(!ok, "one failure must make the group a failure");
        assert_eq!(attempted, 3, "a failure stopped the writes that followed it");
    }
}
