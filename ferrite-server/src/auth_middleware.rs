//! Axum middleware for authentication.
//!
//! - `/ingest/*` routes: optionally checked with `X-API-Key` if configured
//! - `/auth/*` routes: never checked (auth discovery + login endpoints)
//! - All other routes (`/devices`, etc.): require user auth

use axum::{
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::auth;
use crate::AppState;

pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    // Auth discovery and health endpoints are always public
    if path == "/auth/mode" || path == "/health" {
        return next.run(req).await;
    }

    // Ingest endpoints: check API key only if configured
    if path.starts_with("/ingest") {
        if let Some(required_key) = &state.config.ingest_api_key {
            let api_key = auth::extract_api_key_header(req.headers());
            if auth::validate_ingest_api_key(api_key, required_key).is_err() {
                return (
                    StatusCode::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, "ApiKey")],
                    "Missing or invalid X-API-Key",
                )
                    .into_response();
            }
        }
        return next.run(req).await;
    }

    // All other routes require user authentication
    let auth_header = auth::extract_auth_header(req.headers());
    match auth::validate_request(auth_header, &state.config).await {
        Ok(claims) => {
            let mut req = req;
            req.extensions_mut().insert(claims);
            next.run(req).await
        }
        Err(_) => {
            let www_auth = auth::AuthError::www_authenticate_header(&state.config);
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, www_auth)],
                "Authentication required",
            )
                .into_response()
        }
    }
}
