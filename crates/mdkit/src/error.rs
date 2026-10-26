//! mdkit errors.

/// Errors produced by market-data adapters.
#[derive(Debug, thiserror::Error)]
pub enum MdkitError {
    /// Upstream HTTP fetch failed (after fetch-kit retries/breaker).
    #[error("fetch failed: {0}")]
    Fetch(#[from] fetch_kit::FetchError),

    /// Feed-level error surfaced by a poll-based adapter.
    #[error("feed error: {0}")]
    Feed(#[from] feedkit::FeedError),

    /// Upstream responded but the payload could not be normalized.
    #[error("malformed payload: {0}")]
    Malformed(String),

    /// WebSocket transport failure (reconnect handled by the caller loop).
    #[error("websocket error: {0}")]
    Ws(String),
}
