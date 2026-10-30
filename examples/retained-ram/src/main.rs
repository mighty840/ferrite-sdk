//! A boot counter in retained RAM: it survives a reset, not a power cut.
//! Runs in QEMU.
#![no_std]
#![no_main]

use core::ptr::{addr_of, read_volatile};
use cortex_m::peripheral::SCB;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use ferrite_sdk::fault::last_fault;
use ferrite_sdk::memory::get_retained_block_ptr;
use ferrite_sdk::{RamRegion, SdkConfig};
use panic_semihosting as _;

#[entry]
fn main() -> ! {
    #[cfg(feature = "noise")]
    fill_with_noise();

    // What the block's magic number holds before ferrite looks at it.
    let magic = unsafe { read_volatile(addr_of!((*get_retained_block_ptr()).header.magic)) };

    ferrite_sdk::init(SdkConfig {
        device_id: "qemu-demo-01",
        firmware_version: "0.1.0",
        build_id: 0,
        ticks_fn: || 0,
        ram_regions: &[RamRegion { start: 0x2000_0000, end: 0x2000_FC00 }],
    });

    // init() counts boots in the block's header.
    let boot = unsafe { (*get_retained_block_ptr()).header.sequence };
    hprintln!("boot {} (magic before init: {:#010x})", boot, magic);
    match last_fault() {
        Some(fault) => hprintln!("  fault record: pc = {:#010x}", fault.frame.pc),
        None => hprintln!("  no fault record"),
    }

    if boot < 3 && !cfg!(feature = "noise") {
        hprintln!("  software reset");
        SCB::sys_reset();
    }
    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}

/// Writes pseudo-random bytes over the retained block before init(): a
/// constructed stand-in for power-up SRAM. To make the noise look like a
/// crash record, byte 28 (the fault record's `valid` flag) is set to 1 and
/// byte 29 (its fault type) to 0.
#[cfg(feature = "noise")]
fn fill_with_noise() {
    let block = unsafe { get_retained_block_ptr() } as *mut u8;
    let mut x: u32 = 0x2545_F491;
    for i in 0..core::mem::size_of::<ferrite_sdk::memory::RetainedBlock>() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let byte = match i {
            28 => 1,
            29 => 0,
            _ => x as u8,
        };
        unsafe { block.add(i).write_volatile(byte) };
    }
}

// lm3s6965 has 54 interrupts; without a device crate we provide the table.
#[no_mangle]
#[link_section = ".vector_table.interrupts"]
pub static __INTERRUPTS: [unsafe extern "C" fn(); 54] = [DefaultHandler; 54];
extern "C" {
    fn DefaultHandler();
}
