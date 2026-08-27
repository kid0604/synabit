//! Files the vault can see but does not hold.
//!
//! # Why this is not just "Google Drive support"
//!
//! The version this replaces was a single function that asked Drive for a
//! thousand files and wrote them into a table nothing read. Two things were
//! wrong with it, and only one of them was Drive's fault.
//!
//! The listing stopped at a thousand. Not "paginated in batches of a thousand"
//! — stopped. `pageSize=1000` with no `nextPageToken` loop, so a Drive with
//! twelve thousand files reported a thousand of them and gave no sign that the
//! rest existed. That is the bug this module exists to make impossible, which
//! is why the paging loop lives here, separated from the HTTP, where it can be
//! driven by a test.
//!
//! The other was that a remote file had nowhere to live. Local identity is a
//! digest of a file's contents; a cloud file's contents are precisely what we
//! are not downloading. So remote entries carry the provider's own identity,
//! which is stable and machine-independent already, and sit in `remote_files`
//! rather than pretending to be somewhere on this disk.
//!
//! # What a provider is not asked to do
//!
//! Not to download. A listing is metadata, and the bytes stay where they are
//! until somebody opens the file. That is the same rule the sync layer follows
//! for attachments, and for the same reason: a folder of holiday video is not
//! something to copy onto a phone because it appeared in a list.

use crate::error::AppResult;

pub mod gdrive;

/// One file as a provider describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteFile {
    /// The provider's own id, stable across devices and accounts.
    pub remote_id: String,
    pub name: String,
    /// Lower-case, no dot. Derived from the name, or from the provider's type.
    pub extension: String,
    pub size: i64,
    /// `YYYY-MM-DD HH:MM:SS`, to match everything else in the app.
    pub modified_at: String,
    /// Where a person can open it, since we are not holding the bytes.
    pub web_url: String,
}

/// One page of a listing, and how to ask for the next.
#[derive(Debug, Clone, Default)]
pub struct RemotePage {
    pub files: Vec<RemoteFile>,
    /// `None` when this was the last page.
    pub next: Option<String>,
}

/// How many pages a listing will walk before giving up.
///
/// A provider that keeps handing back a token forever — through a bug at their
/// end or ours — would otherwise spin until the process died. At a thousand
/// files a page this is a million files, which is far past any vault and far
/// short of forever.
const MAX_PAGES: usize = 1_000;

/// Walk every page of a listing.
///
/// The paging is here, and the fetching is a closure, so that the loop can be
/// tested against a provider that exists only in the test — which is the only
/// way the "stopped at a thousand" bug could ever have been caught.
///
/// Duplicates are dropped: a file that changes position between two requests
/// can legitimately appear on both pages, and a listing that reports it twice
/// would create two entries for one file.
pub fn collect_pages<F>(mut fetch_page: F) -> AppResult<Vec<RemoteFile>>
where
    F: FnMut(Option<String>) -> AppResult<RemotePage>,
{
    let mut all = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut token: Option<String> = None;

    for _ in 0..MAX_PAGES {
        let page = fetch_page(token.clone())?;
        for file in page.files {
            if seen.insert(file.remote_id.clone()) {
                all.push(file);
            }
        }
        match page.next {
            // A provider that answers the same token twice is looping; treat
            // the repeat as the end rather than following it round again.
            Some(next) if Some(&next) != token.as_ref() => token = Some(next),
            _ => return Ok(all),
        }
    }

    log::warn!("remote listing stopped after {MAX_PAGES} pages");
    Ok(all)
}

/// The identity a remote file takes in the nodes table.
///
/// Namespaced by provider so two services cannot collide, and deliberately not
/// shaped like a content digest — `file_index::hash_from_node_id` rejects it,
/// which is correct: nothing here has been read.
pub fn node_id_for(provider: &str, remote_id: &str) -> String {
    let safe: String = remote_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    format!("Files/{provider}-{safe}.md")
}

/// The extension to file a remote entry under.
///
/// Taken from the name where there is one, and from the provider's own type
/// otherwise — a Google Doc has no filename extension at all, and calling it
/// `gdoc` is what lets it be filtered alongside everything else.
pub fn extension_for(name: &str, mime_type: &str) -> String {
    if let Some((_, ext)) = name.rsplit_once('.') {
        if !ext.is_empty() && ext.len() <= 12 && ext.chars().all(|c| c.is_alphanumeric()) {
            return ext.to_lowercase();
        }
    }
    match mime_type {
        m if m.contains("folder") => "folder",
        m if m.contains("spreadsheet") => "gsheet",
        m if m.contains("presentation") => "gslides",
        m if m.contains("document") => "gdoc",
        m if m.contains("form") => "gform",
        m if m.contains("drawing") => "gdraw",
        _ => "file",
    }
    .to_string()
}

/// RFC 3339, as every cloud API sends it, in the form the rest of the app uses.
pub fn normalise_timestamp(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn file(id: &str) -> RemoteFile {
        RemoteFile {
            remote_id: id.to_string(),
            name: format!("{id}.pdf"),
            extension: "pdf".into(),
            size: 1,
            modified_at: "2026-01-01 00:00:00".into(),
            web_url: String::new(),
        }
    }

    /// The bug this module was built around: the old listing asked for one page
    /// of a thousand and called that the answer, so a larger Drive silently
    /// reported a fraction of itself.
    #[test]
    fn a_listing_walks_past_the_first_page() {
        let pages = RefCell::new(vec![
            RemotePage { files: vec![file("a"), file("b")], next: Some("p2".into()) },
            RemotePage { files: vec![file("c")], next: Some("p3".into()) },
            RemotePage { files: vec![file("d")], next: None },
        ]);

        let all = collect_pages(|_| Ok(pages.borrow_mut().remove(0))).unwrap();

        assert_eq!(all.len(), 4, "every page has to be walked, not just the first");
    }

    /// Each request has to carry the token the last one handed back, or the
    /// loop asks for page one forever.
    #[test]
    fn each_request_carries_the_previous_token() {
        let asked: RefCell<Vec<Option<String>>> = RefCell::new(Vec::new());
        let mut remaining = 3;

        collect_pages(|token| {
            asked.borrow_mut().push(token);
            remaining -= 1;
            Ok(RemotePage {
                files: vec![file(&format!("f{remaining}"))],
                next: (remaining > 0).then(|| format!("token-{remaining}")),
            })
        })
        .unwrap();

        assert_eq!(
            *asked.borrow(),
            vec![None, Some("token-2".into()), Some("token-1".into())]
        );
    }

    /// A file that shifts position between two requests appears on both pages.
    /// Reported twice, it would become two entries for one file.
    #[test]
    fn a_file_seen_on_two_pages_is_listed_once() {
        let pages = RefCell::new(vec![
            RemotePage { files: vec![file("a"), file("b")], next: Some("p2".into()) },
            RemotePage { files: vec![file("b"), file("c")], next: None },
        ]);

        let all = collect_pages(|_| Ok(pages.borrow_mut().remove(0))).unwrap();

        assert_eq!(all.len(), 3);
    }

    /// A provider that answers with the token it was given is looping. Following
    /// it would spin until the process died.
    #[test]
    fn a_provider_that_repeats_its_token_does_not_spin_forever() {
        let calls = RefCell::new(0);
        let all = collect_pages(|_| {
            *calls.borrow_mut() += 1;
            Ok(RemotePage { files: vec![file("a")], next: Some("same".into()) })
        })
        .unwrap();

        assert!(*calls.borrow() <= 2, "made {} requests", calls.borrow());
        assert_eq!(all.len(), 1);
    }

    /// Even a provider that varies its token cannot keep us here indefinitely.
    #[test]
    fn a_listing_that_never_ends_is_eventually_abandoned() {
        let n = RefCell::new(0usize);
        let all = collect_pages(|_| {
            let mut n = n.borrow_mut();
            *n += 1;
            Ok(RemotePage { files: vec![file(&format!("f{n}"))], next: Some(format!("t{n}")) })
        })
        .unwrap();

        assert_eq!(all.len(), MAX_PAGES);
    }

    /// A failure partway through is a failure, not a short listing quietly
    /// reported as complete — which is exactly how the old bug looked.
    #[test]
    fn a_failure_partway_through_is_not_reported_as_success() {
        let first = RefCell::new(true);
        let result = collect_pages(|_| {
            if first.replace(false) {
                Ok(RemotePage { files: vec![file("a")], next: Some("p2".into()) })
            } else {
                Err(crate::error::AppError::General("network died".into()))
            }
        });

        assert!(result.is_err());
    }

    // ── Identity and shape ────────────────────────────────────

    /// A remote id is not a digest, and must not be mistaken for one — nothing
    /// about a cloud file has been read.
    #[test]
    fn a_remote_identity_is_never_read_as_a_content_digest() {
        let id = node_id_for("gdrive", "1AbC_dEf-GhI");
        assert_eq!(id, "Files/gdrive-1AbC_dEf-GhI.md");
        assert_eq!(crate::file_index::hash_from_node_id(&id), None);
    }

    /// Remote ids arrive from a service, so they are untrusted input on their
    /// way to becoming a vault path.
    #[test]
    fn a_hostile_remote_id_cannot_escape_the_files_folder() {
        let id = node_id_for("gdrive", "../../etc/passwd");
        assert!(!id.contains(".."), "got {id}");
        assert_eq!(id.matches('/').count(), 1, "got {id}");
    }

    #[test]
    fn two_providers_with_the_same_id_are_two_files() {
        assert_ne!(node_id_for("gdrive", "abc"), node_id_for("dropbox", "abc"));
    }

    /// A Google Doc has no filename extension at all, and filing every one of
    /// them under "file" would make the type filter useless.
    #[test]
    fn a_native_cloud_document_still_gets_a_type() {
        assert_eq!(extension_for("Báo cáo quý 4", "application/vnd.google-apps.document"), "gdoc");
        assert_eq!(extension_for("Ngân sách", "application/vnd.google-apps.spreadsheet"), "gsheet");
        assert_eq!(extension_for("Ảnh.JPG", "image/jpeg"), "jpg");
        assert_eq!(extension_for("no-extension", "application/octet-stream"), "file");
    }

    /// A name with a dot in it is not necessarily a name with an extension.
    #[test]
    fn a_dot_in_a_name_is_not_always_an_extension() {
        assert_eq!(extension_for("Bản 2.0 cuối cùng", "application/pdf"), "file");
    }

    #[test]
    fn timestamps_arrive_in_the_form_the_rest_of_the_app_uses() {
        let out = normalise_timestamp("2026-06-14T09:12:33.000Z");
        assert_eq!(out.len(), 19, "got {out}");
        assert!(out.starts_with("2026-06-14"), "got {out}");
        // Anything unparseable is passed through rather than discarded.
        assert_eq!(normalise_timestamp("không phải ngày"), "không phải ngày");
    }
}
