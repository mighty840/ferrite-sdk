# Bare-metal Usage

If you are not using Embassy, RTIC, or any other framework, you can use the core `ferrite-sdk` crate directly with blocking calls from your main loop.

## Overview

The bare-metal approach is the simplest integration path:

1. Call `ferrite_sdk::init()` at startup.
2. Record metrics and reboot reasons from your application code.
3. Periodically call `UploadManager::upload()` with a blocking transport.

There is no dedicated integration crate -- you use `ferrite-sdk` directly.

## Dependencies

```toml
[dependencies]
ferrite-sdk = { git = "https://github.com/your-org/ferrite-sdk", features = ["cortex-m"] }
cortex-m = "0.7"
cortex-m-rt = "0.7"
```

Omit the `embassy` feature since you are not using async.

## Minimal example

```rust
#![no_std]
#![no_main]

use cortex_m_rt::entry;
use ferrite_sdk::{SdkConfig, RamRegion};
use ferrite_sdk::upload::UploadManager;

#[entry]
fn main() -> ! {
    // Initialize hardware (clocks, peripherals, etc.)
    let uart = configure_uart();

    // Initialize the SDK
    ferrite_sdk::init(SdkConfig {
        device_id: "bare-metal-device",
        firmware_version: env!("CARGO_PKG_VERSION"),
        build_id: 0,
        ticks_fn: || read_systick_counter(),
        ram_regions: &[RamRegion {
            start: 0x2000_0000,
            end: 0x2002_0000,
        }],
    });

    // Record reboot reason
    ferrite_sdk::reboot_reason::record_reboot_reason(
        ferrite_sdk::RebootReason::PowerOnReset,
    );

    let mut transport = MyUartTransport::new(uart);
    let mut loop_count: u32 = 0;

    loop {
        // Application logic
        let temp = read_temperature();
        let _ = ferrite_sdk::metric_gauge!("temperature", temp);
        let _ = ferrite_sdk::metric_increment!("loop_count");

        loop_count += 1;

        // Upload every 100 iterations
        if loop_count % 100 == 0 {
            match UploadManager::upload(&mut transport) {
                Ok(stats) => {
                    // Upload succeeded, buffers are cleared
                }
                Err(_) => {
                    // Upload failed, data retained for next attempt
                }
            }
        }

        // Delay (busy-wait or WFI)
        cortex_m::asm::delay(16_000_000); // ~1 second at 16 MHz
    }
}
```

## Implementing ChunkTransport

For bare-metal, implement the blocking `ChunkTransport` trait:

```rust
use ferrite_sdk::transport::ChunkTransport;

struct MyUartTransport {
    // Your UART peripheral handle
}

impl ChunkTransport for MyUartTransport {
    type Error = ();

    fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        for &byte in chunk {
            // Wait for TX ready, write byte
            while !uart_tx_ready() {}
            uart_write_byte(byte);
        }
        Ok(())
    }
}
```

## Timing considerations

In a bare-metal superloop, `UploadManager::upload()` blocks until all chunks have been sent. For a typical upload session (3-6 chunks), this takes:

| Transport | Approximate duration |
|---|---|
| UART 115200 baud | ~15 ms |
| UART 9600 baud | ~180 ms |
| SPI to radio module | ~5 ms |

Plan your loop timing accordingly. If the upload takes too long for your real-time requirements, consider dedicating a hardware timer interrupt to trigger uploads or moving to RTIC or Embassy.

## Providing a ticks function

The `ticks_fn` in `SdkConfig` must return a monotonically increasing `u64` value. Common bare-metal options:

```rust
// Option 1: SysTick counter (wraps at 24 bits)
fn read_systick_counter() -> u64 {
    static mut TICK_COUNT: u64 = 0;
    // Increment in SysTick handler
    unsafe { TICK_COUNT }
}

// Option 2: DWT cycle counter (32-bit, wraps)
fn read_dwt_cycles() -> u64 {
    cortex_m::peripheral::DWT::cycle_count() as u64
}

// Option 3: Hardware timer with 32-bit free-running counter
fn read_timer() -> u64 {
    unsafe { (*TIMER0::ptr()).cc[0].read().bits() as u64 }
}
```

The SDK uses ticks only for timestamping metrics and trace entries. The absolute value does not matter -- only the relative ordering.
