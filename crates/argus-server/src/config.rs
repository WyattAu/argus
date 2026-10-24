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
// Seven toggles mirror the flat `[feeds]` TOML block; grouping them would
// churn the config file users already have for no real gain.
#[allow(clippy::struct_excessive_bools)]
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
    /// NASA EONET natural events enabled.
    #[serde(default = "default_true")]
    pub eonet_enabled: bool,
    /// EONET poll interval, seconds.
    #[serde(default = "default_eonet_interval_secs")]
    pub eonet_interval_secs: u64,
    /// NOAA SWPC space-weather alerts enabled.
    #[serde(default = "default_true")]
    pub swpc_enabled: bool,
    /// SWPC poll interval, seconds.
    #[serde(default = "default_swpc_interval_secs")]
    pub swpc_interval_secs: u64,
    /// NVD recent CVEs enabled.
    #[serde(default = "default_true")]
    pub nvd_enabled: bool,
    /// NVD poll interval, seconds (keyless tier: keep >= 60).
    #[serde(default = "default_nvd_interval_secs")]
    pub nvd_interval_secs: u64,
    /// Broadcaster news RSS enabled.
    #[serde(default = "default_true")]
    pub news_enabled: bool,
    /// News poll interval, seconds.
    #[serde(default = "default_news_interval_secs")]
    pub news_interval_secs: u64,
    /// NASA FIRMS hotspots enabled (needs a `MAP_KEY`).
    #[serde(default)]
    pub firms_enabled: bool,
    /// NASA FIRMS `MAP_KEY` (free account); or set `ARGUS_FEEDS_FIRMS_MAP_KEY`.
    #[serde(default)]
    pub firms_map_key: String,
    /// FIRMS poll interval, seconds.
    #[serde(default = "default_firms_interval_secs")]
    pub firms_interval_secs: u64,
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
            eonet_enabled: true,
            eonet_interval_secs: default_eonet_interval_secs(),
            swpc_enabled: true,
            swpc_interval_secs: default_swpc_interval_secs(),
            nvd_enabled: true,
            nvd_interval_secs: default_nvd_interval_secs(),
            news_enabled: true,
            news_interval_secs: default_news_interval_secs(),
            firms_enabled: false,
            firms_map_key: String::new(),
            firms_interval_secs: default_firms_interval_secs(),
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

fn default_eonet_interval_secs() -> u64 {
    300
}

fn default_swpc_interval_secs() -> u64 {
    120
}

fn default_nvd_interval_secs() -> u64 {
    120
}

fn default_news_interval_secs() -> u64 {
    300
}

fn default_firms_interval_secs() -> u64 {
    300
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
