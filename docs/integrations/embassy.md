# Embassy Integration

The `ferrite-embassy` crate provides Embassy executor tasks for periodic and triggered data uploads. This guide covers setup, transport implementation, task spawning, and advanced patterns.

## Overview

Embassy is an async runtime for embedded Rust. The ferrite-sdk Embassy integration provides two `#[embassy_executor::task]` functions:

- **`ferrite_upload_task`** -- uploads on a fixed interval using `embassy_time::Timer`.
- **`ferrite_upload_task_with_trigger`** -- uploads on a fixed interval OR immediately when `trigger_upload_now()` is called, whichever comes first.

Both tasks run forever (return type `-> !`) and should be spawned once at startup.

## Dependencies

```toml
[dependencies]
ferrite-sdk = { git = "https://github.com/your-org/ferrite-sdk", features = ["cortex-m", "defmt", "embassy"] }
ferrite-embassy = { git = "https://github.com/your-org/ferrite-sdk" }

embassy-executor = { version = "0.6", features = ["task-arena-size-65536", "arch-cortex-m", "executor-thread"] }
embassy-time = "0.3"
embassy-sync = "0.5"
embassy-futures = "0.1"
```

The `embassy` feature on `ferrite-sdk` enables the `AsyncChunkTransport` trait and the async `upload_async` method on `UploadManager`.

## Implementing AsyncChunkTransport

You need to provide a type that implements `AsyncChunkTransport`. The trait has one required method and three optional ones:

```rust
pub trait AsyncChunkTransport {
    type Error: core::fmt::Debug;

    /// Send a single encoded chunk (up to 256 bytes).
    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;

    /// Return true if the transport is available for sending.
    fn is_available(&self) -> bool { true }

    /// Called before a batch upload begins.
    async fn begin_session(&mut self) -> Result<(), Self::Error> { Ok(()) }

    /// Called after a batch upload completes.
    async fn end_session(&mut self) -> Result<(), Self::Error> { Ok(()) }
}
```

### UART transport example

```rust
use embassy_nrf::uarte::{self, UarteTx};
use ferrite_sdk::transport::AsyncChunkTransport;

pub struct UartTransport {
    tx: UarteTx<'static, embassy_nrf::peripherals::UARTE0>,
}

impl UartTransport {
    pub fn new(tx: UarteTx<'static, embassy_nrf::peripherals::UARTE0>) -> Self {
        Self { tx }
    }
}

impl AsyncChunkTransport for UartTransport {
    type Error = uarte::Error;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        self.tx.write(chunk).await
    }
}
```

### BLE transport example

For BLE, you typically write chunks as GATT notifications. Here is a sketch using `nrf-softdevice`:

```rust
use nrf_softdevice::ble::gatt_server;

pub struct BleTransport {
    conn: gatt_server::Connection,
    tx_handle: u16,
}

impl AsyncChunkTransport for BleTransport {
    type Error = gatt_server::NotifyValueError;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        gatt_server::notify_value(&self.conn, self.tx_handle, chunk)
    }

    fn is_available(&self) -> bool {
        self.conn.is_connected()
    }
}
```

### HTTP transport example

If your device has a TCP/IP stack (e.g., via ESP32 coprocessor or Ethernet), you can buffer chunks and send them as a single HTTP POST:

```rust
pub struct HttpTransport {
    buffer: heapless::Vec<u8, 4096>,
    // ... TCP socket handle
}

impl AsyncChunkTransport for HttpTransport {
    type Error = TcpError;

    async fn send_chunk(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        // Buffer chunks; flush on end_session
        self.buffer.extend_from_slice(chunk).map_err(|_| TcpError::BufferFull)?;
        Ok(())
    }

    async fn end_session(&mut self) -> Result<(), Self::Error> {
        // Send all buffered chunks as one HTTP POST to /ingest/chunks
        let body = &self.buffer[..];
        self.tcp_post("/ingest/chunks", body).await?;
        self.buffer.clear();
        Ok(())
    }
}
```

## Spawning the upload task

### Periodic upload

The simplest pattern -- upload every N seconds:

```rust
use ferrite_embassy::upload_task::ferrite_upload_task;
use embassy_time::Duration;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // ... init SDK, configure peripherals ...

    let transport = UartTransport::new(uart_tx);
    spawner.spawn(ferrite_upload_task(transport, Duration::from_secs(60))).unwrap();
}
```

The task logs upload results via defmt:

```
INFO  ferrite upload ok: 4 chunks, 218 bytes
```

If the transport fails:

```
WARN  ferrite upload failed: TransportError(Overrun)
```

### Triggered upload

Use `ferrite_upload_task_with_trigger` when you want to upload immediately after a specific event (e.g., detecting a previous crash on boot):

```rust
use ferrite_embassy::upload_task::{ferrite_upload_task_with_trigger, trigger_upload_now};

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // ... init SDK ...

    let transport = UartTransport::new(uart_tx);
    spawner.spawn(
        ferrite_upload_task_with_trigger(transport, Duration::from_secs(60))
    ).unwrap();

    // If we recovered from a fault, upload immediately
    if ferrite_sdk::fault::last_fault().is_some() {
        defmt::info!("fault record found from previous boot, triggering upload");
        trigger_upload_now();
    }
}
```

`trigger_upload_now()` is safe to call from any context -- interrupts, other tasks, or the main task. It uses a `Channel<CriticalSectionRawMutex, (), 1>` internally, so multiple rapid triggers coalesce into a single upload cycle.

The task uses `embassy_futures::select` to race the periodic timer against the trigger channel. Whichever fires first causes an upload. After the upload completes, any extra pending triggers are drained so the task does not immediately loop again.

## Upload session internals

Each upload cycle calls `UploadManager::upload_async()`, which:

1. Checks `is_available()` -- if false, returns `Err(TransportUnavailable)` without touching any data.
2. Calls `begin_session()`.
3. Collects all chunks inside a critical section (to avoid holding the critical section across async points).
4. Sends each chunk via `send_chunk()`.
5. On success, clears all SDK buffers inside a critical section.
6. Calls `end_session()`.

The critical section in step 3 is brief -- it only copies data out of the SDK state into a temporary `heapless::Vec` of chunks. The actual transport I/O happens outside the critical section, so it does not block interrupts.

## Error handling

If `send_chunk()` returns an error for any chunk, the entire session is aborted. Data is **not** cleared from the SDK buffers, so it will be retried on the next upload cycle. This means:

- Metrics are retained until successfully uploaded.
- Fault records survive across multiple failed upload attempts.
- Trace data persists (but may be overwritten by new frames if the circular buffer wraps).

The upload task never panics or exits. Errors are logged via defmt and the task sleeps until the next interval or trigger.

## Task arena sizing

The upload task allocates a temporary buffer of up to 32 chunks (each up to 256 bytes) on the Embassy task's stack. Make sure your `task-arena-size` is large enough. The default `65536` is sufficient for most applications. If you have very constrained memory, you can reduce it but watch for stack overflows.
