//! Device groups / fleet management.
//!
//! Provides CRUD for device groups and membership management.
//! Groups let users organize devices by location, fleet, project, etc.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// POST /groups
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    if req.name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "name is required" })),
        );
    }
    let store = state.store.lock().await;
    match store.create_group(&req.name, req.description.as_deref()) {
        Ok(group) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "group": group })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /groups
pub async fn list_groups(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_groups() {
        Ok(groups) => (
            StatusCode::OK,
            Json(serde_json::json!({ "groups": groups })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /groups/:id
pub async fn get_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.get_group(group_id) {
        Ok(Some(group)) => (StatusCode::OK, Json(serde_json::json!({ "group": group }))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "group not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// PUT /groups/:id
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<i64>,
    Json(req): Json<UpdateGroupRequest>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.update_group(group_id, req.name.as_deref(), req.description.as_deref()) {
        Ok(true) => match store.get_group(group_id) {
            Ok(Some(group)) => (StatusCode::OK, Json(serde_json::json!({ "group": group }))),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "group not found after update" })),
            ),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "group not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /groups/:id
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.delete_group(group_id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "group not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /groups/:id/devices
pub async fn list_group_devices(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_group_devices(group_id) {
        Ok(devices) => (
            StatusCode::OK,
            Json(serde_json::json!({ "devices": devices })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /groups/:id/devices/:device_id
pub async fn add_device_to_group(
    State(state): State<Arc<AppState>>,
    Path((group_id, device_id)): Path<(i64, String)>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.add_device_to_group(group_id, &device_id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "group or device not found" })),
        ),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("UNIQUE") {
                (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({ "error": "device already in group" })),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": msg })),
                )
            }
        }
    }
}

/// DELETE /groups/:id/devices/:device_id
pub async fn remove_device_from_group(
    State(state): State<Arc<AppState>>,
    Path((group_id, device_id)): Path<(i64, String)>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.remove_device_from_group(group_id, &device_id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "membership not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}
