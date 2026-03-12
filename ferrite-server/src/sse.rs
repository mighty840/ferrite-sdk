//! Server-Sent Events broadcasting for live dashboard updates.
//!
//! Events are published to a `broadcast::Sender` in `AppState` whenever data
//! is ingested (heartbeats, faults, metrics, device registration, etc.).
//! The `/events/stream` endpoint streams these events to connected dashboards.

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::Serialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::AppState;

/// An event broadcast to SSE subscribers.
#[derive(Clone, Debug, Serialize)]
pub struct SsePayload {
    /// Event kind: "heartbeat", "fault", "metric", "reboot", "device_registered", "device_updated"
    pub event_type: String,
    /// JSON payload with event details.
    pub data: serde_json::Value,
}

/// GET /events/stream — SSE endpoint for live updates.
pub async fn event_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(payload) => {
            let data = serde_json::to_string(&payload.data).unwrap_or_default();
            Some(Ok(Event::default().event(payload.event_type).data(data)))
        }
        Err(_) => None, // lagged — skip missed events
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

impl SsePayload {
    pub fn heartbeat(device_id: &str, uptime_ticks: u64) -> Self {
        Self {
            event_type: "heartbeat".into(),
            data: serde_json::json!({
                "device_id": device_id,
                "uptime_ticks": uptime_ticks,
            }),
        }
    }

    pub fn fault(device_id: &str, fault_type: u8, pc: u32) -> Self {
        Self {
            event_type: "fault".into(),
            data: serde_json::json!({
                "device_id": device_id,
                "fault_type": fault_type,
                "pc": format!("0x{:08X}", pc),
            }),
        }
    }

    pub fn metric(device_id: &str, key: &str, value_json: &str) -> Self {
        Self {
            event_type: "metric".into(),
            data: serde_json::json!({
                "device_id": device_id,
                "key": key,
                "value": value_json,
            }),
        }
    }

    pub fn reboot(device_id: &str, reason: u8) -> Self {
        Self {
            event_type: "reboot".into(),
            data: serde_json::json!({
                "device_id": device_id,
                "reason": reason,
            }),
        }
    }

    pub fn device_registered(device_id: &str) -> Self {
        Self {
            event_type: "device_registered".into(),
            data: serde_json::json!({ "device_id": device_id }),
        }
    }

    pub fn ota_available(device_id: &str, target_version: &str, target_build_id: i64) -> Self {
        Self {
            event_type: "ota_available".into(),
            data: serde_json::json!({
                "device_id": device_id,
                "target_version": target_version,
                "target_build_id": target_build_id,
            }),
        }
    }
}
