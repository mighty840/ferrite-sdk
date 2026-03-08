use crate::reboot_reason::RebootReasonRecord;
use crate::fault::FaultRecord;

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

/// Check if the retained block contains valid data.
pub fn is_valid() -> bool {
    // SAFETY: We only read the magic field which is atomic-sized
    unsafe { (*RETAINED_BLOCK.get()).header.is_valid() }
}
