//! Shared application state: bus, hub, caches, health signals.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use typed_eventbus::{EventBus, EventEnvelope};
use ws_kit::hub::BroadcastHub;

use crate::config::ArgusConfig;
use feedkit::{Event, Observation, Quote};

/// Ring-buffer capacity for recent observations.
const OBSERVATION_CAPACITY: usize = 512;

/// State shared by handlers, pollers and the bus->hub bridge.
pub struct AppState {
    /// Active configuration.
    pub cfg: ArgusConfig,
    /// Typed event bus — the single source of truth for everything moving.
    pub bus: EventBus<Event>,
    /// WebSocket fan-out mirror of the bus.
    pub hub: Arc<BroadcastHub<EventEnvelope<Event>>>,
    /// Health registry backing `/readyz`.
    pub health: healthkit::HealthRegistry,
    last_sim_ok_ms: AtomicI64,
    last_usgs_ok_ms: AtomicI64,
    observations: RwLock<VecDeque<Observation>>,
    quotes: RwLock<BTreeMap<String, Quote>>,
}

/// Index of a source for liveness tracking.
#[derive(Debug, Clone, Copy)]
pub enum SourceId {
    /// Synthetic market feed.
    Sim,
    /// USGS quake feed.
    Usgs,
}

impl AppState {
    /// Builds empty state for the given configuration.
    #[must_use]
    pub fn new(cfg: ArgusConfig) -> Self {
        Self {
            cfg,
            bus: EventBus::new(),
            hub: Arc::new(BroadcastHub::new(1024)),
            health: healthkit::HealthRegistry::new(),
            last_sim_ok_ms: AtomicI64::new(0),
            last_usgs_ok_ms: AtomicI64::new(0),
            observations: RwLock::new(VecDeque::with_capacity(OBSERVATION_CAPACITY)),
            quotes: RwLock::new(BTreeMap::new()),
        }
    }

    /// Records a successful poll for `source`.
    pub fn record_ok(&self, source: SourceId, at_ms: i64) {
        match source {
            SourceId::Sim => self.last_sim_ok_ms.store(at_ms, Ordering::Relaxed),
            SourceId::Usgs => self.last_usgs_ok_ms.store(at_ms, Ordering::Relaxed),
        }
    }

    /// Age in ms since the source's last successful poll; very large when
    /// the source has never polled successfully.
    #[must_use]
    pub fn ms_since_ok(&self, source: SourceId, now_ms: i64) -> i64 {
        let last = match source {
            SourceId::Sim => self.last_sim_ok_ms.load(Ordering::Relaxed),
            SourceId::Usgs => self.last_usgs_ok_ms.load(Ordering::Relaxed),
        };
        if last == 0 {
            return i64::MAX / 2;
        }
        now_ms - last
    }

    /// Pushes an observation onto the front of the recent ring buffer.
    pub fn push_observation(&self, observation: Observation) {
        let mut guard = write_tolerant(&self.observations);
        while guard.len() >= OBSERVATION_CAPACITY {
            guard.pop_back();
        }
        guard.push_front(observation);
    }

    /// Returns up to `limit` most-recent observations, newest first.
    #[must_use]
    pub fn recent_observations(&self, limit: usize) -> Vec<Observation> {
        let guard = read_tolerant(&self.observations);
        guard.iter().take(limit).cloned().collect()
    }

    /// Inserts or updates a quote.
    pub fn upsert_quote(&self, quote: Quote) {
        write_tolerant(&self.quotes).insert(quote.symbol.clone(), quote);
    }

    /// Snapshot of the latest quote per symbol, symbol-ordered.
    #[must_use]
    pub fn quotes_snapshot(&self) -> BTreeMap<String, Quote> {
        read_tolerant(&self.quotes).clone()
    }
}

/// `RwLock::write` returns `Result` under poison; recover and carry on — a
/// panicked writer must not take the whole dashboard down.
fn write_tolerant<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// `RwLock::read` with poison recovery.
fn read_tolerant<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
