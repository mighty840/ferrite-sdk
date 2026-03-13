//! Ferrite SDK example — NUCLEO-WL55JC1 with LoRa transport.
//!
//! This firmware uses the STM32WL55's built-in SubGHz radio (SX1262-compatible)
//! to transmit telemetry chunks over LoRa. The ferrite-gateway receives packets
//! via a companion LoRa radio and forwards them to ferrite-server.
//!
//! # Hardware
//! - Board: NUCLEO-WL55JC1
//! - MCU: STM32WL55JCIx (Cortex-M4 + Cortex-M0+ dual core)
//! - Radio: Built-in SubGHz (SX1262-compatible)
//! - Transport: LoRa P2P → ferrite-gateway → HTTP → server
//! - Target: thumbv7em-none-eabihf
//!
//! # Architecture
//! ```text
//! [NUCLEO-WL55JC1]                 [RPi Gateway]             [Server]
//!   SubGHz radio ───LoRa 915MHz──▶  LoRa receiver  ──HTTP──▶  /ingest/chunks
//!   (SX1262 internal)                (SX1262 USB)
//! ```
//!
//! # Radio Configuration
//! - Frequency: 915 MHz (US ISM band)
//! - Spreading Factor: 7 (fastest, ~5.5 kbps)
//! - Bandwidth: 125 kHz
//! - Max payload: 222 bytes per packet
//!
//! # Flash & monitor
//! ```bash
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_stm32::gpio;
use embassy_stm32::spi;
use embassy_stm32::subghz::*;
use embassy_time::{Duration, Timer};
use ferrite_sdk::transport::lora::LoraConfig;
use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};

use defmt_rtt as _;
use panic_probe as _;

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

const DEVICE_ID: &str = "stm32wl55-fleet-01";

/// LoRa radio configuration — matches ferrite-gateway's receiver settings.
const LORA_CONFIG: LoraConfig = LoraConfig {
    frequency: 915_000_000, // US ISM band
    spreading_factor: 7,
    bandwidth: 0,   // 125 kHz
    coding_rate: 1,  // 4/5
    tx_power: 14,    // dBm
};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    // STM32WL55 uses MSI at 48MHz by default; configure clocks for SubGHz radio
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: embassy_stm32::time::Hertz(32_000_000),
            mode: HseMode::Bypass, // TCXO on NUCLEO-WL55JC1
        });
        config.rcc.sys = Sysclk::HSE;
    }
    let p = embassy_stm32::init(config);

    // Initialize ferrite SDK
    ferrite_sdk::init(SdkConfig {
        device_id: DEVICE_ID,
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: build_id::get(),
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x20000000,
            end: 0x20010000, // 64KB RAM
        }],
    });

    // Record reboot reason from STM32WL RCC_CSR register
    let reason = read_stm32wl_reset_reason();
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    // Check for previous fault
    if let Some(fault) = ferrite_sdk::fault::last_fault() {
        defmt::error!(
            "Recovered from fault: PC={:#010x} LR={:#010x}",
            fault.frame.pc,
            fault.frame.lr
        );
    }

    defmt::info!(
        "Ferrite STM32WL55 LoRa example started — device_id={}",
        DEVICE_ID
    );
    defmt::info!(
        "LoRa config: {}MHz SF{} BW125kHz {}dBm",
        LORA_CONFIG.frequency / 1_000_000,
        LORA_CONFIG.spreading_factor,
        LORA_CONFIG.tx_power,
    );
    defmt::info!(
        "Max LoRa payload: {} bytes",
        LORA_CONFIG.max_payload(),
    );

    // ── SubGHz Radio Setup ────────────────────────────────────────────
    //
    // The STM32WL's SubGHz radio is accessed via an internal SPI-like interface.
    // embassy-stm32 provides the `SubGhz` peripheral for direct access.
    //
    // Full integration with lora-phy:
    //
    //   let mut radio = SubGhz::new(p.SUBGHZSPI, NoDma, NoDma);
    //   // Configure radio via SubGHz HAL commands:
    //   radio.set_standby(StandbyClk::Rc).unwrap();
    //   radio.set_packet_type(PacketType::LoRa).unwrap();
    //   radio.set_rf_frequency(&RfFreq::from_frequency(LORA_CONFIG.frequency)).unwrap();
    //   radio.set_lora_mod_params(&LoRaModParams::new()
    //       .set_sf(SpreadingFactor::Sf7)
    //       .set_bw(LoRaBandwidth::Bw125)
    //       .set_cr(CodingRate::Cr45)
    //   ).unwrap();
    //   radio.set_tx_params(LORA_CONFIG.tx_power as u8, RampTime::Micros40).unwrap();
    //
    //   // Then wrap in LoraTransport and use with ferrite_embassy::upload_task
    //
    // For now, this example demonstrates SDK setup and metrics collection.
    // Wire up the SubGHz radio when hardware arrives for testing.

    // ── LEDs: NUCLEO-WL55JC1 has 3 user LEDs ─────────────────────────
    // LED1 (blue)  = PB15
    // LED2 (green) = PB09
    // LED3 (red)   = PB11

    let mut led_blue = gpio::Output::new(p.PB15, gpio::Level::Low, gpio::Speed::Low);
    let mut led_green = gpio::Output::new(p.PB9, gpio::Level::Low, gpio::Speed::Low);
    let mut led_red = gpio::Output::new(p.PB11, gpio::Level::Low, gpio::Speed::Low);

    let mut counter: u32 = 0;
    let mut tx_count: u32 = 0;

    loop {
        // ── Heartbeat: blue LED blinks every iteration ────────────────
        led_blue.set_high();
        Timer::after_millis(100).await;
        led_blue.set_low();

        counter += 1;
        ferrite_sdk::metric_increment!("loop_count");
        ferrite_sdk::metric_gauge!("uptime_seconds", counter * 10);

        // ── Simulated LoRa TX every 6 iterations (60 seconds) ─────────
        if counter % 6 == 0 {
            tx_count += 1;

            // Green LED on during "transmission"
            led_green.set_high();
            Timer::after_millis(200).await; // Simulated TX time
            led_green.set_low();

            ferrite_sdk::metric_increment!("lora_tx_count");
            ferrite_sdk::metric_gauge!("lora_tx_total", tx_count);

            defmt::info!(
                "LoRa TX #{}: would send {} bytes at SF{}",
                tx_count,
                LORA_CONFIG.max_payload(),
                LORA_CONFIG.spreading_factor,
            );
        }

        // Red LED flash on error (none in this demo)
        let _ = &led_red;

        defmt::trace!("loop iteration {}", counter);

        Timer::after(Duration::from_secs(10)).await;
    }
}

/// Read the STM32WL55 reset reason from RCC_CSR register.
///
/// RCC_CSR is at 0x5800_0094 on STM32WL55.
fn read_stm32wl_reset_reason() -> RebootReason {
    let rcc_csr = unsafe { core::ptr::read_volatile(0x5800_0094 as *const u32) };
    // Clear reset flags by setting RMVF bit (bit 23)
    unsafe {
        let val = core::ptr::read_volatile(0x5800_0094 as *const u32);
        core::ptr::write_volatile(0x5800_0094 as *mut u32, val | (1 << 23));
    }

    match rcc_csr {
        r if r & (1 << 29) != 0 => RebootReason::WatchdogTimeout, // IWDG
        r if r & (1 << 30) != 0 => RebootReason::WatchdogTimeout, // WWDG
        r if r & (1 << 28) != 0 => RebootReason::SoftwareReset,   // SFTRSTF
        r if r & (1 << 26) != 0 => RebootReason::PinReset,        // PINRSTF
        _ => RebootReason::PowerOnReset,
    }
}
