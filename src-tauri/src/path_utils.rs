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

    // ── is_safe_filename ──────────────────────────

    #[test]
    fn test_is_safe_filename() {
        // Valid names
        assert!(is_safe_filename("valid_name.txt"));
        assert!(is_safe_filename("spaced name.md"));
        assert!(is_safe_filename("日本語ファイル.md"));
        assert!(is_safe_filename("file-with-dashes.txt"));
        assert!(is_safe_filename(".hidden_file"));

        // Invalid names
        assert!(!is_safe_filename(""));
        assert!(!is_safe_filename("."));
        assert!(!is_safe_filename(".."));
        assert!(!is_safe_filename("folder/file.txt"));
        assert!(!is_safe_filename("folder\\file.txt"));
        assert!(!is_safe_filename("../escape.txt"));
    }

    // ── enforce_no_traversal ──────────────────────

    #[test]
    fn test_enforce_no_traversal() {
        // Safe paths
        assert!(enforce_no_traversal("safe/path/to/file.md").is_ok());
        assert!(enforce_no_traversal("safe_file.md").is_ok());
        assert!(enforce_no_traversal("Notes/2024/hello.md").is_ok());

        // Unsafe paths
        assert!(enforce_no_traversal("../unsafe/path").is_err());
        assert!(enforce_no_traversal("safe/../path").is_err());
        assert!(enforce_no_traversal("../../etc/passwd").is_err());
        assert!(enforce_no_traversal("notes/..").is_err());
    }

    // ── to_relative ───────────────────────────────

    #[test]
    fn test_to_relative() {
        let vault_path = "/Users/vault";
        let full_path = PathBuf::from("/Users/vault/Notes/hello.md");
        assert_eq!(to_relative(&full_path, vault_path), "Notes/hello.md");

        // Should return the original if not in vault
        let out_path = PathBuf::from("/Users/other/hello.md");
        assert_eq!(to_relative(&out_path, vault_path), "/Users/other/hello.md");

        // Nested deep path
        let deep = PathBuf::from("/Users/vault/a/b/c/d.md");
        assert_eq!(to_relative(&deep, vault_path), "a/b/c/d.md");
    }

    // ── resolve_safe_path ─────────────────────────
    // These tests use real filesystem (tempdir) to test canonicalization

    #[test]
    fn test_resolve_safe_path_normal() {
        let tmp = std::env::temp_dir().join("synabit_test_vault_normal");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));
        std::fs::write(tmp.join("Notes/hello.md"), "test").unwrap();

        let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/hello.md");
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("Notes/hello.md"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_safe_path_blocks_traversal() {
        let tmp = std::env::temp_dir().join("synabit_test_vault_traversal");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Attempt to escape vault via ../
        let result = resolve_safe_path(tmp.to_str().unwrap(), "../../../etc/passwd");
        assert!(result.is_err(), "Should reject path traversal via ../");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_safe_path_blocks_intermediate_traversal() {
        let tmp = std::env::temp_dir().join("synabit_test_vault_intermediate");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Attempt to escape via Notes/../../../
        let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/../../../etc/shadow");
        assert!(result.is_err(), "Should reject intermediate path traversal");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_safe_path_allows_new_file() {
        let tmp = std::env::temp_dir().join("synabit_test_vault_newfile");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Creating a new file that doesn't exist yet should work
        let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/new_note.md");
        assert!(result.is_ok(), "Should allow creating files inside vault");
        let resolved = result.unwrap();
        assert!(resolved.ends_with("Notes/new_note.md"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_resolve_safe_path_blocks_symlink_escape() {
        let tmp = std::env::temp_dir().join("synabit_test_vault_symlink");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Create a symlink that points outside the vault
        #[cfg(unix)]
        {
            let symlink_path = tmp.join("Notes/escape_link");
            let _ = std::os::unix::fs::symlink("/tmp", &symlink_path);

            let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/escape_link/some_file");
            assert!(result.is_err(), "Should reject symlink escaping vault");

            let _ = std::fs::remove_dir_all(&tmp);
        }
    }

    // ── enforce_within_roots ──────────────────────

    #[test]
    fn test_enforce_within_roots_allows_valid() {
        let tmp = std::env::temp_dir().join("synabit_test_roots_valid");
        let _ = std::fs::create_dir_all(tmp.join("sub"));
        std::fs::write(tmp.join("sub/file.txt"), "test").unwrap();

        let roots = vec![tmp.to_str().unwrap()];
        let root_refs: Vec<&str> = roots.iter().map(|s| &**s).collect();
        let result = enforce_within_roots(&tmp.join("sub/file.txt"), &root_refs);
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_enforce_within_roots_rejects_outside() {
        let tmp1 = std::env::temp_dir().join("synabit_test_roots_inside");
        let tmp2 = std::env::temp_dir().join("synabit_test_roots_outside");
        let _ = std::fs::create_dir_all(&tmp1);
        let _ = std::fs::create_dir_all(&tmp2);
        std::fs::write(tmp2.join("secret.txt"), "sensitive").unwrap();

        let roots = vec![tmp1.to_str().unwrap()];
        let root_refs: Vec<&str> = roots.iter().map(|s| &**s).collect();
        let result = enforce_within_roots(&tmp2.join("secret.txt"), &root_refs);
        assert!(result.is_err(), "Should reject path outside allowed roots");

        let _ = std::fs::remove_dir_all(&tmp1);
        let _ = std::fs::remove_dir_all(&tmp2);
    }

    #[test]
    fn test_enforce_within_roots_multiple_roots() {
        let tmp1 = std::env::temp_dir().join("synabit_test_multi_root1");
        let tmp2 = std::env::temp_dir().join("synabit_test_multi_root2");
        let _ = std::fs::create_dir_all(&tmp1);
        let _ = std::fs::create_dir_all(&tmp2);
        std::fs::write(tmp2.join("file.txt"), "test").unwrap();

        let roots = vec![tmp1.to_str().unwrap(), tmp2.to_str().unwrap()];
        let root_refs: Vec<&str> = roots.iter().map(|s| &**s).collect();

        // File in second root should be allowed
        let result = enforce_within_roots(&tmp2.join("file.txt"), &root_refs);
        assert!(result.is_ok(), "Should allow file in any of the allowed roots");

        let _ = std::fs::remove_dir_all(&tmp1);
        let _ = std::fs::remove_dir_all(&tmp2);
    }
}
