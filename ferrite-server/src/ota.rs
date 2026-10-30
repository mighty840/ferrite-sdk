//! OTA firmware target management and firmware artifact storage.
//!
//! Target endpoints let operators set target firmware versions per device.
//! Firmware endpoints handle binary upload, listing, download, and deletion.
//!
//! Firmware is authenticated by a signed manifest (`ferrite_sdk::ota_manifest`)
//! created offline with `ferrite ota sign`. The server stores and serves the
//! manifest but never holds a signing key. With `OTA_TRUSTED_KEYS` set it
//! also refuses uploads whose manifest isn't signed by a trusted key or
//! doesn't match the uploaded bytes.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use ferrite_sdk::ota_manifest::SignedManifest;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::auth::UserClaims;
use crate::evidence;
use crate::store::NewFirmwareArtifact;
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
    claims: Option<Extension<UserClaims>>,
    Json(req): Json<SetOtaTargetRequest>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.set_ota_target(
        &req.device_id,
        &req.target_version,
        req.target_build_id,
        req.firmware_url.as_deref(),
    ) {
        Ok(target) => {
            evidence::record(
                &state.evidence,
                "ota.target_set",
                &format!("device:{}", req.device_id),
                &evidence::user_actor(claims.as_deref()),
                serde_json::json!({
                    "target_version": req.target_version,
                    "target_build_id": req.target_build_id,
                    "firmware_url": req.firmware_url,
                }),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({ "target": target })),
            )
        }
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
    claims: Option<Extension<UserClaims>>,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.delete_ota_target(&device_id) {
        Ok(true) => {
            evidence::record(
                &state.evidence,
                "ota.target_removed",
                &format!("device:{device_id}"),
                &evidence::user_actor(claims.as_deref()),
                serde_json::json!({}),
            );
            (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
        }
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
/// Body: raw binary firmware data.
///
/// Headers:
/// - `X-Firmware-Manifest`: hex signed manifest. Required when
///   `OTA_TRUSTED_KEYS` is set; version, build id and signer then come from
///   the manifest.
/// - `X-Firmware-Version`, `X-Build-Id`, `X-Signer`: only for uploads without
///   a manifest (no trusted keys configured).
pub async fn upload_firmware(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    claims: Option<Extension<UserClaims>>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if body.len() > MAX_FIRMWARE_SIZE {
        return reject(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "firmware too large: {} bytes (max {})",
                body.len(),
                MAX_FIRMWARE_SIZE
            ),
        );
    }

    if body.is_empty() {
        return reject(StatusCode::BAD_REQUEST, "empty body".into());
    }

    let mut hasher = Sha256::new();
    hasher.update(&body);
    let sha256 = format!("{:x}", hasher.finalize());

    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let trusted_keys = &state.config.ota_trusted_keys;

    let meta = match header("X-Firmware-Manifest") {
        Some(hex) => {
            let signed = match SignedManifest::from_hex(hex.trim().as_bytes()) {
                Ok(s) => s,
                Err(e) => {
                    return reject(StatusCode::BAD_REQUEST, format!("invalid manifest: {e:?}"))
                }
            };
            let (manifest, signer) = if trusted_keys.is_empty() {
                // Nothing to verify against here; devices still verify.
                (signed.unverified(), None)
            } else {
                match signed.verify(trusted_keys) {
                    Ok(m) => (m, Some(hex::encode(m.key_id))),
                    Err(e) => {
                        return reject(
                            StatusCode::FORBIDDEN,
                            format!("manifest signature rejected: {e:?}"),
                        )
                    }
                }
            };
            if manifest.image_size as usize != body.len() || hex::encode(manifest.sha256) != sha256
            {
                return reject(
                    StatusCode::BAD_REQUEST,
                    "manifest does not match the uploaded image (size or sha256)".into(),
                );
            }
            UploadMeta {
                version: manifest.version.to_string(),
                // Stored as the same 64 bits the device reports.
                build_id: manifest.build_id as i64,
                signer,
                manifest: Some(String::from_utf8_lossy(&signed.to_hex()).into_owned()),
                security_version: Some(manifest.security_version as i64),
            }
        }
        None if !trusted_keys.is_empty() => {
            return reject(
                StatusCode::BAD_REQUEST,
                "signed manifest required (X-Firmware-Manifest); create one with `ferrite ota sign`"
                    .into(),
            );
        }
        None => UploadMeta {
            version: header("X-Firmware-Version")
                .unwrap_or("unknown")
                .to_string(),
            build_id: header("X-Build-Id")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            signer: header("X-Signer").map(str::to_string),
            manifest: None,
            security_version: None,
        },
    };

    if !crate::ingest::is_safe_version(&meta.version) {
        return reject(
            StatusCode::BAD_REQUEST,
            "invalid firmware version: use 1 to 64 letters, digits, '.', '-', '_' or '+', starting with a letter or digit".into(),
        );
    }
    let filename = format!("{}-b{}-{}.bin", meta.version, meta.build_id, &sha256[..8]);
    let path = state.firmware_dir.join(&filename);

    if let Err(e) = tokio::fs::write(&path, &body).await {
        return reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to write file: {e}"),
        );
    }

    let store = state.store.lock().await;
    match store.insert_firmware_artifact(&NewFirmwareArtifact {
        version: &meta.version,
        build_id: meta.build_id,
        sha256: &sha256,
        size: body.len() as i64,
        filename: &filename,
        signer: meta.signer.as_deref(),
        manifest: meta.manifest.as_deref(),
        security_version: meta.security_version,
    }) {
        Ok(artifact) => {
            evidence::record(
                &state.evidence,
                "firmware.uploaded",
                &format!("firmware:{}", artifact.id),
                &evidence::user_actor(claims.as_deref()),
                serde_json::json!({
                    "version": artifact.version,
                    "build_id": artifact.build_id,
                    "sha256": artifact.sha256,
                    "size": artifact.size,
                    "security_version": artifact.security_version,
                    "signer_key_id": artifact.signer,
                    "has_manifest": artifact.manifest.is_some(),
                    // Signature checked against OTA_TRUSTED_KEYS at upload.
                    "signature_verified": artifact.manifest.is_some()
                        && !state.config.ota_trusted_keys.is_empty(),
                }),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({ "ok": true, "artifact": artifact })),
            )
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&path).await;
            reject(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    }
}

struct UploadMeta {
    version: String,
    build_id: i64,
    signer: Option<String>,
    manifest: Option<String>,
    security_version: Option<i64>,
}

fn reject(status: StatusCode, error: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "ok": false, "error": error })),
    )
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
        Ok(data) => {
            let mut builder = axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/octet-stream")
                .header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{}\"", artifact.filename),
                )
                .header("X-Firmware-SHA256", &artifact.sha256);
            // Devices verify this before writing anything to flash.
            if let Some(manifest) = &artifact.manifest {
                builder = builder.header("X-Firmware-Manifest", manifest);
            }
            builder.body(axum::body::Body::from(data)).unwrap()
        }
        Err(_) => axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("firmware file missing from disk"))
            .unwrap(),
    }
}

/// GET /ota/firmware/:id/manifest — the signed manifest as raw bytes.
pub async fn download_manifest(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    let store = state.store.lock().await;
    let manifest = match store.get_firmware_artifact(id) {
        Ok(Some(a)) => a.manifest,
        Ok(None) => None,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };
    match manifest.and_then(|m| hex::decode(m).ok()) {
        Some(bytes) => (
            StatusCode::OK,
            [("Content-Type", "application/octet-stream")],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "no manifest for this firmware").into_response(),
    }
}

/// DELETE /ota/firmware/:id — delete a firmware artifact.
pub async fn delete_firmware(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<UserClaims>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    let artifact = store.get_firmware_artifact(id).ok().flatten();

    match store.delete_firmware_artifact(id) {
        Ok(true) => {
            if let Some(a) = &artifact {
                let _ = tokio::fs::remove_file(state.firmware_dir.join(&a.filename)).await;
            }
            evidence::record(
                &state.evidence,
                "firmware.deleted",
                &format!("firmware:{id}"),
                &evidence::user_actor(claims.as_deref()),
                serde_json::json!({
                    "version": artifact.as_ref().map(|a| a.version.clone()),
                    "build_id": artifact.as_ref().map(|a| a.build_id),
                    "sha256": artifact.as_ref().map(|a| a.sha256.clone()),
                }),
            );
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
