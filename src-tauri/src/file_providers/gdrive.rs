//! Listing a Google Drive, one page at a time.
//!
//! Only the HTTP and the JSON live here. The paging loop, the identities and
//! the field mapping are in the parent module, where they are driven by tests —
//! deliberately, because the bug this replaces was in the paging and not in the
//! request.

use serde::Deserialize;

use crate::error::{AppError, AppResult};

use super::{RemoteFile, RemotePage};

pub const PROVIDER: &str = "gdrive";

/// Files per request. Drive's maximum, and the reason the old code looked like
/// it worked: a small Drive fits in one page.
const PAGE_SIZE: &str = "1000";

/// What is asked for, and nothing more.
///
/// Every field here is metadata. Nothing requests content, and nothing here
/// downloads: opening a cloud file sends the reader to `web_url`.
const FIELDS: &str =
    "nextPageToken,files(id,name,mimeType,size,modifiedTime,webViewLink,trashed)";

#[derive(Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    /// A string, because Drive sends 64-bit sizes as JSON strings.
    size: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
    #[serde(rename = "webViewLink")]
    web_view_link: Option<String>,
}

#[derive(Deserialize)]
struct DriveListing {
    files: Vec<DriveFile>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

/// One page of the caller's Drive.
pub async fn fetch_page(
    client: &reqwest::Client,
    access_token: &str,
    page_token: Option<String>,
) -> AppResult<RemotePage> {
    let mut query: Vec<(&str, String)> = vec![
        ("fields", FIELDS.to_string()),
        ("q", "trashed=false".to_string()),
        ("pageSize", PAGE_SIZE.to_string()),
    ];
    if let Some(token) = page_token {
        query.push(("pageToken", token));
    }

    let response = client
        .get("https://www.googleapis.com/drive/v3/files")
        .header("Authorization", format!("Bearer {access_token}"))
        .query(&query)
        .send()
        .await
        .map_err(|e| AppError::General(format!("Could not reach Google Drive: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::General(format!(
            "Google Drive refused the request ({status}): {body}"
        )));
    }

    let listing: DriveListing = response
        .json()
        .await
        .map_err(|e| AppError::General(format!("Could not read Drive's answer: {e}")))?;

    Ok(RemotePage {
        files: listing.files.into_iter().map(into_remote).collect(),
        next: listing.next_page_token,
    })
}

fn into_remote(file: DriveFile) -> RemoteFile {
    let mime = file.mime_type.unwrap_or_default();
    RemoteFile {
        extension: super::extension_for(&file.name, &mime),
        remote_id: file.id,
        name: file.name,
        size: file.size.and_then(|s| s.parse().ok()).unwrap_or(0),
        modified_at: file
            .modified_time
            .map(|t| super::normalise_timestamp(&t))
            .unwrap_or_default(),
        web_url: file.web_view_link.unwrap_or_default(),
    }
}
