//! Data retention policy — auto-purge old metrics, faults, and reboots.
//!
//! Runs as a background task that periodically deletes rows older than
//! the configured retention period. Controlled by `RETENTION_DAYS` env var.

use std::sync::Arc;
use tokio::time::{interval, Duration};

use crate::AppState;

/// Default retention period in days.
const DEFAULT_RETENTION_DAYS: u64 = 90;

/// How often the purge task runs (1 hour).
const PURGE_INTERVAL_SECS: u64 = 3600;

/// Start the background retention purge task.
pub fn spawn_retention_task(state: Arc<AppState>) {
    let retention_days = state
        .config
        .retention_days
        .unwrap_or(DEFAULT_RETENTION_DAYS);
    if retention_days == 0 {
        tracing::info!("Data retention disabled (RETENTION_DAYS=0)");
        return;
    }

    tracing::info!(
        "Data retention: purging data older than {} days (every {}s)",
        retention_days,
        PURGE_INTERVAL_SECS
    );

    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(PURGE_INTERVAL_SECS));
        loop {
            tick.tick().await;
            run_purge(&state, retention_days).await;
        }
    });
}

async fn run_purge(state: &Arc<AppState>, retention_days: u64) {
    let store = state.store.lock().await;
    let cutoff = format!("-{} days", retention_days);

    let mut total = 0u64;

    match store.purge_old_metrics(&cutoff) {
        Ok(n) => total += n as u64,
        Err(e) => tracing::error!("retention: failed to purge metrics: {e}"),
    }
    match store.purge_old_faults(&cutoff) {
        Ok(n) => total += n as u64,
        Err(e) => tracing::error!("retention: failed to purge faults: {e}"),
    }
    match store.purge_old_reboots(&cutoff) {
        Ok(n) => total += n as u64,
        Err(e) => tracing::error!("retention: failed to purge reboots: {e}"),
    }

    if total > 0 {
        tracing::info!("retention: purged {total} old rows");
    }
}
