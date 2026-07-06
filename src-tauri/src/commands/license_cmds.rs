use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, command};

use crate::hwid::{generate_hwid, get_device_name};
use crate::license::{check_license_status, save_license, LicenseStatus};

const LICENSE_SERVER_URL: &str = "https://license.synabit.net/api/license"; // TBD: Change based on env config

#[derive(Serialize)]
struct ActivateRequest {
    hwid: String,
    device_name: String,
    license_key: Option<String>,
}

#[derive(Serialize)]
struct RefreshRequest {
    hwid: String,
    license_key: String,
}

#[derive(Deserialize)]
struct ServerResponse {
    success: bool,
    error: Option<String>,
    license_data: Option<String>, // Raw signed JSON string from server
}

fn get_http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

#[command]
pub async fn get_license_state(app: AppHandle) -> Result<LicenseStatus, String> {
    Ok(check_license_status(&app))
}

#[command]
pub async fn get_hwid() -> Result<String, String> {
    Ok(generate_hwid())
}

#[command]
pub async fn activate_trial(app: AppHandle) -> Result<LicenseStatus, String> {
    let hwid = generate_hwid();
    let device_name = get_device_name();

    let req = ActivateRequest {
        hwid,
        device_name,
        license_key: None,
    };

    let url = format!("{}/trial", LICENSE_SERVER_URL);
    let client = get_http_client();
    let res = client.post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let text = res.text().await.unwrap_or_default();
        return Err(format!("Server rejected: {}", text));
    }

    let raw_json = res.text().await.map_err(|e| e.to_string())?;
    
    save_license(&app, &raw_json)?;

    Ok(check_license_status(&app))
}

#[command]
pub async fn activate_license_key(app: AppHandle, key: String) -> Result<LicenseStatus, String> {
    let hwid = generate_hwid();
    let device_name = get_device_name();

    let req = ActivateRequest {
        hwid,
        device_name,
        license_key: Some(key),
    };

    let url = format!("{}/activate", LICENSE_SERVER_URL);
    let client = get_http_client();
    let res = client.post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let mut text = res.text().await.unwrap_or_default();
        if text.is_empty() {
            text = match status {
                reqwest::StatusCode::NOT_FOUND => "License key not found".to_string(),
                reqwest::StatusCode::FORBIDDEN => "License is expired or revoked".to_string(),
                reqwest::StatusCode::TOO_MANY_REQUESTS => "Device limit reached".to_string(),
                _ => format!("Server error ({})", status),
            };
        }
        return Err(format!("Activation failed: {}", text));
    }

    let raw_json = res.text().await.map_err(|e| e.to_string())?;
    
    save_license(&app, &raw_json)?;

    Ok(check_license_status(&app))
}

#[command]
pub async fn deactivate_license(app: AppHandle) -> Result<(), String> {
    let status = check_license_status(&app);
    if let LicenseStatus::Active(lic) = status {
        let hwid = generate_hwid();
        let req = RefreshRequest {
            hwid,
            license_key: lic.license_key,
        };

        let url = format!("{}/deactivate", LICENSE_SERVER_URL);
        let client = get_http_client();
        let _ = client.post(&url)
            .json(&req)
            .send()
            .await;
        
        // Remove local license file
        let path = crate::license::get_license_path(&app);
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

#[command]
pub async fn refresh_license(app: AppHandle) -> Result<LicenseStatus, String> {
    let status = check_license_status(&app);
    if let LicenseStatus::Active(lic) = status {
        let hwid = generate_hwid();
        let req = RefreshRequest {
            hwid,
            license_key: lic.license_key,
        };

        let url = format!("{}/refresh", LICENSE_SERVER_URL);
        let client = get_http_client();
        let res = client.post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(format!("Refresh failed: {}", text));
        }

        let raw_json = res.text().await.map_err(|e| e.to_string())?;
        save_license(&app, &raw_json)?;
    }
    Ok(check_license_status(&app))
}

#[command]
pub async fn heartbeat_license(app: AppHandle) -> Result<LicenseStatus, String> {
    let status = check_license_status(&app);
    if let LicenseStatus::Active(lic) = status {
        let hwid = generate_hwid();
        let req = RefreshRequest {
            hwid,
            license_key: lic.license_key,
        };

        let url = format!("{}/heartbeat", LICENSE_SERVER_URL);
        let client = get_http_client();
        let res = client.post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !res.status().is_success() {
            let status_code = res.status();
            let text = res.text().await.unwrap_or_default();
            
            // If the server says revoked, we should update our local state
            if status_code == reqwest::StatusCode::FORBIDDEN || text.contains("revoked") {
                // Delete local file to force revoked state
                let path = crate::license::get_license_path(&app);
                let _ = std::fs::remove_file(path);
                return Ok(LicenseStatus::Revoked);
            }
            return Err(format!("Heartbeat failed: {}", text));
        }

        let raw_json = res.text().await.map_err(|e| e.to_string())?;
        save_license(&app, &raw_json)?;
    }
    Ok(check_license_status(&app))
}
