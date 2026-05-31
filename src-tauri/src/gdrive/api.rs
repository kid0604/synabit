use super::{DriveFile, DriveFileList, SyncManifest, VAULT_FOLDER_NAME};
use std::future::Future;
use std::pin::Pin;

// ──────────────────────────────────────────────
// Google Drive API Helpers
// ──────────────────────────────────────────────

pub(crate) async fn drive_list_files(
    client: &reqwest::Client,
    token: &str,
    folder_id: &str,
) -> Result<Vec<DriveFile>, String> {
    let mut all_files = Vec::new();
    let mut page_token: Option<String> = None;

    loop {
        let mut url = format!(
            "https://www.googleapis.com/drive/v3/files?q='{}'+in+parents+and+trashed=false&fields=files(id,name,mimeType,modifiedTime,md5Checksum),nextPageToken&pageSize=1000",
            folder_id
        );
        if let Some(ref pt) = page_token {
            url.push_str(&format!("&pageToken={}", pt));
        }

        let resp = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("Drive list failed: {}", e))?;

        if !resp.status().is_success() {
            let err = resp.text().await.unwrap_or_default();
            return Err(format!("Drive list error: {}", err));
        }

        let list: DriveFileList = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(files) = list.files {
            all_files.extend(files);
        }
        match list.next_page_token {
            Some(pt) => page_token = Some(pt),
            None => break,
        }
    }

    Ok(all_files)
}

pub(crate) async fn drive_download_file(client: &reqwest::Client, token: &str, file_id: &str) -> Result<Vec<u8>, String> {
    let url = format!(
        "https://www.googleapis.com/drive/v3/files/{}?alt=media",
        file_id
    );
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Download error: {}", err));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Read bytes failed: {}", e))
}

pub(crate) async fn drive_upload_file(
    client: &reqwest::Client,
    token: &str,
    folder_id: &str,
    name: &str,
    content: &[u8],
) -> Result<(String, String), String> {

    let metadata = serde_json::json!({
        "name": name,
        "parents": [folder_id]
    });

    let form = reqwest::multipart::Form::new()
        .part(
            "metadata",
            reqwest::multipart::Part::text(metadata.to_string())
                .mime_str("application/json")
                .unwrap(),
        )
        .part(
            "file",
            reqwest::multipart::Part::bytes(content.to_vec())
                .file_name(name.to_string())
                .mime_str("application/octet-stream")
                .unwrap(),
        );

    let resp = client
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,modifiedTime")
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Upload error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let id = result["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No file ID returned".to_string())?;

    let modified_time = result["modifiedTime"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    Ok((id, modified_time))
}

pub(crate) async fn drive_update_file(
    client: &reqwest::Client,
    token: &str,
    file_id: &str,
    content: &[u8],
) -> Result<String, String> {
    let url = format!(
        "https://www.googleapis.com/upload/drive/v3/files/{}?uploadType=media&fields=id,modifiedTime",
        file_id
    );

    let resp = client
        .patch(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/octet-stream")
        .body(content.to_vec())
        .send()
        .await
        .map_err(|e| format!("Update failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Update error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let modified_time = result["modifiedTime"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    Ok(modified_time)
}

pub(crate) async fn drive_delete_file(client: &reqwest::Client, token: &str, file_id: &str) -> Result<(), String> {
    let url = format!("https://www.googleapis.com/drive/v3/files/{}", file_id);

    let resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Delete failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Delete error: {}", err));
    }

    Ok(())
}

pub(crate) async fn drive_create_folder(
    client: &reqwest::Client,
    token: &str,
    parent_id: &str,
    name: &str,
) -> Result<String, String> {

    let metadata = serde_json::json!({
        "name": name,
        "mimeType": "application/vnd.google-apps.folder",
        "parents": [parent_id]
    });

    let resp = client
        .post("https://www.googleapis.com/drive/v3/files?fields=id")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(metadata.to_string())
        .send()
        .await
        .map_err(|e| format!("Create folder failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Create folder error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    result["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No folder ID returned".to_string())
}

/// Find the "Synabit Vault" root folder on Drive, or create it.
pub(crate) async fn find_or_create_vault_folder(client: &reqwest::Client, token: &str) -> Result<String, String> {

    let query = format!(
        "name='{}' and mimeType='application/vnd.google-apps.folder' and trashed=false",
        VAULT_FOLDER_NAME
    );
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)&pageSize=1",
        urlencoding::encode(&query)
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Search folder failed: {}", e))?;

    if resp.status().is_success() {
        let list: DriveFileList = resp.json().await.map_err(|e| e.to_string())?;
        if let Some(files) = list.files {
            if let Some(existing) = files.first() {
                if let Some(ref id) = existing.id {
                    return Ok(id.clone());
                }
            }
        }
    }

    // Not found: create it at root
    let metadata = serde_json::json!({
        "name": VAULT_FOLDER_NAME,
        "mimeType": "application/vnd.google-apps.folder"
    });

    let resp = client
        .post("https://www.googleapis.com/drive/v3/files?fields=id")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(metadata.to_string())
        .send()
        .await
        .map_err(|e| format!("Create vault folder failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Create vault folder error: {}", err));
    }

    let result: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    result["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No vault folder ID returned".to_string())
}

/// Ensure a nested folder path exists on Drive.
/// Returns the folder ID of the deepest folder.
pub(crate) async fn ensure_drive_folder_path(
    client: &reqwest::Client,
    token: &str,
    manifest: &mut SyncManifest,
    relative_dir: &str,
) -> Result<String, String> {
    if relative_dir.is_empty() || relative_dir == "." {
        return Ok(manifest.vault_folder_id.clone());
    }

    if let Some(id) = manifest.folder_ids.get(relative_dir) {
        return Ok(id.clone());
    }

    let parts: Vec<&str> = relative_dir.split('/').filter(|s| !s.is_empty()).collect();
    let mut parent_id = manifest.vault_folder_id.clone();
    let mut current_path = String::new();

    for part in parts {
        if !current_path.is_empty() {
            current_path.push('/');
        }
        current_path.push_str(part);

        if let Some(id) = manifest.folder_ids.get(&current_path) {
            parent_id = id.clone();
            continue;
        }

        let existing = drive_list_files(client, token, &parent_id).await?;
        let folder = existing.iter().find(|f| {
            f.name.as_deref() == Some(part)
                && f.mime_type.as_deref() == Some("application/vnd.google-apps.folder")
        });

        let folder_id = if let Some(f) = folder {
            f.id.clone().unwrap_or_default()
        } else {
            drive_create_folder(client, token, &parent_id, part).await?
        };

        manifest
            .folder_ids
            .insert(current_path.clone(), folder_id.clone());
        parent_id = folder_id;
    }

    Ok(parent_id)
}

/// Recursively collect all files from Drive folder tree.
#[allow(clippy::type_complexity)]
pub(crate) fn collect_drive_files<'a>(
    client: &'a reqwest::Client,
    token: &'a str,
    folder_id: &'a str,
    prefix: &'a str,
) -> Pin<Box<dyn Future<Output = Result<Vec<(String, DriveFile)>, String>> + Send + 'a>> {
    Box::pin(async move {
        let mut result = Vec::new();
        let files = drive_list_files(client, token, folder_id).await?;

        for f in files {
            let name = f.name.clone().unwrap_or_default();
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };

            if f.mime_type.as_deref() == Some("application/vnd.google-apps.folder") {
                let sub_id = f.id.clone().unwrap_or_default();
                let sub_files = collect_drive_files(client, token, &sub_id, &relative).await?;
                result.extend(sub_files);
            } else {
                result.push((relative, f));
            }
        }

        Ok(result)
    })
}
