use std::path::{Path, PathBuf};

/// Converts an absolute path to a vault-relative path with forward slashes.
/// This ensures consistent path representation across all platforms (Windows/macOS/Linux).
///
/// # Examples
/// ```
/// use synabit_lib::path_utils::to_relative;
/// use std::path::Path;
/// // On macOS/Linux:
/// assert_eq!(to_relative(Path::new("Notes/hello.md"), "/Users/vault"), "Notes/hello.md");
///
/// // On Windows:
/// assert_eq!(to_relative(Path::new("Notes\\hello.md"), "C:\\vault"), "Notes/hello.md");
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

/// A title turned into something that can be a filename.
///
/// Titles come from documents and from people, so they carry separators,
/// colons and the occasional newline — all of which either break a path or,
/// worse, change which directory it points at. Everything unsafe becomes a
/// hyphen rather than being dropped, so two titles that differ only in
/// punctuation do not collapse into one name.
pub fn sanitise_for_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            // The Windows-reserved set, plus the separators, plus anything a
            // terminal or a filesystem would rather not see.
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();

    // Collapse the runs the substitution creates, and trim the leading dots
    // that would otherwise hide the file from the vault scanner.
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c| c == '.' || c == '-' || c == ' ');

    if trimmed.is_empty() {
        return "Untitled".to_string();
    }
    // Filesystems cap a single component around 255 bytes; leave room for the
    // extension and for the counter a name collision adds.
    let mut end = trimmed.len().min(180);
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].to_string()
}

pub fn enforce_no_traversal(path: &str) -> Result<(), crate::error::AppError> {
    if path.contains("..") {
        return Err(crate::error::AppError::InvalidPath(
            "Path traversal detected".to_string(),
        ));
    }
    Ok(())
}

pub fn resolve_safe_path(
    vault_path: &str,
    relative_path: &str,
) -> Result<PathBuf, crate::error::AppError> {
    if relative_path.contains("..") {
        return Err(crate::error::AppError::InvalidPath(
            "Path traversal detected".into(),
        ));
    }

    let base = std::fs::canonicalize(vault_path)
        .map_err(|e| crate::error::AppError::InvalidPath(format!("Invalid vault path: {}", e)))?;
    let target = base.join(relative_path);

    // We only canonicalize if the target exists.
    let canonical = if target.exists() {
        std::fs::canonicalize(&target)
            .map_err(|e| crate::error::AppError::InvalidPath(format!("Invalid path: {}", e)))?
    } else {
        // Find the deepest existing ancestor
        let mut current = target.as_path();
        let mut non_existing_parts = Vec::new();

        while !current.exists() {
            if let Some(parent) = current.parent() {
                if let Some(file_name) = current.file_name() {
                    non_existing_parts.push(file_name.to_owned());
                }
                current = parent;
            } else {
                return Err(crate::error::AppError::InvalidPath(
                    "Cannot resolve path root".into(),
                ));
            }
        }

        let mut resolved = std::fs::canonicalize(current).map_err(|e| {
            crate::error::AppError::InvalidPath(format!("Invalid ancestor path: {}", e))
        })?;

        // Re-append the non-existing parts in reverse order
        for part in non_existing_parts.into_iter().rev() {
            resolved = resolved.join(part);
        }
        resolved
    };

    if !canonical.starts_with(&base) {
        return Err(crate::error::AppError::InvalidPath(
            "Path traversal detected".into(),
        ));
    }
    Ok(canonical)
}

/// Validates an absolute path is within one of the allowed root directories.
pub fn enforce_within_roots(
    path: &Path,
    allowed_roots: &[&str],
) -> Result<(), crate::error::AppError> {
    let canonical_path = std::fs::canonicalize(path)
        .map_err(|e| crate::error::AppError::InvalidPath(format!("Invalid path: {}", e)))?;

    for root in allowed_roots {
        if let Ok(canonical_root) = std::fs::canonicalize(root) {
            if canonical_path.starts_with(&canonical_root) {
                return Ok(());
            }
        }
    }
    Err(crate::error::AppError::InvalidPath(
        "Path is outside allowed root directories".into(),
    ))
}

/// Replace a vault file in one step: write a temporary file beside it, flush
/// it to disk, then rename it over the original.
///
/// A plain `fs::write` truncates the file first and fills it after, so a crash,
/// a full disk or a killed process in between leaves half a file — for a board,
/// JSON that no longer parses and hours of work that will not open. A rename
/// within one folder is atomic: a reader sees the old file or the new one.
///
/// The temporary name starts with a dot and ends in `.tmp`, which the vault
/// watcher ignores, and carries a random part so two writers never share one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        uuid::Uuid::new_v4().simple()
    ));
    let written = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        flush_to_disk(&file)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    // On Windows a rename over a file another program has open — a virus
    // scanner, the search indexer, a cloud client — is refused for the moment
    // it holds it. A few short retries ride that out; elsewhere the first try
    // is the only one that can fail.
    let mut attempt = 0;
    loop {
        match std::fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 4 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(25 * attempt));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
}

/// Hand the file's bytes to the disk before the rename makes them visible.
///
/// On Apple platforms `sync_all` is `F_FULLFSYNC`, which also empties the
/// drive's own cache: about 6 ms a file, so importing two thousand contacts
/// went from under two seconds to over twelve. A plain `fsync` there is what
/// SQLite itself uses by default on Apple, and it is enough for what this
/// function promises — the rename never exposes a half-written file after a
/// crash or a killed process. What it gives up is only the last moments before
/// a power cut, which the drive's cache may still be holding.
fn flush_to_disk(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: the descriptor belongs to `file`, which outlives this call.
        if unsafe { libc::fsync(file.as_raw_fd()) } == 0 {
            return Ok(());
        }
        return Err(std::io::Error::last_os_error());
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    file.sync_all()
}

#[cfg(test)]
mod tests {

    /// A fixture directory unique to this run.
    ///
    /// A fixed name under the system temp directory is shared by every process
    /// on the machine, so two `cargo test` runs at once — an IDE beside a
    /// terminal, or CI beside a local one — delete each other's fixtures
    /// mid-test. The failure lands on whichever test lost the race, which reads
    /// as flakiness with no pattern to it. It contaminated a measurement in this
    /// very repository.
    ///
    /// The named subdirectory is not decoration: `tempdir()` hands back a
    /// dot-prefixed path, and vault walking filters dotfiles — including the
    /// root of the walk itself.
    fn unique_dir(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let holder = tempfile::tempdir().expect("tempdir");
        let path = holder.path().join(name);
        std::fs::create_dir_all(&path).expect("create fixture dir");
        (holder, path)
    }
    use super::*;
    use std::path::PathBuf;

    // ── sanitise_for_filename ─────────────────────

    #[test]
    fn a_title_with_separators_cannot_change_directory() {
        assert_eq!(sanitise_for_filename("Notes/../etc/passwd"), "Notes-..-etc-passwd");
        assert_eq!(sanitise_for_filename("a\\b"), "a-b");
        assert!(is_safe_filename(&sanitise_for_filename("Trích dẫn — Hợp đồng 2026/07")));
    }

    /// Substituting rather than dropping keeps two titles that differ only in
    /// punctuation from collapsing onto one filename.
    #[test]
    fn punctuation_becomes_a_hyphen_rather_than_vanishing() {
        assert_ne!(
            sanitise_for_filename("Báo cáo: quý 4"),
            sanitise_for_filename("Báo cáo quý 4")
        );
    }

    /// A leading dot would hide the file from the vault scanner entirely, so a
    /// note exported under such a title would simply never appear.
    #[test]
    fn a_name_never_begins_with_a_dot() {
        assert_eq!(sanitise_for_filename("...ẩn"), "ẩn");
        assert_eq!(sanitise_for_filename("   "), "Untitled");
        assert_eq!(sanitise_for_filename("///"), "Untitled");
    }

    #[test]
    fn a_very_long_title_is_cut_without_splitting_a_character() {
        let long = "đ".repeat(400);
        let cut = sanitise_for_filename(&long);
        assert!(cut.len() <= 180);
        assert!(long.starts_with(&cut), "the cut must land on a character boundary");
    }

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
        let (_holder, tmp) = unique_dir("synabit_test_vault_normal");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));
        std::fs::write(tmp.join("Notes/hello.md"), "test").unwrap();

        let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/hello.md");
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("Notes/hello.md"));

    }

    #[test]
    fn test_resolve_safe_path_blocks_traversal() {
        let (_holder, tmp) = unique_dir("synabit_test_vault_traversal");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Attempt to escape vault via ../
        let result = resolve_safe_path(tmp.to_str().unwrap(), "../../../etc/passwd");
        assert!(result.is_err(), "Should reject path traversal via ../");

    }

    #[test]
    fn test_resolve_safe_path_blocks_intermediate_traversal() {
        let (_holder, tmp) = unique_dir("synabit_test_vault_intermediate");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Attempt to escape via Notes/../../../
        let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/../../../etc/shadow");
        assert!(result.is_err(), "Should reject intermediate path traversal");

    }

    #[test]
    fn test_resolve_safe_path_allows_new_file() {
        let (_holder, tmp) = unique_dir("synabit_test_vault_newfile");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Creating a new file that doesn't exist yet should work
        let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/new_note.md");
        assert!(result.is_ok(), "Should allow creating files inside vault");
        let resolved = result.unwrap();
        assert!(resolved.ends_with("Notes/new_note.md"));

    }

    #[test]
    fn test_resolve_safe_path_blocks_symlink_escape() {
        let (_holder, tmp) = unique_dir("synabit_test_vault_symlink");
        let _ = std::fs::create_dir_all(tmp.join("Notes"));

        // Create a symlink that points outside the vault
        #[cfg(unix)]
        {
            let symlink_path = tmp.join("Notes/escape_link");
            let _ = std::os::unix::fs::symlink("/tmp", &symlink_path);

            let result = resolve_safe_path(tmp.to_str().unwrap(), "Notes/escape_link/some_file");
            assert!(result.is_err(), "Should reject symlink escaping vault");

        }
    }

    // ── enforce_within_roots ──────────────────────

    #[test]
    fn test_enforce_within_roots_allows_valid() {
        let (_holder, tmp) = unique_dir("synabit_test_roots_valid");
        let _ = std::fs::create_dir_all(tmp.join("sub"));
        std::fs::write(tmp.join("sub/file.txt"), "test").unwrap();

        let roots = vec![tmp.to_str().unwrap()];
        let root_refs: Vec<&str> = roots.iter().map(|s| &**s).collect();
        let result = enforce_within_roots(&tmp.join("sub/file.txt"), &root_refs);
        assert!(result.is_ok());

    }

    #[test]
    fn test_enforce_within_roots_rejects_outside() {
        let (_holder, tmp1) = unique_dir("synabit_test_roots_inside");
        let (_holder, tmp2) = unique_dir("synabit_test_roots_outside");
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
        let (_holder, tmp1) = unique_dir("synabit_test_multi_root1");
        let (_holder, tmp2) = unique_dir("synabit_test_multi_root2");
        let _ = std::fs::create_dir_all(&tmp1);
        let _ = std::fs::create_dir_all(&tmp2);
        std::fs::write(tmp2.join("file.txt"), "test").unwrap();

        let roots = vec![tmp1.to_str().unwrap(), tmp2.to_str().unwrap()];
        let root_refs: Vec<&str> = roots.iter().map(|s| &**s).collect();

        // File in second root should be allowed
        let result = enforce_within_roots(&tmp2.join("file.txt"), &root_refs);
        assert!(
            result.is_ok(),
            "Should allow file in any of the allowed roots"
        );

        let _ = std::fs::remove_dir_all(&tmp1);
        let _ = std::fs::remove_dir_all(&tmp2);
    }

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_nothing_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Whiteboards").join("b.whiteboard.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["b.whiteboard.json".to_string()]);
    }
}
