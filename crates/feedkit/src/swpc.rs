//! NOAA SWPC adapter — space-weather alerts, watches and warnings. No key.

use std::collections::HashSet;

use serde_json::Value;

use crate::error::FeedError;
use crate::event::{now_ms, rfc3339_to_ms, Observation, ObservationKind};
use crate::source::{Source, SourceBatch};

const FEED_URL: &str = "https://services.swpc.noaa.gov/products/alerts.json";
const INFO_URL: &str = "https://www.swpc.noaa.gov/products/alerts-watches-and-warnings";
const SWPC_LICENSE: &str = "NOAA SWPC (U.S. Government work, public domain)";
const SEEN_CAPACITY: usize = 1_000;

/// Polls SWPC alerts; each alert becomes a [`ObservationKind::Weather`]
/// observation keyed by `product_id + issue_datetime`.
pub struct SwpcSource {
    client: fetch_kit::Client,
    seen: HashSet<String>,
}

impl SwpcSource {
    /// Creates an adapter with a breaker-protected HTTP client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: crate::usgs::default_client(),
            seen: HashSet::new(),
        }
    }
}

impl Default for SwpcSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for SwpcSource {
    fn name(&self) -> &'static str {
        "swpc"
    }

    async fn poll(&mut self) -> Result<SourceBatch, FeedError> {
        let value = self
            .client
            .get(FEED_URL)
            .json_response::<Value>()
            .await
            .map_err(FeedError::Fetch)?;
        let alerts = value
            .as_array()
            .ok_or_else(|| FeedError::Malformed("SWPC payload is not an array".to_string()))?;
        let mut out = Vec::new();
        for alert in alerts {
            let product_id = alert.get("product_id").and_then(Value::as_str).unwrap_or("");
            let issued = alert
                .get("issue_datetime")
                .and_then(Value::as_str)
                .unwrap_or("");
            let key = format!("{product_id}@{issued}");
            if product_id.is_empty() || !self.seen.insert(key) {
                continue;
            }
            let message = alert
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("space weather alert");
            let title = message.lines().next().unwrap_or(message).to_string();
            let occurred_at_ms = rfc3339_to_ms(issued).unwrap_or_else(now_ms);
            out.push(Observation {
                id: format!("swpc-{issued}-{product_id}"),
                source: "swpc".to_string(),
                kind: ObservationKind::Weather,
                title,
                place: None,
                lat: None,
                lon: None,
                magnitude: None,
                url: Some(INFO_URL.to_string()),
                occurred_at_ms,
                fetched_at_ms: now_ms(),
                license: SWPC_LICENSE.to_string(),
            });
        }
        if self.seen.len() > SEEN_CAPACITY {
            self.seen.clear();
        }
        Ok(SourceBatch::Observations(out))
    }
}
