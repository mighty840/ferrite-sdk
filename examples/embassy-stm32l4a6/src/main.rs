//! Ferrite SDK example — Nucleo-L4A6ZG with USB CDC transport.
//!
//! This firmware sends telemetry chunks over USB CDC (virtual serial) to the
//! ferrite-gateway running on the connected Raspberry Pi. The gateway decodes
//! the wire-format frames and forwards them to ferrite-server via HTTP.
//!
//! # Hardware
//! - Board: Nucleo-L4A6ZG (Nucleo-144)
//! - MCU: STM32L4A6ZGTx (Cortex-M4F, 80MHz)
//! - Transport: USB OTG FS → CDC ACM → ferrite-gateway
//! - Target: thumbv7em-none-eabihf
//!
//! # Architecture
//! ```text
//! [Nucleo-L4A6ZG]               [RPi Gateway]              [Server]
//!   USB OTG FS ──USB CDC──▶  /dev/ttyACM0  ──HTTP──▶  /ingest/chunks
//!   (PA11/PA12)               ferrite-gateway
//! ```
//!
//! # Wiring
//! - Connect Nucleo USB CN13 (USB OTG) to RPi USB port
//! - The ST-LINK USB (CN1) is separate — used for flashing/debug only
//!
//! # Flash & monitor
//! ```bash
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_stm32::gpio;
use embassy_stm32::usb::{self, Driver};
use embassy_stm32::{bind_interrupts, peripherals};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::UsbDevice;
use ferrite_sdk::transport::UsbCdcTransport;
use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};
use static_cell::StaticCell;

use defmt_rtt as _;
use panic_probe as _;

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

const DEVICE_ID: &str = "stm32l4a6-fleet-01";

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

// Static buffers for USB (must outlive the USB driver)
static EP_OUT_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static CDC_STATE: StaticCell<State> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    // Configure clocks: HSE → PLL → 80MHz, USB needs 48MHz from PLLSAI1
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: embassy_stm32::time::Hertz(8_000_000),
            mode: HseMode::Bypass, // Nucleo-144 uses ST-LINK MCO
        });
        config.rcc.pll = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL20,
            divp: None,
            divq: Some(PllQDiv::DIV4), // 40MHz for USB? needs 48MHz
            divr: Some(PllRDiv::DIV2), // 80MHz SYSCLK
        });
        config.rcc.sys = Sysclk::PLL1_R;
        // 48MHz clock for USB from PLLSAI1
        config.rcc.pllsai1 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL12,    // 8 * 12 = 96MHz
            divp: None,
            divq: Some(PllQDiv::DIV2), // 48MHz for USB
            divr: None,
        });
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
            end: 0x20050000, // 320KB SRAM
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

    // Record reboot reason from STM32L4 RCC_CSR
    let reason = read_stm32l4_reset_reason();
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    defmt::info!("Ferrite Nucleo-L4A6ZG USB CDC example started — device_id={}", DEVICE_ID);

    // ── USB CDC Setup ─────────────────────────────────────────────────

    let ep_out_buf = EP_OUT_BUF.init([0u8; 256]);

    let driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, ep_out_buf);

    let mut usb_config = embassy_usb::Config::new(0x1209, 0x0001); // pid.codes test VID/PID
    usb_config.manufacturer = Some("Ferrite");
    usb_config.product = Some("Ferrite Fleet Device");
    usb_config.serial_number = Some(DEVICE_ID);

    let mut builder = embassy_usb::Builder::new(
        driver,
        usb_config,
        &mut make_buf::<256>(),
        &mut make_buf::<256>(),
        &mut make_buf::<256>(),
        &mut make_buf::<64>(),
    );

    let cdc_state = CDC_STATE.init(State::new());
    let cdc_class = CdcAcmClass::new(&mut builder, cdc_state, 64);
    let usb_device = builder.build();

    // Split CDC class into transport
    let transport = UsbCdcTransport::new(cdc_class);

    // Spawn USB device task (handles enumeration, control transfers)
    spawner.spawn(usb_task(usb_device)).ok();

    defmt::info!("USB CDC initialized — waiting for host connection (DTR)");

    // Spawn upload task using the USB transport
    spawner.spawn(upload_task(transport)).ok();

    // ── Main telemetry loop ───────────────────────────────────────────

    // Nucleo-144 LEDs: LD1=PB0 (green), LD2=PB7 (blue), LD3=PB14 (red)
    let mut led_green = gpio::Output::new(p.PB0, gpio::Level::Low, gpio::Speed::Low);
    let mut led_blue = gpio::Output::new(p.PB7, gpio::Level::Low, gpio::Speed::Low);
    let mut counter: u32 = 0;

    loop {
        // Green LED heartbeat
        led_green.set_high();
        Timer::after_millis(100).await;
        led_green.set_low();

        counter += 1;
        ferrite_sdk::metric_increment!("loop_count");
        ferrite_sdk::metric_gauge!("uptime_seconds", counter);

        // Blue LED toggles every 10 iterations
        if counter % 10 == 0 {
            led_blue.toggle();
        }

        defmt::trace!("loop {} — metrics queued", counter);
        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::task]
async fn usb_task(mut device: UsbDevice<'static, Driver<'static, peripherals::USB_OTG_FS>>) {
    device.run().await;
}

#[embassy_executor::task]
async fn upload_task(transport: UsbCdcTransport<'static, Driver<'static, peripherals::USB_OTG_FS>>) {
    // Upload every 30 seconds, or on-demand via trigger_upload_now()
    ferrite_embassy::upload_task::upload_loop_with_trigger(
        transport,
        Duration::from_secs(30),
    )
    .await
}

/// Read STM32L4 reset reason from RCC_CSR register at 0x4002_1094.
fn read_stm32l4_reset_reason() -> RebootReason {
    let rcc_csr = unsafe { core::ptr::read_volatile(0x4002_1094 as *const u32) };
    // Clear flags by setting RMVF (bit 23)
    unsafe {
        let val = core::ptr::read_volatile(0x4002_1094 as *const u32);
        core::ptr::write_volatile(0x4002_1094 as *mut u32, val | (1 << 23));
    }
    match rcc_csr {
        r if r & (1 << 29) != 0 => RebootReason::WatchdogTimeout, // IWDGRSTF
        r if r & (1 << 30) != 0 => RebootReason::WatchdogTimeout, // WWDGRSTF
        r if r & (1 << 28) != 0 => RebootReason::SoftwareReset,   // SFTRSTF
        r if r & (1 << 26) != 0 => RebootReason::PinReset,        // PINRSTF
        _ => RebootReason::PowerOnReset,
    }
}

fn make_buf<const N: usize>() -> [u8; N] {
    [0u8; N]
}
