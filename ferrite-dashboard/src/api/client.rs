use super::types::*;
use crate::auth::{AuthModeInfo, AuthToken};
use serde::{Deserialize, Serialize};

/// HTTP client for communicating with the ferrite backend API.
pub struct ApiClient {
    base_url: String,
    token: Option<AuthToken>,
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
