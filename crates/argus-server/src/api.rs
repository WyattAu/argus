//! REST surface: `/api/v1/observations` and `/api/v1/quotes`.

use std::sync::Arc;


use api_types::ApiResponse;
use axum::extract::{Path, Query, State};

use axum::routing::get;
use axum::{Json, Router};
use ontokit::EntityKind;
use serde::Deserialize;

use crate::authz::Reader;
use crate::error::ServerError;
use crate::state::{read_tolerant, AppState};

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
        .route("/api/v1/candles", get(candles))
        .route("/api/v1/entities", get(entities))
        .route("/api/v1/entities/{id}/neighbors", get(neighbors))
        .route("/api/v1/search", get(search))
        .route("/api/v1/provenance", get(provenance))
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

/// Query for the candles endpoint.
#[derive(Debug, Deserialize)]
pub struct CandlesQuery {
    /// Chart symbol (`BTCUSDT`).
    pub symbol: String,
    /// Max candles to return (1..=600, default 300).
    pub limit: Option<usize>,
}

/// `GET /api/v1/candles` — OHLCV history for one symbol, oldest first.
#[tracing::instrument(skip(state))]
pub async fn candles(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CandlesQuery>,
) -> Json<ApiResponse<Vec<mdkit::Candle>>> {
    let limit = query.limit.unwrap_or(300).clamp(1, 600);
    Json(ApiResponse::success(state.candles_for(&query.symbol, limit)))
}

/// Query for the entities endpoint.
#[derive(Debug, Deserialize)]
pub struct EntitiesQuery {
    /// Filter by kind (`event`, `place`, `source`, `symbol`).
    pub kind: Option<String>,
    /// Max items (1..=500, default 100).
    pub limit: Option<usize>,
}

/// `GET /api/v1/entities` — ontology entities, newest-seen first.
#[tracing::instrument(skip(state))]
pub async fn entities(
    State(state): State<Arc<AppState>>,
    reader: Reader,
    Query(query): Query<EntitiesQuery>,
) -> Result<Json<ApiResponse<Vec<ontokit::Entity>>>, ServerError> {
    tracing::debug!(user = %reader.user, "entities read");
    let kind = match query.kind.as_deref() {
        None | Some("all") => None,
        Some("event") => Some(EntityKind::Event),
        Some("place") => Some(EntityKind::Place),
        Some("source") => Some(EntityKind::Source),
        Some("symbol") => Some(EntityKind::Symbol),
        Some(_) => {
            return Err(ServerError::NotFound("unknown kind"));
        }    };
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let guard = read_tolerant(&state.ontology);
    Ok(Json(ApiResponse::success(guard.entities(kind, limit))))
}

/// Query for the neighbors endpoint.
#[derive(Debug, Deserialize)]
pub struct NeighborsQuery {
    /// Hop depth (1..=3, default 2).
    pub hops: Option<usize>,
}

/// `GET /api/v1/entities/{id}/neighbors` — n-hop neighborhood, entity-joined.
#[tracing::instrument(skip(state))]
pub async fn neighbors(
    State(state): State<Arc<AppState>>,
    reader: Reader,
    Path(id): Path<String>,
    Query(query): Query<NeighborsQuery>,
) -> Result<Json<ApiResponse<Vec<ontokit::NeighborEntity>>>, ServerError> {
    tracing::debug!(user = %reader.user, entity = %id, "neighbors read");
    let hops = query.hops.unwrap_or(2).clamp(1, 3);
    let guard = read_tolerant(&state.ontology);
    if !guard.has_entity(&id) {
        return Err(ServerError::NotFound("entity"));
    }
    Ok(Json(ApiResponse::success(guard.neighbors(&id, hops))))
}

/// Query for the search endpoint.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Query string (title/kind/source, tantivy syntax).
    pub q: String,
    /// Max hits (1..=50, default 20).
    pub limit: Option<usize>,
}

/// `GET /api/v1/search` — full-text over observations.
#[tracing::instrument(skip(state))]
pub async fn search(
    State(state): State<Arc<AppState>>,
    reader: Reader,
    Query(query): Query<SearchQuery>,
) -> Result<Json<ApiResponse<Vec<crate::search::SearchHit>>>, ServerError> {
    tracing::debug!(user = %reader.user, q = %query.q, "search read");
    let limit = query.limit.unwrap_or(20).clamp(1, 50);
    let hits = state
        .search
        .search(&query.q, limit)
        .map_err(|error| ServerError::Internal(error.to_string()))?;
    Ok(Json(ApiResponse::success(hits)))
}

/// Query for the provenance endpoint.
#[derive(Debug, Deserialize)]
pub struct ProvenanceQuery {
    /// Last N mutations to return (default 20).
    pub tail: Option<usize>,
}

/// `GET /api/v1/provenance` — chain head, verification, recent entries.
#[tracing::instrument(skip(state))]
pub async fn provenance(
    State(state): State<Arc<AppState>>,
    reader: Reader,
    Query(query): Query<ProvenanceQuery>,
) -> Json<ApiResponse<serde_json::Value>> {
    tracing::debug!(user = %reader.user, "provenance read");
    let guard = read_tolerant(&state.ontology);
    let value = serde_json::json!({
        "head": guard.provenance_head(),
        "verified": guard.verify_provenance(),
        "mutations": guard.provenance_tail(query.tail.unwrap_or(20)),
    });
    Json(ApiResponse::success(value))
}
