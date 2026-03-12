//! Chunk wire format framing — sync on magic byte, validate CRC.
//!
//! Replicates the chunk wire format from ferrite-sdk for the std gateway.

/// Chunk magic byte.
pub const MAGIC: u8 = 0xEC;
/// Wire format version.
pub const VERSION: u8 = 1;
/// Header size in bytes.
pub const HEADER_SIZE: usize = 8;
/// CRC trailer size.
pub const CRC_SIZE: usize = 2;
/// Minimum valid chunk size (header + CRC, zero payload).
#[allow(dead_code)]
pub const MIN_CHUNK_SIZE: usize = HEADER_SIZE + CRC_SIZE;
/// Maximum chunk size.
pub const MAX_CHUNK_SIZE: usize = 256 + CRC_SIZE; // 258 theoretical max

/// CRC-16/CCITT-FALSE — matches ferrite-sdk and ferrite-server.
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

/// A decoded chunk with header fields and raw payload.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DecodedChunk {
    pub chunk_type: u8,
    pub flags: u8,
    pub payload_len: u16,
    pub sequence_id: u16,
    pub payload: Vec<u8>,
    /// The entire raw chunk bytes (header + payload + CRC).
    pub raw: Vec<u8>,
}

/// Streaming chunk framer — feed bytes and extract complete chunks.
///
/// Buffers incoming bytes and emits complete, CRC-validated chunks.
pub struct ChunkFramer {
    buf: Vec<u8>,
}

impl ChunkFramer {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(512),
        }
    }

    /// Feed raw bytes into the framer. Returns any complete chunks found.
    pub fn feed(&mut self, data: &[u8]) -> Vec<DecodedChunk> {
        self.buf.extend_from_slice(data);
        let mut chunks = Vec::new();

        loop {
            // Find magic byte
            let start = match self.buf.iter().position(|&b| b == MAGIC) {
                Some(pos) => pos,
                None => {
                    self.buf.clear();
                    break;
                }
            };

            // Discard bytes before magic
            if start > 0 {
                self.buf.drain(..start);
            }

            // Need at least header to read payload length
            if self.buf.len() < HEADER_SIZE {
                break;
            }

            // Validate version
            if self.buf[1] != VERSION {
                // Bad version — skip this magic byte and try next
                self.buf.drain(..1);
                continue;
            }

            let payload_len = u16::from_le_bytes([self.buf[4], self.buf[5]]) as usize;
            let total_len = HEADER_SIZE + payload_len + CRC_SIZE;

            if total_len > MAX_CHUNK_SIZE {
                // Invalid payload length — skip this magic byte
                self.buf.drain(..1);
                continue;
            }

            // Wait for complete chunk
            if self.buf.len() < total_len {
                break;
            }

            // Validate CRC
            let crc_offset = HEADER_SIZE + payload_len;
            let stored_crc =
                u16::from_le_bytes([self.buf[crc_offset], self.buf[crc_offset + 1]]);
            let computed_crc = crc16_ccitt(&self.buf[..crc_offset]);

            if stored_crc != computed_crc {
                // CRC mismatch — skip this magic byte
                self.buf.drain(..1);
                continue;
            }

            // Valid chunk
            let raw: Vec<u8> = self.buf[..total_len].to_vec();
            let chunk = DecodedChunk {
                chunk_type: self.buf[2],
                flags: self.buf[3],
                payload_len: payload_len as u16,
                sequence_id: u16::from_le_bytes([self.buf[6], self.buf[7]]),
                payload: self.buf[HEADER_SIZE..HEADER_SIZE + payload_len].to_vec(),
                raw,
            };

            chunks.push(chunk);
            self.buf.drain(..total_len);
        }

        chunks
    }
}

impl Default for ChunkFramer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid chunk with the given type and payload.
    fn make_chunk(chunk_type: u8, payload: &[u8]) -> Vec<u8> {
        let payload_len = payload.len() as u16;
        let mut buf = Vec::new();
        buf.push(MAGIC);
        buf.push(VERSION);
        buf.push(chunk_type);
        buf.push(0x00); // flags
        buf.extend_from_slice(&payload_len.to_le_bytes());
        buf.extend_from_slice(&[0x00, 0x00]); // sequence_id
        buf.extend_from_slice(payload);
        let crc = crc16_ccitt(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    #[test]
    fn frame_single_chunk() {
        let raw = make_chunk(0x01, &[1, 2, 3, 4]);
        let mut framer = ChunkFramer::new();
        let chunks = framer.feed(&raw);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_type, 0x01);
        assert_eq!(chunks[0].payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn frame_two_chunks_concatenated() {
        let mut data = make_chunk(0x01, &[10]);
        data.extend_from_slice(&make_chunk(0x02, &[20, 30]));
        let mut framer = ChunkFramer::new();
        let chunks = framer.feed(&data);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].chunk_type, 0x01);
        assert_eq!(chunks[1].chunk_type, 0x02);
    }

    #[test]
    fn frame_with_garbage_prefix() {
        let mut data = vec![0xFF, 0xAA, 0xBB];
        data.extend_from_slice(&make_chunk(0x03, &[7, 8, 9]));
        let mut framer = ChunkFramer::new();
        let chunks = framer.feed(&data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_type, 0x03);
    }

    #[test]
    fn frame_split_across_feeds() {
        let raw = make_chunk(0x01, &[1, 2, 3]);
        let (part1, part2) = raw.split_at(5);

        let mut framer = ChunkFramer::new();
        let chunks = framer.feed(part1);
        assert_eq!(chunks.len(), 0);
        let chunks = framer.feed(part2);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].payload, vec![1, 2, 3]);
    }

    #[test]
    fn bad_crc_skipped() {
        let mut raw = make_chunk(0x01, &[1, 2, 3]);
        // Corrupt the CRC
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;

        let mut framer = ChunkFramer::new();
        let chunks = framer.feed(&raw);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn crc16_known_value() {
        // Empty input
        assert_eq!(crc16_ccitt(&[]), 0xFFFF);
        // "123456789" should produce 0x29B1
        let data = b"123456789";
        assert_eq!(crc16_ccitt(data), 0x29B1);
    }
}
