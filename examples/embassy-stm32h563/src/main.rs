//! Ferrite SDK — Nucleo-H563ZI with Ethernet/HTTP transport via gateway.
//!
//! POSTs telemetry chunks to the RPi gateway's HTTP ingest endpoint.
//! The gateway then forwards them to ferrite-server.
//!
//! # Hardware
//! - Board: Nucleo-H563ZI (Cortex-M33, 250MHz)
//! - PHY: LAN8742A (RMII, on-board)
//! - Transport: Ethernet → HTTP POST → RPi gateway:4001
//! - Target: thumbv8m.main-none-eabihf

#![no_std]
#![no_main]

use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Ipv4Address, StackResources};
use embassy_stm32::eth::{self, Ethernet, PacketQueue};
use embassy_stm32::gpio;
use embassy_stm32::rng::Rng;
use embassy_stm32::{bind_interrupts, peripherals};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use static_cell::StaticCell;
use cortex_m_rt::entry;

use ferrite_sdk::transport::ChunkTransport;
use ferrite_sdk::upload::UploadManager;
use ferrite_sdk::{RamRegion, RebootReason, SdkConfig};

use defmt_rtt as _;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop { cortex_m::asm::nop(); }
}

const DEVICE_ID: &str = "stm32h563-fleet-01";
// Gateway HTTP ingest on the RPi (connected via Ethernet)
const GATEWAY_HOST: Ipv4Address = Ipv4Address::new(192, 168, 0, 103);
const GATEWAY_PORT: u16 = 4001;
const INGEST_PATH: &str = "/ingest/chunks";

const MAC_ADDR: [u8; 6] = [0x02, 0xFE, 0x77, 0x1E, 0x00, 0x01];

bind_interrupts!(struct Irqs {
    ETH => eth::InterruptHandler;
    RNG => embassy_stm32::rng::InterruptHandler<peripherals::RNG>;
});

static PACKET_QUEUE: StaticCell<PacketQueue<4, 4>> = StaticCell::new();
static STACK_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
static EXECUTOR: StaticCell<embassy_executor::raw::Executor> = StaticCell::new();

// ── Buffering transport ───────────────────────────────────────────────

struct BufferingTransport {
    chunks: heapless::Vec<heapless::Vec<u8, 256>, 32>,
}

impl BufferingTransport {
    fn new() -> Self {
        Self { chunks: heapless::Vec::new() }
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

    fn is_available(&self) -> bool { true }
}

// ── Entry point ───────────────────────────────────────────────────────

#[entry]
fn main() -> ! {
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

    defmt::info!("Ferrite H563 Ethernet — device_id={}", DEVICE_ID);

    let executor = EXECUTOR.init(embassy_executor::raw::Executor::new(cortex_m::asm::sev as *mut ()));
    let spawner = executor.spawner();
    unsafe { spawner.spawn(main_task(p)).unwrap(); }

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
        ram_regions: &[RamRegion { start: 0x20000000, end: 0x200A0000 }],
    });

    let reason = read_h5_reset_reason();
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    if let Some(fault) = ferrite_sdk::fault::last_fault() {
        defmt::error!("Fault: PC={:#010x} LR={:#010x}", fault.frame.pc, fault.frame.lr);
    }

    // ── Ethernet ──────────────────────────────────────────────────────

    let packet_queue = PACKET_QUEUE.init(PacketQueue::new());

    let phy = eth::generic_smi::GenericSMI::new(0);
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
        p.PG13, // TXD0
        p.PB15, // TXD1
        p.PG11, // TX_EN
        phy,
        MAC_ADDR,
    );

    // Static IP — RPi doesn't run DHCP server on eth0
    let net_config = Config::ipv4_static(embassy_net::StaticConfigV4 {
        address: embassy_net::Ipv4Cidr::new(Ipv4Address::new(192, 168, 0, 200), 24),
        gateway: Some(Ipv4Address::new(192, 168, 0, 1)),
        dns_servers: heapless::Vec::new(),
    });
    let resources = STACK_RESOURCES.init(StackResources::new());

    let mut rng = Rng::new(p.RNG, Irqs);
    let seed = {
        let mut buf = [0u8; 8];
        let _ = rng.async_fill_bytes(&mut buf).await;
        u64::from_le_bytes(buf)
    };

    let stack = {
        static STACK: StaticCell<embassy_net::Stack<Ethernet<'static, peripherals::ETH, eth::generic_smi::GenericSMI>>> = StaticCell::new();
        &*STACK.init(embassy_net::Stack::new(eth_device, net_config, resources, seed))
    };

    let net_fut = stack.run();

    let app_fut = async {
        // Wait for link
        defmt::info!("Waiting for Ethernet link...");
        loop {
            if stack.is_link_up() { break; }
            Timer::after_millis(500).await;
        }
        defmt::info!("Link up — static IP 192.168.0.200");

        defmt::info!("Network ready — starting telemetry");

        // LEDs: LD1=PB0 (green), LD2=PE1 (yellow), LD3=PG4 (red)
        let mut led_yellow = gpio::Output::new(p.PE1, gpio::Level::Low, gpio::Speed::Low);
        let mut led_red = gpio::Output::new(p.PG4, gpio::Level::Low, gpio::Speed::Low);
        let mut counter: u32 = 0;

        loop {
            led_yellow.toggle();
            counter += 1;

            let _ = ferrite_sdk::metric_increment!("loop_count");
            let _ = ferrite_sdk::metric_gauge!("uptime_seconds", counter * 5);
            let _ = ferrite_sdk::metric_gauge!("eth_link_up", 1);

            if counter % 30 == 0 {
                defmt::info!("iteration {}", counter);
            }

            // Upload every 30s (6 × 5s)
            if counter % 6 == 0 {
                let mut transport = BufferingTransport::new();
                match UploadManager::upload(&mut transport) {
                    Ok(stats) => {
                        defmt::info!("encoded {} chunks", stats.chunks_sent);
                        let mut ok = 0u32;
                        for chunk in &transport.chunks {
                            match http_post_chunk(stack, chunk).await {
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
                    }
                    Err(e) => defmt::warn!("encode failed: {:?}", defmt::Debug2Format(&e)),
                }
            }

            Timer::after(Duration::from_secs(5)).await;
        }
    };

    embassy_futures::join::join(net_fut, app_fut).await;
}

// ── Raw HTTP POST to gateway ──────────────────────────────────────────

async fn http_post_chunk(
    stack: &embassy_net::Stack<Ethernet<'static, peripherals::ETH, eth::generic_smi::GenericSMI>>,
    chunk: &[u8],
) -> Result<(), ()> {
    let mut rx_buf = [0u8; 512];
    let mut tx_buf = [0u8; 512];
    let mut tcp = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    tcp.set_timeout(Some(Duration::from_secs(5)));

    tcp.connect((GATEWAY_HOST, GATEWAY_PORT)).await.map_err(|_| ())?;

    let mut header_buf = [0u8; 256];
    let header_len = format_http_header(&mut header_buf, chunk.len());
    tcp.write_all(&header_buf[..header_len]).await.map_err(|_| ())?;
    tcp.write_all(chunk).await.map_err(|_| ())?;
    tcp.flush().await.map_err(|_| ())?;

    let mut resp_buf = [0u8; 64];
    let n = tcp.read(&mut resp_buf).await.map_err(|_| ())?;
    tcp.close();

    if n >= 12 && resp_buf[9] == b'2' { Ok(()) } else { Err(()) }
}

fn format_http_header(buf: &mut [u8], content_length: usize) -> usize {
    let mut pos = 0;
    macro_rules! w {
        ($s:expr) => { let b = $s.as_bytes(); buf[pos..pos+b.len()].copy_from_slice(b); pos += b.len(); };
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

fn format_u32<'a>(buf: &'a mut [u8; 10], mut n: u32) -> &'a str {
    if n == 0 { buf[0] = b'0'; return unsafe { core::str::from_utf8_unchecked(&buf[..1]) }; }
    let mut i = 10;
    while n > 0 { i -= 1; buf[i] = b'0' + (n % 10) as u8; n /= 10; }
    unsafe { core::str::from_utf8_unchecked(&buf[i..]) }
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
        r if r & (1 << 26) != 0 => RebootReason::WatchdogTimeout,  // IWDG
        r if r & (1 << 28) != 0 => RebootReason::WatchdogTimeout,  // WWDG
        r if r & (1 << 24) != 0 => RebootReason::SoftwareReset,    // SFTRST
        r if r & (1 << 22) != 0 => RebootReason::PinReset,         // PINRST
        _ => RebootReason::PowerOnReset,
    }
}
