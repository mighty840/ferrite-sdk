use crate::chunks::types::{ChunkHeader, ChunkType, DecodeError, DecodedChunk};
use crate::chunks::encoder::crc16_ccitt;

/// Decodes raw bytes into a chunk, validating magic, version, and CRC.
pub struct ChunkDecoder;

impl ChunkDecoder {
    pub fn decode(bytes: &[u8]) -> Result<DecodedChunk, DecodeError> {
        // Minimum size: 8 (header) + 2 (CRC) = 10
        if bytes.len() < 10 {
            return Err(DecodeError::TooShort);
        }

        // Validate magic
        if bytes[0] != ChunkHeader::MAGIC {
            return Err(DecodeError::BadMagic);
        }

        // Validate version
        if bytes[1] != ChunkHeader::VERSION {
            return Err(DecodeError::BadVersion);
        }

        // Parse chunk type
        let chunk_type = ChunkType::from_u8(bytes[2])
            .ok_or(DecodeError::UnknownType(bytes[2]))?;

        let flags = bytes[3];
        let payload_len = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        let sequence_id = u16::from_le_bytes([bytes[6], bytes[7]]);

        // Check we have enough bytes for header + payload + CRC
        let total_expected = 8 + payload_len + 2;
        if bytes.len() < total_expected {
            return Err(DecodeError::PayloadTruncated);
        }

        // Validate CRC
        let crc_offset = 8 + payload_len;
        let expected_crc = u16::from_le_bytes([bytes[crc_offset], bytes[crc_offset + 1]]);
        let computed_crc = crc16_ccitt(&bytes[..crc_offset]);

        if expected_crc != computed_crc {
            return Err(DecodeError::CrcMismatch {
                expected: expected_crc,
                got: computed_crc,
            });
        }

        // Extract payload
        let mut payload = heapless::Vec::new();
        for &b in &bytes[8..8 + payload_len] {
            let _ = payload.push(b);
        }

        Ok(DecodedChunk {
            chunk_type,
            sequence_id,
            is_last: flags & 0x01 != 0,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    #[test]
    fn decode_too_short() {
        let result = ChunkDecoder::decode(&[0xEC, 1, 0x01]);
        assert_eq!(result, Err(DecodeError::TooShort));
    }

    #[test]
    fn decode_bad_magic() {
        let result = ChunkDecoder::decode(&[0x00, 1, 0x01, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(result, Err(DecodeError::BadMagic));
    }

    #[test]
    fn decode_bad_version() {
        let result = ChunkDecoder::decode(&[0xEC, 99, 0x01, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(result, Err(DecodeError::BadVersion));
    }

    #[test]
    fn decode_unknown_type() {
        let result = ChunkDecoder::decode(&[0xEC, 1, 0xFF, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(result, Err(DecodeError::UnknownType(0xFF)));
    }
}
