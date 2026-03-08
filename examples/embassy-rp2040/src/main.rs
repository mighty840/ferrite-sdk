#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::gpio;
use embassy_time::{Duration, Timer};
use ferrite_sdk::{SdkConfig, RamRegion, RebootReason};
use defmt_rtt as _;
use panic_probe as _;

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Initialize SDK
    ferrite_sdk::init(SdkConfig {
        device_id: "rp2040-example-01",
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: build_id::get(),
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x20000000,
            end: 0x20042000,
        }],
    });

    // Check for previous fault
    if let Some(fault) = ferrite_sdk::fault::last_fault() {
        defmt::error!(
            "Recovered from fault: PC={:#010x} LR={:#010x}",
            fault.frame.pc,
            fault.frame.lr
        );
    }

    // RP2040 has no RESETREAS equivalent — default to PowerOnReset
    ferrite_sdk::reboot_reason::record_reboot_reason(RebootReason::PowerOnReset);

    // Application loop
    let mut led = gpio::Output::new(p.PIN_25, gpio::Level::Low);
    let mut counter: u32 = 0;

    loop {
        led.set_high();
        Timer::after_millis(500).await;
        led.set_low();
        Timer::after_millis(500).await;

        counter += 1;
        ferrite_sdk::metric_increment!("loop_count");
        ferrite_sdk::metric_gauge!("uptime_seconds", counter);

        defmt::info!("loop iteration {}", counter);
    }
}
