//! WebSocket stream: JSON-serialized `EventEnvelope<Event>` frames.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::sync::broadcast::error::RecvError;

use crate::state::AppState;

/// `GET /api/v1/stream` — upgrade and start streaming bus events.
pub async fn stream(
    State(state): State<Arc<AppState>>,
    parts: Parts,
    ws: WebSocketUpgrade,
) -> Response {
    // CSWSH defense: validate Origin before upgrading (ws-kit, REQ-WSKIT-200).
    if !ws_kit::origin::origin_allowed_in_parts(&parts, &state.cfg.server.ws_allowed_origins) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Pumps bus events to one connected client.
async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.hub.subscribe();
    loop {
        match rx.recv().await {
            Ok(envelope) => {
                let json = match serde_json::to_string(&envelope) {
                    Ok(json) => json,
                    Err(error) => {
                        tracing::warn!(%error, "envelope serialize failed; dropping frame");
                        continue;
                    }
                };
                if socket.send(Message::text(json)).await.is_err() {
                    break;
                }
            }
            Err(RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "slow websocket client lagged");
            }
            Err(RecvError::Closed) => break,
        }
    }
}
