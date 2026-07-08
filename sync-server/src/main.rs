//! Synabit Sync Server — entry point.
//!
//! This binary combines:
//! 1. An **Iroh QUIC endpoint** serving the Synabit Mailbox protocol
//!    (custom ALPN `b"synabit/mailbox/1"`) for store-and-forward of
//!    encrypted CRDT blobs.
//! 2. An **HTTP health endpoint** for monitoring and Docker health checks.
//! 3. A **background cleanup task** that garbage-collects fully-ACKed and
//!    expired mailbox entries.
//!
//! ## Architecture
//!
//! ```text
//!  ┌──────────────┐       QUIC (ALPN: synabit/mailbox/1)
//!  │  Iroh Endpoint│◄──────────────────────────────────────── Devices
//!  │  + Router     │
//!  └──────┬───────┘
//!         │ ProtocolHandler::accept()
//!         ▼
//!  ┌──────────────┐     ┌─────────┐     ┌────────┐
//!  │ MailboxHandler│────►│ SQLite  │     │ Blob FS│
//!  └──────────────┘     └─────────┘     └────────┘
//!
//!  ┌──────────────┐       HTTP :8080
//!  │ Health Server │◄──────────────────────────────────────── Monitoring
//!  └──────────────┘
//!
//!  ┌──────────────┐
//!  │ Cleanup Task  │  (periodic background GC)
//!  └──────────────┘
//! ```

mod auth;
mod cleanup;
mod config;
mod db;
mod health;
mod mailbox;
mod protocol;

use anyhow::{Context, Result};
use clap::Parser;
use config::{AppConfig, Cli};
use db::Database;
use iroh::protocol::Router;
use mailbox::MailboxHandler;
use protocol::MAILBOX_ALPN;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

/// Load an existing secret key from disk, or generate and save a new one.
/// The key file stores the 32-byte secret key as hex (64 chars).
fn load_or_create_secret_key(path: &Path) -> Result<iroh::SecretKey> {
    if path.exists() {
        let hex_str = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read key file: {}", path.display()))?;
        let hex_str = hex_str.trim();
        let bytes: [u8; 32] = hex::decode(hex_str)
            .context("invalid hex in key file")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("key file must contain exactly 32 bytes (64 hex chars)"))?;
        let key = iroh::SecretKey::from_bytes(&bytes);
        info!(path = %path.display(), "loaded existing secret key");
        Ok(key)
    } else {
        let key = iroh::SecretKey::generate();
        let hex_str = hex::encode(key.to_bytes());
        std::fs::write(path, &hex_str)
            .with_context(|| format!("failed to write key file: {}", path.display()))?;
        // Restrict permissions (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(path, perms).ok();
        }
        warn!(path = %path.display(), "generated NEW secret key (first run)");
        Ok(key)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // --- Install rustls crypto provider (must match iroh's tls-ring) ---
    let _ = rustls::crypto::ring::default_provider().install_default();

    // --- Initialize tracing ---
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // --- Parse CLI and load config ---
    let cli = Cli::parse();
    let config = AppConfig::load(&cli)?;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        data_dir = %config.data_dir.display(),
        quic_port = config.quic_port,
        health_port = config.health_port,
        "starting synabit-sync-server"
    );

    // --- Ensure data directory exists ---
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("failed to create data dir: {}", config.data_dir.display()))?;

    // --- Open SQLite database ---
    let db_path = config.data_dir.join("mailbox.db");
    let db = Database::open(&db_path)?;
    info!(path = %db_path.display(), "database opened");

    // --- Create the mailbox handler ---
    let handler = Arc::new(MailboxHandler::new(db, config.clone()).await?);

    // --- Bind Iroh endpoint ---
    // We use the N0 preset with relay and discovery for NAT traversal.
    let bind_addr: SocketAddr = format!(
        "{}:{}",
        config.bind_addr, config.quic_port
    )
    .parse()
    .context("invalid QUIC bind address")?;

    // --- Load or create persistent secret key ---
    // This ensures the Endpoint ID stays the same across restarts.
    let key_path = config.data_dir.join("server.key");
    let secret_key = load_or_create_secret_key(&key_path)?;
    info!(key_path = %key_path.display(), "secret key loaded");

    let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .secret_key(secret_key)
        .bind_addr(bind_addr)?
        .bind()
        .await
        .context("failed to bind Iroh endpoint")?;

    let endpoint_id = endpoint.id();
    handler.set_endpoint_id(endpoint_id.to_string());
    info!(endpoint_id = %endpoint_id, "iroh endpoint bound");

    // Print endpoint ID prominently for easy copy-paste
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  SYNABIT SYNC SERVER                                               ║");
    println!("║  Endpoint ID (paste this into the app):                             ║");
    println!("║  {}  ║", endpoint_id);
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    // --- Build the protocol router ---
    let router = Router::builder(endpoint)
        .accept(MAILBOX_ALPN.to_vec(), handler.clone())
        .spawn();

    info!("mailbox protocol handler registered");

    // --- Cancellation token for graceful shutdown ---
    let cancel = CancellationToken::new();

    // --- Spawn background cleanup task ---
    let cleanup_handle = cleanup::spawn_cleanup_task(handler.clone(), cancel.clone());

    // --- Spawn HTTP health endpoint ---
    let health_addr: SocketAddr = format!("{}:{}", config.bind_addr, config.health_port)
        .parse()
        .context("invalid health endpoint address")?;

    let health_cancel = cancel.clone();
    let health_handler = handler.clone();
    let health_handle = tokio::spawn(async move {
        health::serve_health(health_handler, health_addr, health_cancel).await;
    });

    // --- Wait for shutdown signal ---
    info!("server is ready — press Ctrl+C to shut down");
    match tokio::signal::ctrl_c().await {
        Ok(()) => info!("received Ctrl+C, shutting down..."),
        Err(e) => error!(error = %e, "failed to listen for Ctrl+C"),
    }

    // --- Graceful shutdown ---
    cancel.cancel();

    // Shut down the Iroh router (drains active connections).
    router.shutdown().await.map_err(|e| anyhow::anyhow!("router shutdown error: {e}"))?;

    // Wait for background tasks.
    let _ = tokio::join!(cleanup_handle, health_handle);

    info!("server stopped");
    Ok(())
}
