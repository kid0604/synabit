//! HTTP health check endpoint.
//!
//! Exposes a lightweight HTTP server (via `axum`) on a separate port so that
//! load balancers, Docker health checks, and monitoring tools can probe
//! the server without speaking the Mailbox QUIC protocol.
//!
//! Endpoints:
//! - `GET /health`  → `200 OK` with JSON stats
//! - `GET /`        → `200 OK` plain text banner

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info};

use crate::mailbox::MailboxHandler;

/// JSON response for the `/health` endpoint.
#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    endpoint_id: String,
    vaults: u64,
    entries: u64,
    assets: u64,
    storage_bytes: u64,
}

/// Start the HTTP health server. Blocks until the cancellation token fires.
pub async fn serve_health(
    handler: Arc<MailboxHandler>,
    addr: SocketAddr,
    cancel: tokio_util::sync::CancellationToken,
) {
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .with_state(handler);

    info!(addr = %addr, "health endpoint listening");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, "failed to bind health endpoint");
            return;
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await
        .unwrap_or_else(|e| {
            error!(error = %e, "health server error");
        });
}

/// `GET /` — simple banner so humans know they've reached the right server.
async fn root_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        "synabit-sync-server is running\n",
    )
}

/// `GET /health` — JSON health/stats response.
async fn health_handler(
    State(handler): State<Arc<MailboxHandler>>,
) -> impl IntoResponse {
    let db = handler.db();

    // If any DB query fails, return a 503 with the error.
    let (vaults, entries, assets, storage_bytes) = match (|| -> anyhow::Result<_> {
        Ok((
            db.vault_count()?,
            db.entry_count()?,
            db.asset_count()?,
            db.total_storage_bytes()?,
        ))
    })() {
        Ok(stats) => stats,
        Err(e) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "status": "error",
                    "error": format!("{e:#}")
                })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            endpoint_id: handler.endpoint_id(),
            vaults,
            entries,
            assets,
            storage_bytes,
        }),
    )
        .into_response()
}
