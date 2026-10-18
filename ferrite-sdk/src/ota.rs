//! OTA client for polling, downloading, and verifying firmware updates.
//!
//! Requires the `http` feature. The device polls `GET /ota/targets/:device_id`
//! to check for a pending update, then streams firmware from the server's
//! download endpoint directly into flash via a user-supplied write callback.
//!
//! Every image must come with a signed manifest (see [`crate::ota_manifest`])
//! in the `X-Firmware-Manifest` response header. The signature and the
//! anti-rollback floor are checked before the first byte is written to
//! flash; size and SHA-256 are checked while streaming. There is no unsigned
//! fallback: a download without a valid manifest fails.
//!
//! MCUboot / embassy-boot slot marking is left to the caller — this module
//! only handles network I/O and verification.
//!
//! # Example flow
//! ```ignore
//! const OTA_KEYS: [[u8; 32]; 1] = [/* from `ferrite ota keygen` */];
//! const SECURITY_VERSION: u32 = 3;
//!
//! let policy = UpdatePolicy { trusted_keys: &OTA_KEYS, min_security_version: SECURITY_VERSION };
//! let ota = OtaClient::new("MY-DEVICE", build_id, "http://192.168.1.10:4000", None);
//! match ota.check(&mut new_tcp_connection().await?).await {
//!     Ok(OtaCheckResult::UpdateAvailable(target)) => {
//!         let mut tcp = new_tcp_connection().await?; // one connection per request
//!         let manifest = ota.download(&mut tcp, &target, &policy, |offset, data| flash.write(offset, data)).await?;
//!         mcuboot_mark_slot_ready(); // only after download() returned Ok
//!         reboot();
//!     }
//!     Ok(OtaCheckResult::UpToDate) => {}
//!     Err(e) => { /* handle */ }
//! }
//! ```

use embedded_io_async::{Read, Write};
use reqwless::client::HttpConnection;
use reqwless::request::{Method, Request, RequestBuilder};

use crate::ota_manifest::{
    ImageVerifier, Manifest, ManifestError, SignedManifest, UpdatePolicy, MANIFEST_HEX_LEN,
};

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
    /// The download response carried no `X-Firmware-Manifest` header.
    MissingManifest,
    /// The manifest or the image failed verification.
    Manifest(ManifestError),
    /// The verified manifest is for a different build than the target.
    TargetMismatch,
    /// The response body could not be read to the end.
    ReadError,
    /// Flash write callback returned an error.
    FlashError(E),
}

/// OTA client using `reqwless` HTTP (WiFi / Ethernet devices).
///
/// Each call takes its own connection, supplied by the platform's network
/// stack (`T: embedded_io_async::Read + Write`, e.g. an embassy-net
/// `TcpSocket`). Responses are not drained, so a connection must not be
/// reused for a second request; open a fresh one per call.
pub struct OtaClient<'a> {
    device_id: &'a str,
    current_build_id: u64,
    host: &'a str,
    api_key: Option<&'a str>,
}

impl<'a> OtaClient<'a> {
    /// Create a new OTA client.
    ///
    /// - `device_id`: Exact device ID registered with the server.
    /// - `current_build_id`: The `build_id` this firmware was compiled with.
    /// - `base_url`: Server (or gateway) root, e.g. `"http://192.168.0.103:4001"`.
    ///   Only the host part is used, for the `Host` header; the caller's
    ///   connection decides where requests actually go.
    /// - `api_key`: Optional `X-API-Key` header value.
    pub fn new(
        device_id: &'a str,
        current_build_id: u64,
        base_url: &'a str,
        api_key: Option<&'a str>,
    ) -> Self {
        Self {
            device_id,
            current_build_id,
            host: host_of(base_url),
            api_key,
        }
    }

    /// Poll the server for a pending firmware target for this device.
    ///
    /// Returns `UpToDate` when no target is set (404) or when the target's
    /// `build_id` matches `current_build_id`.
    pub async fn check<T: Read + Write>(
        &self,
        connection: &mut T,
    ) -> Result<OtaCheckResult, OtaError> {
        let mut path: heapless::String<160> = heapless::String::new();
        path.push_str("/ota/targets/")
            .and_then(|_| path.push_str(self.device_id))
            .map_err(|_| OtaError::ParseError)?;

        let mut conn = HttpConnection::Plain(connection);
        let mut rx_buf = [0u8; 1024];
        let resp = self.get(&mut conn, &path, &mut rx_buf).await?;

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
                    Err(_) => return Err(OtaError::ReadError),
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

    /// Download a target's firmware, verify it, and write it to flash.
    ///
    /// The manifest from the `X-Firmware-Manifest` header must be signed by
    /// one of `policy.trusted_keys`, meet `policy.min_security_version`, and
    /// name the target's `build_id`; otherwise nothing is written.
    ///
    /// `flash_write(offset, data)` is called for each chunk received and must
    /// write to the secondary (update) slot. Size is checked before each
    /// write; the SHA-256 is only known once the whole image has arrived, so
    /// the slot must not be marked bootable unless this returns `Ok`.
    ///
    /// Returns the verified manifest.
    pub async fn download<T, F, E>(
        &self,
        connection: &mut T,
        target: &OtaTarget,
        policy: &UpdatePolicy<'_>,
        mut flash_write: F,
    ) -> Result<Manifest, OtaError<E>>
    where
        T: Read + Write,
        F: FnMut(u32, &[u8]) -> Result<(), E>,
    {
        let mut conn = HttpConnection::Plain(connection);
        // Larger than check()'s buffer: response headers include the
        // 336-character manifest.
        let mut rx_buf = [0u8; 2048];
        let resp = self
            .get(&mut conn, &target.firmware_url, &mut rx_buf)
            .await
            .map_err(OtaError::widen)?;

        if resp.status.0 != 200 {
            return Err(OtaError::ServerError(resp.status.0));
        }

        // Verify the manifest before body() consumes the response, and
        // before anything touches flash.
        let signed = resp
            .headers()
            .find(|(name, _)| name.eq_ignore_ascii_case("x-firmware-manifest"))
            .map(|(_, value)| value)
            .ok_or(OtaError::MissingManifest)
            .and_then(|hex| {
                if hex.len() != MANIFEST_HEX_LEN {
                    return Err(OtaError::Manifest(ManifestError::Length));
                }
                SignedManifest::from_hex(hex).map_err(OtaError::Manifest)
            })?;
        let manifest = policy.check(&signed).map_err(OtaError::Manifest)?;
        if manifest.build_id != target.build_id {
            return Err(OtaError::TargetMismatch);
        }

        let mut verifier = ImageVerifier::new(manifest);
        let mut chunk = [0u8; 512];
        let mut offset = 0u32;
        let mut body = resp.body().reader();

        loop {
            let n = body
                .read(&mut chunk)
                .await
                .map_err(|_| OtaError::ReadError)?;
            if n == 0 {
                break;
            }
            verifier.update(&chunk[..n]).map_err(OtaError::Manifest)?;
            flash_write(offset, &chunk[..n]).map_err(OtaError::FlashError)?;
            offset += n as u32;
        }

        verifier.finish().map_err(OtaError::Manifest)?;
        Ok(manifest.clone())
    }

    /// Send an origin-form `GET path` with `Host` (and `X-API-Key`).
    async fn get<'x, 'b, T: Read + Write>(
        &self,
        conn: &'x mut HttpConnection<'x, T>,
        path: &str,
        rx_buf: &'b mut [u8],
    ) -> Result<reqwless::response::Response<'x, 'b, HttpConnection<'x, T>>, OtaError> {
        let api_key_header;
        let headers: &[(&str, &str)] = match self.api_key {
            Some(key) => {
                api_key_header = [("X-API-Key", key)];
                &api_key_header
            }
            None => &[],
        };
        let req = Request::new(Method::GET, path)
            .host(self.host)
            .headers(headers)
            .build();
        conn.send(req, rx_buf).await.map_err(OtaError::Http)
    }
}

impl OtaError {
    /// Carry a request-phase error into a download error type.
    fn widen<E>(self) -> OtaError<E> {
        match self {
            OtaError::Http(e) => OtaError::Http(e),
            OtaError::ServerError(c) => OtaError::ServerError(c),
            OtaError::ParseError => OtaError::ParseError,
            OtaError::MissingManifest => OtaError::MissingManifest,
            OtaError::Manifest(e) => OtaError::Manifest(e),
            OtaError::TargetMismatch => OtaError::TargetMismatch,
            OtaError::ReadError => OtaError::ReadError,
            OtaError::FlashError(never) => match never {},
        }
    }
}

/// Host part of a base URL: `http://10.0.0.2:4001/x` → `10.0.0.2:4001`.
fn host_of(base_url: &str) -> &str {
    let rest = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))
        .unwrap_or(base_url);
    rest.split('/').next().unwrap_or(rest)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ota_manifest::{key_id, signing_message};
    use core::convert::Infallible;
    use core::future::Future;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};
    use ed25519_compact::{KeyPair, Seed};
    use sha2::{Digest, Sha256};

    const IMAGE: [u8; 100] = [0xA5; 100];
    const BUILD_ID: u64 = 42;

    /// Connection that records the request and replays a canned response.
    struct MockConn<'a> {
        response: &'a [u8],
        pos: usize,
        request: heapless::Vec<u8, 512>,
    }

    impl embedded_io_async::ErrorType for MockConn<'_> {
        type Error = Infallible;
    }

    impl Read for MockConn<'_> {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Infallible> {
            let n = buf.len().min(self.response.len() - self.pos);
            buf[..n].copy_from_slice(&self.response[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    impl Write for MockConn<'_> {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Infallible> {
            let _ = self.request.extend_from_slice(buf);
            Ok(buf.len())
        }
        async fn flush(&mut self) -> Result<(), Infallible> {
            Ok(())
        }
    }

    /// The mock never pends, so polling until ready is enough.
    fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    fn keypair(seed: u8) -> KeyPair {
        KeyPair::from_seed(Seed::new([seed; 32]))
    }

    fn signed_manifest(kp: &KeyPair, build_id: u64, image: &[u8]) -> SignedManifest {
        let mut version = heapless::String::new();
        version.push_str("2.0.0").unwrap();
        let manifest = Manifest {
            build_id,
            security_version: 1,
            image_size: image.len() as u32,
            sha256: Sha256::digest(image).into(),
            version,
            key_id: key_id(&kp.pk),
        };
        let body = manifest.encode_body();
        let sig = kp.sk.sign(signing_message(&body), None);
        SignedManifest::new(body, *sig).unwrap()
    }

    fn response(manifest_hex: Option<&[u8]>, body: &[u8]) -> heapless::Vec<u8, 1024> {
        let mut r = heapless::Vec::new();
        r.extend_from_slice(b"HTTP/1.1 200 OK\r\nContent-Length: ")
            .unwrap();
        r.extend_from_slice(&itoa_buf(body.len())).unwrap();
        r.extend_from_slice(b"\r\n").unwrap();
        if let Some(hex) = manifest_hex {
            r.extend_from_slice(b"X-Firmware-Manifest: ").unwrap();
            r.extend_from_slice(hex).unwrap();
            r.extend_from_slice(b"\r\n").unwrap();
        }
        r.extend_from_slice(b"\r\n").unwrap();
        r.extend_from_slice(body).unwrap();
        r
    }

    fn itoa_buf(n: usize) -> heapless::Vec<u8, 8> {
        let mut digits = heapless::Vec::<u8, 8>::new();
        let mut n = n;
        loop {
            digits.push(b'0' + (n % 10) as u8).unwrap();
            n /= 10;
            if n == 0 {
                break;
            }
        }
        digits.reverse();
        digits
    }

    fn target(build_id: u64) -> OtaTarget {
        let mut firmware_url = heapless::String::new();
        firmware_url.push_str("/ota/firmware/1/download").unwrap();
        OtaTarget {
            version: heapless::String::new(),
            build_id,
            firmware_url,
        }
    }

    /// Run download() against `resp`, returning the result and what was flashed.
    fn run(
        resp: &[u8],
        keys: &[[u8; 32]],
        target_build_id: u64,
    ) -> (Result<Manifest, OtaError>, heapless::Vec<u8, 256>) {
        let policy = UpdatePolicy {
            trusted_keys: keys,
            min_security_version: 1,
        };
        let mut conn = mock(resp);
        let client = OtaClient::new("dev-1", 0, "http://ferrite.test", None);
        let mut flashed = heapless::Vec::<u8, 256>::new();
        let result = block_on(client.download(
            &mut conn,
            &target(target_build_id),
            &policy,
            |_, data| flashed.extend_from_slice(data).map_err(|_| unreachable!()),
        ));
        (result, flashed)
    }

    fn mock(response: &[u8]) -> MockConn<'_> {
        MockConn {
            response,
            pos: 0,
            request: heapless::Vec::new(),
        }
    }

    #[test]
    fn requests_are_origin_form_with_host_and_key() {
        let body = br#"{"target":{"target_build_id":9,"target_version":"2.0.0","firmware_url":"/ota/firmware/4/download"}}"#;
        let resp = response(None, body);
        let mut conn = mock(&resp);
        let client = OtaClient::new("dev-1", 1, "http://192.168.0.103:4001/", Some("k3y"));
        let result = block_on(client.check(&mut conn)).unwrap();

        let req = core::str::from_utf8(&conn.request).unwrap();
        assert!(
            req.starts_with("GET /ota/targets/dev-1 HTTP/1.1\r\n"),
            "{req}"
        );
        assert!(req.contains("Host: 192.168.0.103:4001\r\n"), "{req}");
        assert!(req.contains("X-API-Key: k3y\r\n"), "{req}");
        match result {
            OtaCheckResult::UpdateAvailable(t) => {
                assert_eq!(t.build_id, 9);
                assert_eq!(t.firmware_url.as_str(), "/ota/firmware/4/download");
            }
            OtaCheckResult::UpToDate => panic!("expected update"),
        }
    }

    #[test]
    fn check_reports_up_to_date_for_same_build_and_404() {
        let body = br#"{"target":{"target_build_id":5,"firmware_url":"/x"}}"#;
        let resp = response(None, body);
        let client = OtaClient::new("dev-1", 5, "http://h", None);
        assert!(matches!(
            block_on(client.check(&mut mock(&resp))),
            Ok(OtaCheckResult::UpToDate)
        ));

        let not_found = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        assert!(matches!(
            block_on(client.check(&mut mock(not_found))),
            Ok(OtaCheckResult::UpToDate)
        ));
    }

    #[test]
    fn host_parsing() {
        assert_eq!(host_of("http://10.0.0.2:4001"), "10.0.0.2:4001");
        assert_eq!(host_of("http://example.com/base/"), "example.com");
        assert_eq!(host_of("gateway:4001"), "gateway:4001");
    }

    #[test]
    fn valid_manifest_and_image_are_flashed() {
        let kp = keypair(7);
        let hex = signed_manifest(&kp, BUILD_ID, &IMAGE).to_hex();
        let (result, flashed) = run(&response(Some(&hex), &IMAGE), &[*kp.pk], BUILD_ID);
        let manifest = result.unwrap();
        assert_eq!(manifest.version.as_str(), "2.0.0");
        assert_eq!(flashed.as_slice(), &IMAGE[..]);
    }

    #[test]
    fn missing_manifest_writes_nothing() {
        let kp = keypair(7);
        let (result, flashed) = run(&response(None, &IMAGE), &[*kp.pk], BUILD_ID);
        assert!(matches!(result, Err(OtaError::MissingManifest)));
        assert!(flashed.is_empty());
    }

    #[test]
    fn untrusted_signer_writes_nothing() {
        let attacker = keypair(9);
        let trusted = keypair(7);
        let hex = signed_manifest(&attacker, BUILD_ID, &IMAGE).to_hex();
        let (result, flashed) = run(&response(Some(&hex), &IMAGE), &[*trusted.pk], BUILD_ID);
        assert!(matches!(
            result,
            Err(OtaError::Manifest(ManifestError::UnknownKey))
        ));
        assert!(flashed.is_empty());
    }

    #[test]
    fn manifest_for_other_build_writes_nothing() {
        let kp = keypair(7);
        let hex = signed_manifest(&kp, BUILD_ID + 1, &IMAGE).to_hex();
        let (result, flashed) = run(&response(Some(&hex), &IMAGE), &[*kp.pk], BUILD_ID);
        assert!(matches!(result, Err(OtaError::TargetMismatch)));
        assert!(flashed.is_empty());
    }

    #[test]
    fn swapped_image_fails_hash() {
        let kp = keypair(7);
        let hex = signed_manifest(&kp, BUILD_ID, &IMAGE).to_hex();
        let evil = [0x5A; 100];
        let (result, _) = run(&response(Some(&hex), &evil), &[*kp.pk], BUILD_ID);
        assert!(matches!(
            result,
            Err(OtaError::Manifest(ManifestError::HashMismatch))
        ));
    }

    #[test]
    fn oversized_image_is_cut_off() {
        let kp = keypair(7);
        let hex = signed_manifest(&kp, BUILD_ID, &IMAGE[..50]).to_hex();
        let (result, flashed) = run(&response(Some(&hex), &IMAGE), &[*kp.pk], BUILD_ID);
        assert!(matches!(
            result,
            Err(OtaError::Manifest(ManifestError::SizeMismatch))
        ));
        assert!(flashed.len() <= 50);
    }
}
