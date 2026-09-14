//! Files that came with a message, held outside the vault until Syn decides.
//!
//! # Why not in the vault
//!
//! The design first put these in `{vault}/.synabit/telegram/`. Two things moved
//! them to the app's own data directory instead:
//!
//! - a message can arrive while no vault is open, and its file still has to be
//!   somewhere once it is fetched;
//! - a photo Syn does not keep should never touch the vault, not even a
//!   dot-directory of it, because what is in the vault is what a person
//!   believes they have.
//!
//! A file becomes part of the vault only through `capture`, which copies it
//! into `assets/` the way QuickCap stores anything pasted into it.
//!
//! # Names
//!
//! `{attachment id}__{a name a person would recognise}` for the file itself,
//! and `{attachment id}~preview.jpg` for the smaller copy of a photo a model is
//! shown. The id comes first so everything belonging to one message can be
//! found and removed without an index.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

use super::api::Api;
use super::inbox::{Attachment, Entry, Kind};

/// The most a model is shown of one picture. A photo's preview is far smaller;
/// this bounds a picture sent as a file, which can be anything up to 20 MB.
const MAX_SHOWN_BYTES: usize = 5 * 1024 * 1024;

/// What QuickCap draws as a picture.
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "heic"];

/// What QuickCap draws as a player — its own list, `AUDIO_EXTENSIONS`.
const AUDIO_EXTENSIONS: &[&str] = &["webm", "m4a", "ogg", "mp3", "wav"];

/// Where fetched files wait, under a given app data directory.
pub fn dir_under(app_data: &Path) -> PathBuf {
    app_data.join("telegram").join("inbox")
}

/// Where fetched files wait, made if it is not there.
pub fn dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> AppResult<PathBuf> {
    use tauri::Manager;
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::General(format!("No app data directory: {e}")))?;
    let dir = dir_under(&base);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn extension_of(name: &str) -> Option<String> {
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    (!ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric())).then_some(ext)
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' })
        .collect();
    cleaned.trim().trim_start_matches('.').chars().take(80).collect()
}

/// The extension a fetched file is stored with.
fn extension(attachment: &Attachment, remote_path: Option<&str>) -> String {
    let from_mime = attachment.mime_type.as_deref().and_then(|mime| {
        Some(match mime {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            "audio/ogg" => "ogg",
            "audio/mpeg" => "mp3",
            "audio/mp4" | "audio/x-m4a" => "m4a",
            "video/mp4" => "mp4",
            "application/pdf" => "pdf",
            _ => return None,
        })
    });
    let ext = attachment
        .file_name
        .as_deref()
        .and_then(extension_of)
        .or_else(|| remote_path.and_then(extension_of))
        .or_else(|| from_mime.map(str::to_string))
        .unwrap_or_else(|| {
            match attachment.kind {
                Kind::Photo => "jpg",
                Kind::Voice => "ogg",
                Kind::Audio => "mp3",
                Kind::Video => "mp4",
                Kind::Document => "bin",
            }
            .to_string()
        });
    // Telegram keeps voice notes as `.oga`. It is Ogg, and QuickCap plays `.ogg`.
    if ext == "oga" {
        "ogg".to_string()
    } else {
        ext
    }
}

/// The name a fetched file is stored under.
fn stored_name(attachment: &Attachment, remote_path: Option<&str>) -> String {
    let ext = extension(attachment, remote_path);
    let stem = match attachment.kind {
        Kind::Photo => "photo",
        Kind::Voice => "voice",
        Kind::Audio => "audio",
        Kind::Video => "video",
        Kind::Document => "file",
    };
    let name = match attachment.file_name.as_deref().map(safe_name).filter(|n| !n.is_empty()) {
        Some(named) if extension_of(&named).is_some() => named,
        Some(named) => format!("{named}.{ext}"),
        None => format!("{stem}.{ext}"),
    };
    format!("{}__{name}", attachment.id)
}

/// Whether a stored file belongs to an attachment.
///
/// By prefix, with the separator, so `a12-1` does not claim `a12-10`'s files.
fn belongs(file_name: &str, id: &str) -> bool {
    file_name
        .strip_prefix(id)
        .is_some_and(|rest| rest.starts_with("__") || rest.starts_with('~'))
}

/// The fetched file for an attachment, if it has been fetched.
pub fn stored(dir: &Path, id: &str) -> Option<PathBuf> {
    let prefix = format!("{id}__");
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix)))
}

/// What a person would call a stored file: its name without the id.
fn shown_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.split_once("__"))
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| "file".to_string())
}

/// Fetch an attachment, unless it already has been. The reason, when it cannot be.
pub async fn fetch(api: &Api, dir: &Path, attachment: &Attachment) -> Result<PathBuf, String> {
    if let Some(done) = stored(dir, &attachment.id) {
        return Ok(done);
    }
    if attachment.too_large() {
        return Err("larger than the 20 MB a bot may download".to_string());
    }
    let remote = api.get_file(&attachment.file_id).await.map_err(|e| e.to_string())?;
    let path = remote.file_path.ok_or("Telegram gave no path for it")?;
    let bytes = api.download(&path).await.map_err(|e| e.to_string())?;
    let target = dir.join(stored_name(attachment, Some(&path)));
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(target)
}

/// A picture as a model is shown it, in base64 — the smaller copy when there is
/// one. `None` when it cannot be had, or is too large to show.
pub async fn preview(api: &Api, dir: &Path, attachment: &Attachment) -> Option<String> {
    use base64::Engine;

    let bytes = match &attachment.preview_file_id {
        Some(file_id) => {
            let path = dir.join(format!("{}~preview.jpg", attachment.id));
            match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => {
                    let remote = api.get_file(file_id).await.ok()?;
                    let bytes = api.download(&remote.file_path?).await.ok()?;
                    if let Err(e) = std::fs::write(&path, &bytes) {
                        log::warn!("[Telegram] Could not keep a preview: {e}");
                    }
                    bytes
                }
            }
        }
        None => std::fs::read(stored(dir, &attachment.id)?).ok()?,
    };
    (bytes.len() <= MAX_SHOWN_BYTES).then(|| base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn remove_where(dir: &Path, doomed: impl Fn(&str) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()).is_some_and(&doomed) {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("[Telegram] Could not remove a fetched file: {e}");
            }
        }
    }
}

/// Remove the files that came with these messages.
pub fn discard(dir: &Path, entries: &[Entry]) {
    let ids: Vec<&str> = entries.iter().flat_map(|e| e.attachments.iter().map(|a| a.id.as_str())).collect();
    if ids.is_empty() {
        return;
    }
    remove_where(dir, |name| ids.iter().any(|id| belongs(name, id)));
}

/// Remove every file that no waiting message owns — left by a crash, or by a
/// message dropped some other way.
pub fn sweep(dir: &Path, waiting: &[Entry]) {
    let ids: Vec<&str> = waiting.iter().flat_map(|e| e.attachments.iter().map(|a| a.id.as_str())).collect();
    remove_where(dir, |name| !ids.iter().any(|id| belongs(name, id)));
}

/// What was moved into the vault.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Moved {
    pub images: usize,
    pub audio: usize,
    pub files: usize,
    /// Ids asked for that are not here to keep.
    pub missing: Vec<String>,
    /// Where each kept attachment went: `(attachment id, assets/… path)`.
    pub assets: Vec<(String, String)>,
}

/// Where a kept attachment went in the vault, by attachment id.
const ASSET_PREFIX: &str = "telegram:asset:";

/// Remember where attachments went, so a later message can still put them in a note.
///
/// # Why this exists
///
/// Two photos were kept in QuickCap, and in the next message Syn was asked to
/// put them in the day's note. It wrote `<img src="assets/a464598973-1">` — the
/// attachment id where the path belonged — because nothing it could see said
/// where the photos had gone: `capture` answered with a count, the fetched
/// files had been cleared, and the conversation held only the `[attachment …]`
/// lines. The note showed two broken images.
pub fn remember_assets(db: &crate::db::DbBridge, assets: &[(String, String)]) -> AppResult<()> {
    for (id, path) in assets {
        db.set_kv(&format!("{ASSET_PREFIX}{id}"), path)?;
    }
    Ok(())
}

/// An attachment named where a file path belongs: `attachment:a812-1`, or
/// `assets/a812-1` as a model will write it unprompted, with or without an
/// extension. Real asset names are content hashes and never look like this.
fn reference_pattern() -> regex::Regex {
    regex::Regex::new(r"(?:attachment:|assets/)(a\d+-\d+)(?:\.[A-Za-z0-9]{1,8})?").expect("a valid pattern")
}

/// Whether some content names an attachment where a path belongs.
pub fn mentions_attachment(content: &str) -> bool {
    reference_pattern().is_match(content)
}

/// Replace attachment ids in content with where those files are in the vault.
///
/// A file already kept is found where it was put. One fetched for the message
/// being answered and not kept yet is kept now — putting it in a note is
/// keeping it. One that is neither is an error, not a broken image: the model
/// is told, and can say so.
pub fn resolve_references(
    vault: &str,
    dir: Option<&Path>,
    db: &crate::db::DbBridge,
    content: &str,
) -> AppResult<String> {
    let pattern = reference_pattern();
    let mut paths: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut missing: Vec<String> = Vec::new();

    for found in pattern.captures_iter(content) {
        let id = found[1].to_string();
        if paths.contains_key(&id) || missing.contains(&id) {
            continue;
        }
        let remembered = db
            .get_kv(&format!("{ASSET_PREFIX}{id}"))?
            .filter(|path| Path::new(vault).join(path).is_file());
        let path = match (remembered, dir) {
            (Some(path), _) => Some(path),
            (None, Some(dir)) => {
                let (_, moved) = into_vault(vault, dir, std::slice::from_ref(&id))?;
                remember_assets(db, &moved.assets)?;
                moved.assets.into_iter().next().map(|(_, path)| path)
            }
            (None, None) => None,
        };
        match path {
            Some(path) => {
                paths.insert(id, path);
            }
            None => missing.push(id),
        }
    }

    if !missing.is_empty() {
        return Err(AppError::General(format!(
            "These attachments are not here any more, so they cannot go in a note: {}. Nothing was \
             written. Say so, and ask for them to be sent again.",
            missing.join(", ")
        )));
    }

    Ok(pattern
        .replace_all(content, |found: &regex::Captures<'_>| {
            paths.get(&found[1]).cloned().unwrap_or_else(|| found[0].to_string())
        })
        .into_owned())
}

/// Copy attachments into the vault's `assets/`, and write the Markdown that
/// points at them in the shape QuickCap gives its own: a picture as an image,
/// a recording as a link its renderer turns into a player, anything else as a
/// link by name.
pub fn into_vault(vault: &str, dir: &Path, ids: &[String]) -> AppResult<(Vec<String>, Moved)> {
    let mut lines = Vec::new();
    let mut moved = Moved::default();
    for id in ids {
        let Some(path) = stored(dir, id) else {
            moved.missing.push(id.clone());
            continue;
        };
        let name = shown_name(&path);
        let asset = crate::commands::nodes::save_asset(vault.to_string(), name.clone(), std::fs::read(&path)?)?;
        let ext = extension_of(&name).unwrap_or_default();
        moved.assets.push((id.clone(), asset.clone()));
        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            moved.images += 1;
            lines.push(format!("![Image]({asset})"));
        } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            moved.audio += 1;
            lines.push(format!("[Voice]({asset})"));
        } else {
            moved.files += 1;
            lines.push(format!("[{}]({asset})", name.replace(['[', ']'], "")));
        }
    }
    Ok((lines, moved))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attachment(id: &str, kind: Kind, file_name: Option<&str>, mime: Option<&str>) -> Attachment {
        Attachment {
            id: id.into(),
            kind,
            file_id: format!("file-{id}"),
            preview_file_id: None,
            file_name: file_name.map(str::to_string),
            mime_type: mime.map(str::to_string),
            size: Some(1000),
            width: None,
            height: None,
            duration: None,
        }
    }

    fn entry(attachments: Vec<Attachment>) -> Entry {
        Entry {
            update_id: 1,
            chat_id: 42,
            message_id: 1,
            text: String::new(),
            forwarded_from: None,
            received_at: "2026-09-13T08:00:00Z".into(),
            attachments,
            album: None,
            resume_run: None,
        }
    }

    /// A voice note is stored so QuickCap will play it, and a file keeps the
    /// name its sender gave it.
    #[test]
    fn stored_names_are_ones_quickcap_and_a_person_recognise() {
        let voice = attachment("a5-1", Kind::Voice, None, Some("audio/ogg"));
        assert_eq!(stored_name(&voice, Some("voice/file_12.oga")), "a5-1__voice.ogg");

        let pdf = attachment("a5-2", Kind::Document, Some("Hoá đơn tháng 9.pdf"), Some("application/pdf"));
        assert_eq!(stored_name(&pdf, Some("documents/file_3.pdf")), "a5-2__Hoá đơn tháng 9.pdf");

        let photo = attachment("a5-3", Kind::Photo, None, Some("image/jpeg"));
        assert_eq!(stored_name(&photo, Some("photos/file_9.jpg")), "a5-3__photo.jpg");

        let sneaky = attachment("a5-4", Kind::Document, Some("../../etc/passwd"), None);
        let name = stored_name(&sneaky, None);
        assert!(!name.contains('/') && name.starts_with("a5-4__"), "{name}");
    }

    /// One message's files are found and removed without touching another's.
    #[test]
    fn files_belong_to_their_own_attachment() {
        let dir = tempfile::tempdir().expect("temp");
        for name in ["a12-1__photo.jpg", "a12-1~preview.jpg", "a12-10__voice.ogg", "a13-1__file.pdf"] {
            std::fs::write(dir.path().join(name), b"x").expect("written");
        }
        assert!(stored(dir.path(), "a12-1").is_some_and(|p| p.ends_with("a12-1__photo.jpg")));

        discard(dir.path(), &[entry(vec![attachment("a12-1", Kind::Photo, None, None)])]);
        let left: Vec<String> = std::fs::read_dir(dir.path())
            .expect("read")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), 2, "{left:?}");
        assert!(left.iter().any(|n| n == "a12-10__voice.ogg"));

        sweep(dir.path(), &[entry(vec![attachment("a13-1", Kind::Document, None, None)])]);
        assert!(stored(dir.path(), "a12-10").is_none(), "nobody waits for it");
        assert!(stored(dir.path(), "a13-1").is_some(), "somebody does");
    }

    /// Kept into the vault in QuickCap's shapes, and what is not there is said.
    #[test]
    fn keeping_writes_assets_and_the_markdown_quickcap_draws() {
        let staging = tempfile::tempdir().expect("temp");
        let vault = tempfile::tempdir().expect("temp");
        let vault_path = vault.path().to_str().expect("utf8");
        std::fs::write(staging.path().join("a7-1__photo.jpg"), b"\xFF\xD8\xFFjpeg").expect("written");
        std::fs::write(staging.path().join("a7-2__voice.ogg"), b"OggS").expect("written");
        std::fs::write(staging.path().join("a7-3__Hoá đơn.pdf"), b"%PDF").expect("written");

        let ids: Vec<String> = ["a7-1", "a7-2", "a7-3", "a7-9"].iter().map(|s| s.to_string()).collect();
        let (lines, moved) = into_vault(vault_path, staging.path(), &ids).expect("kept");

        assert_eq!((moved.images, moved.audio, moved.files), (1, 1, 1));
        assert_eq!(moved.missing, vec!["a7-9".to_string()]);
        assert_eq!(moved.assets.len(), 3);
        assert_eq!(moved.assets[0].0, "a7-1");
        assert!(lines[0].starts_with("![Image](assets/") && lines[0].ends_with(".jpg)"), "{}", lines[0]);
        assert!(lines[1].starts_with("[Voice](assets/") && lines[1].ends_with(".ogg)"), "{}", lines[1]);
        assert!(lines[2].starts_with("[Hoá đơn.pdf](assets/"), "{}", lines[2]);
        assert_eq!(std::fs::read_dir(vault.path().join("assets")).expect("assets").count(), 3);
    }

    /// The case that broke: photos kept in one message, put in a note in the
    /// next, after the fetched files were gone — named by id, the way a model
    /// writes it unprompted.
    #[test]
    fn a_kept_attachment_named_in_a_later_note_is_where_it_was_kept() {
        let staging = tempfile::tempdir().expect("temp");
        let vault = tempfile::tempdir().expect("temp");
        let vault_path = vault.path().to_str().expect("utf8");
        let db = crate::db::DbBridge::new_in_memory_full().expect("schema");

        std::fs::write(staging.path().join("a464598973-1__photo.jpg"), b"\xFF\xD8\xFFone").expect("written");
        let (_, moved) = into_vault(vault_path, staging.path(), &["a464598973-1".to_string()]).expect("kept");
        remember_assets(&db, &moved.assets).expect("remembered");
        let real = moved.assets[0].1.clone();
        // Answered: the fetched file is cleared.
        std::fs::remove_file(staging.path().join("a464598973-1__photo.jpg")).expect("cleared");

        let written = "<div class=\"synabit-gallery\">\n  <img src=\"assets/a464598973-1\" />\n  ![](attachment:a464598973-1.jpg)\n</div>";
        assert!(mentions_attachment(written));
        let resolved = resolve_references(vault_path, Some(staging.path()), &db, written).expect("resolved");
        assert_eq!(resolved.matches(real.as_str()).count(), 2, "{resolved}");
        assert!(!resolved.contains("a464598973-1"), "{resolved}");
    }

    /// Named while its message is still being answered, and not kept yet:
    /// putting it in a note keeps it.
    #[test]
    fn a_fetched_attachment_named_in_a_note_is_kept_by_it() {
        let staging = tempfile::tempdir().expect("temp");
        let vault = tempfile::tempdir().expect("temp");
        let vault_path = vault.path().to_str().expect("utf8");
        let db = crate::db::DbBridge::new_in_memory_full().expect("schema");
        std::fs::write(staging.path().join("a9-1__photo.jpg"), b"\xFF\xD8\xFFtwo").expect("written");

        let resolved = resolve_references(vault_path, Some(staging.path()), &db, "![](attachment:a9-1)").expect("resolved");
        assert!(resolved.starts_with("![](assets/") && resolved.ends_with(".jpg)"), "{resolved}");
        assert_eq!(std::fs::read_dir(vault.path().join("assets")).expect("assets").count(), 1);
    }

    /// Neither kept nor here: nothing is written, and the model is told why.
    #[test]
    fn a_vanished_attachment_is_an_error_not_a_broken_image() {
        let staging = tempfile::tempdir().expect("temp");
        let vault = tempfile::tempdir().expect("temp");
        let db = crate::db::DbBridge::new_in_memory_full().expect("schema");
        let err = resolve_references(vault.path().to_str().expect("utf8"), Some(staging.path()), &db, "<img src=\"assets/a1-1\">")
            .expect_err("nothing to put there");
        assert!(err.to_string().contains("a1-1"), "{err}");
    }

    /// Real assets are content hashes and are left alone.
    #[test]
    fn a_real_asset_path_is_not_an_attachment() {
        assert!(!mentions_attachment("![Image](assets/af3c9e0d1b2a4c5e6f708192a3b4c5d6.jpg)"));
        assert!(!mentions_attachment("See a12-3 in the list"));
    }
}
