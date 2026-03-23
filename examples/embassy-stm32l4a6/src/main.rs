//! Ferrite SDK — Nucleo-L4A6ZG with USB CDC transport.
//!
//! Uses MSI 48MHz for USB clock, HSI 16MHz PLL for 80MHz SYSCLK.
//! Embassy executor with manual WFI loop (WFE has wake issues on this chip).

#![no_std]
#![no_main]

use embassy_stm32::gpio;
use embassy_stm32::usb_otg::{self, Driver};
use embassy_stm32::{bind_interrupts, peripherals};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::UsbDevice;
use ferrite_sdk::transport::usb_cdc::UsbCdcTransport;
use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};
use static_cell::StaticCell;
use cortex_m_rt::entry;

use defmt_rtt as _;

// Use panic-halt instead of panic-probe — probe causes reset loop without debugger
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop { cortex_m::asm::nop(); }
}

const DEVICE_ID: &str = "stm32l4a6-fleet-01";

bind_interrupts!(struct Irqs {
    OTG_FS => usb_otg::InterruptHandler<peripherals::USB_OTG_FS>;
});

static EP_OUT_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static CDC_STATE: StaticCell<State> = StaticCell::new();
static EXECUTOR: StaticCell<embassy_executor::raw::Executor> = StaticCell::new();

#[entry]
fn main() -> ! {
    let mut config = embassy_stm32::Config::default();
    {
        use embassy_stm32::rcc::*;
        // HSI 16MHz sysclk — no USB clock yet (debug first)
        config.rcc.hsi = true;
        config.rcc.mux = ClockSrc::HSI;
    }
    let p = embassy_stm32::init(config);

    defmt::info!("Ferrite L4A6 — 80MHz sysclk, 48MHz USB");

    let executor = EXECUTOR.init(embassy_executor::raw::Executor::new(cortex_m::asm::sev as *mut ()));

    // Spawn all tasks
    let spawner = executor.spawner();
    unsafe {
        spawner.spawn(main_task(p)).unwrap();
    }

    // Spin-poll the executor. WFI/WFE have wake issues on this chip
    // without a debugger attached. Spin-polling uses more power but
    // guarantees responsiveness.
    loop {
        unsafe { executor.poll() };
    }
}

#[embassy_executor::task]
async fn main_task(p: embassy_stm32::Peripherals) {
    // Ferrite SDK init
    ferrite_sdk::init(SdkConfig {
        device_id: DEVICE_ID,
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: 0,
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x20000000,
            end: 0x20050000,
        }],
    });

    let reason = read_stm32l4_reset_reason();
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    if let Some(fault) = ferrite_sdk::fault::last_fault() {
        defmt::error!("Recovered from fault: PC={:#010x} LR={:#010x}", fault.frame.pc, fault.frame.lr);
    }

    defmt::info!("SDK initialized — setting up USB CDC");

    // USB CDC
    let ep_out_buf = EP_OUT_BUF.init([0u8; 256]);
    let mut otg_config = usb_otg::Config::default();
    otg_config.vbus_detection = false;
    let driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, ep_out_buf, otg_config);

    let mut usb_config = embassy_usb::Config::new(0x1209, 0x0001);
    usb_config.manufacturer = Some("Ferrite");
    usb_config.product = Some("Ferrite Fleet Device");
    usb_config.serial_number = Some(DEVICE_ID);

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 0]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

    let mut builder = embassy_usb::Builder::new(
        driver,
        usb_config,
        CONFIG_DESC.init([0u8; 256]),
        BOS_DESC.init([0u8; 256]),
        MSOS_DESC.init([]),
        CONTROL_BUF.init([0u8; 64]),
    );

    let cdc_state = CDC_STATE.init(State::new());
    let class = CdcAcmClass::new(&mut builder, cdc_state, 64);
    let usb_device = builder.build();

    let transport = UsbCdcTransport::new(class);

    // Can't spawn from here (no spawner access), so run USB inline
    // We'll use select to run USB + telemetry concurrently
    let usb_fut = run_usb(usb_device);
    let telemetry_fut = telemetry_loop(p.PB7, p.PB14);
    let upload_fut = upload_loop(transport);

    // Run all three concurrently (first to complete wins, but none should complete)
    embassy_futures::join::join3(usb_fut, telemetry_fut, upload_fut).await;
}

async fn run_usb(mut device: UsbDevice<'static, Driver<'static, peripherals::USB_OTG_FS>>) {
    defmt::info!("USB device task started");
    device.run().await;
}

async fn telemetry_loop(
    pb7: embassy_stm32::peripherals::PB7,
    pb14: embassy_stm32::peripherals::PB14,
) {
    let mut led_blue = gpio::Output::new(pb7, gpio::Level::Low, gpio::Speed::Low);
    let mut led_red = gpio::Output::new(pb14, gpio::Level::Low, gpio::Speed::Low);
    let mut counter: u32 = 0;

    defmt::info!("Telemetry loop started");

    loop {
        led_red.toggle(); // heartbeat on red LED (PB14)
        counter += 1;
        let _ = ferrite_sdk::metric_increment!("loop_count");
        let _ = ferrite_sdk::metric_gauge!("uptime_seconds", counter);

        if counter % 10 == 0 {
            led_blue.toggle();
        }

        if counter % 30 == 0 {
            defmt::info!("metrics: loop_count={}, uptime={}s", counter, counter);
        }

        Timer::after(Duration::from_secs(1)).await;
    }
}

async fn upload_loop(
    mut transport: UsbCdcTransport<'static, Driver<'static, peripherals::USB_OTG_FS>>,
) {
    // Wait for USB connection + initial metrics
    Timer::after(Duration::from_secs(10)).await;

    loop {
        defmt::info!("Upload starting...");
        match ferrite_sdk::upload::UploadManager::upload_async(&mut transport).await {
            Ok(stats) => {
                defmt::info!("Upload OK: {} chunks, {} bytes", stats.chunks_sent, stats.bytes_sent);
                let _ = ferrite_sdk::metric_increment!("upload_ok");
            }
            Err(e) => {
                defmt::warn!("Upload failed: {:?}", defmt::Debug2Format(&e));
                let _ = ferrite_sdk::metric_increment!("upload_fail");
            }
        }
        Timer::after(Duration::from_secs(30)).await;
    }
}

fn read_stm32l4_reset_reason() -> RebootReason {
    let rcc_csr = unsafe { core::ptr::read_volatile(0x4002_1094 as *const u32) };
    unsafe {
        let val = core::ptr::read_volatile(0x4002_1094 as *const u32);
        core::ptr::write_volatile(0x4002_1094 as *mut u32, val | (1 << 23));
    }
    match rcc_csr {
        r if r & (1 << 29) != 0 => RebootReason::WatchdogTimeout,
        r if r & (1 << 30) != 0 => RebootReason::WatchdogTimeout,
        r if r & (1 << 28) != 0 => RebootReason::SoftwareReset,
        r if r & (1 << 26) != 0 => RebootReason::PinReset,
        _ => RebootReason::PowerOnReset,
    }
}
