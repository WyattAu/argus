//! NASA EONET adapter — open natural-event feed (wildfires, storms,
//! volcanoes, ice). No API key.

use std::collections::HashSet;

use serde_json::Value;

use crate::error::FeedError;
use crate::event::{now_ms, rfc3339_to_ms, Observation, ObservationKind};
use crate::source::{Source, SourceBatch};

const FEED_URL: &str = "https://eonet.gsfc.nasa.gov/api/v3/events?status=open&limit=50";
const EONET_LICENSE: &str = "NASA EONET (public, attribution requested)";
const SEEN_CAPACITY: usize = 2_000;

/// Polls open EONET events and maps categories onto [`ObservationKind`].
pub struct EonetSource {
    client: fetch_kit::Client,
    seen: HashSet<String>,
}

impl EonetSource {
    /// Creates an adapter with a breaker-protected HTTP client.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: crate::usgs::default_client(),
            seen: HashSet::new(),
        }
    }

    fn observation_from_event(&mut self, event: &Value) -> Option<Observation> {
        let id = event.get("id")?.as_str()?.to_string();
        if !self.seen.insert(id.clone()) {
            return None;
        }
        let title = event
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("EONET event")
            .to_string();
        let category = event
            .get("categories")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
            .and_then(|c| c.get("id"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let kind = match category {
            "wildfires" => ObservationKind::Fire,
            "severeStorms" => ObservationKind::Weather,
            "volcanoes" => ObservationKind::Volcano,
            _ => ObservationKind::News,
        };
        // Take the first point geometry; EONET may attach several.
        let geometry = event
            .get("geometry")
            .and_then(Value::as_array)
            .and_then(|g| g.first());
        let (lon, lat, occurred) = match geometry {
            Some(g) => {
                let coords = g.get("coordinates").and_then(Value::as_array);
                (
                    coords.and_then(|c| c.first()).and_then(Value::as_f64),
                    coords.and_then(|c| c.get(1)).and_then(Value::as_f64),
                    g.get("date").and_then(Value::as_str).and_then(rfc3339_to_ms),
                )
            }
            None => (None, None, None),
        };
        let url = event
            .get("sources")
            .and_then(Value::as_array)
            .and_then(|s| s.first())
            .and_then(|s| s.get("url"))
            .and_then(Value::as_str)
            .map(String::from);
        let occurred_at_ms = occurred.unwrap_or_else(now_ms);
        Some(Observation {
            id,
            source: "eonet".to_string(),
            kind,
            title,
            place: None,
            lat,
            lon,
            magnitude: None,
            url,
            occurred_at_ms,
            fetched_at_ms: now_ms(),
            license: EONET_LICENSE.to_string(),
        })
    }
}

impl Default for EonetSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for EonetSource {
    fn name(&self) -> &'static str {
        "eonet"
    }

    async fn poll(&mut self) -> Result<SourceBatch, FeedError> {
        let value = self
            .client
            .get(FEED_URL)
            .json_response::<Value>()
            .await
            .map_err(FeedError::Fetch)?;
        let mut out = Vec::new();
        if let Some(events) = value.get("events").and_then(Value::as_array) {
            for event in events {
                if let Some(observation) = self.observation_from_event(event) {
                    out.push(observation);
                }
            }
        } else {
            return Err(FeedError::Malformed(
                "missing `events` array in EONET payload".to_string(),
            ));
        }
        if self.seen.len() > SEEN_CAPACITY {
            self.seen.clear();
        }
        Ok(SourceBatch::Observations(out))
    }
}
