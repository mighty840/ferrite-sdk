use crate::memory;

/// Record stored in retained RAM for device provisioning key.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DeviceKeyRecord {
    pub magic: u32,
    pub key: u32,
}

const DEVICE_KEY_MAGIC: u32 = 0xFE_D1_5C_07;

impl DeviceKeyRecord {
    pub const fn zeroed() -> Self {
        Self { magic: 0, key: 0 }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == DEVICE_KEY_MAGIC
    }
}

// Compile-time size check: should be 8 bytes
const _: () = assert!(core::mem::size_of::<DeviceKeyRecord>() == 8);

/// Provision a device key. If a valid key already exists in retained RAM,
/// return it (idempotent). Otherwise, build a new key from the owner prefix
/// and entropy seed, write it, and return it.
///
/// Key format: `(owner_prefix << 24) | (entropy_seed & 0x00FF_FFFF)`
pub fn provision_device_key(owner_prefix: u8, entropy_seed: u32) -> u32 {
    if let Some(existing) = device_key() {
        return existing;
    }
    let key = ((owner_prefix as u32) << 24) | (entropy_seed & 0x00FF_FFFF);
    unsafe {
        let retained = memory::get_retained_block_ptr();
        (*retained).device_key = DeviceKeyRecord {
            magic: DEVICE_KEY_MAGIC,
            key,
        };
    }
    key
}

/// Read the device key from retained RAM.
/// Returns None if no valid key record exists.
pub fn device_key() -> Option<u32> {
    unsafe {
        let retained = &*memory::get_retained_block_ptr();
        if retained.device_key.is_valid() {
            Some(retained.device_key.key)
        } else {
            None
        }
    }
}

/// Clear the device key record.
pub fn clear_device_key() {
    unsafe {
        let retained = memory::get_retained_block_ptr();
        (*retained).device_key = DeviceKeyRecord::zeroed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    #[test]
    fn roundtrip() {
        clear_device_key();
        let key = provision_device_key(0xA3, 0x00F1B2);
        assert_eq!(key, 0xA300_F1B2);
        assert_eq!(device_key(), Some(0xA300_F1B2));
    }

    #[test]
    fn idempotency() {
        clear_device_key();
        let k1 = provision_device_key(0xA3, 0x00F1B2);
        let k2 = provision_device_key(0xFF, 0xFFFFFF); // different args
        assert_eq!(k1, k2); // should return the existing key
    }

    #[test]
    fn clear_returns_none() {
        provision_device_key(0x01, 0x1234);
        clear_device_key();
        assert_eq!(device_key(), None);
    }

    #[test]
    fn prefix_placement() {
        clear_device_key();
        let key = provision_device_key(0xBB, 0xDEADBE);
        assert_eq!((key >> 24) as u8, 0xBB);
        assert_eq!(key & 0x00FF_FFFF, 0xADBE_u32 | (0xDE << 16)); // 0xDEADBE
    }
}
