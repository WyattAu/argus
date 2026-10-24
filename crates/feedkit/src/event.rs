//! Normalized event payloads shared by every adapter.

use serde::{Deserialize, Serialize};

/// Wall-clock milliseconds since the Unix epoch.
#[must_use]
pub fn now_ms() -> i64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    i64::try_from(millis).unwrap_or(i64::MAX)
}

/// A normalized item observed in the world (news, quake, CVE, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Stable source-local identifier (used for dedupe).
    pub id: String,
    /// Adapter name that produced this observation.
    pub source: String,
    /// Coarse category.
    pub kind: ObservationKind,
    /// Human headline.
    pub title: String,
    /// Free-text place, when the source provides one.
    pub place: Option<String>,
    /// Latitude (WGS84), when geoparsed.
    pub lat: Option<f64>,
    /// Longitude (WGS84), when geoparsed.
    pub lon: Option<f64>,
    /// Source-specific magnitude/severity (quake magnitude, CVSS, ...).
    pub magnitude: Option<f64>,
    /// Canonical link back to the origin.
    pub url: Option<String>,
    /// When the event happened, per the source.
    pub occurred_at_ms: i64,
    /// When we fetched it.
    pub fetched_at_ms: i64,
    /// Data license/attribution requirement, carried as data.
    pub license: String,
}

/// A market quote from a real or simulated venue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    /// Symbol in venue-neutral form (`AAPL`, `BTCUSD`).
    pub symbol: String,
    /// Last/trade price.
    pub price: f64,
    /// Quote timestamp, epoch milliseconds.
    pub ts_ms: i64,
}

/// Everything that can flow through the bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// World observation.
    Observation(Observation),
    /// Market quote.
    Quote(Quote),
}

/// Coarse observation category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    /// Seismic event.
    Quake,
    /// Wildfire hotspot.
    Fire,
    /// Severe weather / space weather.
    Weather,
    /// Published vulnerability.
    Cve,
    /// News item.
    News,
    /// Synthetic/demo event.
    Sim,
}
