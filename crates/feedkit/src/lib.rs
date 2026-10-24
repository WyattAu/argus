//! feedkit — normalized open-source feed adapters for the argus platform.
//!
//! Every adapter implements [`Source`]: a `poll()` that returns a
//! [`SourceBatch`] of already-normalized [`Observation`] or [`Quote`]
//! items. The platform publishes them onto `eventbus-kit` topics
//! (`intel.<source>.<kind>` / `md.<source>.<symbol>`) with source,
//! timestamp and license carried in the payload, so attribution is data,
//! not documentation.

#![forbid(unsafe_code)]

pub mod event;
mod error;
mod eonet;
mod firms;
mod news;
mod nvd;
mod sim;
mod source;
mod swpc;
mod usgs;

pub use error::FeedError;
pub use event::{
    kind_slug, now_ms, rfc2822_to_ms, rfc3339_to_ms, Event, Observation, ObservationKind, Quote,
};
pub use eonet::EonetSource;
pub use firms::FirmsSource;
pub use news::NewsSource;
pub use nvd::NvdSource;
pub use sim::SimMarket;
pub use source::{Source, SourceBatch};
pub use swpc::SwpcSource;
pub use usgs::UsgsSource;
