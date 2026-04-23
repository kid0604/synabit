use std::path::Path;

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
        return Err(crate::error::AppError::InvalidPath("Path traversal detected".to_string()));
    }
    Ok(())
}
