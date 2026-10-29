//! Shared application state: bus, hub, caches, health signals.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

use typed_eventbus::{EventBus, EventEnvelope};
use ws_kit::hub::BroadcastHub;

use crate::config::ArgusConfig;
use crate::search::SearchIndex;
use feedkit::{kind_slug, Event, Observation, Quote};

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
    /// Entity ontology built from the live event stream.
    pub ontology: RwLock<ontokit::EntityStore>,
    /// Full-text index over observations.
    pub search: SearchIndex,
    /// JWT service when `auth.jwt_secret` is configured; None = dev mode.
    pub jwt: Option<tokenkit::service::JwtService>,
    /// Alert-rule engine (cooldown state inside).
    pub alerts_engine: Mutex<alertkit::Engine>,
    /// Telegram sink when credentials are configured.
    pub telegram: Option<Arc<alertkit::TelegramSink>>,
    /// Double-entry books (back-office).
    pub books: Mutex<double_entry::Ledger>,
    /// Snapshot file for the books.
    pub books_path: std::path::PathBuf,
    /// Candle history per symbol (klines refresh).
    candles: RwLock<BTreeMap<String, VecDeque<mdkit::Candle>>>,
    /// Last successful poll per source name, epoch ms.
    last_ok_ms: RwLock<HashMap<String, i64>>,
    observations: RwLock<VecDeque<Observation>>,
    quotes: RwLock<BTreeMap<String, Quote>>,
}

impl AppState {
    /// Builds empty state for the given configuration.
    ///
    /// # Errors
    /// Returns the tantivy error when the search index cannot start.
    pub fn new(cfg: ArgusConfig) -> tantivy::Result<Self> {
        let alert_rules = cfg.alerts.rules.clone();
        let books_path = std::path::PathBuf::from(cfg.backoffice.data_dir.clone()).join("books.json");
        Ok(Self {
            cfg,
            bus: EventBus::new(),
            hub: Arc::new(BroadcastHub::new(1024)),
            health: healthkit::HealthRegistry::new(),
            ontology: RwLock::new(ontokit::EntityStore::new()),
            search: SearchIndex::new()?,
            jwt: None,
            alerts_engine: Mutex::new(alertkit::Engine::new(alert_rules)),
            telegram: None,
            books: Mutex::new(crate::backoffice::seed_ledger()),
            books_path,
            candles: RwLock::new(BTreeMap::new()),
            last_ok_ms: RwLock::new(HashMap::new()),
            observations: RwLock::new(VecDeque::with_capacity(OBSERVATION_CAPACITY)),
            quotes: RwLock::new(BTreeMap::new()),
        })
    }

    /// Feeds one observation into the ontology and the search index.
    pub fn ingest_observation(&self, observation: &Observation) {
        let stats = write_tolerant(&self.ontology).ingest_observation(observation);
        if stats.created > 0 {
            let _ = self.search.add(
                &format!("event:{}:{}", observation.source, observation.id),
                &observation.title,
                kind_slug(observation.kind),
                &observation.source,
            );
        }
    }

    /// Records a successful poll for `source`.
    pub fn record_ok(&self, source: &str, at_ms: i64) {
        write_tolerant(&self.last_ok_ms).insert(source.to_string(), at_ms);
    }

    /// Age in ms since the source's last successful poll; very large when
    /// the source has never polled successfully.
    #[must_use]
    pub fn ms_since_ok(&self, source: &str, now_ms: i64) -> i64 {
        let last = *read_tolerant(&self.last_ok_ms).get(source).unwrap_or(&0);
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

    /// Replaces the candle history for `symbol` (klines refresh).
    pub fn set_candles(&self, symbol: &str, batch: Vec<mdkit::Candle>) {
        let mut deque = VecDeque::with_capacity(batch.len());
        deque.extend(batch);
        write_tolerant(&self.candles).insert(symbol.to_string(), deque);
    }

    /// Candle history for `symbol`, oldest first, at most `limit`.
    #[must_use]
    pub fn candles_for(&self, symbol: &str, limit: usize) -> Vec<mdkit::Candle> {
        let guard = read_tolerant(&self.candles);
        match guard.get(symbol) {
            Some(history) => {
                let skip = history.len().saturating_sub(limit);
                history.iter().skip(skip).copied().collect()
            }
            None => Vec::new(),
        }
    }
}

/// `RwLock::write` returns `Result` under poison; recover and carry on — a
/// panicked writer must not take the whole dashboard down.
pub(crate) fn write_tolerant<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match lock.write() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// `RwLock::read` with poison recovery.
pub(crate) fn read_tolerant<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match lock.read() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
