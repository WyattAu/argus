//! Authorization: accessctl role gate on the read surface.
//!
//! Dev-mode identity: `x-argus-user` / `x-argus-role` headers (default
//! `anonymous` / `viewer`). Real authN (tokenkit + barbican + accessctl's
//! `RequireRole` extractor) wraps this in M4 — the role requirement stays
//! exactly where it is.

use std::sync::Arc;

use accessctl::Role;
use axum::extract::FromRequestParts;
use axum::http::header::HeaderName;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokenkit::extractors::extract_bearer_token;

use crate::error::ServerError;
use crate::state::AppState;

const USER_HEADER: HeaderName = HeaderName::from_static("x-argus-user");
const ROLE_HEADER: HeaderName = HeaderName::from_static("x-argus-role");

/// JWT claims for argus tokens (minted by `/api/v1/auth/token`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgusClaims {
    /// Principal id.
    pub sub: String,
    /// Role (`viewer` / `editor` / `admin`).
    pub role: String,
    /// Issuer — must match `auth.issuer`.
    pub iss: String,
    /// Expiry, unix seconds.
    pub exp: u64,
}

/// Issues a token when JWT auth is configured.
///
/// # Errors
/// Returns [`ServerError::Internal`] when no JWT secret is configured or
/// encoding fails.
pub(crate) fn issue_token(
    state: &AppState,
    user: &str,
    role: &str,
    ttl_secs: i64,
) -> Result<String, ServerError> {
    let Some(service) = &state.jwt else {
        return Err(ServerError::Internal(
            "JWT auth not configured (auth.jwt_secret is empty)".to_string(),
        ));
    };
    let claims = ArgusClaims {
        sub: user.to_string(),
        role: role.to_string(),
        iss: state.cfg.auth.issuer.clone(),
        exp: u64::try_from(feedkit::now_ms() / 1000 + ttl_secs.clamp(60, 86_400))
            .unwrap_or(u64::MAX),
    };
    service.encode(&claims).map_err(|error| ServerError::Internal(error.to_string()))
}

/// An authenticated-for-read request principal.
#[derive(Debug, Clone)]
pub struct Reader {
    /// Principal id (dev header; real identity in M4).
    pub user: String,
    /// Resolved role. Consumed by handlers from M4's write surface onward;
    /// the read gate checks it during extraction.
    #[allow(dead_code)]
    pub role: Role,
}

/// Role-string parser.
pub(crate) fn role_from_str(value: &str) -> Option<Role> {
    match value.to_ascii_lowercase().as_str() {
        "viewer" => Some(Role::Viewer),
        "editor" => Some(Role::Editor),
        "admin" => Some(Role::Admin),
        _ => None,
    }
}

impl FromRequestParts<Arc<AppState>> for Reader {
    type Rejection = Response;

    #[allow(clippy::unused_async_trait_impl)] // axum extractor trait is async
    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        // JWT mode: a Bearer token is mandatory and role comes from claims.
        if let Some(service) = &state.jwt {
            let header = parts
                .headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let Some(token) = extract_bearer_token(header) else {
                return Err((StatusCode::UNAUTHORIZED, "missing bearer token").into_response());
            };
            let claims: ArgusClaims = service
                .decode(token.as_str())
                .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()).into_response())?;
            let Some(role) = role_from_str(&claims.role) else {
                return Err((StatusCode::FORBIDDEN, "unknown role in token").into_response());
            };
            if !role.has_at_least(&Role::Viewer) {
                return Err((StatusCode::FORBIDDEN, "insufficient role").into_response());
            }
            return Ok(Reader { user: claims.sub, role });
        }

        // Dev mode: headers are trusted; a startup warning says so.
        let user = parts
            .headers
            .get(USER_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("anonymous")
            .to_string();
        let role_str = parts
            .headers
            .get(ROLE_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("viewer");
        let Some(role) = role_from_str(role_str) else {
            return Err((StatusCode::FORBIDDEN, "unknown role").into_response());
        };
        // Read surface: every role at or above Viewer passes. accessctl's
        // Cedar-backed `authorize` is still private in 0.1.3 (dogfood note);
        // the hierarchy check is the published path.
        if role.has_at_least(&Role::Viewer) {
            tracing::debug!(%user, role = ?role, "read authorized");
            Ok(Reader { user, role })
        } else {
            Err((StatusCode::FORBIDDEN, "insufficient role").into_response())
        }
    }
}
