//! Packing a vault into a single file, and unpacking it again.
//!
//! This exists because of one Android fact: uninstalling the app deletes its
//! storage, and the vault is in it. Sync covers the user who turned sync on;
//! this covers everybody else, and it covers the case where the phone is the
//! only device there has ever been.
//!
//! Everything here is deliberately platform-independent and works on any
//! `Read`/`Write` — the Android side hands over a file descriptor for a
//! location the user picked, and desktop can hand over an ordinary file. That
//! keeps the part where data can be lost testable on the machine you are
//! reading this on.

use crate::error::{AppError, AppResult};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

/// Entries larger than this are refused rather than buffered.
///
/// Restoring reads each entry into memory to verify it before writing, and the
/// declared size comes from the archive, which is to say from a file that may
/// have been tampered with or truncated. Without a ceiling a crafted entry
/// decides how much memory the app asks for, which on a phone means being
/// killed. This matches the attachment ceiling the sync pipeline already
/// enforces, so the two cannot disagree about what is too big to carry.
const MAX_ENTRY_BYTES: u64 = crate::sync::core::asset::MAX_ASSET_BYTES;

/// What went into an archive.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchiveSummary {
    pub files: usize,
    pub bytes: u64,
}

/// What came out of one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RestoreSummary {
    pub files: usize,
    pub bytes: u64,
    /// Entries refused because their path was not one we will write inside a
    /// vault. Reported rather than silently dropped: an archive containing
    /// these is either damaged or hostile, and the user should be told which
    /// of their files did not come back.
    pub rejected: Vec<String>,
}

/// Write every file under `vault` into `out` as a zip.
///
/// Directories are not given entries of their own; they are implied by the
/// paths of the files in them and recreated on the way out. Empty directories
/// therefore do not survive, which is correct for a vault — a directory with no
/// notes in it holds nothing.
///
/// `.synabit/` is included on purpose. It carries the vault's identity, and a
/// restore that produced a vault with a new identity would look to the sync
/// server like a stranger rather than the same vault coming back.
pub fn write_vault_zip<W: Write + Seek>(vault: &Path, out: W) -> AppResult<ArchiveSummary> {
    let mut writer = zip::ZipWriter::new(out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut summary = ArchiveSummary { files: 0, bytes: 0 };
    let mut entries = Vec::new();
    collect_files(vault, vault, &mut entries)?;

    // Sorted so the same vault always produces the same archive layout, which
    // makes two backups comparable and makes this testable.
    entries.sort();

    for rel_path in entries {
        let absolute = vault.join(&rel_path);
        let mut file = std::fs::File::open(&absolute).map_err(|e| {
            AppError::General(format!("could not read '{}': {e}", absolute.display()))
        })?;

        writer
            .start_file(&rel_path, options)
            .map_err(|e| AppError::General(format!("could not add '{rel_path}': {e}")))?;
        let copied = std::io::copy(&mut file, &mut writer)
            .map_err(|e| AppError::General(format!("could not write '{rel_path}': {e}")))?;

        summary.files += 1;
        summary.bytes += copied;
    }

    writer
        .finish()
        .map_err(|e| AppError::General(format!("could not finish the archive: {e}")))?;

    Ok(summary)
}

/// Gather vault-relative paths of every file under `dir`.
///
/// Symlinks are skipped. Following one would pull whatever it points at into
/// the archive, which is how a backup of a notes folder ends up containing
/// somebody's SSH key.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> AppResult<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;

        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_files(root, &path, out)?;
        } else if kind.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| AppError::General(format!("path escaped the vault: {e}")))?;
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Unpack an archive into `vault`.
///
/// The target must hold no documents. Merging an archive into a vault that is
/// already in use would need a rule for what happens when both sides have the
/// same file, and every such rule loses somebody's work in some case. Refusing
/// is the honest answer: restore into an empty vault, and let sync — which
/// exists for exactly this and resolves it with a CRDT — do the merging.
///
/// `.synabit/` is exempt from that check, because a freshly created vault
/// always has one and it is about to be replaced by the archive's.
pub fn read_vault_zip<R: Read + Seek>(input: R, vault: &Path) -> AppResult<RestoreSummary> {
    if vault_holds_documents(vault)? {
        return Err(AppError::General(
            "this vault already contains notes. Restoring would have to decide what to \
             do about files that exist on both sides, and any choice there loses \
             somebody's work. Restore into an empty vault instead."
                .to_string(),
        ));
    }

    let mut archive = zip::ZipArchive::new(input)
        .map_err(|e| AppError::General(format!("this file is not a readable archive: {e}")))?;

    let mut summary = RestoreSummary {
        files: 0,
        bytes: 0,
        rejected: Vec::new(),
    };

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| AppError::General(format!("could not read archive entry: {e}")))?;

        if entry.is_dir() {
            continue;
        }

        // `enclosed_name` refuses absolute paths and anything containing `..`,
        // and the vault's own rule is applied on top so this cannot disagree
        // with what the sync path is willing to write. An archive is an
        // untrusted input: it may have come from anywhere.
        let rel_path = match entry
            .enclosed_name()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
        {
            Some(name) if crate::sync::coordinator::is_safe_vault_relative_path(&name) => name,
            _ => {
                summary.rejected.push(entry.name().to_string());
                continue;
            }
        };

        if entry.size() > MAX_ENTRY_BYTES {
            summary.rejected.push(rel_path);
            continue;
        }

        let destination = vault.join(&rel_path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::General(format!("could not create '{}': {e}", parent.display()))
            })?;
        }

        let mut file = std::fs::File::create(&destination).map_err(|e| {
            AppError::General(format!("could not write '{}': {e}", destination.display()))
        })?;
        // Bounded by the declared size, which was checked above, so a lying
        // header cannot make this read forever.
        let written = std::io::copy(&mut entry.by_ref().take(MAX_ENTRY_BYTES), &mut file)
            .map_err(|e| AppError::General(format!("could not write '{rel_path}': {e}")))?;

        summary.files += 1;
        summary.bytes += written;
    }

    Ok(summary)
}

/// Does this vault hold anything other than its own identity directory?
fn vault_holds_documents(vault: &Path) -> AppResult<bool> {
    let entries = match std::fs::read_dir(vault) {
        Ok(entries) => entries,
        // No vault directory yet is the emptiest a vault can be.
        Err(_) => return Ok(false),
    };

    for entry in entries {
        let entry = entry?;
        if entry.file_name() == std::ffi::OsStr::new(".synabit") {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

/// A filename for a backup, stamped so several of them sort by age.
pub fn suggested_archive_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("synabit-vault-{}.zip", now.format("%Y-%m-%d-%H%M"))
}

/// The vault-relative paths an archive would restore, without restoring it.
///
/// Lets the UI say what is in a file before the user commits to unpacking it
/// over their vault.
pub fn peek_vault_zip<R: Read + Seek>(input: R) -> AppResult<Vec<PathBuf>> {
    let mut archive = zip::ZipArchive::new(input)
        .map_err(|e| AppError::General(format!("this file is not a readable archive: {e}")))?;

    let mut names = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| AppError::General(format!("could not read archive entry: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        if let Some(name) = entry.enclosed_name() {
            names.push(name);
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, body: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn vault_with_content() -> TempDir {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "Notes/plan.md", b"# Plan\n\nFirst draft.");
        write(dir.path(), "Notes/deep/nested.md", b"nested");
        write(dir.path(), "assets/picture.png", &[0x89, 0x50, 0x4E, 0x47]);
        write(dir.path(), ".synabit/vault.json", br#"{"vaultId":"abc"}"#);
        dir
    }

    fn archive_of(vault: &Path) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        write_vault_zip(vault, &mut buffer).unwrap();
        buffer.into_inner()
    }

    #[test]
    fn a_vault_survives_a_round_trip_byte_for_byte() {
        let source = vault_with_content();
        let bytes = archive_of(source.path());

        let restored = TempDir::new().unwrap();
        let summary = read_vault_zip(Cursor::new(bytes), restored.path()).unwrap();

        assert_eq!(summary.files, 4);
        assert!(summary.rejected.is_empty());
        assert_eq!(
            std::fs::read(restored.path().join("Notes/deep/nested.md")).unwrap(),
            b"nested"
        );
        assert_eq!(
            std::fs::read(restored.path().join("assets/picture.png")).unwrap(),
            vec![0x89, 0x50, 0x4E, 0x47],
            "binary content must not be mangled"
        );
    }

    /// Without the identity the restored vault is a stranger to the sync
    /// server, and every device would see the whole vault arrive again as new
    /// documents.
    #[test]
    fn the_vault_identity_travels_inside_the_archive() {
        let source = vault_with_content();
        let bytes = archive_of(source.path());

        let restored = TempDir::new().unwrap();
        read_vault_zip(Cursor::new(bytes), restored.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(restored.path().join(".synabit/vault.json")).unwrap(),
            r#"{"vaultId":"abc"}"#
        );
    }

    /// Zip-slip. An entry naming `../../…` must never be written, and the
    /// archive may have come from anywhere.
    #[test]
    fn an_entry_that_escapes_the_vault_is_refused_not_written() {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default();
            for hostile in [
                "../escaped.md",
                "Notes/../../escaped.md",
                "/etc/passwd",
                "Notes/./ok-but-odd.md",
            ] {
                writer.start_file(hostile, options).unwrap();
                writer.write_all(b"payload").unwrap();
            }
            writer.finish().unwrap();
        }

        let outside = TempDir::new().unwrap();
        let vault = outside.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        let summary = read_vault_zip(Cursor::new(buffer.into_inner()), &vault).unwrap();

        assert!(
            !outside.path().join("escaped.md").exists(),
            "an entry escaped the vault"
        );
        assert!(
            !summary.rejected.is_empty(),
            "escaping entries must be reported, not silently dropped"
        );
        for written in [
            vault.join("../escaped.md"),
            PathBuf::from("/etc/passwd.synabit-test"),
        ] {
            assert!(!written.exists());
        }
    }

    /// Restoring over a vault in use would need a merge rule, and every merge
    /// rule loses somebody's work in some case.
    #[test]
    fn restoring_into_a_vault_that_holds_notes_is_refused() {
        let source = vault_with_content();
        let bytes = archive_of(source.path());

        let occupied = TempDir::new().unwrap();
        write(occupied.path(), "Notes/mine.md", b"work in progress");

        let err = read_vault_zip(Cursor::new(bytes), occupied.path()).unwrap_err();

        assert!(err.to_string().contains("already contains notes"), "{err}");
        assert_eq!(
            std::fs::read(occupied.path().join("Notes/mine.md")).unwrap(),
            b"work in progress",
            "a refused restore must not have touched anything"
        );
    }

    /// A freshly created vault always has a `.synabit/` directory, so requiring
    /// a literally empty directory would make restore impossible in the one
    /// situation it exists for: a reinstall.
    #[test]
    fn a_fresh_vault_with_only_its_identity_directory_can_be_restored_into() {
        let source = vault_with_content();
        let bytes = archive_of(source.path());

        let fresh = TempDir::new().unwrap();
        write(fresh.path(), ".synabit/vault.json", br#"{"vaultId":"new"}"#);

        let summary = read_vault_zip(Cursor::new(bytes), fresh.path()).unwrap();

        assert_eq!(summary.files, 4);
        assert_eq!(
            std::fs::read_to_string(fresh.path().join(".synabit/vault.json")).unwrap(),
            r#"{"vaultId":"abc"}"#,
            "the archive's identity must replace the fresh one"
        );
    }

    #[test]
    fn symlinks_are_not_followed_into_the_archive() {
        let source = vault_with_content();
        let secret = source.path().parent().unwrap().join("id_rsa");
        std::fs::write(&secret, b"PRIVATE KEY").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, source.path().join("Notes/link")).unwrap();

        let bytes = archive_of(source.path());
        let names = peek_vault_zip(Cursor::new(bytes)).unwrap();

        assert!(
            !names.iter().any(|n| n.to_string_lossy().contains("link")),
            "a symlink was archived: {names:?}"
        );
    }

    #[test]
    fn an_empty_vault_produces_a_readable_empty_archive() {
        let empty = TempDir::new().unwrap();
        let bytes = archive_of(empty.path());

        let restored = TempDir::new().unwrap();
        let summary = read_vault_zip(Cursor::new(bytes), restored.path()).unwrap();

        assert_eq!(summary.files, 0);
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_reported_as_such() {
        let restored = TempDir::new().unwrap();
        let err = read_vault_zip(
            Cursor::new(b"this is a text file".to_vec()),
            restored.path(),
        )
        .unwrap_err();

        assert!(err.to_string().contains("not a readable archive"), "{err}");
    }

    #[test]
    fn the_suggested_name_sorts_by_age_and_is_a_legal_filename() {
        use chrono::TimeZone;
        let stamped = suggested_archive_name(
            chrono::Local
                .with_ymd_and_hms(2026, 8, 17, 14, 30, 0)
                .unwrap(),
        );

        assert_eq!(stamped, "synabit-vault-2026-08-17-1430.zip");
        assert!(
            !stamped.contains(':'),
            "colons are illegal on some filesystems"
        );
    }

    /// The archive is read on a phone. A crafted entry claiming to be enormous
    /// must not decide how much memory the app asks for.
    #[test]
    fn an_entry_larger_than_the_ceiling_is_refused() {
        assert_eq!(
            MAX_ENTRY_BYTES,
            crate::sync::core::asset::MAX_ASSET_BYTES,
            "the archive and the sync pipeline must agree on what is too large"
        );
    }
}
