//! Alpaca adapter — latest US-equity trades (IEX feed). Key-gated.

use std::time::Duration;

use serde_json::Value;

use crate::error::MdkitError;
use feedkit::{FeedError, Quote, Source, SourceBatch};

const TRADES_URL: &str = "https://data.alpaca.markets/v2/stocks/trades/latest";

/// Polls Alpaca's latest-trades endpoint for the configured symbols.
/// Implements [`feedkit::Source`] so the standard poller/health plumbing
/// drives it.
pub struct AlpacaQuotes {
    client: fetch_kit::Client,
    symbols: Vec<String>,
}

impl AlpacaQuotes {
    /// Returns an adapter when both credentials are non-empty.
    #[must_use]
    pub fn new(key_id: &str, secret: &str, symbols: Vec<String>) -> Option<Self> {
        let key_id = key_id.trim();
        let secret = secret.trim();
        if key_id.is_empty() || secret.is_empty() || symbols.is_empty() {
            return None;
        }
        let mut headers = reqwest::header::HeaderMap::new();
        let key = reqwest::header::HeaderName::from_static("APCA-API-KEY-ID");
        let sec = reqwest::header::HeaderName::from_static("APCA-API-SECRET-KEY");
        let key_value = reqwest::header::HeaderValue::from_str(key_id).ok()?;
        let secret_value = reqwest::header::HeaderValue::from_str(secret).ok()?;
        headers.insert(key, key_value);
        headers.insert(sec, secret_value);
        Some(Self {
            client: fetch_kit::Client::builder()
                .user_agent("argus-mdkit/0.1")
                .timeout(Duration::from_secs(10))
                .retries(2)
                .default_headers(headers)
                .build(),
            symbols,
        })
    }

    fn quotes_from_payload(value: &Value) -> Result<Vec<Quote>, MdkitError> {
        let trades = value
            .get("trades")
            .and_then(Value::as_object)
            .ok_or_else(|| MdkitError::Malformed("missing `trades` object".to_string()))?;
        let mut out = Vec::new();
        for (symbol, trade) in trades {
            let price = trade
                .get("p")
                .and_then(Value::as_f64)
                .ok_or_else(|| MdkitError::Malformed(format!("missing price for {symbol}")))?;
            let ts_ms = trade
                .get("t")
                .and_then(Value::as_str)
                .and_then(feedkit::rfc3339_to_ms)
                .unwrap_or(0);
            out.push(Quote {
                symbol: symbol.clone(),
                price,
                ts_ms,
            });
        }
        Ok(out)
    }
}

impl Source for AlpacaQuotes {
    fn name(&self) -> &'static str {
        "alpaca"
    }

    async fn poll(&mut self) -> Result<SourceBatch, FeedError> {
        let joined = self.symbols.join(",");
        let url = format!("{TRADES_URL}?symbols={joined}&feed=iex");
        let value = self
            .client
            .get(url)
            .send()
            .await
            .map_err(FeedError::Fetch)?
            .json::<Value>()
            .await
            .map_err(|e| FeedError::Malformed(format!("json decode failed: {e}")))?;
        let quotes = Self::quotes_from_payload(&value)
            .map_err(|e| FeedError::Malformed(e.to_string()))?;
        Ok(SourceBatch::Quotes(quotes))
    }
}
