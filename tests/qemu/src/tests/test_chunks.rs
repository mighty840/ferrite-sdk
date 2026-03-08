use ferrite_sdk::chunks::encoder::ChunkEncoder;
use ferrite_sdk::chunks::decoder::ChunkDecoder;
use ferrite_sdk::chunks::types::ChunkType;
use ferrite_sdk::fault::FaultRecord;
use ferrite_sdk::metrics::{MetricEntry, MetricValue};

pub fn encode_decode_metrics() -> Result<(), &'static str> {
    let mut encoder = ChunkEncoder::new();
    let mut chunk_buf = [0u8; 256];
    let mut chunk_len = 0usize;

    let mut key = heapless::String::new();
    key.push_str("test_counter").map_err(|_| "key push failed")?;
    let entries = [MetricEntry {
        key,
        value: MetricValue::Counter(42),
        timestamp_ticks: 1000,
    }];

    encoder.encode_metrics(entries.iter(), |chunk| {
        chunk_buf[..chunk.len()].copy_from_slice(chunk);
        chunk_len = chunk.len();
    });

    if chunk_len == 0 {
        return Err("no chunk produced");
    }

    let decoded = ChunkDecoder::decode(&chunk_buf[..chunk_len])
        .map_err(|_| "decode failed")?;

    if decoded.chunk_type != ChunkType::Metrics {
        return Err("wrong chunk type");
    }
    if decoded.payload[0] != 1 {
        return Err("expected 1 entry in payload");
    }
    Ok(())
}

pub fn encode_decode_fault() -> Result<(), &'static str> {
    let mut encoder = ChunkEncoder::new();
    let mut chunk_buf = [0u8; 256];
    let mut chunk_len = 0usize;

    let record = FaultRecord::zeroed();
    encoder.encode_fault(&record, |chunk| {
        chunk_buf[..chunk.len()].copy_from_slice(chunk);
        chunk_len = chunk.len();
    });

    if chunk_len == 0 {
        return Err("no chunk produced");
    }

    let decoded = ChunkDecoder::decode(&chunk_buf[..chunk_len])
        .map_err(|_| "decode failed")?;

    if decoded.chunk_type != ChunkType::FaultRecord {
        return Err("wrong chunk type");
    }
    Ok(())
}

pub fn crc_mismatch_detected() -> Result<(), &'static str> {
    let mut encoder = ChunkEncoder::new();
    let mut out = [0u8; 256];
    let n = encoder.encode(ChunkType::Heartbeat, &[1, 2, 3], true, &mut out);

    // Corrupt a payload byte
    out[8] ^= 0xFF;

    match ChunkDecoder::decode(&out[..n]) {
        Err(ferrite_sdk::chunks::types::DecodeError::CrcMismatch { .. }) => Ok(()),
        Ok(_) => Err("expected CRC mismatch error"),
        Err(_) => Err("expected CRC mismatch, got different error"),
    }
}
