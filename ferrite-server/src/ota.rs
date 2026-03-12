//! OTA firmware target management API handlers.
//!
//! These endpoints let operators set target firmware versions per device.
//! When a device sends an OtaRequest chunk, the server compares its build_id
//! against the target and emits an SSE event if an update is available.

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
pub struct SetOtaTargetRequest {
    pub device_id: String,
    pub target_version: String,
    #[serde(default)]
    pub target_build_id: i64,
    pub firmware_url: Option<String>,
}

/// GET /ota/targets — list all OTA targets.
pub async fn list_ota_targets(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_ota_targets() {
        Ok(targets) => (
            StatusCode::OK,
            Json(serde_json::json!({ "targets": targets })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /ota/targets — set (upsert) an OTA target for a device.
pub async fn set_ota_target(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetOtaTargetRequest>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.set_ota_target(
        &req.device_id,
        &req.target_version,
        req.target_build_id,
        req.firmware_url.as_deref(),
    ) {
        Ok(target) => (
            StatusCode::OK,
            Json(serde_json::json!({ "target": target })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /ota/targets/:device_id — get OTA target for a specific device.
pub async fn get_ota_target(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.get_ota_target_for_device(&device_id) {
        Ok(Some(target)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "target": target })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "no OTA target for this device" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /ota/targets/:device_id — remove OTA target for a device.
pub async fn delete_ota_target(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.delete_ota_target(&device_id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "no OTA target for this device" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}
