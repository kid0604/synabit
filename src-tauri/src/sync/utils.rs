//! Shared utility functions for sync engines (GDrive, P2P, etc.).
//!
//! These were previously duplicated in `gdrive/mod.rs` and `sync/engine.rs`.
//! Centralised here so all sync backends use the same implementations.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// Compute the SHA-256 hex digest of a file.
///
/// Returns an empty string if the file cannot be read.
pub fn file_sha256(path: &Path) -> String {
    use std::io::Read;
    if let Ok(mut file) = fs::File::open(path) {
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => hasher.update(&buffer[..n]),
                Err(_) => return String::new(),
            }
        }
        format!("{:x}", hasher.finalize())
    } else {
        String::new()
    }
}

/// Walk the vault directory and collect relative file paths.
///
/// Skips:
/// - dotfiles / dot-directories (`.git`, `.obsidian`, …)
/// - the sync manifest file
/// - the `.synabit_crdt/` shadow directory
pub fn collect_local_files(vault_path: &str) -> Vec<String> {
    let base = Path::new(vault_path);
    let mut files = Vec::new();

    for entry in walkdir::WalkDir::new(base)
        .into_iter()
        .filter_entry(|e| {
            // Skip hidden directories at every level
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.')
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name.starts_with('.') || name == ".synabit_sync_manifest.json" {
            continue;
        }
        // Skip SQLite databases and temp files
        if name.ends_with(".db") || name.ends_with(".db-shm") || name.ends_with(".db-wal") {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(base) {
            let rel_str = rel.to_string_lossy().to_string().replace('\\', "/");
            files.push(rel_str);
        }
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_sha256_nonexistent() {
        let result = file_sha256(Path::new("/nonexistent/file.txt"));
        assert!(result.is_empty());
    }

    #[test]
    fn test_file_sha256_known_value() {
        // Create a temp file in a unique dir to avoid collisions
        let dir = std::env::temp_dir().join("synabit_test_utils_sha256");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("hello.txt");
        fs::write(&path, b"hello").unwrap();
        let hash = file_sha256(&path);
        // sha256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        assert_eq!(hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_collect_local_files_skips_hidden() {
        let dir = std::env::temp_dir().join("synabit_test_utils_collect");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("visible.md"), "hi").unwrap();
        fs::write(dir.join(".hidden"), "hi").unwrap();
        let hidden_dir = dir.join(".obsidian");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("config.json"), "{}").unwrap();

        let files = collect_local_files(dir.to_str().unwrap());
        assert_eq!(files, vec!["visible.md"]);
        let _ = fs::remove_dir_all(&dir);
    }
}

