//! mdkit — market-data adapters for the argus platform.
//!
//! Candles use [`plycore::CandleData`] directly, so chart panels can later
//! hand the same batches to plychart without conversion. Live trades emit
//! [`feedkit::Quote`] onto the platform bus through a caller-supplied
//! callback; this crate stays eventbus-agnostic.
//!
//! Adapters:
//! - [`binance`]: public REST klines + trade WebSocket (no API key)
//! - [`alpaca`]: key-gated latest-trades poller (US equities, IEX feed)

#![forbid(unsafe_code)]

pub mod alpaca;
pub mod binance;
mod error;

pub use alpaca::AlpacaQuotes;
pub use error::MdkitError;

/// Candle alias over the estate's shared OHLCV type.
pub type Candle = plycore::CandleData;
