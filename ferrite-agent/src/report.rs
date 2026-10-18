//! Encode one report as a batch of ferrite chunks.
//!
//! Every batch starts with a `DeviceInfo` chunk, so the server attributes it
//! correctly even when it arrives late from the offline spool.

use ferrite_sdk::chunks::ChunkEncoder;
use ferrite_sdk::metrics::{MetricEntry, MetricValue};

use crate::boot::Reboot;
use crate::config::MAX_METRIC_KEY;
use crate::identity::Identity;
use crate::system::{Sample, Value};

/// `encode_metrics` takes at most this many entries per call.
const METRICS_PER_CALL: usize = 32;

pub struct Report<'a> {
    pub identity: &'a Identity,
    pub uptime_ms: u64,
    pub mem_available_bytes: u32,
    pub samples: &'a [Sample],
    pub reboot: Option<Reboot>,
}

pub fn encode(encoder: &mut ChunkEncoder, report: &Report) -> Vec<u8> {
    let mut out = Vec::new();
    let mut emit = |chunk: &[u8]| out.extend_from_slice(chunk);

    encoder.encode_device_info(
        &report.identity.device_id,
        &report.identity.firmware_version,
        report.identity.build_id,
        &mut emit,
    );
    if let Some(r) = report.reboot {
        encoder.encode_reboot_reason(
            r.reason as u8,
            0,
            r.boot_sequence,
            r.uptime_before_reboot,
            &mut emit,
        );
    }
    let entries: Vec<MetricEntry> = report
        .samples
        .iter()
        .filter_map(|(key, value)| to_entry(key, value, report.uptime_ms))
        .collect();
    encoder.encode_heartbeat(
        report.uptime_ms,
        report.mem_available_bytes,
        entries.len() as u32,
        0,
        0,
        &mut emit,
    );
    for batch in entries.chunks(METRICS_PER_CALL) {
        encoder.encode_metrics(batch.iter(), &mut emit);
    }
    out
}

fn to_entry(key: &str, value: &Value, uptime_ms: u64) -> Option<MetricEntry> {
    if key.len() > MAX_METRIC_KEY {
        tracing::warn!(key, "metric key too long; skipped");
        return None;
    }
    let mut k = heapless::String::new();
    k.push_str(key).ok()?;
    Some(MetricEntry {
        key: k,
        value: match value {
            Value::Gauge(v) => MetricValue::Gauge(*v),
            Value::Counter(v) => MetricValue::Counter(*v),
        },
        timestamp_ticks: uptime_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_sdk::chunks::decoder::ChunkDecoder;
    use ferrite_sdk::chunks::types::{ChunkHeader, ChunkType};
    use ferrite_sdk::reboot_reason::RebootReason;

    fn split(mut bytes: &[u8]) -> Vec<(ChunkType, Vec<u8>)> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            let len = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
            let total = ChunkHeader::WIRE_SIZE + len + 2;
            let chunk = ChunkDecoder::decode(&bytes[..total]).expect("valid chunk");
            out.push((chunk.chunk_type, chunk.payload.to_vec()));
            bytes = &bytes[total..];
        }
        out
    }

    #[test]
    fn batch_layout_round_trips_through_the_sdk_decoder() {
        let identity = Identity {
            device_id: "linux-0011223344556677".into(),
            firmware_version: "2.3.1".into(),
            build_id: 20261018093000,
            build_id_source: String::new(),
        };
        let samples: Vec<Sample> = (0..40)
            .map(|i| (format!("m{i}"), Value::Gauge(i as f32)))
            .chain([
                ("coredumps".to_string(), Value::Counter(2)),
                ("x".repeat(33), Value::Counter(1)),
            ])
            .collect();
        let mut enc = ChunkEncoder::new();
        let bytes = encode(
            &mut enc,
            &Report {
                identity: &identity,
                uptime_ms: 60_000,
                mem_available_bytes: 1 << 20,
                samples: &samples,
                reboot: Some(Reboot {
                    reason: RebootReason::WatchdogTimeout,
                    boot_sequence: 4,
                    uptime_before_reboot: 3600,
                }),
            },
        );
        let chunks = split(&bytes);
        let types: Vec<ChunkType> = chunks.iter().map(|(t, _)| *t).collect();
        assert_eq!(types[0], ChunkType::DeviceInfo);
        assert_eq!(types[1], ChunkType::RebootReason);
        assert_eq!(types[2], ChunkType::Heartbeat);
        assert!(types[3..].iter().all(|t| *t == ChunkType::Metrics));

        // DeviceInfo: len-prefixed id, len-prefixed version, u64 build id.
        let info = &chunks[0].1;
        assert_eq!(&info[1..1 + info[0] as usize], b"linux-0011223344556677");
        assert_eq!(
            u64::from_le_bytes(info[info.len() - 8..].try_into().unwrap()),
            20261018093000
        );
        // RebootReason: reason, extra, boot_sequence, uptime.
        assert_eq!(chunks[1].1[0], RebootReason::WatchdogTimeout as u8);
        assert_eq!(u32::from_le_bytes(chunks[1].1[2..6].try_into().unwrap()), 4);
        // All 41 valid metrics arrive (the 33-byte key is dropped), across chunks.
        let metric_count: usize = chunks[3..].iter().map(|(_, p)| p[0] as usize).sum();
        assert_eq!(metric_count, 41);
    }
}
