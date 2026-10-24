//! REST surface: `/api/v1/observations` and `/api/v1/quotes`.

use std::sync::Arc;

use api_types::ApiResponse;
use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::state::AppState;

/// Query for the observations endpoint.
#[derive(Debug, Deserialize)]
pub struct ObservationsQuery {
    /// Max items to return (1..=500, default 100).
    pub limit: Option<usize>,
}

/// Builds the full application router.
pub fn router(state: Arc<AppState>) -> Router {
    // Stateless health routes re-typed into the app's state via `with_state`.
    // readiness_route mounts itself at `/readyz` internally.
    let health = Router::new()
        .route("/livez", healthkit::axum::liveness_route().with_state(()))
        .route("/ping", healthkit::axum::ping_route().with_state(()))
        .merge(healthkit::axum::readiness_route(state.health.clone()).with_state(()));

    Router::new()
        .route("/api/v1/stream", get(crate::ws::stream))
        .route("/api/v1/observations", get(observations))
        .route("/api/v1/quotes", get(quotes))
        .with_state(state)
        .merge(health)
}

/// `GET /api/v1/observations` — recent world observations, newest first.
#[tracing::instrument(skip(state))]
pub async fn observations(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ObservationsQuery>,
) -> Json<ApiResponse<Vec<feedkit::Observation>>> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    Json(ApiResponse::success(state.recent_observations(limit)))
}

/// `GET /api/v1/quotes` — latest quote per symbol, symbol-ordered.
#[tracing::instrument(skip(state))]
pub async fn quotes(State(state): State<Arc<AppState>>) -> Json<ApiResponse<serde_json::Value>> {
    let snapshot = state.quotes_snapshot();
    let value = serde_json::to_value(snapshot).unwrap_or(serde_json::Value::Null);
    Json(ApiResponse::success(value))
}
