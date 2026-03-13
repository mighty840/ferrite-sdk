//! Ferrite SDK example — ESP32-C3 with WiFi/HTTP transport.
//!
//! This firmware connects to WiFi and POSTs telemetry chunks directly to the
//! ferrite-server's `/ingest/chunks` endpoint over HTTP. No gateway needed.
//!
//! # Hardware
//! - Board: ESP32-C3-DevKitM-1-N4X
//! - Transport: WiFi → HTTP POST
//! - Target: riscv32imc-unknown-none-elf
//!
//! # Configuration
//! Update `WIFI_SSID`, `WIFI_PASS`, and `SERVER_URL` below before flashing.
//!
//! # Flash & monitor
//! ```bash
//! cargo run --release
//! ```

#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_net::{Config, Ipv4Address, Ipv4Cidr, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
    WifiState,
};
use esp_wifi::EspWifiController;
use static_cell::StaticCell;

use ferrite_sdk::{RamRegion, SdkConfig};

// ── Configuration ─────────────────────────────────────────────────────

const WIFI_SSID: &str = "your-ssid";
const WIFI_PASS: &str = "your-password";
const SERVER_URL: &str = "http://192.168.1.100:4000/ingest/chunks";
const INGEST_API_KEY: Option<&str> = Some("changeme");
const DEVICE_ID: &str = "esp32c3-fleet-01";

// ── Build ID ──────────────────────────────────────────────────────────

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

// ── Static resources ──────────────────────────────────────────────────

static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

// ── Entry point ───────────────────────────────────────────────────────

#[esp_hal::main]
async fn main(spawner: Spawner) {
    esp_println::println!("Ferrite ESP32-C3 WiFi example starting...");

    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Heap allocator (esp-wifi needs it)
    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);

    // Initialize embassy time driver
    esp_hal_embassy::init(timg0.timer0);

    // Initialize WiFi
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let wifi_init = esp_wifi::init(timg1.timer0, rng, peripherals.RADIO_CLK).unwrap();

    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&wifi_init, peripherals.WIFI, WifiStaDevice).unwrap();

    // Initialize ferrite SDK (no cortex-m features on RISC-V)
    ferrite_sdk::init(SdkConfig {
        device_id: DEVICE_ID,
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: build_id::get(),
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x3FC80000, // ESP32-C3 SRAM
            end: 0x3FCE0000,
        }],
    });

    ferrite_sdk::reboot_reason::record_reboot_reason(ferrite_sdk::RebootReason::PowerOnReset);

    // Network stack
    let config = Config::dhcpv4(Default::default());
    let resources = STACK_RESOURCES.init(StackResources::new());
    let seed = 0x1234_5678_9abc_def0u64; // Use RNG in production
    let (stack, runner) = embassy_net::new(wifi_interface, config, resources, seed);

    spawner.spawn(net_task(runner)).ok();
    spawner.spawn(wifi_connect_task(controller)).ok();

    // Wait for IP address
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after_millis(500).await;
    }

    esp_println::println!("WiFi connected, waiting for IP...");

    loop {
        if let Some(config) = stack.config_v4() {
            esp_println::println!("Got IP: {}", config.address.address());
            break;
        }
        Timer::after_millis(500).await;
    }

    esp_println::println!("Network ready — starting telemetry loop");

    // ── Main telemetry loop ──────────────────────────────────────────

    let mut counter: u32 = 0;

    loop {
        Timer::after(Duration::from_secs(5)).await;

        counter += 1;
        ferrite_sdk::metric_increment!("loop_count");
        ferrite_sdk::metric_gauge!("uptime_seconds", counter * 5);

        // Simulated sensor readings
        ferrite_sdk::metric_gauge!("wifi_rssi", -55i32 as u32);
        ferrite_sdk::metric_gauge!("free_heap", esp_alloc::HEAP.free() as u32);

        esp_println::println!("[{}] metrics recorded, awaiting upload cycle", counter);

        // In production, spawn the ferrite_embassy::upload_task with an
        // HttpTransport wrapping a TCP socket from the network stack.
        // The upload task handles periodic POSTs to the server.
        //
        // Example (once TCP connect is wired up):
        //
        //   let tcp = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        //   let transport = HttpTransport::new(tcp, SERVER_URL, INGEST_API_KEY);
        //   ferrite_embassy::upload_task::upload_loop(transport, Duration::from_secs(60)).await;
    }
}

// ── Background tasks ──────────────────────────────────────────────────

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, WifiDevice<'static, WifiStaDevice>>) {
    runner.run().await;
}

#[embassy_executor::task]
async fn wifi_connect_task(mut controller: WifiController<'static>) {
    esp_println::println!("WiFi: connecting to {}", WIFI_SSID);

    loop {
        if matches!(esp_wifi::wifi::wifi_state(), WifiState::StaConnected) {
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            esp_println::println!("WiFi: disconnected, reconnecting...");
            Timer::after_secs(1).await;
        }

        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: WIFI_SSID.try_into().unwrap(),
                password: WIFI_PASS.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            controller.start_async().await.unwrap();
            esp_println::println!("WiFi: started");
        }

        match controller.connect_async().await {
            Ok(()) => esp_println::println!("WiFi: connected"),
            Err(e) => {
                esp_println::println!("WiFi: connect failed: {:?}, retrying in 5s", e);
                Timer::after_secs(5).await;
            }
        }
    }
}
