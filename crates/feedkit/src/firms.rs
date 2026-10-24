//! NASA FIRMS adapter — VIIRS active-fire hotspots (CSV area API).
//! Requires a free `MAP_KEY` (<https://firms.modaps.eosdis.nasa.gov/api/area/>).

use std::collections::HashSet;

use crate::error::FeedError;
use crate::event::{now_ms, Observation, ObservationKind};
use crate::source::{Source, SourceBatch};

const AREA_URL: &str =
    "https://firms.modaps.eosdis.nasa.gov/api/area/csv/{KEY}/VIIRS_SNPP_NRT/world/2";
const FIRMS_LICENSE: &str = "NASA FIRMS (NRT VIIRS); NASA Earth science data — attribute NASA";
const SEEN_CAPACITY: usize = 20_000;

/// Polls the FIRMS world hotspots CSV when a `MAP_KEY` is configured.
pub struct FirmsSource {
    client: fetch_kit::Client,
    url: String,
    seen: HashSet<String>,
}

impl FirmsSource {
    /// Returns an adapter when `map_key` is non-empty, else `None`.
    /// `map_key` comes from the free NASA FIRMS `MAP_KEY`.
    #[must_use]
    pub fn new(map_key: &str) -> Option<Self> {
        let key = map_key.trim();
        if key.is_empty() {
            return None;
        }
        Some(Self {
            client: crate::usgs::default_client(),
            url: AREA_URL.replace("{KEY}", key),
            seen: HashSet::new(),
        })
    }

    fn observation_from_row(&mut self, row: &str) -> Option<Observation> {
        // CSV: latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,
        //      satellite,instrument,confidence,version,bright_ti5,frp,daynight
        let mut cols = row.split(',');
        let lat: f64 = cols.next()?.trim().parse().ok()?;
        let lon: f64 = cols.next()?.trim().parse().ok()?;
        let _bright = cols.next();
        let _scan = cols.next();
        let _track = cols.next();
        let acq_date = cols.next()?.trim();
        let acq_time = cols.next()?.trim();
        // satellite, instrument, confidence, version, bright_ti5, then frp.
        let frp: Option<f64> = cols.nth(5).and_then(|v| v.trim().parse().ok());
        let acquired = format!("{acq_date}T{acq_time}:00Z");
        let occurred_at_ms = crate::event::rfc3339_to_ms(&acquired).unwrap_or_else(now_ms);
        // Dedupe on location+time; hotspot rows repeat across passes.
        let id = format!("firms-{lat:.3}-{lon:.3}-{acquired}");
        if !self.seen.insert(id.clone()) {
            return None;
        }
        Some(Observation {
            id,
            source: "firms".to_string(),
            kind: ObservationKind::Fire,
            title: format!("Active fire hotspot ({lat:.2}, {lon:.2})"),
            place: None,
            lat: Some(lat),
            lon: Some(lon),
            magnitude: frp,
            url: Some("https://firms.modaps.eosdis.nasa.gov/map/".to_string()),
            occurred_at_ms,
            fetched_at_ms: now_ms(),
            license: FIRMS_LICENSE.to_string(),
        })
    }
}

impl Source for FirmsSource {
    fn name(&self) -> &'static str {
        "firms"
    }

    async fn poll(&mut self) -> Result<SourceBatch, FeedError> {
        let body = self
            .client
            .get(self.url.clone())
            .send()
            .await
            .map_err(FeedError::Fetch)?
            .text()
            .await
            .map_err(|e| FeedError::Malformed(format!("body read failed: {e}")))?;
        if body.starts_with("Invalid") || body.contains("Invalid MAP_KEY") {
            return Err(FeedError::Malformed("FIRMS rejected the MAP_KEY".to_string()));
        }
        let mut out = Vec::new();
        for line in body.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(observation) = self.observation_from_row(line) {
                out.push(observation);
            }
        }
        if self.seen.len() > SEEN_CAPACITY {
            self.seen.clear();
        }
        Ok(SourceBatch::Observations(out))
    }
}
