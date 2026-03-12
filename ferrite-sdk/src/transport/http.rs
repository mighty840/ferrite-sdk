//! WiFi/HTTP transport using reqwless (no_std HTTP client).
//!
//! Sends chunks directly to the ferrite-server's `/ingest/chunks` endpoint
//! over an HTTP POST. Designed for WiFi-capable devices like ESP32-C3 or
//! Raspberry Pi Pico W.
//!
//! Requires the `http` feature flag.

use embedded_io_async::{Read, Write};
use reqwless::client::HttpClient;
use reqwless::request::Method;

/// HTTP transport that POSTs chunks directly to a ferrite-server.
///
/// Generic over the TCP connection type — the platform WiFi stack provides
/// the actual TCP implementation:
/// - ESP32-C3: `esp-wifi` + `embassy-net`
/// - Pico W: `cyw43` + `embassy-net`
pub struct HttpTransport<'a, T: Read + Write> {
    client: HttpClient<'a, T>,
    url: &'a str,
    api_key: Option<&'a str>,
}

impl<'a, T: Read + Write> HttpTransport<'a, T> {
    /// Create a new HTTP transport.
    ///
    /// - `connection`: A TCP connection to the server (platform-specific).
    /// - `url`: Full URL to the ingest endpoint, e.g. `"http://192.168.1.100:4000/ingest/chunks"`.
    /// - `api_key`: Optional API key sent as `X-API-Key` header.
    pub fn new(connection: T, url: &'a str, api_key: Option<&'a str>) -> Self {
        Self {
            client: HttpClient::new(connection),
            url,
            api_key,
        }
    }
}

impl<'a, T: Read + Write> crate::transport::AsyncChunkTransport for HttpTransport<'a, T> {
    type Error = reqwless::Error;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        let mut buf = [0u8; 512];
        let mut req = self.client.request(Method::POST, self.url).await?;
        req = req.content_type(reqwless::headers::ContentType::ApplicationOctetStream);
        if let Some(key) = self.api_key {
            req = req.header("X-API-Key", key);
        }
        let _resp = req.body(chunk).send(&mut buf).await?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        true // Assume WiFi is connected if we have a TCP handle
    }

    async fn begin_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn end_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
