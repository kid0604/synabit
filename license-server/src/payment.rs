use axum::{
    body::Bytes,
    extract::{State, Path},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use std::env;
use std::sync::Arc;
use hmac::{Hmac, Mac, KeyInit};
use sha2::Sha256;

use crate::api::AppState;

pub trait PaymentProvider {
    fn verify_webhook(&self, headers: &HeaderMap, body: &[u8]) -> Result<(), StatusCode>;
    fn parse_event(&self, body: &[u8]) -> Result<Option<PaymentEvent>, StatusCode>;
}

pub struct PaymentEvent {
    pub customer_email: String,
    pub payment_id: String, // Subscription ID
    pub action: PaymentAction,
}

pub enum PaymentAction {
    CreatedOrRenewed,
    CancelledOrExpired,
    RevokedOrRefunded,
}

pub struct PolarProvider {
    webhook_secret: String,
}

impl PolarProvider {
    pub fn new() -> Self {
        Self {
            webhook_secret: env::var("POLAR_WEBHOOK_SECRET").expect("POLAR_WEBHOOK_SECRET must be set"),
        }
    }
}

impl PaymentProvider for PolarProvider {
    fn verify_webhook(&self, headers: &HeaderMap, body: &[u8]) -> Result<(), StatusCode> {
        // Polar uses Standard Webhooks (Svix)
        let msg_id = headers.get("webhook-id")
            .ok_or(StatusCode::UNAUTHORIZED)?
            .to_str()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
            
        let msg_timestamp = headers.get("webhook-timestamp")
            .ok_or(StatusCode::UNAUTHORIZED)?
            .to_str()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
            
        let msg_signature = headers.get("webhook-signature")
            .ok_or(StatusCode::UNAUTHORIZED)?
            .to_str()
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        // Construct the signed payload: "msg_id.timestamp.body"
        let mut signed_payload = String::new();
        signed_payload.push_str(msg_id);
        signed_payload.push('.');
        signed_payload.push_str(msg_timestamp);
        signed_payload.push('.');
        
        let body_str = std::str::from_utf8(body).map_err(|_| StatusCode::BAD_REQUEST)?;
        signed_payload.push_str(body_str);

        // Calculate HMAC-SHA256
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
        
        let mut mac = Hmac::<Sha256>::new_from_slice(self.webhook_secret.as_bytes())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            
        mac.update(signed_payload.as_bytes());
        let expected_sig = BASE64.encode(mac.finalize().into_bytes());

        // webhook-signature can contain multiple space-separated signatures: "v1,sig1 v1,sig2"
        let mut is_valid = false;
        for sig_part in msg_signature.split(' ') {
            if let Some(sig) = sig_part.strip_prefix("v1,") {
                if sig == expected_sig {
                    is_valid = true;
                    break;
                }
            }
        }

        if !is_valid {
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(())
    }

    fn parse_event(&self, body: &[u8]) -> Result<Option<PaymentEvent>, StatusCode> {
        #[derive(Deserialize)]
        struct PolarWebhook {
            r#type: String,
            data: PolarWebhookData,
        }

        #[derive(Deserialize)]
        struct PolarWebhookData {
            id: String, // Subscription ID
            status: String,
            customer: Option<PolarCustomer>,
        }
        
        #[derive(Deserialize)]
        struct PolarCustomer {
            email: String,
        }

        let event: PolarWebhook = serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)?;

        let action = match event.r#type.as_str() {
            "subscription.created" | "subscription.updated" => {
                if event.data.status == "active" {
                    PaymentAction::CreatedOrRenewed
                } else if event.data.status == "canceled" {
                    PaymentAction::CancelledOrExpired
                } else {
                    return Ok(None);
                }
            }
            "subscription.canceled" => PaymentAction::CancelledOrExpired,
            "subscription.revoked" => PaymentAction::RevokedOrRefunded,
            _ => return Ok(None),
        };

        let email = event.data.customer.map(|c| c.email).unwrap_or_default();

        Ok(Some(PaymentEvent {
            customer_email: email,
            payment_id: event.data.id,
            action,
        }))
    }
}

pub async fn handle_webhook(
    Path(provider_name): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    let provider = state.payment_providers.get(&provider_name).ok_or(StatusCode::NOT_FOUND)?;
    
    // 1. Verify signature
    provider.verify_webhook(&headers, &body)?;

    // 2. Parse event
    let event = match provider.parse_event(&body)? {
        Some(e) => e,
        None => return Ok(StatusCode::OK), // Ignore unhandled events
    };

    // 3. Update database
    let mut tx = state.db.pool.begin().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match event.action {
        PaymentAction::CreatedOrRenewed => {
            // Check if subscription exists
            let existing_id: Option<i64> = sqlx::query_scalar!(
                r#"SELECT id FROM licenses WHERE payment_id = ?"#,
                event.payment_id
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let now = Utc::now().naive_utc();
            let expires_at = now + Duration::days(31); // Add 31 days for monthly

            if let Some(id) = existing_id {
                // Renew
                sqlx::query!(
                    r#"UPDATE licenses SET expires_at = ?, status = 'active' WHERE id = ?"#,
                    expires_at, id
                )
                .execute(&mut *tx)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            } else {
                // Create new
                let new_key = format!("SYNC-{}", uuid::Uuid::new_v4().to_string().replace("-", "").to_uppercase()[..12].to_string());
                
                sqlx::query!(
                    r#"
                    INSERT INTO licenses (license_key, type, status, max_devices, expires_at, customer_email, payment_id, created_at)
                    VALUES (?, 'pro_monthly', 'active', 10, ?, ?, ?, ?)
                    "#,
                    new_key, expires_at, event.customer_email, event.payment_id, now
                )
                .execute(&mut *tx)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            }
        }
        PaymentAction::CancelledOrExpired => {
            // Usually we just let it expire naturally, but we can mark status if we want
            // We won't block it immediately, let `expires_at` do the job
        }
        PaymentAction::RevokedOrRefunded => {
            // Immediate revoke
            sqlx::query!(
                r#"UPDATE licenses SET status = 'revoked' WHERE payment_id = ?"#,
                event.payment_id
            )
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
    }

    tx.commit().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}
