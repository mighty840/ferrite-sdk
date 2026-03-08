use crate::SdkError;
use crate::memory;

/// Valid RAM region for stack snapshot safety checks.
#[derive(Debug, Clone, Copy)]
pub struct RamRegion {
    pub start: u32,
    pub end: u32,
}

/// Full Cortex-M exception frame pushed by hardware on fault entry.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ExceptionFrame {
    pub r0: u32,
    pub r1: u32,
    pub r2: u32,
    pub r3: u32,
    pub r12: u32,
    pub lr: u32,
    pub pc: u32,
    pub xpsr: u32,
}

impl ExceptionFrame {
    pub const fn zeroed() -> Self {
        Self {
            r0: 0, r1: 0, r2: 0, r3: 0,
            r12: 0, lr: 0, pc: 0, xpsr: 0,
        }
    }
}

/// Additional registers not in the hardware-pushed frame.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ExtendedRegisters {
    pub r4: u32,
    pub r5: u32,
    pub r6: u32,
    pub r7: u32,
    pub r8: u32,
    pub r9: u32,
    pub r10: u32,
    pub r11: u32,
    pub sp: u32,
}

impl ExtendedRegisters {
    pub const fn zeroed() -> Self {
        Self {
            r4: 0, r5: 0, r6: 0, r7: 0,
            r8: 0, r9: 0, r10: 0, r11: 0,
            sp: 0,
        }
    }
}

/// Stack snapshot: first 16 words above SP at fault time.
pub type StackSnapshot = [u32; 16];

/// Fault type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FaultType {
    HardFault = 0,
    MemFault = 1,
    BusFault = 2,
    UsageFault = 3,
}

/// Complete fault record stored in retained RAM.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FaultRecord {
    pub valid: bool,
    pub fault_type: FaultType,
    pub _pad: [u8; 2],
    pub frame: ExceptionFrame,
    pub extended: ExtendedRegisters,
    pub stack_snapshot: StackSnapshot,
    pub cfsr: u32,
    pub hfsr: u32,
    pub mmfar: u32,
    pub bfar: u32,
}

impl FaultRecord {
    pub const fn zeroed() -> Self {
        Self {
            valid: false,
            fault_type: FaultType::HardFault,
            _pad: [0; 2],
            frame: ExceptionFrame::zeroed(),
            extended: ExtendedRegisters::zeroed(),
            stack_snapshot: [0; 16],
            cfsr: 0,
            hfsr: 0,
            mmfar: 0,
            bfar: 0,
        }
    }

    /// Create a test fault record for unit tests.
    #[cfg(test)]
    pub fn default_for_test() -> Self {
        Self {
            valid: true,
            fault_type: FaultType::HardFault,
            _pad: [0; 2],
            frame: ExceptionFrame {
                r0: 1, r1: 2, r2: 3, r3: 4,
                r12: 5, lr: 0x0800_1000, pc: 0x0800_2000, xpsr: 0x6100_0000,
            },
            extended: ExtendedRegisters {
                r4: 10, r5: 11, r6: 12, r7: 13,
                r8: 14, r9: 15, r10: 16, r11: 17,
                sp: 0x2000_3F00,
            },
            stack_snapshot: [0xDEAD_BEEF; 16],
            cfsr: 0x0000_0400,
            hfsr: 0x4000_0000,
            mmfar: 0,
            bfar: 0,
        }
    }

    /// Serialize the fault record to a byte buffer. Returns number of bytes written.
    pub fn serialize_to(&self, out: &mut [u8]) -> usize {
        // 1B fault_type + 32B frame + 36B extended + 64B stack + 16B fault regs = 149
        let mut pos = 0;

        if out.len() < 149 {
            return 0;
        }

        out[pos] = self.fault_type as u8;
        pos += 1;

        // ExceptionFrame: 8 x u32
        for val in [
            self.frame.r0, self.frame.r1, self.frame.r2, self.frame.r3,
            self.frame.r12, self.frame.lr, self.frame.pc, self.frame.xpsr,
        ] {
            out[pos..pos + 4].copy_from_slice(&val.to_le_bytes());
            pos += 4;
        }

        // ExtendedRegisters: 9 x u32
        for val in [
            self.extended.r4, self.extended.r5, self.extended.r6, self.extended.r7,
            self.extended.r8, self.extended.r9, self.extended.r10, self.extended.r11,
            self.extended.sp,
        ] {
            out[pos..pos + 4].copy_from_slice(&val.to_le_bytes());
            pos += 4;
        }

        // StackSnapshot: 16 x u32
        for val in &self.stack_snapshot {
            out[pos..pos + 4].copy_from_slice(&val.to_le_bytes());
            pos += 4;
        }

        // Fault status registers: 4 x u32
        for val in [self.cfsr, self.hfsr, self.mmfar, self.bfar] {
            out[pos..pos + 4].copy_from_slice(&val.to_le_bytes());
            pos += 4;
        }

        pos
    }
}

// Global RAM regions for fault handler stack snapshot validation.
use core::sync::atomic::{AtomicUsize, Ordering};

static RAM_REGION_COUNT: AtomicUsize = AtomicUsize::new(0);
static mut RAM_REGIONS: [RamRegion; 4] = [RamRegion { start: 0, end: 0 }; 4];

/// Check if an address falls within known valid RAM regions.
pub fn is_valid_ram_address(addr: u32) -> bool {
    let count = RAM_REGION_COUNT.load(Ordering::Relaxed);
    // SAFETY: We only read up to `count` entries which were fully written before
    // the count was incremented.
    unsafe {
        for i in 0..count {
            let r = &RAM_REGIONS[i];
            if addr >= r.start && addr < r.end {
                return true;
            }
        }
    }
    false
}

/// Register a valid RAM region for stack snapshot safety checks.
/// Call at SDK init time, before any fault can occur.
pub fn register_ram_region(start: u32, end: u32) -> Result<(), SdkError> {
    let count = RAM_REGION_COUNT.load(Ordering::Relaxed);
    if count >= 4 {
        return Err(SdkError::TooManyRamRegions);
    }
    // SAFETY: We write to `count` index then increment the count atomically.
    // No other writer runs concurrently (init is single-threaded).
    unsafe {
        RAM_REGIONS[count] = RamRegion { start, end };
    }
    RAM_REGION_COUNT.store(count + 1, Ordering::Release);
    Ok(())
}

/// Read the fault record from the previous boot.
/// Returns None if no valid fault record is present in retained RAM.
pub fn last_fault() -> Option<FaultRecord> {
    unsafe {
        let retained = &*memory::get_retained_block_ptr();
        if retained.fault_record.valid {
            Some(retained.fault_record)
        } else {
            None
        }
    }
}

/// Clear the fault record after it has been uploaded.
pub fn clear_fault_record() {
    unsafe {
        let retained = memory::get_retained_block_ptr();
        (*retained).fault_record = FaultRecord::zeroed();
    }
}

// Cortex-M specific: HardFault handler and register capture
#[cfg(feature = "cortex-m")]
mod cortex_m_handler {
    use super::*;

    #[inline(always)]
    unsafe fn capture_extended_registers() -> ExtendedRegisters {
        let mut regs = ExtendedRegisters::zeroed();
        core::arch::asm!(
            "str r4,  [{ptr}, #0]",
            "str r5,  [{ptr}, #4]",
            "str r6,  [{ptr}, #8]",
            "str r7,  [{ptr}, #12]",
            "str r8,  [{ptr}, #16]",
            "str r9,  [{ptr}, #20]",
            "str r10, [{ptr}, #24]",
            "str r11, [{ptr}, #28]",
            "mov r0,  sp",
            "str r0,  [{ptr}, #32]",
            ptr = in(reg) &mut regs,
            out("r0") _,
            options(nostack)
        );
        regs
    }

    use cortex_m_rt::exception;

    #[exception]
    unsafe fn HardFault(frame: &cortex_m_rt::ExceptionFrame) -> ! {
        let extended = capture_extended_registers();

        let sp = extended.sp as *const u32;
        let mut snapshot = [0u32; 16];
        for i in 0..16 {
            let addr = sp.add(i);
            if is_valid_ram_address(addr as u32) {
                snapshot[i] = addr.read_volatile();
            }
        }

        let scb = &*cortex_m::peripheral::SCB::PTR;
        let cfsr = scb.cfsr.read();
        let hfsr = scb.hfsr.read();
        let mmfar = scb.mmfar.read();
        let bfar = scb.bfar.read();

        let retained = memory::get_retained_block_ptr();
        (*retained).fault_record = FaultRecord {
            valid: true,
            fault_type: FaultType::HardFault,
            _pad: [0; 2],
            frame: ExceptionFrame {
                r0: frame.r0(),
                r1: frame.r1(),
                r2: frame.r2(),
                r3: frame.r3(),
                r12: frame.r12(),
                lr: frame.lr(),
                pc: frame.pc(),
                xpsr: frame.xpsr(),
            },
            extended,
            stack_snapshot: snapshot,
            cfsr, hfsr, mmfar, bfar,
        };
        (*retained).reboot_reason.reason = crate::reboot_reason::RebootReason::HardFault;

        cortex_m::peripheral::SCB::sys_reset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    #[test]
    fn ram_region_validation() {
        // Note: RAM_REGIONS is global state shared across tests.
        // Register a region if not already registered.
        let _ = register_ram_region(0x2000_0000, 0x2004_0000);

        assert!(is_valid_ram_address(0x2000_0000));
        assert!(is_valid_ram_address(0x2003_FFFF));
        assert!(!is_valid_ram_address(0x1000_0000));
        assert!(!is_valid_ram_address(0x2004_0000)); // end is exclusive
    }

    #[test]
    fn fault_record_serialize_roundtrip() {
        let record = FaultRecord::default_for_test();
        let mut buf = [0u8; 256];
        let len = record.serialize_to(&mut buf);
        assert_eq!(len, 149);
        assert_eq!(buf[0], FaultType::HardFault as u8);
    }

    #[test]
    fn no_fault_returns_none() {
        clear_fault_record();
        assert!(last_fault().is_none());
    }
}
