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

use crate::state::AppState;

const USER_HEADER: HeaderName = HeaderName::from_static("x-argus-user");
const ROLE_HEADER: HeaderName = HeaderName::from_static("x-argus-role");

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

fn role_from_str(value: &str) -> Option<Role> {
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
        _state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
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
