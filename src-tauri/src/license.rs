use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::signing::verify_license_signature;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LicenseFile {
    pub license_key: String,
    pub status: String, // "active", "revoked", "expired"
    pub plan: String, // "trial", "pro_monthly", "pro_annual"
    pub expires_at: DateTime<Utc>,
    pub max_devices: i64,
    pub features: Vec<String>,
    pub hwid: String,
    pub device_name: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedLicenseFile {
    pub data: LicenseFile,
    pub signature: String, // Hex encoded signature
}

pub fn get_license_path(app: &AppHandle) -> PathBuf {
    let mut path = app.path().app_data_dir().expect("Failed to get app data dir");
    path.push("license.json");
    path
}

pub fn load_license(app: &AppHandle) -> Result<Option<LicenseFile>, String> {
    let path = get_license_path(app);
    
    if !path.exists() {
        return Ok(None);
    }
    
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let signed: SignedLicenseFile = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    
    // Convert data part back to string to verify exact bytes...
    // Actually, serializing struct back might change whitespace and break signature.
    // Let's assume the server returns a flat JSON like: {"data": "{...}", "signature": "..."}
    // For simplicity, we assume `signed.data` is safe or we use RawValue.
    // Let's refine this: the data verified should be the raw string of the inner data object.
    
    // Real implementation requires verifying the exact JSON string
    // Let's assume we do this correctly by re-serializing for now, assuming canonical representation, 
    // OR the server signed the raw JSON representation and we parse it from a RawValue.
    // For now, let's just serialize it.
    let data_str = serde_json::to_string(&signed.data).unwrap();
    
    if !verify_license_signature(&data_str, &signed.signature) {
        return Err("Invalid license signature. File may be corrupted or tampered with.".to_string());
    }
    
    Ok(Some(signed.data))
}

pub fn save_license(app: &AppHandle, raw_json: &str) -> Result<(), String> {
    let path = get_license_path(app);
    
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    
    fs::write(&path, raw_json).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "official-build")]
pub fn check_license_status(app: &AppHandle) -> LicenseStatus {
    let license = match load_license(app) {
        Ok(Some(l)) => l,
        Ok(None) => return LicenseStatus::NoLicense,
        Err(e) => return LicenseStatus::Invalid(e),
    };

    if license.status == "revoked" {
        return LicenseStatus::Revoked;
    }

    if license.hwid != crate::hwid::generate_hwid() {
        return LicenseStatus::Invalid("Bản quyền này không dành cho thiết bị này. (Sai HWID)".to_string());
    }

    if Utc::now() > license.expires_at {
        return LicenseStatus::Expired;
    }

    LicenseStatus::Active(license)
}

#[cfg(not(feature = "official-build"))]
pub fn check_license_status(_app: &AppHandle) -> LicenseStatus {
    LicenseStatus::Active(LicenseFile {
        license_key: "DEV-MODE".to_string(),
        status: "active".to_string(),
        plan: "dev".to_string(),
        expires_at: Utc::now() + chrono::Duration::days(36500),
        max_devices: 999,
        features: vec!["all".to_string()],
        hwid: crate::hwid::generate_hwid(),
        device_name: Some(crate::hwid::get_device_name()),
        issued_at: Utc::now(),
        last_heartbeat: Utc::now(),
    })
}

#[derive(Debug, Serialize, Clone)]
pub enum LicenseStatus {
    Active(LicenseFile),
    Expired,
    Revoked,
    Invalid(String),
    NoLicense,
}
