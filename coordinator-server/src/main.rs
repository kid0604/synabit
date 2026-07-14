use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::{Duration, Instant}};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

/// TTL for a pairing code (5 minutes)
const CODE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterReq {
    code: String,
    node_id_hex: String,
    device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LookupRes {
    node_id_hex: String,
    device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AcceptReq {
    code: String,
    acceptor_node_id_hex: String,
    acceptor_device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PollRes {
    acceptor_node_id_hex: String,
    acceptor_device_name: String,
}

#[derive(Debug, Clone)]
struct RegisteredCode {
    node_id_hex: String,
    device_name: String,
    created_at: Instant,
}

#[derive(Debug, Clone)]
struct AcceptedCode {
    acceptor_node_id_hex: String,
    acceptor_device_name: String,
    created_at: Instant,
}

#[derive(Clone)]
struct AppState {
    registered: Arc<RwLock<HashMap<String, RegisteredCode>>>,
    accepted: Arc<RwLock<HashMap<String, AcceptedCode>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = AppState {
        registered: Arc::new(RwLock::new(HashMap::new())),
        accepted: Arc::new(RwLock::new(HashMap::new())),
    };

    // Background task to cleanup expired codes
    let state_clone = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let now = Instant::now();
            let mut reg = state_clone.registered.write().await;
            reg.retain(|_, v| now.duration_since(v.created_at) < CODE_TTL);
            let mut acc = state_clone.accepted.write().await;
            acc.retain(|_, v| now.duration_since(v.created_at) < CODE_TTL);
            info!("Cleanup run: {} registered, {} accepted remaining", reg.len(), acc.len());
        }
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/pair/register", post(register))
        .route("/pair/lookup/:code", get(lookup))
        .route("/pair/accept", post(accept))
        .route("/pair/poll/:code", get(poll))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8090));
    info!("Coordinator server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "OK"
}

async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterReq>,
) -> impl IntoResponse {
    let mut reg = state.registered.write().await;
    reg.insert(
        payload.code.clone(),
        RegisteredCode {
            node_id_hex: payload.node_id_hex.clone(),
            device_name: payload.device_name.clone(),
            created_at: Instant::now(),
        },
    );
    info!("Registered code: {} for {}", payload.code, payload.device_name);
    StatusCode::OK
}

async fn lookup(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> impl IntoResponse {
    let reg = state.registered.read().await;
    if let Some(entry) = reg.get(&code) {
        if Instant::now().duration_since(entry.created_at) < CODE_TTL {
            return (
                StatusCode::OK,
                Json(Some(LookupRes {
                    node_id_hex: entry.node_id_hex.clone(),
                    device_name: entry.device_name.clone(),
                })),
            );
        }
    }
    (StatusCode::NOT_FOUND, Json(None))
}

async fn accept(
    State(state): State<AppState>,
    Json(payload): Json<AcceptReq>,
) -> impl IntoResponse {
    // We optionally verify the code was registered, but it's fine to just save it
    let mut acc = state.accepted.write().await;
    acc.insert(
        payload.code.clone(),
        AcceptedCode {
            acceptor_node_id_hex: payload.acceptor_node_id_hex.clone(),
            acceptor_device_name: payload.acceptor_device_name.clone(),
            created_at: Instant::now(),
        },
    );
    info!("Accepted code: {} by {}", payload.code, payload.acceptor_device_name);
    StatusCode::OK
}

async fn poll(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> impl IntoResponse {
    let acc = state.accepted.read().await;
    if let Some(entry) = acc.get(&code) {
        if Instant::now().duration_since(entry.created_at) < CODE_TTL {
            return (
                StatusCode::OK,
                Json(Some(PollRes {
                    acceptor_node_id_hex: entry.acceptor_node_id_hex.clone(),
                    acceptor_device_name: entry.acceptor_device_name.clone(),
                })),
            );
        }
    }
    (StatusCode::NOT_FOUND, Json(None))
}
