mod api;
mod crypto;
mod db;
mod payment;
pub mod email;

use axum::{
    routing::{post, get},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::api::{AppState, handle_trial, handle_activate, handle_refresh, handle_heartbeat, handle_deactivate};
use crate::payment::handle_webhook;

#[tokio::main]
async fn main() {
    // Load .env
    let _ = dotenvy::dotenv();

    // Init logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,license_server=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Init DB
    let db = db::Db::new().await.expect("Failed to init database");

    // Init Crypto
    let crypto = crypto::CryptoService::new();

    // Init Payment Providers
    let mut payment_providers: std::collections::HashMap<String, Box<dyn crate::payment::PaymentProvider + Send + Sync>> = std::collections::HashMap::new();
    payment_providers.insert("polar".to_string(), Box::new(crate::payment::PolarProvider::new()));

    let state = Arc::new(AppState { db, crypto, payment_providers });

    // Build routes
    let app = Router::new()
        .route("/api/license/trial", post(handle_trial))
        .route("/api/license/activate", post(handle_activate))
        .route("/api/license/refresh", post(handle_refresh))
        .route("/api/license/heartbeat", post(handle_heartbeat))
        .route("/api/license/deactivate", post(handle_deactivate))
        .route("/api/webhook/{provider}", post(handle_webhook))
        .route("/health", get(|| async { "OK" }))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive()); // Adjust for production

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);
    
    tracing::info!("License Server listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
