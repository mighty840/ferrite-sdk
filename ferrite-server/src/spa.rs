//! Serve the dashboard for browser navigations to its routes.
//!
//! Dashboard routes such as `/ota/campaigns/:id` or `/devices/:id` share
//! their paths with API endpoints. When the dashboard is served from the same
//! origin (`--static-dir`), opening or refreshing such a page would hit the
//! API. A browser navigation asks for `text/html`; API clients (CLI, devices,
//! the gateway, the dashboard's own fetch calls, SSE) don't, so those
//! requests get the dashboard's `index.html` and everything else goes on to
//! the API.

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// Whether a request is a browser navigation to a dashboard route.
pub fn is_spa_navigation(method: &Method, path: &str, accept: Option<&str>) -> bool {
    if method != Method::GET && method != Method::HEAD {
        return false;
    }
    let wants_html = accept.is_some_and(|a| {
        a.split(',')
            .any(|part| part.trim().starts_with("text/html"))
    });
    if !wants_html {
        return false;
    }
    is_dashboard_route(path)
}

/// Mirrors the routes in `ferrite-dashboard/src/main.rs`; keep in sync.
fn is_dashboard_route(path: &str) -> bool {
    let segments: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    matches!(
        segments.as_slice(),
        [] | ["devices"]
            | ["devices", _]
            | ["faults"]
            | ["crashes"]
            | ["crashes", _]
            | ["metrics"]
            | ["fleet"]
            | ["compare"]
            | ["ota"]
            | ["ota", "campaigns", _]
            | ["settings"]
            | ["callback"]
    )
}

/// Middleware: answer browser navigations with `index_html`.
pub async fn serve_index_for_navigation(
    index_html: Arc<String>,
    req: Request,
    next: Next,
) -> Response {
    let accept = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok());
    if is_spa_navigation(req.method(), req.uri().path(), accept) {
        let mut resp = (StatusCode::OK, Body::from(index_html.as_str().to_owned())).into_response();
        resp.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        // The shell must not be cached: it references hashed asset names.
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return resp;
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::is_spa_navigation;
    use axum::http::Method;

    const BROWSER: Option<&str> =
        Some("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8");

    #[test]
    fn browser_navigations_get_the_dashboard() {
        assert!(is_spa_navigation(&Method::GET, "/ota/campaigns/1", BROWSER));
        assert!(is_spa_navigation(&Method::GET, "/devices/edge-01", BROWSER));
        assert!(is_spa_navigation(&Method::HEAD, "/", BROWSER));
    }

    #[test]
    fn only_dashboard_routes() {
        for path in [
            "/",
            "/devices",
            "/devices/register",
            "/crashes/7",
            "/ota/campaigns/new",
            "/callback",
        ] {
            assert!(is_spa_navigation(&Method::GET, path, BROWSER), "{path}");
        }
        // API-only paths stay API even when opened in a browser.
        for path in [
            "/health",
            "/reports/campaigns/1",
            "/ota/firmware/3/download",
            "/ota/targets/dev-1",
            "/metrics/prometheus",
            "/devices/edge-01/faults",
        ] {
            assert!(!is_spa_navigation(&Method::GET, path, BROWSER), "{path}");
        }
    }

    #[test]
    fn api_and_files_are_untouched() {
        // API clients
        assert!(!is_spa_navigation(&Method::GET, "/ota/campaigns/1", None));
        assert!(!is_spa_navigation(
            &Method::GET,
            "/ota/campaigns/1",
            Some("*/*")
        ));
        assert!(!is_spa_navigation(
            &Method::GET,
            "/devices",
            Some("application/json")
        ));
        assert!(!is_spa_navigation(
            &Method::GET,
            "/events/stream",
            Some("text/event-stream")
        ));
        assert!(!is_spa_navigation(&Method::POST, "/auth/login", BROWSER));
        // Static files
        assert!(!is_spa_navigation(
            &Method::GET,
            "/assets/app-1a2b.js",
            BROWSER
        ));
        assert!(!is_spa_navigation(&Method::GET, "/favicon.ico", BROWSER));
    }
}
