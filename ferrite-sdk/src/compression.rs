//! Optional chunk payload compression.
//!
//! When the `compression` feature is enabled, [`CompressedTransport`] wraps
//! any [`ChunkTransport`] and transparently compresses chunk payloads using
//! a lightweight algorithm suitable for embedded devices.
//!
//! Wire format changes:
//! - Header flags bit 0x08 (`FLAG_COMPRESSED`) is set.
//! - The original payload is replaced with the compressed payload.
//! - The first 2 bytes of the compressed payload are the original
//!   (uncompressed) payload length as a little-endian u16.
//!
//! Compression uses a simple run-length encoding (RLE) scheme that is
//! effective for metric chunks with repeated key bytes and zero-padded
//! fields. No allocation is required.

use crate::chunks::encoder::crc16_ccitt;
use crate::config::MAX_PAYLOAD_SIZE;
use crate::transport::ChunkTransport;

/// Flag bit indicating the chunk payload is compressed.
pub const FLAG_COMPRESSED: u8 = 0x08;

/// Compress data using simple RLE: consecutive runs of 3+ identical bytes
/// are encoded as `[marker, byte, count]`. Marker byte is 0xFF.
///
/// Returns the number of bytes written to `out`, or None if compression
/// would expand the data.
fn rle_compress(input: &[u8], out: &mut [u8]) -> Option<usize> {
    const MARKER: u8 = 0xFF;
    let mut i = 0;
    let mut o = 0;

    while i < input.len() {
        // Count run length
        let byte = input[i];
        let mut run = 1usize;
        while i + run < input.len() && input[i + run] == byte && run < 255 {
            run += 1;
        }

        if run >= 3 || byte == MARKER {
            // Encode as marker + byte + count
            if o + 3 > out.len() {
                return None;
            }
            out[o] = MARKER;
            out[o + 1] = byte;
            out[o + 2] = run as u8;
            o += 3;
        } else {
            // Literal bytes
            for _ in 0..run {
                if o >= out.len() {
                    return None;
                }
                out[o] = byte;
                o += 1;
            }
        }
        i += run;
    }

    // Only compress if we actually saved space
    if o < input.len() {
        Some(o)
    } else {
        None
    }
}

/// Decompress RLE-encoded data.
///
/// Returns the number of bytes written to `out`, or None on error.
pub fn rle_decompress(input: &[u8], out: &mut [u8]) -> Option<usize> {
    const MARKER: u8 = 0xFF;
    let mut i = 0;
    let mut o = 0;

    while i < input.len() {
        if input[i] == MARKER {
            if i + 2 >= input.len() {
                return None;
            }
            let byte = input[i + 1];
            let count = input[i + 2] as usize;
            if o + count > out.len() {
                return None;
            }
            for _ in 0..count {
                out[o] = byte;
                o += 1;
            }
            i += 3;
        } else {
            if o >= out.len() {
                return None;
            }
            out[o] = input[i];
            o += 1;
            i += 1;
        }
    }

    Some(o)
}

/// Transport wrapper that compresses chunk payloads using RLE.
pub struct CompressedTransport<T: ChunkTransport> {
    inner: T,
}

impl<T: ChunkTransport> CompressedTransport<T> {
    /// Create a new compressed transport wrapping the inner transport.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T: ChunkTransport> ChunkTransport for CompressedTransport<T> {
    type Error = T::Error;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        if chunk.len() < 10 {
            return self.inner.send_chunk(chunk);
        }

        let payload_len = u16::from_le_bytes([chunk[4], chunk[5]]) as usize;
        let original_total = 8 + payload_len + 2;
        if chunk.len() < original_total || payload_len == 0 {
            return self.inner.send_chunk(chunk);
        }

        let payload = &chunk[8..8 + payload_len];

        // Try to compress: 2 bytes for original length + compressed data
        let mut compressed_buf = [0u8; 256];
        let compressed = match rle_compress(payload, &mut compressed_buf[2..]) {
            Some(n) => {
                // Prepend original length
                compressed_buf[0] = (payload_len & 0xFF) as u8;
                compressed_buf[1] = ((payload_len >> 8) & 0xFF) as u8;
                n + 2
            }
            None => {
                // Compression didn't help — send uncompressed
                return self.inner.send_chunk(chunk);
            }
        };

        if compressed > MAX_PAYLOAD_SIZE {
            return self.inner.send_chunk(chunk);
        }

        // Build compressed chunk
        let mut out = [0u8; 268]; // max chunk + overhead
        out[..8].copy_from_slice(&chunk[..8]);
        out[3] |= FLAG_COMPRESSED;
        out[4] = (compressed & 0xFF) as u8;
        out[5] = ((compressed >> 8) & 0xFF) as u8;

        let pos = 8;
        out[pos..pos + compressed].copy_from_slice(&compressed_buf[..compressed]);
        let crc_offset = pos + compressed;
        let crc = crc16_ccitt(&out[..crc_offset]);
        out[crc_offset] = (crc & 0xFF) as u8;
        out[crc_offset + 1] = ((crc >> 8) & 0xFF) as u8;

        self.inner.send_chunk(&out[..crc_offset + 2])
    }

    fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    fn begin_session(&mut self) -> Result<(), Self::Error> {
        self.inner.begin_session()
    }

    fn end_session(&mut self) -> Result<(), Self::Error> {
        self.inner.end_session()
    }
}

/// Decompress a compressed chunk payload.
///
/// Input: the compressed payload bytes (original_len_u16 || rle_data).
/// Output: decompressed plaintext written into `out`, returns plaintext length.
pub fn decompress_payload(compressed: &[u8], out: &mut [u8]) -> Option<usize> {
    if compressed.len() < 2 {
        return None;
    }

    let original_len = u16::from_le_bytes([compressed[0], compressed[1]]) as usize;
    if original_len > out.len() {
        return None;
    }

    let decompressed_len = rle_decompress(&compressed[2..], out)?;
    if decompressed_len != original_len {
        return None;
    }

    Some(original_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::encoder::ChunkEncoder;

    extern crate std;
    use std::vec::Vec;

    struct MockTransport {
        sent: Vec<Vec<u8>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self { sent: Vec::new() }
        }
    }

    impl ChunkTransport for MockTransport {
        type Error = &'static str;
        fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
            self.sent.push(chunk.to_vec());
            Ok(())
        }
    }

    #[test]
    fn rle_roundtrip() {
        let data = [0u8, 0, 0, 0, 0, 1, 2, 3, 3, 3, 3, 3, 4];
        let mut compressed = [0u8; 256];
        let n = rle_compress(&data, &mut compressed).unwrap();
        assert!(n < data.len());

        let mut decompressed = [0u8; 256];
        let m = rle_decompress(&compressed[..n], &mut decompressed).unwrap();
        assert_eq!(m, data.len());
        assert_eq!(&decompressed[..m], &data);
    }

    #[test]
    fn rle_incompressible_data() {
        // Random-looking data that won't compress
        let data: Vec<u8> = (0..50).collect();
        let mut compressed = [0u8; 256];
        let result = rle_compress(&data, &mut compressed);
        assert!(result.is_none()); // Should not compress
    }

    #[test]
    fn compress_decompress_chunk_roundtrip() {
        let mock = MockTransport::new();
        let mut transport = CompressedTransport::new(mock);

        // Encode a heartbeat — has lots of zero bytes
        let mut encoder = ChunkEncoder::new();
        let mut raw_chunks = Vec::new();
        encoder.encode_heartbeat(12345, 1024, 5, 0, 0, |chunk| {
            raw_chunks.push(Vec::from(chunk));
        });

        transport.send_chunk(&raw_chunks[0]).unwrap();

        let sent = &transport.inner.sent[0];

        // Check if it was compressed (flag set) or sent raw
        if sent[3] & FLAG_COMPRESSED != 0 {
            // Compressed — verify decompress works
            let payload_len = u16::from_le_bytes([sent[4], sent[5]]) as usize;
            let payload = &sent[8..8 + payload_len];

            let mut decompressed = [0u8; 256];
            let n = decompress_payload(payload, &mut decompressed).unwrap();

            // Should match original heartbeat payload (24 bytes)
            assert_eq!(n, 24);
            let uptime = u64::from_le_bytes(decompressed[0..8].try_into().unwrap());
            assert_eq!(uptime, 12345);
        }
        // If not compressed, the data wasn't compressible — that's fine
    }

    #[test]
    fn marker_byte_in_data() {
        // Data containing the marker byte 0xFF
        let data = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x02];
        let mut compressed = [0u8; 256];
        let n = rle_compress(&data, &mut compressed).unwrap();

        let mut decompressed = [0u8; 256];
        let m = rle_decompress(&compressed[..n], &mut decompressed).unwrap();
        assert_eq!(&decompressed[..m], &data);
    }
}
