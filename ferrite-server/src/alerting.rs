//! Webhook alerting for faults and device offline events.
//!
//! When `ALERT_WEBHOOK_URL` is configured, the server sends JSON payloads
//! to the webhook on:
//! - New fault event
//! - Device offline (no heartbeat for >N minutes)
//!
//! The payload format is compatible with Slack/Discord incoming webhooks
//! when wrapped in a `text` field.

use serde::Serialize;
use std::sync::Arc;
use tokio::time::{interval, Duration};

use crate::AppState;

/// Alert payload sent to the webhook.
#[derive(Debug, Clone, Serialize)]
pub struct AlertPayload {
    /// Alert type: "fault", "device_offline"
    pub alert_type: String,
    /// Device identifier
    pub device_id: String,
    /// Human-readable summary
    pub summary: String,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Slack/Discord compatible text field
    pub text: String,
}

impl AlertPayload {
    pub fn fault(device_id: &str, fault_type: u8, pc: u32, symbol: Option<&str>) -> Self {
        let sym_str = symbol.unwrap_or("unknown");
        let summary = format!(
            "Fault on device {}: type={}, pc=0x{:08X} ({})",
            device_id, fault_type, pc, sym_str
        );
        Self {
            alert_type: "fault".into(),
            device_id: device_id.into(),
            summary: summary.clone(),
            timestamp: chrono_now(),
            details: Some(serde_json::json!({
                "fault_type": fault_type,
                "pc": format!("0x{:08X}", pc),
                "symbol": symbol,
            })),
            text: format!(":warning: {}", summary),
        }
    }

    pub fn device_offline(device_id: &str, last_seen: &str, offline_minutes: u64) -> Self {
        let summary = format!(
            "Device {} offline for >{} minutes (last seen: {})",
            device_id, offline_minutes, last_seen
        );
        Self {
            alert_type: "device_offline".into(),
            device_id: device_id.into(),
            summary: summary.clone(),
            timestamp: chrono_now(),
            details: Some(serde_json::json!({
                "last_seen": last_seen,
                "offline_threshold_minutes": offline_minutes,
            })),
            text: format!(":red_circle: {}", summary),
        }
    }
}

fn chrono_now() -> String {
    // Use a simple UTC timestamp without pulling in chrono crate.
    // The server already has datetime('now') in SQLite; for alerts we use system time.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Format as ISO 8601 (approximate — no chrono dependency)
    format!("{}Z", now)
}

/// Send an alert to the configured webhook URL.
pub async fn send_alert(state: &Arc<AppState>, payload: AlertPayload) {
    let Some(ref url) = state.config.alert_webhook_url else {
        return;
    };

    let client = reqwest::Client::new();
    match client.post(url).json(&payload).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                tracing::warn!(
                    "Alert webhook returned {}: {}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                );
            } else {
                tracing::debug!("Alert sent: {}", payload.summary);
            }
        }
        Err(e) => {
            tracing::error!("Failed to send alert webhook: {e}");
        }
    }
}

/// Send a fault alert (called from ingest handler).
pub fn send_fault_alert(
    state: &Arc<AppState>,
    device_id: &str,
    fault_type: u8,
    pc: u32,
    symbol: Option<&str>,
) {
    if state.config.alert_webhook_url.is_none() {
        return;
    }
    let payload = AlertPayload::fault(device_id, fault_type, pc, symbol);
    let state = state.clone();
    tokio::spawn(async move {
        send_alert(&state, payload).await;
    });
}

/// Background task that periodically checks for offline devices and sends alerts.
pub fn spawn_offline_check_task(state: Arc<AppState>) {
    if state.config.alert_webhook_url.is_none() {
        tracing::info!("Alerting disabled (ALERT_WEBHOOK_URL not set)");
        return;
    }

    let offline_minutes = state.config.alert_offline_minutes;
    tracing::info!(
        "Alerting enabled: webhook={}, offline_threshold={}min",
        state.config.alert_webhook_url.as_deref().unwrap_or(""),
        offline_minutes
    );

    tokio::spawn(async move {
        // Check every minute
        let mut tick = interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            check_offline_devices(&state, offline_minutes).await;
        }
    });
}

async fn check_offline_devices(state: &Arc<AppState>, offline_minutes: u64) {
    let store = state.store.lock().await;
    let devices = match store.list_devices() {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("alerting: failed to list devices: {e}");
            return;
        }
    };

    let cutoff_modifier = format!("-{} minutes", offline_minutes);
    let cutoff = match store.datetime_now_offset(&cutoff_modifier) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Drop the store lock before sending HTTP requests
    let alerts: Vec<AlertPayload> = devices
        .iter()
        .filter(|d| {
            // Only alert on devices that were previously "online" and are now stale
            d.status.as_deref() == Some("online") && d.last_seen < cutoff
        })
        .map(|d| AlertPayload::device_offline(&d.device_id, &d.last_seen, offline_minutes))
        .collect();

    // Update status to "offline" for stale devices
    for device in &devices {
        if device.status.as_deref() == Some("online") && device.last_seen < cutoff {
            let _ = store.update_device_status(device.id, "offline");
        }
    }
    drop(store);

    for alert in alerts {
        send_alert(state, alert).await;
    }
}
