//! Crash deduplication and analytics endpoints.
//!
//! Provides crash group listing and detail views for fault analysis.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;

/// Pagination query parameters for crash group endpoints.
#[derive(Debug, Deserialize)]
pub struct CrashListParams {
    /// Maximum number of results (default 100, max 1000).
    limit: Option<usize>,
    /// Offset for pagination (default 0).
    offset: Option<usize>,
}

impl CrashListParams {
    fn limit(&self) -> usize {
        self.limit.unwrap_or(100).min(1000)
    }
    fn offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

/// GET /crashes — list crash groups ordered by occurrence count.
pub async fn list_crash_groups(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<CrashListParams>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    let groups = match store.list_crash_groups(params.limit(), params.offset()) {
        Ok(g) => g,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };
    let total = match store.count_crash_groups() {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({ "crash_groups": groups, "total": total })),
    )
}

/// GET /crashes/:id — get crash group detail with paginated occurrences.
pub async fn get_crash_group_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    axum::extract::Query(params): axum::extract::Query<CrashListParams>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    let group = match store.get_crash_group(id) {
        Ok(Some(g)) => g,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "crash group not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };
    let occurrences =
        match store.list_faults_for_crash_group(id, params.limit(), params.offset()) {
            Ok(f) => f,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        };
    (
        StatusCode::OK,
        Json(serde_json::json!({ "group": group, "occurrences": occurrences })),
    )
}
