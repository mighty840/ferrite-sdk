//! WiFi/HTTP transport using reqwless (no_std HTTP client).
//!
//! Sends chunks directly to the ferrite-server's `/ingest/chunks` endpoint
//! over an HTTP POST. Designed for WiFi-capable devices like ESP32-C3 or
//! STM32H563 (Ethernet).
//!
//! Requires the `http` feature flag.

use embedded_io_async::{Read, Write};
use reqwless::client::HttpConnection;
use reqwless::headers::ContentType;
use reqwless::request::{Method, Request, RequestBuilder};

/// HTTP transport that POSTs chunks directly to a ferrite-server.
///
/// Generic over the TCP connection type — the platform WiFi/Ethernet stack
/// provides the actual TCP implementation:
/// - ESP32-C3: `esp-wifi` + `embassy-net`
/// - STM32H563: `embassy-net` Ethernet
pub struct HttpTransport<'a, T: Read + Write> {
    connection: T,
    url: &'a str,
    api_key: Option<&'a str>,
}

impl<'a, T: Read + Write> HttpTransport<'a, T> {
    /// Create a new HTTP transport.
    ///
    /// - `connection`: A TCP socket from the platform's network stack.
    /// - `url`: Full URL to the ingest endpoint, e.g. `"http://192.168.1.100:4000/ingest/chunks"`.
    /// - `api_key`: Optional API key sent as `X-API-Key` header.
    pub fn new(connection: T, url: &'a str, api_key: Option<&'a str>) -> Self {
        Self {
            connection,
            url,
            api_key,
        }
    }
}

impl<'a, T: Read + Write> crate::transport::AsyncChunkTransport for HttpTransport<'a, T> {
    type Error = reqwless::Error;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        let mut conn = HttpConnection::Plain(&mut self.connection);
        let mut rx_buf = [0u8; 512];

        if let Some(key) = self.api_key {
            let headers = [("X-API-Key", key)];
            let request = Request::new(Method::POST, self.url)
                .content_type(ContentType::ApplicationOctetStream)
                .headers(&headers)
                .body(chunk)
                .build();
            conn.send(request, &mut rx_buf).await?;
        } else {
            let request = Request::new(Method::POST, self.url)
                .content_type(ContentType::ApplicationOctetStream)
                .body(chunk)
                .build();
            conn.send(request, &mut rx_buf).await?;
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn begin_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn end_session(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
