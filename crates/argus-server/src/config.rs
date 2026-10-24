//! Layered configuration (`config-kit`): file -> `ARGUS_` env overrides.

use std::path::Path;

use serde::Deserialize;

/// Top-level configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ArgusConfig {
    /// HTTP/WS listener settings.
    #[serde(default)]
    pub server: ServerConfig,
    /// Feed polling settings.
    #[serde(default)]
    pub feeds: FeedsConfig,
    /// Telemetry settings.
    #[serde(default)]
    pub telemetry: TelemetrySection,
}

/// HTTP/WS listener settings.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Bind host.
    #[serde(default = "default_host")]
    pub host: String,
    /// Bind port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Exact-match WebSocket origins allowed in the upgrade handshake.
    /// Empty allows everything — acceptable for loopback M0 only.
    #[serde(default)]
    pub ws_allowed_origins: Vec<String>,
}

/// Feed polling settings.
#[derive(Debug, Clone, Deserialize)]
pub struct FeedsConfig {
    /// Synthetic market feed enabled.
    #[serde(default = "default_true")]
    pub sim_enabled: bool,
    /// Synthetic feed tick interval, milliseconds.
    #[serde(default = "default_sim_interval_ms")]
    pub sim_interval_ms: u64,
    /// USGS quake feed enabled.
    #[serde(default = "default_true")]
    pub usgs_enabled: bool,
    /// USGS poll interval, seconds.
    #[serde(default = "default_usgs_interval_secs")]
    pub usgs_interval_secs: u64,
}

/// Telemetry settings.
#[derive(Debug, Clone, Deserialize)]
pub struct TelemetrySection {
    /// Max log level (`info`, `debug`, ...).
    #[serde(default = "default_level")]
    pub level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            ws_allowed_origins: Vec::new(),
        }
    }
}

impl Default for FeedsConfig {
    fn default() -> Self {
        Self {
            sim_enabled: true,
            sim_interval_ms: default_sim_interval_ms(),
            usgs_enabled: true,
            usgs_interval_secs: default_usgs_interval_secs(),
        }
    }
}

impl Default for TelemetrySection {
    fn default() -> Self {
        Self {
            level: default_level(),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8420
}

fn default_sim_interval_ms() -> u64 {
    500
}

fn default_usgs_interval_secs() -> u64 {
    60
}

fn default_level() -> String {
    "info".to_string()
}

fn default_true() -> bool {
    true
}

/// Loads the configuration: optional TOML file layer, then `ARGUS_` env.
///
/// # Errors
/// Returns the `config-kit` error when a layer fails to parse.
pub fn load(path: &Path) -> Result<ArgusConfig, config_kit::ConfigError> {
    if path.exists() {
        config_kit::ConfigBuilder::new()
            .layer(config_kit::ConfigLayer::file(path))
            .layer(config_kit::ConfigLayer::env_prefix("ARGUS_"))
            .load()
    } else {
        tracing::warn!(path = %path.display(), "config file missing; using defaults + env");
        config_kit::ConfigBuilder::new()
            .layer(config_kit::ConfigLayer::env_prefix("ARGUS_"))
            .load()
    }
}
