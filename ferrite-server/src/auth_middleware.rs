//! Axum middleware for authentication.
//!
//! - `/ingest/chunks` routes: optionally checked with `X-API-Key` if configured
//! - `/ingest/elf` routes: always require authentication (API key, or a
//!   provisioner/admin user)
//! - `/auth/*` routes: never checked (auth discovery + login endpoints)
//! - Device-facing OTA reads (`GET /ota/targets/:device_id`,
//!   `GET /ota/firmware/:id/{download,manifest}`): `X-API-Key` when an ingest
//!   key is configured, otherwise user auth
//! - All other routes (`/devices`, etc.): require user auth

use axum::{
    extract::State,
    http::{header, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

use crate::auth;
use crate::config::UserRole;
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

    // Auth discovery, health and Prometheus endpoints are always public.
    // The SSE stream carries live device data, so it needs a signed-in user.
    if path == "/auth/mode" || path == "/health" || path == "/metrics/prometheus" {
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
            // Fall back to user auth. Uploaded symbols decide how every crash
            // report reads, so only provisioners and admins may upload.
            let auth_header = auth::extract_auth_header(req.headers());
            return match auth::validate_request(auth_header, state.config).await {
                Ok(claims) if !claims.role.can_write() => (
                    StatusCode::FORBIDDEN,
                    "Insufficient permissions: provisioner or admin role required",
                )
                    .into_response(),
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

    // Devices poll their OTA target and download firmware with the ingest
    // API key, like /ingest/chunks. Without a configured key these stay
    // behind user auth, so firmware is never readable anonymously. The
    // image itself is authenticated by its signed manifest, not by this check.
    if req.method() == Method::GET && is_device_ota_path(&path) {
        if let Some(required_key) = &state.config.ingest_api_key {
            let api_key = auth::extract_api_key_header(req.headers());
            if auth::validate_ingest_api_key(api_key, required_key).is_ok() {
                return next.run(req).await;
            }
        }
    }

    // All other routes require user authentication
    let auth_header = auth::extract_auth_header(req.headers());
    match auth::validate_request(auth_header, state.config).await {
        Ok(claims) => {
            // Role-based access control
            let method = req.method().clone();
            let is_write =
                method == Method::POST || method == Method::PUT || method == Method::PATCH;
            let is_delete = method == Method::DELETE;
            let is_admin_path = path.starts_with("/admin")
                || path.starts_with("/groups")
                || path.starts_with("/ota")
                || path.starts_with("/evidence");
            let is_evidence = path == "/evidence" || path.starts_with("/evidence/");

            if is_evidence && !claims.role.can_read_evidence() {
                return (
                    StatusCode::FORBIDDEN,
                    "Insufficient permissions: admin or auditor role required",
                )
                    .into_response();
            }

            if claims.role == UserRole::Auditor && !(is_evidence && method == Method::GET) {
                return (
                    StatusCode::FORBIDDEN,
                    "Insufficient permissions: the auditor role can only read the evidence log",
                )
                    .into_response();
            }

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

/// `/ota/targets/<device_id>` or `/ota/firmware/<id>/{download,manifest}`.
fn is_device_ota_path(path: &str) -> bool {
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    matches!(
        segments.as_slice(),
        ["ota", "targets", id] if !id.is_empty()
    ) || matches!(
        segments.as_slice(),
        ["ota", "firmware", id, "download" | "manifest"] if !id.is_empty()
    )
}

#[cfg(test)]
mod tests {
    use super::is_device_ota_path;

    #[test]
    fn device_ota_paths() {
        assert!(is_device_ota_path("/ota/targets/dev-1"));
        assert!(is_device_ota_path("/ota/firmware/3/download"));
        assert!(is_device_ota_path("/ota/firmware/3/manifest"));

        assert!(!is_device_ota_path("/ota/targets"));
        assert!(!is_device_ota_path("/ota/targets/"));
        assert!(!is_device_ota_path("/ota/firmware"));
        assert!(!is_device_ota_path("/ota/firmware/3"));
        assert!(!is_device_ota_path("/ota/campaigns/1"));
        assert!(!is_device_ota_path("/ota/targets/dev-1/extra"));
    }
}
