use crate::memory;

/// Why the device rebooted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RebootReason {
    Unknown = 0,
    PowerOnReset = 1,
    SoftwareReset = 2,
    WatchdogTimeout = 3,
    HardFault = 4,
    MemoryFault = 5,
    BusFault = 6,
    UsageFault = 7,
    AssertFailed = 8,
    PinReset = 9,
    BrownoutReset = 10,
    FirmwareUpdate = 11,
    UserRequested = 12,
}

impl From<u8> for RebootReason {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::PowerOnReset,
            2 => Self::SoftwareReset,
            3 => Self::WatchdogTimeout,
            4 => Self::HardFault,
            5 => Self::MemoryFault,
            6 => Self::BusFault,
            7 => Self::UsageFault,
            8 => Self::AssertFailed,
            9 => Self::PinReset,
            10 => Self::BrownoutReset,
            11 => Self::FirmwareUpdate,
            12 => Self::UserRequested,
            _ => Self::Unknown,
        }
    }
}

/// Record stored in retained RAM.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RebootReasonRecord {
    pub magic: u32,
    pub reason: RebootReason,
    pub extra: u8,
    pub _pad: [u8; 10],
}

const REBOOT_REASON_MAGIC: u32 = 0xBB_CC_DD_EE;

impl RebootReasonRecord {
    pub const fn zeroed() -> Self {
        Self {
            magic: 0,
            reason: RebootReason::Unknown,
            extra: 0,
            _pad: [0; 10],
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == REBOOT_REASON_MAGIC
    }
}

// Compile-time size check: should be 16 bytes
const _: () = assert!(core::mem::size_of::<RebootReasonRecord>() == 16);

/// Record the reboot reason for the current boot.
/// Call at firmware startup after reading the MCU's reset cause register.
pub fn record_reboot_reason(reason: RebootReason) {
    record_reboot_reason_with_extra(reason, 0);
}

/// Record the reboot reason with an extra byte (e.g. watchdog timer ID).
pub fn record_reboot_reason_with_extra(reason: RebootReason, extra: u8) {
    unsafe {
        let retained = memory::get_retained_block_ptr();
        (*retained).reboot_reason = RebootReasonRecord {
            magic: REBOOT_REASON_MAGIC,
            reason,
            extra,
            _pad: [0; 10],
        };
    }
}

/// Read the reboot reason from the previous boot.
/// Returns None if no valid record exists in retained RAM.
pub fn last_reboot_reason() -> Option<RebootReason> {
    unsafe {
        let retained = &*memory::get_retained_block_ptr();
        if retained.reboot_reason.is_valid() {
            Some(retained.reboot_reason.reason)
        } else {
            None
        }
    }
}

/// Clear the reboot reason record after it has been uploaded.
pub fn clear_reboot_reason() {
    unsafe {
        let retained = memory::get_retained_block_ptr();
        (*retained).reboot_reason = RebootReasonRecord::zeroed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    #[test]
    fn record_and_read_roundtrip() {
        record_reboot_reason(RebootReason::WatchdogTimeout);
        let reason = last_reboot_reason();
        assert_eq!(reason, Some(RebootReason::WatchdogTimeout));
    }

    #[test]
    fn clear_returns_none() {
        record_reboot_reason(RebootReason::SoftwareReset);
        clear_reboot_reason();
        assert_eq!(last_reboot_reason(), None);
    }

    #[test]
    fn from_u8_invalid_returns_unknown() {
        assert_eq!(RebootReason::from(255), RebootReason::Unknown);
        assert_eq!(RebootReason::from(100), RebootReason::Unknown);
    }

    #[test]
    fn from_u8_all_variants() {
        assert_eq!(RebootReason::from(0), RebootReason::Unknown);
        assert_eq!(RebootReason::from(1), RebootReason::PowerOnReset);
        assert_eq!(RebootReason::from(4), RebootReason::HardFault);
        assert_eq!(RebootReason::from(12), RebootReason::UserRequested);
    }

    #[test]
    fn record_with_extra() {
        record_reboot_reason_with_extra(RebootReason::WatchdogTimeout, 3);
        unsafe {
            let retained = &*crate::memory::get_retained_block_ptr();
            assert_eq!(retained.reboot_reason.extra, 3);
        }
        assert_eq!(last_reboot_reason(), Some(RebootReason::WatchdogTimeout));
    }
}
