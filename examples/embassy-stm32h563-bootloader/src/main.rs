//! embassy-boot bootloader for the Nucleo-H563ZI ferrite OTA example.
//!
//! On every reset: if the app marked an update, swap ACTIVE and DFU (page
//! by page, resumable after power loss); if the previous swap was never
//! confirmed with `mark_booted`, swap back. Then jump to ACTIVE.
//!
//! Signature checking happens in the app before it marks an update (see
//! `ferrite_sdk::ota`), so the bootloader stays small and key-free.

#![no_std]
#![no_main]

use core::cell::RefCell;

use cortex_m_rt::{entry, exception};
use embassy_boot_stm32::{BootLoader, BootLoaderConfig};
use embassy_stm32::flash::{Flash, BANK1_REGION};
use embassy_sync::blocking_mutex::Mutex;

/// Copy buffer for swaps; must divide the 8 KB sector size.
const SWAP_BUFFER: usize = 2048;

#[entry]
fn main() -> ! {
    let p = embassy_stm32::init(Default::default());

    // One flash instance spanning both banks; partition offsets come from
    // memory.x relative to the start of flash.
    let flash = Mutex::new(RefCell::new(Flash::new_blocking(p.FLASH)));
    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);
    let active_offset = config.active.offset();
    let bl = BootLoader::prepare::<_, _, _, SWAP_BUFFER>(config);

    unsafe { bl.load(BANK1_REGION.base + active_offset) }
}

#[no_mangle]
#[cfg_attr(target_os = "none", link_section = ".HardFault.user")]
unsafe extern "C" fn HardFault() {
    cortex_m::peripheral::SCB::sys_reset();
}

#[exception]
unsafe fn DefaultHandler(_: i16) -> ! {
    const SCB_ICSR: *const u32 = 0xE000_ED04 as *const u32;
    let irqn = core::ptr::read_volatile(SCB_ICSR) as u8 as i16 - 16;
    panic!("DefaultHandler #{:?}", irqn);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // A failed prepare (e.g. flash error) must not hang forever: reset and
    // let the next boot retry. The swap is resumable.
    cortex_m::peripheral::SCB::sys_reset();
}
