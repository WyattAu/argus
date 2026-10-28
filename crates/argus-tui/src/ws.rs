//! WebSocket + REST seed for the TUI, with reconnect and stale marking.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use crate::state::TuiState;

const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Runs forever: seeds observations over REST, then streams the bus.
pub async fn run(
    ws_url: String,
    base_url: String,
    state: Arc<Mutex<TuiState>>,
    quit: Arc<std::sync::atomic::AtomicBool>,
) {
    seed(&base_url, &state).await;
    let mut backoff = Duration::from_secs(2);
    loop {
        if quit.load(Ordering::Relaxed) {
            return;
        }
        match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((ws, _)) => {
                backoff = Duration::from_secs(2);
                stream_session(ws, &state).await;
            }
            Err(error) => {
                tracing::warn!(%error, "tui stream connect failed");
            }
        }
        if quit.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut guard) = state.lock() {
            guard.mark_stale();
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn seed(base_url: &str, state: &Arc<Mutex<TuiState>>) {
    let client = fetch_kit::Client::builder()
        .user_agent("argus-tui/0.1")
        .timeout(Duration::from_secs(10))
        .build();
    let url = format!("{base_url}/api/v1/observations?limit=40");
    if let Ok(value) = client.get(url).json_response::<serde_json::Value>().await {
        if let Some(data) = value.get("data").and_then(|v| v.as_array()) {
            let snapshot: Vec<serde_json::Value> = data.clone();
            if let Ok(mut guard) = state.lock() {
                guard.seed_observations(&snapshot);
            }
        }
    }
}

async fn stream_session<S>(
    ws: tokio_tungstenite::WebSocketStream<S>,
    state: &Arc<Mutex<TuiState>>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    while let Some(message) = read.next().await {
        match message {
            Ok(Message::Text(text)) => {
                if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&text) {
                    let topic = envelope
                        .get("topic")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if let Some(payload) = envelope.get("payload") {
                        if let Ok(mut guard) = state.lock() {
                            guard.ingest(&topic, payload);
                        }
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = write.send(Message::Pong(payload)).await;
            }
            Ok(Message::Close(_)) | Err(_) => return,
            Ok(_) => {}
        }
    }
}
