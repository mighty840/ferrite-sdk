# Quickstart

This guide walks through adding ferrite-sdk to an Embassy project targeting the nRF52840. By the end you will have crash capture, metrics, and periodic upload working.

## Prerequisites

| Tool | Version | Purpose |
|---|---|---|
| Rust (nightly) | 1.80+ | `#![no_std]` + async in traits |
| `thumbv7em-none-eabihf` target | -- | ARM Cortex-M4F compilation |
| probe-rs | 0.24+ | Flashing and RTT log output |
| nRF52840-DK or similar | -- | Hardware target |

Install the target and probe-rs if you have not already:

```bash
rustup target add thumbv7em-none-eabihf
cargo install probe-rs-tools
```

## Step 1 -- Add dependencies

In your firmware project's `Cargo.toml`:

```toml
[dependencies]
ferrite-sdk = { git = "https://github.com/mighty840/ferrite-sdk", features = ["cortex-m", "defmt", "embassy"] }
ferrite-embassy = { git = "https://github.com/mighty840/ferrite-sdk" }

embassy-executor = { version = "0.6", features = ["task-arena-size-65536", "arch-cortex-m", "executor-thread"] }
embassy-time = { version = "0.3", features = ["tick-hz-32_768"] }
embassy-nrf = { version = "0.2", features = ["nrf52840", "time-driver-rtc1", "gpiote"] }

cortex-m = "0.7"
cortex-m-rt = "0.7"
defmt = "0.3"
defmt-rtt = "0.4"
panic-probe = { version = "0.3", features = ["print-defmt"] }
```

## Step 2 -- Add the linker script fragment

ferrite-sdk stores fault records and reboot reasons in a dedicated section of RAM that is **not zeroed on soft reset**. You need to reserve 256 bytes at the end of your RAM.

Create or edit `memory.x` in your project root:

```ld
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 255K  /* 256K minus 256 bytes */
  RETAINED (rwx) : ORIGIN = 0x2003FF00, LENGTH = 0x100
}

SECTIONS
{
  .uninit.ferrite (NOLOAD) : {
    . = ALIGN(4);
    _ferrite_retained_start = .;
    KEEP(*(.uninit.ferrite))
    _ferrite_retained_end = .;
    . = ALIGN(4);
  } > RETAINED
}
```

::: tip
Pre-built linker fragments are available in the `linker/` directory of the repository for nRF52840, RP2040, and STM32F4. Copy the appropriate file and `INCLUDE` it from your `memory.x`.
:::

## Step 3 -- Initialize the SDK

In your `main` function, call `ferrite_sdk::init()` before spawning any tasks:

```rust
#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::Duration;
use ferrite_sdk::{SdkConfig, RamRegion};
use defmt_rtt as _;
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    // Initialize the observability SDK
    ferrite_sdk::init(SdkConfig {
        device_id: "sensor-42",
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: 0,
        ticks_fn: || embassy_time::Instant::now().as_ticks(),
        ram_regions: &[RamRegion {
            start: 0x2000_0000,
            end: 0x2004_0000,
        }],
    });

    defmt::info!("ferrite-sdk initialized, boot sequence started");

    // Record why we rebooted (read the nRF RESETREAS register)
    let resetreas = unsafe {
        core::ptr::read_volatile(0x4000_0400 as *const u32)
    };
    let reason = match resetreas {
        r if r & 0x01 != 0 => ferrite_sdk::RebootReason::PinReset,
        r if r & 0x02 != 0 => ferrite_sdk::RebootReason::WatchdogTimeout,
        r if r & 0x04 != 0 => ferrite_sdk::RebootReason::SoftwareReset,
        _ => ferrite_sdk::RebootReason::PowerOnReset,
    };
    ferrite_sdk::reboot_reason::record_reboot_reason(reason);

    // Clear the RESETREAS register
    unsafe {
        core::ptr::write_volatile(0x4000_0400 as *mut u32, 0xFFFF_FFFF);
    }

    // Spawn the upload task (see step 5)
    // spawner.spawn(upload_task(/* transport */)).unwrap();

    // Your application logic here...
    loop {
        // Record a gauge metric
        ferrite_sdk::metric_gauge!("temperature", read_temperature());

        // Increment a counter
        ferrite_sdk::metric_increment!("loop_iterations");

        embassy_time::Timer::after(Duration::from_secs(10)).await;
    }
}

fn read_temperature() -> f32 {
    // Replace with real ADC reading
    23.5
}
```

### What `init()` does

1. Registers your RAM regions so the HardFault handler knows which addresses are safe to read when capturing a stack snapshot.
2. Validates the retained RAM block. If the magic number (`0xABCD1234`) is missing or corrupted, the block is re-initialized. Otherwise the existing fault record and reboot reason from the previous boot are preserved.
3. Increments the boot sequence counter in retained RAM.
4. Stores your `ticks_fn` so every metric and trace entry gets a monotonic timestamp.
5. Creates the global `SdkState` containing the `MetricsBuffer<32>`, `TraceBuffer<512>`, and `ChunkEncoder`.

## Step 4 -- Record metrics

The SDK provides three metric macros that operate on the global SDK state inside a critical section:

```rust
// Counters: monotonically increasing, wraps at u32::MAX
ferrite_sdk::metric_increment!("packets_sent");         // increment by 1
ferrite_sdk::metric_increment!("bytes_sent", 128);      // increment by 128

// Gauges: last-write-wins, stores an f32
ferrite_sdk::metric_gauge!("battery_mv", 3720.0);
ferrite_sdk::metric_gauge!("rssi_dbm", -67);            // auto-cast to f32

// Histograms: tracks min, max, sum, count
ferrite_sdk::metric_observe!("request_latency_ms", 12.5);
ferrite_sdk::metric_observe!("request_latency_ms", 8.3);
```

Each metric entry occupies `1 + key_len + 1 + 8 + 8` bytes in the serialized chunk payload. With the default `MetricsBuffer<32>`, you can track up to 32 distinct metric keys simultaneously. When a 33rd key is recorded, the oldest entry is evicted.

Counters with the same key accumulate: calling `metric_increment!("x", 1)` twice produces a single entry with value 2. Gauges overwrite: calling `metric_gauge!("temp", 25.0)` replaces any previous "temp" value.

## Step 5 -- Implement a transport and spawn the upload task

The upload task needs something that implements `AsyncChunkTransport`. Here is a minimal example using a UART peripheral:

```rust
use ferrite_sdk::transport::AsyncChunkTransport;
use embassy_nrf::uarte::{self, UarteTx};

struct UartUplink {
    tx: UarteTx<'static, embassy_nrf::peripherals::UARTE0>,
}

impl AsyncChunkTransport for UartUplink {
    type Error = uarte::Error;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        self.tx.write(chunk).await
    }
}
```

Then spawn the Embassy upload task:

```rust
use ferrite_embassy::upload_task::ferrite_upload_task;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // ... init code from step 3 ...

    let uart_tx = /* configure UARTE0 TX pin */;
    let transport = UartUplink { tx: uart_tx };

    spawner.spawn(ferrite_upload_task(transport, Duration::from_secs(60))).unwrap();

    // ... rest of main ...
}
```

The upload task runs forever. Every 60 seconds it performs a full upload session:

1. **DeviceInfo** -- device_id, firmware_version, build_id
2. **RebootReason** -- if a reason was recorded this boot
3. **FaultRecord** -- if a fault was captured before the last reboot
4. **Metrics** -- all buffered metric entries (split into multiple chunks if needed)
5. **TraceFragment** -- buffered defmt log data (fragmented into 240-byte chunks)
6. **Heartbeat** -- uptime, free stack estimate, metrics count, frames lost

After a successful upload, all buffers are cleared. If the transport fails mid-session, data is retained for the next attempt.

### Triggered uploads

If you want to upload immediately after a crash recovery (rather than waiting for the next interval), use the triggered variant:

```rust
use ferrite_embassy::upload_task::{ferrite_upload_task_with_trigger, trigger_upload_now};

// In main:
spawner.spawn(ferrite_upload_task_with_trigger(transport, Duration::from_secs(60))).unwrap();

// Anywhere else (even from an interrupt):
trigger_upload_now();
```

## Step 6 -- Run the server

On your host machine, build and start the companion server:

```bash
cd ferrite-server
cargo run -- --http 0.0.0.0:4000 --db ./ferrite.db --elf-dir ./elfs
```

The server listens for binary chunk data on `POST /ingest/chunks`. Point your device's transport at this endpoint (or pipe UART output through a bridge script).

Upload your ELF file for symbolication:

```bash
curl -X POST http://localhost:4000/ingest/elf \
  -H "X-Firmware-Version: 0.1.0" \
  --data-binary @target/thumbv7em-none-eabihf/release/my-firmware
```

Now when a fault is received, the server will resolve the PC address to a source location like `app::main at src/main.rs:42`.

## Step 7 -- Verify

Flash your firmware and watch the RTT output:

```bash
cargo run --release
```

You should see:

```
INFO  ferrite-sdk initialized, boot sequence started
INFO  ferrite upload ok: 3 chunks, 142 bytes
```

Query the server:

```bash
# List all known devices
curl http://localhost:4000/devices

# View faults for a device
curl http://localhost:4000/devices/sensor-42/faults

# View metrics
curl http://localhost:4000/devices/sensor-42/metrics
```

## Next steps

- [Core Concepts](./concepts) -- understand retained RAM, the chunk lifecycle, and the transport abstraction in depth
- [Embassy Integration](../integrations/embassy) -- advanced Embassy patterns (BLE transport, triggered uploads, multi-peripheral)
- [nRF52840 Target Guide](../targets/nrf52840) -- nRF-specific memory layout, RESETREAS register, and probe-rs tips
