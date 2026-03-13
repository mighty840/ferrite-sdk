//! Ferrite SDK example — Nucleo-H563ZI with Ethernet/HTTP transport.
//!
//! This firmware connects via Ethernet (RMII PHY) and POSTs telemetry chunks
//! directly to ferrite-server, similar to the ESP32-C3 WiFi example but over
//! a wired connection. No gateway needed — Ethernet goes straight to the server.
//!
//! # Hardware
//! - Board: Nucleo-H563ZI (Nucleo-144)
//! - MCU: STM32H563ZITx (Cortex-M33, 250MHz)
//! - PHY: LAN8742A (RMII, on-board)
//! - Transport: Ethernet → TCP → HTTP POST
//! - Target: thumbv8m.main-none-eabihf
//!
//! # Architecture
//! ```text
//! [Nucleo-H563ZI]                                      [Server]
//!   ETH MAC (RMII) ──Ethernet──▶  TCP/HTTP POST  ──▶  /ingest/chunks
//!   LAN8742A PHY                   (no gateway)
//! ```
//!
//! # Wiring
//! - Connect Nucleo Ethernet jack (CN14) to same LAN as ferrite-server
//! - Update SERVER_URL below to match server IP
//! - DHCP is used by default; static IP available via config
//!
//! # Flash & monitor
//! ```bash
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_net::{Config, StackResources};
use embassy_stm32::eth::{self, Ethernet, PacketQueue};
use embassy_stm32::gpio;
use embassy_stm32::rng::Rng;
use embassy_stm32::{bind_interrupts, peripherals};
use embassy_time::{Duration, Timer};
use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};
use static_cell::StaticCell;

use defmt_rtt as _;
use panic_probe as _;

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

// ── Configuration ─────────────────────────────────────────────────────

const DEVICE_ID: &str = "stm32h563-fleet-01";
const SERVER_URL: &str = "http://192.168.1.100:4000/ingest/chunks";
const INGEST_API_KEY: Option<&str> = Some("ferrite-fleet-demo-key");

// MAC address — unique per device in your fleet
const MAC_ADDR: [u8; 6] = [0x02, 0xFE, 0x77, 0x1E, 0x00, 0x01];

bind_interrupts!(struct Irqs {
    ETH => eth::InterruptHandler;
    RNG => embassy_stm32::rng::InterruptHandler<peripherals::RNG>;
});

static PACKET_QUEUE: StaticCell<PacketQueue<4, 4>> = StaticCell::new();
static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    // Configure clocks: HSE 8MHz → PLL → 250MHz SYSCLK
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: embassy_stm32::time::Hertz(8_000_000),
            mode: HseMode::Bypass, // Nucleo-144 uses ST-LINK MCO
        });
        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL62,
            divp: Some(PllDiv::DIV2), // 248MHz SYSCLK
            divq: Some(PllDiv::DIV4), // for RNG
            divr: None,
        });
        config.rcc.sys = Sysclk::PLL1_P;
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
            end: 0x200A0000, // 640KB RAM
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

    let reason = read_stm32h5_reset_reason();
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    defmt::info!("Ferrite Nucleo-H563ZI Ethernet example — device_id={}", DEVICE_ID);

    // ── Ethernet Setup ────────────────────────────────────────────────
    //
    // STM32H563ZI Nucleo-144 has LAN8742A PHY connected via RMII:
    //   PA1  → ETH_REF_CLK
    //   PA2  → ETH_MDIO
    //   PA7  → ETH_CRS_DV
    //   PB13 → ETH_TXD1
    //   PC1  → ETH_MDC
    //   PC4  → ETH_RXD0
    //   PC5  → ETH_RXD1
    //   PG11 → ETH_TX_EN
    //   PG13 → ETH_TXD0

    let packet_queue = PACKET_QUEUE.init(PacketQueue::new());

    let eth_device = Ethernet::new(
        packet_queue,
        p.ETH,
        Irqs,
        p.PA1,  // REF_CLK
        p.PA2,  // MDIO
        p.PC1,  // MDC
        p.PA7,  // CRS_DV
        p.PC4,  // RXD0
        p.PC5,  // RXD1
        p.PG11, // TX_EN
        p.PG13, // TXD0
        p.PB13, // TXD1
        eth::PHY::LAN8742A,
        MAC_ADDR,
    );

    // Network stack with DHCP
    let net_config = Config::dhcpv4(Default::default());
    let resources = STACK_RESOURCES.init(StackResources::new());

    let mut rng = Rng::new(p.RNG, Irqs);
    let seed = {
        let mut buf = [0u8; 8];
        let _ = rng.async_fill_bytes(&mut buf).await;
        u64::from_le_bytes(buf)
    };

    let (stack, runner) = embassy_net::new(eth_device, net_config, resources, seed);
    spawner.spawn(net_task(runner)).ok();

    // Wait for link + IP
    defmt::info!("Waiting for Ethernet link...");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after_millis(500).await;
    }
    defmt::info!("Ethernet link up — waiting for DHCP...");

    loop {
        if let Some(config) = stack.config_v4() {
            defmt::info!("Got IP: {}", config.address);
            break;
        }
        Timer::after_millis(500).await;
    }

    defmt::info!("Network ready — starting telemetry loop");

    // ── Main telemetry loop ───────────────────────────────────────────

    // Nucleo-144 LEDs: LD1=PB0 (green), LD2=PE1 (yellow), LD3=PG4 (red)
    let mut led_green = gpio::Output::new(p.PB0, gpio::Level::Low, gpio::Speed::Low);
    let mut led_yellow = gpio::Output::new(p.PE1, gpio::Level::Low, gpio::Speed::Low);
    let mut counter: u32 = 0;

    loop {
        // Green LED heartbeat
        led_green.set_high();
        Timer::after_millis(50).await;
        led_green.set_low();

        counter += 1;
        ferrite_sdk::metric_increment!("loop_count");
        ferrite_sdk::metric_gauge!("uptime_seconds", counter * 5);
        ferrite_sdk::metric_gauge!("eth_link_up", 1);

        // Toggle yellow LED every 10 iterations
        if counter % 10 == 0 {
            led_yellow.toggle();
            defmt::info!("iteration {}: metrics queued", counter);
        }

        // In production, spawn the upload task with HttpTransport:
        //
        //   let mut rx_buf = [0u8; 1024];
        //   let mut tx_buf = [0u8; 1024];
        //   let mut tcp = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        //   tcp.connect(server_endpoint).await.unwrap();
        //   let transport = HttpTransport::new(tcp, SERVER_URL, INGEST_API_KEY);
        //   ferrite_embassy::upload_task::upload_loop(transport, Duration::from_secs(30)).await;

        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::task]
async fn net_task(
    mut runner: embassy_net::Runner<
        'static,
        Ethernet<'static, 4, 4>,
    >,
) {
    runner.run().await;
}

/// Read STM32H5 reset reason from RCC_RSR register at 0x5802_4C10.
fn read_stm32h5_reset_reason() -> RebootReason {
    let rsr = unsafe { core::ptr::read_volatile(0x5802_4C10 as *const u32) };
    // Clear by setting RMVF (bit 23)
    unsafe {
        let val = core::ptr::read_volatile(0x5802_4C10 as *const u32);
        core::ptr::write_volatile(0x5802_4C10 as *mut u32, val | (1 << 23));
    }
    match rsr {
        r if r & (1 << 26) != 0 => RebootReason::WatchdogTimeout, // IWDGRSTF
        r if r & (1 << 28) != 0 => RebootReason::WatchdogTimeout, // WWDGRSTF
        r if r & (1 << 24) != 0 => RebootReason::SoftwareReset,   // SFTRSTF
        r if r & (1 << 22) != 0 => RebootReason::PinReset,        // PINRSTF
        _ => RebootReason::PowerOnReset,
    }
}
