//! REST surface: `/api/v1/observations` and `/api/v1/quotes`.

use std::sync::Arc;


use api_types::ApiResponse;
use axum::extract::{Path, Query, State};

use axum::routing::{get, post};
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
        .route("/api/v1/auth/token", post(mint_token))
        .route("/api/v1/backoffice/portfolio", get(backoffice_portfolio))
        .route("/api/v1/backoffice/trial-balance", get(backoffice_trial_balance))
        .route("/api/v1/backoffice/post", post(backoffice_post))
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

/// Query for the portfolio endpoint.
#[derive(Debug, Deserialize)]
pub struct PortfolioQuery {
    /// Max positions returned (1..=200, default 50).
    pub limit: Option<usize>,
}

/// `GET /api/v1/backoffice/portfolio` — holdings marked against live quotes.
#[tracing::instrument(skip(state))]
pub async fn backoffice_portfolio(
    State(state): State<Arc<AppState>>,
    reader: Reader,
    Query(query): Query<PortfolioQuery>,
) -> Result<Json<ApiResponse<crate::backoffice::PortfolioView>>, ServerError> {
    tracing::debug!(user = %reader.user, "portfolio read");
    if !reader.role.has_at_least(&accessctl::Role::Viewer) {
        return Err(ServerError::Forbidden("insufficient role"));
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let quotes = state.quotes_snapshot();
    let holdings: Vec<crate::backoffice::Holding> =
        state.cfg.backoffice.holdings.iter().take(limit).cloned().collect();
    Ok(Json(ApiResponse::success(crate::backoffice::portfolio(
        &holdings,
        &quotes,
        feedkit::now_ms(),
    ))))
}

/// `GET /api/v1/backoffice/trial-balance` — report-kit trial balance.
#[tracing::instrument(skip(state))]
pub async fn backoffice_trial_balance(
    State(state): State<Arc<AppState>>,
    reader: Reader,
) -> Result<Json<ApiResponse<serde_json::Value>>, ServerError> {
    tracing::debug!(user = %reader.user, "trial balance read");
    if !reader.role.has_at_least(&accessctl::Role::Viewer) {
        return Err(ServerError::Forbidden("insufficient role"));
    }
    let ledger = state.books.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let report = report_kit::TrialBalance::from_ledger(&ledger, "9999-12-31");
    let rows: Vec<serde_json::Value> = report
        .rows_for(state.cfg.backoffice.base_currency.as_str())
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "account": row.account_id,
                "name": row.account_name,
                "debit_minor": row.debit_total,
                "credit_minor": row.credit_total,
                "net_minor": row.net(),
            })
        })
        .collect();
    let value = serde_json::json!({
        "base": state.cfg.backoffice.base_currency,
        "balanced": report.is_balanced(),
        "accounting_equation_holds": report
            .accounting_equation_holds(state.cfg.backoffice.base_currency.as_str()),
        "rows": rows,
    });
    Ok(Json(ApiResponse::success(value)))
}

/// `POST /api/v1/backoffice/post` — manual balanced posting (editor+).
#[tracing::instrument(skip(state, request))]
pub async fn backoffice_post(
    State(state): State<Arc<AppState>>,
    reader: Reader,
    Json(request): Json<crate::backoffice::ManualPost>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ServerError> {
    if !reader.role.has_at_least(&accessctl::Role::Editor) {
        return Err(ServerError::Forbidden("editor role required"));
    }
    let now = feedkit::now_ms();
    let mut ledger = state
        .books
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let version = crate::backoffice::post_manual(&mut ledger, &request, now)
        .map_err(ServerError::Internal)?;
    Ok(Json(ApiResponse::success(serde_json::json!({
        "posted": true,
        "ledger_version": version,
        "by": reader.user,
    }))))
}
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

/// Request body for token minting.
#[derive(Debug, Deserialize)]
pub struct MintRequest {
    /// Principal id to embed in the token.
    pub user: String,
    /// Role to embed (`viewer` / `editor` / `admin`).
    pub role: String,
    /// Token lifetime, seconds (`60..=86_400`, default `3_600`).
    #[serde(default = "default_ttl_secs")]
    pub ttl_secs: i64,
}

fn default_ttl_secs() -> i64 {
    3_600
}

/// `POST /api/v1/auth/token` — mint a bearer token.
///
/// Gated by `x-argus-mint-key` matching `auth.jwt_secret`; mints with the
/// same secret (HMAC), so possession of the secret is the mint credential.
#[tracing::instrument(skip(state, headers))]
pub async fn mint_token(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<MintRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ServerError> {
    let secret = state.cfg.auth.jwt_secret.trim();
    if secret.is_empty() {
        return Err(ServerError::Internal(
            "JWT auth not configured (auth.jwt_secret is empty)".to_string(),
        ));
    }
    let presented = headers
        .get("x-argus-mint-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if presented != secret {
        return Err(ServerError::NotFound("valid mint key"));
    }
    // Validate the role through the same gate vocabulary as authz.
    if crate::authz::role_from_str(&request.role).is_none() {
        return Err(ServerError::Forbidden("unknown role"));
    }
    let token = crate::authz::issue_token(
        &state,
        &request.user,
        &request.role,
        request.ttl_secs,
    )?;
    Ok(Json(ApiResponse::success(serde_json::json!({
        "token": token,
        "token_type": "Bearer",
        "expires_in": request.ttl_secs.clamp(60, 86_400),
    }))))
}
