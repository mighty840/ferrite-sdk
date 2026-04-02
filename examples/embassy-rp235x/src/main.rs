//! Ferrite SDK example — Raspberry Pi Pico 2 W (RP2350) with WiFi/HTTP transport.
//!
//! Connects to WiFi via CYW43 (RM2 module), collects telemetry via ferrite-sdk,
//! and POSTs chunks directly to ferrite-server over HTTP. No gateway needed.
//!
//! # Hardware
//! - Board: Raspberry Pi Pico 2 W (RP2350A + CYW43439 RM2)
//! - Transport: WiFi → HTTP POST to ferrite-server
//! - Target: thumbv8m.main-none-eabihf
//!
//! # Configuration
//! Set `WIFI_SSID`, `WIFI_PASS`, `SERVER_HOST` in `.cargo/config.toml` or `.env`.
//!
//! # Flash & monitor
//! ```bash
//! cargo run --release
//! ```

#![no_std]
#![no_main]
#![allow(async_fn_in_trait)]

use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Ipv4Address, StackResources};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::dma;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler as PioInterruptHandler, Pio};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

use ferrite_sdk::transport::ChunkTransport;
use ferrite_sdk::upload::UploadManager;

// ── Configuration ─────────────────────────────────────────────────────

const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASS: &str = env!("WIFI_PASS");
const SERVER_PORT: u16 = 4000;
const INGEST_PATH: &str = "/ingest/chunks";
const INGEST_API_KEY: Option<&str> = option_env!("INGEST_API_KEY");
const DEVICE_ID: &str = "pico2w-embassy-01";

/// Parsed at compile time from SERVER_HOST env var (e.g. "192.168.178.116").
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

mod build_id {
    pub fn get() -> u64 {
        env!("FERRITE_BUILD_ID").parse().unwrap_or(0)
    }
}

// ── Interrupt bindings ───────────────────────────────────────────────

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => PioInterruptHandler<PIO0>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>;
});

// ── Static resources ─────────────────────────────────────────────────

static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

// ── Buffering transport ──────────────────────────────────────────────

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

// ── Background tasks ─────────────────────────────────────────────────

#[embassy_executor::task]
async fn cyw43_task(
    runner: cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

// ── Entry point ──────────────────────────────────────────────────────

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Ferrite Pico W WiFi example starting...");

    let p = embassy_rp::init(Default::default());
    let mut rng = RoscRng;

    // ── CYW43 WiFi init ──────────────────────────────────────────────
    //
    // Firmware + CLM from cyw43-firmware crate (aligned for DMA).
    static FW: &cyw43::Aligned<cyw43::A4, [u8]> =
        &cyw43::Aligned(*cyw43_firmware::CYW43_43439A0);
    let fw = &*FW;
    let clm: &[u8] = cyw43_firmware::CYW43_43439A0_CLM;

    // Board-specific NVRAM config (was embedded in older cyw43 versions).
    static NVRAM_DATA: &cyw43::Aligned<cyw43::A4, [u8]> = &cyw43::Aligned(*b"\
        NVRAMRev=$Rev$\x00\
        manfid=0x2d0\x00\
        prodid=0x0727\x00\
        vendid=0x14e4\x00\
        devid=0x43e2\x00\
        boardtype=0x0887\x00\
        boardrev=0x1100\x00\
        boardnum=22\x00\
        macaddr=00:A0:50:b5:59:5e\x00\
        sromrev=11\x00\
        boardflags=0x00404001\x00\
        boardflags3=0x04000000\x00\
        xtalfreq=37400\x00\
        nocrc=1\x00\
        ag0=255\x00\
        aa2g=1\x00\
        ccode=ALL\x00\
        pa0itssit=0x20\x00\
        extpagain2g=0\x00\
        pa2ga0=-168,6649,-778\x00\
        AvVmid_c0=0x0,0xc8\x00\
        cckpwroffset0=5\x00\
        maxp2ga0=84\x00\
        txpwrbckof=6\x00\
        cckbw202gpo=0\x00\
        legofdmbw202gpo=0x66111111\x00\
        mcsbw202gpo=0x77711111\x00\
        propbw202gpo=0xdd\x00\
        ofdmdigfilttype=18\x00\
        ofdmdigfilttypebe=18\x00\
        papdmode=1\x00\
        papdvalidtest=1\x00\
        pacalidx2g=45\x00\
        papdepsoffset=-30\x00\
        papdendidx=58\x00\
        ltecxmux=0\x00\
        ltecxpadnum=0x0102\x00\
        ltecxfnsel=0x44\x00\
        ltecxgcigpio=0x01\x00\
        il0macaddr=00:90:4c:c5:12:38\x00\
        wl0id=0x431b\x00\
        deadman_to=0xffffffff\x00\
        muxenab=0x100\x00\
        spurconfig=0x3\x00\
        glitch_based_crsmin=1\x00\
        btc_mode=1\x00\
        \x00");

    // Pico 2 W pin assignments (same as Pico W):
    //   PIN_23 = WL_ON (CYW43 power enable)
    //   PIN_25 = SPI CS (NOT the LED — LED is on CYW43 GPIO 0)
    //   PIN_24 = SPI DIO
    //   PIN_29 = SPI CLK
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let dma_ch = dma::Channel::new(p.DMA_CH0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        RM2_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        dma_ch,
    );

    static CYW43_STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = CYW43_STATE.init(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw, NVRAM_DATA).await;
    // Runner must be spawned before control.init() — control sends ioctls through the runner.
    spawner.spawn(cyw43_task(runner).unwrap());

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    // ── Network stack ────────────────────────────────────────────────

    let net_config = Config::dhcpv4(Default::default());
    let seed = rng.next_u64();
    let resources = STACK_RESOURCES.init(StackResources::new());
    let (stack, net_runner) = embassy_net::new(net_device, net_config, resources, seed);

    spawner.spawn(net_task(net_runner).unwrap());

    // ── WiFi connect ─────────────────────────────────────────────────

    info!("WiFi: joining '{}'...", WIFI_SSID);
    loop {
        match control
            .join(WIFI_SSID, JoinOptions::new(WIFI_PASS.as_bytes()))
            .await
        {
            Ok(_) => break,
            Err(_) => {
                warn!("WiFi join failed, retrying...");
                Timer::after_secs(3).await;
            }
        }
    }
    info!("WiFi connected!");

    // Wait for DHCP
    info!("Waiting for DHCP...");
    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }
    info!("DHCP is up!");

    // ── Ferrite SDK init ─────────────────────────────────────────────

    ferrite_sdk::init(ferrite_sdk::SdkConfig {
        device_id: DEVICE_ID,
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: build_id::get(),
        ticks_fn: || embassy_time::Instant::now().as_millis(),
        ram_regions: &[ferrite_sdk::RamRegion {
            start: 0x20000000,
            end: 0x20082000,
        }],
    });

    ferrite_sdk::reboot_reason::record_reboot_reason(ferrite_sdk::RebootReason::PowerOnReset);

    info!("Network ready — starting telemetry loop");

    // ── Main loop: collect metrics + upload ───────────────────────────

    let mut counter: u32 = 0;
    loop {
        Timer::after(Duration::from_secs(5)).await;

        counter += 1;

        // Toggle LED via CYW43 (GPIO 0 = on-board LED on Pico W)
        control.gpio_set(0, counter % 2 == 0).await;

        let _ = ferrite_sdk::metric_increment!("loop_count");
        let _ = ferrite_sdk::metric_gauge!("uptime_seconds", counter * 5);

        info!("[{}] metrics recorded", counter);

        // Upload every 30 seconds (6 x 5s)
        if counter % 6 == 0 {
            // Step 1: collect chunks synchronously into buffer
            let mut transport = BufferingTransport::new();
            match UploadManager::upload(&mut transport) {
                Ok(stats) => {
                    info!(
                        "encoded {} chunks ({} bytes) — sending via HTTP",
                        stats.chunks_sent, stats.bytes_sent,
                    );
                    // Step 2: send buffered chunks over async TCP
                    let mut ok = 0u32;
                    let mut fail = 0u32;
                    for chunk in &transport.chunks {
                        match http_post_chunk(stack, chunk).await {
                            Ok(()) => ok += 1,
                            Err(()) => {
                                fail += 1;
                                break;
                            }
                        }
                    }
                    if fail == 0 {
                        info!("upload OK: {} chunks sent", ok);
                    } else {
                        info!("upload partial: {} ok, {} failed", ok, fail);
                    }
                }
                Err(_e) => {
                    error!("upload encode failed");
                }
            }
        }
    }
}

// ── Raw HTTP POST ────────────────────────────────────────────────────

async fn http_post_chunk(stack: embassy_net::Stack<'_>, chunk: &[u8]) -> Result<(), ()> {
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
