//! OTA Campaign Engine — orchestrate firmware rollouts across device fleets.
//!
//! Campaigns tie a firmware artifact to a set of target devices and manage
//! the rollout lifecycle: created -> active -> completed (or paused/rolled_back/failed).

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
pub struct CreateCampaignRequest {
    pub name: String,
    pub firmware_id: i64,
    pub target_version: String,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    pub target_group_id: Option<i64>,
    pub target_tags: Option<String>,
    #[serde(default = "default_rollout_percent")]
    pub rollout_percent: Option<i64>,
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: Option<f64>,
}

fn default_strategy() -> String {
    "immediate".to_string()
}

fn default_rollout_percent() -> Option<i64> {
    Some(100)
}

fn default_failure_threshold() -> Option<f64> {
    Some(5.0)
}

/// POST /ota/campaigns — create a new OTA campaign.
///
/// Automatically resolves target devices from the specified group (or all devices)
/// and adds them as "pending" campaign devices.
pub async fn create_campaign(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateCampaignRequest>,
) -> impl IntoResponse {
    if req.name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "name is required" })),
        );
    }

    let valid_strategies = ["immediate", "canary", "scheduled"];
    if !valid_strategies.contains(&req.strategy.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("invalid strategy '{}', must be one of: immediate, canary, scheduled", req.strategy)
            })),
        );
    }

    let rollout_percent = req.rollout_percent.unwrap_or(100);
    let failure_threshold = req.failure_threshold.unwrap_or(5.0);

    let store = state.store.lock().await;

    // Verify firmware artifact exists.
    match store.get_firmware_artifact(req.firmware_id) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "firmware artifact not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    }

    // Create the campaign.
    let campaign = match store.create_campaign(
        &req.name,
        req.firmware_id,
        &req.target_version,
        &req.strategy,
        req.target_group_id,
        req.target_tags.as_deref(),
        rollout_percent,
        failure_threshold,
    ) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Resolve target devices.
    let devices = if let Some(group_id) = req.target_group_id {
        match store.list_group_devices(group_id) {
            Ok(d) => d,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        }
    } else {
        match store.list_devices() {
            Ok(d) => d,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        }
    };

    let device_ids: Vec<String> = devices.iter().map(|d| d.device_id.clone()).collect();
    let added = match store.add_devices_to_campaign(campaign.id, &device_ids) {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "campaign": campaign,
            "devices_added": added,
        })),
    )
}

/// GET /ota/campaigns — list all campaigns.
pub async fn list_campaigns(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_campaigns() {
        Ok(campaigns) => (
            StatusCode::OK,
            Json(serde_json::json!({ "campaigns": campaigns })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /ota/campaigns/:id — get campaign with device status summary.
pub async fn get_campaign(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.get_campaign_summary(id) {
        Ok(Some(summary)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "campaign": summary })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "campaign not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /ota/campaigns/:id/activate — set campaign to "active" and create OTA targets.
///
/// For "immediate" strategy, sets OTA targets for all campaign devices.
/// For "canary" strategy, sets OTA targets for the first N devices based on rollout_percent.
pub async fn activate_campaign(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;

    let campaign = match store.get_campaign(id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "campaign not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    if campaign.status != "created" && campaign.status != "paused" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("cannot activate campaign in '{}' status", campaign.status)
            })),
        );
    }

    // Get firmware artifact for the URL.
    let firmware = match store.get_firmware_artifact(campaign.firmware_id) {
        Ok(Some(fw)) => fw,
        Ok(None) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "campaign firmware artifact not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Get all campaign devices.
    let devices = match store.list_campaign_devices(id) {
        Ok(d) => d,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    // Determine how many devices to target.
    let pending_devices: Vec<_> = devices.iter().filter(|d| d.status == "pending").collect();

    let target_count = match campaign.strategy.as_str() {
        "canary" => {
            let count = (pending_devices.len() as i64 * campaign.rollout_percent / 100).max(1);
            count as usize
        }
        _ => pending_devices.len(), // "immediate" and "scheduled" target all
    };

    let firmware_url = format!("/ota/firmware/{}/download", firmware.id);

    // Set OTA targets for the selected devices.
    let mut targets_set = 0usize;
    for device in pending_devices.iter().take(target_count) {
        if let Err(e) = store.set_ota_target(
            &device.device_id,
            &campaign.target_version,
            firmware.build_id,
            Some(&firmware_url),
        ) {
            tracing::warn!(
                device = %device.device_id,
                error = %e,
                "failed to set OTA target for campaign device"
            );
            continue;
        }
        // Mark device as downloading.
        let _ = store.update_campaign_device_status(id, &device.device_id, "downloading");
        targets_set += 1;
    }

    // Update campaign status to active.
    if let Err(e) = store.update_campaign_status(id, "active") {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        );
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "status": "active",
            "targets_set": targets_set,
        })),
    )
}

/// POST /ota/campaigns/:id/pause — set campaign status to "paused".
pub async fn pause_campaign(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;

    let campaign = match store.get_campaign(id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "campaign not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    if campaign.status != "active" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("cannot pause campaign in '{}' status", campaign.status)
            })),
        );
    }

    match store.update_campaign_status(id, "paused") {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "status": "paused" })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "campaign not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /ota/campaigns/:id/rollback — set status to "rolled_back" and remove all OTA targets.
pub async fn rollback_campaign(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;

    let campaign = match store.get_campaign(id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "campaign not found" })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    if campaign.status != "active" && campaign.status != "paused" {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("cannot rollback campaign in '{}' status", campaign.status)
            })),
        );
    }

    // Delete OTA targets for all campaign devices.
    let targets_removed = match store.delete_ota_targets_for_campaign(id) {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            );
        }
    };

    match store.update_campaign_status(id, "rolled_back") {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "status": "rolled_back",
                "targets_removed": targets_removed,
            })),
        ),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "campaign not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /ota/campaigns/:id/devices — list all devices in the campaign with their status.
pub async fn list_campaign_devices(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_campaign_devices(id) {
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
