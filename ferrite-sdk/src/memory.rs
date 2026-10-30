use crate::device_key::DeviceKeyRecord;
use crate::fault::FaultRecord;
use crate::reboot_reason::RebootReasonRecord;

pub const RETAINED_MAGIC: u32 = 0xAB_CD_12_34;

/// Header for the retained RAM block, validated on each boot.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RetainedHeader {
    pub magic: u32,
    pub sequence: u32,
    pub crc: u16,
    pub _pad: u16,
}

impl RetainedHeader {
    pub fn is_valid(&self) -> bool {
        self.magic == RETAINED_MAGIC
    }
}

/// Full retained RAM block layout.
/// Stored in `.uninit.ferrite` section — survives soft resets.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RetainedBlock {
    pub header: RetainedHeader,
    pub reboot_reason: RebootReasonRecord,
    pub fault_record: FaultRecord,
    pub metrics_dirty: bool,
    pub _pad: [u8; 3],
    pub device_key: DeviceKeyRecord,
}

// Compile-time size check
const _: () = assert!(core::mem::size_of::<RetainedBlock>() <= 256);

impl RetainedBlock {
    pub const fn zeroed() -> Self {
        Self {
            header: RetainedHeader {
                magic: 0,
                sequence: 0,
                crc: 0,
                _pad: 0,
            },
            reboot_reason: RebootReasonRecord::zeroed(),
            fault_record: FaultRecord::zeroed(),
            metrics_dirty: false,
            _pad: [0; 3],
            device_key: DeviceKeyRecord::zeroed(),
        }
    }
}

/// Wrapper around UnsafeCell that implements Sync.
/// Safety: Access must be coordinated via critical sections or from fault handler only.
#[repr(transparent)]
pub struct RetainedCell<T>(core::cell::UnsafeCell<T>);

// SAFETY: Access to retained RAM is coordinated via critical sections
// or happens from the fault handler (which has exclusive access).
unsafe impl<T> Sync for RetainedCell<T> {}

impl<T> RetainedCell<T> {
    pub const fn new(val: T) -> Self {
        Self(core::cell::UnsafeCell::new(val))
    }

    pub fn get(&self) -> *mut T {
        self.0.get()
    }
}

/// Macro to place a static in the `.uninit.ferrite` (retained) section.
/// Variables in this section are NOT zeroed on reset.
#[macro_export]
macro_rules! retained {
    ($vis:vis static $name:ident: $ty:ty = $default:expr) => {
        #[cfg_attr(not(test), link_section = ".uninit.ferrite")]
        #[used]
        $vis static $name: $crate::memory::RetainedCell<$ty> =
            $crate::memory::RetainedCell::new($default);
    };
}

// Global retained block instance
retained!(pub static RETAINED_BLOCK: RetainedBlock = RetainedBlock::zeroed());

/// Get a mutable pointer to the global retained block.
///
/// # Safety
/// Caller must ensure exclusive access (e.g., within a critical section or fault handler).
pub unsafe fn get_retained_block_ptr() -> *mut RetainedBlock {
    RETAINED_BLOCK.get()
}

/// Validate the retained block at boot and count the boot.
///
/// With a wrong magic, the block holds whatever RAM contained (power-up
/// noise, another firmware's data), so the whole block is overwritten with
/// zeroes before the magic is written, without reading the old records.
/// Returns this boot's sequence number (1 after a reinitialisation).
///
/// # Safety
/// `block` must point to a `RetainedBlock`-sized, aligned region that no
/// one else accesses during the call. Its contents may be uninitialised.
pub unsafe fn begin_boot(block: *mut RetainedBlock) -> u32 {
    let magic = core::ptr::addr_of!((*block).header.magic).read_volatile();
    if magic != RETAINED_MAGIC {
        core::ptr::write_volatile(block, RetainedBlock::zeroed());
        (*block).header.magic = RETAINED_MAGIC;
    }
    let sequence = (*block).header.sequence.wrapping_add(1);
    (*block).header.sequence = sequence;
    sequence
}

/// Check if the retained block contains valid data.
pub fn is_valid() -> bool {
    // SAFETY: We only read the magic field which is atomic-sized
    unsafe { (*RETAINED_BLOCK.get()).header.is_valid() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{size_of, MaybeUninit};
    use core::ptr::addr_of_mut;

    /// A block filled with noise whose fault record looks valid.
    fn noisy_block() -> MaybeUninit<RetainedBlock> {
        let mut block = MaybeUninit::<RetainedBlock>::uninit();
        let p = block.as_mut_ptr();
        let mut x: u32 = 0x2545_F491;
        for i in 0..size_of::<RetainedBlock>() {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            unsafe { p.cast::<u8>().add(i).write(x as u8) };
        }
        unsafe {
            addr_of_mut!((*p).header.magic).write(0xDEAD_BEEF);
            addr_of_mut!((*p).fault_record.valid).cast::<u8>().write(1);
            addr_of_mut!((*p).fault_record.fault_type)
                .cast::<u8>()
                .write(0);
        }
        block
    }

    #[test]
    fn block_size_matches_the_docs() {
        // docs/guide/concepts.md, introduction.md and reference/config.md
        // give this size; update them when the layout changes.
        assert_eq!(size_of::<RetainedBlock>(), 192);
    }

    #[test]
    fn noise_is_cleared_on_boot() {
        let mut block = noisy_block();
        let sequence = unsafe { begin_boot(block.as_mut_ptr()) };
        let block = unsafe { block.assume_init() };
        assert_eq!(sequence, 1);
        assert!(block.header.is_valid());
        assert!(!block.fault_record.valid);
        assert!(!block.reboot_reason.is_valid());
        assert!(!block.device_key.is_valid());
        assert!(!block.metrics_dirty);
    }

    #[test]
    fn valid_block_keeps_its_records() {
        let mut block = RetainedBlock::zeroed();
        block.header.magic = RETAINED_MAGIC;
        block.header.sequence = 5;
        block.fault_record.valid = true;
        block.fault_record.frame.pc = 0x0800_1234;
        let sequence = unsafe { begin_boot(&mut block) };
        assert_eq!(sequence, 6);
        assert!(block.fault_record.valid);
        assert_eq!(block.fault_record.frame.pc, 0x0800_1234);
    }
}
