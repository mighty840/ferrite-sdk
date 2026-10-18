//! OTA relay — lets devices behind the gateway reach the server's OTA reads.
//!
//! Devices on the local network (e.g. the STM32H563 on the boards-only
//! Ethernet) can't reach ferrite-server directly. The HTTP ingest listener
//! hands the three device-facing OTA requests to this relay, which fetches
//! them from the server with the gateway's credentials and streams the
//! response back:
//!
//! - `GET /ota/targets/<device_id>`
//! - `GET /ota/firmware/<id>/download`
//! - `GET /ota/firmware/<id>/manifest`
//!
//! The gateway is not trusted with firmware integrity: the
//! `X-Firmware-Manifest` header is passed through unchanged and the device
//! verifies its signature itself.

use reqwest::Client;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tracing::{debug, warn};

use crate::config::GatewayConfig;

/// Response headers copied from the server to the device.
const PASSTHROUGH_HEADERS: &[&str] = &["content-type", "x-firmware-manifest", "x-firmware-sha256"];

pub struct OtaRelay {
    client: Client,
    base_url: String,
    auth_header: Option<String>,
    api_key: Option<String>,
}

impl OtaRelay {
    pub fn new(config: &GatewayConfig) -> Self {
        Self {
            client: Client::new(),
            base_url: config.server_url.trim_end_matches('/').to_string(),
            auth_header: config.basic_auth_header(),
            api_key: config.api_key.clone(),
        }
    }

    /// Relay `GET path` to the server and write the response to `out`.
    /// `path` must already be validated with [`relay_path`].
    pub async fn relay<W: AsyncWrite + Unpin>(
        &self,
        path: &str,
        out: &mut W,
    ) -> anyhow::Result<()> {
        let mut req = self.client.get(format!("{}{}", self.base_url, path));
        if let Some(auth) = &self.auth_header {
            req = req.header("Authorization", auth);
        }
        if let Some(key) = &self.api_key {
            req = req.header("X-API-Key", key);
        }

        let mut resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("OTA relay: server unreachable for {}: {}", path, e);
                out.write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
                return Ok(());
            }
        };

        let status = resp.status();
        debug!("OTA relay: {} -> {}", path, status);
        let mut head = format!(
            "HTTP/1.1 {} {}\r\n",
            status.as_u16(),
            status.canonical_reason().unwrap_or("")
        );
        for name in PASSTHROUGH_HEADERS {
            if let Some(value) = resp.headers().get(*name).and_then(|v| v.to_str().ok()) {
                head.push_str(&format!("{name}: {value}\r\n"));
            }
        }
        if let Some(len) = resp.content_length() {
            head.push_str(&format!("content-length: {len}\r\n"));
        }
        head.push_str("connection: close\r\n\r\n");
        out.write_all(head.as_bytes()).await?;

        while let Some(chunk) = resp.chunk().await? {
            out.write_all(&chunk).await?;
        }
        out.flush().await?;
        Ok(())
    }
}

/// The request target, if it is one of the relayed OTA reads.
///
/// Accepts origin-form (`/ota/...`) and absolute-form (`http://host/ota/...`)
/// targets and returns the origin-form path.
pub fn relay_path(target: &str) -> Option<&str> {
    let path = match target.strip_prefix("http://") {
        Some(rest) => &rest[rest.find('/')?..],
        None => target,
    };
    let segments: Vec<&str> = path.strip_prefix('/')?.split('/').collect();
    let ok = match segments.as_slice() {
        ["ota", "targets", device_id] => is_device_id(device_id),
        ["ota", "firmware", id, "download" | "manifest"] => {
            !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
        }
        _ => false,
    };
    ok.then_some(path)
}

fn is_device_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && s != "."
        && s != ".."
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn relayed_paths() {
        assert_eq!(
            relay_path("/ota/targets/stm32h563-fleet-01"),
            Some("/ota/targets/stm32h563-fleet-01")
        );
        assert_eq!(
            relay_path("/ota/firmware/12/download"),
            Some("/ota/firmware/12/download")
        );
        assert_eq!(
            relay_path("/ota/firmware/12/manifest"),
            Some("/ota/firmware/12/manifest")
        );
        assert_eq!(
            relay_path("http://192.168.0.103:4001/ota/targets/dev-1"),
            Some("/ota/targets/dev-1")
        );

        for bad in [
            "/ota/targets",
            "/ota/targets/",
            "/ota/targets/..",
            "/ota/targets/a/b",
            "/ota/targets/a%2Fb",
            "/ota/targets/dev?x=1",
            "/ota/firmware",
            "/ota/firmware/abc/download",
            "/ota/firmware/1",
            "/ota/firmware/1/delete",
            "/ota/campaigns/1",
            "/devices",
            "ota/targets/dev",
            "http://host",
        ] {
            assert_eq!(relay_path(bad), None, "{bad}");
        }
    }

    /// Fake ferrite-server: records the request, sends a canned response.
    async fn fake_server(response: &'static [u8]) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap();
            sock.write_all(response).await.unwrap();
            sock.shutdown().await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (url, handle)
    }

    fn relay_to(url: &str) -> OtaRelay {
        OtaRelay::new(&GatewayConfig {
            server_url: url.to_string(),
            auth_user: Some("admin".into()),
            auth_pass: Some("secret".into()),
            api_key: Some("gw-key".into()),
            ..GatewayConfig::default()
        })
    }

    #[tokio::test]
    async fn relays_firmware_with_manifest_and_credentials() {
        let (url, server) = fake_server(
            b"HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\nx-firmware-manifest: abcdef\r\nx-internal: hidden\r\ncontent-length: 4\r\n\r\nBODY",
        )
        .await;
        let mut out = Vec::new();
        relay_to(&url)
            .relay("/ota/firmware/3/download", &mut out)
            .await
            .unwrap();

        let upstream = server.await.unwrap();
        assert!(upstream.starts_with("GET /ota/firmware/3/download HTTP/1.1"));
        assert!(upstream.contains("authorization: Basic YWRtaW46c2VjcmV0"));
        assert!(upstream.contains("x-api-key: gw-key"));

        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "{text}");
        assert!(text.contains("x-firmware-manifest: abcdef\r\n"));
        assert!(text.contains("content-length: 4\r\n"));
        assert!(!text.contains("x-internal"));
        assert!(text.ends_with("\r\n\r\nBODY"));
    }

    #[tokio::test]
    async fn passes_server_status_through() {
        let (url, server) =
            fake_server(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n").await;
        let mut out = Vec::new();
        relay_to(&url)
            .relay("/ota/targets/dev-1", &mut out)
            .await
            .unwrap();
        server.await.unwrap();
        assert!(String::from_utf8(out)
            .unwrap()
            .starts_with("HTTP/1.1 404 Not Found\r\n"));
    }

    #[tokio::test]
    async fn unreachable_server_is_502() {
        // Bind and drop to get a port nothing listens on.
        let port = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut out = Vec::new();
        relay_to(&format!("http://127.0.0.1:{port}"))
            .relay("/ota/targets/dev-1", &mut out)
            .await
            .unwrap();
        assert!(String::from_utf8(out).unwrap().starts_with("HTTP/1.1 502"));
    }
}
