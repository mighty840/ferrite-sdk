//! Ferrite SDK example — ESP32-C3 with WiFi/HTTP transport.
//!
//! Connects to WiFi, collects telemetry via ferrite-sdk, and POSTs chunks
//! directly to ferrite-server over HTTP. No gateway needed.
//!
//! # Hardware
//! - Board: ESP32-C3-DevKitM-1-N4X
//! - Transport: WiFi → HTTP POST to ferrite-server
//! - Target: riscv32imc-unknown-none-elf
//!
//! # Configuration
//! Update `WIFI_SSID`, `WIFI_PASS`, and `SERVER_HOST`/`SERVER_PORT` below.
//!
//! # Flash & monitor
//! ```bash
//! cargo run --release
//! ```

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Ipv4Address, StackResources};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use esp_backtrace as _;
use esp_hal::rng::Rng;
use esp_hal::timer::timg::TimerGroup;
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
    WifiState,
};
use static_cell::StaticCell;

use ferrite_sdk::transport::ChunkTransport;
use ferrite_sdk::upload::UploadManager;

// ── Configuration ─────────────────────────────────────────────────────

const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASS: &str = env!("WIFI_PASS");
const SERVER_PORT: u16 = 4000;
const INGEST_PATH: &str = "/ingest/chunks";
const INGEST_API_KEY: Option<&str> = option_env!("INGEST_API_KEY");
const DEVICE_ID: &str = "esp32c3-embassy-01";

/// Parsed at compile time from SERVER_HOST env var (e.g. "192.168.1.100").
const SERVER_HOST: Ipv4Address = {
    const BYTES: [u8; 4] = parse_ipv4(env!("SERVER_HOST").as_bytes());
    Ipv4Address::new(BYTES[0], BYTES[1], BYTES[2], BYTES[3])
};

/// Compile-time IPv4 parser (const fn).
const fn parse_ipv4(s: &[u8]) -> [u8; 4] {
    let mut octets = [0u8; 4];
    let mut octet_idx = 0;
    let mut current: u16 = 0;
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'.' {
            octets[octet_idx] = current as u8;
            octet_idx += 1;
            current = 0;
        } else {
            current = current * 10 + (s[i] - b'0') as u16;
        }
        i += 1;
    }
    octets[octet_idx] = current as u8;
    octets
}

// ── Static resources ──────────────────────────────────────────────────

static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

// ── Buffering transport ───────────────────────────────────────────────

/// A ChunkTransport that buffers encoded chunks in memory.
/// After `UploadManager::upload()` collects them synchronously,
/// we drain the buffer and POST each chunk over async TCP.
struct BufferingTransport {
    chunks: heapless::Vec<heapless::Vec<u8, 256>, 32>,
}

impl BufferingTransport {
    fn new() -> Self {
        Self {
            chunks: heapless::Vec::new(),
        }
    }
}

#[derive(Debug)]
struct BufferFull;

impl ChunkTransport for BufferingTransport {
    type Error = BufferFull;

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        let mut v = heapless::Vec::new();
        v.extend_from_slice(chunk).map_err(|_| BufferFull)?;
        self.chunks.push(v).map_err(|_| BufferFull)?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        true
    }
}

// ── Entry point ───────────────────────────────────────────────────────

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::println!("Ferrite ESP32-C3 WiFi example starting...");

    let config = esp_hal::Config::default();
    let peripherals = esp_hal::init(config);

    // Heap allocator (esp-wifi needs it)
    // Note: esp_alloc::heap_allocator! can only be called once globally.
    // If esp_hal_embassy::main already sets one up, remove this line.
    esp_alloc::heap_allocator!(72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);

    // Initialize embassy time driver
    esp_hal_embassy::init(timg0.timer0);

    // Initialize WiFi — wifi_init must be 'static
    let timg1 = TimerGroup::new(peripherals.TIMG1);
    let wifi_init = {
        static WIFI_INIT: StaticCell<esp_wifi::EspWifiController<'static>> = StaticCell::new();
        WIFI_INIT.init(esp_wifi::init(timg1.timer0, rng, peripherals.RADIO_CLK).unwrap())
    };

    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(wifi_init, peripherals.WIFI, WifiStaDevice).unwrap();

    // Initialize ferrite SDK
    ferrite_sdk::init(ferrite_sdk::SdkConfig {
        device_id: DEVICE_ID,
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: 0,
        ticks_fn: || embassy_time::Instant::now().as_millis(),
        ram_regions: &[ferrite_sdk::RamRegion {
            start: 0x3FC80000,
            end: 0x3FCE0000,
        }],
    });

    ferrite_sdk::reboot_reason::record_reboot_reason(ferrite_sdk::RebootReason::PowerOnReset);

    // Network stack
    let net_config = Config::dhcpv4(Default::default());
    let resources = STACK_RESOURCES.init(StackResources::new());
    let seed = 0x1234_5678_9abc_def0u64;
    let (stack, runner) = embassy_net::new(wifi_interface, net_config, resources, seed);
    let stack: &'static _ = {
        static STACK: StaticCell<embassy_net::Stack<'static>> = StaticCell::new();
        STACK.init(stack)
    };

    spawner.spawn(net_task(runner)).ok();
    spawner.spawn(wifi_connect_task(controller)).ok();

    // Wait for IP
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after_millis(500).await;
    }
    esp_println::println!("WiFi connected, waiting for IP...");

    loop {
        if let Some(cfg) = stack.config_v4() {
            esp_println::println!("Got IP: {}", cfg.address.address());
            break;
        }
        Timer::after_millis(500).await;
    }

    esp_println::println!("Network ready — starting telemetry loop");

    // ── Main loop: collect metrics + upload ────────────────────────────

    let mut counter: u32 = 0;

    loop {
        Timer::after(Duration::from_secs(5)).await;

        counter += 1;
        ferrite_sdk::metric_increment!("loop_count");
        ferrite_sdk::metric_gauge!("uptime_seconds", counter * 5);
        ferrite_sdk::metric_gauge!("free_heap", esp_alloc::HEAP.free() as u32);

        esp_println::println!("[{}] metrics recorded", counter);

        // Upload every 30 seconds (6 × 5s)
        if counter % 6 == 0 {
            // Step 1: collect chunks synchronously into buffer
            let mut transport = BufferingTransport::new();
            match UploadManager::upload(&mut transport) {
                Ok(stats) => {
                    esp_println::println!(
                        "encoded {} chunks ({} bytes) — sending via HTTP",
                        stats.chunks_sent,
                        stats.bytes_sent,
                    );
                    // Step 2: send buffered chunks over async TCP
                    let mut ok = 0u32;
                    let mut fail = 0u32;
                    for chunk in &transport.chunks {
                        match http_post_chunk(*stack, chunk).await {
                            Ok(()) => ok += 1,
                            Err(()) => {
                                fail += 1;
                                break;
                            }
                        }
                    }
                    if fail == 0 {
                        esp_println::println!("upload OK: {} chunks sent", ok);
                    } else {
                        esp_println::println!("upload partial: {} ok, {} failed", ok, fail);
                    }
                }
                Err(e) => {
                    esp_println::println!("upload encode failed: {:?}", e);
                }
            }
        }
    }
}

// ── Raw HTTP POST ─────────────────────────────────────────────────────

async fn http_post_chunk(stack: embassy_net::Stack<'static>, chunk: &[u8]) -> Result<(), ()> {
    let mut rx_buf = [0u8; 512];
    let mut tx_buf = [0u8; 512];
    let mut tcp = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    tcp.set_timeout(Some(Duration::from_secs(10)));

    tcp.connect((SERVER_HOST, SERVER_PORT))
        .await
        .map_err(|_| ())?;

    // Build minimal HTTP/1.0 POST
    let mut header_buf = [0u8; 256];
    let header_len = format_http_header(&mut header_buf, chunk.len());
    tcp.write_all(&header_buf[..header_len])
        .await
        .map_err(|_| ())?;
    tcp.write_all(chunk).await.map_err(|_| ())?;
    tcp.flush().await.map_err(|_| ())?;

    // Read response status
    let mut resp_buf = [0u8; 64];
    let n = tcp.read(&mut resp_buf).await.map_err(|_| ())?;
    tcp.close();

    // Check "HTTP/1.x 2xx"
    if n >= 12 && resp_buf[9] == b'2' {
        Ok(())
    } else {
        Err(())
    }
}

/// Format an HTTP/1.0 POST header into the buffer, return bytes written.
fn format_http_header(buf: &mut [u8], content_length: usize) -> usize {
    let mut pos = 0;

    macro_rules! w {
        ($s:expr) => {
            let bytes = $s.as_bytes();
            buf[pos..pos + bytes.len()].copy_from_slice(bytes);
            pos += bytes.len();
        };
    }

    w!("POST ");
    w!(INGEST_PATH);
    w!(" HTTP/1.0\r\nHost: ferrite\r\nContent-Type: application/octet-stream\r\nX-Device-Id: ");
    w!(DEVICE_ID);
    w!("\r\nContent-Length: ");

    let mut num_buf = [0u8; 10];
    let num_str = format_u32(&mut num_buf, content_length as u32);
    w!(num_str);
    w!("\r\n");

    if let Some(key) = INGEST_API_KEY {
        w!("X-API-Key: ");
        w!(key);
        w!("\r\n");
    }

    w!("\r\n");
    pos
}

fn format_u32<'a>(buf: &'a mut [u8; 10], mut n: u32) -> &'a str {
    if n == 0 {
        buf[0] = b'0';
        return unsafe { core::str::from_utf8_unchecked(&buf[..1]) };
    }
    let mut i = 10;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    unsafe { core::str::from_utf8_unchecked(&buf[i..]) }
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
