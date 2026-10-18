//! Ferrite SDK — Nucleo-H563ZI with Ethernet/HTTP transport via gateway,
//! and signed OTA updates through embassy-boot.
//!
//! POSTs telemetry chunks to the RPi gateway's HTTP ingest endpoint, which
//! forwards them to ferrite-server. The same gateway port relays the OTA
//! target/firmware reads, so the board updates without reaching the server.
//!
//! # Hardware
//! - Board: Nucleo-H563ZI (Cortex-M33, 250MHz)
//! - PHY: LAN8742A (RMII, on-board)
//! - Transport: Ethernet → HTTP POST → RPi gateway:4001
//! - Target: thumbv8m.main-none-eabihf
//!
//! Runs from the ACTIVE partition behind `embassy-stm32h563-bootloader`;
//! flash both (see docs/guide/ota.md).

#![no_std]
#![no_main]

mod ota;

mod identity {
    include!(concat!(env!("OUT_DIR"), "/firmware_identity.rs"));
}

use core::cell::RefCell;

use embassy_boot_stm32::AlignedBuffer;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, IpEndpoint, Ipv4Address, StackResources};
use embassy_stm32::eth::{self, Ethernet, GenericPhy, PacketQueue};
use embassy_stm32::flash::{Flash, WRITE_SIZE};
use embassy_stm32::peripherals::ETH;
use embassy_stm32::rng::Rng;
use embassy_stm32::wdg::IndependentWatchdog;
use embassy_stm32::{bind_interrupts, gpio, peripherals};
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use static_cell::StaticCell;

use ferrite_sdk::chunks::ChunkEncoder;
use ferrite_sdk::transport::ChunkTransport;
use ferrite_sdk::upload::UploadManager;
use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};

use defmt_rtt as _;

use identity::BUILD_ID;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    // Spin until the watchdog resets the board; an unconfirmed update is
    // then rolled back by the bootloader.
    loop {
        cortex_m::asm::nop();
    }
}

const DEVICE_ID: &str = "stm32h563-fleet-01";
// Gateway on the RPi (boards-only Ethernet): chunk ingest + OTA relay
const GATEWAY_HOST: Ipv4Address = Ipv4Address::new(192, 168, 0, 103);
const GATEWAY_PORT: u16 = 4001;
const GATEWAY_URL: &str = "http://192.168.0.103:4001";
const INGEST_PATH: &str = "/ingest/chunks";

const MAC_ADDR: [u8; 6] = [0x02, 0xFE, 0x77, 0x1E, 0x00, 0x01];

/// Telemetry loop period; uploads every 6th tick (30 s), OTA poll every 12th.
const TICK: Duration = Duration::from_secs(5);
/// Longer than the slowest stretch between pets (OTA download chunks, TCP
/// timeouts), shorter than anyone's patience with a hung image.
const WATCHDOG_TIMEOUT_US: u32 = 20_000_000;

bind_interrupts!(struct Irqs {
    ETH => eth::InterruptHandler;
    RNG => embassy_stm32::rng::InterruptHandler<peripherals::RNG>;
});

type Device = Ethernet<'static, ETH, GenericPhy>;

static PACKET_QUEUE: StaticCell<PacketQueue<4, 4>> = StaticCell::new();
static STACK_RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
static FLASH: StaticCell<ota::FlashMutex> = StaticCell::new();
static OTA_STATE_BUF: StaticCell<AlignedBuffer<WRITE_SIZE>> = StaticCell::new();

// ── Buffering transport ───────────────────────────────────────────────

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

// arch-spin has no default entry point; use cortex-m-rt's.
#[embassy_executor::main(entry = "cortex_m_rt::entry")]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hse = Some(Hse {
            freq: embassy_stm32::time::Hertz(8_000_000),
            mode: HseMode::Bypass,
        });
        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL62,
            divp: Some(PllDiv::DIV2), // 248MHz SYSCLK
            divq: Some(PllDiv::DIV4),
            divr: None,
        });
        config.rcc.sys = Sysclk::PLL1_P;
    }
    let p = embassy_stm32::init(config);

    defmt::info!(
        "Ferrite H563 Ethernet — device_id={} build_id={}",
        DEVICE_ID,
        BUILD_ID
    );

    // Started first: a hung image must reset so the bootloader can roll back.
    let mut wdg = IndependentWatchdog::new(p.IWDG, WATCHDOG_TIMEOUT_US);
    wdg.unleash();

    ferrite_sdk::init(SdkConfig {
        device_id: DEVICE_ID,
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: BUILD_ID,
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x20000000,
            end: 0x200A0000,
        }],
    });

    let reason = read_h5_reset_reason();
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    if let Some(fault) = ferrite_sdk::fault::last_fault() {
        defmt::error!(
            "Fault: PC={:#010x} LR={:#010x}",
            fault.frame.pc,
            fault.frame.lr
        );
    }

    let flash = FLASH.init(Mutex::new(RefCell::new(Flash::new_blocking(p.FLASH))));
    let mut ota = ota::new(flash, OTA_STATE_BUF.init(AlignedBuffer([0; WRITE_SIZE])));

    // ── Ethernet ──────────────────────────────────────────────────────

    let eth_device = Ethernet::new(
        PACKET_QUEUE.init(PacketQueue::new()),
        p.ETH,
        Irqs,
        p.PA1,  // REF_CLK
        p.PA2,  // MDIO
        p.PC1,  // MDC
        p.PA7,  // CRS_DV
        p.PC4,  // RXD0
        p.PC5,  // RXD1
        p.PG13, // TXD0
        p.PB15, // TXD1
        p.PG11, // TX_EN
        GenericPhy::new_auto(),
        MAC_ADDR,
    );

    // Static IP — RPi doesn't run DHCP server on eth0
    let net_config = Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: embassy_net::Ipv4Cidr::new(Ipv4Address::new(192, 168, 0, 200), 24),
        gateway: Some(Ipv4Address::new(192, 168, 0, 1)),
        dns_servers: heapless::Vec::new(),
    });

    let mut rng = Rng::new(p.RNG, Irqs);
    let seed = {
        let mut buf = [0u8; 8];
        let _ = rng.async_fill_bytes(&mut buf).await;
        u64::from_le_bytes(buf)
    };

    let (stack, runner) = embassy_net::new(
        eth_device,
        net_config,
        STACK_RESOURCES.init(StackResources::new()),
        seed,
    );
    spawner.must_spawn(net_task(runner));

    defmt::info!("Waiting for Ethernet link...");
    while !stack.is_link_up() {
        wdg.pet();
        Timer::after_millis(500).await;
    }
    defmt::info!("Link up — static IP 192.168.0.200");

    // Test builds only: a "bad" image that never confirms. Without pets the
    // watchdog resets it and the bootloader restores the previous image.
    #[cfg(test_hang_after_boot)]
    {
        defmt::warn!("TEST: hanging before confirm to exercise rollback");
        loop {
            cortex_m::asm::nop();
        }
    }

    // LEDs: LD1=PB0 (green), LD2=PE1 (yellow), LD3=PG4 (red)
    let mut led_yellow = gpio::Output::new(p.PE1, gpio::Level::Low, gpio::Speed::Low);
    let mut led_red = gpio::Output::new(p.PG4, gpio::Level::Low, gpio::Speed::Low);
    let mut encoder = ChunkEncoder::new();
    let gateway = IpEndpoint::new(GATEWAY_HOST.into(), GATEWAY_PORT);
    let mut counter: u32 = 0;

    loop {
        wdg.pet();
        led_yellow.toggle();
        counter += 1;

        let _ = ferrite_sdk::metric_increment!("loop_count");
        let _ = ferrite_sdk::metric_gauge!("uptime_seconds", counter * 5);
        let _ = ferrite_sdk::metric_gauge!("eth_link_up", 1);

        // Upload every 30s (6 × 5s)
        if counter % 6 == 0 {
            let mut transport = BufferingTransport::new();
            match UploadManager::upload(&mut transport) {
                Ok(_) => {
                    // Lets the server mark this build "installed" in a campaign.
                    encoder.encode_ota_request(BUILD_ID, 256, |chunk| {
                        let _ = transport.send_chunk(chunk);
                    });
                    let mut ok = 0u32;
                    for chunk in &transport.chunks {
                        wdg.pet();
                        match http_post_chunk(stack, gateway, chunk).await {
                            Ok(()) => ok += 1,
                            Err(()) => {
                                led_red.set_high();
                                Timer::after_millis(200).await;
                                led_red.set_low();
                                break;
                            }
                        }
                    }
                    defmt::info!("upload: {}/{} chunks sent", ok, transport.chunks.len());
                    // Self-test passed: network up and the server path works.
                    if ok > 0 && ok as usize == transport.chunks.len() && ota.pending_confirm() {
                        ota.confirm();
                    }
                }
                Err(e) => defmt::warn!("encode failed: {:?}", defmt::Debug2Format(&e)),
            }
        }

        // Poll for updates every 60s (12 × 5s)
        if counter % 12 == 3 {
            ota.poll_and_update(stack, gateway, GATEWAY_URL, DEVICE_ID, &mut || wdg.pet())
                .await;
        }

        Timer::after(TICK).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, Device>) -> ! {
    runner.run().await
}

// ── Raw HTTP POST to gateway ──────────────────────────────────────────

async fn http_post_chunk(
    stack: embassy_net::Stack<'static>,
    gateway: IpEndpoint,
    chunk: &[u8],
) -> Result<(), ()> {
    let mut rx_buf = [0u8; 512];
    let mut tx_buf = [0u8; 512];
    let mut tcp = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    tcp.set_timeout(Some(Duration::from_secs(5)));

    tcp.connect(gateway).await.map_err(|_| ())?;

    let mut header_buf = [0u8; 256];
    let header_len = format_http_header(&mut header_buf, chunk.len());
    tcp.write_all(&header_buf[..header_len])
        .await
        .map_err(|_| ())?;
    tcp.write_all(chunk).await.map_err(|_| ())?;
    tcp.flush().await.map_err(|_| ())?;

    let mut resp_buf = [0u8; 64];
    let n = tcp.read(&mut resp_buf).await.map_err(|_| ())?;
    tcp.close();

    if n >= 12 && resp_buf[9] == b'2' {
        Ok(())
    } else {
        Err(())
    }
}

fn format_http_header(buf: &mut [u8], content_length: usize) -> usize {
    let mut pos = 0;
    macro_rules! w {
        ($s:expr) => {
            let b = $s.as_bytes();
            buf[pos..pos + b.len()].copy_from_slice(b);
            pos += b.len();
        };
    }
    w!("POST ");
    w!(INGEST_PATH);
    w!(" HTTP/1.0\r\nHost: ferrite-gw\r\nContent-Type: application/octet-stream\r\nX-Device-Id: ");
    w!(DEVICE_ID);
    w!("\r\nContent-Length: ");
    let mut num_buf = [0u8; 10];
    let num_str = format_u32(&mut num_buf, content_length as u32);
    w!(num_str);
    w!("\r\n\r\n");
    pos
}

fn format_u32(buf: &mut [u8; 10], mut n: u32) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap_or("0");
    }
    let mut i = 10;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("0")
}

fn read_h5_reset_reason() -> RebootReason {
    // STM32H563 RCC_RSR: RCC base 0x44020C00 + RSR offset 0x0D0 = 0x44020CD0
    let rsr = unsafe { core::ptr::read_volatile(0x4402_0CD0 as *const u32) };
    // Clear flags: RMVF is bit 23
    unsafe {
        let val = core::ptr::read_volatile(0x4402_0CD0 as *const u32);
        core::ptr::write_volatile(0x4402_0CD0 as *mut u32, val | (1 << 23));
    }
    match rsr {
        r if r & (1 << 26) != 0 => RebootReason::WatchdogTimeout, // IWDG
        r if r & (1 << 28) != 0 => RebootReason::WatchdogTimeout, // WWDG
        r if r & (1 << 24) != 0 => RebootReason::SoftwareReset,   // SFTRST
        r if r & (1 << 22) != 0 => RebootReason::PinReset,        // PINRST
        _ => RebootReason::PowerOnReset,
    }
}
