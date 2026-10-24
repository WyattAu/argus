//! Adapter errors.

/// Errors produced by feed adapters.
#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    /// Upstream HTTP fetch failed (after fetch-kit retries/breaker).
    #[error("fetch failed: {0}")]
    Fetch(#[from] fetch_kit::FetchError),

    /// Upstream responded but the payload could not be normalized.
    #[error("malformed payload: {0}")]
    Malformed(String),
}
