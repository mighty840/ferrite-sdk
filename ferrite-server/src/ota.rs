//! OTA firmware target management and firmware artifact storage.
//!
//! Target endpoints let operators set target firmware versions per device.
//! Firmware endpoints handle binary upload, listing, download, and deletion.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::AppState;

/// Maximum firmware binary size (20 MB).
const MAX_FIRMWARE_SIZE: usize = 20 * 1024 * 1024;

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

// ---------------------------------------------------------------------------
// Firmware artifact endpoints
// ---------------------------------------------------------------------------

/// POST /ota/firmware — upload a firmware binary.
///
/// Headers: X-Firmware-Version (required), X-Build-Id (optional), X-Signer (optional).
/// Body: raw binary firmware data.
pub async fn upload_firmware(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if body.len() > MAX_FIRMWARE_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "ok": false,
                "error": format!(
                    "firmware too large: {} bytes (max {})",
                    body.len(),
                    MAX_FIRMWARE_SIZE
                ),
            })),
        );
    }

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "empty body" })),
        );
    }

    let version = headers
        .get("X-Firmware-Version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let build_id: i64 = headers
        .get("X-Build-Id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let signer = headers
        .get("X-Signer")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let mut hasher = Sha256::new();
    hasher.update(&body);
    let sha256 = format!("{:x}", hasher.finalize());

    let filename = format!("{}-b{}-{}.bin", version, build_id, &sha256[..8]);
    let path = state.firmware_dir.join(&filename);

    if let Err(e) = tokio::fs::write(&path, &body).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": format!("failed to write file: {e}") })),
        );
    }

    let store = state.store.lock().await;
    match store.insert_firmware_artifact(
        &version,
        build_id,
        &sha256,
        body.len() as i64,
        &filename,
        signer.as_deref(),
    ) {
        Ok(artifact) => (
            StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "artifact": artifact })),
        ),
        Err(e) => {
            let _ = tokio::fs::remove_file(&path).await;
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
            )
        }
    }
}

/// GET /ota/firmware — list all firmware artifacts.
pub async fn list_firmware(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_firmware_artifacts() {
        Ok(artifacts) => (
            StatusCode::OK,
            Json(serde_json::json!({ "artifacts": artifacts })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /ota/firmware/:id — get firmware artifact metadata.
pub async fn get_firmware(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.get_firmware_artifact(id) {
        Ok(Some(artifact)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "artifact": artifact })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "firmware artifact not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /ota/firmware/:id/download — download the firmware binary.
pub async fn download_firmware(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let store = state.store.lock().await;
    let artifact = match store.get_firmware_artifact(id) {
        Ok(Some(a)) => a,
        Ok(None) => {
            return axum::response::Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(axum::body::Body::from("not found"))
                .unwrap();
        }
        Err(e) => {
            return axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from(e.to_string()))
                .unwrap();
        }
    };
    drop(store);

    let path = state.firmware_dir.join(&artifact.filename);
    match tokio::fs::read(&path).await {
        Ok(data) => axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/octet-stream")
            .header(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", artifact.filename),
            )
            .header("X-Firmware-SHA256", &artifact.sha256)
            .body(axum::body::Body::from(data))
            .unwrap(),
        Err(_) => axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("firmware file missing from disk"))
            .unwrap(),
    }
}

/// DELETE /ota/firmware/:id — delete a firmware artifact.
pub async fn delete_firmware(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    let filename = store
        .get_firmware_artifact(id)
        .ok()
        .flatten()
        .map(|a| a.filename.clone());

    match store.delete_firmware_artifact(id) {
        Ok(true) => {
            if let Some(f) = filename {
                let _ = tokio::fs::remove_file(state.firmware_dir.join(f)).await;
            }
            (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "firmware artifact not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}
