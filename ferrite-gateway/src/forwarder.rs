//! HTTP forwarder — POSTs chunks to ferrite-server's /ingest/chunks endpoint.

use anyhow::Result;
use reqwest::Client;
use tracing::{debug, warn};

use crate::config::GatewayConfig;

/// Forwards chunks to the ferrite-server over HTTP.
pub struct Forwarder {
    client: Client,
    ingest_url: String,
    auth_header: Option<String>,
    api_key: Option<String>,
    max_retries: u32,
}

impl Forwarder {
    /// Create a new forwarder from gateway config.
    pub fn new(config: &GatewayConfig) -> Self {
        let ingest_url = format!("{}/ingest/chunks", config.server_url.trim_end_matches('/'));

        let auth_header = match (&config.auth_user, &config.auth_pass) {
            (Some(user), Some(pass)) => {
                use base64::Engine;
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
                Some(format!("Basic {encoded}"))
            }
            _ => None,
        };

        Self {
            client: Client::new(),
            ingest_url,
            auth_header,
            api_key: config.api_key.clone(),
            max_retries: config.max_retries,
        }
    }

    /// Forward a single raw chunk to the server.
    /// Returns Ok(true) if accepted, Ok(false) if server rejected (4xx), Err on network failure.
    pub async fn forward_chunk(&self, chunk: &[u8]) -> Result<bool> {
        for attempt in 0..self.max_retries {
            let mut req = self
                .client
                .post(&self.ingest_url)
                .header("Content-Type", "application/octet-stream")
                .body(chunk.to_vec());

            if let Some(auth) = &self.auth_header {
                req = req.header("Authorization", auth);
            }
            if let Some(key) = &self.api_key {
                req = req.header("X-API-Key", key);
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        debug!("Forwarded chunk ({} bytes), status {}", chunk.len(), status);
                        return Ok(true);
                    } else if status.is_client_error() {
                        warn!("Server rejected chunk: {}", status);
                        return Ok(false);
                    } else {
                        warn!(
                            "Server error {} on attempt {}/{}",
                            status,
                            attempt + 1,
                            self.max_retries
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Network error on attempt {}/{}: {}",
                        attempt + 1,
                        self.max_retries,
                        e
                    );
                }
            }

            if attempt + 1 < self.max_retries {
                let delay = std::time::Duration::from_secs(1 << attempt);
                tokio::time::sleep(delay).await;
            }
        }

        anyhow::bail!("Failed to forward chunk after {} attempts", self.max_retries)
    }

    /// Forward a batch of raw chunks concatenated in a single HTTP body.
    #[allow(dead_code)]
    pub async fn forward_batch(&self, chunks: &[Vec<u8>]) -> Result<bool> {
        let mut body = Vec::new();
        for chunk in chunks {
            body.extend_from_slice(chunk);
        }
        self.forward_chunk(&body).await
    }

    /// Check if the server is reachable.
    pub async fn health_check(&self) -> bool {
        let url = self
            .ingest_url
            .replace("/ingest/chunks", "/health");
        matches!(
            self.client.get(&url).send().await,
            Ok(resp) if resp.status().is_success()
        )
    }
}
