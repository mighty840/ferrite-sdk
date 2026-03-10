use super::types::*;
use crate::auth::{AuthModeInfo, AuthToken};

/// HTTP client for communicating with the ferrite backend API.
pub struct ApiClient {
    base_url: String,
    token: Option<AuthToken>,
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

    /// Discover the server's authentication mode.
    pub async fn get_auth_mode(&self) -> Result<AuthModeInfo, ApiError> {
        let url = format!("{}/auth/mode", self.base_url);
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
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
        let url = format!("{}/api/v1/devices", self.base_url);
        let client = reqwest::Client::new();
        let mut req = client.get(&url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => resp
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string())),
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn get_device(&self, device_id: &str) -> Result<Device, ApiError> {
        let url = format!("{}/api/v1/devices/{}", self.base_url, device_id);
        let client = reqwest::Client::new();
        let mut req = client.get(&url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => resp
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string())),
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn list_faults(&self) -> Result<Vec<FaultEvent>, ApiError> {
        let url = format!("{}/api/v1/faults", self.base_url);
        let client = reqwest::Client::new();
        let mut req = client.get(&url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => resp
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string())),
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn get_metrics(&self, device_id: &str) -> Result<Vec<MetricEntry>, ApiError> {
        let url = format!("{}/api/v1/devices/{}/metrics", self.base_url, device_id);
        let client = reqwest::Client::new();
        let mut req = client.get(&url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => resp
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string())),
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }

    pub async fn get_traces(&self, device_id: &str) -> Result<Vec<TraceEntry>, ApiError> {
        let url = format!("{}/api/v1/devices/{}/traces", self.base_url, device_id);
        let client = reqwest::Client::new();
        let mut req = client.get(&url);
        for (k, v) in self.auth_header() {
            req = req.header(&k, &v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => resp
                .json()
                .await
                .map_err(|e| ApiError::Parse(e.to_string())),
            401 => Err(ApiError::Unauthorized),
            404 => Err(ApiError::NotFound),
            code => Err(ApiError::Server(format!("HTTP {}", code))),
        }
    }
}
