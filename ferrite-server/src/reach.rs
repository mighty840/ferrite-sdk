//! Update-reach reports: which devices run which build, and whether a
//! campaign's security update reached them (CRA security-update record).
//!
//! - `GET /reports/fleet/builds?group_id=&tag=` — devices per build
//! - `GET /reports/campaigns/:id` — campaign reach and time-to-patch
//! - `GET /reports/devices/:device_id/firmware-history` — builds over time
//!
//! All accept `?format=csv` for export; JSON otherwise.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Deserialize, Default)]
pub struct ReportQuery {
    pub format: Option<String>,
    pub group_id: Option<i64>,
    pub tag: Option<String>,
}

impl ReportQuery {
    fn csv(&self) -> bool {
        self.format.as_deref() == Some("csv")
    }
}

/// GET /reports/fleet/builds
pub async fn fleet_builds(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let online_minutes = state.config.alert_offline_minutes;
    let store = state.store.lock().await;
    let builds = match store.build_distribution(q.group_id, q.tag.as_deref(), online_minutes) {
        Ok(b) => b,
        Err(e) => return error(e),
    };
    drop(store);

    if q.csv() {
        let mut out = Csv::new(&[
            "firmware_version",
            "build_id",
            "devices",
            "online",
            "last_seen",
        ]);
        for b in &builds {
            out.row(&[
                &b.firmware_version,
                &b.build_id.to_string(),
                &b.devices.to_string(),
                &b.online.to_string(),
                b.last_seen.as_deref().unwrap_or(""),
            ]);
        }
        return out.into_response("fleet-builds.csv");
    }
    let total: i64 = builds.iter().map(|b| b.devices).sum();
    Json(serde_json::json!({
        "total_devices": total,
        "online_window_minutes": online_minutes,
        "builds": builds,
    }))
    .into_response()
}

/// GET /reports/campaigns/:id
pub async fn campaign_reach(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let online_minutes = state.config.alert_offline_minutes;
    let store = state.store.lock().await;
    let reach = match store.campaign_reach(id, online_minutes) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "campaign not found" })),
            )
                .into_response()
        }
        Err(e) => return error(e),
    };
    drop(store);

    if q.csv() {
        let mut out = Csv::new(&[
            "campaign_id",
            "campaign",
            "target_version",
            "target_build_id",
            "device_id",
            "status",
            "on_target",
            "current_version",
            "current_build_id",
            "reachable",
            "last_seen",
            "patched_before_campaign",
            "installed_at",
            "time_to_patch_secs",
        ]);
        for d in &reach.devices {
            out.row(&[
                &reach.campaign_id.to_string(),
                &reach.name,
                &reach.target_version,
                &reach.target_build_id.to_string(),
                &d.device_id,
                &d.status,
                &d.on_target.to_string(),
                d.current_version.as_deref().unwrap_or(""),
                &d.current_build_id
                    .map(|b| b.to_string())
                    .unwrap_or_default(),
                &d.reachable.to_string(),
                d.last_seen.as_deref().unwrap_or(""),
                &d.patched_before_campaign.to_string(),
                d.installed_at.as_deref().unwrap_or(""),
                &d.time_to_patch_secs
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
            ]);
        }
        return out.into_response(&format!("campaign-{id}-reach.csv"));
    }
    Json(serde_json::json!({ "online_window_minutes": online_minutes, "reach": reach }))
        .into_response()
}

/// GET /reports/devices/:device_id/firmware-history
pub async fn firmware_history(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Query(q): Query<ReportQuery>,
) -> Response {
    let store = state.store.lock().await;
    let history = match store.firmware_history(&device_id) {
        Ok(h) => h,
        Err(e) => return error(e),
    };
    drop(store);

    if q.csv() {
        let mut out = Csv::new(&["device_id", "firmware_version", "build_id", "observed_at"]);
        for h in &history {
            out.row(&[
                &device_id,
                &h.firmware_version,
                &h.build_id.to_string(),
                &h.observed_at,
            ]);
        }
        return out.into_response(&format!("{device_id}-firmware-history.csv"));
    }
    Json(serde_json::json!({ "device_id": device_id, "history": history })).into_response()
}

fn error(e: rusqlite::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
        .into_response()
}

/// Minimal RFC 4180 CSV writer.
struct Csv {
    buf: String,
}

impl Csv {
    fn new(header: &[&str]) -> Self {
        let mut csv = Self { buf: String::new() };
        csv.row(header);
        csv
    }

    fn row(&mut self, fields: &[&str]) {
        for (i, field) in fields.iter().enumerate() {
            if i > 0 {
                self.buf.push(',');
            }
            self.buf.push_str(&escape(field));
        }
        self.buf.push_str("\r\n");
    }

    fn into_response(self, filename: &str) -> Response {
        let disposition = format!("attachment; filename=\"{}\"", filename.replace('"', ""));
        (
            [
                (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
                (header::CONTENT_DISPOSITION, disposition),
            ],
            self.buf,
        )
            .into_response()
    }
}

/// Quote when needed, and neutralize spreadsheet formulas: device IDs and
/// versions come from devices, so a value like `=HYPERLINK(...)` must not
/// execute when an auditor opens the export.
fn escape(field: &str) -> String {
    let field = if field.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{field}")
    } else {
        field.to_string()
    };
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field
    }
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn csv_escaping() {
        assert_eq!(escape("plain"), "plain");
        assert_eq!(escape("a,b"), "\"a,b\"");
        assert_eq!(escape("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape("line\nbreak"), "\"line\nbreak\"");
        assert_eq!(escape("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(escape("-1"), "'-1");
    }
}
