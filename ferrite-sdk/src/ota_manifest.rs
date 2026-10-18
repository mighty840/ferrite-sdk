//! Signed OTA update manifests.
//!
//! A manifest describes one firmware image and is signed offline with an
//! Ed25519 key. The device holds only the public key(s), so neither the
//! server nor the transport can make it accept an image the key holder did
//! not sign.
//!
//! Wire format (little-endian, 168 bytes):
//!
//! ```text
//! [0..4]     magic             b"FOTA"
//! [4]        format version    1
//! [5..8]     reserved          0
//! [8..16]    build_id          u64
//! [16..20]   security_version  u32   (anti-rollback, monotonic)
//! [20..24]   image_size        u32
//! [24..56]   sha256            [u8; 32] of the image
//! [56..88]   version           UTF-8, zero-padded
//! [88..96]   key_id            first 8 bytes of SHA-256(public key)
//! [96..104]  reserved          0
//! [104..168] signature         Ed25519 over SIGNING_CONTEXT || body[0..104]
//! ```
//!
//! `build_id` is a build timestamp or hash and is not ordered, so rollback
//! protection uses the separate `security_version`: a device rejects any
//! manifest below its own minimum.

use ed25519_compact::{PublicKey, Signature};
use sha2::{Digest, Sha256};

/// Manifest magic bytes.
pub const MAGIC: [u8; 4] = *b"FOTA";
/// Current manifest format version.
pub const FORMAT_VERSION: u8 = 1;
/// Length of the signed body.
pub const BODY_LEN: usize = 104;
/// Length of the Ed25519 signature.
pub const SIGNATURE_LEN: usize = 64;
/// Total manifest length.
pub const MANIFEST_LEN: usize = BODY_LEN + SIGNATURE_LEN;
/// Length of the hex encoding (as sent in the `X-Firmware-Manifest` header).
pub const MANIFEST_HEX_LEN: usize = MANIFEST_LEN * 2;
/// Maximum length of the version string.
pub const VERSION_LEN: usize = 32;
/// Domain separation prefix for signatures, so a manifest key can't be
/// tricked into signing anything else that verifies as a manifest.
pub const SIGNING_CONTEXT: &[u8] = b"ferrite-ota-manifest-v1";
/// Length of the message the signature covers.
pub const SIGNING_MESSAGE_LEN: usize = SIGNING_CONTEXT.len() + BODY_LEN;

/// Errors from parsing or verifying a manifest or image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestError {
    /// Input is not exactly [`MANIFEST_LEN`] bytes (or hex chars).
    Length,
    /// Hex input contains a non-hex character.
    InvalidHex,
    /// Magic bytes don't match.
    Magic,
    /// Format version this SDK doesn't understand.
    UnsupportedFormat(u8),
    /// Reserved bytes are not zero.
    Reserved,
    /// Version field is not valid UTF-8.
    InvalidVersion,
    /// No trusted key matches the manifest's `key_id`.
    UnknownKey,
    /// Signature does not verify.
    BadSignature,
    /// Manifest `security_version` is below the device's minimum.
    Rollback { manifest: u32, minimum: u32 },
    /// Image is longer or shorter than the manifest says.
    SizeMismatch,
    /// Image SHA-256 doesn't match the manifest.
    HashMismatch,
}

/// The signed content of a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub build_id: u64,
    pub security_version: u32,
    pub image_size: u32,
    pub sha256: [u8; 32],
    pub version: heapless::String<VERSION_LEN>,
    pub key_id: [u8; 8],
}

impl Manifest {
    /// Encode the body that gets signed.
    pub fn encode_body(&self) -> [u8; BODY_LEN] {
        let mut b = [0u8; BODY_LEN];
        b[0..4].copy_from_slice(&MAGIC);
        b[4] = FORMAT_VERSION;
        b[8..16].copy_from_slice(&self.build_id.to_le_bytes());
        b[16..20].copy_from_slice(&self.security_version.to_le_bytes());
        b[20..24].copy_from_slice(&self.image_size.to_le_bytes());
        b[24..56].copy_from_slice(&self.sha256);
        let v = self.version.as_bytes();
        b[56..56 + v.len()].copy_from_slice(v);
        b[88..96].copy_from_slice(&self.key_id);
        b
    }

    /// Decode a body, checking magic, format and reserved bytes.
    pub fn decode_body(b: &[u8; BODY_LEN]) -> Result<Self, ManifestError> {
        if b[0..4] != MAGIC {
            return Err(ManifestError::Magic);
        }
        if b[4] != FORMAT_VERSION {
            return Err(ManifestError::UnsupportedFormat(b[4]));
        }
        if b[5..8].iter().chain(&b[96..104]).any(|&x| x != 0) {
            return Err(ManifestError::Reserved);
        }

        let raw_version = &b[56..88];
        let len = raw_version
            .iter()
            .position(|&x| x == 0)
            .unwrap_or(VERSION_LEN);
        // Padding must be zero all the way, so one body has one encoding.
        if raw_version[len..].iter().any(|&x| x != 0) {
            return Err(ManifestError::InvalidVersion);
        }
        let version_str =
            core::str::from_utf8(&raw_version[..len]).map_err(|_| ManifestError::InvalidVersion)?;
        let mut version = heapless::String::new();
        version
            .push_str(version_str)
            .map_err(|_| ManifestError::InvalidVersion)?;

        Ok(Self {
            build_id: u64::from_le_bytes(b[8..16].try_into().unwrap_or_default()),
            security_version: u32::from_le_bytes(b[16..20].try_into().unwrap_or_default()),
            image_size: u32::from_le_bytes(b[20..24].try_into().unwrap_or_default()),
            sha256: b[24..56].try_into().unwrap_or_default(),
            version,
            key_id: b[88..96].try_into().unwrap_or_default(),
        })
    }
}

/// Key identifier stored in manifests: first 8 bytes of SHA-256(public key).
pub fn key_id(public_key: &[u8; 32]) -> [u8; 8] {
    let digest = Sha256::digest(public_key);
    let mut id = [0u8; 8];
    id.copy_from_slice(&digest[..8]);
    id
}

/// The exact message an Ed25519 signature covers.
pub fn signing_message(body: &[u8; BODY_LEN]) -> [u8; SIGNING_MESSAGE_LEN] {
    let mut msg = [0u8; SIGNING_MESSAGE_LEN];
    msg[..SIGNING_CONTEXT.len()].copy_from_slice(SIGNING_CONTEXT);
    msg[SIGNING_CONTEXT.len()..].copy_from_slice(body);
    msg
}

/// A parsed but not yet verified manifest.
#[derive(Debug, Clone)]
pub struct SignedManifest {
    body: [u8; BODY_LEN],
    signature: [u8; SIGNATURE_LEN],
    manifest: Manifest,
}

impl SignedManifest {
    /// Assemble from a body and its signature (used by signing tools).
    pub fn new(
        body: [u8; BODY_LEN],
        signature: [u8; SIGNATURE_LEN],
    ) -> Result<Self, ManifestError> {
        let manifest = Manifest::decode_body(&body)?;
        Ok(Self {
            body,
            signature,
            manifest,
        })
    }

    /// Parse the 168-byte binary form.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ManifestError> {
        if bytes.len() != MANIFEST_LEN {
            return Err(ManifestError::Length);
        }
        let mut body = [0u8; BODY_LEN];
        let mut signature = [0u8; SIGNATURE_LEN];
        body.copy_from_slice(&bytes[..BODY_LEN]);
        signature.copy_from_slice(&bytes[BODY_LEN..]);
        Self::new(body, signature)
    }

    /// Parse the hex form (upper or lower case).
    pub fn from_hex(hex: &[u8]) -> Result<Self, ManifestError> {
        if hex.len() != MANIFEST_HEX_LEN {
            return Err(ManifestError::Length);
        }
        let mut bytes = [0u8; MANIFEST_LEN];
        decode_hex(hex, &mut bytes)?;
        Self::from_bytes(&bytes)
    }

    /// The 168-byte binary form.
    pub fn to_bytes(&self) -> [u8; MANIFEST_LEN] {
        let mut out = [0u8; MANIFEST_LEN];
        out[..BODY_LEN].copy_from_slice(&self.body);
        out[BODY_LEN..].copy_from_slice(&self.signature);
        out
    }

    /// The lowercase hex form.
    pub fn to_hex(&self) -> [u8; MANIFEST_HEX_LEN] {
        let mut out = [0u8; MANIFEST_HEX_LEN];
        encode_hex(&self.to_bytes(), &mut out);
        out
    }

    /// Manifest content. Not authenticated until [`Self::verify`] succeeds.
    pub fn unverified(&self) -> &Manifest {
        &self.manifest
    }

    /// Verify the signature against the trusted key whose id matches.
    pub fn verify(&self, trusted_keys: &[[u8; 32]]) -> Result<&Manifest, ManifestError> {
        let key = trusted_keys
            .iter()
            .find(|k| key_id(k) == self.manifest.key_id)
            .ok_or(ManifestError::UnknownKey)?;
        let public_key = PublicKey::from_slice(key).map_err(|_| ManifestError::UnknownKey)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| ManifestError::BadSignature)?;
        public_key
            .verify(signing_message(&self.body), &signature)
            .map_err(|_| ManifestError::BadSignature)?;
        Ok(&self.manifest)
    }
}

/// What a device accepts: manifests signed by one of `trusted_keys` with a
/// `security_version` of at least `min_security_version`.
#[derive(Debug, Clone, Copy)]
pub struct UpdatePolicy<'a> {
    pub trusted_keys: &'a [[u8; 32]],
    pub min_security_version: u32,
}

impl UpdatePolicy<'_> {
    /// Verify the signature, then the anti-rollback floor.
    pub fn check<'m>(&self, signed: &'m SignedManifest) -> Result<&'m Manifest, ManifestError> {
        let manifest = signed.verify(self.trusted_keys)?;
        if manifest.security_version < self.min_security_version {
            return Err(ManifestError::Rollback {
                manifest: manifest.security_version,
                minimum: self.min_security_version,
            });
        }
        Ok(manifest)
    }
}

/// Streaming check of an image against a verified manifest.
pub struct ImageVerifier {
    hasher: Sha256,
    received: u32,
    expected_size: u32,
    expected_sha256: [u8; 32],
}

impl ImageVerifier {
    pub fn new(manifest: &Manifest) -> Self {
        Self {
            hasher: Sha256::new(),
            received: 0,
            expected_size: manifest.image_size,
            expected_sha256: manifest.sha256,
        }
    }

    /// Feed the next part of the image. Fails as soon as the image is longer
    /// than the manifest says, before the caller writes the excess to flash.
    pub fn update(&mut self, data: &[u8]) -> Result<(), ManifestError> {
        let len = u32::try_from(data.len()).map_err(|_| ManifestError::SizeMismatch)?;
        let total = self
            .received
            .checked_add(len)
            .ok_or(ManifestError::SizeMismatch)?;
        if total > self.expected_size {
            return Err(ManifestError::SizeMismatch);
        }
        self.hasher.update(data);
        self.received = total;
        Ok(())
    }

    /// Check the complete image.
    pub fn finish(self) -> Result<(), ManifestError> {
        if self.received != self.expected_size {
            return Err(ManifestError::SizeMismatch);
        }
        if self.hasher.finalize().as_slice() != self.expected_sha256 {
            return Err(ManifestError::HashMismatch);
        }
        Ok(())
    }
}

const HEX_LOWER: [u8; 16] = *b"0123456789abcdef";

fn encode_hex(bytes: &[u8], out: &mut [u8]) {
    for (i, &byte) in bytes.iter().enumerate() {
        out[i * 2] = HEX_LOWER[(byte >> 4) as usize];
        out[i * 2 + 1] = HEX_LOWER[(byte & 0xF) as usize];
    }
}

fn decode_hex(hex: &[u8], out: &mut [u8]) -> Result<(), ManifestError> {
    fn nibble(c: u8) -> Result<u8, ManifestError> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(ManifestError::InvalidHex),
        }
    }
    for (i, pair) in hex.chunks_exact(2).enumerate() {
        out[i] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_compact::{KeyPair, Seed};

    const IMAGE: &[u8] = b"firmware image bytes for the manifest tests";

    fn keypair(seed: u8) -> KeyPair {
        KeyPair::from_seed(Seed::new([seed; 32]))
    }

    fn manifest_for(kp: &KeyPair, security_version: u32) -> Manifest {
        let pk: [u8; 32] = *kp.pk;
        let mut version = heapless::String::new();
        version.push_str("1.2.3").unwrap();
        Manifest {
            build_id: 0xDEAD_BEEF_0000_0001,
            security_version,
            image_size: IMAGE.len() as u32,
            sha256: Sha256::digest(IMAGE).into(),
            version,
            key_id: key_id(&pk),
        }
    }

    fn sign(kp: &KeyPair, manifest: &Manifest) -> SignedManifest {
        let body = manifest.encode_body();
        let sig = kp.sk.sign(signing_message(&body), None);
        SignedManifest::new(body, *sig).unwrap()
    }

    fn policy(keys: &[[u8; 32]], min: u32) -> UpdatePolicy<'_> {
        UpdatePolicy {
            trusted_keys: keys,
            min_security_version: min,
        }
    }

    #[test]
    fn roundtrip_verify_and_image_check() {
        let kp = keypair(1);
        let keys = [*kp.pk];
        let signed = sign(&kp, &manifest_for(&kp, 3));

        let parsed = SignedManifest::from_hex(&signed.to_hex()).unwrap();
        let m = policy(&keys, 3).check(&parsed).unwrap();
        assert_eq!(m.version.as_str(), "1.2.3");
        assert_eq!(m.build_id, 0xDEAD_BEEF_0000_0001);

        let mut v = ImageVerifier::new(m);
        for part in IMAGE.chunks(7) {
            v.update(part).unwrap();
        }
        v.finish().unwrap();
    }

    #[test]
    fn uppercase_hex_is_accepted() {
        let kp = keypair(1);
        let signed = sign(&kp, &manifest_for(&kp, 0));
        let upper = signed.to_hex().map(|c| c.to_ascii_uppercase());
        assert!(SignedManifest::from_hex(&upper).is_ok());
    }

    #[test]
    fn tampered_body_fails_signature() {
        let kp = keypair(1);
        let mut bytes = sign(&kp, &manifest_for(&kp, 0)).to_bytes();
        bytes[8] ^= 1; // build_id
        let parsed = SignedManifest::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.verify(&[*kp.pk]), Err(ManifestError::BadSignature));
    }

    #[test]
    fn tampered_signature_fails() {
        let kp = keypair(1);
        let mut bytes = sign(&kp, &manifest_for(&kp, 0)).to_bytes();
        bytes[MANIFEST_LEN - 1] ^= 0x80;
        let parsed = SignedManifest::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.verify(&[*kp.pk]), Err(ManifestError::BadSignature));
    }

    #[test]
    fn untrusted_key_is_rejected() {
        let signer = keypair(1);
        let other = keypair(2);
        let signed = sign(&signer, &manifest_for(&signer, 0));
        assert_eq!(signed.verify(&[*other.pk]), Err(ManifestError::UnknownKey));
        assert_eq!(signed.verify(&[]), Err(ManifestError::UnknownKey));
    }

    #[test]
    fn key_id_collision_still_needs_valid_signature() {
        // Attacker signs with their own key but claims the trusted key's id.
        let trusted = keypair(1);
        let attacker = keypair(2);
        let mut m = manifest_for(&attacker, 0);
        m.key_id = key_id(&trusted.pk);
        let signed = sign(&attacker, &m);
        assert_eq!(
            signed.verify(&[*trusted.pk]),
            Err(ManifestError::BadSignature)
        );
    }

    #[test]
    fn second_trusted_key_verifies() {
        let old = keypair(1);
        let new = keypair(2);
        let signed = sign(&new, &manifest_for(&new, 0));
        assert!(signed.verify(&[*old.pk, *new.pk]).is_ok());
    }

    #[test]
    fn rollback_is_rejected() {
        let kp = keypair(1);
        let keys = [*kp.pk];
        let signed = sign(&kp, &manifest_for(&kp, 4));
        assert_eq!(
            policy(&keys, 5).check(&signed),
            Err(ManifestError::Rollback {
                manifest: 4,
                minimum: 5
            })
        );
        assert!(policy(&keys, 4).check(&signed).is_ok());
    }

    #[test]
    fn image_too_long_fails_before_hashing_the_excess() {
        let kp = keypair(1);
        let m = manifest_for(&kp, 0);
        let mut v = ImageVerifier::new(&m);
        v.update(IMAGE).unwrap();
        assert_eq!(v.update(b"x"), Err(ManifestError::SizeMismatch));
    }

    #[test]
    fn truncated_image_fails() {
        let kp = keypair(1);
        let m = manifest_for(&kp, 0);
        let mut v = ImageVerifier::new(&m);
        v.update(&IMAGE[..IMAGE.len() - 1]).unwrap();
        assert_eq!(v.finish(), Err(ManifestError::SizeMismatch));
    }

    #[test]
    fn modified_image_fails_hash() {
        let kp = keypair(1);
        let m = manifest_for(&kp, 0);
        let mut image = [0u8; IMAGE.len()];
        image.copy_from_slice(IMAGE);
        image[0] ^= 1;
        let mut v = ImageVerifier::new(&m);
        v.update(&image).unwrap();
        assert_eq!(v.finish(), Err(ManifestError::HashMismatch));
    }

    #[test]
    fn malformed_inputs_are_rejected() {
        let kp = keypair(1);
        let bytes = sign(&kp, &manifest_for(&kp, 0)).to_bytes();

        assert_eq!(
            SignedManifest::from_bytes(&bytes[..MANIFEST_LEN - 1]).err(),
            Some(ManifestError::Length)
        );

        let mut bad_magic = bytes;
        bad_magic[0] = b'X';
        assert_eq!(
            SignedManifest::from_bytes(&bad_magic).err(),
            Some(ManifestError::Magic)
        );

        let mut bad_format = bytes;
        bad_format[4] = 2;
        assert_eq!(
            SignedManifest::from_bytes(&bad_format).err(),
            Some(ManifestError::UnsupportedFormat(2))
        );

        let mut bad_reserved = bytes;
        bad_reserved[100] = 1;
        assert_eq!(
            SignedManifest::from_bytes(&bad_reserved).err(),
            Some(ManifestError::Reserved)
        );

        let mut bad_padding = bytes;
        bad_padding[87] = b'x'; // non-zero byte after the version's terminator
        assert_eq!(
            SignedManifest::from_bytes(&bad_padding).err(),
            Some(ManifestError::InvalidVersion)
        );

        let mut bad_hex = sign(&kp, &manifest_for(&kp, 0)).to_hex();
        bad_hex[10] = b'g';
        assert_eq!(
            SignedManifest::from_hex(&bad_hex).err(),
            Some(ManifestError::InvalidHex)
        );
    }
}
