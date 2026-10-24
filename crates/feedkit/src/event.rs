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
    /// Wildfire hotspot / active fire.
    Fire,
    /// Severe weather or space weather.
    Weather,
    /// Volcanic activity.
    Volcano,
    /// Published vulnerability.
    Cve,
    /// News item.
    News,
    /// Synthetic/demo event.
    Sim,
}

/// Human-facing slug for a category.
#[must_use]
pub fn kind_slug(kind: ObservationKind) -> &'static str {
    match kind {
        ObservationKind::Quake => "quake",
        ObservationKind::Fire => "fire",
        ObservationKind::Weather => "weather",
        ObservationKind::Volcano => "volcano",
        ObservationKind::Cve => "cve",
        ObservationKind::News => "news",
        ObservationKind::Sim => "sim",
    }
}

/// Parses an RFC 3339 timestamp (most feeds) to epoch milliseconds.
#[must_use]
pub fn rfc3339_to_ms(value: &str) -> Option<i64> {
    let dt = time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()?;
    epoch_ms(dt)
}

/// Parses an RFC 2822 timestamp (RSS `pubDate`) to epoch milliseconds.
#[must_use]
pub fn rfc2822_to_ms(value: &str) -> Option<i64> {
    let dt = time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc2822)
        .ok()?;
    epoch_ms(dt)
}

fn epoch_ms(dt: time::OffsetDateTime) -> Option<i64> {
    let secs = dt.unix_timestamp();
    let nanos = i64::from(dt.nanosecond());
    secs.checked_mul(1000)
        .and_then(|ms| ms.checked_add(nanos / 1_000_000))
}
