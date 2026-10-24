//! NVD adapter — recent CVE publications (API 2.0). Keyless tier allows a
//! few requests per 30 s; poll at >=60 s intervals.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;

use crate::error::FeedError;
use crate::event::{now_ms, rfc3339_to_ms, Observation, ObservationKind};
use crate::source::{Source, SourceBatch};

const FEED_URL: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0/?resultsPerPage=30";
const NVD_LICENSE: &str = "NVD (public); CVE terms apply";
const SEEN_CAPACITY: usize = 2_000;

/// Polls the NVD 2.0 CVE feed and emits new CVEs as
/// [`ObservationKind::Cve`] observations with the CVSS base score as
/// `magnitude` when present.
pub struct NvdSource {
    client: fetch_kit::Client,
    seen: HashSet<String>,
}

impl NvdSource {
    /// Creates an adapter with a breaker-protected HTTP client.
    #[must_use]
    pub fn new() -> Self {
        let client = fetch_kit::Client::builder()
            .user_agent("argus-feedkit/0.1")
            .timeout(Duration::from_secs(15))
            .retries(2)
            .build();
        Self {
            client,
            seen: HashSet::new(),
        }
    }

    fn observation_from_cve(&mut self, wrapper: &Value) -> Option<Observation> {
        let cve = wrapper.get("cve")?;
        let id = cve.get("id")?.as_str()?.to_string();
        if !self.seen.insert(id.clone()) {
            return None;
        }
        let title = cve
            .get("descriptions")
            .and_then(Value::as_array)
            .and_then(|ds| {
                ds.iter()
                    .find(|d| d.get("lang").and_then(Value::as_str) == Some("en"))
            })
            .and_then(|d| d.get("value"))
            .and_then(Value::as_str)
            .unwrap_or("vulnerability published")
            .to_string();
        let score = cve
            .get("metrics")
            .and_then(|m| {
                m.get("cvssMetricV31")
                    .or_else(|| m.get("cvssMetricV30"))
                    .or_else(|| m.get("cvssMetricV2"))
            })
            .and_then(Value::as_array)
            .and_then(|m| m.first())
            .and_then(|m| m.get("cvssData"))
            .and_then(|d| d.get("baseScore"))
            .and_then(Value::as_f64);
        let published = cve
            .get("published")
            .and_then(Value::as_str)
            .and_then(rfc3339_to_ms)
            .unwrap_or_else(now_ms);
        Some(Observation {
            id,
            source: "nvd".to_string(),
            kind: ObservationKind::Cve,
            title: truncate(&title, 220),
            place: None,
            lat: None,
            lon: None,
            magnitude: score,
            url: Some(format!("https://nvd.nist.gov/vuln/detail/{}", cve.get("id").and_then(Value::as_str).unwrap_or(""))),
            occurred_at_ms: published,
            fetched_at_ms: now_ms(),
            license: NVD_LICENSE.to_string(),
        })
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let cut: String = value.chars().take(max_chars).collect();
    format!("{cut}...")
}

impl Default for NvdSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for NvdSource {
    fn name(&self) -> &'static str {
        "nvd"
    }

    async fn poll(&mut self) -> Result<SourceBatch, FeedError> {
        let value = self
            .client
            .get(FEED_URL)
            .json_response::<Value>()
            .await
            .map_err(FeedError::Fetch)?;
        let items = value
            .get("vulnerabilities")
            .and_then(Value::as_array)
            .ok_or_else(|| FeedError::Malformed("missing `vulnerabilities` array".to_string()))?;
        let mut out = Vec::new();
        for item in items {
            if let Some(observation) = self.observation_from_cve(item) {
                out.push(observation);
            }
        }
        if self.seen.len() > SEEN_CAPACITY {
            self.seen.clear();
        }
        Ok(SourceBatch::Observations(out))
    }
}
