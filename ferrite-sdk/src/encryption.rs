//! Optional AES-128-CCM chunk encryption and per-device authentication.
//!
//! When the `encryption` feature is enabled, [`EncryptedTransport`] wraps any
//! [`ChunkTransport`] (or [`AsyncChunkTransport`](crate::transport::AsyncChunkTransport))
//! and seals every chunk before sending. CCM is authenticated encryption, so
//! the server only accepts a chunk that was sealed with the key it expects.
//!
//! Two kinds of key:
//! - **Per-device credential** ([`DeviceCredential`], from
//!   `POST /devices/:id/credential`): the server picks the key by the key ID
//!   in the nonce and stores the chunk under the device that key belongs to.
//!   This is what proves who sent a report.
//! - **Shared key** ([`EncryptedTransport::new`], the server's
//!   `CHUNK_ENCRYPTION_KEY`): confidentiality only; every device holding the
//!   key looks the same to the server.
//!
//! Wire format:
//! - Header flags bit 0x04 (`FLAG_ENCRYPTED`) is set.
//! - The payload is replaced with `nonce (13B) || ciphertext || tag (8B)`.
//! - Nonce: `counter (u32 LE) || boot_epoch (u32 LE) || key_id (u32 LE) || version (u8)`.
//!   Version 0 is the shared key (older SDKs send zeros after the counter);
//!   version 1 is a device credential.
//!
//! A nonce must never repeat under the same key. The counter restarts at
//! every boot, so pass a `boot_epoch` that differs per boot: a random value
//! from a hardware RNG, or a boot counter persisted in flash. The server also
//! uses `(boot_epoch, counter)` to reject replayed chunks.

use crate::chunks::encoder::crc16_ccitt;
use crate::chunks::types::ChunkHeader;
use crate::config::MAX_CHUNK_SIZE;
use crate::transport::ChunkTransport;

use aes::Aes128;
use ccm::aead::generic_array::GenericArray;
use ccm::aead::{AeadInPlace, KeyInit};
use ccm::consts::{U13, U8};
use ccm::Ccm;

/// AES-128-CCM with 13-byte nonce and 8-byte tag.
type Aes128Ccm = Ccm<Aes128, U8, U13>;

/// Nonce size in bytes.
pub const NONCE_SIZE: usize = 13;
/// Authentication tag size in bytes.
const TAG_SIZE: usize = 8;
/// Encryption overhead: nonce + tag.
pub const ENCRYPTION_OVERHEAD: usize = NONCE_SIZE + TAG_SIZE;
/// Header (8) + CRC (2) around every payload.
const FRAMING: usize = 10;

/// Nonce version byte for chunks sealed with the shared key.
pub const NONCE_VERSION_SHARED: u8 = 0;
/// Nonce version byte for chunks sealed with a per-device credential.
pub const NONCE_VERSION_CREDENTIAL: u8 = 1;

/// A device's own key, issued by the server (`POST /devices/:id/credential`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DeviceCredential {
    /// Public identifier the server looks the key up by. Never 0.
    pub key_id: u32,
    /// AES-128 key. Keep it secret: it is what the device's reports are trusted on.
    pub key: [u8; 16],
}

impl core::fmt::Debug for DeviceCredential {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceCredential")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

/// The fields of a 13-byte chunk nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonceFields {
    pub counter: u32,
    pub boot_epoch: u32,
    pub key_id: u32,
    pub version: u8,
}

impl NonceFields {
    /// Split a nonce into its fields; `None` unless it is exactly 13 bytes.
    pub fn parse(nonce: &[u8]) -> Option<Self> {
        if nonce.len() != NONCE_SIZE {
            return None;
        }
        let word =
            |i: usize| u32::from_le_bytes([nonce[i], nonce[i + 1], nonce[i + 2], nonce[i + 3]]);
        Some(Self {
            counter: word(0),
            boot_epoch: word(4),
            key_id: word(8),
            version: nonce[12],
        })
    }

    fn to_bytes(self) -> [u8; NONCE_SIZE] {
        let mut nonce = [0u8; NONCE_SIZE];
        nonce[0..4].copy_from_slice(&self.counter.to_le_bytes());
        nonce[4..8].copy_from_slice(&self.boot_epoch.to_le_bytes());
        nonce[8..12].copy_from_slice(&self.key_id.to_le_bytes());
        nonce[12] = self.version;
        nonce
    }
}

/// Seals raw wire-format chunks with AES-128-CCM. Used by
/// [`EncryptedTransport`], and directly by hosts that build request bodies
/// themselves (e.g. `ferrite-agent`).
pub struct ChunkSealer {
    cipher: Aes128Ccm,
    counter: u32,
    boot_epoch: u32,
    key_id: u32,
    version: u8,
}

impl ChunkSealer {
    /// Seal with a per-device credential.
    pub fn for_device(credential: &DeviceCredential, boot_epoch: u32) -> Self {
        Self {
            cipher: Aes128Ccm::new(GenericArray::from_slice(&credential.key)),
            counter: 0,
            boot_epoch,
            key_id: credential.key_id,
            version: NONCE_VERSION_CREDENTIAL,
        }
    }

    /// Seal with a key shared by the whole fleet (`CHUNK_ENCRYPTION_KEY`).
    pub fn shared(key: [u8; 16], boot_epoch: u32) -> Self {
        Self {
            cipher: Aes128Ccm::new(GenericArray::from_slice(&key)),
            counter: 0,
            boot_epoch,
            key_id: 0,
            version: NONCE_VERSION_SHARED,
        }
    }

    /// True when sealing with a per-device credential.
    pub fn is_device_credential(&self) -> bool {
        self.version == NONCE_VERSION_CREDENTIAL
    }

    /// Seal one raw chunk (header + payload + CRC). Returns the sealed chunk
    /// and its length, or `None` if `raw` isn't a whole chunk or its payload
    /// is too large to seal within [`MAX_CHUNK_SIZE`].
    pub fn seal(&mut self, raw: &[u8]) -> Option<([u8; MAX_CHUNK_SIZE], usize)> {
        if raw.len() < FRAMING {
            return None;
        }
        let payload_len = u16::from_le_bytes([raw[4], raw[5]]) as usize;
        if raw.len() < FRAMING + payload_len {
            return None;
        }
        let sealed_len = NONCE_SIZE + payload_len + TAG_SIZE;
        if FRAMING + sealed_len > MAX_CHUNK_SIZE {
            return None;
        }

        let nonce = NonceFields {
            counter: self.counter,
            boot_epoch: self.boot_epoch,
            key_id: self.key_id,
            version: self.version,
        }
        .to_bytes();
        self.counter = self.counter.wrapping_add(1);

        let mut out = [0u8; MAX_CHUNK_SIZE];
        out[..8].copy_from_slice(&raw[..8]);
        out[3] |= ChunkHeader::FLAG_ENCRYPTED;
        out[4..6].copy_from_slice(&(sealed_len as u16).to_le_bytes());

        let mut pos = 8;
        out[pos..pos + NONCE_SIZE].copy_from_slice(&nonce);
        pos += NONCE_SIZE;
        out[pos..pos + payload_len].copy_from_slice(&raw[8..8 + payload_len]);
        let tag = self
            .cipher
            .encrypt_in_place_detached(
                GenericArray::from_slice(&nonce),
                &[],
                &mut out[pos..pos + payload_len],
            )
            .ok()?;
        pos += payload_len;
        out[pos..pos + TAG_SIZE].copy_from_slice(tag.as_slice());
        pos += TAG_SIZE;

        let crc = crc16_ccitt(&out[..pos]);
        out[pos..pos + 2].copy_from_slice(&crc.to_le_bytes());
        Some((out, pos + 2))
    }
}

/// Transport wrapper that seals every chunk with AES-128-CCM.
///
/// # Example
///
/// ```ignore
/// // Issued by the server for this device; boot_epoch from the hardware RNG.
/// let credential = DeviceCredential { key_id: 0x1234_5678, key: KEY };
/// let mut transport = EncryptedTransport::with_credential(uart, &credential, rng.next_u32());
/// UploadManager::upload(&mut transport).unwrap();
/// ```
pub struct EncryptedTransport<T> {
    inner: T,
    sealer: ChunkSealer,
}

impl<T> EncryptedTransport<T> {
    /// Authenticate every chunk with this device's credential.
    pub fn with_credential(inner: T, credential: &DeviceCredential, boot_epoch: u32) -> Self {
        Self {
            inner,
            sealer: ChunkSealer::for_device(credential, boot_epoch),
        }
    }

    /// Encrypt with a key shared by the fleet. Without a `boot_epoch` this
    /// repeats nonces after every reboot; prefer [`Self::with_shared_key`].
    pub fn new(inner: T, key: [u8; 16]) -> Self {
        Self::with_shared_key(inner, key, 0)
    }

    /// Encrypt with a key shared by the fleet (`CHUNK_ENCRYPTION_KEY`).
    pub fn with_shared_key(inner: T, key: [u8; 16], boot_epoch: u32) -> Self {
        Self {
            inner,
            sealer: ChunkSealer::shared(key, boot_epoch),
        }
    }

    /// The wrapped transport.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// What to send for `chunk`: the sealed chunk, the chunk itself (shared
    /// key, too large to seal), or nothing (a device credential never falls
    /// back to cleartext; the encoder keeps payloads small enough to seal).
    fn prepare<'a>(
        &mut self,
        chunk: &'a [u8],
        buf: &'a mut [u8; MAX_CHUNK_SIZE],
    ) -> Option<&'a [u8]> {
        match self.sealer.seal(chunk) {
            Some((sealed, len)) => {
                *buf = sealed;
                Some(&buf[..len])
            }
            None if self.sealer.is_device_credential() => None,
            None => Some(chunk),
        }
    }
}

impl<T: ChunkTransport> ChunkTransport for EncryptedTransport<T> {
    type Error = T::Error;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        let mut buf = [0u8; MAX_CHUNK_SIZE];
        match self.prepare(chunk, &mut buf) {
            Some(bytes) => self.inner.send_chunk(bytes),
            None => Ok(()),
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

#[cfg(feature = "embassy")]
impl<T: crate::transport::AsyncChunkTransport> crate::transport::AsyncChunkTransport
    for EncryptedTransport<T>
{
    type Error = T::Error;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        let mut buf = [0u8; MAX_CHUNK_SIZE];
        match self.prepare(chunk, &mut buf) {
            Some(bytes) => self.inner.send_chunk(bytes).await,
            None => Ok(()),
        }
    }

    fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    async fn begin_session(&mut self) -> Result<(), Self::Error> {
        self.inner.begin_session().await
    }

    async fn end_session(&mut self) -> Result<(), Self::Error> {
        self.inner.end_session().await
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
        let encrypted_raw = &transport.inner().sent[0];
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

        let encrypted_raw = &transport.inner().sent[0];
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

        let encrypted_raw = &transport.inner().sent[0];
        // Verify CRC by checking header bytes
        let payload_len = u16::from_le_bytes([encrypted_raw[4], encrypted_raw[5]]) as usize;
        let crc_offset = 8 + payload_len;
        let stored_crc =
            u16::from_le_bytes([encrypted_raw[crc_offset], encrypted_raw[crc_offset + 1]]);
        let computed_crc = crc16_ccitt(&encrypted_raw[..crc_offset]);
        assert_eq!(stored_crc, computed_crc);
    }

    fn credential() -> DeviceCredential {
        DeviceCredential {
            key_id: 0xA1B2_C3D4,
            key: [0x5Au8; 16],
        }
    }

    fn sealed_payload(chunk: &[u8]) -> &[u8] {
        let len = u16::from_le_bytes([chunk[4], chunk[5]]) as usize;
        &chunk[8..8 + len]
    }

    #[test]
    fn credential_nonce_carries_key_id_and_boot_epoch() {
        let mut transport =
            EncryptedTransport::with_credential(MockTransport::new(), &credential(), 77);
        let mut encoder = ChunkEncoder::new();
        let mut raw = Vec::new();
        encoder.encode_heartbeat(1, 2, 3, 4, 5, |c| raw.push(Vec::from(c)));
        encoder.encode_heartbeat(6, 7, 8, 9, 10, |c| raw.push(Vec::from(c)));
        for c in &raw {
            transport.send_chunk(c).unwrap();
        }

        let sent = &transport.inner().sent;
        let first = NonceFields::parse(&sealed_payload(&sent[0])[..NONCE_SIZE]).unwrap();
        let second = NonceFields::parse(&sealed_payload(&sent[1])[..NONCE_SIZE]).unwrap();
        assert_eq!(
            first,
            NonceFields {
                counter: 0,
                boot_epoch: 77,
                key_id: 0xA1B2_C3D4,
                version: NONCE_VERSION_CREDENTIAL
            }
        );
        assert_eq!(second.counter, 1);

        let mut out = [0u8; 256];
        let n = decrypt_payload(&credential().key, sealed_payload(&sent[1]), &mut out).unwrap();
        assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 6);
        assert_eq!(n, 24);
    }

    #[test]
    fn nonces_differ_across_boots() {
        let mut encoder = ChunkEncoder::new();
        let mut raw = Vec::new();
        encoder.encode_heartbeat(1, 2, 3, 4, 5, |c| raw.push(Vec::from(c)));
        let nonce_for = |epoch| {
            let (sealed, _) = ChunkSealer::for_device(&credential(), epoch)
                .seal(&raw[0])
                .unwrap();
            sealed[8..8 + NONCE_SIZE].to_vec()
        };
        assert_ne!(nonce_for(1), nonce_for(2));
    }

    #[test]
    fn credential_never_falls_back_to_cleartext() {
        let mut transport =
            EncryptedTransport::with_credential(MockTransport::new(), &credential(), 1);
        // A foreign chunk whose payload is too large to seal.
        let mut big = [0u8; 256];
        big[0] = ChunkHeader::MAGIC;
        big[1] = ChunkHeader::VERSION;
        big[2] = ChunkType::Heartbeat as u8;
        big[4..6].copy_from_slice(&240u16.to_le_bytes());
        let crc = crc16_ccitt(&big[..248]);
        big[248..250].copy_from_slice(&crc.to_le_bytes());
        transport.send_chunk(&big[..250]).unwrap();
        assert!(transport.inner().sent.is_empty());

        // The shared key keeps its old behaviour and sends it as is.
        let mut shared = EncryptedTransport::with_shared_key(MockTransport::new(), [1; 16], 1);
        shared.send_chunk(&big[..250]).unwrap();
        assert_eq!(shared.inner().sent[0], big[..250].to_vec());
    }

    #[test]
    fn every_encoded_chunk_can_be_sealed() {
        use crate::metrics::{MetricEntry, MetricValue};
        let mut encoder = ChunkEncoder::new();
        let mut raw = Vec::new();
        let entries: Vec<MetricEntry> = (0..32u32)
            .map(|i| {
                let mut key = heapless::String::new();
                key.push_str(&std::format!("metric_key_number_{i:02}"))
                    .unwrap();
                MetricEntry {
                    key,
                    value: MetricValue::Counter(i),
                    timestamp_ticks: u64::from(i),
                }
            })
            .collect();
        encoder.encode_metrics(entries.iter(), |c| raw.push(Vec::from(c)));
        encoder.encode_device_info(&"d".repeat(127), &"v".repeat(127), 1, |c| {
            raw.push(Vec::from(c))
        });
        assert!(raw.len() > 2);

        let mut sealer = ChunkSealer::for_device(&credential(), 9);
        for c in &raw {
            assert!(
                sealer.seal(c).is_some(),
                "chunk of {} bytes could not be sealed",
                c.len()
            );
        }
    }

    #[test]
    fn nonce_parse_rejects_wrong_length() {
        assert!(NonceFields::parse(&[0u8; 12]).is_none());
        assert!(NonceFields::parse(&[0u8; 14]).is_none());
    }
}
