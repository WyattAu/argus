//! Poller tasks and the bus -> (state, hub) bridge.

use std::sync::Arc;
use std::time::Duration;

use typed_eventbus::{EventBus, EventEnvelope};
use tokio::time::MissedTickBehavior;
use ws_kit::hub::BroadcastHub;

use crate::state::AppState;
use feedkit::{kind_slug, Event, Source, SourceBatch};

/// Runs one source on a fixed interval, publishing every batch onto the bus.
pub async fn run_source<S: Source>(state: Arc<AppState>, mut source: S, interval: Duration) {
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        match source.poll().await {
            Ok(batch) => {
                state.record_ok(source.name(), feedkit::now_ms());
                publish_batch(&state.bus, source.name(), batch).await;
            }
            Err(error) => {
                tracing::warn!(source = source.name(), %error, "feed poll failed");
            }
        }
    }
}

/// Bridges bus events into shared state and the WebSocket hub.
/// Subscriptions use wildcard patterns: `intel.**` and `md.**`.
pub async fn bridge_bus(state: Arc<AppState>) {
    let hub: Arc<BroadcastHub<EventEnvelope<Event>>> = Arc::clone(&state.hub);
    let st = Arc::clone(&state);
    state
        .bus
        .subscribe_sync("intel.**", move |envelope| {
            if let Event::Observation(observation) = &envelope.payload {
                st.push_observation(observation.clone());
            }
            let _ = hub.broadcast((*envelope).clone());
        })
        .await;

    let hub: Arc<BroadcastHub<EventEnvelope<Event>>> = Arc::clone(&state.hub);
    let st = Arc::clone(&state);
    state
        .bus
        .subscribe_sync("md.**", move |envelope| {
            if let Event::Quote(quote) = &envelope.payload {
                st.upsert_quote(quote.clone());
            }
            let _ = hub.broadcast((*envelope).clone());
        })
        .await;
}

async fn publish_batch(bus: &EventBus<Event>, source: &str, batch: SourceBatch) {
    match batch {
        SourceBatch::Observations(items) => {
            for observation in items {
                let topic = format!("intel.{source}.{}", kind_slug(observation.kind));
                publish(bus, &topic, Event::Observation(observation)).await;
            }
        }
        SourceBatch::Quotes(items) => {
            for quote in items {
                let topic = format!("md.{source}.{}", quote.symbol.to_lowercase());
                publish(bus, &topic, Event::Quote(quote)).await;
            }
        }
    }
}

async fn publish(bus: &EventBus<Event>, topic: &str, event: Event) {
    if let Err(error) = bus.publish(topic.to_string(), event).await {
        tracing::warn!(topic, %error, "bus publish failed");
    }
}

