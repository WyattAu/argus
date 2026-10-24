//! USGS earthquake adapter — `all_hour` `GeoJSON` summary feed, no API key.
//!
//! Emits [`Observation`]s for each new quake.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;

use crate::error::FeedError;
use crate::event::{now_ms, Observation, ObservationKind};
use crate::source::{Source, SourceBatch};

const FEED_URL: &str = "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_hour.geojson";
const SEEN_CAPACITY: usize = 10_000;
const USGS_LICENSE: &str = "USGS public domain (attribution requested)";

/// Breaker-protected HTTP client shared by the public-feed adapters.
#[must_use]
pub fn default_client() -> fetch_kit::Client {
    let breaker_cfg = breaker::CircuitBreakerConfigBuilder::default()
        .failure_rate_threshold(0.5)
        .wait_duration(Duration::from_secs(10))
        .build();
    fetch_kit::Client::builder()
        .user_agent("argus-feedkit/0.1")
        .timeout(Duration::from_secs(10))
        .retries(2)
        .with_breaker(breaker::CircuitBreaker::new(breaker_cfg))
        .build()
}

/// Polls the USGS `all_hour` earthquake feed and emits new quakes as
/// [`Observation`]s. Deduplicates by USGS event id across polls.
pub struct UsgsSource {
    client: fetch_kit::Client,
    seen: HashSet<String>,
}

impl UsgsSource {
    /// Creates an adapter with a breaker-protected HTTP client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: default_client(),
            seen: HashSet::new(),
        }
    }

    fn observation_from_feature(&mut self, feature: &Value) -> Option<Observation> {
        let id = feature.get("id")?.as_str()?.to_string();
        if !self.seen.insert(id.clone()) {
            return None;
        }
        let props = feature.get("properties")?;
        let title = props
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("earthquake")
            .to_string();
        let coordinates = feature
            .get("geometry")
            .and_then(|g| g.get("coordinates"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let lon = coordinates.first().and_then(Value::as_f64);
        let lat = coordinates.get(1).and_then(Value::as_f64);
        let occurred_at_ms = props
            .get("time")
            .and_then(Value::as_i64)
            .unwrap_or_else(now_ms);
        Some(Observation {
            id,
            source: "usgs".to_string(),
            kind: ObservationKind::Quake,
            title,
            place: props.get("place").and_then(Value::as_str).map(String::from),
            lat,
            lon,
            magnitude: props.get("mag").and_then(Value::as_f64),
            url: props.get("url").and_then(Value::as_str).map(String::from),
            occurred_at_ms,
            fetched_at_ms: now_ms(),
            license: USGS_LICENSE.to_string(),
        })
    }
}

impl Default for UsgsSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for UsgsSource {
    fn name(&self) -> &'static str {
        "usgs"
    }

    #[allow(clippy::future_not_send)] // adapter state is Send; reqwest futures are Send
    async fn poll(&mut self) -> Result<SourceBatch, FeedError> {
        let value = self
            .client
            .get(FEED_URL)
            .json_response::<Value>()
            .await
            .map_err(FeedError::Fetch)?;
        let mut out = Vec::new();
        if let Some(features) = value.get("features").and_then(Value::as_array) {
            for feature in features {
                if let Some(observation) = self.observation_from_feature(feature) {
                    out.push(observation);
                }
            }
        } else {
            return Err(FeedError::Malformed(
                "missing `features` array in GeoJSON".to_string(),
            ));
        }
        if self.seen.len() > SEEN_CAPACITY {
            self.seen.clear();
        }
        Ok(SourceBatch::Observations(out))
    }
}
