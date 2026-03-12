//! Axum middleware for authentication.
//!
//! - `/ingest/chunks` routes: optionally checked with `X-API-Key` if configured
//! - `/ingest/elf` routes: always require authentication (user auth or API key)
//! - `/auth/*` routes: never checked (auth discovery + login endpoints)
//! - All other routes (`/devices`, etc.): require user auth

use axum::{
    extract::State,
    http::{header, Method, Request, StatusCode},
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
    // Let CORS preflight requests through — the CorsLayer handles them
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }

    let path = req.uri().path().to_string();

    // Auth discovery, health, Prometheus, and SSE endpoints are always public
    if path == "/auth/mode"
        || path == "/health"
        || path == "/metrics/prometheus"
        || path == "/events/stream"
    {
        return next.run(req).await;
    }

    // Ingest endpoints
    if path.starts_with("/ingest") {
        // /ingest/elf always requires authentication (API key or user auth)
        if path.starts_with("/ingest/elf") {
            // Try API key first
            if let Some(required_key) = &state.config.ingest_api_key {
                let api_key = auth::extract_api_key_header(req.headers());
                if auth::validate_ingest_api_key(api_key, required_key).is_ok() {
                    return next.run(req).await;
                }
            }
            // Fall back to user auth
            let auth_header = auth::extract_auth_header(req.headers());
            return match auth::validate_request(auth_header, state.config).await {
                Ok(claims) => {
                    let mut req = req;
                    req.extensions_mut().insert(claims);
                    next.run(req).await
                }
                Err(_) => (
                    StatusCode::UNAUTHORIZED,
                    [(
                        header::WWW_AUTHENTICATE,
                        auth::AuthError::www_authenticate_header(state.config),
                    )],
                    "Authentication required for ELF upload",
                )
                    .into_response(),
            };
        }

        // /ingest/chunks and other ingest routes: check API key only if configured
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
    match auth::validate_request(auth_header, state.config).await {
        Ok(claims) => {
            // Role-based access control
            let method = req.method().clone();
            let is_write = method == Method::POST || method == Method::PUT || method == Method::PATCH;
            let is_delete = method == Method::DELETE;
            let is_admin_path = path.starts_with("/admin")
                || path.starts_with("/groups")
                || path.starts_with("/ota");

            if is_delete && !claims.role.can_admin() {
                return (
                    StatusCode::FORBIDDEN,
                    "Insufficient permissions: admin role required for delete operations",
                )
                    .into_response();
            }

            if is_write && is_admin_path && !claims.role.can_admin() {
                return (
                    StatusCode::FORBIDDEN,
                    "Insufficient permissions: admin role required",
                )
                    .into_response();
            }

            if is_write && !claims.role.can_write() {
                return (
                    StatusCode::FORBIDDEN,
                    "Insufficient permissions: provisioner or admin role required",
                )
                    .into_response();
            }

            let mut req = req;
            req.extensions_mut().insert(claims);
            next.run(req).await
        }
        Err(_) => {
            let www_auth = auth::AuthError::www_authenticate_header(state.config);
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, www_auth)],
                "Authentication required",
            )
                .into_response()
        }
    }
}
