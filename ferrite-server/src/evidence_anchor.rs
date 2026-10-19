//! Anchor the evidence log's head hash outside the server.
//!
//! The hash chain shows the log is internally consistent; an anchor shows it
//! wasn't replaced wholesale. On a schedule (`EVIDENCE_ANCHOR_INTERVAL_SECS`)
//! or on demand (`POST /evidence/anchor`), the current head is sent to
//!
//! - a webhook (`EVIDENCE_ANCHOR_WEBHOOK_URL`, optional bearer token), e.g.
//!   the BreakPilot platform, and/or
//! - an RFC 3161 timestamp authority (`EVIDENCE_ANCHOR_TSA_URL`), whose signed
//!   token proves the head existed at that time and can be checked offline
//!   with `openssl ts -verify`.
//!
//! Each successful anchor is itself appended to the log as `evidence.anchored`
//! (with the TSA token), so the next anchor covers it. Nothing is sent while
//! the newest entry is already an anchor.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;

use crate::AppState;

pub const ANCHOR_KIND: &str = "evidence.anchored";

/// Configuration from the environment; anchoring is off unless a sink is set.
#[derive(Debug, Clone, Default)]
pub struct EvidenceAnchorConfig {
    /// Seconds between scheduled anchors; 0 = only on demand.
    pub interval_secs: u64,
    pub webhook_url: Option<String>,
    pub webhook_token: Option<String>,
    pub tsa_url: Option<String>,
}

impl EvidenceAnchorConfig {
    pub fn is_configured(&self) -> bool {
        self.webhook_url.is_some() || self.tsa_url.is_some()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum AnchorOutcome {
    Anchored {
        head_seq: i64,
        head_hash: String,
        /// Sequence number of the `evidence.anchored` entry.
        anchor_seq: i64,
    },
    /// The log is empty or its newest entry is already an anchor.
    Unchanged,
}

#[derive(Debug)]
pub enum AnchorError {
    NotConfigured,
    ChainBroken(i64),
    AllSinksFailed(Vec<String>),
    Db(String),
}

impl std::fmt::Display for AnchorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(
                f,
                "no anchor configured (EVIDENCE_ANCHOR_WEBHOOK_URL or EVIDENCE_ANCHOR_TSA_URL)"
            ),
            Self::ChainBroken(seq) => {
                write!(f, "evidence chain is broken at seq {seq}; not anchoring")
            }
            Self::AllSinksFailed(errors) => write!(f, "anchoring failed: {}", errors.join("; ")),
            Self::Db(e) => write!(f, "evidence log error: {e}"),
        }
    }
}

impl std::error::Error for AnchorError {}

/// Anchor the current head now.
pub async fn anchor_once(state: &AppState) -> Result<AnchorOutcome, AnchorError> {
    let cfg = &state.config.evidence_anchor;
    if !cfg.is_configured() {
        return Err(AnchorError::NotConfigured);
    }
    let head = state
        .evidence
        .head()
        .map_err(|e| AnchorError::Db(e.to_string()))?;
    let Some(head) = head else {
        return Ok(AnchorOutcome::Unchanged);
    };
    if head.kind == ANCHOR_KIND {
        return Ok(AnchorOutcome::Unchanged);
    }
    let report = state
        .evidence
        .verify()
        .map_err(|e| AnchorError::Db(e.to_string()))?;
    if let Some(seq) = report.first_invalid_seq {
        return Err(AnchorError::ChainBroken(seq));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AnchorError::AllSinksFailed(vec![e.to_string()]))?;
    let mut details = serde_json::Map::new();
    details.insert("head_seq".into(), head.seq.into());
    details.insert("head_hash".into(), head.hash.clone().into());
    let mut errors = Vec::new();
    let mut anchored = false;

    if let Some(url) = &cfg.webhook_url {
        match send_webhook(
            &client,
            url,
            cfg.webhook_token.as_deref(),
            head.seq,
            &head.hash,
        )
        .await
        {
            Ok(status) => {
                anchored = true;
                details.insert(
                    "webhook".into(),
                    serde_json::json!({ "url": url, "status": status }),
                );
            }
            Err(e) => errors.push(format!("webhook: {e}")),
        }
    }
    if let Some(url) = &cfg.tsa_url {
        match request_timestamp(&client, url, &head.hash).await {
            Ok((status, response)) => {
                anchored = true;
                details.insert(
                    "tsa".into(),
                    serde_json::json!({
                        "url": url,
                        "status": status,
                        "response_der_b64": base64::engine::general_purpose::STANDARD.encode(response),
                    }),
                );
            }
            Err(e) => errors.push(format!("tsa: {e}")),
        }
    }
    if !anchored {
        return Err(AnchorError::AllSinksFailed(errors));
    }
    for e in &errors {
        tracing::warn!(error = %e, "evidence anchor sink failed");
    }
    let entry = state
        .evidence
        .append(
            ANCHOR_KIND,
            &format!("evidence:{}", head.seq),
            "system",
            serde_json::Value::Object(details),
        )
        .map_err(|e| AnchorError::Db(e.to_string()))?;
    tracing::info!(head_seq = head.seq, head_hash = %head.hash, "evidence log anchored");
    Ok(AnchorOutcome::Anchored {
        head_seq: head.seq,
        head_hash: head.hash,
        anchor_seq: entry.seq,
    })
}

async fn send_webhook(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    head_seq: i64,
    head_hash: &str,
) -> Result<u16, String> {
    let mut req = client.post(url).json(&serde_json::json!({
        "kind": "ferrite.evidence.head",
        "head_seq": head_seq,
        "head_hash": head_hash,
    }));
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() {
        Ok(status.as_u16())
    } else {
        Err(format!("HTTP {status}"))
    }
}

/// Ask a timestamp authority to sign the head hash. Returns the PKI status
/// (0 granted, 1 granted with modifications) and the raw `TimeStampResp`.
async fn request_timestamp(
    client: &reqwest::Client,
    url: &str,
    head_hash_hex: &str,
) -> Result<(u32, Vec<u8>), String> {
    let digest: [u8; 32] = hex::decode(head_hash_hex)
        .ok()
        .and_then(|d| d.try_into().ok())
        .ok_or("head hash is not 32 bytes of hex")?;
    let nonce = rand_nonce();
    let resp = client
        .post(url)
        .header("Content-Type", "application/timestamp-query")
        .body(tsa_request(&digest, nonce))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let body = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    let status = tsa_response_status(&body)?;
    if status > 1 {
        return Err(format!("timestamp refused (PKIStatus {status})"));
    }
    Ok((status, body))
}

fn rand_nonce() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish()
}

// ---------------------------------------------------------------------------
// Minimal DER for RFC 3161 (one fixed request shape, one field of the reply)
// ---------------------------------------------------------------------------

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let skip = bytes.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | (bytes.len() - skip) as u8);
        out.extend_from_slice(&bytes[skip..]);
    }
    out.extend_from_slice(content);
    out
}

fn der_uint(v: u64) -> Vec<u8> {
    let bytes = v.to_be_bytes();
    let skip = bytes.iter().take_while(|b| **b == 0).count().min(7);
    let mut content = bytes[skip..].to_vec();
    if content[0] & 0x80 != 0 {
        content.insert(0, 0);
    }
    der(0x02, &content)
}

/// `TimeStampReq` for a SHA-256 digest, with a nonce and `certReq = TRUE`.
pub fn tsa_request(digest: &[u8; 32], nonce: u64) -> Vec<u8> {
    const SHA256_OID: [u8; 9] = [0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01];
    let algorithm = der(0x30, &[der(0x06, &SHA256_OID), der(0x05, &[])].concat());
    let imprint = der(0x30, &[algorithm, der(0x04, digest)].concat());
    der(
        0x30,
        &[der_uint(1), imprint, der_uint(nonce), der(0x01, &[0xFF])].concat(),
    )
}

/// Split one DER TLV off the front of `buf`: `(tag, content, rest)`.
fn read_tlv(buf: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = buf.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (len, rest) = if first < 0x80 {
        (first as usize, rest)
    } else {
        let n = (first & 0x7F) as usize;
        if n == 0 || n > 4 || rest.len() < n {
            return None;
        }
        let len = rest[..n]
            .iter()
            .fold(0usize, |acc, b| (acc << 8) | *b as usize);
        (len, &rest[n..])
    };
    if rest.len() < len {
        return None;
    }
    Some((tag, &rest[..len], &rest[len..]))
}

/// PKIStatus of a `TimeStampResp`; a granted response must carry a token.
pub fn tsa_response_status(resp: &[u8]) -> Result<u32, String> {
    let bad = || "malformed timestamp response".to_string();
    let (tag, outer, _) = read_tlv(resp).ok_or_else(bad)?;
    if tag != 0x30 {
        return Err(bad());
    }
    let (tag, status_info, token) = read_tlv(outer).ok_or_else(bad)?;
    if tag != 0x30 {
        return Err(bad());
    }
    let (tag, status, _) = read_tlv(status_info).ok_or_else(bad)?;
    if tag != 0x02 || status.is_empty() || status.len() > 4 {
        return Err(bad());
    }
    let status = status.iter().fold(0u32, |acc, b| (acc << 8) | *b as u32);
    if status <= 1 && token.is_empty() {
        return Err("timestamp response has no token".into());
    }
    Ok(status)
}

/// Run [`anchor_once`] every `interval_secs`, if configured.
pub fn spawn_anchor_task(state: Arc<AppState>) {
    let cfg = &state.config.evidence_anchor;
    if cfg.interval_secs == 0 || !cfg.is_configured() {
        return;
    }
    let every = Duration::from_secs(cfg.interval_secs);
    tracing::info!(
        every_secs = cfg.interval_secs,
        "evidence anchoring scheduled"
    );
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            if let Err(e) = anchor_once(&state).await {
                tracing::error!(error = %e, "scheduled evidence anchor failed");
            }
        }
    });
}

/// POST /evidence/anchor — anchor the head now (admin).
pub async fn anchor_now(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    match anchor_once(&state).await {
        Ok(outcome) => axum::Json(outcome).into_response(),
        Err(e) => {
            let status = match e {
                AnchorError::NotConfigured => StatusCode::BAD_REQUEST,
                AnchorError::ChainBroken(_) => StatusCode::CONFLICT,
                AnchorError::AllSinksFailed(_) => StatusCode::BAD_GATEWAY,
                AnchorError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_the_expected_der() {
        let req = tsa_request(&[0xAB; 32], 0x8000_0000_0000_0001);
        // SEQUENCE { INTEGER 1, imprint, INTEGER nonce (with 0x00 pad), BOOLEAN TRUE }
        assert_eq!(&req[..2], &[0x30, req.len() as u8 - 2]);
        assert_eq!(&req[2..5], &[0x02, 0x01, 0x01]);
        let (_, body, _) = read_tlv(&req).unwrap();
        let (_, _version, rest) = read_tlv(body).unwrap();
        let (tag, imprint, rest) = read_tlv(rest).unwrap();
        assert_eq!(tag, 0x30);
        let (_, _alg, digest) = read_tlv(imprint).unwrap();
        assert_eq!(digest[0..2], [0x04, 32]);
        let (tag, nonce, rest) = read_tlv(rest).unwrap();
        assert_eq!(tag, 0x02);
        assert_eq!(nonce, &[0x00, 0x80, 0, 0, 0, 0, 0, 0, 0x01]);
        assert_eq!(rest, &[0x01, 0x01, 0xFF]);
    }

    #[test]
    fn response_status_parsing() {
        // SEQUENCE { SEQUENCE { INTEGER 0 }, <token> }
        let granted = der(
            0x30,
            &[der(0x30, &der_uint(0)), der(0x30, &[0x05, 0x00])].concat(),
        );
        assert_eq!(tsa_response_status(&granted), Ok(0));
        let rejected = der(0x30, &der(0x30, &der_uint(2)));
        assert_eq!(tsa_response_status(&rejected), Ok(2));
        let tokenless = der(0x30, &der(0x30, &der_uint(0)));
        assert!(tsa_response_status(&tokenless).is_err());
        assert!(tsa_response_status(&[0x30, 0x05, 0x01]).is_err());
        assert!(tsa_response_status(b"<html>").is_err());
    }

    #[test]
    fn long_form_lengths() {
        let long = der(0x04, &[7u8; 300]);
        assert_eq!(&long[..4], &[0x04, 0x82, 0x01, 0x2C]);
        let (tag, content, rest) = read_tlv(&long).unwrap();
        assert_eq!((tag, content.len(), rest.len()), (0x04, 300, 0));
    }
}
