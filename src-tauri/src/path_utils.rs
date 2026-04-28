use std::path::{Path, PathBuf};

/// Converts an absolute path to a vault-relative path with forward slashes.
/// This ensures consistent path representation across all platforms (Windows/macOS/Linux).
///
/// # Examples
/// ```
/// // On macOS/Linux:
/// to_relative("Notes/hello.md", "/Users/vault") == "Notes/hello.md"
///
/// // On Windows:
/// to_relative("Notes\\hello.md", "C:\\vault") == "Notes/hello.md"
/// ```
pub fn to_relative(full_path: &Path, vault_path: &str) -> String {
    full_path
        .strip_prefix(vault_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| full_path.to_string_lossy().to_string())
        .replace('\\', "/")
}

pub fn is_safe_filename(filename: &str) -> bool {
    if filename.is_empty() || filename == "." || filename == ".." {
        return false;
    }
    !filename.contains('/') && !filename.contains('\\')
}

pub fn enforce_no_traversal(path: &str) -> Result<(), crate::error::AppError> {
    if path.contains("..") {
        return Err(crate::error::AppError::InvalidPath(
            "Path traversal detected".to_string(),
        ));
    }
    Ok(())
}

/// Resolves a relative path within a vault, returning the safe absolute path.
/// Rejects any path that escapes the vault root after canonicalization.
pub fn resolve_safe_path(vault_path: &str, relative_path: &str) -> Result<PathBuf, crate::error::AppError> {
    let base = std::fs::canonicalize(vault_path)
        .map_err(|e| crate::error::AppError::InvalidPath(format!("Invalid vault path: {}", e)))?;
    let target = base.join(relative_path);
    
    // We only canonicalize if the target exists, if it doesn't we check its parent.
    // canonicalize() fails if the path does not exist.
    let canonical = if target.exists() {
        std::fs::canonicalize(&target)
            .map_err(|e| crate::error::AppError::InvalidPath(format!("Invalid path: {}", e)))?
    } else {
        // If it doesn't exist, we canonicalize the parent and append the filename
        let parent = target.parent().unwrap_or(&target);
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| crate::error::AppError::InvalidPath(format!("Invalid parent path: {}", e)))?;
        if let Some(file_name) = target.file_name() {
            canonical_parent.join(file_name)
        } else {
            canonical_parent
        }
    };

    if !canonical.starts_with(&base) {
        return Err(crate::error::AppError::InvalidPath("Path traversal detected".into()));
    }
    Ok(canonical)
}

/// Validates an absolute path is within one of the allowed root directories.
pub fn enforce_within_roots(path: &Path, allowed_roots: &[&str]) -> Result<(), crate::error::AppError> {
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|e| crate::error::AppError::InvalidPath(format!("Invalid path: {}", e)))?;
        
    for root in allowed_roots {
        if let Ok(canonical_root) = std::fs::canonicalize(root) {
            if canonical_path.starts_with(&canonical_root) {
                return Ok(());
            }
        }
    }
    Err(crate::error::AppError::InvalidPath("Path is outside allowed root directories".into()))
}
