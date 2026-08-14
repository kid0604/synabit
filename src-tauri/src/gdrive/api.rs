use super::{DriveChange, DriveChangeList, DriveFile, DriveFileList, VAULT_FOLDER_NAME};

// ──────────────────────────────────────────────
// Google Drive API Helpers
// ──────────────────────────────────────────────

pub(crate) async fn drive_list_files_page(
    client: &reqwest::Client,
    token: &str,
    folder_id: &str,
    page_token: Option<&str>,
    page_size: u32,
) -> Result<(Vec<DriveFile>, Option<String>), String> {
    let mut url = format!(
        "https://www.googleapis.com/drive/v3/files?q='{}'+in+parents+and+trashed=false&fields=files(id,name),nextPageToken&pageSize={}",
        folder_id, page_size
    );
    if let Some(pt) = page_token {
        if !pt.is_empty() {
            url.push_str(&format!("&pageToken={}", urlencoding::encode(pt)));
        }
    }

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Drive list page failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Drive list page error: {}", err));
    }

    let list: DriveFileList = resp.json().await.map_err(|e| e.to_string())?;
    Ok((list.files.unwrap_or_default(), list.next_page_token))
}

pub(crate) async fn drive_get_start_page_token(
    client: &reqwest::Client,
    token: &str,
) -> Result<String, String> {
    let url = "https://www.googleapis.com/drive/v3/changes/startPageToken";
    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Get start page token failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Get start page token error: {}", err));
    }

    let val: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    val["startPageToken"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No startPageToken returned".to_string())
}

pub(crate) async fn drive_list_changes_page(
    client: &reqwest::Client,
    token: &str,
    page_token: &str,
    page_size: u32,
) -> Result<(Vec<DriveChange>, Option<String>, Option<String>), String> {
    let url = format!(
        "https://www.googleapis.com/drive/v3/changes?pageToken={}&fields=changes(fileId,removed,file(id,name)),nextPageToken,newStartPageToken&pageSize={}",
        urlencoding::encode(page_token),
        page_size
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Drive list changes failed: {}", e))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Drive list changes error: {}", err));
    }

    let list: DriveChangeList = resp.json().await.map_err(|e| e.to_string())?;
    Ok((
        list.changes.unwrap_or_default(),
        list.next_page_token,
        list.new_start_page_token,
    ))
}

#[allow(dead_code)]
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

pub(crate) async fn drive_download_file(
    client: &reqwest::Client,
    token: &str,
    file_id: &str,
) -> Result<Vec<u8>, String> {
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
pub(crate) async fn find_or_create_vault_folder(
    client: &reqwest::Client,
    token: &str,
) -> Result<String, String> {
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
