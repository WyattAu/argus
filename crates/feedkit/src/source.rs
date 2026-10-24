//! The [`Source`] contract every adapter implements.

use std::future::Future;

use crate::error::FeedError;
use crate::event::{Observation, Quote};

/// A batch of normalized items from one poll.
#[derive(Debug, Clone)]
pub enum SourceBatch {
    /// World observations.
    Observations(Vec<Observation>),
    /// Market quotes.
    Quotes(Vec<Quote>),
}

/// A pollable feed adapter.
///
/// `poll` is called on a fixed interval by the platform; adapters keep their
/// own dedupe state and must be cheap when nothing changed. Adapters are not
/// expected to be cloneable or shared — the platform owns one instance per
/// task. The returned future is `Send`, so pollers run happily on the
/// multi-threaded runtime.
pub trait Source: Send {
    /// Stable adapter name (used in topics and health checks).
    fn name(&self) -> &'static str;

    /// Poll the upstream source once.
    ///
    /// # Errors
    /// Returns [`FeedError`] when the upstream fetch or normalization fails.
    /// Transient upstream failures are expected; the platform logs and
    /// retries on the next tick.
    fn poll(&mut self) -> impl Future<Output = Result<SourceBatch, FeedError>> + Send;
}
