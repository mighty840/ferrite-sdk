//! Everything ferrite's fault handler saves, printed after the reset.
//! Runs in QEMU. Build with `--features guard` for a fault with an address.
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use ferrite_sdk::fault::{last_fault, FaultRecord};
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
        print_record(&fault);
        debug::exit(debug::EXIT_SUCCESS);
    }

    hprintln!("boot: no crash recorded, handling a message");
    #[cfg(not(feature = "guard"))]
    handle_message();
    #[cfg(feature = "guard")]
    read_guarded();
    hprintln!("message handled");
    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}

fn print_record(f: &FaultRecord) {
    let (c, x) = (&f.frame, &f.extended);
    hprintln!("pushed by the processor:");
    hprintln!("  r0  = {:#010x}  r1 = {:#010x}  r2 = {:#010x}  r3   = {:#010x}", c.r0, c.r1, c.r2, c.r3);
    hprintln!("  r12 = {:#010x}  lr = {:#010x}  pc = {:#010x}  xpsr = {:#010x}", c.r12, c.lr, c.pc, c.xpsr);
    hprintln!("saved by ferrite's handler:");
    hprintln!("  r4  = {:#010x}  r5 = {:#010x}  r6  = {:#010x}  r7  = {:#010x}", x.r4, x.r5, x.r6, x.r7);
    hprintln!("  r8  = {:#010x}  r9 = {:#010x}  r10 = {:#010x}  r11 = {:#010x}", x.r8, x.r9, x.r10, x.r11);
    hprintln!("  sp  = {:#010x}", x.sp);
    hprintln!("  cfsr = {:#010x}  hfsr = {:#010x}", f.cfsr, f.hfsr);
    hprintln!("  mmfar = {:#010x}  bfar = {:#010x}", f.mmfar, f.bfar);
    hprintln!("  stack from sp:");
    for row in f.stack_snapshot.chunks(4) {
        hprintln!("    {:#010x} {:#010x} {:#010x} {:#010x}", row[0], row[1], row[2], row[3]);
    }
}

/// The crash from examples/qemu-fault: `udf` is an instruction the processor
/// refuses to execute.
#[cfg(not(feature = "guard"))]
#[inline(never)]
fn handle_message() {
    unsafe { core::arch::asm!("udf #0") };
}

/// Makes the 32 bytes at 0x2000_8000 inaccessible with the MPU, as a stand-in
/// for memory the program must not touch, and reads from it.
#[cfg(feature = "guard")]
#[inline(never)]
fn read_guarded() {
    let mpu = unsafe { &*cortex_m::peripheral::MPU::PTR };
    unsafe {
        // Region 0: the whole address space, full access.
        mpu.rnr.write(0);
        mpu.rbar.write(0);
        mpu.rasr.write((0b011 << 24) | (31 << 1) | 1);
        // Region 1: 32 bytes at 0x2000_8000, no access.
        mpu.rnr.write(1);
        mpu.rbar.write(0x2000_8000);
        mpu.rasr.write((4 << 1) | 1);
        mpu.ctrl.write(1);
    }
    cortex_m::asm::dsb();
    cortex_m::asm::isb();
    let value = unsafe { core::ptr::read_volatile(0x2000_8004 as *const u32) };
    hprintln!("read {:#010x}", value);
}

// lm3s6965 has 54 interrupts; without a device crate we provide the table.
#[no_mangle]
#[link_section = ".vector_table.interrupts"]
pub static __INTERRUPTS: [unsafe extern "C" fn(); 54] = [DefaultHandler; 54];
extern "C" {
    fn DefaultHandler();
}
