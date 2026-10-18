use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tabled::Tabled;

/// API client for the ferrite-server REST API.
pub struct ApiClient {
    pub base_url: String,
    pub auth_header: String,
    pub http: Client,
}

// ---------------------------------------------------------------------------
// API response types (matching ferrite-server/src/store.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct Device {
    pub id: i64,
    pub device_id: String,
    pub firmware_version: String,
    pub build_id: u64,
    pub first_seen: String,
    pub last_seen: String,
    #[tabled(display_with = "display_option")]
    pub device_key: Option<i64>,
    #[tabled(display_with = "display_option")]
    pub name: Option<String>,
    #[tabled(display_with = "display_option")]
    pub status: Option<String>,
    #[tabled(display_with = "display_option")]
    pub tags: Option<String>,
    #[tabled(display_with = "display_option")]
    pub provisioned_by: Option<String>,
    #[tabled(display_with = "display_option")]
    pub provisioned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct FaultEvent {
    pub id: i64,
    pub device_rowid: i64,
    pub device_id: String,
    pub fault_type: u8,
    #[tabled(display_with = "display_hex32")]
    pub pc: u32,
    #[tabled(display_with = "display_hex32")]
    pub lr: u32,
    #[tabled(display_with = "display_hex32")]
    pub cfsr: u32,
    #[tabled(display_with = "display_hex32")]
    pub hfsr: u32,
    #[tabled(display_with = "display_hex32")]
    pub mmfar: u32,
    #[tabled(display_with = "display_hex32")]
    pub bfar: u32,
    #[tabled(display_with = "display_hex32")]
    pub sp: u32,
    pub stack_snapshot: String,
    #[tabled(display_with = "display_option")]
    pub symbol: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct MetricRow {
    pub id: i64,
    pub device_rowid: i64,
    pub device_id: String,
    pub key: String,
    pub metric_type: u8,
    pub value_json: String,
    pub timestamp_ticks: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct DeviceGroup {
    pub id: i64,
    pub name: String,
    #[tabled(display_with = "display_option")]
    pub description: Option<String>,
    pub device_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct OtaTarget {
    pub id: i64,
    pub device_id: String,
    pub target_version: String,
    pub target_build_id: i64,
    #[tabled(display_with = "display_option")]
    pub firmware_url: Option<String>,
    pub created_at: String,
}

/// Devices running one build (`GET /reports/fleet/builds`).
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct BuildRow {
    pub firmware_version: String,
    pub build_id: u64,
    pub devices: i64,
    pub online: i64,
    #[tabled(display_with = "display_option")]
    pub last_seen: Option<String>,
}

/// One campaign device in an update-reach report.
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct ReachDevice {
    pub device_id: String,
    pub status: String,
    pub on_target: bool,
    #[tabled(display_with = "display_option")]
    pub current_version: Option<String>,
    #[tabled(display_with = "display_option")]
    pub current_build_id: Option<u64>,
    pub reachable: bool,
    #[tabled(display_with = "display_option")]
    pub installed_at: Option<String>,
    #[tabled(display_with = "display_option")]
    pub time_to_patch_secs: Option<i64>,
}

/// A build a device reported, first seen at `observed_at`.
#[derive(Debug, Clone, Serialize, Deserialize, Tabled)]
pub struct FirmwareObservation {
    pub build_id: u64,
    pub firmware_version: String,
    pub observed_at: String,
}

// ---------------------------------------------------------------------------
// Display helpers for tabled
// ---------------------------------------------------------------------------

fn display_option<T: std::fmt::Display>(o: &Option<T>) -> String {
    match o {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    }
}

fn display_hex32(v: &u32) -> String {
    format!("0x{v:08X}")
}

// ---------------------------------------------------------------------------
// ApiClient implementation
// ---------------------------------------------------------------------------

impl ApiClient {
    pub fn new(base_url: &str, auth_header: &str) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build HTTP client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header: auth_header.to_string(),
            http,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Check that a response was successful; surface server error messages.
    fn check_response(&self, resp: reqwest::blocking::Response) -> Result<serde_json::Value> {
        let status = resp.status();
        let body: serde_json::Value = resp.json().context("failed to parse response body")?;
        if !status.is_success() {
            let err_msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            bail!("server returned {}: {}", status, err_msg);
        }
        Ok(body)
    }

    // -- Devices --

    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let resp = self
            .http
            .get(self.url("/devices"))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let devices: Vec<Device> = serde_json::from_value(
            body.get("devices")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(devices)
    }

    pub fn get_device(&self, device_key: &str) -> Result<Device> {
        let resp = self
            .http
            .get(self.url(&format!("/devices/{device_key}")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let device: Device =
            serde_json::from_value(body.get("device").cloned().unwrap_or_default())?;
        Ok(device)
    }

    pub fn delete_device(&self, device_key: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/devices/{device_key}")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        self.check_response(resp)?;
        Ok(())
    }

    pub fn list_device_faults(
        &self,
        device_id: &str,
        since: Option<&str>,
    ) -> Result<Vec<FaultEvent>> {
        let mut url = format!("/devices/{device_id}/faults");
        if let Some(s) = since {
            url.push_str(&format!("?since={s}"));
        }
        let resp = self
            .http
            .get(self.url(&url))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let faults: Vec<FaultEvent> = serde_json::from_value(
            body.get("faults")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(faults)
    }

    pub fn list_device_metrics(
        &self,
        device_id: &str,
        since: Option<&str>,
    ) -> Result<Vec<MetricRow>> {
        let mut url = format!("/devices/{device_id}/metrics");
        if let Some(s) = since {
            url.push_str(&format!("?since={s}"));
        }
        let resp = self
            .http
            .get(self.url(&url))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let metrics: Vec<MetricRow> = serde_json::from_value(
            body.get("metrics")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(metrics)
    }

    // -- Faults (all) --

    pub fn list_all_faults(&self, since: Option<&str>) -> Result<Vec<FaultEvent>> {
        let mut url = "/faults".to_string();
        if let Some(s) = since {
            url.push_str(&format!("?since={s}"));
        }
        let resp = self
            .http
            .get(self.url(&url))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let faults: Vec<FaultEvent> = serde_json::from_value(
            body.get("faults")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(faults)
    }

    // -- Metrics (all) --

    pub fn list_all_metrics(
        &self,
        since: Option<&str>,
        key: Option<&str>,
    ) -> Result<Vec<MetricRow>> {
        let mut params = vec![];
        if let Some(s) = since {
            params.push(format!("since={s}"));
        }
        if let Some(k) = key {
            params.push(format!("key={k}"));
        }
        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        let resp = self
            .http
            .get(self.url(&format!("/metrics{query}")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let metrics: Vec<MetricRow> = serde_json::from_value(
            body.get("metrics")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(metrics)
    }

    // -- Groups --

    pub fn list_groups(&self) -> Result<Vec<DeviceGroup>> {
        let resp = self
            .http
            .get(self.url("/groups"))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let groups: Vec<DeviceGroup> = serde_json::from_value(
            body.get("groups")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(groups)
    }

    pub fn create_group(&self, name: &str, description: Option<&str>) -> Result<DeviceGroup> {
        let mut body = serde_json::json!({ "name": name });
        if let Some(desc) = description {
            body["description"] = serde_json::Value::String(desc.to_string());
        }
        let resp = self
            .http
            .post(self.url("/groups"))
            .header("Authorization", &self.auth_header)
            .json(&body)
            .send()
            .context("failed to reach server")?;
        let resp_body = self.check_response(resp)?;
        let group: DeviceGroup =
            serde_json::from_value(resp_body.get("group").cloned().unwrap_or_default())?;
        Ok(group)
    }

    pub fn add_device_to_group(&self, group_id: i64, device_id: &str) -> Result<()> {
        let resp = self
            .http
            .post(self.url(&format!("/groups/{group_id}/devices/{device_id}")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        self.check_response(resp)?;
        Ok(())
    }

    pub fn remove_device_from_group(&self, group_id: i64, device_id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/groups/{group_id}/devices/{device_id}")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        self.check_response(resp)?;
        Ok(())
    }

    pub fn list_group_devices(&self, group_id: i64) -> Result<Vec<Device>> {
        let resp = self
            .http
            .get(self.url(&format!("/groups/{group_id}/devices")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let devices: Vec<Device> = serde_json::from_value(
            body.get("devices")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(devices)
    }

    // -- OTA --

    pub fn set_ota_target(
        &self,
        device_id: &str,
        target_version: &str,
        target_build_id: i64,
        firmware_url: Option<&str>,
    ) -> Result<OtaTarget> {
        let mut body = serde_json::json!({
            "device_id": device_id,
            "target_version": target_version,
            "target_build_id": target_build_id,
        });
        if let Some(url) = firmware_url {
            body["firmware_url"] = serde_json::Value::String(url.to_string());
        }
        let resp = self
            .http
            .post(self.url("/ota/targets"))
            .header("Authorization", &self.auth_header)
            .json(&body)
            .send()
            .context("failed to reach server")?;
        let resp_body = self.check_response(resp)?;
        let target: OtaTarget =
            serde_json::from_value(resp_body.get("target").cloned().unwrap_or_default())?;
        Ok(target)
    }

    // -- Reports --

    fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let resp = self
            .http
            .get(self.url(path))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        self.check_response(resp)
    }

    pub fn report_builds(&self, group_id: Option<i64>, tag: Option<&str>) -> Result<Vec<BuildRow>> {
        let mut query = Vec::new();
        if let Some(g) = group_id {
            query.push(format!("group_id={g}"));
        }
        if let Some(t) = tag {
            query.push(format!("tag={}", urlencode(t)));
        }
        let path = if query.is_empty() {
            "/reports/fleet/builds".to_string()
        } else {
            format!("/reports/fleet/builds?{}", query.join("&"))
        };
        let body = self.get_json(&path)?;
        Ok(serde_json::from_value(body["builds"].clone())?)
    }

    /// The campaign's update-reach report as returned by the server.
    pub fn report_campaign(&self, campaign_id: i64) -> Result<serde_json::Value> {
        let body = self.get_json(&format!("/reports/campaigns/{campaign_id}"))?;
        Ok(body["reach"].clone())
    }

    pub fn report_firmware_history(&self, device_id: &str) -> Result<Vec<FirmwareObservation>> {
        let body = self.get_json(&format!(
            "/reports/devices/{}/firmware-history",
            urlencode(device_id)
        ))?;
        Ok(serde_json::from_value(body["history"].clone())?)
    }

    /// Upload a firmware image; returns the stored artifact as JSON.
    pub fn upload_firmware(
        &self,
        image: Vec<u8>,
        manifest_hex: Option<&str>,
        version: Option<&str>,
        build_id: Option<u64>,
    ) -> Result<serde_json::Value> {
        let mut req = self
            .http
            .post(self.url("/ota/firmware"))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/octet-stream")
            .body(image);
        if let Some(m) = manifest_hex {
            req = req.header("X-Firmware-Manifest", m);
        }
        if let Some(v) = version {
            req = req.header("X-Firmware-Version", v);
        }
        if let Some(b) = build_id {
            req = req.header("X-Build-Id", b.to_string());
        }
        let resp = req.send().context("failed to reach server")?;
        let body = self.check_response(resp)?;
        Ok(body.get("artifact").cloned().unwrap_or_default())
    }

    pub fn get_ota_target(&self, device_id: &str) -> Result<OtaTarget> {
        let resp = self
            .http
            .get(self.url(&format!("/ota/targets/{device_id}")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let resp_body = self.check_response(resp)?;
        let target: OtaTarget =
            serde_json::from_value(resp_body.get("target").cloned().unwrap_or_default())?;
        Ok(target)
    }

    pub fn delete_ota_target(&self, device_id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/ota/targets/{device_id}")))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        self.check_response(resp)?;
        Ok(())
    }

    pub fn list_ota_targets(&self) -> Result<Vec<OtaTarget>> {
        let resp = self
            .http
            .get(self.url("/ota/targets"))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        let body = self.check_response(resp)?;
        let targets: Vec<OtaTarget> = serde_json::from_value(
            body.get("targets")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )?;
        Ok(targets)
    }

    // -- Health (for login validation) --

    pub fn health(&self) -> Result<()> {
        let resp = self
            .http
            .get(self.url("/health"))
            .header("Authorization", &self.auth_header)
            .send()
            .context("failed to reach server")?;
        if !resp.status().is_success() {
            bail!("server returned {}", resp.status());
        }
        Ok(())
    }
}

/// Percent-encode a path or query component.
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
