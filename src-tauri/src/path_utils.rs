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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_safe_filename() {
        assert!(is_safe_filename("valid_name.txt"));
        assert!(is_safe_filename("spaced name.md"));
        assert!(!is_safe_filename(""));
        assert!(!is_safe_filename("."));
        assert!(!is_safe_filename(".."));
        assert!(!is_safe_filename("folder/file.txt"));
        assert!(!is_safe_filename("folder\\file.txt"));
    }

    #[test]
    fn test_enforce_no_traversal() {
        assert!(enforce_no_traversal("safe/path/to/file.md").is_ok());
        assert!(enforce_no_traversal("safe_file.md").is_ok());
        assert!(enforce_no_traversal("../unsafe/path").is_err());
        assert!(enforce_no_traversal("safe/../path").is_err());
    }

    #[test]
    fn test_to_relative() {
        let vault_path = "/Users/vault";
        let full_path = PathBuf::from("/Users/vault/Notes/hello.md");
        assert_eq!(to_relative(&full_path, vault_path), "Notes/hello.md");

        // Should return the original if not in vault
        let out_path = PathBuf::from("/Users/other/hello.md");
        assert_eq!(to_relative(&out_path, vault_path), "/Users/other/hello.md");
    }
}
