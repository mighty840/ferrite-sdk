//! Database backup/export functionality.
//!
//! Provides a `GET /admin/backup` endpoint that returns the SQLite database
//! as a downloadable file, and a `GET /admin/retention` endpoint for retention info.

use axum::{body::Body, extract::State, http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;

use crate::AppState;

/// GET /admin/backup — download a consistent SQLite backup.
pub async fn backup_database(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.lock().await;

    // Use SQLite's backup API to create a consistent snapshot.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, std::io::Error>>(4);

    match store.backup_to_bytes() {
        Ok(bytes) => {
            let len = bytes.len();
            // Send the backup in chunks to avoid holding everything in a single allocation.
            tokio::spawn(async move {
                for chunk in bytes.chunks(64 * 1024) {
                    if tx.send(Ok(chunk.to_vec())).await.is_err() {
                        break;
                    }
                }
            });

            let stream = ReceiverStream::new(rx);
            let body = Body::from_stream(stream);

            (
                StatusCode::OK,
                [
                    ("content-type", "application/x-sqlite3"),
                    (
                        "content-disposition",
                        "attachment; filename=\"ferrite-backup.db\"",
                    ),
                    ("content-length", &len.to_string()),
                ],
                body,
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("backup failed: {e}") })),
        )
            .into_response(),
    }
}

/// GET /admin/retention — show current retention policy.
pub async fn retention_info(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let retention_days = state.config.retention_days.unwrap_or(90);

    let store = state.store.lock().await;
    let total_faults = store.count_all_faults().unwrap_or(0);
    let total_metrics = store.count_all_metrics().unwrap_or(0);
    let total_reboots = store.count_all_reboots().unwrap_or(0);

    Json(serde_json::json!({
        "retention_days": retention_days,
        "enabled": retention_days > 0,
        "counts": {
            "faults": total_faults,
            "metrics": total_metrics,
            "reboots": total_reboots,
        }
    }))
}
