//! Prometheus-compatible `/metrics/prometheus` endpoint.
//!
//! Exposes server statistics in Prometheus text exposition format.
//! Includes both database-level counts and runtime request counters.

use axum::{extract::State, http::StatusCode, response::IntoResponse};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::AppState;

/// Runtime request counters (incremented by handlers).
pub struct RequestCounters {
    pub ingest_requests: AtomicU64,
    pub ingest_chunks: AtomicU64,
    pub auth_failures: AtomicU64,
    pub sse_connections: AtomicU64,
}

impl Default for RequestCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestCounters {
    pub fn new() -> Self {
        Self {
            ingest_requests: AtomicU64::new(0),
            ingest_chunks: AtomicU64::new(0),
            auth_failures: AtomicU64::new(0),
            sse_connections: AtomicU64::new(0),
        }
    }
}

/// GET /metrics/prometheus
pub async fn prometheus_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.lock().await;

    let devices = store.list_devices().map(|d| d.len()).unwrap_or(0);
    let online_devices = store
        .list_devices()
        .map(|devs| {
            devs.iter()
                .filter(|d| d.status.as_deref() == Some("online"))
                .count()
        })
        .unwrap_or(0);

    let total_faults = store.count_all_faults().unwrap_or(0);
    let total_metrics = store.count_all_metrics().unwrap_or(0);
    let total_reboots = store.count_all_reboots().unwrap_or(0);
    let total_groups = store.count_all_groups().unwrap_or(0);

    let counters = &state.counters;

    let body = format!(
        "# HELP ferrite_devices_total Total number of registered devices.\n\
         # TYPE ferrite_devices_total gauge\n\
         ferrite_devices_total {devices}\n\
         \n\
         # HELP ferrite_devices_online Number of devices with status 'online'.\n\
         # TYPE ferrite_devices_online gauge\n\
         ferrite_devices_online {online_devices}\n\
         \n\
         # HELP ferrite_faults_total Total number of stored fault events.\n\
         # TYPE ferrite_faults_total gauge\n\
         ferrite_faults_total {total_faults}\n\
         \n\
         # HELP ferrite_metrics_total Total number of stored metric rows.\n\
         # TYPE ferrite_metrics_total gauge\n\
         ferrite_metrics_total {total_metrics}\n\
         \n\
         # HELP ferrite_reboots_total Total number of stored reboot events.\n\
         # TYPE ferrite_reboots_total gauge\n\
         ferrite_reboots_total {total_reboots}\n\
         \n\
         # HELP ferrite_groups_total Total number of device groups.\n\
         # TYPE ferrite_groups_total gauge\n\
         ferrite_groups_total {total_groups}\n\
         \n\
         # HELP ferrite_ingest_requests_total Total ingest HTTP requests since startup.\n\
         # TYPE ferrite_ingest_requests_total counter\n\
         ferrite_ingest_requests_total {}\n\
         \n\
         # HELP ferrite_ingest_chunks_total Total chunks ingested since startup.\n\
         # TYPE ferrite_ingest_chunks_total counter\n\
         ferrite_ingest_chunks_total {}\n\
         \n\
         # HELP ferrite_auth_failures_total Total authentication failures since startup.\n\
         # TYPE ferrite_auth_failures_total counter\n\
         ferrite_auth_failures_total {}\n\
         \n\
         # HELP ferrite_sse_connections_total Total SSE connections opened since startup.\n\
         # TYPE ferrite_sse_connections_total counter\n\
         ferrite_sse_connections_total {}\n",
        counters.ingest_requests.load(Ordering::Relaxed),
        counters.ingest_chunks.load(Ordering::Relaxed),
        counters.auth_failures.load(Ordering::Relaxed),
        counters.sse_connections.load(Ordering::Relaxed),
    );

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}
