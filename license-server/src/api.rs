use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashMap;
use chrono::{Utc, Duration};

use crate::db::{Db, License, Device};
use crate::crypto::CryptoService;

pub struct AppState {
    pub db: Db,
    pub crypto: CryptoService,
    pub payment_providers: HashMap<String, Box<dyn crate::payment::PaymentProvider + Send + Sync>>,
}

#[derive(Serialize)]
pub struct LicenseData {
    pub license_key: String,
    pub status: String,
    pub plan: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub max_devices: i64,
    pub features: Vec<String>,
    pub hwid: String,
    pub device_name: Option<String>,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct LicenseFile {
    pub data: LicenseData,
    pub signature: String,
}

#[derive(Deserialize)]
pub struct TrialRequest {
    pub hwid: String,
    pub device_name: Option<String>,
}

pub async fn handle_trial(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TrialRequest>,
) -> Result<Json<LicenseFile>, StatusCode> {
    // Check if HWID already had a trial
    let existing: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1 FROM devices d 
        JOIN licenses l ON d.license_id = l.id 
        WHERE l.type = 'trial' AND d.hwid = ?
        "#
    )
    .bind(&req.hwid)
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if existing.is_some() {
        return Err(StatusCode::CONFLICT); // 409 Conflict: Already used trial
    }

    let now = Utc::now().naive_utc();
    let expires_at = now + Duration::days(100);

    let mut tx = state.db.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let license_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO licenses (type, max_devices, expires_at, created_at)
        VALUES ('trial', 1, ?, ?)
        RETURNING id
        "#
    )
    .bind(expires_at)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        r#"
        INSERT INTO devices (license_id, hwid, device_name, activated_at, last_heartbeat)
        VALUES (?, ?, ?, ?, ?)
        "#
    )
    .bind(license_id)
    .bind(&req.hwid)
    .bind(&req.device_name)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = LicenseData {
        license_key: "TRIAL".to_string(),
        status: "active".to_string(),
        plan: "trial".to_string(),
        expires_at: expires_at.and_utc(),
        max_devices: 1,
        features: vec!["all".to_string()],
        hwid: req.hwid.clone(),
        device_name: req.device_name.clone(),
        issued_at: now.and_utc(),
        last_heartbeat: now.and_utc(),
    };

    let signature = state.crypto.sign_data(&data);

    Ok(Json(LicenseFile { data, signature }))
}

#[derive(Deserialize)]
pub struct ActivateRequest {
    pub license_key: String,
    pub hwid: String,
    pub device_name: Option<String>,
}

pub async fn handle_activate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ActivateRequest>,
) -> Result<Json<LicenseFile>, StatusCode> {
    let license = sqlx::query_as!(
        License,
        r#"SELECT * FROM licenses WHERE license_key = ?"#,
        req.license_key
    )
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if license.status == "revoked" || license.status == "expired" {
        return Err(StatusCode::FORBIDDEN);
    }

    let now = Utc::now().naive_utc();
    if license.expires_at < now {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut tx = state.db.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Check if device already exists
    let existing_device = sqlx::query_scalar!(
        r#"SELECT id FROM devices WHERE license_id = ? AND hwid = ?"#,
        license.id, req.hwid
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if existing_device.is_none() {
        // Count devices
        let count: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM devices WHERE license_id = ?"#,
            license.id
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if count >= license.max_devices {
            return Err(StatusCode::TOO_MANY_REQUESTS); // 429 Limit reached
        }

        sqlx::query!(
            r#"
            INSERT INTO devices (license_id, hwid, device_name, activated_at, last_heartbeat)
            VALUES (?, ?, ?, ?, ?)
            "#,
            license.id, req.hwid, req.device_name, now, now
        )
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        sqlx::query!(
            r#"UPDATE devices SET last_heartbeat = ?, device_name = ? WHERE id = ?"#,
            now, req.device_name, existing_device.unwrap()
        )
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = LicenseData {
        license_key: license.license_key.clone().unwrap_or_else(|| "TRIAL".to_string()),
        status: license.status.clone(),
        plan: license.r#type.clone(),
        expires_at: license.expires_at.and_utc(),
        max_devices: license.max_devices,
        features: vec!["all".to_string()], // TBD parse from JSON later
        hwid: req.hwid.clone(),
        device_name: req.device_name.clone(),
        issued_at: now.and_utc(),
        last_heartbeat: now.and_utc(),
    };

    let signature = state.crypto.sign_data(&data);

    Ok(Json(LicenseFile { data, signature }))
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub license_key: String,
    pub hwid: String,
}

pub async fn handle_refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<LicenseFile>, StatusCode> {
    let license = sqlx::query_as!(
        License,
        r#"SELECT * FROM licenses WHERE license_key = ?"#,
        req.license_key
    )
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if license.status == "revoked" || license.status == "expired" {
        return Err(StatusCode::FORBIDDEN);
    }

    let device = sqlx::query_as!(
        Device,
        r#"SELECT * FROM devices WHERE license_id = ? AND hwid = ?"#,
        license.id, req.hwid
    )
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let now = Utc::now().naive_utc();

    sqlx::query!(
        r#"UPDATE devices SET last_heartbeat = ? WHERE id = ?"#,
        now, device.id
    )
    .execute(&state.db.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let data = LicenseData {
        license_key: license.license_key.clone().unwrap_or_else(|| "TRIAL".to_string()),
        status: license.status.clone(),
        plan: license.r#type.clone(),
        expires_at: license.expires_at.and_utc(),
        max_devices: license.max_devices,
        features: vec!["all".to_string()],
        hwid: req.hwid.clone(),
        device_name: device.device_name.clone(),
        issued_at: device.activated_at.and_utc(),
        last_heartbeat: now.and_utc(),
    };

    let signature = state.crypto.sign_data(&data);

    Ok(Json(LicenseFile { data, signature }))
}

pub async fn handle_heartbeat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<LicenseFile>, StatusCode> {
    // Heartbeat logic is essentially the same as refresh (returns updated file to extend grace period)
    handle_refresh(State(state), Json(req)).await
}

#[derive(Deserialize)]
pub struct DeactivateRequest {
    pub license_key: String,
    pub hwid: String,
}

#[derive(Serialize)]
pub struct DeactivateResponse {
    pub success: bool,
    pub remaining_devices: i64,
}

pub async fn handle_deactivate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeactivateRequest>,
) -> Result<Json<DeactivateResponse>, StatusCode> {
    let license = sqlx::query_as!(
        License,
        r#"SELECT * FROM licenses WHERE license_key = ?"#,
        req.license_key
    )
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let mut tx = state.db.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let deleted = sqlx::query!(
        r#"DELETE FROM devices WHERE license_id = ? AND hwid = ?"#,
        license.id, req.hwid
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    let count: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) FROM devices WHERE license_id = ?"#,
        license.id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let remaining = license.max_devices - count;

    Ok(Json(DeactivateResponse {
        success: true,
        remaining_devices: remaining,
    }))
}
