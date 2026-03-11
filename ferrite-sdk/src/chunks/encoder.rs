use crate::chunks::types::{ChunkHeader, ChunkType};
use crate::config::MAX_PAYLOAD_SIZE;
use crate::fault::FaultRecord;
use crate::metrics::MetricEntry;
use crate::trace::TraceBuffer;

/// Encodes SDK data into binary chunks for transport.
pub struct ChunkEncoder {
    sequence: u16,
}

impl Default for ChunkEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkEncoder {
    pub const fn new() -> Self {
        Self { sequence: 0 }
    }

    fn next_seq(&mut self) -> u16 {
        let seq = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        seq
    }

    /// Encode a single chunk with the given type and payload.
    /// Returns the number of bytes written to `out`.
    pub fn encode(
        &mut self,
        chunk_type: ChunkType,
        payload: &[u8],
        last: bool,
        out: &mut [u8; 256],
    ) -> usize {
        // Reserve space for header (8) + CRC (2) = 10 bytes overhead
        let max_payload = MAX_PAYLOAD_SIZE.min(256 - 10);
        let payload_len = payload.len().min(max_payload);
        let seq = self.next_seq();

        let mut flags = 0u8;
        if last {
            flags |= 0x01;
        }

        // Write header
        out[0] = ChunkHeader::MAGIC;
        out[1] = ChunkHeader::VERSION;
        out[2] = chunk_type as u8;
        out[3] = flags;
        out[4] = (payload_len & 0xFF) as u8;
        out[5] = ((payload_len >> 8) & 0xFF) as u8;
        out[6] = (seq & 0xFF) as u8;
        out[7] = ((seq >> 8) & 0xFF) as u8;

        // Write payload
        out[8..8 + payload_len].copy_from_slice(&payload[..payload_len]);

        // CRC-16/CCITT-FALSE over header + payload
        let crc_data = &out[..8 + payload_len];
        let crc_val = crc16_ccitt(crc_data);
        let total = 8 + payload_len;
        out[total] = (crc_val & 0xFF) as u8;
        out[total + 1] = ((crc_val >> 8) & 0xFF) as u8;

        total + 2
    }

    /// Encode a FaultRecord into one chunk, calling `emit` with the encoded bytes.
    pub fn encode_fault<F: FnMut(&[u8])>(&mut self, record: &FaultRecord, mut emit: F) {
        let mut payload = [0u8; MAX_PAYLOAD_SIZE];
        let len = record.serialize_to(&mut payload);
        let mut out = [0u8; 256];
        let n = self.encode(ChunkType::FaultRecord, &payload[..len], true, &mut out);
        emit(&out[..n]);
    }

    /// Encode all metrics, calling `emit` for each chunk.
    pub fn encode_metrics<'a, I, F>(&mut self, entries: I, mut emit: F)
    where
        I: Iterator<Item = &'a MetricEntry>,
        F: FnMut(&[u8]),
    {
        let entries_vec: heapless::Vec<&MetricEntry, 32> = entries.collect();
        if entries_vec.is_empty() {
            return;
        }

        // Serialize metrics into payload chunks
        let mut payload = [0u8; MAX_PAYLOAD_SIZE];
        let mut pos = 0;

        // Entry count byte (placeholder, we may need multiple chunks)
        let mut count = 0u8;
        pos += 1; // reserve for count

        for entry in &entries_vec {
            let entry_size = entry.serialized_size();
            if pos + entry_size > MAX_PAYLOAD_SIZE {
                // Emit current chunk
                payload[0] = count;
                let mut out = [0u8; 256];
                let n = self.encode(ChunkType::Metrics, &payload[..pos], false, &mut out);
                emit(&out[..n]);
                // Reset
                pos = 1;
                count = 0;
            }
            entry.serialize_to(&mut payload[pos..]);
            pos += entry_size;
            count += 1;
        }

        if count > 0 {
            payload[0] = count;
            let mut out = [0u8; 256];
            let n = self.encode(ChunkType::Metrics, &payload[..pos], true, &mut out);
            emit(&out[..n]);
        }
    }

    /// Encode trace buffer contents, calling `emit` for each fragment chunk.
    pub fn encode_trace<const N: usize, F>(&mut self, buffer: &TraceBuffer<N>, mut emit: F)
    where
        F: FnMut(&[u8]),
    {
        // Collect all trace data
        let mut data = [0u8; 1024]; // temp buffer for trace data
        let mut data_len = 0usize;
        for frame in buffer.iter_frames() {
            let frame_bytes = frame.as_bytes();
            if data_len + frame_bytes.len() <= data.len() {
                data[data_len..data_len + frame_bytes.len()].copy_from_slice(frame_bytes);
                data_len += frame_bytes.len();
            }
        }

        if data_len == 0 {
            return;
        }

        // Fragment into chunks: 8 bytes for byte_offset prefix, rest is data
        let max_data_per_chunk = MAX_PAYLOAD_SIZE - 8;
        let mut offset = 0usize;

        while offset < data_len {
            let chunk_data_len = (data_len - offset).min(max_data_per_chunk);
            let mut payload = [0u8; MAX_PAYLOAD_SIZE];

            // Write byte_offset as u64 LE
            let offset_bytes = (offset as u64).to_le_bytes();
            payload[..8].copy_from_slice(&offset_bytes);
            payload[8..8 + chunk_data_len].copy_from_slice(&data[offset..offset + chunk_data_len]);

            let is_last = offset + chunk_data_len >= data_len;
            let mut out = [0u8; 256];
            let n = self.encode(
                ChunkType::TraceFragment,
                &payload[..8 + chunk_data_len],
                is_last,
                &mut out,
            );
            emit(&out[..n]);
            offset += chunk_data_len;
        }
    }

    /// Encode a heartbeat chunk.
    pub fn encode_heartbeat<F: FnMut(&[u8])>(
        &mut self,
        uptime_ticks: u64,
        free_stack_bytes: u32,
        metrics_count: u32,
        frames_lost: u32,
        device_key: u32,
        mut emit: F,
    ) {
        let mut payload = [0u8; 24];
        payload[0..8].copy_from_slice(&uptime_ticks.to_le_bytes());
        payload[8..12].copy_from_slice(&free_stack_bytes.to_le_bytes());
        payload[12..16].copy_from_slice(&metrics_count.to_le_bytes());
        payload[16..20].copy_from_slice(&frames_lost.to_le_bytes());
        payload[20..24].copy_from_slice(&device_key.to_le_bytes());

        let mut out = [0u8; 256];
        let n = self.encode(ChunkType::Heartbeat, &payload, true, &mut out);
        emit(&out[..n]);
    }

    /// Encode a reboot reason chunk.
    pub fn encode_reboot_reason<F: FnMut(&[u8])>(
        &mut self,
        reason: u8,
        extra: u8,
        boot_sequence: u32,
        uptime_before_reboot: u32,
        mut emit: F,
    ) {
        let mut payload = [0u8; 10];
        payload[0] = reason;
        payload[1] = extra;
        payload[2..6].copy_from_slice(&boot_sequence.to_le_bytes());
        payload[6..10].copy_from_slice(&uptime_before_reboot.to_le_bytes());

        let mut out = [0u8; 256];
        let n = self.encode(ChunkType::RebootReason, &payload, true, &mut out);
        emit(&out[..n]);
    }

    /// Encode a device info chunk.
    pub fn encode_device_info<F: FnMut(&[u8])>(
        &mut self,
        device_id: &str,
        firmware_version: &str,
        build_id: u64,
        mut emit: F,
    ) {
        let mut payload = [0u8; MAX_PAYLOAD_SIZE];
        let mut pos = 0;

        let did_len = device_id.len().min(127);
        payload[pos] = did_len as u8;
        pos += 1;
        payload[pos..pos + did_len].copy_from_slice(&device_id.as_bytes()[..did_len]);
        pos += did_len;

        let fw_len = firmware_version.len().min(127);
        payload[pos] = fw_len as u8;
        pos += 1;
        payload[pos..pos + fw_len].copy_from_slice(&firmware_version.as_bytes()[..fw_len]);
        pos += fw_len;

        payload[pos..pos + 8].copy_from_slice(&build_id.to_le_bytes());
        pos += 8;

        let mut out = [0u8; 256];
        let n = self.encode(ChunkType::DeviceInfo, &payload[..pos], true, &mut out);
        emit(&out[..n]);
    }
}

/// CRC-16/CCITT-FALSE
pub fn crc16_ccitt(data: &[u8]) -> u16 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::decoder::ChunkDecoder;

    extern crate std;

    #[test]
    fn encode_decode_heartbeat_roundtrip() {
        let mut encoder = ChunkEncoder::new();
        let mut chunks = std::vec::Vec::new();

        encoder.encode_heartbeat(12345, 1024, 5, 0, 0, |chunk| {
            chunks.push(std::vec::Vec::from(chunk));
        });

        assert_eq!(chunks.len(), 1);
        let decoded = ChunkDecoder::decode(&chunks[0]).unwrap();
        assert_eq!(decoded.chunk_type, ChunkType::Heartbeat);
        assert!(decoded.is_last);
    }

    #[test]
    fn encode_decode_device_info_roundtrip() {
        let mut encoder = ChunkEncoder::new();
        let mut chunks = std::vec::Vec::new();

        encoder.encode_device_info("test-device", "1.0.0", 0xDEADBEEF, |chunk| {
            chunks.push(std::vec::Vec::from(chunk));
        });

        assert_eq!(chunks.len(), 1);
        let decoded = ChunkDecoder::decode(&chunks[0]).unwrap();
        assert_eq!(decoded.chunk_type, ChunkType::DeviceInfo);
        assert!(decoded.is_last);

        // Verify payload contents
        let p = &decoded.payload;
        let did_len = p[0] as usize;
        assert_eq!(&p[1..1 + did_len], b"test-device");
    }

    #[test]
    fn encode_decode_reboot_reason_roundtrip() {
        let mut encoder = ChunkEncoder::new();
        let mut chunks = std::vec::Vec::new();

        encoder.encode_reboot_reason(4, 0, 42, 100000, |chunk| {
            chunks.push(std::vec::Vec::from(chunk));
        });

        let decoded = ChunkDecoder::decode(&chunks[0]).unwrap();
        assert_eq!(decoded.chunk_type, ChunkType::RebootReason);
        assert_eq!(decoded.payload[0], 4); // HardFault
    }

    #[test]
    fn crc_mismatch_detected() {
        let mut encoder = ChunkEncoder::new();
        let mut out = [0u8; 256];
        let n = encoder.encode(ChunkType::Heartbeat, &[1, 2, 3], true, &mut out);

        // Corrupt a payload byte (index 8 is first payload byte)
        out[8] ^= 0xFF;

        let result = ChunkDecoder::decode(&out[..n]);
        assert!(matches!(
            result,
            Err(crate::chunks::types::DecodeError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn sequence_number_wraps() {
        let mut encoder = ChunkEncoder::new();
        encoder.sequence = 0xFFFE;

        let mut out = [0u8; 256];
        let _ = encoder.encode(ChunkType::Heartbeat, &[], true, &mut out);
        assert_eq!(encoder.sequence, 0xFFFF);

        let _ = encoder.encode(ChunkType::Heartbeat, &[], true, &mut out);
        assert_eq!(encoder.sequence, 0x0000);
    }

    #[test]
    fn encode_decode_fault_roundtrip() {
        let mut encoder = ChunkEncoder::new();
        let mut chunks = std::vec::Vec::new();

        let record = FaultRecord::default_for_test();
        encoder.encode_fault(&record, |chunk| {
            chunks.push(std::vec::Vec::from(chunk));
        });

        assert_eq!(chunks.len(), 1);
        let decoded = ChunkDecoder::decode(&chunks[0]).unwrap();
        assert_eq!(decoded.chunk_type, ChunkType::FaultRecord);
    }

    #[test]
    fn encode_decode_metrics_roundtrip() {
        use crate::metrics::{MetricEntry, MetricValue};

        let mut encoder = ChunkEncoder::new();
        let mut chunks = std::vec::Vec::new();

        let mut key = heapless::String::new();
        let _ = key.push_str("test_counter");
        let entries = [MetricEntry {
            key,
            value: MetricValue::Counter(42),
            timestamp_ticks: 1000,
        }];

        encoder.encode_metrics(entries.iter(), |chunk| {
            chunks.push(std::vec::Vec::from(chunk));
        });

        assert_eq!(chunks.len(), 1);
        let decoded = ChunkDecoder::decode(&chunks[0]).unwrap();
        assert_eq!(decoded.chunk_type, ChunkType::Metrics);
        assert_eq!(decoded.payload[0], 1); // entry count
    }

    #[test]
    fn oversized_payload_truncated() {
        let mut encoder = ChunkEncoder::new();
        let big_payload = [0xAA; 300];
        let mut out = [0u8; 256];
        let n = encoder.encode(ChunkType::Heartbeat, &big_payload, true, &mut out);
        // Should be limited to MAX_PAYLOAD_SIZE
        assert!(n <= 256);
    }
}
