//! Ferrite SDK example — nRF5340-DK with BLE transport.
//!
//! This firmware runs on the nRF5340 application core and uses BLE to stream
//! telemetry chunks to the ferrite-gateway's BLE scanner. The gateway then
//! forwards chunks to ferrite-server over HTTP.
//!
//! # Hardware
//! - Board: nRF5340-DK (PCA10095)
//! - Transport: BLE GATT notifications → ferrite-gateway
//! - Target: thumbv8m.main-none-eabihf (Cortex-M33)
//!
//! # Architecture
//! ```text
//! [nRF5340-DK]                    [RPi Gateway]             [Server]
//!   App Core (this firmware)
//!     └─ BLE GATT server  ──BLE──▶  BLE scanner  ──HTTP──▶  /ingest/chunks
//!        Service: FE771E00-0001-...
//!        Char:    FE771E00-0002-...
//! ```
//!
//! # Prerequisites
//! - Flash the Nordic BLE connectivity firmware on the network core
//! - Install target: `rustup target add thumbv8m.main-none-eabihf`
//!
//! # Flash & monitor
//! ```bash
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_nrf::{bind_interrupts, gpio, peripherals};
use embassy_time::{Duration, Timer};
use ferrite_ble_nrf::{CHUNK_CHAR_UUID, FERRITE_SERVICE_UUID, MAX_BLE_PAYLOAD};
use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};

use defmt_rtt as _;
use panic_probe as _;

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

const DEVICE_ID: &str = "nrf5340-fleet-01";

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    // Initialize SDK
    ferrite_sdk::init(SdkConfig {
        device_id: DEVICE_ID,
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: build_id::get(),
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x20000000,
            end: 0x20080000, // 512KB app core RAM
        }],
    });

    // Check if we recovered from a fault
    if let Some(fault) = ferrite_sdk::fault::last_fault() {
        defmt::error!(
            "Recovered from fault: PC={:#010x} LR={:#010x}",
            fault.frame.pc,
            fault.frame.lr
        );
    }

    // Record reboot reason from nRF5340 RESETREAS register
    let reason = read_nrf5340_reset_reason();
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    defmt::info!(
        "Ferrite nRF5340 BLE example started — device_id={}",
        DEVICE_ID
    );
    defmt::info!(
        "BLE service UUID: {:#034x}",
        FERRITE_SERVICE_UUID
    );
    defmt::info!(
        "Max BLE chunk payload: {} bytes",
        MAX_BLE_PAYLOAD
    );

    // ── BLE Setup ─────────────────────────────────────────────────────
    //
    // Full BLE integration requires nrf-softdevice:
    //
    //   let sd = Softdevice::enable(&softdevice_config);
    //   let server = gatt_server::register(sd).unwrap();
    //   let adv_data = &[
    //       0x02, 0x01, 0x06,             // flags
    //       0x11, 0x07,                    // 128-bit UUID list
    //       // FERRITE_SERVICE_UUID bytes (little-endian)
    //   ];
    //   let adv = peripheral::advertise_connectable(sd, adv_data, &scan_data).await?;
    //
    //   let transport = BleTransport::new(&server, chunk_handle);
    //   ferrite_embassy::upload_task::upload_loop(transport, Duration::from_secs(30)).await;
    //
    // For now, this example demonstrates SDK initialization, metrics collection,
    // and the LED heartbeat pattern. Wire up BLE when nrf-softdevice is configured.

    // ── LED heartbeat + metrics loop ──────────────────────────────────

    // nRF5340-DK has 4 LEDs on P0.28-P0.31
    let mut led1 = gpio::Output::new(p.P0_28, gpio::Level::High, gpio::OutputDrive::Standard);
    let mut led2 = gpio::Output::new(p.P0_29, gpio::Level::High, gpio::OutputDrive::Standard);
    let mut counter: u32 = 0;

    loop {
        // Double-blink pattern: indicates BLE advertising mode
        led1.set_low(); // LED on (active low)
        Timer::after_millis(100).await;
        led1.set_high();
        Timer::after_millis(100).await;
        led1.set_low();
        Timer::after_millis(100).await;
        led1.set_high();
        Timer::after_millis(700).await;

        counter += 1;
        ferrite_sdk::metric_increment!("loop_count");
        ferrite_sdk::metric_gauge!("uptime_seconds", counter);

        // Toggle LED2 every 10 iterations to show activity
        if counter % 10 == 0 {
            led2.toggle();
            defmt::info!("iteration {}: toggled LED2", counter);
        }

        defmt::trace!("loop iteration {}", counter);
    }
}

/// Read the nRF5340 application core reset reason register.
///
/// RESETREAS is at address 0x5000_0400 on the application core.
fn read_nrf5340_reset_reason() -> RebootReason {
    let resetreas = unsafe { core::ptr::read_volatile(0x5000_0400 as *const u32) };
    // Clear by writing 1s to read bits
    unsafe { core::ptr::write_volatile(0x5000_0400 as *mut u32, resetreas) };

    match resetreas {
        r if r & (1 << 0) != 0 => RebootReason::PinReset,
        r if r & (1 << 1) != 0 => RebootReason::WatchdogTimeout,
        r if r & (1 << 2) != 0 => RebootReason::SoftwareReset,
        r if r & (1 << 18) != 0 => RebootReason::HardFault, // CTRLAP reset
        _ => RebootReason::PowerOnReset,
    }
}
