#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_nrf::{bind_interrupts, gpio, peripherals, uarte};
use embassy_time::{Duration, Timer};
use iotai_sdk::{SdkConfig, RamRegion, RebootReason};
use defmt_rtt as _;
use panic_probe as _;

mod build_id {
    pub fn get() -> u64 {
        env!("IOTAI_BUILD_ID").parse().unwrap_or(0)
    }
}

bind_interrupts!(struct Irqs {
    UARTE0_UART0 => uarte::InterruptHandler<peripherals::UARTE0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    // Initialize SDK
    iotai_sdk::init(SdkConfig {
        device_id: "nrf52840-example-01",
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: build_id::get(),
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x20000000,
            end: 0x20040000,
        }],
    });

    // Check if we rebooted from a fault
    if let Some(fault) = iotai_sdk::fault::last_fault() {
        defmt::error!(
            "Recovered from fault: PC={:#010x} LR={:#010x}",
            fault.frame.pc,
            fault.frame.lr
        );
    }

    // Record why we booted
    let reason = read_nrf_reset_reason();
    iotai_sdk::reboot_reason::record_reboot_reason(reason);

    // Application loop
    let mut led = gpio::Output::new(p.P0_13, gpio::Level::Low, gpio::OutputDrive::Standard);
    let mut counter: u32 = 0;

    loop {
        led.set_high();
        Timer::after_millis(500).await;
        led.set_low();
        Timer::after_millis(500).await;

        counter += 1;
        iotai_sdk::metric_increment!("loop_count");
        iotai_sdk::metric_gauge!("uptime_seconds", counter);

        defmt::info!("loop iteration {}", counter);
    }
}

fn read_nrf_reset_reason() -> RebootReason {
    let resetreas = unsafe { core::ptr::read_volatile(0x40000400 as *const u32) };
    unsafe { core::ptr::write_volatile(0x40000400 as *mut u32, resetreas) };
    match resetreas {
        r if r & 0x01 != 0 => RebootReason::PinReset,
        r if r & 0x02 != 0 => RebootReason::WatchdogTimeout,
        r if r & 0x04 != 0 => RebootReason::SoftwareReset,
        r if r & 0x10000 != 0 => RebootReason::HardFault,
        _ => RebootReason::PowerOnReset,
    }
}
