//! Binance public market data — klines over REST, trades over WebSocket.
//! No API key required for either.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::error::MdkitError;
use crate::Candle;

const REST_BASE: &str = "https://api.binance.com";
const WS_BASE: &str = "wss://stream.binance.com:9443/stream";
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Fetches the most recent klines for `symbol` as [`Candle`] batches
/// (oldest first), shaped for direct chart consumption.
///
/// # Errors
/// Returns [`MdkitError`] on HTTP or normalization failure.
pub async fn klines(
    client: &fetch_kit::Client,
    symbol: &str,
    interval: &str,
    limit: u32,
) -> Result<Vec<Candle>, MdkitError> {
    let url = format!("{REST_BASE}/api/v3/klines?symbol={symbol}&interval={interval}&limit={limit}");
    let value = client
        .get(url)
        .json_response::<Value>()
        .await
        .map_err(MdkitError::Fetch)?;
    let rows = value
        .as_array()
        .ok_or_else(|| MdkitError::Malformed("klines payload is not an array".to_string()))?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let cells = row
            .as_array()
            .ok_or_else(|| MdkitError::Malformed("kline row is not an array".to_string()))?;
        // [openTime, open, high, low, close, volume, closeTime, ...]
        let num = |i: usize| -> Option<f64> {
            match cells.get(i) {
                Some(Value::String(s)) => s.parse().ok(),
                Some(Value::Number(n)) => n.as_f64(),
                _ => None,
            }
        };
        let candle = Candle {
            time: num(0).ok_or_else(|| MdkitError::Malformed("missing openTime".to_string()))? / 1000.0,
            open: num(1).ok_or_else(|| MdkitError::Malformed("missing open".to_string()))?,
            high: num(2).ok_or_else(|| MdkitError::Malformed("missing high".to_string()))?,
            low: num(3).ok_or_else(|| MdkitError::Malformed("missing low".to_string()))?,
            close: num(4).ok_or_else(|| MdkitError::Malformed("missing close".to_string()))?,
            volume: num(5).unwrap_or(0.0),
        };
        out.push(candle);
    }
    Ok(out)
}

/// Refreshes klines for `symbol` on a fixed interval forever, handing each
/// batch (oldest first) to `on_candles`. Owns its breaker-protected client.
pub async fn klines_refresh<F>(
    symbol: String,
    interval: String,
    limit: u32,
    refresh: Duration,
    on_candles: F,
) where
    F: Fn(Vec<Candle>) + Send + Sync + 'static,
{
    let client = fetch_kit::Client::builder()
        .user_agent("argus-mdkit/0.1")
        .timeout(Duration::from_secs(10))
        .retries(2)
        .build();
    loop {
        match klines(&client, &symbol, &interval, limit).await {
            Ok(batch) if !batch.is_empty() => on_candles(batch),
            Ok(_) => {}
            Err(error) => tracing::warn!(symbol = %symbol, %error, "klines fetch failed"),
        }
        tokio::time::sleep(refresh).await;
    }
}

/// Streams live trades for `symbols` forever, invoking `on_quote` per trade.
/// Reconnects with capped exponential backoff; answers server pings.
pub async fn trades_ws<F>(symbols: &[String], on_quote: F)
where
    F: Fn(feedkit::Quote) + Send + Sync + 'static,
{
    if symbols.is_empty() {
        return;
    }
    let streams = symbols
        .iter()
        .map(|s| format!("{}@trade", s.to_lowercase()))
        .collect::<Vec<_>>()
        .join("/");
    let url = format!("{WS_BASE}?streams={streams}");
    let mut backoff = Duration::from_secs(2);
    loop {
        let request = url
            .as_str()
            .into_client_request()
            .expect("static binance url is a valid request");
        tracing::info!(uri = %request.uri(), "binance ws dialing");
        match tokio_tungstenite::connect_async(request).await {
            Ok((ws, _)) => {
                tracing::info!("binance ws connected");
                backoff = Duration::from_secs(2);
                stream_session(ws, &on_quote).await;
                tracing::warn!("binance trade stream closed; reconnecting");
            }
            Err(error) => {
                tracing::warn!(%error, "binance ws connect failed");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn stream_session<F, S>(ws: tokio_tungstenite::WebSocketStream<S>, on_quote: &F)
where
    F: Fn(feedkit::Quote),
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    let mut seen = 0u32;
    while let Some(message) = read.next().await {
        match message {
            Ok(Message::Text(text)) => {
                seen += 1;
                if seen <= 2 {
                    tracing::info!(frame = %text, "binance frame");
                }
                match parse_trade(&text) {
                    Some(quote) => on_quote(quote),
                    None => {
                        tracing::warn!(frame = %text, "binance trade parse failed")
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = write.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(_)) | Err(_) => return,
            Ok(other) => {
                tracing::info!(?other, "binance ws other frame")
            }
        }
    }
}

/// Combined-stream envelope: `{"stream":"btcusdt@trade","data":{...}}`
/// where data is a trade event with `s` (symbol), `p` (price), `T` (ms).
fn parse_trade(text: &str) -> Option<feedkit::Quote> {
    let value: Value = serde_json::from_str(text).ok()?;
    let data = value.get("data").unwrap_or(&value);
    let symbol = data.get("s")?.as_str()?;
    let price: f64 = data.get("p")?.as_str()?.parse().ok()?;
    let ts_ms = data.get("T").and_then(Value::as_i64).unwrap_or(0);
    Some(feedkit::Quote {
        symbol: symbol.to_string(),
        price,
        ts_ms,
    })
}
