//! News adapter — broadcaster RSS (default: BBC World). No key.

use std::collections::HashSet;
use std::time::Duration;

use crate::error::FeedError;
use crate::event::{now_ms, rfc2822_to_ms, Observation, ObservationKind};
use crate::source::{Source, SourceBatch};

const DEFAULT_FEED_URL: &str = "https://feeds.bbci.co.uk/news/world/rss.xml";
const SEEN_CAPACITY: usize = 1_000;

/// Polls an RSS feed and emits [`ObservationKind::News`] observations.
/// The feed URL and license line are configurable.
pub struct NewsSource {
    client: fetch_kit::Client,
    url: String,
    license: String,
    seen: HashSet<String>,
}

impl NewsSource {
    /// Adapter for a specific RSS feed.
    #[must_use]
    pub fn new(url: String, license: String) -> Self {
        Self {
            client: fetch_kit::Client::builder()
                .user_agent("argus-feedkit/0.1")
                .timeout(Duration::from_secs(15))
                .retries(2)
                .build(),
            url,
            license,
            seen: HashSet::new(),
        }
    }

    /// Default BBC World feed.
    #[must_use]
    pub fn bbc_world() -> Self {
        Self::new(
            DEFAULT_FEED_URL.to_string(),
            "(c) BBC — headline + link only, attribution required".to_string(),
        )
    }
}

impl Source for NewsSource {
    fn name(&self) -> &'static str {
        "news"
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
        let channel =
            rss::Channel::read_from(body.as_bytes()).map_err(|e| FeedError::Malformed(format!("rss parse failed: {e}")))?;
        let mut out = Vec::new();
        for item in channel.items() {
            let Some(link) = item.link() else {
                continue;
            };
            if !self.seen.insert(link.to_string()) {
                continue;
            }
            let title = item.title().unwrap_or("(untitled)").to_string();
            let occurred_at_ms = item
                .pub_date()
                .and_then(rfc2822_to_ms)
                .unwrap_or_else(now_ms);
            out.push(Observation {
                id: format!("news-{:x}", fnv1a(link)),
                source: "news".to_string(),
                kind: ObservationKind::News,
                title,
                place: None,
                lat: None,
                lon: None,
                magnitude: None,
                url: Some(link.to_string()),
                occurred_at_ms,
                fetched_at_ms: now_ms(),
                license: self.license.clone(),
            });
        }
        if self.seen.len() > SEEN_CAPACITY {
            self.seen.clear();
        }
        Ok(SourceBatch::Observations(out))
    }
}

/// FNV-1a 64-bit — stable ids for link-less dedupe.
fn fnv1a(value: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
