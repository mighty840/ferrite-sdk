use axum::http;
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, Any, CorsLayer};

use crate::sse::SsePayload;
use crate::AppState;

/// Maximum allowed ELF upload size (50 MB).
const MAX_ELF_SIZE: usize = 50 * 1024 * 1024;

// ---------------------------------------------------------------------------
// CRC-16/CCITT-FALSE (reimplemented for std, matching ferrite-sdk's encoder)
// ---------------------------------------------------------------------------

fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

// ---------------------------------------------------------------------------
// Chunk wire format types (reimplemented for std)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChunkType {
    Heartbeat = 0x01,
    Metrics = 0x02,
    FaultRecord = 0x03,
    TraceFragment = 0x04,
    RebootReason = 0x05,
    DeviceInfo = 0x06,
    OtaRequest = 0x07,
}

impl ChunkType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x01 => Some(Self::Heartbeat),
            0x02 => Some(Self::Metrics),
            0x03 => Some(Self::FaultRecord),
            0x04 => Some(Self::TraceFragment),
            0x05 => Some(Self::RebootReason),
            0x06 => Some(Self::DeviceInfo),
            0x07 => Some(Self::OtaRequest),
            _ => None,
        }
    }
}

/// Flag bit indicating the chunk payload is encrypted (AES-128-CCM).
const FLAG_ENCRYPTED: u8 = 0x04;

const MAGIC: u8 = 0xEC;
const VERSION: u8 = 1;
const HEADER_SIZE: usize = 8;
const CRC_SIZE: usize = 2;
const MIN_CHUNK_SIZE: usize = HEADER_SIZE + CRC_SIZE;

#[derive(Debug)]
pub struct DecodedChunk {
    pub chunk_type: ChunkType,
    pub sequence_id: u16,
    pub flags: u8,
    pub payload: Vec<u8>,
}

impl DecodedChunk {
    pub fn is_last(&self) -> bool {
        self.flags & 0x01 != 0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("too short")]
    TooShort,
    #[error("bad magic byte: 0x{0:02X}")]
    BadMagic(u8),
    #[error("bad version: {0}")]
    BadVersion(u8),
    #[error("unknown chunk type: 0x{0:02X}")]
    UnknownType(u8),
    #[error("payload truncated")]
    PayloadTruncated,
    #[error("CRC mismatch: expected 0x{expected:04X}, got 0x{got:04X}")]
    CrcMismatch { expected: u16, got: u16 },
}

/// Decode a single chunk from raw bytes. Returns the decoded chunk and
/// the total number of bytes consumed.
fn decode_chunk(bytes: &[u8]) -> Result<(DecodedChunk, usize), DecodeError> {
    if bytes.len() < MIN_CHUNK_SIZE {
        return Err(DecodeError::TooShort);
    }

    if bytes[0] != MAGIC {
        return Err(DecodeError::BadMagic(bytes[0]));
    }

    if bytes[1] != VERSION {
        return Err(DecodeError::BadVersion(bytes[1]));
    }

    let chunk_type = ChunkType::from_u8(bytes[2]).ok_or(DecodeError::UnknownType(bytes[2]))?;
    let flags = bytes[3];
    let payload_len = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let sequence_id = u16::from_le_bytes([bytes[6], bytes[7]]);

    let total = HEADER_SIZE + payload_len + CRC_SIZE;
    if bytes.len() < total {
        return Err(DecodeError::PayloadTruncated);
    }

    let crc_offset = HEADER_SIZE + payload_len;
    let expected_crc = u16::from_le_bytes([bytes[crc_offset], bytes[crc_offset + 1]]);
    let computed_crc = crc16_ccitt(&bytes[..crc_offset]);

    if expected_crc != computed_crc {
        return Err(DecodeError::CrcMismatch {
            expected: expected_crc,
            got: computed_crc,
        });
    }

    let payload = bytes[HEADER_SIZE..HEADER_SIZE + payload_len].to_vec();

    Ok((
        DecodedChunk {
            chunk_type,
            sequence_id,
            flags,
            payload,
        },
        total,
    ))
}

/// Decrypt an encrypted chunk payload using AES-128-CCM.
/// Returns the decrypted plaintext, or None on failure.
fn decrypt_chunk_payload(key: &[u8; 16], encrypted: &[u8]) -> Option<Vec<u8>> {
    use aes::Aes128;
    use ccm::aead::generic_array::GenericArray;
    use ccm::aead::AeadInPlace;
    use ccm::aead::KeyInit;
    use ccm::consts::{U13, U8};
    use ccm::Ccm;

    type Aes128Ccm = Ccm<Aes128, U8, U13>;

    const NONCE_SIZE: usize = 13;
    const TAG_SIZE: usize = 8;

    if encrypted.len() < NONCE_SIZE + TAG_SIZE {
        return None;
    }

    let cipher = Aes128Ccm::new(GenericArray::from_slice(key));
    let nonce = &encrypted[..NONCE_SIZE];
    let ciphertext_len = encrypted.len() - NONCE_SIZE - TAG_SIZE;
    let ciphertext = &encrypted[NONCE_SIZE..NONCE_SIZE + ciphertext_len];
    let tag = &encrypted[NONCE_SIZE + ciphertext_len..];

    let mut plaintext = ciphertext.to_vec();
    let nonce_ga = GenericArray::from_slice(nonce);
    let tag_ga = GenericArray::from_slice(tag);

    cipher
        .decrypt_in_place_detached(nonce_ga, &[], &mut plaintext, tag_ga)
        .ok()?;

    Some(plaintext)
}

/// Decode all chunks from a byte stream. Stops at the first decode error
/// but returns all successfully decoded chunks up to that point.
fn decode_all_chunks(bytes: &[u8]) -> Vec<DecodedChunk> {
    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        match decode_chunk(&bytes[offset..]) {
            Ok((chunk, consumed)) => {
                chunks.push(chunk);
                offset += consumed;
            }
            Err(e) => {
                tracing::warn!("chunk decode error at offset {offset}: {e}");
                break;
            }
        }
    }
    chunks
}

// ---------------------------------------------------------------------------
// Payload parsers
// ---------------------------------------------------------------------------

/// Parse a FaultRecord payload (149 bytes).
/// Layout: 1B fault_type + 8x4B frame + 9x4B extended + 16x4B stack + 4x4B fault_regs
struct ParsedFault {
    fault_type: u8,
    pc: u32,
    lr: u32,
    sp: u32,
    cfsr: u32,
    hfsr: u32,
    mmfar: u32,
    bfar: u32,
    stack_snapshot: Vec<u32>,
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn parse_fault_payload(payload: &[u8]) -> Option<ParsedFault> {
    if payload.len() < 149 {
        tracing::warn!("fault payload too short: {} bytes", payload.len());
        return None;
    }

    let fault_type = payload[0];
    let mut pos = 1;

    // ExceptionFrame: r0, r1, r2, r3, r12, lr, pc, xpsr  (8 x u32)
    let _r0 = read_u32_le(payload, pos);
    pos += 4;
    let _r1 = read_u32_le(payload, pos);
    pos += 4;
    let _r2 = read_u32_le(payload, pos);
    pos += 4;
    let _r3 = read_u32_le(payload, pos);
    pos += 4;
    let _r12 = read_u32_le(payload, pos);
    pos += 4;
    let lr = read_u32_le(payload, pos);
    pos += 4;
    let pc = read_u32_le(payload, pos);
    pos += 4;
    let _xpsr = read_u32_le(payload, pos);
    pos += 4;

    // ExtendedRegisters: r4..r11, sp  (9 x u32)
    // Skip r4..r11 (8 registers)
    pos += 4 * 8;
    let sp = read_u32_le(payload, pos);
    pos += 4;

    // StackSnapshot: 16 x u32
    let mut stack_snapshot = Vec::with_capacity(16);
    for _ in 0..16 {
        stack_snapshot.push(read_u32_le(payload, pos));
        pos += 4;
    }

    // Fault status registers: cfsr, hfsr, mmfar, bfar
    let cfsr = read_u32_le(payload, pos);
    pos += 4;
    let hfsr = read_u32_le(payload, pos);
    pos += 4;
    let mmfar = read_u32_le(payload, pos);
    pos += 4;
    let bfar = read_u32_le(payload, pos);

    Some(ParsedFault {
        fault_type,
        pc,
        lr,
        sp,
        cfsr,
        hfsr,
        mmfar,
        bfar,
        stack_snapshot,
    })
}

/// Metric type identifiers matching ferrite-sdk.
const METRIC_TYPE_COUNTER: u8 = 0;
const METRIC_TYPE_GAUGE: u8 = 1;
const METRIC_TYPE_HISTOGRAM: u8 = 2;

struct ParsedMetricEntry {
    key: String,
    metric_type: u8,
    value_json: String,
    timestamp_ticks: u64,
}

fn parse_metrics_payload(payload: &[u8]) -> Vec<ParsedMetricEntry> {
    let mut entries = Vec::new();
    if payload.is_empty() {
        return entries;
    }

    let count = payload[0] as usize;
    let mut pos = 1;

    for _ in 0..count {
        if pos >= payload.len() {
            break;
        }

        // key_len + key
        let key_len = payload[pos] as usize;
        pos += 1;
        if pos + key_len > payload.len() {
            break;
        }
        let key = String::from_utf8_lossy(&payload[pos..pos + key_len]).to_string();
        pos += key_len;

        // metric_type
        if pos >= payload.len() {
            break;
        }
        let metric_type = payload[pos];
        pos += 1;

        // value: 8 bytes
        if pos + 8 > payload.len() {
            break;
        }
        let value_json = match metric_type {
            METRIC_TYPE_COUNTER => {
                let v = u32::from_le_bytes([
                    payload[pos],
                    payload[pos + 1],
                    payload[pos + 2],
                    payload[pos + 3],
                ]);
                format!(r#"{{"counter":{v}}}"#)
            }
            METRIC_TYPE_GAUGE => {
                let v = f32::from_le_bytes([
                    payload[pos],
                    payload[pos + 1],
                    payload[pos + 2],
                    payload[pos + 3],
                ]);
                format!(r#"{{"gauge":{v}}}"#)
            }
            METRIC_TYPE_HISTOGRAM => {
                let min = f32::from_le_bytes([
                    payload[pos],
                    payload[pos + 1],
                    payload[pos + 2],
                    payload[pos + 3],
                ]);
                let max = f32::from_le_bytes([
                    payload[pos + 4],
                    payload[pos + 5],
                    payload[pos + 6],
                    payload[pos + 7],
                ]);
                format!(r#"{{"histogram":{{"min":{min},"max":{max}}}}}"#)
            }
            _ => format!(r#"{{"raw":"{}"}}"#, hex::encode(&payload[pos..pos + 8])),
        };
        pos += 8;

        // timestamp_ticks: 8 bytes
        if pos + 8 > payload.len() {
            break;
        }
        let timestamp_ticks = u64::from_le_bytes([
            payload[pos],
            payload[pos + 1],
            payload[pos + 2],
            payload[pos + 3],
            payload[pos + 4],
            payload[pos + 5],
            payload[pos + 6],
            payload[pos + 7],
        ]);
        pos += 8;

        entries.push(ParsedMetricEntry {
            key,
            metric_type,
            value_json,
            timestamp_ticks,
        });
    }

    entries
}

struct ParsedDeviceInfo {
    device_id: String,
    firmware_version: String,
    build_id: u64,
}

fn parse_device_info_payload(payload: &[u8]) -> Option<ParsedDeviceInfo> {
    if payload.len() < 3 {
        return None;
    }

    let mut pos = 0;

    let did_len = payload[pos] as usize;
    pos += 1;
    if pos + did_len > payload.len() {
        return None;
    }
    let device_id = String::from_utf8_lossy(&payload[pos..pos + did_len]).to_string();
    pos += did_len;

    if pos >= payload.len() {
        return None;
    }
    let fw_len = payload[pos] as usize;
    pos += 1;
    if pos + fw_len > payload.len() {
        return None;
    }
    let firmware_version = String::from_utf8_lossy(&payload[pos..pos + fw_len]).to_string();
    pos += fw_len;

    if pos + 8 > payload.len() {
        return None;
    }
    let build_id = u64::from_le_bytes([
        payload[pos],
        payload[pos + 1],
        payload[pos + 2],
        payload[pos + 3],
        payload[pos + 4],
        payload[pos + 5],
        payload[pos + 6],
        payload[pos + 7],
    ]);

    Some(ParsedDeviceInfo {
        device_id,
        firmware_version,
        build_id,
    })
}

struct ParsedRebootReason {
    reason: u8,
    extra: u8,
    boot_sequence: u32,
    uptime_before_reboot: u32,
}

fn parse_reboot_reason_payload(payload: &[u8]) -> Option<ParsedRebootReason> {
    if payload.len() < 10 {
        return None;
    }
    Some(ParsedRebootReason {
        reason: payload[0],
        extra: payload[1],
        boot_sequence: u32::from_le_bytes([payload[2], payload[3], payload[4], payload[5]]),
        uptime_before_reboot: u32::from_le_bytes([payload[6], payload[7], payload[8], payload[9]]),
    })
}

struct ParsedOtaRequest {
    build_id: u64,
    max_chunk_size: u16,
    ota_capable: bool,
}

fn parse_ota_request_payload(payload: &[u8]) -> Option<ParsedOtaRequest> {
    if payload.len() < 14 {
        return None;
    }
    let build_id = u64::from_le_bytes([
        payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
        payload[7],
    ]);
    let max_chunk_size = u16::from_le_bytes([payload[8], payload[9]]);
    let ota_capable = payload[10] != 0;

    Some(ParsedOtaRequest {
        build_id,
        max_chunk_size,
        ota_capable,
    })
}

struct ParsedHeartbeat {
    uptime_ticks: u64,
    free_stack_bytes: u32,
    metrics_count: u32,
    frames_lost: u32,
    device_key: u32,
}

fn parse_heartbeat_payload(payload: &[u8]) -> Option<ParsedHeartbeat> {
    if payload.len() < 20 {
        return None;
    }
    let device_key = if payload.len() >= 24 {
        u32::from_le_bytes([payload[20], payload[21], payload[22], payload[23]])
    } else {
        0
    };
    Some(ParsedHeartbeat {
        uptime_ticks: u64::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
            payload[7],
        ]),
        free_stack_bytes: u32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]),
        metrics_count: u32::from_le_bytes([payload[12], payload[13], payload[14], payload[15]]),
        frames_lost: u32::from_le_bytes([payload[16], payload[17], payload[18], payload[19]]),
        device_key,
    })
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct IngestResponse {
    ok: bool,
    chunks_received: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

/// POST /ingest/chunks
/// Accepts raw binary body containing one or more concatenated wire-format chunks.
/// Optionally, a `X-Device-Id` header can provide a fallback device ID when no
/// DeviceInfo chunk is present in the payload.
async fn ingest_chunks(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let mut chunks = decode_all_chunks(&body);
    let num_chunks = chunks.len();
    let mut errors: Vec<String> = Vec::new();

    // Decrypt encrypted chunks if key is configured
    if let Some(ref key) = state.config.chunk_encryption_key {
        for chunk in &mut chunks {
            if chunk.flags & FLAG_ENCRYPTED != 0 {
                match decrypt_chunk_payload(key, &chunk.payload) {
                    Some(plaintext) => {
                        chunk.payload = plaintext;
                        chunk.flags &= !FLAG_ENCRYPTED; // clear encrypted flag
                    }
                    None => {
                        errors.push(format!(
                            "failed to decrypt chunk seq={} type={:?}",
                            chunk.sequence_id, chunk.chunk_type
                        ));
                    }
                }
            }
        }
    }

    // Prometheus counters
    state
        .counters
        .ingest_requests
        .fetch_add(1, Ordering::Relaxed);
    state
        .counters
        .ingest_chunks
        .fetch_add(num_chunks as u64, Ordering::Relaxed);

    // Determine device context: first try to find a DeviceInfo chunk, else use header.
    let fallback_device_id = headers
        .get("X-Device-Id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let mut current_device_id = fallback_device_id.clone();
    let mut device_rowid: Option<i64> = None;

    let store = state.store.lock().await;

    // Wrap all DB writes in a transaction for atomicity.
    if let Err(e) = store.begin_transaction() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(IngestResponse {
                ok: false,
                chunks_received: 0,
                errors: vec![format!("failed to begin transaction: {e}")],
            }),
        );
    }

    // Process DeviceInfo chunks first to establish device context.
    for chunk in &chunks {
        if chunk.chunk_type == ChunkType::DeviceInfo {
            if let Some(info) = parse_device_info_payload(&chunk.payload) {
                current_device_id = info.device_id.clone();
                match store.upsert_device(&info.device_id, &info.firmware_version, info.build_id) {
                    Ok(rid) => device_rowid = Some(rid),
                    Err(e) => errors.push(format!("db error upserting device: {e}")),
                }
            }
        }
    }

    // Ensure we have a device row.
    // Skip creating "unknown" devices — these are from batches without DeviceInfo
    // (e.g. BLE chunks arriving before the DeviceInfo notification).
    // The next batch will contain DeviceInfo and create the device properly.
    if device_rowid.is_none() && current_device_id != "unknown" {
        match store.touch_device(&current_device_id) {
            Ok(rid) => device_rowid = Some(rid),
            Err(e) => {
                errors.push(format!("db error touching device: {e}"));
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(IngestResponse {
                        ok: false,
                        chunks_received: 0,
                        errors,
                    }),
                );
            }
        }
    }

    let Some(dev_rid) = device_rowid else {
        // No device context — either "unknown" device or missing DeviceInfo.
        // Accept the request (chunks will arrive again with DeviceInfo) but don't store.
        let _ = store.commit_transaction();
        return (
            StatusCode::OK,
            Json(IngestResponse {
                ok: true,
                chunks_received: num_chunks,
                errors,
            }),
        );
    };

    // Process remaining chunk types.
    for chunk in &chunks {
        match chunk.chunk_type {
            ChunkType::DeviceInfo => {
                // Already handled above.
            }
            ChunkType::FaultRecord => {
                if let Some(fault) = parse_fault_payload(&chunk.payload) {
                    // Attempt symbolication.
                    let symbol = {
                        let sym = state.symbolicator.lock().await;
                        sym.symbolize(fault.pc).await.ok().flatten()
                    };

                    // Find or create crash group for deduplication.
                    let crash_group_id = match store.find_or_create_crash_group(
                        fault.fault_type,
                        fault.pc,
                        symbol.as_deref(),
                        dev_rid,
                    ) {
                        Ok(id) => Some(id),
                        Err(e) => {
                            errors.push(format!("db error upserting crash group: {e}"));
                            None
                        }
                    };

                    if let Err(e) = store.insert_fault(
                        dev_rid,
                        fault.fault_type,
                        fault.pc,
                        fault.lr,
                        fault.cfsr,
                        fault.hfsr,
                        fault.mmfar,
                        fault.bfar,
                        fault.sp,
                        &fault.stack_snapshot,
                        symbol.as_deref(),
                        crash_group_id,
                    ) {
                        errors.push(format!("db error inserting fault: {e}"));
                    } else {
                        // Update affected device count for the crash group.
                        if let Some(cg_id) = crash_group_id {
                            if let Err(e) = store.update_crash_group_device_count(cg_id) {
                                errors.push(format!(
                                    "db error updating crash group device count: {e}"
                                ));
                            }
                        }
                        let _ = state.event_tx.send(SsePayload::fault(
                            &current_device_id,
                            fault.fault_type,
                            fault.pc,
                        ));
                        // Send fault alert webhook (#28)
                        crate::alerting::send_fault_alert(
                            &state,
                            &current_device_id,
                            fault.fault_type,
                            fault.pc,
                            symbol.as_deref(),
                        );
                    }
                } else {
                    errors.push("failed to parse fault payload".to_string());
                }
            }
            ChunkType::Metrics => {
                let entries = parse_metrics_payload(&chunk.payload);
                for entry in &entries {
                    if let Err(e) = store.insert_metric(
                        dev_rid,
                        &entry.key,
                        entry.metric_type,
                        &entry.value_json,
                        entry.timestamp_ticks,
                    ) {
                        errors.push(format!("db error inserting metric '{}': {e}", entry.key));
                    } else {
                        let _ = state.event_tx.send(SsePayload::metric(
                            &current_device_id,
                            &entry.key,
                            &entry.value_json,
                        ));
                    }
                }
            }
            ChunkType::RebootReason => {
                if let Some(reboot) = parse_reboot_reason_payload(&chunk.payload) {
                    if let Err(e) = store.insert_reboot(
                        dev_rid,
                        reboot.reason,
                        reboot.extra,
                        reboot.boot_sequence,
                        reboot.uptime_before_reboot,
                    ) {
                        errors.push(format!("db error inserting reboot: {e}"));
                    } else {
                        let _ = state
                            .event_tx
                            .send(SsePayload::reboot(&current_device_id, reboot.reason));
                    }
                } else {
                    errors.push("failed to parse reboot reason payload".to_string());
                }
            }
            ChunkType::Heartbeat => {
                if let Some(hb) = parse_heartbeat_payload(&chunk.payload) {
                    tracing::info!(
                        device = %current_device_id,
                        uptime_ticks = hb.uptime_ticks,
                        free_stack = hb.free_stack_bytes,
                        metrics_count = hb.metrics_count,
                        frames_lost = hb.frames_lost,
                        device_key = hb.device_key,
                        "heartbeat received"
                    );
                    // If device_key is present, resolve device by key and update status.
                    if hb.device_key != 0 {
                        let _ = store.touch_device_by_key(hb.device_key as i64, "online");
                    }
                    // Always update status to "online" and touch last_seen,
                    // even for auto-discovered devices without a device_key.
                    if current_device_id != "unknown" {
                        let _ = store.update_device_status_by_id(&current_device_id, "online");
                    }
                    let _ = state
                        .event_tx
                        .send(SsePayload::heartbeat(&current_device_id, hb.uptime_ticks));
                }
            }
            ChunkType::TraceFragment => {
                // Trace fragments are logged but not yet persisted to DB.
                tracing::debug!(
                    device = %current_device_id,
                    seq = chunk.sequence_id,
                    len = chunk.payload.len(),
                    last = chunk.is_last(),
                    "trace fragment received"
                );
            }
            ChunkType::OtaRequest => {
                if let Some(ota) = parse_ota_request_payload(&chunk.payload) {
                    tracing::info!(
                        device = %current_device_id,
                        build_id = ota.build_id,
                        max_chunk_size = ota.max_chunk_size,
                        ota_capable = ota.ota_capable,
                        "OTA request received"
                    );
                    // Check if there's an OTA target for this device
                    if let Ok(Some(target)) = store.get_ota_target_for_device(&current_device_id) {
                        if target.target_build_id as u64 != ota.build_id {
                            tracing::info!(
                                device = %current_device_id,
                                current_build = ota.build_id,
                                target_build = target.target_build_id,
                                target_version = %target.target_version,
                                "OTA update available for device"
                            );
                            let _ = state.event_tx.send(SsePayload::ota_available(
                                &current_device_id,
                                &target.target_version,
                                target.target_build_id,
                            ));
                        }
                    }
                } else {
                    errors.push("failed to parse OTA request payload".to_string());
                }
            }
        }
    }

    // Commit the transaction (or rollback on total failure).
    if errors.is_empty() {
        if let Err(e) = store.commit_transaction() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(IngestResponse {
                    ok: false,
                    chunks_received: num_chunks,
                    errors: vec![format!("failed to commit transaction: {e}")],
                }),
            );
        }
    } else {
        // Partial success — still commit so we don't lose good data.
        let _ = store.commit_transaction();
    }

    let status = if errors.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::MULTI_STATUS
    };

    (
        status,
        Json(IngestResponse {
            ok: errors.is_empty(),
            chunks_received: num_chunks,
            errors,
        }),
    )
}

/// POST /ingest/elf
/// Accepts a multipart or raw binary ELF file upload.
/// The firmware version should be provided in the `X-Firmware-Version` header.
async fn ingest_elf(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    if body.len() > MAX_ELF_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "ok": false,
                "error": format!(
                    "ELF file too large: {} bytes exceeds {} byte limit",
                    body.len(),
                    MAX_ELF_SIZE
                ),
            })),
        );
    }

    let version = headers
        .get("X-Firmware-Version")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let filename = format!("{version}.elf");
    let path = state.elf_dir.join(&filename);

    match tokio::fs::write(&path, &body).await {
        Ok(_) => {
            tracing::info!(version = version, path = %path.display(), "ELF file stored");
            let mut sym = state.symbolicator.lock().await;
            sym.register_elf(version, path.clone());
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": true,
                    "path": path.display().to_string(),
                })),
            )
        }
        Err(e) => {
            tracing::error!("failed to write ELF: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": e.to_string(),
                })),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Query handlers
// ---------------------------------------------------------------------------

async fn list_devices(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_devices() {
        Ok(devices) => (
            StatusCode::OK,
            Json(serde_json::json!({ "devices": devices })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_device_faults(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_faults_for_device_paginated(
        &device_id,
        params.limit(),
        params.offset(),
        params.since.as_deref(),
        params.until.as_deref(),
    ) {
        Ok(faults) => (
            StatusCode::OK,
            Json(serde_json::json!({ "faults": faults })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_device_metrics(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_metrics_for_device_paginated(
        &device_id,
        params.limit(),
        params.offset(),
        params.since.as_deref(),
        params.until.as_deref(),
    ) {
        Ok(metrics) => (
            StatusCode::OK,
            Json(serde_json::json!({ "metrics": metrics })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

// ---------------------------------------------------------------------------
// Device Registration handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RegisterDeviceRequest {
    device_key: String,
    name: Option<String>,
    tags: Option<String>,
    provisioned_by: Option<String>,
}

#[derive(Deserialize)]
struct UpdateDeviceRequest {
    name: Option<String>,
    tags: Option<String>,
}

/// Parse a hex device key string (e.g. "A300F1B2") into an i64.
fn parse_device_key_hex(s: &str) -> Option<i64> {
    // Strip optional "0x" prefix and dashes
    let clean: String = s.replace('-', "").replace("0x", "").replace("0X", "");
    u32::from_str_radix(&clean, 16).ok().map(|v| v as i64)
}

/// POST /devices/register
async fn register_device(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterDeviceRequest>,
) -> impl IntoResponse {
    let Some(key) = parse_device_key_hex(&req.device_key) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid device_key hex" })),
        );
    };

    let store = state.store.lock().await;
    match store.register_device(
        key,
        req.name.as_deref(),
        req.tags.as_deref(),
        req.provisioned_by.as_deref(),
    ) {
        Ok(_) => match store.get_device_by_key(key) {
            Ok(Some(device)) => (
                StatusCode::OK,
                Json(serde_json::json!({ "device": device })),
            ),
            Ok(None) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "device not found after registration" })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("db error: {e}") })),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// POST /devices/register/bulk
async fn register_devices_bulk(
    State(state): State<Arc<AppState>>,
    Json(devices): Json<Vec<RegisterDeviceRequest>>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    let mut registered = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Wrap bulk registration in a transaction for atomicity.
    if let Err(e) = store.begin_transaction() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("failed to begin transaction: {e}") })),
        );
    }

    for req in &devices {
        let Some(key) = parse_device_key_hex(&req.device_key) else {
            errors.push(format!("invalid device_key: {}", req.device_key));
            continue;
        };
        match store.register_device(
            key,
            req.name.as_deref(),
            req.tags.as_deref(),
            req.provisioned_by.as_deref(),
        ) {
            Ok(_) => {
                if let Ok(Some(dev)) = store.get_device_by_key(key) {
                    registered.push(dev);
                }
            }
            Err(e) => errors.push(format!("{}: {}", req.device_key, e)),
        }
    }

    // Commit regardless of partial errors (valid registrations should persist).
    let _ = store.commit_transaction();

    let status = if errors.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::MULTI_STATUS
    };
    (
        status,
        Json(serde_json::json!({
            "registered": registered.len(),
            "devices": registered,
            "errors": errors,
        })),
    )
}

/// PUT /devices/{key}
async fn update_device_handler(
    State(state): State<Arc<AppState>>,
    Path(key_str): Path<String>,
    Json(req): Json<UpdateDeviceRequest>,
) -> impl IntoResponse {
    let Some(key) = parse_device_key_hex(&key_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid device_key hex" })),
        );
    };
    let store = state.store.lock().await;
    match store.update_device(key, req.name.as_deref(), req.tags.as_deref()) {
        Ok(true) => match store.get_device_by_key(key) {
            Ok(Some(device)) => (
                StatusCode::OK,
                Json(serde_json::json!({ "device": device })),
            ),
            Ok(None) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "device not found after update" })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("db error: {e}") })),
            ),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "device not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /devices/{key}
async fn get_device_handler(
    State(state): State<Arc<AppState>>,
    Path(key_str): Path<String>,
) -> impl IntoResponse {
    let Some(key) = parse_device_key_hex(&key_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid device_key hex" })),
        );
    };
    let store = state.store.lock().await;
    match store.get_device_by_key(key) {
        Ok(Some(device)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "device": device })),
        ),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "device not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /devices/{key}
async fn delete_device_handler(
    State(state): State<Arc<AppState>>,
    Path(key_str): Path<String>,
) -> impl IntoResponse {
    let Some(key) = parse_device_key_hex(&key_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid device_key hex" })),
        );
    };
    let store = state.store.lock().await;
    match store.delete_device(key) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "device not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /admin/devices/:device_id — delete a device by its device_id string.
async fn admin_delete_device_handler(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.delete_device_by_id(&device_id) {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "device not found" })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// Pagination and time-range query parameters shared by list endpoints.
#[derive(Debug, Deserialize)]
struct ListParams {
    /// Maximum number of results (default 100, max 1000).
    limit: Option<usize>,
    /// Offset for pagination (default 0).
    offset: Option<usize>,
    /// ISO 8601 start time filter (inclusive).
    since: Option<String>,
    /// ISO 8601 end time filter (exclusive).
    until: Option<String>,
}

impl ListParams {
    fn limit(&self) -> usize {
        self.limit.unwrap_or(100).min(1000)
    }
    fn offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

async fn list_all_faults(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_all_faults_paginated(
        params.limit(),
        params.offset(),
        params.since.as_deref(),
        params.until.as_deref(),
    ) {
        Ok(faults) => (
            StatusCode::OK,
            Json(serde_json::json!({ "faults": faults })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn list_all_metrics(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ListParams>,
) -> impl IntoResponse {
    let store = state.store.lock().await;
    match store.list_all_metrics_paginated(
        params.limit(),
        params.offset(),
        params.since.as_deref(),
        params.until.as_deref(),
    ) {
        Ok(metrics) => (
            StatusCode::OK,
            Json(serde_json::json!({ "metrics": metrics })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /health
async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

async fn auth_mode(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.config.mode_response())
}

pub fn router(state: Arc<AppState>) -> Router {
    let allow_origin: AllowOrigin = match &state.config.cors_origin {
        Some(origin) => {
            tracing::info!("CORS: allowing origin {}", origin);
            origin
                .parse::<http::HeaderValue>()
                .expect("invalid CORS_ORIGIN value")
                .into()
        }
        None => {
            tracing::warn!(
                "CORS_ORIGIN not set, allowing all origins (not recommended for production)"
            );
            Any.into()
        }
    };

    let cors = CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods(AllowMethods::from([
            http::Method::GET,
            http::Method::POST,
            http::Method::PUT,
            http::Method::DELETE,
            http::Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::from([
            http::header::AUTHORIZATION,
            http::header::CONTENT_TYPE,
            http::HeaderName::from_static("x-device-id"),
            http::HeaderName::from_static("x-api-key"),
        ]));

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/auth/mode", get(auth_mode))
        .route("/ingest/chunks", post(ingest_chunks))
        .route(
            "/ingest/elf",
            post(ingest_elf).layer(DefaultBodyLimit::max(MAX_ELF_SIZE)),
        )
        .route("/devices", get(list_devices))
        .route("/devices/register", post(register_device))
        .route("/devices/register/bulk", post(register_devices_bulk))
        .route(
            "/devices/:key",
            get(get_device_handler)
                .put(update_device_handler)
                .delete(delete_device_handler),
        )
        .route("/devices/:id/faults", get(list_device_faults))
        .route("/devices/:id/metrics", get(list_device_metrics))
        .route("/faults", get(list_all_faults))
        .route("/crashes", get(crate::crashes::list_crash_groups))
        .route("/crashes/:id", get(crate::crashes::get_crash_group_detail))
        .route("/metrics", get(list_all_metrics))
        // SSE live event stream (#26)
        .route("/events/stream", get(crate::sse::event_stream))
        // Device groups (#27)
        .route(
            "/groups",
            get(crate::groups::list_groups).post(crate::groups::create_group),
        )
        .route(
            "/groups/:id",
            get(crate::groups::get_group)
                .put(crate::groups::update_group)
                .delete(crate::groups::delete_group),
        )
        .route(
            "/groups/:id/devices",
            get(crate::groups::list_group_devices),
        )
        .route(
            "/groups/:id/devices/:device_id",
            post(crate::groups::add_device_to_group)
                .delete(crate::groups::remove_device_from_group),
        )
        // Prometheus metrics (#33)
        .route(
            "/metrics/prometheus",
            get(crate::prometheus::prometheus_metrics),
        )
        // OTA firmware targets (#31)
        .route(
            "/ota/targets",
            get(crate::ota::list_ota_targets).post(crate::ota::set_ota_target),
        )
        .route(
            "/ota/targets/:device_id",
            get(crate::ota::get_ota_target).delete(crate::ota::delete_ota_target),
        )
        // OTA firmware artifact storage (#Sprint2a)
        .route(
            "/ota/firmware",
            get(crate::ota::list_firmware).post(crate::ota::upload_firmware),
        )
        .route(
            "/ota/firmware/:id",
            get(crate::ota::get_firmware).delete(crate::ota::delete_firmware),
        )
        .route(
            "/ota/firmware/:id/download",
            get(crate::ota::download_firmware),
        )
        // Admin: backup, retention, device management
        .route("/admin/backup", get(crate::backup::backup_database))
        .route("/admin/retention", get(crate::backup::retention_info))
        .route(
            "/admin/devices/:device_id",
            delete(admin_delete_device_handler),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth_middleware::require_auth,
        ))
        .layer(cors);

    // Rate limiting layer (#34) — inject limiter into extensions
    if let Some(ref limiter) = state.rate_limiter {
        let limiter = limiter.clone();
        app = app.layer(axum::middleware::from_fn(
            move |req: axum::http::Request<axum::body::Body>, next: axum::middleware::Next| {
                let limiter = limiter.clone();
                async move {
                    let path = req.uri().path().to_string();
                    if path.starts_with("/ingest") || path.starts_with("/auth") {
                        let ip = req
                            .extensions()
                            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                            .map(|ci| ci.0.ip())
                            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
                        if !limiter.try_acquire(ip).await {
                            return (
                                StatusCode::TOO_MANY_REQUESTS,
                                [("retry-after", "1")],
                                "Rate limit exceeded",
                            )
                                .into_response();
                        }
                    }
                    next.run(req).await
                }
            },
        ));
    }

    app.with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc16_ccitt_known_value() {
        // "123456789" should produce 0x29B1 for CRC-16/CCITT-FALSE
        let data = b"123456789";
        let crc = crc16_ccitt(data);
        assert_eq!(crc, 0x29B1);
    }

    #[test]
    fn test_crc16_empty() {
        let crc = crc16_ccitt(&[]);
        assert_eq!(crc, 0xFFFF);
    }

    #[test]
    fn test_decode_chunk_too_short() {
        let result = decode_chunk(&[0xEC, 1, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_chunk_bad_magic() {
        let result = decode_chunk(&[0x00, 1, 0x01, 0, 0, 0, 0, 0, 0, 0]);
        assert!(matches!(result, Err(DecodeError::BadMagic(0x00))));
    }

    #[test]
    fn test_decode_chunk_bad_version() {
        let result = decode_chunk(&[0xEC, 99, 0x01, 0, 0, 0, 0, 0, 0, 0]);
        assert!(matches!(result, Err(DecodeError::BadVersion(99))));
    }

    /// Build a valid chunk for testing, matching the ferrite-sdk encoder format.
    fn build_test_chunk(chunk_type: u8, flags: u8, seq: u16, payload: &[u8]) -> Vec<u8> {
        let payload_len = payload.len() as u16;
        let mut buf = Vec::with_capacity(HEADER_SIZE + payload.len() + CRC_SIZE);

        buf.push(MAGIC);
        buf.push(VERSION);
        buf.push(chunk_type);
        buf.push(flags);
        buf.extend_from_slice(&payload_len.to_le_bytes());
        buf.extend_from_slice(&seq.to_le_bytes());
        buf.extend_from_slice(payload);

        let crc = crc16_ccitt(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    #[test]
    fn test_decode_valid_heartbeat() {
        let mut payload = [0u8; 20];
        payload[0..8].copy_from_slice(&12345u64.to_le_bytes());
        payload[8..12].copy_from_slice(&1024u32.to_le_bytes());
        payload[12..16].copy_from_slice(&5u32.to_le_bytes());
        payload[16..20].copy_from_slice(&0u32.to_le_bytes());

        let raw = build_test_chunk(0x01, 0x01, 0, &payload);
        let (chunk, consumed) = decode_chunk(&raw).unwrap();

        assert_eq!(consumed, raw.len());
        assert_eq!(chunk.chunk_type, ChunkType::Heartbeat);
        assert!(chunk.is_last());

        let hb = parse_heartbeat_payload(&chunk.payload).unwrap();
        assert_eq!(hb.uptime_ticks, 12345);
        assert_eq!(hb.free_stack_bytes, 1024);
    }

    #[test]
    fn test_decode_multiple_chunks() {
        let c1 = build_test_chunk(0x01, 0x01, 0, &[0u8; 20]);
        let c2 = build_test_chunk(0x05, 0x01, 1, &[4, 0, 0, 0, 0, 42, 0, 0, 0, 0]);

        let mut combined = Vec::new();
        combined.extend_from_slice(&c1);
        combined.extend_from_slice(&c2);

        let chunks = decode_all_chunks(&combined);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].chunk_type, ChunkType::Heartbeat);
        assert_eq!(chunks[1].chunk_type, ChunkType::RebootReason);
    }

    #[test]
    fn test_decode_crc_mismatch() {
        let mut raw = build_test_chunk(0x01, 0x01, 0, &[0u8; 20]);
        // Corrupt a payload byte.
        raw[8] ^= 0xFF;
        let result = decode_chunk(&raw);
        assert!(matches!(result, Err(DecodeError::CrcMismatch { .. })));
    }

    #[test]
    fn test_parse_device_info() {
        let device_id = b"test-device";
        let fw = b"1.0.0";
        let build_id: u64 = 0xDEADBEEF;

        let mut payload = Vec::new();
        payload.push(device_id.len() as u8);
        payload.extend_from_slice(device_id);
        payload.push(fw.len() as u8);
        payload.extend_from_slice(fw);
        payload.extend_from_slice(&build_id.to_le_bytes());

        let info = parse_device_info_payload(&payload).unwrap();
        assert_eq!(info.device_id, "test-device");
        assert_eq!(info.firmware_version, "1.0.0");
        assert_eq!(info.build_id, 0xDEADBEEF);
    }

    #[test]
    fn test_parse_reboot_reason() {
        let mut payload = vec![0u8; 10];
        payload[0] = 4; // HardFault
        payload[1] = 0; // extra
        payload[2..6].copy_from_slice(&42u32.to_le_bytes());
        payload[6..10].copy_from_slice(&100_000u32.to_le_bytes());

        let reboot = parse_reboot_reason_payload(&payload).unwrap();
        assert_eq!(reboot.reason, 4);
        assert_eq!(reboot.boot_sequence, 42);
        assert_eq!(reboot.uptime_before_reboot, 100_000);
    }

    #[test]
    fn test_parse_metrics_counter() {
        // Build a metrics payload: count=1, then one entry
        let key = b"clicks";
        let mut payload = Vec::new();
        payload.push(1); // count
        payload.push(key.len() as u8);
        payload.extend_from_slice(key);
        payload.push(METRIC_TYPE_COUNTER);
        // value: 8 bytes (counter u32 + 4 padding)
        payload.extend_from_slice(&42u32.to_le_bytes());
        payload.extend_from_slice(&[0; 4]); // padding
                                            // timestamp: 8 bytes
        payload.extend_from_slice(&1000u64.to_le_bytes());

        let entries = parse_metrics_payload(&payload);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "clicks");
        assert_eq!(entries[0].metric_type, METRIC_TYPE_COUNTER);
        assert!(entries[0].value_json.contains("42"));
    }

    #[test]
    fn test_parse_fault_payload() {
        // Build a 149-byte fault payload
        let mut payload = vec![0u8; 149];
        payload[0] = 0; // HardFault

        // Set PC at offset 1 + 6*4 = 25
        let pc: u32 = 0x0800_2000;
        payload[25..29].copy_from_slice(&pc.to_le_bytes());

        // Set LR at offset 1 + 5*4 = 21
        let lr: u32 = 0x0800_1000;
        payload[21..25].copy_from_slice(&lr.to_le_bytes());

        let fault = parse_fault_payload(&payload).unwrap();
        assert_eq!(fault.fault_type, 0);
        assert_eq!(fault.pc, pc);
        assert_eq!(fault.lr, lr);
    }
}
