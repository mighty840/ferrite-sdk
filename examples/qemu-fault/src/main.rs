//! A device that crashes once and finds out after the reset. Runs in QEMU.
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use ferrite_sdk::fault::last_fault;
use ferrite_sdk::{RamRegion, SdkConfig};
use panic_semihosting as _;

#[entry]
fn main() -> ! {
    ferrite_sdk::init(SdkConfig {
        device_id: "qemu-demo-01",
        firmware_version: "0.1.0",
        build_id: 0,
        ticks_fn: || 0,
        ram_regions: &[RamRegion { start: 0x2000_0000, end: 0x2000_FC00 }],
    });

    if let Some(fault) = last_fault() {
        hprintln!("boot: the previous run crashed");
        hprintln!("  pc   = {:#010x}", fault.frame.pc);
        hprintln!("  cfsr = {:#010x}", fault.cfsr);
        debug::exit(debug::EXIT_SUCCESS);
    }

    hprintln!("boot: no crash recorded, handling a message");
    handle_message();
    hprintln!("message handled");
    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}

/// The bug we are going to find. `udf` is an instruction the processor
/// refuses to execute, so this function crashes every time.
#[inline(never)]
fn handle_message() {
    unsafe { core::arch::asm!("udf #0") };
}

// lm3s6965 has 54 interrupts; without a device crate we provide the table.
#[no_mangle]
#[link_section = ".vector_table.interrupts"]
pub static __INTERRUPTS: [unsafe extern "C" fn(); 54] = [DefaultHandler; 54];
extern "C" {
    fn DefaultHandler();
}
