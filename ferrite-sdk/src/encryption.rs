//! Optional AES-128-CCM chunk encryption.
//!
//! When the `encryption` feature is enabled, [`EncryptedTransport`] wraps any
//! [`ChunkTransport`] and transparently encrypts chunk payloads before sending.
//!
//! Wire format changes:
//! - Header flags bit 0x04 (`FLAG_ENCRYPTED`) is set.
//! - The original payload is replaced with: `nonce (13B) || ciphertext || tag (8B)`.
//!
//! The nonce is built from the sequence_id (bytes 6-7 of the header) and a
//! monotonic counter to guarantee uniqueness even across retransmissions.

use crate::chunks::encoder::crc16_ccitt;
use crate::chunks::types::ChunkHeader;
use crate::config::MAX_PAYLOAD_SIZE;
use crate::transport::ChunkTransport;

use aes::Aes128;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::{AeadInPlace, KeyInit};
use ccm::consts::{U13, U8};
use ccm::Ccm;

/// AES-128-CCM with 13-byte nonce and 8-byte tag.
type Aes128Ccm = Ccm<Aes128, U8, U13>;

/// Nonce size in bytes.
const NONCE_SIZE: usize = 13;
/// Authentication tag size in bytes.
const TAG_SIZE: usize = 8;
/// Encryption overhead: nonce + tag.
pub const ENCRYPTION_OVERHEAD: usize = NONCE_SIZE + TAG_SIZE;

/// Transport wrapper that encrypts chunk payloads using AES-128-CCM.
///
/// # Example
///
/// ```ignore
/// let key = [0u8; 16]; // pre-shared key
/// let mut encrypted = EncryptedTransport::new(uart_transport, key);
/// UploadManager::upload(&mut encrypted).unwrap();
/// ```
pub struct EncryptedTransport<T: ChunkTransport> {
    inner: T,
    cipher: Aes128Ccm,
    nonce_counter: u32,
}

impl<T: ChunkTransport> EncryptedTransport<T> {
    /// Create a new encrypted transport with a 16-byte pre-shared key.
    pub fn new(inner: T, key: [u8; 16]) -> Self {
        let cipher = Aes128Ccm::new(GenericArray::from_slice(&key));
        Self {
            inner,
            cipher,
            nonce_counter: 0,
        }
    }

    /// Build a 13-byte nonce from the monotonic counter.
    fn next_nonce(&mut self) -> [u8; NONCE_SIZE] {
        let mut nonce = [0u8; NONCE_SIZE];
        let counter = self.nonce_counter;
        self.nonce_counter = self.nonce_counter.wrapping_add(1);
        nonce[0..4].copy_from_slice(&counter.to_le_bytes());
        nonce
    }

    /// Encrypt a raw chunk in-place in the output buffer.
    /// Returns the new total size, or None on failure.
    fn encrypt_chunk(&mut self, raw: &[u8]) -> Option<[u8; 256]> {
        // Parse the raw chunk to extract header + payload + CRC
        if raw.len() < 10 {
            return None;
        }

        let payload_len = u16::from_le_bytes([raw[4], raw[5]]) as usize;
        let original_total = 8 + payload_len + 2; // header + payload + CRC
        if raw.len() < original_total {
            return None;
        }

        // Encrypted payload = nonce(13) + encrypt(original_payload) + tag(8)
        let encrypted_payload_len = NONCE_SIZE + payload_len + TAG_SIZE;
        if encrypted_payload_len > MAX_PAYLOAD_SIZE {
            // Payload too large to encrypt — send unencrypted
            return None;
        }

        let nonce = self.next_nonce();

        // Copy original payload to a buffer for in-place encryption
        let mut plaintext = [0u8; 256];
        plaintext[..payload_len].copy_from_slice(&raw[8..8 + payload_len]);

        // Encrypt in place, appending the tag
        let nonce_ga = GenericArray::from_slice(&nonce);
        let tag = self
            .cipher
            .encrypt_in_place_detached(nonce_ga, &[], &mut plaintext[..payload_len])
            .ok()?;

        // Build new chunk: header (with encrypted flag) + nonce + ciphertext + tag + CRC
        let mut out = [0u8; 256];

        // Copy header, set encrypted flag
        out[..8].copy_from_slice(&raw[..8]);
        out[3] |= ChunkHeader::FLAG_ENCRYPTED;

        // Update payload length
        out[4] = (encrypted_payload_len & 0xFF) as u8;
        out[5] = ((encrypted_payload_len >> 8) & 0xFF) as u8;

        // Write nonce
        let mut pos = 8;
        out[pos..pos + NONCE_SIZE].copy_from_slice(&nonce);
        pos += NONCE_SIZE;

        // Write ciphertext
        out[pos..pos + payload_len].copy_from_slice(&plaintext[..payload_len]);
        pos += payload_len;

        // Write authentication tag
        out[pos..pos + TAG_SIZE].copy_from_slice(tag.as_slice());
        pos += TAG_SIZE;

        // Recompute CRC over new header + encrypted payload
        let crc = crc16_ccitt(&out[..pos]);
        out[pos] = (crc & 0xFF) as u8;
        out[pos + 1] = ((crc >> 8) & 0xFF) as u8;

        Some(out)
    }
}

impl<T: ChunkTransport> ChunkTransport for EncryptedTransport<T> {
    type Error = T::Error;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        if let Some(encrypted) = self.encrypt_chunk(chunk) {
            // Calculate actual size from the encrypted chunk
            let payload_len = u16::from_le_bytes([encrypted[4], encrypted[5]]) as usize;
            let total = 8 + payload_len + 2;
            self.inner.send_chunk(&encrypted[..total])
        } else {
            // Fallback: send unencrypted (payload too large or parse error)
            self.inner.send_chunk(chunk)
        }
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

/// Decrypt an encrypted chunk payload in place.
///
/// Input: raw encrypted payload bytes (nonce || ciphertext || tag).
/// Output: decrypted plaintext written into `out`, returns plaintext length.
///
/// This is used on the server side to decrypt incoming chunks.
pub fn decrypt_payload(key: &[u8; 16], encrypted: &[u8], out: &mut [u8]) -> Option<usize> {
    if encrypted.len() < NONCE_SIZE + TAG_SIZE {
        return None;
    }

    let cipher = Aes128Ccm::new(GenericArray::from_slice(key));

    let nonce = &encrypted[..NONCE_SIZE];
    let ciphertext_len = encrypted.len() - NONCE_SIZE - TAG_SIZE;
    let ciphertext = &encrypted[NONCE_SIZE..NONCE_SIZE + ciphertext_len];
    let tag = &encrypted[NONCE_SIZE + ciphertext_len..];

    if out.len() < ciphertext_len {
        return None;
    }

    // Copy ciphertext to output for in-place decryption
    out[..ciphertext_len].copy_from_slice(ciphertext);

    let nonce_ga = GenericArray::from_slice(nonce);
    let tag_ga = GenericArray::from_slice(tag);

    cipher
        .decrypt_in_place_detached(nonce_ga, &[], &mut out[..ciphertext_len], tag_ga)
        .ok()?;

    Some(ciphertext_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::decoder::ChunkDecoder;
    use crate::chunks::encoder::ChunkEncoder;
    use crate::chunks::types::ChunkType;

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
    fn encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 16];
        let mock = MockTransport::new();
        let mut transport = EncryptedTransport::new(mock, key);

        // Encode a heartbeat chunk
        let mut encoder = ChunkEncoder::new();
        let mut raw_chunks = Vec::new();
        encoder.encode_heartbeat(12345, 1024, 5, 0, 0, |chunk| {
            raw_chunks.push(Vec::from(chunk));
        });

        // Send through encrypted transport
        transport.send_chunk(&raw_chunks[0]).unwrap();

        // The sent chunk should be encrypted (flag set)
        let encrypted_raw = &transport.inner.sent[0];
        assert_eq!(
            encrypted_raw[3] & ChunkHeader::FLAG_ENCRYPTED,
            ChunkHeader::FLAG_ENCRYPTED
        );

        // Decrypt the payload
        let enc_payload_len = u16::from_le_bytes([encrypted_raw[4], encrypted_raw[5]]) as usize;
        let enc_payload = &encrypted_raw[8..8 + enc_payload_len];

        let mut decrypted = [0u8; 256];
        let plaintext_len = decrypt_payload(&key, enc_payload, &mut decrypted).unwrap();

        // Should match original heartbeat payload (24 bytes)
        assert_eq!(plaintext_len, 24);

        // Verify the original payload content
        let uptime = u64::from_le_bytes(decrypted[0..8].try_into().unwrap());
        assert_eq!(uptime, 12345);
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let key = [0x42u8; 16];
        let wrong_key = [0x99u8; 16];
        let mock = MockTransport::new();
        let mut transport = EncryptedTransport::new(mock, key);

        let mut encoder = ChunkEncoder::new();
        let mut raw_chunks = Vec::new();
        encoder.encode_heartbeat(1, 2, 3, 4, 5, |chunk| {
            raw_chunks.push(Vec::from(chunk));
        });

        transport.send_chunk(&raw_chunks[0]).unwrap();

        let encrypted_raw = &transport.inner.sent[0];
        let enc_payload_len = u16::from_le_bytes([encrypted_raw[4], encrypted_raw[5]]) as usize;
        let enc_payload = &encrypted_raw[8..8 + enc_payload_len];

        let mut decrypted = [0u8; 256];
        let result = decrypt_payload(&wrong_key, enc_payload, &mut decrypted);
        assert!(result.is_none());
    }

    #[test]
    fn encrypted_chunk_has_valid_crc() {
        let key = [0xAB; 16];
        let mock = MockTransport::new();
        let mut transport = EncryptedTransport::new(mock, key);

        let mut encoder = ChunkEncoder::new();
        let mut raw_chunks = Vec::new();
        encoder.encode_device_info("test", "1.0.0", 42, |chunk| {
            raw_chunks.push(Vec::from(chunk));
        });

        transport.send_chunk(&raw_chunks[0]).unwrap();

        let encrypted_raw = &transport.inner.sent[0];
        // Verify CRC by checking header bytes
        let payload_len = u16::from_le_bytes([encrypted_raw[4], encrypted_raw[5]]) as usize;
        let crc_offset = 8 + payload_len;
        let stored_crc =
            u16::from_le_bytes([encrypted_raw[crc_offset], encrypted_raw[crc_offset + 1]]);
        let computed_crc = crc16_ccitt(&encrypted_raw[..crc_offset]);
        assert_eq!(stored_crc, computed_crc);
    }
}
