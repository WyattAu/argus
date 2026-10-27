//! Rule model — config-driven, serde-deserializable straight from TOML.

use serde::Deserialize;

/// A single alert rule.
#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    /// Stable rule id (used in topics and cooldown keys).
    pub id: String,
    /// Human name (used in messages).
    #[serde(default)]
    pub name: String,
    /// Only match events from this source (`usgs`, `binance`, ...).
    #[serde(default)]
    pub source: Option<String>,
    /// Only match observations of this kind (`quake`, `fire`, `cve`, ...).
    #[serde(default)]
    pub kind: Option<String>,
    /// Only match quotes for this symbol (`BTCUSDT`).
    #[serde(default)]
    pub symbol: Option<String>,
    /// The threshold to evaluate.
    pub condition: Condition,
    /// Minimum seconds between fires for the same rule+entity.
    #[serde(default = "default_cooldown_secs")]
    pub cooldown_secs: u64,
}

/// Threshold conditions. Numeric thresholds are >= / <= comparisons against
/// the event's natural magnitude (quake magnitude, CVSS, price).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    /// Observation magnitude at least `value`.
    MagnitudeAtLeast {
        /// Threshold the magnitude must reach.
        value: f64,
    },
    /// Quote price above `value`.
    PriceAbove {
        /// Threshold the price must exceed.
        value: f64,
    },
    /// Quote price below `value`.
    PriceBelow {
        /// Threshold the price must fall under.
        value: f64,
    },
}

fn default_cooldown_secs() -> u64 {
    900
}
