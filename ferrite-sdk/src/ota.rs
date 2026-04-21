//! OTA client for polling, downloading, and verifying firmware updates.
//!
//! Requires the `http` feature. The device polls `GET /ota/targets/:device_id`
//! to check for a pending update, then streams firmware from the server's
//! download endpoint directly into flash via a user-supplied write callback.
//!
//! SHA-256 integrity is verified against the `X-Firmware-SHA256` response
//! header. MCUboot slot marking is left to the caller — this module only
//! handles network I/O and integrity.
//!
//! # Example flow
//! ```ignore
//! let mut ota = OtaClient::new(tcp, "MY-DEVICE", build_id, "http://192.168.1.10:4000", None);
//! match ota.check().await {
//!     Ok(OtaCheckResult::UpdateAvailable(target)) => {
//!         ota.download(&target.firmware_url, |offset, data| flash.write(offset, data)).await?;
//!         mcuboot_mark_slot_ready(); // caller's responsibility
//!         reboot();
//!     }
//!     Ok(OtaCheckResult::UpToDate) => {}
//!     Err(e) => { /* handle */ }
//! }
//! ```

use embedded_io_async::{Read, Write};
use reqwless::client::HttpConnection;
use reqwless::request::{Method, Request, RequestBuilder};
use sha2::{Digest, Sha256};

/// Result of a successful OTA check.
pub enum OtaCheckResult {
    /// Device firmware matches the server's target — no action needed.
    UpToDate,
    /// The server has a newer firmware target for this device.
    UpdateAvailable(OtaTarget),
}

/// Firmware target returned by the server for this device.
pub struct OtaTarget {
    pub version: heapless::String<32>,
    pub build_id: u64,
    /// Relative URL path, e.g. `"/ota/firmware/3/download"`.
    pub firmware_url: heapless::String<128>,
}

/// Errors produced by the OTA client.
#[derive(Debug)]
pub enum OtaError<E = core::convert::Infallible> {
    /// An HTTP-level or transport error.
    Http(reqwless::Error),
    /// The server returned an unexpected non-200 / non-404 status.
    ServerError(u16),
    /// Response JSON could not be parsed to extract the required fields.
    ParseError,
    /// Downloaded firmware SHA-256 does not match the server's value.
    HashMismatch,
    /// Flash write callback returned an error.
    FlashError(E),
}

/// OTA client using `reqwless` HTTP (WiFi / Ethernet devices).
///
/// Generic over the TCP connection type; the platform's network stack
/// supplies a `T: embedded_io_async::Read + Write` handle.
pub struct OtaClient<'a, T: Read + Write> {
    connection: T,
    device_id: &'a str,
    current_build_id: u64,
    base_url: &'a str,
    api_key: Option<&'a str>,
}

impl<'a, T: Read + Write> OtaClient<'a, T> {
    /// Create a new OTA client.
    ///
    /// - `connection`: Platform TCP socket (from esp-wifi / embassy-net).
    /// - `device_id`: Exact device ID registered with the server.
    /// - `current_build_id`: The `build_id` this firmware was compiled with.
    /// - `base_url`: Server root, e.g. `"http://192.168.1.10:4000"`.
    /// - `api_key`: Optional `X-API-Key` header value.
    pub fn new(
        connection: T,
        device_id: &'a str,
        current_build_id: u64,
        base_url: &'a str,
        api_key: Option<&'a str>,
    ) -> Self {
        Self {
            connection,
            device_id,
            current_build_id,
            base_url,
            api_key,
        }
    }

    /// Poll the server for a pending firmware target for this device.
    ///
    /// Returns `UpToDate` when no target is set (404) or when the target's
    /// `build_id` matches `current_build_id`.
    pub async fn check(&mut self) -> Result<OtaCheckResult, OtaError> {
        let mut url: heapless::String<256> = heapless::String::new();
        url.push_str(self.base_url)
            .and_then(|_| url.push_str("/ota/targets/"))
            .and_then(|_| url.push_str(self.device_id))
            .map_err(|_| OtaError::ParseError)?;

        let mut conn = HttpConnection::Plain(&mut self.connection);
        let mut rx_buf = [0u8; 1024];

        let resp = if let Some(key) = self.api_key {
            let headers = [("X-API-Key", key)];
            let req = Request::new(Method::GET, url.as_str())
                .headers(&headers)
                .build();
            conn.send(req, &mut rx_buf).await.map_err(OtaError::Http)?
        } else {
            let req = Request::new(Method::GET, url.as_str()).build();
            conn.send(req, &mut rx_buf).await.map_err(OtaError::Http)?
        };

        match resp.status.0 {
            404 => return Ok(OtaCheckResult::UpToDate),
            200 => {}
            code => return Err(OtaError::ServerError(code)),
        }

        // Read body into a buffer for parsing
        let mut body_buf = [0u8; 512];
        let n = {
            let mut body = resp.body().reader();
            let mut total = 0usize;
            while total < body_buf.len() {
                match body.read(&mut body_buf[total..]).await {
                    Ok(0) => break,
                    Ok(n) => total += n,
                    Err(_) => break,
                }
            }
            total
        };
        let json = &body_buf[..n];

        let target_build_id =
            parse_u64_field(json, b"\"target_build_id\":").ok_or(OtaError::ParseError)?;

        if target_build_id == self.current_build_id {
            return Ok(OtaCheckResult::UpToDate);
        }

        let mut version: heapless::String<32> = heapless::String::new();
        if let Some(v) = parse_str_field(json, b"\"target_version\":\"") {
            for &b in v {
                version.push(b as char).ok();
            }
        }

        let mut firmware_url: heapless::String<128> = heapless::String::new();
        let url_bytes =
            parse_str_field(json, b"\"firmware_url\":\"").ok_or(OtaError::ParseError)?;
        for &b in url_bytes {
            firmware_url.push(b as char).ok();
        }

        Ok(OtaCheckResult::UpdateAvailable(OtaTarget {
            version,
            build_id: target_build_id,
            firmware_url,
        }))
    }

    /// Download firmware from the server and write it to flash in chunks.
    ///
    /// `firmware_url` is the relative path from `OtaTarget::firmware_url`
    /// (e.g. `"/ota/firmware/3/download"`).
    ///
    /// `flash_write(offset, data)` is called for each chunk received. The
    /// callback must write the data to the secondary MCUboot slot.
    ///
    /// SHA-256 is verified against the `X-Firmware-SHA256` response header
    /// when present. Returns `HashMismatch` if verification fails.
    pub async fn download<F, E>(
        &mut self,
        firmware_url: &str,
        mut flash_write: F,
    ) -> Result<(), OtaError<E>>
    where
        F: FnMut(u32, &[u8]) -> Result<(), E>,
    {
        let mut url: heapless::String<256> = heapless::String::new();
        url.push_str(self.base_url)
            .and_then(|_| url.push_str(firmware_url))
            .map_err(|_| OtaError::ParseError)?;

        let mut conn = HttpConnection::Plain(&mut self.connection);
        let mut rx_buf = [0u8; 1024];

        let resp = if let Some(key) = self.api_key {
            let headers = [("X-API-Key", key)];
            let req = Request::new(Method::GET, url.as_str())
                .headers(&headers)
                .build();
            conn.send(req, &mut rx_buf).await.map_err(OtaError::Http)?
        } else {
            let req = Request::new(Method::GET, url.as_str()).build();
            conn.send(req, &mut rx_buf).await.map_err(OtaError::Http)?
        };

        if resp.status.0 != 200 {
            return Err(OtaError::ServerError(resp.status.0));
        }

        // Capture X-Firmware-SHA256 header value before body() consumes the response.
        let mut sha256_buf = [0u8; 64];
        let mut sha256_len = 0usize;
        for (name, value) in resp.headers() {
            if name.eq_ignore_ascii_case("x-firmware-sha256") {
                sha256_len = value.len().min(64);
                sha256_buf[..sha256_len].copy_from_slice(&value[..sha256_len]);
                break;
            }
        }

        let mut hasher = Sha256::new();
        let mut chunk = [0u8; 512];
        let mut offset = 0u32;
        let mut body = resp.body().reader();

        loop {
            let n = body.read(&mut chunk).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            hasher.update(&chunk[..n]);
            flash_write(offset, &chunk[..n]).map_err(OtaError::FlashError)?;
            offset += n as u32;
        }

        // Verify SHA-256 if the server sent the header.
        if sha256_len == 64 {
            let digest = hasher.finalize();
            if !sha256_hex_matches(digest.as_ref(), &sha256_buf) {
                return Err(OtaError::HashMismatch);
            }
        }

        Ok(())
    }
}

// ── JSON helpers ─────────────────────────────────────────────────────────────

/// Find `"key":VALUE` in JSON bytes and parse VALUE as a u64.
fn parse_u64_field(json: &[u8], key: &[u8]) -> Option<u64> {
    let pos = json.windows(key.len()).position(|w| w == key)?;
    let rest = &json[pos + key.len()..];
    let start = rest.iter().position(|b| b.is_ascii_digit())?;
    let digits = &rest[start..];
    let end = digits
        .iter()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(digits.len());
    core::str::from_utf8(&digits[..end]).ok()?.parse().ok()
}

/// Find `"key":"VALUE"` in JSON bytes and return the VALUE slice.
/// The `key` argument must include the trailing `:"` (opening quote).
fn parse_str_field<'a>(json: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let pos = json.windows(key.len()).position(|w| w == key)?;
    let after = &json[pos + key.len()..];
    let end = after.iter().position(|&b| b == b'"')?;
    Some(&after[..end])
}

// ── SHA-256 comparison ────────────────────────────────────────────────────────

const HEX_LOWER: [u8; 16] = *b"0123456789abcdef";

/// Compare a raw SHA-256 digest against a lowercase hex string in a byte slice.
fn sha256_hex_matches(digest: &[u8], hex: &[u8; 64]) -> bool {
    if digest.len() != 32 {
        return false;
    }
    for (i, &byte) in digest.iter().enumerate() {
        if hex[i * 2] != HEX_LOWER[(byte >> 4) as usize]
            || hex[i * 2 + 1] != HEX_LOWER[(byte & 0xF) as usize]
        {
            return false;
        }
    }
    true
}
