//! Server configuration.
//!
//! Configuration is loaded from a TOML file, environment variables, and CLI
//! flags (in increasing precedence). Secrets are never embedded — they come
//! from env vars or a `.env` file.

use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

/// Synabit Sync Server — encrypted mailbox for P2P CRDT sync.
#[derive(Parser, Debug)]
#[command(name = "synabit-sync-server", version, about)]
pub struct Cli {
    /// Path to the TOML configuration file.
    #[arg(short, long, default_value = "config.toml")]
    pub config: PathBuf,

    /// Override: directory for persistent data (blobs, SQLite DB, key).
    #[arg(long)]
    pub data_dir: Option<PathBuf>,

    /// Override: port for the QUIC endpoint.
    #[arg(long)]
    pub quic_port: Option<u16>,

    /// Override: port for the HTTP health endpoint.
    #[arg(long)]
    pub health_port: Option<u16>,
}

// ---------------------------------------------------------------------------
// TOML configuration
// ---------------------------------------------------------------------------

/// Top-level configuration deserialized from `config.toml`.
#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
}

/// Server networking settings.
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// Port for the QUIC (Iroh) endpoint.
    #[serde(default = "default_quic_port")]
    pub quic_port: u16,
    /// Port for the HTTP health endpoint.
    #[serde(default = "default_health_port")]
    pub health_port: u16,
    /// Bind address (IPv4/IPv6).
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            quic_port: default_quic_port(),
            health_port: default_health_port(),
            bind_addr: default_bind_addr(),
        }
    }
}

/// Storage paths and limits.
#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    /// Root directory for all persistent data.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Default per-vault storage quota in bytes (1 GiB).
    #[serde(default = "default_max_vault_bytes")]
    pub default_max_vault_bytes: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            default_max_vault_bytes: default_max_vault_bytes(),
        }
    }
}

/// Retention / garbage-collection settings.
#[derive(Debug, Deserialize)]
pub struct RetentionConfig {
    /// How often (in seconds) the cleanup task runs.
    #[serde(default = "default_cleanup_interval_secs")]
    pub cleanup_interval_secs: u64,
    /// Maximum age (in seconds) for mailbox entries, even if un-ACKed.
    /// Default: 30 days.
    #[serde(default = "default_max_entry_age_secs")]
    pub max_entry_age_secs: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            cleanup_interval_secs: default_cleanup_interval_secs(),
            max_entry_age_secs: default_max_entry_age_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// Resolved configuration (all overrides applied)
// ---------------------------------------------------------------------------

/// Fully resolved runtime configuration, produced by merging TOML + CLI + env.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub quic_port: u16,
    pub health_port: u16,
    pub bind_addr: String,
    pub data_dir: PathBuf,
    pub default_max_vault_bytes: u64,
    pub cleanup_interval_secs: u64,
    pub max_entry_age_secs: u64,
}

impl AppConfig {
    /// Build the resolved configuration from CLI args + config file.
    ///
    /// Precedence: CLI flags > TOML file > defaults.
    pub fn load(cli: &Cli) -> anyhow::Result<Self> {
        let file_config: ConfigFile = if cli.config.exists() {
            let raw = std::fs::read_to_string(&cli.config)?;
            toml::from_str(&raw)?
        } else {
            tracing::info!(
                path = %cli.config.display(),
                "config file not found, using defaults"
            );
            ConfigFile {
                server: ServerConfig::default(),
                storage: StorageConfig::default(),
                retention: RetentionConfig::default(),
            }
        };

        Ok(Self {
            quic_port: cli.quic_port.unwrap_or(file_config.server.quic_port),
            health_port: cli.health_port.unwrap_or(file_config.server.health_port),
            bind_addr: file_config.server.bind_addr,
            data_dir: cli
                .data_dir
                .clone()
                .unwrap_or(file_config.storage.data_dir),
            default_max_vault_bytes: file_config.storage.default_max_vault_bytes,
            cleanup_interval_secs: file_config.retention.cleanup_interval_secs,
            max_entry_age_secs: file_config.retention.max_entry_age_secs,
        })
    }
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

fn default_quic_port() -> u16 {
    4433
}
fn default_health_port() -> u16 {
    8080
}
fn default_bind_addr() -> String {
    "0.0.0.0".to_string()
}
fn default_data_dir() -> PathBuf {
    PathBuf::from("./data")
}
fn default_max_vault_bytes() -> u64 {
    1_073_741_824 // 1 GiB
}
fn default_cleanup_interval_secs() -> u64 {
    300 // 5 minutes
}
fn default_max_entry_age_secs() -> u64 {
    30 * 24 * 3600 // 30 days
}
