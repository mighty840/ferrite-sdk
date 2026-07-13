use super::types::*;
use crate::auth::{AuthModeInfo, AuthState, AuthToken};
use serde::{Deserialize, Serialize};

/// HTTP client for communicating with the ferrite backend API.
pub struct ApiClient {
    base_url: String,
    token: Option<AuthToken>,
}

/// Resolve the API base URL from the browser's origin (same-origin for proxied dev).
pub fn api_url() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "http://localhost:4000".into())
}

/// Create an ApiClient configured with the current auth token (if authenticated).
pub fn authenticated_client(auth_state: &AuthState) -> ApiClient {
    let mut client = ApiClient::new(&api_url());
    if let AuthState::Authenticated { ref token, .. } = auth_state {
        client.set_token(token.clone());
    }
    client
}

#[derive(Deserialize)]
struct DevicesResponse {
    devices: Vec<Device>,
}

#[derive(Deserialize)]
struct FaultsResponse {
    faults: Vec<FaultEvent>,
}

#[derive(Deserialize)]
struct MetricsResponse {
    metrics: Vec<MetricRow>,
}

#[derive(Serialize)]
pub struct RegisterDeviceRequest {
    device_key: String,
    name: Option<String>,
    tags: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: None,
        }
    }

    pub fn set_token(&mut self, token: AuthToken) {
        self.token = Some(token);
    }

    fn auth_header(&self) -> Vec<(String, String)> {
        match &self.token {
            Some(token) => vec![("Authorization".into(), token.header_value())],
            None => vec![],
        }
    }

    fn build_get(&self, url: &str) -> reqwest::RequestBuilder {
        let client = reqwest::Client::new();
        let mut req = client.get(url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        req
    }

    fn build_post(&self, url: &str) -> reqwest::RequestBuilder {
        let client = reqwest::Client::new();
        let mut req = client.post(url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        req
    }

    fn build_put(&self, url: &str) -> reqwest::RequestBuilder {
        let client = reqwest::Client::new();
        let mut req = client.put(url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        req
    }

    fn build_delete(&self, url: &str) -> reqwest::RequestBuilder {
        let client = reqwest::Client::new();
        let mut req = client.delete(url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        req
    }

    /// Discover the server's authentication mode.
    pub async fn get_auth_mode(&self) -> Result<AuthModeInfo, ApiError> {
        let url = format!("{}/auth/mode", self.base_url);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => resp
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string())),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn list_devices(&self) -> Result<Vec<Device>, ApiError> {
        let url = format!("{}/devices", self.base_url);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                let wrapper: DevicesResponse = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(wrapper.devices)
            }
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn list_faults(&self) -> Result<Vec<FaultEvent>, ApiError> {
        let url = format!("{}/faults", self.base_url);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                let wrapper: FaultsResponse = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(wrapper.faults)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn list_device_faults(&self, device_id: &str) -> Result<Vec<FaultEvent>, ApiError> {
        let url = format!("{}/devices/{}/faults", self.base_url, device_id);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                let wrapper: FaultsResponse = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(wrapper.faults)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn list_device_metrics(&self, device_id: &str) -> Result<Vec<MetricRow>, ApiError> {
        let url = format!("{}/devices/{}/metrics", self.base_url, device_id);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                let wrapper: MetricsResponse = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(wrapper.metrics)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn list_all_metrics(&self) -> Result<Vec<MetricRow>, ApiError> {
        let url = format!("{}/metrics", self.base_url);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                let wrapper: MetricsResponse = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(wrapper.metrics)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn list_crash_groups(&self) -> Result<Vec<CrashGroup>, ApiError> {
        let url = format!("{}/crashes", self.base_url);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct Wrapper {
                    crash_groups: Vec<CrashGroup>,
                }
                let w: Wrapper = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.crash_groups)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn get_crash_group(
        &self,
        id: i64,
    ) -> Result<(CrashGroup, Vec<FaultEvent>), ApiError> {
        let url = format!("{}/crashes/{}", self.base_url, id);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct Wrapper {
                    group: CrashGroup,
                    occurrences: Vec<FaultEvent>,
                }
                let w: Wrapper = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok((w.group, w.occurrences))
            }
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn register_device(
        &self,
        device_key: &str,
        name: Option<String>,
        tags: Option<String>,
    ) -> Result<Device, ApiError> {
        let url = format!("{}/devices/register", self.base_url);
        let resp = self
            .build_post(&url)
            .json(&RegisterDeviceRequest {
                device_key: device_key.to_string(),
                name,
                tags,
            })
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct Wrapper {
                    device: Device,
                }
                let w: Wrapper = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.device)
            }
            400 => Err(ApiError::Parse("invalid device key".into())),
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn register_devices_bulk(
        &self,
        devices: &[RegisterDeviceRequest],
    ) -> Result<Vec<Device>, ApiError> {
        let url = format!("{}/devices/register/bulk", self.base_url);
        let resp = self
            .build_post(&url)
            .json(devices)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 | 207 => {
                #[derive(Deserialize)]
                struct Wrapper {
                    devices: Vec<Device>,
                }
                let w: Wrapper = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.devices)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn update_device(
        &self,
        device_key: &str,
        name: Option<String>,
        tags: Option<String>,
    ) -> Result<Device, ApiError> {
        let url = format!("{}/devices/{}", self.base_url, device_key);
        #[derive(Serialize)]
        struct Req {
            name: Option<String>,
            tags: Option<String>,
        }
        let resp = self
            .build_put(&url)
            .json(&Req { name, tags })
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct Wrapper {
                    device: Device,
                }
                let w: Wrapper = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.device)
            }
            404 => Err(ApiError::NotFound),
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    // ── OTA Campaigns ────────────────────────────────────────────────────────

    pub async fn list_campaigns(&self) -> Result<Vec<OtaCampaign>, ApiError> {
        let url = format!("{}/ota/campaigns", self.base_url);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct W {
                    campaigns: Vec<OtaCampaign>,
                }
                let w: W = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.campaigns)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    pub async fn get_campaign(&self, id: i64) -> Result<CampaignSummary, ApiError> {
        let url = format!("{}/ota/campaigns/{}", self.base_url, id);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct W {
                    campaign: CampaignSummary,
                }
                let w: W = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.campaign)
            }
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    pub async fn list_campaign_devices(&self, id: i64) -> Result<Vec<CampaignDevice>, ApiError> {
        let url = format!("{}/ota/campaigns/{}/devices", self.base_url, id);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct W {
                    devices: Vec<CampaignDevice>,
                }
                let w: W = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.devices)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    pub async fn create_campaign(
        &self,
        name: &str,
        firmware_id: i64,
        target_version: &str,
        strategy: &str,
        rollout_percent: i64,
    ) -> Result<OtaCampaign, ApiError> {
        let url = format!("{}/ota/campaigns", self.base_url);
        let body = serde_json::json!({
            "name": name,
            "firmware_id": firmware_id,
            "target_version": target_version,
            "strategy": strategy,
            "rollout_percent": rollout_percent,
        });
        let resp = self
            .build_post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            201 | 200 => {
                #[derive(Deserialize)]
                struct W {
                    campaign: OtaCampaign,
                }
                let w: W = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.campaign)
            }
            400 => Err(ApiError::Parse("invalid campaign request".into())),
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    pub async fn activate_campaign(&self, id: i64) -> Result<(), ApiError> {
        let url = format!("{}/ota/campaigns/{}/activate", self.base_url, id);
        let resp = self
            .build_post(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => Ok(()),
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    pub async fn pause_campaign(&self, id: i64) -> Result<(), ApiError> {
        let url = format!("{}/ota/campaigns/{}/pause", self.base_url, id);
        let resp = self
            .build_post(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => Ok(()),
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    pub async fn rollback_campaign(&self, id: i64) -> Result<(), ApiError> {
        let url = format!("{}/ota/campaigns/{}/rollback", self.base_url, id);
        let resp = self
            .build_post(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => Ok(()),
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    // ── Firmware Artifacts ───────────────────────────────────────────────────

    pub async fn list_firmware(&self) -> Result<Vec<FirmwareArtifact>, ApiError> {
        let url = format!("{}/ota/firmware", self.base_url);
        let resp = self
            .build_get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                #[derive(Deserialize)]
                struct W {
                    artifacts: Vec<FirmwareArtifact>,
                }
                let w: W = resp
                    .json()
                    .await
                    .map_err(|e| ApiError::Parse(e.to_string()))?;
                Ok(w.artifacts)
            }
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {code}"))),
        }
    }

    pub async fn delete_device(&self, device_key: &str) -> Result<(), ApiError> {
        let url = format!("{}/devices/{}", self.base_url, device_key);
        let resp = self
            .build_delete(&url)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => Ok(()),
            404 => Err(ApiError::NotFound),
            401 => Err(ApiError::Unauthorized),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }
}
