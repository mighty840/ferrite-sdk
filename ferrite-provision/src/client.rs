use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
struct RegisterRequest {
    device_key: String,
    name: Option<String>,
    tags: Option<String>,
    provisioned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    pub device: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkRegisterResponse {
    pub registered: Option<usize>,
    pub devices: Option<Vec<serde_json::Value>>,
    pub errors: Option<Vec<String>>,
}

pub struct ServerClient {
    base_url: String,
    auth_header: Option<String>,
}

impl ServerClient {
    pub fn new(base_url: &str, user: Option<&str>, password: Option<&str>) -> Self {
        let auth_header = user.map(|u| {
            let pass = password.unwrap_or("");
            let encoded = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                format!("{u}:{pass}"),
            );
            format!("Basic {encoded}")
        });
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_header,
        }
    }

    pub fn register_device(
        &self,
        device_key: u32,
        name: Option<&str>,
        tags: Option<&str>,
        provisioned_by: Option<&str>,
    ) -> Result<RegisterResponse> {
        let url = format!("{}/devices/register", self.base_url);
        let client = reqwest::blocking::Client::new();
        let mut req = client.post(&url).json(&RegisterRequest {
            device_key: format!("{:08X}", device_key),
            name: name.map(String::from),
            tags: tags.map(String::from),
            provisioned_by: provisioned_by.map(String::from),
        });
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().context("failed to contact server")?;
        let body: RegisterResponse = resp.json().context("failed to parse response")?;
        Ok(body)
    }

    pub fn register_devices_bulk(
        &self,
        devices: &[BulkEntry],
    ) -> Result<BulkRegisterResponse> {
        let url = format!("{}/devices/register/bulk", self.base_url);
        let reqs: Vec<RegisterRequest> = devices
            .iter()
            .map(|d| RegisterRequest {
                device_key: d.device_key.clone(),
                name: d.name.clone(),
                tags: d.tags.clone(),
                provisioned_by: d.provisioned_by.clone(),
            })
            .collect();
        let client = reqwest::blocking::Client::new();
        let mut req = client.post(&url).json(&reqs);
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth);
        }
        let resp = req.send().context("failed to contact server")?;
        let body: BulkRegisterResponse = resp.json().context("failed to parse response")?;
        Ok(body)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BulkEntry {
    pub device_key: String,
    pub name: Option<String>,
    pub tags: Option<String>,
    pub provisioned_by: Option<String>,
}

/// Compute an owner prefix from a username using SHA-256.
pub fn compute_owner_prefix(username: &str) -> u8 {
    let mut hasher = Sha256::new();
    hasher.update(username.as_bytes());
    let hash = hasher.finalize();
    hash[0]
}
