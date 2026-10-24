//! Deterministic simulated market feed — drives chart/quote panels without
//! any upstream dependency. xorshift64* PRNG, fixed seed: runs are
//! reproducible for demos and tests.

use std::future::Future;
use crate::error::FeedError;
use crate::event::{now_ms, Quote};
use crate::source::{Source, SourceBatch};

/// Emits a geometric random walk per symbol on every poll.
pub struct SimMarket {
    symbols: Vec<(String, f64)>,
    rng: u64,
}

impl SimMarket {
    /// Three representative symbols across equities/crypto/FX.
    #[must_use]
    pub fn new() -> Self {
        Self {
            symbols: vec![
                ("AAPL".to_string(), 232.50),
                ("BTCUSD".to_string(), 67_400.0),
                ("EURUSD".to_string(), 1.0815),
            ],
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// xorshift64* — deterministic, dependency-free.
    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl Default for SimMarket {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for SimMarket {
    fn name(&self) -> &'static str {
        "sim"
    }

    fn poll(&mut self) -> impl Future<Output = Result<SourceBatch, FeedError>> {
        let ts = now_ms();
        // Pre-roll steps so the price mutation below does not alias `self`.
        let steps: Vec<i64> = (0..self.symbols.len())
            .map(|_| i64::try_from(self.next_rand() % 2001).unwrap_or(0) - 1000)
            .collect();
        let quotes = self
            .symbols
            .iter_mut()
            .zip(steps)
            .map(|((symbol, price), step)| {
                // +/- 0.01% per tick.
                *price *= 1.0 + f64::from(i32::try_from(step).unwrap_or(0)) / 100_000.0;
                Quote {
                    symbol: symbol.clone(),
                    price: *price,
                    ts_ms: ts,
                }
            })
            .collect();
        std::future::ready(Ok(SourceBatch::Quotes(quotes)))
    }
}
